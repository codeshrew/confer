//! `confer arm` — the one paved command to (re-)arm your watcher correctly (design/49).
//!
//! Self-locates your role's clone (the current clone, or the single watch target this session
//! owns in the auto-heal registry), then runs the watch loop with `--replace` and
//! `--delivery monitor` so you can never forget either. It is a long-lived streamer, same as
//! `confer watch` — meant to be hosted under the Monitor tool via the `/confer-arm` skill, which
//! is Monitor-only by construction so the watch can't be backgrounded (the one mistake that sends
//! wakes nowhere). `confer watch` stays the low-level primitive; `arm` is the pit-of-success path.

use crate::{autoheal, config, watch};
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
pub fn run(role: Option<String>) -> Result<()> {
    let clone = resolve_clone(&role)?;
    std::env::set_current_dir(&clone)
        .map_err(|e| anyhow!("confer arm: cannot enter clone {}: {e}", clone.display()))?;
    // The one right way, baked in so it can't be forgotten: take over any orphan (`--replace`),
    // and stamp the delivery method so `watch-status` can affirm we actually deliver wakes. Every
    // other option is the plain `watch` default.
    watch::run(watch::WatchOpts {
        topic: None,
        role,
        json: false,
        poll_secs: 10,
        advance: true,
        replace: true,
        all: false,
        min_priority: 0,
        no_version_notice: false,
        delivery: Some("monitor".to_string()),
    })
}
