//! design/45 — the patch primitive: a `confer-patch` body fence (a raw unified diff) paired
//! with `patch: true` refs derived from the diff itself. Three concerns live here:
//!
//! - **diff parsing** (`parse_diff_touched_files`, `changed_line_count`) — cheap, dependency-free
//!   parsing of a unified diff's per-file headers/hunks, shared by the write path (deriving refs +
//!   the size gate) and the pairing rule.
//! - **the fence + the anti-spoof pairing rule** (`patch_fence`, `parse_patch_fence`, `pair_patch`)
//!   — §1.2: a `confer-patch` fence is honored only when frontmatter `patch: true` refs matching
//!   its repo+sha cover EVERY path the diff touches; otherwise it's an orphan (renders as a plain
//!   code block; `confer apply` refuses it).
//! - **write-time validation** (`validate_and_derive`, §1.4) and **apply** (`cmd_apply`, §1.5) —
//!   both go through a temp `GIT_INDEX_FILE`/a real `git apply`, never the message's own working
//!   tree at write time, and never a commit/push at apply time.

use crate::schema::CodeRef;
use crate::{
    config, gitcmd, repomap, repos, resolve_unique, short_id, store, AlreadyLanded, PredicateFalse,
};
use anyhow::{anyhow, bail, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ── size gate (design/45 §1.2) ──────────────────────────────────────────────────────────────

/// Above this many CHANGED (+/-) lines, `--patch` warns (not an error).
pub(crate) const WARN_LINES: usize = 150;
/// Above this, `--patch` refuses unless `--allow-large-patch`.
pub(crate) const REFUSE_LINES: usize = 400;
/// The hard cap even WITH `--allow-large-patch` — beyond this the change is structural: it
/// travels as a git branch, not a patch.
pub(crate) const HARD_CAP_LINES: usize = 2000;

/// Count of actual +/- content lines (never `+++`/`---` file headers) across a unified diff —
/// the size-gate's unit (design/45 §1.2: "~150 changed lines" / "~400 total diff lines").
pub(crate) fn changed_line_count(diff: &str) -> usize {
    diff.lines()
        .filter(|l| {
            (l.starts_with('+') && !l.starts_with("+++")) || (l.starts_with('-') && !l.starts_with("---"))
        })
        .count()
}

/// The size gate: `Ok(None)` under the warn line, `Ok(Some(warning))` between warn/refuse (or
/// above refuse with `--allow-large-patch`, under the hard cap), `Err` above refuse (without the
/// flag) or above the hard cap (even with it) — "suggestion-scale changes travel as patches;
/// change-scale changes travel as git branches" (design/45 §1.2).
pub(crate) fn size_gate(diff: &str, allow_large: bool) -> Result<Option<String>> {
    let n = changed_line_count(diff);
    if n > HARD_CAP_LINES {
        return Err(anyhow!(
            "--patch: {n} changed lines exceeds the hard cap of {HARD_CAP_LINES} even with \
             --allow-large-patch — suggestion-scale changes travel as patches; change-scale \
             changes travel as git branches: push the branch and --ref the commit instead."
        ));
    }
    if n > REFUSE_LINES {
        if allow_large {
            return Ok(Some(format!(
                "--patch: {n} changed lines (> {REFUSE_LINES}) — large for a suggestion; sent anyway (--allow-large-patch)"
            )));
        }
        return Err(anyhow!(
            "--patch: {n} changed lines exceeds {REFUSE_LINES} — refusing. Pass --allow-large-patch \
             (hard cap {HARD_CAP_LINES}), or — better — travel this as a git branch: push it and --ref the commit."
        ));
    }
    if n > WARN_LINES {
        return Ok(Some(format!(
            "--patch: {n} changed lines (> {WARN_LINES}) — large for a suggestion; consider a branch instead"
        )));
    }
    Ok(None)
}

// ── unified-diff parsing (dependency-free; no external diff/patch crate) ───────────────────

/// One file a unified diff touches: its (new-side) path, the bounding span of its OLD-side
/// hunks (design/45 §1.2's ref `range` — `None` for a pure creation, which has no old side), and
/// whether it's a creation/deletion (governs `content_hash`/`result_hash` presence downstream).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TouchedFile {
    pub(crate) path: String,
    pub(crate) old_range: Option<[u64; 2]>,
    pub(crate) is_creation: bool,
    pub(crate) is_deletion: bool,
}

