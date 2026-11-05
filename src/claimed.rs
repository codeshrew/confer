//! Claimed work: requests a role claimed and has not closed.
//!
//! Claiming a request marks it read, so it leaves `confer inbox` for good, and every reminder
//! confer printed (the watch footer, the session-start hook) counted unread mail only. An agent
//! with two claimed requests checked its inbox, saw it clear, and took that to mean there was
//! nothing to do (field report, 2026-10-30). These helpers let each of those places say what is
//! still on the agent's plate.

use crate::projection::{self, Board};
use crate::schema::Message;
use chrono::{DateTime, Utc};

/// How long a claim can sit without a word from its owner before the watch reminds them, and how
/// often it reminds. A claim being worked on usually gets a note; one that hasn't had any for this
/// long has probably been forgotten. `CONFER_CLAIM_NAG_SECS` overrides it (tests, tuning).
pub(crate) fn nag_every() -> std::time::Duration {
    let secs = std::env::var("CONFER_CLAIM_NAG_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(4 * 3600);
    std::time::Duration::from_secs(secs)
}

/// One request `me` owns and has not closed.
pub(crate) struct Claim {
    pub id: String,
    pub summary: String,
    pub deferred: bool,
    /// Since the request was made.
    pub age_secs: i64,
    /// Since `me` last wrote anything in its thread (the claim, a note, a reply).
    pub idle_secs: i64,
}

/// Requests whose current owner (first claimant) is `me` and that are still CLAIMED, oldest first.
/// A contested claim someone else won is theirs, not mine.
pub(crate) fn mine(msgs: &[Message], me: &str, now: DateTime<Utc>) -> Vec<Claim> {
    if me.is_empty() {
        return Vec::new();
    }
    let board = Board::fold(msgs, now);
    let owned: Vec<_> = board
        .rows
        .iter()
        .filter(|r| r.status == "CLAIMED" && r.claimants.first().map(String::as_str) == Some(me))
        .collect();
    if owned.is_empty() {
        return Vec::new();
    }
    // The last time I said anything in each owned thread.
    let mut last: std::collections::HashMap<&str, DateTime<Utc>> = std::collections::HashMap::new();
    for m in msgs.iter().filter(|m| m.front.from == me) {
        let Ok(ts) = DateTime::parse_from_rfc3339(&m.front.ts) else { continue };
        let root = projection::thread_root(msgs, m).front.id.as_str();
        if let Some(r) = owned.iter().find(|r| r.id == root) {
            let e = last.entry(r.id.as_str()).or_insert(ts.with_timezone(&Utc));
            *e = (*e).max(ts.with_timezone(&Utc));
        }
    }
    owned
        .into_iter()
        .map(|r| Claim {
            id: r.id.clone(),
            summary: r.summary.clone(),
            deferred: r.deferred,
            age_secs: r.age_secs,
            idle_secs: last
                .get(r.id.as_str())
                .map(|t| (now - *t).num_seconds().max(0))
                .unwrap_or(r.age_secs),
        })
        .collect()
}

/// One display line per claim, sanitized (summaries are peer-written).
pub(crate) fn line(c: &Claim) -> String {
    format!(
        "   {}  idle {:>4} · open {:>4}{} — {}",
        crate::short_id(&c.id),
        crate::inbox::fmt_age(c.idle_secs),
        crate::inbox::fmt_age(c.age_secs),
        if c.deferred { " ⏳" } else { "" },
        crate::schema::sanitize_term(&crate::truncate(&c.summary, 60), false)
    )
}

/// Print the watch's periodic reminder, if there is anything to remind about.
pub(crate) fn remind(root: &std::path::Path, me: &str, idle: std::time::Duration) {
    use std::io::Write;
    if let Some(b) = crate::store::all_messages(root).ok().and_then(|all| idle_block(&all, me, idle)) {
        let mut out = std::io::stdout().lock();
        let _ = write!(out, "{b}");
        let _ = out.flush();
    }
}

/// Claims with no word from `me` for at least `idle`, or None. A claim being worked on usually gets
/// a note, so only forgotten-looking ones wake the agent.
fn idle_block(msgs: &[Message], me: &str, idle: std::time::Duration) -> Option<String> {
    let claims = mine(msgs, me, Utc::now());
    let idle_secs = i64::try_from(idle.as_secs()).unwrap_or(i64::MAX);
    let quiet: Vec<&Claim> = claims.iter().filter(|c| c.idle_secs >= idle_secs).collect();
    (!quiet.is_empty()).then(|| block(&quiet, " (no word from you on these lately)"))
}

/// The block printed under `confer inbox` and by the watch: a header, up to five claims, a hint.
pub(crate) fn block(claims: &[&Claim], header: &str) -> String {
    let mut s = format!("── ⏳ {} claimed by you, not done{header} ──\n", claims.len());
    for c in claims.iter().take(5) {
        s.push_str(&line(c));
        s.push('\n');
    }
    if claims.len() > 5 {
        s.push_str(&format!("   (+{} more)\n", claims.len() - 5));
    }
    s.push_str(
        "   → `confer done <id>` when finished · `confer blocked <id>` if waiting · \
         `confer done <id> --as wont-do` to drop it\n",
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, ts: &str, from: &str, ty: &str, of: Option<&str>) -> Message {
        let front = serde_json::from_value(serde_json::json!({
            "id": id, "ts": ts, "from": from, "type": ty, "of": of, "summary": format!("{ty} {id}"),
        }))
        .unwrap();
        Message { front, body: String::new(), source_path: None }
    }

    #[test]
    fn only_requests_i_own_and_have_not_closed() {
        let now = DateTime::parse_from_rfc3339("2026-10-30T12:00:00Z").unwrap().with_timezone(&Utc);
        let msgs = vec![
            msg("01A", "2026-10-26T12:00:00Z", "bob", "request", None),
            msg("01B", "2026-10-26T13:00:00Z", "me", "claim", Some("01A")),
            msg("01C", "2026-10-29T12:00:00Z", "me", "note", Some("01A")),
            // claimed, then done: off my plate
            msg("01D", "2026-10-27T12:00:00Z", "bob", "request", None),
            msg("01E", "2026-10-27T13:00:00Z", "me", "claim", Some("01D")),
            msg("01F", "2026-10-27T14:00:00Z", "me", "done", Some("01D")),
            // someone else claimed first: theirs
            msg("01G", "2026-10-28T12:00:00Z", "bob", "request", None),
            msg("01H", "2026-10-28T13:00:00Z", "carol", "claim", Some("01G")),
            msg("01J", "2026-10-28T14:00:00Z", "me", "claim", Some("01G")),
            // open, unclaimed: not mine yet
            msg("01K", "2026-10-28T12:00:00Z", "bob", "request", None),
        ];
        let got = mine(&msgs, "me", now);
        assert_eq!(got.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["01A"]);
        assert_eq!(got[0].age_secs, 4 * 86400);
        assert_eq!(got[0].idle_secs, 86400, "idle counts from my last note in the thread, not the claim");
    }
}
