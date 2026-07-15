//! Thin shell-out wrappers over the `git` binary + the hub sync loop.
//!
//! We deliberately shell out rather than link libgit2/gix: it inherits the
//! user's exact git config, SSH keys, credential helpers, and GitHub auth for
//! free, and behaves identically to what they'd run by hand.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

/// Hard cap on any single git subprocess. A hung fetch/push (the 1Password-agent
/// stall) must not wedge a watcher or hold the clone lock forever (audit R2).
/// `CONFER_GIT_TIMEOUT_SECS` overrides the 60s default (operators can tune it;
/// tests shorten it to exercise the timeout without a 60s wait).
fn git_timeout() -> Duration {
    std::env::var("CONFER_GIT_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .map_or(Duration::from_secs(60), Duration::from_secs)
}

fn base(root: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C").arg(root);
    c
}

/// Run git with a timeout and no inherited stdin (so a credential/SSH prompt
/// fails fast instead of blocking forever). Drains pipes on a helper thread to
/// avoid a full-pipe deadlock; kills the child if it exceeds git_timeout().
fn run(root: &Path, args: &[&str]) -> Result<Output> {
    run_to(root, args, git_timeout())
}

/// True if a git invocation failed because another process holds a per-clone git lock — either the
/// `.git/index.lock` (an index-write collision) or the config lock (`git config` reports it as
/// "could not lock config file" / `config.lock`). Both are TRANSIENT: confer runs a background
/// watch/poll and the SessionStart auto-heal fires `reconnect`, so a lock collision at session
/// start (e.g. the auto-reconnect writing `user.name` while a manual op runs) is an ordinary
/// coincidence, not a real error — worth a bounded retry, not a hard fail. A read-only op never
/// creates either lock, so this only ever affects writers.
fn is_lock_contention(out: &Output) -> bool {
    if out.status.success() {
        return false;
    }
    let e = String::from_utf8_lossy(&out.stderr);
    e.contains("index.lock") || e.contains("config.lock") || e.contains("could not lock config")
}

/// Like `run`, but with an explicit per-call timeout — the sync path uses a SHORT
/// one so a stalled fetch/push (e.g. a rate-limited credential helper) is killed in
/// seconds, not the 60s default; keeps total sync wall-time bounded under contention.
///
/// (Hardening B) Retries a transient `.git/index.lock` collision with bounded, jittered
/// backoff. confer's own index writes are serialized by the clone flock, but an EXTERNAL
/// git process (an IDE, a `git gc`, a human's manual commit) can still grab index.lock —
/// so instead of hard-failing, we wait it out for a few seconds, then surface the real
/// error. A read-only op never creates index.lock, so this only ever affects writers.
fn run_to(root: &Path, args: &[&str], timeout: Duration) -> Result<Output> {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let out = run_once(root, args, timeout)?;
        if !is_lock_contention(&out) || std::time::Instant::now() >= deadline {
            return Ok(out);
        }
        std::thread::sleep(Duration::from_millis(40 + jitter_ms(80)));
    }
}

/// One git invocation (no index.lock retry). Split from `run_to` so the retry wrapper
/// stays readable.
fn run_once(root: &Path, args: &[&str], timeout: Duration) -> Result<Output> {
    let mut cmd = base(root);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let child = cmd.spawn().map_err(|e| anyhow!("spawn git: {e}"))?;
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout) {
        Ok(r) => Ok(r?),
        Err(_) => {
            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
            Err(anyhow!("git {} timed out after {}s (killed)", args.join(" "), timeout.as_secs()))
        }
    }
}

/// Run git, returning the raw Output (does NOT error on non-zero exit).
pub fn output(root: &Path, args: &[&str]) -> Result<Output> {
    run(root, args)
}

/// Like `output`, but feeds `input` to git's stdin — for plumbing that reads
/// stdin (`hash-object --stdin`, `mktree`). Writes stdin on a helper thread so a
/// large record can't deadlock against a full stdout pipe; same timeout as `run`.
pub fn output_stdin(root: &Path, args: &[&str], input: &str) -> Result<Output> {
    use std::io::Write;
    let mut cmd = base(root);
    cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| anyhow!("spawn git: {e}"))?;
    let mut stdin = child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?;
    let inp = input.to_string();
    std::thread::spawn(move || {
        let _ = stdin.write_all(inp.as_bytes());
    });
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(git_timeout()) {
        Ok(r) => Ok(r?),
        Err(_) => {
            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
            Err(anyhow!("git {} timed out after {}s (killed)", args.join(" "), git_timeout().as_secs()))
        }
    }
}

