//! confer — git-native coordination blackboard for AI agents.
//! Messages are Markdown files with YAML frontmatter (Obsidian-compatible),
//! one file per message under threads/<topic>/. See DESIGN.md for the architecture and threat model.

// Keep command handlers from silently growing back into monoliths — flags any fn over the
// clippy.toml threshold (150). Advisory in CI; the per-file budget is enforced by the size-budget
// CI job (clippy has no per-file lint). See CLAUDE.md for the module/size conventions.
#![warn(clippy::too_many_lines)]

mod alias;
mod append;
mod autoheal;
mod cli;
mod clonehome;
mod config;
mod config_hub;
mod crosshub;
mod cursor;
#[cfg(feature = "dashboard")]
mod dashboard;
mod doctor;
mod envelope;
mod fleet;
mod ghapp;
mod gitcmd;
mod groups;
mod hooks;
mod identity;
mod inbox;
mod init;
mod join;
mod keygen_release;
mod keyring;
mod knownhubs;
mod machineconfig;
mod presence;
mod projection;
mod reconnect;
mod repos;
mod roster;
mod schema;
mod screen;
mod secrets;
#[cfg(feature = "serve")]
mod serve;
mod skills;
mod store;
mod templates;
mod tiers;
mod transport;
mod trust;
mod verify;
mod version;
mod watch;
mod watchlock;

use anyhow::{anyhow, Result};
use append::{cmd_append, cmd_lifecycle, AppendArgs};
use clap::Parser;
use cli::{Cli, Cmd};
use config_hub::{cmd_config, cmd_hub, cmd_rewatch, cmd_status};
use fleet::{cmd_fleet, cmd_require, cmd_version, update_hint};
use inbox::{cmd_ack, cmd_inbox, cmd_read, cmd_requests, cmd_show, cmd_thread};
use init::{cmd_adopt_clone, cmd_clones, cmd_hubs, cmd_init, cmd_where};
use hooks::{cmd_autoheal, cmd_install_hook, cmd_session_heal, cmd_uninstall_hook};
use identity::{cmd_describe, cmd_identity, cmd_rename, cmd_set_status, cmd_who, cmd_whois};
use join::cmd_join;
use keygen_release::{cmd_changelog, cmd_keygen, cmd_update};
#[cfg(test)]
use reconnect::canonical_hub_id;
use reconnect::{cmd_onboard, cmd_reconnect};
use skills::cmd_install_skill;
#[cfg(test)]
use transport::clone_candidates;
use transport::scheme_from;
use trust::{
    cmd_confirm_key, cmd_doctor, cmd_invite, cmd_repos, cmd_screen, cmd_seen, cmd_trust,
    cmd_verify,
};
// The board/agent folds live in `projection` (shared with the dashboard TUI). Re-
// export the pure helpers so existing call sites (and tests) resolve unqualified.
use projection::id_ref_matches;
#[cfg(test)]
use projection::claimants;
#[cfg(test)]
use schema::Frontmatter;
use schema::{is_actionable, Message};
use std::collections::HashSet;
use std::io::Write;

/// The confer repo commit this build was made from.
pub(crate) const BUILD_SHA: &str = env!("CONFER_GIT_SHA");

// ── Diagnostic conventions (one glyph legend so an AI agent can reliably classify output) ─────────
//   ‼ trust violation — do NOT proceed (identity mismatch / impersonation)   [emitted by verify paths]
//   ⚠ SAFETY — a real problem or silent-failure; action recommended          → `warn_safety`
//   · advisory / tuning — no action required (a hint)                        → `hint`
// All go to STDERR (diagnostics), never stdout (which carries the command's actual output).

/// A SAFETY diagnostic: a real failure or a silent-failure the agent must notice. Always
/// `confer: ⚠ …` on stderr, so `grep ⚠` reliably finds every one.
pub(crate) fn warn_safety(msg: impl std::fmt::Display) {
    eprintln!("confer: ⚠ {msg}");
}

/// An advisory/tuning hint — not a failure. `confer: · …` on stderr, visually distinct from `⚠` so
/// it can be grepped/filtered separately from real problems.
pub(crate) fn hint(msg: impl std::fmt::Display) {
    eprintln!("confer: · {msg}");
}

/// A TRUST-VIOLATION diagnostic — an identity mismatch / impersonation signal, the HIGHEST severity:
/// do NOT proceed. Always `confer: ‼ …` on stderr (the same glyph the message feed + verify paths use).
pub(crate) fn warn_trust(msg: impl std::fmt::Display) {
    eprintln!("confer: ‼ {msg}");
}

/// If this role has ARMED a watch before (an auto-heal target exists) but no live watcher is running
/// on this machine, warn — this is the "backgrounded/reaped watch died silently and I'm no longer
/// receiving peer messages" case, surfaced on the next confer command the agent runs. Gated on a
/// prior autoheal target so a deliberately poll-only agent (never watches) is never nagged.
pub(crate) fn warn_if_watch_should_be_live(root: &std::path::Path, role: &str) {
    if role.is_empty() {
        return;
    }
    let clone = root.to_string_lossy();
    let armed = autoheal::load()
        .targets
        .iter()
        .any(|t| t.role == role && t.hub == clone);
    if !armed {
        return; // poll-only / never armed a watch — nothing to nag about
    }
    let hub = config::hub_key(root);
    match watchlock::classify(&watchlock::inspect(&hub, role, 90), BUILD_SHA) {
        watchlock::WatchState::Stale | watchlock::WatchState::NotWatching => warn_safety(format!(
            "no live watcher for '{role}' on this machine — you are NOT being woken by peer \
             messages. Re-arm via /confer-watch (host it under your Monitor tool, never background \
             bash); check anytime with `confer watch-status`."
        )),
        _ => {}
    }
}
pub(crate) const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("CONFER_GIT_SHA"), ")");

/// The changelog THIS build was compiled from — so `confer changelog` shows exactly what shipped in
/// the binary you're running. A freshly-updated binary carries the new entries; the old one can't,
/// which is why "show me what changed" only makes sense AFTER the update, from the new binary.
const CHANGELOG_MD: &str = include_str!("../CHANGELOG.md");

/// The repo confer's own source lives in — what `invite` tells a cold agent to
/// install from. SSH default (matches our fleet); swap to the https form if you
/// clone GitHub over HTTPS.
const TOOL_REPO_SSH: &str = "git@github.com:codeshrew/confer.git";
const TOOL_REPO_HTTPS: &str = "https://github.com/codeshrew/confer.git";

