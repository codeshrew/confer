//! `confer arm` — the one paved command to (re-)arm your watcher correctly (design/49).
//!
//! Self-locates your role's clone (the current clone, or the single watch target this session
//! owns in the auto-heal registry), then runs the watch loop with `--replace` and
//! `--delivery monitor` so you can never forget either. It is a long-lived streamer, same as
//! `confer watch` — meant to be hosted under the Monitor tool via the `/confer-arm` skill, which
//! is Monitor-only by construction so the watch can't be backgrounded (the one mistake that sends
//! wakes nowhere). `confer watch` stays the low-level primitive; `arm` is the pit-of-success path.

use crate::{autoheal, config, watch, watchlock, BUILD_SHA};
use anyhow::{anyhow, Result};
use std::path::PathBuf;

/// Resolve the clone dir to arm. The current dir wins when it's inside a confer hub clone (the
/// unambiguous common case); otherwise fall back to a watch target this session (or the named
/// role) owns in the auto-heal registry. Refuses to guess across multiple owned targets — asks
/// for a `cd` or `--role` instead, so `arm` never arms the wrong role's watcher.
fn resolve_clone(role: &Option<String>) -> Result<PathBuf> {
    // 1. CWD is itself a hub clone → unambiguous, use it (mirrors how `watch` resolves).
    if let Ok(root) = config::repo_root() {
        return Ok(root);
    }
    // 2. Fall back to the watch-registry targets this session/role owns (the post-compaction
    //    case, where the agent's cwd isn't its clone). `owned_by_session` never returns a
    //    co-resident peer's target, so this can't arm someone else's watcher.
    let reg = autoheal::load();
    let me_session = autoheal::current_session();
    let mut owned: Vec<&autoheal::Target> = reg
        .targets
        .iter()
        .filter(|t| autoheal::owned_by_session(t, &me_session, role))
        .filter(|t| role.as_ref().is_none_or(|r| &t.role == r))
        .collect();
    owned.sort_by(|a, b| a.hub.cmp(&b.hub).then(a.role.cmp(&b.role)));
    owned.dedup_by(|a, b| a.hub == b.hub && a.role == b.role);

    match owned.as_slice() {
        [] => Err(anyhow!(
            "confer arm: not inside a confer clone, and no watch target owned by this session.\n\
             cd into your role's clone and re-run (see `confer clones`), pass `--role <r>`, or \
             `confer reconnect`."
        )),
        [t] => Ok(PathBuf::from(&t.hub)),
        many => {
            let list = many
                .iter()
                .map(|t| format!("  • {} @ {}", t.role, t.hub))
                .collect::<Vec<_>>()
                .join("\n");
            Err(anyhow!(
                "confer arm: several watch targets are owned here — cd into the one you mean, or \
                 pass `--role <r>`:\n{list}"
            ))
        }
    }
}

