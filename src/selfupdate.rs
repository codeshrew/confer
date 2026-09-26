//! Re-exec a long-lived confer process when the binary it was started as is replaced.
//!
//! `brew upgrade` and `cargo install` rewrite the path the process was launched from. A plugin
//! reader (0.8.37) already becomes that new build in place. A detached watcher is the other
//! process that outlives the session — on Grok it is reparented to pid 1 — so an upgrade otherwise
//! leaves it on the old build until the next `confer arm`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

pub(crate) type Fingerprint = (PathBuf, Option<SystemTime>, u64);

/// The binary this process was started as, by path and unresolved. An upgrade replaces the file
/// behind that path (brew relinks it; `cargo install` rewrites it), and that is what we watch.
pub(crate) fn launched_as() -> Option<PathBuf> {
    let a0 = PathBuf::from(std::env::args_os().next()?);
    if a0.components().count() > 1 {
        return Some(a0);
    }
    std::env::var_os("PATH")
        .and_then(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join(&a0))
                .find(|c| c.is_file())
        })
        .or_else(|| std::env::current_exe().ok())
}

pub(crate) fn fingerprint(exe: &Path) -> Option<Fingerprint> {
    let real = exe.canonicalize().ok()?;
    let m = std::fs::metadata(&real).ok()?;
    Some((real, m.modified().ok(), m.len()))
}

pub(crate) enum Installed {
    /// A different confer build whose `--help` contains `marker`.
    Upgrade(String),
    /// Our own build, or one that cannot do this job: nothing to do until the file changes again.
    Stay,
    /// It would not run (mid-install, say): look again next time.
    Unknown,
}

/// `probe` is a help invocation whose stdout must contain `marker`, so we never exec a confer too
/// old for this job (`--plugin` for the reader, `--delivery` for a spool watcher).
pub(crate) fn installed(exe: &Path, probe: &[&str], marker: &str) -> Installed {
    let Ok(v) = Command::new(exe).arg("--version").output() else {
        return Installed::Unknown;
    };
    let v = String::from_utf8_lossy(&v.stdout).trim().to_string();
    if !v.starts_with("confer ") {
        return Installed::Unknown;
    }
    if v.contains(crate::BUILD_SHA) {
        return Installed::Stay;
    }
    match Command::new(exe).args(probe).output() {
        Ok(h) if String::from_utf8_lossy(&h.stdout).contains(marker) => Installed::Upgrade(v),
        Ok(_) => Installed::Stay,
        Err(_) => Installed::Unknown,
    }
}

pub(crate) fn interval(env_key: &str) -> Duration {
    Duration::from_secs(
        std::env::var(env_key)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30),
    )
}

/// Replace this process with `exe`, keeping the pid, the fds, and the original arguments.
/// Returns only when exec fails.
pub(crate) fn exec_replacement(exe: &Path, env: &[(&str, &str)]) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    let mut cmd = Command::new(exe);
    cmd.args(std::env::args_os().skip(1));
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.exec()
}

const REEXEC: &str = "CONFER_WATCH_REEXEC";
pub(crate) const WATCH_UPGRADED_FROM: &str = "CONFER_WATCH_UPGRADED_FROM";

/// True when this process is the image just exec'd over a live watcher. The lock file still names
/// our pid; acquire must adopt it instead of signalling itself.
pub(crate) fn adopting_own_lock(pid: u32) -> bool {
    pid == std::process::id() && std::env::var(REEXEC).ok().as_deref() == Some("1")
}

/// Spool watchers only. An inline watcher dies with its monitor, and the next arm is a new binary.
pub(crate) struct WatchUpgrade {
    exe: Option<PathBuf>,
    fp: Option<Fingerprint>,
    last: Instant,
    every: Duration,
}

impl WatchUpgrade {
    pub(crate) fn new(spooled: bool) -> Self {
        if let Ok(from) = std::env::var(WATCH_UPGRADED_FROM) {
            if !from.is_empty() {
                println!(
                    "confer watch: upgraded from {from} to confer {}",
                    crate::VERSION
                );
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
        }
        let exe = spooled.then(launched_as).flatten();
        let fp = exe.as_deref().and_then(fingerprint);
        Self {
            exe,
            fp,
            last: Instant::now(),
            every: interval("CONFER_WATCH_UPGRADE_SECS"),
        }
    }

    pub(crate) fn poll(&mut self) {
        let Some(exe) = self.exe.clone() else { return };
        if self.last.elapsed() < self.every {
            return;
        }
        self.last = Instant::now();
        let Some(fp) = fingerprint(&exe) else { return };
        if self.fp.as_ref() == Some(&fp) {
            return;
        }
        match installed(&exe, &["watch", "--help"], "--delivery") {
            Installed::Upgrade(v) => {
                let from = format!("confer {}", crate::VERSION);
                let err = exec_replacement(&exe, &[(REEXEC, "1"), (WATCH_UPGRADED_FROM, &from)]);
                eprintln!(
                    "confer watch: could not re-exec {} ({v}): {err}",
                    exe.display()
                );
                self.fp = Some(fp);
            }
            Installed::Stay => self.fp = Some(fp),
            Installed::Unknown => {}
        }
    }
}
