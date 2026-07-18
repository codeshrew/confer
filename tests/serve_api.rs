//! Integration tests for `confer serve`'s `/api/*` JSON endpoints — spins up the real
//! server as a subprocess against a temp hub (messages + a request + a code ref),
//! hits it with a bare TCP/HTTP client (no client dep needed), and asserts the JSON
//! shapes/keys the frontend contract requires. Mirrors `tests/cli.rs`'s Hub/Clone
//! harness (kept self-contained here rather than shared, since each `tests/*.rs`
//! file is its own binary crate).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_confer");
static SEQ: AtomicU32 = AtomicU32::new(0);

fn tmp(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let p = std::env::temp_dir().join(format!("confer-api-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t.local", "-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .expect("run git")
}

fn ok(o: &Output) -> bool {
    o.status.success()
}
fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

struct Hub {
    bare: PathBuf,
    home: PathBuf,
}

fn new_hub() -> Hub {
    let root = tmp("hub");
    let bare = root.join("hub.git");
    assert!(git(&root, &["init", "--bare", "-q", "-b", "main", bare.to_str().unwrap()]).status.success());
    let seed = tmp("seed");
    assert!(git(&seed, &["init", "-q", "-b", "main"]).status.success());
    for d in ["threads", "roles"] {
        std::fs::create_dir_all(seed.join(d)).unwrap();
        std::fs::write(seed.join(d).join(".gitkeep"), "").unwrap();
    }
    std::fs::write(seed.join(".gitignore"), ".confer/\n").unwrap();
    std::fs::write(seed.join(".confer-version"), "0.6.5\n").unwrap();
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-q", "-m", "init"]);
    git(&seed, &["remote", "add", "origin", bare.to_str().unwrap()]);
    assert!(git(&seed, &["push", "-q", "-u", "origin", "main"]).status.success());
    let home = tmp("home");
    std::fs::create_dir_all(home.join(".confer")).unwrap();
    Hub { bare, home }
}

struct Clone {
    dir: PathBuf,
    role: String,
    home: PathBuf,
}

impl Hub {
    fn clone(&self, role: &str) -> Clone {
        let dir = tmp(&format!("clone-{role}"));
        let o = Command::new("git").args(["clone", "-q", self.bare.to_str().unwrap(), dir.to_str().unwrap()]).output().unwrap();
        assert!(o.status.success(), "clone failed: {}", String::from_utf8_lossy(&o.stderr));
        git(&dir, &["config", "user.name", role]);
        git(&dir, &["config", "user.email", &format!("{role}@t.local")]);
        Clone { dir, role: role.to_string(), home: self.home.clone() }
    }
}

impl Clone {
    fn confer(&self, args: &[&str]) -> Output {
        Command::new(BIN).env("HOME", &self.home).env("CONFER_HUB", &self.dir).env("CONFER_ROLE", &self.role).args(args).output().expect("run confer")
    }
    fn append(&self, extra: &[&str]) -> Output {
        let mut a: Vec<&str> = vec!["append", "--from", &self.role];
        a.extend_from_slice(extra);
        self.confer(&a)
    }
}

/// A running `confer serve` subprocess bound to a fixed loopback port, killed on drop
/// so a failing assertion never leaks the child.
struct Server {
    child: Child,
    addr: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_server(c: &Clone) -> Server {
    // Bind an EPHEMERAL port (`:0`) and read back the actual port serve prints — the OS
    // assigns it atomically at bind, so there's no find-a-free-port-then-bind race
    // (parallel tests picking the same "free" port was the flake this replaces).
    let mut child = Command::new(BIN)
        .env("HOME", &c.home)
        .env("CONFER_HUB", &c.dir)
        .env("CONFER_ROLE", &c.role)
        .args(["serve", "--bind", "127.0.0.1:0"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn confer serve");
    let stderr = child.stderr.take().expect("piped stderr");
    let mut reader = std::io::BufReader::new(stderr);
    let addr;
    loop {
        let mut line = String::new();
        // Blocking read is safe: serve prints its banner immediately after a successful
        // bind, and a failed bind exits (→ EOF → 0) rather than hanging.
        if std::io::BufRead::read_line(&mut reader, &mut line).unwrap_or(0) == 0 {
            panic!("confer serve exited before printing its address");
        }
        if let Some(rest) = line.split("http://localhost:").nth(1) {
            let port: String = rest.trim().chars().take_while(|ch| ch.is_ascii_digit()).collect();
            if !port.is_empty() {
                addr = format!("127.0.0.1:{port}");
                break;
            }
        }
    }
    // Drain the rest of serve's stderr in the background so its pipe can never fill and
    // block a worker thread mid-request.
    std::thread::spawn(move || {
        let mut sink = String::new();
        while std::io::BufRead::read_line(&mut reader, &mut sink).unwrap_or(0) > 0 {
            sink.clear();
        }
    });
    // Wait until serve actually answers 200 — the banner prints right after bind, before
    // the first snapshot fold populates the cache, so poll a real endpoint (not just a
    // TCP accept) so a request never races an un-warmed server.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if TcpStream::connect(&addr).is_ok() && http_get(&addr, "/api/hubs").0 == 200 {
            break;
        }
        if std::time::Instant::now() > deadline {
            panic!("confer serve did not become ready on {addr}");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Server { child, addr }
}

/// A bare-bones blocking HTTP/1.1 GET (no client dep) — connects, sends the request
/// with `Connection: close` so the server closes the socket when done, then reads to
/// EOF. Returns (status, body).
fn http_get(addr: &str, path: &str) -> (u16, String) {
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf); // best-effort: a read timeout still leaves buf usable
    let text = String::from_utf8_lossy(&buf).into_owned();
    let mut parts = text.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or("");
    let body = parts.next().unwrap_or("").to_string();
    let status: u16 = head.lines().next().and_then(|l| l.split_whitespace().nth(1)).and_then(|s| s.parse().ok()).unwrap_or(0);
    (status, body)
}

/// Seeds a hub with a note, a request (with a claim), and a message carrying a
/// `--ref` into a tiny local code repo. Returns the clone + the request's id.
fn seed(c: &Clone) -> String {
    let code = tmp("coderepo");
    assert!(git(&code, &["init", "-q"]).status.success());
    std::fs::write(code.join("lib.rs"), "one\ntwo\nthree\nfour\n").unwrap();
    assert!(git(&code, &["add", "-A"]).status.success());
    assert!(git(&code, &["commit", "-q", "-m", "c0"]).status.success());
    std::fs::create_dir_all(c.dir.join("repos")).unwrap();
    std::fs::write(c.dir.join("repos").join("mylib.md"), "---\nrole: code\n---\n").unwrap();
    assert!(ok(&c.confer(&["repos", "map", "mylib", code.to_str().unwrap()])));

    assert!(ok(&c.append(&["--type", "note", "--to", "beta", "--summary", "hello there", "--text", "just a note"])));

    let r = c.append(&[
        "--type", "request", "--to", "beta", "--summary", "wire the search index", "--text", "please do the thing", "--topic",
        "search", "--ref", "mylib:lib.rs#L1-2",
    ]);
    assert!(ok(&r), "seed request failed: {}", err(&r));

    // Recover the request's id from `read --json` (stable, no format guessing).
    let read = c.confer(&["read", "--last", "1", "--json"]);
    assert!(ok(&read), "read --json failed: {}", err(&read));
    let out = String::from_utf8_lossy(&read.stdout).into_owned();
    let v: serde_json::Value = out.lines().last().and_then(|l| serde_json::from_str(l).ok()).unwrap_or_else(|| panic!("no json line in: {out}"));
    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or_else(|| panic!("no id in: {v}")).to_string();

    assert!(ok(&c.confer(&["claim", "--from", "beta", "--of", &id])));
    id
}

#[test]
fn hubs_lists_the_current_hub() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha);
    let server = start_server(&alpha);

    let (status, body) = http_get(&server.addr, "/api/hubs");
    assert_eq!(status, 200);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("bad json ({e}): {body}"));
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 1);
    let h = &arr[0];
    for key in ["id", "label", "name", "current", "agentCount"] {
        assert!(h.get(key).is_some(), "missing {key} in {h}");
    }
    assert_eq!(h["current"], true);
    assert!(h["agentCount"].as_u64().unwrap() >= 1);
}

