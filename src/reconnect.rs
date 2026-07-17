//! Reconnect/onboard command handlers: (re)joining a hub, canonical hub-id
//! matching, managed-clone discovery, and reactive-arming diagnostics.

use anyhow::{anyhow, Result};

use crate::keygen_release::cmd_keygen;
use crate::skills::cmd_install_skill;
use crate::transport::{git_ssh_command, parse_remote, validate_transport_key, Scheme};
use crate::{clonehome, config, gitcmd, schema};
use crate::join::{cmd_join, safe_clone_dir, warn_if_nested};
use crate::init::cmd_init;
use crate::valid_slug;

/// Write the canonical /confer-watch + /confer-poll skills, adapted to this machine.
/// Bulletproof (re)connect. Idempotent: resolve-or-clone the hub, (re)join, install
/// the full reactive stack (skills + auto-heal hook), then print the one remaining
/// agent-driven step (arm `/confer-watch`). Safe whether cold or stale.
pub(crate) fn cmd_reconnect(
    role: Option<String>,
    hub: Option<String>,
    dir: Option<String>,
    host: Option<String>,
    ssh_key: Option<String>,
    force: bool,
) -> Result<()> {
    if let Some(k) = &ssh_key {
        validate_transport_key(k)?;
    }
    // 1. Resolve the hub clone — reuse an existing one, or clone from a URL (clone
    //    only; we do the join ourselves below so --host applies uniformly).
    let root: std::path::PathBuf = match &hub {
        Some(h) if std::path::Path::new(h).join(".git").exists() => std::fs::canonicalize(h)?,
        Some(h) => {
            let remote = parse_remote(h);
            let name_src = remote.shorthand.clone().unwrap_or_else(|| h.clone());
            let basename = name_src.rsplit('/').next().unwrap_or("hub").trim_end_matches(".git").to_string();
            // Don't nest inside a work repo when no --dir was given (#4) — agents run from a project dir.
            let clonedir = safe_clone_dir(dir.clone(), &basename);
            // Resolve to absolute BEFORE cloning — cmd_init changes the process cwd,
            // which would break a later relative-path canonicalize.
            let clonedir_abs = if std::path::Path::new(&clonedir).is_absolute() {
                std::path::PathBuf::from(&clonedir)
            } else {
                std::env::current_dir()?.join(&clonedir)
            };
            if !clonedir_abs.join(".git").exists() {
                cmd_init(h.clone(), Some(clonedir.clone()), None, Scheme::Auto, None, None, None, ssh_key.clone(), true, false)?;
            }
            clonedir_abs.canonicalize().unwrap_or(clonedir_abs)
        }
        None => match &dir {
            Some(d) => std::fs::canonicalize(d)?,
            None => config::repo_root().map_err(|_| {
                anyhow!("no hub found — run inside your hub clone, or pass --hub <url|owner/repo> [--dir <path>]")
            })?,
        },
    };
    // Point the following steps at this hub.
    std::env::set_var("CONFER_HUB", &root);
    warn_if_nested(&root);

    // Guard (#B): refuse to write confer state into a repo that ISN'T a confer hub. `reconnect
    // --hub <any .git>` would otherwise join + PUSH confer commits to that repo's real origin. A
    // confer hub carries the scaffold markers (a fresh clone gets them from `init` above); a random
    // work repo has none. 0.5.0 made `reconnect --hub <pasted value>` a headline command, so gate it.
    // Require the AUTHORITATIVE marker `.confer-version` (every real hub scaffolds it — a fresh
    // one gets it from `init` above). Do NOT accept a bare `roles/` or `threads/` dir: those are
    // common dir names (an Ansible repo has `roles/`), so an OR over them false-accepts non-confer
    // repos — the exact misdirection this gate exists to block (red-team #2, reproduced).
    if !root.join(".confer-version").exists() {
        return Err(anyhow!(
            "{} is a git repo but not a confer hub (no .confer-version marker) — refusing to join \
             and push confer state into it. Point --hub at your confer hub, or run \
             `confer init <url> --role <you>` to create one.",
            root.display()
        ));
    }

    // Pin transport auth to this clone (idempotent) — covers an EXISTING clone that predates the
    // key, and re-asserts it after a fresh clone. Keeps the headless watch's transport self-contained.
    if let Some(k) = &ssh_key {
        let _ = gitcmd::check(
            &root,
            &["config", "--local", "core.sshCommand", &git_ssh_command(k)],
        );
    }

    // 2. Refresh + (re)join with the requested host (idempotent).
    let _ = gitcmd::integrate(&root); // pull latest, best-effort
    if let Some(r) = &role {
        // Ensure a signing identity, exactly like `init --role`: reuse this clone's existing key,
        // else the fleet-standard per-role key at ~/.confer/keys/<role>, MINTING it if absent — so
        // reconnect yields a SIGNED, verifiable identity by default. Previously it passed whatever
        // `signing_key(&root)` returned (None when the clone had no key), producing a silent keyless,
        // UNVERIFIED join that then broke `where`/`adopt-clone` and left a cold agent unverified on
        // the happy path without realizing it (field report). Keygen failure is a hard error, never a
        // quiet degrade; `join --signing-key <path>` (or pre-placing the key) still bypasses.
        let sk = match config::signing_key(&root).map(|p| p.to_string_lossy().into_owned()) {
            Some(existing) => Some(existing),
            None => {
                // Fail fast on a bad role id BEFORE it reaches keys.join(r) — an absolute/`..` role
                // would turn that into an arbitrary-path existence probe (the exact guard `init --role`
                // has; the parity the commit claimed was missing here). Also gives a role-specific
                // error instead of the misleading "install ssh-keygen" one.
                if !valid_slug(r) {
                    return Err(anyhow!(
                        "invalid role '{r}': must match [a-z0-9][a-z0-9-]* (≤64 chars)"
                    ));
                }
                let kp = config::home()?.join(".confer").join("keys").join(r);
                if !kp.exists() {
                    cmd_keygen(Some(r.clone()), false).map_err(|e| {
                        anyhow!(
                            "could not mint a signing key for '{r}': {e}\n\
                             install ssh-keygen (openssh) and ensure ~/.confer/keys is writable, \
                             or run `confer join --role {r} --signing-key <path>` with an existing key"
                        )
                    })?;
                }
                Some(kp.to_string_lossy().into_owned())
            }
        };
        // Propagate — every cmd_join failure here is a hard precondition (invalid/reserved slug,
        // homoglyph display, re-key mismatch, or a re-role clobber of a clone already bound to
        // another role). None are transient, so aborting beats printing "✅ reconnected" over a
        // join that didn't happen. `--force` is threaded through for a deliberate re-role.
        cmd_join(r.clone(), host.clone(), None, None, sk, force)?;
    }

    // 3. Full reactive stack: skills + auto-heal hook (idempotent; migrates legacy names).
    cmd_install_skill(
        None,
        Some(root.to_string_lossy().to_string()),
        role.clone(),
        false,
    )?;

    // 4. The one remaining, agent-driven step.
    let r = role.unwrap_or_else(|| "<you>".into());
    println!();
    println!("✅ reconnected to hub {}", root.display());
    print_reactive_next(&r);
    Ok(())
}

