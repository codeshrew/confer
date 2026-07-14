//! Local, out-of-band **key pinning (TOFU)** — the trust root the read-path signature
//! check verifies against.
//!
//! DESIGN.md review #2: a role's published pubkey lives in `roles/<id>.md` INSIDE the
//! mutable shared repo, so anyone who can write the hub could swap it and then forge a
//! "verified" message signed with their own key. We defend the way `known_hosts` does:
//! the first time we see a role's key we PIN it in local `~/.confer` state — never the
//! repo, so a peer can't rewrite our trust (the same discipline that keeps trust tiers
//! local). Signature verification then checks against the PINNED key, not the card; a
//! later card-side key change is a loud MISMATCH, never a silent re-trust.
//!
//! TOFU's known limit: if the very first card we sync for a role is already malicious,
//! we pin the bad key. It defends against *changes* after first trust, not a key that
//! was hostile before we ever saw the role — exactly like SSH known_hosts.

use crate::config;
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The outcome of presenting a role's currently-published key to the local pin store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pin {
    /// Nothing was pinned before; `pubkey` is now this role's pinned key.
    First,
    /// The presented key matches what we pinned earlier.
    Match,
    /// The presented key DIFFERS from the pinned one — the card changed under us. The
    /// pinned key is retained (never silently overwritten); `pinned` is it.
    Mismatch { pinned: String },
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Entry {
    pubkey: String,
    first_seen: String,
    /// Whether the human has confirmed this key OUT-OF-BAND (DESIGN.md Phase 3 first-sight
    /// guard). A NEW pin starts **unconfirmed** → verification renders it *provisional*
    /// (`FirstSight`) until `confer confirm-key`, so an attacker who wins the TOFU race on an
    /// un-pinned role can't pass as fully `Verified`. Pins written before this field default to
    /// confirmed, so the rollout never retroactively un-trusts an existing pin.
    #[serde(default = "confirmed_default")]
    confirmed: bool,
}

fn confirmed_default() -> bool {
    true
}

fn dir() -> Result<PathBuf> {
    Ok(config::home()?.join(".confer").join("keyring"))
}

fn file(base: &Path, hub_key: &str) -> PathBuf {
    base.join(format!("{hub_key}.json"))
}

fn load_all(base: &Path, hub_key: &str) -> BTreeMap<String, Entry> {
    std::fs::read_to_string(file(base, hub_key))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_all(base: &Path, hub_key: &str, map: &BTreeMap<String, Entry>) -> Result<()> {
    std::fs::create_dir_all(base)?;
    let p = file(base, hub_key);
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(map)?)?;
    std::fs::rename(&tmp, &p)?; // atomic replace, no torn read
    Ok(())
}

/// Compare only algorithm + key material — an ssh pubkey carries a trailing comment
/// (`ssh-ed25519 AAAA… alice@host`) that must not affect identity.
fn normalize(pubkey: &str) -> String {
    let mut it = pubkey.split_whitespace();
    match (it.next(), it.next()) {
        (Some(a), Some(b)) => format!("{a} {b}"),
        _ => pubkey.trim().to_string(),
    }
}

// ── dir-injectable core (unit-testable without touching $HOME) ──────────────────────

fn pin_or_check_in(base: &Path, hub_key: &str, role: &str, pubkey: &str, seen_at: &str) -> Result<Pin> {
    let pubkey = normalize(pubkey);
    // Serialize the whole read-modify-write against concurrent confer processes (a background
    // `watch` + a manual `who`), so one can't load a stale map and clobber another's fresh pin —
    // which would silently drop a pin and let the next read TOFU-re-pin the card's current key
    // with no mismatch (a review finding).
    let guard = crate::config::state_lock(&base.join("keyring.lock"));
    let mut map = load_all(base, hub_key);
    match map.get(role) {
        Some(e) if normalize(&e.pubkey) == pubkey => Ok(Pin::Match),
        Some(e) => Ok(Pin::Mismatch { pinned: e.pubkey.clone() }),
        None => {
            // WRITE path: refuse to write UNLOCKED (a lost update would silently drop a pin — a
            // review finding). Fail closed, matching `gitcmd::lock`'s err-on-timeout, rather than
            // the previous silent-degrade. A read (Match/Mismatch above) still proceeds.
            if guard.is_none() {
                return Err(anyhow::anyhow!(
                    "keyring is locked by another confer process (couldn't acquire it) — retry"
                ));
            }
            // A brand-new pin is UNCONFIRMED until the human vouches for it out-of-band.
            map.insert(role.to_string(), Entry { pubkey, first_seen: seen_at.to_string(), confirmed: false });
            save_all(base, hub_key, &map)?;
            Ok(Pin::First)
        }
    }
}

fn pinned_in(base: &Path, hub_key: &str, role: &str) -> Option<String> {
    load_all(base, hub_key).get(role).map(|e| e.pubkey.clone())
}

fn confirmed_in(base: &Path, hub_key: &str, role: &str) -> bool {
    load_all(base, hub_key).get(role).map(|e| e.confirmed).unwrap_or(false)
}

fn confirm_in(base: &Path, hub_key: &str, role: &str) -> Result<bool> {
    let guard = crate::config::state_lock(&base.join("keyring.lock"));
    let mut map = load_all(base, hub_key);
    match map.get_mut(role) {
        Some(e) => {
            let was = e.confirmed;
            e.confirmed = true;
            if !was {
                if guard.is_none() {
                    return Err(anyhow::anyhow!("keyring is locked by another confer process — retry the confirm"));
                }
                save_all(base, hub_key, &map)?;
            }
            Ok(true)
        }
        None => Ok(false), // nothing pinned for this role yet — nothing to confirm
    }
}

// ── public API (keyed under ~/.confer/keyring) ──────────────────────────────────────

