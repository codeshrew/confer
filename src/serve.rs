//! `confer serve` — a read-only web view of the fleet, all Rust.
//!
//! Same data as the `dashboard` TUI: it renders `projection::Snapshot` to HTML.
//! Architecture mirrors the TUI — a background sync worker is the ONLY place git
//! runs (staggered `integrate` + presence fetch, reachability from integrate), and
//! the HTTP handler just renders the cached snapshots. Read-only: never posts,
//! never takes a lock, never publishes presence. tiny_http is pure-Rust and
//! synchronous (no async runtime) — the same no-tokio choice as the TUI.

use crate::{presence, projection, roster};
use anyhow::Result;
use chrono::Utc;
use std::net::{IpAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// TUI-matching palette (a dark terminal theme).
const BG: &str = "#1e2127";
const PANEL: &str = "#22262e";
const FG: &str = "#abb2bf";
const DIM: &str = "#5c6370";
const ACCENT: &str = "#61afef";

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

    let server = tiny_http::Server::http(bind).map_err(|e| anyhow::anyhow!("bind {bind}: {e}"))?;
    let n = cache.lock().map(|c| c.len()).unwrap_or(0);
    let port = bind.rsplit(':').next().unwrap_or("8422");
    eprintln!("confer serve: {n} hub(s), read-only, on:");
    eprintln!("  http://localhost:{port}");
    if let Some(ip) = lan_ip() {
        eprintln!("  http://{ip}:{port}   ← open this on your phone (same wifi)");
    }
    eprintln!("(Ctrl-C to stop)");

    for req in server.incoming_requests() {
        let (mut sel, closed) = parse_query(req.url());
        let html = {
            let snaps = cache.lock().unwrap();
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
    }
    Ok(())
}
