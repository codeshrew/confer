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
pub(crate) struct Target {
    pub(crate) root: PathBuf,
    pub(crate) role: String,
    pub(crate) hub_key: String,
    pub(crate) label: String,
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
            if is_member(&root, &r) {
                found.push((root, r));
            } else {
                // A directory that merely LOOKS like a hub is not one of this role's hubs. Adopting
                // it started a watcher there and published presence to its remote (the plugin
                // monitor prototype ran `attach --role` from a planning repo that has threads/).
                eprintln!(
                    "confer attach: not adopting {} — role '{r}' has never joined it (no roles/{r}.md, \
                     and this clone's identity is not '{r}').",
                    root.display()
                );
            }
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
        if !is_member(&p, &t.role) {
            continue; // registered by mistake (e.g. the bug above, before it was fixed)
        }
        found.push((p, t.role.clone()));
    }
    let out = finalize(found);
    if out.is_empty() {
        return Err(anyhow!(
            "confer attach: nothing to attach to — not inside a confer clone, and no watch target \
             owned by this session. cd into a clone and re-run, pass `--role <r>`, or `confer reconnect`."
        ));
    }
    Ok(out)
}

/// Dedup (hub, role) candidates by canonical path and give each its key and label.
pub(crate) fn finalize(found: Vec<(PathBuf, String)>) -> Vec<Target> {
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
    out
}

/// Has `role` actually joined the hub at `root`? Its signed role card is in `roles/`, or this clone
/// was joined as that role (the card may not have reached the roster yet). A directory that only
/// looks like a hub, such as a repo with a `threads/` folder, is not a hub this role is on.
pub(crate) fn is_member(root: &Path, role: &str) -> bool {
    if role.is_empty() {
        return true; // an observer watch belongs to no role
    }
    if crate::roster::load(root).contains_key(role) {
        return true;
    }
    std::fs::read_to_string(root.join(".confer").join("identity.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("role").and_then(|r| r.as_str()).map(|r| r == role))
        .unwrap_or(false)
}

/// Make sure a detached, spool-mode watcher is running for `t`. Returns what it did.
pub(crate) fn ensure_watcher(t: &Target, extra: &[String], session_confirmed: bool, force: bool) -> Result<&'static str> {
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
        // Everything else is replaced — including an Orphaned inline watcher, which looks live but
        // belongs to nobody (its host is gone), so H2's reason to refuse does not apply.
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
            Ok(if state == watchlock::WatchState::Orphaned {
                "started (replaced an orphaned watcher — its host was gone, nothing was reading it)"
            } else {
                "started"
            })
        }
    }
}

/// Claim a spool: the marker names this process as its reader. It is also a LEASE — one reader
/// per spool. Two readers share one byte offset, so between them each wake reaches only one, or
/// both; a reader that finds another live process named here stops reading (see `other_reader`).
/// `mode` is `monitor` for an attach hosted by a Monitor, `plugin` for the plugin monitor.
pub(crate) fn write_marker(log: &Path, mode: &str, session: Option<&str>) {
    let m = spool::attach_marker(log);
    let _ = std::fs::write(
        &m,
        serde_json::json!({
            "pid": std::process::id(),
            "since": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "mode": mode,
            "session": session,
        })
        .to_string(),
    );
}

/// Another live reader holds this spool's lease: (pid, mode, session).
///
/// The heartbeat rewrites the marker outright rather than `set_len(current_len)`. Measured: a
/// same-size truncate DOES bump mtime on APFS, but POSIX does not promise it and ext4 does not do
/// it. The file is ~100 bytes; rewriting it every few seconds is nothing, and it is unambiguous.
pub(crate) fn other_reader(log: &Path) -> Option<(u32, String, Option<String>)> {
    let txt = std::fs::read_to_string(spool::attach_marker(log)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
    let pid = v.get("pid")?.as_u64()? as u32;
    if pid == std::process::id() || !watchlock::pid_is_live_confer(pid) {
        return None;
    }
    let mode = v.get("mode").and_then(|m| m.as_str()).unwrap_or("monitor").to_string();
    let session = v.get("session").and_then(|m| m.as_str()).map(String::from);
    Some((pid, mode, session))
}

pub(crate) fn install_stop_handlers() {
    let handler = on_term as extern "C" fn(libc::c_int) as *const () as libc::sighandler_t;
    unsafe {
        libc::signal(libc::SIGTERM, handler);
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGHUP, handler);
    }
}

pub(crate) fn stopping() -> bool {
    STOP.load(Ordering::SeqCst)
}

