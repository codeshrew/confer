//! Agent presence / heartbeat on single-writer side refs `refs/presence/<role>`
//!. Each update is a fresh **orphan** commit carrying a tiny JSON
//! record (`last_seen`, `cursor`, host, poll interval), force-pushed to the
//! agent's own ref. Because it lives OFF `main`, it never pollutes the durable
//! message log (which must stay immutable — cursors + signatures depend on it),
//! and because each agent writes only its own ref there is zero cross-agent
//! contention. Readers fetch `refs/presence/*` on demand (it's not in the default
//! refspec, so it never burdens the hot watch loop).
//!
//! Doubling as read-receipts: `cursor` is the commit the agent has consumed up to,
//! so "has X seen message M?" = M.commit is an ancestor of `presence[X].cursor`.

use crate::gitcmd;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Presence {
    pub role: String,
    #[serde(default)]
    pub host: Option<String>,
    /// RFC 3339 UTC of the last heartbeat.
    pub last_seen: String,
    /// Commit consumed up to (the read-receipt frontier).
    #[serde(default)]
    pub cursor: Option<String>,
    /// The agent's watch poll interval — lets `who` scale the liveness window.
    #[serde(default)]
    pub poll_secs: u64,
    /// The confer build this agent is running, as `"<semver> <sha>"` (the pin form).
    /// Published so peers can audit fleet version state without a manual sweep. Optional
    /// for back-compat with pre-build presence records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
}

fn presence_ref(role: &str) -> String {
    format!("refs/presence/{role}")
}

/// Publish this agent's presence: orphan commit → single-file tree → force-push to
/// `refs/presence/<role>`. Best-effort by contract (presence is ephemeral) — the
/// caller treats failure (e.g. offline) as non-fatal. Never touches the index or
/// working tree (pure plumbing), so it can't disturb a concurrent read.
pub fn publish(root: &Path, p: &Presence) -> Result<()> {
    let json = serde_json::to_string(p)?;
    let blob = must(gitcmd::output_stdin(root, &["hash-object", "-w", "--stdin"], &json)?, "hash-object")?;
    // A tree with exactly one entry: presence.json -> blob.
    let tree_input = format!("100644 blob {blob}\tpresence.json\n");
    let tree = must(gitcmd::output_stdin(root, &["mktree"], &tree_input)?, "mktree")?;
    // Parentless commit (no -p) — the ref stays depth-1 forever; the prior commit
    // dangles and the host's gc reclaims it.
    let msg = format!("presence {} {}", p.role, p.last_seen);
    // SIGN the heartbeat with the agent's key so peers can verify liveness
    // — an unsigned/forged beat must not be trusted once a role has a pinned key. Unsigned only
    // when this clone has no key (legacy/advisory).
    let commit = if let Some(key) = crate::config::signing_key(root) {
        let keygen = crate::ssh_keygen_path();
        must(
            gitcmd::output(
                root,
                &[
                    "-c", "gpg.format=ssh",
                    "-c", &format!("user.signingkey={}", key.display()),
                    "-c", &format!("gpg.ssh.program={keygen}"),
                    "commit-tree", &tree, "-S", "-m", &msg,
                ],
            )?,
            "commit-tree -S",
        )?
    } else {
        must(gitcmd::output(root, &["commit-tree", &tree, "-m", &msg])?, "commit-tree")?
    };
    let refname = presence_ref(&p.role);
    gitcmd::check(root, &["update-ref", &refname, &commit])?;
    // Single-writer ref → plain --force is safe (non-fast-forward by construction).
    // (Hardening TODO: --force-with-lease against the last-pushed sha to guard a
    // stale same-agent process; the watch single-lock already makes that rare.)
    let refspec = format!("{commit}:{refname}");
    let push = gitcmd::output(root, &["push", "--force", "origin", &refspec])?;
    if !push.status.success() {
        return Err(anyhow!("presence push: {}", String::from_utf8_lossy(&push.stderr).trim()));
    }
    Ok(())
}

fn must(o: std::process::Output, what: &str) -> Result<String> {
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        Err(anyhow!("{what}: {}", String::from_utf8_lossy(&o.stderr).trim()))
    }
}