fn parse_hunk_span(s: &str) -> Option<(u64, u64)> {
    match s.split_once(',') {
        Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
        None => Some((s.parse().ok()?, 1)),
    }
}

/// Fold one `@@ -a[,b] +c[,d] @@` header into the running old-side hunk list for the file in
/// progress (a no-op on a line that isn't a hunk header).
fn fold_hunk_header(line: &str, hunks: &mut Vec<(u64, u64)>) {
    let Some(rest) = line.strip_prefix("@@ -") else { return };
    let Some(end) = rest.find(" @@") else { return };
    let Some((old, _new)) = rest[..end].split_once(" +") else { return };
    if let Some(span) = parse_hunk_span(old) {
        hunks.push(span);
    }
}

/// Close out the file currently being parsed: fold its collected old-side hunks into a bounding
/// `[min_start, max_end]` span (§1.2 — "range = the bounding span of that file's old-side
/// hunks"), `None` when there were none (a pure creation: every hunk is `-0,0`).
fn close_file(cur: Option<TouchedFile>, hunks: &mut Vec<(u64, u64)>, out: &mut Vec<TouchedFile>) {
    if let Some(mut f) = cur {
        let starts: Vec<u64> = hunks.iter().map(|h| h.0).filter(|&s| s > 0).collect();
        if !starts.is_empty() {
            let s = starts.iter().copied().min().unwrap();
            let e = hunks
                .iter()
                .filter(|h| h.0 > 0)
                .map(|h| if h.1 == 0 { h.0 } else { h.0 + h.1 - 1 })
                .max()
                .unwrap_or(s);
            f.old_range = Some([s, e.max(s)]);
        }
        out.push(f);
    }
    hunks.clear();
}

/// Parse a unified diff (`git diff --full-index -U3` form, or any hand-authored equivalent) into
/// the files it touches. Dependency-free: reads only `--- a/…`/`+++ b/…`/`@@ … @@` lines, so it
/// tolerates any hunk content (including a body that itself contains `diff --git` as text, since
/// file boundaries are anchored on the `---`/`+++` pair, not the `diff --git` header alone).
pub(crate) fn parse_diff_touched_files(diff: &str) -> Vec<TouchedFile> {
    let mut out = Vec::new();
    let mut cur: Option<TouchedFile> = None;
    let mut hunks: Vec<(u64, u64)> = Vec::new();
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            close_file(cur.take(), &mut hunks, &mut out);
        } else if let Some(rest) = line.strip_prefix("--- ") {
            close_file(cur.take(), &mut hunks, &mut out);
            let rest = rest.trim();
            let old_null = rest == "/dev/null";
            let old_path = rest.strip_prefix("a/").unwrap_or(rest).to_string();
            cur = Some(TouchedFile {
                path: old_path,
                old_range: None,
                is_creation: old_null,
                is_deletion: false,
            });
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            let rest = rest.trim();
            let new_null = rest == "/dev/null";
            if let Some(f) = cur.as_mut() {
                f.is_deletion = new_null;
                if !new_null {
                    f.path = rest.strip_prefix("b/").unwrap_or(rest).to_string();
                }
            }
        } else {
            fold_hunk_header(line, &mut hunks);
        }
    }
    close_file(cur, &mut hunks, &mut out);
    out
}

// ── the `confer-patch` fence: emit + parse + the anti-spoof pairing rule (§1.2) ────────────

/// Emit the `confer-patch` body fence (design/45 §1.2): `repo=`/`sha=` attrs, then the raw
/// unified diff verbatim.
pub(crate) fn patch_fence(repo: &str, sha: &str, diff: &str) -> String {
    let mut s = format!("```confer-patch repo={repo} sha={sha}\n");
    s.push_str(diff.trim_end_matches('\n'));
    s.push('\n');
    s.push_str("```\n");
    s
}

/// One parsed `confer-patch` fence, BEFORE the pairing rule is applied.
pub(crate) struct ParsedFence {
    pub(crate) repo: String,
    pub(crate) sha: String,
    pub(crate) diff: String,
}

