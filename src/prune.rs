//! Surveying and clearing machine-local debris: watch-registry entries that no longer describe
//! anything, and per-hub state filed under hub ids nothing uses any more.
//!
//! `confer autoheal prune` used to test exactly one thing — does the registered directory still
//! exist — which left two kinds of residue behind.
//!
//! **A registered path that was never a hub.** `/Users/sk/git/book-business` is a project repo with
//! no `threads/` or `roles/`. It survived every prune because the directory *does* exist, so it sat
//! in the registry generating "you have no watcher there" noise that nothing could clear (studio).
//!
//! **State under a dead hub id.** `cursor/`, `inbox/`, `watch/` and `tips/` are keyed by hub id and
//! nothing ever removed an entry. On this fleet one orphaned cursor turned out to be the *same
//! phantom* as a dead registry target — two records of one resolver failure, in two stores, only one
//! of which had a cleaner (studio again).
//!
//! # What this will not delete, on purpose
//!
//! `keyring/` holds TOFU pins and `presence_hwm/` holds the replay-defence monotonic anchor. A false
//! positive there does not cost a re-read — it silently re-TOFUs a key, or reopens a replay window.
//! Those are surveyed and *reported*, never removed, because the cost of being wrong is asymmetric
//! and the benefit is a few hundred bytes. Everything this does remove is recoverable by re-reading:
//! the worst case for a wrongly-pruned cursor is a backlog re-emitted once.

use crate::autoheal::{self, Target};
use crate::config;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Stores keyed by hub id that are safe to clear: losing an entry costs at most a re-read.
const CLEARABLE: [&str; 4] = ["cursor", "inbox", "watch", "tips"];

/// Stores keyed by hub id that are NEVER cleared here — trust and replay state (see module docs).
const RETAINED: [&str; 2] = ["keyring", "presence_hwm"];

/// A directory, or a `.confer` path, that looks like a confer hub working tree.
pub fn looks_like_hub(dir: &Path) -> bool {
    dir.join("threads").is_dir() && dir.join("roles").is_dir()
}

/// Per-hub-id state found on disk under one store.
pub struct OrphanState {
    pub store: String,
    pub key: String,
    pub path: PathBuf,
}

/// Everything prune can act on, split by how confident we are and what it would cost to be wrong.
#[derive(Default)]
pub struct Survey {
    /// Registered watch targets whose directory is gone.
    pub missing: Vec<Target>,
    /// Registered watch targets whose directory exists but is not a confer hub.
    pub not_a_hub: Vec<Target>,
    /// Clearable per-hub state under an id no live hub uses.
    pub orphan_state: Vec<OrphanState>,
    /// Trust/replay state under an id no live hub uses — reported, never removed.
    pub retained_state: Vec<OrphanState>,
    /// We could not establish which hub ids are live, so no state was surveyed. Registry entries
    /// are still reported — those do not depend on the hub-id set.
    pub state_undetermined: bool,
    /// How many hub ids were treated as live, and therefore protected. Reported so a human can
    /// sanity-check the BASIS before authorising a deletion: "138 orphans against 4 live hubs" is
    /// a number you can judge, where "138 orphans" alone is not.
    pub live_count: usize,
}

impl Survey {
    pub fn is_empty(&self) -> bool {
        self.missing.is_empty() && self.not_a_hub.is_empty() && self.orphan_state.is_empty()
    }
}

/// Every hub id this machine still has a reason to hold state for, or `None` if we cannot tell.
///
/// Deliberately a UNION of three independent sources, because each misses cases the others catch:
/// the watch registry knows hubs you ARMED, `known_hubs` knows hubs you JOINED (seeded at join, so
/// a poll-only agent that never armed a watch is still covered), and the managed clone dir catches
/// a clone whose registry entry was already pruned. A hub absent from all three is the only thing
/// treated as dead.
///
/// Returns `None` when `known_hubs.json` exists but will not parse. That store's loader is
/// deliberately tolerant — any failure degrades to an empty map — which is right for a read path
/// and wrong here: an unreadable pin file would silently shrink the live set and make live state
/// look orphaned. Not knowing has to be expressible, or this command deletes on a bad read.
pub fn live_hub_keys() -> Option<BTreeSet<String>> {
    let mut keys = BTreeSet::new();

    for t in autoheal::load().targets {
        let p = PathBuf::from(&t.hub);
        if looks_like_hub(&p) {
            keys.insert(config::hub_key(&p));
        }
    }

    let home = config::home().ok()?;
    let pins = home.join(".confer").join("known_hubs.json");
    if pins.exists() {
        let parses = std::fs::read_to_string(&pins)
            .is_ok_and(|t| serde_json::from_str::<serde_json::Value>(&t).is_ok());
        if !parses {
            return None; // present but unreadable — refuse to guess which hubs are live
        }
        for rec in crate::knownhubs::load().values() {
            if !rec.root.is_empty() {
                keys.insert(rec.root.clone());
            }
        }
    }

    // Managed clones on disk: ~/.confer/clones/<hub-slug>/<role-slug>/
    if let Ok(hubs) = std::fs::read_dir(home.join(".confer").join("clones")) {
        for hub in hubs.flatten() {
            if let Ok(roles) = std::fs::read_dir(hub.path()) {
                for role in roles.flatten() {
                    let p = role.path();
                    if looks_like_hub(&p) {
                        keys.insert(config::hub_key(&p));
                    }
                }
            }
        }
    }
    Some(keys)
}

