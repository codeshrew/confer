//! `--ref`/`--patch` resolution for `append` and its lifecycle sugar verbs: parsing a
//! `--ref` token, classifying/pinning it against a local clone (branch/tag/detached,
//! symbolic-rev resolution, fork-point + base-ref capture), the write-time integrity
//! gate (dirty/untracked/remapped detection + working-tree embed fallback), and
//! `--patch` attachment (apply-gate + per-file derived refs). Pure move out of
//! `append.rs` — see CLAUDE.md's module taxonomy.

use anyhow::{anyhow, Result};
use std::io::Read;
use std::path::Path;

use crate::patch;
use crate::{crosshub, gitcmd, refcode, repomap, repos, schema, valid_slug};

/// Parse a `--ref` token `repo:path[@sha][#Lstart-Lend]` into a CodeRef.
/// sha defaults to `HEAD` ("go look at latest"); pin a sha for a durable pointer.
pub(crate) fn parse_ref(s: &str) -> Result<schema::CodeRef> {
    let bad = || anyhow!("invalid --ref '{s}': expected repo:path[@sha][#Lstart-Lend]");
    let (repo, rest) = s.split_once(':').ok_or_else(bad)?;
    let (rest, range) = match rest.split_once('#') {
        Some((r, span)) => (r, Some(parse_range(span)?)), // malformed range → error, not silent drop
        None => (rest, None),
    };
    let (path, sha) = match rest.split_once('@') {
        Some((p, sha)) => (p, sha.to_string()),
        None => (rest, "HEAD".to_string()),
    };
    if repo.is_empty() || path.is_empty() {
        return Err(bad());
    }
    // The repo token keys into the `repos/<slug>.md` inventory — hold it to the
    // slug rule; and keep control chars out of the path (SEC1).
    if !valid_slug(repo) {
        return Err(anyhow!(
            "invalid --ref repo '{repo}': must be a repos/<slug> key ([a-z0-9][a-z0-9-]*)"
        ));
    }
    if path.chars().any(|c| c.is_control()) {
        return Err(anyhow!(
            "invalid --ref path '{path}': contains control characters"
        ));
    }
    Ok(schema::CodeRef {
        repo: repo.to_string(),
        sha,
        path: path.to_string(),
        range,
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
    })
}

/// Parse `Lstart-Lend` (range) or `L46` / `46` (single line → `[n, n]`) into a line
/// range — errors (not silently drops) on a malformed or overflowing span, since the
/// ref would lose its span.
pub(crate) fn parse_range(span: &str) -> Result<[u64; 2]> {
    let bad = || anyhow!("invalid line range '{span}': expected Lstart-Lend or Lstart");
    match span.split_once('-') {
        Some((a, b)) => {
            let a = a.trim_start_matches('L').parse().map_err(|_| bad())?;
            let b = b.trim_start_matches('L').parse().map_err(|_| bad())?;
            Ok([a, b])
        }
        // A single line `#L46` — a legitimate, common reference (one line), not a
        // malformed range. Fold it to the degenerate range [n, n].
        None => {
            let n = span.trim_start_matches('L').parse().map_err(|_| bad())?;
            Ok([n, n])
        }
    }
}

/// Bound on an embedded `confer-ref` fence (design/44 §2; sized like design/40's ~150-line
/// diff-embed gate). A working-tree snapshot beyond this refuses to embed ("too large —
/// commit it") rather than bloat the permanent, fleet-wide hub log.
const EMBED_MAX_LINES: usize = 200;

/// What the caller (`cmd_append`) must fold back into the message for one pinned `--ref`
/// (design/44 §1/§2): an optional stderr-only capture-provenance line (NEVER persisted —
/// worktree paths are machine-local), an optional `confer-ref` body fence (only produced
/// under `--allow-dirty`), and non-fatal advisories.
pub(crate) struct PinOutcome {
    pub(crate) provenance: Option<String>,
    pub(crate) fence: Option<String>,
    pub(crate) warnings: Vec<String>,
}

/// How the pinned commit was reached — feeds `ref_name`/`ref_type` (design/44 §1.2). `Clone` so
/// design/45's `attach_patch` can apply ONE captured identity to several derived per-file refs.
#[derive(Clone)]
enum RefKind {
    Branch(String),
    Tag(String),
    Detached,
}