/// Run git, erroring on non-zero exit (for operations that must succeed).
pub fn check(root: &Path, args: &[&str]) -> Result<()> {
    let o = run(root, args)?;
    if o.status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&o.stderr).trim()
        ))
    }
}

/// Holds an exclusive `flock(2)` on `.confer/gitlock`; releasing = dropping the
/// file handle. flock is the right primitive here: the OS auto-releases it when the
/// holder exits (even on SIGKILL) — no stale-file heuristics, and no create-then-
/// write TOCTOU where a concurrent process reads a half-initialized lock, judges it
/// dead, and reclaims a LIVE holder's lock (the `.git/index.lock` collision found
/// by a review probe under concurrent same-clone appends).
pub struct Lock {
    _file: std::fs::File, // held open to hold the flock; dropping releases it
}

/// A tiny non-crypto jitter (sub-second nanos) to de-synchronize retry backoffs so
/// concurrent contenders don't lockstep. No rng dependency.
fn jitter_ms(cap: u64) -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.subsec_nanos() as u64) % cap.max(1))
        .unwrap_or(0)
}

fn acquire_lock(root: &Path) -> Result<Lock> {
    use fs2::FileExt;
    use std::io::{Seek, Write};
    let p = root.join(".confer").join("gitlock");
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // A persistent lock file (never removed) — flock is on the open handle, so a
    // leftover empty file is harmless and reused.
    let mut file = std::fs::OpenOptions::new().create(true).read(true).write(true).open(&p)?;
    // Poll for the lock with a budget so a genuinely wedged-but-alive holder can't
    // block us forever. Holds are bounded (the sync retry below is capped), so this
    // rarely waits. flock frees automatically if the holder dies.
    // Bounded wait: a genuinely wedged-but-alive holder can't block us forever. Default 30s;
    // `CONFER_LOCK_BUDGET_SECS` tunes it (tests shorten it to exercise the busy path fast).
    let budget = std::env::var("CONFER_LOCK_BUDGET_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(30);
    let deadline = std::time::Instant::now() + Duration::from_secs(budget);
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => {
                // Record the holder pid (advisory — for humans inspecting a busy clone).
                let _ = file.set_len(0);
                let _ = file.seek(std::io::SeekFrom::Start(0));
                let _ = writeln!(file, "{}", std::process::id());
                return Ok(Lock { _file: file });
            }
            Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(30 + jitter_ms(70)));
            }
            Err(_) => {
                return Err(anyhow!(
                    "clone busy — another confer op has held {} for >30s",
                    p.display()
                ));
            }
        }
    }
}

/// Acquire the per-clone git lock (flock on `.confer/gitlock`). Hold the returned guard
/// across a raw `add`+`commit`(+`push`) so those ops serialize against `commit_and_sync`
/// and the watch's `integrate` — closing the confer-races-itself index.lock gap
/// (Hardening A). Dropping the guard releases the lock.
pub fn lock(root: &Path) -> Result<Lock> {
    acquire_lock(root)
}

fn rev_count(root: &Path, range: &str) -> usize {
    output(root, &["rev-list", "--count", range])
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Current HEAD commit sha (the local tip; == hub tip after integrate).
pub fn head(root: &Path) -> Result<String> {
    let o = output(root, &["rev-parse", "HEAD"])?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        Err(anyhow!("no HEAD commit yet"))
    }
}

