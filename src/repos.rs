//! Repo inventory: the repos a hub is "about," one card per repo as
//! `repos/<slug>.md` (same conflict-free, Obsidian-legible format as roles/groups).
//!
//! Durable docs/specs live in these repos (or the hub itself for cross-owner work);
//! confer messages POINT at them (`--ref repo:path`) instead of re-transmitting them.
//! `access` records which roles can actually clone a repo, so an author can tell
//! whether the audience can follow a pointer or needs the content carried. See
//! DESIGN.md.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

pub type Repos = HashMap<String, Repo>;

#[derive(Deserialize, Default, Clone)]
pub struct Repo {
    /// code | docs | tooling | reference | private — the repo's role in the conversation.
    #[serde(default = "default_role")]
    pub role: String,
    /// clone URL; omitted for a private/unshared repo.
    #[serde(default)]
    pub url: Option<String>,
    /// role ids that can clone/read it; empty = every hub participant.
    #[serde(default)]
    pub access: Vec<String>,
    /// where durable docs live inside the repo (the "about" target), e.g. `docs/`.
    #[serde(default)]
    pub docs: Option<String>,
    /// human owner (forward-compat with F2 cross-owner access).
    #[serde(default)]
    pub owner: Option<String>,
}

fn default_role() -> String {
    "code".to_string()
}

pub fn load(root: &Path) -> Repos {
    let mut repos = Repos::new();
    let dir = root.join("repos");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return repos;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Some(name) = p.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Ok(txt) = std::fs::read_to_string(&p) {
            match parse(&txt) {
                Some(card) => {
                    repos.insert(name.to_string(), card);
                }
                None => eprintln!("confer: skipping malformed repo card {}", p.display()),
            }
        }
    }
    repos
}

fn parse(text: &str) -> Option<Repo> {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return None;
    }
    let mut yaml = String::new();
    for line in lines {
        if line.trim_end() == "---" {
            break;
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    serde_yaml::from_str(&yaml).ok()
}

/// Can role `who` clone/read this repo? Empty access = hub-wide (yes); otherwise
/// the role (or the reserved `all`) must be listed.
pub fn accessible_to(repo: &Repo, who: &str) -> bool {
    repo.access.is_empty() || repo.access.iter().any(|a| a == who || a == "all")
}