/// Print the final reactive-arming step, agent-agnostically. Claude Code arms `/confer-watch`;
/// any other agent loops `confer poll`. Shared by `reconnect` and `init --role` so the two
/// idempotent do-commands end the same way. (install-skill wires the CC convenience; the
/// poll-loop is the mechanism that works on ANY harness — name both so no path is CC-only.)
pub(crate) fn print_reactive_next(role: &str) {
    // A role can arrive from a value an agent copied out of an untrusted peer message — strip any
    // terminal control sequences before echoing it (#D defense-in-depth).
    let role = schema::sanitize_term(role, false);
    println!("   final step — arm your reactive watch:  run  /confer-watch");
    println!("   (headless / no Monitor tool:  confer watch --role {role} --replace)");
    println!(
        "   (not Claude Code:  loop  `confer poll --role {role}`  inside your agent's run loop)"
    );
}

/// The literacy pointer for a cold agent: what confer is + the ONE next command for the
/// caller's situation. Agent-agnostic — a fresh agent runs this, learns confer, and gets a
/// single idempotent command to run next. Deliberately NOT `invite` (that onboards a newcomer
/// INTO a live hub, filled from hub state); `onboard` self-bootstraps a create-or-join when
/// there is no hub and no inviter yet.
/// A transport- and case-independent canonical id for a hub, used to MATCH an existing managed clone
/// to a requested hub. Remote URLs collapse to `host/owner/repo` — scheme, `user@`, `:port`, a
/// `.git` suffix and a trailing slash all stripped, then lowercased (GitHub/GitLab paths are
/// case-insensitive; matching a shade too loosely across ssh/https of the SAME repo is the whole
/// point). Local filesystem hubs canonicalize to an absolute path and compare EXACTLY — never a
/// suffix test, which would false-match a different hub that merely shares a basename (red-team #1).
/// Returns None for anything not recognizable as a hub ref, so an unknown value matches nothing.
pub(crate) fn canonical_hub_id(input: &str) -> Option<String> {
    let s = input.trim().trim_end_matches('/');
    if s.is_empty() {
        return None;
    }
    // Local filesystem hub (a bare-repo path): absolute, ~, ., or an existing path.
    if s.starts_with(['/', '~', '.']) || std::path::Path::new(s).exists() {
        let expanded = if s == "~" {
            config::home().ok()?
        } else if let Some(rest) = s.strip_prefix("~/") {
            config::home().ok()?.join(rest)
        } else {
            std::path::PathBuf::from(s)
        };
        let canon = std::fs::canonicalize(&expanded).unwrap_or(expanded);
        let c = canon.to_string_lossy();
        return Some(format!("file:{}", c.trim_end_matches(".git").trim_end_matches('/')));
    }
    // Remote: pull out (host, path) for scp-like, scheme://, and bare owner/repo forms.
    let (host, path) = if let Some(rest) = s.strip_prefix("git@") {
        rest.split_once(':')?
    } else if let Some((_scheme, after)) = s.split_once("://") {
        let after = after.rsplit_once('@').map_or(after, |(_, h)| h); // strip user@
        after.split_once('/')?
    } else if !s.contains(':') && s.matches('/').count() == 1 {
        ("github.com", s) // bare owner/repo → github.com
    } else {
        return None;
    };
    let host = host.split(':').next().unwrap_or(host); // drop :port
    let path = path
        .trim_start_matches('/')
        .trim_end_matches('/')
        .trim_end_matches(".git");
    if host.is_empty() || path.is_empty() {
        return None;
    }
    Some(format!(
        "{}/{}",
        host.to_ascii_lowercase(),
        path.to_ascii_lowercase()
    ))
}

