//! Message = a Markdown file with YAML frontmatter (Obsidian-compatible).
//! One file per message, under threads/<topic>/. See ../../DESIGN.md.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// The reserved target meaning "everyone".
pub const ALL: &str = "all";

/// Deserialize a YAML field that may be a single string OR a list of strings
/// into a `Vec<String>` — so `to: bob` and `to: [carol, bob]` both work
/// (and old scalar data keeps parsing).
fn string_or_seq<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    Ok(match Option::<OneOrMany>::deserialize(d)? {
        None => Vec::new(),
        Some(OneOrMany::One(s)) => vec![s],
        Some(OneOrMany::Many(v)) => v,
    })
}

/// A structured pointer to code, resilient across moves/renames.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRef {
    pub repo: String,
    pub sha: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<[u64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// YAML frontmatter — the structured metadata about a message "page".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    /// Globally unique, time-sortable id (ULID). The stable message identity.
    pub id: String,
    /// The ROLE id (stable) that wrote this — resolved to a display name via the roster.
    pub from: String,
    /// note | request | claim | done | error | supersede (unknown → ignore-but-log).
    #[serde(rename = "type")]
    pub msg_type: String,
    /// RFC 3339 UTC — advisory; ordering authority is the id (ULID) + hub commit order.
    pub ts: String,
    /// Hostname the message was written on (provenance — which machine a role ran on).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,

    /// Primary addressees (required for `request`) — "this is for you". Each is a
    /// target: a role id, a group name, or `all`.
    #[serde(
        default,
        deserialize_with = "string_or_seq",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub to: Vec<String>,
    /// Secondary audience — "you may care, act only if you choose". Same target grammar.
    #[serde(
        default,
        deserialize_with = "string_or_seq",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub cc: Vec<String>,
    /// low | normal | high — triage hint (default normal, omitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    /// Thread/topic slug (= the containing folder).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    /// Request id (for claim/done/error).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of: Option<String>,
    /// Superseded message id (for supersede).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    /// Resolution carried by a terminal `done`: `done` (default) | `wont-do` |
    /// `duplicate` | `obsolete`. Resolution is separate from status — a request can
    /// leave the board completed OR consciously dropped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    /// A deferred/backlog request — captured but kept OFF the active board until
    /// promoted (GTD someday-maybe).
    #[serde(default, skip_serializing_if = "is_false")]
    pub defer: bool,
    /// Delegated sub-actor path (`role/subrole`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// Content provenance (`agent`|`web`|`human`|…) — external → downweight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    /// Maintained one-line summary for glanceability (falls back to first body line).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<CodeRef>,
}

/// A message = frontmatter + a Markdown body.
#[derive(Debug, Clone)]
pub struct Message {
    pub front: Frontmatter,
    pub body: String,
}

impl Message {
    /// Serialize to a Markdown file: `---\n<yaml>---\n\n<body>\n`.
    pub fn to_markdown(&self) -> Result<String> {
        let yaml = serde_yaml::to_string(&self.front)?;
        Ok(format!("---\n{yaml}---\n\n{}\n", self.body.trim_end()))
    }

    /// One-line summary for list/skim views: the `summary` field or the first body
    /// line. A blank summary (a hand-edited or malformed file) falls back too, so a
    /// list line is never empty. Control chars are stripped (a one-liner) since this
    /// text is peer-authored and lands on terminals/TUI/HTML — see `sanitize_term`.
    pub fn summary_line(&self) -> String {
        let s = self.front.summary.as_deref().unwrap_or("").trim();
        let raw = if !s.is_empty() {
            s.to_string()
        } else {
            self.body
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("")
                .to_string()
        };
        sanitize_term(&raw, false)
    }
}

/// Remove terminal-hostile control characters before peer-authored text is shown on a
/// terminal. A message body is untrusted content the reader folds into context
///; raw ANSI/C0 escapes in it can rewrite the reader's screen, forge a fake
/// envelope line, hide text, or emit OSC-8 hyperlinks. Strips C0 (0x00–0x1F), DEL
/// (0x7F), and C1 (0x80–0x9F); keeps `\n`/`\t` only when `multiline` (a body view). This
/// is the load-bearing gate: foreign-hub messages never pass through our append-time
/// lint, so every human-facing render must sanitize.
pub fn sanitize_term(s: &str, multiline: bool) -> String {
    s.chars()
        .filter(|&c| match c {
            '\n' | '\t' => multiline,
            _ => {
                let cp = c as u32;
                !(cp < 0x20 || cp == 0x7f || (0x80..=0x9f).contains(&cp))
            }
        })
        .collect()
}

/// Parse a Markdown-with-frontmatter file into a Message.
pub fn parse_message(text: &str) -> Result<Message> {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        bail!("missing YAML frontmatter (no leading ---)");
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
    let front: Frontmatter = serde_yaml::from_str(&yaml)?;
    Ok(Message {
        front,
        body: body.trim().to_string(),
    })
}

/// The closed v1 type vocabulary.
pub const TYPES: [&str; 8] = [
    "note", "request", "claim", "done", "error", "supersede", "blocked", "defer",
];

/// Types a watcher should surface as actionable (vs. informational notes).
/// `blocked` counts — a stall on a request its sender should learn about.
pub fn is_actionable(m: &Message) -> bool {
    matches!(
        m.front.msg_type.as_str(),
        "request" | "claim" | "done" | "error" | "blocked"
    )
}

