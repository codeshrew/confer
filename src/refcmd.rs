//! `confer repos map/discover` and `confer refs`/`ref-contains` command handlers
//! (design/40): the machine-local repo clone map and the code-reference reverse
//! index. Pure command handlers moved out of `main.rs` — see CLAUDE.md's module
//! taxonomy.

use crate::{
    append_ref, config, crosshub, gitcmd, projection, refcode, repomap, repos, reposdiscover,
    schema, store,
};
use crate::{short_id, valid_slug, PredicateFalse};
use anyhow::{anyhow, Result};

/// `confer repos map <slug> [path]` — record this machine's clone of a repo (design/40
/// layer 2). Local-only (`~/.confer/repos.json`), never in the hub. Warns if the slug
/// isn't in the hub's repos registry (peers can't resolve `--ref <slug>:…` until it is).
pub(crate) fn cmd_repos_map(slug: String, path: Option<String>) -> Result<()> {
    if !valid_slug(&slug) {
        return Err(anyhow!(
            "invalid repo slug '{slug}': must match a repos/<slug> key ([a-z0-9][a-z0-9-]*)"
        ));
    }
    let dir = match path {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_dir()?,
    };
    let abs = repomap::set(&slug, &dir)?;
    println!("mapped {slug} → {}", abs.display());
    if let Some(rsha) = crosshub::root_sha(&abs) {
        println!("  root-sha {} (identity anchor)", &rsha[..rsha.len().min(12)]);
    }
    // Layer-1 check: without a hub card, the slug is private to this machine — peers
    // can't resolve it. Surface that as a diagnostic (stderr), not an error.
    let known = config::repo_root().ok().map(|r| repos::load(&r).contains_key(&slug)).unwrap_or(false);
    if !known {
        eprintln!(
            "note: '{slug}' isn't in this hub's repos/ registry — peers can't resolve `--ref {slug}:…` \
             until it's shared (add repos/{slug}.md with its url + root_sha)."
        );
    }
    Ok(())
}

/// `confer repos discover [--root <dir>]…` — local-only backfill: match every repo
/// registered in a hub you follow to a git clone already on this machine, and record it
/// (`repomap::set`), so a fresh machine (or one that never ran `repos map`) doesn't need
/// each slug typed in by hand. Never touches a hub card, never commits. A REPORT (exit 0)
/// — even an all-unmatched run is not an error. See `reposdiscover.rs`.
pub(crate) fn cmd_repos_discover(roots: Vec<String>) -> Result<()> {
    let roots: Vec<std::path::PathBuf> = roots.into_iter().map(std::path::PathBuf::from).collect();
    let report = reposdiscover::run(&roots)?;
    for (slug, path) in &report.mapped {
        println!("mapped {slug} → {}", path.display());
    }
    for (slug, url) in &report.unmatched {
        let url = url.as_deref().unwrap_or("(no url)");
        println!("unmatched {slug} ({url}) — no local clone found");
    }
    if report.mapped.is_empty() && report.unmatched.is_empty() {
        println!("no repos registered across the hubs you follow — nothing to discover.");
    }
    Ok(())
}

/// Parse a reverse-lookup target `repo[:path[@sha][#Lstart-Lend]]` into
/// `(repo, path?, range?)`. The sha is accepted but ignored for the query — we match
/// by file + line-range across ALL shas ("what was ever said about these lines").
pub(crate) fn parse_ref_query(s: &str) -> Result<(String, Option<String>, Option<[u64; 2]>)> {
    let bad = || anyhow!("invalid refs target '{s}': expected repo[:path[#Lstart-Lend]]");
    let (repo, rest) = match s.split_once(':') {
        Some((r, rest)) => (r.to_string(), Some(rest)),
        None => (s.to_string(), None),
    };
    if repo.is_empty() {
        return Err(bad());
    }
    let (path, range) = match rest {
        None => (None, None),
        Some(rest) => {
            let (before_hash, range) = match rest.split_once('#') {
                Some((p, span)) => (p, Some(append_ref::parse_range(span)?)),
                None => (rest, None),
            };
            let path = before_hash.split('@').next().unwrap_or(before_hash);
            (if path.is_empty() { None } else { Some(path.to_string()) }, range)
        }
    };
    Ok((repo, path, range))
}

