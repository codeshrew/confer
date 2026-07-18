//! JSON API for `confer serve` — `/api/*` endpoints alongside the existing HTML view
//! (`serve.rs`'s request loop routes here). Read-only, same discipline as the rest of
//! `serve`: handlers only re-fold already-on-disk data (a fresh, cheap local read per
//! request — no network fetch, no lock, no publish). A later step swaps `/` to an
//! embedded SPA that consumes these shapes; the exact camelCase keys here are a
//! contract with that frontend, not incidental.

use crate::schema::{sanitize_term, CodeRef, Message};
use crate::{append, config, crosshub, gitcmd, presence, projection, refcode, repos, roster, store, verify};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

/// Resolve `?hub=<id>` against the server's followed hubs. Omitted → the current hub
/// (`config::repo_root()`) if it's one of the followed dirs, else the first. `Some(id)`
/// that matches nothing → `None` (the caller 404s).
fn resolve_hub<'a>(dirs: &'a [PathBuf], q: &HashMap<String, String>) -> Option<&'a PathBuf> {
    match q.get("hub").filter(|s| !s.is_empty()) {
        Some(id) => dirs.iter().find(|d| &hub_id(d) == id),
        None => {
            if let Ok(cwd) = config::repo_root() {
                if let Some(d) = dirs.iter().find(|d| same_dir(d, &cwd)) {
                    return Some(d);
                }
            }
            dirs.first()
        }
    }
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
pub fn dispatch(dirs: &[PathBuf], path: &str, url: &str) -> ApiResponse {
    let q = parse_qs(url);
    match path {
        "/api/hubs" => hubs(dirs),
        "/api/overview" => overview(dirs, &q),
        "/api/messages" => messages(dirs, &q),
        "/api/thread" => thread(dirs, &q),
        "/api/refs" => refs(dirs, &q),
        "/api/code" => code(dirs, &q),
        _ => ApiResponse::err(404, "no such API endpoint"),
    }
}

fn hubs(dirs: &[PathBuf]) -> ApiResponse {
    let list: Vec<Value> = dirs
        .iter()
        .map(|d| {
            let n = store::all_messages(d)
                .map(|msgs| {
                    let ros = roster::load(d);
                    let pres = presence_map(d);
                    let xh = crosshub::appearances(d);
                    projection::agents(&msgs, &ros, &pres, &xh).len()
                })
                .unwrap_or(0);
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

fn overview(dirs: &[PathBuf], q: &HashMap<String, String>) -> ApiResponse {
    let Some(dir) = resolve_hub(dirs, q) else { return ApiResponse::err(404, "unknown hub") };
    let msgs = match store::all_messages(dir) {
        Ok(m) => m,
        Err(e) => return ApiResponse::err(500, format!("cannot read hub: {e}")),
    };
    let now = Utc::now();
    let ros = roster::load(dir);
    let pres = presence_map(dir);
    let xhub = crosshub::appearances(dir);
    let board = projection::Board::fold(&msgs, now);
    let agent_rows = projection::agents(&msgs, &ros, &pres, &xhub);

    let mut topic_of: HashMap<&str, Option<&str>> = HashMap::new();
    let mut topics: std::collections::BTreeMap<String, TopicAgg> = Default::default();
    for m in &msgs {
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
            let trust = verify::card_trust(dir, &hub_key, &ros, &mut vcache, &a.id);
            agent_row_json(&board, a, verified_of(trust.status_str()), now)
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

fn coderef_json(r: &CodeRef) -> Value {
    json!({ "repo": r.repo, "path": r.path, "sha": r.sha, "range": r.range, "contentHash": r.content_hash })
}

fn message_json(m: &Message) -> Value {
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
        "refs": m.front.refs.iter().map(coderef_json).collect::<Vec<_>>(),
    })
}

fn messages(dirs: &[PathBuf], q: &HashMap<String, String>) -> ApiResponse {
    let Some(dir) = resolve_hub(dirs, q) else { return ApiResponse::err(404, "unknown hub") };
    let mut msgs = match store::all_messages(dir) {
        Ok(m) => m,
        Err(e) => return ApiResponse::err(500, format!("cannot read hub: {e}")),
    };
    msgs.sort_by(|a, b| a.front.id.cmp(&b.front.id));
    if let Some(topic) = q.get("topic").filter(|s| !s.is_empty()) {
        msgs.retain(|m| m.front.topic.as_deref() == Some(topic.as_str()));
    }
    ApiResponse::ok(json!(msgs.iter().map(message_json).collect::<Vec<_>>()))
}

fn thread(dirs: &[PathBuf], q: &HashMap<String, String>) -> ApiResponse {
    let Some(dir) = resolve_hub(dirs, q) else { return ApiResponse::err(404, "unknown hub") };
    let Some(id) = q.get("id").filter(|s| !s.is_empty()) else {
        return ApiResponse::err(400, "missing ?id=");
    };
    let msgs = match store::all_messages(dir) {
        Ok(m) => m,
        Err(e) => return ApiResponse::err(500, format!("cannot read hub: {e}")),
    };
    let Some(target) = msgs.iter().find(|m| projection::id_ref_matches(&m.front.id, id)) else {
        return ApiResponse::err(404, "message not found");
    };
    let root_id = projection::thread_root(&msgs, target).front.id.clone();
    let mut thread: Vec<&Message> =
        msgs.iter().filter(|m| projection::thread_root(&msgs, m).front.id == root_id).collect();
    thread.sort_by(|a, b| a.front.id.cmp(&b.front.id));
    let out: Vec<Value> = thread
        .iter()
        .map(|m| {
            json!({
                "msgId": m.front.id,
                "from": m.front.from,
                "type": m.front.msg_type,
                "topic": m.front.topic,
                "summary": m.summary_line(),
                "refs": m.front.refs.iter().map(coderef_json).collect::<Vec<_>>(),
            })
        })
        .collect();
    ApiResponse::ok(json!(out))
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

    let mut out = Vec::new();
    for hub in &hubs {
        let Ok(msgs) = store::all_messages(hub) else { continue };
        let idx = projection::RefIndex::fold(&msgs);
        let repo_inv = repos::load(hub);
        let mut clone_cache: HashMap<String, Option<PathBuf>> = HashMap::new();
        for h in idx.query(&repo, path.as_deref(), range) {
            let clone = clone_cache.entry(h.repo.clone()).or_insert_with(|| refcode::clone_for(&repo_inv, &h.repo)).clone();
            let st = refcode::staleness(clone.as_deref(), &h.sha, &h.path, h.content_hash.as_deref()).label();
            out.push(json!({
                "repo": h.repo,
                "path": h.path,
                "sha": h.sha,
                "range": h.range,
                "contentHash": h.content_hash,
                "staleness": st,
                "msgId": h.msg_id,
                "from": h.from,
                "msgType": h.msg_type,
                "ts": h.ts,
                "topic": h.topic,
                "summary": sanitize_term(&h.summary, false),
                "threadRoot": h.thread_root,
                "requestStatus": h.request_status,
            }));
        }
    }
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
    // Optional — a caller that already has the RefHit's contentHash can pass it through
    // for a real staleness verdict; without it staleness degrades to unpinned/unknown.
    let content_hash = q.get("contentHash").filter(|s| !s.is_empty()).cloned();
    let repo_inv = repos::load(dir);
    let clone = refcode::clone_for(&repo_inv, repo);
    let st = refcode::staleness(clone.as_deref(), sha, path, content_hash.as_deref());
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
}