impl RefKind {
    fn apply(self, r: &mut schema::CodeRef) {
        match self {
            RefKind::Branch(n) => {
                r.ref_type = Some("branch".to_string());
                r.ref_name = Some(n);
            }
            RefKind::Tag(n) => {
                r.ref_type = Some("tag".to_string());
                r.ref_name = Some(n);
            }
            RefKind::Detached => {
                r.ref_type = Some("detached".to_string());
                r.ref_name = None;
            }
        }
    }
}

fn is_full_hex(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// A short/full hex-looking token — the boundary design/44 §1.2 classifies as `detached`
/// regardless of whether a same-named branch/tag happens to exist ("you typed a sha").
fn looks_like_hex(s: &str) -> bool {
    s.len() >= 4 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Classify an IMPLICIT `HEAD` capture (design/44 §1.2): the checked-out branch, else an
/// exact-match tag (detached-at-a-tag), else plain detached.
fn classify_implicit_head(dir: &Path) -> RefKind {
    if let Ok(o) = gitcmd::output(dir, &["symbolic-ref", "--short", "-q", "HEAD"]) {
        if o.status.success() {
            let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !name.is_empty() {
                return RefKind::Branch(name);
            }
        }
    }
    if let Ok(o) = gitcmd::output(dir, &["describe", "--tags", "--exact-match", "HEAD"]) {
        if o.status.success() {
            let name = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !name.is_empty() {
                return RefKind::Tag(name);
            }
        }
    }
    RefKind::Detached
}

/// Classify an EXPLICIT `@R` capture (design/44 §1.2): a hex-looking token → detached;
/// else `refs/heads/<R>` → branch, `refs/tags/<R>` → tag; anything else → detached.
fn classify_explicit(dir: &Path, token: &str) -> RefKind {
    if looks_like_hex(token) {
        return RefKind::Detached;
    }
    let verified = |refname: &str| {
        gitcmd::output(dir, &["show-ref", "--verify", "--quiet", refname])
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if verified(&format!("refs/heads/{token}")) {
        return RefKind::Branch(token.to_string());
    }
    if verified(&format!("refs/tags/{token}")) {
        return RefKind::Tag(token.to_string());
    }
    RefKind::Detached
}

/// One `git diff -U0` hunk header (`@@ -a[,b] +c[,d] @@`), old/new line spans.
struct Hunk {
    old_len: u64,
    new_start: u64,
    new_len: u64,
}

/// Parse `-U0` hunk headers into their old/new coordinates — the exact line mapping the
/// integrity gate's remap uses (design/44 §2). Ignores anything that doesn't parse (never
/// panics on unexpected diff output).
fn parse_hunks(diff_text: &str) -> Vec<Hunk> {
    let mut out = Vec::new();
    let parse_span = |s: &str| -> Option<(u64, u64)> {
        match s.split_once(',') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => Some((s.parse().ok()?, 1)),
        }
    };
    for line in diff_text.lines() {
        let Some(rest) = line.strip_prefix("@@ -") else { continue };
        let Some(end) = rest.find(" @@") else { continue };
        let Some((old, new)) = rest[..end].split_once(" +") else { continue };
        let (Some((_, old_len)), Some((new_start, new_len))) = (parse_span(old), parse_span(new))
        else {
            continue;
        };
        out.push(Hunk { old_len, new_start, new_len });
    }
    out
}

/// The write-time integrity gate's verdict for one ref (design/44 §2). Only called when
/// the pinned sha equals the capture dir's CURRENT HEAD commit — a deliberate historical
/// `@sha` never reaches this (§1.3).
enum GateVerdict {
    /// Nothing to remap or fail.
    Clean,
    /// Above-range edits shifted the stored range into blob coordinates.
    Remapped { range: [u64; 2], note: String },
    /// Untracked (or `.gitignore`d) — no blob exists at any sha.
    Untracked { ignored: bool },
    /// Tracked, but the referenced content itself is uncommitted (or past EOF at the pin).
    Dirty { reason: String },
}

/// Hunk-overlap (not file-level) dirty check (design/44 §2): a hunk whose NEW-side span
/// (working-tree coordinates, matching the ref's stored range) intersects `[s,e]` → the
/// referenced content itself is uncommitted; hunks entirely above shift the range by
/// `Σ(new_len − old_len)`, remapped into blob coordinates; no range → any hunk fails.
fn integrity_gate(dir: &Path, pinned_sha: &str, path: &str, range: Option<[u64; 2]>) -> Result<GateVerdict> {
    let tracked = gitcmd::output(dir, &["ls-files", "--error-unmatch", "--", path])
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !tracked {
        let ignored = gitcmd::output(dir, &["check-ignore", "-q", "--", path])
            .map(|o| o.status.success())
            .unwrap_or(false);
        return Ok(GateVerdict::Untracked { ignored });
    }
    let o = gitcmd::output(dir, &["diff", "-U0", pinned_sha, "--", path])?;
    if !o.status.success() {
        return Ok(GateVerdict::Clean); // diff couldn't run (e.g. binary) — don't block on it
    }
    let hunks = parse_hunks(&String::from_utf8_lossy(&o.stdout));
    if hunks.is_empty() {
        // Nothing uncommitted at all — working tree == the pinned blob exactly. The ONLY
        // way a range can still be invalid here is if it was never valid: pointing past
        // the file's (unchanged) length. A genuine dirty-EOF case (lines that exist in
        // the working tree but not yet in the blob) is instead an INTERSECTING hunk,
        // caught below — never reached when `hunks` is empty.
        if let Some([_, e]) = range {
            if let Some(n) = refcode::blob_line_count(dir, pinned_sha, path) {
                if e > n {
                    return Ok(GateVerdict::Dirty {
                        reason: format!("those lines aren't committed yet (the pinned commit only has {n} lines)"),
                    });
                }
            }
        }
        return Ok(GateVerdict::Clean);
    }
    let Some([s, e]) = range else {
        // Whole-file ref: any uncommitted change at all breaks "this file as pinned".
        return Ok(GateVerdict::Dirty { reason: "the file has uncommitted changes".to_string() });
    };
    let mut shift: i64 = 0;
    for h in &hunks {
        let new_end = if h.new_len == 0 { h.new_start } else { h.new_start + h.new_len - 1 };
        if h.new_len > 0 && h.new_start <= e && new_end >= s {
            return Ok(GateVerdict::Dirty {
                reason: format!("uncommitted changes overlapping L{s}-{e}"),
            });
        }
        if new_end < s {
            shift += h.new_len as i64 - h.old_len as i64;
        }
    }
    if shift != 0 {
        let ns = (s as i64 - shift).max(1) as u64;
        let ne = (e as i64 - shift).max(1) as u64;
        return Ok(GateVerdict::Remapped {
            range: [ns, ne],
            note: format!("range remapped L{s}-{e} → L{ns}-{ne}: uncommitted insertion above"),
        });
    }
    Ok(GateVerdict::Clean)
}

/// Build a `confer-ref` body fence (design/40) embedding the CURRENT working-tree content
/// of the referenced range (or whole file) — the `--allow-dirty` escape hatch (design/44
/// §2). `Err` (not embedded) when it doesn't fit the size gate or can't be read.
fn embed_fence(working_path: &Path, repo: &str, path: &str, sha: &str, range: Option<[u64; 2]>) -> Result<String, String> {
    let text = std::fs::read_to_string(working_path)
        .map_err(|e| format!("could not read {} to embed: {e}", working_path.display()))?;
    let lines: Vec<&str> = text.lines().collect();
    let (start, end, header_range) = match range {
        Some([s, e]) => (s.max(1) as usize, e as usize, format!(" range=L{s}-{e}")),
        None => (1, lines.len(), String::new()),
    };
    let snippet: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            let n = i + 1;
            n >= start && n <= end
        })
        .map(|(_, l)| *l)
        .collect();
    if snippet.len() > EMBED_MAX_LINES {
        return Err(format!(
            "{} lines is too large to embed (> {EMBED_MAX_LINES}) — commit it instead of --allow-dirty",
            snippet.len()
        ));
    }
    let mut fence = format!("```confer-ref repo={repo} path={path} sha={sha}{header_range}\n");
    for l in &snippet {
        fence.push_str(l);
        fence.push('\n');
    }
    fence.push_str("```\n");
    Ok(fence)
}

