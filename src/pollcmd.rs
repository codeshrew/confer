//! `confer poll` — the non-Monitor incremental-read command handler. Filters new
//! messages against a cursor + audience/topic filters, prints them, and optionally
//! advances the delivery cursor. Pure command handler moved out of `main.rs` — see
//! CLAUDE.md's module taxonomy.

use crate::schema::{is_actionable, Message};
use crate::{config, cursor, gitcmd, groups, roster, store, tiers, verify};
use anyhow::{anyhow, Result};
use std::io::Write;

pub(crate) struct PollArgs {
    pub(crate) advance: bool,
    pub(crate) topic: Option<String>,
    pub(crate) hook: bool,
    pub(crate) json: bool,
    pub(crate) role: Option<String>,
    pub(crate) all: bool,
    pub(crate) to_me: bool,
    /// escape hatch (M1): advance the cursor PAST an in-history-but-unreadable message instead of
    /// holding for it — accept losing that one message when it's permanently gone.
    pub(crate) force: bool,
}

pub(crate) fn cmd_poll(p: PollArgs) -> Result<()> {
    let root = config::repo_root()?;
    let me = config::resolve_role(p.role.clone(), &root).unwrap_or_default();
    // If you armed a watch but it isn't live, a poll won't fix that — surface it (poll-only agents,
    // which never armed one, are not nagged; the check is gated on a prior watch).
    crate::warn_if_watch_should_be_live(&root, &me);
    // Fetch the hub first — otherwise the whole non-Monitor fallback is blind (B2).
    if let Err(e) = gitcmd::integrate(&root) {
        crate::warn_safety(format!("hub sync failed ({e}); showing local state"));
    }
    let hub = config::hub_key(&root);
    let roster = roster::load(&root);
    let since = cursor::load(&hub, &me)?;

    // A filtered/firehose view must not move the shared cursor (B1).
    let filtered = p.topic.is_some() || p.to_me || p.all;
    if p.advance && filtered {
        return Err(anyhow!(
            "--advance is only allowed on an unfiltered poll (filtered/firehose views must not move the shared cursor)"
        ));
    }

    // Commit-ordered incremental read: only messages added since the cursor.
    let grps = groups::load(&root);
    let msgs = store::messages_since(&root, since.as_deref())?;
    let new: Vec<&Message> = msgs
        .iter()
        .filter(|m| relevant(m, &me, &p, &grps))
        .collect();

    // Stop-hook mode reads STDERR on exit 2; normal mode writes stdout (M2).
    let mut out: Box<dyn Write> = if p.hook {
        Box::new(std::io::stderr())
    } else {
        Box::new(std::io::stdout())
    };
    let mut vc = verify::Cache::default();
    for m in &new {
        let line = if p.json {
            let t = verify::status(&root, &hub, &roster, &mut vc, m);
            let tier = tiers::get(&hub);
            crate::to_json(m, &t, tier, crate::screen_note(m, tier).as_deref())?
        } else {
            let t = verify::status(&root, &hub, &roster, &mut vc, m);
            crate::format_line(&roster, m, true, Some(&t))
        };
        writeln!(out, "{line}")?;
    }
    drop(out);

    // M6 (grok 9Q530H), field-demonstrated by codex: `watch` and `poll --advance` advance the SAME
    // delivery cursor. When a healthy watcher is running for this (hub, role) it consumes that cursor
    // as mail arrives, so a later poll truthfully reports NOTHING NEW while unread mail is waiting —
    // and the caller reads that empty result as "no news". codex followed both documented
    // instructions (hold a watch, poll each turn) and sat on four unread addressed messages while
    // poll said nothing. The empty case is the dangerous one, so say it loudest exactly then.
    let watcher_owns_cursor = matches!(
        crate::watchlock::classify(&crate::watchlock::inspect(&hub, &me, 90), crate::BUILD_SHA),
        crate::watchlock::WatchState::Healthy
    );
    if watcher_owns_cursor && !p.hook {
        if new.is_empty() {
            eprintln!(
                "confer: a healthy watcher for '{me}' is consuming this delivery stream, so an empty \n\
                 poll here does NOT mean an empty mailbox — the watcher already advanced past anything \n\
                 that arrived. Unread mail addressed to you: `confer inbox`."
            );
        } else {
            eprintln!(
                "confer: note — a watcher for '{me}' is also consuming this stream; these items may \n\
                 already have been delivered to it as wakes."
            );
        }
    }

    // An unfiltered poll consumes the whole actionable stream, so it's caught up
    // to HEAD; non-actionable notes remain browsable via `read`/`--all` (B1).
    if p.advance && watcher_owns_cursor && !p.force {
        // Don't fight the watcher for the cursor. Refusing the ADVANCE (not the read) keeps this a
        // useful report while removing the race, and `--force` preserves a deliberate manual drain.
        eprintln!(
            "confer: not advancing the cursor — the watcher owns it (re-run with --force to advance anyway)."
        );
    } else if p.advance {
        // Anchor at the last stable pushed ancestor of HEAD, not local HEAD (R3) — but never advance
        // PAST an in-history-but-unreadable message (M1 / grok PHEQ76): hold at the last fully-delivered
        // commit so it's retried, not silently skipped. `--force` overrides the hold (accept losing that
        // message and move on) — the operator escape hatch for a permanently-unreadable one.
        if let Some(anchor) = gitcmd::cursor_anchor(&root) {
            let (safe, undelivered) = store::safe_advance(&root, since.as_deref(), &anchor);
            if !undelivered.is_empty() {
                eprintln!(
                    "confer: ⚠ {} message(s) in history but unreadable in the tree — cursor {} (re-fetch, or re-run with --force to move past). [M1]",
                    undelivered.len(),
                    if p.force { "advanced anyway (--force)" } else { "HELD, not advanced past them" }
                );
            }
            let advance_to = if p.force { Some(anchor) } else { safe };
            if let Some(s) = advance_to {
                cursor::save(&hub, &me, &s)?;
            }
        }
        // NOTE: poll advances the DELIVERY cursor only — it does NOT mark directly-addressed mail
        // read. Delivery ≠ read: a request stays in your inbox until you `show`/`ack` it, so a
        // polling loop can't silently clear mail it merely streamed past (inbox.rs).
    }
    if p.hook && !new.is_empty() {
        // Claude Code Stop-hook protocol: exit 2 = block the stop, the payload (already on stderr in
        // hook mode) is fed to the model. Signalled via a marker so `main` sets the code — no mid-stack
        // process::exit. (design/37 — this is an ADAPTER contract, not confer's own exit scheme.)
        return Err(crate::StopHookBlock.into());
    }
    Ok(())
}

/// Is a message relevant to a poll/watch consumer, given its filters?
/// Surfaces actionable items AND anything addressed to me (role/group/`all`) —
/// a message directed at me must never be invisible.
fn relevant(m: &Message, me: &str, p: &PollArgs, groups: &groups::Groups) -> bool {
    m.front.from != me
        && p.topic
            .as_ref()
            .is_none_or(|t| m.front.topic.as_deref() == Some(t.as_str()))
        && (p.all || is_actionable(m) || groups::addressed(m, me, groups))
        && (!p.to_me || groups::addressed(m, me, groups))
}
