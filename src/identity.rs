//! Identity / roster-card commands: `who`, `identity`, `whois`, `rename`, `describe`,
//! and the dormant/retired/resume status setter. Moved verbatim from `main.rs`.

use crate::append::{cmd_append, AppendArgs};
use crate::{alias, config, crosshub, gitcmd, presence, projection, roster, schema, store, verify};
use crate::warn_safety;
use anyhow::{anyhow, Result};
use std::collections::HashMap;

/// `--json` mirrors the text columns — role/display/desc/host/expected_host/liveness/
/// last_posted/aliases/card-trust/self-declared status/cross-hub appearances — as a JSON ARRAY
/// (one snapshot object per role, not an NDJSON stream: `who` isn't a message feed). `[]` when
/// there are no roles yet (design/37 item 6/11).
pub(crate) fn cmd_who(json: bool) -> Result<()> {
    let root = config::repo_root()?;
    let roster = roster::load(&root);
    let msgs = store::all_messages(&root)?;
    // Self-register this hub + build the cross-hub pubkey index (F3): a peer whose
    // published key also appears in another hub you've joined is the same agent.
    if let Ok(me) = config::resolve_role(None, &root) {
        crosshub::record(&root, &me);
    }
    let xhub = crosshub::appearances(&root);

    // Liveness: fetch refs/presence/* on demand, VERIFY each
    // heartbeat against the role's pinned key, and reject a non-monotonic beat. A forged or
    // replayed/suppressed heartbeat (Untrusted) is dropped — the agent then renders as aged-out
    // rather than trusting a lie — and its role is surfaced in a warning line.
    let now = chrono::Utc::now();
    let hub_key = config::hub_key(&root);
    let beats = presence::load_verified(&root, &hub_key, &roster, true);
    let untrusted: Vec<String> = beats
        .iter()
        .filter(|b| !b.trust.ok())
        .map(|b| b.p.role.clone())
        .collect();
    let pres: HashMap<String, presence::Presence> = beats
        .into_iter()
        .filter(|b| b.trust.ok())
        .map(|b| (b.p.role.clone(), b.p))
        .collect();

    let rows = projection::agents(&msgs, &roster, &pres, &xhub);

    if json {
        let mut vc = verify::Cache::default();
        let mut arr = Vec::with_capacity(rows.len());
        for a in &rows {
            let ct = verify::card_trust(&root, &hub_key, &roster, &mut vc, &a.id);
            let live = matches!(&a.presence, Some(p) if presence::liveness(p, now) == presence::Live::Up);
            let liveness = a.presence.as_ref().map(|p| match presence::liveness(p, now) {
                presence::Live::Up => "up",
                presence::Live::Stale => "stale",
                presence::Live::Down => "down",
            });
            let status = if matches!(ct, verify::Trust::Verified { .. }) {
                roster.get(&a.id).and_then(|r| r.status.clone())
            } else {
                None
            };
            let aliases: Vec<&str> = roster
                .get(&a.id)
                .map(|r| r.aliases.iter().map(String::as_str).collect())
                .unwrap_or_default();
            arr.push(serde_json::json!({
                "role": a.id,
                "display": a.display,
                "desc": a.desc,
                "host": a.last_host,
                "expected_host": a.expected_host,
                "live": live,
                "liveness": liveness,
                "last_posted": a.last_ts,
                "aliases": aliases,
                "trust": { "status": ct.status_str(), "detail": ct.tag() },
                "status": status,
                "xhub": a.xhub.iter().map(|(h, r)| serde_json::json!({"hub": h, "role": r})).collect::<Vec<_>>(),
            }));
        }
        println!("{}", serde_json::to_string(&serde_json::Value::Array(arr))?);
        return Ok(());
    }

    if rows.is_empty() {
        // Empty result: text-mode prose moves to stderr (item 11) — `who` is a command an agent
        // shells out to for a live roster, and stdout must stay a clean payload stream.
        eprintln!("no roles yet (add roles.toml or have agents post).");
    }
    // Card-trust: a role card's fields are only as trustworthy as the signature on
    // its latest edit. Every line carries a trust glyph (· unverified · ✓ verified · ‼ mismatch)
    // — so an UNVERIFIED card (whose peer-writable fields a hub writer could have forged) is
    // never visually indistinguishable from a signed one. ALL card-derived text
    // (display/desc/host/status) is terminal-sanitized: a peer body must not be able to rewrite
    // the reader's terminal, and `who`/`whois` were bypassing that.
    let mut vc = verify::Cache::default();
    let mut any_unverified = false;
    let mut any_firstsight = false;
    for a in &rows {
        let disp = schema::sanitize_term(&a.display, false);
        let about = a
            .desc
            .as_deref()
            .map(|d| format!(" — {}", schema::sanitize_term(d, false)))
            .unwrap_or_default();
        let expected = a
            .expected_host
            .as_deref()
            .map(|h| format!(" (expected on {})", schema::sanitize_term(h, false)))
            .unwrap_or_default();
        let seen = match (&a.last_ts, &a.last_host) {
            (Some(t), Some(host)) => {
                format!("last posted {t} on {}", schema::sanitize_term(host, false))
            }
            (Some(t), None) => format!("last posted {t}"),
            _ => "no messages".to_string(),
        };
        let live = agent_liveness_prefix(a, now);
        let xh = if a.xhub.is_empty() {
            String::new()
        } else {
            format!(
                "  ≡ {}",
                a.xhub
                    .iter()
                    .map(|(l, r)| format!("{l}:{r}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let ct = verify::card_trust(&root, &hub_key, &roster, &mut vc, &a.id);
        let tg = ct.glyph(); // ·/✓/⚠/‼ — same vocabulary as the message feed
        match ct {
            verify::Trust::Unverified { .. } => any_unverified = true,
            verify::Trust::FirstSight { .. } => any_firstsight = true,
            _ => {}
        }
        let cmark = match &ct {
            verify::Trust::Mismatch { .. } => {
                "  ‼ CARD KEY MISMATCH — this card was re-keyed; do not trust its name/host/desc"
                    .to_string()
            }
            _ => String::new(),
        };
        // Honor a self-declared status ONLY when the card edit is verified (self-sovereign);
        // otherwise treat as active. It overlays the heartbeat: a dormant/retired
        // agent that's Down reads as intentional (not a crash alarm); one still heartbeating is
        // flagged as a zombie.
        let status = if matches!(ct, verify::Trust::Verified { .. }) {
            roster
                .get(&a.id)
                .and_then(|r| r.status.as_deref())
                .filter(|s| *s != "active")
        } else {
            None
        };
        let smark = match status {
            Some(s) => {
                let s = schema::sanitize_term(s, false);
                let beating = matches!(&a.presence, Some(p) if presence::liveness(p, now) == presence::Live::Up);
                if beating {
                    format!("  ⟨{s}⟩ ⚠ still heartbeating")
                } else {
                    format!("  ⟨{s}⟩")
                }
            }
            None => String::new(),
        };
        println!(
            "{live}{tg} {disp}{about} [{}]{expected}{xh} — {seen}{smark}{cmark}",
            a.id
        );
    }
    if any_firstsight {
        println!("  (⚠ = first-sight key, signed but NOT yet confirmed out-of-band — check the fingerprint, then `confer confirm-key <role>`)");
    }
    if any_unverified {
        println!("  (· = card not cryptographically verified — treat its name/desc/host as advisory; ✓ = signed by the pinned key)");
    }
    if !untrusted.is_empty() {
        println!(
            "  ‼ presence REJECTED for: {} — a forged/replayed heartbeat (unsigned-but-pinned, wrong key, or timestamp went backwards). Their liveness is shown as aged-out, not trusted.",
            untrusted.join(", ")
        );
    }
    Ok(())
}

/// The `●/○/✕ word (hb HH:MM) · ` liveness prefix for an agent (shared by `who`
/// and the dashboard). Two spaces when the agent has published no heartbeat.
fn agent_liveness_prefix(a: &projection::AgentRow, now: chrono::DateTime<chrono::Utc>) -> String {
    match &a.presence {
        Some(p) => {
            let l = presence::liveness(p, now);
            let hb = p.last_seen.get(11..16).unwrap_or(&p.last_seen);
            let word = match l {
                presence::Live::Up => "watching",
                presence::Live::Stale => "idle",
                presence::Live::Down => "down",
            };
            format!("{} {word} (hb {hb}) · ", presence::glyph(&l))
        }
        None => "  ".to_string(),
    }
}

/// Show this agent's cross-hub identity: its signing-key fingerprint and the other
/// hubs where the SAME key appears (F3 recognition; docs/06).
pub(crate) fn cmd_identity(role: Option<String>) -> Result<()> {
    let root = config::repo_root()?;
    let me = config::resolve_role(role, &root)?;
    crosshub::record(&root, &me); // self-register this hub
    let roster = roster::load(&root);
    let this = crosshub::hub_label(&root);
    match roster::pubkey(&roster, &me) {
        None => {
            println!("{me} @ {this}: no signing key published — cross-hub recognition needs a signed identity (`join --signing-key <ssh-key>`).");
        }
        Some(pk) => {
            println!("You are {} — {me} @ {this}", crosshub::fingerprint(pk));
            let idx = crosshub::appearances(&root);
            match idx.get(pk) {
                Some(apps) if !apps.is_empty() => {
                    for (label, rid) in apps {
                        println!("  ≡ also {label}:{rid} (same key)");
                    }
                }
                _ => println!("  (not yet recognized in any other hub you've joined)"),
            }
        }
    }
    Ok(())
}

/// Resolve a loose human phrase to a role (fuzzy over id/display/desc/aliases/host).
pub(crate) fn cmd_whois(phrase: String) -> Result<()> {
    let root = config::repo_root()?;
    let roster = roster::load(&root);
    let matches = alias::resolve(&roster, &phrase);
    if matches.is_empty() {
        println!("no role matches \"{phrase}\". Try `confer who`, or teach it: the agent runs `confer describe --add-alias \"{phrase}\"`.");
        return Ok(());
    }
    // A name resolves via the card's display/aliases — which a hub writer could have rewritten to
    // redirect a phrase to an impostor. If the card was re-keyed vs the pin, say so loudly so the
    // human doesn't trust the redirection.
    let hub_key = config::hub_key(&root);
    let mut vc = verify::Cache::default();
    for (i, m) in matches.iter().take(4).enumerate() {
        let disp = schema::sanitize_term(roster::display(&roster, &m.id), false);
        let about = roster
            .get(&m.id)
            .and_then(|r| r.desc.as_deref())
            .map(|d| format!(" — {}", schema::sanitize_term(d, false)))
            .unwrap_or_default();
        let warn = match verify::card_trust(&root, &hub_key, &roster, &mut vc, &m.id) {
            verify::Trust::Mismatch { .. } => "  ‼ this card was RE-KEYED — the name/desc may be an impostor's; verify out-of-band before trusting".to_string(),
            verify::Trust::FirstSight { .. } => "  ⚠ first-sight key — confirm out-of-band (`confer confirm-key`) before trusting this name".to_string(),
            verify::Trust::Unverified { .. } => "  (· unverified card — name/desc advisory)".to_string(),
            verify::Trust::Verified { .. } => String::new(),
        };
        println!(
            "{} {disp} [{}]{about}{warn}",
            if i == 0 { "→" } else { " " },
            m.id
        );
    }
    Ok(())
}

/// Parse a role card into (frontmatter mapping, freeform body). FAILS CLOSED: if a `---`-fenced
/// frontmatter block is present but does NOT parse as YAML, return Err instead of silently
/// degrading to an empty map. Every caller here is a key/metadata WRITE path, and a corrupt card
/// that reads as "empty" would be a security hole: (a) it bypasses the write-side 1:1 key guard,
/// letting a hub writer re-key a role by first committing ONE malformed frontmatter line — a silent
/// identity hijack (red-team CRITICAL); and (b) it lets a self-mutation (`describe`/`rename`/
/// `set-status`) overwrite the card, destroying display/host/aliases/status with no signal. A card
/// with NO frontmatter fence, or an empty fence, is legitimately empty and returns Ok. (The lenient
/// READ path is `roster::parse_role`, which skip-and-warns; it deliberately does NOT share this.)
pub(crate) fn parse_card(raw: &str) -> Result<(serde_yaml::Mapping, String)> {
    // Strip a leading UTF-8 BOM before the fence-sniff. A BOM (common from Windows/Notepad/PowerShell
    // editors — cards are explicitly hand-editable) would otherwise make the first line `\u{FEFF}---`
    // fail the `== "---"` check, so a KEYED card would be misread as "no frontmatter" (Ok, empty map)
    // and slip past the re-key guard as key-less (red-team). Keep this identical to roster::parse_role.
    let raw = raw.strip_prefix('\u{FEFF}').unwrap_or(raw);
    let mut lines = raw.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Ok((serde_yaml::Mapping::new(), raw.to_string()));
    }
    let (mut yaml, mut body, mut in_body) = (String::new(), String::new(), false);
    for line in lines {
        if !in_body && line.trim_end() == "---" {
            in_body = true;
            continue;
        }
        let buf = if in_body { &mut body } else { &mut yaml };
        buf.push_str(line);
        buf.push('\n');
    }
    let map = if yaml.trim().is_empty() {
        serde_yaml::Mapping::new()
    } else {
        serde_yaml::from_str::<serde_yaml::Mapping>(&yaml).map_err(|e| {
            anyhow!(
                "role card frontmatter is not valid YAML ({e}) — refusing to read or modify a card \
                 in an unknown state (possible tampering, or a hand-edit that broke it). Inspect \
                 the roles/*.md card and fix or revert its frontmatter."
            )
        })?
    };
    Ok((map, body.trim_matches('\n').to_string()))
}

/// Update your own role card: description + aliases, with collision-checked adds.
/// Rename yourself: set a short, voice-friendly display name and register it as an alias
/// so the owner can refer to you by it. Sugar over `describe --display`; the role ID never
/// changes, so history/attribution stay stable.
pub(crate) fn cmd_rename(name: String, role: Option<String>, force: bool) -> Result<()> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(anyhow!("give a name: confer rename <name>"));
    }
    // Resolve who + the current display up front (needed for the alias-preserve and the
    // rename broadcast). Best-effort: if we can't resolve, fall through to describe.
    let (me, old) =
        match config::repo_root().and_then(|r| Ok((config::resolve_role(role.clone(), &r)?, r))) {
            Ok((me, root)) => (
                Some(me.clone()),
                schema::sanitize_term(roster::display(&roster::load(&root), &me), false),
            ),
            Err(_) => (None, String::new()),
        };
    // Register the new name AND keep the OLD display as an alias, so a name the owner has
    // been using still resolves after a rename (friendlier for voice — a review probe).
    let mut add = vec![name.to_lowercase()];
    if let Some(me) = &me {
        if !old.is_empty() && !old.eq_ignore_ascii_case(&name) && !old.eq_ignore_ascii_case(me) {
            add.push(old.to_lowercase());
        }
    }
    // Display = the name; aliases resolve via `confer whois`. Adds are collision-checked.
    cmd_describe(role, None, Some(name.clone()), add, vec![], force)?;

    // L3 — rename broadcast: announce to peers so LIVE agents refresh their
    // working memory immediately, plus a who-was-called-what audit trail. Only when the
    // display actually changed; best-effort (a rename still succeeds if the note can't send).
    if let Some(me) = &me {
        if !old.eq_ignore_ascii_case(&name) {
            let text = format!(
                "Peer rename: role {me} now displays as '{name}' (previous names still resolve as aliases). \
                 Resolve any peer reference with `confer whois <name>` at use — don't rely on a cached display name."
            );
            let note = AppendArgs {
                msg_type: "note".into(),
                text: Some(text),
                summary: format!("renamed: {me} now displays as '{name}'"),
                to: vec!["all".into()],
                cc: vec![],
                priority: None,
                topic: None,
                reply_to: None,
                of: None,
                supersedes: None,
                from: Some(me.clone()),
                src: None,
                refs: vec![],
                allow_empty_body: false,
                resolution: None,
                defer: false,
                allow_secret: false,
                ref_from: None,
                allow_dirty: false,
                patch: None,
                patch_repo: None,
                allow_large_patch: false,
            };
            if let Err(e) = cmd_append(note) {
                warn_safety(format!("renamed, but the peer broadcast failed ({e}) — peers still resolve you via `confer whois`."));
            }
        }
    }
    Ok(())
}

pub(crate) fn cmd_describe(
    role: Option<String>,
    desc: Option<String>,
    display: Option<String>,
    add_alias: Vec<String>,
    remove_alias: Vec<String>,
    force: bool,
) -> Result<()> {
    let root = config::repo_root()?;
    let me = config::resolve_role(role, &root)?;
    let card_path = root.join("roles").join(format!("{me}.md"));
    if !card_path.exists() {
        return Err(anyhow!(
            "no role card roles/{me}.md — join first: confer join --role {me}"
        ));
    }
    let _ = gitcmd::integrate(&root); // freshen the roster so collision checks see peers
    let roster = roster::load(&root);

    // Show current state when called with nothing to change.
    if desc.is_none() && display.is_none() && add_alias.is_empty() && remove_alias.is_empty() {
        let r = roster.get(&me);
        println!(
            "{me}: {} — {}",
            schema::sanitize_term(roster::display(&roster, &me), false),
            &schema::sanitize_term(r.and_then(|r| r.desc.as_deref()).unwrap_or("(no description)"), false)
        );
        let al = r.map(|r| r.aliases.clone()).unwrap_or_default();
        println!(
            "aliases: {}",
            if al.is_empty() {
                "(none)".into()
            } else {
                al.join(", ")
            }
        );
        return Ok(());
    }

    // Fail closed (`?`): refuse to mutate a card whose frontmatter won't parse, rather than
    // read it as empty and overwrite it — which would silently destroy display/host/aliases/status.
    let (mut map, body) = parse_card(&std::fs::read_to_string(&card_path)?)?;
    let mut changed = false;
    // Rename: set the display peers see. Guarded against homoglyph impersonation and,
    // unless --force, against colliding with another role's name.
    if let Some(d) = &display {
        let d = d.trim();
        if d.is_empty() {
            return Err(anyhow!("--display must not be empty"));
        }
        if alias::homoglyph_risk(d) {
            return Err(anyhow!(
                "display '{d}' mixes Latin with Cyrillic/Greek look-alikes (impersonation risk); use plain ASCII"
            ));
        }
        if !force {
            if let Some((who, s, why)) = alias::conflict(&roster, &me, d) {
                let owner = if who.is_empty() {
                    String::new()
                } else {
                    format!(" '{s}' ({} [{who}])", roster::display(&roster, &who))
                };
                return Err(anyhow!(
                    "display '{d}' {why}{owner}; pick another or pass --force"
                ));
            }
        }
        map.insert("display".into(), d.into());
        changed = true;
        println!("renamed {me} → display '{d}'");
    }
    if let Some(d) = &desc {
        if roster.get(&me).and_then(|r| r.desc.as_deref()) != Some(d.as_str()) {
            map.insert("desc".into(), d.clone().into());
            changed = true;
        }
    }
    let mut aliases: Vec<String> = map
        .get("aliases")
        .and_then(|v| v.as_sequence())
        .map(|s| {
            s.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    for rm in &remove_alias {
        let before = aliases.len();
        aliases.retain(|a| !a.eq_ignore_ascii_case(rm.trim()));
        if aliases.len() < before {
            println!("removed alias '{}'", rm.trim());
            changed = true;
        }
    }
    for add in &add_alias {
        let add = add.trim();
        if add.is_empty() || aliases.iter().any(|a| a.eq_ignore_ascii_case(add)) {
            continue;
        }
        if !force {
            if let Some((who, s, why)) = alias::conflict(&roster, &me, add) {
                if who.is_empty() {
                    eprintln!("confer describe: skipping alias '{add}' — {why}.");
                } else {
                    eprintln!(
                        "confer describe: skipping alias '{add}' — it {why} '{s}' ({} [{who}]). Use --force to add anyway.",
                        roster::display(&roster, &who)
                    );
                }
                continue;
            }
        }
        aliases.push(add.to_string());
        println!("added alias '{add}'");
        changed = true;
    }
    if !changed {
        println!("roles/{me}.md: nothing changed.");
        return Ok(());
    }
    if aliases.is_empty() {
        map.remove("aliases");
    } else {
        let seq: serde_yaml::Sequence = aliases
            .iter()
            .map(|a| serde_yaml::Value::String(a.clone()))
            .collect();
        map.insert("aliases".into(), serde_yaml::Value::Sequence(seq));
    }

    let yaml = serde_yaml::to_string(&map)?;
    let content = if body.trim().is_empty() {
        format!("---\n{yaml}---\n")
    } else {
        format!("---\n{yaml}---\n\n{}\n", body.trim())
    };
    std::fs::write(&card_path, content)?;
    let sign = config::signing_key(&root).is_some();
    match gitcmd::commit_and_sync(&root, &me, &card_path, "describe: update role card", sign) {
        Ok(gitcmd::Committed::Synced) => {
            config::touch_signal(&config::hub_key(&root));
            println!("updated roles/{me}.md");
        }
        Ok(gitcmd::Committed::DeferredLocal) => {
            println!("updated roles/{me}.md (committed locally; hub push deferred — flushes on the next confer command)");
        }
        // NOT committed — undo the edit so the card isn't left dirty (a review finding, 0.2.1).
        Err(e) => {
            let _ = gitcmd::check(&root, &["checkout", "--", &format!("roles/{me}.md")]);
            return Err(anyhow!(
                "did NOT update roles/{me}.md — not committed ({e}); the clone may be busy. Retry."
            ));
        }
    }
    Ok(())
}

/// Set your own lifecycle status — a self-sovereign, SIGNED edit of YOUR card.
/// `active` clears the field (the default); `dormant`/`retired` set it. Peers can't do this to
/// you: it's a card mutation, so `verify::card_trust` only honors it when signed by your pinned
/// key. Intent only — liveness/aging still come from the presence heartbeat.
pub(crate) fn cmd_set_status(role: Option<String>, value: &str) -> Result<()> {
    let root = config::repo_root()?;
    let me = config::resolve_role(role, &root)?;
    let card_path = root.join("roles").join(format!("{me}.md"));
    if !card_path.exists() {
        return Err(anyhow!(
            "no role card roles/{me}.md — join first: confer join --role {me}"
        ));
    }
    let _ = gitcmd::integrate(&root); // freshen the card first, so we edit HEAD's version (avoids a stale-card clobber/stuck-defer)
    // Fail closed (`?`): don't overwrite a card whose frontmatter won't parse.
    let (mut map, body) = parse_card(&std::fs::read_to_string(&card_path)?)?;
    let current = map
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("active");
    if current == value {
        println!("{me} is already {value}.");
        return Ok(());
    }
    if value == "active" {
        map.remove("status"); // active is the default — keep the card clean
    } else {
        map.insert("status".into(), value.into());
    }
    let yaml = serde_yaml::to_string(&map)?;
    let content = if body.trim().is_empty() {
        format!("---\n{yaml}---\n")
    } else {
        format!("---\n{yaml}---\n\n{}\n", body.trim())
    };
    std::fs::write(&card_path, content)?;
    let sign = config::signing_key(&root).is_some();
    if !sign {
        eprintln!(
            "warning: this clone has no signing key, so peers will NOT honor this status — a status \
             is only trusted when the card edit is signed by your pinned key. \
             Adopt a key: confer join --role {me} --signing-key <path>."
        );
    }
    match gitcmd::commit_and_sync(&root, &me, &card_path, &format!("status: {value}"), sign) {
        Ok(gitcmd::Committed::Synced) => {
            config::touch_signal(&config::hub_key(&root));
            println!("{me} → {value}");
        }
        Ok(gitcmd::Committed::DeferredLocal) => {
            println!("{me} → {value} (committed locally; hub push deferred — flushes on the next confer command)");
        }
        // NOT committed — undo the working-tree edit so we don't leave a dirty card that blocks a
        // later rebase or gets swept into an unrelated commit (a review finding, 0.2.1).
        Err(e) => {
            let _ = gitcmd::check(&root, &["checkout", "--", &format!("roles/{me}.md")]);
            return Err(anyhow!(
                "did NOT set status — not committed ({e}); the clone may be busy. Retry."
            ));
        }
    }
    Ok(())
}
