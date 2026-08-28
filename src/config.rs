//! Hub/role resolution and path helpers.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The coordination hub clone root. `$CONFER_HUB` (if set) points hub-operating
/// commands at the hub from anywhere — so an agent living in its own code repo
/// doesn't have to `cd` into the hub first; otherwise use the enclosing repo
/// (`git rev-parse --show-toplevel`).
pub fn repo_root() -> Result<PathBuf> {
    if let Ok(h) = std::env::var("CONFER_HUB") {
        if !h.is_empty() {
            let p = PathBuf::from(&h).canonicalize().map_err(|_| {
                anyhow!("$CONFER_HUB points at '{h}', which does not exist")
            })?;
            // Validate the SAME way the cwd branch does — a stale $CONFER_HUB pointing at a
            // directory whose `.git` is gone (a clone that was moved/deleted) must fail HERE, on the
            // first syscall, NOT after a command has written a message file into it and then can't
            // commit — which orphans that file in a non-repo dir (Pipeline bug #1).
            ensure_git_repo(&p).map_err(|e| anyhow!("$CONFER_HUB '{}' {e}", p.display()))?;
            ensure_confer_hub(&p)?;
            return Ok(p);
        }
    }
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;
    if !out.status.success() {
        return Err(anyhow!(
            "not inside a git repository — cd into the hub clone or set $CONFER_HUB to its path"
        ));
    }
    let root = PathBuf::from(String::from_utf8(out.stdout)?.trim());
    ensure_confer_hub(&root)?;
    Ok(root)
}

/// Is `p` inside a git work tree? A hub clone whose `.git` was removed (moved/deleted clone) fails
/// this — the check that stops a write from landing in a non-repo dir (Pipeline bug #1).
fn ensure_git_repo(p: &Path) -> Result<()> {
    let inside = Command::new("git")
        .args(["-C", &p.to_string_lossy(), "rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !inside {
        return Err(anyhow!(
            "is not a git repository — a hub clone's .git is missing (the clone was moved or deleted). \
             Re-clone with `confer reconnect --hub <hub>`, or point $CONFER_HUB at a valid clone."
        ));
    }
    Ok(())
}

/// The git repo at `root` must actually be a confer hub. Without this, running confer from a NON-hub
/// repo (a product repo you happen to be in) silently treats THAT repo as the hub — the split-brain
/// footgun. A real hub (scaffolded by `clone`/`init`) always has threads/ or roles/.
fn ensure_confer_hub(root: &Path) -> Result<()> {
    if !root.join("threads").is_dir() && !root.join("roles").is_dir() {
        return Err(anyhow!(
            "the git repo ({}) is not a confer hub (no threads/ or roles/). \
             cd into your hub clone, set $CONFER_HUB=<hub-path>, or run `confer reconnect --hub <hub>`.",
            root.display()
        ));
    }
    Ok(())
}

/// Resolve the role: explicit flag → .confer/identity.json → $CONFER_ROLE.
pub fn resolve_role(explicit: Option<String>, root: &Path) -> Result<String> {
    if let Some(r) = explicit {
        if !r.is_empty() {
            return Ok(r);
        }
    }
    let id = root.join(".confer").join("identity.json");
    if let Ok(txt) = std::fs::read_to_string(&id) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
            if let Some(r) = v.get("role").and_then(|x| x.as_str()) {
                return Ok(r.to_string());
            }
        }
    }
    if let Ok(r) = std::env::var("CONFER_ROLE") {
        if !r.is_empty() {
            return Ok(r);
        }
    }
    Err(anyhow!(
        "no role resolved: pass --role/--from, run `confer join --role <role>`, or set CONFER_ROLE"
    ))
}