/// Parse the FIRST ` ```confer-patch ` fence out of a message body. Malformed (missing `repo=`/
/// `sha=`, or no closing fence) → `None` — never partially trust a broken header.
pub(crate) fn parse_patch_fence(body: &str) -> Option<ParsedFence> {
    let start = body.find("```confer-patch")?;
    let header_end = body[start..].find('\n')? + start;
    let header = body[start..header_end].strip_prefix("```confer-patch")?.trim();
    let mut repo = None;
    let mut sha = None;
    for tok in header.split_whitespace() {
        if let Some(v) = tok.strip_prefix("repo=") {
            repo = Some(v.to_string());
        } else if let Some(v) = tok.strip_prefix("sha=") {
            sha = Some(v.to_string());
        }
    }
    let rest = &body[header_end + 1..];
    let end = rest.find("\n```")?;
    // `patch_fence` always emits the diff with EXACTLY one trailing `\n` before the closing
    // fence — `end` points at that newline, so `rest[..end]` excludes it; restore it here. A
    // unified diff missing its final newline is a DIFFERENT, meaningful thing to `git apply`
    // (a "\ No newline at end of file" marker) — dropping it silently produced a "corrupt patch"
    // (git counts the trailing content line as unterminated).
    Some(ParsedFence { repo: repo?, sha: sha?, diff: format!("{}\n", &rest[..end]) })
}

/// One touched file as `confer apply`/the write path see it after pairing: the path and the
/// `result_hash` its matching ref carried (absent on a deletion).
#[derive(Debug, Clone)]
pub(crate) struct PatchFileRef {
    pub(crate) path: String,
    pub(crate) result_hash: Option<String>,
}

/// A `confer-patch` fence that PASSED the anti-spoof pairing rule.
pub(crate) struct PairedPatch {
    pub(crate) repo: String,
    pub(crate) sha: String,
    pub(crate) diff: String,
    pub(crate) files: Vec<PatchFileRef>,
}

/// The anti-spoof pairing rule (design/45 §1.2): a `confer-patch` fence is honored ONLY when the
/// frontmatter carries `patch: true` refs matching its repo+sha and covering EVERY path the diff
/// touches. `None` on any gap — an orphan fence, a fence whose refs don't cover every touched
/// path, or one with no parseable files at all. Since the write path DERIVES refs from the diff,
/// an honestly-authored patch always pairs; a body-only forgery never does.
pub(crate) fn pair_patch(body: &str, refs: &[CodeRef]) -> Option<PairedPatch> {
    let fence = parse_patch_fence(body)?;
    let touched = parse_diff_touched_files(&fence.diff);
    if touched.is_empty() {
        return None;
    }
    let matching: Vec<&CodeRef> =
        refs.iter().filter(|r| r.patch && r.repo == fence.repo && r.sha == fence.sha).collect();
    let mut files = Vec::with_capacity(touched.len());
    for t in &touched {
        let r = matching.iter().find(|r| r.path == t.path)?;
        files.push(PatchFileRef { path: t.path.clone(), result_hash: r.result_hash.clone() });
    }
    Some(PairedPatch { repo: fence.repo, sha: fence.sha, diff: fence.diff, files })
}

// ── write-time validation: one temp-index apply yields the gate AND `result_hash` (§1.4) ───

/// A path removed on drop — the temp index file and the temp patch file this module writes are
/// both machine-local scratch, never left behind on success OR on an early `?` return.
struct TempFile(PathBuf);
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn write_temp_patch(diff: &str) -> Result<PathBuf> {
    let p = std::env::temp_dir().join(format!(
        "confer-patch-{}-{}.diff",
        std::process::id(),
        ulid::Ulid::new()
    ));
    std::fs::write(&p, diff)?;
    Ok(p)
}

