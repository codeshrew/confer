//! Pure projections over the message log — the folds that answer "what's the state
//! of the board?" and "who's on this hub?", returning **structs** (never printing).
//!
//! Extracted from the `cmd_*` handlers so the same folds drive the CLI's stdout
//! rendering, the `dashboard` TUI, and the `serve` web view. The core
//! folds (`Board::fold`, `agents`) are pure functions of already-loaded data. The
//! `Snapshot` fold on top loads a hub from disk (messages, roster, presence, git
//! health) into one render-ready struct that every front-end shares.

use crate::schema::Message;
use crate::{config, crosshub, gitcmd, presence, roster, store, watchlock};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Strict id match for the **fold layer**: exact full id, or a trailing suffix of
/// ≥8 chars (the random ULID tail — the documented short-id affordance, safe
/// because it's random). Never leading-prefix (ULIDs share a timestamp prefix) and
/// never empty. `of` is resolved to a full id at append time, so exact is the
/// normal path; the suffix tolerates legacy refs.
pub fn id_ref_matches(full: &str, reference: &str) -> bool {
    !reference.is_empty()
        && (full == reference || (reference.len() >= 8 && full.ends_with(reference)))
}

/// Fold a request's terminal/active status from the log. DONE/ERROR/SUPERSEDED are
/// terminal; BLOCKED is a stall a `claim` clears; else CLAIMED or OPEN.
pub fn request_status(msgs: &[Message], req_id: &str) -> &'static str {
    let mut st = "OPEN";
    for m in msgs {
        // Fold the SUPERSEDE in chronological order like the other terminals — NOT ahead of them.
        // Checking supersede first (as an any() before the loop) let a `supersede` posted AFTER a
        // `done` retroactively flip a completed request to SUPERSEDED, erasing the completion (and
        // any role could do it). The FIRST terminal event in fold order wins, whatever it is.
        if m.front.supersedes.as_deref().is_some_and(|s| id_ref_matches(req_id, s)) {
            return "SUPERSEDED";
        }
        if m.front.of.as_deref().is_some_and(|of| id_ref_matches(req_id, of)) {
            match m.front.msg_type.as_str() {
                "done" => return "DONE",
                "error" => return "ERROR",
                "blocked" => st = "BLOCKED",
                // claiming (re-)commits to active work, clearing a prior block.
                "claim" => st = "CLAIMED",
                _ => {}
            }
        }
    }
    st
}

/// Is a request deferred to the backlog? — its sender-time `defer` facet OR a later
/// `defer` event (settable by anyone, e.g. the addressee), cleared once someone
/// `claim`s it (committing to active work). Event-sourced, DESIGN.md Phase 2.
pub fn request_deferred(msgs: &[Message], req: &Message) -> bool {
    let mut deferred = req.front.defer;
    for m in msgs {
        if m.front.of.as_deref().is_some_and(|of| id_ref_matches(&req.front.id, of)) {
            match m.front.msg_type.as_str() {
                "defer" => deferred = true,
                "claim" => deferred = false,
                _ => {}
            }
        }
    }
    deferred
}

/// The resolution recorded on a request's CLOSING `done`, if any. Uses the FIRST matching `done`
/// (the one that terminated the request per `request_status`'s fold order) and returns ITS
/// resolution — NOT the first done that merely happens to carry one. Using `find_map` skipped a
/// resolution-less closing done (a genuine completion) and picked up a LATER `done --as wont-do`,
/// mislabelling completed work as consciously dropped.
pub fn done_resolution(msgs: &[Message], req_id: &str) -> Option<String> {
    msgs.iter()
        .find(|m| {
            m.front.msg_type == "done"
                && m.front.of.as_deref().is_some_and(|of| id_ref_matches(req_id, of))
        })
        .and_then(|m| m.front.resolution.clone())
}

/// Distinct roles that have `claim`ed a request, in first-claim order (the head is
/// the owner; a tail means a contested claim race).
pub fn claimants(msgs: &[Message], req_id: &str) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for m in msgs {
        if m.front.msg_type == "claim"
            && m.front.of.as_deref().is_some_and(|of| id_ref_matches(req_id, of))
            && !seen.contains(&m.front.from)
        {
            seen.push(m.front.from.clone());
        }
    }
    seen
}

/// A stale-open request is flagged for debt visibility.
pub const STALE_SECS: i64 = 3 * 86400;

