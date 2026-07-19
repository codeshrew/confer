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
    /// The FULL markdown BODY below the YAML frontmatter of `roles/<id>.md` — the freeform
    /// role-profile prose (`desc` is only the one-line frontmatter summary). NOT a YAML field:
    /// it's the text after the closing `---`, captured by `parse_role`, so it's `#[serde(skip)]`
    /// (never deserialized/serialized) and `None` for the legacy `roles.toml` path or a card with
    /// no body. Empty/whitespace-only body normalizes to `None`.
    #[serde(skip)]
    pub profile: Option<String>,
}

/// The role's published SSH public key, if any.
pub fn pubkey<'a>(roster: &'a Roster, id: &str) -> Option<&'a str> {
    roster.get(id).and_then(|r| r.pubkey.as_deref())
}

/// The SINGLE source of truth for "what key does this card publish", shared by the read/pin side
/// (`parse_role`, below) and the write-side 1:1 guard (`published_pubkey` in main.rs) so the two can
/// never disagree. Absent / null / "" → Ok(None) (no key). A non-empty string → Ok(Some). A present
/// but NON-string value (bool/number/list/mapping) → Err(type-name).
///
/// This is load-bearing: `roster::parse_role` used to deserialize straight into a typed
/// `Option<String>`, which silently STRINGIFIES a YAML bareword (`pubkey: true` → "true",
/// `pubkey: 12345` → "12345"), while the write guard decodes an untyped Value and refuses it. That
/// split let a hub writer poison a new peer's TOFU pin with a bogus literal like "true", permanently
/// mismatching the real role (identity-DoS — red-team). Routing BOTH sides through this one
/// classifier closes the divergence by construction.
pub(crate) fn classify_pubkey(map: &serde_yaml::Mapping) -> Result<Option<String>, String> {
    match map.get("pubkey") {
        None => Ok(None),
        Some(v) if v.is_null() => Ok(None),
        Some(v) => match v.as_str() {
            Some(s) => {
                let t = s.trim();
                Ok((!t.is_empty()).then(|| t.to_string()))
            }
            None => Err(match v {
                serde_yaml::Value::Sequence(_) => "list",
                serde_yaml::Value::Mapping(_) => "mapping",
                serde_yaml::Value::Number(_) => "number",
                serde_yaml::Value::Bool(_) => "boolean",
                _ => "non-string value",
            }
            .to_string()),
        },
    }
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
                            profile: None, // legacy roles.toml has no markdown body
                        },
                    );
                }
            }
        }
    }

    // roles/<id>.md wins over legacy.
    let dir = root.join("roles");
    if dir.is_dir() {
        match std::fs::read_dir(&dir) {
          Err(e) => eprintln!(
            "confer: ⚠ cannot read {} ({e}) — every role's display/host/pubkey is MISSING this load \
             (who/whois/verification will be blind).",
            dir.display()
          ),
          Ok(entries) => {
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
    }
    m
}

/// Parse a role card's YAML frontmatter, plus the freeform markdown body below it
/// (surfaced as `Role.profile`; `desc` remains the one-line frontmatter summary).
fn parse_role(text: &str) -> Option<Role> {
    // Strip a leading UTF-8 BOM before the fence-sniff — keep this identical to `parse_card` in
    // main.rs, so the read side and the write-side key guard can't disagree about whether a BOM'd
    // card has frontmatter (a divergence there was an identity-hijack vector — red-team).
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return None;
    }
    let mut yaml = String::new();
    let mut body = String::new();
    let mut in_body = false;
    for line in lines {
        if !in_body && line.trim_end() == "---" {
            in_body = true;
            continue;
        }
        if in_body {
            body.push_str(line);
            body.push('\n');
        } else {
            yaml.push_str(line);
            yaml.push('\n');
        }
    }
    let mut role: Role = serde_yaml::from_str(&yaml).ok()?;
    // The body below the closing fence — empty/whitespace-only → None (a frontmatter-only card
    // has no profile). Mirrors `parse_message`'s body split (schema.rs), so a role card and a
    // message card treat "prose below the frontmatter" identically.
    let trimmed = body.trim();
    role.profile = (!trimmed.is_empty()).then(|| trimmed.to_string());
    // Re-derive `pubkey` through the SHARED classifier rather than trusting the typed
    // `Option<String>` deserialize, which stringifies a YAML bareword (`pubkey: true` → "true") that
    // the write-side guard refuses — the split let an attacker poison a peer's TOFU pin (red-team).
    // On the read side a non-string / absent pubkey degrades to None (unverifiable), never a bogus
    // literal, so it can't be pinned.
    if let Ok(m) = serde_yaml::from_str::<serde_yaml::Mapping>(&yaml) {
        role.pubkey = classify_pubkey(&m).ok().flatten();
    } else {
        role.pubkey = None;
    }
    Some(role)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_role_never_pins_a_nonstring_pubkey() {
        // Read-side classification MUST match the write guard: a YAML bareword (`pubkey: true`) or
        // any non-string value degrades to None (unverifiable), never a bogus literal like "true"
        // that would poison a peer's TOFU pin (red-team round 5).
        assert_eq!(parse_role("---\ndisplay: v\npubkey: true\n---\n").unwrap().pubkey, None);
        assert_eq!(parse_role("---\npubkey: 12345\n---\n").unwrap().pubkey, None);
        assert_eq!(parse_role("---\npubkey: null\n---\n").unwrap().pubkey, None);
        // In YAML 1.2 (serde_yaml 0.9) `yes` is a STRING, not a bool — so both sides agree on
        // Some("yes") (no divergence; it just can't satisfy a real signature). Documented here so the
        // distinction from `true`/`false`/numbers is explicit.
        assert_eq!(
            parse_role("---\npubkey: yes\n---\n").unwrap().pubkey.as_deref(),
            Some("yes")
        );
        // A real key string is read normally, and a BOM before the fence doesn't hide it.
        assert_eq!(
            parse_role("---\npubkey: ssh-ed25519 AAAA x\n---\n").unwrap().pubkey.as_deref(),
            Some("ssh-ed25519 AAAA x")
        );
        assert_eq!(
            parse_role("\u{FEFF}---\npubkey: ssh-ed25519 AAAA x\n---\n").unwrap().pubkey.as_deref(),
            Some("ssh-ed25519 AAAA x")
        );
    }

    #[test]
    fn parse_role_captures_body_as_profile_but_frontmatter_only_is_none() {
        // The markdown BODY below the closing fence becomes `profile` (distinct from the
        // one-line `desc`); a frontmatter-only card, or a body that is only whitespace, → None.
        let r = parse_role("---\ndisplay: Prosy\ndesc: one-liner\n---\n# Heading\n\nParagraph body.\n").unwrap();
        assert_eq!(r.desc.as_deref(), Some("one-liner"));
        assert_eq!(r.profile.as_deref(), Some("# Heading\n\nParagraph body."));
        // Frontmatter-only (no closing fence content) → no profile.
        assert_eq!(parse_role("---\ndisplay: Bare\n---\n").unwrap().profile, None);
        // Closing fence present but body is whitespace-only → normalized to None.
        assert_eq!(parse_role("---\ndisplay: Bare\n---\n\n   \n").unwrap().profile, None);
    }
}
