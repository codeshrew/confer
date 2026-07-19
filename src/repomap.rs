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
///
/// **Shallow-clone fix (design/44 §1.5):** a shallow clone's root-SHA is the shallow
/// boundary, not the true root, so it would never match a real card and get wrongly
/// refused. When the mapped clone is shallow, skip the comparison and accept it —
/// identity is *unverifiable*, not mismatched. Never hard-refuse a shallow clone.
pub fn resolve(slug: &str, card_root_sha: Option<&str>) -> Option<PathBuf> {
    let p = path(slug)?;
    match card_root_sha {
        Some(want) if !crosshub::is_shallow(&p) && crosshub::root_sha(&p).as_deref() != Some(want) => {
            None
        }
        _ => Some(p),
    }
}

/// Where a `--ref`'s capture directory came from — printed on the send receipt
/// (stderr) only; NEVER persisted (worktree paths are machine-local, design/44 §1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureSource {
    /// The explicit `--ref-from <dir>` escape hatch.
    RefFrom,
    /// The agent's cwd — a linked worktree or a separate clone of the same repo.
    Cwd,
    /// The machine-local clone map (`repos map`) — the fallback, today's behavior.
    Mapped,
}

/// The resolved capture directory for one `--ref`, plus where it came from.
#[derive(Debug, Clone)]
pub struct Capture {
    pub dir: PathBuf,
    pub source: CaptureSource,
}

/// `git -C dir rev-parse --show-toplevel`, canonicalized. Works from ANY subdir of a
/// worktree (including a linked worktree) — the mechanic design/44 §1.1 relies on.
fn toplevel(dir: &Path) -> Option<PathBuf> {
    let o = gitcmd::output(dir, &["rev-parse", "--show-toplevel"]).ok()?;
    if !o.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    PathBuf::from(s).canonicalize().ok()
}

/// `git -C dir rev-parse --git-common-dir`, canonicalized to an absolute path (git may
/// print it relative to `dir`). For a linked worktree this resolves to the MAIN clone's
/// `.git` dir — the cheap, exact "same repo" signal (§1.1 rule 2's worktree case).
fn common_dir(dir: &Path) -> Option<PathBuf> {
    let o = gitcmd::output(dir, &["rev-parse", "--git-common-dir"]).ok()?;
    if !o.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    let p = PathBuf::from(&s);
    let abs = if p.is_absolute() { p } else { dir.join(p) };
    abs.canonicalize().ok()
}

/// Is `candidate` (a toplevel dir) the SAME repo as `mapped` (the machine-local clone),
/// per design/44 §1.1 rule 2/1: linked-worktree case (exact `git-common-dir` match) OR
/// separate-clone case (root-SHA match against the card identity, falling back to the
/// mapped clone's own root-SHA when the card doesn't carry one yet).
fn is_same_repo(candidate: &Path, mapped: Option<&Path>, card_root_sha: Option<&str>) -> bool {
    if let Some(m) = mapped {
        if let (Some(a), Some(b)) = (common_dir(candidate), common_dir(m)) {
            if a == b {
                return true;
            }
        }
    }
    let want = card_root_sha.map(str::to_string).or_else(|| mapped.and_then(crosshub::root_sha));
    match want {
        Some(want) => crosshub::root_sha(candidate).as_deref() == Some(want.as_str()),
        None => false, // nothing to compare identity against
    }
}

