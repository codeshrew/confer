//! `confer whoami` — which confer role(s) is this session?
//!
//! A session-start hook wants to record the role a harness session belongs to (jarvis). Until now
//! every answer needed the caller to stand in a hub clone or to have `CONFER_ROLE` set, and agents
//! mostly run in project repos. This tries the sources confer already has, in order, stops at the
//! first that answers, and says which one it was. Nothing resolves: exit 1, nothing on stdout.
//! Local files only (plus local git in the cwd case), because it runs on every session start.

use crate::{attach, autoheal, prune};
use anyhow::Result;
use std::path::PathBuf;

struct Answer {
    /// Machine name of the source: env | hub | session | project.
    source: &'static str,
    /// What it was, for a human: the clone path, the session id, the project dir.
    detail: String,
    /// role → hub labels (empty for env).
    roles: Vec<(String, Vec<String>)>,
}

/// The hub's short name, from its origin URL, read straight from `.git/config` (no git process:
/// this runs on every session start, and a git spawn per hub was most of the time).
fn label(root: &std::path::Path) -> String {
    let url = std::fs::read_to_string(root.join(".git").join("config")).ok().and_then(|cfg| {
        let mut in_origin = false;
        cfg.lines().find_map(|l| {
            let l = l.trim();
            if l.starts_with('[') {
                in_origin = l == "[remote \"origin\"]";
                return None;
            }
            let (k, v) = l.split_once('=')?;
            (in_origin && k.trim() == "url").then(|| v.trim().to_string())
        })
    });
    url.and_then(|u| crate::reconnect::canonical_hub_id(&u))
        .and_then(|c| c.rsplit('/').next().map(str::to_string))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
}

fn group(found: Vec<(PathBuf, String)>) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for (root, role) in found {
        if !prune::looks_like_hub(&root) || !attach::is_member(&root, &role) {
            continue;
        }
        let l = label(&root);
        match out.iter_mut().find(|(r, _)| *r == role) {
            Some((_, hubs)) if hubs.contains(&l) => {}
            Some((_, hubs)) => hubs.push(l),
            None => out.push((role, vec![l])),
        }
    }
    for (_, hubs) in &mut out {
        hubs.sort();
    }
    out.sort();
    out
}

/// The hub clone we are standing in: `$CONFER_HUB`, else the nearest ancestor of cwd with a
/// `.confer/identity.json`. Found by looking at files, not by asking git.
fn clone_here() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("CONFER_HUB").filter(|h| !h.is_empty()) {
        return Some(PathBuf::from(h));
    }
    let cwd = std::env::current_dir().ok()?;
    cwd.ancestors().find(|d| d.join(".confer").join("identity.json").is_file()).map(PathBuf::from)
}

fn resolve() -> Option<Answer> {
    if let Some(r) = std::env::var("CONFER_ROLE").ok().filter(|r| !r.is_empty()) {
        return Some(Answer { source: "env", detail: "$CONFER_ROLE".into(), roles: vec![(r, Vec::new())] });
    }
    if let Some(root) = clone_here() {
        let id = std::fs::read_to_string(root.join(".confer").join("identity.json")).ok();
        let role = id
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v.get("role").and_then(|r| r.as_str()).map(String::from));
        if let Some(r) = role {
            let roles = group(vec![(root.clone(), r.clone())]);
            let roles = if roles.is_empty() { vec![(r, Vec::new())] } else { roles };
            return Some(Answer { source: "hub", detail: root.display().to_string(), roles });
        }
    }
    if let Some(s) = autoheal::current_session() {
        let found: Vec<(PathBuf, String)> = autoheal::load()
            .targets
            .into_iter()
            .filter(|t| t.session.as_deref() == Some(s.as_str()))
            .map(|t| (PathBuf::from(t.hub), t.role))
            .collect();
        let roles = group(found);
        if !roles.is_empty() {
            return Some(Answer { source: "session", detail: s, roles });
        }
    }
    let project = std::env::var_os("CLAUDE_PROJECT_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .map(|p| p.canonicalize().unwrap_or(p).to_string_lossy().to_string())?;
    let hubs = crate::plugin::remembered(&project)?;
    let roles = group(hubs.into_iter().map(|(h, r)| (PathBuf::from(h), r)).collect());
    (!roles.is_empty()).then_some(Answer { source: "project", detail: project, roles })
}

pub(crate) fn cmd(json: bool) -> Result<()> {
    let Some(a) = resolve() else {
        eprintln!(
            "confer whoami: no role resolved (checked $CONFER_ROLE, the hub clone at cwd/$CONFER_HUB, \
             this session's armed watches, and this project's remembered hubs)"
        );
        return Err(crate::PredicateFalse.into());
    };
    if json {
        let roles: Vec<_> = a.roles.iter().map(|(r, h)| serde_json::json!({ "role": r, "hubs": h })).collect();
        println!("{}", serde_json::json!({ "source": a.source, "detail": a.detail, "roles": roles }));
        return Ok(());
    }
    let from = match a.source {
        "env" => "from $CONFER_ROLE".to_string(),
        "hub" => format!("from the hub clone {}", a.detail),
        "session" => format!("from this session's armed watches (session {})", a.detail),
        _ => format!("from the hubs remembered for project {}", a.detail),
    };
    for (r, hubs) in &a.roles {
        if hubs.is_empty() {
            println!("{r}");
        } else {
            println!("{r}  ({})", hubs.join(", "));
        }
    }
    eprintln!("confer whoami: {from}");
    Ok(())
}
