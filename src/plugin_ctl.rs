//! `confer plugin status|restart` — see and restart the confer plugin monitor's readers.
//!
//! A reader records itself in `~/.confer/plugin/readers/<session>.json`. Agents were told to stop
//! an old reader by reading that file by hand, which failed two ways on a real machine: a file
//! whose reader had died still named the dead pid, and the session id the agent saw was not the
//! one its live reader ran under (batcave-net). These commands find the LIVE reader, for this
//! session or else this project, and only ever signal a pid that a reader file names and that is
//! a live confer process.

use crate::cli::PluginAction;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Removes the reader's own file when the reader exits normally. An exec (the self-upgrade) skips
/// this, which is right: the pid is unchanged and the new build rewrites the file.
pub(crate) struct OwnFile(pub(crate) PathBuf);

impl Drop for OwnFile {
    fn drop(&mut self) {
        if read(&self.0).is_some_and(|r| r.pid == std::process::id()) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

pub(crate) struct Reader {
    pub(crate) file: PathBuf,
    pub(crate) pid: u32,
    pub(crate) session: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) version: Option<String>,
}

fn read(f: &Path) -> Option<Reader> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(f).ok()?).ok()?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(String::from);
    Some(Reader {
        file: f.to_path_buf(),
        pid: v.get("pid")?.as_u64()? as u32,
        // Files written before 0.8.38 have no session field; the file name is the session.
        session: s("session").or_else(|| f.file_stem().map(|x| x.to_string_lossy().to_string())),
        project: s("project"),
        version: s("version"),
    })
}

fn all() -> Vec<Reader> {
    let Some(dir) = crate::plugin::readers_dir() else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<Reader> = rd.flatten().filter_map(|e| read(&e.path())).collect();
    v.sort_by_key(|r| r.pid);
    v
}

fn live(r: &Reader) -> bool {
    crate::watchlock::pid_is_live_confer(r.pid)
}

/// Delete reader files whose pid is no longer a live confer process. Returns how many.
pub(crate) fn reap() -> usize {
    let mut n = 0;
    for r in all() {
        if !live(&r) && std::fs::remove_file(&r.file).is_ok() {
            n += 1;
        }
    }
    n
}

fn this_project(explicit: Option<PathBuf>) -> Option<String> {
    explicit
        .or_else(|| std::env::var_os("CLAUDE_PROJECT_DIR").map(PathBuf::from))
        .or_else(|| std::env::current_dir().ok())
        .map(|p| p.canonicalize().unwrap_or(p).to_string_lossy().to_string())
}

fn describe(r: &Reader) -> String {
    format!(
        "pid {} · confer {} · session {} · project {}",
        r.pid,
        r.version.as_deref().unwrap_or("unknown (0.8.37 or older)"),
        r.session.as_deref().unwrap_or("?"),
        r.project.as_deref().unwrap_or("?")
    )
}

pub(crate) fn cmd(action: PluginAction, pid: Option<u32>, project: Option<PathBuf>) -> Result<()> {
    let reaped = reap();
    let readers: Vec<Reader> = all().into_iter().filter(live).collect();
    match action {
        PluginAction::Status => {
            if readers.is_empty() {
                println!("no confer plugin reader is running on this machine");
            }
            let me = crate::autoheal::current_session();
            for r in &readers {
                let mine = me.is_some() && r.session == me;
                println!("{} {}", if mine { "→" } else { "·" }, describe(r));
            }
            if reaped > 0 {
                println!("(removed {reaped} record(s) of readers that are no longer running)");
            }
            Ok(())
        }
        PluginAction::Restart => restart(&readers, pid, project),
    }
}

fn restart(readers: &[Reader], pid: Option<u32>, project: Option<PathBuf>) -> Result<()> {
    let target: &Reader = if let Some(p) = pid {
        readers
            .iter()
            .find(|r| r.pid == p)
            .ok_or_else(|| anyhow!("pid {p} is not a running confer plugin reader (see: confer plugin status)"))?
    } else if let Some(r) = crate::autoheal::current_session()
        .and_then(|s| readers.iter().find(|r| r.session.as_deref() == Some(s.as_str())))
    {
        r
    } else {
        let proj = this_project(project);
        let here: Vec<&Reader> = readers.iter().filter(|r| r.project.is_some() && r.project == proj).collect();
        match here.as_slice() {
            [one] => one,
            [] => {
                return Err(anyhow!(
                    "no running confer plugin reader for this session or for project {}. \
                     See all of them with: confer plugin status",
                    proj.as_deref().unwrap_or("?")
                ))
            }
            many => {
                let list: Vec<String> = many.iter().map(|r| format!("  {}", describe(r))).collect();
                return Err(anyhow!(
                    "{} readers serve project {}; pick yours with --pid:\n{}",
                    many.len(),
                    proj.as_deref().unwrap_or("?"),
                    list.join("\n")
                ));
            }
        }
    };
    let old = target.pid;
    let session = target.session.clone();
    println!("stopping the confer plugin reader: {}", describe(target));
    let _ = std::process::Command::new("kill").args(["-TERM", &old.to_string()]).status();

    // The plugin's wrapper script starts a fresh reader (on whatever confer is installed now). Wait
    // for it: the same session's file, naming a different live pid.
    let wait = std::env::var("CONFER_PLUGIN_RESTART_WAIT").ok().and_then(|s| s.parse().ok()).unwrap_or(45);
    let deadline = Instant::now() + Duration::from_secs(wait);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
        if let Some(new) = all()
            .into_iter()
            .find(|r| r.session == session && r.pid != old && live(r))
        {
            println!("restarted: {}", describe(&new));
            return Ok(());
        }
    }
    Err(anyhow!(
        "stopped pid {old}, but no new reader for this session appeared within {wait}s. The plugin's \
         wrapper restarts it; if it does not, see ~/.confer/plugin/monitor.log. A new Claude Code \
         session always starts one."
    ))
}
