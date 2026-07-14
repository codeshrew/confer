//! The role roster: stable role id → a Role card (mutable display name, expected
//! host, description). One Markdown file per role (`roles/<id>.md`, YAML
//! frontmatter) — same conflict-free, Obsidian-editable format as messages;
//! the id (the filename) is permanent, the `display` is cosmetic and renameable.
//! Legacy `roles.toml` (a single shared-write file) is still read for migration.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Role {
    #[serde(default)]
    pub display: Option<String>,
    /// Expected host (declaration) — where this role is meant to run.
    #[serde(default)]
    pub host: Option<String>,
    /// One-line "what this agent is / does" — surfaced in `who`, matched by `whois`.
    #[serde(default)]
    pub desc: Option<String>,
    /// Human-friendly nicknames/phrases the owner uses for this agent (e.g.
    /// "iOS agent", "the mac mini one"). Matched by `whois`; self-maintained via
    /// `describe`. See DESIGN.md.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Self-declared lifecycle INTENT — `active` (default) | `dormant` | `retired`
    ///. Self-sovereign: honored only when the card edit is signature-verified
    /// against the role's pinned key (`verify::card_trust`), so a peer can't set it. It's an
    /// overlay on the presence heartbeat (which alone drives liveness/aging), never a liveness
    /// claim itself.
    #[serde(default)]
    pub status: Option<String>,
    /// The role's SSH public key (`ssh-ed25519 AAAA… comment`) — published so peers
    /// can verify this role's signed commits. See DESIGN.md.
    #[serde(default)]
    pub pubkey: Option<String>,
}

/// The role's published SSH public key, if any.
pub fn pubkey<'a>(roster: &'a Roster, id: &str) -> Option<&'a str> {
    roster.get(id).and_then(|r| r.pubkey.as_deref())
}

pub type Roster = HashMap<String, Role>;

/// Load `roles/<id>.md` (preferred) unioned over legacy `roles.toml`.
pub fn load(root: &Path) -> Roster {
    let mut m = Roster::new();

    // Legacy roles.toml (migration path).
    if let Ok(txt) = std::fs::read_to_string(root.join("roles.toml")) {
        if let Ok(val) = txt.parse::<toml::Value>() {
            if let Some(roles) = val.get("roles").and_then(toml::Value::as_table) {
                for (id, cfg) in roles {
                    let s = |k: &str| cfg.get(k).and_then(toml::Value::as_str).map(String::from);
                    m.insert(
                        id.clone(),
                        Role {
                            display: s("display"),
                            host: s("host"),
                            desc: s("desc"),
                            aliases: Vec::new(),
                            status: s("status"),
                            pubkey: None,
                        },
                    );
                }
            }
        }
    }

    // roles/<id>.md wins over legacy.
    let dir = root.join("roles");
    if dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) != Some("md") {
                    continue;
                }
                let Some(id) = p.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if let Ok(txt) = std::fs::read_to_string(&p) {
                    match parse_role(&txt) {
                        Some(role) => {
                            m.insert(id.to_string(), role);
                        }
                        // A malformed card would otherwise make the role vanish
                        // from `who`/display resolution with no signal (S3).
                        None => eprintln!("confer: skipping malformed role card {}", p.display()),
                    }
                }
            }
        }
    }
    m
}

/// Parse a role card's YAML frontmatter (body is ignored / freeform notes).
fn parse_role(text: &str) -> Option<Role> {
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

/// Resolve a role id to its display name (falls back to the id).
pub fn display<'a>(roster: &'a Roster, id: &'a str) -> &'a str {
    roster
        .get(id)
        .and_then(|r| r.display.as_deref())
        .unwrap_or(id)
}

/// The role's declared (expected) host, if any.
pub fn host<'a>(roster: &'a Roster, id: &str) -> Option<&'a str> {
    roster.get(id).and_then(|r| r.host.as_deref())
}