/// `confer refs <repo[:path[#range]]>` — the reverse index (design/40 #4): the threads
/// that reference this code. A report; `--check` is a predicate (exit 1 if none).
pub(crate) fn cmd_refs(target: String, check: bool, all_hubs: bool, json: bool) -> Result<()> {
    let (repo, path, range) = parse_ref_query(&target)?;
    let hubs: Vec<std::path::PathBuf> =
        if all_hubs { crosshub::hub_dirs() } else { vec![config::repo_root()?] };

    // (hub_label, hit, staleness). Staleness compares the pinned blob OID vs HEAD's in
    // the locally-mapped clone (design/40 #5) — "unknown" when the repo isn't cloned here.
    let mut hits: Vec<(String, projection::RefHit, &'static str)> = Vec::new();
    for hub in &hubs {
        let Ok(msgs) = store::all_messages(hub) else { continue };
        let idx = projection::RefIndex::fold(&msgs);
        let repo_inv = repos::load(hub);
        let label = crosshub::hub_label(hub);
        let mut clone_cache: std::collections::HashMap<String, Option<std::path::PathBuf>> =
            std::collections::HashMap::new();
        for h in idx.query(&repo, path.as_deref(), range) {
            let clone = clone_cache
                .entry(h.repo.clone())
                .or_insert_with(|| refcode::clone_for(&repo_inv, &h.repo))
                .clone();
            // design/45 §1.7: a patch's staleness IS the landed-detection (result_hash vs
            // HEAD:<path>), not the ordinary base-drift signal (patch refs carry no content_hash).
            let st = if h.kind == projection::RefKind::Patch {
                refcode::patch_staleness(clone.as_deref(), &h.path, h.result_hash.as_deref()).label()
            } else {
                refcode::staleness_ex(
                    clone.as_deref(),
                    &h.sha,
                    &h.path,
                    h.content_hash.as_deref(),
                    h.base_ref.as_deref(),
                    h.fork_point.as_deref(),
                )
                .label()
            };
            hits.push((label.clone(), h.clone(), st));
        }
    }

    // Predicate: 0 if something references it, 1 if not. No listing (stdout stays clean).
    if check {
        return if hits.is_empty() { Err(PredicateFalse.into()) } else { Ok(()) };
    }

    if json {
        for (hub, h, st) in &hits {
            let mut refj = serde_json::json!({ "repo": h.repo, "path": h.path, "sha": h.sha });
            if let Some(r) = h.range {
                refj["range"] = serde_json::json!(r);
            }
            if let Some(ch) = &h.content_hash {
                refj["content_hash"] = serde_json::json!(ch);
            }
            if let Some(n) = &h.ref_name {
                refj["ref_name"] = serde_json::json!(n);
            }
            if let Some(t) = &h.ref_type {
                refj["ref_type"] = serde_json::json!(t);
            }
            if let Some(d) = &h.commit_date {
                refj["commit_date"] = serde_json::json!(d);
            }
            if h.dirty {
                refj["dirty"] = serde_json::json!(true);
            }
            if h.untracked {
                refj["untracked"] = serde_json::json!(true);
            }
            if let Some(b) = &h.base_ref {
                refj["base_ref"] = serde_json::json!(b);
            }
            if let Some(f) = &h.fork_point {
                refj["fork_point"] = serde_json::json!(f);
            }
            if h.kind == projection::RefKind::Patch {
                refj["patch"] = serde_json::json!(true);
            }
            if let Some(rh) = &h.result_hash {
                refj["result_hash"] = serde_json::json!(rh);
            }
            let line = serde_json::json!({
                "event": "ref-hit",
                "hub": hub,
                "ref": refj,
                "staleness": st,
                "message": {
                    "id": h.msg_id, "from": h.from, "type": h.msg_type,
                    "ts": h.ts, "topic": h.topic, "summary": h.summary,
                },
                "thread": { "root": h.thread_root, "status": h.request_status },
            });
            println!("{}", serde_json::to_string(&line)?);
        }
        return Ok(());
    }

    let target_disp = match (&path, range) {
        (Some(p), Some(r)) => format!("{repo}:{p}#L{}-{}", r[0], r[1]),
        (Some(p), None) => format!("{repo}:{p}"),
        (None, _) => repo.clone(),
    };
    if hits.is_empty() {
        println!("no conversations reference {target_disp}");
        return Ok(());
    }
    println!("{} conversation(s) reference {target_disp}:", hits.len());
    for (hub, h, st) in &hits {
        let hubp = if all_hubs { format!("{hub} · ") } else { String::new() };
        let loc = h.topic.as_deref().map(|t| format!("#{t}")).unwrap_or_else(|| "—".into());
        let status = h.request_status.map(|s| format!(" [{s}]")).unwrap_or_default();
        let rng = h.range.map(|r| format!("#L{}-{}", r[0], r[1])).unwrap_or_default();
        let paren = refcode::identity_paren(h.ref_name.as_deref(), h.ref_type.as_deref(), h.commit_date.as_deref());
        // Flag drift: mark a ref whose code moved/changed under the pin, or is off the
        // current history entirely (silent when "current"/"unknown" — no clone, or
        // unchanged, needs no callout). "unpinned" reads as a legacy marker.
        let stmark = match *st {
            "changed" => "  ⚠changed",
            "moved" => "  ⚠moved",
            "reachable" => "  ⚠reachable",
            "offline" => "  ⚠offline",
            "squashed" => "  ⚠squashed",
            "unpinned" => "  ⚠unpinned — legacy",
            _ => "",
        };
        let flags = match (h.dirty, h.untracked) {
            (true, true) => "  [dirty][untracked]",
            (true, false) => "  [dirty]",
            (false, true) => "  [untracked]",
            (false, false) => "",
        };
        // design/45 §1.7: the patch chip — "proposed a change here (applied/open)", `applied`
        // read straight off the landed-detection staleness computed above.
        let patch_chip = if h.kind == projection::RefKind::Patch {
            format!("  ⟳ proposed a change here ({})", if *st == "landed" { "applied" } else { "open" })
        } else {
            String::new()
        };
        println!(
            "  {hubp}{loc}  {}  {}{status}  {}  ({}:{}{paren}{rng}){stmark}{flags}{patch_chip}",
            short_id(&h.msg_id),
            h.from,
            h.summary,
            h.repo,
            h.path
        );
    }
    Ok(())
}

/// `confer ref-contains <sha> [<ref>] [--repo <slug>]` — plumbing predicate (design/44
/// Addendum 1): is `<sha>` reachable from `<ref>` (default `HEAD`)? Exit 0 if yes, 1 if
/// no — `git merge-base --is-ancestor` under the hood, a more robust liveness check
/// than "is it still HEAD" (HEAD advances constantly; ancestry doesn't go stale on
/// every further commit). Resolves the repo via `--repo <slug>`'s machine-local clone
/// map, else the git working tree at the current directory — no fetch either way.
pub(crate) fn cmd_ref_contains(sha: String, against: String, repo: Option<String>) -> Result<()> {
    let dir = match repo {
        Some(slug) => {
            let hub = config::repo_root()?;
            let repo_inv = repos::load(&hub);
            refcode::clone_for(&repo_inv, &slug).ok_or_else(|| {
                anyhow!("repo '{slug}' has no mapped clone here (`confer repos map {slug} <path>`)")
            })?
        }
        None => {
            let cwd = std::env::current_dir()?;
            let o = gitcmd::output(&cwd, &["rev-parse", "--show-toplevel"])?;
            if !o.status.success() {
                return Err(anyhow!(
                    "not inside a git working tree — pass --repo <slug> to resolve via the clone map"
                ));
            }
            std::path::PathBuf::from(String::from_utf8_lossy(&o.stdout).trim())
        }
    };
    if refcode::is_ancestor(&dir, &sha, &against) {
        println!("{sha} is reachable from {against}");
        Ok(())
    } else {
        println!("{sha} is NOT reachable from {against}");
        Err(PredicateFalse.into())
    }
}

/// Compact pointer tag for the one-line view: ` ⟶ repo:path (branch · date)` (first
/// ref, +N more). The parenthetical (design/44 §5.1) is omitted when neither field is
/// present — legacy refs render exactly as before.
pub(crate) fn render_refs(refs: &[schema::CodeRef]) -> String {
    let Some(first) = refs.first() else {
        return String::new();
    };
    let more = if refs.len() > 1 {
        format!(" +{}", refs.len() - 1)
    } else {
        String::new()
    };
    let paren = refcode::identity_paren(
        first.ref_name.as_deref(),
        first.ref_type.as_deref(),
        first.commit_date.as_deref(),
    );
    format!(" ⟶ {}:{}{paren}{more}", first.repo, first.path)
}
