//! Installing confer's Claude Code skills (/confer-watch, /confer-poll) and the tier-1 auto-resync
//! that keeps them current with the running binary.
//!
//! `cmd_install_skill` writes each skill in CONFER_SKILLS (from `templates`) with the machine's binary
//! path baked in, and (unless opted out) arms the SessionStart auto-heal hook. `resync_skills_if_stale`
//! is the SessionStart-time counterpart: if skills already exist but were baked from a different build,
//! silently re-derive them — never creating skills where none exist.

use crate::templates::CONFER_SKILLS;
use crate::{autoheal, config, hooks::write_session_hook, BUILD_SHA};
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

/// Agent harnesses confer installs skills for, and their global skills dir under $HOME (design/52
/// axis 3). Extend this to add a harness — both install and resync read it.
pub(crate) const HARNESS_SKILL_HOMES: &[(&str, &str)] = &[("claude", ".claude"), ("grok", ".grok")];

/// The skills dir for `harness` under `home`, if it's a known harness.
fn harness_skill_dir(home: &Path, harness: &str) -> Option<PathBuf> {
    HARNESS_SKILL_HOMES
        .iter()
        .find(|(h, _)| *h == harness)
        .map(|(_, sub)| home.join(sub).join("skills"))
}

/// The harness running THIS process (design/52): Grok Build sets `GROK_AGENT`; default Claude Code.
pub(crate) fn detect_harness() -> &'static str {
    if std::env::var("GROK_AGENT").ok().filter(|s| !s.is_empty()).is_some() {
        "grok"
    } else {
        "claude"
    }
}

/// Re-derive the confer skills in ONE dir if they exist there but were baked from a different build.
/// Returns whether it acted. Never creates skills where none exist; bails (role-blind safety) if a
/// template unexpectedly bakes {ROLE}/{HUB}.
fn resync_dir(dir: &Path, bin: &str) -> bool {
    if !dir.join("confer-watch").join("SKILL.md").is_file() {
        return false; // not installed here → not ours to create
    }
    let marker = dir.join("confer-watch").join(".confer-build");
    if std::fs::read_to_string(&marker).unwrap_or_default().trim() == BUILD_SHA {
        return false; // already current — cheap stat+read
    }
    for (name, tmpl) in CONFER_SKILLS {
        let filled = tmpl.replace("{CONFER}", bin);
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
    for (_, sub) in HARNESS_SKILL_HOMES {
        acted |= resync_dir(&home.join(sub).join("skills"), &bin);
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
    let targets: Vec<PathBuf> = if let Some(d) = dir {
        vec![PathBuf::from(d)]
    } else {
        match harness.as_deref().unwrap_or("auto") {
            "all" => HARNESS_SKILL_HOMES.iter().map(|(_, s)| home.join(s).join("skills")).collect(),
            "auto" => vec![harness_skill_dir(&home, detect_harness())
                .expect("the detected harness is always a known one")],
            h => vec![harness_skill_dir(&home, h)
                .ok_or_else(|| anyhow!("unknown --harness '{h}' — expected auto | claude | grok | all"))?],
        }
    };
    let fill = |t: &str| {
        t.replace("{CONFER}", &bin)
            .replace("{HUB}", &hub_root.to_string_lossy())
            .replace("{ROLE}", &role)
    };

    // ONE generic skill set, role-agnostic (commands resolve the caller's role from the hub clone
    // they run in), so co-resident agents don't clobber each other (design/32) — only {CONFER} (the
    // shared binary path) is baked. Written to each selected harness dir.
    for dir in &targets {
        for (name, tmpl) in CONFER_SKILLS {
            let d = dir.join(name);
            std::fs::create_dir_all(&d)?;
            std::fs::write(d.join("SKILL.md"), fill(tmpl))?;
        }
        // Stamp the build so the SessionStart tier-1 auto-heal can tell, cheaply, when a later binary
        // update left these stale and silently re-derive them.
        let _ = std::fs::write(dir.join("confer-watch").join(".confer-build"), BUILD_SHA);
        let names = CONFER_SKILLS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(",");
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
    if !no_autoheal {
        let settings = config::home()?.join(".claude").join("settings.json");
        match write_session_hook(&settings, &format!("{bin} session-heal")) {
            Ok(()) => {
                let _ = autoheal::set_enabled(true);
                println!("  auto-heal: installed SessionStart hook → {} and enabled (confer autoheal off to disable)", settings.display());
            }
            Err(e) => eprintln!(
                "  auto-heal: skipped (couldn't edit {}: {e})",
                settings.display()
            ),
        }
    }
    println!(
        "use: /confer-watch (Monitor, reactive/dormant) or /loop 45s /confer-poll (poll fallback)."
    );
    Ok(())
}
