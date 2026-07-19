//! Resolving a `--ref` against a local clone (design/40 #5, #6): **staleness** — has
//! the code moved under the pin since it was referenced? — and a bounded, sanitized
//! **snippet** of the referenced lines. All best-effort and graceful: no mapped clone
//! or a missing object degrades to Unknown / pointer-only, never an error.

use crate::{gitcmd, repomap, repos, schema};
use std::path::{Path, PathBuf};

/// How a pinned ref relates to the repo's CURRENT HEAD.
///
/// `Reachable`/`Offline`/`Squashed` (design/44 Addendum 1+2) are ANCESTRY signals —
/// orthogonal to the CONTENT signal (`Current`/`Changed`/`Moved`) that the base
/// `staleness()` computes. See `staleness_ex` for how the two compose and the
/// precedence that was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Staleness {
    /// The referenced bytes are unchanged at HEAD (blob OID matches).
    Current,
    /// The file still exists at HEAD but its bytes changed since the pin. Only
    /// returned when ancestry couldn't be computed (see `staleness_ex`) — when it
    /// COULD be computed, a changed-content verdict resolves to `Reachable` or
    /// `Offline`/`Squashed` instead, which is strictly more informative.
    Changed,
    /// The path no longer exists at HEAD (moved / renamed / deleted).
    Moved,
    /// *(Addendum 1, new)* The pinned commit IS an ancestor of HEAD (still on the
    /// line leading to HEAD — valid, in-line history), but the file's bytes changed
    /// since. The ancestry-confirmed, more robust replacement for the fragile
    /// implicit "not HEAD ⇒ stale" read.
    Reachable,
    /// *(Addendum 1, new)* The pinned commit is NOT an ancestor of HEAD and no
    /// squash anchor accounts for it (rebased away, abandoned/side branch, or
    /// GC'd) — the genuinely fragile case worth a warning.
    Offline,
    /// *(Addendum 2, new)* Like `Offline`, but the ref's `fork_point` is still an
    /// ancestor of its `base_ref`'s HEAD — the branch was very likely merged or
    /// squashed away rather than abandoned. Inferred, not certain.
    Squashed,
    /// The sha isn't a full-hex pin (a legacy `HEAD`/branch ref) — not durable.
    Unpinned,
    /// No mapped clone, no `content_hash` recorded, or the object isn't present
    /// locally (shallow/unfetched) — can't tell.
    Unknown,
    /// *(design/45 §1.2/§1.7, new)* A patch (`patch: true` ref) whose proposed change has
    /// already landed: `HEAD:<path>`'s blob OID equals `result_hash` (or, for a deletion, the
    /// path is now absent at HEAD). Detected purely from hub data + any clone — no tracking
    /// state, no CI hook, no server.
    Landed,
}