/// Arm (or re-arm) the watcher the one correct way. Locates the clone, enters it, and streams
/// wakes with `--replace` + `--delivery monitor` baked in. Long-lived: returns only when the
/// watch loop ends (killed / replaced).
///
/// `topic`/`all`/`min_priority`/`wake_on` resolve exactly like `confer watch`'s own flags — explicit
/// CLI > saved per-(hub,role) machine-config preference > built-in default — and an explicit flag here
/// saves the resolved bundle, so the NEXT bare `arm` (including the post-compaction auto-heal re-arm)
/// reloads it without re-deciding (design/51 §6/Phase B).
pub fn run(
    role: Option<String>,
    topic: Option<String>,
    all: bool,
    min_priority: Option<String>,
    wake_on: Option<String>,
    session: Option<String>,
    force: bool,
) -> Result<()> {
    let clone = resolve_clone(&role)?;
    std::env::set_current_dir(&clone)
        .map_err(|e| anyhow!("confer arm: cannot enter clone {}: {e}", clone.display()))?;
    let hub_key = config::hub_key(&clone);
    let resolved_role = config::resolve_role(role.clone(), &clone).unwrap_or_default();

    // H2 — don't blind-replace a HEALTHY watcher this session can't confirm it owns. `resolve_clone`
    // gates which target we pick, but `replace: true` below would still SIGTERM whatever holds the
    // (hub, role) lock — including a co-resident peer that armed the SAME role from a different
    // session (common under Grok, where the session id is often absent so ownership falls back to
    // role-only). A dead/stale/OUTDATED watcher is NOT Healthy, so the normal re-arm/resume/adopt-a-
    // -new-build paths are unaffected; only a live, fresh, current-build watcher trips this.
    if !resolved_role.is_empty() {
        let state = watchlock::classify(&watchlock::inspect(&hub_key, &resolved_role, 90), BUILD_SHA);
        // Confirmed ours only if a registry target for (hub, role) carries OUR session stamp
        // (explicit `--session`, or a matching auto-detected id) — never the role-only fallback.
        let sess = session.clone().or_else(autoheal::current_session);
        let session_confirmed = sess.as_deref().is_some_and(|s| {
            autoheal::load().targets.iter().any(|t| {
                config::hub_key(std::path::Path::new(&t.hub)) == hub_key
                    && t.role == resolved_role
                    && t.session.as_deref() == Some(s)
            })
        });
        if !may_replace(state, session_confirmed, force) {
            return Err(anyhow!(
                "confer arm: a HEALTHY watcher for role '{resolved_role}' is already running on this \
                 machine, and this session isn't confirmed as its owner (no matching session stamp). \
                 Refusing to replace it — that would steal a co-resident peer's cursor. If it IS yours, \
                 re-run with `--session <your-session-id>` or `--force`; if a peer owns it, leave it \
                 alone (you're already covered)."
            ));
        }
    }
    let (wake_on, min_priority, topic, all) = watch::resolve_watch_prefs(
        &hub_key,
        &resolved_role,
        wake_on.as_deref(),
        min_priority.as_deref(),
        topic.as_deref(),
        all,
    )?;
    // The one right way, baked in so it can't be forgotten: take over any orphan (`--replace`),
    // and stamp the delivery method so `watch-status` can affirm we actually deliver wakes. Every
    // other option is the plain `watch` default.
    watch::run(watch::WatchOpts {
        topic,
        role,
        json: false,
        poll_secs: 10,
        advance: true,
        replace: true,
        all,
        min_priority,
        wake_on,
        no_version_notice: false,
        delivery: Some("monitor".to_string()),
        session,
    })
}

/// May `arm` replace the current (hub, role) watcher? (H2) Yes unless it's a strictly-`Healthy`
/// watcher this session can't confirm it owns — that case is almost certainly a live co-resident
/// peer (or your own already-fine watcher), so replacing it would steal a cursor. `--force` and a
/// confirmed session stamp both override; any non-`Healthy` state (dead / stale / outdated / other
/// host / not-watching) always replaces, so normal re-arm/resume/adopt-a-new-build is unaffected.
fn may_replace(state: watchlock::WatchState, session_confirmed: bool, force: bool) -> bool {
    force || session_confirmed || !matches!(state, watchlock::WatchState::Healthy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use watchlock::WatchState::*;

    #[test]
    fn may_replace_protects_only_an_unconfirmed_healthy_watcher() {
        // The one refusal: a healthy watcher we can't confirm is ours, without --force.
        assert!(!may_replace(Healthy, false, false), "unconfirmed healthy → refuse");
        // Overrides.
        assert!(may_replace(Healthy, true, false), "session-confirmed healthy → replace");
        assert!(may_replace(Healthy, false, true), "--force healthy → replace");
        // Every non-healthy state replaces freely (normal re-arm / resume / adopt-new-build).
        for s in [NotWatching, Stale, Outdated, OtherHost] {
            assert!(may_replace(s, false, false), "{s:?} always replaces");
        }
    }
}