/// The hub id a state entry is filed under, or `None` if the name is not one we recognise.
///
/// `presence_hwm` sanitises non-alphanumerics to `_` when building its filename, which is identity
/// for a 40-hex id — so comparing the stem is correct there too.
fn entry_key(name: &str) -> Option<String> {
    let stem = name.strip_suffix(".json").unwrap_or(name);
    (!stem.is_empty() && !stem.starts_with('.')).then(|| stem.to_string())
}

/// Does this `watch/<hub>/` directory record a pid that is running right now?
fn holds_a_live_watcher(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        // Unreadable: assume occupied. The safe direction is to leave it.
        return true;
    };
    entries.flatten().any(|f| {
        std::fs::read_to_string(f.path())
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("pid").and_then(|p| p.as_u64()))
            .is_some_and(|pid| crate::watchlock::pid_is_live_confer(pid as u32))
    })
}

fn survey_store(store: &str, live: &BTreeSet<String>) -> Vec<OrphanState> {
    let Ok(home) = config::home() else {
        return Vec::new();
    };
    let dir = home.join(".confer").join(store);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        // Never touch lock files — they belong to a running process, not to a hub.
        if name.ends_with(".lock") {
            continue;
        }
        let Some(key) = entry_key(&name) else { continue };
        if live.contains(&key) {
            continue;
        }
        // A watch lock whose pid is RUNNING is not debris, whatever we think of its hub id.
        // Deleting it would free the single-watcher lock under a live watcher and let a second one
        // start — manufacturing the duplicate-watcher condition out of a cleanup command. If our
        // live-hub set is wrong, this is the guard that stops it costing anything.
        if store == "watch" && holds_a_live_watcher(&e.path()) {
            continue;
        }
        out.push(OrphanState { store: store.to_string(), key, path: e.path() });
    }
    out.sort_by(|a, b| (&a.store, &a.key).cmp(&(&b.store, &b.key)));
    out
}

/// Everything prune could act on, without changing anything.
pub fn survey() -> Survey {
    let live = live_hub_keys();
    let mut s = Survey::default();

    for t in autoheal::load().targets {
        let p = PathBuf::from(&t.hub);
        if !p.exists() {
            s.missing.push(t);
        } else if !looks_like_hub(&p) {
            s.not_a_hub.push(t);
        }
    }

    // A machine with NO live hubs is far more likely to be mid-setup, or to have had an unreadable
    // registry, than to have genuinely orphaned every store it owns. Declining to survey state
    // there costs nothing and removes the only path by which this could mass-delete.
    s.live_count = live.as_ref().map_or(0, |l| l.len());
    match live {
        Some(l) if !l.is_empty() => {
            for store in CLEARABLE {
                s.orphan_state.extend(survey_store(store, &l));
            }
            for store in RETAINED {
                s.retained_state.extend(survey_store(store, &l));
            }
        }
        _ => s.state_undetermined = true,
    }
    s
}

/// What a prune actually removed.
#[derive(Default)]
pub struct Removed {
    pub targets: usize,
    pub state: usize,
    pub failed: Vec<(PathBuf, String)>,
}

/// Remove what `survey` found — registry entries in both bad categories, and clearable state.
/// Never touches `RETAINED` stores.
pub fn apply(s: &Survey) -> Removed {
    let mut r = Removed::default();

    let doomed: BTreeSet<(String, String)> = s
        .missing
        .iter()
        .chain(s.not_a_hub.iter())
        .map(|t| (t.hub.clone(), t.role.clone()))
        .collect();
    if !doomed.is_empty() {
        r.targets = autoheal::retain_targets(|t| !doomed.contains(&(t.hub.clone(), t.role.clone())));
    }

    for o in &s.orphan_state {
        let res = if o.path.is_dir() {
            std::fs::remove_dir_all(&o.path)
        } else {
            std::fs::remove_file(&o.path)
        };
        match res {
            Ok(()) => r.state += 1,
            // Report rather than swallow: a prune that silently fails to remove something still
            // prints "removed N" and the debris comes back next run, unexplained.
            Err(e) => r.failed.push((o.path.clone(), e.to_string())),
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_key_strips_json_and_skips_dotfiles() {
        assert_eq!(entry_key("da5d.json").as_deref(), Some("da5d"));
        assert_eq!(entry_key("da5d").as_deref(), Some("da5d"));
        // A stray .DS_Store is not a hub id, and treating it as one would list it for deletion.
        assert_eq!(entry_key(".DS_Store"), None);
        assert_eq!(entry_key(""), None);
    }

    #[test]
    fn looks_like_hub_needs_both_dirs() {
        let base = std::env::temp_dir().join(format!("confer-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("threads")).unwrap();
        assert!(!looks_like_hub(&base), "threads/ alone is not a hub");
        std::fs::create_dir_all(base.join("roles")).unwrap();
        assert!(looks_like_hub(&base), "threads/ + roles/ is a hub");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn an_unreadable_watch_dir_is_treated_as_occupied() {
        // Fail safe: if we cannot read a lock dir we must assume something holds it. The opposite
        // default would delete a live watcher's lock on a transient read error.
        assert!(holds_a_live_watcher(Path::new("/definitely/not/a/real/path")));
    }
}