impl Staleness {
    pub fn label(self) -> &'static str {
        match self {
            Staleness::Current => "current",
            Staleness::Changed => "changed",
            Staleness::Moved => "moved",
            Staleness::Reachable => "reachable",
            Staleness::Offline => "offline",
            Staleness::Squashed => "squashed",
            Staleness::Unpinned => "unpinned",
            Staleness::Unknown => "unknown",
            Staleness::Landed => "landed",
        }
    }
    /// A compact human badge — empty for the common no-clone `Unknown` case, so we
    /// never nag when we simply can't see the repo.
    fn badge(self) -> &'static str {
        match self {
            Staleness::Current => "  [current]",
            Staleness::Changed => "  [⚠ changed since pinned]",
            Staleness::Moved => "  [⚠ moved/renamed since pinned]",
            Staleness::Reachable => "  [⚠ reachable — changed since pinned]",
            Staleness::Offline => "  [⚠ offline — rebased/abandoned]",
            Staleness::Squashed => "  [⚠ squashed — merged away]",
            Staleness::Unpinned => "  [⚠ unpinned — legacy HEAD ref]",
            Staleness::Unknown => "",
            Staleness::Landed => "  [patch landed]",
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

/// Is `sha` present locally at all (`git cat-file -e`)? False on any failure
/// (missing object, no dir, shallow/unfetched) — the guard ancestry checks need
/// before shelling out `merge-base`, which errors noisily on an absent object.
fn object_present(dir: &Path, sha: &str) -> bool {
    gitcmd::output(dir, &["cat-file", "-e", sha]).map(|o| o.status.success()).unwrap_or(false)
}

/// `git merge-base --is-ancestor <a> <b>` — exit 0 ⇒ true. False on any failure
/// (unknown ref, unrelated history) rather than erroring — this is a best-effort
/// display-tier signal (design/44 Addendum 1), never a correctness gate. `pub(crate)`
/// so the `ref-contains` plumbing predicate (design/44 Addendum 1) can reuse it.
pub(crate) fn is_ancestor(dir: &Path, a: &str, b: &str) -> bool {
    gitcmd::output(dir, &["merge-base", "--is-ancestor", a, b]).map(|o| o.status.success()).unwrap_or(false)
}

/// design/44 Addendum 2: is the branch's `fork_point` still an ancestor of
/// `base_ref`'s HEAD? Tries `base_ref` as a local branch first, then as a remote-
/// tracking branch (`origin/<base_ref>`) — best-effort, never fetches.
fn squash_anchor_reachable(dir: &Path, fork_point: &str, base_ref: &str) -> bool {
    is_ancestor(dir, fork_point, base_ref) || is_ancestor(dir, fork_point, &format!("origin/{base_ref}"))
}

/// The full staleness verdict (design/44 Addendum 1+2): composes the cheap content
/// signal (`staleness`, above — pinned blob vs `HEAD:<path>`) with an ancestry
/// signal (is the pinned commit still in HEAD's history?) that's orthogonal to it.
/// Best-effort throughout; never fetches.
///
/// **Precedence (decided, documented here):**
/// 1. `Unpinned` (sha not full-hex) — wins immediately, no git calls.
/// 2. `Current` / `Moved` / the content-signal `Unknown` (no clone or no
///    `content_hash`) — returned as-is. `Current` is already the strongest
///    possible signal (the bytes match HEAD, so ancestry is moot); `Moved` is a
///    path fact that ancestry can't refine.
/// 3. `Changed` is the ONLY verdict this function augments with ancestry:
///    - if the pinned commit object isn't present locally (shallow/unfetched),
///      ancestry can't be computed — per the addendum, fall back to the content
///      signal as-is (`Changed`), never claim `Unknown`/`Offline` from a guess.
///    - if the commit is present and IS an ancestor of HEAD → `Reachable` (the
///      ancestry-confirmed refinement of "changed": still valid, in-line
///      history, just not the tip).
///    - if it is NOT an ancestor of HEAD → `Offline`, UNLESS `fork_point` is
///      present and still an ancestor of `base_ref`'s HEAD → `Squashed` (the
///      branch was very likely merged/squashed away, not abandoned).
pub fn staleness_ex(
    clone: Option<&Path>,
    sha: &str,
    path: &str,
    content_hash: Option<&str>,
    base_ref: Option<&str>,
    fork_point: Option<&str>,
) -> Staleness {
    let base = staleness(clone, sha, path, content_hash);
    if base != Staleness::Changed {
        return base; // Current/Moved/Unpinned/Unknown need no augmentation.
    }
    let Some(dir) = clone else { return base }; // Changed implies clone was Some; defensive.
    if !object_present(dir, sha) {
        return base; // can't test ancestry — content signal stands (addendum fallback).
    }
    if is_ancestor(dir, sha, "HEAD") {
        return Staleness::Reachable;
    }
    match (base_ref, fork_point) {
        (Some(b), Some(f)) if squash_anchor_reachable(dir, f, b) => Staleness::Squashed,
        _ => Staleness::Offline,
    }
}

/// design/45 §1.2/§1.7 + design/48 §4: has a patch's proposed change already landed?
///
/// Two tiers:
/// 1. **Fast path** (cheap, the original check): `HEAD:<path>`'s blob OID equals
///    `result_hash` (a deletion — no `result_hash` — landed iff the path is now absent at
///    HEAD). Only a single `rev-parse`, so it's always tried first.
/// 2. **Durable fallback** (design/48 §4): the fast path is TRANSIENT — it only holds until
///    the next edit to that file, so a patch that genuinely landed long ago reads as not
///    landed the moment the file changes again. When the fast path says "not at HEAD", fall
///    back to `git log --find-object=<result_hash> -- <path>`: did ANY historical commit's
///    tree contain this exact blob at this path? That answers the archival question ("did
///    this land, ever") from any clone, with no tracking state, and survives later edits.
///
/// `Unknown` with no clone; only meaningful for a `patch: true` ref (an ordinary ref's
/// staleness comes from `staleness_ex` instead).
pub fn patch_staleness(clone: Option<&Path>, path: &str, result_hash: Option<&str>) -> Staleness {
    let Some(dir) = clone else { return Staleness::Unknown };
    let at_head = gitcmd::output(dir, &["rev-parse", "--verify", "--quiet", &format!("HEAD:{path}")])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let landed_at_head = match (result_hash, &at_head) {
        (Some(rh), Some(oid)) => oid == rh,
        (None, None) => true,
        _ => false,
    };
    if landed_at_head {
        return Staleness::Landed;
    }
    // Fast path missed (file changed since, or — for a non-deletion patch — the path is
    // currently absent). Fall back to the durable history walk, if we have a blob to search
    // for (a deletion has none, and the fast-path miss already stands as its verdict).
    if let Some(rh) = result_hash {
        if history_contains_blob_at_path(dir, rh, path) {
            return Staleness::Landed;
        }
    }
    Staleness::Unknown
}

/// design/48 §4: the durable half of `patch_staleness` — did ANY historical commit's tree
/// (walking HEAD's ancestry) contain blob `oid` at `path`? `git log --find-object` is the
/// tool built for exactly this ("which commits touched a tree/blob with this OID"), scoped
/// to `path` so an unrelated blob collision elsewhere in the repo can't false-positive.
/// `-1`/`--format=%H` keeps the call cheap — one match is proof enough, we don't need the
/// full list. Degrades to `false` (never panics/propagates) on: no match (blob never landed
/// at this path in this history), a git error (bad object, path never existed, etc.), or
/// anything else non-zero-exit — the caller reads that as `Unknown`, an honest "can't tell",
/// not a false `Landed`.
fn history_contains_blob_at_path(dir: &Path, oid: &str, path: &str) -> bool {
    match gitcmd::output(dir, &["log", "-1", "--format=%H", &format!("--find-object={oid}"), "--", path]) {
        Ok(o) if o.status.success() => !String::from_utf8_lossy(&o.stdout).trim().is_empty(),
        _ => false,
    }
}

/// Total line count of `<sha>:<path>` in `dir` — the cheap general check behind the
/// integrity gate's "range past EOF" rule (design/44 §2). `None` if unresolvable or
/// too large to bother reading (mirrors `snippet`'s size guard).
pub fn blob_line_count(dir: &Path, sha: &str, path: &str) -> Option<u64> {
    let spec = format!("{sha}:{path}");
    let szo = gitcmd::output(dir, &["cat-file", "-s", &spec]).ok()?;
    if !szo.status.success() {
        return None;
    }
    let size: u64 = String::from_utf8_lossy(&szo.stdout).trim().parse().ok()?;
    if size > 2_000_000 {
        return None;
    }
    let o = gitcmd::output(dir, &["cat-file", "-p", &spec]).ok()?;
    if !o.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&o.stdout).lines().count() as u64)
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

