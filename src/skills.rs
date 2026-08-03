//! Installing confer's Claude Code skills (/confer-watch, /confer-poll) and the tier-1 auto-resync
//! that keeps them current with the running binary.
//!
//! `cmd_install_skill` writes each skill in CONFER_SKILLS (from `templates`) with the machine's binary
//! path baked in, and (unless opted out) arms the SessionStart auto-heal hook. `resync_skills_if_stale`
//! is the SessionStart-time counterpart: if skills already exist but were baked from a different build,
//! silently re-derive them — never creating skills where none exist.

use crate::templates::{CODEX_POLL_SKILL, CONFER_SKILLS};
use crate::{
    autoheal, config,
    hooks::{write_codex_hook, write_grok_hook, write_session_hook},
    BUILD_SHA,
};
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

/// Agent harnesses confer installs skills for, and their global SKILLS dir under $HOME (design/52
/// axis 3). Extend this to add a harness — both install and resync read it. NOTE: Codex's skills live
/// under `~/.agents/skills` (its 0.146 discovery table), a DIFFERENT root than its hooks
/// (`~/.codex/hooks.json` — see `hooks::write_codex_hook`); design/54 axis 3b decouples the two.
pub(crate) const HARNESS_SKILL_HOMES: &[(&str, &str)] =
    &[("claude", ".claude"), ("grok", ".grok"), ("codex", ".agents")];

/// The skill that every harness installs — used as the resync sentinel + build-marker holder. Codex
/// omits the Monitor-based `confer-arm`/`confer-watch` (design/54), so `confer-watch` can't be the
/// sentinel; `confer-poll` is present in every harness's set.
fn sentinel_skill(harness: &str) -> &'static str {
    if harness == "codex" { "confer-poll" } else { "confer-watch" }
}

/// The skills to install for `harness`, as (name, template). Delivery is a harness CAPABILITY
/// (design/54 axis 10): Claude/Grok get the Monitor-reactive set; Codex has no idle wake, so it OMITS
/// the Monitor-only `confer-arm` and the reactive `confer-watch`, and swaps in a poll-first
/// `confer-poll`. All other skills are shared (rewritten per harness by `harness_rewrite`).
fn skills_for(harness: &str) -> Vec<(&'static str, &'static str)> {
    if harness == "codex" {
        CONFER_SKILLS
            .iter()
            .filter(|(n, _)| *n != "confer-arm" && *n != "confer-watch")
            .map(|(n, t)| if *n == "confer-poll" { (*n, CODEX_POLL_SKILL) } else { (*n, *t) })
            .collect()
    } else {
        CONFER_SKILLS.to_vec()
    }
}

/// The skills dir for `harness` under `home`, if it's a known harness.
fn harness_skill_dir(home: &Path, harness: &str) -> Option<PathBuf> {
    HARNESS_SKILL_HOMES
        .iter()
        .find(|(h, _)| *h == harness)
        .map(|(_, sub)| home.join(sub).join("skills"))
}

/// The harness running THIS process (design/52): Grok Build sets `GROK_AGENT`; a live Codex turn sets
/// `CODEX_THREAD_ID` (codex field-confirmed 2026-08-03 as the reliable marker — `CODEX_HOME` is often
/// UNSET), with `CODEX_HOME` as a secondary signal; default Claude Code. Best-effort — explicit
/// `--harness codex` always wins, and `CODEX_THREAD_ID` is undocumented so it's a heuristic, not an
/// API. Auto-resync doesn't depend on this (it rewrites each installed harness dir by its own name),
/// so a missed Codex auto-detect only affects `--harness auto`, never a codex dir that's installed.
pub(crate) fn detect_harness() -> &'static str {
    let set = |k: &str| std::env::var(k).ok().filter(|s| !s.is_empty()).is_some();
    if set("GROK_AGENT") {
        "grok"
    } else if set("CODEX_THREAD_ID") || set("CODEX_HOME") {
        "codex"
    } else {
        "claude"
    }
}

