//! `append` and its lifecycle sugar verbs (`claim`/`done`/`error`/`blocked`/`defer`):
//! arg parsing, ref/range parsing, addressing/recipient advisories, and the send path.

use anyhow::{anyhow, Result};
use std::io::{IsTerminal, Read};
use std::path::Path;

use crate::patch;
use crate::projection::claimants;
use crate::schema::{self, Frontmatter, Message, TYPES};
use crate::{
    config, crosshub, gitcmd, groups, hint, id_matches, is_full_ulid, is_reserved_name, now,
    refcode, repomap, repos, resolve_unique, roster, secrets, short_id, store, truncate,
    valid_slug, warn_if_watch_should_be_live, CreateArgs, LifecycleArgs,
};

pub(crate) struct AppendArgs {
    pub(crate) msg_type: String,
    pub(crate) text: Option<String>,
    pub(crate) summary: String,
    pub(crate) to: Vec<String>,
    pub(crate) cc: Vec<String>,
    pub(crate) priority: Option<String>,
    pub(crate) topic: Option<String>,
    pub(crate) reply_to: Option<String>,
    pub(crate) of: Option<String>,
    pub(crate) supersedes: Option<String>,
    pub(crate) from: Option<String>,
    pub(crate) src: Option<String>,
    pub(crate) refs: Vec<String>,
    pub(crate) allow_empty_body: bool,
    pub(crate) resolution: Option<String>,
    pub(crate) defer: bool,
    /// override the secret-shape lint (post even if the body looks like it has a key).
    pub(crate) allow_secret: bool,
    /// design/44 §1.1: capture EVERY `--ref`'s identity from this dir instead of the
    /// mapped clone (message-wide; only applies to refs whose repo identity matches it).
    pub(crate) ref_from: Option<String>,
    /// design/44 §2: downgrade the write-time integrity gate from a hard FAIL to a
    /// warning + auto-embed of the working-tree content actually referenced.
    pub(crate) allow_dirty: bool,
    /// design/45 §1.3: attach a prepared unified diff (file path, or `-` for stdin) as a
    /// `confer-patch` — requires `patch_repo`.
    pub(crate) patch: Option<String>,
    /// the `repos/<slug>` `patch` is against (design/45 §1.3).
    pub(crate) patch_repo: Option<String>,
    /// raise `patch`'s size gate to the hard cap (design/45 §1.2).
    pub(crate) allow_large_patch: bool,
}

