//! `confer attach --plugin` — the reader the confer Claude Code plugin runs as a plugin monitor.
//!
//! Claude Code caps every Monitor at 30 minutes, so an agent hosting `confer arm` under one is woken
//! every half hour just to re-arm it: a turn and a line of scrollback for nothing (design/56). A
//! plugin monitor is different. Claude Code starts it at session start, keeps it for the whole
//! session, and delivers its stdout as notifications the same way. Nothing expires, and nothing
//! needs arming. The detached watchers (0.8.33) already do the watching; this is only the reader.
//!
//! Three rules follow from how plugin monitors behave:
//!
//! - **It never exits on its own.** A plugin monitor that exits is not restarted for the rest of
//!   the session. So "nothing to read yet" means wait quietly, and an error means log it and try
//!   again, never return.
//! - **It is silent unless there is something to say.** Every stdout line wakes the agent. It
//!   prints once when it starts delivering a hub, and then only the wakes themselves.
//! - **It never takes a spool someone else is reading.** One reader per spool (the lease in
//!   `attach::write_marker`). A live reader from another session keeps its spool. A newer reader
//!   that takes one of ours gets it, and we pick it back up if that reader goes away.
//!
//! Which hubs to read: every watch target stamped with this session (`confer arm` stamps them),
//! plus the hubs this project used last time, so a fresh session or a `/clear` starts delivering
//! without anyone arming anything.
//!
//! **It upgrades itself.** A reader runs for the whole session, and the watchers it keeps alive run
//! its build. So after a `brew upgrade` the whole session would stay on the old build until someone
//! ran `/reload-plugins`. Instead the reader watches the binary it was started as, and when that
//! file becomes a different confer build that can run the reader, it execs it: same pid, same
//! stdout, so Claude Code never sees the monitor exit. The new build then replaces the watchers.

use crate::attach::{self, Target};
use crate::{autoheal, config, prune, spool};
use anyhow::Result;
use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

pub(crate) fn readers_dir() -> Option<PathBuf> {
    config::home().ok().map(|h| h.join(".confer").join("plugin").join("readers"))
}

fn memory_path() -> Option<PathBuf> {
    config::home().ok().map(|h| h.join(".confer").join("plugin").join("projects.json"))
}

fn reader_file(session: &str) -> Option<PathBuf> {
    let safe: String = session.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    readers_dir().map(|d| d.join(format!("{safe}.json")))
}

/// The pid of a live plugin-monitor reader for `session`, if there is one. `confer arm` asks this
/// before hosting anything: if the plugin is delivering, there is nothing to host.
pub fn live_reader_for_session(session: &str) -> Option<u32> {
    let txt = std::fs::read_to_string(reader_file(session)?).ok()?;
    let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
    let pid = v.get("pid")?.as_u64()? as u32;
    crate::watchlock::pid_is_live_confer(pid).then_some(pid)
}

/// The plugin reader's pid if, for the CURRENT session, it is live and (hub, role) is one of the
/// hubs it will read, so a missing watcher there is one it is about to start, not a fault.
pub fn starting(hub_key: &str, role: &str) -> Option<u32> {
    let session = autoheal::current_session()?;
    let pid = live_reader_for_session(&session)?;
    let project = std::fs::read_to_string(reader_file(&session)?)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("project").and_then(|p| p.as_str()).map(String::from));
    wanted(&Some(session), &project)
        .iter()
        .any(|t| t.hub_key == hub_key && t.role == role)
        .then_some(pid)
}

type Memory = BTreeMap<String, Vec<(String, String)>>;

