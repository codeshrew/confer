//! `confer watch` — stream new actionable entries until stopped (for the Monitor tool).
//!
//! Git has no push notification, so the cross-machine path is a `git fetch` on an
//! interval. A `notify` filesystem watch on `threads/` wakes us early on any local
//! change (a co-resident write, or a pull landing) for sub-interval latency.

use crate::machineconfig::{self, WatchPrefs};
use crate::schema::{is_actionable, Message};
use crate::{config, cursor, gitcmd, hint, roster, store, watchlock, BUILD_SHA};
use anyhow::{anyhow, Result};
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
    /// minimum intrinsic wake-rung that pages me — a SECOND, orthogonal gate on top of
    /// `min_priority`/addressing (design/51). Below-floor events still land (poll/inbox), same
    /// land-vs-wake split as `min_priority`. Default: `Notice` (mutes only claim/ack/defer).
    pub wake_on: WakeRung,
    /// suppress the one-shot "a newer confer is on this hub — update" wake. On by default; set to
    /// true (`--no-version-notice`) if you don't want the watch to nudge you about version drift.
    pub no_version_notice: bool,
    /// self-declared arming method stamped onto the watch lock (`--delivery`), so `watch-status` can
    /// affirm this watcher DELIVERS wakes vs. just runs. The `/confer-watch` skill passes `monitor`.
    pub delivery: Option<String>,
    /// explicit owning-session id for the watch-registry stamp (`--session`) — overrides env/disk
    /// detection. For a harness (e.g. Grok Build) that doesn't expose the session to this process, a
    /// hook or the arm skill can pass the id it knows.
    pub session: Option<String>,
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

/// Intrinsic urgency rung of an event (design/51 §3), ascending. Computed receiver-side from
/// (message type, whether the underlying request is MINE, sender priority) — orthogonal to
/// *whether* the event is addressed to me at all (that's `should_wake`'s existing
/// addressed-to-me/firehose gate; this is the second, independent axis, log-level style).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WakeRung {
    /// pure board mechanics: `claim` / `ack` / `defer`.
    Transactional,
    /// substantive but not urgent: a `note`→me, a `done` on MY request, `supersede`.
    Notice,
    /// act-now: a `request`→me, an `error`/`blocked` on MY request, or anything sent at
    /// sender `--priority high` (which promotes ANY event to alert — the break-through override).
    Alert,
}

/// Compute an event's intrinsic wake-rung. `all_msgs` is the FULL log (not just the incremental
/// batch being emitted this cycle) so a `done`/`error`/`blocked` can be traced back via `of` to its
/// request's author — the request itself may have landed (and been read) many cycles ago.
fn wake_rung(m: &Message, me: &str, grps: &crate::groups::Groups, all_msgs: &[Message]) -> WakeRung {
    // Escalation always breaks through: sender `--priority high` promotes ANY event to alert,
    // regardless of type/relevance — keeps `high` meaningful as a deliberate sender override.
    if priority_rank(m.front.priority.as_deref()) >= 2 {
        return WakeRung::Alert;
    }
    let is_mine_request = || {
        m.front
            .of
            .as_deref()
            .and_then(|of| crate::projection::request_author(all_msgs, of))
            .is_some_and(|author| author == me)
    };
    match m.front.msg_type.as_str() {
        "claim" | "ack" | "defer" => WakeRung::Transactional,
        "request" => {
            if crate::groups::addressed(m, me, grps) {
                WakeRung::Alert
            } else {
                WakeRung::Notice
            }
        }
        "error" | "blocked" => {
            if is_mine_request() {
                WakeRung::Alert
            } else {
                WakeRung::Transactional // a break/stall on a request I merely observe
            }
        }
        "done" => {
            if is_mine_request() {
                WakeRung::Notice
            } else {
                WakeRung::Transactional // a resolution on a request I merely observe
            }
        }
        "note" | "supersede" => WakeRung::Notice,
        _ => WakeRung::Notice, // conservative default for any future/unclassified type
    }
}

