//! `confer repos discover` — local-only backfill (design/40 layer 2): match every repo
//! registered in ANY hub this machine follows (plus the current hub, if not yet in that
//! registry) to a git clone already on disk, and record the mapping (`repomap::set`)
//! without ever touching a hub card or committing. Automates what `confer repos map
//! <slug> <path>` does by hand, one slug at a time.
//!
//! Matching prefers the root-commit SHA (design/40's F3 anchor — survives a forked/
//! mirrored/renamed remote); falls back to canonical `owner/repo` shorthand equality
//! between the card's `url` and the candidate's `origin` (via `transport::parse_remote`,
//! so `git@github.com:o/r.git` and `https://github.com/o/r` are recognized as the same
//! repo). See DESIGN.md / design/40.

use crate::{config, crosshub, gitcmd, repomap, repos, transport};
use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A local git clone found while scanning a candidate root: its canonical origin
/// (`owner/repo` shorthand, if the remote parses as one) and root-commit SHA — the two
/// signals a hub's repo card can be matched against.
struct Candidate {
    path: PathBuf,
    shorthand: Option<String>,
    root_sha: Option<String>,
}

/// The result of one `discover` run: what got mapped, and what stayed unmatched (for the
/// human to map by hand, or clone).
#[derive(Default)]
pub(crate) struct Report {
    pub(crate) mapped: Vec<(String, PathBuf)>,
    pub(crate) unmatched: Vec<(String, Option<String>)>,
}

/// The user's usual dev roots, plus the parent dir of every hub clone they already
/// follow (a repo card's clone is often a sibling of the hub itself).
fn default_roots() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(home) = config::home() {
        for d in ["git", "src", "code", "Projects", "dev", "repos"] {
            out.push(home.join(d));
        }
        out.push(home);
    }
    for hub in crosshub::hub_dirs() {
        if let Some(parent) = hub.parent() {
            out.push(parent.to_path_buf());
        }
    }
    out
}

/// True iff `dir` is a git repo (worktree or bare) — the same cheap gate `repomap` uses.
fn is_git_repo(dir: &Path) -> bool {
    dir.is_dir()
        && gitcmd::output(dir, &["rev-parse", "--git-dir"])
            .map(|o| o.status.success())
            .unwrap_or(false)
}

/// The canonical `owner/repo` shorthand of `dir`'s `origin` remote, if it has one and it
/// parses as a GitHub-style remote (`transport::parse_remote`) — the same canonicalization
/// `parse_remote_canonicalizes_github_forms` exercises, reused here so a card's `url` and a
/// clone's `origin` compare equal regardless of scheme (`git@…` vs `https://…`).
fn canonical_origin(dir: &Path) -> Option<String> {
    let o = gitcmd::output(dir, &["config", "--get", "remote.origin.url"]).ok()?;
    if !o.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if url.is_empty() {
        return None;
    }
    transport::parse_remote(&url).shorthand
}

/// Scan `root`'s IMMEDIATE children for git repos (one level — fast), each turned into a
/// `Candidate` with its canonical origin + root SHA.
fn scan_root(root: &Path) -> Vec<Candidate> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for e in entries.flatten() {
        let p = e.path();
        if !is_git_repo(&p) {
            continue;
        }
        out.push(Candidate {
            shorthand: canonical_origin(&p),
            root_sha: crosshub::root_sha(&p),
            path: p,
        });
    }
    out
}

/// Scan every root (deduped by canonical path), collecting all candidates found.
fn scan_all(roots: &[PathBuf]) -> Vec<Candidate> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for root in roots {
        let canon = root.canonicalize().unwrap_or_else(|_| root.clone());
        if !seen.insert(canon) {
            continue;
        }
        out.extend(scan_root(root));
    }
    out
}

/// Are `a` and `b` the same directory (canonicalized; falls back to as-given equality if
/// either doesn't exist)?
fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Every hub this machine has any relationship with: the followed-hubs registry
/// (`crosshub::hub_dirs`) plus the CURRENT hub, if it isn't already in that registry (a
/// hub a human is sitting in but hasn't yet `join`ed/`who`'d from — the registry is
/// populated by those, not by every command).
fn all_hub_dirs() -> Vec<PathBuf> {
    let mut dirs = crosshub::hub_dirs();
    if let Ok(cur) = config::repo_root() {
        if !dirs.iter().any(|d| same_dir(d, &cur)) {
            dirs.push(cur);
        }
    }
    dirs
}