/// Print a spool line: wake lines get the hub prefix; the watcher's other notices pass through.
///
/// A watcher's own startup lines (took the lock, replaced its predecessor, started streaming) are
/// not wakes. Every line printed here wakes the agent, and a watcher restart used to deliver six of
/// them for nothing (astrolabos-voice, 0.8.37). They stay in the spool file for anyone debugging.
pub(crate) fn emit(out: &mut impl Write, label: &str, line: &str) -> std::io::Result<()> {
    const LIFECYCLE: [&str; 7] = [
        "confer watch: --replace killed the existing watcher",
        "confer watch: upgraded from",
        "confer watch: (that lock was recorded under host",
        "confer watch: reclaimed a stale watch lock",
        "confer watch: owned by role",
        "confer watch: streaming new items",
        "confer watch: detached watcher for",
    ];
    if LIFECYCLE.iter().any(|p| line.starts_with(p)) {
        return Ok(());
    }
    if line.starts_with("confer") || line.starts_with("──") || line.starts_with("   ") {
        writeln!(out, "{line}")
    } else {
        writeln!(out, "[{label}] {line}")
    }
}

/// Attach: ensure watchers, then stream. Long-lived; returns on SIGTERM/SIGINT (the Monitor
/// expiring) or when told to stop.
pub fn run(role: Option<String>, session: Option<String>, force: bool, extra: Vec<String>) -> Result<()> {
    let ts = targets(&role, &session)?;
    let me_session = session.clone().or_else(autoheal::current_session);
    let reg = autoheal::load();

    // An explicit arm claims these targets for this session, so the plugin monitor (which follows
    // this session's targets) can find them, and a later session's arm can tell they were ours.
    for t in &ts {
        autoheal::add_target(&t.root.to_string_lossy(), &t.role, me_session.clone());
    }
    // Under Claude Code with the confer plugin installed, a plugin monitor is already reading for
    // this session and lasts the whole session. Hosting a second reader under a Monitor would only
    // fight it for the spools and expire every 30 minutes. Say so and stop: nothing to host.
    if let Some(pid) = me_session.as_deref().and_then(crate::plugin::live_reader_for_session) {
        // The plugin reader starts any watcher these need on its next tick (within 5s). Starting
        // them here too would race it: two spawns for one (hub, role), one killing the other. But
        // do not return before they exist: an agent that checks `watch-status` straight after arm
        // would otherwise read "not watching" and start a competing one (batcave-net).
        let up = |t: &Target| {
            matches!(
                watchlock::classify(&watchlock::inspect(&t.hub_key, &t.role, 90), BUILD_SHA),
                watchlock::WatchState::Healthy | watchlock::WatchState::Outdated
            )
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline && !ts.iter().all(up) {
            std::thread::sleep(Duration::from_millis(250));
        }
        let pending: Vec<String> = ts.iter().filter(|t| !up(t)).map(|t| format!("{} [{}]", t.label, t.role)).collect();
        if !pending.is_empty() {
            println!(
                "confer arm: still starting: {}. The plugin monitor retries every few seconds; \
                 check `confer watch-status` shortly. Do not start another watcher.",
                pending.join(", ")
            );
        }
        println!(
            "confer arm: the confer plugin monitor (pid {pid}) is delivering for this session — {} hub(s) \
             handed to it: {}. Nothing to host; no Monitor needed, and nothing to re-arm when one \
             would have expired.",
            ts.len(),
            ts.iter().map(|t| format!("{} [{}]", t.label, t.role)).collect::<Vec<_>>().join(", ")
        );
        return Ok(());
    }

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
        write_marker(&log, "monitor", me_session.as_deref());
        tails.push((t.label.clone(), spool::Tail::open(log)));
    }
    install_stop_handlers();

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
    while !stopping() {
        let mut any = false;
        for (label, tail) in tails.iter_mut() {
            for line in tail.drain() {
                any = true;
                emit(&mut out, label, &line)?;
            }
        }
        if any {
            out.flush()?;
        }
        if last_touch.elapsed() >= Duration::from_secs(5) {
            // A newer reader took a spool's lease (another arm, or the plugin monitor): let it
            // have it. Checked before re-touching, so we never overwrite its claim.
            tails.retain(|(_, tail)| other_reader(&tail.log).is_none());
            if tails.is_empty() {
                break; // nothing left to read; another reader has every spool
            }
            for (_, tail) in &tails {
                write_marker(&tail.log, "monitor", me_session.as_deref());
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

/// For `watch-status`: is anything attached to this (hub, role)'s spool, how long since its
/// heartbeat, and is it the plugin monitor?
pub fn attachment(hub_key: &str, role: &str) -> Option<(u32, u64, bool)> {
    let log = spool::log_path(hub_key, role).ok()?;
    let pid = spool::attached_pid(&log)?;
    let plugin = std::fs::read_to_string(spool::attach_marker(&log))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .is_some_and(|v| v.get("mode").and_then(|m| m.as_str()) == Some("plugin"));
    Some((pid, spool::attach_age_secs(&log).unwrap_or(0), plugin))
}