/// Drop the `allowed-tools:` / `disallowed-tools:` frontmatter lines — Codex skill frontmatter takes
/// only `name` + `description` (design/54 axis 4); the tool vocabulary lives in the body.
fn strip_tool_frontmatter(text: &str) -> String {
    let mut out = text
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("allowed-tools:") && !t.starts_with("disallowed-tools:")
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Rewrite a (Claude-authored) skill for another harness's tool vocabulary + loop floor (design/52
/// axes 4/6). The templates are authored for Claude Code; for another harness, map the tool NAMES the
/// skill declares/references so they match that runtime's tools. Whole-token replacements — in these
/// skills `Monitor`/`Bash`/`AskUserQuestion` only ever name the tools. (Grok's `allowed-tools` is
/// guidance, not a hard sandbox, so the `confer-arm` no-shell guarantee is enforced by confer's
/// runtime backgrounded-watch check, not this frontmatter — design/52 §open-Q2.) NOTE: Claude-only
/// `` !`cmd` `` inline-exec blocks are NOT yet rewritten — a follow-up (grok field-testing whether
/// they bite); the agent can still run the command shown.
fn harness_rewrite(text: &str, harness: &str) -> String {
    match harness {
        "grok" => text
            .replace("Monitor", "monitor")
            .replace("Bash", "run_terminal_command")
            .replace("AskUserQuestion", "ask_user_question")
            .replace("/loop 45s", "/loop 60s")
            // Claude's `!`cmd`` auto-exec syntax is inert on Grok (field-confirmed) — neutralize it to
            // plain `cmd` inline code the agent runs itself.
            .replace("!`", "`"),
        // Codex (design/54): frontmatter is name+description only (drop tool decls), and the Claude
        // `!`cmd`` auto-exec is inert → plain inline code the agent runs via `exec_command`. Delivery
        // skills are handled by `skills_for` (poll-first `confer-poll`; no Monitor `confer-arm`/watch),
        // so no Monitor/`/loop` token-mangling is needed here.
        "codex" => strip_tool_frontmatter(&text.replace("!`", "`")),
        _ => text.to_string(), // claude = the templates as authored (the identity)
    }
}

/// Re-derive the confer skills in ONE harness dir if they exist there but were baked from a different
/// build. Returns whether it acted. Never creates skills where none exist; bails (role-blind safety)
/// if a template unexpectedly bakes {ROLE}/{HUB}.
fn resync_dir(dir: &Path, bin: &str, harness: &str) -> bool {
    let sentinel = sentinel_skill(harness);
    if !dir.join(sentinel).join("SKILL.md").is_file() {
        return false; // not installed here → not ours to create
    }
    let marker = dir.join(sentinel).join(".confer-build");
    if std::fs::read_to_string(&marker).unwrap_or_default().trim() == BUILD_SHA {
        return false; // already current — cheap stat+read
    }
    for (name, tmpl) in skills_for(harness) {
        let filled = harness_rewrite(&tmpl.replace("{CONFER}", bin), harness);
        if filled.contains("{ROLE}") || filled.contains("{HUB}") {
            return false; // role-blind resync must not write a role/hub-baked skill (design/32)
        }
        let d = dir.join(name);
        if std::fs::create_dir_all(&d).is_err() || std::fs::write(d.join("SKILL.md"), filled).is_err() {
            return false;
        }
    }
    let _ = std::fs::write(&marker, BUILD_SHA);
    true
}

/// Tier-1 auto-heal: refresh confer skills in EVERY installed harness dir, not just ~/.claude/skills
/// (design/52 axis 3 — a Grok-only or dual install must self-heal too). Silently re-derives any baked
/// from a different build; SessionStart runs the NEW binary and skills are a pure function of it, so
/// it's safe with zero agent action. NEVER creates skills where none exist (a fresh install is an
/// explicit `install-skill`); a custom `--dir` install stays the agent's own to manage. Returns the
/// build synced to if any dir acted.
pub(crate) fn resync_skills_if_stale() -> Option<String> {
    let home = config::home().ok()?;
    let bin = std::env::current_exe().ok()?.to_string_lossy().to_string();
    let mut acted = false;
    for (harness, sub) in HARNESS_SKILL_HOMES {
        acted |= resync_dir(&home.join(sub).join("skills"), &bin, harness);
    }
    acted.then(|| BUILD_SHA.to_string())
}

pub(crate) fn cmd_install_skill(
    dir: Option<String>,
    harness: Option<String>,
    hub: Option<String>,
    role: Option<String>,
    no_autoheal: bool,
) -> Result<()> {
    let bin = std::env::current_exe()?.to_string_lossy().to_string();
    let hub_root = match hub {
        Some(h) => std::fs::canonicalize(&h).unwrap_or_else(|_| std::path::PathBuf::from(h)),
        None => config::repo_root()?,
    };
    let role = match role {
        Some(r) => r,
        None => config::resolve_role(None, &hub_root)?,
    };
    let home = config::home()?;
    // WHICH skill dir(s) to write. `--dir` is an explicit single override (back-compat,
    // harness-agnostic — the agent's own placement). Else `--harness` selects: `auto` (default) = the
    // runtime detected from the env (Claude, or Grok via GROK_AGENT); `claude`/`grok` = that one;
    // `all` = every known harness (design/52 axis 3). A coordination skill is cross-project infra, so
    // it lives in the harness's GLOBAL skills dir (Grok: ~/.grok/skills; Claude: ~/.claude/skills) —
    // writing into the hub repo would hide it from a session living in its own code repo.
    let targets: Vec<(&str, PathBuf)> = if let Some(d) = dir {
        // a --dir install gets THIS runtime's vocabulary (the placing agent is running under one).
        vec![(detect_harness(), PathBuf::from(d))]
    } else {
        match harness.as_deref().unwrap_or("auto") {
            "all" => HARNESS_SKILL_HOMES.iter().map(|(h, s)| (*h, home.join(s).join("skills"))).collect(),
            "auto" => {
                let h = detect_harness();
                vec![(h, harness_skill_dir(&home, h).expect("the detected harness is always known"))]
            }
            want => match HARNESS_SKILL_HOMES.iter().find(|(h, _)| *h == want) {
                Some((h, s)) => vec![(*h, home.join(s).join("skills"))],
                None => {
                    return Err(anyhow!("unknown --harness '{want}' — expected auto | claude | grok | codex | all"))
                }
            },
        }
    };
    let base_fill = |t: &str| {
        t.replace("{CONFER}", &bin)
            .replace("{HUB}", &hub_root.to_string_lossy())
            .replace("{ROLE}", &role)
    };

    // ONE generic skill set, role-agnostic (commands resolve the caller's role from the hub clone
    // they run in), so co-resident agents don't clobber each other (design/32) — only {CONFER} (the
    // shared binary path) is baked. Written to each selected harness dir.
    for (harness, dir) in &targets {
        let skills = skills_for(harness);
        for (name, tmpl) in &skills {
            let d = dir.join(name);
            std::fs::create_dir_all(&d)?;
            std::fs::write(d.join("SKILL.md"), harness_rewrite(&base_fill(tmpl), harness))?;
        }
        // Stamp the build so the SessionStart tier-1 auto-heal can tell, cheaply, when a later binary
        // update left these stale and silently re-derive them.
        let _ = std::fs::write(dir.join(sentinel_skill(harness)).join(".confer-build"), BUILD_SHA);
        let names = skills.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(",");
        println!("wrote {}/{{{names}}}/SKILL.md", dir.display());
        // Migrate OUR superseded skill dirs (pre-namespacing watch/check-blackboard + retired
        // fleet-ops/fleetop→/confer-fleet, norms→the safety-kernel hook) IN THIS dir. Only ones that
        // mention confer — never an unrelated skill; exact names, so a current skill is untouched.
        for legacy in ["watch", "check-blackboard", "confer-fleet-ops", "confer-fleetop", "confer-norms"] {
            let sk = dir.join(legacy).join("SKILL.md");
            if std::fs::read_to_string(&sk).map(|s| s.contains("confer")).unwrap_or(false) {
                let _ = std::fs::remove_dir_all(dir.join(legacy));
                println!("  migrated: removed legacy /{legacy}");
            }
        }
    }
    println!("  confer: {bin}");
    println!("  hub:    {}", hub_root.display());
    println!("  role:   {role}");

    // Full reactive stack: also install + enable the SessionStart auto-heal hook
    // so a compacted session is told to re-arm a stale watcher. Inert
    // until a watch registers a target; opt out with --no-autoheal.
    // Install the auto-heal hook for EACH target harness: Claude's SessionStart entry in
    // ~/.claude/settings.json (matchers), Grok's native ~/.grok/hooks/confer.json (no matchers,
    // SessionStart + Pre/PostCompact). design/52 axis 7/8.
    if !no_autoheal {
        let cmd = format!("{bin} session-heal");
        let mut installed = false;
        for h in targets.iter().map(|(h, _)| *h).collect::<std::collections::BTreeSet<_>>() {
            let done = match h {
                "grok" => write_grok_hook(&home, &cmd)
                    .map(|_| home.join(".grok").join("hooks").join("confer.json")),
                // Codex merges into ~/.codex/hooks.json (a DIFFERENT root than its ~/.agents skills),
                // and won't RUN the new hook until the human reviews + trusts it via `/hooks` (design/54).
                "codex" => write_codex_hook(&home, &cmd),
                _ => {
                    let s = home.join(".claude").join("settings.json");
                    write_session_hook(&s, &cmd).map(|_| s)
                }
            };
            match done {
                Ok(p) => {
                    installed = true;
                    if h == "codex" {
                        println!("  auto-heal: merged codex hook → {} — REVIEW + TRUST via /hooks in Codex, or it won't run", p.display());
                    } else {
                        println!("  auto-heal: installed {h} hook → {}", p.display());
                    }
                }
                Err(e) => eprintln!("  auto-heal: {h} hook skipped ({e})"),
            }
        }
        if installed {
            let _ = autoheal::set_enabled(true);
            println!("  (confer autoheal off to disable)");
        }
    }
    // Harness-aware final banner. Codex has no idle-wake transport (design/54): poll-first, no
    // Monitor/`/loop`. Claude/Grok get the reactive line (Grok's /loop floor is 60s, Claude 45s).
    let has_reactive = targets.iter().any(|(h, _)| *h == "claude" || *h == "grok");
    let has_codex = targets.iter().any(|(h, _)| *h == "codex");
    if has_reactive {
        let loop_secs = if targets.iter().any(|(h, _)| *h == "grok") { 60 } else { 45 };
        println!(
            "use: /confer-watch (reactive, hosted by your monitor tool) or /loop {loop_secs}s /confer-poll (poll fallback). Skills run `confer session-context` at session start."
        );
    }
    if has_codex {
        println!(
            "codex: no idle wake — use /confer-poll at the start of each turn + after each human prompt (poll-first). The SessionStart hook runs `confer session-heal`."
        );
    }
    Ok(())
}