/// The `(ref_name · yyyy-mm-dd)` parenthetical shown next to a rendered sha (design/44
/// §5.1) — empty when neither is present. A tag gets a `tag: ` prefix so the common
/// (branch) case reads bare: `(main · 2026-07-12)` vs `(tag: v1.2.0 · 2026-07-12)`.
pub fn identity_paren(ref_name: Option<&str>, ref_type: Option<&str>, commit_date: Option<&str>) -> String {
    let name = ref_name.map(|n| match ref_type {
        Some("tag") => format!("tag: {n}"),
        _ => n.to_string(),
    });
    // commit_date is strict ISO 8601 (`%cI`); the display column is just the date part.
    let date = commit_date.map(|d| d.split('T').next().unwrap_or(d).to_string());
    match (name, date) {
        (Some(n), Some(d)) => format!(" ({n} · {d})"),
        (Some(n), None) => format!(" ({n})"),
        (None, Some(d)) => format!(" ({d})"),
        (None, None) => String::new(),
    }
}

/// `[dirty]`/`[untracked]` flag badges (design/44 §5.1), appended after the staleness
/// badge — the working-tree-snapshot warning distinct from ancestry/content staleness.
fn flag_badges(r: &schema::CodeRef) -> String {
    let mut s = String::new();
    if r.dirty {
        s.push_str("  [dirty]");
    }
    if r.untracked {
        s.push_str("  [untracked]");
    }
    s
}

