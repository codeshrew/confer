//! `confer dashboard` — a live, read-only TUI window into the fleet.
//!
//! Reads **local git clones directly** (no server, no daemon): each hub's board,
//! roster, presence, and health fold from its working tree via `projection`. It is
//! read-only in the honest sense — it never posts a message, never saves a cursor,
//! never takes the watchlock, never publishes presence. (Git-level *sync* —
//! integrating a hub so its working tree advances — is added in the sync worker.)
//!
//! Structure: an `App` holds N `HubView`s (one per followed hub) + a selected tab.
//! Step 1 renders a single hub statically and refreshes on a keypress; the notify
//! FS watch + background sync worker land in step 2.

use crate::projection::{self, render_targets};
use crate::{config, gitcmd, presence, roster, watchlock};
use anyhow::Result;
use chrono::{DateTime, Utc};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use notify::{recommended_watcher, RecursiveMode, Watcher};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap};
use std::path::PathBuf;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::Duration;

/// A hub folded for rendering (board + agents + tail + health) — shared with the
/// web view. See `projection::Snapshot`.
type HubView = projection::Snapshot;

/// Events that drive the render loop, from three sources: the input thread, the
/// per-hub FS watchers, and the background sync worker.
enum Ev {
    Key(KeyEvent),
    /// A hub's working tree changed on disk (FS watch) → re-fold it.
    Dirty(usize),
    /// The sync worker integrated a hub: its reachability, and whether HEAD moved.
    Synced { hub: usize, reachable: bool, changed: bool },
    /// Redraw only (e.g. a terminal resize).
    Redraw,
}

// ── palette (16/256-safe; no truecolor requirement, degrades over SSH) ──────────
const C_DIM: Color = Color::DarkGray;
const C_OPEN: Color = Color::Yellow;
const C_CLAIMED: Color = Color::Cyan;
const C_BLOCKED: Color = Color::Magenta;
const C_DONE: Color = Color::Green;
const C_ERR: Color = Color::Red;

fn status_color(status: &str) -> Color {
    match status {
        "OPEN" => C_OPEN,
        "CLAIMED" => C_CLAIMED,
        "BLOCKED" => C_BLOCKED,
        "DONE" => C_DONE,
        "ERROR" => C_ERR,
        _ => C_DIM,
    }
}

/// The running dashboard: the followed hubs + which tab is selected.
struct App {
    hubs: Vec<HubView>,
    tab: usize,
    /// Show closed requests in the board pane (toggle with `c`).
    show_closed: bool,
}

impl App {
    fn load(dirs: Vec<PathBuf>) -> Result<App> {
        let hubs: Vec<HubView> = dirs.into_iter().map(|d| HubView::load(d, true)).collect();
        Ok(App { hubs, tab: 0, show_closed: false })
    }

    /// Re-fold the currently selected hub (manual refresh; the worker will drive
    /// this automatically in step 2).
    fn refresh_current(&mut self) {
        if let Some(h) = self.hubs.get_mut(self.tab) {
            *h = HubView::load(h.dir.clone(), true);
        }
    }

    /// The event-driven render loop. One mpsc channel
    /// carries three sources: a blocking input thread, per-hub FS watchers, and a
    /// background sync worker. The main thread NEVER runs a git subprocess — it only
    /// folds pure-FS projections (on a Dirty) and redraws. All integrate/fetch lives
    /// on the worker; reachability is derived from the worker's integrate outcome.
    fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
        let (tx, rx) = channel::<Ev>();
        spawn_input(tx.clone());
        let _watchers = spawn_fs_watchers(&self.hubs, tx.clone());
        spawn_sync_worker(self.hubs.iter().map(|h| h.dir.clone()).collect(), tx);