/// Parse `--min-priority`'s string form. Shared by the CLI parse path and by preference resolution
/// (below), so a saved value round-trips through the exact same validation as a freshly-typed flag.
pub fn parse_min_priority(s: &str) -> Result<u8> {
    match s {
        "low" => Ok(0),
        "normal" => Ok(1),
        "high" => Ok(2),
        other => Err(anyhow!("invalid --min-priority '{other}': expected low | normal | high")),
    }
}

/// Parse `--wake-on`'s string form, applying the `verbose` = "lowest floor + whole-board scope" sugar
/// (design/51 §4) on top of the incoming `all` baseline (which may itself already be true from an
/// explicit `--all` or a saved preference). Shared by the CLI parse path and preference resolution.
pub fn parse_wake_on(s: &str, all_in: bool) -> Result<(WakeRung, bool)> {
    match s {
        "alert" => Ok((WakeRung::Alert, all_in)),
        "notice" => Ok((WakeRung::Notice, all_in)),
        "all" => Ok((WakeRung::Transactional, all_in)),
        "verbose" => Ok((WakeRung::Transactional, true)),
        other => Err(anyhow!("invalid --wake-on '{other}': expected alert | notice | all | verbose")),
    }
}

/// Resolve `wake_on` / `min_priority` / `topic` / `all` for one (hub, role) `watch`/`arm` invocation
/// (design/51 §6/Phase B). Resolution order per field: **explicit CLI flag > saved machine config for
/// this (hub, role) > built-in default** (notice / low / no topic / false). `cli_*` are `None`/`false`
/// when the flag wasn't passed this run — that's what lets "explicitly notice" be told apart from
/// "defaulted to notice" (an `Option<String>` CLI flag with no clap default, not a defaulted plain
/// value; see cli.rs). If ANY flag was explicit this run, the full resolved bundle is saved back —
/// replacing the prior record — so the next bare invocation for this (hub, role) reproduces it exactly
/// without re-deciding. A bare invocation with nothing saved just returns the built-in defaults and
/// writes nothing.
pub fn resolve_watch_prefs(
    hub_key: &str,
    role: &str,
    cli_wake_on: Option<&str>,
    cli_min_priority: Option<&str>,
    cli_topic: Option<&str>,
    cli_all: bool,
) -> Result<(WakeRung, u8, Option<String>, bool)> {
    let saved = machineconfig::get_watch_prefs(hub_key, role);

    let wake_on_str = cli_wake_on
        .map(str::to_string)
        .or_else(|| saved.wake_on.clone())
        .unwrap_or_else(|| "notice".to_string());
    let min_priority_str = cli_min_priority
        .map(str::to_string)
        .or_else(|| saved.min_priority.clone())
        .unwrap_or_else(|| "low".to_string());
    let topic = cli_topic.map(str::to_string).or_else(|| saved.topic.clone());
    // A bool store-true flag can never be explicitly "false" — its only explicit state is present/true —
    // so folding cli_all into the baseline before parsing is the correct (and only possible) resolution.
    let all_baseline = cli_all || saved.all.unwrap_or(false);

    let min_priority = parse_min_priority(&min_priority_str)?;
    let (wake_on, all) = parse_wake_on(&wake_on_str, all_baseline)?;

    let explicit = cli_wake_on.is_some() || cli_min_priority.is_some() || cli_topic.is_some() || cli_all;
    if explicit {
        let _ = machineconfig::save_watch_prefs(
            hub_key,
            role,
            WatchPrefs {
                wake_on: Some(wake_on_str),
                min_priority: Some(min_priority_str),
                topic: topic.clone(),
                all: Some(all),
                extra: Default::default(),
            },
        );
    }
    Ok((wake_on, min_priority, topic, all))
}