/// One request as the board sees it — raw data (ids/roles unresolved), so the
/// renderer owns display formatting (short-id, roster display, glyphs).
#[derive(Debug, Clone)]
pub struct RequestRow {
    pub id: String,
    pub from: String,
    pub to: Vec<String>,
    pub summary: String,
    /// OPEN | CLAIMED | BLOCKED | DONE | ERROR | SUPERSEDED.
    pub status: &'static str,
    /// Resolution on a closing `done` (wont-do | obsolete | duplicate).
    pub resolution: Option<String>,
    /// Deferred to backlog (off the active board).
    pub deferred: bool,
    /// Distinct claimants, first-claim order (head = owner; tail = contested).
    pub claimants: Vec<String>,
    pub age_secs: i64,
    /// Open (OPEN|CLAIMED) and older than STALE_SECS — surfaced as debt.
    pub stale: bool,
}

impl RequestRow {
    pub fn is_open(&self) -> bool {
        matches!(self.status, "OPEN" | "CLAIMED")
    }
    /// On the ACTIVE board = open and not deferred and not blocked.
    pub fn is_active(&self) -> bool {
        self.is_open() && !self.deferred
    }
    /// On the backlog = deferred while still in a live state.
    pub fn is_backlog(&self) -> bool {
        self.deferred && matches!(self.status, "OPEN" | "CLAIMED" | "BLOCKED")
    }
}

/// The whole board folded from the log: every request as a row (sorted by id =
/// chronological), plus the cumulative-flow tally and per-agent WIP.
#[derive(Debug, Clone, Default)]
pub struct Board {
    pub rows: Vec<RequestRow>,
    pub open: usize,
    pub claimed: usize,
    pub blocked: usize,
    pub backlog: usize,
    pub closed: usize,
    pub wip: BTreeMap<String, usize>,
}

impl Board {
    /// Fold the board from all messages as of `now` (for age/stale).
    pub fn fold(msgs: &[Message], now: DateTime<Utc>) -> Board {
        let mut reqs: Vec<&Message> = msgs.iter().filter(|m| m.front.msg_type == "request").collect();
        reqs.sort_by(|a, b| a.front.id.cmp(&b.front.id));

        let mut board = Board::default();
        for r in reqs {
            let status = request_status(msgs, &r.front.id);
            let deferred = request_deferred(msgs, r);
            let cs = if status == "CLAIMED" { claimants(msgs, &r.front.id) } else { Vec::new() };
            let resolution = done_resolution(msgs, &r.front.id);
            let age_secs = DateTime::parse_from_rfc3339(&r.front.ts)
                .ok()
                .map(|t| (now - t.with_timezone(&Utc)).num_seconds().max(0))
                .unwrap_or(0);
            let row = RequestRow {
                id: r.front.id.clone(),
                from: r.front.from.clone(),
                to: r.front.to.clone(),
                summary: r.summary_line(),
                status,
                resolution,
                deferred,
                claimants: cs,
                age_secs,
                stale: matches!(status, "OPEN" | "CLAIMED") && age_secs >= STALE_SECS,
            };

            // Flow tally: backlog wins over its underlying live state.
            if row.is_backlog() {
                board.backlog += 1;
            } else {
                match status {
                    "OPEN" => board.open += 1,
                    "CLAIMED" => {
                        board.claimed += 1;
                        for c in &row.claimants {
                            *board.wip.entry(c.clone()).or_default() += 1;
                        }
                    }
                    "BLOCKED" => board.blocked += 1,
                    _ => board.closed += 1,
                }
            }
            board.rows.push(row);
        }
        board
    }
}

/// Walk `of` → `reply_to` → `supersedes` back to the conversation HEAD (the request
/// or original message a message hangs off), so a reverse-lookup hit answers with a
/// THREAD, not a loose message id. Bounded against cycles / dangling parents.
pub fn thread_root<'a>(msgs: &'a [Message], m: &'a Message) -> &'a Message {
    let mut cur = m;
    for _ in 0..64 {
        let Some(pref) = cur
            .front
            .of
            .as_deref()
            .or(cur.front.reply_to.as_deref())
            .or(cur.front.supersedes.as_deref())
        else {
            break;
        };
        match msgs.iter().find(|x| id_ref_matches(&x.front.id, pref)) {
            Some(p) if !std::ptr::eq(p, cur) => cur = p,
            _ => break,
        }
    }
    cur
}

