//! `confer init` / clone setup and the managed-clone lifecycle commands.
//!
//! `cmd_init` clones a hub, pins `main`, scaffolds an empty hub, verifies auth, and can join a role.
//! The rest manages this machine's clone layout: `adopt-clone`/migrate a loose clone into the managed
//! home (`~/.confer/clones/…`), and `clones`/`hubs`/`where` enumerate what's here. Highest-coupling
//! of the extracted command families — it drives join/transport/config and shares clone-move helpers.

use crate::join::{cmd_join, configure_signing, read_pubkey, safe_clone_dir};
use crate::keygen_release::cmd_keygen;
use crate::reconnect::{print_reactive_next, warn_reactive_arm_failed};
use crate::skills::cmd_install_skill;
use crate::templates::README_TEMPLATE;
use crate::transport::{
    clone_url_candidates, git_ssh_command, parse_remote, validate_transport_key, Scheme,
};
use crate::{
    autoheal, clonehome, config, gitcmd, my_build, roster, store, tiers, valid_slug, watchlock,
    BUILD_SHA,
};
use anyhow::{anyhow, Result};

/// Clone a hub, pin the `main` branch, scaffold if empty, verify auth, health-check.
#[allow(clippy::too_many_arguments)]
pub(crate) fn cmd_init(
    url: String,
    dir: Option<String>,
    role: Option<String>,
    scheme: Scheme,
    display: Option<String>,
    desc: Option<String>,
    signing_key: Option<String>,
    ssh_key: Option<String>,
    is_clone: bool,
    managed: bool,
) -> Result<()> {
    // Zero-dependency CREATE: a local-path url with nothing there yet becomes a fresh bare hub.
    let url = expand_local_hub(url)?;
    let remote = parse_remote(&url);
    // Transport auth for a PRIVATE hub: build the `GIT_SSH_COMMAND` from --ssh-key. Used for the
    // clone AND (below) pinned to the clone's local `core.sshCommand`, so the identity isn't
    // ambient — a fresh shell or the headless watch keeps reaching the hub. (#1 field feedback.)
    if let Some(k) = &ssh_key {
        validate_transport_key(k)?;
    }
    let ssh_cmd: Option<String> = ssh_key.as_deref().map(git_ssh_command);
    let name_src = remote.shorthand.clone().unwrap_or_else(|| url.clone());
    let basename = name_src
        .rsplit('/')
        .next()
        .unwrap_or("hub")
        .trim_end_matches(".git")
        .to_string();
    // Don't nest the working clone inside a work repo when no dir was named (#4 field feedback).
    let dir = safe_clone_dir(dir, &basename);
    let dir_path = std::path::PathBuf::from(&dir);
    if dir_path.exists() {
        return Err(anyhow!(
            "target '{dir}' already exists — remove it or pick another dir"
        ));
    }

    // Try each candidate URL in order; on auth/other failure fall back to the
    // other scheme (a failed `git clone` may leave a partial dir — remove it
    // before the next attempt; safe because we verified dir didn't pre-exist).
    // Honor the scheme the user actually TYPED: an explicit https:// (or ssh)
    // URL must set an https (or ssh) origin, or a no-SSH agent gets a git@ origin
    // whose fetch then silently fails (a review finding). Only the
    // bare owner/repo shorthand falls back to prefer_ssh ordering.
    let candidates = clone_url_candidates(&url, &remote, scheme);
    let multi = candidates.len() > 1;
    let mut used = None;
    let mut last_err = String::new();
    for cand in &candidates {
        // Prefer a BLOBLESS partial clone: keeps the full commit graph
        // so `merge-base` cursors stay exact, but defers historical blobs we rarely
        // reopen. NOT shallow (`--depth` breaks merge-base) and NOT sparse (confer
        // reads bodies from the working tree). Fall back to a full clone if the
        // server rejects filters (older / self-hosted git).
        let mut cloned = false;
        for filter in [true, false] {
            let mut args: Vec<&str> = vec!["clone"];
            if filter {
                args.push("--filter=blob:none");
            }
            // `--` before the positionals: `cand`/`dir` are caller/onboarding-supplied, so
            // without it a hostile `--upload-pack=<cmd>`-shaped url is parsed by git as a FLAG
            // (arg-injection → RCE with a file:///ssh:// target that invokes upload-pack).
            args.push("--");
            args.push(cand);
            args.push(&dir);
            let mut gclone = std::process::Command::new("git");
            gclone.args(&args);
            // Never block on an interactive prompt during a headless clone (#3): null stdin, and
            // (with BatchMode in GIT_SSH_COMMAND) a passphrase/host-key prompt fails fast, not hangs.
            gclone.stdin(std::process::Stdio::null());
            if let Some(sc) = &ssh_cmd {
                gclone.env("GIT_SSH_COMMAND", sc); // authenticate the clone with the transport key
            }
            let out = gclone.output()?;
            if out.status.success() {
                used = Some(cand.clone());
                cloned = true;
                break;
            }
            last_err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            if dir_path.exists() {
                let _ = std::fs::remove_dir_all(&dir_path);
            }
        }
        if cloned {
            break;
        }
        if multi {
            eprintln!("confer: clone via {cand} failed; trying the other URL scheme…");
        }
    }
    let url = used.ok_or_else(|| anyhow!("git clone failed: {last_err}"))?;
    let root = dir_path.canonicalize()?;

    // Pin the transport key to THIS clone (local config) so it's self-contained: the next
    // ls-remote/push/fetch — and the headless watch — reach the hub without ambient ~/.ssh.
    if let Some(sc) = &ssh_cmd {
        gitcmd::check(&root, &["config", "--local", "core.sshCommand", sc])?;
    }

    // Determine emptiness from the HUB's branches (ls-remote), not the local
    // checkout — a bare hub's HEAD may point at an unborn branch and mislead us.
    let heads = gitcmd::output(&root, &["ls-remote", "--heads", "origin"])?;
    if !heads.status.success() {
        return Err(anyhow!(
            "cannot reach hub (check auth/URL): {}",
            String::from_utf8_lossy(&heads.stderr).trim()
        ));
    }
    let heads_s = String::from_utf8_lossy(&heads.stdout);
    let has_any = !heads_s.trim().is_empty();
    let has_main = heads_s.contains("refs/heads/main");

    if !has_any {
        // Fresh hub: pin main, scaffold, push.
        gitcmd::check(&root, &["symbolic-ref", "HEAD", "refs/heads/main"])?;
        std::fs::create_dir_all(root.join("threads"))?;
        std::fs::write(root.join("threads").join(".gitkeep"), "")?;
        std::fs::create_dir_all(root.join("roles"))?;
        std::fs::write(root.join("roles").join(".gitkeep"), "")?;
        // Pin as "<semver> <sha>" so agents can grade drift (major/minor/patch),
        // not just detect a sha mismatch. Legacy sha-only pins still parse.
        std::fs::write(root.join(".confer-version"), my_build().pin_string())?;
        std::fs::write(root.join("README.md"), README_TEMPLATE)?;
        // Gitignore confer's per-clone LOCAL state so `git add -A` (by confer, an
        // agent, or a hook) never commits a lock/cursor/identity into the SHARED
        // hub — which would pollute the log and leak identity.json across the fleet.
        std::fs::write(root.join(".gitignore"), ".confer/\n")?;
        gitcmd::check(&root, &["add", "-A"])?;
        gitcmd::check(
            &root,
            &[
                "-c",
                "user.name=confer",
                "-c",
                "user.email=confer@confer.local",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "-m",
                "confer: initialize hub",
            ],
        )?;
        let p = gitcmd::output(&root, &["push", "-u", "origin", "main"])?;
        if !p.status.success() {
            return Err(anyhow!(
                "push failed (check auth/URL): {}",
                String::from_utf8_lossy(&p.stderr).trim()
            ));
        }
        // Point the hub's default branch at main so future clones don't land on
        // an unborn master (only possible for a local bare hub; hosted hubs
        // set their own default on first push).
        let hub = std::path::Path::new(&url);
        if hub.is_dir() {
            let _ = gitcmd::output(hub, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        }
        println!("initialized a fresh hub on branch 'main'.");
    } else if has_main {
        gitcmd::check(&root, &["checkout", "-q", "main"])?;
    } else {
        eprintln!(
            "confer: warning — hub has branches but no 'main'; confer standardizes on 'main'. \
             Consider migrating the hub's default branch to main."
        );
    }

    // Health check.
    let branch =
        String::from_utf8_lossy(&gitcmd::output(&root, &["branch", "--show-current"])?.stdout)
            .trim()
            .to_string();
    let msg_count = store::all_messages(&root)?.len();
    let roster = roster::load(&root);
    let roles = if roster.is_empty() {
        "(none — add to roles.toml)".to_string()
    } else {
        let mut ids: Vec<&String> = roster.keys().collect();
        ids.sort();
        ids.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    println!("hub ready: {url}");
    println!(
        "  dir:      {}{}",
        root.display(),
        if managed { "  (staging — relocated under --managed; see below)" } else { "" }
    );
    println!("  branch:   {branch}");
    println!("  messages: {msg_count}");
    println!("  roles:    {roles}");

    // Default trust tier: `own` for a hub you init, `foreign` for one you clone/join
    //. Set BEFORE join so an init's `own` isn't clobbered by join's default.
    let _ = tiers::set_default(
        &config::hub_key(&root),
        if is_clone {
            tiers::Tier::Foreign
        } else {
            tiers::Tier::Own
        },
    );

    // Keep the role available after the move below, so a `--managed` create can arm the reactive
    // stack from the FINAL (relocated) clone path — making `clone/init --role --managed` a complete
    // one-command join+arm, not a join that leaves you to `cd` and arm by hand.
    let managed_role = role.clone();
    if let Some(r) = role {
        // Fail fast on a bad role id BEFORE it reaches `keys.join(&r)` (an absolute `r` would
        // turn that into an arbitrary-path existence probe) — don't lean on join/keygen catching
        // it downstream.
        if !valid_slug(&r) {
            return Err(anyhow!(
                "invalid role '{r}': must match [a-z0-9][a-z0-9-]* (≤64 chars)"
            ));
        }
        std::env::set_current_dir(&root)?;
        // Ensure a signing identity: the provided key, else the fleet-standard key for this role,
        // MINTING it if absent — so a create yields a signed, verifiable identity by default. A
        // keygen FAILURE is a HARD ERROR, never a silent keyless join: the "signed by default"
        // guarantee this path advertises must not degrade quietly. Pass --signing-key to bypass.
        let signing_key = match signing_key {
            Some(k) => Some(k),
            None => {
                let kp = config::home()?.join(".confer").join("keys").join(&r);
                if !kp.exists() {
                    cmd_keygen(Some(r.clone()), None, false).map_err(|e| {
                        anyhow!(
                            "could not mint a signing key for '{r}': {e}\n\
                             install ssh-keygen (openssh) and ensure ~/.confer/keys is writable, \
                             or pass --signing-key <path> to use an existing key"
                        )
                    })?;
                }
                Some(kp.to_string_lossy().into_owned())
            }
        };
        println!();
        // Fresh clone from `init` — no prior identity to clobber, so force is irrelevant here.
        cmd_join(r.clone(), None, display, desc, signing_key, false)?;
        // Full reactive stack (mirrors `reconnect`), so `init --role` is the one-command CREATE
        // that `onboard` points to. Skip under --managed: the clone relocates below, so the
        // skills' resolved paths + the arm-from-here advice would be stale; managed prints its own.
        if !managed {
            match cmd_install_skill(
                None,
                Some(root.to_string_lossy().to_string()),
                Some(r.clone()),
                false,
            ) {
                Ok(_) => {
                    println!();
                    println!("✅ fleet ready at {}", root.display());
                    print_reactive_next(&r);
                }
                Err(e) => warn_reactive_arm_failed(&e, &root, &r),
            }
        }
    } else {
        println!("next: cd {dir} && confer join --role <your-role>");
    }
    if managed {
        // Relocate the freshly-set-up clone into confer's managed home. Step out of it first
        // (cwd may be inside it from the join above), and force (it's brand new — nothing to lose).
        let _ = std::env::set_current_dir(config::home()?);
        let (dest, _) = migrate_to_managed(&root, true)?;
        println!("\nmanaged: this clone now lives at {}", dest.display());
        // Arm the reactive stack FROM the final path — skipped before the move (stale paths), done
        // now so a managed join is complete in one command, exactly like the non-managed branch.
        if let Some(r) = &managed_role {
            match cmd_install_skill(None, Some(dest.to_string_lossy().to_string()), Some(r.clone()), false) {
                Ok(_) => {
                    println!();
                    println!("✅ fleet ready at {}", dest.display());
                    print_reactive_next(r);
                }
                Err(e) => warn_reactive_arm_failed(&e, &dest, r),
            }
        } else {
            println!(
                "  watch from there: cd {} && confer watch --role <you>",
                dest.display()
            );
        }
    }
    Ok(())
}

/// Move an existing agent clone into confer's managed home (~/.confer/clones/…):
/// validate it's an agent clone, compute the managed path from (hub_key, role, pubkey), guard
/// against losing unpushed/uncommitted work (unless `force`), move it, and re-point autoheal.
/// Returns (new path, moved?) — `moved=false` when it was already at its managed location.
fn migrate_to_managed(src: &std::path::Path, force: bool) -> Result<(std::path::PathBuf, bool)> {
    let src =
        std::fs::canonicalize(src).map_err(|e| anyhow!("cannot access {}: {e}", src.display()))?;
    if !src.join(".confer").join("identity.json").is_file() {
        return Err(anyhow!(
            "{} is not a confer agent clone (no .confer/identity.json) — refusing to manage it",
            src.display()
        ));
    }
    let role = config::resolve_role(None, &src)?;
    // pubkey: prefer identity.json, else the on-disk signing key, else the published card.
    let pubkey = clonehome::identity_pubkey(&src)
        .or_else(|| config::signing_key(&src).and_then(|k| read_pubkey(&k).ok()))
        .or_else(|| roster::pubkey(&roster::load(&src), &role).map(String::from));
    let Some(pubkey) = pubkey else {
        return Err(anyhow!(
            "'{role}' has no signing key/pubkey — a managed clone needs a keyed identity (join with --signing-key first)"
        ));
    };
    let hub_key = config::hub_key(&src);
    let dest = clonehome::clone_dir(&hub_slug_for(&src), &hub_key, &role, &pubkey)?;
    // Already at its managed location? Compare CANONICALLY — `$HOME` may be symlinked (e.g.
    // /tmp → /private/tmp on macOS), so a raw path compare would spuriously differ. A DIFFERENT
    // clone occupying the path is a refusal.
    if dest.exists() {
        if std::fs::canonicalize(&dest).ok().as_deref() == Some(src.as_path()) {
            return Ok((dest, false));
        }
        return Err(anyhow!(
            "a clone already exists at the managed path {} — resolve that manually first",
            dest.display()
        ));
    }
    if !force {
        if let Err(why) = clone_move_safe(&src) {
            return Err(anyhow!(
                "{} has {why} — push/commit first, or pass --force (a clone may be the only copy of unpushed work)",
                src.display()
            ));
        }
    }
    if matches!(
        watchlock::classify(&watchlock::inspect(&hub_key, &role, 90), BUILD_SHA),
        watchlock::WatchState::Healthy | watchlock::WatchState::Outdated
    ) {
        eprintln!("note: a watch is running for '{role}' — it will stop when the clone moves; re-arm it at the new path.");
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // rename (same filesystem) or fall back to `mv` (which copies+deletes across devices). On a
    // partial-failure, clean up any half-written debris at dest so it doesn't block future
    // adopt-clone/--managed for this identity (a review finding).
    if std::fs::rename(&src, &dest).is_err() {
        let o = std::process::Command::new("mv")
            .arg(&src)
            .arg(&dest)
            .output();
        let failed = match &o {
            Ok(o) if o.status.success() => None,
            Ok(o) => Some(String::from_utf8_lossy(&o.stderr).trim().to_string()),
            Err(e) => Some(e.to_string()),
        };
        if let Some(why) = failed {
            if src.exists() {
                let _ = std::fs::remove_dir_all(&dest); // src intact → dest is partial debris
            }
            return Err(anyhow!("move failed: {why}"));
        }
    }
    autoheal::retarget(&src.to_string_lossy(), &dest.to_string_lossy());
    // Backfill the pubkey into identity.json so `confer where`/resolve can verify this clone by
    // KEY, not just its (public, replayable) path tag. Clones joined before pubkey was recorded
    // (every pre-0.4.0 identity.json) migrate without it, which made `where` report "not managed
    // yet" for an already-adopted clone — disagreeing with `confer clones` (a fleet finding).
    clonehome::backfill_pubkey(&dest, &pubkey);
    // Sign-by-default after migration: if the identity records a signing key that exists,
    // (re)assert the FULL signer config — key + gpg.format + program + commit.gpgsign=true.
    // A clone that had the key set but `commit.gpgsign=false` (e.g. joined keyless, keyed up
    // later outside `join`) went silently UNSIGNED after migration — the trust model off by
    // default, the wrong state for a trust tool (a pre-launch finding). This turns it on.
    if let Some(sk) = config::signing_key(&dest).filter(|p| p.exists()) {
        let was = gitcmd::output(&dest, &["config", "--get", "commit.gpgsign"])
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        match configure_signing(&dest, &sk) {
            // Be loud when we actually flipped signing on — a trust tool shouldn't change a
            // trust-affecting setting silently (a review transparency nit).
            Ok(_) if was != "true" => println!(
                "re-enabled commit signing for this migrated clone (was '{}') — its messages will be signed",
                if was.is_empty() { "unset" } else { &was }
            ),
            Ok(_) => {}
            Err(e) => eprintln!(
                "note: could not assert commit signing at the new path ({e}) — run `confer doctor --fix`"
            ),
        }
    }
    Ok((dest, true))
}

/// A readable hub slug for a managed-clone dir name — from the clone's origin URL, or its own
/// dir name for a local/no-origin hub. `clonehome::slug` sanitizes it.
fn hub_slug_for(clone: &std::path::Path) -> String {
    let origin = gitcmd::output(clone, &["config", "--get", "remote.origin.url"])
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    origin
        .as_deref()
        .and_then(|u| parse_remote(u).shorthand)
        .or_else(|| {
            origin.as_deref().and_then(|u| {
                u.rsplit('/')
                    .next()
                    .map(|s| s.trim_end_matches(".git").to_string())
            })
        })
        .or_else(|| clone.file_name().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "hub".to_string())
}

/// Is a clone safe to MOVE without losing work? Errors with a human reason on uncommitted changes,
/// unpushed commits, or no upstream at all.
fn clone_move_safe(src: &std::path::Path) -> std::result::Result<(), String> {
    let dirty = gitcmd::output(src, &["status", "--porcelain"])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);
    if dirty {
        return Err("uncommitted or untracked changes".to_string());
    }
    let has_upstream = gitcmd::output(
        src,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .map(|o| o.status.success())
    .unwrap_or(false);
    if !has_upstream {
        return Err("no upstream branch (this clone may be the only copy)".to_string());
    }
    let unpushed = gitcmd::output(src, &["log", "--oneline", "@{u}..HEAD"])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);
    if unpushed {
        return Err("unpushed commits".to_string());
    }
    Ok(())
}

/// List confer's managed clones (`confer clones`).
pub(crate) fn cmd_clones() -> Result<()> {
    let mut clones = clonehome::list();
    if clones.is_empty() {
        println!("no managed clones yet.");
        println!("  create one:  confer clone <url> --role <r> --signing-key <k> --managed");
        println!("  or move one: confer adopt-clone <path>");
        return Ok(());
    }
    clones.sort_by(|a, b| {
        (a.hub_slug.as_str(), a.role.as_str()).cmp(&(b.hub_slug.as_str(), b.role.as_str()))
    });
    println!(
        "managed clones ({}, under ~/.confer/clones/):",
        clones.len()
    );
    for c in &clones {
        println!("  {:<20} {:<14} {}", c.hub_slug, c.role, c.path.display());
    }
    Ok(())
}

/// One clone path per DISTINCT hub (deduped), one per line — the discovery primitive a portable
/// multi-hub skill iterates so it never hardcodes a machine path. Unions MANAGED clones with AD-HOC
/// ones discovered by their `.confer-version` marker (an `init <url> <dir>` clone outside the managed
/// home) — a fleet view that SILENTLY omits a hub is the same "wrong-but-confident" failure as the
/// bug this replaces. Deduped by hub IDENTITY (origin), so a managed + ad-hoc clone of one hub is
/// one line, and N co-resident roles collapse too.
pub(crate) fn cmd_hubs() -> Result<()> {
    let mut candidates: Vec<std::path::PathBuf> =
        clonehome::list().into_iter().map(|c| c.path).collect();
    candidates.extend(discover_marker_clones());

    let mut seen = std::collections::BTreeSet::new();
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    for path in candidates {
        if !path.join(".confer-version").is_file() {
            continue; // only real hub clones
        }
        // hub identity: the origin's github shorthand (git@ / https collapse to owner/repo), else the
        // raw origin url, else the canonical path (a local bare hub with no remote).
        let ident = gitcmd::output(&path, &["config", "--get", "remote.origin.url"])
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .map(|u| parse_remote(&u).shorthand.unwrap_or(u))
            .unwrap_or_else(|| {
                path.canonicalize().unwrap_or_else(|_| path.clone()).to_string_lossy().into_owned()
            });
        if seen.insert(ident) {
            out.push(path);
        }
    }
    out.sort();
    for p in &out {
        println!("{}", p.display());
    }
    Ok(())
}

/// Discover ad-hoc hub clones (NOT under the managed home) by their `.confer-version` marker, in a
/// bounded set of common dev roots + the cwd — so `confer hubs` doesn't silently drop an
/// `init <url> <dir>` clone. Cheap + deterministic: fixed roots, shallow depth, skips heavy dirs.
fn discover_marker_clones() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(home) = config::home() {
        for r in ["git", "src", "code", "projects", "dev", "work"] {
            find_hub_markers(&home.join(r), 2, &mut out);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        find_hub_markers(&cwd, 1, &mut out);
    }
    out
}

fn find_hub_markers(dir: &std::path::Path, depth: usize, out: &mut Vec<std::path::PathBuf>) {
    if dir.join(".confer-version").is_file() {
        out.push(dir.to_path_buf());
        return; // it's a hub clone — don't descend into it
    }
    if depth == 0 {
        return;
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if !e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || matches!(name.as_ref(), "node_modules" | "target" | "vendor")
            {
                continue;
            }
            find_hub_markers(&e.path(), depth - 1, out);
        }
    }
}

/// Print the managed-home path for this clone's identity (`confer where`).
pub(crate) fn cmd_where() -> Result<()> {
    let root = config::repo_root()?;
    let role = config::resolve_role(None, &root)?;
    let pubkey = clonehome::identity_pubkey(&root)
        .or_else(|| config::signing_key(&root).and_then(|k| read_pubkey(&k).ok()))
        .or_else(|| roster::pubkey(&roster::load(&root), &role).map(String::from));
    let Some(pubkey) = pubkey else {
        return Err(anyhow!(
            "no signing key/pubkey for '{role}' — a managed clone is keyed by identity"
        ));
    };
    let hub_key = config::hub_key(&root);
    match clonehome::resolve(&hub_key, &pubkey)? {
        Some(p) => println!("{}", p.display()),
        None => {
            let expected = clonehome::clone_dir(&hub_slug_for(&root), &hub_key, &role, &pubkey)?;
            println!("not managed yet — this identity has no clone under ~/.confer/clones/.");
            println!("  its managed path would be: {}", expected.display());
            println!(
                "  move it in with:           confer adopt-clone {}",
                root.display()
            );
        }
    }
    Ok(())
}

/// Move an existing clone into the managed home (`confer adopt-clone <path>`).
pub(crate) fn cmd_adopt_clone(path: String, force: bool) -> Result<()> {
    let (dest, moved) = migrate_to_managed(std::path::Path::new(&path), force)?;
    if !moved {
        println!("already at its managed location: {}", dest.display());
        return Ok(());
    }
    let role = config::resolve_role(None, &dest).unwrap_or_default();
    println!("moved into the managed home:\n  {}", dest.display());
    println!("then, from the NEW path ({}):", dest.display());
    println!("  1. re-arm the watch:            confer watch --role {role} --replace");
    println!("  2. re-point skills + autoheal:  confer install-skill");
    println!(
        "     (the old hub path is gone, so the SessionStart hook + /confer-watch skill still"
    );
    println!(
        "      point at it until you re-run install-skill — otherwise a future session goes deaf)"
    );
    Ok(())
}

/// If `url` is a local filesystem path (starts with `/`, `~`, or `.`) that isn't a git repo
/// yet, create a bare hub there and return the expanded absolute path — the zero-dependency
/// CREATE path (no gh auth / no network). git runs without a shell, so a leading `~` is expanded
/// here. Remote URLs (`owner/repo`, `git@…`, `https://…`) pass through unchanged.
fn expand_local_hub(url: String) -> Result<String> {
    let is_local = matches!(url.chars().next(), Some('/') | Some('~') | Some('.'));
    if !is_local {
        return Ok(url);
    }
    let expanded: std::path::PathBuf = if url == "~" {
        config::home()?
    } else if let Some(rest) = url.strip_prefix("~/") {
        config::home()?.join(rest)
    } else {
        std::path::PathBuf::from(&url)
    };
    // Already a repo (bare hub has HEAD; a worktree has .git)? Leave it — clone handles it.
    let is_repo = expanded.join("HEAD").exists() || expanded.join(".git").exists();
    if !is_repo {
        // Only create a hub in a NEW or EMPTY dir — never scatter git plumbing into an existing
        // non-repo directory (e.g. a fat-fingered `confer init ~/.ssh --role x`).
        if expanded.exists()
            && std::fs::read_dir(&expanded)
                .map(|mut d| d.next().is_some())
                .unwrap_or(true)
        {
            return Err(anyhow!(
                "{} already exists and is not a confer hub — pick an empty path for a new local \
                 hub, or point at an existing hub URL",
                expanded.display()
            ));
        }
        std::fs::create_dir_all(&expanded)
            .map_err(|e| anyhow!("cannot create local hub dir {}: {e}", expanded.display()))?;
        let out = std::process::Command::new("git")
            .args(["init", "--bare"])
            .arg(&expanded)
            .output()
            .map_err(|e| anyhow!("could not run `git init --bare`: {e}"))?;
        if !out.status.success() {
            return Err(anyhow!(
                "git init --bare failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        eprintln!("confer: created a local bare hub at {}", expanded.display());
    }
    Ok(expanded.to_string_lossy().into_owned())
}
