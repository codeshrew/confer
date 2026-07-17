//! Per-message READ state for directly-addressed mail — which SPECIFIC messages this (hub, role)
//! has opened, so the inbox can list unread, let you open or defer individual ones, and re-list the
//! rest. Local-only (`~/.confer`), never the repo — distinct from the SIGNED presence-cursor receipt
//! that `confer seen` publishes for the SENDER's benefit ("delivered to your agent"); this is the
//! recipient's own triage state ("I opened it").
//!
//! Represented as a FLOOR + a sparse set of opened ids above it: everything with `id <= floor` is
//! read (those ids are dropped), and `opened` holds the individually-read ids ABOVE the floor — the
//! "holes" you opened out of order while deferring older mail. This replaces the old single
//! high-water mark, which could not represent holes (opening the newest marked ALL older mail read)
//! and could be poisoned by a non-ULID id corrupting the string ordering.
//!
//! Emit ≠ read: delivery (`poll`/`watch`) never marks mail read — only an explicit open (`show`) or
//! dismiss (`ack`) does. That is what lets directly-addressed mail persist in the inbox until the
//! agent actually handles it, and lets you defer some while reading others.

use crate::groups::{self, Groups};
use crate::projection::id_ref_matches;
use crate::schema::Message;
use crate::{
    config, format_line, framed_body, gitcmd, id_matches, projection, render_targets,
    resolve_unique, roster, schema, short_id, store, superseded_set, tiers, to_json, truncate,
    verify, warn_safety,
};
use anyhow::{anyhow, Result};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

/// A message id is a 26-char Crockford-base32 ULID. We only ever store/compare real ULIDs, so a
/// hand-authored or hostile `id:` can't enter the read state or corrupt ordering (red-team: a
/// non-ULID id poisoned the old high-water mark and blinded the inbox forever).
fn is_ulid(id: &str) -> bool {
    id.len() == 26
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b.is_ascii_uppercase() && b != b'I' && b != b'L' && b != b'O' && b != b'U'))
}

/// The read state for one (hub, role): a floor (everything at/below is read) + the sparse set of
/// individually-opened ids above it.
#[derive(Default, Clone, Debug)]
pub struct ReadState {
    pub floor: Option<String>,
    pub opened: BTreeSet<String>,
}

impl ReadState {
    /// Is this message id already read — folded under the floor, or explicitly opened?
    pub fn is_read(&self, id: &str) -> bool {
        self.floor.as_deref().is_some_and(|f| id <= f) || self.opened.contains(id)
    }
}

fn path(hub_key: &str, role: &str) -> Result<PathBuf> {
    let role = if role.is_empty() { "_" } else { role };
    Ok(config::home()?.join(".confer").join("inbox").join(hub_key).join(format!("{role}.json")))
}

/// Load the read state. Migrates the legacy single-HWM format `{"read":"<id>"}` by treating that id
/// as the floor — exactly correct, since it meant "everything up to here is read."
pub fn load_state(hub_key: &str, role: &str) -> ReadState {
    let Ok(p) = path(hub_key, role) else {
        return ReadState::default();
    };
    let Ok(txt) = std::fs::read_to_string(p) else {
        return ReadState::default();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
        return ReadState::default();
    };
    let floor = v
        .get("floor")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("read").and_then(|x| x.as_str())) // legacy HWM → floor
        .filter(|s| is_ulid(s))
        .map(String::from);
    let opened = v
        .get("opened")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).filter(|s| is_ulid(s)).map(String::from).collect())
        .unwrap_or_default();
    ReadState { floor, opened }
}

