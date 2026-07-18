//! Resolving a `--ref` against a local clone (design/40 #5, #6): **staleness** — has
//! the code moved under the pin since it was referenced? — and a bounded, sanitized
//! **snippet** of the referenced lines. All best-effort and graceful: no mapped clone
//! or a missing object degrades to Unknown / pointer-only, never an error.

use crate::{gitcmd, repomap, repos, schema};
use std::path::{Path, PathBuf};

/// How a pinned ref relates to the repo's CURRENT HEAD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Staleness {
    /// The referenced bytes are unchanged at HEAD (blob OID matches).
    Current,
    /// The file still exists at HEAD but its bytes changed since the pin.
    Changed,
    /// The path no longer exists at HEAD (moved / renamed / deleted).
    Moved,
    /// The sha isn't a full-hex pin (a legacy `HEAD`/branch ref) — not durable.
    Unpinned,
    /// No mapped clone, or no `content_hash` recorded — can't tell.
    Unknown,
}

impl Staleness {
    pub fn label(self) -> &'static str {
        match self {
            Staleness::Current => "current",
            Staleness::Changed => "changed",
            Staleness::Moved => "moved",
            Staleness::Unpinned => "unpinned",
            Staleness::Unknown => "unknown",
        }
    }
    /// A compact human badge — empty for the common no-clone `Unknown` case, so we
    /// never nag when we simply can't see the repo.
    fn badge(self) -> &'static str {
        match self {
            Staleness::Current => "  [current]",
            Staleness::Changed => "  [⚠ changed since pinned]",
            Staleness::Moved => "  [⚠ moved/renamed since pinned]",
            Staleness::Unpinned => "  [⚠ unpinned — legacy HEAD ref]",
            Staleness::Unknown => "",
        }
    }
}

fn is_full_hex(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// The local clone for a repo (machine-local map, validated against the card root_sha).
pub fn clone_for(repo_inv: &repos::Repos, repo: &str) -> Option<PathBuf> {
    let card_root_sha = repo_inv.get(repo).and_then(|c| c.root_sha.clone());
    repomap::resolve(repo, card_root_sha.as_deref())
}

/// Cheap, lazy staleness: compare the pinned blob OID (`content_hash`) against
/// `HEAD:<path>`'s. Works even when the pinned COMMIT is GC'd/unfetched — you never
/// need the commit to ask "have these bytes changed?".
pub fn staleness(
    clone: Option<&Path>,
    sha: &str,
    path: &str,
    content_hash: Option<&str>,
) -> Staleness {
    if !is_full_hex(sha) {
        return Staleness::Unpinned;
    }
    let (Some(dir), Some(pinned)) = (clone, content_hash) else {
        return Staleness::Unknown;
    };
    match gitcmd::output(dir, &["rev-parse", "--verify", "--quiet", &format!("HEAD:{path}")]) {
        Ok(o) if o.status.success() => {
            if String::from_utf8_lossy(&o.stdout).trim() == pinned {
                Staleness::Current
            } else {
                Staleness::Changed
            }
        }
        Ok(_) => Staleness::Moved, // path not at HEAD
        Err(_) => Staleness::Unknown,
    }
}

/// The referenced lines (1-based, inclusive), read from the clone at the PINNED sha,
/// bounded to `max_lines` and sanitized. None if unresolvable (no clone / missing
/// object / too large / empty range).
pub fn snippet(
    clone: Option<&Path>,
    sha: &str,
    path: &str,
    range: Option<[u64; 2]>,
    max_lines: usize,
) -> Option<Vec<(u64, String)>> {
    let dir = clone?;
    let spec = format!("{sha}:{path}");
    // Size guard BEFORE reading content — don't slurp a huge blob to slice a few lines.
    let szo = gitcmd::output(dir, &["cat-file", "-s", &spec]).ok()?;
    if !szo.status.success() {
        return None;
    }
    let size: u64 = String::from_utf8_lossy(&szo.stdout).trim().parse().ok()?;
    if size > 2_000_000 {
        return None; // >2MB → pointer-only
    }
    let o = gitcmd::output(dir, &["cat-file", "-p", &spec]).ok()?;
    if !o.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&o.stdout);
    let all: Vec<&str> = text.lines().collect();
    let (start, end) = match range {
        Some([a, b]) => (a.max(1), b),
        None => (1, all.len() as u64),
    };
    let mut out = Vec::new();
    for (i, line) in all.iter().enumerate() {
        let n = i as u64 + 1;
        if n < start {
            continue;
        }
        if n > end || out.len() >= max_lines {
            break;
        }
        out.push((n, schema::sanitize_term(line, false)));
    }
    (!out.is_empty()).then_some(out)
}

