//! JSON API for `confer serve` — `/api/*` endpoints alongside the existing HTML view
//! (`serve.rs`'s request loop routes here). Read-only, same discipline as the rest of
//! `serve`: handlers only re-fold already-on-disk data (a fresh, cheap local read per
//! request — no network fetch, no lock, no publish). A later step swaps `/` to an
//! embedded SPA that consumes these shapes; the exact camelCase keys here are a
//! contract with that frontend, not incidental.

use crate::schema::{sanitize_term, CodeRef, Message};
use crate::{append, config, crosshub, gitcmd, presence, projection, refcode, repomap, repos, roster, store, verify};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A dispatched API result: an HTTP status + a JSON body (error shape `{"error":".."}`
/// on non-200, always valid JSON otherwise).
pub struct ApiResponse {
    pub status: u16,
    pub body: Value,
}

impl ApiResponse {
    fn ok(body: Value) -> Self {
        Self { status: 200, body }
    }
    fn err(status: u16, msg: impl Into<String>) -> Self {
        Self { status, body: json!({ "error": msg.into() }) }
    }
}

/// Parse a `?k=v&k2=v2` query string into a map (percent/`+` decoded). Last value
/// wins on a repeated key — no endpoint here relies on repeats.
pub fn parse_qs(url: &str) -> HashMap<String, String> {
    let q = url.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut out = HashMap::new();
    for kv in q.split('&').filter(|s| !s.is_empty()) {
        match kv.split_once('=') {
            Some((k, v)) => {
                out.insert(urldecode(k), urldecode(v));
            }
            None => {
                out.insert(urldecode(kv), String::new());
            }
        }
    }
    out
}

/// Minimal percent-decoding (no external dep) — good enough for query VALUES (repo
/// paths, ref targets, ranges). An invalid escape is passed through byte-for-byte
/// rather than erroring; these are diagnostic query params, not trust boundaries.
fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..=i + 2]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(b) => {
                        out.push(b);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A hub's stable-ish API id — reuses the same human label `serve`'s HTML tabs show
/// (`owner/repo`, or `<dir> (local <sha8>)`). Good enough to round-trip in `?hub=`.
pub fn hub_id(dir: &Path) -> String {
    crosshub::hub_label(dir)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let ca = a.canonicalize().unwrap_or_else(|_| a.to_path_buf());
    let cb = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    ca == cb
}

/// Resolve `?hub=<id>` against the server's followed hubs, as an INDEX into `dirs` (and,
/// by construction, the same index into the parallel `cache: Vec<Snapshot>` that `serve`
/// keeps warm). Omitted → the current hub (`config::repo_root()`) if it's one of the
/// followed dirs, else the first. `Some(id)` that matches nothing → `None` (404).
fn resolve_hub_idx(dirs: &[PathBuf], q: &HashMap<String, String>) -> Option<usize> {
    match q.get("hub").filter(|s| !s.is_empty()) {
        Some(id) => dirs.iter().position(|d| &hub_id(d) == id),
        None => {
            if let Ok(cwd) = config::repo_root() {
                if let Some(i) = dirs.iter().position(|d| same_dir(d, &cwd)) {
                    return Some(i);
                }
            }
            if dirs.is_empty() {
                None
            } else {
                Some(0)
            }
        }
    }
}

/// Resolve `?hub=<id>` to the dir itself — for endpoints (`refs`, `code`, `repos`) that
/// only need the path, not a cached Snapshot.
fn resolve_hub<'a>(dirs: &'a [PathBuf], q: &HashMap<String, String>) -> Option<&'a PathBuf> {
    resolve_hub_idx(dirs, q).map(|i| &dirs[i])
}

/// A borrowed-or-owned Snapshot: the common case holds the server's warm-cache lock
/// (populated every ~2s by `serve`'s background fold); the fallback (cache miss / a
/// poisoned lock) does one fresh, uncached `Snapshot::load` so a request is still
/// correct — just not as fast — rather than erroring.
enum SnapHolder<'a> {
    Cached(std::sync::MutexGuard<'a, Vec<projection::Snapshot>>, usize),
    Owned(Box<projection::Snapshot>),
}

impl SnapHolder<'_> {
    fn get(&self) -> &projection::Snapshot {
        match self {
            SnapHolder::Cached(g, i) => &g[*i],
            SnapHolder::Owned(s) => s,
        }
    }
}