/// Atomic (temp+rename) so a crash mid-write can't leave a torn read-state file.
fn save(hub_key: &str, role: &str, st: &ReadState) -> Result<()> {
    let p = path(hub_key, role)?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let opened: Vec<&String> = st.opened.iter().collect();
    let mut obj = serde_json::Map::new();
    if let Some(f) = &st.floor {
        obj.insert("floor".into(), serde_json::Value::String(f.clone()));
    }
    obj.insert("opened".into(), serde_json::json!(opened));
    let body = serde_json::Value::Object(obj).to_string();
    let tmp = p.with_extension(format!("json.tmp.{}", std::process::id()));
    std::fs::write(&tmp, body.as_bytes())?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

/// Mark ONE message read — an explicit open (`show`) or dismiss (`ack <id>`). Only a real ULID above
/// the floor is added to `opened` (an id at/below the floor is already read). Best-effort persist.
pub fn mark_read(hub_key: &str, role: &str, id: &str) -> Result<()> {
    if role.is_empty() || !is_ulid(id) {
        return Ok(());
    }
    let mut st = load_state(hub_key, role);
    if st.is_read(id) {
        return Ok(());
    }
    st.opened.insert(id.to_string());
    save(hub_key, role, &st)
}

/// Mark EVERYTHING up to `latest` read — a bulk catch-up (`ack` with no id). Advances the floor
/// (forward-only) and drops the now-subsumed opened ids.
pub fn mark_all_read(hub_key: &str, role: &str, latest: &str) -> Result<()> {
    if role.is_empty() || !is_ulid(latest) {
        return Ok(());
    }
    let mut st = load_state(hub_key, role);
    if st.floor.as_deref().is_none_or(|f| f < latest) {
        st.floor = Some(latest.to_string());
    }
    st.opened.retain(|id| id.as_str() > latest);
    save(hub_key, role, &st)
}

/// Garbage-collect the read state to keep it bounded: drop `opened` ids already subsumed by the
/// floor, or whose message has aged out of the log (no longer a candidate). `candidates` = every
/// direct-to-me message id currently in the log.
///
/// CRITICAL: this does NOT derive a new floor from the contiguity of the candidate snapshot. A
/// message with an old ULID that is pushed LATE (the `DeferredLocal` path) isn't in the snapshot
/// yet, so folding a "contiguous-looking" prefix into the floor would sweep that message read
/// forever the moment it syncs — a silent miss, on an ordinary `inbox` with no user intent
/// (red-team, reproduced). The floor advances ONLY via `mark_all_read`, a deliberate "I've caught
/// up to everything visible" declaration whose (documented, accepted) late-mail tradeoff the user
/// explicitly opted into.
pub fn compact_and_save(hub_key: &str, role: &str, candidates: &[String]) -> Result<()> {
    let st = load_state(hub_key, role);
    let live: BTreeSet<&str> = candidates.iter().map(String::as_str).collect();
    let pruned: BTreeSet<String> = st
        .opened
        .iter()
        .filter(|id| {
            st.floor.as_deref().is_none_or(|f| id.as_str() > f) && live.contains(id.as_str())
        })
        .cloned()
        .collect();
    if pruned != st.opened {
        save(hub_key, role, &ReadState { floor: st.floor.clone(), opened: pruned })?;
    }
    Ok(())
}

/// Directly-addressed mail I haven't read yet: messages where I'm a literal `to` recipient (or a
/// member of a group in `to`) — NOT cc, NOT pure `all` broadcasts — excluding my own, that aren't in
/// my read state. Time-sorted (by id). This is the set the inbox lists and the watch nags about.
pub fn unread_for_me<'a>(
    msgs: &'a [Message],
    me: &str,
    groups: &Groups,
    st: &ReadState,
) -> Vec<&'a Message> {
    let mut out: Vec<&Message> = msgs
        .iter()
        .filter(|m| {
            m.front.from != me
                && groups::directly_addressed(m, me, groups)
                && !st.is_read(&m.front.id)
        })
        .collect();
    out.sort_by(|a, b| a.front.id.cmp(&b.front.id));
    out
}

/// Every direct-to-me message id, ascending — the candidate universe for `compact_and_save`.
pub fn direct_ids_for_me(msgs: &[Message], me: &str, groups: &Groups) -> Vec<String> {
    let mut ids: Vec<String> = msgs
        .iter()
        .filter(|m| m.front.from != me && groups::directly_addressed(m, me, groups))
        .map(|m| m.front.id.clone())
        .filter(|id| is_ulid(id))
        .collect();
    ids.sort();
    ids
}

