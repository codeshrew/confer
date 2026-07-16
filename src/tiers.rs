//! Per-hub **trust tier** — how much to trust the peers on a hub.
//!
//! Stored LOCAL-only (`~/.confer`), never the shared repo, so a peer cannot declare
//! itself trusted (the same discipline as the key pins in `keyring`). It is **advisory**:
//! it scales an agent's caution and tags the untrusted-data envelope, it is NOT an
//! enforcement gate. Defaults: `own` when you `init` a hub, `foreign` when you join/clone
//! someone else's — set on first sight and never clobbered thereafter.

use crate::config;
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// You created/own the hub; peers are your own agents. Highest trust.
    Own,
    /// Co-owned with a trusted collaborator (a cross-owner hub you set up).
    Shared,
    /// You joined someone else's invite; peers are outside your control. Lowest trust.
    Foreign,
}

impl Tier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Own => "own",
            Tier::Shared => "shared",
            Tier::Foreign => "foreign",
        }
    }
    pub fn parse(s: &str) -> Option<Tier> {
        match s.trim().to_lowercase().as_str() {
            "own" => Some(Tier::Own),
            "shared" => Some(Tier::Shared),
            "foreign" => Some(Tier::Foreign),
            _ => None,
        }
    }
    /// A short caution word for display next to a message.
    pub fn caution(&self) -> &'static str {
        match self {
            Tier::Own => "high-trust",
            Tier::Shared => "co-owned",
            Tier::Foreign => "LOW-TRUST",
        }
    }
    /// True for a hub whose peers are outside your control — the ones the screen and
    /// `--ref` gating treat with extra suspicion.
    pub fn is_untrusted(&self) -> bool {
        matches!(self, Tier::Foreign)
    }
}

fn dir() -> Result<PathBuf> {
    Ok(config::home()?.join(".confer"))
}

fn file(base: &Path) -> PathBuf {
    base.join("tiers.json")
}

fn load_all(base: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(file(base))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_all(base: &Path, map: &BTreeMap<String, String>) -> Result<()> {
    std::fs::create_dir_all(base)?;
    let p = file(base);
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(map)?)?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

// ── dir-injectable core (unit-testable) ─────────────────────────────────────────────

fn get_in(base: &Path, hub_key: &str) -> Option<Tier> {
    load_all(base).get(hub_key).and_then(|s| Tier::parse(s))
}

fn set_in(base: &Path, hub_key: &str, tier: Tier) -> Result<()> {
    // Serialize the load-modify-save under a state lock, like keyring/presence — otherwise two
    // concurrent writers (e.g. `join` setting Foreign while `init` set Own, or two roles bootstrapping
    // on one machine) can lost-update: A loads, B loads the same map, B saves, A clobbers B.
    let _guard = crate::config::state_lock(&base.join("tiers.lock"));
    let mut map = load_all(base);
    map.insert(hub_key.to_string(), tier.as_str().to_string());
    save_all(base, &map)
}

fn set_default_in(base: &Path, hub_key: &str, tier: Tier) -> Result<()> {
    // Hold the lock across the whole check-then-set so two concurrent defaults can't both pass the
    // "unset?" test and race. Inlined (not `get_in` + `set_in`) to avoid re-acquiring the same
    // non-reentrant lock. Matches the prior semantics: set only when no PARSEABLE tier is present.
    let _guard = crate::config::state_lock(&base.join("tiers.lock"));
    let mut map = load_all(base);
    if map.get(hub_key).and_then(|s| Tier::parse(s)).is_none() {
        map.insert(hub_key.to_string(), tier.as_str().to_string());
        save_all(base, &map)?;
    }
    Ok(())
}

// ── public API (~/.confer/tiers.json) ───────────────────────────────────────────────

/// The hub's tier, if one has been assigned.
pub fn get(hub_key: &str) -> Option<Tier> {
    dir().ok().and_then(|d| get_in(&d, hub_key))
}

/// Explicitly set the hub's tier (overwrites).
pub fn set(hub_key: &str, tier: Tier) -> Result<()> {
    set_in(&dir()?, hub_key, tier)
}

/// Assign a default tier only if none is set — used at `init`/`join` so a later explicit
/// `confer trust` is never clobbered on a re-run.
pub fn set_default(hub_key: &str, tier: Tier) -> Result<()> {
    set_default_in(&dir()?, hub_key, tier)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("confer-tiers-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn parse_and_roundtrip() {
        assert_eq!(Tier::parse("own"), Some(Tier::Own));
        assert_eq!(Tier::parse("FOREIGN"), Some(Tier::Foreign));
        assert_eq!(Tier::parse("nope"), None);
        assert_eq!(Tier::Own.as_str(), "own");
        assert!(Tier::Foreign.is_untrusted() && !Tier::Own.is_untrusted());
    }

    #[test]
    fn get_set_and_default() {
        let d = tmp();
        assert_eq!(get_in(&d, "hub"), None);
        set_in(&d, "hub", Tier::Foreign).unwrap();
        assert_eq!(get_in(&d, "hub"), Some(Tier::Foreign));
        // set_default must NOT clobber an explicit choice.
        set_default_in(&d, "hub", Tier::Own).unwrap();
        assert_eq!(get_in(&d, "hub"), Some(Tier::Foreign));
        // but it does assign when unset.
        set_default_in(&d, "fresh", Tier::Own).unwrap();
        assert_eq!(get_in(&d, "fresh"), Some(Tier::Own));
    }

    #[test]
    fn tiers_are_per_hub() {
        let d = tmp();
        set_in(&d, "a", Tier::Own).unwrap();
        set_in(&d, "b", Tier::Foreign).unwrap();
        assert_eq!(get_in(&d, "a"), Some(Tier::Own));
        assert_eq!(get_in(&d, "b"), Some(Tier::Foreign));
    }
}
