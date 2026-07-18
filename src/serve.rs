//! `confer serve` — a read-only web view of the fleet, all Rust.
//!
//! Same data as the `dashboard` TUI: it renders `projection::Snapshot` to HTML.
//! Architecture mirrors the TUI — a background sync worker is the ONLY place git
//! runs (staggered `integrate` + presence fetch, reachability from integrate), and
//! the HTTP handler just renders the cached snapshots. Read-only: never posts,
//! never takes a lock, never publishes presence. tiny_http is pure-Rust and
//! synchronous (no async runtime) — the same no-tokio choice as the TUI.

use crate::{api, presence, projection, roster};
use anyhow::Result;
use chrono::Utc;
use std::net::{IpAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// TUI-matching palette (a dark terminal theme).
const BG: &str = "#1e2127";
const PANEL: &str = "#22262e";
const FG: &str = "#abb2bf";
const DIM: &str = "#5c6370";
const ACCENT: &str = "#61afef";

/// The web dashboard — a single self-contained SPA built from `ui/`, embedded at build
/// time (see build.rs; a placeholder if the UI wasn't built). Served at `/`; the
/// server-rendered view remains the no-JS fallback at `/classic`.
const DASHBOARD: &str = include_str!(concat!(env!("OUT_DIR"), "/dashboard.html"));

fn status_color(status: &str) -> &'static str {
    match status {
        "OPEN" => "#e5c07b",
        "CLAIMED" => "#56b6c2",
        "BLOCKED" => "#c678dd",
        "DONE" => "#98c379",
        "ERROR" => "#e06c75",
        _ => "#5c6370",
    }
}

/// Minimal HTML escaping (no external dep).
fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(c),
        }
    }
    o
}