        let mut dirty = vec![false; self.hubs.len()];
        loop {
            // Fold any hubs flagged dirty (pure FS — cheap, main-thread-safe), then
            // draw. The worker already fetched, so re-fold reads local refs (no net).
            for (i, d) in dirty.iter_mut().enumerate() {
                if *d {
                    let reach = self.hubs[i].health.reachable; // preserve worker's verdict
                    self.hubs[i] = HubView::load(self.hubs[i].dir.clone(), false);
                    self.hubs[i].health.reachable = reach;
                    *d = false;
                }
            }
            terminal.draw(|f| self.draw(f))?;

            // Block until something happens (or a ~2s tick to refresh clock-relative
            // liveness); then coalesce any burst so a flurry of FS events folds once.
            let ev = match rx.recv_timeout(Duration::from_millis(2000)) {
                Ok(ev) => ev,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            };
            let apply = |ev: Ev, app: &mut App, dirty: &mut [bool]| -> Option<()> {
                match ev {
                    Ev::Key(k) => match k.code {
                        KeyCode::Char('q') | KeyCode::Esc => return None,
                        KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => return None,
                        KeyCode::Char('r') => app.refresh_current(),
                        KeyCode::Char('c') => app.show_closed = !app.show_closed,
                        KeyCode::Tab | KeyCode::Right => {
                            if !app.hubs.is_empty() {
                                app.tab = (app.tab + 1) % app.hubs.len();
                            }
                        }
                        KeyCode::BackTab | KeyCode::Left => {
                            if !app.hubs.is_empty() {
                                app.tab = (app.tab + app.hubs.len() - 1) % app.hubs.len();
                            }
                        }
                        _ => {}
                    },
                    Ev::Redraw => {}
                    Ev::Dirty(i) => {
                        if let Some(d) = dirty.get_mut(i) {
                            *d = true;
                        }
                    }
                    Ev::Synced { hub, reachable, changed } => {
                        if let Some(h) = app.hubs.get_mut(hub) {
                            h.health.reachable = Some(reachable);
                        }
                        if changed {
                            if let Some(d) = dirty.get_mut(hub) {
                                *d = true;
                            }
                        }
                    }
                }
                Some(())
            };
            if apply(ev, self, &mut dirty).is_none() {
                return Ok(());
            }
            // Drain the rest of the burst without blocking.
            while let Ok(ev) = rx.try_recv() {
                if apply(ev, self, &mut dirty).is_none() {
                    return Ok(());
                }
            }
        }
    }

    fn draw(&self, f: &mut Frame) {
        // Fresh clock each frame so liveness (●/○/✕) stays current on a bare tick,
        // between folds. Board age/stale are folded values (advance on re-fold).
        let now = Utc::now();
        let area = f.area();
        // top tabs+health strip · body · bottom activity tail · footer keys
        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(6),
            Constraint::Length(9),
            Constraint::Length(1),
        ])
        .split(area);

        self.draw_header(f, rows[0], now);
        match self.hubs.get(self.tab) {
            Some(h) if h.error.is_none() => {
                let cols = Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)]).split(rows[1]);
                draw_agents(f, cols[0], h, now);
                draw_board(f, cols[1], h, self.show_closed);
                draw_tail(f, rows[2], h);
            }
            Some(h) => {
                let p = Paragraph::new(format!("⚠ {}\n\n{}", h.label, h.error.as_deref().unwrap_or("error")))
                    .style(Style::default().fg(C_ERR))
                    .block(Block::default().borders(Borders::ALL).title(" hub unavailable "));
                f.render_widget(p, rows[1]);
            }
            None => {
                let p = Paragraph::new("no hubs to display").style(Style::default().fg(C_DIM));
                f.render_widget(p, rows[1]);
            }
        }
        self.draw_footer(f, rows[3]);
    }

    fn draw_header(&self, f: &mut Frame, area: Rect, now: DateTime<Utc>) {
        let split = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);
        // Tabs row.
        let titles: Vec<Line> = self
            .hubs
            .iter()
            .map(|h| Line::from(format!(" {} ", h.label)))
            .collect();
        let tabs = Tabs::new(titles)
            .select(self.tab)
            .highlight_style(Style::default().fg(Color::Black).bg(C_CLAIMED).add_modifier(Modifier::BOLD))
            .divider(" ");
        f.render_widget(tabs, split[0]);
        // Health strip for the selected hub.
        if let Some(h) = self.hubs.get(self.tab) {
            f.render_widget(Paragraph::new(health_line(h, now)), split[1]);
        }
    }

    fn draw_footer(&self, f: &mut Frame, area: Rect) {
        let keys = "  q quit · r refresh · c toggle closed · Tab/←→ switch hub";
        f.render_widget(Paragraph::new(keys).style(Style::default().fg(C_DIM)), area);
    }
}