fn load_snapshot<'a>(cache: &'a Mutex<Vec<projection::Snapshot>>, dir: &Path, idx: usize) -> SnapHolder<'a> {
    if let Ok(g) = cache.lock() {
        if idx < g.len() {
            return SnapHolder::Cached(g, idx);
        }
    }
    SnapHolder::Owned(Box::new(projection::Snapshot::load(dir.to_path_buf(), false)))
}

fn presence_map(dir: &Path) -> HashMap<String, presence::Presence> {
    presence::load_all(dir, false).into_iter().map(|p| (p.role.clone(), p)).collect()
}

fn hub_json(dirs: &[PathBuf], dir: &Path, agent_count: usize) -> Value {
    let _ = dirs;
    let label = hub_id(dir);
    let name = dir.file_name().and_then(|s| s.to_str()).unwrap_or("hub").to_string();
    let current = config::repo_root().ok().map(|c| same_dir(dir, &c)).unwrap_or(false);
    json!({ "id": label, "label": label, "name": name, "current": current, "agentCount": agent_count })
}

/// Route `path` (already stripped of query) to a handler. Anything unrecognized under
/// `/api/` is a 404 with the same `{"error":".."}` shape as an unknown hub.
pub fn dispatch(dirs: &[PathBuf], cache: &Mutex<Vec<projection::Snapshot>>, path: &str, url: &str) -> ApiResponse {
    let q = parse_qs(url);
    match path {
        "/api/hubs" => hubs(dirs, cache),
        "/api/overview" => overview(dirs, cache, &q),
        "/api/messages" => messages(dirs, cache, &q),
        "/api/thread" => thread(dirs, cache, &q),
        "/api/refs" => refs(dirs, &q),
        "/api/codefiles" => codefiles(dirs, cache, &q),
        "/api/code" => code(dirs, &q),
        "/api/repos" => repos_inventory(dirs, &q),
        _ => ApiResponse::err(404, "no such API endpoint"),
    }
}

/// `GET /api/repos?hub=<id>` — the selected hub's registered repo inventory (design/40
/// `repos/<slug>.md` cards), enriched with THIS machine's local clone-map facts
/// (`cloned`/`clonePath`, from `repomap::path` — never from the hub, which never carries
/// per-machine paths). A dashboard "which repos does this hub care about, and do I have
/// them cloned" view.
fn repo_json(slug: &str, r: &repos::Repo) -> Value {
    let clone = repomap::path(slug);
    json!({
        "slug": sanitize_term(slug, false),
        "role": sanitize_term(&r.role, false),
        "url": r.url.as_deref().map(|u| sanitize_term(u, false)),
        "access": r.access.iter().map(|a| sanitize_term(a, false)).collect::<Vec<_>>(),
        "docs": r.docs.as_deref().map(|d| sanitize_term(d, false)),
        "owner": r.owner.as_deref().map(|o| sanitize_term(o, false)),
        "cloned": clone.is_some(),
        "clonePath": clone.map(|p| p.to_string_lossy().into_owned()),
        "rootSha": r.root_sha.as_deref().map(|s| sanitize_term(s, false)),
    })
}

fn repos_inventory(dirs: &[PathBuf], q: &HashMap<String, String>) -> ApiResponse {
    let Some(dir) = resolve_hub(dirs, q) else { return ApiResponse::err(404, "unknown hub") };
    let inv = repos::load(dir);
    let mut ids: Vec<&String> = inv.keys().collect();
    ids.sort();
    let list: Vec<Value> = ids.into_iter().map(|slug| repo_json(slug, &inv[slug])).collect();
    ApiResponse::ok(json!(list))
}

/// `/api/hubs`'s per-hub agent count, straight from the warm cache's already-folded
/// `agents` (no per-request read of every hub's whole log). Falls back to a fresh fold
/// only if the cache doesn't (yet) have that index.
fn hubs(dirs: &[PathBuf], cache: &Mutex<Vec<projection::Snapshot>>) -> ApiResponse {
    let guard = cache.lock().ok();
    let list: Vec<Value> = dirs
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let n = guard.as_ref().and_then(|g| g.get(i)).map(|s| s.agents.len()).unwrap_or_else(|| {
                store::all_messages(d)
                    .map(|msgs| {
                        let ros = roster::load(d);
                        let pres = presence_map(d);
                        let xh = crosshub::appearances(d);
                        projection::agents(&msgs, &ros, &pres, &xh).len()
                    })
                    .unwrap_or(0)
            });
            hub_json(dirs, d, n)
        })
        .collect();
    ApiResponse::ok(json!(list))
}

const PALETTE: [&str; 8] =
    ["#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#d19a66", "#abb2bf"];