/// Warn (non-fatal) if this build drifts from the hub's expected confer version.
/// The hub pins its version in `.confer-version`; agents that built an older
/// commit get told to update — the fix for "stale build filtering wrong".
fn check_version(root: &std::path::Path) {
    if let Some(pin) = hub_pin(root) {
        let a = version::assess(&my_build(), Some(&pin));
        // Passively surface only genuine SEMVER drift. A sha-only "rebuild" (same
        // version, newer commit) fires on every dev build — pure noise across the fleet —
        // and stays reportable on demand via `confer version` / `confer status`.
        if a.outdated && a.grade != "rebuild" {
            eprintln!(
                "confer: {} — {} (adopt: confer reconnect --role <you>)",
                a.grade,
                update_hint(a.grade)
            );
        }
    }
}


fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Clip a one-liner for a HUMAN browse view, backing off to a word boundary so we
/// never chop mid-word (one giant word falls back to a hard cut). Machine-streaming
/// paths (`watch`/`poll`) skip this and emit the full summary — an agent consumer
/// must get the whole triage field it was handed.
pub(crate) fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let head: String = s.chars().take(n).collect();
    match head.rsplit_once(char::is_whitespace) {
        Some((keep, _)) if !keep.trim_end().is_empty() => format!("{}…", keep.trim_end()),
        _ => format!("{head}…"),
    }
}

/// Short, distinguishing id fragment (ULID random tail) shown in output and
/// matched by `show` — so the triage → open/close loop is executable from a line.
pub(crate) fn short_id(id: &str) -> &str {
    if id.len() > 6 {
        &id[id.len() - 6..]
    } else {
        id
    }
}

/// Lenient match for **user queries** (`show`/`thread`/`--of` resolution): exact,
/// or a leading/trailing fragment. Callers MUST resolve to a *unique* hit and
/// ambiguity-check (see `resolve_unique`) — never fold on this directly, or a
/// short leading fragment cross-contaminates ids that share a ULID timestamp
/// prefix. Empty `q` never matches (guards the empty-`of` whole-board bug).
fn id_matches(full: &str, q: &str) -> bool {
    !q.is_empty() && (full == q || full.ends_with(q) || full.starts_with(q))
}

/// A full ULID is 26 Crockford-base32 chars — used to accept an as-yet-unfetched
/// canonical id in `resolve` without collapsing a short fragment.
fn is_full_ulid(s: &str) -> bool {
    s.len() == 26 && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Resolve a user-supplied id/fragment to a single canonical message id, or fail
/// loudly on ambiguity — so a fold never runs on a fragment that hits many ids.
fn resolve_unique<'a>(msgs: &'a [Message], query: &str) -> Result<&'a str> {
    let mut hits: Vec<&str> = msgs
        .iter()
        .map(|m| m.front.id.as_str())
        .filter(|id| id_matches(id, query))
        .collect();
    hits.sort();
    hits.dedup();
    match hits.len() {
        1 => Ok(hits[0]),
        0 => Err(anyhow!("no message matches id '{query}'")),
        n => Err(anyhow!(
            "id '{query}' is ambiguous — matches {n} messages; use a longer or full id"
        )),
    }
}

/// Slug rule for role/topic ids: `[a-z0-9][a-z0-9-]*`. Prevents path traversal
/// and keeps filenames/folders clean.
fn valid_slug(s: &str) -> bool {
    let ok_first = s
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    ok_first
        && s.len() <= 64 // bound filename length (role/topic become path components)
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Names reserved by the addressing grammar: valid as a `--to`/`--cc` *target*
/// (`all` = broadcast) but not usable as an *identity* (role / topic / group) —
/// a role literally named `all` would collide with the broadcast keyword.
fn is_reserved_name(s: &str) -> bool {
    s == schema::ALL
}

/// `[‼ ]KIND | HH:MM | from[glyph][→to] — summary[ ⟶ repo:path]`, roles resolved to
/// display names. A high-priority item leads with `‼` so it stands out at triage;
/// a `--ref` shows a compact pointer tag so peers see it without opening the body.
/// `full` = emit the whole summary (machine feeds: `watch`/`poll`); otherwise clip
/// to a word boundary for a human browse view. `trust`, when present, appends a compact
/// verification glyph next to the sender.
pub(crate) fn format_line(
    roster: &roster::Roster,
    m: &Message,
    full: bool,
    trust: Option<&verify::Trust>,
) -> String {
    let kind = m.front.msg_type.to_uppercase();
    let ts = m.front.ts.get(11..16).unwrap_or(&m.front.ts);
    let who = roster::display(roster, &m.front.from);
    let pri = if m.front.priority.as_deref() == Some("high") {
        "‼ "
    } else {
        ""
    };
    // Verification glyph immediately after the sender; omitted (empty) when not computed.
    let vg = trust.map(|t| format!(" {}", t.glyph())).unwrap_or_default();
    let summary = if full {
        m.summary_line()
    } else {
        truncate(&m.summary_line(), 80)
    };
    // Sanitize the whole one-liner: `who`/targets resolve to peer-authored display
    // names, also untrusted. Our own template glyphs (‼ — → ⟶ ✓ ·) carry no control
    // chars, so stripping is a no-op on them. (SEC: terminal-control injection.)
    schema::sanitize_term(
        &format!(
            "{pri}{kind} {} | {ts} | {who}{vg}{} — {}{}",
            short_id(&m.front.id),
            render_targets(roster, &m.front.to),
            summary,
            render_refs(&m.front.refs),
        ),
        false,
    )
}

/// Compact pointer tag for the one-line view: ` ⟶ repo:path` (first ref, +N more).
fn render_refs(refs: &[schema::CodeRef]) -> String {
    let Some(first) = refs.first() else {
        return String::new();
    };
    let more = if refs.len() > 1 {
        format!(" +{}", refs.len() - 1)
    } else {
        String::new()
    };
    format!(" ⟶ {}:{}{more}", first.repo, first.path)
}

/// Render a target list (`to`) as ` → a, b` with role display names resolved
/// (group names and `all` pass through literally).
fn render_targets(roster: &roster::Roster, targets: &[String]) -> String {
    if targets.is_empty() {
        return String::new();
    }
    // Sanitize peer-authored display names before they reach the terminal — a hostile card's
    // `display` could otherwise inject ANSI to spoof/hide a message's addressee line (red-team).
    let names: Vec<String> = targets
        .iter()
        .map(|t| schema::sanitize_term(roster::display(roster, t), false))
        .collect();
    format!(" → {}", names.join(", "))
}

/// JSON view of a message: frontmatter fields + a `body` string.
pub(crate) fn to_json(m: &Message) -> Result<String> {
    let mut v = serde_json::to_value(&m.front)?;
    if let serde_json::Value::Object(map) = &mut v {
        map.insert("body".into(), serde_json::Value::String(m.body.clone()));
    }
    Ok(serde_json::to_string(&v)?)
}

/// A predicate command's valid NEGATIVE result — e.g. `watch-status --check` on an unhealthy watcher,
/// `verify` on a key mismatch. NOT an error: it maps to exit code 1 in `main`, distinct from an
/// execution failure (exit 3). Carried through the `Result` channel so a predicate handler can `return`
/// it AFTER printing its report, without a mid-stack `process::exit` (which would skip `Drop` on locks).
#[derive(Debug)]
pub(crate) struct PredicateFalse;
impl std::fmt::Display for PredicateFalse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("predicate not satisfied")
    }
}
impl std::error::Error for PredicateFalse {}