/// Resolve the ONE capture directory for a `--ref <slug>:…` (design/44 §1.1) — the
/// worktree-correct precedence: `--ref-from` (repo-matching) → the agent's cwd (same
/// repo) → the mapped clone (fallback). `None` = nothing resolves (rule 4, handled by
/// the caller). Every subsequent capture command (sha/ref_name/commit_date/content_hash/
/// dirty check) must run against the SAME returned dir — never mixed.
pub fn capture_dir(slug: &str, card_root_sha: Option<&str>, ref_from: Option<&Path>) -> Option<Capture> {
    let mapped_raw = path(slug); // unvalidated — used only for identity comparisons below
    if let Some(dir) = ref_from {
        if let Some(top) = toplevel(dir) {
            if is_same_repo(&top, mapped_raw.as_deref(), card_root_sha) {
                return Some(Capture { dir: top, source: CaptureSource::RefFrom });
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(top) = toplevel(&cwd) {
            if is_same_repo(&top, mapped_raw.as_deref(), card_root_sha) {
                return Some(Capture { dir: top, source: CaptureSource::Cwd });
            }
        }
    }
    if let Some(dir) = resolve(slug, card_root_sha) {
        return Some(Capture { dir, source: CaptureSource::Mapped });
    }
    None
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

    /// A fresh repo (in a uniquely-named tempdir) with one commit whose CONTENT is
    /// unique to `tag` — two calls must never collide on the same root-commit sha
    /// (which git commit hashing would do if content/message/timestamp all matched).
    fn fresh_repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("confer-repomap-{}-{tag}-{}", std::process::id(), ulid::Ulid::new()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        std::fs::write(dir.join("f.txt"), format!("{tag}-{}\n", ulid::Ulid::new())).unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", &format!("c0-{tag}")]);
        dir
    }

    #[test]
    fn is_same_repo_matches_linked_worktree_via_common_dir() {
        // design/44 §1.1 rule 2, worktree case: a linked worktree's git-common-dir
        // canonicalizes to the main clone's — is_same_repo must accept it WITHOUT
        // needing a root-sha/card at all.
        let main = fresh_repo("wt-main");
        let wt = main.parent().unwrap().join(format!("wt-{}", ulid::Ulid::new()));
        git(&main, &["worktree", "add", "-q", "-b", "feature", wt.to_str().unwrap()]);
        assert!(is_same_repo(&wt, Some(&main), None));
        let _ = std::fs::remove_dir_all(&main);
        let _ = std::fs::remove_dir_all(&wt);
    }

    #[test]
    fn is_same_repo_matches_separate_clone_via_root_sha() {
        // Separate-clone case: no shared git dir, but the SAME root-commit sha —
        // accepted via the card's root_sha (or the mapped clone's own, when no card).
        let a = fresh_repo("root-a");
        let root = crosshub::root_sha(&a).unwrap();
        // A second, unrelated repo — different root sha, must NOT match.
        let b = fresh_repo("root-b");
        assert!(is_same_repo(&a, None, Some(&root)));
        assert!(!is_same_repo(&b, None, Some(&root)));
        // No card root_sha at all → falls back to matching the MAPPED clone's own root sha.
        assert!(is_same_repo(&a, Some(&a), None));
        assert!(!is_same_repo(&b, Some(&a), None));
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }

    #[test]
    fn is_same_repo_false_with_nothing_to_compare() {
        let a = fresh_repo("nocompare");
        assert!(!is_same_repo(&a, None, None));
        let _ = std::fs::remove_dir_all(&a);
    }

    #[test]
    fn shallow_clone_root_sha_is_unverifiable_not_refused() {
        // design/44 §1.5: a shallow clone's root-sha is the shallow boundary, not the
        // true root. `resolve` (tested via subprocess in tests/cli.rs, which needs a
        // real `~/.confer/repos.json`) skips the comparison when `is_shallow` is true —
        // here we just pin down the primitive it depends on.
        let origin = fresh_repo("shallow-origin");
        git(&origin, &["commit", "--allow-empty", "-q", "-m", "c1"]);
        git(&origin, &["commit", "--allow-empty", "-q", "-m", "c2"]);
        let shallow = origin.parent().unwrap().join(format!("shallow-{}", ulid::Ulid::new()));
        // `--depth` is a no-op on a plain local-path clone (git optimizes it to a hardlink
        // clone and ignores shallowing) — force the real network-clone codepath via `file://`.
        let ok = Command::new("git")
            .args([
                "clone",
                "-q",
                "--depth",
                "1",
                &format!("file://{}", origin.display()),
                shallow.to_str().unwrap(),
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "shallow clone failed");
        assert!(crosshub::is_shallow(&shallow));
        assert!(!crosshub::is_shallow(&origin));
        let _ = std::fs::remove_dir_all(&origin);
        let _ = std::fs::remove_dir_all(&shallow);
    }
}