fn abbr_of(display: &str) -> String {
    let letters: String = display.chars().filter(|c| c.is_alphanumeric()).take(2).collect();
    if letters.is_empty() {
        "??".to_string()
    } else {
        letters.to_uppercase()
    }
}

/// Deterministic palette pick keyed by a stable hash of the role id (FNV-1a — no dep).
fn color_of(id: &str) -> &'static str {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in id.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    PALETTE[(h as usize) % PALETTE.len()]
}

fn agent_wip(board: &projection::Board, id: &str) -> Vec<Value> {
    board
        .rows
        .iter()
        .filter(|r| r.status == "CLAIMED" && r.claimants.iter().any(|c| c == id))
        .map(|r| json!({ "id": r.id, "summary": sanitize_term(&r.summary, false), "status": r.status }))
        .collect()
}

/// `verify::Trust::status_str()` carries a 4th value (`mismatch`, a loud impersonation
/// alarm) that the frontend's `verified` vocabulary doesn't have a slot for (spec: only
/// signed|first-sight|unverified). Folding it into "unverified" keeps the JSON's a
/// closed enum for the consumer; the mismatch itself is still visible via `doctor`/`who`.
fn verified_of(status_str: &str) -> &'static str {
    match status_str {
        "verified" => "signed",
        "first-sight" => "first-sight",
        _ => "unverified",
    }
}

fn agent_row_json(board: &projection::Board, a: &projection::AgentRow, verified: &'static str, now: chrono::DateTime<Utc>) -> Value {
    let live = a
        .presence
        .as_ref()
        .map(|p| presence::liveness(p, now) == presence::Live::Up)
        .unwrap_or(false);
    json!({
        "id": a.id,
        "display": sanitize_term(&a.display, false),
        "desc": a.desc.as_deref().map(|d| sanitize_term(d, false)),
        "expectedHost": a.expected_host,
        "lastTs": a.last_ts,
        "lastHost": a.last_host,
        "live": live,
        "verified": verified,
        "color": color_of(&a.id),
        "abbr": abbr_of(&a.display),
        "wip": agent_wip(board, &a.id),
    })
}

fn request_row_json(row: &projection::RequestRow, topic_of: &HashMap<&str, Option<&str>>) -> Value {
    let topic = topic_of.get(row.id.as_str()).copied().flatten().map(|t| sanitize_term(t, false));
    json!({
        "id": row.id,
        "from": row.from,
        "to": row.to,
        "summary": sanitize_term(&row.summary, false),
        "status": row.status,
        "resolution": row.resolution.as_deref().map(|r| sanitize_term(r, false)),
        "deferred": row.deferred,
        "claimants": row.claimants,
        "ageSecs": row.age_secs,
        "stale": row.stale,
        "topic": topic,
    })
}

#[derive(Default)]
struct TopicAgg {
    messages: u64,
    requests: u64,
    open: u64,
    stale: bool,
    last_ts: String,
}

fn overview(dirs: &[PathBuf], cache: &Mutex<Vec<projection::Snapshot>>, q: &HashMap<String, String>) -> ApiResponse {
    let Some(idx) = resolve_hub_idx(dirs, q) else { return ApiResponse::err(404, "unknown hub") };
    let dir = &dirs[idx];
    let holder = load_snapshot(cache, dir, idx);
    let snap = holder.get();
    let now = Utc::now();
    let ros = &snap.roster;
    let msgs = &snap.messages;
    let board = &snap.board;
    let agent_rows = &snap.agents;

    let mut topic_of: HashMap<&str, Option<&str>> = HashMap::new();
    let mut topics: std::collections::BTreeMap<String, TopicAgg> = Default::default();
    for m in msgs {
        topic_of.insert(&m.front.id, m.front.topic.as_deref());
        if let Some(t) = &m.front.topic {
            let e = topics.entry(t.clone()).or_default();
            e.messages += 1;
            if m.front.ts.as_str() > e.last_ts.as_str() {
                e.last_ts = m.front.ts.clone();
            }
        }
    }
    for row in &board.rows {
        if let Some(Some(t)) = topic_of.get(row.id.as_str()) {
            let e = topics.entry((*t).to_string()).or_default();
            e.requests += 1;
            if row.is_open() {
                e.open += 1;
            }
            if row.stale {
                e.stale = true;
            }
        }
    }
    let topics_json: Vec<Value> = topics
        .into_iter()
        .map(|(slug, a)| {
            let status = if a.requests == 0 {
                "discussion"
            } else if a.open == 0 {
                "closed"
            } else {
                "open"
            };
            json!({
                "slug": sanitize_term(&slug, false),
                "messages": a.messages,
                "open": a.open,
                "requests": a.requests,
                "status": status,
                "stale": a.stale,
                "lastTs": if a.last_ts.is_empty() { Value::Null } else { json!(a.last_ts) },
            })
        })
        .collect();

    let hub_key = config::hub_key(dir);
    let mut vcache = verify::Cache::default();
    let fleet_json: Vec<Value> = agent_rows
        .iter()
        .map(|a| {
            let trust = verify::card_trust(dir, &hub_key, ros, &mut vcache, &a.id);
            agent_row_json(board, a, verified_of(trust.status_str()), now)
        })
        .collect();

    let requests_json: Vec<Value> = board.rows.iter().map(|r| request_row_json(r, &topic_of)).collect();

    ApiResponse::ok(json!({
        "hub": hub_json(dirs, dir, agent_rows.len()),
        "topics": topics_json,
        "board": {
            "requests": requests_json,
            "open": board.open,
            "claimed": board.claimed,
            "blocked": board.blocked,
            "backlog": board.backlog,
            "closed": board.closed,
        },
        "fleet": fleet_json,
    }))
}

