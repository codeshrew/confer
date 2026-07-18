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
///
/// Temporal-identity fields (design/44 §3), all additive/optional so old messages parse
/// unchanged and old binaries parse new refs (serde ignores unknown fields, and every new
/// field here is `Option`/defaulted). `sha` stays a required String — it's either full
/// 40/64-hex or the literal `"unresolved"` — so an old binary's non-full-hex → `Unpinned`
/// path (`refcode::staleness`) keeps rendering both legacy `HEAD` refs AND new `unresolved`
/// refs correctly with zero compat work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRef {
    pub repo: String,
    pub sha: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<[u64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// The one branch or tag name in play at capture (never a list — see design/44 §1.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_name: Option<String>,
    /// How the pinned commit was reached: `branch` | `tag` | `detached`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_type: Option<String>,
    /// `%cI` (strict ISO 8601, committer's own offset) of the pinned commit, stored verbatim —
    /// the timeline's load-bearing field (design/44 §5.2). Never lexically sort across offsets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_date: Option<String>,
    /// The referenced range had uncommitted working-tree changes at capture (only via
    /// `--allow-dirty`) — the body carries a `confer-ref` fence with what was actually seen.
    #[serde(default, skip_serializing_if = "is_false")]
    pub dirty: bool,
    /// The path wasn't in git at all at capture (implies `sha: "unresolved"` + a mandatory fence).
    #[serde(default, skip_serializing_if = "is_false")]
    pub untracked: bool,
    /// The raw rev token as typed (`HEAD`, a branch name, …), preserved intent — only present
    /// when `sha == "unresolved"` (never alongside a real pin).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
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

    #[test]
    fn code_ref_new_fields_round_trip() {
        // design/44 §3: the temporal-identity fields serialize and deserialize intact.
        let r = CodeRef {
            repo: "mylib".into(),
            sha: "a".repeat(40),
            path: "f.rs".into(),
            range: Some([1, 2]),
            content_hash: Some("b".repeat(40)),
            ref_name: Some("main".into()),
            ref_type: Some("branch".into()),
            commit_date: Some("2026-07-18T09:26:31-06:00".into()),
            dirty: true,
            untracked: false,
            rev: None,
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        let back: CodeRef = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.ref_name.as_deref(), Some("main"));
        assert_eq!(back.ref_type.as_deref(), Some("branch"));
        assert_eq!(back.commit_date.as_deref(), Some("2026-07-18T09:26:31-06:00"));
        assert!(back.dirty);
        assert!(!back.untracked);
        assert_eq!(back.rev, None);
    }

    #[test]
    fn legacy_ref_without_new_fields_parses_with_defaults() {
        // A pre-design/44 message's ref carries none of the new fields at all — must
        // still parse, with every addition defaulting (old messages parse unchanged).
        let yaml = "repo: mylib\nsha: HEAD\npath: f.rs\n";
        let r: CodeRef = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(r.sha, "HEAD");
        assert_eq!(r.ref_name, None);
        assert_eq!(r.ref_type, None);
        assert_eq!(r.commit_date, None);
        assert!(!r.dirty);
        assert!(!r.untracked);
        assert_eq!(r.rev, None);
    }

    #[test]
    fn unresolved_sha_ref_parses_like_any_other_ref() {
        // The new `sha: "unresolved"` marker (forced no-pin case) round-trips fine —
        // it's just a required string, same as any other value.
        let yaml = "repo: mylib\nsha: unresolved\npath: new.rs\nuntracked: true\nrev: HEAD\n";
        let r: CodeRef = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(r.sha, "unresolved");
        assert!(r.untracked);
        assert_eq!(r.rev.as_deref(), Some("HEAD"));
    }
}