/// Blocking-read the terminal on its own thread → `Ev::Key` / `Ev::Redraw`. Exits
/// when the receiver is dropped (main loop returned) or stdin errors.
fn spawn_input(tx: Sender<Ev>) {
    std::thread::spawn(move || loop {
        match event::read() {
            Ok(Event::Key(k)) => {
                if tx.send(Ev::Key(k)).is_err() {
                    break;
                }
            }
            Ok(Event::Resize(..)) => {
                if tx.send(Ev::Redraw).is_err() {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    });
}

/// One `notify` FS watch per hub on its `threads/` dir → `Ev::Dirty(i)` on any
/// change. The returned watchers must be kept alive for the loop's duration.
fn spawn_fs_watchers(hubs: &[HubView], tx: Sender<Ev>) -> Vec<notify::RecommendedWatcher> {
    hubs.iter()
        .enumerate()
        .filter_map(|(i, h)| {
            let threads = h.dir.join("threads");
            if !threads.is_dir() {
                return None;
            }
            let tx = tx.clone();
            let mut w = recommended_watcher(move |res: notify::Result<notify::Event>| {
                if res.is_ok() {
                    let _ = tx.send(Ev::Dirty(i));
                }
            })
            .ok()?;
            w.watch(&threads, RecursiveMode::Recursive).ok()?;
            Some(w)
        })
        .collect()
}

/// The background sync worker: the ONLY place git subprocesses
/// run. Round-robins the hubs on a staggered cadence, integrating each (fetch +
/// ff-merge, so the working tree the folds read actually advances) and warming
/// presence refs, then reports reachability + whether HEAD moved. Never touches the
/// UI directly — it only sends events.
fn spawn_sync_worker(dirs: Vec<PathBuf>, tx: Sender<Ev>) {
    // Per-hub cadence knobs: stagger between hubs, and a rest after a full sweep,
    // so N hubs don't fetch in lockstep. ~15s/hub for one hub.
    const STAGGER: Duration = Duration::from_secs(3);
    const SWEEP_REST: Duration = Duration::from_secs(12);
    std::thread::spawn(move || {
        let mut last_head: Vec<Option<String>> = vec![None; dirs.len()];
        loop {
            for (i, dir) in dirs.iter().enumerate() {
                // integrate() = fetch + ff-merge under the shared gitlock; success is
                // our reachability signal (bounded subprocess, no local commits made).
                let reachable = gitcmd::integrate(dir).is_ok();
                // Warm refs/presence/* locally (not in the default refspec) so the
                // main thread's fetch=false re-fold sees fresh heartbeats.
                let _ = presence::load_all(dir, true);
                let head = gitcmd::head(dir).ok();
                let changed = head != last_head[i];
                last_head[i] = head;
                if tx.send(Ev::Synced { hub: i, reachable, changed }).is_err() {
                    return;
                }
                std::thread::sleep(STAGGER);
            }
            std::thread::sleep(SWEEP_REST);
        }
    });
}

/// The one-line health strip: live count · sync · watch · disk.
fn health_line(h: &HubView, now: DateTime<Utc>) -> Line<'static> {
    let live = h
        .agents
        .iter()
        .filter(|a| a.presence.as_ref().map(|p| presence::liveness(p, now) == presence::Live::Up).unwrap_or(false))
        .count();
    let mut spans = Vec::new();
    if !h.health.role.is_empty() {
        spans.push(Span::styled(format!("  as {}", h.health.role), Style::default().fg(C_DIM)));
        spans.push(Span::styled("  · ", Style::default().fg(C_DIM)));
    } else {
        spans.push(Span::raw("  "));
    }
    spans.push(Span::styled(format!("● {live} live"), Style::default().fg(C_DONE)));
    spans.push(Span::styled("  · ", Style::default().fg(C_DIM)));
    let reach = match h.health.reachable {
        Some(true) => Span::styled("hub reachable", Style::default().fg(C_DONE)),
        Some(false) => Span::styled("hub UNREACHABLE", Style::default().fg(C_ERR)),
        None => Span::styled("hub …", Style::default().fg(C_DIM)),
    };
    spans.push(reach);
    if let Some(p) = h.health.pending.filter(|n| *n > 0) {
        spans.push(Span::styled(format!("  ↑{p}"), Style::default().fg(C_OPEN)));
    }
    if let Some(b) = h.health.behind.filter(|n| *n > 0) {
        spans.push(Span::styled(format!("  ↓{b}"), Style::default().fg(C_OPEN)));
    }
    if let Some(w) = &h.health.watch {
        let (txt, col) = match w {
            watchlock::WatchState::Healthy => ("watch ok".to_string(), C_DONE),
            other => (format!("watch {other:?}"), C_ERR),
        };
        spans.push(Span::styled("  · ", Style::default().fg(C_DIM)));
        spans.push(Span::styled(txt, Style::default().fg(col)));
    }
    if let Some(g) = h.health.disk_gb {
        let col = if g < 1.0 { C_ERR } else { C_DIM };
        spans.push(Span::styled("  · ", Style::default().fg(C_DIM)));
        spans.push(Span::styled(format!("{g:.0}GB free"), Style::default().fg(col)));
    }
    Line::from(spans)
}

fn draw_agents(f: &mut Frame, area: Rect, h: &HubView, now: DateTime<Utc>) {
    let items: Vec<ListItem> = h
        .agents
        .iter()
        .map(|a| {
            let (glyph, col) = match &a.presence {
                Some(p) => match presence::liveness(p, now) {
                    presence::Live::Up => ("●", C_DONE),
                    presence::Live::Stale => ("○", C_OPEN),
                    presence::Live::Down => ("✕", C_ERR),
                },
                None => ("·", C_DIM),
            };
            let mut spans = vec![
                Span::styled(format!("{glyph} "), Style::default().fg(col)),
                Span::styled(a.display.clone(), Style::default().add_modifier(Modifier::BOLD)),
            ];
            if let Some(d) = &a.desc {
                spans.push(Span::styled(format!(" — {}", crate::truncate(d, 28)), Style::default().fg(C_DIM)));
            }
            if !a.xhub.is_empty() {
                spans.push(Span::styled(
                    format!("  ≡ {}", a.xhub.iter().map(|(l, r)| format!("{l}:{r}")).collect::<Vec<_>>().join(", ")),
                    Style::default().fg(C_CLAIMED),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let title = format!(" agents ({}) ", h.agents.len());
    f.render_widget(List::new(items).block(Block::default().borders(Borders::ALL).title(title)), area);
}

fn draw_board(f: &mut Frame, area: Rect, h: &HubView, show_closed: bool) {
    let mut items: Vec<ListItem> = Vec::new();
    for row in &h.board.rows {
        let closed = !row.is_open() && row.status != "BLOCKED";
        if closed && !show_closed {
            continue;
        }
        let status_disp = match &row.resolution {
            Some(x) => format!("{}·{x}", row.status),
            None => row.status.to_string(),
        };
        let owner = match row.claimants.as_slice() {
            [] => String::new(),
            [one] => format!(" [by {one}]"),
            [first, rest @ ..] => format!(" [by {first}; ⚠ {}]", rest.join(",")),
        };
        let tag = if row.deferred { " ⏳" } else { "" };
        let stale = if row.stale { "! " } else { "  " };
        let spans = vec![
            Span::styled(stale, Style::default().fg(C_ERR)),
            Span::styled(format!("{status_disp:<12}"), Style::default().fg(status_color(row.status)).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{:>4}  ", crate::fmt_age(row.age_secs)), Style::default().fg(C_DIM)),
            Span::styled(
                format!(
                    "{} → {}{tag} — {}{owner}",
                    roster::display(&h.roster, &row.from),
                    render_targets(&h.roster, &row.to),
                    crate::truncate(&row.summary, 52),
                ),
                Style::default(),
            ),
        ];
        items.push(ListItem::new(Line::from(spans)));
    }
    let b = &h.board;
    let title = format!(
        " board — {} open · {} claimed · {} blocked · {} backlog · {} closed ",
        b.open, b.claimed, b.blocked, b.backlog, b.closed
    );
    f.render_widget(List::new(items).block(Block::default().borders(Borders::ALL).title(title)), area);
}

fn draw_tail(f: &mut Frame, area: Rect, h: &HubView) {
    let lines: Vec<Line> = h
        .tail
        .iter()
        .map(|t| {
            Line::from(vec![
                Span::styled(format!("{} ", t.time), Style::default().fg(C_DIM)),
                Span::styled(format!("{:<9}", t.kind), Style::default().fg(status_color(&t.kind.to_uppercase()))),
                Span::raw(format!(
                    "{} → {} — {}",
                    roster::display(&h.roster, &t.from),
                    render_targets(&h.roster, &t.to),
                    crate::truncate(&t.summary, 60)
                )),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(Block::default().borders(Borders::ALL).title(" activity ")),
        area,
    );
}

/// Entry point for the `dashboard` subcommand. Redirects stderr to a log file
/// before entering the alternate screen so stray warnings can't garble the TUI.
pub fn run(dirs: Vec<PathBuf>) -> Result<()> {
    redirect_stderr_to_log();
    let mut app = App::load(dirs)?;
    if app.hubs.is_empty() {
        anyhow::bail!("no hubs to display — run inside a hub clone or pass --hub <dir>");
    }
    let mut terminal = ratatui::init();
    let res = app.run(&mut terminal);
    ratatui::restore();
    res
}

/// Point fd 2 at `~/.confer/dashboard.log` so the folds' `eprintln!` warnings and
/// git subprocess stderr don't corrupt the alternate screen.
fn redirect_stderr_to_log() {
    let Ok(home) = config::home() else { return };
    let dir = home.join(".confer");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("dashboard.log")) {
        use std::os::unix::io::AsRawFd;
        // SAFETY: dup2 onto the process's stderr fd; f stays open for the process
        // lifetime (leaked intentionally) so the fd remains valid.
        unsafe {
            libc::dup2(f.as_raw_fd(), libc::STDERR_FILENO);
        }
        std::mem::forget(f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn row(id: &str, from: &str, to: &[&str], summary: &str, status: &'static str) -> projection::RequestRow {
        projection::RequestRow {
            id: id.into(),
            from: from.into(),
            to: to.iter().map(|s| s.to_string()).collect(),
            summary: summary.into(),
            status,
            resolution: None,
            deferred: false,
            claimants: Vec::new(),
            age_secs: 3600,
            stale: false,
        }
    }

    fn agent(id: &str, display: &str) -> projection::AgentRow {
        projection::AgentRow {
            id: id.into(),
            display: display.into(),
            desc: Some("test agent".into()),
            expected_host: None,
            last_ts: Some("2026-07-10T04:00:00Z".into()),
            last_host: None,
            presence: None,
            xhub: Vec::new(),
        }
    }

    fn synthetic_hub() -> HubView {
        let mut board = projection::Board::default();
        board.rows = vec![
            row("01AAAAAAAAAAAAAAAAAAAAOPEN", "carol", &["bob"], "wire up the plate gallery", "OPEN"),
            row("01BBBBBBBBBBBBBBBBBBBBDONE", "bob", &["all"], "shipped the alt-text pass", "DONE"),
        ];
        board.open = 1;
        board.closed = 1;
        HubView {
            dir: PathBuf::from("/tmp/hub"),
            label: "team-hub".into(),
            roster: roster::Roster::new(),
            board,
            agents: vec![agent("carol", "Design Studio"), agent("bob", "Mobile Reader")],
            tail: vec![projection::TailItem {
                time: "04:00".into(),
                from: "carol".into(),
                to: vec!["bob".into()],
                kind: "note".into(),
                summary: "adopted the new build".into(),
            }],
            health: projection::Health { role: "alice".into(), reachable: Some(true), pending: None, behind: None, watch: None, disk_gb: Some(120.0) },
            error: None,
        }
    }

    fn render_to_string(app: &App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| app.draw(f)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..h {
            for x in 0..w {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn renders_agents_board_and_tail_without_panic() {
        let app = App { hubs: vec![synthetic_hub()], tab: 0, show_closed: false };
        let s = render_to_string(&app, 120, 30);
        // Tab + health strip.
        assert!(s.contains("team-hub"), "hub tab label missing");
        assert!(s.contains("as alice"), "role missing from health strip");
        // Agents pane.
        assert!(s.contains("Design Studio"), "agent display missing");
        assert!(s.contains("Mobile Reader"));
        // Board pane: open row shown, flow counts in the title.
        assert!(s.contains("wire up the plate gallery"), "open request missing");
        assert!(s.contains("1 open"), "flow tally missing");
        // Closed row hidden by default.
        assert!(!s.contains("shipped the alt-text pass"), "closed row should be hidden");
        // Footer keys.
        assert!(s.contains("q quit"));
    }

    #[test]
    fn toggle_closed_reveals_done_rows() {
        let app = App { hubs: vec![synthetic_hub()], tab: 0, show_closed: true };
        let s = render_to_string(&app, 120, 30);
        assert!(s.contains("shipped the alt-text pass"), "closed row should show when toggled");
    }

    #[test]
    fn error_hub_renders_message_not_panic() {
        let hub = HubView::errored(PathBuf::from("/tmp/x"), "broken".into(), "not a confer hub");
        let app = App { hubs: vec![hub], tab: 0, show_closed: false };
        let s = render_to_string(&app, 100, 20);
        assert!(s.contains("hub unavailable"));
        assert!(s.contains("not a confer hub"));
    }
}