#[test]
fn overview_has_topic_board_and_fleet_shapes() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha);
    let server = start_server(&alpha);

    let (status, body) = http_get(&server.addr, "/api/overview");
    assert_eq!(status, 200);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(v["hub"]["id"].is_string());

    let topics = v["topics"].as_array().expect("topics array");
    let search = topics.iter().find(|t| t["slug"] == "search").expect("search topic present");
    assert_eq!(search["messages"], 1);
    assert_eq!(search["requests"], 1);
    for key in ["status", "stale", "lastTs"] {
        assert!(search.get(key).is_some(), "missing {key} in {search}");
    }

    let board = &v["board"];
    for key in ["requests", "open", "claimed", "blocked", "backlog", "closed"] {
        assert!(board.get(key).is_some(), "missing board.{key}");
    }
    let requests = board["requests"].as_array().expect("requests array");
    let row = requests.iter().find(|r| r["summary"] == "wire the search index").expect("seeded request present");
    for key in ["id", "from", "to", "summary", "status", "resolution", "deferred", "claimants", "ageSecs", "stale", "topic"] {
        assert!(row.get(key).is_some(), "missing request.{key} in {row}");
    }
    assert_eq!(row["status"], "CLAIMED");
    assert_eq!(row["topic"], "search");

    let fleet = v["fleet"].as_array().expect("fleet array");
    assert!(!fleet.is_empty());
    let agent = &fleet[0];
    for key in ["id", "display", "desc", "expectedHost", "lastTs", "lastHost", "live", "verified", "color", "abbr", "wip"] {
        assert!(agent.get(key).is_some(), "missing agent.{key} in {agent}");
    }
    assert!(matches!(agent["verified"].as_str(), Some("signed" | "first-sight" | "unverified")));
}