/// design/44 §3: the temporal-identity fields carried alongside the existing ones
/// (camelCase — the shared contract with the web frontend, built in parallel against
/// this exact shape). `commitDate` gets the §3 legacy enrichment (best-effort,
/// derived from a mapped clone at read time when a full-hex `sha` has no stored
/// date) via `clone_cache` (keyed by repo, so a busy response doesn't resolve the
/// same clone repeatedly).
fn coderef_json(r: &CodeRef, repo_inv: &repos::Repos, clone_cache: &mut HashMap<String, Option<PathBuf>>) -> Value {
    let clone = clone_cache.entry(r.repo.clone()).or_insert_with(|| refcode::clone_for(repo_inv, &r.repo)).clone();
    let commit_date = refcode::enrich_commit_date(clone.as_deref(), &r.sha, r.commit_date.as_deref());
    json!({
        "repo": r.repo,
        "path": r.path,
        "sha": r.sha,
        "range": r.range,
        "contentHash": r.content_hash,
        "refName": r.ref_name,
        "refType": r.ref_type,
        "commitDate": commit_date,
        "dirty": r.dirty,
        "untracked": r.untracked,
        "baseRef": r.base_ref,
        "forkPoint": r.fork_point,
    })
}

fn message_json(m: &Message, repo_inv: &repos::Repos, clone_cache: &mut HashMap<String, Option<PathBuf>>) -> Value {
    json!({
        "id": m.front.id,
        "from": m.front.from,
        "type": m.front.msg_type,
        "ts": m.front.ts,
        "host": m.front.host,
        "to": m.front.to,
        "cc": m.front.cc,
        "topic": m.front.topic,
        "summary": m.summary_line(),
        "body": sanitize_term(&m.body, true),
        "of": m.front.of,
        "replyTo": m.front.reply_to,
        "supersedes": m.front.supersedes,
        "refs": m.front.refs.iter().map(|r| coderef_json(r, repo_inv, clone_cache)).collect::<Vec<_>>(),
    })
}

/// `GET /api/messages?hub=&topic=&limit=&before=` — served from the warm cache's
/// retained `Snapshot::messages` (no per-request `store::all_messages` read). Without
/// `limit`, returns everything (back-compat, unbounded). With `limit=N`, returns the
/// most-recent N (of the topic filter, if any), in chronological order; `before=<id>`
/// (a ULID, so lexicographic order == chronological) restricts to messages strictly
/// older than it, so a client pages backward by repeating with `before=<oldest-id-seen>`.
fn messages(dirs: &[PathBuf], cache: &Mutex<Vec<projection::Snapshot>>, q: &HashMap<String, String>) -> ApiResponse {
    let Some(idx) = resolve_hub_idx(dirs, q) else { return ApiResponse::err(404, "unknown hub") };
    let dir = &dirs[idx];
    let holder = load_snapshot(cache, dir, idx);
    let mut msgs: Vec<&Message> = holder.get().messages.iter().collect();
    msgs.sort_by(|a, b| a.front.id.cmp(&b.front.id));
    if let Some(topic) = q.get("topic").filter(|s| !s.is_empty()) {
        msgs.retain(|m| m.front.topic.as_deref() == Some(topic.as_str()));
    }
    if let Some(before) = q.get("before").filter(|s| !s.is_empty()) {
        msgs.retain(|m| m.front.id.as_str() < before.as_str());
    }
    if let Some(limit) = q.get("limit").filter(|s| !s.is_empty()).and_then(|s| s.parse::<usize>().ok()) {
        if msgs.len() > limit {
            let cut = msgs.len() - limit;
            msgs.drain(0..cut);
        }
    }
    let repo_inv = repos::load(dir);
    let mut clone_cache: HashMap<String, Option<PathBuf>> = HashMap::new();
    ApiResponse::ok(json!(msgs
        .iter()
        .map(|m| message_json(m, &repo_inv, &mut clone_cache))
        .collect::<Vec<_>>()))
}

