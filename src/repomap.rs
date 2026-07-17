//! Machine-local repo clone map (design/40, layer 2). Where THIS machine has each
//! referenced repo cloned — private, per-machine, and NEVER written to the hub
//! (clone paths differ per machine, the same reasoning that keeps key pins and
//! trust tiers only in `~/.confer`). The hub's `repos/<slug>.md` card carries the
//! SHARED identity (url + root_sha, layer 1); this file only records "…and I have
//! it cloned here."
//!
//! Content-addressing does the rest: a commit sha means the same object regardless
//! of which remote served it or where it's cloned, so all we store locally is the
//! path — resolution reads the bytes from there. Validation against the card's
//! root-commit SHA (the F3 anchor, `crosshub::root_sha`) proves a mapped clone is
//! the SAME repo lineage even across forks / mirrors / renamed remotes.

use crate::{config, crosshub, gitcmd};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn map_path() -> Option<PathBuf> {
    config::home().ok().map(|h| h.join(".confer").join("repos.json"))
}

#[derive(Serialize, Deserialize, Default)]
struct Map {
    /// slug → absolute clone path on this machine.
    #[serde(default)]
    clones: BTreeMap<String, String>,
}

/// The recorded `slug → path` clones for this machine (empty if none/unreadable).
pub fn load() -> BTreeMap<String, String> {
    map_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Map>(&s).ok())
        .map(|m| m.clones)
        .unwrap_or_default()
}

fn write(clones: &BTreeMap<String, String>) -> Result<()> {
    let p = map_path().ok_or_else(|| anyhow!("no home directory for ~/.confer/repos.json"))?;
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, serde_json::to_string_pretty(&Map { clones: clones.clone() })?)?;
    Ok(())
}

/// True iff `dir` is a git repository (work tree or bare) — the cheap validity gate.
fn is_git_repo(dir: &Path) -> bool {
    dir.is_dir()
        && gitcmd::output(dir, &["rev-parse", "--git-dir"])
            .map(|o| o.status.success())
            .unwrap_or(false)
}

/// Record that `slug` is cloned at `path` on this machine (canonicalized). Errors if
/// the path isn't a git repository — a wrong path would silently render every ref
/// unresolvable. Returns the canonical path stored.
pub fn set(slug: &str, path: &Path) -> Result<PathBuf> {
    let abs = path
        .canonicalize()
        .map_err(|e| anyhow!("{}: {e}", path.display()))?;
    if !is_git_repo(&abs) {
        return Err(anyhow!("{} is not a git repository", abs.display()));
    }
    let mut clones = load();
    clones.insert(slug.to_string(), abs.to_string_lossy().to_string());
    write(&clones)?;
    Ok(abs)
}

/// The mapped clone path for `slug`, if it's recorded AND still a live git dir
/// (a moved/deleted clone silently drops to "unmapped" rather than erroring later).
pub fn path(slug: &str) -> Option<PathBuf> {
    let p = PathBuf::from(load().get(slug)?);
    is_git_repo(&p).then_some(p)
}

/// Resolve a repo slug to a usable local clone, enforcing layer-1 identity when the
/// hub card carries a `root_sha`: the clone's root-commit SHA must match, else the
/// mapped dir is a DIFFERENT repo and we refuse it (returns None → pointer-only
/// render). With no card root_sha yet, any mapped clone is accepted (trust-on-first
/// -use; the caller may record the observed root_sha into the card).
#[allow(dead_code)] // wired in by Increment 2 (pin-at-write + content_hash resolution)
pub fn resolve(slug: &str, card_root_sha: Option<&str>) -> Option<PathBuf> {
    let p = path(slug)?;
    match card_root_sha {
        Some(want) if crosshub::root_sha(&p).as_deref() != Some(want) => None,
        _ => Some(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t.local", "-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "git {args:?} failed");
    }

    #[test]
    fn is_git_repo_detects_repo_vs_plain_dir() {
        let base = std::env::temp_dir().join(format!("confer-repomap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("r");
        let plain = base.join("p");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&plain).unwrap();
        git(&repo, &["init", "-q"]);
        assert!(is_git_repo(&repo));
        assert!(!is_git_repo(&plain));
        assert!(!is_git_repo(&base.join("does-not-exist")));
        let _ = std::fs::remove_dir_all(&base);
    }
}