fn fmt_age(secs: i64) -> String {
    if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

/// Best-effort LAN IP (no packets sent — just resolves the default route's source).
fn lan_ip() -> Option<IpAddr> {
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    s.local_addr().ok().map(|a| a.ip())
}

/// Flags relevant to choosing the bind address — kept separate from clap's `Serve` variant
/// so the resolution rule below is a pure function, testable without touching the CLI or a
/// socket.
pub struct BindFlags {
    pub bind: Option<String>,
    pub lan: bool,
    pub port: Option<u16>,
    pub env_port: Option<u16>,
}

/// The default port when nothing else specifies one (8787 collides with RStudio Server
/// and some studio apps).
const DEFAULT_PORT: u16 = 8422;

/// Resolve `--bind`/`--lan`/`--port` (+ the `CONFER_SERVE_PORT` env fallback) into the
/// actual address to bind. Precedence, simplest-clean-rule:
///
/// 1. explicit `--bind <addr>` always wins (power-user escape hatch, incl. non-loopback).
/// 2. `--lan` (no `--bind`) → `0.0.0.0:<port>` (all interfaces).
/// 3. otherwise → `127.0.0.1:<port>` (loopback-only, the private default).
///
/// `<port>` comes from `--port`, else `CONFER_SERVE_PORT`, else `DEFAULT_PORT`.
pub fn resolve_bind(flags: &BindFlags) -> String {
    if let Some(explicit) = &flags.bind {
        return explicit.clone();
    }
    let port = flags.port.or(flags.env_port).unwrap_or(DEFAULT_PORT);
    let host = if flags.lan { "0.0.0.0" } else { "127.0.0.1" };
    format!("{host}:{port}")
}

/// Is this resolved bind address something OTHER than loopback — i.e. should the
/// unauthenticated-exposure warning fire? Best-effort string parse (the bind string isn't
/// always a valid `SocketAddr` on its own — e.g. a bare hostname — so anything that isn't
/// recognizably loopback is treated as non-loopback, fail safe toward warning).
fn is_non_loopback_bind(bind: &str) -> bool {
    let host = bind.rsplit_once(':').map(|(h, _)| h).unwrap_or(bind);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    match host.parse::<IpAddr>() {
        Ok(ip) => !ip.is_loopback(),
        Err(_) => host != "localhost",
    }
}

fn health_html(s: &projection::Snapshot) -> String {
    let now = Utc::now();
    let live = s
        .agents
        .iter()
        .filter(|a| a.presence.as_ref().map(|p| presence::liveness(p, now) == presence::Live::Up).unwrap_or(false))
        .count();
    let mut parts = Vec::new();
    if !s.health.role.is_empty() {
        parts.push(format!("<span style='color:{DIM}'>as {}</span>", esc(&s.health.role)));
    }
    parts.push(format!("<span style='color:#98c379'>● {live} live</span>"));
    parts.push(match s.health.reachable {
        Some(true) => "<span style='color:#98c379'>hub reachable</span>".to_string(),
        Some(false) => "<span style='color:#e06c75'>hub UNREACHABLE</span>".to_string(),
        None => format!("<span style='color:{DIM}'>hub …</span>"),
    });
    if let Some(p) = s.health.pending.filter(|n| *n > 0) {
        parts.push(format!("<span style='color:#e5c07b'>↑{p}</span>"));
    }
    if let Some(b) = s.health.behind.filter(|n| *n > 0) {
        parts.push(format!("<span style='color:#e5c07b'>↓{b}</span>"));
    }
    if let Some(w) = &s.health.watch {
        let (t, c) = match w {
            crate::watchlock::WatchState::Healthy => ("watch ok", "#98c379"),
            _ => ("watch !", "#e06c75"),
        };
        parts.push(format!("<span style='color:{c}'>{t}</span>"));
    }
    if let Some(g) = s.health.disk_gb {
        let c = if g < 1.0 { "#e06c75" } else { DIM };
        parts.push(format!("<span style='color:{c}'>{g:.0}GB free</span>"));
    }
    parts.join(&format!(" <span style='color:{DIM}'>·</span> "))
}

fn agents_html(s: &projection::Snapshot) -> String {
    let now = Utc::now();
    if s.agents.is_empty() {
        return format!("<div style='color:{DIM}'>no agents</div>");
    }
    let mut out = String::new();
    for a in &s.agents {
        let (glyph, col) = match &a.presence {
            Some(p) => match presence::liveness(p, now) {
                presence::Live::Up => ("●", "#98c379"),
                presence::Live::Stale => ("○", "#e5c07b"),
                presence::Live::Down => ("✕", "#e06c75"),
            },
            None => ("·", DIM),
        };
        let desc = a.desc.as_deref().map(|d| format!(" <span style='color:{DIM}'>— {}</span>", esc(d))).unwrap_or_default();
        let xh = if a.xhub.is_empty() {
            String::new()
        } else {
            format!(
                " <span style='color:#56b6c2'>≡ {}</span>",
                esc(&a.xhub.iter().map(|(l, r)| format!("{l}:{r}")).collect::<Vec<_>>().join(", "))
            )
        };
        out.push_str(&format!(
            "<div class='row'><span style='color:{col}'>{glyph}</span> <b>{}</b>{desc}{xh}</div>",
            esc(&a.display)
        ));
    }
    out
}

fn board_html(s: &projection::Snapshot, show_closed: bool) -> (String, String) {
    let b = &s.board;
    let title = format!(
        "board — {} open · {} claimed · {} blocked · {} backlog · {} closed",
        b.open, b.claimed, b.blocked, b.backlog, b.closed
    );
    let mut out = String::new();
    for row in &b.rows {
        let closed = !row.is_open() && row.status != "BLOCKED";
        if closed && !show_closed {
            continue;
        }
        let label = match &row.resolution {
            Some(x) => format!("{}·{x}", row.status),
            None => row.status.to_string(),
        };
        let owner = match row.claimants.as_slice() {
            [] => String::new(),
            [one] => format!(" <span style='color:{DIM}'>[by {}]</span>", esc(one)),
            [first, rest @ ..] => format!(" <span style='color:{DIM}'>[by {}; ⚠ {}]</span>", esc(first), esc(&rest.join(","))),
        };
        let tag = if row.deferred { " <span style='color:#e5c07b'>⏳</span>" } else { "" };
        let bang = if row.stale { "<span style='color:#e06c75'>! </span>" } else { "" };
        out.push_str(&format!(
            "<div class='row'>{bang}<span style='color:{};font-weight:600'>{:<12}</span>\
             <span style='color:{DIM}'>{:>4}</span>  {} <span style='color:{DIM}'>→</span> {}{tag} \
             <span style='color:{DIM}'>—</span> {}{owner}</div>",
            status_color(row.status),
            esc(&label),
            fmt_age(row.age_secs),
            esc(roster::display(&s.roster, &row.from)),
            esc(&projection::render_targets(&s.roster, &row.to)),
            esc(&crate::truncate(&row.summary, 72)),
        ));
    }
    if out.is_empty() {
        out = format!("<div style='color:{DIM}'>no open requests</div>");
    }
    (title, out)
}

fn tail_html(s: &projection::Snapshot) -> String {
    if s.tail.is_empty() {
        return format!("<div style='color:{DIM}'>no activity</div>");
    }
    let mut out = String::new();
    for t in s.tail.iter().rev().take(25).rev() {
        out.push_str(&format!(
            "<div class='row'><span style='color:{DIM}'>{}</span> \
             <span style='color:{}'>{:<9}</span>{} <span style='color:{DIM}'>→</span> {} \
             <span style='color:{DIM}'>—</span> {}</div>",
            esc(&t.time),
            status_color(&t.kind.to_uppercase()),
            esc(&t.kind),
            esc(roster::display(&s.roster, &t.from)),
            esc(&projection::render_targets(&s.roster, &t.to)),
            esc(&crate::truncate(&t.summary, 80)),
        ));
    }
    out
}

/// Render the full page for the selected hub.
fn page(snaps: &[projection::Snapshot], sel: usize, show_closed: bool) -> String {
    // Hub tabs (links preserving the closed toggle).
    let closed_q = if show_closed { "&closed=1" } else { "" };
    let tabs: String = snaps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let active = i == sel;
            let style = if active {
                format!("background:#56b6c2;color:#1e2127;font-weight:700;border-radius:4px")
            } else {
                format!("color:{ACCENT}")
            };
            format!("<a href='/?hub={i}{closed_q}' style='{style};padding:2px 8px;margin-right:4px;text-decoration:none'>{}</a>", esc(&s.label))
        })
        .collect();

    let s = &snaps[sel];
    if let Some(err) = &s.error {
        return shell(&s.label, &tabs, &format!("<div class='panel' style='color:#e06c75'>⚠ {}</div>", esc(err)));
    }
    let (btitle, board) = board_html(s, show_closed);
    let toggle = if show_closed {
        format!("<a href='/?hub={sel}' style='color:{DIM}'>hide closed</a>")
    } else {
        format!("<a href='/?hub={sel}&closed=1' style='color:{DIM}'>show closed</a>")
    };
    let body = format!(
        "<div class='bar'>{tabs}<br><span style='margin-left:2px'>{}</span></div>\
         <div class='wrap'>\
           <div class='panel agents'><div class='pt'>agents ({})</div>{}</div>\
           <div class='panel board'><div class='pt'>{}</div>{}</div>\
           <div class='panel tail'><div class='pt'>activity</div>{}</div>\
         </div>\
         <div class='foot'>confer web view · read-only · auto-refresh 5s · {toggle} · the live TUI is <code>confer dashboard</code></div>",
        health_html(s),
        s.agents.len(),
        agents_html(s),
        esc(&btitle),
        board,
        tail_html(s),
    );
    shell(&s.label, &tabs, &body)
}

