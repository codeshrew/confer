//! `confer watch` — stream new actionable entries until stopped (for the Monitor tool).
//!
//! Git has no push notification, so the cross-machine path is a `git fetch` on an
//! interval. A `notify` filesystem watch on `threads/` wakes us early on any local
//! change (a co-resident write, or a pull landing) for sub-interval latency.

use crate::schema::{is_actionable, Message};
use crate::{config, cursor, gitcmd, hint, roster, store, watchlock, BUILD_SHA};
use anyhow::Result;
use notify::{recommended_watcher, RecursiveMode, Watcher};
use std::io::Write;
use std::path::Path;
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::Duration;

pub struct WatchOpts {
    pub topic: Option<String>,
    /// consumer role (defaults to the joined role; used to skip own messages)
    pub role: Option<String>,
    pub json: bool,
    pub poll_secs: u64,
    /// persist the cursor to disk after each cycle (in-memory advance always happens)
    pub advance: bool,
    /// take over a live watcher for this (hub, role) instead of refusing
    pub replace: bool,
    /// firehose: wake on ALL actionable board traffic, not just what's addressed
    /// to me (for an overseer/coordinator role). Default false — a worker wakes
    /// only for its own mail, so N agents don't each burn a turn on every request.
    pub all: bool,
    /// minimum message priority that wakes me: 0=low (all), 1=normal, 2=high.
    /// A chatty agent can set `--min-priority high` to only wake on urgent items;
    /// lower-priority ones still land, seen on the next `poll`/sweep.
    pub min_priority: u8,
    /// suppress the one-shot "a newer confer is on this hub — update" wake. On by default; set to
    /// true (`--no-version-notice`) if you don't want the watch to nudge you about version drift.
    pub no_version_notice: bool,
    /// self-declared arming method stamped onto the watch lock (`--delivery`), so `watch-status` can
    /// affirm this watcher DELIVERS wakes vs. just runs. The `/confer-watch` skill passes `monitor`.
    pub delivery: Option<String>,
}

/// If this watch's stdout is a discard — a regular file (`> file`) or a non-terminal char device
/// (`> /dev/null`) — the wakes go nowhere and the agent never sees them. Returns a description of the
/// bad sink. A pipe/socket (a real Monitor host, OR a reaped background-bash pipe) is indistinguishable
/// here, so it's deliberately NOT flagged: the reaped-bg case is caught by the dead-watch check on the
/// next command + the /confer-watch skill's anti-background rule.
#[cfg(unix)]
fn stdout_is_discarded() -> Option<&'static str> {
    use std::io::IsTerminal;
    use std::os::fd::AsRawFd;
    if std::io::stdout().is_terminal() {
        return None; // an interactive terminal is fine
    }
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(std::io::stdout().as_raw_fd(), &mut st) } != 0 {
        return None;
    }
    match (st.st_mode as u32) & (libc::S_IFMT as u32) {
        m if m == libc::S_IFREG as u32 => Some("a file (a `>` redirect)"),
        m if m == libc::S_IFCHR as u32 => Some("/dev/null (or a device)"), // non-tty char dev
        _ => None, // FIFO/pipe/socket — could be a real host; don't false-warn
    }
}
#[cfg(not(unix))]
fn stdout_is_discarded() -> Option<&'static str> {
    None
}

