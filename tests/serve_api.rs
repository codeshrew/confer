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
    let home = tmp("home");
    std::fs::create_dir_all(home.join(".confer")).unwrap();
    new_hub_with_home(home)
}

/// Same as `new_hub()`, but sharing a CALLER-PROVIDED home dir rather than minting a
/// fresh one — lets a test put TWO distinct hubs under the same `~/.confer/hubs.json`
/// registry (the realistic shape of the P0 leak: one machine identity following more
/// than one hub, `confer serve` scoped to just one of them).
fn new_hub_with_home(home: PathBuf) -> Hub {
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
    // Give each hub a DISTINCT root-commit date so two independently-seeded hubs never
    // collide on their root-commit SHA. `crosshub::hub_dirs()` dedupes the hubs a machine
    // follows by root SHA (the F3 identity anchor); with byte-identical seed content, two
    // hubs whose root commits land in the SAME wall-clock second hash to the SAME root, so
    // one is silently dropped. On a fast CI box the two `new_hub_with_home` seed commits
    // fall in one second → the shared-home two-hub fixture collapsed to one hub and
    // `refs … --all-hubs` saw hub B vanish ([] instead of its ref). A per-hub monotonic
    // date makes the root SHA deterministically unique regardless of machine speed, while
    // leaving the hub tree byte-identical to a real hub.
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let date = format!("@{} +0000", 978_307_200u64 + u64::from(n)); // 2001-01-01Z + n secs
    let commit = Command::new("git")
        .arg("-C")
        .arg(&seed)
        .args(["-c", "user.name=t", "-c", "user.email=t@t.local", "-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .args(["commit", "-q", "-m", "init"])
        .output()
        .expect("run git");
    assert!(commit.status.success(), "seed commit failed: {}", String::from_utf8_lossy(&commit.stderr));
    git(&seed, &["remote", "add", "origin", bare.to_str().unwrap()]);
    assert!(git(&seed, &["push", "-q", "-u", "origin", "main"]).status.success());
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
    start_server_with(c, &[])
}

/// Same as `start_server`, but with extra CLI args appended (e.g. `&["--all-hubs"]`) —
/// used by the P0 scoping tests, which need to start `confer serve` both with and
/// without `--all-hubs` against the same shared-HOME two-hub fixture.
fn start_server_with(c: &Clone, extra: &[&str]) -> Server {
    // Bind an EPHEMERAL port (`:0`) and read back the actual port serve prints — the OS
    // assigns it atomically at bind, so there's no find-a-free-port-then-bind race
    // (parallel tests picking the same "free" port was the flake this replaces).
    let mut args: Vec<&str> = vec!["serve", "--bind", "127.0.0.1:0"];
    args.extend_from_slice(extra);
    let mut child = Command::new(BIN)
        .env("HOME", &c.home)
        .env("CONFER_HUB", &c.dir)
        .env("CONFER_ROLE", &c.role)
        .args(&args)
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
        // serve binds 127.0.0.1 by default (loopback-private) and prints the actual bound
        // port in a `http://<host>:<port>` line. Parse the port off the first `:` after
        // `http://`, host-agnostically, so a banner-wording change can't wedge this loop.
        if let Some(after) = line.split("http://").nth(1) {
            if let Some(port_part) = after.split(':').nth(1) {
                let port: String = port_part.trim().chars().take_while(|ch| ch.is_ascii_digit()).collect();
                if !port.is_empty() {
                    addr = format!("127.0.0.1:{port}");
                    break;
                }
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
/// EOF. Returns (status, head, body) — `head` (raw header block, one line per header)
/// so callers that need to assert on response headers (the P3 security-headers test)
/// don't need a second request path.
fn http_get_full(addr: &str, path: &str) -> (u16, String, String) {
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    write!(s, "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf); // best-effort: a read timeout still leaves buf usable
    let text = String::from_utf8_lossy(&buf).into_owned();
    let mut parts = text.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or("").to_string();
    let body = parts.next().unwrap_or("").to_string();
    let status: u16 = head.lines().next().and_then(|l| l.split_whitespace().nth(1)).and_then(|s| s.parse().ok()).unwrap_or(0);
    (status, head, body)
}

/// Returns (status, body) — the common case that doesn't care about headers.
fn http_get(addr: &str, path: &str) -> (u16, String) {
    let (status, _head, body) = http_get_full(addr, path);
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
    // design/47 §4.2 items 3/4/6: `liveness`/`hbAgeSecs`/`trust` are ADDITIVE alongside the
    // existing `live` bool and `verified` enum — old consumers keep working.
    for key in [
        "id", "display", "desc", "profileMarkdown", "expectedHost", "lastTs", "lastHost", "live", "liveness", "hbAgeSecs", "verified", "trust", "version",
        "watchState", "keyFingerprint", "color", "abbr", "wip",
    ] {
        assert!(agent.get(key).is_some(), "missing agent.{key} in {agent}");
    }
    assert!(matches!(agent["verified"].as_str(), Some("signed" | "first-sight" | "unverified")));
    assert!(matches!(agent["trust"].as_str(), Some("signed" | "first-sight" | "mismatch" | "unverified")));
    assert!(matches!(agent["liveness"].as_str(), Some("live" | "stale" | "down")));
    // No presence ever published for the seeded agents in this test → no beat → `down` +
    // no age, and `live` derives false from it (liveness === "live").
    assert_eq!(agent["liveness"], "down");
    assert_eq!(agent["live"], false);
    assert!(agent["hbAgeSecs"].is_null());
    assert!(agent["version"].is_null(), "no beat at all → no build to report: {agent}");
    assert!(agent["watchState"].is_null(), "no trustworthy beat → honestly unknown, not 'idle': {agent}");
    assert!(agent["keyFingerprint"].is_null(), "seeded agents never joined/published a key: {agent}");
}

/// Force-pushes a raw presence beat straight onto `refs/presence/<role>` (bypassing
/// `confer`'s own `watch`), the same low-level technique `tests/cli.rs`'s presence-trust
/// tests use — lets a test dictate an exact `last_seen`/signed-ness that a real watcher's
/// timing can't reliably produce. `sign_key` present → sign the beat commit with that SSH
/// key (`commit-tree -S`, as `presence::publish` does for a real agent); `None` → an
/// unsigned commit (the "forged/legacy" beat shape).
fn push_beat(dir: &Path, role: &str, ts: &str, sign_key: Option<&Path>) {
    push_beat_ex(dir, role, ts, sign_key, None);
}

/// Same as `push_beat`, plus an optional `build` (the pin-form `"<semver> <sha>"` a real
/// heartbeat carries) — lets a test exercise the honest-nullable `version` projection
/// without needing a real running watcher.
fn push_beat_ex(dir: &Path, role: &str, ts: &str, sign_key: Option<&Path>, build: Option<&str>) {
    let mk = match sign_key {
        Some(key) => format!(
            "git -c gpg.format=ssh -c user.signingkey='{}' -c gpg.ssh.program=ssh-keygen commit-tree $t -S -m beat",
            key.display()
        ),
        None => "git commit-tree $t -m beat".to_string(),
    };
    let build_field = match build {
        Some(b) => format!(",\"build\":\"{b}\""),
        None => String::new(),
    };
    let o = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "cd '{dir}' && printf '{{\"role\":\"{role}\",\"last_seen\":\"{ts}\",\"poll_secs\":10{build_field}}}' > pres.json && \
             b=$(git hash-object -w pres.json) && \
             t=$(printf '100644 blob %s\\tpresence.json\\n' \"$b\" | git mktree) && \
             c=$({mk}) && \
             git update-ref refs/presence/{role} $c && \
             git push --force origin refs/presence/{role}:refs/presence/{role} && rm -f pres.json",
            dir = dir.display()
        ))
        .output()
        .unwrap();
    assert!(o.status.success(), "push_beat({role}, sign={}): {}", sign_key.is_some(), String::from_utf8_lossy(&o.stderr));
}

/// Generates an ed25519 keypair under `dir/<name>`, returns the private key path.
fn keygen(dir: &Path, name: &str) -> PathBuf {
    let key = dir.join(name);
    let st = Command::new("ssh-keygen").args(["-t", "ed25519", "-f", key.to_str().unwrap(), "-N", "", "-C", name, "-q"]).status().unwrap();
    assert!(st.success(), "ssh-keygen failed for {name}");
    key
}

#[test]
fn overview_three_state_liveness_and_heartbeat_age() {
    // design/47 §4.2 items 3+6: three roles, each with exactly one SIGNED beat at a
    // different age, exercise all three `presence::Live` states end-to-end through
    // `/api/overview`'s `liveness`/`hbAgeSecs`/`live` fields.
    let hub = new_hub();
    let keydir = tmp("key");

    // Each role joins from its OWN clone (`join` sets that clone's local git identity/
    // signing config), but all push cards + beats to the same bare remote, so any clone's
    // `.dir` can be used afterward to read/write shared refs (presence, messages).
    let alpha = hub.clone("alpha");
    let beta = hub.clone("beta");
    let gamma = hub.clone("gamma");
    for (c, role) in [(&alpha, "alpha"), (&beta, "beta"), (&gamma, "gamma")] {
        let key = keygen(&keydir, role);
        assert!(ok(&c.confer(&["join", "--role", role, "--signing-key", key.to_str().unwrap()])));
    }
    // A note (from alpha's now-joined clone) so all three roles show up in the
    // roster/agents union regardless of presence.
    assert!(ok(&alpha.append(&["--type", "note", "--to", "beta", "--summary", "hi", "--text", "hi"])));

    let now = chrono::Utc::now();
    let ts = |secs_ago: i64| (now - chrono::Duration::seconds(secs_ago)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    push_beat(&alpha.dir, "alpha", &ts(30), Some(&keydir.join("alpha"))); // well within the live window
    push_beat(&alpha.dir, "beta", &ts(1200), Some(&keydir.join("beta"))); // 20 min ago → stale
    push_beat(&alpha.dir, "gamma", &ts(2400), Some(&keydir.join("gamma"))); // 40 min ago → down

    let server = start_server(&alpha);
    let (status, body) = http_get(&server.addr, "/api/overview");
    assert_eq!(status, 200, "body: {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let fleet = v["fleet"].as_array().expect("fleet array");
    let find = |id: &str| fleet.iter().find(|a| a["id"] == id).unwrap_or_else(|| panic!("no {id} in {fleet:?}"));

    let a = find("alpha");
    assert_eq!(a["liveness"], "live", "alpha: {a}");
    assert_eq!(a["live"], true, "alpha: {a}");
    assert!(a["hbAgeSecs"].as_i64().unwrap() < 120, "alpha hbAgeSecs should be small: {a}");

    let b = find("beta");
    assert_eq!(b["liveness"], "stale", "beta: {b}");
    assert_eq!(b["live"], false, "beta: {b}");
    let b_age = b["hbAgeSecs"].as_i64().unwrap();
    assert!((1100..1400).contains(&b_age), "beta hbAgeSecs ~1200: {b}");

    let g = find("gamma");
    assert_eq!(g["liveness"], "down", "gamma: {g}");
    assert_eq!(g["live"], false, "gamma: {g}");
    let g_age = g["hbAgeSecs"].as_i64().unwrap();
    assert!(g_age > 1800, "gamma hbAgeSecs should be well past the stale window: {g}");
}

#[test]
fn overview_agent_projects_version_watch_state_and_key_fingerprint() {
    // The three new per-agent fields, same honest-nullable pattern as tier/sync/seenBy:
    // never faked/guessed, present only when the underlying signal actually exists.
    //  - `version`: the agent's own build, straight off its (trust-gated) heartbeat.
    //  - `watchState`: the existing 3-state `liveness` re-mapped to armed/idle/null —
    //    no new detection mechanism, a pure fold of the same signal.
    //  - `keyFingerprint`: the pinned signing key's SHA256 fingerprint, when known.
    let hub = new_hub();
    let keydir = tmp("key");

    let alpha = hub.clone("alpha"); // signed + fresh beat WITH a build → live/armed/version+fpr
    let beta = hub.clone("beta"); // signed + stale beat, no build published → idle/no version
    let gamma = hub.clone("gamma"); // signed but no beat at all → down/null/null, still has a card key
    for (c, role) in [(&alpha, "alpha"), (&beta, "beta"), (&gamma, "gamma")] {
        let key = keygen(&keydir, role);
        assert!(ok(&c.confer(&["join", "--role", role, "--signing-key", key.to_str().unwrap()])));
    }
    assert!(ok(&alpha.append(&["--type", "note", "--to", "beta", "--summary", "hi", "--text", "hi"])));
    // delta never joins/publishes a key — it just authors a message (union-of-authors
    // membership), so its whole new-fields shape must stay honestly null. Posted through
    // alpha's OWN clone (rather than a separate delta clone) so it lands in alpha's local
    // working tree immediately — a fresh clone pushing straight to origin wouldn't be
    // visible without a fetch this harness never does (same reason every other test in
    // this file posts through one already-`start_server`'d clone's directory).
    assert!(ok(&alpha.confer(&["append", "--from", "delta", "--type", "note", "--to", "alpha", "--summary", "hi2", "--text", "hi2"])));

    let now = chrono::Utc::now();
    let ts = |secs_ago: i64| (now - chrono::Duration::seconds(secs_ago)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    push_beat_ex(&alpha.dir, "alpha", &ts(30), Some(&keydir.join("alpha")), Some("0.6.9 45a9c04"));
    push_beat_ex(&alpha.dir, "beta", &ts(1200), Some(&keydir.join("beta")), None); // stale, legacy no-build beat
    // gamma: no beat published at all.

    let server = start_server(&alpha);
    let (status, body) = http_get(&server.addr, "/api/overview");
    assert_eq!(status, 200, "body: {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let fleet = v["fleet"].as_array().expect("fleet array");
    let find = |id: &str| fleet.iter().find(|a| a["id"] == id).unwrap_or_else(|| panic!("no {id} in {fleet:?}"));

    let a = find("alpha");
    assert_eq!(a["liveness"], "live", "alpha: {a}");
    assert_eq!(a["watchState"], "armed", "alpha watchState should fold live->armed: {a}");
    assert_eq!(a["version"], "0.6.9 (45a9c04)", "alpha version should be the human build label: {a}");
    assert!(a["keyFingerprint"].as_str().unwrap_or("").starts_with("SHA256:"), "alpha keyFingerprint: {a}");

    let b = find("beta");
    assert_eq!(b["liveness"], "stale", "beta: {b}");
    assert_eq!(b["watchState"], "idle", "beta watchState should fold stale->idle: {b}");
    assert!(b["version"].is_null(), "beta published no build on its beat, so version stays null: {b}");
    assert!(b["keyFingerprint"].as_str().unwrap_or("").starts_with("SHA256:"), "beta still has a pinned key: {b}");

    let g = find("gamma");
    assert_eq!(g["liveness"], "down", "gamma: {g}");
    assert!(g["watchState"].is_null(), "gamma has no trustworthy beat, so watchState is honestly null (not 'idle'): {g}");
    assert!(g["version"].is_null(), "gamma has no beat at all, so version is null: {g}");
    // gamma DID join with a card/key, so its fingerprint should still resolve from the
    // card-trust path independent of any beat.
    assert!(g["keyFingerprint"].as_str().unwrap_or("").starts_with("SHA256:"), "gamma keyFingerprint from its card: {g}");

    let d = find("delta");
    assert_eq!(d["liveness"], "down", "delta: {d}");
    assert!(d["watchState"].is_null(), "delta: {d}");
    assert!(d["version"].is_null(), "delta: {d}");
    assert!(d["keyFingerprint"].is_null(), "delta never joined/published a key, so keyFingerprint is honestly null: {d}");
}

#[test]
fn overview_agent_projects_full_role_profile_markdown_body() {
    // `profileMarkdown` = the FULL markdown body below a `roles/<id>.md` card's frontmatter
    // (the prose `desc` is NOT). Honest-nullable: a frontmatter-only card → null. Written
    // straight into the clone's working tree (what serve's roster fold reads), same as any
    // hand-authored role card.
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    let roles = alpha.dir.join("roles");
    std::fs::create_dir_all(&roles).unwrap();
    // A card with a real prose body below the closing fence.
    std::fs::write(
        roles.join("prosy.md"),
        "---\ndisplay: Prosy\ndesc: one-line summary\n---\n# Prosy\n\nDoes the **profile** work.\nSecond line.\n",
    )
    .unwrap();
    // A card with frontmatter ONLY (no body) → profileMarkdown must be null, desc still present.
    std::fs::write(roles.join("bare.md"), "---\ndisplay: Bare\ndesc: just a slug\n---\n").unwrap();

    let server = start_server(&alpha);
    let (status, body) = http_get(&server.addr, "/api/overview");
    assert_eq!(status, 200, "body: {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let fleet = v["fleet"].as_array().expect("fleet array");
    let find = |id: &str| fleet.iter().find(|a| a["id"] == id).unwrap_or_else(|| panic!("no {id} in {fleet:?}"));

    let p = find("prosy");
    assert_eq!(p["desc"], "one-line summary", "desc stays the one-line frontmatter field: {p}");
    assert_eq!(
        p["profileMarkdown"], "# Prosy\n\nDoes the **profile** work.\nSecond line.",
        "profileMarkdown is the full body below the frontmatter, newlines preserved: {p}"
    );

    let b = find("bare");
    assert_eq!(b["desc"], "just a slug", "bare card still has its desc slug: {b}");
    assert!(b["profileMarkdown"].is_null(), "a frontmatter-only card has no body, so profileMarkdown is null: {b}");
}

#[test]
fn overview_forged_heartbeat_does_not_render_live() {
    // design/47 §4.2 items 3+6, the trust hole: the health path used to read UNVERIFIED
    // presence (`presence::load_all`), so a forged/replayed heartbeat rendered `live: true`.
    // Here alpha signs a real beat first (pinning its key + recording it as a
    // presence-signer), then a FRESHER but UNSIGNED beat is pushed — a downgrade that
    // `presence::load_verified` must reject (`BeatTrust::Untrusted`). Even though the forged
    // beat's own timestamp is recent enough to look "live", `/api/overview` must NOT report
    // the agent as live from it.
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    let keydir = tmp("key");
    let key = keygen(&keydir, "alpha");
    assert!(ok(&alpha.confer(&["join", "--role", "alpha", "--signing-key", key.to_str().unwrap()])));
    assert!(ok(&alpha.append(&["--type", "note", "--to", "beta", "--summary", "hi", "--text", "hi"])));

    let now = chrono::Utc::now();
    let ts = |secs_ago: i64| (now - chrono::Duration::seconds(secs_ago)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    push_beat(&alpha.dir, "alpha", &ts(60), Some(&key)); // a real signed beat: pins the key, ever_signed=true
    push_beat(&alpha.dir, "alpha", &ts(5), None); // a forged, unsigned "fresher" beat — must be rejected

    let server = start_server(&alpha);
    let (status, body) = http_get(&server.addr, "/api/overview");
    assert_eq!(status, 200, "body: {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let fleet = v["fleet"].as_array().expect("fleet array");
    let a = fleet.iter().find(|a| a["id"] == "alpha").unwrap_or_else(|| panic!("no alpha in {fleet:?}"));
    assert_eq!(a["live"], false, "a forged/rejected beat must not render live:true: {a}");
    assert_eq!(a["liveness"], "down", "a forged beat is treated as no trustworthy beat at all: {a}");
    assert!(a["hbAgeSecs"].is_null(), "no trustworthy age to report from a rejected beat: {a}");
}

#[test]
fn overview_trust_field_distinguishes_signed_first_sight_mismatch_and_unverified() {
    // design/47 §4.2 item 2: `verified` folds `mismatch` into `unverified` for the closed
    // legacy enum; the additive `trust` field must expose the real state.
    //
    // The keyring (pin/confirmed store) lives under `$HOME/.confer` and `join` self-confirms
    // the identity it just created (a human running `join` IS the out-of-band confirmation
    // for itself) — so from alpha's OWN home, alpha's key is `signed`, never `first-sight`.
    // `first-sight` is a genuinely different-observer state: a separate machine/home seeing
    // alpha's key for the first time, before ITS operator has confirmed it. So this test uses
    // two distinct `$HOME`s (this rig's `Hub::clone` shares one home across roles by default,
    // modeling "one machine hosts several role clones" — realistic, but not what first-sight
    // needs) to model that.
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    let keydir = tmp("key");
    let key1 = keygen(&keydir, "alpha1");
    assert!(ok(&alpha.confer(&["join", "--role", "alpha", "--signing-key", key1.to_str().unwrap()])));
    assert!(ok(&alpha.append(&["--type", "note", "--to", "beta", "--summary", "hi", "--text", "hi"])));
    // beta never joins with a key → stays `unverified` throughout (posted from alpha's clone
    // with an explicit `--from beta`; `card_trust` checks the ROSTER's published key for the
    // `from` role, not who actually signed the underlying commit, so this is enough to put
    // "beta" into the agents union as an unsigned role).
    assert!(ok(&alpha.confer(&["append", "--from", "beta", "--type", "note", "--to", "alpha", "--summary", "hi2", "--text", "hi2"])));

    let get_agent = |server: &Server, id: &str| -> serde_json::Value {
        let (status, body) = http_get(&server.addr, "/api/overview");
        assert_eq!(status, 200, "body: {body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        v["fleet"].as_array().unwrap().iter().find(|a| a["id"] == id).unwrap_or_else(|| panic!("no {id} in {v}")).clone()
    };

    // 1) alpha's OWN view of itself: self-confirmed at join → signed.
    let self_server = start_server(&alpha);
    let a_self = get_agent(&self_server, "alpha");
    assert_eq!(a_self["trust"], "signed", "{a_self}");
    assert_eq!(a_self["verified"], "signed", "{a_self}");
    let b_self = get_agent(&self_server, "beta");
    assert_eq!(b_self["trust"], "unverified", "{b_self}");
    assert_eq!(b_self["verified"], "unverified", "{b_self}");
    drop(self_server);

    // 2) a fresh OBSERVER machine (its own $HOME, never confirmed anything) seeing alpha's
    // card for the first time: TOFU-pins it right here → first-sight (signed, not yet vouched
    // for by THIS machine's operator).
    let mut observer = hub.clone("observer");
    let observer_home = tmp("observer-home");
    std::fs::create_dir_all(observer_home.join(".confer")).unwrap();
    observer.home = observer_home;
    let obs_server = start_server(&observer);
    let a_obs = get_agent(&obs_server, "alpha");
    assert_eq!(a_obs["trust"], "first-sight", "{a_obs}");
    assert_eq!(a_obs["verified"], "first-sight", "{a_obs}");
    drop(obs_server);

    // 3) A hub writer rewrites alpha's published pubkey in the card → the loud MISMATCH
    // alarm — detected by ANY observer that had already pinned the old key, including
    // alpha's own machine. `verified` (the closed legacy enum) folds it into "unverified";
    // `trust` must not. A freshly-started server picks up the rewritten card (roster is
    // cached per `serve` process, refreshed only periodically; restarting forces a fresh
    // fold rather than waiting out that refresh interval).
    let key2 = keygen(&keydir, "alpha2");
    let newpub = std::fs::read_to_string(format!("{}.pub", key2.display())).unwrap();
    let card = alpha.dir.join("roles/alpha.md");
    let txt = std::fs::read_to_string(&card).unwrap();
    let mut out_lines = Vec::new();
    for line in txt.lines() {
        if line.starts_with("pubkey:") {
            out_lines.push(format!("pubkey: {}", newpub.trim()));
        } else {
            out_lines.push(line.to_string());
        }
    }
    std::fs::write(&card, out_lines.join("\n") + "\n").unwrap();
    git(&alpha.dir, &["add", "roles/alpha.md"]);
    git(&alpha.dir, &["-c", "commit.gpgsign=false", "commit", "-q", "-m", "swap alpha's key"]);
    assert!(git(&alpha.dir, &["push", "-q", "origin", "HEAD"]).status.success());

    let mismatch_server = start_server(&alpha);
    let a3 = get_agent(&mismatch_server, "alpha");
    assert_eq!(a3["trust"], "mismatch", "{a3} — the loud impersonation alarm must not be folded away");
    assert_eq!(a3["verified"], "unverified", "{a3} — the closed legacy enum keeps folding mismatch into unverified");
    drop(mismatch_server);
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
    for key in ["repo", "path", "sha", "range", "contentHash", "refName", "refType", "commitDate", "dirty", "untracked", "baseRef", "forkPoint"] {
        assert!(refs[0].get(key).is_some(), "missing ref.{key}");
    }
    assert_eq!(refs[0]["repo"], "mylib");

    let (_, all_body) = http_get(&server.addr, "/api/messages");
    let all: serde_json::Value = serde_json::from_str(&all_body).unwrap();
    assert!(all.as_array().unwrap().len() >= 2, "no topic filter returns everything");
}

#[test]
fn messages_pagination_limit_and_before() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha); // already posts a note + a request (2 messages) in different topics

    // Add several more messages on the "search" topic so there's a real page to walk.
    for i in 0..4 {
        assert!(ok(&alpha.append(&[
            "--type", "note", "--to", "beta", "--summary", &format!("update {i}"), "--text", "body", "--topic", "search",
        ])));
    }

    let server = start_server(&alpha);

    // No `limit` at all → back-compat: every message on the hub.
    let (status_all, body_all) = http_get(&server.addr, "/api/messages");
    assert_eq!(status_all, 200);
    let all: serde_json::Value = serde_json::from_str(&body_all).unwrap();
    let all_arr = all.as_array().expect("array");
    assert_eq!(all_arr.len(), 7, "1 note + 1 request + 1 claim + 4 more notes: {all_arr:?}");

    // `?topic=search&limit=2` → the 2 newest search-topic messages, chronological order.
    let (status, body) = http_get(&server.addr, "/api/messages?topic=search&limit=2");
    assert_eq!(status, 200, "body: {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let page1 = v.as_array().expect("array");
    assert_eq!(page1.len(), 2, "limit=2: {page1:?}");
    let ids: Vec<&str> = page1.iter().map(|m| m["id"].as_str().unwrap()).collect();
    assert!(ids[0] < ids[1], "chronological order (oldest first) within the page: {ids:?}");
    assert_eq!(page1[1]["summary"], "update 3", "the newest of the search topic");

    // `before=<newest-id-in-page1>` → the next-older page, excluding that id.
    let newest_id = ids[1];
    let (status2, body2) = http_get(&server.addr, &format!("/api/messages?topic=search&limit=2&before={newest_id}"));
    assert_eq!(status2, 200, "body: {body2}");
    let v2: serde_json::Value = serde_json::from_str(&body2).unwrap();
    let page2 = v2.as_array().expect("array");
    assert_eq!(page2.len(), 2, "page2: {page2:?}");
    let ids2: Vec<&str> = page2.iter().map(|m| m["id"].as_str().unwrap()).collect();
    assert!(ids2.iter().all(|id| *id < newest_id), "every id in page2 is strictly older: {ids2:?}");
    assert!(!ids2.contains(&newest_id), "before is exclusive");
    assert_eq!(page2[1]["summary"], "update 2", "the next-newest after excluding update 3");
}

#[test]
fn refs_reverse_lookup_returns_ref_hits() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha);
    let server = start_server(&alpha);

    // The hub's own id (same form `/api/hubs` reports) — refs hits must carry a
    // matching `hub` so a cross-hub (`allHubs=1`) caller can route each hit back
    // to the hub it came from.
    let (hubs_status, hubs_body) = http_get(&server.addr, "/api/hubs");
    assert_eq!(hubs_status, 200);
    let hubs_v: serde_json::Value = serde_json::from_str(&hubs_body).unwrap();
    let expected_hub_id = hubs_v.as_array().expect("array")[0]["id"].as_str().unwrap().to_string();

    let (status, body) = http_get(&server.addr, "/api/refs?target=mylib:lib.rs");
    assert_eq!(status, 200);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 1);
    let hit = &arr[0];
    for key in ["repo", "path", "sha", "range", "contentHash", "staleness", "msgId", "from", "msgType", "ts", "topic", "summary", "threadRoot", "requestStatus", "hub", "hubPrivate"] {
        assert!(hit.get(key).is_some(), "missing refhit.{key} in {hit}");
    }
    assert_eq!(hit["repo"], "mylib");
    assert_eq!(hit["requestStatus"], "CLAIMED");
    assert_eq!(hit["hub"].as_str().unwrap(), expected_hub_id, "refhit.hub must match /api/hubs's id for this hub");
    assert!(!hit["hub"].as_str().unwrap().is_empty());
    // design/47 §4.2 item 4: no cheap synchronous hub-visibility signal exists (the real
    // check is a live network probe, wrong to run per-request), so `hubPrivate` reports
    // the honest "unknown" (`null`) rather than a misleading hardcoded `false`.
    assert!(hit["hubPrivate"].is_null(), "refhit.hubPrivate must be null (unknown) pending a cheap sync signal: {hit}");

    // A file nothing references comes back empty, not an error.
    let (status2, body2) = http_get(&server.addr, "/api/refs?target=mylib:other.rs");
    assert_eq!(status2, 200);
    let v2: serde_json::Value = serde_json::from_str(&body2).unwrap();
    assert_eq!(v2.as_array().unwrap().len(), 0);

    // design/44 §0/§4: a bare, PATH-LESS repo target ("every conversation touching
    // anything in this repo") must parse and return the same hits as the file-scoped
    // query — no engine/API gap, confirming the repo-rollup query already works.
    let (status3, body3) = http_get(&server.addr, "/api/refs?target=mylib");
    assert_eq!(status3, 200, "body: {body3}");
    let v3: serde_json::Value = serde_json::from_str(&body3).unwrap();
    let arr3 = v3.as_array().expect("array");
    assert_eq!(arr3.len(), 1, "bare-repo query must find the same hit as the file-scoped one: {arr3:?}");
    assert_eq!(arr3[0]["repo"], "mylib");
    assert_eq!(arr3[0]["path"], "lib.rs");

    // The new design/44 fields ride along on every hit (camelCase; null when absent —
    // legacy/never-set fields degrade gracefully rather than being omitted).
    let hit = &arr3[0];
    for key in ["refName", "refType", "commitDate", "dirty", "untracked", "baseRef", "forkPoint"] {
        assert!(hit.get(key).is_some(), "missing refhit.{key} in {hit}");
    }
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
    seed(&alpha); // registers + maps "mylib" with lib.rs = "one\ntwo\nthree\nfour\n"; refs mylib:lib.rs#L1-2
    let server = start_server(&alpha);

    // missing required query params → 400.
    for path in ["/api/code", "/api/code?repo=mylib", "/api/code?repo=mylib&path=lib.rs", "/api/code?repo=&path=lib.rs&sha=HEAD"] {
        let (status, body) = http_get(&server.addr, path);
        assert_eq!(status, 400, "path {path} body {body}");
    }

    // Recover the exact (repo, path, sha) `seed()` actually pinned via `/api/refs` — the
    // P2 restriction (Jarvis's 0.8.0 review) means `/api/code` only serves tuples that
    // appear in the hub's RefIndex, so a literal "HEAD" (never a pinned sha) is no longer
    // a valid probe here.
    let (rstatus, rbody) = http_get(&server.addr, "/api/refs?target=mylib:lib.rs");
    assert_eq!(rstatus, 200, "body: {rbody}");
    let rv: serde_json::Value = serde_json::from_str(&rbody).unwrap();
    let referenced_sha = rv.as_array().unwrap()[0]["sha"].as_str().unwrap().to_string();

    // a real, mapped, REFERENCED repo/path/sha + a valid range returns the lines + rust
    // language detection.
    let (status, body) = http_get(&server.addr, &format!("/api/code?repo=mylib&path=lib.rs&sha={referenced_sha}&range=L1-2"));
    assert_eq!(status, 200, "body: {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("bad json ({e}): {body}"));
    assert_eq!(v["lang"], "rust");
    let lines = v["lines"].as_array().expect("lines array");
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["text"], "one");
    assert_eq!(lines[1]["text"], "two");
    assert!(v["staleness"].is_string());

    // a malformed range on an otherwise-referenced tuple is a 400, not silently ignored.
    let (s2, b2) = http_get(&server.addr, &format!("/api/code?repo=mylib&path=lib.rs&sha={referenced_sha}&range=notarange"));
    assert_eq!(s2, 400, "body: {b2}");

    // P2: a (repo, path, sha) NOBODY ever referenced via --ref 404s — this is the actual
    // fix, not a side effect. Same repo+path, wrong sha:
    let unreferenced_sha = "f".repeat(40);
    let (s3, b3) = http_get(&server.addr, &format!("/api/code?repo=mylib&path=lib.rs&sha={unreferenced_sha}"));
    assert_eq!(s3, 404, "an unreferenced sha must 404, not full-repo-browse: {b3}");
    // same repo, an unreferenced path:
    let (s3b, b3b) = http_get(&server.addr, &format!("/api/code?repo=mylib&path=never-referenced.rs&sha={referenced_sha}"));
    assert_eq!(s3b, 404, "an unreferenced path must 404: {b3b}");
    // a repo nobody ever referenced at all:
    let (s3c, b3c) = http_get(&server.addr, "/api/code?repo=nope-no-such-repo&path=x.rs&sha=HEAD");
    assert_eq!(s3c, 404, "an unreferenced repo must 404: {b3c}");

    // A REFERENCED tuple whose repo happens not to be locally cloned still degrades
    // gracefully (200, empty lines) rather than erroring — the P2 restriction is about
    // WHICH tuples are servable, not a replacement for the existing clone-map graceful
    // degradation.
    let unmapped_sha = "b".repeat(40);
    assert!(ok(&alpha.append(&[
        "--type", "note", "--to", "beta", "--summary", "unmapped ref", "--text", "poking",
        "--ref", &format!("neverlib:x.rs@{unmapped_sha}#L1-2"),
    ])));
    let (s5, b5) = http_get(&server.addr, &format!("/api/code?repo=neverlib&path=x.rs&sha={unmapped_sha}"));
    assert_eq!(s5, 200, "a referenced-but-unmapped repo must still degrade gracefully: {b5}");
    let v5: serde_json::Value = serde_json::from_str(&b5).unwrap();
    assert_eq!(v5["lines"].as_array().unwrap().len(), 0);
    assert_eq!(v5["staleness"], "unknown");

    // unknown hub still 404s the same way as the other endpoints (and before the
    // referenced-tuple check even runs).
    let (s4, b4) = http_get(&server.addr, &format!("/api/code?repo=mylib&path=lib.rs&sha={referenced_sha}&hub=nope-such-hub"));
    assert_eq!(s4, 404, "body: {b4}");
}

#[test]
fn codefiles_lists_distinct_referenced_files_ordered_by_ref_count() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha); // one ref: mylib:lib.rs#L1-2 (mapped, since seed() maps mylib)

    // "wealdlore" is registered nowhere and never mapped on this machine — a second,
    // distinct file referenced TWICE, so it should outrank mylib:lib.rs (1 ref) once
    // sorted, and its `mapped` should come back false.
    let fake_sha = "b".repeat(40);
    assert!(ok(&alpha.append(&[
        "--type", "note", "--to", "beta", "--summary", "first look", "--text", "poking around",
        "--ref", &format!("wealdlore:pipeline/plates.py@{fake_sha}#L1-2"),
    ])));
    assert!(ok(&alpha.append(&[
        "--type", "note", "--to", "beta", "--summary", "second look", "--text", "still poking",
        "--ref", &format!("wealdlore:pipeline/plates.py@{fake_sha}#L3-4"),
    ])));

    let server = start_server(&alpha);

    // missing/empty ?hub= falls back to the current hub (same as the other handlers).
    let (status, body) = http_get(&server.addr, "/api/codefiles");
    assert_eq!(status, 200, "body: {body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("bad json ({e}): {body}"));
    let arr = v.as_array().expect("array");
    assert_eq!(arr.len(), 2, "two distinct referenced files: {arr:?}");

    // twice-referenced file sorts first.
    let first = &arr[0];
    assert_eq!(first["repo"], "wealdlore");
    assert_eq!(first["path"], "pipeline/plates.py");
    assert_eq!(first["refCount"], 2);
    assert_eq!(first["mapped"], false, "wealdlore was never mapped in this test's isolated HOME");
    assert!(first["lastTs"].as_str().is_some(), "lastTs should be present: {first}");

    let second = &arr[1];
    assert_eq!(second["repo"], "mylib");
    assert_eq!(second["path"], "lib.rs");
    assert_eq!(second["refCount"], 1);
    assert_eq!(second["mapped"], true, "mylib was mapped by seed()");

    // unknown hub still 404s the same way as the other endpoints.
    let (s2, b2) = http_get(&server.addr, "/api/codefiles?hub=nope-such-hub");
    assert_eq!(s2, 404, "body: {b2}");
    let v2: serde_json::Value = serde_json::from_str(&b2).unwrap();
    assert!(v2.get("error").is_some());
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

    // `/` serves the embedded single-file SPA (script-driven, self-contained) — built by
    // `npm --prefix ui run build` before `cargo build` (see build.rs). Without that build
    // step `build.rs` embeds a documented placeholder page instead (so `cargo build`/`cargo
    // test` still work with no `node` toolchain at all) — this used to be a CI landmine:
    // this exact assertion would spuriously fail on any checkout that ran `cargo test`
    // without first building the UI. Skip the SPA-shape assertion gracefully in that case
    // (loudly, via eprintln, so it's visibly a skip and not a silently-vanished check) —
    // `/api/*` and `/classic` are unaffected by whether the UI was built and are asserted
    // unconditionally below.
    let (status, body) = http_get(&server.addr, "/");
    assert_eq!(status, 200);
    if body.contains("dashboard isn't built yet") {
        eprintln!(
            "root_serves_spa_and_classic_serves_server_html: ui/dist wasn't built (placeholder page) — \
             skipping the SPA-shape assertion; run `npm --prefix ui install && npm --prefix ui run build` first to exercise it"
        );
    } else {
        assert!(body.contains("<script"), "root should serve the SPA: {}", &body[..body.len().min(400)]);
        assert!(!body.contains("confer web view"), "root should be the SPA, not the classic page");
    }

    // `/classic` is the no-JS server-rendered fallback.
    let (cs, cbody) = http_get(&server.addr, "/classic");
    assert_eq!(cs, 200);
    assert!(cbody.contains("confer web view"), "classic body: {cbody}");
}

/// P0 (Jarvis's 0.8.0 review): `/api/refs?allHubs=1` used to jump straight to
/// `crosshub::hub_dirs()` — EVERY hub this machine's identity follows, registered in
/// `~/.confer/hubs.json` — regardless of what `confer serve` was told to expose. The
/// realistic leak shape: one machine identity follows two hubs (unrelated projects), and
/// an operator runs `confer serve --lan` scoped to just one of them. Two SEPARATE hubs
/// sharing one `HOME` (so both land in the same `hubs.json`) reproduces exactly that.
#[test]
fn refs_all_hubs_is_scoped_to_the_operators_own_choice_not_every_hub_on_the_machine() {
    let home = tmp("shared-home");
    std::fs::create_dir_all(home.join(".confer")).unwrap();

    // Hub A: the one the operator actually starts `serve` against.
    let hub_a = new_hub_with_home(home.clone());
    let alpha = hub_a.clone("alpha");
    assert!(ok(&alpha.confer(&["join", "--role", "alpha"])), "alpha join must register hub A in the shared hubs.json");
    seed(&alpha); // refs mylib:lib.rs#L1-2

    // Hub B: a DIFFERENT hub the same machine identity also follows, with its own
    // secret-shaped ref the operator never asked to expose via hub A's server.
    let hub_b = new_hub_with_home(home.clone());
    let gamma = hub_b.clone("gamma");
    assert!(ok(&gamma.confer(&["join", "--role", "gamma"])), "gamma join must register hub B in the shared hubs.json");
    let secret_sha = "c".repeat(40);
    assert!(ok(&gamma.append(&[
        "--type", "note", "--to", "beta", "--summary", "hub B secret", "--text", "shh",
        "--ref", &format!("secretlib:secret.rs@{secret_sha}#L1-2"),
    ])));

    // 1. `confer serve` started scoped to hub A ONLY (no --all-hubs). `allHubs=1` must
    //    NOT reach hub B's content — the P0 leak. It must be rejected outright (400),
    //    not silently narrowed, so a caller relying on fleet-wide results gets a loud
    //    signal instead of a quietly-scoped-down 200.
    {
        let server = start_server(&alpha); // no --all-hubs
        let (status, body) = http_get(&server.addr, "/api/refs?target=secretlib:secret.rs&allHubs=1");
        assert_eq!(status, 400, "allHubs=1 against a NOT-all-hubs server must be rejected, not silently scoped: {body}");

        // hub A's own content is unaffected and never carries hub B's secret ref.
        let (s2, b2) = http_get(&server.addr, "/api/refs?target=mylib:lib.rs");
        assert_eq!(s2, 200, "body: {b2}");
        let v2: serde_json::Value = serde_json::from_str(&b2).unwrap();
        assert_eq!(v2.as_array().unwrap().len(), 1);
        assert!(!b2.contains("secretlib"), "hub A's own-scope query must never see hub B's ref: {b2}");
    }

    // 2. `confer serve --all-hubs` (the operator's own opt-in to the fleet-wide view) DOES
    //    let `allHubs=1` see hub B's content — the same query now legitimately crosses
    //    hubs because the operator consented to that scope at startup.
    {
        let server = start_server_with(&alpha, &["--all-hubs"]);
        let (status, body) = http_get(&server.addr, "/api/refs?target=secretlib:secret.rs&allHubs=1");
        assert_eq!(status, 200, "body: {body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("bad json ({e}): {body}"));
        let arr = v.as_array().expect("array");
        assert_eq!(arr.len(), 1, "hub B's ref must be visible once the operator started --all-hubs: {arr:?}");
        assert_eq!(arr[0]["repo"], "secretlib");
    }
}

/// P3 (defense-in-depth): every non-SSE response carries a CSP + framing/sniffing
/// header set, on the JSON API, `/classic`, and the embedded SPA alike.
#[test]
fn responses_carry_security_headers() {
    let hub = new_hub();
    let alpha = hub.clone("alpha");
    seed(&alpha);
    let server = start_server(&alpha);

    for path in ["/api/hubs", "/classic", "/"] {
        let (status, head, _body) = http_get_full(&server.addr, path);
        assert_eq!(status, 200, "path {path}");
        let head_lower = head.to_lowercase();
        assert!(head_lower.contains("content-security-policy:"), "{path} missing CSP: {head}");
        assert!(head_lower.contains("x-frame-options: deny"), "{path} missing X-Frame-Options: {head}");
        assert!(head_lower.contains("x-content-type-options: nosniff"), "{path} missing X-Content-Type-Options: {head}");
    }
}