pub fn run(opts: WatchOpts) -> Result<()> {
    let root = config::repo_root()?;
    crate::check_version(&root);
    let me = config::resolve_role(opts.role.clone(), &root).unwrap_or_default();
    let hub = config::hub_key(&root);
    // Register this (hub, role) so the SessionStart auto-heal hook knows to keep an
    // eye on it after a compaction. Best-effort, idempotent.
    crate::autoheal::add_target(&root.to_string_lossy(), &me, opts.session.clone());

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
    if opts.wake_on != WakeRung::Notice {
        crate::hint(format!(
            "wake-rung floor is {}; below-floor events still land (see them via `poll`/`inbox`). \
             `--priority high` always breaks through.",
            match opts.wake_on {
                WakeRung::Alert => "alert (act-now only — notes/dones on your own requests are muted)",
                WakeRung::Notice => unreachable!(),
                WakeRung::Transactional => "all (board mechanics — claim/ack/defer — now wake too)",
            }
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
        // per session to STDOUT (so the Monitor host actually wakes you), and only for genuine semver
        // drift (a sha-only rebuild is noise). In TEXT mode it's a prose wake line; in `--json` it's a
        // structured `update-available` EVENT, so the stream stays parseable AND still carries the
        // signal — informational notices are typed events with a standard `event` key, never prose
        // mixed into the message stream and never silently dropped. (design/37 F5 + notice contract)
        if !opts.no_version_notice && !version_noticed {
            if let Some(pin) = crate::hub_pin(&root) {
                let a = crate::version::assess(&crate::my_build(), Some(&pin));
                if a.outdated && a.grade != "rebuild" {
                    let mut o = std::io::stdout().lock();
                    if opts.json {
                        let _ = writeln!(
                            o,
                            "{}",
                            serde_json::json!({
                                "event": "update-available",
                                "hub_version": pin.pin_string(),
                                "your_version": crate::my_build().pin_string(),
                            })
                        );
                    } else {
                        let _ = writeln!(
                            o,
                            "⟳ UPDATE — a newer confer ({}) is running on this hub; you're on {}. Update \
                             (`confer update`, or `brew update && brew upgrade confer` if brew is your \
                             install path — the tap may need `brew update` first), then re-arm your watch \
                             + `confer install-skill`. (silence: add --no-version-notice)",
                            pin.pin_string(),
                            crate::my_build().pin_string()
                        );
                    }
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
                    // The footer is itself a stdout line a Monitor host treats as a wake — so it
                    // must honor the SAME wake-rung floor as per-event emission, or a muted
                    // transactional item (a claim/ack/defer) would leak through here instead
                    // (design/51). It still LANDS — `poll`/`inbox` are unfiltered by rung.
                    let unread: Vec<&Message> = crate::inbox::unread_for_me(&all, &me, &grps, &st)
                        .into_iter()
                        .filter(|m| wake_rung(m, &me, &grps, &all) >= opts.wake_on)
                        .collect();
                    if unread.is_empty() {
                        last_inbox_key.clear(); // cleared — re-show if new mail arrives
                    } else {
                        // Key on the exact unread SET (sorted ids), not len+last-id: the latter
                        // flapped when `unread_for_me`'s order shifted, re-printing the footer for
                        // an unchanged set. Sorted-join is order-independent, so within the re-nag
                        // window an unchanged set prints exactly once.
                        let mut ids: Vec<&str> = unread.iter().map(|m| m.front.id.as_str()).collect();
                        ids.sort_unstable();
                        let key = ids.join(",");
                        if key != last_inbox_key || periodic {
                            print_unread_footer(&roster::load(&root), &unread);
                            last_inbox_key = key;
                            last_inbox = Some(std::time::Instant::now());
                        }
                    }
                }
            }
        }

        // (Version-drift awareness is emitted above as a single ONE-shot text-mode stdout line per
        // session — see the `!version_noticed` block — plus the startup stderr line (check_version)
        // and pull-time `status`/`version`. It is one-shot precisely so a Monitor-driven agent isn't
        // re-nagged every line for a non-actionable event, and it's suppressed in `--json`.)

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
#[allow(clippy::too_many_arguments)]
fn should_wake(
    m: &Message,
    me: &str,
    grps: &crate::groups::Groups,
    firehose: bool,
    topic: Option<&str>,
    min_priority: u8,
    wake_on: WakeRung,
    all_msgs: &[Message],
) -> bool {
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
    if wake_rung(m, me, grps, all_msgs) < wake_on {
        return false; // below the wake-rung floor — still readable via poll/sweep (design/51)
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
/// Above this many wake-worthy items in a single emit batch, the watch coalesces them into one
/// summary line instead of streaming each (see `emit_new`). Sized so a normal reactive trickle
/// (a peer or two posting while you watch) still streams individually, but a stale-cursor catch-up
/// or a real flurry can't blow past a Monitor host's rate cap and kill the watch.
const COALESCE_THRESHOLD: usize = 8;

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
    // `done`/`error`/`blocked` need the FULL log to trace `of` back to the request's author (the
    // request itself may be long-since read, outside this incremental batch) — only pay for that
    // extra load when the batch actually contains one of those types (design/51 wake-rung gate).
    let need_all = msgs
        .iter()
        .any(|m| matches!(m.front.msg_type.as_str(), "done" | "error" | "blocked"));
    let all_msgs: Vec<Message> = if need_all {
        store::all_messages(root).unwrap_or_default()
    } else {
        Vec::new()
    };
    let new: Vec<&Message> = msgs
        .iter()
        .filter(|m| {
            should_wake(
                m,
                me,
                &grps,
                firehose,
                opts.topic.as_deref(),
                opts.min_priority,
                opts.wake_on,
                &all_msgs,
            )
        })
        .collect();

    let mut out = std::io::stdout().lock();
    if new.len() > COALESCE_THRESHOLD {
        // A large catch-up burst — armed after being away (a stale cursor), or a flurry that
        // landed at once — would stream dozens of individual wake lines. That is enough to trip a
        // Monitor host's rate cap, which then SILENTLY kills the watch and the agent goes deaf
        // (observed 2026-07-19). Coalesce it into ONE line: reactive-going-forward is the watch's
        // job; replaying a backlog is `poll`/`inbox`'s. The cursor still advances to head below, so
        // steady-state resumes clean, and the unread-for-you footer still surfaces the direct-mail
        // subset that actually needs action. In `--json` it's a typed `backlog` event so the stream
        // stays parseable (same contract as `update-available`).
        if opts.json {
            writeln!(
                out,
                "{}",
                serde_json::json!({
                    "event": "backlog",
                    "count": new.len(),
                    "addressed": !firehose,
                })
            )?;
        } else if firehose {
            writeln!(
                out,
                "⏩ {} board items since you were last here — `confer poll` to review.",
                new.len()
            )?;
        } else {
            writeln!(
                out,
                "⏩ {} message(s) for you since you were last here — `confer inbox` to read them \
                 (`confer poll` for the full stream).",
                new.len()
            )?;
        }
        out.flush()?;
    } else {
        let hub_key = crate::config::hub_key(root);
        let mut vc = crate::verify::Cache::default();
        for m in &new {
            let line = if opts.json {
                let t = crate::verify::status(root, &hub_key, &roster, &mut vc, m);
                let tier = crate::tiers::get(&hub_key);
                crate::to_json(m, &t, tier, crate::screen_note(m, tier).as_deref())?
            } else {
                // machine feed → full summary, with the read-path verification glyph
                let t = crate::verify::status(root, &hub_key, &roster, &mut vc, m);
                crate::format_line(&roster, m, true, Some(&t))
            };
            writeln!(out, "{line}")?;
            out.flush()?; // Monitor reads line-by-line over a pipe; flush each event
        }
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

    /// `should_wake` wrapper defaulting the two design/51 params to "don't gate on rung, no full
    /// log needed" — the pre-design/51 test surface below only exercises addressing/topic/firehose.
    #[allow(clippy::too_many_arguments)]
    fn sw(m: &Message, me: &str, g: &crate::groups::Groups, firehose: bool, topic: Option<&str>, min_priority: u8) -> bool {
        should_wake(m, me, g, firehose, topic, min_priority, WakeRung::Transactional, &[])
    }

    #[test]
    fn wake_only_on_my_mail_by_default_firehose_opt_in() {
        let g = crate::groups::Groups::new();
        // A request aimed at ANOTHER agent must NOT wake me by default — this is
        // the fix for every-agent-wakes-on-every-request token burn.
        let other = m("bob", "request", &["carol"]);
        assert!(!sw(&other, "alice", &g, false, None, 0), "must not wake on peer-to-peer request");
        // …but the firehose (overseer / --all) still sees it.
        assert!(sw(&other, "alice", &g, true, None, 0), "firehose should surface it");

        // Addressed to me → wake regardless of firehose.
        let mine = m("bob", "request", &["alice"]);
        assert!(sw(&mine, "alice", &g, false, None, 0), "must wake on my own mail");

        // Broadcast to `all` → wake (work-stealing still works without firehose).
        let broadcast = m("bob", "request", &["all"]);
        assert!(sw(&broadcast, "alice", &g, false, None, 0), "must wake on `all`");

        // Never wake on my own echo.
        let echo = m("alice", "request", &["all"]);
        assert!(!sw(&echo, "alice", &g, false, None, 0), "must not wake on own message");

        // Topic filter still gates.
        let mut off_topic = m("bob", "request", &["alice"]);
        off_topic.front.topic = Some("other".into());
        assert!(!sw(&off_topic, "alice", &g, false, Some("general"), 0), "topic filter must gate");
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
        assert!(!sw(&normal, "alice", &g, false, None, 2), "normal must not wake at min=high");
        assert!(sw(&high, "alice", &g, false, None, 2), "high must wake at min=high");
        // min=low (0): both wake.
        assert!(sw(&normal, "alice", &g, false, None, 0), "min=low wakes on all");
    }

    /// Build a request from `req_from`, plus its `done`/`error`/`blocked` reply from `reply_from`
    /// (all addressed to `me` so the addressing gate itself never trips these tests — only the
    /// wake-rung gate is under test).
    fn req_and_reply(req_from: &str, req_id: &str, reply_from: &str, reply_ty: &str, me: &str) -> (Message, Message) {
        let mut req = m(req_from, "request", &[me]);
        req.front.id = req_id.into();
        let mut reply = m(reply_from, reply_ty, &[me]);
        reply.front.id = "01J8Z9K3QH7Y".into();
        reply.front.of = Some(req_id.into());
        (req, reply)
    }

    #[test]
    fn wake_rung_computation_matches_design_51() {
        let g = crate::groups::Groups::new();
        let all: Vec<Message> = Vec::new();

        // Board mechanics: always transactional.
        for ty in ["claim", "ack", "defer"] {
            let e = m("bob", ty, &["alice"]);
            assert_eq!(wake_rung(&e, "alice", &g, &all), WakeRung::Transactional, "{ty} must be transactional");
        }

        // A request addressed to me: alert. Not addressed to me: notice.
        let to_me = m("bob", "request", &["alice"]);
        assert_eq!(wake_rung(&to_me, "alice", &g, &all), WakeRung::Alert, "request to me is alert");
        let to_other = m("bob", "request", &["carol"]);
        assert_eq!(wake_rung(&to_other, "alice", &g, &all), WakeRung::Notice, "request to someone else is notice");

        // note / supersede: notice.
        assert_eq!(wake_rung(&m("bob", "note", &["alice"]), "alice", &g, &all), WakeRung::Notice);
        assert_eq!(wake_rung(&m("bob", "supersede", &["alice"]), "alice", &g, &all), WakeRung::Notice);

        // done/error/blocked on MY request → notice/alert; on someone else's request I merely
        // observe → transactional.
        let (req_mine, done_mine) = req_and_reply("alice", "01J8Z9K3QH7Z", "bob", "done", "alice");
        let all_with_mine = vec![req_mine.clone()];
        assert_eq!(wake_rung(&done_mine, "alice", &g, &all_with_mine), WakeRung::Notice, "done on my request is notice");

        let (req_theirs, done_theirs) = req_and_reply("carol", "01J8Z9K3QH80", "bob", "done", "alice");
        let all_with_theirs = vec![req_theirs.clone()];
        assert_eq!(
            wake_rung(&done_theirs, "alice", &g, &all_with_theirs),
            WakeRung::Transactional,
            "done on someone else's request I merely observe is transactional"
        );

        let (_, error_mine) = req_and_reply("alice", "01J8Z9K3QH7Z", "bob", "error", "alice");
        assert_eq!(wake_rung(&error_mine, "alice", &g, &all_with_mine), WakeRung::Alert, "error on my request is alert");
        let (_, error_theirs) = req_and_reply("carol", "01J8Z9K3QH80", "bob", "error", "alice");
        assert_eq!(
            wake_rung(&error_theirs, "alice", &g, &all_with_theirs),
            WakeRung::Transactional,
            "error on someone else's request I merely observe is transactional"
        );

        // `--priority high` always breaks through, even on a transactional type.
        let mut high_claim = m("bob", "claim", &["alice"]);
        high_claim.front.priority = Some("high".into());
        assert_eq!(wake_rung(&high_claim, "alice", &g, &all), WakeRung::Alert, "priority high always promotes to alert");
    }

    #[test]
    fn wake_on_default_notice_mutes_transactional_only() {
        let g = crate::groups::Groups::new();
        let all: Vec<Message> = Vec::new();
        // default floor = notice
        let claim = m("bob", "claim", &["alice"]);
        assert!(
            !should_wake(&claim, "alice", &g, false, None, 0, WakeRung::Notice, &all),
            "claim must NOT wake at default (notice) floor"
        );
        let ack = m("bob", "ack", &["alice"]);
        assert!(!should_wake(&ack, "alice", &g, false, None, 0, WakeRung::Notice, &all));
        let defer = m("bob", "defer", &["alice"]);
        assert!(!should_wake(&defer, "alice", &g, false, None, 0, WakeRung::Notice, &all));

        // a note to me and a done on MY request DO wake at the default.
        let note = m("bob", "note", &["alice"]);
        assert!(should_wake(&note, "alice", &g, false, None, 0, WakeRung::Notice, &all), "note must wake at notice floor");

        let (req_mine, done_mine) = req_and_reply("alice", "01J8Z9K3QH7Z", "bob", "done", "alice");
        let all_with_mine = vec![req_mine];
        assert!(
            should_wake(&done_mine, "alice", &g, false, None, 0, WakeRung::Notice, &all_with_mine),
            "done on my request must wake at notice floor"
        );
    }

    #[test]
    fn wake_on_alert_mutes_notice_too() {
        let g = crate::groups::Groups::new();
        // a done on MY request does NOT wake at the alert floor…
        let (req_mine, done_mine) = req_and_reply("alice", "01J8Z9K3QH7Z", "bob", "done", "alice");
        let all_with_mine = vec![req_mine];
        assert!(
            !should_wake(&done_mine, "alice", &g, false, None, 0, WakeRung::Alert, &all_with_mine),
            "done on my request must NOT wake at alert floor"
        );
        // …but a request to me and an error on my request DO.
        let req_to_me = m("bob", "request", &["alice"]);
        assert!(should_wake(&req_to_me, "alice", &g, false, None, 0, WakeRung::Alert, &[]), "request to me wakes at alert floor");

        let (req2, error_mine) = req_and_reply("alice", "01J8Z9K3QH81", "bob", "error", "alice");
        let all_with_error = vec![req2];
        assert!(
            should_wake(&error_mine, "alice", &g, false, None, 0, WakeRung::Alert, &all_with_error),
            "error on my request wakes at alert floor"
        );
    }

    #[test]
    fn priority_high_breaks_through_wake_on_alert() {
        let g = crate::groups::Groups::new();
        // A `claim` (normally transactional) sent at --priority high must wake even at the
        // strictest floor (--wake-on alert) — the deliberate sender override (design/51 §4).
        let mut high_claim = m("bob", "claim", &["alice"]);
        high_claim.front.priority = Some("high".into());
        assert!(
            should_wake(&high_claim, "alice", &g, false, None, 0, WakeRung::Alert, &[]),
            "priority high must break through even --wake-on alert"
        );
    }
}

/// Report the local watcher state for a role so a compacted session can self-heal:
/// is one running, is it MINE (this host), and is it on the CURRENT build? The lock
/// is keyed by (hub, role) on the machine, so ownership survives compaction — the
/// new session is still "role X on host H" and can reclaim its own orphan safely.
/// Exits 1 when action (re-arm) is needed so a hook/loop can branch. See DESIGN.md.
pub(crate) fn cmd_watch_status(role: Option<String>, json: bool, check: bool) -> Result<()> {
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
    // `watch-status` is a REPORT: it always exits 0 once it has produced the report above, however bad
    // the news. The scriptable gate lives behind `--check` — exit 1 when the watcher needs action. A
    // genuine "couldn't determine" is an Err raised earlier → exit 3. (design/37; the report already
    // printed, so this only sets the code — no mid-stack process::exit.)
    if check && !healthy {
        return Err(crate::PredicateFalse.into());
    }
    Ok(())
}
