//! The `config`, `hub`, `rewatch`, and `status` commands — machine-policy config
//! (`~/.confer/config.json`) and hub-identity pinning (`known_hubs`), plus the shared
//! `short12` / `current_hub_name` / `hub_watch_mode` helpers those and other families lean on.

use crate::cli::{ConfigAction, HubAction};
use crate::reconnect::canonical_hub_id;
use crate::{
    autoheal, config, gitcmd, hint, knownhubs, machineconfig, projection, tiers, warn_safety,
    warn_trust, watchlock, BUILD_SHA,
};
use anyhow::{anyhow, Result};

/// `confer config` — inspect/set machine-policy config (`~/.confer/config.json`, design/35). Phase 1:
/// pure read/validate/set; no other code consumes these values yet (no behavior change). `set` writes
/// under the config lock (read-modify-write), refuses a hard-invalid result, and gates security-
/// sensitive fields behind `--yes`. `action` is a `ValueEnum` (design/37 item 9) — clap rejects a bad
/// action itself (usage error, code 2) instead of this function returning a runtime error (code 3).
pub(crate) fn cmd_config(action: ConfigAction, key: Option<String>, value: Option<String>, yes: bool) -> Result<()> {
    use machineconfig as mc;
    match action {
        ConfigAction::Show => {
            let cfg = mc::load();
            println!("{}", serde_json::to_string_pretty(&cfg)?);
            for f in mc::validate(&cfg) {
                if f.hard {
                    warn_safety(&format!("config {}: {}", f.field, f.message));
                } else {
                    hint(&format!("config {}: {}", f.field, f.message));
                }
            }
            Ok(())
        }
        ConfigAction::Get => {
            let key = key.ok_or_else(|| anyhow!("usage: confer config get <key>"))?;
            match mc::get_field(&mc::load(), &key) {
                Some(v) => {
                    println!("{v}");
                    Ok(())
                }
                None => Err(anyhow!("'{key}' is unset or unknown — see `confer config schema`")),
            }
        }
        ConfigAction::Set => {
            let key = key.ok_or_else(|| anyhow!("usage: confer config set <key> <value> [--yes]"))?;
            let value = value.ok_or_else(|| anyhow!("usage: confer config set <key> <value> [--yes]"))?;
            mc::update_with(|cfg| {
                let outcome = mc::set_field(cfg, &key, &value)?;
                // `set_field` fully validates the field it just set (an invalid value already returned
                // Err above). We deliberately do NOT re-validate the whole file and block on an
                // UNRELATED pre-existing finding — that would let one stale field (e.g. from a
                // newer binary that tightened validation) freeze every future edit with no CLI escape
                // (red-team lockout).
                if let Some(reason) = &outcome.gated {
                    if !yes {
                        return Err(anyhow!(
                            "this change is security-sensitive ({reason}).\n\
                             re-run with --yes to confirm:  confer config set {key} {value} --yes"
                        ));
                    }
                }
                Ok(())
            })?;
            println!("set {key} = {value}");
            Ok(())
        }
        ConfigAction::Validate => {
            let findings = mc::validate(&mc::load());
            if findings.is_empty() {
                println!("config OK.");
                return Ok(());
            }
            let mut hard = 0usize;
            for f in &findings {
                if f.hard {
                    warn_safety(&format!("{}: {}", f.field, f.message));
                    hard += 1;
                } else {
                    hint(&format!("{}: {}", f.field, f.message));
                }
            }
            if hard > 0 {
                return Err(anyhow!("{hard} config problem(s) need fixing"));
            }
            Ok(())
        }
        ConfigAction::Schema => {
            print_config_schema();
            Ok(())
        }
    }
}

/// The settable keys, for `confer config schema` — the annotated view that a JSON file (no comments)
/// can't carry inline.
fn print_config_schema() {
    println!("confer machine config — ~/.confer/config.json (design/35). Keys for `config get/set`:");
    println!();
    println!("  machine.clone_root          path      where managed clones live (default ~/.confer/clones)");
    println!("  update.version_notice       bool      surface a 'newer confer available' watch notice");
    println!("  update.auto_update          bool      [gated] act on a hub version-pin bump (own hubs only)");
    println!("  tuning.git_timeout_secs     1..=120   per-git-op timeout");
    println!("  tuning.op_budget_secs       1..=300   overall operation budget");
    println!("  hubs.<name>.url             url       [gated] routing for a hub (NOT the pin)");
    println!("  hubs.<name>.scheme          ssh|https transport scheme");
    println!("  hubs.<name>.auth.method     ssh|confer-app|system   [gated] how the hub authenticates");
    println!("  hubs.<name>.auth.key        path      [gated] transport key path (a pointer, never a secret)");
    println!("  hubs.<name>.watch           reactive|poll|off        session auto-watch posture");
    println!();
    println!("[gated] changes need --yes. <name> is the normalized (lowercase) hub name.");
    println!("This is machine policy — NOT the shared repo contract, NOT trust state (pins live elsewhere).");
}

