//! Single-watcher lock per (hub, role) on a machine.
//!
//! Two `confer watch` processes for the same role on one machine share the same
//! cursor (`~/.confer/cursor/<hub>/<role>.json`), so a second watcher silently
//! steals events from the first. This happens most often when a Claude Code
//! session is compacted or ends but its background watch keeps running — an
//! orphan that races the new session's watch. The lock makes a live duplicate
//! refuse to start (protection), and lets a fresh watcher reclaim a dead/hung
//! lock automatically or `--replace` a live orphan (cleanup). It's machine-local
//! because the race is machine-local (the cursor lives under `$HOME`).

use crate::config;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

pub struct WatchLock {
    path: PathBuf,
    /// When this watcher started (stable across heartbeats) — for `watch-status`.
    started_at: String,
    /// How this watcher was armed — a self-declared stamp (e.g. `monitor`, `poll`, `background`) so
    /// `watch-status` can affirm the watcher actually DELIVERS wake events to the agent vs. just runs.
    /// A plain background watcher and a Monitor-hosted one look identical from the outside, but only
    /// the latter reaches the AI (Heliosphere field report / design/36). Portable BY DESIGN: whatever
    /// harness arms the watch passes its own method string — nothing here is Claude-Code-specific.
    delivery: Option<String>,
}

/// A snapshot of a watcher lock, for `watch-status` / healing decisions.
pub struct LockInfo {
    pub pid: u32,
    pub host: String,
    pub version: Option<String>,
    pub started_at: Option<String>,
    pub age_secs: u64,
    /// pid is alive AND on this host.
    pub alive: bool,
    pub same_host: bool,
    /// heartbeat is newer than the stale window.
    pub fresh: bool,
    /// Self-declared arming method (see [`WatchLock::delivery`]). `None` = not recorded — an older
    /// watcher, or one armed without the stamp (possibly a plain background process not delivering).
    pub delivery: Option<String>,
}

/// The health of a role's watcher — shared by `watch-status` and `session-heal`
/// so they can never disagree on what "healthy/stale/outdated" means.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum WatchState {
    Healthy,
    NotWatching,
    Stale,
    Outdated,
    OtherHost,
}

/// Classify a lock snapshot against the current binary version.
pub fn classify(info: &Option<LockInfo>, cur_version: &str) -> WatchState {
    match info {
        None => WatchState::NotWatching,
        Some(i) if !i.same_host => WatchState::OtherHost,
        Some(i) if !(i.alive && i.fresh) => WatchState::Stale,
        Some(i) if i.version.as_deref() != Some(cur_version) => WatchState::Outdated,
        Some(_) => WatchState::Healthy,
    }
}

/// Read + classify the lock for (hub, role) without touching it. `None` = no lock
/// (not watching). Powers `confer watch-status` and the session-start self-heal.
pub fn inspect(hub: &str, role: &str, stale_secs: u64) -> Option<LockInfo> {
    let path = lock_path(hub, role).ok()?;
    if !path.exists() {
        return None;
    }
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
    let pid = v.get("pid")?.as_u64()? as u32;
    let host = v.get("host").and_then(|x| x.as_str()).unwrap_or_default().to_string();
    let version = v.get("version").and_then(|x| x.as_str()).map(String::from);
    let started_at = v.get("started_at").and_then(|x| x.as_str()).map(String::from);
    let delivery = v.get("delivery").and_then(|x| x.as_str()).map(String::from);
    let same_host = host == config::hostname().unwrap_or_default();
    let age = age_secs(&path);
    Some(LockInfo {
        alive: same_host && process_alive(pid),
        same_host,
        fresh: age < stale_secs,
        age_secs: age,
        pid,
        host,
        version,
        started_at,
        delivery,
    })
}

fn lock_path(hub: &str, role: &str) -> Result<PathBuf> {
    let role = if role.is_empty() { "_all" } else { role };
    Ok(config::home()?
        .join(".confer")
        .join("watch")
        .join(hub)
        .join(format!("{role}.json")))
}

/// Same-host liveness via `kill -0` (signal 0 probes without delivering).
fn process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        // Silence "kill: (pid): No such process" on a dead pid — a liveness probe read by
        // fleet/where/adopt-clone must not leak stderr for a stale watch pid (a fleet finding).
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn read_info(path: &Path) -> Option<(u32, String)> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    Some((v.get("pid")?.as_u64()? as u32, v.get("host")?.as_str()?.to_string()))
}

/// True iff the lock file still records THIS process on THIS host — i.e. a `--replace` successor
/// hasn't taken it over. Gates `Drop` + `heartbeat` so a departing watcher never deletes or clobbers
/// the new holder's lock (H1). A missing file counts as "not ours" — a successor's acquire may be
/// mid-flight, so we neither resurrect nor remove it.
fn we_still_hold(path: &Path) -> bool {
    match read_info(path) {
        Some((pid, host)) => pid == std::process::id() && host == config::hostname().unwrap_or_default(),
        None => false,
    }
}

/// Seconds since the lock file was last refreshed (its heartbeat).
fn age_secs(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
        .unwrap_or(u64::MAX)
}