/// design/44 §3 legacy enrichment: when a ref has a full-hex `sha` but no stored
/// `commit_date` (a pre-Phase-1 ref), best-effort DERIVE it from a mapped clone at
/// read time (`git log -1 --format=%cI <sha>`) — the date is intrinsic to the sha,
/// so this is derivation, not guessing; omitted when no clone/object holds it.
/// Never enriches `ref_name` — the checked-out branch at write time is gone, and
/// resolving a symbolic name *now* would answer the wrong question (§3).
pub fn enrich_commit_date(clone: Option<&Path>, sha: &str, stored: Option<&str>) -> Option<String> {
    if stored.is_some() {
        return stored.map(str::to_string);
    }
    if !is_full_hex(sha) {
        return None;
    }
    let dir = clone?;
    let o = gitcmd::output(dir, &["log", "-1", "--format=%cI", sha]).ok()?;
    if !o.status.success() {
        return None;
    }
    let d = String::from_utf8_lossy(&o.stdout).trim().to_string();
    (!d.is_empty()).then_some(d)
}

/// A full multi-line render of a ref for `show`: the pointer + branch/date + a
/// staleness badge, then a bounded snippet when the repo is cloned here, or a "map a
/// clone" hint when not.
pub fn render_resolved(repo_inv: &repos::Repos, r: &schema::CodeRef, max_lines: usize) -> String {
    let clone = clone_for(repo_inv, &r.repo);
    let rng = r.range.map(|x| format!("#L{}-{}", x[0], x[1])).unwrap_or_default();
    let short: String = if is_full_hex(&r.sha) {
        r.sha[..r.sha.len().min(9)].to_string()
    } else {
        r.sha.clone()
    };
    let st = staleness_ex(
        clone.as_deref(),
        &r.sha,
        &r.path,
        r.content_hash.as_deref(),
        r.base_ref.as_deref(),
        r.fork_point.as_deref(),
    );
    // §3 legacy enrichment: fill a missing commit_date for a full-hex sha, best-effort.
    let enriched_date = enrich_commit_date(clone.as_deref(), &r.sha, r.commit_date.as_deref());
    let mut s = format!(
        "⟶ {}:{} @{}{}{}{}{}",
        r.repo,
        r.path,
        short,
        identity_paren(r.ref_name.as_deref(), r.ref_type.as_deref(), enriched_date.as_deref()),
        rng,
        st.badge(),
        flag_badges(r)
    );
    if clone.is_none() {
        s.push_str(&format!(
            "\n   (not cloned here — `confer repos map {} <path>` to see the code)",
            r.repo
        ));
        return s;
    }
    // design/45 §1.7: the patch chip — "proposed a change here (applied/open)", detected purely
    // from `result_hash` vs `HEAD:<path>` (Staleness::Landed), no tracking state involved.
    if r.patch {
        let applied = patch_staleness(clone.as_deref(), &r.path, r.result_hash.as_deref()) == Staleness::Landed;
        s.push_str(&format!("\n   ⟳ proposed a change here ({})", if applied { "applied" } else { "open" }));
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
    fn staleness_unresolved_sha_is_unpinned_like_legacy_head() {
        // design/44 §3: the new `sha: "unresolved"` marker (the untracked / no-clone
        // forced no-pin case) flows through the SAME non-full-hex → Unpinned path as a
        // legacy `sha: HEAD` ref — old binaries render both correctly with zero compat work.
        assert_eq!(staleness(None, "unresolved", "f.rs", None), Staleness::Unpinned);
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
    fn patch_staleness_fast_path_at_head() {
        let (dir, _head, blob) = repo_with_file("patch-fast", "a\nb\n");
        // HEAD:<path> already matches result_hash — Landed via the cheap peek, no history walk.
        assert_eq!(patch_staleness(Some(&dir), "f.rs", Some(&blob)), Staleness::Landed);
        // a deletion (no result_hash) whose path is genuinely absent at HEAD — also Landed.
        assert_eq!(patch_staleness(Some(&dir), "gone.rs", None), Staleness::Landed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn patch_staleness_durable_fallback_after_a_later_edit() {
        // design/48 §4: the exact bug this fixes. A patch's result_hash landed at some commit,
        // but the file has since changed again — the fast HEAD:<path> peek now reads "not
        // landed" even though the patch demonstrably landed at some point in history. The
        // durable `git log --find-object` walk must still say Landed.
        let (dir, _head1, landed_blob) = repo_with_file("patch-durable", "landed content\n");
        std::fs::write(dir.join("f.rs"), "a later, unrelated edit\n").unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "c1 supersedes the landed content"]);
        // fast path alone would say Unknown (HEAD:<path> no longer matches landed_blob) —
        // patch_staleness must fall back to history and still report Landed.
        assert_eq!(patch_staleness(Some(&dir), "f.rs", Some(&landed_blob)), Staleness::Landed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn patch_staleness_unknown_when_blob_never_landed() {
        let (dir, _head, _blob) = repo_with_file("patch-never", "a\nb\n");
        // a result_hash that never existed at this path, at HEAD or anywhere in history.
        assert_eq!(
            patch_staleness(Some(&dir), "f.rs", Some(&"f".repeat(40))),
            Staleness::Unknown
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn patch_staleness_unknown_without_a_clone() {
        assert_eq!(patch_staleness(None, "f.rs", Some(&"a".repeat(40))), Staleness::Unknown);
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
            ref_name: None,
            ref_type: None,
            commit_date: None,
            dirty: false,
            untracked: false,
            rev: None,
            base_ref: None,
            fork_point: None,
            patch: false,
            result_hash: None,
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
        let r = schema::CodeRef {
            repo: "mylib".into(),
            sha: "HEAD".into(),
            path: "f.rs".into(),
            range: None,
            content_hash: None,
            ref_name: None,
            ref_type: None,
            commit_date: None,
            dirty: false,
            untracked: false,
            rev: None,
            base_ref: None,
            fork_point: None,
            patch: false,
            result_hash: None,
        };
        let out = render_resolved(&repo_inv, &r, 50);
        // a non-full-hex sha (legacy "HEAD") is shown as-is, not truncated/mangled.
        assert!(out.contains("@HEAD"), "out: {out}");
        assert!(out.contains("unpinned"), "legacy HEAD refs should badge as unpinned: {out}");
    }

    // ── design/44 Addendum 1+2: ancestry-augmented staleness (`staleness_ex`) ──────

    #[test]
    fn staleness_ex_reachable_when_ancestor_but_content_changed() {
        // Two commits on the same line: pin the FIRST (still an ancestor of HEAD),
        // content differs at HEAD → the ancestry-confirmed "reachable" verdict, not
        // the bare "changed" `staleness()` alone would give.
        let (dir, head1, blob1) = repo_with_file("reachable", "a\nb\n");
        std::fs::write(dir.join("f.rs"), "a\nb\nc\n").unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "c1"]);
        assert_eq!(
            staleness_ex(Some(&dir), &head1, "f.rs", Some(&blob1), None, None),
            Staleness::Reachable
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn staleness_ex_offline_when_rebased_away_and_no_squash_anchor() {
        // A commit that was on `main` gets rewritten away (simulating a rebase/reset
        // discarding it) — the pinned sha is no longer an ancestor of HEAD, and with
        // no base_ref/fork_point to check, that's "offline" (genuinely fragile).
        let (dir, old_head, blob) = repo_with_file("offline", "a\nb\n");
        // Rewrite history in place: amending the sole (root) commit replaces it with a
        // DIFFERENT commit object (different tree) — `old_head` is now unreachable from
        // the rewritten HEAD, the same shape a rebase/force-push leaves behind.
        std::fs::write(dir.join("f.rs"), "changed\n").unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "--amend", "-q", "-m", "rewritten"]);
        assert_eq!(
            staleness_ex(Some(&dir), &old_head, "f.rs", Some(&blob), None, None),
            Staleness::Offline
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn staleness_ex_squashed_when_fork_point_still_reachable_on_base() {
        // A feature branch forks off main, gets a commit (the pinned sha), then main
        // squash-merges it away (the branch commit itself becomes unreachable, but the
        // fork point — the merge-base — survives on main). That's the "squashed"
        // verdict: offline, but explained by a real merge, not abandonment.
        let dir = std::env::temp_dir().join(format!("confer-refcode-squash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("f.rs"), "base\n").unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "base"]);
        let fork_point = String::from_utf8_lossy(
            &Command::new("git").arg("-C").arg(&dir).args(["rev-parse", "HEAD"]).output().unwrap().stdout,
        )
        .trim()
        .to_string();
        let blob = String::from_utf8_lossy(
            &Command::new("git").arg("-C").arg(&dir).args(["rev-parse", "HEAD:f.rs"]).output().unwrap().stdout,
        )
        .trim()
        .to_string();
        git(&dir, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(dir.join("f.rs"), "feature work\n").unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "feature commit"]); // the ref's pinned sha
        let feature_sha = String::from_utf8_lossy(
            &Command::new("git").arg("-C").arg(&dir).args(["rev-parse", "HEAD"]).output().unwrap().stdout,
        )
        .trim()
        .to_string();
        // Squash-merge onto main: a NEW commit with the feature's content, main's
        // history never contains `feature_sha` itself.
        git(&dir, &["checkout", "-q", "main"]);
        git(&dir, &["merge", "-q", "--squash", "feature"]);
        git(&dir, &["commit", "-q", "-m", "squashed feature"]);
        // sanity: the feature commit is genuinely gone from main's history.
        assert!(!is_ancestor(&dir, &feature_sha, "main"));
        assert!(is_ancestor(&dir, &fork_point, "main"), "the fork point must survive the squash");

        assert_eq!(
            staleness_ex(Some(&dir), &feature_sha, "f.rs", Some(&blob), Some("main"), Some(&fork_point)),
            Staleness::Squashed
        );
        // Without the fork_point/base_ref, the SAME unreachable sha reads as plain Offline.
        assert_eq!(
            staleness_ex(Some(&dir), &feature_sha, "f.rs", Some(&blob), None, None),
            Staleness::Offline
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn staleness_ex_falls_back_to_changed_when_object_absent_locally() {
        // design/44 Addendum 1: when the pinned commit ISN'T present locally (shallow
        // clone / unfetched), ancestry can't be computed — fall back to the content
        // signal as-is ("changed"), never claim Offline/Unknown from a guess.
        let (dir, _head, blob) = repo_with_file("noobject", "a\nb\n");
        std::fs::write(dir.join("f.rs"), "a\nb\nc\n").unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "c1"]);
        let bogus_sha = "f".repeat(40); // well-formed hex, but no such object exists
        assert_eq!(
            staleness_ex(Some(&dir), &bogus_sha, "f.rs", Some(&blob), None, None),
            Staleness::Changed
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn staleness_ex_passes_through_current_moved_unpinned_unknown_untouched() {
        let (dir, head, blob) = repo_with_file("passthrough", "a\nb\n");
        assert_eq!(staleness_ex(Some(&dir), &head, "f.rs", Some(&blob), None, None), Staleness::Current);
        assert_eq!(
            staleness_ex(Some(&dir), &head, "renamed.rs", Some(&blob), None, None),
            Staleness::Moved
        );
        assert_eq!(staleness_ex(Some(&dir), "HEAD", "f.rs", Some(&blob), None, None), Staleness::Unpinned);
        assert_eq!(staleness_ex(None, &head, "f.rs", Some(&blob), None, None), Staleness::Unknown);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── design/44 §3: legacy `commit_date` enrichment ───────────────────────────────

    #[test]
    fn enrich_commit_date_keeps_a_stored_value_untouched() {
        // Even with a clone that could derive a DIFFERENT date, an already-stored
        // value is never overwritten — enrichment is only for what's MISSING.
        let (dir, head, _blob) = repo_with_file("enrich-stored", "a\n");
        assert_eq!(
            enrich_commit_date(Some(&dir), &head, Some("2020-01-01T00:00:00Z")),
            Some("2020-01-01T00:00:00Z".to_string())
        );
    }

    #[test]
    fn enrich_commit_date_derives_from_a_mapped_clone_for_a_full_hex_sha() {
        let (dir, head, _blob) = repo_with_file("enrich-derive", "a\n");
        let got = enrich_commit_date(Some(&dir), &head, None).expect("should derive a date");
        assert!(got.contains('T'), "expected an ISO 8601 date, got {got}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn enrich_commit_date_omits_for_legacy_non_full_hex_sha() {
        // "ref_name is not reconstructable" reasoning extends to refusing to derive
        // ANYTHING for a non-pinned (`HEAD`/branch) legacy sha — there's no single
        // commit to date.
        assert_eq!(enrich_commit_date(None, "HEAD", None), None);
    }

    #[test]
    fn enrich_commit_date_omits_without_a_clone() {
        assert_eq!(enrich_commit_date(None, &"a".repeat(40), None), None);
    }
}