/// One code reference as the reverse index sees it: the ref itself + the message and
/// thread that made it (resolved to the thread root + its request status).
#[derive(Debug, Clone)]
pub struct RefHit {
    pub repo: String,
    pub path: String,
    pub sha: String,
    pub range: Option<[u64; 2]>,
    pub content_hash: Option<String>,
    pub msg_id: String,
    pub from: String,
    pub msg_type: String,
    pub ts: String,
    pub topic: Option<String>,
    pub summary: String,
    /// The conversation head this ref hangs off (walk of/reply_to/supersedes).
    pub thread_root: String,
    /// Folded status when the thread root is a request (OPEN|CLAIMED|…), else None.
    pub request_status: Option<&'static str>,
}

/// Reverse index — `(repo, path) → refs`, folded from the log. The backbone of
/// "given this code, what conversations reference it": a PURE projection (no server
/// index), rebuilt per query, mirroring `Board::fold`. `git blame` is a later
/// precision layer, never the discovery mechanism (design/40).
#[derive(Debug, Clone, Default)]
pub struct RefIndex {
    pub by_file: BTreeMap<(String, String), Vec<RefHit>>,
}

impl RefIndex {
    pub fn fold(msgs: &[Message]) -> RefIndex {
        let mut idx = RefIndex::default();
        for m in msgs {
            if m.front.refs.is_empty() {
                continue;
            }
            let root = thread_root(msgs, m);
            let rstatus =
                (root.front.msg_type == "request").then(|| request_status(msgs, &root.front.id));
            for r in &m.front.refs {
                idx.by_file.entry((r.repo.clone(), r.path.clone())).or_default().push(RefHit {
                    repo: r.repo.clone(),
                    path: r.path.clone(),
                    sha: r.sha.clone(),
                    range: r.range,
                    content_hash: r.content_hash.clone(),
                    msg_id: m.front.id.clone(),
                    from: m.front.from.clone(),
                    msg_type: m.front.msg_type.clone(),
                    ts: m.front.ts.clone(),
                    topic: m.front.topic.clone(),
                    summary: m.summary_line(),
                    thread_root: root.front.id.clone(),
                    request_status: rstatus,
                });
            }
        }
        idx
    }

    /// Hits matching `repo` (+ optional `path`), overlapping `range` when given. A hit
    /// with no range matches any line query (a whole-file reference). Newest-first.
    pub fn query(&self, repo: &str, path: Option<&str>, range: Option<[u64; 2]>) -> Vec<&RefHit> {
        let overlaps = |hr: Option<[u64; 2]>| match (hr, range) {
            (_, None) => true,
            (None, Some(_)) => true,
            (Some(h), Some(q)) => h[0] <= q[1] && q[0] <= h[1],
        };
        let mut out: Vec<&RefHit> = self
            .by_file
            .iter()
            .filter(|((rp, pt), _)| rp == repo && path.is_none_or(|p| pt == p))
            .flat_map(|(_, hits)| hits.iter())
            .filter(|h| overlaps(h.range))
            .collect();
        out.sort_by(|a, b| b.msg_id.cmp(&a.msg_id));
        out
    }
}

/// One agent as the roster/presence fold sees it — resolved display fields so a
/// renderer only formats. `presence` is carried raw so the renderer computes
/// liveness against its own `now`.
#[derive(Debug, Clone)]
pub struct AgentRow {
    pub id: String,
    pub display: String,
    pub desc: Option<String>,
    pub expected_host: Option<String>,
    /// Most recent post ts (RFC3339) and the host it came from, if any.
    pub last_ts: Option<String>,
    pub last_host: Option<String>,
    pub presence: Option<presence::Presence>,
    /// Cross-hub appearances of the same signing key: (hub_label, role) pairs (F3).
    pub xhub: Vec<(String, String)>,
}