fn shell(title: &str, _tabs: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset='utf-8'>\
         <meta name='viewport' content='width=device-width, initial-scale=1'>\
         <meta http-equiv='refresh' content='5'>\
         <title>confer · {}</title><style>\
         *{{box-sizing:border-box}}\
         body{{margin:0;background:{BG};color:{FG};font-family:ui-monospace,'SF Mono',Menlo,Consolas,monospace;font-size:13px;line-height:1.5}}\
         .bar{{background:{PANEL};padding:8px 12px;border-bottom:1px solid #333;position:sticky;top:0}}\
         .wrap{{display:flex;flex-wrap:wrap;gap:10px;padding:10px}}\
         .panel{{background:{PANEL};border:1px solid #333;border-radius:6px;padding:8px 10px;overflow:auto}}\
         .agents{{flex:1 1 320px}} .board{{flex:2 1 460px}} .tail{{flex:1 1 100%}}\
         .pt{{color:{ACCENT};font-weight:600;margin-bottom:6px;border-bottom:1px solid #333;padding-bottom:4px}}\
         .row{{white-space:pre-wrap;word-break:break-word}}\
         .foot{{padding:6px 12px;color:{DIM}}} a{{text-decoration:none}}\
         </style></head><body>{}</body></html>",
        esc(title),
        body
    )
}

/// Global cap on concurrent `/api/events` (SSE) connections. Each accepted SSE gets its
/// OWN detached thread (never a shared worker), so this is the only backpressure on how
/// many long-lived connections can exist at once — otherwise an unauthenticated client
/// could open unboundedly many threads/fds.
const MAX_SSE: usize = 32;

/// RAII guard for one held SSE slot: decrements the shared counter on every exit path
/// (client disconnect, write error, normal return) via `Drop`, so a slot can never leak.
struct SseGuard(Arc<AtomicUsize>);

impl Drop for SseGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Try to reserve one SSE slot out of `MAX_SSE`. `None` means the cap is already hit —
/// the caller must not spawn an SSE thread and should respond 503 instead.
fn try_acquire_sse(counter: &Arc<AtomicUsize>) -> Option<SseGuard> {
    loop {
        let cur = counter.load(Ordering::SeqCst);
        if cur >= MAX_SSE {
            return None;
        }
        if counter.compare_exchange(cur, cur + 1, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            return Some(SseGuard(Arc::clone(counter)));
        }
    }
}

/// Parse `?hub=<i>&closed=1` from a request URL.
fn parse_query(url: &str) -> (usize, bool) {
    let q = url.splitn(2, '?').nth(1).unwrap_or("");
    let mut hub = 0;
    let mut closed = false;
    for kv in q.split('&') {
        match kv.split_once('=') {
            Some(("hub", v)) => hub = v.parse().unwrap_or(0),
            Some(("closed", v)) => closed = v == "1" || v == "true",
            _ => {}
        }
    }
    (hub, closed)
}

/// Start the read-only web server over the given hubs at `bind`. Blocks.
pub fn run(dirs: Vec<PathBuf>, bind: &str) -> Result<()> {
    let snaps: Vec<projection::Snapshot> = dirs.iter().map(|d| projection::Snapshot::load(d.clone(), true)).collect();
    let cache = Arc::new(Mutex::new(snaps));

    // Background sync worker — the only place git runs (staggered integrate +
    // presence fetch), re-folding each hub into the cache with reachability set.
    {
        let cache = Arc::clone(&cache);
        let dirs = dirs.clone();
        std::thread::spawn(move || {
            const STAGGER: Duration = Duration::from_secs(3);
            const SWEEP_REST: Duration = Duration::from_secs(12);
            loop {
                for (i, dir) in dirs.iter().enumerate() {
                    let reachable = crate::gitcmd::integrate(dir).is_ok();
                    let _ = presence::load_all(dir, true);
                    let mut snap = projection::Snapshot::load(dir.clone(), false);
                    snap.health.reachable = Some(reachable);
                    if let Ok(mut c) = cache.lock() {
                        if i < c.len() {
                            c[i] = snap;
                        }
                    }
                    std::thread::sleep(STAGGER);
                }
                std::thread::sleep(SWEEP_REST);
            }
        });
    }

    let server = Arc::new(tiny_http::Server::http(bind).map_err(|e| anyhow::anyhow!("bind {bind}: {e}"))?);
    let sse_count = Arc::new(AtomicUsize::new(0));
    let n = cache.lock().map(|c| c.len()).unwrap_or(0);
    // Report the ACTUAL bound port from the listener, not the bind string — so
    // `--bind 127.0.0.1:0` (pick any free port) prints a usable URL instead of ":0".
    let port = server
        .server_addr()
        .to_ip()
        .map(|s| s.port().to_string())
        .unwrap_or_else(|| bind.rsplit(':').next().unwrap_or("8422").to_string());
    eprintln!("confer serve: {n} hub(s), read-only, on:");
    if is_non_loopback_bind(bind) {
        eprintln!(
            "⚠ LAN mode: this dashboard is unauthenticated — anyone on your network can read all hub content and code."
        );
        eprintln!("  http://localhost:{port}  (this machine)");
        if let Some(ip) = lan_ip() {
            eprintln!("  http://{ip}:{port}   ← open this on your phone (same wifi)");
        } else {
            eprintln!("  listening on {bind} (resolved port {port})");
        }
    } else {
        eprintln!("  http://127.0.0.1:{port}  (this machine only — use --lan for phone/LAN access)");
    }
    eprintln!("(Ctrl-C to stop)");

    // A small worker pool, not the single-threaded `incoming_requests()` loop: an open
    // `/api/events` SSE connection blocks whichever thread is holding it, so ordinary
    // JSON/HTML requests need OTHER threads free to answer while it's held open.
    // `Server::recv()` is safe to call from multiple threads concurrently (tiny_http's
    // documented worker-pool pattern) — it hands each thread the next request.
    const WORKERS: usize = 4;
    let mut handles = Vec::with_capacity(WORKERS.saturating_sub(1));
    for _ in 1..WORKERS {
        let server = Arc::clone(&server);
        let cache = Arc::clone(&cache);
        let dirs = dirs.clone();
        let sse_count = Arc::clone(&sse_count);
        handles.push(std::thread::spawn(move || worker_loop(&server, &cache, &dirs, &sse_count)));
    }
    worker_loop(&server, &cache, &dirs, &sse_count);
    for h in handles {
        let _ = h.join();
    }
    Ok(())
}

/// One worker's request loop: pull requests off the shared server until it shuts down
/// (`recv` errors), routing each to the JSON API, SSE, or the existing HTML render.
fn worker_loop(server: &tiny_http::Server, cache: &Mutex<Vec<projection::Snapshot>>, dirs: &[PathBuf], sse_count: &Arc<AtomicUsize>) {
    loop {
        match server.recv() {
            Ok(req) => handle(req, cache, dirs, sse_count),
            Err(_) => return,
        }
    }
}

fn handle(req: tiny_http::Request, cache: &Mutex<Vec<projection::Snapshot>>, dirs: &[PathBuf], sse_count: &Arc<AtomicUsize>) {
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or("/").to_string();

    if path == "/api/events" {
        // SSE must NEVER occupy a shared worker-pool thread (it holds the connection
        // open for the client's whole session) — hand it to its OWN detached thread and
        // let this worker go straight back to `server.recv()`. Total concurrent SSE is
        // still bounded (`MAX_SSE`) so unbounded clients can't spawn unbounded threads.
        match try_acquire_sse(sse_count) {
            Some(guard) => {
                let dirs = dirs.to_vec();
                std::thread::spawn(move || {
                    let _guard = guard; // decrements MAX_SSE's counter on every exit path
                    let writer = req.into_writer();
                    api::serve_sse(&dirs, writer);
                });
            }
            None => {
                let _ = req.respond(tiny_http::Response::from_string("SSE connection limit reached").with_status_code(503));
            }
        }
        return;
    }

    if path.starts_with("/api/") {
        let resp = api::dispatch(dirs, cache, &path, &url);
        let body = serde_json::to_string(&resp.body).unwrap_or_else(|_| "{}".to_string());
        let ct = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();
        let cc = tiny_http::Header::from_bytes(&b"Cache-Control"[..], &b"no-store"[..]).unwrap();
        let response = tiny_http::Response::from_string(body)
            .with_status_code(resp.status)
            .with_header(ct)
            .with_header(cc);
        let _ = req.respond(response);
        return;
    }

    // The server-rendered dashboard is the no-JS fallback (progressive enhancement).
    if path == "/classic" {
        let (mut sel, closed) = parse_query(&url);
        let html = {
            let snaps = cache.lock().unwrap_or_else(|e| e.into_inner());
            if snaps.is_empty() {
                "<h1>no hubs</h1>".to_string()
            } else {
                if sel >= snaps.len() {
                    sel = 0;
                }
                page(&snaps, sel, closed)
            }
        };
        let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
        let _ = req.respond(tiny_http::Response::from_string(html).with_header(header));
        return;
    }

    // Everything else → the embedded SPA (it does its own client-side view routing and
    // reads data from the /api/* endpoints on this same origin).
    let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
    let _ = req.respond(tiny_http::Response::from_string(DASHBOARD).with_header(header));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_acquire_refuses_past_the_cap() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut guards = Vec::new();
        for _ in 0..MAX_SSE {
            guards.push(try_acquire_sse(&counter).expect("under cap must succeed"));
        }
        assert_eq!(counter.load(Ordering::SeqCst), MAX_SSE);
        assert!(try_acquire_sse(&counter).is_none(), "at cap must be refused, not spawn another slot");
    }

    #[test]
    fn sse_guard_decrements_on_every_drop() {
        let counter = Arc::new(AtomicUsize::new(0));
        {
            let _g1 = try_acquire_sse(&counter).unwrap();
            let _g2 = try_acquire_sse(&counter).unwrap();
            assert_eq!(counter.load(Ordering::SeqCst), 2);
        }
        // both guards dropped at scope exit (covers the normal-return exit path; a
        // write-error/disconnect exit is the same code path — `_guard` just drops
        // whenever the SSE thread's closure returns, for any reason).
        assert_eq!(counter.load(Ordering::SeqCst), 0, "guards must decrement on drop");
    }

    fn flags(bind: Option<&str>, lan: bool, port: Option<u16>, env_port: Option<u16>) -> BindFlags {
        BindFlags { bind: bind.map(String::from), lan, port, env_port }
    }

    #[test]
    fn resolve_bind_defaults_to_loopback() {
        assert_eq!(resolve_bind(&flags(None, false, None, None)), "127.0.0.1:8422");
    }

    #[test]
    fn resolve_bind_lan_flag_binds_all_interfaces() {
        assert_eq!(resolve_bind(&flags(None, true, None, None)), "0.0.0.0:8422");
    }

    #[test]
    fn resolve_bind_explicit_bind_wins_over_lan() {
        // Explicit --bind always wins, even if --lan is also (redundantly) passed.
        assert_eq!(resolve_bind(&flags(Some("1.2.3.4:9000"), true, None, None)), "1.2.3.4:9000");
    }

    #[test]
    fn resolve_bind_explicit_bind_wins_over_port() {
        assert_eq!(resolve_bind(&flags(Some("1.2.3.4:9000"), false, Some(1234), None)), "1.2.3.4:9000");
    }

    #[test]
    fn resolve_bind_port_overrides_default_on_loopback() {
        assert_eq!(resolve_bind(&flags(None, false, Some(9090), None)), "127.0.0.1:9090");
    }

    #[test]
    fn resolve_bind_env_port_used_when_no_explicit_port() {
        assert_eq!(resolve_bind(&flags(None, false, None, Some(7777))), "127.0.0.1:7777");
    }

    #[test]
    fn resolve_bind_explicit_port_beats_env_port() {
        assert_eq!(resolve_bind(&flags(None, false, Some(1111), Some(2222))), "127.0.0.1:1111");
    }

    #[test]
    fn resolve_bind_lan_uses_port_too() {
        assert_eq!(resolve_bind(&flags(None, true, Some(5555), None)), "0.0.0.0:5555");
    }

    #[test]
    fn loopback_binds_are_not_flagged() {
        assert!(!is_non_loopback_bind("127.0.0.1:8422"));
        assert!(!is_non_loopback_bind("localhost:8422"));
        assert!(!is_non_loopback_bind("[::1]:8422"));
    }

    #[test]
    fn non_loopback_binds_are_flagged() {
        assert!(is_non_loopback_bind("0.0.0.0:8422"));
        assert!(is_non_loopback_bind("1.2.3.4:8422"));
        assert!(is_non_loopback_bind("192.168.1.5:8422"));
    }

    #[test]
    fn sse_acquire_frees_a_slot_after_a_guard_drops() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut guards: Vec<SseGuard> = (0..MAX_SSE).map(|_| try_acquire_sse(&counter).unwrap()).collect();
        assert!(try_acquire_sse(&counter).is_none());
        guards.pop(); // drop one held slot
        assert!(try_acquire_sse(&counter).is_some(), "freeing one slot must let a new connection in");
    }
}