fn thread(dirs: &[PathBuf], cache: &Mutex<Vec<projection::Snapshot>>, q: &HashMap<String, String>) -> ApiResponse {
    let Some(idx) = resolve_hub_idx(dirs, q) else { return ApiResponse::err(404, "unknown hub") };
    let dir = &dirs[idx];
    let Some(id) = q.get("id").filter(|s| !s.is_empty()) else {
        return ApiResponse::err(400, "missing ?id=");
    };
    let holder = load_snapshot(cache, dir, idx);
    let msgs = &holder.get().messages;
    let Some(target) = msgs.iter().find(|m| projection::id_ref_matches(&m.front.id, id)) else {
        return ApiResponse::err(404, "message not found");
    };
    let root_id = projection::thread_root(msgs, target).front.id.clone();
    let mut thread: Vec<&Message> =
        msgs.iter().filter(|m| projection::thread_root(msgs, m).front.id == root_id).collect();
    thread.sort_by(|a, b| a.front.id.cmp(&b.front.id));
    let repo_inv = repos::load(dir);
    let mut clone_cache: HashMap<String, Option<PathBuf>> = HashMap::new();
    let out: Vec<Value> = thread
        .iter()
        .map(|m| {
            json!({
                "msgId": m.front.id,
                "from": m.front.from,
                "type": m.front.msg_type,
                "topic": m.front.topic,
                "summary": m.summary_line(),
                "refs": m.front.refs.iter().map(|r| coderef_json(r, &repo_inv, &mut clone_cache)).collect::<Vec<_>>(),
            })
        })
        .collect();
    ApiResponse::ok(json!(out))
}

/// Compute `staleness` for `key` (a `(repo, sha, path, base_ref, fork_point)` tuple —
/// the last two included because `staleness_ex`'s ancestry augmentation depends on
/// them too, not just the pinned blob) at most once per distinct key, tracked in
/// `cache`, and refuse to compute more than `cap` distinct keys — beyond that it
/// returns `Unknown` (with `capped = true`) rather than calling `compute` (which, in
/// `refs()`, shells out to `git`). Returns `(staleness, capped)`; `capped` is only
/// ever true on the computations that got refused, so callers OR it across the whole
/// loop.
type StalenessKey = (String, String, String, Option<String>, Option<String>);
fn memoized_staleness(
    cache: &mut HashMap<StalenessKey, refcode::Staleness>,
    cap: usize,
    key: StalenessKey,
    compute: impl FnOnce() -> refcode::Staleness,
) -> (refcode::Staleness, bool) {
    if let Some(v) = cache.get(&key) {
        return (*v, false);
    }
    if cache.len() >= cap {
        return (refcode::Staleness::Unknown, true);
    }
    let v = compute();
    cache.insert(key, v);
    (v, false)
}