/// Fold the agent list: union of roster ids, observed authors, and presence roles,
/// each resolved to display + last-post + liveness + cross-hub identity. Pure over
/// the passed data — the caller supplies presence and the cross-hub index (both of
/// which involve I/O it controls: a presence fetch, the `~/.confer/hubs.json` scan).
pub fn agents(
    msgs: &[Message],
    roster: &roster::Roster,
    pres: &std::collections::HashMap<String, presence::Presence>,
    xhub: &std::collections::HashMap<String, Vec<(String, String)>>,
) -> Vec<AgentRow> {
    use std::collections::HashMap;
    // last-posted (ts, host) per role id.
    let mut last: HashMap<String, (String, Option<String>)> = HashMap::new();
    for m in msgs {
        let e = last.entry(m.front.from.clone()).or_insert_with(|| (String::new(), None));
        if m.front.ts > e.0 {
            *e = (m.front.ts.clone(), m.front.host.clone());
        }
    }
    // union of roster ids + observed authors + presence roles.
    let mut ids: Vec<String> = roster.keys().cloned().collect();
    for k in last.keys().chain(pres.keys()) {
        if !roster.contains_key(k) {
            ids.push(k.clone());
        }
    }
    ids.sort();
    ids.dedup();

    ids.into_iter()
        .map(|id| {
            let xh = roster::pubkey(roster, &id)
                .and_then(|pk| xhub.get(pk))
                .cloned()
                .unwrap_or_default();
            let (last_ts, last_host) = match last.get(&id) {
                Some((t, h)) if !t.is_empty() => (Some(t.clone()), h.clone()),
                _ => (None, None),
            };
            AgentRow {
                display: roster::display(roster, &id).to_string(),
                desc: roster.get(&id).and_then(|r| r.desc.clone()),
                expected_host: roster::host(roster, &id).map(str::to_string),
                last_ts,
                last_host,
                presence: pres.get(&id).cloned(),
                xhub: xh,
                id,
            }
        })
        .collect()
}

/// Free disk (GB) at `root`'s filesystem — a resilience signal (low disk stalls
/// git/watch). `None` if `df` is unavailable/unparsable.
pub fn disk_free_gb(root: &std::path::Path) -> Option<f64> {
    let o = std::process::Command::new("df").args(["-Pk", &root.to_string_lossy()]).output().ok()?;
    if !o.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&o.stdout);
    let avail_kb: f64 = s.lines().nth(1)?.split_whitespace().nth(3)?.parse().ok()?;
    Some(avail_kb / 1024.0 / 1024.0)
}

/// How many recent messages a snapshot's activity tail keeps.
pub const TAIL: usize = 40;

/// Folded health of one hub. Local-only probes here; `reachable` is left `None`
/// (unknown) for a live front-end's own sync worker to fill from its integrate
/// outcome — never a per-tick `ls-remote`.
#[derive(Clone)]
pub struct Health {
    pub role: String,
    pub reachable: Option<bool>,
    pub pending: Option<u64>,
    pub behind: Option<u64>,
    pub watch: Option<watchlock::WatchState>,
    pub disk_gb: Option<f64>,
}

/// One line in the activity tail (raw ids; the renderer resolves display).
#[derive(Clone)]
pub struct TailItem {
    pub time: String,
    pub from: String,
    pub to: Vec<String>,
    pub kind: String,
    pub summary: String,
}

/// A hub folded into one render-ready struct — shared by the TUI and the web view
/// so both show identical state. Everything a front-end needs, no front-end types.
pub struct Snapshot {
    pub dir: PathBuf,
    pub label: String,
    pub roster: roster::Roster,
    pub board: Board,
    pub agents: Vec<AgentRow>,
    pub tail: Vec<TailItem>,
    pub health: Health,
    pub error: Option<String>,
}

impl Snapshot {
    /// Fold a hub from its local clone. `fetch` controls whether presence refs are
    /// fetched over the network (a live worker passes true; a cheap re-fold passes
    /// false and reads whatever presence is already local). A non-hub or unreadable
    /// dir folds to an error snapshot rather than failing.
    pub fn load(dir: PathBuf, fetch: bool) -> Snapshot {
        let label = crosshub::hub_label(&dir);
        let now = Utc::now();
        if !dir.join("threads").is_dir() && !dir.join("roles").is_dir() {
            return Snapshot::errored(dir, label, "not a confer hub (no threads/ or roles/)");
        }
        let roster = roster::load(&dir);
        let msgs = match store::all_messages(&dir) {
            Ok(m) => m,
            Err(e) => return Snapshot::errored(dir, label, &format!("cannot read messages: {e}")),
        };
        let pres: std::collections::HashMap<String, presence::Presence> = presence::load_all(&dir, fetch)
            .into_iter()
            .map(|p| (p.role.clone(), p))
            .collect();
        let xhub = crosshub::appearances(&dir);
        let board = Board::fold(&msgs, now);
        let agents = agents(&msgs, &roster, &pres, &xhub);

        let mut sorted: Vec<&Message> = msgs.iter().collect();
        sorted.sort_by(|a, b| a.front.id.cmp(&b.front.id));
        let tail: Vec<TailItem> = sorted
            .iter()
            .rev()
            .take(TAIL)
            .rev()
            .map(|m| TailItem {
                time: m.front.ts.get(11..16).unwrap_or("").to_string(),
                from: m.front.from.clone(),
                to: m.front.to.clone(),
                kind: m.front.msg_type.clone(),
                summary: m.summary_line(),
            })
            .collect();

        let health = probe_health(&dir);
        Snapshot { dir, label, roster, board, agents, tail, health, error: None }
    }

