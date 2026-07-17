//! Fleet/version audit command handlers: version floor management, fleet-wide
//! version audit, and this hub's own version reporting.

use anyhow::{anyhow, Result};

use crate::{config, gitcmd, presence, roster, schema, version, watchlock};
use crate::{hub_pin, hub_require, my_build, BUILD_SHA, VERSION};

/// The version a LIVE watcher for `role` was built at (from its lock) — the third
/// build layer (watcher / installed / pin). `None` if no watcher is running here.
fn running_watcher_version(root: &std::path::Path, role: &str) -> Option<String> {
    if role.is_empty() {
        return None;
    }
    let info = watchlock::inspect(&config::hub_key(root), role, u64::MAX)?;
    if info.alive {
        info.version
    } else {
        None
    }
}

/// Human-readable update hint for a grade.
pub(crate) fn update_hint(grade: &str) -> &'static str {
    match grade {
        "major" => "⚠ MAJOR update — reconnect to adopt promptly",
        "minor" => "update available (minor) — reconnect to adopt",
        "patch" => "update available (patch) — reconnect when convenient",
        "rebuild" => "newer build available (same version) — reconnect to adopt",
        "drift" => "build drift — reconnect to adopt",
        _ => "",
    }
}

/// Write the hub's version requirement floor to `.confer-require`, commit + push (a
/// maintainer action, same signing policy as `--pin`).
fn write_require(root: &std::path::Path, req: &str) -> Result<()> {
    // Hold the clone lock across add+commit+push so this raw commit serializes against a
    // concurrent watch integrate on the same clone (Hardening A).
    let _lock = gitcmd::lock(root)?;
    std::fs::write(root.join(".confer-require"), format!("{req}\n"))?;
    gitcmd::check(root, &["add", ".confer-require"])?;
    let mut commit: Vec<&str> = Vec::new();
    if config::signing_key(root).is_none() {
        commit.extend(["-c", "commit.gpgsign=false"]);
    }
    let msg = format!("confer: require {req}");
    commit.extend(["commit", "-m", &msg]);
    gitcmd::check(root, &commit)?;
    match gitcmd::output(root, &["push", "origin", "HEAD"]) {
        Ok(o) if o.status.success() => println!("hub now requires {req} (pushed)"),
        _ => println!("hub now requires {req} locally — push failed, flushes on reconnect"),
    }
    Ok(())
}

/// Show or set the hub's version requirement floor (a semver `VersionReq`, the fuzzy
/// repo-level contract). `--bump` raises it to `>=<lowest live-agent version>` — the
/// auto-bump once the whole fleet has moved up (advances only, never lowers).
pub(crate) fn cmd_require(req: Option<String>, bump: bool) -> Result<()> {
    let root = config::repo_root()?;
    if bump {
        // Only TRUSTED heartbeats: a forged/suppressed beat must not be able
        // to skew the version floor and lock a real agent out fleet-wide.
        let roster = roster::load(&root);
        let hub_key = config::hub_key(&root);
        // Only SIGNED beats — a forged `build` on an advisory unsigned beat must not skew the floor.
        let agents: Vec<presence::Presence> =
            presence::load_verified(&root, &hub_key, &roster, true)
                .into_iter()
                .filter(|b| b.trust.is_signed())
                .map(|b| b.p)
                .collect();
        let now = chrono::Utc::now();
        let live: Vec<version::BuildId> = agents
            .iter()
            .filter(|a| presence::liveness(a, now) == presence::Live::Up)
            .filter_map(|a| a.build.as_ref().map(|b| version::BuildId::parse(b)))
            .collect();
        let Some(min) = version::min_version(&live) else {
            return Err(anyhow!(
                "no live agent published a semver build — nothing to bump the floor to"
            ));
        };
        // --bump ADVANCES only. If any live agent is below the current floor, the lowest
        // live build is below it too — bumping to it would LOWER the floor. Refuse and say
        // to update the stragglers first, rather than silently weakening the requirement.
        if let Some(cur) = hub_require(&root) {
            let below = live.iter().filter(|b| !version::satisfies(b, &cur)).count();
            if below > 0 {
                return Err(anyhow!(
                    "{below} live agent(s) are below the current floor {cur} — get them onto >={min}+ before raising it (--bump only advances, never lowers)"
                ));
            }
        }
        let newreq = format!(">={min}");
        if hub_require(&root).map(|r| r.to_string())
            == semver::VersionReq::parse(&newreq)
                .ok()
                .map(|r| r.to_string())
        {
            println!("floor already at the lowest live build ({min}) — nothing to bump.");
            return Ok(());
        }
        return write_require(&root, &newreq);
    }
    match req {
        Some(r) => {
            let parsed = semver::VersionReq::parse(&r)
                .map_err(|e| anyhow!("invalid requirement '{r}': {e}"))?;
            write_require(&root, &parsed.to_string())
        }
        None => {
            match hub_require(&root) {
                Some(r) => println!("hub requires: {r}  (audit with `confer fleet`)"),
                None => println!("hub has no version floor — set one: confer require '>=0.1.0'"),
            }
            Ok(())
        }
    }
}

