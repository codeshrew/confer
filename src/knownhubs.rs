//! `known_hubs` — the hub-identity TOFU pin store (design/35). Confer's `known_hosts` for HUBS: it
//! maps a stable hub name → the pinned identity `(root-commit SHA + last-confirmed-good tip)`. It is
//! split OUT of `config.json` on purpose: if the pin sat next to the routing `url` with the same write
//! authority, one edit would rewrite both and the verify would pass. `~/.confer/known_hubs.json`,
//! confer-only-written, `0600`.
//!
//! The pin is root **plus** a moving confirmed-good tip because a root commit is content-addressed and
//! reproducible for free (anyone with read access can clone + fork-at-root), so a root-only pin proves
//! shared *ancestry*, not *legitimacy*. Verification is by REACHABILITY: the pinned-good tip must be an
//! ancestor of the freshly-fetched HEAD — a rewritten-history fork can't contain it (hard-fail), while
//! a true mirror or normal DAG growth still does (pass). Phase 2 records + reports; the auto-join
//! hard-fail enforcement is phase 3 (gated on designs 33/34).

use crate::{config, gitcmd};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Record {
    /// Root-commit SHA — the topology-proof hub identity.
    pub root: String,
    /// Last-confirmed-good tip SHA — the reachability anchor. Advances on each verified-good check.
    #[serde(default)]
    pub tip: String,
    /// A human established this hub on this machine (the first-sight gate). Invisible-join (phase 3)
    /// only auto-joins a hub carrying this flag; it is NEVER inferred from pin-presence (a crash mid
    /// seed-on-join could otherwise look like a fresh, silently-pinnable hub).
    #[serde(default)]
    pub confirmed: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

pub type Store = BTreeMap<String, Record>;

fn path() -> Result<PathBuf> {
    Ok(config::home()?.join(".confer").join("known_hubs.json"))
}
fn lock_path() -> Result<PathBuf> {
    Ok(config::home()?.join(".confer").join("known_hubs.lock"))
}

/// Tolerant read — any failure degrades to an empty store rather than erroring a read path.
pub fn load() -> Store {
    path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_locked(p: &Path, store: &Store) -> Result<()> {
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(store)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, p)?; // atomic replace, no torn read
    Ok(())
}

/// Locked read-modify-write (load + save one critical section — a co-resident writer must not be able
/// to lose our change). Fail-closed if the lock can't be taken; if the closure `Err`s, nothing writes.
pub fn update_with<T>(f: impl FnOnce(&mut Store) -> Result<T>) -> Result<T> {
    let p = path()?;
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let _guard = config::state_lock(&lock_path()?)
        .ok_or_else(|| anyhow!("could not lock known_hubs (another confer is writing it) — try again"))?;
    let mut store = load();
    let out = f(&mut store)?;
    write_locked(&p, &store)?;
    Ok(out)
}

pub fn get(name: &str) -> Option<Record> {
    load().get(name).cloned()
}

/// The result of checking a hub clone at `root_dir` against the pin store under `name`.
#[derive(Debug)]
pub enum Verdict {
    /// No pin yet — first sight. Recording it needs a human confirm (never silent).
    FirstSight { root: String, tip: String },
    /// Root matches and the pinned-good tip is reachable from HEAD — the pin holds; advance to `new_tip`.
    Match { new_tip: String },
    /// Same name, DIFFERENT root commit — a different repo entirely (redirect/attack).
    RootMismatch { pinned: String, got: String },
    /// Same root, but the pinned-good tip is NOT reachable from HEAD — history rewritten (force-push).
    TipUnreachable { pinned_tip: String },
    /// No commits yet / ambiguous multi-root — not verifiable.
    NotVerifiable(String),
}

/// Check a hub clone against its pin. Read-only (never records/advances — the caller decides based on
/// the verdict, so a human-confirm gate can sit between check and write).
pub fn verify(name: &str, root_dir: &Path) -> Verdict {
    let root = match config::hub_root_strict(root_dir) {
        Ok(config::HubRoot::Commit(sha)) => sha,
        Ok(config::HubRoot::NoCommits) => return Verdict::NotVerifiable("hub has no commits yet".into()),
        Err(e) => return Verdict::NotVerifiable(e.to_string()),
    };
    let head = match gitcmd::output(root_dir, &["rev-parse", "HEAD"]) {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => return Verdict::NotVerifiable("cannot resolve HEAD".into()),
    };
    match load().get(name) {
        None => Verdict::FirstSight { root, tip: head },
        Some(rec) => {
            if rec.root != root {
                return Verdict::RootMismatch { pinned: rec.root.clone(), got: root };
            }
            if rec.tip.is_empty() {
                // A pin with NO recorded tip (a partial/hand-written store) must NOT verify as `Match`
                // — that would skip the reachability check entirely and silently trust ANY same-root
                // history, defeating the whole root+tip design (a same-root fork is free to produce).
                // Fail closed: re-confirm to establish a real tip. (red-team: root-only bypass.)
                return Verdict::NotVerifiable(
                    "pin has no confirmed-good tip — re-confirm with `confer hub repin`".into(),
                );
            }
            if gitcmd::is_ancestor(root_dir, &rec.tip, "HEAD") {
                Verdict::Match { new_tip: head }
            } else {
                Verdict::TipUnreachable { pinned_tip: rec.tip.clone() }
            }
        }
    }
}

/// Record or replace a pin (first-sight or an explicit repin). The CALLER must have obtained the human
/// confirm BEFORE calling this — unlike the keyring's pin-on-first-sight, the known_hubs write blocks
/// on confirmation.
pub fn record(name: &str, root: &str, tip: &str, confirmed: bool) -> Result<()> {
    let (name, root, tip) = (name.to_string(), root.to_string(), tip.to_string());
    update_with(move |store| {
        let rec = store.entry(name).or_default();
        rec.root = root;
        rec.tip = tip;
        rec.confirmed = confirmed;
        Ok(())
    })
}

/// Advance the confirmed-good tip after a verified-good check (a `Match`). Best-effort (never fails a
/// caller); a no-op if the pin is gone or the new tip is empty.
pub fn advance_tip(name: &str, new_tip: &str) {
    let (name, new_tip) = (name.to_string(), new_tip.to_string());
    let _ = update_with(move |store| {
        if let Some(rec) = store.get_mut(&name) {
            if !new_tip.is_empty() {
                rec.tip = new_tip;
            }
        }
        Ok(())
    });
}

/// Forget pins whose name isn't in `keep` (hubs no longer present in the machine config). Returns the
/// forgotten names. The `known_hubs` analog of `autoheal prune` — but pins are cheap + local, so this
/// is a straightforward config-driven cleanup rather than a hub-liveness judgment.
pub fn prune(keep: &BTreeSet<String>) -> Result<Vec<String>> {
    update_with(|store| {
        let gone: Vec<String> = store.keys().filter(|k| !keep.contains(*k)).cloned().collect();
        for g in &gone {
            store.remove(g);
        }
        Ok(gone)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_round_trips_and_tolerates_unknown_fields() {
        let raw = r#"{"codeshrew/agent-coord":{"root":"abc","tip":"def","confirmed":true,"future":9}}"#;
        let store: Store = serde_json::from_str(raw).unwrap();
        let rec = &store["codeshrew/agent-coord"];
        assert_eq!(rec.root, "abc");
        assert_eq!(rec.tip, "def");
        assert!(rec.confirmed);
        assert!(rec.extra.contains_key("future"));
        // round-trips the unknown field
        assert!(serde_json::to_string(&store).unwrap().contains("future"));
    }

    #[test]
    fn prune_forgets_names_not_in_keep() {
        let mut store: Store = BTreeMap::new();
        store.insert("keep-me".into(), Record { root: "r".into(), ..Default::default() });
        store.insert("drop-me".into(), Record { root: "r2".into(), ..Default::default() });
        let keep: BTreeSet<String> = ["keep-me".to_string()].into_iter().collect();
        let gone: Vec<String> = store.keys().filter(|k| !keep.contains(*k)).cloned().collect();
        assert_eq!(gone, vec!["drop-me".to_string()]);
    }

    #[test]
    fn missing_store_loads_empty() {
        // A bad/absent path degrades to an empty store, never an error.
        let store: Store = serde_json::from_str("").ok().unwrap_or_default();
        assert!(store.is_empty());
    }
}