/// The Claude Code Stop-hook "block the stop" signal (`poll --hook` with new mail): exit code 2, payload
/// already on stderr for the model. An ADAPTER contract imposed by the host, not confer's own scheme —
/// carried through `Result` like `PredicateFalse` so there's no mid-stack `process::exit`.
#[derive(Debug)]
pub(crate) struct StopHookBlock;
impl std::fmt::Display for StopHookBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("stop-hook: new mail")
    }
}
impl std::error::Error for StopHookBlock {}

/// Exit-code contract (DESIGN.md): 0 = success / report produced / predicate YES; 1 = predicate NO (a
/// valid negative, ONLY from predicate commands); 2 = usage (clap) or the Stop-hook block; 3 =
/// execution/environment error. Codes return UP through here — never `process::exit` mid-stack — so
/// clone locks and cursor state always `Drop`.
fn main() -> std::process::ExitCode {
    use std::process::ExitCode;
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is::<PredicateFalse>() => ExitCode::from(1),
        Err(e) if e.is::<StopHookBlock>() => ExitCode::from(2),
        Err(e) => {
            // Match Rust's default Result-termination output so error TEXT is unchanged; only the CODE
            // moves (1 → 3), decoupling "confer failed" from a predicate's "the answer is no".
            eprintln!("Error: {e:?}");
            ExitCode::from(3)
        }
    }
}

fn run() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Join {
            role,
            host,
            display,
            desc,
            signing_key,
            force,
        } => cmd_join(role, host, display, desc, signing_key, force),
        Cmd::Append {
            msg_type,
            text,
            summary,
            to,
            cc,
            priority,
            topic,
            reply_to,
            of,
            supersedes,
            from,
            src,
            refs,
            allow_empty_body,
            resolution,
            defer,
            allow_secret,
        } => cmd_append(AppendArgs {
            msg_type,
            text,
            summary,
            to,
            cc,
            priority,
            topic,
            reply_to,
            of,
            supersedes,
            from,
            src,
            refs,
            allow_empty_body,
            allow_secret,
            resolution,
            defer,
        }),
        Cmd::Claim { args } => cmd_lifecycle("claim", args, None),
        Cmd::Done { args, resolution } => cmd_lifecycle("done", args, resolution),
        Cmd::Error { args } => cmd_lifecycle("error", args, None),
        Cmd::Blocked { args } => cmd_lifecycle("blocked", args, None),
        Cmd::Defer { args } => cmd_lifecycle("defer", args, None),
        Cmd::Poll {
            advance,
            topic,
            hook,
            json,
            role,
            all,
            to_me,
            ..
        } => {
            let r = cmd_poll(PollArgs {
                advance,
                topic,
                hook,
                json,
                role,
                all,
                to_me,
            });
            // Hook adapters are FAIL-OPEN: `poll --hook`'s own malfunction must NEVER block the agent's
            // Stop (a transient git/IO error would otherwise wedge the session). Swallow non-signal errors
            // → exit 0 with a stderr note; the StopHookBlock signal (new mail) still propagates. (design/37)
            if hook {
                match r {
                    Err(e) if !e.is::<StopHookBlock>() => {
                        warn_safety(format!(
                            "poll --hook: {e:#} — continuing without blocking the stop (fail-open)"
                        ));
                        Ok(())
                    }
                    other => other,
                }
            } else {
                r
            }
        }
        Cmd::Show { id } => cmd_show(id),
        Cmd::Requests {
            open,
            mine,
            role,
            json,
            backlog,
            blocked,
        } => cmd_requests(open, mine, role, json, backlog, blocked),
        Cmd::Thread { id, full } => cmd_thread(id, full),
        Cmd::Init {
            url,
            dir,
            role,
            ssh,
            https,
            display,
            desc,
            signing_key,
            ssh_key,
            managed,
        } => cmd_init(
            url,
            dir,
            role,
            scheme_from(ssh, https),
            display,
            desc,
            signing_key,
            ssh_key,
            false,
            managed,
        ),
        Cmd::Clone {
            url,
            dir,
            role,
            ssh,
            https,
            display,
            desc,
            signing_key,
            ssh_key,
            managed,
        } => cmd_init(
            url,
            dir,
            role,
            scheme_from(ssh, https),
            display,
            desc,
            signing_key,
            ssh_key,
            true,
            managed,
        ),
        Cmd::Clones => cmd_clones(),
        Cmd::Hubs => cmd_hubs(),
        Cmd::Where => cmd_where(),
        Cmd::Keygen { role } => cmd_keygen(role, true),
        Cmd::Update { check } => cmd_update(check),
        Cmd::AdoptClone { path, force } => cmd_adopt_clone(path, force),
        Cmd::Invite {
            role,
            host,
            ssh,
            https,
        } => cmd_invite(role, host, scheme_from(ssh, https)),
        Cmd::Repos { json } => cmd_repos(json),
        Cmd::Verify { id } => cmd_verify(id),
        Cmd::ConfirmKey { role } => cmd_confirm_key(role),
        Cmd::Doctor { dir, fix } => cmd_doctor(dir, fix),
        Cmd::Trust { tier } => cmd_trust(tier),
        Cmd::Screen { corpus, text } => cmd_screen(corpus, text),
        Cmd::Seen { id } => cmd_seen(id),
        Cmd::Inbox { role, peek } => cmd_inbox(role, peek),
        Cmd::Ack { id, role } => cmd_ack(id, role),
        Cmd::Credential { op } => ghapp::credential(&op),
        Cmd::AppToken => {
            println!("{}", ghapp::token(&ghapp::load_config()?)?);
            Ok(())
        }
        Cmd::AppConfig {
            app_id,
            key,
            installation_id,
            find_installation,
        } => cmd_app_config(app_id, key, installation_id, find_installation),
        Cmd::InstallSkill {
            dir,
            hub,
            role,
            no_autoheal,
        } => cmd_install_skill(dir, hub, role, no_autoheal),
        Cmd::Reconnect {
            role,
            hub,
            dir,
            host,
            ssh_key,
            force,
        } => cmd_reconnect(role, hub, dir, host, ssh_key, force),
        Cmd::Onboard { role, hub } => cmd_onboard(role, hub),
        Cmd::Version { json, check, pin } => cmd_version(json, check, pin),
        Cmd::Changelog { since, all } => cmd_changelog(since, all),
        Cmd::Fleet { json } => cmd_fleet(json),
        Cmd::Require { req, bump } => cmd_require(req, bump),
        Cmd::Read {
            last,
            topic,
            full,
            json,
        } => cmd_read(last, topic, full, json),
        Cmd::Watch {
            topic,
            role,
            json,
            poll_secs,
            no_advance,
            replace,
            all,
            min_priority,
            no_version_notice,
            delivery,
            ..
        } => {
            let min_priority = match min_priority.as_str() {
                "low" => 0,
                "normal" => 1,
                "high" => 2,
                other => {
                    return Err(anyhow!(
                        "invalid --min-priority '{other}': expected low | normal | high"
                    ))
                }
            };
            watch::run(watch::WatchOpts {
                topic,
                role,
                json,
                poll_secs,
                advance: !no_advance,
                replace,
                all,
                min_priority,
                no_version_notice,
                delivery,
            })
        }
        Cmd::WatchStatus { role, json, check } => watch::cmd_watch_status(role, json, check),
        Cmd::Status => cmd_status(),
        #[cfg(feature = "dashboard")]
        Cmd::Dashboard { hub } => cmd_dashboard(hub),
        #[cfg(feature = "serve")]
        Cmd::Serve { hub, bind } => serve::run(resolve_hubs(hub)?, &bind),
        Cmd::InstallHook { project } => cmd_install_hook(project),
        Cmd::UninstallHook { project } => cmd_uninstall_hook(project),
        Cmd::SessionHeal => cmd_session_heal(),
        Cmd::Autoheal { action, yes } => cmd_autoheal(action, yes),
        Cmd::Config { action, key, value, yes } => cmd_config(action, key, value, yes),
        Cmd::Hub { action, yes } => cmd_hub(action, yes),
        Cmd::Rewatch { only } => cmd_rewatch(only),
        Cmd::Identity { role } => cmd_identity(role),
        Cmd::Whois { phrase } => cmd_whois(phrase.join(" ")),
        Cmd::Rename { name, role, force } => cmd_rename(name.join(" "), role, force),
        Cmd::Describe {
            role,
            desc,
            display,
            add_alias,
            remove_alias,
            force,
        } => cmd_describe(role, desc, display, add_alias, remove_alias, force),
        Cmd::Retire { role, permanent } => {
            cmd_set_status(role, if permanent { "retired" } else { "dormant" })
        }
        Cmd::Resume { role } => cmd_set_status(role, "active"),
        Cmd::Who => cmd_who(),
        Cmd::Leave => {
            eprintln!(
                "confer leave: not yet implemented (planned: release lease + handoff marker)"
            );
            Ok(())
        }
    }
}