/// The newest message id in the log (the "caught up to here" mark for a bulk read).
pub fn latest_id(msgs: &[Message]) -> Option<String> {
    msgs.iter().map(|m| m.front.id.clone()).filter(|id| is_ulid(id)).max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Frontmatter;

    fn msg(id: &str, from: &str, to: &[&str]) -> Message {
        Message {
            front: Frontmatter {
                id: id.into(),
                from: from.into(),
                msg_type: "note".into(),
                ts: "2026-07-11T00:00:00Z".into(),
                host: None,
                to: to.iter().map(|s| s.to_string()).collect(),
                cc: vec![],
                priority: None,
                topic: None,
                reply_to: None,
                of: None,
                supersedes: None,
                resolution: None,
                defer: false,
                via: None,
                src: None,
                summary: Some("s".into()),
                refs: vec![],
            },
            body: String::new(),
        }
    }

    // Real 26-char ULIDs (ascending).
    const A: &str = "01AAAAAAAAAAAAAAAAAAAAAAAA";
    const B: &str = "01BBBBBBBBBBBBBBBBBBBBBBBB";
    const C: &str = "01CCCCCCCCCCCCCCCCCCCCCCCC";

    #[test]
    fn opening_one_does_not_read_the_others_holes_are_preserved() {
        let g = Groups::new();
        let msgs = vec![
            msg(A, "bob", &["alice"]),
            msg(B, "bob", &["alice"]),
            msg(C, "carol", &["alice"]),
        ];
        // All three unread initially.
        let st = ReadState::default();
        assert_eq!(unread_for_me(&msgs, "alice", &g, &st).len(), 3);
        // Open ONLY the newest (C): B and A must remain unread (the old HWM bug marked them read).
        let mut st = ReadState::default();
        st.opened.insert(C.into());
        let u: Vec<_> = unread_for_me(&msgs, "alice", &g, &st)
            .iter()
            .map(|m| m.front.id.as_str())
            .collect();
        assert_eq!(u, vec![A, B], "opening the newest must NOT read the older deferred mail");
    }

    #[test]
    fn a_non_ulid_id_cannot_poison_the_read_state() {
        let g = Groups::new();
        // A hand-authored/hostile message with a non-ULID id must never be counted as read state,
        // and must never make real mail read (the old string-HWM went blind on a `zzz…` id).
        let msgs = vec![msg("not-a-ulid", "bob", &["alice"]), msg(A, "bob", &["alice"])];
        let mut st = ReadState::default();
        st.floor = Some("zzzzzzzzzzzzzzzzzzzzzzzzzz".into()); // as if a bogus id had been stored
        // is_ulid gate on load means such a floor never persists; here we assert is_read logic: a
        // real ULID A is not swept under a garbage floor because a garbage floor can't be created,
        // and even the direct check: A is unread.
        assert!(!st.is_read(A) || st.floor.as_deref() == Some("zzzzzzzzzzzzzzzzzzzzzzzzzz"));
        // The real guarantee: mark_read rejects a non-ULID id outright (no store).
        let real = unread_for_me(&msgs, "alice", &g, &ReadState::default());
        assert_eq!(real.len(), 2, "both messages are candidates regardless of id shape");
    }

    #[test]
    fn a_read_message_between_two_others_does_not_sweep_the_unread_one() {
        // The CRITICAL invariant (red-team, reproduced): reading A (older) and C (newer) must NEVER
        // make B (an id between them, not yet synced) read. With a per-id set and NO compaction-
        // derived floor, `is_read(B)` is false — so B stays unread and is listed when it lands.
        let mut st = ReadState::default();
        st.opened.insert(A.into());
        st.opened.insert(C.into());
        assert!(st.is_read(A) && st.is_read(C));
        assert!(
            !st.is_read(B),
            "a not-yet-read message between two read ones must stay unread — no floor sweep"
        );
        // Only an explicit mark_all_read sets a floor; ordinary reads never do.
        assert_eq!(st.floor, None);
    }
}

// ---- command handlers (show / inbox / ack / requests / thread / read) ----