fn load_memory() -> Memory {
    memory_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_memory(project: &str, hubs: Vec<(String, String)>) {
    let Some(p) = memory_path() else { return };
    let mut m = load_memory();
    if m.get(project) == Some(&hubs) {
        return;
    }
    m.insert(project.to_string(), hubs);
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(txt) = serde_json::to_string_pretty(&m) {
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, txt).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

/// The hubs this session should read: targets stamped with this session, plus this project's hubs
/// from last time. Only real hubs the role has joined.
fn wanted(session: &Option<String>, project: &Option<String>) -> Vec<Target> {
    let mut found: Vec<(PathBuf, String)> = Vec::new();
    if let Some(s) = session {
        for t in autoheal::load().targets {
            if t.session.as_deref() == Some(s.as_str()) {
                found.push((PathBuf::from(&t.hub), t.role));
            }
        }
    }
    if let Some(p) = project {
        if let Some(hubs) = load_memory().get(p) {
            found.extend(hubs.iter().map(|(h, r)| (PathBuf::from(h), r.clone())));
        }
    }
    found.retain(|(p, r)| prune::looks_like_hub(p) && attach::is_member(p, r));
    attach::finalize(found)
}

/// The binary this reader was started as, by path and unresolved: an upgrade replaces the file
/// behind that path (brew relinks it; `cargo install` rewrites it), and that is what we watch.
fn launched_as() -> Option<PathBuf> {
    let a0 = PathBuf::from(std::env::args_os().next()?);
    if a0.components().count() > 1 {
        return Some(a0);
    }
    std::env::var_os("PATH")
        .and_then(|p| std::env::split_paths(&p).map(|d| d.join(&a0)).find(|c| c.is_file()))
        .or_else(|| std::env::current_exe().ok())
}

type Fingerprint = (PathBuf, Option<SystemTime>, u64);

fn fingerprint(exe: &Path) -> Option<Fingerprint> {
    let real = exe.canonicalize().ok()?;
    let m = std::fs::metadata(&real).ok()?;
    Some((real, m.modified().ok(), m.len()))
}

enum Installed {
    /// A different confer build that can run the plugin reader: its `--version` line.
    Upgrade(String),
    /// Our own build, or one without `attach --plugin`: nothing to do until the file changes again.
    Stay,
    /// It would not run (mid-install, say): look again next time.
    Unknown,
}

fn installed(exe: &Path) -> Installed {
    let Ok(v) = Command::new(exe).arg("--version").output() else { return Installed::Unknown };
    let v = String::from_utf8_lossy(&v.stdout).trim().to_string();
    if !v.starts_with("confer ") {
        return Installed::Unknown;
    }
    if v.contains(crate::BUILD_SHA) {
        return Installed::Stay;
    }
    match Command::new(exe).args(["attach", "--help"]).output() {
        Ok(h) if String::from_utf8_lossy(&h.stdout).contains("--plugin") => Installed::Upgrade(v),
        Ok(_) => Installed::Stay,
        Err(_) => Installed::Unknown,
    }
}

const UPGRADED_FROM: &str = "CONFER_PLUGIN_UPGRADED_FROM";

/// Run as the plugin monitor. Returns only when the session ends (SIGTERM/SIGHUP) or stdout is
/// gone; everything else is waited out or logged to stderr.
pub fn run(project: Option<PathBuf>) -> Result<()> {
    let session = autoheal::current_session();
    let project = project
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.canonicalize().unwrap_or(p).to_string_lossy().to_string());
    // Readers that died without cleaning up (a crash, a reboot) leave files naming dead pids, and a
    // remedy keyed on them signals nothing (batcave-net). Clear them, and clear ours on the way out.
    crate::plugin_ctl::reap();
    let _own = session.as_deref().and_then(reader_file).map(|f| {
        if let Some(parent) = f.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
            &f,
            serde_json::json!({
                "pid": std::process::id(),
                "project": project,
                "session": session,
                "version": crate::VERSION,
            })
            .to_string(),
        );
        crate::plugin_ctl::OwnFile(f)
    });
    attach::install_stop_handlers();
    eprintln!(
        "confer attach --plugin: session {} project {} (pid {})",
        session.as_deref().unwrap_or("?"),
        project.as_deref().unwrap_or("?"),
        std::process::id()
    );

    let exe = launched_as();
    let mut exe_fp = exe.as_deref().and_then(fingerprint);
    let upgrade_every = Duration::from_secs(
        std::env::var("CONFER_PLUGIN_UPGRADE_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(30),
    );
    let mut last_upgrade_check = Instant::now();
    let upgraded_from = std::env::var(UPGRADED_FROM).ok().filter(|v| !v.is_empty());

    // (hub_key, role) → (label, root, tail)
    let mut tails: BTreeMap<(String, String), (String, PathBuf, spool::Tail)> = BTreeMap::new();
    let mut out = std::io::stdout().lock();
    let mut last_tick: Option<Instant> = None;
    let mut last_ensure = Instant::now();
    while !attach::stopping() {
        if last_tick.is_none_or(|t| t.elapsed() >= Duration::from_secs(5)) {
            last_tick = Some(Instant::now());

            // Yield any spool a newer reader has taken; keep our claim fresh on the rest.
            tails.retain(|_, (_, _, tail)| attach::other_reader(&tail.log).is_none());
            for (_, _, tail) in tails.values() {
                attach::write_marker(&tail.log, "plugin", session.as_deref());
            }

            // Pick up hubs we should be reading and are not.
            let mut started: Vec<String> = Vec::new();
            for t in wanted(&session, &project) {
                let key = (t.hub_key.clone(), t.role.clone());
                if tails.contains_key(&key) {
                    continue;
                }
                let Ok(log) = spool::log_path(&t.hub_key, &t.role) else { continue };
                if let Some((_, _, other)) = attach::other_reader(&log) {
                    if other != session {
                        continue; // someone else is reading this one right now
                    }
                }
                if let Err(e) = attach::ensure_watcher(&t, &[], false, false) {
                    eprintln!("confer attach --plugin: {} [{}]: {e:#}", t.label, t.role);
                    continue;
                }
                if let Some(parent) = log.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                attach::write_marker(&log, "plugin", session.as_deref());
                started.push(format!("{} [{}]", t.label, t.role));
                tails.insert(key, (t.label.clone(), t.root.clone(), spool::Tail::open(log)));
            }
            if !started.is_empty() {
                // The one line this prints on its own account: that delivery has started, and for
                // which hubs. It reaches the agent once, not every 30 minutes.
                let upgraded = match &upgraded_from {
                    Some(v) => format!("upgraded the plugin reader from {v} to confer {}; ", crate::VERSION),
                    None => String::new(),
                };
                if writeln!(
                    out,
                    "confer: {upgraded}delivering {} hub(s) through the confer plugin monitor, for the whole \
                     session (no Monitor to arm or re-arm) — {}",
                    tails.len(),
                    started.join(", ")
                )
                .and_then(|_| out.flush())
                .is_err()
                {
                    return Ok(()); // the session is gone
                }
                if let Some(p) = &project {
                    let hubs = tails
                        .iter()
                        .map(|((_, role), (_, root, _))| (root.to_string_lossy().to_string(), role.clone()))
                        .collect();
                    save_memory(p, hubs);
                }
            }
        }

        // Watchers can die between arms (a crash, a reboot, a kill). Restart any that did.
        if last_ensure.elapsed() >= Duration::from_secs(60) {
            last_ensure = Instant::now();
            let ts: Vec<Target> = tails
                .iter()
                .map(|((hub_key, role), (label, root, _))| Target {
                    root: root.clone(),
                    role: role.clone(),
                    hub_key: hub_key.clone(),
                    label: label.clone(),
                })
                .collect();
            for t in &ts {
                if let Err(e) = attach::ensure_watcher(t, &[], false, false) {
                    eprintln!("confer attach --plugin: {} [{}]: {e:#}", t.label, t.role);
                }
            }
        }

        // A new confer installed under us: become it. exec keeps the pid and stdout, so the plugin
        // monitor never exits, the leases (keyed by pid) stay ours, and the spool offsets are on disk.
        if last_upgrade_check.elapsed() >= upgrade_every {
            last_upgrade_check = Instant::now();
            if let Some(exe) = &exe {
                let fp = fingerprint(exe);
                if fp.is_some() && fp != exe_fp {
                    match installed(exe) {
                        Installed::Upgrade(v) => {
                            eprintln!("confer attach --plugin: {} installed; re-executing as it", v);
                            let _ = out.flush();
                            let err = Command::new(exe)
                                .args(std::env::args_os().skip(1))
                                .env(UPGRADED_FROM, format!("confer {}", crate::VERSION))
                                .exec();
                            eprintln!("confer attach --plugin: could not run {}: {err}", exe.display());
                            exe_fp = fp;
                        }
                        Installed::Stay => exe_fp = fp,
                        Installed::Unknown => {}
                    }
                }
            }
        }

        let mut any = false;
        for (label, _, tail) in tails.values_mut() {
            for line in tail.drain() {
                any = true;
                if attach::emit(&mut out, label, &line).is_err() {
                    return Ok(());
                }
            }
        }
        if any && out.flush().is_err() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Ok(())
}
