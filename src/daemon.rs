//! Starting a detached watcher: the daemon `confer arm`, `attach` and the plugin reader keep alive.
//! Split from watch.rs (at its size cap) when the watcher's command line gained its hub label.

use crate::{config, watchlock};
use anyhow::{anyhow, Context, Result};

/// Start this same `watch` as a DETACHED daemon: its own session (so a host's process-group kill
/// cannot take it), stdin from /dev/null, stdout+stderr appended to the spool. Returns the pid.
///
/// The one thing this must never do is lose a wake between the child starting and something
/// attaching: it cannot, because the spool is opened before the child exists and every line the
/// child would have printed to a Monitor goes there instead. `confer attach` picks up from the
/// saved offset whenever it next runs.
pub(crate) fn spawn_detached(root: &std::path::Path, role: &str, extra: &[String]) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let hub = config::hub_key(root);
    watchlock::spawn_guard(&hub, root, role)?;
    let (log, out) = crate::spool::open_for_append(&hub, role)?;
    let err = out.try_clone()?;
    // The path confer was launched as, not current_exe(): on Linux that resolves a brew symlink to
    // the versioned keg, which an upgrade never rewrites, so the watcher could not see it change.
    let exe = crate::selfupdate::launched_as().ok_or_else(|| anyhow!("cannot locate the confer binary"))?;
    let mut cmd = std::process::Command::new(&exe);
    cmd.current_dir(root)
        .arg("watch")
        .arg("--replace")
        .args(["--delivery", "spool"])
        // Only for a reader of `ps`: one role on two hubs is two watchers, and the command line
        // showed the role but not the hub (batcave-net read two as a duplicate).
        .args(["--hub-label", &crate::whoami::label(root)])
        .args(extra)
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err);
    if !role.is_empty() {
        cmd.args(["--role", role]);
    }
    // DOUBLE-FORK, not just setsid. A new session stops a process-GROUP kill from reaching the
    // daemon, and that is what a first test proved. It is not what the harness does: it tears a
    // Monitor down by walking the process TREE from the command it started, and setsid does
    // nothing to parentage — the daemon's ppid was still `confer arm`, so it went down with it.
    // (Observed live: every re-attach after an expiry reported "started", not "already running".)
    //
    // So the forked child sets its session, forks AGAIN, and the intermediate exits at once. The
    // grandchild — the real daemon — is reparented to pid 1 before anything can walk to it. The
    // parent reaps the intermediate immediately so it never lingers as a zombie.
    //
    // SAFETY: setsid/fork/_exit in the forked child before exec — async-signal-safe, no allocation.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            match libc::fork() {
                -1 => Err(std::io::Error::last_os_error()),
                0 => Ok(()),           // grandchild: carry on to exec the watcher
                _ => libc::_exit(0),   // intermediate: vanish, orphaning the grandchild to pid 1
            }
        });
    }
    // Name both paths: ENOENT from a deleted binary read as a missing clone dir (jarvis, pop-os).
    let mut intermediate =
        cmd.spawn().with_context(|| format!("spawn detached watcher: run {} in {}", exe.display(), root.display()))?;
    let _ = intermediate.wait(); // it has already _exit(0)'d; reap it
    eprintln!(
        "confer watch: detached watcher for '{}' started, spooling to {} (its pid is in the watch lock)",
        if role.is_empty() { "<all>" } else { role },
        log.display()
    );
    Ok(())
}