/// Pin a `--ref` to an immutable full sha AT WRITE TIME, capture its temporal identity, and
/// run the write-time integrity gate (design/44 §1–2). A code reference is either pinned to
/// a full commit sha with its identity captured, or explicitly marked `sha: "unresolved"` —
/// the literal `HEAD`, a branch name, or a short hash never lands in `sha` again.
///
/// Capture-directory precedence (§1.1, worktree-correct): `--ref-from` (repo-matching) → the
/// agent's cwd (same repo) → the mapped clone (fallback) → none. EVERY subsequent command for
/// this ref — sha, ref_name/ref_type, commit_date, content_hash, the dirty check — runs
/// against that SAME directory; never mixed. Pinning does NOT depend on hub-card registration
/// (§1.3/task-#49): the clone map alone governs local resolvability.
pub(crate) fn resolve_and_pin_ref(
    repo_inv: &repos::Repos,
    r: &mut schema::CodeRef,
    ref_from: Option<&Path>,
    allow_dirty: bool,
) -> Result<PinOutcome> {
    let raw_rev = r.sha.clone(); // as typed/defaulted ("HEAD" or an explicit token)
    let is_full_hex_sha = is_full_hex(&raw_rev);
    let hex_token = looks_like_hex(&raw_rev);
    let card_root_sha = repo_inv.get(&r.repo).and_then(|c| c.root_sha.clone());
    let capture = repomap::capture_dir(&r.repo, card_root_sha.as_deref(), ref_from);
    let mut warnings = Vec::new();

    // Shallow-clone advisory (design/44 §1.5): the capture dir's root-sha identity check
    // was SKIPPED (unverifiable, not mismatched) — accepted anyway, but say so.
    if let Some(cap) = capture.as_ref() {
        if card_root_sha.is_some() && crosshub::is_shallow(&cap.dir) {
            warnings.push(format!(
                "--ref {}:{}: '{}' is a shallow clone — its root-sha identity can't be verified against \
                 the hub card; accepted anyway (unverifiable, not mismatched).",
                r.repo, r.path, r.repo
            ));
        }
    }

    if !is_full_hex_sha {
        let Some(cap) = capture.as_ref() else {
            if allow_dirty {
                r.sha = "unresolved".to_string();
                r.rev = Some(raw_rev.clone());
                warnings.push(format!(
                    "--ref {}:{}@{}: no local clone resolvable on this machine (checked --ref-from, cwd, \
                     and the repo map) — stored as unresolved (--allow-dirty). Map a clone \
                     (`confer repos map {} <path>`) for a durable pin.",
                    r.repo, r.path, raw_rev, r.repo
                ));
                return Ok(PinOutcome { provenance: None, fence: None, warnings });
            }
            return Err(anyhow!(
                "cannot pin --ref {}:{}@{}: no local clone of '{}' is mapped on this machine (or reachable \
                 from your cwd/--ref-from), and a non-sha ref can't be made durable without one. Map a clone \
                 (`confer repos map {} <path>`), pass an explicit full commit sha (`@<40-hex>`), or use \
                 --allow-dirty to store it as unresolved.",
                r.repo, r.path, raw_rev, r.repo, r.repo
            ));
        };
        r.sha = resolve_symbolic_sha(&cap.dir, &raw_rev, &r.repo, &r.path)?;
    }

    // Identity capture (§1.2 + Addendum 2) — best-effort, only when a capture dir
    // resolved; a hex-looking token is `detached` regardless (no git call needed).
    if hex_token {
        r.ref_type = Some("detached".to_string());
        r.ref_name = None;
    }
    if let Some(cap) = capture.as_ref() {
        let id = capture_identity(&cap.dir, &r.sha, &raw_rev, hex_token);
        if let Some(kind) = id.kind {
            kind.apply(r);
        }
        r.commit_date = id.commit_date;
        r.base_ref = id.base_ref;
        r.fork_point = id.fork_point;
    }

    // Best-effort content_hash: the blob OID of `<sha>:<path>`, when a clone has the
    // object. Lets staleness be a one-line comparison later (design/40 #5) that works
    // even if the pinned commit is GC'd/unfetched — you never need the commit to ask
    // "have these bytes changed?".
    if r.content_hash.is_none() {
        if let Some(cap) = capture.as_ref() {
            r.content_hash = capture_content_hash(&cap.dir, &r.sha, &r.path);
        }
    }

    // The write-time integrity gate (§2) — only when the pinned sha IS the capture dir's
    // current HEAD commit (§1.3): a deliberate historical `@sha` is never blocked by today's
    // working tree.
    let mut fence = None;
    if let Some(cap) = capture.as_ref() {
        let outcome = run_integrity_gate(&cap.dir, r, allow_dirty)?;
        warnings.extend(outcome.warnings);
        fence = outcome.fence;
    }

    let provenance = capture.map(|cap| build_provenance(&cap, r));

    Ok(PinOutcome { provenance, fence, warnings })
}