fn refs(dirs: &[PathBuf], q: &HashMap<String, String>) -> ApiResponse {
    let Some(target) = q.get("target").filter(|s| !s.is_empty()) else {
        return ApiResponse::err(400, "missing ?target=");
    };
    let (repo, path, range) = match crate::parse_ref_query(target) {
        Ok(v) => v,
        Err(e) => return ApiResponse::err(400, e.to_string()),
    };
    let all_hubs = matches!(q.get("allHubs").map(String::as_str), Some("1") | Some("true"));
    let hubs: Vec<PathBuf> = if all_hubs {
        crosshub::hub_dirs()
    } else {
        match resolve_hub(dirs, q) {
            Some(d) => vec![d.clone()],
            None => return ApiResponse::err(404, "unknown hub"),
        }
    };

    // A single target (esp. a bare repo/path with no range) can match hundreds/thousands
    // of hits sharing the same pinned (repo, sha, path) — `staleness` shells out to `git`
    // per call, so without memoizing this a busy target turns one HTTP request into a
    // synchronous git-spawn storm. Memoize within THIS request (hits recur far more than
    // they're distinct) and cap the number of DISTINCT computations so a request can
    // never spawn unbounded git processes; anything beyond the cap reports "unknown"
    // rather than silently doing more work.
    const MAX_STALENESS_COMPUTATIONS: usize = 100;
    let mut out = Vec::new();
    let mut truncated = false;
    for hub in &hubs {
        let Ok(msgs) = store::all_messages(hub) else { continue };
        let this_hub_id = hub_id(hub);
        let idx = projection::RefIndex::fold(&msgs);
        let repo_inv = repos::load(hub);
        let mut clone_cache: HashMap<String, Option<PathBuf>> = HashMap::new();
        let mut staleness_cache: HashMap<StalenessKey, refcode::Staleness> = HashMap::new();
        for h in idx.query(&repo, path.as_deref(), range) {
            let clone = clone_cache.entry(h.repo.clone()).or_insert_with(|| refcode::clone_for(&repo_inv, &h.repo)).clone();
            let key = (h.repo.clone(), h.sha.clone(), h.path.clone(), h.base_ref.clone(), h.fork_point.clone());
            let (st, capped) = memoized_staleness(&mut staleness_cache, MAX_STALENESS_COMPUTATIONS, key, || {
                refcode::staleness_ex(
                    clone.as_deref(),
                    &h.sha,
                    &h.path,
                    h.content_hash.as_deref(),
                    h.base_ref.as_deref(),
                    h.fork_point.as_deref(),
                )
            });
            truncated |= capped;
            let commit_date = refcode::enrich_commit_date(clone.as_deref(), &h.sha, h.commit_date.as_deref());
            let st = st.label();
            out.push(json!({
                "repo": h.repo,
                "path": h.path,
                "sha": h.sha,
                "range": h.range,
                "contentHash": h.content_hash,
                "refName": h.ref_name,
                "refType": h.ref_type,
                "commitDate": commit_date,
                "dirty": h.dirty,
                "untracked": h.untracked,
                "baseRef": h.base_ref,
                "forkPoint": h.fork_point,
                "staleness": st,
                "msgId": h.msg_id,
                "from": h.from,
                "msgType": h.msg_type,
                "ts": h.ts,
                "topic": h.topic,
                "summary": sanitize_term(&h.summary, false),
                "threadRoot": h.thread_root,
                "requestStatus": h.request_status,
                "hub": this_hub_id,
                // No per-hub "private" (non-anonymous-read) fact is cached anywhere the
                // server can read synchronously — `doctor`'s PUBLIC check is a live network
                // probe, not stored state — so this defaults to `false` pending a real
                // per-hub visibility source.
                "hubPrivate": false,
            }));
        }
    }
    if truncated {
        eprintln!(
            "confer serve: /api/refs truncated staleness computation for target {target:?} at {MAX_STALENESS_COMPUTATIONS} distinct (repo,sha,path) — remaining hits report \"unknown\""
        );
    }
    ApiResponse::ok(json!(out))
}

/// `GET /api/codefiles?hub=<id>` — the distinct code files this hub's messages
/// reference via `--ref`, for the web Code view to hydrate its file tree from (instead
/// of a hardcoded fixture). Sourced from the same `RefIndex::fold` reverse projection
/// `/api/refs` queries, just summarized to distinct targets (`RefIndex::files`) rather
/// than queried by one target. `mapped` reuses `/api/code`'s own clone-resolution
/// (`refcode::clone_for` over the hub's registered repo inventory) so the frontend can
/// show the mapped/unmapped dot without a failed `getCode` round-trip.
fn codefiles(dirs: &[PathBuf], cache: &Mutex<Vec<projection::Snapshot>>, q: &HashMap<String, String>) -> ApiResponse {
    let Some(idx) = resolve_hub_idx(dirs, q) else { return ApiResponse::err(404, "unknown hub") };
    let dir = &dirs[idx];
    let holder = load_snapshot(cache, dir, idx);
    let ref_idx = projection::RefIndex::fold(&holder.get().messages);
    let repo_inv = repos::load(dir);
    let mut mapped_cache: HashMap<String, bool> = HashMap::new();
    let mut files = ref_idx.files();
    files.sort_by(|a, b| b.ref_count.cmp(&a.ref_count).then_with(|| a.path.cmp(&b.path)));
    let out: Vec<Value> = files
        .into_iter()
        .map(|f| {
            let mapped = *mapped_cache
                .entry(f.repo.clone())
                .or_insert_with(|| refcode::clone_for(&repo_inv, &f.repo).is_some());
            json!({
                "repo": f.repo,
                "path": f.path,
                "refCount": f.ref_count,
                "mapped": mapped,
                "lastTs": f.last_ts,
            })
        })
        .collect();
    ApiResponse::ok(json!(out))
}

