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
            let p = PathBuf::from(&h);
            return p.canonicalize().map_err(|_| {
                anyhow!("$CONFER_HUB points at '{h}', which does not exist")
            });
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
    // Guard: the enclosing git repo must actually be a confer hub. Without this,
    // running confer from a NON-hub repo (a product repo you happen to be in)
    // silently treats THAT repo as the hub — creating .confer/ + threads/ + roles/
    // in the wrong place and operating there (the split-brain footgun). A real hub — scaffolded by
    // `clone`/`init` — always has threads/ or roles/. If neither is present, refuse
    // loudly instead of doing the wrong thing silently.
    if !root.join("threads").is_dir() && !root.join("roles").is_dir() {
        return Err(anyhow!(
            "the current git repo ({}) is not a confer hub (no threads/ or roles/). \
             cd into your hub clone, set $CONFER_HUB=<hub-path>, or run `confer reconnect --hub <hub>`.",
            root.display()
        ));
    }
    Ok(root)
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
pub fn hub_key(root: &Path) -> String {
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