/// The best-common-ancestor of `a` and `b`, or None if either is unknown / they
/// share no history.
fn merge_base(root: &Path, a: &str, b: &str) -> Option<String> {
    let o = output(root, &["merge-base", a, b]).ok()?;
    if !o.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// The commit to store as the read cursor: `merge-base(@{u}, HEAD)` — the latest
/// PUSHED commit that is an ancestor of HEAD. It is always stable (pushed commits
/// aren't rebased, so the cursor can never be orphaned by a rebase and later
/// trigger a full-history re-emit) AND never ahead of HEAD (so it can't skip an
/// un-integrated upstream commit — the failure mode of anchoring on `@{u}` alone
/// when a rebase aborts). We still *read* `cursor..HEAD`, so nothing in HEAD is
/// missed; the cursor merely re-scans our own un-pushed commits until they land.
/// Falls back to local HEAD when there is no upstream (a fresh/local hub). R3.
pub fn cursor_anchor(root: &Path) -> Option<String> {
    merge_base(root, "@{u}", "HEAD").or_else(|| head(root).ok())
}

/// Message files ADDED since `since` (exclusive), in git commit order. `None` →
/// full history. Falls back to full history if `since` isn't a known ancestor
/// (e.g. a cursor from before history was reset). This is the incremental,
/// skew-proof, O(new) read that replaces re-parsing the whole store each cycle.
pub fn added_message_files(root: &Path, since: Option<&str>) -> Result<Vec<PathBuf>> {
    let collect = |range: &str| -> Option<Vec<PathBuf>> {
        let o = output(
            root,
            &[
                "log",
                "--reverse",
                "--diff-filter=A",
                "--name-only",
                "--format=",
                range,
                "--",
                "threads",
            ],
        )
        .ok()?;
        if !o.status.success() {
            return None;
        }
        let mut seen = std::collections::HashSet::new();
        let mut files = Vec::new();
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            let line = line.trim();
            if line.starts_with("threads/") && line.ends_with(".md") && seen.insert(line.to_string())
            {
                files.push(root.join(line));
            }
        }
        Some(files)
    };
    if let Some(c) = since {
        let short = &c[..c.len().min(10)];
        if is_ancestor(root, c, "HEAD") {
            // Normal path: cursor is an ancestor of HEAD → the incremental range.
            if let Some(files) = collect(&format!("{c}..HEAD")) {
                return Ok(files);
            }
        } else if let Some(base) = merge_base(root, c, "HEAD") {
            // Cursor is a real commit but diverged from HEAD (a force-push or
            // re-clone rewrote history) — reading `c..HEAD` would replay the whole
            // divergent history. Bound it to the merge-base tail instead, loudly.
            eprintln!("confer: cursor {short} diverged from HEAD (force-push/re-clone?); re-reading from its merge-base");
            return Ok(collect(&format!("{base}..HEAD")).unwrap_or_default());
        }
        // Cursor is entirely unknown (GC'd / foreign sha) — replay full history,
        // but never SILENTLY: a firehose must be diagnosable (G1).
        eprintln!("confer: cursor {short} is unknown (history reset/pruned); re-reading full store");
    }
    Ok(collect("HEAD").unwrap_or_default())
}