    pub fn errored(dir: PathBuf, label: String, msg: &str) -> Snapshot {
        Snapshot {
            dir,
            label,
            roster: roster::Roster::new(),
            board: Board::default(),
            agents: Vec::new(),
            tail: Vec::new(),
            health: Health { role: String::new(), reachable: Some(false), pending: None, behind: None, watch: None, disk_gb: None },
            error: Some(msg.to_string()),
        }
    }
}

/// Local-only health probe (no network): role, pending/behind vs upstream, watch
/// state, free disk. `reachable` stays `None` until a live worker sets it.
pub fn probe_health(dir: &Path) -> Health {
    let role = config::resolve_role(None, dir).unwrap_or_default();
    let count = |range: &str| {
        gitcmd::output(dir, &["rev-list", "--count", range])
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u64>().ok())
    };
    let watch = if role.is_empty() {
        None
    } else {
        Some(watchlock::classify(&watchlock::inspect(&config::hub_key(dir), &role, 90), env!("CARGO_PKG_VERSION")))
    };
    Health {
        reachable: None,
        pending: count("@{u}..HEAD"),
        behind: count("HEAD..@{u}"),
        watch,
        disk_gb: disk_free_gb(dir),
        role,
    }
}

/// Compact target rendering: display names joined, `all` kept literal, empty → `—`.
pub fn render_targets(roster: &roster::Roster, targets: &[String]) -> String {
    if targets.is_empty() {
        return "—".to_string();
    }
    targets
        .iter()
        .map(|t| if t == "all" { "all".to_string() } else { roster::display(roster, t).to_string() })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Frontmatter;

    fn m(id: &str, from: &str, ty: &str, of: Option<&str>, sup: Option<&str>, res: Option<&str>) -> Message {
        Message {
            front: Frontmatter {
                id: id.into(),
                from: from.into(),
                msg_type: ty.into(),
                ts: "2026-07-16T00:00:00Z".into(),
                host: None,
                to: vec![],
                cc: vec![],
                priority: None,
                topic: None,
                reply_to: None,
                of: of.map(String::from),
                supersedes: sup.map(String::from),
                resolution: res.map(String::from),
                defer: false,
                via: None,
                src: None,
                summary: Some("s".into()),
                refs: vec![],
            },
            body: String::new(),
        }
    }

    #[test]
    fn a_supersede_after_done_does_not_erase_the_completion() {
        // done at 01B, then superseded at 01C — the FIRST terminal in fold order wins → still DONE.
        let msgs = vec![
            m("01A", "alice", "request", None, None, None),
            m("01B", "bob", "done", Some("01A"), None, None),
            m("01C", "carol", "supersede", None, Some("01A"), None),
        ];
        assert_eq!(request_status(&msgs, "01A"), "DONE");
        // superseded BEFORE any done → correctly SUPERSEDED.
        let msgs = vec![
            m("01A", "alice", "request", None, None, None),
            m("01B", "carol", "supersede", None, Some("01A"), None),
            m("01C", "bob", "done", Some("01A"), None, None),
        ];
        assert_eq!(request_status(&msgs, "01A"), "SUPERSEDED");
    }

    #[test]
    fn done_resolution_is_the_closing_dones_not_a_later_wont_do() {
        // The closing done (01B) is a real completion (no resolution); a LATER `done --as wont-do`
        // must NOT be picked up and mislabel the completed work.
        let msgs = vec![
            m("01A", "alice", "request", None, None, None),
            m("01B", "bob", "done", Some("01A"), None, None),
            m("01C", "alice", "done", Some("01A"), None, Some("wont-do")),
        ];
        assert_eq!(done_resolution(&msgs, "01A"), None);
        // When the closing done itself carries a resolution, use it.
        let msgs = vec![
            m("01A", "alice", "request", None, None, None),
            m("01B", "bob", "done", Some("01A"), None, Some("duplicate")),
        ];
        assert_eq!(done_resolution(&msgs, "01A").as_deref(), Some("duplicate"));
    }
}
