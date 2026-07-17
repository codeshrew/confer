//! Installing confer's Claude Code skills (/confer-watch, /confer-poll) and the tier-1 auto-resync
//! that keeps them current with the running binary.
//!
//! `cmd_install_skill` writes each skill in CONFER_SKILLS (from `templates`) with the machine's binary
//! path baked in, and (unless opted out) arms the SessionStart auto-heal hook. `resync_skills_if_stale`
//! is the SessionStart-time counterpart: if skills already exist but were baked from a different build,
//! silently re-derive them — never creating skills where none exist.

use crate::templates::CONFER_SKILLS;
use crate::{autoheal, config, hooks::write_session_hook, BUILD_SHA};
use anyhow::Result;

/// Tier-1 auto-heal: if confer skills are already installed in the default global dir but were baked
/// from a DIFFERENT build than this binary, silently re-derive them. This is safe to do without
/// asking — skills are a pure function of the on-disk binary (fixed templates + the binary's own
/// path), and SessionStart runs the NEW binary — so it closes the "updated the binary but forgot to
/// re-sync the skills" gap with zero agent action and nothing to judge. Returns the build it synced
/// to when it acted (for a one-line note), None when nothing was stale. It NEVER creates skills where
/// none exist — a fresh install is an explicit `install-skill` choice, not something to auto-heal —
/// and only touches the DEFAULT global dir (a `--dir` install is the agent's own placement to manage).
pub(crate) fn resync_skills_if_stale() -> Option<String> {
    let dir = config::home().ok()?.join(".claude").join("skills");
    let marker = dir.join("confer-watch").join(".confer-build");
    // Present = installed here. Absent = not installed → not ours to create.
    if !dir.join("confer-watch").join("SKILL.md").is_file() {
        return None;
    }
    if std::fs::read_to_string(&marker).unwrap_or_default().trim() == BUILD_SHA {
        return None; // already current — the common case, a cheap stat+read
    }
    let bin = std::env::current_exe().ok()?.to_string_lossy().to_string();
    for (name, tmpl) in CONFER_SKILLS {
        let filled = tmpl.replace("{CONFER}", &bin);
        // Defensive: these templates are role-agnostic by design (design/32). If a future one ever
        // baked {ROLE}/{HUB}, a role-blind resync would write a broken skill — so bail rather than
        // corrupt a working install; the explicit `install-skill` (which knows role+hub) still fixes it.
        if filled.contains("{ROLE}") || filled.contains("{HUB}") {
            return None;
        }
        let d = dir.join(name);
        std::fs::create_dir_all(&d).ok()?;
        std::fs::write(d.join("SKILL.md"), filled).ok()?;
    }
    let _ = std::fs::write(&marker, BUILD_SHA);
    Some(BUILD_SHA.to_string())
}

pub(crate) fn cmd_install_skill(
    dir: Option<String>,
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
    // Default to the GLOBAL skills dir (~/.claude/skills): a coordination watch
    // skill is cross-project infrastructure, and Claude Code only auto-discovers
    // skills from ~/.claude or the *current* project — so writing into the hub
    // repo hides /watch from an agent whose session lives in its own code repo.
    let dir = match dir {
        Some(d) => std::path::PathBuf::from(d),
        None => config::home()?.join(".claude").join("skills"),
    };
    let fill = |t: &str| {
        t.replace("{CONFER}", &bin)
            .replace("{HUB}", &hub_root.to_string_lossy())
            .replace("{ROLE}", &role)
    };

    // ONE generic skill, shared by every agent on the machine — the skill text is role-agnostic
    // (commands resolve the caller's role from the hub clone they're run in), so co-resident agents
    // no longer clobber each other by baking their own role into a shared `confer-watch/SKILL.md`
    // (design/32). Only {CONFER} (the machine's binary path, shared by co-resident agents) is baked.
    for (name, tmpl) in CONFER_SKILLS {
        let d = dir.join(name);
        std::fs::create_dir_all(&d)?;
        std::fs::write(d.join("SKILL.md"), fill(tmpl))?;
    }
    // Stamp the build these skills were baked from so the SessionStart tier-1 auto-heal can tell,
    // cheaply, when a later binary update has left them stale and silently re-derive them.
    let _ = std::fs::write(dir.join("confer-watch").join(".confer-build"), BUILD_SHA);
    println!(
        "wrote {}/{{confer-watch,confer-poll}}/SKILL.md",
        dir.display()
    );
    // Migrate: remove OUR pre-namespacing skill dirs so an agent doesn't keep both /watch and
    // /confer-watch. Only remove ones clearly OURS (mention confer) — never an unrelated skill.
    for legacy in ["watch", "check-blackboard"] {
        let sk = dir.join(legacy).join("SKILL.md");
        if std::fs::read_to_string(&sk)
            .map(|s| s.contains("confer"))
            .unwrap_or(false)
        {
            let _ = std::fs::remove_dir_all(dir.join(legacy));
            println!("  migrated: removed legacy /{legacy}");
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