/// Fleet version audit: each agent's published build (from presence) vs the hub pin and
/// the requirement floor. The "are we up to date" view — computed live from presence, no
/// stored aggregate.
pub(crate) fn cmd_fleet(json: bool) -> Result<()> {
    let root = config::repo_root()?;
    let pin = hub_pin(&root);
    let require = hub_require(&root);
    // Only TRUSTED heartbeats count in the audit — a forged build must not
    // masquerade as a live agent's version.
    let roster = roster::load(&root);
    let hub_key = config::hub_key(&root);
    // The audit VIEW shows every live agent (incl. advisory unsigned beats), so it's useful during
    // the rollout when the fleet hasn't signed yet — it drops only rejected (Untrusted) forgeries.
    // The security-critical ACTION `require --bump` gates on is_signed(); this is just the picture.
    let agents: Vec<presence::Presence> = presence::load_verified(&root, &hub_key, &roster, true)
        .into_iter()
        .filter(|b| b.trust.ok())
        .map(|b| b.p)
        .collect();
    let now = chrono::Utc::now();

    struct Row {
        role: String,
        host: String,
        live: presence::Live,
        build: Option<version::BuildId>,
        grade: &'static str,
        compat: Option<bool>,
        last_seen: String,
        age_secs: Option<i64>,
    }
    let mut rows: Vec<Row> = agents
        .iter()
        .map(|a| {
            let build = a.build.as_ref().map(|b| version::BuildId::parse(b));
            let grade = match &build {
                Some(b) => version::assess(b, pin.as_ref()).grade,
                None => "unknown",
            };
            let compat = match (&build, &require) {
                (Some(b), Some(r)) => Some(version::satisfies(b, r)),
                _ => None,
            };
            // Heartbeat age — the "how connected am I" signal. `None` only if the
            // published `last_seen` fails to parse (shouldn't happen on a real beat).
            let age_secs = chrono::DateTime::parse_from_rfc3339(&a.last_seen)
                .ok()
                .map(|seen| (now - seen.with_timezone(&chrono::Utc)).num_seconds());
            Row {
                role: a.role.clone(),
                host: a.host.clone().unwrap_or_else(|| "?".into()),
                live: presence::liveness(a, now),
                build,
                grade,
                compat,
                last_seen: a.last_seen.clone(),
                age_secs,
            }
        })
        .collect();
    // Live first, then by role.
    rows.sort_by(|x, y| {
        (x.live != presence::Live::Up)
            .cmp(&(y.live != presence::Live::Up))
            .then(x.role.cmp(&y.role))
    });

    if json {
        let arr: Vec<serde_json::Value> = rows
            .iter()
            .map(|r| {
                serde_json::json!({
                    "role": r.role, "host": r.host,
                    "live": matches!(r.live, presence::Live::Up),
                    "build": r.build.as_ref().map(|b| b.label()),
                    "grade": r.grade,
                    "satisfies_floor": r.compat,
                    "last_seen": r.last_seen,
                    "age_secs": r.age_secs,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string(&serde_json::json!({
                "pin": pin.as_ref().map(|p| p.label()),
                "requires": require.as_ref().map(|r| r.to_string()),
                "agents": arr,
            }))?
        );
        return Ok(());
    }

    println!(
        "fleet version audit — hub pins {}{}",
        pin.as_ref()
            .map(|p| p.label())
            .unwrap_or_else(|| "(none)".into()),
        require
            .as_ref()
            .map(|r| format!(" · requires {r}"))
            .unwrap_or_default()
    );
    if rows.is_empty() {
        println!("  (no agent presence yet — peers publish their build on the watch heartbeat)");
        return Ok(());
    }
    for r in &rows {
        let g = presence::glyph(&r.live);
        let word = match r.live {
            presence::Live::Up => "up",
            presence::Live::Stale => "stale",
            presence::Live::Down => "down",
        };
        let age = match r.age_secs {
            Some(s) => format!("{} ago", crate::inbox::fmt_age(s)),
            None => "?".into(),
        };
        let bl = r
            .build
            .as_ref()
            .map(|b| b.label())
            .unwrap_or_else(|| "unknown".into());
        // Flag only a genuine SEMVER-behind build; a sha-only "rebuild" can't be graded
        // ahead-vs-behind without ancestry, so it's not an alarm.
        let flag = if matches!(r.grade, "patch" | "minor" | "major") {
            format!("  [{} behind]", r.grade)
        } else {
            String::new()
        };
        let cflag = if r.compat == Some(false) {
            "  ✗ BELOW FLOOR"
        } else {
            ""
        };
        println!(
            "  {g} {word:<5} {age:>8}  {:<16} {:<12} {bl}{flag}{cflag}",
            r.role,
            schema::sanitize_term(&r.host, false)
        );
    }

    // Summary — the up-to-date verdict, computed from live agents. We lead with build
    // UNIFORMITY (are all reporting agents on the same build?) rather than the exact pin:
    // it's ancestry-free and doesn't false-positive an ahead-of-pin build as "behind".
    // Agents that haven't published a build yet are called out separately,
    // not lumped in as "behind".
    let live: Vec<&Row> = rows
        .iter()
        .filter(|r| r.live == presence::Live::Up)
        .collect();
    let known: Vec<&&Row> = live.iter().filter(|r| r.build.is_some()).collect();
    let unknown = live.len() - known.len();
    let mut builds: Vec<String> = known
        .iter()
        .filter_map(|r| r.build.as_ref().map(|b| b.label()))
        .collect();
    builds.sort();
    builds.dedup();
    println!("\n{} agent(s), {} live.", rows.len(), live.len());
    match builds.len() {
        0 => println!("no live agent has published a build yet (peers report on the watch heartbeat, once on a build-aware binary)."),
        1 => println!("✓ all {} reporting agent(s) are on the same build ({}) — up to date.", known.len(), builds[0]),
        n => println!("⚠ reporting agents are split across {n} builds: {}", builds.join(", ")),
    }
    if unknown > 0 {
        println!("  ({unknown} live agent(s) not yet reporting a build — they'll appear once re-armed on a build-aware binary)");
    }
    // Floor compat + auto-bump hint.
    if let Some(r) = &require {
        let below: Vec<&str> = live
            .iter()
            .filter(|x| x.compat == Some(false))
            .map(|x| x.role.as_str())
            .collect();
        if below.is_empty() {
            println!("✓ all live agents satisfy the floor {r}.");
        } else {
            println!("⚠ below the floor {r}: {}", below.join(", "));
        }
    }
    // Auto-bump hint — only when it would RAISE the floor: every live agent must already
    // satisfy the current floor (nobody below), else the lowest build is below the floor and
    // "bumping" to it would lower it. When some are below, the fix is to update them.
    let any_below = live.iter().any(|r| r.compat == Some(false));
    let live_builds: Vec<version::BuildId> = live.iter().filter_map(|r| r.build.clone()).collect();
    if !any_below {
        if let Some(min) = version::min_version(&live_builds) {
            let suggested = format!(">={min}");
            let already = require.as_ref().map(|r| r.to_string())
                == semver::VersionReq::parse(&suggested)
                    .ok()
                    .map(|r| r.to_string());
            if !already {
                println!("↑ every live agent is ≥ {min} — raise the floor with `confer require --bump` (sets {suggested}).");
            }
        }
    }
    // Local self-check: the presence build above is each agent's RUNNING WATCH version
    // (the watch process stamps its own compiled sha). Separately, is THIS machine's watch
    // running an older build than the binary installed here now? That's the "restart your
    // watch to adopt" signal — a local, immediately-fixable action distinct from the
    // fleet's cross-agent view.
    let me = config::resolve_role(None, &root).unwrap_or_default();
    if let Some(running) = running_watcher_version(&root, &me) {
        if running != BUILD_SHA {
            println!(
                "\n⟳ your watch here is running {running} but {BUILD_SHA} is installed — restart to adopt: `confer watch --role {me} --replace`"
            );
        }
    }
    Ok(())
}

pub(crate) fn cmd_version(json: bool, check: bool, pin: bool) -> Result<()> {
    let built = my_build();
    // Maintainer release action: move the hub pin to this build, commit + push.
    if pin {
        let root = config::repo_root()?;
        // Hold the clone lock across add+commit+push so this raw commit serializes against
        // a concurrent watch integrate on the same clone (Hardening A).
        let _lock = gitcmd::lock(&root)?;
        let s = built.pin_string();
        std::fs::write(root.join(".confer-version"), &s)?;
        let msg = format!("confer: pin hub to {s}");
        gitcmd::check(&root, &["add", ".confer-version"])?;
        // Sign the pin commit only if a signing key is configured (else force it off
        // so a global SSH-signing setup can't block the commit) — same policy as
        // message commits.
        let mut commit: Vec<&str> = Vec::new();
        if config::signing_key(&root).is_none() {
            commit.extend(["-c", "commit.gpgsign=false"]);
        }
        commit.extend(["commit", "-m", &msg]);
        gitcmd::check(&root, &commit)?;
        match gitcmd::output(&root, &["push", "origin", "HEAD"]) {
            Ok(o) if o.status.success() => println!("pinned hub to {s} (pushed)"),
            _ => println!("pinned hub to {s} locally — push failed, flushes on reconnect"),
        }
        return Ok(());
    }
    let root = config::repo_root().ok();
    let pin = root.as_ref().and_then(|r| hub_pin(r));
    let a = version::assess(&built, pin.as_ref());
    // Third layer: is a running watcher on an OLDER build than this binary?
    let watcher = root.as_ref().and_then(|r| {
        let me = config::resolve_role(None, r).unwrap_or_default();
        running_watcher_version(r, &me)
    });

    if json {
        let mut v = serde_json::json!({
            "built": { "version": env!("CARGO_PKG_VERSION"), "sha": BUILD_SHA },
            "grade": a.grade,
            "outdated": a.outdated,
        });
        if let Some(p) = &pin {
            v["pin"] = serde_json::json!({
                "version": p.version.as_ref().map(|x| x.to_string()),
                "sha": p.sha,
            });
        }
        if let Some(w) = &watcher {
            v["running_watcher"] = serde_json::json!(w);
        }
        println!("{}", serde_json::to_string(&v)?);
    } else {
        println!("confer {VERSION}");
        match &pin {
            None => println!("hub pin: none"),
            Some(p) => match a.grade {
                "current" => println!("hub pin: {} — current", p.label()),
                "ahead" => println!("hub pin: {} — you're ahead (fine)", p.label()),
                _ => {
                    println!(
                        "hub pin: {} — {} ({})",
                        p.label(),
                        a.grade,
                        update_hint(a.grade)
                    );
                    println!("adopt:   confer reconnect --role <you>");
                }
            },
        }
        // Surface the three layers only when the running watcher lags this binary.
        if let Some(w) = &watcher {
            if w != BUILD_SHA && !w.is_empty() {
                println!("watcher: running an older build ({w}) than this binary ({BUILD_SHA}) — re-arm with `confer watch --replace`");
            }
        }
    }

    // `version --check` is the scriptable gate: exit 1 when this build is behind the hub pin (a valid
    // negative predicate result), exit 3 if the check itself failed (Err raised earlier). The bare
    // `version` report always exits 0. (design/37 — no mid-stack process::exit.)
    if check && a.outdated {
        return Err(crate::PredicateFalse.into());
    }
    Ok(())
}