/// A stable, topology-proof key identifying this hub: the root-commit SHA
/// (survives remote-URL changes and URL-form differences). Falls back to a
/// sanitized origin URL / repo path if there's no commit yet.
/// The hub's DECLARED identity, read from `.confer-hub-id` in the working tree.
///
/// `hub_key` derived this from `git rev-list --max-parents=0 HEAD`, and that derivation can fail
/// permanently: on a `blob:none` promisor clone an ancestor commit object can be absent outright
/// (`Not a valid object name`), so traversal to the root cannot complete no matter how it is asked —
/// bypassing the commit-graph does not help, because the graph was reporting the failure, not causing
/// it. When that happens the old code silently answered with a URL-derived key instead, which forks
/// the whole per-hub identity namespace: watch lock, delivery cursor, read frontier, watch
/// preferences, presence and trust state. Two watchers that cannot see each other, an `inbox` that
/// clears one namespace while the watcher counts the other.
///
/// A DECLARED id needs no traversal, so nothing about the local object database can break it. It is
/// a plain checked-out file: present or absent, and absent is unambiguous.
///
/// MIGRATION SAFETY: the declared value for an existing hub MUST be its root-commit sha — the same
/// string the derivation already produces. Declaring anything else would silently repoint every
/// cursor, frontier and preference on the fleet, which is the same damage as the bug. `confer hub
/// declare-id` computes it from a healthy clone for exactly that reason.
pub fn declared_hub_id(root: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(root.join(".confer-hub-id")).ok()?;
    let id = raw.split_whitespace().next()?.to_string();
    // Only accept what this function is for: a git object id. Anything else is a corrupted or
    // hand-edited file, and guessing at it would reintroduce the silent-fork failure by another door.
    (id.len() == 40 && id.chars().all(|c| c.is_ascii_hexdigit())).then_some(id)
}

pub fn hub_key(root: &Path) -> String {
    // Declared beats derived: a checked-out file cannot fail the way a traversal can.
    if let Some(id) = declared_hub_id(root) {
        return id;
    }
    if let Ok(o) = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-list", "--max-parents=0", "HEAD"])
        .output()
    {
        if o.status.success() {
            if let Some(sha) = String::from_utf8_lossy(&o.stdout).split_whitespace().next() {
                return sha.to_string();
            }
        }
    }
    // FALLING BACK IS THE BUG SURFACE, SO SAY SO. This key namespaces the watch lock, the delivery
    // cursor, the read frontier, watch preferences, presence and trust state. Silently answering with
    // a DIFFERENT key splits all of it: two watchers that cannot see each other, an `inbox` that
    // clears one namespace while the watcher counts the other, and a backlog whose count can never go
    // down — every command reporting success about the namespace it happened to touch.
    //
    // jarvis measured the same build on one box writing BOTH forms on the same day, so the choice
    // varies per invocation and nothing in the output ever said which was chosen. We do not yet know
    // what decides it (a promisor boundary where the root is unreachable is one confirmed cause; it is
    // not the only one, and concurrent git load reproduces nothing here). Until the key is declared
    // rather than derived, the least confer can do is refuse to fork identity in silence.
    let why = match Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-list", "--max-parents=0", "HEAD"])
        .output()
    {
        Err(e) => format!("could not run git ({e})"),
        Ok(o) if !o.status.success() => {
            let err = String::from_utf8_lossy(&o.stderr);
            format!("git failed: {}", err.lines().next().unwrap_or("(no stderr)").trim())
        }
        Ok(_) => "git succeeded but reported no root commit (a promisor boundary can hide it)".to_string(),
    };
    eprintln!(
        "confer: ⚠ cannot determine this hub's root-commit id ({why}) — falling back to a          URL-derived key for {}. That is a DIFFERENT identity namespace: this run's cursor, inbox,          watch lock and preferences will not be the ones a run that resolved the root sha uses.          Report this (it is a known split, cause under investigation).",
        root.display()
    );
    let raw = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| root.to_string_lossy().to_string());
    raw.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// The strict, PIN-GRADE hub root identity for the `known_hubs` trust store (design/35). Unlike
/// [`hub_key`] — which is lenient (first-line of `rev-list`, with a URL-string fallback) because it's
/// only KEYING clones/keyrings — this refuses to guess: a multi-root history is ambiguous (which root
/// is "the" identity depends on traversal order, not on any stable rule) → hard error; an empty repo
/// is a DISTINCT state, never a URL string masquerading as a SHA (pinning that fallback before the
/// first commit lands would permanently mismatch the real root — a self-inflicted DoS). Only a single
/// unambiguous root commit is pinnable.
pub enum HubRoot {
    /// The single root-commit SHA — the pinnable identity.
    Commit(String),
    /// The repo has no commits yet — NOT pinnable (pin only after the first commit exists).
    NoCommits,
}

