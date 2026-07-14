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
    pub fn acquire(hub: &str, role: &str, stale_secs: u64, replace: bool) -> Result<Self> {
        let path = lock_path(hub, role)?;
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
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
        });
        std::fs::write(&self.path, serde_json::to_string_pretty(&info)?)?;
        Ok(())
    }

    /// Refresh the lock's mtime so a later watcher can tell we're alive. Call each cycle.
    pub fn heartbeat(&self) {
        let _ = self.write();
    }
}

impl Drop for WatchLock {
    fn drop(&mut self) {
        // Clean exit removes the lock; a hard kill leaves it, and the next
        // watcher reclaims it as stale (dead pid / old heartbeat).
        let _ = std::fs::remove_file(&self.path);
    }
}