/// Write-time integrity gate + `result_hash` derivation (design/45 §1.4): read the pinned base
/// `sha` into a TEMP index (never the repo's real index), `apply --cached` the diff there, then
/// read back each touched path's new blob OID — one operation, three products: the apply-gate
/// (an `Err` here is "refuse the send"), `result_hash` per file (absent for a deletion — the path
/// is gone from the temp index), and (via the caller's own `parse_diff_touched_files`) the
/// touched-path list the derived refs are built from. The working tree is NEVER touched.
pub(crate) fn validate_and_derive(dir: &Path, sha: &str, diff: &str) -> Result<HashMap<String, String>> {
    let idx_path = std::env::temp_dir().join(format!(
        "confer-patch-index-{}-{}",
        std::process::id(),
        ulid::Ulid::new()
    ));
    let _idx_guard = TempFile(idx_path.clone());
    let idx_s = idx_path.to_string_lossy().to_string();
    let env = [("GIT_INDEX_FILE", idx_s.as_str())];

    let o = gitcmd::output_env(dir, &["read-tree", sha], &env)?;
    if !o.status.success() {
        bail!(
            "cannot pin --patch to {sha} in {}: {}",
            dir.display(),
            String::from_utf8_lossy(&o.stderr).trim()
        );
    }

    let patch_path = write_temp_patch(diff)?;
    let _patch_guard = TempFile(patch_path.clone());
    let p = patch_path.to_string_lossy().to_string();
    let o = gitcmd::output_env(dir, &["apply", "--cached", &p], &env)?;
    if !o.status.success() {
        return Err(anyhow!(
            "the patch does not apply at {} — regenerate it against the base you mean ({})",
            &sha[..sha.len().min(9)],
            String::from_utf8_lossy(&o.stderr).trim()
        ));
    }

    let touched = parse_diff_touched_files(diff);
    let paths: Vec<&str> = touched.iter().map(|t| t.path.as_str()).collect();
    let mut args = vec!["ls-files", "-s", "--"];
    args.extend(paths.iter().copied());
    let o = gitcmd::output_env(dir, &args, &env)?;
    if !o.status.success() {
        bail!("could not read the temp index after applying --patch");
    }
    let mut hashes = HashMap::new();
    for line in String::from_utf8_lossy(&o.stdout).lines() {
        if let Some((meta, path)) = line.split_once('\t') {
            if let Some(blob) = meta.split_whitespace().nth(1) {
                hashes.insert(path.to_string(), blob.to_string());
            }
        }
    }
    Ok(hashes)
}

// ── applying a patch: `confer apply` (design/45 §1.5) ───────────────────────────────────────

