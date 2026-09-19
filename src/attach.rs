//! `confer attach` — the ONE process a Monitor hosts, for every hub you are on.
//!
//! It makes sure a detached watcher is running for each hub registered to your role (starting any
//! that are not), then tails all of their spools into a single stream, one line per wake, each
//! prefixed with the hub it came from. When the Monitor hosting it expires, only this tail dies;
//! the watchers keep running, and the next `attach` resumes each spool from its saved offset —
//! delivering whatever landed in the gap, never repeating what was already shown.
//!
//! Why this shape: the harness caps a Monitor at 30 minutes. With one inline watcher per hub that
//! meant, for four hubs, four expiries and four re-arms every half hour — eight to ten turns for
//! zero messages — and each re-arm flapped presence so peers saw the agent go down and come back.
//! One attach per session turns that into one expiry, one cheap re-attach, and no flapping.

use crate::{autoheal, config, gitcmd, prune, spool, watch, watchlock, BUILD_SHA};
use anyhow::{anyhow, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_term(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

/// One hub this attach covers.
struct Target {
    root: PathBuf,
    role: String,
    hub_key: String,
    label: String,
}

/// A short human label for a hub: the last segment of its canonical remote id, else the dir name.
fn label_for(root: &Path) -> String {
    let remote = gitcmd::output(root, &["config", "--get", "remote.origin.url"])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    crate::reconnect::canonical_hub_id(&remote)
        .and_then(|c| c.rsplit('/').next().map(str::to_string))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "hub".into())
        })
}

/// Every hub this session should be attached to: the current clone if we are in one, plus each
/// watch-registry target this session (or the named role) owns. Ownership uses the same rule
/// `arm` and `rewatch` do — never a co-resident peer's target.
fn targets(role: &Option<String>, session: &Option<String>) -> Result<Vec<Target>> {
    let mut found: Vec<(PathBuf, String)> = Vec::new();
    if let Ok(root) = config::repo_root() {
        if let Ok(r) = config::resolve_role(role.clone(), &root) {
            found.push((root, r));
        }
    }
    let me_session = session.clone().or_else(autoheal::current_session);
    for t in autoheal::load().targets {
        if !autoheal::owned_by_session(&t, &me_session, role) {
            continue;
        }
        if role.as_ref().is_some_and(|r| r != &t.role) {
            continue;
        }
        let p = PathBuf::from(&t.hub);
        if !prune::looks_like_hub(&p) {
            continue; // a registered path that is not a hub has no mail to attach to
        }
        found.push((p, t.role.clone()));
    }
    // Dedup by canonical path + role.
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (root, r) in found {
        let canon = root.canonicalize().unwrap_or(root.clone());
        if !seen.insert((canon.clone(), r.clone())) {
            continue;
        }
        let hub_key = config::hub_key(&canon);
        let label = label_for(&canon);
        out.push(Target { root: canon, role: r, hub_key, label });
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    if out.is_empty() {
        return Err(anyhow!(
            "confer attach: nothing to attach to — not inside a confer clone, and no watch target \
             owned by this session. cd into a clone and re-run, pass `--role <r>`, or `confer reconnect`."
        ));
    }
    Ok(out)
}

/// Make sure a detached, spool-mode watcher is running for `t`. Returns what it did.
fn ensure_watcher(t: &Target, extra: &[String], session_confirmed: bool, force: bool) -> Result<&'static str> {
    let info = watchlock::inspect(&t.hub_key, &t.role, 90);
    let state = watchlock::classify(&info, BUILD_SHA);
    let is_spool = info.as_ref().and_then(|i| i.delivery.as_deref()) == Some("spool");
    match state {
        watchlock::WatchState::Healthy if is_spool => Ok("already running"),
        // A live inline watcher (someone's Monitor is reading it directly). Converting it means
        // killing it, which is exactly the H2 case: only if we can confirm it is ours.
        watchlock::WatchState::Healthy if !(session_confirmed || force) => Ok("left alone (live, not spool, ownership unconfirmed)"),
        // Unknown liveness: never kill on a contradiction (0.8.32). Attach to its spool if it has
        // one; otherwise leave it and say so.
        watchlock::WatchState::Indeterminate => Ok(if is_spool { "attached (liveness unconfirmed)" } else { "left alone (liveness unconfirmed)" }),
        _ => {
            watch::spawn_detached(&t.root, &t.role, extra)?;
            // Give it a moment to take the lock so watch-status is truthful immediately after.
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                let now = watchlock::classify(&watchlock::inspect(&t.hub_key, &t.role, 90), BUILD_SHA);
                if matches!(now, watchlock::WatchState::Healthy | watchlock::WatchState::Outdated) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Ok("started")
        }
    }
}