#[test]
fn messages_filters_by_topic_and_sanitizes_body() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha);
    let server = start_server(&alpha);

    let (status, body) = http_get(&server.addr, "/api/messages?topic=search");
    assert_eq!(status, 200);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 1, "only the seeded search-topic message: {arr:?}");
    let m = &arr[0];
    for key in ["id", "from", "type", "ts", "host", "to", "cc", "topic", "summary", "body", "of", "replyTo", "supersedes", "refs"] {
        assert!(m.get(key).is_some(), "missing message.{key} in {m}");
    }
    assert_eq!(m["body"], "please do the thing");
    let refs = m["refs"].as_array().expect("refs array");
    assert_eq!(refs.len(), 1);
    for key in ["repo", "path", "sha", "range", "contentHash"] {
        assert!(refs[0].get(key).is_some(), "missing ref.{key}");
    }
    assert_eq!(refs[0]["repo"], "mylib");

    let (_, all_body) = http_get(&server.addr, "/api/messages");
    let all: serde_json::Value = serde_json::from_str(&all_body).unwrap();
    assert!(all.as_array().unwrap().len() >= 2, "no topic filter returns everything");
}

#[test]
fn refs_reverse_lookup_returns_ref_hits() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha);
    let server = start_server(&alpha);

    let (status, body) = http_get(&server.addr, "/api/refs?target=mylib:lib.rs");
    assert_eq!(status, 200);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 1);
    let hit = &arr[0];
    for key in ["repo", "path", "sha", "range", "contentHash", "staleness", "msgId", "from", "msgType", "ts", "topic", "summary", "threadRoot", "requestStatus"] {
        assert!(hit.get(key).is_some(), "missing refhit.{key} in {hit}");
    }
    assert_eq!(hit["repo"], "mylib");
    assert_eq!(hit["requestStatus"], "CLAIMED");

    // A file nothing references comes back empty, not an error.
    let (status2, body2) = http_get(&server.addr, "/api/refs?target=mylib:other.rs");
    assert_eq!(status2, 200);
    let v2: serde_json::Value = serde_json::from_str(&body2).unwrap();
    assert_eq!(v2.as_array().unwrap().len(), 0);
}