fn lang_of(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("") {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "tsx",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "jsx",
        "py" => "python",
        "go" => "go",
        "rb" => "ruby",
        "java" => "java",
        "kt" => "kotlin",
        "swift" => "swift",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "md" => "markdown",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "sh" | "bash" => "bash",
        "html" | "htm" => "html",
        "css" => "css",
        "sql" => "sql",
        _ => "text",
    }
}

fn code(dirs: &[PathBuf], q: &HashMap<String, String>) -> ApiResponse {
    let Some(dir) = resolve_hub(dirs, q) else { return ApiResponse::err(404, "unknown hub") };
    let (Some(repo), Some(path), Some(sha)) = (q.get("repo"), q.get("path"), q.get("sha")) else {
        return ApiResponse::err(400, "missing ?repo=&path=&sha=");
    };
    if repo.is_empty() || path.is_empty() || sha.is_empty() {
        return ApiResponse::err(400, "missing ?repo=&path=&sha=");
    }
    let range = match q.get("range").filter(|s| !s.is_empty()) {
        Some(r) => match append::parse_range(r) {
            Ok(v) => Some(v),
            Err(e) => return ApiResponse::err(400, e.to_string()),
        },
        None => None,
    };
    // Optional — a caller that already has the RefHit's contentHash (+ base_ref/fork_point,
    // design/44 Addenda 1+2) can pass them through for the full staleness verdict; without
    // them staleness degrades gracefully (content-only, or unpinned/unknown).
    let content_hash = q.get("contentHash").filter(|s| !s.is_empty()).cloned();
    let base_ref = q.get("baseRef").filter(|s| !s.is_empty()).cloned();
    let fork_point = q.get("forkPoint").filter(|s| !s.is_empty()).cloned();
    let repo_inv = repos::load(dir);
    let clone = refcode::clone_for(&repo_inv, repo);
    let st = refcode::staleness_ex(
        clone.as_deref(),
        sha,
        path,
        content_hash.as_deref(),
        base_ref.as_deref(),
        fork_point.as_deref(),
    );
    let lines = refcode::snippet(clone.as_deref(), sha, path, range, 2000).unwrap_or_default();
    let lines_json: Vec<Value> = lines.into_iter().map(|(n, text)| json!({ "n": n, "text": text })).collect();
    ApiResponse::ok(json!({ "lines": lines_json, "staleness": st.label(), "lang": lang_of(path) }))
}