/// Absolute path to the stock ssh-keygen — used to OVERRIDE a global
/// `gpg.ssh.program` (e.g. 1Password's op-ssh-sign) so signing uses the on-disk
/// agent key instead of the interactive agent. See DESIGN.md.
pub(crate) fn ssh_keygen_path() -> String {
    std::process::Command::new("which")
        .arg("ssh-keygen")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "ssh-keygen".to_string())
}

/// Shared flags for the lifecycle sugar verbs (`claim`/`done`/`error`/`blocked`/
/// `defer`). They are all thin wrappers over `append --type <verb>`, so they accept
/// the same addressing as `append` — add a flag here once and every verb gains it.
/// With no `--to`/`--cc`, the update auto-addresses the request's author (via `--of`),
/// so `done --of X` already reaches the opener; `--to`/`--reply-to` override that.
#[derive(clap::Args)]
pub(crate) struct LifecycleArgs {
    /// the request id this update is about
    #[arg(long)]
    of: String,
    /// one-line summary (a sensible default is used if omitted)
    #[arg(long)]
    summary: Option<String>,
    /// optional explanatory body (`-` reads stdin) — for a substantive close/claim
    /// without dropping to `append --type`
    #[arg(long)]
    text: Option<String>,
    /// act as this role (default: the resolved role for this hub)
    #[arg(long)]
    from: Option<String>,
    /// address the update to specific roles (default: the request's author)
    #[arg(long)]
    to: Vec<String>,
    /// secondary audience (FYI)
    #[arg(long)]
    cc: Vec<String>,
    /// reply within a thread — with no `--to`, addresses the replied-to author
    #[arg(long = "reply-to")]
    reply_to: Option<String>,
    /// point at a durable doc/artifact that resolves this: `repo:path[@sha][#Lstart-Lend]`;
    /// repeatable. A good `done` often points at what actually resolved the request (field report:
    /// the sugar verbs used to drop `--ref`, forcing a fallback to `append --type done`).
    #[arg(long = "ref")]
    refs: Vec<String>,
}

struct PollArgs {
    advance: bool,
    topic: Option<String>,
    hook: bool,
    json: bool,
    role: Option<String>,
    all: bool,
    to_me: bool,
}