/// A full multi-line render of a ref for `show`: the pointer + a staleness badge, then
/// a bounded snippet when the repo is cloned here, or a "map a clone" hint when not.
pub fn render_resolved(repo_inv: &repos::Repos, r: &schema::CodeRef, max_lines: usize) -> String {
    let clone = clone_for(repo_inv, &r.repo);
    let rng = r.range.map(|x| format!("#L{}-{}", x[0], x[1])).unwrap_or_default();
    let short: String = if is_full_hex(&r.sha) {
        r.sha[..r.sha.len().min(9)].to_string()
    } else {
        r.sha.clone()
    };
    let st = staleness(clone.as_deref(), &r.sha, &r.path, r.content_hash.as_deref());
    let mut s = format!("⟶ {}:{} @{}{}{}", r.repo, r.path, short, rng, st.badge());
    if clone.is_none() {
        s.push_str(&format!(
            "\n   (not cloned here — `confer repos map {} <path>` to see the code)",
            r.repo
        ));
        return s;
    }
    if let Some(lines) = snippet(clone.as_deref(), &r.sha, &r.path, r.range, max_lines) {
        for (n, line) in lines {
            s.push_str(&format!("\n   {n:>5} │ {line}"));
        }
    }
    s
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

    /// A tiny repo with one committed file, returning (dir, head_sha, blob_oid_of_file).
    fn repo_with_file(tag: &str, contents: &str) -> (PathBuf, String, String) {
        let dir = std::env::temp_dir().join(format!("confer-refcode-{}-{tag}-{}", std::process::id(), std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        std::fs::write(dir.join("f.rs"), contents).unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "c0"]);
        let head = String::from_utf8_lossy(
            &Command::new("git").arg("-C").arg(&dir).args(["rev-parse", "HEAD"]).output().unwrap().stdout,
        )
        .trim()
        .to_string();
        let blob = String::from_utf8_lossy(
            &Command::new("git").arg("-C").arg(&dir).args(["rev-parse", "HEAD:f.rs"]).output().unwrap().stdout,
        )
        .trim()
        .to_string();
        (dir, head, blob)
    }

    #[test]
    fn staleness_unpinned_for_non_full_hex_sha_regardless_of_clone() {
        // A short/symbolic sha ("HEAD", a branch, an abbreviated sha) was never durably
        // pinned — Unpinned wins even with a perfectly good clone and content_hash.
        let (dir, _head, blob) = repo_with_file("unpinned", "a\nb\n");
        assert_eq!(staleness(Some(&dir), "HEAD", "f.rs", Some(&blob)), Staleness::Unpinned);
        assert_eq!(staleness(None, "abcdef", "f.rs", None), Staleness::Unpinned);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn staleness_unknown_without_clone_or_content_hash() {
        let (dir, head, blob) = repo_with_file("unknown", "a\nb\n");
        // no clone at all
        assert_eq!(staleness(None, &head, "f.rs", Some(&blob)), Staleness::Unknown);
        // clone present but no content_hash recorded (pre-design/40 legacy ref)
        assert_eq!(staleness(Some(&dir), &head, "f.rs", None), Staleness::Unknown);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn staleness_moved_when_path_gone_at_head() {
        let (dir, head, blob) = repo_with_file("moved", "a\nb\n");
        // the pinned path never existed (or was renamed away) — HEAD:<path> fails to
        // resolve, not an error: the path MOVED under the pin.
        assert_eq!(staleness(Some(&dir), &head, "renamed.rs", Some(&blob)), Staleness::Moved);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn staleness_current_vs_changed() {
        let (dir, head, blob) = repo_with_file("curchg", "a\nb\n");
        assert_eq!(staleness(Some(&dir), &head, "f.rs", Some(&blob)), Staleness::Current);
        // a different (stale) content_hash at the SAME head → Changed, not Moved/Unknown.
        assert_eq!(staleness(Some(&dir), &head, "f.rs", Some("0000000000000000000000000000000000000000")), Staleness::Changed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snippet_none_without_a_clone() {
        assert_eq!(snippet(None, "abc", "f.rs", None, 100), None);
    }

    #[test]
    fn snippet_none_for_missing_object() {
        let (dir, head, _blob) = repo_with_file("missingobj", "a\nb\n");
        assert_eq!(snippet(Some(&dir), &head, "does-not-exist.rs", None, 100), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snippet_size_guard_rejects_oversized_blob() {
        let big = "x".repeat(2_100_000); // > 2MB guard
        let (dir, head, _blob) = repo_with_file("big", &big);
        assert_eq!(snippet(Some(&dir), &head, "f.rs", None, 100), None, "a >2MB blob must degrade to pointer-only, not slurp");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snippet_range_is_clamped_and_bounded_by_max_lines() {
        let (dir, head, _blob) = repo_with_file("range", "one\ntwo\nthree\nfour\nfive\n");
        // range [0, 3] — start clamps up to 1 (line numbers are 1-based).
        let lines = snippet(Some(&dir), &head, "f.rs", Some([0, 3]), 100).unwrap();
        assert_eq!(lines, vec![(1, "one".to_string()), (2, "two".to_string()), (3, "three".to_string())]);
        // an end past EOF just stops at the last line, no error/panic.
        let lines = snippet(Some(&dir), &head, "f.rs", Some([4, 999]), 100).unwrap();
        assert_eq!(lines, vec![(4, "four".to_string()), (5, "five".to_string())]);
        // max_lines caps the count even within a valid range.
        let lines = snippet(Some(&dir), &head, "f.rs", None, 2).unwrap();
        assert_eq!(lines.len(), 2);
        // a range entirely past EOF yields no lines → None (not an empty Some(vec![])).
        assert_eq!(snippet(Some(&dir), &head, "f.rs", Some([50, 60]), 100), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_resolved_hints_map_a_clone_when_unmapped() {
        let repo_inv: repos::Repos = Default::default(); // no "mylib" card at all → clone_for → None
        let r = schema::CodeRef {
            repo: "mylib".into(),
            sha: "a".repeat(40),
            path: "f.rs".into(),
            range: None,
            content_hash: None,
        };
        let out = render_resolved(&repo_inv, &r, 50);
        assert!(out.contains("not cloned here"), "out: {out}");
        assert!(out.contains("confer repos map mylib"), "out: {out}");
        // the full-hex sha is truncated to a 9-char short form in the header line.
        assert!(out.contains(&format!("@{}", &"a".repeat(9))), "out: {out}");
    }

    #[test]
    fn render_resolved_keeps_short_sha_unmodified() {
        let repo_inv: repos::Repos = Default::default();
        let r = schema::CodeRef { repo: "mylib".into(), sha: "HEAD".into(), path: "f.rs".into(), range: None, content_hash: None };
        let out = render_resolved(&repo_inv, &r, 50);
        // a non-full-hex sha (legacy "HEAD") is shown as-is, not truncated/mangled.
        assert!(out.contains("@HEAD"), "out: {out}");
        assert!(out.contains("unpinned"), "legacy HEAD refs should badge as unpinned: {out}");
    }
}