#[test]
fn repos_endpoint_lists_registered_repos_with_clone_status() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha); // registers + maps "mylib"

    // a second repo, registered but NOT cloned anywhere.
    std::fs::write(
        alpha.dir.join("repos").join("unlinked.md"),
        "---\nrole: docs\nurl: git@github.com:o/unlinked.git\naccess: [beta]\ndocs: docs/\nowner: o\n---\n",
    )
    .unwrap();

    let server = start_server(&alpha);
    let (status, body) = http_get(&server.addr, "/api/repos");
    assert_eq!(status, 200);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("bad json ({e}): {body}"));
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 2, "both registered repos listed: {arr:?}");

    let mylib = arr.iter().find(|r| r["slug"] == "mylib").expect("mylib present");
    for key in ["slug", "role", "url", "access", "docs", "owner", "cloned", "clonePath", "rootSha"] {
        assert!(mylib.get(key).is_some(), "missing repo.{key} in {mylib}");
    }
    assert_eq!(mylib["cloned"], true, "mylib was mapped by seed(): {mylib}");
    assert!(mylib["clonePath"].as_str().is_some(), "cloned repo should carry a path: {mylib}");

    let unlinked = arr.iter().find(|r| r["slug"] == "unlinked").expect("unlinked present");
    assert_eq!(unlinked["role"], "docs");
    assert_eq!(unlinked["owner"], "o");
    assert_eq!(unlinked["docs"], "docs/");
    assert_eq!(unlinked["access"], serde_json::json!(["beta"]));
    assert_eq!(unlinked["cloned"], false, "never mapped: {unlinked}");
    assert!(unlinked["clonePath"].is_null());

    // unknown hub still 404s the same way the other endpoints do.
    let (s2, b2) = http_get(&server.addr, "/api/repos?hub=nope-such-hub");
    assert_eq!(s2, 404, "body: {b2}");
    let v2: serde_json::Value = serde_json::from_str(&b2).unwrap();
    assert!(v2.get("error").is_some());
}

#[test]
fn unknown_hub_404s_with_json_error() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha);
    let server = start_server(&alpha);

    for path in ["/api/overview?hub=nope-such-hub", "/api/messages?hub=nope-such-hub", "/api/refs?target=mylib:lib.rs&hub=nope-such-hub"] {
        let (status, body) = http_get(&server.addr, path);
        assert_eq!(status, 404, "path {path} body {body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("bad json ({e}) for {path}: {body}"));
        assert!(v.get("error").is_some(), "expected error key for {path}: {v}");
    }
}

#[test]
fn malformed_refs_target_is_400() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha);
    let server = start_server(&alpha);

    let (status, body) = http_get(&server.addr, "/api/refs?target=");
    assert_eq!(status, 400, "body: {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(v.get("error").is_some());
}

#[test]
fn thread_returns_the_thread_and_errors_on_bad_id() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    let req_id = seed(&alpha);
    let server = start_server(&alpha);

    // missing ?id= is a 400, not a panic/500.
    let (s0, b0) = http_get(&server.addr, "/api/thread");
    assert_eq!(s0, 400, "body: {b0}");
    let v0: serde_json::Value = serde_json::from_str(&b0).unwrap();
    assert!(v0.get("error").is_some());

    // an id that matches nothing is a 404, same JSON error shape.
    let (s1, b1) = http_get(&server.addr, "/api/thread?id=nonexistent00000000");
    assert_eq!(s1, 404, "body: {b1}");
    let v1: serde_json::Value = serde_json::from_str(&b1).unwrap();
    assert!(v1.get("error").is_some());

    // the seeded request's thread comes back as an ordered array of its own messages.
    let (status, body) = http_get(&server.addr, &format!("/api/thread?id={req_id}"));
    assert_eq!(status, 200, "body: {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("bad json ({e}): {body}"));
    let arr = v.as_array().expect("array");
    assert!(!arr.is_empty(), "the request itself must be in its own thread: {arr:?}");
    let root = arr.iter().find(|m| m["msgId"] == req_id).expect("request present in its own thread");
    for key in ["msgId", "from", "type", "topic", "summary", "refs"] {
        assert!(root.get(key).is_some(), "missing thread item.{key} in {root}");
    }
    // the claim posted `--of <req_id>` in seed() hangs off the same thread root.
    assert!(arr.iter().any(|m| m["type"] == "claim"), "claim should be part of the thread: {arr:?}");
}