pub fn hub_root_strict(root: &Path) -> Result<HubRoot> {
    let o = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-list", "--max-parents=0", "HEAD"])
        .output()?;
    if !o.status.success() {
        // Distinguish a genuinely empty repo (no HEAD / no commits) from a REAL git failure (poisoned
        // PATH, wedged worktree, disk pressure). Coalescing the latter into NoCommits masks the cause
        // and can later pin a fallback that permanently mismatches the real root (red-team). Only the
        // recognized no-HEAD signatures are NoCommits; anything else is a hard error.
        let err = String::from_utf8_lossy(&o.stderr);
        if err.contains("does not have any commits yet")
            || err.contains("unknown revision")
            || err.contains("ambiguous argument 'HEAD'")
        {
            return Ok(HubRoot::NoCommits);
        }
        return Err(anyhow!(
            "could not resolve the root commit of {} (git rev-list failed): {}",
            root.display(),
            err.trim()
        ));
    }
    let out = String::from_utf8_lossy(&o.stdout);
    let roots: Vec<&str> = out.split_whitespace().collect();
    match roots.as_slice() {
        [] => Ok(HubRoot::NoCommits),
        [sha] => Ok(HubRoot::Commit((*sha).to_string())),
        many => Err(anyhow!(
            "hub at {} has {} root commits — an ambiguous/multi-root history is not a stable identity \
             and cannot be pinned; investigate before trusting it (a legitimate hub has exactly one root)",
            root.display(),
            many.len()
        )),
    }
}

/// The agent's SSH signing key path recorded at join (`.confer/identity.json`),
/// if this clone is configured to sign commits. See DESIGN.md.
pub fn signing_key(root: &Path) -> Option<PathBuf> {
    let txt = std::fs::read_to_string(root.join(".confer").join("identity.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
    let p = v.get("signing_key")?.as_str()?;
    (!p.is_empty()).then(|| PathBuf::from(p))
}

/// The role this clone was JOINED as, read straight from its own `.confer/identity.json`.
///
/// Deliberately NOT `resolve_role`, which falls back to `$CONFER_ROLE`. Callers here are asking
/// "who owns this clone?", not "who does the caller want to act as" — and that question must never
/// be answered by an environment variable, or a stray `CONFER_ROLE` re-roles someone else's clone.
pub fn clone_role(root: &Path) -> Option<String> {
    let txt = std::fs::read_to_string(root.join(".confer").join("identity.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
    let r = v.get("role")?.as_str()?;
    (!r.is_empty()).then(|| r.to_string())
}

pub fn home() -> Result<PathBuf> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| anyhow!("$HOME not set"))
}

/// An exclusive advisory flock guard for serializing a read-modify-write of shared `~/.confer`
/// state (keyring pins, presence HWM) across concurrent confer processes — otherwise a lost
/// update can silently DROP a pin, and the next read TOFU-re-pins whatever the card presents, no
/// mismatch ever surfaced (a review finding). Best-effort: on a wedged holder it gives up after a
/// bounded wait and returns `None`, so a read path degrades rather than hangs. Dropping the guard
/// (the returned file handle) releases the lock.
pub fn state_lock(lock_path: &Path) -> Option<std::fs::File> {
    use fs2::FileExt;
    if let Some(parent) = lock_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = std::fs::OpenOptions::new().create(true).read(true).write(true).truncate(false).open(lock_path).ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Some(file),
            Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            Err(_) => return None,
        }
    }
}

/// A NON-BLOCKING exclusive flock over the machine-local `~/.confer/update.lock`. Co-resident agents
/// (many roles/sessions on one host) share ONE installed `confer` binary, so a concurrent
/// self-replace would have several processes swapping the same file at once. Unlike `state_lock`,
/// this does NOT wait: if another agent already holds it, we return `None` so the caller skips
/// cleanly ("someone else on this box is updating") instead of piling on. Dropping the returned
/// handle releases the lock; a crashed holder's flock is released by the OS on exit.
pub fn try_update_lock() -> Option<std::fs::File> {
    use fs2::FileExt;
    let path = home().ok()?.join(".confer").join("update.lock");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let file = std::fs::OpenOptions::new().create(true).read(true).write(true).truncate(false).open(&path).ok()?;
    match file.try_lock_exclusive() {
        Ok(()) => Some(file),
        Err(_) => None,
    }
}

/// Machine-local "tip" signal dir — a same-machine `append` touches a file here
/// so co-resident `watch`ers wake instantly (their `notify` watches this dir),
/// bounding local latency by push+fetch instead of the poll interval. Purely
/// local; remote agents never see it and fall back to the fetch-loop.
pub fn signal_dir() -> Result<PathBuf> {
    Ok(home()?.join(".confer").join("tips"))
}

/// Touch the signal for `hub_key` (fires a filesystem event for local watchers).
pub fn touch_signal(hub_key: &str) {
    if let Ok(dir) = signal_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(hub_key), b"1");
    }
}

/// Best-effort hostname for provenance.
pub fn hostname() -> Option<String> {
    std::env::var("HOSTNAME").ok().or_else(|| {
        Command::new("hostname")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    })
}