/// Every repo registered in ANY hub this machine has a relationship with, deduped by
/// slug (first-seen wins — a slug is a hub-scoped key, so a collision across hubs is
/// coincidental, not a conflict to resolve here).
fn registered_repos() -> Vec<(String, repos::Repo)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for hub in all_hub_dirs() {
        for (slug, card) in repos::load(&hub) {
            if seen.insert(slug.clone()) {
                out.push((slug, card));
            }
        }
    }
    out
}

/// Does `candidate` match `card`? The root-SHA anchor wins when the card carries one
/// (survives a forked/renamed/mirrored remote); otherwise canonical `owner/repo`
/// shorthand equality between the card's url and the candidate's origin.
fn is_match(card: &repos::Repo, candidate: &Candidate) -> bool {
    if let (Some(want), Some(got)) = (&card.root_sha, &candidate.root_sha) {
        return want == got;
    }
    if let (Some(url), Some(got)) = (&card.url, &candidate.shorthand) {
        if let Some(want) = transport::parse_remote(url).shorthand {
            return &want == got;
        }
    }
    false
}

/// Find the first candidate matching `card`, if any.
fn find_match<'a>(card: &repos::Repo, candidates: &'a [Candidate]) -> Option<&'a Candidate> {
    candidates.iter().find(|c| is_match(card, c))
}

/// Run one discovery pass: scan `roots` (or the defaults, if empty) for local clones,
/// then map every UNMAPPED registered repo it can match. Local-only, idempotent — never
/// writes to a hub, never commits, and skips a slug that's already mapped.
pub(crate) fn run(roots: &[PathBuf]) -> Result<Report> {
    let roots: Vec<PathBuf> = if roots.is_empty() { default_roots() } else { roots.to_vec() };
    let candidates = scan_all(&roots);
    let mut report = Report::default();
    for (slug, card) in registered_repos() {
        if repomap::path(&slug).is_some() {
            continue; // already mapped — idempotent skip
        }
        match find_match(&card, &candidates) {
            Some(c) => {
                // `repomap::set`'s returned path is canonicalized (and re-validated as a
                // git repo) — report that, not the raw scanned path, so the printed
                // mapping always matches what `repos`/`repomap::path` show afterward.
                let abs = repomap::set(&slug, &c.path)?;
                report.mapped.push((slug, abs));
            }
            None => report.unmatched.push((slug, card.url.clone())),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(url: Option<&str>, root_sha: Option<&str>) -> repos::Repo {
        repos::Repo {
            url: url.map(String::from),
            root_sha: root_sha.map(String::from),
            ..Default::default()
        }
    }

    fn candidate(shorthand: Option<&str>, root_sha: Option<&str>) -> Candidate {
        Candidate {
            path: PathBuf::from("/tmp/whatever"),
            shorthand: shorthand.map(String::from),
            root_sha: root_sha.map(String::from),
        }
    }

    #[test]
    fn matches_via_canonicalized_url_regardless_of_scheme() {
        let c = card(Some("git@github.com:codeshrew/git-conversations.git"), None);
        let cand = candidate(Some("codeshrew/git-conversations"), None);
        assert!(is_match(&c, &cand));
    }

    #[test]
    fn matches_via_root_sha_even_with_different_owners() {
        let c = card(Some("https://example.com/original/fork.git"), Some("abc123"));
        let cand = candidate(Some("someone-else/renamed-fork"), Some("abc123"));
        assert!(is_match(&c, &cand));
    }

    #[test]
    fn root_sha_mismatch_beats_url_match_being_absent() {
        let c = card(None, Some("abc123"));
        let cand = candidate(None, Some("def456"));
        assert!(!is_match(&c, &cand));
    }

    #[test]
    fn no_shared_signal_is_no_match() {
        let c = card(Some("git@github.com:o/foo.git"), None);
        let cand = candidate(Some("o/bar"), None);
        assert!(!is_match(&c, &cand));
    }

    #[test]
    fn find_match_returns_none_when_nothing_matches() {
        let c = card(Some("git@github.com:o/foo.git"), None);
        let candidates = vec![candidate(Some("o/bar"), None), candidate(None, Some("zzz"))];
        assert!(find_match(&c, &candidates).is_none());
    }
}