fn cmd_poll(p: PollArgs) -> Result<()> {
    let root = config::repo_root()?;
    let me = config::resolve_role(p.role.clone(), &root).unwrap_or_default();
    // If you armed a watch but it isn't live, a poll won't fix that — surface it (poll-only agents,
    // which never armed one, are not nagged; the check is gated on a prior watch).
    warn_if_watch_should_be_live(&root, &me);
    // Fetch the hub first — otherwise the whole non-Monitor fallback is blind (B2).
    if let Err(e) = gitcmd::integrate(&root) {
        warn_safety(format!("hub sync failed ({e}); showing local state"));
    }
    let hub = config::hub_key(&root);
    let roster = roster::load(&root);
    let since = cursor::load(&hub, &me)?;

    // A filtered/firehose view must not move the shared cursor (B1).
    let filtered = p.topic.is_some() || p.to_me || p.all;
    if p.advance && filtered {
        return Err(anyhow!(
            "--advance is only allowed on an unfiltered poll (filtered/firehose views must not move the shared cursor)"
        ));
    }

    // Commit-ordered incremental read: only messages added since the cursor.
    let grps = groups::load(&root);
    let msgs = store::messages_since(&root, since.as_deref())?;
    let new: Vec<&Message> = msgs
        .iter()
        .filter(|m| relevant(m, &me, &p, &grps))
        .collect();

    // Stop-hook mode reads STDERR on exit 2; normal mode writes stdout (M2).
    let mut out: Box<dyn Write> = if p.hook {
        Box::new(std::io::stderr())
    } else {
        Box::new(std::io::stdout())
    };
    let mut vc = verify::Cache::default();
    for m in &new {
        let line = if p.json {
            to_json(m)?
        } else {
            let t = verify::status(&root, &hub, &roster, &mut vc, m);
            format_line(&roster, m, true, Some(&t))
        };
        writeln!(out, "{line}")?;
    }
    drop(out);

    // An unfiltered poll consumes the whole actionable stream, so it's caught up
    // to HEAD; non-actionable notes remain browsable via `read`/`--all` (B1).
    if p.advance {
        // Anchor at the last stable pushed ancestor of HEAD, not local HEAD (R3).
        if let Some(anchor) = gitcmd::cursor_anchor(&root) {
            cursor::save(&hub, &me, &anchor)?;
        }
        // NOTE: poll advances the DELIVERY cursor only — it does NOT mark directly-addressed mail
        // read. Delivery ≠ read: a request stays in your inbox until you `show`/`ack` it, so a
        // polling loop can't silently clear mail it merely streamed past (inbox.rs).
    }
    if p.hook && !new.is_empty() {
        // Claude Code Stop-hook protocol: exit 2 = block the stop, the payload (already on stderr in
        // hook mode) is fed to the model. Signalled via a marker so `main` sets the code — no mid-stack
        // process::exit. (design/37 — this is an ADAPTER contract, not confer's own exit scheme.)
        return Err(StopHookBlock.into());
    }
    Ok(())
}

/// Is a message relevant to a poll/watch consumer, given its filters?
/// Surfaces actionable items AND anything addressed to me (role/group/`all`) —
/// a message directed at me must never be invisible.
fn relevant(m: &Message, me: &str, p: &PollArgs, groups: &groups::Groups) -> bool {
    m.front.from != me
        && p.topic
            .as_ref()
            .is_none_or(|t| m.front.topic.as_deref() == Some(t.as_str()))
        && (p.all || is_actionable(m) || groups::addressed(m, me, groups))
        && (!p.to_me || groups::addressed(m, me, groups))
}

/// Wrap a rendered body in the untrusted-data envelope, annotating it with the heuristic
/// screen's verdict (⚠) computed from the RAW body — not the framed markdown, whose
/// `---\nfrom:` frontmatter would self-trigger format-injection. DESIGN.md §2 + §3.
fn framed_body(
    display_md: &str,
    m: &Message,
    who: &str,
    trust: &verify::Trust,
    tier: Option<tiers::Tier>,
) -> String {
    let v = screen::heuristic(&screen::Input {
        body: &m.body,
        from_role: &m.front.from,
        tier,
        refs: vec![],
    });
    let note = match v.level {
        screen::Level::Allow => None,
        _ => Some(format!(
            "⚠ possible injection ({})",
            v.category.unwrap_or("?")
        )),
    };
    envelope::frame(display_md, who, &m.front.from, trust, tier, note.as_deref())
}

/// Ids that have been superseded (some message's `supersedes` points at them).
fn superseded_set(msgs: &[Message]) -> HashSet<String> {
    let mut s = HashSet::new();
    for m in msgs {
        if let Some(sup) = &m.front.supersedes {
            if let Some(t) = msgs.iter().find(|x| id_ref_matches(&x.front.id, sup)) {
                s.insert(t.front.id.clone());
            }
        }
    }
    s
}

/// Resolve the hubs a viewer (dashboard/serve) should show: explicit `--hub` paths
/// (with a leading `~` expanded), else the current hub if we're in one (the common
/// case — one predictable view), else every followed hub in the pruned registry.
#[cfg(any(feature = "dashboard", feature = "serve"))]
fn resolve_hubs(hub: Vec<String>) -> Result<Vec<std::path::PathBuf>> {
    if !hub.is_empty() {
        let home = config::home().ok();
        return Ok(hub
            .into_iter()
            .map(|h| match (h.strip_prefix("~/"), &home) {
                (Some(rest), Some(home)) => home.join(rest),
                _ => std::path::PathBuf::from(h),
            })
            .collect());
    }
    match config::repo_root() {
        Ok(cwd) => Ok(vec![cwd]),
        Err(_) => {
            let ds = crosshub::hub_dirs();
            if ds.is_empty() {
                anyhow::bail!("no hubs found — run inside a hub clone or pass --hub <dir>");
            }
            Ok(ds)
        }
    }
}

/// Launch the live TUI dashboard over the resolved hubs.
#[cfg(feature = "dashboard")]
fn cmd_dashboard(hub: Vec<String>) -> Result<()> {
    dashboard::run(resolve_hubs(hub)?)
}

/// Set or show the GitHub App config used by `confer credential`.
fn cmd_app_config(
    app_id: Option<String>,
    key: Option<String>,
    installation_id: Option<u64>,
    find_installation: bool,
) -> Result<()> {
    let mut c = ghapp::load_config().unwrap_or_default();
    let mut changed = false;
    if let Some(a) = app_id {
        c.app_id = a;
        changed = true;
    }
    if let Some(k) = key {
        c.key_path = k;
        changed = true;
    }
    if let Some(i) = installation_id {
        c.installation_id = Some(i);
        changed = true;
    }
    // Persist app_id/key FIRST so they survive even if the App isn't installed yet
    // (find-installation can then be re-run once it is).
    if changed {
        ghapp::save_config(&c)?;
    }
    if find_installation {
        match ghapp::find_installation(&c) {
            Ok(id) => {
                println!("found installation id: {id}");
                c.installation_id = Some(id);
                ghapp::save_config(&c)?;
            }
            Err(e) => eprintln!(
                "confer: {e}\n(config saved; install the App on your repos, then re-run `confer app-config --find-installation`)"
            ),
        }
    }
    println!(
        "app_id:          {}\nkey:             {}\ninstallation_id: {}",
        if c.app_id.is_empty() {
            "(unset)"
        } else {
            &c.app_id
        },
        if c.key_path.is_empty() {
            "(unset)"
        } else {
            &c.key_path
        },
        c.installation_id
            .map(|i| i.to_string())
            .unwrap_or_else(|| "(unset)".into()),
    );
    if !changed {
        println!("\nwire the credential helper: git config credential.\"https://github.com\".helper \"!confer credential\"");
    }
    Ok(())
}