#[test]
fn code_endpoint_returns_snippet_and_degrades_gracefully() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha); // registers + maps "mylib" with lib.rs = "one\ntwo\nthree\nfour\n"
    let server = start_server(&alpha);

    // missing required query params → 400.
    for path in ["/api/code", "/api/code?repo=mylib", "/api/code?repo=mylib&path=lib.rs", "/api/code?repo=&path=lib.rs&sha=HEAD"] {
        let (status, body) = http_get(&server.addr, path);
        assert_eq!(status, 400, "path {path} body {body}");
    }

    // a real, mapped repo + a valid range returns the lines + rust language detection.
    let (status, body) = http_get(&server.addr, "/api/code?repo=mylib&path=lib.rs&sha=HEAD&range=L1-2");
    assert_eq!(status, 200, "body: {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("bad json ({e}): {body}"));
    assert_eq!(v["lang"], "rust");
    let lines = v["lines"].as_array().expect("lines array");
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["text"], "one");
    assert_eq!(lines[1]["text"], "two");
    assert!(v["staleness"].is_string());

    // a malformed range is a 400, not silently ignored.
    let (s2, b2) = http_get(&server.addr, "/api/code?repo=mylib&path=lib.rs&sha=HEAD&range=notarange");
    assert_eq!(s2, 400, "body: {b2}");

    // an UNMAPPED repo degrades gracefully: 200, empty lines, never an error just
    // because this machine doesn't have the clone. A symbolic sha (HEAD) is "unpinned"
    // regardless of clone status; a full-hex sha against no clone is "unknown".
    let (s3, b3) = http_get(&server.addr, "/api/code?repo=nope-no-such-repo&path=x.rs&sha=HEAD");
    assert_eq!(s3, 200, "body: {b3}");
    let v3: serde_json::Value = serde_json::from_str(&b3).unwrap();
    assert_eq!(v3["lines"].as_array().unwrap().len(), 0);
    assert_eq!(v3["staleness"], "unpinned");

    let full_hex_sha = "a".repeat(40);
    let (s3b, b3b) = http_get(&server.addr, &format!("/api/code?repo=nope-no-such-repo&path=x.rs&sha={full_hex_sha}"));
    assert_eq!(s3b, 200, "body: {b3b}");
    let v3b: serde_json::Value = serde_json::from_str(&b3b).unwrap();
    assert_eq!(v3b["lines"].as_array().unwrap().len(), 0);
    assert_eq!(v3b["staleness"], "unknown");

    // unknown hub still 404s the same way as the other endpoints.
    let (s4, b4) = http_get(&server.addr, "/api/code?repo=mylib&path=lib.rs&sha=HEAD&hub=nope-such-hub");
    assert_eq!(s4, 404, "body: {b4}");
}

#[test]
fn unknown_api_path_404s() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha);
    let server = start_server(&alpha);

    let (status, body) = http_get(&server.addr, "/api/nope-not-a-real-endpoint");
    assert_eq!(status, 404, "body: {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(v.get("error").is_some());
}

#[test]
fn root_serves_spa_and_classic_serves_server_html() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha);
    let server = start_server(&alpha);

    // `/` serves the embedded single-file SPA (script-driven, self-contained).
    let (status, body) = http_get(&server.addr, "/");
    assert_eq!(status, 200);
    assert!(body.contains("<script"), "root should serve the SPA: {}", &body[..body.len().min(400)]);
    assert!(!body.contains("confer web view"), "root should be the SPA, not the classic page");

    // `/classic` is the no-JS server-rendered fallback.
    let (cs, cbody) = http_get(&server.addr, "/classic");
    assert_eq!(cs, 200);
    assert!(cbody.contains("confer web view"), "classic body: {cbody}");
}