/// Is `a` an ancestor of `b`? (false if `a` is unknown / unrelated.)
fn is_ancestor(root: &Path, a: &str, b: &str) -> bool {
    output(root, &["merge-base", "--is-ancestor", a, b])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn has_upstream(root: &Path) -> bool {
    output(
        root,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .map(|o| o.status.success())
    .unwrap_or(false)
}

/// Commit `file` authored as `role`, then sync — all under the clone lock.
/// `sign`: when false, force `commit.gpgsign=false` so a user's global signing
/// config (esp. 1Password's ssh signer) can't break the commit; when true, the
/// clone's own git config (written by `join --signing-key`) signs with the agent's
/// key — including re-signing on rebase via `rebase.gpgSign`. See DESIGN.md.
/// Whether a message landed — BOTH variants mean it is DURABLY committed locally. A caller
/// may report "sent" for either; only an `Err` from `commit_and_sync` means NOT committed.
pub enum Committed {
    /// Committed + pushed to the hub.
    Synced,
    /// Committed locally; the push deferred (offline/contended), flushes on the next sync.
    DeferredLocal,
}

/// Write path: commit `file` and get it to the hub. Returns `Ok` iff the message is
/// durably committed locally (`Synced` = also pushed; `DeferredLocal` = push deferred but
/// the message is safe). An `Err` means the commit did NOT happen — the caller must treat
/// it as "did not send", not "committed but unsynced" (studio bug B: the two were conflated,
/// so a lock-failure looked like a successful unsynced send while the message vanished).
pub fn commit_and_sync(root: &Path, role: &str, file: &Path, msg: &str, sign: bool) -> Result<Committed> {
    // Fetch OUTSIDE the lock — it's the slow, read-only part (updates only refs/remotes/*).
    // Keeping it out means a peer's watcher poll fetching the same clone can't starve this
    // write (studio bug A). Everything under the lock is fast-local + a bounded push.
    let fetched = fetch_unlocked(root);
    let _lock = acquire_lock(root)?; // Err here → NOTHING committed
    let file_s = file.to_string_lossy().to_string();
    check(root, &["add", file_s.as_str()])?;
    let name_cfg = format!("user.name={role}");
    let email_cfg = format!("user.email={role}@confer.local");
    let mut args: Vec<&str> = vec!["-c", name_cfg.as_str(), "-c", email_cfg.as_str()];
    if !sign {
        args.extend(["-c", "commit.gpgsign=false"]);
    }
    args.extend(["commit", "-q", "-m", msg]);
    check(root, &args)?;
    // From here the message is durably committed locally — a push failure only DEFERS it.
    match reconcile_push(root, fetched) {
        Ok(_) => Ok(Committed::Synced),
        Err(_) => Ok(Committed::DeferredLocal),
    }
}

/// Fetch (outside the lock), then reconcile with upstream and push — under the lock.
pub fn integrate(root: &Path) -> Result<SyncResult> {
    let fetched = fetch_unlocked(root);
    let _lock = acquire_lock(root)?;
    reconcile_push(root, fetched)
}

/// Fetch remote-tracking refs. READ-ONLY (updates only `refs/remotes/*`, never HEAD/index),
/// so it deliberately does NOT hold the clone lock — the whole point of keeping the slow
/// network fetch out of the write path. Bounded to the short sync timeout; offline → false
/// (non-fatal: the local commit still lands and the push defers).
fn fetch_unlocked(root: &Path) -> bool {
    // Retry once on failure/timeout: under heavy concurrent git load a single fetch can exceed the
    // timeout, and a swallowed fetch means a READ folds STALE (misses a peer just-pushed event).
    // A jittered retry rides out a transient contention spike; SyncResult still reports
    // `fetched: false` if BOTH attempts fail, so a genuinely-offline read surfaces as stale.
    for attempt in 0..2 {
        if run_to(root, &["fetch", "--quiet"], Duration::from_secs(15))
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return true;
        }
        if attempt == 0 {
            std::thread::sleep(Duration::from_millis(150 + jitter_ms(250)));
        }
    }
    false
}

#[allow(dead_code)] // returned for callers that want to report; not all do
pub struct SyncResult {
    pub fetched: bool,
    pub pushed: usize,
}

/// Reconcile with upstream → push, RETRYING a lost push race (a concurrent push between our
/// fetch and ours → non-fast-forward) with bounded, jittered backoff. Makes progress under
/// contention instead of failing on the first race — and never HANGS: after a capped
/// wall-clock budget it DEFERS (the commit stays local, flushes on the next confer command),
/// returning a clean "hub busy" message. The caller holds the clone lock and has already
/// fetched OUTSIDE it (`fetched` = whether that succeeded; offline → defer, don't error).
fn reconcile_push(root: &Path, fetched: bool) -> Result<SyncResult> {
    if !has_upstream(root) {
        // No remote/upstream yet (e.g. brand-new local repo in tests).
        return Ok(SyncResult { fetched, pushed: 0 });
    }
    // A short per-git-call timeout for the sync path — a stalled fetch/push under
    // contention (the 29-min hang the killer test found: a rate-limited credential
    // helper stalling every call to the 60s default) is killed in seconds instead.
    let sg = |args: &[&str]| run_to(root, args, Duration::from_secs(15));
    // Hard WALL-CLOCK budget: retry within this, then DEFER — a write can NEVER hang
    // for minutes regardless of contention (bounds total time, not just attempt count).
    // `CONFER_SYNC_BUDGET_SECS` overrides the 25s default (operators tune it; tests
    // shorten it to exercise the defer path fast).
    let budget = std::env::var("CONFER_SYNC_BUDGET_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .map_or(Duration::from_secs(25), Duration::from_secs);
    let started = std::time::Instant::now();
    let mut attempt = 0u32;
    let deferred = loop {
        // Reconcile to the latest upstream: fast-forward if we've added nothing,
        // else rebase our commits on top (clean by the unique-file-per-message
        // invariant; abort + surface on any real conflict).
        let behind = rev_count(root, "HEAD..@{u}");
        let ahead = rev_count(root, "@{u}..HEAD");
        if behind > 0 {
            if ahead == 0 {
                let m = sg(&["merge", "--ff-only", "--quiet", "@{u}"])?;
                if !m.status.success() {
                    return Err(anyhow!("ff-merge failed: {}", String::from_utf8_lossy(&m.stderr).trim()));
                }
            } else {
                let r = sg(&["rebase", "@{u}"])?;
                if !r.status.success() {
                    let _ = sg(&["rebase", "--abort"]);
                    return Err(anyhow!(
                        "rebase onto upstream failed (aborted; resolve manually): {}",
                        String::from_utf8_lossy(&r.stderr).trim()
                    ));
                }
            }
        }
        let ahead_now = rev_count(root, "@{u}..HEAD");
        if ahead_now == 0 {
            return Ok(SyncResult { fetched, pushed: 0 });
        }
        let p = sg(&["push", "--quiet"])?;
        if p.status.success() {
            return Ok(SyncResult { fetched, pushed: ahead_now });
        }
        // Push rejected (almost always a concurrent push won the race). Stop once the
        // wall-clock budget is spent — defer rather than spin or stack slow calls.
        let last = String::from_utf8_lossy(&p.stderr).trim().to_string();
        if started.elapsed() >= budget {
            break last;
        }
        let base = (100u64 << attempt.min(4)).min(1500); // 100,200,400,800,1500,… ms
        std::thread::sleep(Duration::from_millis(base + jitter_ms(150)));
        let _ = sg(&["fetch", "--quiet"]);
        attempt += 1;
    };
    Err(anyhow!(
        "hub busy — push contended for {}s ({} tries); committed locally, will sync on the next confer command ({deferred})",
        started.elapsed().as_secs(),
        attempt + 1
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// A fresh, unique temp directory for a throwaway git repo.
    fn tmp() -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let p = std::env::temp_dir().join(format!("confer-git-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// Run git in `root` with a deterministic, signing-off identity; assert success.
    fn git(root: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(root)
            .args([
                "-c", "user.name=t",
                "-c", "user.email=t@t.local",
                "-c", "commit.gpgsign=false",
                "-c", "init.defaultBranch=main",
            ])
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "git {args:?} failed in {}", root.display());
    }

    fn rev(root: &Path, spec: &str) -> String {
        let o = Command::new("git").arg("-C").arg(root).args(["rev-parse", spec]).output().unwrap();
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }

    /// Add threads/general/<name>.md and commit; return the new HEAD sha.
    fn commit(root: &Path, name: &str) -> String {
        let d = root.join("threads").join("general");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(format!("{name}.md")), format!("---\ntype: note\n---\n{name}")).unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", name]);
        rev(root, "HEAD")
    }

    fn clone_of(hub: &Path) -> PathBuf {
        let dst = tmp();
        let ok = Command::new("git")
            .args(["clone", "-q", &hub.to_string_lossy(), &dst.to_string_lossy()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "clone failed");
        dst
    }

    /// A bare hub with one commit (m0) and a work clone that tracks it (@{u} set).
    fn hub_and_clone() -> (PathBuf, PathBuf, String) {
        let hub = tmp();
        git(&hub, &["init", "--bare", "-q", "-b", "main"]);
        let seed = tmp();
        git(&seed, &["init", "-q", "-b", "main"]);
        let m0 = commit(&seed, "m0");
        git(&seed, &["remote", "add", "origin", &hub.to_string_lossy()]);
        git(&seed, &["push", "-q", "-u", "origin", "main"]);
        (hub.clone(), clone_of(&hub), m0)
    }

    fn names(files: &[PathBuf]) -> Vec<String> {
        let mut v: Vec<String> =
            files.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn transient_index_lock_is_retried_not_failed() {
        // Hardening B: an EXTERNAL holder places .git/index.lock; a `git add` would fail
        // instantly ("Unable to create index.lock"). With the retry, it waits out the lock
        // (released here after 250ms) and succeeds within the 5s budget.
        let r = tmp();
        git(&r, &["init", "-q", "-b", "main"]);
        std::fs::write(r.join("f.txt"), "hi").unwrap();
        let lock = r.join(".git").join("index.lock");
        std::fs::write(&lock, "").unwrap();
        let lock2 = lock.clone();
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            let _ = std::fs::remove_file(&lock2);
        });
        let res = check(&r, &["add", "f.txt"]);
        releaser.join().unwrap();
        assert!(res.is_ok(), "add should retry past a transient index.lock, got {res:?}");
    }

    #[test]
    fn cursor_anchor_no_upstream_falls_back_to_head() {
        let r = tmp();
        git(&r, &["init", "-q", "-b", "main"]);
        let h = commit(&r, "a");
        assert_eq!(cursor_anchor(&r), Some(h)); // no @{u} → local HEAD
    }

    #[test]
    fn cursor_anchor_synced_is_head() {
        let (_hub, work, _m0) = hub_and_clone();
        assert_eq!(cursor_anchor(&work), Some(rev(&work, "HEAD")));
    }

    #[test]
    fn cursor_anchor_diverged_is_merge_base_not_head_nor_upstream() {
        let (hub, work, m0) = hub_and_clone();
        // a peer pushes m1
        let other = clone_of(&hub);
        let m1 = commit(&other, "m1");
        git(&other, &["push", "-q", "origin", "main"]);
        // local diverges (c1), then fetches: HEAD=m0+c1, @{u}=m0+m1
        let c1 = commit(&work, "c1");
        git(&work, &["fetch", "-q", "origin"]);
        let anchor = cursor_anchor(&work).unwrap();
        assert_eq!(anchor, m0, "anchor must be the merge-base");
        assert_ne!(anchor, c1, "not local HEAD (would be orphaned by rebase)");
        assert_ne!(anchor, m1, "not the upstream tip (would skip the un-integrated peer commit)");
    }

    #[test]
    fn cursor_anchor_after_rebase_push_is_stable_head() {
        let (hub, work, _m0) = hub_and_clone();
        let other = clone_of(&hub);
        commit(&other, "m1");
        git(&other, &["push", "-q", "origin", "main"]);
        commit(&work, "c1");
        git(&work, &["fetch", "-q", "origin"]);
        git(&work, &["rebase", "-q", "origin/main"]);
        git(&work, &["push", "-q", "origin", "main"]);
        let head = rev(&work, "HEAD");
        assert_eq!(cursor_anchor(&work), Some(head.clone())); // HEAD == @{u}
        assert_eq!(rev(&work, "@{u}"), head);
    }

    #[test]
    fn added_files_incremental_and_full() {
        let r = tmp();
        git(&r, &["init", "-q", "-b", "main"]);
        let a = commit(&r, "a");
        commit(&r, "b");
        assert_eq!(names(&added_message_files(&r, Some(&a)).unwrap()), vec!["b.md"]);
        assert_eq!(names(&added_message_files(&r, None).unwrap()), vec!["a.md", "b.md"]);
        // cursor == HEAD → nothing new (must not fall through to full re-read)
        assert!(added_message_files(&r, Some(&rev(&r, "HEAD"))).unwrap().is_empty());
    }

    #[test]
    fn added_files_unknown_cursor_falls_back_to_full_not_empty() {
        let r = tmp();
        git(&r, &["init", "-q", "-b", "main"]);
        commit(&r, "a");
        commit(&r, "b");
        let bogus = "0".repeat(40);
        assert_eq!(names(&added_message_files(&r, Some(&bogus)).unwrap()), vec!["a.md", "b.md"]);
    }

    #[test]
    fn added_files_diverged_cursor_uses_merge_base_not_full_nor_orphan() {
        // cursor points at a valid-but-not-ancestor commit (force-push shape):
        // base -> x  (x becomes the orphaned cursor); HEAD is base -> y -> z.
        let r = tmp();
        git(&r, &["init", "-q", "-b", "main"]);
        let base = commit(&r, "base");
        let x = commit(&r, "x");
        git(&r, &["reset", "--hard", "-q", &base]);
        commit(&r, "y");
        commit(&r, "z");
        let got = names(&added_message_files(&r, Some(&x)).unwrap());
        assert!(got.contains(&"y.md".into()) && got.contains(&"z.md".into()), "emits the divergent tail");
        assert!(!got.contains(&"base.md".into()), "merge-base excludes the shared base");
        assert!(!got.contains(&"x.md".into()), "the orphaned branch is not replayed");
    }

    #[test]
    fn gitlock_records_pid_persists_and_reacquires() {
        // Same-process flock exclusion is platform-flaky (two handles, one process);
        // the REAL exclusion — concurrent confer PROCESSES on one clone — is covered
        // by the cross-process cli test. Here we assert the deterministic invariants:
        // the lock records its holder, the file persists (flock is on the handle,
        // not the file's existence), and re-acquiring after release never wedges.
        let r = tmp();
        std::fs::create_dir_all(r.join(".confer")).unwrap();
        let p = r.join(".confer").join("gitlock");
        let lock = acquire_lock(&r).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap().trim(), std::process::id().to_string());
        drop(lock);
        assert!(p.exists(), "the lock file persists after release");
        let _relock = acquire_lock(&r).expect("re-acquire after release must not wedge");
    }
}
