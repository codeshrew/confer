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
    let file = std::fs::OpenOptions::new().create(true).read(true).write(true).open(lock_path).ok()?;
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