pub fn run(opts: WatchOpts) -> Result<()> {
    let root = config::repo_root()?;
    crate::check_version(&root);
    let me = config::resolve_role(opts.role.clone(), &root).unwrap_or_default();
    let hub = config::hub_key(&root);
    // Register this (hub, role) so the SessionStart auto-heal hook knows to keep an
    // eye on it after a compaction. Best-effort, idempotent.
    crate::autoheal::add_target(&root.to_string_lossy(), &me);

    // Single-watcher lock: two watchers for the same (hub, role) on one machine
    // share the cursor and silently steal each other's events (classic orphan
    // after a session compacts). Refuse a live duplicate; reclaim a stale one;
    // `--replace` takes over. Held for the lifetime of this run (Drop releases).
    let stale_secs = opts.poll_secs.max(15) * 4 + 20;
    let lock = crate::watchlock::WatchLock::acquire(&hub, &me, stale_secs, opts.replace, opts.delivery.clone())?;
    eprintln!(
        "confer watch: owned by role '{}' on {} (pid {}, confer {}). A later session for this \
         role reclaims it with `watch --replace` — you don't need to remember this process.",
        if me.is_empty() { "<all>" } else { &me },
        config::hostname().unwrap_or_default(),
        std::process::id(),
        env!("CARGO_PKG_VERSION"),
    );

    let watched = root.join("threads");
    std::fs::create_dir_all(&watched)?;
    // Machine-local signal dir: a co-resident `append` touches it after push, so
    // we wake instantly instead of waiting for the poll interval (fast local path).
    let tips = config::signal_dir().ok();
    if let Some(t) = &tips {
        let _ = std::fs::create_dir_all(t);
    }

    // Best-effort FS watcher. If it can't be set up, `tx` is dropped and
    // `recv_timeout` returns Disconnected → we fall back to pure timer polling.
    let (tx, rx) = channel();
    let _watcher: Option<notify::RecommendedWatcher> = (|| {
        let mut w = recommended_watcher(move |res: notify::Result<notify::Event>| {
            let _ = tx.send(res);
        })
        .ok()?;
        w.watch(&watched, RecursiveMode::Recursive).ok()?;
        if let Some(t) = &tips {
            let _ = w.watch(t, RecursiveMode::NonRecursive);
        }
        Some(w)
    })();

    let interval = Duration::from_secs(opts.poll_secs.max(1));
    let mut since = cursor::load(&hub, &me)?;
    // A topic-filtered watch must not persist the shared cursor (would skip
    // other topics on the next unfiltered read) — like poll (B1).
    let persist = opts.advance && opts.topic.is_none();
    eprintln!(
        "confer watch: streaming new items for '{}' (every {}s, + on-change){}",
        if me.is_empty() { "<all>" } else { &me },
        interval.as_secs(),
        if persist { "" } else { " [cursor not persisted]" }
    );

    // Tier 1 self-observability: surface likely mis-configuration once at startup
    //. Machine-local, no git writes — just help the agent notice.
    // Host self-check: if our output is a discard, the agent will never see a wake — the classic
    // "arm a watch, redirect it to /dev/null / a file, then silently miss everything" trap.
    if let Some(sink) = stdout_is_discarded() {
        crate::warn_safety(format!(
            "this watch's output is going to {sink} — you will NOT see any wakes. A watch must run \
             under a host that READS its stdout (your Monitor tool / the /confer-watch skill), never \
             a `>` redirect. Re-arm via /confer-watch."
        ));
    }
    let firehose = opts.all || me.is_empty();
    if gitcmd::output(&root, &["rev-parse", "--is-shallow-repository"])
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
        .unwrap_or(false)
    {
        crate::warn_safety(
            "this clone is SHALLOW — merge-base cursors can break (events re-emit or get skipped). \
             Run `git fetch --unshallow` (confer clones blobless, not shallow).",
        );
    }
    if opts.poll_secs < 5 {
        crate::hint(format!(
            "watch poll={}s is aggressive on the hub; 10s+ is plenty (idle costs nothing).",
            opts.poll_secs
        ));
    }
    if firehose {
        crate::hint(
            "firehose mode — waking on ALL board traffic, not just your mail. Expect high volume; \
             drop --all for addressed-only.",
        );
    }
    if opts.min_priority > 0 {
        crate::hint(format!(
            "waking only on {}+ priority; lower-priority items still land (see them via `poll`).",
            if opts.min_priority >= 2 { "high" } else { "normal" }
        ));
    }
    // Rolling wake-volume advisory (throttled): high sustained volume suggests
    // mis-tuning (too broad, too chatty). We can measure what we EMIT locally; the
    // true wake→action ratio needs agent cooperation (a future signal).
    let mut emitted_total: u64 = 0;
    let started = std::time::Instant::now();
    let mut last_vol_warn = started;

    // Presence/heartbeat: publish this agent's liveness + cursor to
    // refs/presence/<role> on a throttle. Off `main`, so it never wakes peers or
    // bloats the durable log. Skipped for an unroled/observer watch (nothing to
    // publish) and best-effort (offline failures are non-fatal).
    let heartbeat_every = Duration::from_secs(opts.poll_secs.max(10) * 12).max(Duration::from_secs(180));
    let mut last_heartbeat: Option<std::time::Instant> = None;

    // Unread-inbox nag (emit ≠ read): re-surface directly-addressed mail I haven't
    // CONSUMED, until my read frontier passes it — so a resolution/answer the
    // session missed (a dropped wake, a compaction, an unopened body) doesn't vanish.
    // Never advances the frontier; only real reads (`inbox`/`ack`/`show`/`poll`) do.
    let inbox_every = Duration::from_secs(900); // re-nag at most ~every 15 min
    let mut last_inbox: Option<std::time::Instant> = None;
    let mut last_inbox_key = String::new();


    // Resilience: a transient failure (disk full, a git hiccup) must
    // NOT kill the watcher — the agent would go silently deaf until re-armed. The
    // loop catches errors, backs off, and recovers on its own; it never exits on a
    // recoverable condition. `synced` tracks hub reachability (offline is normal
    // and local emit still works); `io_degraded` + `wait` back off local-read
    // failures so a full disk doesn't spin.
    let base = interval;
    let mut wait = base;
    let mut io_degraded = false;
    let mut synced = true;
    let mut version_noticed = false;
    loop {
        lock.heartbeat(); // prove liveness so a later watcher can tell we're alive
        match gitcmd::integrate(&root) {
            Ok(_) if !synced => {
                eprintln!("confer watch: hub reachable again");
                synced = true;
            }
            Ok(_) => {}
            Err(e) => {
                if synced {
                    crate::warn_safety(format!("hub sync failed ({e}); showing local state"));
                    synced = false;
                }
            }
        }

        // Version-availability wake (on by default; `--no-version-notice` to disable). Unlike the
        // one-shot startup line, this catches a newer build that lands on the hub WHILE you're
        // watching — a long-lived watcher would otherwise never learn until a restart. Emitted ONCE
        // per session to STDOUT (so the Monitor host actually wakes you), and only for genuine
        // semver drift (a sha-only rebuild is noise). The peer-message stream stays uncluttered:
        // this is a single, distinct, opt-out line, not mixed into the KIND-wake format.
        if !opts.no_version_notice && !version_noticed {
            if let Some(pin) = crate::hub_pin(&root) {
                let a = crate::version::assess(&crate::my_build(), Some(&pin));
                if a.outdated && a.grade != "rebuild" {
                    let mut o = std::io::stdout().lock();
                    let _ = writeln!(
                        o,
                        "⟳ UPDATE — a newer confer ({}) is running on this hub; you're on {}. Update \
                         (`confer update`, or `brew update && brew upgrade confer` if brew is your \
                         install path — the tap may need `brew update` first), then re-arm your watch \
                         + `confer install-skill`. (silence: add --no-version-notice)",
                        pin.pin_string(),
                        crate::my_build().pin_string()
                    );
                    let _ = o.flush();
                    version_noticed = true;
                }
            }
        }

        let mut n_emitted: usize = 0;
        match emit_new(&root, &me, &opts, &mut since) {
            Ok(n) => {
                n_emitted = n;
                if io_degraded {
                    eprintln!("confer watch: local read recovered — resuming normally");
                    io_degraded = false;
                }
                wait = base;
                emitted_total += n as u64;
                let elapsed = started.elapsed().as_secs().max(1);
                let per_hour = emitted_total.saturating_mul(3600) / elapsed;
                if emitted_total >= 50 && per_hour >= 60 && last_vol_warn.elapsed().as_secs() >= 1800 {
                    crate::hint(format!(
                        "high wake volume (~{per_hour}/hr, {emitted_total} events). If these aren't \
                         all yours to act on, narrow with --topic{}.",
                        if firehose { " or drop --all" } else { " or --min-priority high" }
                    ));
                    last_vol_warn = std::time::Instant::now();
                }
            }
            Err(e) => {
                // Transient local failure (e.g. disk full). Stay up, back off, and
                // retry — never exit. The cursor didn't advance, so nothing is
                // missed once I/O recovers.
                if !io_degraded {
                    crate::warn_safety(format!(
                        "local read failed ({e}) — retrying with backoff (watch stays up)"
                    ));
                    io_degraded = true;
                }
                wait = next_wait(wait, base, false);
            }
        }
        if persist {
            if let Some(sha) = &since {
                let _ = cursor::save(&hub, &me, sha); // best-effort; in-memory cursor still advances
            }
        }

        // Unread-for-you footer: re-surface directly-addressed mail past my READ
        // frontier (not the delivery cursor). Scan on a fresh emit or on the periodic
        // re-nag; print only when the set changed OR the re-nag interval elapsed, so
        // it's a self-clearing reminder, not spam. Suppressed for --json/topic feeds.
        if !me.is_empty() && !opts.json && opts.topic.is_none() {
            let periodic = last_inbox.is_none_or(|t| t.elapsed() >= inbox_every);
            if n_emitted > 0 || periodic {
                if let Ok(all) = store::all_messages(&root) {
                    let grps = crate::groups::load(&root);
                    let st = crate::inbox::load_state(&hub, &me);
                    let unread = crate::inbox::unread_for_me(&all, &me, &grps, &st);
                    if unread.is_empty() {
                        last_inbox_key.clear(); // cleared — re-show if new mail arrives
                    } else {
                        let key = format!("{}:{}", unread.len(), unread.last().map(|m| m.front.id.as_str()).unwrap_or(""));
                        if key != last_inbox_key || periodic {
                            print_unread_footer(&roster::load(&root), &unread);
                            last_inbox_key = key;
                            last_inbox = Some(std::time::Instant::now());
                        }
                    }
                }
            }
        }

        // Version drift is deliberately NOT pushed into the watch stream. `watch` stdout
        // is the MESSAGE event stream — a Monitor-driven agent wakes on every line, so a
        // repeated "update available" nag burned peers' turns for a non-actionable event
        // (you adopt a new binary on restart, not mid-run). Update awareness now lives in
        // two non-waking places: a one-shot stderr line at watch startup (check_version),
        // and pull-time (`confer status` / `confer version`). See DESIGN.md.

        // Heartbeat: publish liveness + cursor on a throttle, best-effort. Only
        // when we have a role and the hub is reachable (else the push would fail).
        if !me.is_empty() && synced {
            let due = last_heartbeat.is_none_or(|t| t.elapsed() >= heartbeat_every);
            if due {
                let p = crate::presence::Presence {
                    role: me.clone(),
                    host: config::hostname(),
                    last_seen: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    cursor: since.clone(),
                    poll_secs: opts.poll_secs,
                    build: Some(crate::my_build().pin_string()),
                };
                if let Err(e) = crate::presence::publish(&root, &p) {
                    if last_heartbeat.is_none() {
                        crate::warn_safety(format!(
                            "presence publish failed ({e}); peers will see you as stale"
                        ));
                    }
                }
                last_heartbeat = Some(std::time::Instant::now());
            }
        }

        match rx.recv_timeout(wait) {
            Ok(_) => {
                while rx.try_recv().is_ok() {}
                std::thread::sleep(Duration::from_millis(300));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => std::thread::sleep(wait),
        }
    }
}

/// Back-off schedule for the watch loop: reset to `base` on success, else double
/// up to a 5-minute cap. Keeps a persistent failure (full disk, vanished hub) from
/// spinning, while still recovering within one cycle once it clears (an FS event or
/// the timeout re-runs the loop).
fn next_wait(current: Duration, base: Duration, ok: bool) -> Duration {
    if ok {
        base
    } else {
        (current * 2).min(Duration::from_secs(300))
    }
}

/// Should this message wake the consumer `me`? Default: only if addressed to me
/// (to/cc/group/`all`). `firehose` (opt-in `--all`, or an unroled observer) also
/// wakes on any actionable board message. Never wake on my own messages; honor a
/// topic filter. This predicate is the token-cost knob — see emit_new.
fn should_wake(m: &Message, me: &str, grps: &crate::groups::Groups, firehose: bool, topic: Option<&str>, min_priority: u8) -> bool {
    if m.front.from.as_str() == me {
        return false; // never wake on my own echo
    }
    if let Some(t) = topic {
        if m.front.topic.as_deref() != Some(t) {
            return false;
        }
    }
    if priority_rank(m.front.priority.as_deref()) < min_priority {
        return false; // below the wake threshold — still readable via poll/sweep
    }
    crate::groups::addressed(m, me, grps) || (firehose && is_actionable(m))
}

/// Priority as a rank for the wake threshold. Unset = normal (the documented default).
fn priority_rank(p: Option<&str>) -> u8 {
    match p {
        Some("low") => 0,
        Some("high") => 2,
        _ => 1,
    }
}

/// Print the "unread for you" reminder — directly-addressed mail the agent has been
/// shown but not yet consumed (newest first, capped). It never advances the read
/// frontier; only a real read (`inbox`/`ack`/`show`/`poll`) clears it.
fn print_unread_footer(roster: &roster::Roster, unread: &[&Message]) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "── ⚠ {} unread for you (delivered, not yet read) ──", unread.len());
    for m in unread.iter().rev().take(5) {
        let _ = writeln!(
            out,
            "   {}  {} — {}",
            crate::short_id(&m.front.id),
            roster::display(roster, &m.front.from),
            crate::truncate(&m.summary_line(), 66)
        );
    }
    if unread.len() > 5 {
        let _ = writeln!(out, "   (+{} more)", unread.len() - 5);
    }
    let _ = writeln!(out, "   → `confer inbox` to read & clear · `confer ack <id>` to dismiss one");
    let _ = out.flush();
}