/// This running binary's build identity (semver from Cargo + short git sha).
pub(crate) fn my_build() -> version::BuildId {
    version::BuildId {
        version: semver::Version::parse(env!("CARGO_PKG_VERSION")).ok(),
        sha: BUILD_SHA.to_string(),
    }
}

/// The hub's pinned build id from `.confer-version`, if present + non-empty.
pub(crate) fn hub_pin(root: &std::path::Path) -> Option<version::BuildId> {
    let raw = std::fs::read_to_string(root.join(".confer-version")).ok()?;
    if raw.trim().is_empty() {
        return None;
    }
    Some(version::BuildId::parse(&raw))
}

/// The hub's version REQUIREMENT floor/range from `.confer-require` (a semver
/// `VersionReq` like `>=0.1.0`) — the fuzzy repo-level compatibility contract. Agents
/// report exact builds; this is what they're audited against. None if unset/unparseable.
pub(crate) fn hub_require(root: &std::path::Path) -> Option<semver::VersionReq> {
    let raw = std::fs::read_to_string(root.join(".confer-require")).ok()?;
    semver::VersionReq::parse(raw.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_hub::short12;
    use crate::identity::parse_card;
    use crate::join::published_pubkey;
    use crate::projection::request_status;
    use crate::transport::{clone_url_candidates, parse_remote, Scheme};

    #[test]
    fn short12_is_char_boundary_safe() {
        // A tampered/corrupt known_hubs.json can carry a non-hex, multibyte root/tip; short12 must
        // never panic on a byte-slice off a char boundary (the release-blocker the red-team found).
        assert_eq!(short12("0123456789abcdef"), "0123456789ab"); // 12 ascii hex
        assert_eq!(short12("abc"), "abc"); // shorter than 12
        let multibyte = "aaaaaaaaaaaé"; // 11 ascii + a 2-byte char → byte index 12 is mid-character
        assert_eq!(short12(multibyte), multibyte); // falls back to the whole string, no panic
    }

    #[test]
    fn slug_rules() {
        for ok in ["carol", "cover-restoration", "a1", "x", "all"] {
            assert!(valid_slug(ok), "{ok} should be valid");
        }
        for bad in [
            "",
            "-x",
            "A",
            "a/b",
            "../x",
            "a b",
            "a_b",
            "a.",
            &"a".repeat(65),
        ] {
            assert!(!valid_slug(bad), "{bad:?} should be invalid");
        }
    }

    #[test]
    fn all_is_a_valid_slug_but_reserved_as_an_identity() {
        // `all` passes the slug rule (so it works as a --to target = broadcast)…
        assert!(valid_slug("all"));
        // …but is reserved as an identity (role/topic/group) to avoid collision.
        assert!(is_reserved_name("all"));
        assert!(!is_reserved_name("carol"));
    }

    #[test]
    fn truncate_clips_at_word_boundary_not_midword() {
        let s = "alpha beta gamma delta epsilon zeta"; // 35 chars
        assert_eq!(truncate(s, 100), s); // under limit → unchanged
        let t = truncate(s, 14); // "alpha beta gamma"… would clip mid-word "gam|ma"
        assert!(t.ends_with('…'));
        assert!(!t.contains("gam…"), "must not clip mid-word: {t}");
        assert!(t.starts_with("alpha beta"));
        // one giant word with no spaces → hard cut (can't back off to a boundary)
        let big = "supercalifragilisticexpialidocious";
        assert_eq!(truncate(big, 5), "super…");
    }

    #[test]
    fn short_id_takes_trailing_six() {
        assert_eq!(short_id("01J8Z9K3QH7X4Q9W0C"), "4Q9W0C");
        assert_eq!(short_id("abc"), "abc");
    }

    #[test]
    fn id_match_prefix_suffix_and_exact() {
        let full = "01J8Z9K3QH7X4Q9W0C";
        assert!(id_matches(full, full));
        assert!(id_matches(full, "4Q9W0C")); // trailing (what output shows)
        assert!(id_matches(full, "01J8Z9")); // leading
        assert!(!id_matches(full, "ZZZ"));
    }

    fn tmsg(msg_type: &str, id: &str, of: Option<&str>) -> Message {
        Message {
            front: Frontmatter {
                id: id.into(),
                from: "x".into(),
                msg_type: msg_type.into(),
                ts: "t".into(),
                host: None,
                to: vec![],
                cc: vec![],
                priority: None,
                topic: None,
                reply_to: None,
                of: of.map(String::from),
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
    fn accessible_to_empty_is_hubwide_else_listed() {
        use crate::repos::{accessible_to, Repo};
        let open = Repo::default(); // empty access
        assert!(accessible_to(&open, "anyone"));
        let restricted = Repo {
            access: vec!["bob".into(), "alice".into()],
            ..Default::default()
        };
        assert!(accessible_to(&restricted, "bob"));
        assert!(!accessible_to(&restricted, "sister-bot"));
        let everyone = Repo {
            access: vec!["all".into()],
            ..Default::default()
        };
        assert!(accessible_to(&everyone, "sister-bot"));
    }

    #[test]
    fn parse_remote_canonicalizes_github_forms() {
        let ssh = "git@github.com:codeshrew/team-hub.git";
        let https = "https://github.com/codeshrew/team-hub.git";
        for input in [
            ssh,
            https,
            "https://github.com/codeshrew/team-hub",
            "codeshrew/team-hub",
        ] {
            let r = parse_remote(input);
            assert_eq!(r.ssh.as_deref(), Some(ssh), "ssh from {input}");
            assert_eq!(r.https.as_deref(), Some(https), "https from {input}");
            assert_eq!(
                r.shorthand.as_deref(),
                Some("codeshrew/team-hub"),
                "shorthand from {input}"
            );
        }
        // non-GitHub host: still splits both schemes, but no shorthand
        let gl = parse_remote("git@gitlab.com:team/hub.git");
        assert_eq!(gl.ssh.as_deref(), Some("git@gitlab.com:team/hub.git"));
        assert_eq!(gl.shorthand, None);
        // local path / unrecognized: pass through as raw, no alternate scheme
        let local = parse_remote("/srv/hubs/team-hub.git");
        assert_eq!(local.ssh, None);
        assert_eq!(local.https, None);
        assert_eq!(local.raw, "/srv/hubs/team-hub.git");
    }

    #[test]
    fn parse_card_fails_closed_on_unparsable_frontmatter() {
        // A well-formed card parses.
        let (m, body) = parse_card("---\ndisplay: alice\npubkey: ssh-ed25519 AAA\n---\nhello").unwrap();
        assert_eq!(m.get("display").and_then(|v| v.as_str()), Some("alice"));
        assert_eq!(body, "hello");
        // No frontmatter fence → legitimately empty, Ok.
        let (m, _) = parse_card("just a body, no fence").unwrap();
        assert!(m.is_empty());
        // Empty fence → empty, Ok.
        let (m, _) = parse_card("---\n---\n").unwrap();
        assert!(m.is_empty());
        // CRITICAL: a duplicate key (or any unparsable frontmatter) must ERR, never degrade to an
        // empty map — that was the identity-hijack bypass of the write-side 1:1 key guard.
        assert!(
            parse_card("---\ndisplay: a\ndisplay: b\n---\n").is_err(),
            "duplicate frontmatter key must fail closed, not read as empty"
        );
        assert!(
            parse_card("---\n: : : not yaml\n---\n").is_err(),
            "malformed frontmatter must fail closed"
        );
        // A leading UTF-8 BOM must NOT hide the frontmatter (that misread a keyed card as key-less —
        // red-team): a BOM'd card parses its frontmatter normally.
        let (m, _) = parse_card("\u{FEFF}---\npubkey: ssh-ed25519 AAA\ndisplay: v\n---\n").unwrap();
        assert_eq!(
            m.get("pubkey").and_then(|v| v.as_str()),
            Some("ssh-ed25519 AAA"),
            "a BOM before --- must not hide the pubkey"
        );
    }

    #[test]
    fn published_pubkey_classifies_and_fails_closed_on_type_confusion() {
        let pk = |yaml: &str| {
            let m: serde_yaml::Mapping = serde_yaml::from_str(yaml).unwrap();
            published_pubkey(&m)
        };
        // A real key string reads as published.
        assert_eq!(
            pk("pubkey: ssh-ed25519 AAAA").unwrap(),
            Some("ssh-ed25519 AAAA".to_string())
        );
        // Absent / null / empty-string are legit "no key here" placeholders (Ok(None)).
        assert_eq!(pk("display: x").unwrap(), None);
        assert_eq!(pk("pubkey: null").unwrap(), None);
        assert_eq!(pk("pubkey: \"\"").unwrap(), None);
        // Non-string, non-null types are never legit → fail closed (the type-confusion bypass).
        assert!(pk("pubkey: [a, b]").is_err(), "list pubkey must fail closed");
        assert!(pk("pubkey: 123").is_err(), "number pubkey must fail closed");
        assert!(pk("pubkey: true").is_err(), "bool pubkey must fail closed");
    }

    #[test]
    fn canonical_hub_id_matches_same_hub_across_scheme_case_host() {
        // Same GitHub repo across ssh / https / shorthand / trailing-slash / .git / CASE → one id.
        let want = Some("github.com/codeshrew/confer-lab".to_string());
        for input in [
            "git@github.com:codeshrew/confer-lab.git",
            "https://github.com/codeshrew/confer-lab",
            "https://github.com/codeshrew/confer-lab.git/",
            "codeshrew/confer-lab",
            "https://github.com/CodeShrew/Confer-Lab", // GitHub paths are case-insensitive (red-team #2)
        ] {
            assert_eq!(canonical_hub_id(input), want, "canonical of {input}");
        }
        // NON-github host must ALSO normalize ssh vs https of the SAME repo (red-team #2: the old
        // matcher only handled github.com, so self-hosted hubs never matched themselves).
        assert_eq!(
            canonical_hub_id("git@git.example.com:org/hub.git"),
            canonical_hub_id("https://git.example.com/org/hub"),
            "self-hosted ssh vs https must match"
        );
        assert_eq!(
            canonical_hub_id("ssh://git@git.example.com:2222/org/hub.git"),
            Some("git.example.com/org/hub".to_string()),
            ":port and user@ are stripped"
        );
    }

    #[test]
    fn canonical_hub_id_does_not_false_match_different_hubs() {
        // Different org / host → distinct ids (never a cross-fleet mismatch).
        assert_ne!(
            canonical_hub_id("orgA/hub"),
            canonical_hub_id("orgB/hub"),
            "different org must not match"
        );
        assert_ne!(
            canonical_hub_id("git@github.com:o/hub.git"),
            canonical_hub_id("git@gitlab.com:o/hub.git"),
            "different host must not match"
        );
        // red-team #1: local-path fallback must be EXACT, never a suffix test. A different hub that
        // merely shares a basename, or a bare word that is a raw suffix, must NOT match.
        let real = canonical_hub_id("/srv/hubs/myhub.git");
        assert_ne!(real, canonical_hub_id("/other/place/myhub.git"), "same basename, different path");
        assert_eq!(canonical_hub_id("myhub"), None, "a bare non-owner/repo word is not a hub ref");
        assert_ne!(real, canonical_hub_id("/srv/hubs/aaamyhub.git"), "aaamyhub must not match myhub");
        assert_ne!(real, canonical_hub_id("/srv/hubs/notmyhub.git"), "notmyhub must not match myhub");
    }

    #[test]
    fn clone_url_candidates_honor_typed_scheme() {
        // explicit https URL → https origin first, ssh as fallback
        let r = parse_remote("https://github.com/o/repo.git");
        let c = clone_url_candidates("https://github.com/o/repo.git", &r, Scheme::Auto);
        assert_eq!(c[0], "https://github.com/o/repo.git");
        assert_eq!(c[1], "git@github.com:o/repo.git");
        // explicit ssh URL → ssh first
        let r2 = parse_remote("git@github.com:o/repo.git");
        let c2 = clone_url_candidates("git@github.com:o/repo.git", &r2, Scheme::Auto);
        assert_eq!(c2[0], "git@github.com:o/repo.git");
        assert_eq!(c2[1], "https://github.com/o/repo.git");
        // an explicit --https flag forces https only (no fallback), overriding the URL
        let c3 = clone_url_candidates("git@github.com:o/repo.git", &r2, Scheme::Https);
        assert_eq!(c3, vec!["https://github.com/o/repo.git".to_string()]);
        // bare shorthand → prefer-ssh ordering (both schemes present)
        let r4 = parse_remote("o/repo");
        assert_eq!(clone_url_candidates("o/repo", &r4, Scheme::Auto).len(), 2);
    }

    #[test]
    fn clone_candidates_respect_scheme_and_fallback() {
        let r = parse_remote("codeshrew/team-hub");
        assert_eq!(
            clone_candidates(&r, Scheme::Ssh),
            vec![r.ssh.clone().unwrap()]
        );
        assert_eq!(
            clone_candidates(&r, Scheme::Https),
            vec![r.https.clone().unwrap()]
        );
        // Auto always yields both (order is a hint; fallback is the guarantee)
        assert_eq!(clone_candidates(&r, Scheme::Auto).len(), 2);
        // local path: only the raw candidate, no fallback
        let local = parse_remote("/srv/hubs/x.git");
        assert_eq!(
            clone_candidates(&local, Scheme::Auto),
            vec!["/srv/hubs/x.git".to_string()]
        );
    }

    #[test]
    fn empty_reference_folds_against_nothing() {
        // C1: an empty `of`/`supersedes` must not touch any request.
        let a = "01AAAAAAAAAAAAAAAAAAAAAREQ1";
        let done = tmsg("done", "01DDDDDDDDDDDDDDDDDDDDDDON1", Some(""));
        let mut sup = tmsg("supersede", "01SSSSSSSSSSSSSSSSSSSSSSUP1", None);
        sup.front.supersedes = Some(String::new());
        let msgs = vec![tmsg("request", a, None), done, sup];
        assert_eq!(request_status(&msgs, a), "OPEN");
        assert!(claimants(&msgs, a).is_empty());
        assert!(superseded_set(&msgs).is_empty());
    }

    #[test]
    fn leading_prefix_does_not_crosscontaminate() {
        // C2: two ids sharing an 8-char ULID timestamp prefix; folds must not bleed.
        let a = "01KX2YTCAX0000000000000001";
        let b = "01KX2YTCKY0000000000000002";
        // a `done` on the FULL id of a closes only a.
        let full = vec![
            tmsg("request", a, None),
            tmsg("request", b, None),
            tmsg("done", "01DDDDDDDDDDDDDDDDDDDDDDON1", Some(a)),
        ];
        assert_eq!(request_status(&full, a), "DONE");
        assert_eq!(request_status(&full, b), "OPEN");
        // a bare shared prefix as a reference folds against NEITHER (strict: no leading).
        let pfx = vec![
            tmsg("request", a, None),
            tmsg("request", b, None),
            tmsg("done", "01DDDDDDDDDDDDDDDDDDDDDDON2", Some("01KX2YTC")),
        ];
        assert_eq!(request_status(&pfx, a), "OPEN");
        assert_eq!(request_status(&pfx, b), "OPEN");
    }

    #[test]
    fn id_ref_matches_is_strict_but_id_matches_is_lenient() {
        let full = "01KX2YTCAX0000000000000001";
        assert!(id_ref_matches(full, full)); // exact
        assert!(id_ref_matches(full, "0000000000000001")); // suffix ≥8
        assert!(!id_ref_matches(full, "")); // empty never
        assert!(!id_ref_matches(full, "01KX2YTC")); // leading prefix rejected
        assert!(!id_ref_matches(full, "0001")); // suffix <8 rejected
        assert!(!id_matches(full, "")); // C1 guard on the lenient matcher too
        assert!(id_matches(full, "01KX2YTC")); // lenient still allows prefix (user query)
    }

    #[test]
    fn resolve_unique_errors_on_ambiguity_and_miss() {
        let a = "01KX2YTCAX0000000000000001";
        let b = "01KX2YTCKY0000000000000002";
        let msgs = vec![tmsg("request", a, None), tmsg("request", b, None)];
        assert_eq!(resolve_unique(&msgs, a).unwrap(), a);
        assert!(resolve_unique(&msgs, "01KX2YTC").is_err()); // ambiguous shared prefix
        assert!(resolve_unique(&msgs, "zzzzzz").is_err()); // no match
    }

    #[test]
    fn claimants_lists_distinct_roles_in_fold_order() {
        // `of` stores full ids (resolve produces them); folds match exactly.
        let req = "01AAAAAAAAAAAAAAAAAAAAAREQ1";
        let msgs = vec![
            tmsg("request", req, None),
            {
                let mut m = tmsg("claim", "01C1CCCCCCCCCCCCCCCCCCCLM1", Some(req));
                m.front.from = "carol".into();
                m
            },
            {
                let mut m = tmsg("claim", "01C2CCCCCCCCCCCCCCCCCCCLM2", Some(req));
                m.front.from = "bob".into();
                m
            },
            {
                // a duplicate claim by the same role must not double-count
                let mut m = tmsg("claim", "01C3CCCCCCCCCCCCCCCCCCCLM3", Some(req));
                m.front.from = "carol".into();
                m
            },
        ];
        // owner (first) = carol; contested by bob; carol appears once.
        assert_eq!(claimants(&msgs, req), vec!["carol", "bob"]);
        assert!(claimants(&msgs, "01ZZZZZZZZZZZZZZZZZZZZZNONE").is_empty());
    }

    #[test]
    fn request_status_folds_done_over_claim_over_open() {
        let r1 = "01AAAAAAAAAAAAAAAAAAAAAREQ1";
        let r2 = "01BBBBBBBBBBBBBBBBBBBBBREQ2";
        let msgs = vec![
            tmsg("request", r1, None),
            tmsg("claim", "01C1CCCCCCCCCCCCCCCCCCCLM1", Some(r1)),
            tmsg("done", "01D1DDDDDDDDDDDDDDDDDDDON1", Some(r1)),
            tmsg("request", r2, None),
            tmsg("claim", "01C2CCCCCCCCCCCCCCCCCCCLM2", Some(r2)),
        ];
        assert_eq!(request_status(&msgs, r1), "DONE");
        assert_eq!(request_status(&msgs, r2), "CLAIMED");
        assert_eq!(request_status(&msgs, "01ZZZZZZZZZZZZZZZZZZZZZREQ3"), "OPEN");
    }
}