/// Parse a `--ref` token `repo:path[@sha][#Lstart-Lend]` into a CodeRef.
/// sha defaults to `HEAD` ("go look at latest"); pin a sha for a durable pointer.
fn parse_ref(s: &str) -> Result<schema::CodeRef> {
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
fn resolve_and_pin_ref(
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
fn read_patch_source(src: &str) -> Result<String> {
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
fn attach_patch(
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
        hint(warning);
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
    hint(format!(
        "--patch: {} file(s) against {repo}@{}",
        refs.len(),
        &base_sha[..base_sha.len().min(9)]
    ));
    Ok((refs, patch::patch_fence(repo, &base_sha, diff)))
}

/// Warn (non-fatally) when a message's addressees can't receive it in THIS hub:
/// a named `--to`/`--cc` role that hasn't joined, or a broadcast/group that
/// resolves to no one but the sender. This is the guardrail for the split-brain
/// footgun — an agent posting into the wrong repo/hub (e.g. the product repo
/// instead of the coordination hub), where its intended peers aren't present, so
/// the message is silently stranded. Deliberately a **warning**, not an error:
/// a role may legitimately join later, and leaving a note for an arriving agent
/// is a valid use — but the far more common cause is being in the wrong hub, and
/// naming the hub + who's actually joined makes that obvious. See DESIGN.md.
fn recipient_advisory(
    root: &std::path::Path,
    roster: &roster::Roster,
    grps: &groups::Groups,
    from: &str,
    to: &[String],
    cc: &[String],
    summary: &str,
) {
    // Nothing addressed → a topic-only post; there's no delivery claim to check.
    if to.is_empty() && cc.is_empty() {
        return;
    }
    let hub = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("this hub");
    let mut known: Vec<&str> = roster.keys().map(String::as_str).collect();
    known.sort_unstable();
    // Reachable peers = every joined role other than the sender.
    let has_other_peer = known.iter().any(|r| *r != from);

    let mut unknown: Vec<&str> = Vec::new(); // named roles that haven't joined
    let mut broadcast_empty = false; // `all`/group that reaches no one but you
    for t in to.iter().chain(cc.iter()) {
        if t == from {
            continue; // self-addressing is odd but not a delivery failure
        }
        if is_reserved_name(t) {
            // `all` — reaches every other joined role.
            broadcast_empty |= !has_other_peer;
        } else if let Some(members) = grps.get(t) {
            // a group — reachable if any member (other than you) has joined.
            broadcast_empty |= !members.iter().any(|m| m != from && roster.contains_key(m));
        } else if !roster.contains_key(t) {
            unknown.push(t);
        }
    }
    unknown.sort_unstable();
    unknown.dedup();
    if unknown.is_empty() && !broadcast_empty {
        return;
    }

    if !unknown.is_empty() {
        let joined = if known.is_empty() {
            "(none yet)".to_string()
        } else {
            known.join(", ")
        };
        let names = unknown
            .iter()
            .map(|r| format!("'{r}'"))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "confer: warning — {} {names} {} not joined hub '{hub}'; they won't see this until they join. Joined roles: {joined}. If you expected them here, you may be in the wrong hub.",
            if unknown.len() == 1 { "role" } else { "roles" },
            if unknown.len() == 1 { "has" } else { "have" },
        );
    }
    if broadcast_empty {
        let s = truncate(summary, 60);
        eprintln!(
            "confer: warning — you are the only role in hub '{hub}'; no other agent will receive \"{s}\" until they join."
        );
    }
}

/// Ergonomic first-class lifecycle verbs (`confer claim/done/error/blocked/defer
/// --of <id>`) — thin sugar over `append` with the type set and a sensible default
/// summary, so closing/reclassifying a request is one short command.
pub(crate) fn cmd_lifecycle(
    msg_type: &str,
    a: LifecycleArgs,
    resolution: Option<String>,
) -> Result<()> {
    let default_summary = match (msg_type, resolution.as_deref()) {
        ("done", Some(r)) => r.to_string(),
        ("done", None) => "done".to_string(),
        ("claim", _) => "claiming".to_string(),
        ("error", _) => "failed".to_string(),
        ("blocked", _) => "blocked/waiting".to_string(),
        ("defer", _) => "deferred to backlog".to_string(),
        _ => msg_type.to_string(),
    };
    cmd_append(AppendArgs {
        msg_type: msg_type.to_string(),
        text: a.text, // optional body; summary-only still allowed (allow_empty_body)
        summary: a.summary.unwrap_or(default_summary),
        // Addressing passes straight through to append. Empty --to/--cc leaves
        // cmd_append to auto-address the request's author (via --of); an explicit
        // --to or --reply-to overrides that (append resolves the precedence).
        to: a.to,
        cc: a.cc,
        priority: None,
        topic: None,
        reply_to: a.reply_to,
        of: Some(a.of),
        supersedes: None,
        from: a.from,
        src: None,
        refs: a.refs, // the sugar verbs now carry --ref through to append (field report)
        allow_empty_body: true, // lifecycle markers are summary-only
        resolution,
        defer: false,
        allow_secret: false,
        ref_from: a.ref_from,
        allow_dirty: a.allow_dirty,
        patch: None,
        patch_repo: None,
        allow_large_patch: false,
    })
}

/// Ergonomic first-class creation verbs (`confer request`/`note`) — thin sugar over
/// `append` with the type fixed, so opening a ticket or posting a chat message
/// doesn't require spelling out `--type`. note = chat, request = ticket; a request
/// may `--reply-to` a prior note to promote it into tracked work (the escalation
/// idiom) — `note` itself has no `reply_to` param since only `request` exposes it.
pub(crate) fn cmd_create(msg_type: &str, a: CreateArgs, reply_to: Option<String>) -> Result<()> {
    cmd_append(AppendArgs {
        msg_type: msg_type.to_string(),
        text: a.text,
        summary: a.summary,
        to: a.to,
        cc: a.cc,
        priority: a.priority,
        topic: a.topic,
        reply_to,
        of: None,
        supersedes: None,
        from: a.from,
        src: a.src,
        refs: a.refs,
        allow_empty_body: a.allow_empty_body,
        resolution: None,
        defer: a.defer,
        allow_secret: a.allow_secret,
        ref_from: a.ref_from,
        allow_dirty: a.allow_dirty,
        patch: a.patch,
        patch_repo: a.patch_repo,
        allow_large_patch: a.allow_large_patch,
    })
}

/// `confer suggest` — sugar for `append --type request --patch …` (design/45 §1.3): a
/// suggestion aimed at someone is a proposable change WITH a resolution — design/39's Track
/// side — so it gets the full request lifecycle (claim/done/wont-do/supersede), `--to` required
/// exactly like any request. Requires `--patch`; the `--worktree` capture flow (diffing an
/// agent's own dirty tree instead of a prepared file) is design/45's M-phase, not implemented
/// here — an FYI alternative with no expectation of action is the Talk-side `note --patch`.
pub(crate) fn cmd_suggest(a: CreateArgs) -> Result<()> {
    if a.patch.is_none() {
        return Err(anyhow!(
            "confer suggest requires --patch <file|-> (the --worktree capture flow isn't implemented yet)"
        ));
    }
    cmd_create("request", a, None)
}

/// Split comma-lists inside repeated `--to`/`--cc` values (`--to a,b` == `--to a --to b`), trimming
/// and dropping empties — so a fleet can address a subset of peers in one flag instead of hitting the
/// slug regex on `a,b,c` (field report). Groups/`all` still work; this just pre-flattens.
fn split_comma_targets(v: Vec<String>) -> Vec<String> {
    v.into_iter()
        .flat_map(|s| s.split(',').map(str::trim).map(str::to_string).collect::<Vec<_>>())
        .filter(|s| !s.is_empty())
        .collect()
}

pub(crate) fn cmd_append(mut a: AppendArgs) -> Result<()> {
    // Accept `--to a,b,c` (and `--cc`) as a convenience for addressing several peers at once.
    a.to = split_comma_targets(a.to);
    a.cc = split_comma_targets(a.cc);
    let root = config::repo_root()?;
    let role = config::resolve_role(a.from, &root)?;
    // Surface a silently-dead watch on the next active command: if you armed a watch but it isn't
    // running (backgrounded/reaped), you're not being woken — say so now rather than let you go dark.
    warn_if_watch_should_be_live(&root, &role);

    if !TYPES.contains(&a.msg_type.as_str()) {
        return Err(anyhow!(
            "unknown --type '{}': expected one of {:?}",
            a.msg_type,
            TYPES
        ));
    }
    if let Some(p) = &a.priority {
        if !matches!(p.as_str(), "low" | "normal" | "high") {
            return Err(anyhow!(
                "invalid --priority '{p}': expected low | normal | high"
            ));
        }
    }
    let mut refs = a
        .refs
        .iter()
        .map(|s| parse_ref(s))
        .collect::<Result<Vec<_>>>()?;
    // Pin each --ref to an immutable full sha AT WRITE TIME, capture its temporal identity,
    // and run the write-time integrity gate (design/44 §1–2; design/40 #2, #3) — a durable
    // reference never stores a moving HEAD/branch. `--ref-from` is message-wide: the same
    // escape-hatch dir applies to every ref whose repo identity matches it (§1.1).
    let ref_from = a.ref_from.as_deref().map(Path::new);
    let mut ref_fences: Vec<String> = Vec::new();
    let mut ref_provenance: Vec<String> = Vec::new();
    if !refs.is_empty() {
        let repo_inv = repos::load(&root);
        for r in refs.iter_mut() {
            let outcome = resolve_and_pin_ref(&repo_inv, r, ref_from, a.allow_dirty)?;
            for w in outcome.warnings {
                hint(w);
            }
            ref_provenance.extend(outcome.provenance);
            ref_fences.extend(outcome.fence);
        }
    }
    // design/45 §1.3: attach a prepared unified diff as a `confer-patch` — reads its stdin (if
    // `-`) BEFORE the body's own stdin fallback below, resolves+pins a real base, runs the
    // write-time apply-gate, and derives one `patch: true` ref per touched file. The fence is
    // folded into `ref_fences` (below) exactly like a `confer-ref` embed, so it rides the same
    // non-empty-body / secret-shape / control-char lints as any other body content.
    if let Some(patch_src) = &a.patch {
        let repo = a.patch_repo.clone().ok_or_else(|| {
            anyhow!("--patch requires --repo <slug> (which repo the diff is against)")
        })?;
        if !valid_slug(&repo) {
            return Err(anyhow!(
                "invalid --repo '{repo}': must be a repos/<slug> key ([a-z0-9][a-z0-9-]*)"
            ));
        }
        let diff = read_patch_source(patch_src)?;
        if diff.trim().is_empty() {
            return Err(anyhow!("--patch {patch_src}: empty diff"));
        }
        let repo_inv = repos::load(&root);
        let (mut derived, fence) = attach_patch(&repo_inv, &repo, &diff, ref_from, a.allow_large_patch)?;
        refs.append(&mut derived);
        ref_fences.push(fence);
    }
    // A blank value counts as absent (an empty `--of`/`--supersedes` must not slip
    // past the required-field guard — see C1).
    let blank = |o: &Option<String>| o.as_deref().is_none_or(|s| s.trim().is_empty());
    // Imperative frontmatter contract: guarantee routing/triage metadata.
    if a.msg_type == "request" && a.to.is_empty() {
        return Err(anyhow!("--to <target> is required for type 'request'"));
    }
    if matches!(
        a.msg_type.as_str(),
        "claim" | "done" | "error" | "blocked" | "defer"
    ) && blank(&a.of)
    {
        return Err(anyhow!(
            "--of <request-id> is required for type '{}'",
            a.msg_type
        ));
    }
    if a.msg_type == "supersede" && blank(&a.supersedes) {
        return Err(anyhow!(
            "--supersedes <id> is required for type 'supersede'"
        ));
    }
    if a.summary.trim().is_empty() {
        return Err(anyhow!(
            "--summary must not be empty (it's the triage line peers read)"
        ));
    }
    // Resolution — only on a terminal `done`; validate the small vocab.
    // `done` is the default and stores nothing; the others record *why* it closed.
    let resolution = match a
        .resolution
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => None,
        Some(_) if a.msg_type != "done" => {
            return Err(anyhow!("--as <resolution> is only valid on --type done"));
        }
        Some("done") => None,
        Some(r @ ("wont-do" | "dropped" | "duplicate" | "obsolete")) => Some(r.to_string()),
        Some(other) => {
            return Err(anyhow!(
                "invalid --as '{other}': expected wont-do | duplicate | obsolete"
            ));
        }
    };
    if a.defer && a.msg_type != "request" {
        return Err(anyhow!(
            "--defer is only valid on --type request (it's a backlog marker)"
        ));
    }

    let topic = a.topic.unwrap_or_else(|| "general".to_string());

    // Slug validation (H2 — prevent path traversal / broken filenames).
    for (label, s) in [("role", role.as_str()), ("topic", topic.as_str())] {
        if !valid_slug(s) {
            return Err(anyhow!(
                "invalid {label} '{s}': must match [a-z0-9][a-z0-9-]* (≤64 chars)"
            ));
        }
        if is_reserved_name(s) {
            return Err(anyhow!(
                "'{s}' is reserved (the broadcast target) and can't be a {label}"
            ));
        }
    }
    for r in a.to.iter().chain(a.cc.iter()) {
        if !valid_slug(r) {
            return Err(anyhow!("invalid role '{r}': must match [a-z0-9][a-z0-9-]*"));
        }
    }

    // Resolve id references (--of/--supersedes/--reply-to) to canonical full ids
    // so lifecycle folding is exact. A blank value is treated as absent (guards
    // the empty-`of` whole-board fold); a fragment that matches no local message
    // fails loudly unless it is already a full ULID — never persist an ambiguous
    // fragment, which would fold by prefix onto sibling ids forever (C2).
    let all = store::all_messages(&root)?;
    let resolve = |label: &str, v: &Option<String>| -> Result<Option<String>> {
        let Some(raw) = v.as_ref() else {
            return Ok(None);
        };
        let s = raw.trim();
        if s.is_empty() {
            return Ok(None);
        }
        match resolve_unique(&all, s) {
            Ok(id) => Ok(Some(id.to_string())),
            Err(_) if is_full_ulid(s) => Ok(Some(s.to_string())), // canonical, just not fetched yet
            Err(_) if all.iter().any(|m| id_matches(&m.front.id, s)) => {
                Err(anyhow!("--{label} '{s}' is ambiguous; use the full id"))
            }
            Err(_) => Err(anyhow!(
                "--{label} '{s}' matches no known message; fetch it first or pass the full id"
            )),
        }
    };
    let of = resolve("of", &a.of)?;
    let supersedes = resolve("supersedes", &a.supersedes)?;
    let reply_to = resolve("reply-to", &a.reply_to)?;
    let mut to = a.to;
    if to.is_empty() && !matches!(a.msg_type.as_str(), "request") {
        if let Some(of_id) = &of {
            if let Some(req) = all.iter().find(|m| &m.front.id == of_id) {
                to = vec![req.front.from.clone()];
                // #5b (field report): closing a `--to all` request auto-addresses ONLY the author, so
                // the peers who actually responded to the broadcast don't get the resolution. Nudge
                // toward re-broadcasting when the request was a broadcast.
                if matches!(a.msg_type.as_str(), "done" | "error" | "blocked" | "defer")
                    && req.front.to.iter().any(|t| t == "all")
                {
                    hint(format!(
                        "this closes a `--to all` request — it reaches only the author ({}). Add `--to all` (or `--cc` the responders) if the peers who replied should hear it.",
                        req.front.from
                    ));
                }
            }
        }
    }
    // A reply with no explicit audience auto-addresses the author you're replying to
    // — so replying doesn't require `--cc all` (which wakes uninvolved roles). Peers
    // can still add more `--to`; this just makes the sane thing the default.
    if to.is_empty() && a.cc.is_empty() {
        if let Some(rt) = &reply_to {
            if let Some(orig) = all.iter().find(|m| &m.front.id == rt) {
                if orig.front.from != role {
                    // Replying to a peer → address that peer.
                    to = vec![orig.front.from.clone()];
                } else {
                    // Replying to YOUR OWN message in a thread → continue it to whoever THAT message
                    // addressed (minus yourself/`all`), so the reply doesn't go out unaddressed and
                    // wake nobody. (Field bug: a `--reply-to` pointing at your own thread post
                    // resolved to no audience, so the message never woke the participant.)
                    to = orig
                        .front
                        .to
                        .iter()
                        .filter(|t| t.as_str() != role && !is_reserved_name(t))
                        .cloned()
                        .collect();
                }
            }
        }
    }
    // Surface the silent "wakes nobody" case: a REPLY (`--reply-to`/`--of`) or a REQUEST that still
    // has NO audience reaches no inbox and wakes no peer — the exact trap where an addressing intent
    // resolved to no one. (A plain `note` with no `--to` is a deliberate board post; left alone.)
    if to.is_empty()
        && a.cc.is_empty()
        && (reply_to.is_some() || of.is_some() || a.msg_type == "request")
    {
        eprintln!(
            "confer: ⚠ this {} is addressed to NO ONE — it lands on the board but reaches no inbox \
             and wakes no peer. Add `--to <role>` (or `--to all`) so it's actually delivered.",
            if a.msg_type == "request" { "request" } else { "reply" }
        );
    }

    // Recipient-reachability advisory (guardrail against split-brain / wrong-hub
    // posting): warn if this targets a role that hasn't joined THIS hub, or `all`
    // resolves to just yourself. See DESIGN.md.
    let grps = groups::load(&root);
    recipient_advisory(
        &root,
        &roster::load(&root),
        &grps,
        &role,
        &to,
        &a.cc,
        &a.summary,
    );

    // Reference advisory (point-vs-carry): if a --ref points at a repo the
    // audience can't reach, they can't follow the pointer — nudge to inline the
    // content. Non-fatal; see DESIGN.md.
    if !refs.is_empty() {
        let inv = repos::load(&root);
        let audience: Vec<&str> = to.iter().chain(a.cc.iter()).map(String::as_str).collect();
        for r in &refs {
            match inv.get(&r.repo) {
                None => hint(format!(
                    "repo '{}' isn't registered; add repos/{}.md so peers know its role/access (confer repos).",
                    r.repo, r.repo
                )),
                Some(card) if !card.access.is_empty() => {
                    let to_all = audience.contains(&"all");
                    let blocked: Vec<&str> = audience
                        .iter()
                        .copied()
                        .filter(|t| *t != "all" && !grps.contains_key(*t) && !repos::accessible_to(card, t))
                        .collect();
                    if to_all || !blocked.is_empty() {
                        let who = if to_all {
                            "some recipients (you targeted `all`)".to_string()
                        } else {
                            blocked.join(", ")
                        };
                        hint(format!(
                            "repo '{}' isn't accessible to {who}; they can't follow this pointer. Consider inlining the key content (condensed) so the message is self-contained.",
                            r.repo
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    // Body: --text, else stdin (multi-line / fenced Markdown). A literal
    // `--text -` means "read stdin" (Unix convention) — not the body text "-";
    // taking it literally silently wrote a bare "-" body and dropped real detail.
    let mut body = match a.text {
        Some(t) if t == "-" => String::new(),
        Some(t) => t,
        None => String::new(),
    };
    if body.is_empty() && !std::io::stdin().is_terminal() {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        body = s.trim_end().to_string();
    }
    // `--allow-dirty` embeds: fold the working-tree `confer-ref` fence(s) into the body BEFORE
    // the empty-body/secret/control-char lints below, so an otherwise summary-only message that
    // embeds dirty code isn't rejected as empty, and the embedded content is screened too.
    for fence in &ref_fences {
        if !body.trim().is_empty() {
            body.push_str("\n\n");
        }
        body.push_str(fence);
    }
    // Fail loud on an empty / lone-sentinel body — the silent `-`/empty-body data
    // loss the fleet hit (a review finding). A genuine
    // summary-only note must opt in with --allow-empty-body — EXCEPT lifecycle
    // markers (claim/done/error/supersede), where the summary IS the payload, so
    // requiring a body just discourages closing requests.
    let lifecycle = matches!(
        a.msg_type.as_str(),
        "claim" | "done" | "error" | "supersede" | "blocked" | "defer"
    );
    if !a.allow_empty_body && !lifecycle && matches!(body.trim(), "" | "-" | ".") {
        return Err(anyhow!(
            "refusing to send an empty message body (got {:?}) — pass --text \"…\" or pipe stdin; \
             use --allow-empty-body for an intentional summary-only note",
            body.trim()
        ));
    }

    // Secret-shape lint (a review finding): the log is permanent + fleet-wide, so a pasted
    // token/key would leak forever. Block on a match unless explicitly overridden.
    if !a.allow_secret {
        let findings = secrets::scan(&format!("{}\n{body}", a.summary));
        if !findings.is_empty() {
            return Err(anyhow!(
                "refusing to send — the message looks like it contains a secret: {}. \
                 The hub history is permanent and cloned by every agent. Remove it, or pass \
                 --allow-secret if this is a false positive.",
                secrets::summarize(&findings)
            ));
        }
    }

    // Terminal-control lint (Fable review): a body/summary with raw ANSI/C0 escapes can
    // rewrite a reading agent's terminal, forge a fake envelope, or hide text. Render is
    // sanitized defensively (schema::sanitize_term), but block it at the source too so a
    // fleet message never carries them. `\n`/`\t` are fine in a body; the summary is a
    // one-liner so no control chars at all.
    let ctrl_body = body
        .chars()
        .find(|&c| c != '\n' && c != '\t' && c.is_control());
    if let Some(c) = ctrl_body {
        return Err(anyhow!(
            "refusing to send — the body contains a control character (U+{:04X}). \
             Strip terminal escape/control sequences; only newlines and tabs are allowed.",
            c as u32
        ));
    }
    if let Some(c) = a.summary.chars().find(|c| c.is_control()) {
        return Err(anyhow!(
            "refusing to send — the --summary contains a control character (U+{:04X}); \
             it must be a single clean line.",
            c as u32
        ));
    }

    let id = ulid::Ulid::new().to_string();
    let ts = now();
    let msg = Message {
        front: Frontmatter {
            id: id.clone(),
            from: role.clone(),
            msg_type: a.msg_type,
            ts: ts.clone(),
            host: config::hostname(),
            to,
            cc: a.cc,
            priority: a.priority,
            topic: Some(topic.clone()),
            reply_to,
            of,
            supersedes,
            resolution,
            defer: a.defer,
            via: None,
            src: a.src,
            summary: Some(a.summary),
            refs,
        },
        body,
    };

    let path = store::message_path(&root, &topic, &id, &role, &ts);
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(&path, msg.to_markdown()?)?;

    // Send receipt (stderr) so the sender SEES the body size immediately — a
    // 0-char body is now impossible, but the receipt makes content visible and
    // pairs with the drift/version checks.
    let synced = match gitcmd::commit_and_sync(
        &root,
        &role,
        &path,
        &format!("{role}: {} {}", msg.front.msg_type, id),
        config::signing_key(&root).is_some(),
    ) {
        // Pushed — nudge co-resident watchers instantly (they notify-watch this).
        Ok(gitcmd::Committed::Synced) => {
            config::touch_signal(&config::hub_key(&root));
            true
        }
        // Committed locally, push deferred — the message is SAFE and flushes on next sync.
        Ok(gitcmd::Committed::DeferredLocal) => {
            eprintln!(
                "confer: committed locally, hub push deferred; flushes on the next confer command"
            );
            false
        }
        // NOT committed (e.g. the clone was busy). Remove the orphaned working-tree file and
        // FAIL LOUDLY — never report "sent" for a message that didn't land (a review finding: a
        // backgrounded append must exit non-zero so the caller knows it did not go out).
        Err(e) => {
            let _ = std::fs::remove_file(&path);
            return Err(anyhow!(
                "did NOT send {} — not committed ({e}); the clone may be busy. Retry, e.g. `timeout 60 confer append …`.",
                short_id(&id)
            ));
        }
    };
    eprintln!(
        "confer: sent {} ({} type, summary {} chars, body {} chars){}",
        short_id(&id),
        msg.front.msg_type,
        msg.front.summary.as_deref().unwrap_or("").chars().count(),
        msg.body.chars().count(),
        if synced {
            ""
        } else {
            " [NOT synced — committed locally]"
        }
    );
    // Capture provenance (design/44 §1.1) — which dir each --ref's identity came from.
    // NEVER persisted (worktree paths are machine-local); this stderr line is the only record.
    for p in &ref_provenance {
        eprintln!("confer: {p}");
    }

    // Claim-race check: on a broadcast request two agents can both
    // claim. Resolution is by fold order — the earliest claim owns. After sync
    // (which pulls in any racing claim), warn the loser so they yield instead of
    // doing duplicate work, rather than both silently proceeding.
    if msg.front.msg_type == "claim" {
        if let Some(req) = &msg.front.of {
            if let Ok(after) = store::all_messages(&root) {
                let cs = claimants(&after, req);
                if cs.len() > 1 && cs.first().map(String::as_str) != Some(role.as_str()) {
                    eprintln!(
                        "confer: ⚠ contested claim — '{}' already claimed {} (owns by fold order). \
                         Yield (append a note and stand down) or coordinate to avoid duplicate work.",
                        cs[0],
                        short_id(req)
                    );
                }
            }
        }
    }
    println!("{id}"); // machine-readable id on stdout regardless of sync outcome
    if !synced {
        // Non-zero exit so a hook/loop can distinguish committed-locally from
        // reached-the-hub (audit S2) — the id above still identifies the message.
        return Err(anyhow!(
            "message {} committed locally but not synced to the hub",
            short_id(&id)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comma_targets_split_trim_and_drop_empties() {
        // `--to a,b,c` == `--to a --to b --to c`; trims whitespace and drops empties (field report).
        assert_eq!(split_comma_targets(vec!["a,b,c".into()]), vec!["a", "b", "c"]);
        assert_eq!(split_comma_targets(vec!["a".into(), "b, c".into()]), vec!["a", "b", "c"]);
        assert_eq!(split_comma_targets(vec!["a,,".into(), "".into()]), vec!["a"]);
        assert!(split_comma_targets(vec![]).is_empty());
        // a plain single target is unchanged
        assert_eq!(split_comma_targets(vec!["all".into()]), vec!["all"]);
    }

    #[test]
    fn parse_ref_handles_repo_path_sha_and_range() {
        let r = parse_ref("proj:docs/spec.md@6c513dca").unwrap();
        assert_eq!(r.repo, "proj");
        assert_eq!(r.path, "docs/spec.md");
        assert_eq!(r.sha, "6c513dca");
        assert_eq!(r.range, None);
        // sha defaults to HEAD when omitted
        let d = parse_ref("proj:docs/spec.md").unwrap();
        assert_eq!(d.sha, "HEAD");
        // line range, with and without the L prefix
        let ranged = parse_ref("app:src/main.rs@abc#L10-L42").unwrap();
        assert_eq!(ranged.path, "src/main.rs");
        assert_eq!(ranged.sha, "abc");
        assert_eq!(ranged.range, Some([10, 42]));
        // single-line ref (#L46) → degenerate range [46, 46]
        let one = parse_ref("app:src/main.rs@abc#L46").unwrap();
        assert_eq!(one.range, Some([46, 46]));
        // malformed → error, not panic
        assert!(parse_ref("no-colon").is_err());
        assert!(parse_ref("repo:").is_err());
        assert!(parse_ref(":path").is_err());
    }

    #[test]
    fn parse_range_errors_on_malformed() {
        assert_eq!(parse_range("10-42").unwrap(), [10, 42]);
        assert_eq!(parse_range("L10-L42").unwrap(), [10, 42]);
        // single line (#L46 / #46) → the degenerate range [n, n], not an error
        assert_eq!(parse_range("46").unwrap(), [46, 46]);
        assert_eq!(parse_range("L46").unwrap(), [46, 46]);
        assert!(parse_range("L10-Lx").is_err()); // nonnumeric
        assert!(parse_range("Lx").is_err()); // nonnumeric single
        assert!(parse_range("").is_err()); // empty
        assert!(parse_range("99999999999999999999-2").is_err()); // overflow
    }
}