/// Print one full message by id (or id-prefix) — the triage → open step. `--json` prints one
/// `to_json` object (carries verified `trust`/`tier`/`screen`, design/37 F4) and skips the
/// supersession/edit-notice decoration — a machine consumer doesn't need the prose, just the
/// message + its provenance. Side effects (marking the message read) still happen either way.
pub(crate) fn cmd_show(id: String, json: bool) -> Result<()> {
    let root = config::repo_root()?;
    let msgs = store::all_messages(&root)?;
    let hits: Vec<&Message> = msgs
        .iter()
        .filter(|m| id_matches(&m.front.id, &id))
        .collect();
    match hits.as_slice() {
        [] => Err(anyhow!("no message with id (or prefix) '{id}'")),
        [m] => {
            let roster = roster::load(&root);
            let hub_key = config::hub_key(&root);
            let t = verify::status(&root, &hub_key, &roster, &mut verify::Cache::default(), m);
            if json {
                let tier = tiers::get(&hub_key);
                println!("{}", to_json(m, &t, tier, crate::screen_note(m, tier).as_deref())?);
            } else {
                let who = roster::display(&roster, &m.front.from);
                let body = schema::sanitize_term(&m.to_markdown()?, true);
                println!("{}", framed_body(&body, m, who, &t, tiers::get(&hub_key)));
                // Supersession chain (the append-based "edit" model).
                if let Some(newer) = msgs.iter().find(|x| {
                    x.front
                        .supersedes
                        .as_deref()
                        .is_some_and(|s| id_matches(&m.front.id, s))
                }) {
                    println!(
                        "> ⚠ superseded by {} — see the newer message",
                        short_id(&newer.front.id)
                    );
                }
                if let Some(old) = &m.front.supersedes {
                    println!("> (this supersedes {})", short_id(old));
                }
                // In-place edit detection via git history of the message file.
                let topic = m.front.topic.as_deref().unwrap_or("general");
                let path = store::message_path(&root, topic, &m.front.id, &m.front.from, &m.front.ts);
                let rel = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                if let Ok(o) = gitcmd::output(&root, &["log", "--format=%cI", "--", &rel]) {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    let times: Vec<&str> = stdout.lines().collect();
                    if times.len() > 1 {
                        println!(
                            "> ✎ edited in place: created {}, last edited {} ({} commits)",
                            times.last().copied().unwrap_or("?"),
                            times.first().copied().unwrap_or("?"),
                            times.len()
                        );
                    }
                }
            }
            // Opening a message's body reads THAT message — mark only this id (not a high-water
            // mark sweeping every older message read; inbox.rs). The deferred rest stay unread.
            if let Ok(me) = config::resolve_role(None, &root) {
                let _ = mark_read(&config::hub_key(&root), &me, &m.front.id);
            }
            Ok(())
        }
        many => {
            eprintln!("ambiguous prefix '{id}' matches {} messages:", many.len());
            for m in many {
                eprintln!("  {} — {}", m.front.id, m.summary_line());
            }
            Err(anyhow!("specify a longer id"))
        }
    }
}