/// Emit new actionable messages added since the cursor commit, then advance the
/// cursor to HEAD (commit-ordered incremental read; no wall-clock comparison).
fn emit_new(root: &Path, me: &str, opts: &WatchOpts, since: &mut Option<String>) -> Result<usize> {
    let roster = roster::load(root);
    let grps = crate::groups::load(root);
    let msgs = store::messages_since(root, since.as_deref())?;
    // Wake only for messages ADDRESSED to me (to/cc/group/`all`). The firehose —
    // every actionable board message regardless of recipient — is opt-in (`--all`,
    // or an unroled observer). Waking every agent on every request/claim/done is
    // the dominant token cost: each no-op wake re-reads full context to conclude
    // "not for me". Requests to `all` (work-stealing) still match via addressing,
    // so nothing legitimate is lost by default.
    let firehose = opts.all || me.is_empty();
    let new: Vec<&Message> = msgs
        .iter()
        .filter(|m| should_wake(m, me, &grps, firehose, opts.topic.as_deref(), opts.min_priority))
        .collect();

    let mut out = std::io::stdout().lock();
    let hub_key = crate::config::hub_key(root);
    let mut vc = crate::verify::Cache::default();
    for m in &new {
        let line = if opts.json {
            crate::to_json(m)?
        } else {
            // machine feed → full summary, with the read-path verification glyph
            let t = crate::verify::status(root, &hub_key, &roster, &mut vc, m);
            crate::format_line(&roster, m, true, Some(&t))
        };
        writeln!(out, "{line}")?;
        out.flush()?; // Monitor reads line-by-line over a pipe; flush each event
    }
    // Caught up: anchor the cursor at the last stable (pushed) ancestor of HEAD,
    // not local HEAD — a rebased-away HEAD sha would later orphan the cursor and
    // trigger a full re-emit (R3). We read up to HEAD above, so nothing is missed.
    if let Some(anchor) = gitcmd::cursor_anchor(root) {
        *since = Some(anchor);
    }
    Ok(new.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Frontmatter;

    fn m(from: &str, ty: &str, to: &[&str]) -> Message {
        Message {
            front: Frontmatter {
                id: "01J8Z9K3QH7X".into(),
                from: from.into(),
                msg_type: ty.into(),
                ts: "2026-07-10T00:00:00Z".into(),
                host: None,
                to: to.iter().map(|s| s.to_string()).collect(),
                cc: vec![],
                priority: None,
                topic: Some("general".into()),
                reply_to: None,
                of: None,
                supersedes: None,
                resolution: None,
                defer: false,
                via: None,
                src: None,
                summary: Some("s".into()),
                refs: vec![],
            },
            body: String::new(),
        }
    }

    #[test]
    fn wake_only_on_my_mail_by_default_firehose_opt_in() {
        let g = crate::groups::Groups::new();
        // A request aimed at ANOTHER agent must NOT wake me by default — this is
        // the fix for every-agent-wakes-on-every-request token burn.
        let other = m("bob", "request", &["carol"]);
        assert!(!should_wake(&other, "alice", &g, false, None, 0), "must not wake on peer-to-peer request");
        // …but the firehose (overseer / --all) still sees it.
        assert!(should_wake(&other, "alice", &g, true, None, 0), "firehose should surface it");

        // Addressed to me → wake regardless of firehose.
        let mine = m("bob", "request", &["alice"]);
        assert!(should_wake(&mine, "alice", &g, false, None, 0), "must wake on my own mail");

        // Broadcast to `all` → wake (work-stealing still works without firehose).
        let broadcast = m("bob", "request", &["all"]);
        assert!(should_wake(&broadcast, "alice", &g, false, None, 0), "must wake on `all`");

        // Never wake on my own echo.
        let echo = m("alice", "request", &["all"]);
        assert!(!should_wake(&echo, "alice", &g, false, None, 0), "must not wake on own message");

        // Topic filter still gates.
        let mut off_topic = m("bob", "request", &["alice"]);
        off_topic.front.topic = Some("other".into());
        assert!(!should_wake(&off_topic, "alice", &g, false, Some("general"), 0), "topic filter must gate");
    }

    #[test]
    fn backoff_grows_capped_then_resets() {
        let base = Duration::from_secs(10);
        assert_eq!(next_wait(base, base, false), Duration::from_secs(20));
        assert_eq!(next_wait(Duration::from_secs(20), base, false), Duration::from_secs(40));
        // caps at 5 min no matter how long it keeps failing
        let mut w = base;
        for _ in 0..20 {
            w = next_wait(w, base, false);
        }
        assert_eq!(w, Duration::from_secs(300));
        // a single success resets to base (recover fast)
        assert_eq!(next_wait(w, base, true), base);
    }

    #[test]
    fn min_priority_gates_the_wake() {
        let g = crate::groups::Groups::new();
        let mut normal = m("bob", "note", &["alice"]); // unset priority = normal
        normal.front.priority = None;
        let mut high = m("bob", "note", &["alice"]);
        high.front.priority = Some("high".into());
        // min=high (2): only high wakes; normal is held (still readable via poll).
        assert!(!should_wake(&normal, "alice", &g, false, None, 2), "normal must not wake at min=high");
        assert!(should_wake(&high, "alice", &g, false, None, 2), "high must wake at min=high");
        // min=low (0): both wake.
        assert!(should_wake(&normal, "alice", &g, false, None, 0), "min=low wakes on all");
    }
}

/// Report the local watcher state for a role so a compacted session can self-heal:
/// is one running, is it MINE (this host), and is it on the CURRENT build? The lock
/// is keyed by (hub, role) on the machine, so ownership survives compaction — the
/// new session is still "role X on host H" and can reclaim its own orphan safely.
/// Exits 1 when action (re-arm) is needed so a hook/loop can branch. See DESIGN.md.
pub(crate) fn cmd_watch_status(role: Option<String>, json: bool) -> Result<()> {
    let root = config::repo_root()?;
    let me = config::resolve_role(role, &root).unwrap_or_default();
    let hub = config::hub_key(&root);
    let this_host = config::hostname().unwrap_or_default();
    let cur = BUILD_SHA;
    let info = watchlock::inspect(&hub, &me, 90);
    let arm = format!(
        "confer watch --role {} --replace",
        if me.is_empty() { "<role>" } else { &me }
    );
    // Placeholder for the None arms below (never read when info is Some).
    let i = info.as_ref();
    let (state, detail, rec, healthy): (&str, String, String, bool) = match watchlock::classify(
        &info, cur,
    ) {
        watchlock::WatchState::NotWatching => (
            "not-watching",
            "no watcher running for this role on this machine".into(),
            format!("arm it: {arm}"),
            false,
        ),
        watchlock::WatchState::OtherHost => {
            let i = i.unwrap();
            (
                "other-host",
                format!(
                    "a watcher for '{me}' is registered on host '{}' (you are on '{this_host}')",
                    i.host
                ),
                format!("if this machine should run it, re-arm here: {arm}"),
                false,
            )
        }
        watchlock::WatchState::Stale => {
            let i = i.unwrap();
            (
                    "stale",
                    format!(
                        "a watch lock exists (pid {}) but it's {} (last heartbeat {}s ago) — likely a compaction orphan",
                        i.pid,
                        if !i.alive { "not running" } else { "unresponsive" },
                        i.age_secs
                    ),
                    format!("reclaim it: {arm}"),
                    false,
                )
        }
        watchlock::WatchState::Outdated => {
            let i = i.unwrap();
            (
                "outdated",
                format!(
                    "watching (pid {}, confer {}, since {}) — but your binary is {cur}",
                    i.pid,
                    i.version.as_deref().unwrap_or("?"),
                    i.started_at.as_deref().unwrap_or("?")
                ),
                format!("replace to adopt the new build: {arm}"),
                false,
            )
        }
        watchlock::WatchState::Healthy => {
            let i = i.unwrap();
            (
                "healthy",
                format!(
                    "watching (pid {}, confer {}, since {})",
                    i.pid,
                    i.version.as_deref().unwrap_or("?"),
                    i.started_at.as_deref().unwrap_or("?")
                ),
                String::new(),
                true,
            )
        }
    };

    let delivery = info.as_ref().and_then(|i| i.delivery.clone());
    if json {
        let obj = serde_json::json!({
            "role": me, "host": this_host, "state": state, "healthy": healthy,
            "your_version": cur,
            "watcher_version": info.as_ref().and_then(|i| i.version.clone()),
            "pid": info.as_ref().map(|i| i.pid),
            "delivery": delivery,
            "recommendation": rec,
        });
        println!("{}", serde_json::to_string(&obj)?);
    } else {
        let glyph = if healthy { "✓" } else { "⚠" };
        println!(
            "{glyph} watch [{}]: {state} — {detail}",
            if me.is_empty() { "<role>" } else { &me }
        );
        // Running ≠ delivering: a Monitor-hosted watcher wakes the agent; a plain background one just
        // streams to a place nobody reads. We can only affirm this from the self-declared stamp
        // (design/36); absent it, flag the ambiguity rather than imply healthy-means-delivering.
        if healthy {
            match &delivery {
                Some(m) => println!("  delivery: {m} — armed to deliver wakes."),
                None => hint(
                    "delivery method not recorded — if you didn't arm via /confer-watch (or another \
                     event-delivering wrapper), this watcher may be RUNNING but not waking you. Re-arm via /confer-watch.",
                ),
            }
        }
        if !rec.is_empty() {
            println!("  → {rec}");
        }
    }
    if !healthy {
        std::process::exit(1);
    }
    Ok(())
}
