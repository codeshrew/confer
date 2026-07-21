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

/// The session env keys we recognize, in priority order, one per supported agent harness
/// (design/52 axis 1). Extend this list to add a harness — the read sites don't change.
pub const SESSION_ENV_KEYS: &[&str] = &["CLAUDE_CODE_SESSION_ID", "GROK_SESSION_ID"];

/// First non-empty value among `keys`, via the provided getter. Pure (no env) so it's testable;
/// `current_session` wires it to the real environment.
pub(crate) fn session_from(get: impl Fn(&str) -> Option<String>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| get(k).filter(|s| !s.is_empty()))
}

/// The agent session driving this process, if any (Claude Code, Grok Build, …). Purely local — read
/// from the env, never persisted anywhere the hub can see. Harness-agnostic: tries each runtime's
/// session env var, then a per-harness on-disk fallback. NOTE: a session var present in a HOOK
/// process may be ABSENT in the interactive/monitor-hosted `arm`/`watch` process — hence the disk
/// fallback (Grok) and the hook stdin `sessionId`/`session_id` field (see `cmd_session_heal`).
pub fn current_session() -> Option<String> {
    session_from(|k| std::env::var(k).ok(), SESSION_ENV_KEYS).or_else(grok_session_from_disk)
}

/// Grok Build (≤0.2.106) does NOT expose the session id as an env var to the agent shell or a
/// monitor-hosted process — only to hook processes — but it records the live session on disk. When
/// we detect we're under Grok (`GROK_AGENT`) and the env gave us nothing, recover the id from
/// `~/.grok/active_sessions.json`, matched NARROWLY so co-resident Grok sessions don't collide. Only
/// touches Grok's files when actually under Grok; returns None (caller warns) rather than guess.
fn grok_session_from_disk() -> Option<String> {
    // Only under Grok — never read another runtime's private files.
    std::env::var("GROK_AGENT").ok().filter(|s| !s.is_empty())?;
    let path = config::home().ok()?.join(".grok").join("active_sessions.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let cwd = std::env::current_dir().ok();
    pick_grok_session(&v, &ancestor_pids(), cwd.as_deref().and_then(|p| p.to_str()))
}

/// Choose the ONE session id from Grok's `active_sessions.json` that belongs to this process — pure
/// (no env / fs / /proc) so it's testable. Priority: (1) the entry whose `pid` is an ANCESTOR of this
/// process (the session that spawned us — precise under multi-session); (2) the SOLE entry sharing
/// our `cwd`; (3) the SOLE entry overall. Ambiguous ⇒ None (never a wrong guess). Accepts either a
/// JSON array of entries or a single object.
fn pick_grok_session(
    v: &serde_json::Value,
    ancestors: &std::collections::HashSet<u32>,
    cwd: Option<&str>,
) -> Option<String> {
    let entries: Vec<&serde_json::Value> = match v {
        serde_json::Value::Array(a) => a.iter().collect(),
        serde_json::Value::Object(_) => vec![v],
        _ => return None,
    };
    if entries.is_empty() {
        return None;
    }
    let sid = |e: &serde_json::Value| e.get("session_id").and_then(|x| x.as_str()).map(String::from);
    // (1) pid-ancestor match.
    if !ancestors.is_empty() {
        let m: Vec<&&serde_json::Value> = entries
            .iter()
            .filter(|e| e.get("pid").and_then(|p| p.as_u64()).is_some_and(|p| ancestors.contains(&(p as u32))))
            .collect();
        if let [only] = m.as_slice() {
            return sid(only);
        }
    }
    // (2) sole entry sharing our cwd.
    if let Some(cwd) = cwd {
        let m: Vec<&&serde_json::Value> = entries
            .iter()
            .filter(|e| e.get("cwd").and_then(|c| c.as_str()) == Some(cwd))
            .collect();
        if let [only] = m.as_slice() {
            return sid(only);
        }
    }
    // (3) sole entry overall.
    if let [only] = entries.as_slice() {
        return sid(only);
    }
    None
}

/// This process's ancestor pids (self + parents up the tree) via `/proc` (Linux — Grok's platform;
/// empty elsewhere, e.g. macOS, where the Grok path never runs anyway). Bounded + loop-guarded.
fn ancestor_pids() -> std::collections::HashSet<u32> {
    let mut set = std::collections::HashSet::new();
    let mut pid = std::process::id();
    for _ in 0..64 {
        if !set.insert(pid) {
            break; // cycle guard
        }
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            break;
        };
        // `pid (comm) state ppid …` — comm may hold spaces/parens, so read the fields AFTER the
        // last ')': index 0 = state, index 1 = ppid.
        let Some((_, after)) = stat.rsplit_once(')') else { break };
        let Some(ppid) = after.split_whitespace().nth(1).and_then(|p| p.parse::<u32>().ok()) else {
            break;
        };
        if ppid == 0 || ppid == pid {
            break;
        }
        pid = ppid;
    }
    set
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
pub fn add_target(hub: &str, role: &str, session_override: Option<String>) {
    if role.is_empty() {
        return;
    }
    let mut r = load();
    // Precedence: explicit `--session` > env/disk auto-detection. Under a harness that hides the
    // session from this process (Grok Build) with no override, note it — ownership falls back to
    // role-only, which is unsafe for co-resident same-role multi-session (design/52).
    let session = session_override.or_else(current_session);
    if session.is_none() && std::env::var("GROK_AGENT").is_ok() {
        crate::hint(
            "session id unknown (Grok exposes it only to hooks) — watch ownership is role-only; \
             fine for a single session, but pass `confer arm --session <id>` (e.g. from \
             ~/.grok/active_sessions.json) if you run several Grok sessions on this machine.",
        );
    }
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
/// WHY a target counts as owned — the caller needs this to stay safe. `Session` = the arming session
/// id matched, so it is unambiguously mine (safe to auto-`--replace`). `Role` = only the role matched
/// (a post-`--resume`/rotation reclaim — OR, under a role-name collision the design forbids, possibly a
/// co-resident PEER's watcher). The two are informationally indistinguishable from the registry, so a
/// `Role`-owned target that is HEALTHY must NOT be blindly `--replace`d (peer-hijack red-team: killing
/// a live process on spec is the actual harm). Session-heal already skips healthy; `rewatch` must too.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ownership {
    Session,
    Role,
}

pub fn ownership(
    t: &Target,
    me_session: &Option<String>,
    me_role: &Option<String>,
) -> Option<Ownership> {
    if let Some(s) = me_session {
        if t.session.as_deref() == Some(s.as_str()) {
            return Some(Ownership::Session);
        }
    }
    if let Some(r) = me_role {
        if &t.role == r {
            return Some(Ownership::Role);
        }
    }
    None
}

pub fn owned_by_session(
    t: &Target,
    me_session: &Option<String>,
    me_role: &Option<String>,
) -> bool {
    ownership(t, me_session, me_role).is_some()
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
    fn session_from_tries_each_harness_key_in_order() {
        // Grok key present, Claude absent → Grok session (design/52 axis 1).
        let g = |k: &str| (k == "GROK_SESSION_ID").then(|| "grok-1".to_string());
        assert_eq!(session_from(g, SESSION_ENV_KEYS).as_deref(), Some("grok-1"));
        // Both present → the first listed key (Claude) wins, deterministically.
        let both = |k: &str| match k {
            "CLAUDE_CODE_SESSION_ID" => Some("cc-1".to_string()),
            "GROK_SESSION_ID" => Some("grok-1".to_string()),
            _ => None,
        };
        assert_eq!(session_from(both, SESSION_ENV_KEYS).as_deref(), Some("cc-1"));
        // An empty value is skipped, not treated as a session.
        let empty_claude = |k: &str| match k {
            "CLAUDE_CODE_SESSION_ID" => Some(String::new()),
            "GROK_SESSION_ID" => Some("grok-1".to_string()),
            _ => None,
        };
        assert_eq!(session_from(empty_claude, SESSION_ENV_KEYS).as_deref(), Some("grok-1"));
        // Nothing set anywhere → None (no identity).
        assert_eq!(session_from(|_| None, SESSION_ENV_KEYS), None);
    }

    #[test]
    fn pick_grok_session_matches_narrowly_or_declines() {
        use std::collections::HashSet;
        let j = serde_json::json!([
            {"session_id": "s-anc", "pid": 4242, "cwd": "/w/a"},
            {"session_id": "s-cwd", "pid": 9999, "cwd": "/w/b"},
        ]);
        // (1) pid-ancestor wins, even when cwd points elsewhere.
        let anc: HashSet<u32> = [1u32, 4242].into_iter().collect();
        assert_eq!(pick_grok_session(&j, &anc, Some("/w/zzz")).as_deref(), Some("s-anc"));
        // (2) no ancestor match → the SOLE entry sharing our cwd.
        let none: HashSet<u32> = HashSet::new();
        assert_eq!(pick_grok_session(&j, &none, Some("/w/b")).as_deref(), Some("s-cwd"));
        // ambiguous (no ancestor, cwd matches nothing, >1 entry) → None, never a guess.
        assert_eq!(pick_grok_session(&j, &none, Some("/w/zzz")), None);
        // (3) a single object (not array), no ancestor/cwd match → the sole entry.
        let one = serde_json::json!({"session_id": "solo", "pid": 7, "cwd": "/w/x"});
        assert_eq!(pick_grok_session(&one, &none, Some("/nope")).as_deref(), Some("solo"));
        // empty / non-object-or-array → None.
        assert_eq!(pick_grok_session(&serde_json::json!([]), &none, None), None);
        assert_eq!(pick_grok_session(&serde_json::json!("nope"), &none, None), None);
    }

    #[test]
    fn ownership_basis_distinguishes_session_from_role_fallback() {
        // The basis matters for safety: Session = unambiguously mine (safe to auto --replace);
        // Role = a resume/rotation reclaim OR a co-resident peer under a role collision (rewatch must
        // not blindly kill a HEALTHY such watcher — peer-hijack red-team).
        let me_s = Some("sess-A".to_string());
        let me_r = Some("alice".to_string());
        assert_eq!(ownership(&t(Some("sess-A"), "alice"), &me_s, &me_r), Some(Ownership::Session));
        // my role but a rotated/stale session id → Role (indistinguishable from a live peer's id)
        assert_eq!(ownership(&t(Some("sess-OLD"), "alice"), &me_s, &me_r), Some(Ownership::Role));
        assert_eq!(ownership(&t(None, "alice"), &me_s, &me_r), Some(Ownership::Role));
        // a co-resident peer → owned by neither basis
        assert_eq!(ownership(&t(Some("sess-B"), "carol"), &me_s, &me_r), None);
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