/// Find the HEALTHY managed clone (under `~/.confer/clones/`) for a hub + role, if one exists on THIS
/// machine — matched by role and by `canonical_hub_id` (transport/case-independent), and gated on a
/// `.confer-version` marker so a half-migrated/broken clone isn't reported as "already joined".
/// Read-only; `onboard` uses it to tell a returning agent to RE-ARM rather than clone again.
fn find_managed_clone(hub: &str, role: &str) -> Option<std::path::PathBuf> {
    let want = canonical_hub_id(hub)?;
    clonehome::list()
        .into_iter()
        .filter(|c| c.role == role)
        .filter(|c| c.path.join(".confer-version").is_file() && c.path.join("threads").is_dir())
        .find(|c| {
            gitcmd::output(&c.path, &["config", "--get", "remote.origin.url"])
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .and_then(|o| canonical_hub_id(&o))
                .as_deref()
                == Some(want.as_str())
        })
        .map(|c| c.path)
}

pub(crate) fn cmd_onboard(role: Option<String>, hub: Option<String>) -> Result<()> {
    // A concrete, paste-safe default role — NEVER a `<...>` placeholder (a shell chokes on `<`/`>`,
    // so a pasted command would silently fail). The user swaps it for a meaningful role id. Sanitize
    // both echoed values for DISPLAY; keep the RAW role/hub for looking up an existing managed clone.
    let r = schema::sanitize_term(role.as_deref().unwrap_or("agent"), false);
    let hub_display = hub.as_deref().map(|h| schema::sanitize_term(h, false));
    println!("confer — a git-native coordination layer for AI agents.");
    println!("A \"fleet\" is one private git repo (the hub). Each agent joins it with a signed");
    println!(
        "identity and coordinates by appending signed, verifiable messages — no server, no db."
    );
    println!();
    match hub_display.as_deref() {
        Some(h) => {
            // Already joined this fleet as this role on THIS machine? Managed clones are per-role
            // (`~/.confer/clones/<hub>/<role>-<key>/`), so a returning agent should RE-ARM its clone,
            // not clone again. Only resolvable when a concrete role was given (not the placeholder).
            let existing = match (role.as_deref(), hub.as_deref()) {
                (Some(rr), Some(hh)) => find_managed_clone(hh, rr),
                _ => None,
            };
            if let Some(p) = existing {
                println!("You're already joined to this fleet as {r} — in your managed clone:");
                println!("    {}", p.display());
                println!();
                println!("Don't re-clone. Just RE-ARM your reactive watch from there:");
                println!("    cd {} && confer watch --role {r} --replace", p.display());
                println!("    (Claude Code: run  /confer-watch  from that directory — same thing.)");
            } else {
                println!("You were pointed at a fleet. JOIN it with one command:");
                println!();
                println!("    confer clone {h} --role {r} --managed");
                println!();
                println!(
                    "That clones the hub, mints your key, joins as {r}, and arms your reactive layer"
                );
                println!("— landing in a PER-ROLE managed clone (~/.confer/clones/…), so several roles");
                println!("on ONE machine each get their own clone and never collide. One clone = one role.");
                println!(
                    "Private hub authed by a deploy key (not your default SSH)? add:  --ssh-key <path>"
                );
                println!();
                println!(
                    "(Re-running is safe — `confer onboard --hub {h} --role {r}` finds your clone and"
                );
                println!(" points you at re-arming it instead of cloning twice.)");
            }
        }
        None => {
            println!("You have no fleet yet. START one with a single command (local, zero-setup):");
            println!();
            println!("    confer init ~/confer/team.git --role {r}");
            println!();
            println!(
                "That scaffolds a local hub, mints your signing key, joins as {r}, and wires your"
            );
            println!("reactive layer — one idempotent command, no GitHub or network needed.");
            println!();
            println!(
                "For agents on OTHER machines to join, start the hub on a PRIVATE repo instead:"
            );
            println!(
                "    confer init your-org/your-hub --role {r}     # a private GitHub/GitLab repo"
            );
            println!("    # each peer then runs:  confer clone your-org/your-hub --role frontend --managed");
            println!();
            println!("Private-hub auth — a headless watch needs non-interactive push credentials:");
            println!(
                "  • deploy key / non-default SSH:  add  --ssh-key <path>  (pinned to the clone)"
            );
            println!(
                "  • HTTPS + a GitHub App token:    see  confer credential / app-config --help"
            );
            println!("  • `confer doctor` flags a clone whose transport isn't self-contained");
        }
    }
    println!();
    if role.is_none() {
        println!("(`{r}` is a placeholder — replace it with a role id for this agent: any lowercase name.)");
    }
    println!("Reactive layer: on Claude Code, `confer install-skill` wires `/confer-watch`.");
    println!("On any other agent, loop `confer poll --role {r}` in your run loop instead.");
    Ok(())
}

/// Loud degrade when the reactive-layer wiring fails during a join. The clone + signed join already
/// SUCCEEDED, so we don't abort — but we must NOT print "✅ fleet ready" over a watch that isn't set
/// up (the silent-success class). Surface the failure on stderr and give the exact by-hand fix.
pub(crate) fn warn_reactive_arm_failed(e: &anyhow::Error, dir: &std::path::Path, role: &str) {
    eprintln!(
        "\nconfer: ⚠ joined as {role}, but arming the reactive layer FAILED ({e}) — your \
         /confer-watch is NOT wired yet.\n  arm it by hand: cd {} && confer install-skill --role \
         {role}   (then run /confer-watch)",
        dir.display()
    );
}