fn write_marker(log: &Path) {
    let m = spool::attach_marker(log);
    let _ = std::fs::write(
        &m,
        serde_json::json!({
            "pid": std::process::id(),
            "since": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        })
        .to_string(),
    );
}

fn touch_marker(log: &Path) {
    // Rewrite the marker outright rather than `set_len(current_len)`. Measured: a same-size
    // truncate DOES bump mtime on APFS, so this was not the cause of the 24h idle-exit incident —
    // but POSIX does not promise it and ext4 does not do it, and half the fleet is on Linux. The
    // file is ~60 bytes; rewriting it every few seconds is nothing, and it is unambiguous.
    write_marker(log);
}

/// Attach: ensure watchers, then stream. Long-lived; returns on SIGTERM/SIGINT (the Monitor
/// expiring) or when told to stop.
pub fn run(role: Option<String>, session: Option<String>, force: bool, extra: Vec<String>) -> Result<()> {
    let ts = targets(&role, &session)?;
    let me_session = session.clone().or_else(autoheal::current_session);
    let reg = autoheal::load();

    let mut tails: Vec<(String, spool::Tail)> = Vec::new();
    let mut summary: Vec<String> = Vec::new();
    for t in &ts {
        let session_confirmed = me_session.as_deref().is_some_and(|s| {
            reg.targets.iter().any(|r| {
                config::hub_key(Path::new(&r.hub)) == t.hub_key && r.role == t.role && r.session.as_deref() == Some(s)
            })
        });
        let what = ensure_watcher(t, &extra, session_confirmed, force)?;
        summary.push(format!("{} [{}]: {what}", t.label, t.role));
        let log = spool::log_path(&t.hub_key, &t.role)?;
        if let Some(parent) = log.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_marker(&log);
        tails.push((t.label.clone(), spool::Tail::open(log)));
    }

    let handler = on_term as extern "C" fn(libc::c_int) as *const () as libc::sighandler_t;
    unsafe {
        libc::signal(libc::SIGTERM, handler);
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGHUP, handler);
    }

    // On STDOUT, deliberately: this is the line the skill tells an agent proves they are armed,
    // and a Monitor is only guaranteed to read stdout. Everything the daemons print reaches
    // stdout the same way, via their spools.
    let mut out = std::io::stdout().lock();
    writeln!(
        out,
        "confer attach: {} hub(s) — {}. Watchers keep running when this ends; re-run `confer arm` \
         to re-attach and resume from where each spool left off.",
        ts.len(),
        summary.join("; ")
    )?;
    out.flush()?;
    let mut last_touch = Instant::now();
    while !STOP.load(Ordering::SeqCst) {
        let mut any = false;
        for (label, tail) in tails.iter_mut() {
            for line in tail.drain() {
                any = true;
                // Only wake lines get the hub prefix; the watcher's own chatter passes through.
                if line.starts_with("confer") || line.starts_with("──") || line.starts_with("   ") {
                    writeln!(out, "{line}")?;
                } else {
                    writeln!(out, "[{label}] {line}")?;
                }
            }
        }
        if any {
            out.flush()?;
        }
        if last_touch.elapsed() >= Duration::from_secs(5) {
            for (_, tail) in &tails {
                touch_marker(&tail.log);
            }
            last_touch = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    // Deliberately LEAVE the markers in place. Removing them on exit made the seconds between a
    // Monitor expiring and the next attach look, to the watcher, like "nothing has ever been
    // attached since I started" — and after 24h of faithful half-hourly re-attaches every daemon
    // idle-exited in one of those gaps. A stale marker is harmless: `attached_pid` checks the pid
    // is live before believing it, and its mtime is exactly the "last attached" fact the idle
    // exit needs.
    Ok(())
}

/// For `watch-status`: is anything attached to this (hub, role)'s spool, and since when?
pub fn attachment(hub_key: &str, role: &str) -> Option<(u32, u64)> {
    let log = spool::log_path(hub_key, role).ok()?;
    let pid = spool::attached_pid(&log)?;
    Some((pid, spool::attach_age_secs(&log).unwrap_or(0)))
}