/// Local presence-refs fingerprint (`sha refname` lines, sorted by ref listing order) —
/// cheap enough to poll every ~2s: changes iff any role's heartbeat moved. No fetch.
fn presence_fingerprint(dir: &Path) -> String {
    gitcmd::output(dir, &["for-each-ref", "--format=%(objectname) %(refname)", "refs/presence/"])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Serve `/api/events` as Server-Sent Events directly on `writer` (the request's raw
/// socket writer — the caller must have already taken it via `Request::into_writer`).
/// Polls each followed hub's local HEAD (a `message` event) and presence-refs
/// fingerprint (a `presence` event) every ~2s — no network fetch, matching the
/// read-only contract. A ~30s keepalive ping otherwise. Returns as soon as a write
/// fails (the client disconnected) so the handling thread is freed, not leaked.
pub fn serve_sse(dirs: &[PathBuf], mut writer: impl std::io::Write) {
    let preamble = "HTTP/1.1 200 OK\r\n\
         Content-Type: text/event-stream\r\n\
         Cache-Control: no-store\r\n\
         Connection: keep-alive\r\n\r\n";
    if writer.write_all(preamble.as_bytes()).is_err() || writer.flush().is_err() {
        return;
    }

    let mut heads: HashMap<PathBuf, String> = dirs.iter().map(|d| (d.clone(), gitcmd::head(d).unwrap_or_default())).collect();
    let mut pres: HashMap<PathBuf, String> = dirs.iter().map(|d| (d.clone(), presence_fingerprint(d))).collect();
    let mut last_emit = std::time::Instant::now();
    const POLL: std::time::Duration = std::time::Duration::from_secs(2);
    const KEEPALIVE: std::time::Duration = std::time::Duration::from_secs(30);

    loop {
        std::thread::sleep(POLL);
        let mut sent = false;
        for d in dirs {
            let h = gitcmd::head(d).unwrap_or_default();
            if heads.get(d).is_some_and(|prev| prev != &h) {
                heads.insert(d.clone(), h);
                let line = format!("data: {}\n\n", json!({ "event": "message", "hub": hub_id(d), "topic": Value::Null }));
                if writer.write_all(line.as_bytes()).is_err() {
                    return;
                }
                sent = true;
            }
            let p = presence_fingerprint(d);
            if pres.get(d).is_some_and(|prev| prev != &p) {
                pres.insert(d.clone(), p);
                let line = format!("data: {}\n\n", json!({ "event": "presence", "hub": hub_id(d), "topic": Value::Null }));
                if writer.write_all(line.as_bytes()).is_err() {
                    return;
                }
                sent = true;
            }
        }
        if sent {
            if writer.flush().is_err() {
                return;
            }
            last_emit = std::time::Instant::now();
        } else if last_emit.elapsed() >= KEEPALIVE {
            if writer.write_all(b"data: {\"event\":\"ping\"}\n\n").is_err() || writer.flush().is_err() {
                return;
            }
            last_emit = std::time::Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_qs_decodes_percent_and_plus() {
        let q = parse_qs("/api/refs?target=repo%3Apath%23L1-2&note=a+b");
        assert_eq!(q.get("target").unwrap(), "repo:path#L1-2");
        assert_eq!(q.get("note").unwrap(), "a b");
    }

    #[test]
    fn abbr_and_color_are_deterministic() {
        assert_eq!(abbr_of("Bob"), "BO");
        assert_eq!(abbr_of("*"), "??");
        assert_eq!(color_of("bob"), color_of("bob"));
    }

    #[test]
    fn verified_of_maps_closed_vocabulary() {
        assert_eq!(verified_of("verified"), "signed");
        assert_eq!(verified_of("first-sight"), "first-sight");
        assert_eq!(verified_of("unverified"), "unverified");
        assert_eq!(verified_of("mismatch"), "unverified");
    }

    #[test]
    fn lang_of_maps_common_extensions() {
        assert_eq!(lang_of("src/main.rs"), "rust");
        assert_eq!(lang_of("a/b.tsx"), "tsx");
        assert_eq!(lang_of("README"), "text");
    }

    fn key(n: usize) -> StalenessKey {
        ("repo".to_string(), format!("sha{n}"), "path.rs".to_string(), None, None)
    }

    #[test]
    fn memoized_staleness_computes_a_repeated_key_only_once() {
        let mut cache = HashMap::new();
        let mut calls = 0;
        let (st1, capped1) = memoized_staleness(&mut cache, 100, key(1), || {
            calls += 1;
            refcode::Staleness::Current
        });
        let (st2, capped2) = memoized_staleness(&mut cache, 100, key(1), || {
            calls += 1;
            refcode::Staleness::Current
        });
        assert_eq!(calls, 1, "the second lookup of the same key must hit the cache, not recompute");
        assert_eq!(st1, refcode::Staleness::Current);
        assert_eq!(st2, refcode::Staleness::Current);
        assert!(!capped1 && !capped2);
    }

    #[test]
    fn memoized_staleness_refuses_past_the_cap() {
        let mut cache = HashMap::new();
        for i in 0..5 {
            let (_, capped) = memoized_staleness(&mut cache, 5, key(i), || refcode::Staleness::Current);
            assert!(!capped, "under the cap must compute normally");
        }
        assert_eq!(cache.len(), 5);
        let mut called = false;
        let (st, capped) = memoized_staleness(&mut cache, 5, key(99), || {
            called = true;
            refcode::Staleness::Current
        });
        assert!(!called, "over the cap must not invoke compute (no git spawn)");
        assert!(capped);
        assert_eq!(st, refcode::Staleness::Unknown);
    }

    #[test]
    fn memoized_staleness_cached_hit_bypasses_the_cap() {
        // A key already cached before the cap was reached must keep returning its
        // cached value even once the cache is otherwise full — the cap only refuses
        // NEW distinct keys, never invalidates ones already computed.
        let mut cache = HashMap::new();
        let (_, capped) = memoized_staleness(&mut cache, 1, key(1), || refcode::Staleness::Changed);
        assert!(!capped);
        let (_, capped) = memoized_staleness(&mut cache, 1, key(2), || refcode::Staleness::Current);
        assert!(capped, "cache already at cap=1, a second distinct key must be refused");
        let (st, capped) = memoized_staleness(&mut cache, 1, key(1), || refcode::Staleness::Current);
        assert!(!capped);
        assert_eq!(st, refcode::Staleness::Changed, "repeated key must still return its originally cached value");
    }
}
