//! Agent groups: a named set of role ids, defined one file per group as
//! `groups/<name>.md` (YAML frontmatter `members: [...]`) — same conflict-free,
//! Obsidian-editable format as roles. A message target (`to`/`cc`) may be a role
//! id, a group name, or the reserved `all`.

use crate::schema::{self, Message};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// group name → member role ids.
pub type Groups = HashMap<String, Vec<String>>;

#[derive(Deserialize, Default)]
struct GroupCard {
    #[serde(default)]
    members: Vec<String>,
}

pub fn load(root: &Path) -> Groups {
    let mut g = Groups::new();
    let dir = root.join("groups");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return g;
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
                    g.insert(name.to_string(), card.members);
                }
                None => eprintln!("confer: skipping malformed group card {}", p.display()),
            }
        }
    }
    g
}

fn parse(text: &str) -> Option<GroupCard> {
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

/// Is `me` addressed by this message — as a literal recipient, via `all`, or as a
/// member of a group named in `to`/`cc`?
pub fn addressed(m: &Message, me: &str, groups: &Groups) -> bool {
    schema::is_recipient(m, me)
        || schema::targets(m).any(|t| groups.get(t).is_some_and(|mem| mem.iter().any(|x| x == me)))
}

/// Is `me` a DIRECT addressee — named in `to` (or a member of a group in `to`) —
/// excluding `cc` (optional/FYI) and pure `all` broadcasts? This is the "my inbox"
/// test: mail a sender explicitly put on me to act on. The unread-nag uses it so
/// broadcasts and FYIs don't re-surface forever (`inbox.rs`).
pub fn directly_addressed(m: &Message, me: &str, groups: &Groups) -> bool {
    m.front
        .to
        .iter()
        .any(|t| t == me || groups.get(t).is_some_and(|mem| mem.iter().any(|x| x == me)))
}
