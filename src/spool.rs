//! The wake spool: how a DETACHED watcher hands wakes to whatever is attached to read them.
//!
//! Until now the watcher ran *inside* the Monitor tool's process: the Monitor read its stdout, and
//! when the Monitor ended, so did the watcher. That was fine while a Monitor could be persistent.
//! It stopped being fine when the harness capped every Monitor at 30 minutes: four hubs meant four
//! expiries, four re-arms and four "reclaimed a stale watch lock" lines every half hour — for an
//! agent that had received no messages at all — and every one of those re-arms flapped presence,
//! so peers saw the agent go down and come back forty-eight times a day.
//!
//! The spool decouples the two lifetimes. A detached watcher (`confer watch --detach`) has its
//! stdout redirected to `~/.confer/spool/<hub_key>/<role>.log` and keeps running when nothing is
//! reading. `confer attach` tails every spool for the role into ONE stream — the thing the Monitor
//! hosts — and when the Monitor expires only the tail dies. The watcher keeps its lock, keeps
//! heartbeating, keeps advancing its cursor. Re-attaching resumes from a saved byte offset, so the
//! wakes that arrived in the gap are replayed, not lost, and nothing already shown is shown twice.
//!
//! Rotation is the writer's job, not the reader's: the watcher holds one open fd to the spool, so
//! a rename by the reader would leave it writing into the renamed file. Instead the watcher
//! truncates its OWN spool, and only when the reader's saved offset says it has consumed
//! everything — the watcher is single-threaded and the only writer, so nothing can land between
//! that check and the truncate. The reader treats "offset beyond end of file" as "rotated, start
//! from zero".

use crate::config;
use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// The writer truncates its spool past this once the reader has caught up.
pub const ROTATE_AT_BYTES: u64 = 2 * 1024 * 1024;

fn dir(hub_key: &str) -> Result<PathBuf> {
    Ok(config::home()?.join(".confer").join("spool").join(hub_key))
}

/// Where a watcher for (hub, role) spools its wakes.
pub fn log_path(hub_key: &str, role: &str) -> Result<PathBuf> {
    let role = if role.is_empty() { "_" } else { role };
    Ok(dir(hub_key)?.join(format!("{role}.log")))
}

/// The reader's high-water mark: how many bytes of the spool have been delivered.
fn offset_path(log: &Path) -> PathBuf {
    log.with_extension("offset")
}

/// Written by `attach` while it is reading; its mtime is the attach heartbeat.
pub fn attach_marker(log: &Path) -> PathBuf {
    log.with_extension("attach.json")
}

/// Open (creating) the spool for appending. Both stdout and stderr of a detached watcher point
/// here, so the reader sees exactly what a Monitor would have seen.
pub fn open_for_append(hub_key: &str, role: &str) -> Result<(PathBuf, File)> {
    let p = log_path(hub_key, role)?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .with_context(|| format!("open spool {}", p.display()))?;
    Ok((p, f))
}

pub fn read_offset(log: &Path) -> u64 {
    std::fs::read_to_string(offset_path(log))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

pub fn write_offset(log: &Path, off: u64) {
    let _ = std::fs::write(offset_path(log), off.to_string());
}

/// The reader has consumed everything currently in the spool.
pub fn reader_caught_up(log: &Path) -> bool {
    let len = std::fs::metadata(log).map(|m| m.len()).unwrap_or(0);
    read_offset(log) >= len
}

/// Is something attached and reading this spool right now?
pub fn attached_pid(log: &Path) -> Option<u32> {
    let txt = std::fs::read_to_string(attach_marker(log)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
    let pid = v.get("pid")?.as_u64()? as u32;
    crate::watchlock::pid_is_live_confer(pid).then_some(pid)
}

/// Seconds since the attach marker was last touched, or `None` if there is no marker.
pub fn attach_age_secs(log: &Path) -> Option<u64> {
    let m = std::fs::metadata(attach_marker(log)).ok()?;
    let t = m.modified().ok()?;
    Some(t.elapsed().map(|d| d.as_secs()).unwrap_or(0))
}

/// An incremental reader over one spool: hands back complete lines written since the saved
/// offset, persisting the offset as it goes so a killed reader resumes where it stopped.
pub struct Tail {
    pub log: PathBuf,
    offset: u64,
}

impl Tail {
    pub fn open(log: PathBuf) -> Self {
        let offset = read_offset(&log);
        Tail { log, offset }
    }

    /// Complete new lines since the last call. A partial trailing line (the writer mid-write) is
    /// left in the file for next time rather than delivered half-formed.
    pub fn drain(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        let Ok(meta) = std::fs::metadata(&self.log) else {
            return out;
        };
        let len = meta.len();
        if len < self.offset {
            // The writer rotated (truncated) under us. Everything before the truncate was
            // already delivered — that is the writer's precondition for truncating.
            self.offset = 0;
            write_offset(&self.log, 0);
        }
        if len == self.offset {
            return out;
        }
        let Ok(mut f) = File::open(&self.log) else {
            return out;
        };
        if f.seek(SeekFrom::Start(self.offset)).is_err() {
            return out;
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_err() {
            return out;
        }
        let mut consumed = 0usize;
        let mut reader = BufReader::new(&buf[..]);
        let mut line = String::new();
        loop {
            line.clear();
            let n = match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            if !line.ends_with('\n') {
                break; // partial: leave for next drain
            }
            consumed += n;
            out.push(line.trim_end_matches(['\n', '\r']).to_string());
        }
        if consumed > 0 {
            self.offset += consumed as u64;
            write_offset(&self.log, self.offset);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("confer-spool-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.join("r.log")
    }

    #[test]
    fn a_tail_resumes_from_its_saved_offset_and_never_repeats() {
        let log = scratch("resume");
        let mut w = OpenOptions::new().create(true).append(true).open(&log).unwrap();
        writeln!(w, "one").unwrap();
        writeln!(w, "two").unwrap();
        let mut t = Tail::open(log.clone());
        assert_eq!(t.drain(), vec!["one", "two"]);
        assert!(t.drain().is_empty(), "nothing new → nothing delivered");
        drop(t);
        // A killed reader comes back and must NOT re-deliver "one"/"two".
        writeln!(w, "three").unwrap();
        let mut t2 = Tail::open(log.clone());
        assert_eq!(t2.drain(), vec!["three"], "only what arrived in the gap");
    }

    #[test]
    fn a_partial_line_waits_for_its_newline() {
        let log = scratch("partial");
        let mut w = OpenOptions::new().create(true).append(true).open(&log).unwrap();
        write!(w, "half").unwrap();
        let mut t = Tail::open(log.clone());
        assert!(t.drain().is_empty(), "a line without its newline is not a wake yet");
        writeln!(w, "-done").unwrap();
        assert_eq!(t.drain(), vec!["half-done"]);
    }

    #[test]
    fn a_rotation_under_the_reader_restarts_from_zero_without_loss() {
        let log = scratch("rotate");
        let mut w = OpenOptions::new().create(true).append(true).open(&log).unwrap();
        writeln!(w, "before").unwrap();
        let mut t = Tail::open(log.clone());
        assert_eq!(t.drain(), vec!["before"]);
        assert!(reader_caught_up(&log));
        // Writer truncates (its precondition — reader caught up — holds), then writes more.
        w.set_len(0).unwrap();
        writeln!(w, "after").unwrap();
        assert_eq!(t.drain(), vec!["after"], "the post-rotation line is delivered, once");
    }
}