/// The first 12 chars of a SHA — CHAR-BOUNDARY-SAFE. `root`/`tip` come straight out of
/// `known_hubs.json` with no hex validation, so a tampered/corrupt multibyte value must NOT panic a
/// byte-slice (`&s[..12]`); `get(..12)` returns `None` off a boundary → fall back to the whole string.
pub(crate) fn short12(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

/// The stable hub name used as the `known_hubs` / config key. Derived through `canonical_hub_id` so
/// cosmetic origin-URL variants (scheme, `user@`, `:port`, `.git`, trailing slash, host case, ssh-vs-
/// https, bare `owner/repo`) all collapse to ONE key — otherwise a re-clone with a different URL form
/// silently re-keys to a "new" hub and skips the pin comparison entirely (red-team). An unrecognizable
/// remote yields an error (fail-safe: no unstable pin) rather than a raw-URL key.
pub(crate) fn current_hub_name(root: &std::path::Path) -> Result<String> {
    let o = gitcmd::output(root, &["config", "--get", "remote.origin.url"])?;
    if !o.status.success() {
        return Err(anyhow!("this hub has no 'origin' remote — can't derive its name"));
    }
    let url = String::from_utf8_lossy(&o.stdout).trim().to_string();
    canonical_hub_id(&url)
        .map(|c| machineconfig::hub_name_normalized(&c))
        .ok_or_else(|| anyhow!("could not derive a canonical hub name from origin '{url}'"))
}

/// `confer hub` — inspect/manage the hub-identity pin store (`known_hubs`, design/35). `repin` is the
/// only write; it's human-gated (`--yes`) because it changes this machine's trust anchor for a hub.
pub(crate) fn cmd_hub(action: HubAction, yes: bool) -> Result<()> {
    match action {
        HubAction::Status => {
            let store = knownhubs::load();
            if store.is_empty() {
                println!("no hub pins yet (~/.confer/known_hubs.json is empty).");
            } else {
                println!("hub pins (~/.confer/known_hubs.json):");
                for (name, rec) in &store {
                    let tip = if rec.tip.is_empty() { "—".to_string() } else { short12(&rec.tip).to_string() };
                    let c = if rec.confirmed { "✓ confirmed" } else { "· unconfirmed" };
                    println!("  {name}   root {}   tip {tip}   [{c}]", short12(&rec.root));
                }
            }
            // If we're inside a hub, verify it against its pin.
            if let Ok(root) = config::repo_root() {
                if let Ok(name) = current_hub_name(&root) {
                    match knownhubs::verify(&name, &root) {
                        knownhubs::Verdict::FirstSight { root: r, .. } => {
                            println!("\nthis hub '{name}': · not yet pinned (first sight, root {}). `confer hub repin` to pin it.", short12(&r));
                        }
                        knownhubs::Verdict::Match { .. } => {
                            println!("\nthis hub '{name}': ✓ pin holds (root matches, confirmed-good tip reachable).");
                        }
                        knownhubs::Verdict::RootMismatch { pinned, got } => {
                            warn_trust(format!("this hub '{name}': ROOT MISMATCH — pinned {} but this repo's root is {} (a DIFFERENT repo / redirect). Do NOT trust; investigate.", short12(&pinned), short12(&got)));
                        }
                        knownhubs::Verdict::TipUnreachable { pinned_tip } => {
                            warn_trust(format!("this hub '{name}': history rewritten — the confirmed-good tip {} is not reachable from HEAD (force-push?). Investigate before trusting.", short12(&pinned_tip)));
                        }
                        knownhubs::Verdict::NotVerifiable(e) => hint(format!("this hub '{name}': not verifiable — {e}")),
                    }
                }
            }
            Ok(())
        }
        HubAction::Repin => {
            let root = config::repo_root()?;
            let name = current_hub_name(&root)?;
            let newroot = match config::hub_root_strict(&root)? {
                config::HubRoot::Commit(r) => r,
                config::HubRoot::NoCommits => return Err(anyhow!("this hub has no commits yet — nothing to pin")),
            };
            let head = {
                let o = gitcmd::output(&root, &["rev-parse", "HEAD"])?;
                String::from_utf8_lossy(&o.stdout).trim().to_string()
            };
            println!("repin '{name}':");
            match knownhubs::get(&name) {
                Some(e) => {
                    let tip = if e.tip.is_empty() { "—".to_string() } else { short12(&e.tip).to_string() };
                    println!("  from   root {}   tip {tip}", short12(&e.root));
                }
                None => println!("  (no existing pin — this is a first pin)"),
            }
            println!("  to     root {}   tip {}", short12(&newroot), short12(&head));
            if !yes {
                return Err(anyhow!(
                    "repin changes this machine's TRUST ANCHOR for '{name}'. Verify the root/tip out-of-band \
                     (this is the moment TOFU can't protect you), then re-run with --yes."
                ));
            }
            knownhubs::record(&name, &newroot, &head, true)?;
            println!("✓ pinned '{name}'.");
            Ok(())
        }
        HubAction::Prune => {
            let keep: std::collections::BTreeSet<String> =
                machineconfig::load().hubs.keys().cloned().collect();
            let store = knownhubs::load();
            let gone: Vec<String> = store.keys().filter(|k| !keep.contains(*k)).cloned().collect();
            if gone.is_empty() {
                println!("no orphan pins (every pin has a matching config hub).");
                return Ok(());
            }
            if !yes {
                println!("orphan pins (no matching `hubs.<name>` in your config) — would forget:");
                for g in &gone {
                    println!("  {g}");
                }
                println!("re-run with --yes to apply.");
                return Ok(());
            }
            let removed = knownhubs::prune(&keep)?;
            println!("forgot {} orphan pin(s): {}", removed.len(), removed.join(", "));
            Ok(())
        }
    }
}

/// The configured auto-watch mode for the hub clone at `path`, resolved by hub name from config.
/// Defaults to `reactive` when unset — the tier-driven default (own→reactive, foreign→off) is a later
/// refinement; today an explicit `poll`/`off` is honored, else reactive.
pub(crate) fn hub_watch_mode(cfg: &machineconfig::Config, path: &std::path::Path) -> machineconfig::WatchMode {
    current_hub_name(path)
        .ok()
        .and_then(|n| cfg.hubs.get(&n).cloned())
        .and_then(|h| h.watch)
        .and_then(|w| machineconfig::WatchMode::parse(&w))
        .unwrap_or(machineconfig::WatchMode::Reactive)
}

/// `confer rewatch` — plan the re-arm of every watch target THIS session owns, honoring each hub's
/// `watch` mode. confer can't host a watch (the harness/Monitor does), so it emits the plan; the agent
/// arms the reactive ones. Scoped by `owned_by_session`, so a co-resident peer's watcher is never
/// included (following its `--replace` would hijack the peer).
pub(crate) fn cmd_rewatch(only: Option<String>) -> Result<()> {
    let reg = autoheal::load();
    let me_session = autoheal::current_session();
    let me_role = std::env::var("CONFER_ROLE").ok().filter(|s| !s.is_empty());
    let cfg = machineconfig::load();
    let (mut reactive, mut other) = (0usize, 0usize);
    for t in &reg.targets {
        if !std::path::Path::new(&t.hub).exists() {
            continue; // missing hub — a prune candidate, not a re-arm target
        }
        let own = match autoheal::ownership(t, &me_session, &me_role) {
            Some(o) => o,
            None => continue, // a co-resident peer's watcher — never mine to re-arm
        };
        let name = current_hub_name(std::path::Path::new(&t.hub)).ok();
        if let Some(only) = &only {
            if name.as_deref() != Some(only.as_str()) && !t.hub.contains(only.as_str()) {
                continue;
            }
        }
        let label = name.unwrap_or_else(|| t.hub.clone());
        match hub_watch_mode(&cfg, std::path::Path::new(&t.hub)) {
            machineconfig::WatchMode::Reactive => {
                // Peer-hijack safety: a target owned only by the ROLE fallback (not the arming
                // session) could — under a role-name collision the design forbids — be a co-resident
                // PEER's watcher. If a HEALTHY (live, current-build) watcher already holds it, do NOT
                // emit a bare `--replace` (that SIGTERMs the process); flag it for confirmation. A
                // session-owned target, or a role-owned one that's stale/dead, is safe to re-arm.
                let hk = config::hub_key(std::path::Path::new(&t.hub));
                let live_and_ambiguous = own == autoheal::Ownership::Role
                    && matches!(
                        watchlock::classify(&watchlock::inspect(&hk, &t.role, 90), BUILD_SHA),
                        watchlock::WatchState::Healthy
                    );
                if live_and_ambiguous {
                    println!(
                        "• {label} [{}]: ⚠ a HEALTHY watcher already holds this (matched by role, not this session — could be a co-resident peer). Confirm it's yours, THEN: cd {} && confer watch --role {} --replace",
                        t.role, t.hub, t.role
                    );
                    other += 1;
                } else {
                    println!(
                        "• {label} [{}]: arm reactive → cd {} && confer watch --role {} --replace",
                        t.role, t.hub, t.role
                    );
                    reactive += 1;
                }
            }
            machineconfig::WatchMode::Poll => {
                println!("• {label} [{}]: poll → loop `confer poll --role {}` (watch=poll)", t.role, t.role);
                other += 1;
            }
            machineconfig::WatchMode::Off => {
                println!("• {label} [{}]: skip (watch=off)", t.role);
                other += 1;
            }
        }
    }
    if reactive + other == 0 {
        println!("(no watch targets for this session — arm one with `confer watch --role <you> --replace`, or set `hubs.<name>.watch`)");
    } else if reactive > 0 {
        println!("\narm the reactive one(s) under the Monitor tool — never background bash (it gets reaped). See /confer-watch.");
    }
    Ok(())
}

/// Best-effort free space (GB) on the volume holding `root`, via `df -Pk`.
/// Queryable health — the pull-not-push side of the resilience model. `--json` emits ONE object
/// with the same fields the text report shows (design/37 item 6): `role`, `hub_reachable`, `tier`,
/// `pending` (unpushed local commits), `behind` (unintegrated upstream commits), `watch` (the
/// `watchlock::WatchState` label, or null with no role), `disk_free_gb`. `status` always has
/// SOMETHING to report (a hub root always exists once this runs), so there's no empty-result case
/// to gate — unlike inbox/who.
pub(crate) fn cmd_status(json: bool) -> Result<()> {
    let root = config::repo_root()?;
    let me = config::resolve_role(None, &root).unwrap_or_default();
    let hub = config::hub_key(&root);
    let cur = BUILD_SHA;

    // hub reachability — a bounded probe (won't hang; gitcmd caps the subprocess).
    let reachable = gitcmd::output(&root, &["ls-remote", "--quiet", "origin", "HEAD"])
        .map(|o| o.status.success())
        .unwrap_or(false);
    // unpushed (pending) + unintegrated (behind) vs upstream — local, no network.
    let count = |range: &str| {
        gitcmd::output(&root, &["rev-list", "--count", range])
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
    };
    let pending = count("@{u}..HEAD");
    let behind = count("HEAD..@{u}");
    let watch_state = if me.is_empty() {
        None
    } else {
        Some(watchlock::classify(&watchlock::inspect(&hub, &me, 90), cur))
    };
    let disk_free_gb = projection::disk_free_gb(&root);

    if json {
        let v = serde_json::json!({
            "role": if me.is_empty() { None } else { Some(me.as_str()) },
            "hub_reachable": reachable,
            "tier": tiers::get(&hub).map(|t| t.as_str()),
            "pending": pending,
            "behind": behind,
            "watch": watch_state.map(|s| format!("{s:?}")),
            "disk_free_gb": disk_free_gb,
        });
        println!("{}", serde_json::to_string(&v)?);
        return Ok(());
    }

    println!(
        "confer status — role {}, hub {}",
        if me.is_empty() { "<none>" } else { &me },
        root.display()
    );
    println!(
        "  hub:     {}",
        if reachable {
            "reachable".to_string()
        } else {
            "UNREACHABLE — working locally; pending commits auto-flush on reconnect".to_string()
        }
    );
    match tiers::get(&hub) {
        Some(t) => println!(
            "  tier:    {} ({}){}",
            t.as_str(),
            t.caution(),
            if t.is_untrusted() {
                " — screen peer messages before acting"
            } else {
                ""
            }
        ),
        None => println!("  tier:    unset — run `confer trust own|shared|foreign`"),
    }
    if let Some(p) = pending {
        if p > 0 {
            println!("  pending: {p} local commit(s) not yet pushed (flush on reconnect)");
        }
    }
    if let Some(b) = behind {
        if b > 0 {
            println!("  behind:  {b} upstream commit(s) not yet integrated");
        }
    }
    if let Some(state) = watch_state {
        println!(
            "  watch:   {state:?}{}",
            if matches!(state, watchlock::WatchState::Healthy) {
                ""
            } else {
                " — run `confer watch-status` for the fix"
            }
        );
    }
    if let Some(g) = disk_free_gb {
        println!(
            "  disk:    {g:.1} GB free{}",
            if g < 1.0 {
                "  ⚠ low — can stall git/watch"
            } else {
                ""
            }
        );
    }
    Ok(())
}