/// Resolve a symbolic/short rev to its full commit sha in `dir` (design/44 §1.2 #1):
/// `^{commit}` peels an annotated tag → always lands on a commit sha. `Err` (unknown
/// revision) carries a shallow-clone hint when applicable.
fn resolve_symbolic_sha(dir: &Path, raw_rev: &str, repo: &str, path: &str) -> Result<String> {
    let spec = format!("{raw_rev}^{{commit}}");
    let o = gitcmd::output(dir, &["rev-parse", "--verify", "--quiet", &spec])?;
    if !o.status.success() {
        let shallow_hint = if crosshub::is_shallow(dir) {
            " (shallow clone — fetch with `--unshallow` or deepen to reach it)"
        } else {
            ""
        };
        return Err(anyhow!(
            "cannot resolve --ref {repo}:{path}@{raw_rev} in {}{shallow_hint} (unknown revision)",
            dir.display(),
        ));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Temporal identity captured for a pinned commit (design/44 §1.2 + Addendum 2), all
/// best-effort — a failed git call just leaves the corresponding field `None`, never
/// fails the ref.
struct Identity {
    kind: Option<RefKind>,
    commit_date: Option<String>,
    base_ref: Option<String>,
    fork_point: Option<String>,
}

/// Classify the pinned commit's symbolic identity (branch/tag/detached, §1.2), capture
/// its committer date (`%cI`), and — for a branch — its fork point off its base
/// (Addendum 2). `hex_token` means the caller already classified `detached` from the
/// typed token alone (no git call needed), so classification is skipped here.
fn capture_identity(dir: &Path, sha: &str, raw_rev: &str, hex_token: bool) -> Identity {
    let kind = if hex_token {
        None
    } else if raw_rev == "HEAD" {
        Some(classify_implicit_head(dir))
    } else {
        Some(classify_explicit(dir, raw_rev))
    };

    let commit_date = gitcmd::output(dir, &["log", "-1", "--format=%cI", sha])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|d| !d.is_empty());

    let (base_ref, fork_point) = match &kind {
        Some(RefKind::Branch(branch)) => resolve_fork_point(dir, sha, branch),
        _ => (None, None),
    };

    Identity { kind, commit_date, base_ref, fork_point }
}

/// design/44 Addendum 2: the branch's fork point off its base — the stable anchor that
/// survives a squash/merge (which GCs the branch's own mid-feature commits). Best-effort
/// and additive: any unresolved step (no upstream/default branch, no common ancestor,
/// no real divergence) just omits the corresponding field, never fails the ref.
fn resolve_fork_point(dir: &Path, sha: &str, branch: &str) -> (Option<String>, Option<String>) {
    let Some(base_ref) = resolve_base_ref(dir, branch) else {
        return (None, None);
    };
    let fork_point = gitcmd::output(dir, &["merge-base", sha, &base_ref])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|f| !f.is_empty() && f != sha);
    (Some(base_ref), fork_point)
}