impl WatchLock {
    /// Acquire the (hub, role) watch lock. If a *live, fresh* watcher already
    /// holds it, refuse — unless `replace`, which kills it and takes over. A
    /// dead / hung / other-host lock is reclaimed silently (auto-cleanup).
    pub fn acquire(
        hub: &str,
        role: &str,
        stale_secs: u64,
        replace: bool,
        delivery: Option<String>,
    ) -> Result<Self> {
        let path = lock_path(hub, role)?;
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        // Serialize the acquire critical section (check → kill old → write our stamp) against a
        // concurrent acquirer of the SAME (hub, role) — otherwise two `--replace`s could both read
        // the old lock and both write their own pid (TOCTOU: two watchers each think they won). The
        // flock is held ONLY for acquire (dropped when this fn returns), not for the watch lifetime;
        // the pid stamp remains the actual holder record (M4). Best-effort: a wedged holder yields
        // None after a bounded wait and we proceed (degrading to the pre-M4 behavior, not hanging).
        let _acq = config::state_lock(&path.with_extension("acquire"));
        let label = if role.is_empty() { "<all>" } else { role };
        if path.exists() {
            let this_host = config::hostname().unwrap_or_default();
            let info = read_info(&path);
            let same_host = info.as_ref().is_some_and(|(_, h)| *h == this_host);
            let pid = info.as_ref().map(|(p, _)| *p);
            let alive = same_host && pid.is_some_and(process_alive);
            let fresh = age_secs(&path) < stale_secs;
            if alive && fresh {
                if !replace {
                    return Err(anyhow!(
                        "another confer watch for role '{label}' is already running on {this_host} \
                         (pid {}). It owns the cursor — a second watcher would race it and silently \
                         drop events. Stop it, or run `confer watch --replace` to take over.",
                        pid.unwrap_or(0)
                    ));
                }
                if let Some(p) = pid {
                    let _ = std::process::Command::new("kill").arg(p.to_string())
                        .stderr(std::process::Stdio::null()).status();
                    // Wait for the old watcher to actually EXIT before we write our lock — a still-
                    // running old watcher's final heartbeat or clean-exit Drop would otherwise
                    // clobber/delete the lock we're about to write (H1). Escalate to SIGKILL if it
                    // lingers past the grace window, then proceed regardless.
                    let mut waited_ms = 0u64;
                    while process_alive(p) && waited_ms < 1500 {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        waited_ms += 50;
                    }
                    if process_alive(p) {
                        let _ = std::process::Command::new("kill").args(["-9", &p.to_string()])
                            .stderr(std::process::Stdio::null()).status();
                        let mut w = 0u64;
                        while process_alive(p) && w < 500 {
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            w += 50;
                        }
                    }
                    eprintln!("confer watch: --replace killed the existing watcher (pid {p}) for role '{label}'.");
                }
            } else {
                eprintln!(
                    "confer watch: reclaimed a stale watch lock for role '{label}' \
                     (previous watcher not running or unresponsive)."
                );
            }
        }
        let lock = WatchLock {
            path,
            started_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            delivery,
        };
        lock.write()?;
        Ok(lock)
    }

    fn write(&self) -> Result<()> {
        // version + started_at let `watch-status` tell an agent "your watcher is on
        // an OLD confer — replace it" (the studio-didn't-adopt-the-fix case). We store
        // the build SHA (not CARGO_PKG_VERSION, which never changes between rebuilds)
        // so a stale watcher is actually detectable at build granularity.
        let info = serde_json::json!({
            "pid": std::process::id(),
            "host": config::hostname().unwrap_or_default(),
            "version": env!("CONFER_GIT_SHA"),
            "started_at": self.started_at,
            "delivery": self.delivery,
        });
        std::fs::write(&self.path, serde_json::to_string_pretty(&info)?)?;
        Ok(())
    }

    /// Refresh the lock's mtime so a later watcher can tell we're alive. Call each cycle.
    pub fn heartbeat(&self) {
        // Don't resurrect our stamp over a successor's lock: if a `--replace` has taken over (the
        // file now holds a foreign pid, or is gone mid-takeover), stop refreshing — we're being torn
        // down and would otherwise clobber the new holder (H1).
        if !we_still_hold(&self.path) {
            return;
        }
        let _ = self.write();
    }
}

impl Drop for WatchLock {
    fn drop(&mut self) {
        // Unlink ONLY if we still hold it. A `--replace` successor may have written its own pid to
        // this path; deleting that would leave the new, live watcher unlocked — the exact dual-watcher
        // race the lock exists to prevent (H1). A hard-killed watcher never runs Drop at all, so this
        // specifically guards the clean-exit-races-a-replace case. Otherwise: normal cleanup.
        if we_still_hold(&self.path) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(path: &Path, pid: u32, host: &str) {
        std::fs::write(path, serde_json::json!({ "pid": pid, "host": host }).to_string()).unwrap();
    }

    /// H1 (grok B0V3DA): a departing watcher must never delete or overwrite a `--replace` successor's
    /// lock. With a foreign pid in the file, both `heartbeat` and `Drop` are no-ops; with OUR pid,
    /// `Drop` cleans up as before.
    #[test]
    fn drop_and_heartbeat_respect_lock_ownership() {
        let host = config::hostname().unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("confer-h1-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("role.json");

        // A successor (different pid) now holds the lock.
        let successor = std::process::id().wrapping_add(1);
        stamp(&path, successor, &host);
        {
            let lk = WatchLock { path: path.clone(), started_at: "t".into(), delivery: None };
            lk.heartbeat(); // foreign pid present → must NOT clobber
            assert_eq!(read_info(&path).unwrap().0, successor, "heartbeat must not overwrite a successor's lock");
        } // Drop runs here: foreign pid present → must NOT remove
        assert!(path.exists(), "Drop must not delete a successor's lock");
        assert_eq!(read_info(&path).unwrap().0, successor, "successor's lock survives our Drop intact");

        // Now WE own it → Drop cleans up normally.
        stamp(&path, std::process::id(), &host);
        {
            let _lk = WatchLock { path: path.clone(), started_at: "t".into(), delivery: None };
        }
        assert!(!path.exists(), "Drop removes our OWN lock on clean exit");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
