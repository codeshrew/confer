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

    // An unfiltered poll consumes the whole actionable stream, so it's caught up
    // to HEAD; non-actionable notes remain browsable via `read`/`--all` (B1).
    if p.advance {
        // Anchor at the last stable pushed ancestor of HEAD, not local HEAD (R3).
        if let Some(anchor) = gitcmd::cursor_anchor(&root) {
            cursor::save(&hub, &me, &anchor)?;
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