// NB: there is deliberately NO `repin` — a pinned key is IMMUTABLE for the life of the
// identity (the identity IS the key). A differing key is a permanent `Mismatch`,
// never a "rotation" you can accept; a genuinely new agent must use its own role-id.

/// TOFU: compare a role's currently-published `pubkey` against the local pin. First
/// sight pins it (stamped `seen_at`); a match is a no-op; a differing key returns
/// `Mismatch` and leaves the pin untouched — permanently.
pub fn pin_or_check(hub_key: &str, role: &str, pubkey: &str, seen_at: &str) -> Result<Pin> {
    pin_or_check_in(&dir()?, hub_key, role, pubkey, seen_at)
}

/// The pinned pubkey for a role, if any.
pub fn pinned(hub_key: &str, role: &str) -> Option<String> {
    dir().ok().and_then(|d| pinned_in(&d, hub_key, role))
}

/// Has the human confirmed this role's pinned key out-of-band?
pub fn confirmed(hub_key: &str, role: &str) -> bool {
    dir().map(|d| confirmed_in(&d, hub_key, role)).unwrap_or(false)
}

/// Mark a role's pinned key as human-confirmed. Returns false if nothing is pinned yet.
pub fn confirm(hub_key: &str, role: &str) -> Result<bool> {
    confirm_in(&dir()?, hub_key, role)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("confer-keyring-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    const K1: &str = "ssh-ed25519 AAAAKEYMATERIALONE";
    const K2: &str = "ssh-ed25519 AAAAKEYMATERIALTWO";

    #[test]
    fn first_sight_pins_then_matches() {
        let d = tmp();
        assert_eq!(pin_or_check_in(&d, "hub", "alice", K1, "t0").unwrap(), Pin::First);
        assert_eq!(pin_or_check_in(&d, "hub", "alice", K1, "t1").unwrap(), Pin::Match);
        assert_eq!(pinned_in(&d, "hub", "alice").as_deref(), Some(K1));
    }

    #[test]
    fn comment_suffix_is_ignored() {
        let d = tmp();
        pin_or_check_in(&d, "hub", "alice", "ssh-ed25519 AAAAKEYMATERIALONE first@box", "t0").unwrap();
        // Same key material, different trailing comment → still a match.
        assert_eq!(
            pin_or_check_in(&d, "hub", "alice", "ssh-ed25519 AAAAKEYMATERIALONE other@host", "t1").unwrap(),
            Pin::Match
        );
    }

    #[test]
    fn changed_key_is_mismatch_and_pin_is_retained() {
        let d = tmp();
        pin_or_check_in(&d, "hub", "alice", K1, "t0").unwrap();
        // The card's key changed under us (review #2 attack) → loud mismatch, no re-trust.
        assert_eq!(
            pin_or_check_in(&d, "hub", "alice", K2, "t1").unwrap(),
            Pin::Mismatch { pinned: K1.to_string() }
        );
        // The ORIGINAL key is still what's pinned — verification keeps using it.
        assert_eq!(pinned_in(&d, "hub", "alice").as_deref(), Some(K1));
    }

    #[test]
    fn a_mismatch_is_permanent_the_pin_never_moves() {
        // The identity IS the key: there is no repin. Once a key is pinned, a
        // different key stays a Mismatch forever, and the original pin is never overwritten.
        let d = tmp();
        pin_or_check_in(&d, "hub", "alice", K1, "t0").unwrap();
        for t in ["t1", "t2", "t3"] {
            assert_eq!(
                pin_or_check_in(&d, "hub", "alice", K2, t).unwrap(),
                Pin::Mismatch { pinned: K1.to_string() }
            );
        }
        assert_eq!(pinned_in(&d, "hub", "alice").as_deref(), Some(K1), "pin is immutable");
    }

    #[test]
    fn a_new_pin_is_unconfirmed_until_confirmed_but_old_pins_default_confirmed() {
        // DESIGN.md Phase 3 first-sight guard: a freshly-pinned key needs out-of-band confirm.
        let d = tmp();
        assert_eq!(pin_or_check_in(&d, "hub", "peer", K1, "t0").unwrap(), Pin::First);
        assert!(!confirmed_in(&d, "hub", "peer"), "a new pin starts UNconfirmed");
        assert!(confirm_in(&d, "hub", "peer").unwrap(), "confirm succeeds on a pinned role");
        assert!(confirmed_in(&d, "hub", "peer"), "now confirmed");
        // confirming an unpinned role is a no-op (nothing to confirm).
        assert!(!confirm_in(&d, "hub", "nobody").unwrap());

        // Back-compat: a pin file written WITHOUT the `confirmed` field (pre-Phase-3) must
        // deserialize as confirmed, so the rollout never retroactively un-trusts a pin.
        let d2 = tmp();
        std::fs::create_dir_all(&d2).unwrap();
        std::fs::write(
            file(&d2, "hub"),
            r#"{"old":{"pubkey":"ssh-ed25519 AAAAOLD","first_seen":"t0"}}"#,
        ).unwrap();
        assert!(confirmed_in(&d2, "hub", "old"), "a legacy pin (no field) defaults to confirmed");
    }

    #[test]
    fn pins_are_per_role_and_per_hub() {
        let d = tmp();
        pin_or_check_in(&d, "hubA", "alice", K1, "t0").unwrap();
        // Different role, and different hub, are independent slots.
        assert_eq!(pin_or_check_in(&d, "hubA", "carol", K2, "t0").unwrap(), Pin::First);
        assert_eq!(pin_or_check_in(&d, "hubB", "alice", K2, "t0").unwrap(), Pin::First);
        assert_eq!(pinned_in(&d, "hubA", "alice").as_deref(), Some(K1));
    }
}
