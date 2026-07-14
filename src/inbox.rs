//! The **read frontier** — the last message this (hub, role) has genuinely
//! CONSUMED, distinct from the delivery cursor (what the watch has *streamed*).
//!
//! Emit ≠ read. The watch advancing its delivery cursor on emit does NOT mark mail
//! read — otherwise "seen" means "delivered once," and a resolution the agent's
//! session missed (compaction, a dropped wake, an unopened body) is silently lost.
//! This frontier advances only on real consumption (`inbox`/`ack`/`show`/`thread`/an
//! unfiltered `poll`), so the watch can re-surface directly-addressed mail until the
//! agent actually reads it. ULID high-water-mark (ids are time-sortable); local-only,
//! never stored in the repo. See DESIGN.md and the watch notes.

use crate::config;
use crate::groups::{self, Groups};
use crate::schema::Message;
use anyhow::Result;
use std::path::PathBuf;

fn path(hub_key: &str, role: &str) -> Result<PathBuf> {
    let role = if role.is_empty() { "_" } else { role };
    Ok(config::home()?.join(".confer").join("inbox").join(hub_key).join(format!("{role}.json")))
}

/// The last CONSUMED message id (ULID), if any.
pub fn load(hub_key: &str, role: &str) -> Option<String> {
    let txt = std::fs::read_to_string(path(hub_key, role).ok()?).ok()?;
    serde_json::from_str::<serde_json::Value>(&txt)
        .ok()?
        .get("read")
        .and_then(|c| c.as_str())
        .map(String::from)
}

fn save(hub_key: &str, role: &str, id: &str) -> Result<()> {
    let p = path(hub_key, role)?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&p, serde_json::json!({ "read": id }).to_string())?;
    Ok(())
}

/// Advance the read frontier to `id` — but only forward (high-water-mark). Consuming
/// an older message never un-reads newer mail. No-op (Ok) if `id` isn't past the
/// current frontier. Best-effort persistence.
pub fn advance(hub_key: &str, role: &str, id: &str) -> Result<()> {
    if role.is_empty() || id.is_empty() {
        return Ok(());
    }
    match load(hub_key, role) {
        Some(cur) if cur.as_str() >= id => Ok(()), // already past it
        _ => save(hub_key, role, id),
    }
}

/// Directly-addressed mail I haven't consumed yet: messages where I'm a literal `to`
/// recipient (or a member of a group in `to`) — NOT cc, NOT pure `all` broadcasts —
/// with an id strictly past my read frontier, excluding my own. Time-sorted (by id).
/// This is the set the watch nags about, so a resolution/answer re-surfaces until read.
pub fn unread_for_me<'a>(
    msgs: &'a [Message],
    me: &str,
    groups: &Groups,
    frontier: Option<&str>,
) -> Vec<&'a Message> {
    let mut out: Vec<&Message> = msgs
        .iter()
        .filter(|m| {
            m.front.from != me
                && groups::directly_addressed(m, me, groups)
                && frontier.is_none_or(|f| m.front.id.as_str() > f)
        })
        .collect();
    out.sort_by(|a, b| a.front.id.cmp(&b.front.id));
    out
}

/// The newest message id in the log (the "caught up to here" mark for a bulk read).
pub fn latest_id(msgs: &[Message]) -> Option<String> {
    msgs.iter().map(|m| m.front.id.clone()).max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Frontmatter;

    fn msg(id: &str, from: &str, to: &[&str]) -> Message {
        Message {
            front: Frontmatter {
                id: id.into(),
                from: from.into(),
                msg_type: "note".into(),
                ts: "2026-07-11T00:00:00Z".into(),
                host: None,
                to: to.iter().map(|s| s.to_string()).collect(),
                cc: vec![],
                priority: None,
                topic: None,
                reply_to: None,
                of: None,
                supersedes: None,
                resolution: None,
                defer: false,
                via: None,
                src: None,
                summary: Some("s".into()),
                refs: vec![],
            },
            body: String::new(),
        }
    }

    #[test]
    fn unread_is_direct_mail_past_the_frontier_excluding_broadcasts_and_self() {
        let g = Groups::new();
        let msgs = vec![
            msg("01A", "bob", &["alice"]),  // direct to me, old
            msg("01B", "bob", &["all"]),       // broadcast — not personal mail
            msg("01C", "carol", &["alice"]),   // direct to me, newer
            msg("01D", "alice", &["bob"]),   // my own — never my unread
            msg("01E", "bob", &["carol"]),    // to someone else
        ];
        // No frontier → all direct-to-me mail is unread (01A, 01C); not 01B/01D/01E.
        let u = unread_for_me(&msgs, "alice", &g, None);
        assert_eq!(u.iter().map(|m| m.front.id.as_str()).collect::<Vec<_>>(), vec!["01A", "01C"]);
        // Frontier at 01A → only 01C remains unread (strictly past the frontier).
        let u = unread_for_me(&msgs, "alice", &g, Some("01A"));
        assert_eq!(u.iter().map(|m| m.front.id.as_str()).collect::<Vec<_>>(), vec!["01C"]);
        // Frontier at the latest → inbox clear.
        assert!(unread_for_me(&msgs, "alice", &g, Some("01C")).is_empty());
    }
}