/// All targets of a message (to ∪ cc): each a role id, group name, or `all`.
pub fn targets(m: &Message) -> impl Iterator<Item = &String> {
    m.front.to.iter().chain(m.front.cc.iter())
}

/// Is `me` a LITERAL recipient (named in to/cc) or is the message to `all`?
/// Group expansion is layered on by the `groups` module (it holds memberships).
pub fn is_recipient(m: &Message, me: &str) -> bool {
    targets(m).any(|t| t == me || t == ALL)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(msg_type: &str, body: &str) -> Message {
        Message {
            front: Frontmatter {
                id: "01J8Z9K3QH7X".into(),
                from: "carol".into(),
                msg_type: msg_type.into(),
                ts: "2026-07-08T17:20:04Z".into(),
                host: Some("host-a.local".into()),
                to: vec!["bob".into()],
                cc: vec!["alice".into()],
                priority: Some("high".into()),
                topic: Some("px".into()),
                reply_to: None,
                of: None,
                supersedes: None,
                resolution: None,
                defer: false,
                via: None,
                src: None,
                summary: Some("wire it".into()),
                refs: vec![],
            },
            body: body.into(),
        }
    }

    #[test]
    fn round_trip_preserves_fields_and_body() {
        let m = msg("request", "hello\n\nsecond para");
        let parsed = parse_message(&m.to_markdown().unwrap()).unwrap();
        assert_eq!(parsed.front.id, m.front.id);
        assert_eq!(parsed.front.msg_type, "request");
        assert_eq!(parsed.front.to, vec!["bob".to_string()]);
        assert_eq!(parsed.front.cc, vec!["alice".to_string()]);
        assert_eq!(parsed.front.priority.as_deref(), Some("high"));
        assert_eq!(parsed.body, "hello\n\nsecond para");
    }

    #[test]
    fn body_with_fenced_code_and_dashes_survives() {
        // A body containing a fenced block AND a `---` line must not confuse the
        // frontmatter delimiter parsing (the closing --- is the first one after
        // the header; everything after is body verbatim).
        let body = "Look:\n\n```yaml\nx: 1\n---\ny: 2\n```\n\nthematic break below\n\n---\n\ndone";
        let m = msg("note", body);
        let parsed = parse_message(&m.to_markdown().unwrap()).unwrap();
        assert_eq!(parsed.body, body);
    }

    #[test]
    fn crlf_frontmatter_parses() {
        let text = "---\r\nid: x\r\nfrom: carol\r\ntype: note\r\nts: t\r\nsummary: s\r\n---\r\nbody line\r\n";
        let parsed = parse_message(text).unwrap();
        assert_eq!(parsed.front.id, "x");
        assert_eq!(parsed.body, "body line");
    }

    #[test]
    fn malformed_yaml_errors_not_panics() {
        assert!(parse_message("---\n: : : not yaml\n---\nbody").is_err());
    }

    #[test]
    fn missing_frontmatter_errors() {
        assert!(parse_message("just a body, no frontmatter").is_err());
    }

    #[test]
    fn summary_line_falls_back_to_first_nonblank_body_line() {
        let mut m = msg("note", "\n\n  first real line  \nsecond");
        m.front.summary = None;
        assert_eq!(m.summary_line(), "first real line");
    }

    #[test]
    fn sanitize_term_strips_control_keeps_content() {
        // ESC/ANSI, C0, DEL, C1 removed; ordinary text and unicode glyphs kept.
        assert_eq!(sanitize_term("a\x1b[31mred\x1b[0m", true), "a[31mred[0m");
        assert_eq!(sanitize_term("hi\x07\x7f\u{85}there", true), "hithere");
        assert_eq!(sanitize_term("‼ done → ok ⟶ repo:x", false), "‼ done → ok ⟶ repo:x");
        // multiline=false collapses newlines/tabs (a one-liner); true keeps them.
        assert_eq!(sanitize_term("line1\nline2\tcol", false), "line1line2col");
        assert_eq!(sanitize_term("line1\nline2\tcol", true), "line1\nline2\tcol");
    }

    #[test]
    fn summary_line_sanitizes_ansi() {
        let mut m = msg("note", "body");
        // ESC (0x1b) and BEL (0x07) that make the OSC-8 hyperlink escape are stripped;
        // the printable remainder stays (the link can no longer fire on the terminal).
        m.front.summary = Some("evil\x1b]8;;http://x\x07click".to_string());
        assert_eq!(m.summary_line(), "evil]8;;http://xclick");
    }

    #[test]
    fn is_recipient_matches_to_cc_and_all() {
        let m = msg("request", "x");
        assert!(is_recipient(&m, "bob")); // to
        assert!(is_recipient(&m, "alice")); // cc
        assert!(!is_recipient(&m, "nobody"));
        let mut m2 = msg("note", "x");
        m2.front.to = vec![ALL.into()];
        m2.front.cc = vec![];
        assert!(is_recipient(&m2, "anyone")); // `all`
    }

    #[test]
    fn is_actionable_excludes_notes() {
        assert!(is_actionable(&msg("request", "x")));
        assert!(!is_actionable(&msg("note", "x")));
    }
}