/// Load every agent's latest presence record. When `fetch`, pulls `refs/presence/*`
/// from the hub first (on-demand — presence is not in the default refspec).
pub fn load_all(root: &Path, fetch: bool) -> Vec<Presence> {
    if fetch {
        let _ = gitcmd::output(root, &["fetch", "-q", "origin", "+refs/presence/*:refs/presence/*"]);
    }
    let mut out = Vec::new();
    let Ok(o) = gitcmd::output(root, &["for-each-ref", "--format=%(refname)", "refs/presence/"]) else {
        return out;
    };
    for refname in String::from_utf8_lossy(&o.stdout).lines() {
        let spec = format!("{}:presence.json", refname.trim());
        if let Ok(b) = gitcmd::output(root, &["cat-file", "blob", &spec]) {
            if b.status.success() {
                if let Ok(p) = serde_json::from_slice::<Presence>(&b.stdout) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// A presence record plus the trust verdict on the heartbeat that carried it (DESIGN.md
/// Phase 2b). Liveness/cursor/build from an `Untrusted` beat must NOT be believed.
pub struct Beat {
    pub p: Presence,
    pub trust: BeatTrust,
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum BeatTrust {
    /// Verified against the role's pinned key.
    Signed,
    /// No pin for this role yet — advisory (renders, like an unsigned card), not vouched.
    Unsigned,
    /// Has a pin but the beat isn't validly signed by it, OR the timestamp regressed — a forged
    /// or replayed/suppressed heartbeat. The reason is human-facing.
    Untrusted(String),
}

impl BeatTrust {
    /// May its LIVENESS be rendered? (Signed or advisory-Unsigned — not a rejected forge.)
    pub fn ok(&self) -> bool {
        !matches!(self, BeatTrust::Untrusted(_))
    }
    /// May its `build`/`cursor` be BELIEVED for a decision (require --bump, seen)? Only a
    /// cryptographically signed beat — an advisory Unsigned beat's fields are attacker-forgeable
    /// during the rollout window (red-team), so they must not feed version floors or read-receipts.
    pub fn is_signed(&self) -> bool {
        matches!(self, BeatTrust::Signed)
    }
}

fn hwm_path(hub_key: &str) -> Option<std::path::PathBuf> {
    let safe: String = hub_key.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    Some(crate::config::home().ok()?.join(".confer").join("presence_hwm").join(format!("{safe}.json")))
}

/// Per-role presence anchor: the newest `last_seen` we've accepted (monotonicity, to defeat
/// signed-replay SUPPRESSION), and whether this role has EVER signed a beat (per-role presence
/// TOFU — once it has, an unsigned beat is a downgrade; before that, unsigned is advisory so the
/// pre-signing fleet isn't wrongly rejected). Local-only, per hub.
#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
struct Hwm {
    #[serde(default)]
    last_seen: String,
    #[serde(default)]
    ever_signed: bool,
}

fn load_hwm(hub_key: &str) -> std::collections::HashMap<String, Hwm> {
    hwm_path(hub_key)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_hwm(hub_key: &str, updates: &std::collections::HashMap<String, Hwm>) {
    let Some(p) = hwm_path(hub_key) else { return };
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    // Serialize + MERGE under a lock: reload the CURRENT map, fold our updates in monotonically
    // (newer last_seen wins, ever_signed is sticky), then atomic-replace. This stops a concurrent
    // writer's update for a DIFFERENT role being clobbered by our stale snapshot (a review
    // finding) — and the tmp+rename still prevents a torn read (a review probe).
    let _guard = crate::config::state_lock(&p.with_extension("lock"));
    let mut cur = load_hwm(hub_key);
    for (role, u) in updates {
        let e = cur.entry(role.clone()).or_default();
        e.ever_signed |= u.ever_signed;
        let newer = e.last_seen.is_empty()
            || matches!(
                (DateTime::parse_from_rfc3339(&u.last_seen), DateTime::parse_from_rfc3339(&e.last_seen)),
                (Ok(a), Ok(b)) if a > b
            );
        if newer && !u.last_seen.is_empty() {
            e.last_seen = u.last_seen.clone();
        }
    }
    if let Ok(s) = serde_json::to_string(&cur) {
        let tmp = p.with_extension(format!("tmp.{}", std::process::id()));
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

/// Load every agent's latest presence WITH a trust verdict: verify the
/// heartbeat commit's signature against the role's pinned key, and reject a non-monotonic
/// `last_seen`. Callers that make decisions on liveness/cursor/build (`who`, `seen`, `require`)
/// must use this and ignore `Untrusted` beats; `load_all` stays for raw/legacy reads.
pub fn load_verified(root: &Path, hub_key: &str, ros: &crate::roster::Roster, fetch: bool) -> Vec<Beat> {
    if fetch {
        let _ = gitcmd::output(root, &["fetch", "-q", "origin", "+refs/presence/*:refs/presence/*"]);
    }
    let mut cache = crate::verify::Cache::default();
    let mut hwm = load_hwm(hub_key);
    let mut hwm_dirty = false;
    let mut out = Vec::new();
    let now = Utc::now();
    let skew = chrono::Duration::seconds(300);
    let Ok(o) = gitcmd::output(root, &["for-each-ref", "--format=%(objectname) %(refname)", "refs/presence/"]) else {
        return out;
    };
    for line in String::from_utf8_lossy(&o.stdout).lines() {
        let Some((sha, refname)) = line.trim().split_once(' ') else { continue };
        let refname = refname.trim();
        let spec = format!("{refname}:presence.json");
        let Ok(b) = gitcmd::output(root, &["cat-file", "blob", &spec]) else { continue };
        if !b.status.success() {
            continue;
        }
        let Ok(p) = serde_json::from_slice::<Presence>(&b.stdout) else { continue };
        // Bind the record to its ref: the blob's `role` MUST match the ref segment, else it's a
        // beat planted at the wrong ref (a signed beat for A replayed onto refs/presence/B) —
        // skip it, so it can't cry-wolf or duplicate (red-team).
        if refname != format!("refs/presence/{}", p.role) {
            continue;
        }
        let prev = hwm.get(&p.role).cloned().unwrap_or_default();
        let ct = crate::verify::commit_trust(root, hub_key, ros, &mut cache, &p.role, sha);
        // Cryptographically signed by the pinned key? (FirstSight counts — the beat IS signed;
        // the "confirm the key" caveat is an identity/`who` concern, not a beat-authenticity one.)
        let signed = matches!(
            ct,
            crate::verify::Trust::Verified { .. } | crate::verify::Trust::FirstSight { .. }
        );
        // An unsigned beat is a DOWNGRADE (not merely advisory) once the role is EXPECTED to sign:
        // it has a human-CONFIRMED pinned key, or we've ever seen it sign a beat. Deriving the
        // trigger from the confirmed pin — not only local `ever_signed` — closes the "suppress the
        // signed beat to keep the window open" attack (red-team).
        let must_sign = prev.ever_signed || crate::keyring::confirmed(hub_key, &p.role);
        let mut trust = match ct {
            crate::verify::Trust::Verified { .. } | crate::verify::Trust::FirstSight { .. } => BeatTrust::Signed,
            crate::verify::Trust::Mismatch { .. } => BeatTrust::Untrusted("presence key mismatch vs the pin".into()),
            crate::verify::Trust::Unverified { .. } if must_sign => {
                BeatTrust::Untrusted("heartbeat not signed, but this role is expected to sign (downgrade/forge?)".into())
            }
            crate::verify::Trust::Unverified { .. } => BeatTrust::Unsigned,
        };
        let cur_ts = DateTime::parse_from_rfc3339(&p.last_seen).ok().map(|t| t.with_timezone(&Utc));
        let prev_ts = DateTime::parse_from_rfc3339(&prev.last_seen).ok().map(|t| t.with_timezone(&Utc));
        // Monotonicity applies only to a SIGNED beat: an advisory Unsigned beat must never move the
        // high-water mark (else one forged future-dated advisory beat poisons it and rejects every
        // later real beat — red-team HWM poisoning).
        if matches!(trust, BeatTrust::Signed) {
            if let (Some(cur), Some(old)) = (cur_ts, prev_ts) {
                if cur < old {
                    trust = BeatTrust::Untrusted("last_seen went backwards (replay/suppression?)".into());
                }
            }
        }
        // Persist: `ever_signed` from any genuinely signed beat; advance the HWM only from a
        // currently-trusted Signed beat whose timestamp is NEWER and NOT in the future.
        let mut rec = prev;
        if signed && !rec.ever_signed {
            rec.ever_signed = true;
            hwm_dirty = true;
        }
        if matches!(trust, BeatTrust::Signed) {
            if let Some(cur) = cur_ts {
                let not_future = cur <= now + skew;
                let newer = prev_ts.map(|old| cur > old).unwrap_or(true);
                if not_future && newer {
                    rec.last_seen = p.last_seen.clone();
                    hwm_dirty = true;
                }
            }
        }
        hwm.insert(p.role.clone(), rec);
        out.push(Beat { p, trust });
    }
    if hwm_dirty {
        save_hwm(hub_key, &hwm);
    }
    out
}

#[derive(PartialEq, Eq, Debug)]
pub enum Live {
    Up,
    Stale,
    Down,
}

/// Classify liveness from `last_seen` age. The heartbeat publishes every
/// ~max(12×poll, 180)s, so "live" must span ~2 cadences or a healthy watcher
/// flickers to "idle" between beats. Live within max(24×poll, 360s); stale within
/// 30 min; else down.
pub fn liveness(p: &Presence, now: DateTime<Utc>) -> Live {
    let Ok(seen) = DateTime::parse_from_rfc3339(&p.last_seen) else {
        return Live::Down;
    };
    let age = (now - seen.with_timezone(&Utc)).num_seconds();
    if age < 0 {
        // Tolerate small clock skew, but a FAR-future stamp is a forged "fresh forever" beat
        // — don't let it pin the agent Up indefinitely.
        return if age > -300 { Live::Up } else { Live::Down };
    }
    let live_win = (p.poll_secs.max(10) * 24).max(360) as i64;
    if age <= live_win {
        Live::Up
    } else if age <= 1800 {
        Live::Stale
    } else {
        Live::Down
    }
}

/// The glyph for a liveness state (for `who`).
pub fn glyph(l: &Live) -> &'static str {
    match l {
        Live::Up => "●",
        Live::Stale => "○",
        Live::Down => "✕",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(last_seen: &str, poll: u64) -> Presence {
        Presence { role: "x".into(), host: None, last_seen: last_seen.into(), cursor: None, poll_secs: poll, build: None }
    }

    #[test]
    fn liveness_windows() {
        let now = DateTime::parse_from_rfc3339("2026-07-10T12:00:00Z").unwrap().with_timezone(&Utc);
        // 30s ago, poll 10 → within the live window → Up
        assert_eq!(liveness(&p("2026-07-10T11:59:30Z", 10), now), Live::Up);
        // 5 min ago → still within the live window (360s spans ~2 heartbeats) → Up
        assert_eq!(liveness(&p("2026-07-10T11:55:00Z", 10), now), Live::Up);
        // 10 min ago → past live window, within 30min → Stale
        assert_eq!(liveness(&p("2026-07-10T11:50:00Z", 10), now), Live::Stale);
        // 45 min ago → Down
        assert_eq!(liveness(&p("2026-07-10T11:15:00Z", 10), now), Live::Down);
        // unparseable → Down
        assert_eq!(liveness(&p("not-a-date", 10), now), Live::Down);
    }

    #[test]
    fn far_future_stamp_is_not_trusted_as_up() {
        // DESIGN.md Phase 2b: a small clock skew is fine, but a far-future stamp is a forged
        // "fresh forever" beat and must not pin the agent Up.
        let now = DateTime::parse_from_rfc3339("2026-07-10T12:00:00Z").unwrap().with_timezone(&Utc);
        assert_eq!(liveness(&p("2026-07-10T12:02:00Z", 10), now), Live::Up); // +2 min tolerated
        assert_eq!(liveness(&p("2026-07-11T12:00:00Z", 10), now), Live::Down); // +1 day rejected
    }

    #[test]
    fn beat_trust_ok_only_when_not_untrusted() {
        assert!(BeatTrust::Signed.ok());
        assert!(BeatTrust::Unsigned.ok());
        assert!(!BeatTrust::Untrusted("forged".into()).ok());
    }
}