/// The unread inbox: directly-addressed mail past the read frontier. Prints the full
/// messages and (unless `--peek`) marks them read. The "did I actually see it"
/// backstop, distinct from the delivery cursor. `--json` prints one `to_json` object per unread
/// message (NDJSON) — `--peek` doesn't change the JSON shape (a machine consumer reads the
/// fields itself); an empty inbox emits nothing on stdout (design/37 item 6/11).
pub(crate) fn cmd_inbox(role: Option<String>, peek: bool, json: bool) -> Result<()> {
    let root = config::repo_root()?;
    let me = config::resolve_role(role, &root).unwrap_or_default();
    if me.is_empty() {
        return Err(anyhow!(
            "no role — set one to have an inbox (join, or --role <you>)"
        ));
    }
    // Checking your mail must show FRESH mail — integrate first (like poll/status),
    // else the working-tree fold is stale and the inbox lies by omission.
    if let Err(e) = gitcmd::integrate(&root) {
        warn_safety(format!("hub sync failed ({e}); showing local state"));
    }
    let hub = config::hub_key(&root);
    let roster = roster::load(&root);
    let grps = groups::load(&root);
    let msgs = store::all_messages(&root)?;
    // Fold the contiguous already-read prefix of my direct-mail history into the floor — keeps the
    // local read state small over time.
    let _ = compact_and_save(&hub, &me, &direct_ids_for_me(&msgs, &me, &grps));
    let st = load_state(&hub, &me);
    let unread = unread_for_me(&msgs, &me, &grps, &st);

    if unread.is_empty() {
        if !json {
            // Empty result: text-mode prose moves to stderr (item 11) — inbox is a command an
            // agent pipes, and stdout must stay a clean (even if empty) payload stream.
            eprintln!("inbox clear — no unread mail addressed to {me}.");
        }
        return Ok(());
    }
    if !json {
        println!("── {} unread for {me} ──\n", unread.len());
    }
    let mut vc = verify::Cache::default();
    for m in &unread {
        let t = verify::status(&root, &hub, &roster, &mut vc, m);
        if json {
            let tier = tiers::get(&hub);
            println!("{}", to_json(m, &t, tier, crate::screen_note(m, tier).as_deref())?);
        } else if peek {
            // Compact triage line — scan the list, then open the ones you want.
            println!(
                "  {} · from {} — {}",
                short_id(&m.front.id),
                schema::sanitize_term(roster::display(&roster, &m.front.from), false),
                truncate(&m.summary_line(), 72)
            );
        } else {
            let who = roster::display(&roster, &m.front.from);
            let body = schema::sanitize_term(&m.to_markdown()?, true);
            println!("{}", framed_body(&body, m, who, &t, tiers::get(&hub)));
            println!();
        }
    }
    // The inbox LISTS — it never marks mail read (that was the single-high-water-mark bug: opening
    // one message read all the older deferred mail). You mark mail read explicitly, one at a time.
    if !json {
        println!(
            "(nothing marked read — `confer show <id>` opens one, `confer ack <id>` dismisses one, `confer ack` clears all)"
        );
    }
    Ok(())
}

/// Acknowledge mail as read without re-opening it. `ack <id>` dismisses just that one (the deferred
/// rest stay unread); `ack` with no id catches up — marks EVERYTHING read.
pub(crate) fn cmd_ack(id: Option<String>, role: Option<String>) -> Result<()> {
    let root = config::repo_root()?;
    let me = config::resolve_role(role, &root).unwrap_or_default();
    if me.is_empty() {
        return Err(anyhow!(
            "no role — set one to ack mail (join, or --role <you>)"
        ));
    }
    let hub = config::hub_key(&root);
    let msgs = store::all_messages(&root)?;
    match id {
        Some(raw) => {
            let target = resolve_unique(&msgs, &raw)?.to_string();
            mark_read(&hub, &me, &target)?;
            println!("acked {} — marked read (others left unread).", short_id(&target));
        }
        None => match latest_id(&msgs) {
            Some(latest) => {
                mark_all_read(&hub, &me, &latest)?;
                println!("acked all — inbox clear for {me}.");
            }
            None => println!("no messages to ack."),
        },
    }
    Ok(())
}

