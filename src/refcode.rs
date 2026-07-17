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