/// The branch `branch` forked from (design/44 Addendum 2), best-effort: the configured
/// upstream, else the repo's default branch (`origin/HEAD`, falling back to `main` then
/// `master`). `None` when nothing resolves, or when the resolved base IS `branch` itself
/// (nothing to report — nothing forked from itself).
fn resolve_base_ref(dir: &Path, branch: &str) -> Option<String> {
    let upstream = gitcmd::output(dir, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| s.rsplit('/').next().unwrap_or(&s).to_string());
    let base = upstream.or_else(|| default_branch(dir))?;
    (base != branch).then_some(base)
}

/// The repo's default branch: `origin/HEAD` (stripped of the remote prefix), else the
/// first of `main`/`master` that exists as a local branch. `None` if neither resolves.
fn default_branch(dir: &Path) -> Option<String> {
    if let Ok(o) = gitcmd::output(dir, &["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"]) {
        if o.status.success() {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let name = s.rsplit('/').next().unwrap_or(&s).to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    ["main", "master"]
        .into_iter()
        .find(|cand| {
            gitcmd::output(dir, &["show-ref", "--verify", "--quiet", &format!("refs/heads/{cand}")])
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
        .map(str::to_string)
}

/// Best-effort blob OID of `<sha>:<path>` — `None` when the clone doesn't have the
/// object (never an error; content_hash is an optional staleness aid).
fn capture_content_hash(dir: &Path, sha: &str, path: &str) -> Option<String> {
    let spec = format!("{sha}:{path}");
    gitcmd::output(dir, &["rev-parse", "--verify", "--quiet", &spec])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|oid| !oid.is_empty())
}

/// What the integrity gate (§2) produced for one ref: an optional embedded fence (only
/// under `--allow-dirty`) and any non-fatal advisories (a remap note, an embed notice).
struct GateOutcome {
    fence: Option<String>,
    warnings: Vec<String>,
}

/// Run the write-time integrity gate (§2) against the capture dir `dir`, mutating `r`
/// in place per the verdict — ONLY when the pinned sha is that dir's CURRENT HEAD commit
/// (§1.3); a deliberate historical `@sha` is left untouched (returns an empty outcome).
fn run_integrity_gate(dir: &Path, r: &mut schema::CodeRef, allow_dirty: bool) -> Result<GateOutcome> {
    let head_sha = gitcmd::output(dir, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    if head_sha.as_deref() != Some(r.sha.as_str()) {
        return Ok(GateOutcome { fence: None, warnings: Vec::new() });
    }

    let mut warnings = Vec::new();
    let mut fence = None;
    match integrity_gate(dir, &r.sha, &r.path, r.range)? {
        GateVerdict::Clean => {}
        GateVerdict::Remapped { range, note } => {
            r.range = Some(range);
            warnings.push(format!("--ref {}:{}: {note}", r.repo, r.path));
        }
        GateVerdict::Untracked { ignored } => {
            if !allow_dirty {
                let what = if ignored { "is ignored by .gitignore" } else { "is untracked" };
                return Err(anyhow!(
                    "cannot --ref {}:{}: the file {what} — there is no committed content for peers \
                     to retrieve. Commit it, or pass --allow-dirty to embed the current lines instead.",
                    r.repo, r.path
                ));
            }
            let working = dir.join(&r.path);
            match embed_fence(&working, &r.repo, &r.path, "unresolved", r.range) {
                Ok(f) => {
                    r.sha = "unresolved".to_string();
                    r.content_hash = None;
                    r.untracked = true;
                    r.rev = None; // §2: rev is omitted for the untracked case
                    warnings.push(format!(
                        "--ref {}:{}: {} — embedded (--allow-dirty)",
                        r.repo,
                        r.path,
                        if ignored { "ignored by .gitignore" } else { "untracked" }
                    ));
                    fence = Some(f);
                }
                Err(e) => return Err(anyhow!("--ref {}:{}: {e}", r.repo, r.path)),
            }
        }
        GateVerdict::Dirty { reason } => {
            if !allow_dirty {
                return Err(anyhow!(
                    "cannot --ref {}:{}{}: {reason} (working tree ≠ pinned commit {}). Commit the \
                     change so peers can retrieve what you mean, or pass --allow-dirty to embed the \
                     current lines into the message instead.",
                    r.repo,
                    r.path,
                    r.range.map(|x| format!("#L{}-{}", x[0], x[1])).unwrap_or_default(),
                    &r.sha[..r.sha.len().min(9)]
                ));
            }
            let working = dir.join(&r.path);
            match embed_fence(&working, &r.repo, &r.path, &r.sha, r.range) {
                Ok(f) => {
                    r.dirty = true;
                    warnings.push(format!("--ref {}:{}: {reason} — embedded (--allow-dirty)", r.repo, r.path));
                    fence = Some(f);
                }
                Err(e) => return Err(anyhow!("--ref {}:{}: {e}", r.repo, r.path)),
            }
        }
    }
    Ok(GateOutcome { fence, warnings })
}

/// The stderr-only send-receipt line naming where/what got pinned (§1.1 — NEVER
/// persisted, worktree paths are machine-local): short sha, branch/tag + date, capture
/// source, and — for a branch with a known fork point — where it forked from
/// (design/44 Addendum 2).
fn build_provenance(cap: &repomap::Capture, r: &schema::CodeRef) -> String {
    let label = match cap.source {
        repomap::CaptureSource::RefFrom => "ref-from",
        repomap::CaptureSource::Cwd => "cwd",
        repomap::CaptureSource::Mapped => "mapped clone",
    };
    let short = &r.sha[..r.sha.len().min(9)];
    let name = r.ref_name.as_deref().unwrap_or_else(|| r.ref_type.as_deref().unwrap_or("?"));
    let date = r.commit_date.as_deref().map(|d| format!(", {}", &d[..d.len().min(10)])).unwrap_or_default();
    let fork = match (r.base_ref.as_deref(), r.fork_point.as_deref()) {
        (Some(base), Some(fp)) => format!(", forked from {base}@{}", &fp[..fp.len().min(7)]),
        (Some(base), None) => format!(", forked from {base}"),
        (None, _) => String::new(),
    };
    format!("pinned {short} ({name}{date}) from {} [{label}]{fork}", cap.dir.display())
}

/// Read a `--patch <file|->` source: `-` reads stdin (the natural channel for an agent that just
/// computed a diff), else the given file path.
pub(crate) fn read_patch_source(src: &str) -> Result<String> {
    if src == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        Ok(s)
    } else {
        std::fs::read_to_string(src).map_err(|e| anyhow!("--patch {src}: {e}"))
    }
}

/// Attach a prepared unified diff as a `confer-patch` (design/45 §1.3–1.4): resolve+pin its base
/// sha (design/44 §1.1's capture-dir precedence — reused verbatim; a patch is NEVER written with
/// `sha: unresolved`, a hard error, since a proposal against an unpinnable base is meaningless),
/// run the write-time apply-gate (a temp-index `read-tree` + `apply --cached`, `patch::
/// validate_and_derive`), and derive one `patch: true` ref per touched file straight from the
/// diff — so an honestly-authored patch always pairs with its fence (the anti-spoof rule, §1.2).
/// Returns the derived refs plus the `confer-patch` body fence to fold into the message (which
/// also puts the diff through the same secret/control-char lints as any other body content).
pub(crate) fn attach_patch(
    repo_inv: &repos::Repos,
    repo: &str,
    diff: &str,
    ref_from: Option<&Path>,
    allow_large: bool,
) -> Result<(Vec<schema::CodeRef>, String)> {
    // design/45 review, P1: binary refusal + the hard byte ceiling + the line-count size gate,
    // ALL through the one `validate_patch` chokepoint `confer apply` also uses — the diff below
    // this point is always a `ValidatedPatch`, so it structurally cannot skip these gates.
    let (validated, warning) = patch::validate_patch(diff, allow_large)?;
    if let Some(warning) = warning {
        crate::hint(warning);
    }
    let card_root_sha = repo_inv.get(repo).and_then(|c| c.root_sha.clone());
    let capture = repomap::capture_dir(repo, card_root_sha.as_deref(), ref_from).ok_or_else(|| {
        anyhow!(
            "cannot pin --patch: no local clone of '{repo}' is mapped on this machine (or reachable \
             from your cwd/--ref-from) — a patch needs a real base to apply against. Map one: \
             `confer repos map {repo} <path>`."
        )
    })?;
    let dir = &capture.dir;
    let head = gitcmd::output(dir, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])?;
    if !head.status.success() {
        return Err(anyhow!("cannot pin --patch: '{repo}' has no commits at HEAD in {}", dir.display()));
    }
    let base_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();

    let hashes = patch::validate_and_derive(dir, &base_sha, &validated)?;
    let touched = patch::parse_diff_touched_files(diff);
    if touched.is_empty() {
        return Err(anyhow!(
            "--patch: no files found in the diff (expected `diff --git a/… b/…` / `--- `/`+++ ` headers)"
        ));
    }

    let identity = capture_identity(dir, &base_sha, "HEAD", false);
    let mut refs = Vec::with_capacity(touched.len());
    for t in &touched {
        let mut r = schema::CodeRef {
            repo: repo.to_string(),
            sha: base_sha.clone(),
            path: t.path.clone(),
            range: t.old_range,
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
            result_hash: hashes.get(&t.path).cloned(),
        };
        if let Some(kind) = identity.kind.clone() {
            kind.apply(&mut r);
        }
        r.commit_date = identity.commit_date.clone();
        r.base_ref = identity.base_ref.clone();
        r.fork_point = identity.fork_point.clone();
        refs.push(r);
    }
    crate::hint(format!(
        "--patch: {} file(s) against {repo}@{}",
        refs.len(),
        &base_sha[..base_sha.len().min(9)]
    ));
    Ok((refs, patch::patch_fence(repo, &base_sha, diff)))
}