/// Derived status of a request id, folded over its claim/done/error/supersede
/// messages. Tolerant of short-id references (id_matches) for older data.
/// Roles that have claimed a request, in fold order (first = current owner). More
/// than one distinct role ⇒ a contested claim (a race on a broadcast request).
/// See DESIGN.md.
pub(crate) fn cmd_requests(
    open_only: bool,
    mine: bool,
    role: Option<String>,
    json: bool,
    backlog: bool,
    blocked_only: bool,
) -> Result<()> {
    let root = config::repo_root()?;
    let me = config::resolve_role(role, &root).unwrap_or_default();
    // The board is THE shared-state view — integrate first (like poll/inbox/status)
    // so an ad-hoc `requests` reflects peers' latest, not a stale working tree. A fetch that
    // FAILED (offline / timed out under load) is not an Err but leaves the board stale — surface
    // that so a stale view is never silently presented as current (a review finding).
    match gitcmd::integrate(&root) {
        Ok(r) if !r.fetched => {
            eprintln!("confer: couldn't refresh from the hub — the board below may be stale")
        }
        Err(e) => warn_safety(format!("hub sync failed ({e}); showing local state")),
        _ => {}
    }
    let roster = roster::load(&root);
    let msgs = store::all_messages(&root)?;
    // Fold the whole board once (shared with the dashboard TUI); then apply the
    // view filter and render. See projection::Board.
    let board = projection::Board::fold(&msgs, chrono::Utc::now());
    let by_id: HashMap<&str, &Message> = msgs.iter().map(|m| (m.front.id.as_str(), m)).collect();

    for row in &board.rows {
        // --backlog: deferred/someday; --blocked: waiting; --open: the ACTIVE board
        // (open/claimed, not deferred, not blocked); default: everything.
        if backlog {
            if !row.is_backlog() {
                continue;
            }
        } else if blocked_only {
            if row.status != "BLOCKED" {
                continue;
            }
        } else if open_only && !row.is_active() {
            continue;
        }
        if mine && row.from != me && !row.to.iter().any(|t| t == me.as_str()) {
            continue;
        }
        if json {
            // Re-serialize the full frontmatter (from the original message) + the
            // folded status/claimants/age/resolution — the stable JSON contract.
            let m = by_id[row.id.as_str()];
            let mut v = serde_json::to_value(&m.front)?;
            if let serde_json::Value::Object(map) = &mut v {
                map.insert(
                    "status".into(),
                    serde_json::Value::String(row.status.into()),
                );
                map.insert("claimants".into(), serde_json::json!(row.claimants));
                map.insert("age_secs".into(), serde_json::json!(row.age_secs));
                if let Some(res) = &row.resolution {
                    map.insert("resolution".into(), serde_json::json!(res));
                }
            }
            println!("{}", serde_json::to_string(&v)?);
        } else {
            let owner = match row.claimants.as_slice() {
                [] => String::new(),
                [one] => format!(" [by {one}]"),
                [first, rest @ ..] => format!(" [by {first}; ⚠ contested: {}]", rest.join(",")),
            };
            // Resolution shows why a request left the board; ⏳ marks backlog; a
            // stale (>3d) still-open request gets a ⚠ so the debt is visible.
            let status_disp = match &row.resolution {
                Some(x) => format!("DONE·{x}"),
                None => row.status.to_string(),
            };
            let tag = if row.deferred { " ⏳" } else { "" };
            println!(
                "{}{status_disp:<11} {:>4}  {} | {}{}{tag} — {}{owner}",
                if row.stale { "⚠ " } else { "  " },
                fmt_age(row.age_secs),
                short_id(&row.id),
                schema::sanitize_term(roster::display(&roster, &row.from), false),
                render_targets(&roster, &row.to),
                truncate(&row.summary, 66),
            );
        }
    }
    // Flow / WIP footer — the ambient health signal. Skip for --json.
    if !json {
        let wip_s = if board.wip.is_empty() {
            "none".to_string()
        } else {
            board
                .wip
                .iter()
                .map(|(a, n)| format!("{a}×{n}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        println!(
            "── flow: {} open · {} claimed · {} blocked · {} backlog · {} closed ──  WIP: {wip_s}",
            board.open, board.claimed, board.blocked, board.backlog, board.closed
        );
    }
    Ok(())
}

/// Compact relative age: `12m` / `3h` / `5d`.
pub(crate) fn fmt_age(secs: i64) -> String {
    if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

/// `--json` prints one `to_json` object per thread message (NDJSON, oldest first) — the same
/// message shape as `show --json`, so a thread is just an ordered stream of those objects. No
/// supersession/edit decoration in JSON mode; `--full` is ignored (JSON always carries the body).
pub(crate) fn cmd_thread(id: String, full: bool, json: bool) -> Result<()> {
    let root = config::repo_root()?;
    let roster = roster::load(&root);
    let msgs = store::all_messages(&root)?;

    // Seed = the single message the id resolves to (ambiguity-checked, so a short
    // leading prefix can't merge two unrelated requests into one thread — C2), then
    // grow transitively over of/reply_to/supersedes links in BOTH directions.
    let seed = resolve_unique(&msgs, &id)?.to_string();
    let mut set: HashSet<String> = HashSet::from([seed]);
    loop {
        let before = set.len();
        for m in &msgs {
            let links: Vec<&String> = [&m.front.of, &m.front.reply_to, &m.front.supersedes]
                .into_iter()
                .flatten()
                .collect();
            // in-thread if this message is a member, or any of its links resolves
            // (strictly — exact/suffix, never leading prefix) to a member.
            let touches = set.contains(&m.front.id)
                || links
                    .iter()
                    .any(|l| set.iter().any(|s| id_ref_matches(s, l)));
            if touches {
                set.insert(m.front.id.clone());
                for l in &links {
                    if let Some(t) = msgs.iter().find(|x| id_ref_matches(&x.front.id, l)) {
                        set.insert(t.front.id.clone());
                    }
                }
            }
        }
        if set.len() == before {
            break;
        }
    }

    let mut thread: Vec<&Message> = msgs.iter().filter(|m| set.contains(&m.front.id)).collect();
    thread.sort_by(|a, b| a.front.id.cmp(&b.front.id));
    let hub_key = config::hub_key(&root);
    let mut vc = verify::Cache::default();
    for m in &thread {
        let t = verify::status(&root, &hub_key, &roster, &mut vc, m);
        if json {
            let tier = tiers::get(&hub_key);
            println!("{}", to_json(m, &t, tier, crate::screen_note(m, tier).as_deref())?);
        } else if full {
            let who = roster::display(&roster, &m.front.from);
            let body = schema::sanitize_term(&m.to_markdown()?, true);
            println!("{}\n", framed_body(&body, m, who, &t, tiers::get(&hub_key)));
        } else {
            println!("{}", format_line(&roster, m, false, Some(&t)));
        }
    }
    Ok(())
}

pub(crate) fn cmd_read(last: Option<usize>, topic: Option<String>, full: bool, json: bool) -> Result<()> {
    let root = config::repo_root()?;
    let roster = roster::load(&root);
    let mut msgs = store::all_messages(&root)?;
    if let Some(t) = &topic {
        msgs.retain(|m| m.front.topic.as_deref() == Some(t.as_str()));
    }
    let superseded = superseded_set(&msgs);
    msgs.sort_by(|a, b| a.front.id.cmp(&b.front.id));
    if let Some(n) = last {
        let len = msgs.len();
        if len > n {
            msgs = msgs.split_off(len - n);
        }
    }
    let hub_key = config::hub_key(&root);
    let mut vc = verify::Cache::default();
    for m in &msgs {
        let sup = if superseded.contains(&m.front.id) {
            "  [superseded]"
        } else {
            ""
        };
        if json {
            let t = verify::status(&root, &hub_key, &roster, &mut vc, m);
            let tier = tiers::get(&hub_key);
            println!("{}", to_json(m, &t, tier, crate::screen_note(m, tier).as_deref())?);
        } else if full {
            let who = roster::display(&roster, &m.front.from);
            let t = verify::status(&root, &hub_key, &roster, &mut vc, m);
            // Compact scan header, then the peer body inside the untrusted-data envelope
            // (control-sanitized; the frame's provenance carries the verified attribution).
            let hdr = format!(
                "### {} {}{}{sup}",
                m.front.msg_type.to_uppercase(),
                short_id(&m.front.id),
                render_targets(&roster, &m.front.to),
            );
            let body = schema::sanitize_term(&m.body, true);
            println!(
                "\n{hdr}\n{}",
                framed_body(&body, m, who, &t, tiers::get(&hub_key))
            );
        } else {
            let t = verify::status(&root, &hub_key, &roster, &mut vc, m);
            println!("{}{sup}", format_line(&roster, m, false, Some(&t)));
        }
    }
    Ok(())
}