fn head_sha(dir: &Path) -> Option<String> {
    gitcmd::output(dir, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Is the base commit object present locally at all (shallow/unfetched clones may not have it) —
/// the "unresolvable here" gate before attempting a `--3way` reconstruction.
fn base_object_present(dir: &Path, sha: &str) -> bool {
    gitcmd::output(dir, &["cat-file", "-e", &format!("{sha}^{{commit}}")])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Uncommitted-modification guard (mirror of the write-time gate, §1.5 rule 3): the touched
/// paths with any `git status --porcelain` entry — never stack a proposal onto unsaved work.
fn dirty_paths(dir: &Path, paths: &[&str]) -> Vec<String> {
    let mut args = vec!["status", "--porcelain", "--"];
    args.extend(paths.iter().copied());
    let Ok(o) = gitcmd::output(dir, &args) else { return Vec::new() };
    if !o.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| l.get(3..).map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect()
}

/// The landed short-circuit (§1.5 rule 2): every touched path's `HEAD:<path>` blob OID equals
/// its `result_hash` (a deletion — no `result_hash` — landed iff the path is absent at HEAD).
fn already_landed(dir: &Path, files: &[PatchFileRef]) -> bool {
    if files.is_empty() {
        return false;
    }
    files.iter().all(|f| {
        let at_head = gitcmd::output(dir, &["rev-parse", "--verify", "--quiet", &format!("HEAD:{}", f.path)])
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        match (&f.result_hash, at_head) {
            (Some(rh), Some(oid)) => &oid == rh,
            (None, None) => true, // a deleted file's path is (still) absent at HEAD
            _ => false,
        }
    })
}

/// A scratch linked worktree, force-removed on drop — never leaves a stray worktree registration
/// or directory behind, success or not.
struct ScratchWorktree {
    main: PathBuf,
    wt: PathBuf,
}
impl Drop for ScratchWorktree {
    fn drop(&mut self) {
        let wt_s = self.wt.to_string_lossy().to_string();
        let _ = gitcmd::output(&self.main, &["worktree", "remove", "--force", &wt_s]);
        let _ = std::fs::remove_dir_all(&self.wt);
    }
}

/// `git apply --check [--3way] <patch>` — never writes to `dir`. For the plain (base == HEAD)
/// case, `git apply --check` itself is reliable. For `--3way`, `git apply --check --3way` is
/// NOT: git reports success even when the merge would leave conflict markers (verified — `--check`
/// can't simulate a real 3-way merge). So the 3-way case instead performs the REAL `git apply
/// --3way` inside a disposable linked worktree (fast — shares objects, no full clone) and looks
/// for leftover unmerged (`U`) paths; the worktree is force-removed before returning either way,
/// so `dir` itself is never touched.
fn apply_check(dir: &Path, patch_path: &Path, use_3way: bool) -> Result<bool> {
    let p = patch_path.to_string_lossy().to_string();
    if !use_3way {
        let o = gitcmd::output(dir, &["apply", "--check", &p])?;
        return Ok(o.status.success());
    }
    let wt = std::env::temp_dir().join(format!(
        "confer-apply-check-{}-{}",
        std::process::id(),
        ulid::Ulid::new()
    ));
    let wt_s = wt.to_string_lossy().to_string();
    let o = gitcmd::output(dir, &["worktree", "add", "--detach", "-q", &wt_s, "HEAD"])?;
    if !o.status.success() {
        bail!(
            "could not create a scratch worktree to check the 3-way apply: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        );
    }
    let _cleanup = ScratchWorktree { main: dir.to_path_buf(), wt: wt.clone() };
    let o = gitcmd::output(&wt, &["apply", "--3way", &p])?;
    if !o.status.success() {
        return Ok(false);
    }
    let unmerged = gitcmd::output(&wt, &["diff", "--name-only", "--diff-filter=U"])?;
    Ok(unmerged.status.success() && String::from_utf8_lossy(&unmerged.stdout).trim().is_empty())
}

/// `git apply [--3way] <patch>` against the real working tree — the ONLY write `confer apply`
/// ever performs; never a commit, never a push (design/45 §1.5 rule 5).
fn apply_real(dir: &Path, patch_path: &Path, use_3way: bool) -> Result<()> {
    let p = patch_path.to_string_lossy().to_string();
    let mut args = vec!["apply"];
    if use_3way {
        args.push("--3way");
    }
    args.push(&p);
    let o = gitcmd::output(dir, &args)?;
    if !o.status.success() {
        // A --3way conflict DOES modify the working tree (conflict markers, like `git merge`) —
        // git's own stderr says so; surface it as an error anyway so a script can't mistake this
        // for a clean apply, but the markers are real and left in place for manual resolution.
        return Err(anyhow!(
            "git apply{} did not complete cleanly: {}",
            if use_3way { " --3way" } else { "" },
            String::from_utf8_lossy(&o.stderr).trim()
        ));
    }
    Ok(())
}

/// Resolve the target repo dir for `confer apply`: `--repo-dir` is the `--ref-from` analogue
/// (design/44 §1.1's precedence, reused verbatim via `repomap::capture_dir`).
fn resolve_apply_dir(root: &Path, repo: &str, repo_dir: Option<&str>) -> Option<repomap::Capture> {
    let repo_inv = repos::load(root);
    let card_root_sha = repo_inv.get(repo).and_then(|c| c.root_sha.clone());
    repomap::capture_dir(repo, card_root_sha.as_deref(), repo_dir.map(Path::new))
}

/// `confer apply <msg-id> [--check] [--repo-dir <dir>] [--force]` (design/45 §1.5) — apply a
/// message's `confer-patch` to its target repo's WORKING TREE. Never commits, never pushes:
/// confer stops the instant `git apply` returns, leaving review/commit/attribution to the
/// applier in their own repo. `--check` is the predicate form (design/37): exit 0 applies
/// cleanly, 1 conflicts/drift, 2 already landed, 3 unresolvable here.
pub(crate) fn cmd_apply(id: String, check: bool, repo_dir: Option<String>, force: bool) -> Result<()> {
    let root = config::repo_root()?;
    let msgs = store::all_messages(&root)?;
    let full_id = resolve_unique(&msgs, &id)?.to_string();
    let m = msgs
        .iter()
        .find(|m| m.front.id == full_id)
        .ok_or_else(|| anyhow!("message {id} not found"))?;

    let paired = pair_patch(&m.body, &m.front.refs).ok_or_else(|| {
        anyhow!(
            "{} is not a valid patch — no confer-patch fence paired with matching patch:true refs covering every touched path",
            short_id(&full_id)
        )
    })?;

    let Some(capture) = resolve_apply_dir(&root, &paired.repo, repo_dir.as_deref()) else {
        return Err(anyhow!(
            "unresolvable: no local clone of '{}' is mapped on this machine (or reachable from \
             --repo-dir) — map one: `confer repos map {} <path>`.",
            paired.repo,
            paired.repo
        ));
    };
    let dir = &capture.dir;

    if already_landed(dir, &paired.files) {
        if check {
            return Err(AlreadyLanded.into());
        }
        println!("{}: already landed — HEAD already has this change; nothing to apply", short_id(&full_id));
        return Ok(());
    }

    let touched_paths: Vec<&str> = paired.files.iter().map(|f| f.path.as_str()).collect();
    let dirty = dirty_paths(dir, &touched_paths);
    if !dirty.is_empty() && !force {
        return Err(anyhow!(
            "refusing to apply — uncommitted changes on {} (commit/stash first, or pass --force)",
            dirty.join(", ")
        ));
    }

    let use_3way = head_sha(dir).as_deref() != Some(paired.sha.as_str());
    if use_3way && !base_object_present(dir, &paired.sha) {
        return Err(anyhow!(
            "unresolvable: base {} isn't present in {} (a shallow/unfetched clone) — fetch it first",
            &paired.sha[..paired.sha.len().min(9)],
            dir.display()
        ));
    }

    let patch_path = write_temp_patch(&paired.diff)?;
    let _guard = TempFile(patch_path.clone());

    if check {
        return if apply_check(dir, &patch_path, use_3way)? { Ok(()) } else { Err(PredicateFalse.into()) };
    }

    apply_real(dir, &patch_path, use_3way)?;
    println!(
        "{}: applied{} in {} — review + commit it yourself; confer never commits/pushes to a work repo.",
        short_id(&full_id),
        if use_3way { " (3-way)" } else { "" },
        dir.display(),
    );
    println!(
        "close the loop once committed: confer done --of <req> --ref {}:<path>@<landed-sha>",
        paired.repo
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_line_count_ignores_file_headers() {
        let diff = "diff --git a/f b/f\nindex 111..222 100644\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n-old1\n-old2\n+new1\n+new2\n context\n";
        assert_eq!(changed_line_count(diff), 4);
    }

    #[test]
    fn size_gate_thresholds() {
        let make = |n: usize| {
            let mut s = String::from("diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,1 +1,1 @@\n");
            for _ in 0..n {
                s.push_str("+x\n");
            }
            s
        };
        assert_eq!(size_gate(&make(10), false).unwrap(), None);
        assert!(size_gate(&make(200), false).unwrap().is_some(), "between warn/refuse → Some(warning)");
        assert!(size_gate(&make(500), false).is_err(), "above refuse without the flag → Err");
        assert!(size_gate(&make(500), true).unwrap().is_some(), "above refuse WITH the flag → Some(warning)");
        assert!(size_gate(&make(3000), true).is_err(), "above the hard cap even with the flag → Err");
    }

    #[test]
    fn parse_diff_touched_files_modification_creation_deletion() {
        let diff = "\
diff --git a/mod.rs b/mod.rs
index aaa..bbb 100644
--- a/mod.rs
+++ b/mod.rs
@@ -10,3 +10,4 @@
 ctx
-old
+new
+extra
diff --git a/new.rs b/new.rs
new file mode 100644
index 0000000..ccc
--- /dev/null
+++ b/new.rs
@@ -0,0 +1,2 @@
+line1
+line2
diff --git a/gone.rs b/gone.rs
deleted file mode 100644
index ddd..0000000
--- a/gone.rs
+++ /dev/null
@@ -1,2 +0,0 @@
-bye1
-bye2
";
        let files = parse_diff_touched_files(diff);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "mod.rs");
        assert!(!files[0].is_creation && !files[0].is_deletion);
        assert_eq!(files[0].old_range, Some([10, 12]));

        assert_eq!(files[1].path, "new.rs");
        assert!(files[1].is_creation);
        assert_eq!(files[1].old_range, None, "a pure creation has no old-side range");

        assert_eq!(files[2].path, "gone.rs");
        assert!(files[2].is_deletion);
        assert_eq!(files[2].old_range, Some([1, 2]));
    }

    #[test]
    fn fence_emit_and_parse_round_trip() {
        let diff = "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n";
        let fence = patch_fence("app", &"a".repeat(40), diff);
        assert!(fence.starts_with("```confer-patch repo=app sha="));
        let body = format!("some text\n\n{fence}\nmore text");
        let parsed = parse_patch_fence(&body).expect("should parse");
        assert_eq!(parsed.repo, "app");
        assert_eq!(parsed.sha, "a".repeat(40));
        // EXACT round-trip, including the trailing newline `git apply` needs on the last content
        // line (dropping it silently turns a valid diff into a "corrupt patch").
        assert_eq!(parsed.diff, diff);
    }

    #[test]
    fn parse_patch_fence_missing_attrs_is_none() {
        assert!(parse_patch_fence("```confer-patch repo=app\nx\n```").is_none(), "missing sha=");
        assert!(parse_patch_fence("no fence here at all").is_none());
    }

    fn code_ref_patch(repo: &str, path: &str, sha: &str, result_hash: Option<&str>) -> CodeRef {
        CodeRef {
            repo: repo.into(),
            sha: sha.into(),
            path: path.into(),
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
            patch: true,
            result_hash: result_hash.map(str::to_string),
        }
    }

    #[test]
    fn pair_patch_honors_a_fully_covered_fence() {
        let sha = "a".repeat(40);
        let diff = "diff --git a/f.rs b/f.rs\n--- a/f.rs\n+++ b/f.rs\n@@ -1 +1 @@\n-a\n+b\n";
        let body = format!("suggestion\n\n{}", patch_fence("app", &sha, diff));
        let refs = vec![code_ref_patch("app", "f.rs", &sha, Some(&"b".repeat(40)))];
        let paired = pair_patch(&body, &refs).expect("should pair");
        assert_eq!(paired.repo, "app");
        assert_eq!(paired.files.len(), 1);
        assert_eq!(paired.files[0].result_hash.as_deref(), Some(&*"b".repeat(40)));
    }

    #[test]
    fn pair_patch_rejects_an_orphan_fence() {
        // A confer-patch fence with NO matching patch:true refs at all — a body-only forgery.
        let sha = "a".repeat(40);
        let diff = "diff --git a/f.rs b/f.rs\n--- a/f.rs\n+++ b/f.rs\n@@ -1 +1 @@\n-a\n+b\n";
        let body = patch_fence("app", &sha, diff);
        assert!(pair_patch(&body, &[]).is_none());
    }

    #[test]
    fn pair_patch_rejects_a_partially_covered_fence() {
        // The diff touches TWO files but only one has a matching patch:true ref — under-covered,
        // must not pair (a peer could otherwise smuggle an unreviewed second-file edit through).
        let sha = "a".repeat(40);
        let diff = "\
diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1 +1 @@
-a
+b
diff --git a/b.rs b/b.rs
--- a/b.rs
+++ b/b.rs
@@ -1 +1 @@
-a
+b
";
        let body = patch_fence("app", &sha, diff);
        let refs = vec![code_ref_patch("app", "a.rs", &sha, None)];
        assert!(pair_patch(&body, &refs).is_none(), "must not pair when a touched path has no ref");
    }

    #[test]
    fn pair_patch_rejects_repo_or_sha_mismatch() {
        let sha = "a".repeat(40);
        let other_sha = "b".repeat(40);
        let diff = "diff --git a/f.rs b/f.rs\n--- a/f.rs\n+++ b/f.rs\n@@ -1 +1 @@\n-a\n+b\n";
        let body = patch_fence("app", &sha, diff);
        // right path, WRONG sha on the ref.
        let refs = vec![code_ref_patch("app", "f.rs", &other_sha, None)];
        assert!(pair_patch(&body, &refs).is_none());
        // right path, right sha, but patch:false (an ordinary ref, not a patch anchor).
        let mut plain = code_ref_patch("app", "f.rs", &sha, None);
        plain.patch = false;
        assert!(pair_patch(&body, &[plain]).is_none());
    }
}
