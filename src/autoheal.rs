//! Auto-heal registry + toggle for the SessionStart hook. Records
//! whether auto-heal is enabled and which `(hub, role)` targets to keep alive.
//! Lives at `~/.confer/autoheal.json`. The hook (`session-heal`) reads this; the
//! toggle (`autoheal on|off`) and `watch` (auto-register) write it.

use crate::config;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default)]
pub struct Registry {
    pub enabled: bool,
    #[serde(default)]
    pub targets: Vec<Target>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Target {
    /// Absolute hub working-directory path (so the nudge can `cd` there).
    pub hub: String,
    pub role: String,
    /// The `CLAUDE_CODE_SESSION_ID` that last armed this watcher — LOCAL-ONLY (never
    /// shared to the hub), used to scope SessionStart healing to the OWNING session so a
    /// resuming agent never nudges a co-resident peer's watcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

/// The Claude session driving this process, if any. Purely local — read from the env,
/// never persisted anywhere the hub can see.
pub fn current_session() -> Option<String> {
    std::env::var("CLAUDE_CODE_SESSION_ID").ok().filter(|s| !s.is_empty())
}

fn path() -> Result<PathBuf> {
    Ok(config::home()?.join(".confer").join("autoheal.json"))
}

pub fn load() -> Registry {
    path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(r: &Registry) -> Result<()> {
    let p = path()?;
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, serde_json::to_string_pretty(r)?)?;
    Ok(())
}

pub fn set_enabled(on: bool) -> Result<()> {
    let mut r = load();
    r.enabled = on;
    save(&r)
}

/// Record a `(hub, role)` as something to keep watched, stamping the CURRENT session
/// as its owner. Re-arming in a new session **takes ownership** (so the resuming
/// session — not the one that first armed it — is who `session-heal` nudges). Idempotent,
/// best-effort (never fails a caller — e.g. `watch` startup).
pub fn add_target(hub: &str, role: &str) {
    if role.is_empty() {
        return;
    }
    let mut r = load();
    let session = current_session();
    if let Some(t) = r.targets.iter_mut().find(|t| t.hub == hub && t.role == role) {
        if t.session != session {
            t.session = session;
            let _ = save(&r);
        }
    } else {
        r.targets.push(Target { hub: hub.to_string(), role: role.to_string(), session });
        let _ = save(&r);
    }
}

/// Re-point any watch-liveness target from an OLD hub path to a NEW one — used when a clone
/// moves into the managed home so healing tracks it. Best-effort.
pub fn retarget(old: &str, new: &str) {
    let mut r = load();
    let mut changed = false;
    for t in &mut r.targets {
        if t.hub == old {
            t.hub = new.to_string();
            changed = true;
        }
    }
    if changed {
        let _ = save(&r);
    }
}

/// Watch-liveness targets whose hub directory is currently MISSING — the candidates a human
/// reviews before pruning. Read-only; never deletes.
pub fn stale_targets() -> Vec<Target> {
    load()
        .targets
        .into_iter()
        .filter(|t| !std::path::Path::new(&t.hub).exists())
        .collect()
}

/// Remove watch-liveness targets whose hub directory no longer exists. Returns the removed
/// targets. **Manual + human-verified only** (`confer autoheal prune`) — deliberately NEVER
/// automatic: a transiently-absent hub (unmounted volume, offline network FS, a clone mid-move)
/// must not silently drop a live watcher. Touches only the ephemeral registry — never identity,
/// keys, roster, or role cards.
pub fn prune() -> Vec<Target> {
    let mut r = load();
    let (live, dead): (Vec<Target>, Vec<Target>) = r
        .targets
        .drain(..)
        .partition(|t| std::path::Path::new(&t.hub).exists());
    if !dead.is_empty() {
        r.targets = live;
        let _ = save(&r);
    }
    dead
}

/// Should this session heal (re-arm) a target? Owned when it carries MY session id (I armed it
/// this session), or — as the resume / session-id-rotation fallback — when it's for MY role (the
/// registry is per-machine, so every entry is already on this host). A co-resident PEER's target
/// (different role AND different session) is excluded — the scoping fix. Legacy `None`-session
/// targets are healed ONLY via the role match (then self-stamp on re-arm), so a co-resident peer
/// is never told to re-arm another role's watcher. With neither my session nor my role known we
/// stay conservative and own nothing — better a missed nudge than hijacking a peer.
pub fn owned_by_session(
    t: &Target,
    me_session: &Option<String>,
    me_role: &Option<String>,
) -> bool {
    if let Some(s) = me_session {
        if t.session.as_deref() == Some(s.as_str()) {
            return true;
        }
    }
    if let Some(r) = me_role {
        if &t.role == r {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(session: Option<&str>, role: &str) -> Target {
        Target { hub: "/x".into(), role: role.into(), session: session.map(String::from) }
    }

    #[test]
    fn scoping_owns_my_session_and_my_role_but_never_a_peer() {
        let me_s = Some("sess-A".to_string());
        let me_r = Some("alice".to_string());
        // my own watcher (session matches) — heal it
        assert!(owned_by_session(&t(Some("sess-A"), "alice"), &me_s, &me_r));
        // MY role but a stale/rotated session id (e.g. after --resume) — reclaim via role
        assert!(owned_by_session(&t(Some("sess-OLD"), "alice"), &me_s, &me_r));
        // legacy None-session target for MY role — heal via role (then self-stamps)
        assert!(owned_by_session(&t(None, "alice"), &me_s, &me_r));
        // a co-resident PEER (different role + different session) — NEVER (the fix)
        assert!(!owned_by_session(&t(Some("sess-B"), "carol"), &me_s, &me_r));
        // a legacy None-session target for a PEER role — NOT healed (no cross-role hijack)
        assert!(!owned_by_session(&t(None, "carol"), &me_s, &me_r));
    }

    #[test]
    fn no_identity_owns_nothing_rather_than_over_listing() {
        // Can't identify this session (no session id, no role) → own nothing, so a resuming
        // agent never nudges a peer. Conservative on purpose.
        let none: Option<String> = None;
        assert!(!owned_by_session(&t(Some("sess-A"), "alice"), &none, &none));
        assert!(!owned_by_session(&t(None, "alice"), &none, &none));
        // role-only (session unknown) still scopes to my role
        let me_r = Some("alice".to_string());
        assert!(owned_by_session(&t(None, "alice"), &none, &me_r));
        assert!(!owned_by_session(&t(None, "carol"), &none, &me_r));
    }
}
