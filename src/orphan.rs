//! Is anything still reading this watcher? The question a live pid cannot answer.
//!
//! A Monitor-hosted watcher delivers by writing to its stdout, a pipe the harness reads. When the
//! harness tears that Monitor down, the reading end goes away — but the watcher only finds out the
//! next time it writes, and a quiet hub can go hours without a write. Until then it heartbeats its
//! lock, answers `kill -0`, and looks exactly like a healthy watcher.
//!
//! Argus hit this on Grok Build (0.8.24, 2026-09-25): the session restarted, Grok killed its Monitor
//! tasks, and the `confer arm` processes survived them, reparented. `watch-status` said healthy with
//! `delivery: monitor` for 1h39m while nothing read a byte, and the next `confer arm` REFUSED to
//! replace them — H2 correctly declining to steal what looked like a live co-resident's watcher.
//! Dark, with a green light. The inverse of 0.8.31's false death.
//!
//! Two answers, for the two places the question gets asked:
//!
//! - from INSIDE the watcher, [`stdout_reader_gone`] asks the kernel directly: a pipe whose read
//!   end is closed polls as `POLLHUP` on macOS and `POLLERR` on Linux, with no write needed. A
//!   watcher that sees this exits and releases its lock.
//! - from OUTSIDE — `arm`, `watch-status`, deciding whether an older binary's watcher is anyone's —
//!   [`orphaned_by_host`] checks parentage. A monitor-delivery watcher has the harness's shell as
//!   its parent; once that shell is gone it is reparented to init (or a subreaper), and nothing
//!   can still hold the other end of the pipe it was given.

/// This process's stdout is a pipe or socket whose reading end has been closed. `false` for a
/// terminal, a file (a spool), or anything we cannot inspect — the safe direction, since a `true`
/// makes the watcher exit.
#[cfg(unix)]
pub fn stdout_reader_gone() -> bool {
    use std::os::fd::AsRawFd;
    let fd = std::io::stdout().as_raw_fd();
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return false;
    }
    let kind = (st.st_mode as u32) & (libc::S_IFMT as u32);
    if kind != libc::S_IFIFO as u32 && kind != libc::S_IFSOCK as u32 {
        return false;
    }
    let mut p = libc::pollfd { fd, events: libc::POLLOUT, revents: 0 };
    // Zero timeout: a probe, never a wait.
    let n = unsafe { libc::poll(&mut p, 1, 0) };
    n > 0 && (p.revents & (libc::POLLERR | libc::POLLHUP)) != 0
}
#[cfg(not(unix))]
pub fn stdout_reader_gone() -> bool {
    false
}

/// `pid`'s parent is init or a known subreaper — the harness that started it is gone. Only
/// meaningful for a MONITOR-delivery watcher: a detached spool watcher is reparented to init on
/// purpose, and its reader is a file, not a parent.
pub fn orphaned_by_host(pid: u32) -> bool {
    let Some(ppid) = ps_field(pid, "ppid=").and_then(|s| s.parse::<u32>().ok()) else {
        return false; // cannot tell → not orphaned; this answer licenses a replacement
    };
    if ppid == 1 {
        return true;
    }
    // Linux with a user-session subreaper (systemd --user) adopts orphans instead of pid 1.
    ps_field(ppid, "comm=").is_some_and(|c| {
        let name = c.rsplit('/').next().unwrap_or(&c);
        matches!(name, "systemd" | "launchd" | "init")
    })
}

fn ps_field(pid: u32, field: &str) -> Option<String> {
    let o = std::process::Command::new("ps")
        .args(["-o", field, "-p", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (o.status.success() && !s.is_empty()).then_some(s)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_pipe_with_its_reader_closed_polls_as_gone() {
        // The kernel fact the watcher relies on, pinned on whichever OS runs the suite: macOS
        // reports POLLHUP, Linux POLLERR, and the check accepts either.
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        let (r, w) = (fds[0], fds[1]);
        let probe = |fd| {
            let mut p = libc::pollfd { fd, events: libc::POLLOUT, revents: 0 };
            let n = unsafe { libc::poll(&mut p, 1, 0) };
            n > 0 && (p.revents & (libc::POLLERR | libc::POLLHUP)) != 0
        };
        assert!(!probe(w), "a pipe with a live reader is not gone");
        unsafe { libc::close(r) };
        assert!(probe(w), "closing the reader must be visible without a write");
        unsafe { libc::close(w) };
    }

    #[test]
    fn this_test_process_is_not_orphaned() {
        // The test harness is our parent, not init.
        assert!(!orphaned_by_host(std::process::id()));
    }

    #[test]
    fn an_unknown_pid_is_not_called_orphaned() {
        // "Cannot tell" must never read as "orphaned": that answer licenses killing it.
        assert!(!orphaned_by_host(u32::MAX - 7));
    }
}
