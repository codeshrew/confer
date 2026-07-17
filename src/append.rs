//! `append` and its lifecycle sugar verbs (`claim`/`done`/`error`/`blocked`/`defer`):
//! arg parsing, ref/range parsing, addressing/recipient advisories, and the send path.

use anyhow::{anyhow, Result};
use std::io::{IsTerminal, Read};

use crate::projection::claimants;
use crate::schema::{self, Frontmatter, Message, TYPES};
use crate::{
    config, gitcmd, groups, hint, id_matches, is_full_ulid, is_reserved_name, now, repos,
    resolve_unique, roster, secrets, short_id, store, truncate, valid_slug,
    warn_if_watch_should_be_live, CreateArgs, LifecycleArgs,
};

pub(crate) struct AppendArgs {
    pub(crate) msg_type: String,
    pub(crate) text: Option<String>,
    pub(crate) summary: String,
    pub(crate) to: Vec<String>,
    pub(crate) cc: Vec<String>,
    pub(crate) priority: Option<String>,
    pub(crate) topic: Option<String>,
    pub(crate) reply_to: Option<String>,
    pub(crate) of: Option<String>,
    pub(crate) supersedes: Option<String>,
    pub(crate) from: Option<String>,
    pub(crate) src: Option<String>,
    pub(crate) refs: Vec<String>,
    pub(crate) allow_empty_body: bool,
    pub(crate) resolution: Option<String>,
    pub(crate) defer: bool,
    /// override the secret-shape lint (post even if the body looks like it has a key).
    pub(crate) allow_secret: bool,
}

/// Parse a `--ref` token `repo:path[@sha][#Lstart-Lend]` into a CodeRef.
/// sha defaults to `HEAD` ("go look at latest"); pin a sha for a durable pointer.
fn parse_ref(s: &str) -> Result<schema::CodeRef> {
    let bad = || anyhow!("invalid --ref '{s}': expected repo:path[@sha][#Lstart-Lend]");
    let (repo, rest) = s.split_once(':').ok_or_else(bad)?;
    let (rest, range) = match rest.split_once('#') {
        Some((r, span)) => (r, Some(parse_range(span)?)), // malformed range → error, not silent drop
        None => (rest, None),
    };
    let (path, sha) = match rest.split_once('@') {
        Some((p, sha)) => (p, sha.to_string()),
        None => (rest, "HEAD".to_string()),
    };
    if repo.is_empty() || path.is_empty() {
        return Err(bad());
    }
    // The repo token keys into the `repos/<slug>.md` inventory — hold it to the
    // slug rule; and keep control chars out of the path (SEC1).
    if !valid_slug(repo) {
        return Err(anyhow!(
            "invalid --ref repo '{repo}': must be a repos/<slug> key ([a-z0-9][a-z0-9-]*)"
        ));
    }
    if path.chars().any(|c| c.is_control()) {
        return Err(anyhow!(
            "invalid --ref path '{path}': contains control characters"
        ));
    }
    Ok(schema::CodeRef {
        repo: repo.to_string(),
        sha,
        path: path.to_string(),
        range,
        content_hash: None,
    })
}

/// Parse `Lstart-Lend` (range) or `L46` / `46` (single line → `[n, n]`) into a line
/// range — errors (not silently drops) on a malformed or overflowing span, since the
/// ref would lose its span.
pub(crate) fn parse_range(span: &str) -> Result<[u64; 2]> {
    let bad = || anyhow!("invalid line range '{span}': expected Lstart-Lend or Lstart");
    match span.split_once('-') {
        Some((a, b)) => {
            let a = a.trim_start_matches('L').parse().map_err(|_| bad())?;
            let b = b.trim_start_matches('L').parse().map_err(|_| bad())?;
            Ok([a, b])
        }
        // A single line `#L46` — a legitimate, common reference (one line), not a
        // malformed range. Fold it to the degenerate range [n, n].
        None => {
            let n = span.trim_start_matches('L').parse().map_err(|_| bad())?;
            Ok([n, n])
        }
    }
}

/// Pin a `--ref`'s sha to an immutable full sha AT WRITE TIME, and record the
/// referenced blob's OID as `content_hash` when the repo is cloned here (design/40
/// #2 + #3). A code reference must be reproducible — we never persist a moving
/// `HEAD`/branch. Rules (per the Fable review):
///  - an explicit full-hex sha (40 or 64) is accepted as-is (you may reference code
///    you only have a sha for — no clone required); `content_hash` is filled in only
///    if a clone is mapped AND has the object.
///  - a symbolic sha (`HEAD`, a branch, a short sha) REQUIRES a local clone — resolved
///    via the machine-local repo map, validated against the card's `root_sha` — and is
///    resolved to a full sha there. Without a clone it's an error: we won't persist an
///    unpinnable ref.
fn resolve_and_pin_ref(repo_inv: &repos::Repos, r: &mut schema::CodeRef) -> Result<()> {
    let is_full_hex =
        (r.sha.len() == 40 || r.sha.len() == 64) && r.sha.chars().all(|c| c.is_ascii_hexdigit());
    let card_root_sha = repo_inv.get(&r.repo).and_then(|c| c.root_sha.clone());
    let clone = crate::repomap::resolve(&r.repo, card_root_sha.as_deref());

    if !is_full_hex {
        let Some(dir) = clone.as_ref() else {
            return Err(anyhow!(
                "cannot pin --ref {}:{}@{}: no local clone of '{}' is mapped on this machine, and a \
                 non-sha ref can't be made durable without one. Map a clone (`confer repos map {} <path>`), \
                 or pass an explicit full commit sha (`@<40-hex>`).",
                r.repo, r.path, r.sha, r.repo, r.repo
            ));
        };
        let spec = format!("{}^{{commit}}", r.sha);
        let o = gitcmd::output(dir, &["rev-parse", "--verify", "--quiet", &spec])?;
        if !o.status.success() {
            return Err(anyhow!(
                "cannot resolve --ref {}:{}@{} in the mapped clone {} (unknown revision)",
                r.repo, r.path, r.sha, dir.display()
            ));
        }
        r.sha = String::from_utf8_lossy(&o.stdout).trim().to_string();
    }

    // Best-effort content_hash: the blob OID of `<sha>:<path>`, when a clone has the
    // object. Lets staleness be a one-line comparison later (design/40 #5) that works
    // even if the pinned commit is GC'd/unfetched — you never need the commit to ask
    // "have these bytes changed?".
    if r.content_hash.is_none() {
        if let Some(dir) = clone.as_ref() {
            let spec = format!("{}:{}", r.sha, r.path);
            if let Ok(o) = gitcmd::output(dir, &["rev-parse", "--verify", "--quiet", &spec]) {
                if o.status.success() {
                    let oid = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if !oid.is_empty() {
                        r.content_hash = Some(oid);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Warn (non-fatally) when a message's addressees can't receive it in THIS hub:
/// a named `--to`/`--cc` role that hasn't joined, or a broadcast/group that
/// resolves to no one but the sender. This is the guardrail for the split-brain
/// footgun — an agent posting into the wrong repo/hub (e.g. the product repo
/// instead of the coordination hub), where its intended peers aren't present, so
/// the message is silently stranded. Deliberately a **warning**, not an error:
/// a role may legitimately join later, and leaving a note for an arriving agent
/// is a valid use — but the far more common cause is being in the wrong hub, and
/// naming the hub + who's actually joined makes that obvious. See DESIGN.md.
fn recipient_advisory(
    root: &std::path::Path,
    roster: &roster::Roster,
    grps: &groups::Groups,
    from: &str,
    to: &[String],
    cc: &[String],
    summary: &str,
) {
    // Nothing addressed → a topic-only post; there's no delivery claim to check.
    if to.is_empty() && cc.is_empty() {
        return;
    }
    let hub = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("this hub");
    let mut known: Vec<&str> = roster.keys().map(String::as_str).collect();
    known.sort_unstable();
    // Reachable peers = every joined role other than the sender.
    let has_other_peer = known.iter().any(|r| *r != from);

    let mut unknown: Vec<&str> = Vec::new(); // named roles that haven't joined
    let mut broadcast_empty = false; // `all`/group that reaches no one but you
    for t in to.iter().chain(cc.iter()) {
        if t == from {
            continue; // self-addressing is odd but not a delivery failure
        }
        if is_reserved_name(t) {
            // `all` — reaches every other joined role.
            broadcast_empty |= !has_other_peer;
        } else if let Some(members) = grps.get(t) {
            // a group — reachable if any member (other than you) has joined.
            broadcast_empty |= !members.iter().any(|m| m != from && roster.contains_key(m));
        } else if !roster.contains_key(t) {
            unknown.push(t);
        }
    }
    unknown.sort_unstable();
    unknown.dedup();
    if unknown.is_empty() && !broadcast_empty {
        return;
    }

    if !unknown.is_empty() {
        let joined = if known.is_empty() {
            "(none yet)".to_string()
        } else {
            known.join(", ")
        };
        let names = unknown
            .iter()
            .map(|r| format!("'{r}'"))
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!(
            "confer: warning — {} {names} {} not joined hub '{hub}'; they won't see this until they join. Joined roles: {joined}. If you expected them here, you may be in the wrong hub.",
            if unknown.len() == 1 { "role" } else { "roles" },
            if unknown.len() == 1 { "has" } else { "have" },
        );
    }
    if broadcast_empty {
        let s = truncate(summary, 60);
        eprintln!(
            "confer: warning — you are the only role in hub '{hub}'; no other agent will receive \"{s}\" until they join."
        );
    }
}

/// Ergonomic first-class lifecycle verbs (`confer claim/done/error/blocked/defer
/// --of <id>`) — thin sugar over `append` with the type set and a sensible default
/// summary, so closing/reclassifying a request is one short command.
pub(crate) fn cmd_lifecycle(
    msg_type: &str,
    a: LifecycleArgs,
    resolution: Option<String>,
) -> Result<()> {
    let default_summary = match (msg_type, resolution.as_deref()) {
        ("done", Some(r)) => r.to_string(),
        ("done", None) => "done".to_string(),
        ("claim", _) => "claiming".to_string(),
        ("error", _) => "failed".to_string(),
        ("blocked", _) => "blocked/waiting".to_string(),
        ("defer", _) => "deferred to backlog".to_string(),
        _ => msg_type.to_string(),
    };
    cmd_append(AppendArgs {
        msg_type: msg_type.to_string(),
        text: a.text, // optional body; summary-only still allowed (allow_empty_body)
        summary: a.summary.unwrap_or(default_summary),
        // Addressing passes straight through to append. Empty --to/--cc leaves
        // cmd_append to auto-address the request's author (via --of); an explicit
        // --to or --reply-to overrides that (append resolves the precedence).
        to: a.to,
        cc: a.cc,
        priority: None,
        topic: None,
        reply_to: a.reply_to,
        of: Some(a.of),
        supersedes: None,
        from: a.from,
        src: None,
        refs: a.refs, // the sugar verbs now carry --ref through to append (field report)
        allow_empty_body: true, // lifecycle markers are summary-only
        resolution,
        defer: false,
        allow_secret: false,
    })
}

/// Ergonomic first-class creation verbs (`confer request`/`note`) — thin sugar over
/// `append` with the type fixed, so opening a ticket or posting a chat message
/// doesn't require spelling out `--type`. note = chat, request = ticket; a request
/// may `--reply-to` a prior note to promote it into tracked work (the escalation
/// idiom) — `note` itself has no `reply_to` param since only `request` exposes it.
pub(crate) fn cmd_create(msg_type: &str, a: CreateArgs, reply_to: Option<String>) -> Result<()> {
    cmd_append(AppendArgs {
        msg_type: msg_type.to_string(),
        text: a.text,
        summary: a.summary,
        to: a.to,
        cc: a.cc,
        priority: a.priority,
        topic: a.topic,
        reply_to,
        of: None,
        supersedes: None,
        from: a.from,
        src: a.src,
        refs: a.refs,
        allow_empty_body: a.allow_empty_body,
        resolution: None,
        defer: a.defer,
        allow_secret: a.allow_secret,
    })
}

/// Split comma-lists inside repeated `--to`/`--cc` values (`--to a,b` == `--to a --to b`), trimming
/// and dropping empties — so a fleet can address a subset of peers in one flag instead of hitting the
/// slug regex on `a,b,c` (field report). Groups/`all` still work; this just pre-flattens.
fn split_comma_targets(v: Vec<String>) -> Vec<String> {
    v.into_iter()
        .flat_map(|s| s.split(',').map(str::trim).map(str::to_string).collect::<Vec<_>>())
        .filter(|s| !s.is_empty())
        .collect()
}

pub(crate) fn cmd_append(mut a: AppendArgs) -> Result<()> {
    // Accept `--to a,b,c` (and `--cc`) as a convenience for addressing several peers at once.
    a.to = split_comma_targets(a.to);
    a.cc = split_comma_targets(a.cc);
    let root = config::repo_root()?;
    let role = config::resolve_role(a.from, &root)?;
    // Surface a silently-dead watch on the next active command: if you armed a watch but it isn't
    // running (backgrounded/reaped), you're not being woken — say so now rather than let you go dark.
    warn_if_watch_should_be_live(&root, &role);

    if !TYPES.contains(&a.msg_type.as_str()) {
        return Err(anyhow!(
            "unknown --type '{}': expected one of {:?}",
            a.msg_type,
            TYPES
        ));
    }
    if let Some(p) = &a.priority {
        if !matches!(p.as_str(), "low" | "normal" | "high") {
            return Err(anyhow!(
                "invalid --priority '{p}': expected low | normal | high"
            ));
        }
    }
    let mut refs = a
        .refs
        .iter()
        .map(|s| parse_ref(s))
        .collect::<Result<Vec<_>>>()?;
    // Pin each --ref to an immutable full sha AT WRITE TIME + record its blob OID
    // (design/40 #2, #3) — a durable reference never stores a moving HEAD/branch.
    if !refs.is_empty() {
        let repo_inv = repos::load(&root);
        for r in refs.iter_mut() {
            resolve_and_pin_ref(&repo_inv, r)?;
        }
    }
    // A blank value counts as absent (an empty `--of`/`--supersedes` must not slip
    // past the required-field guard — see C1).
    let blank = |o: &Option<String>| o.as_deref().is_none_or(|s| s.trim().is_empty());
    // Imperative frontmatter contract: guarantee routing/triage metadata.
    if a.msg_type == "request" && a.to.is_empty() {
        return Err(anyhow!("--to <target> is required for type 'request'"));
    }
    if matches!(
        a.msg_type.as_str(),
        "claim" | "done" | "error" | "blocked" | "defer"
    ) && blank(&a.of)
    {
        return Err(anyhow!(
            "--of <request-id> is required for type '{}'",
            a.msg_type
        ));
    }
    if a.msg_type == "supersede" && blank(&a.supersedes) {
        return Err(anyhow!(
            "--supersedes <id> is required for type 'supersede'"
        ));
    }
    if a.summary.trim().is_empty() {
        return Err(anyhow!(
            "--summary must not be empty (it's the triage line peers read)"
        ));
    }
    // Resolution — only on a terminal `done`; validate the small vocab.
    // `done` is the default and stores nothing; the others record *why* it closed.
    let resolution = match a
        .resolution
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        None => None,
        Some(_) if a.msg_type != "done" => {
            return Err(anyhow!("--as <resolution> is only valid on --type done"));
        }
        Some("done") => None,
        Some(r @ ("wont-do" | "dropped" | "duplicate" | "obsolete")) => Some(r.to_string()),
        Some(other) => {
            return Err(anyhow!(
                "invalid --as '{other}': expected wont-do | duplicate | obsolete"
            ));
        }
    };
    if a.defer && a.msg_type != "request" {
        return Err(anyhow!(
            "--defer is only valid on --type request (it's a backlog marker)"
        ));
    }

    let topic = a.topic.unwrap_or_else(|| "general".to_string());

    // Slug validation (H2 — prevent path traversal / broken filenames).
    for (label, s) in [("role", role.as_str()), ("topic", topic.as_str())] {
        if !valid_slug(s) {
            return Err(anyhow!(
                "invalid {label} '{s}': must match [a-z0-9][a-z0-9-]* (≤64 chars)"
            ));
        }
        if is_reserved_name(s) {
            return Err(anyhow!(
                "'{s}' is reserved (the broadcast target) and can't be a {label}"
            ));
        }
    }
    for r in a.to.iter().chain(a.cc.iter()) {
        if !valid_slug(r) {
            return Err(anyhow!("invalid role '{r}': must match [a-z0-9][a-z0-9-]*"));
        }
    }

    // Resolve id references (--of/--supersedes/--reply-to) to canonical full ids
    // so lifecycle folding is exact. A blank value is treated as absent (guards
    // the empty-`of` whole-board fold); a fragment that matches no local message
    // fails loudly unless it is already a full ULID — never persist an ambiguous
    // fragment, which would fold by prefix onto sibling ids forever (C2).
    let all = store::all_messages(&root)?;
    let resolve = |label: &str, v: &Option<String>| -> Result<Option<String>> {
        let Some(raw) = v.as_ref() else {
            return Ok(None);
        };
        let s = raw.trim();
        if s.is_empty() {
            return Ok(None);
        }
        match resolve_unique(&all, s) {
            Ok(id) => Ok(Some(id.to_string())),
            Err(_) if is_full_ulid(s) => Ok(Some(s.to_string())), // canonical, just not fetched yet
            Err(_) if all.iter().any(|m| id_matches(&m.front.id, s)) => {
                Err(anyhow!("--{label} '{s}' is ambiguous; use the full id"))
            }
            Err(_) => Err(anyhow!(
                "--{label} '{s}' matches no known message; fetch it first or pass the full id"
            )),
        }
    };
    let of = resolve("of", &a.of)?;
    let supersedes = resolve("supersedes", &a.supersedes)?;
    let reply_to = resolve("reply-to", &a.reply_to)?;
    let mut to = a.to;
    if to.is_empty() && !matches!(a.msg_type.as_str(), "request") {
        if let Some(of_id) = &of {
            if let Some(req) = all.iter().find(|m| &m.front.id == of_id) {
                to = vec![req.front.from.clone()];
                // #5b (field report): closing a `--to all` request auto-addresses ONLY the author, so
                // the peers who actually responded to the broadcast don't get the resolution. Nudge
                // toward re-broadcasting when the request was a broadcast.
                if matches!(a.msg_type.as_str(), "done" | "error" | "blocked" | "defer")
                    && req.front.to.iter().any(|t| t == "all")
                {
                    hint(format!(
                        "this closes a `--to all` request — it reaches only the author ({}). Add `--to all` (or `--cc` the responders) if the peers who replied should hear it.",
                        req.front.from
                    ));
                }
            }
        }
    }
    // A reply with no explicit audience auto-addresses the author you're replying to
    // — so replying doesn't require `--cc all` (which wakes uninvolved roles). Peers
    // can still add more `--to`; this just makes the sane thing the default.
    if to.is_empty() && a.cc.is_empty() {
        if let Some(rt) = &reply_to {
            if let Some(orig) = all.iter().find(|m| &m.front.id == rt) {
                if orig.front.from != role {
                    // Replying to a peer → address that peer.
                    to = vec![orig.front.from.clone()];
                } else {
                    // Replying to YOUR OWN message in a thread → continue it to whoever THAT message
                    // addressed (minus yourself/`all`), so the reply doesn't go out unaddressed and
                    // wake nobody. (Field bug: a `--reply-to` pointing at your own thread post
                    // resolved to no audience, so the message never woke the participant.)
                    to = orig
                        .front
                        .to
                        .iter()
                        .filter(|t| t.as_str() != role && !is_reserved_name(t))
                        .cloned()
                        .collect();
                }
            }
        }
    }
    // Surface the silent "wakes nobody" case: a REPLY (`--reply-to`/`--of`) or a REQUEST that still
    // has NO audience reaches no inbox and wakes no peer — the exact trap where an addressing intent
    // resolved to no one. (A plain `note` with no `--to` is a deliberate board post; left alone.)
    if to.is_empty()
        && a.cc.is_empty()
        && (reply_to.is_some() || of.is_some() || a.msg_type == "request")
    {
        eprintln!(
            "confer: ⚠ this {} is addressed to NO ONE — it lands on the board but reaches no inbox \
             and wakes no peer. Add `--to <role>` (or `--to all`) so it's actually delivered.",
            if a.msg_type == "request" { "request" } else { "reply" }
        );
    }

    // Recipient-reachability advisory (guardrail against split-brain / wrong-hub
    // posting): warn if this targets a role that hasn't joined THIS hub, or `all`
    // resolves to just yourself. See DESIGN.md.
    let grps = groups::load(&root);
    recipient_advisory(
        &root,
        &roster::load(&root),
        &grps,
        &role,
        &to,
        &a.cc,
        &a.summary,
    );

    // Reference advisory (point-vs-carry): if a --ref points at a repo the
    // audience can't reach, they can't follow the pointer — nudge to inline the
    // content. Non-fatal; see DESIGN.md.
    if !refs.is_empty() {
        let inv = repos::load(&root);
        let audience: Vec<&str> = to.iter().chain(a.cc.iter()).map(String::as_str).collect();
        for r in &refs {
            match inv.get(&r.repo) {
                None => hint(format!(
                    "repo '{}' isn't registered; add repos/{}.md so peers know its role/access (confer repos).",
                    r.repo, r.repo
                )),
                Some(card) if !card.access.is_empty() => {
                    let to_all = audience.contains(&"all");
                    let blocked: Vec<&str> = audience
                        .iter()
                        .copied()
                        .filter(|t| *t != "all" && !grps.contains_key(*t) && !repos::accessible_to(card, t))
                        .collect();
                    if to_all || !blocked.is_empty() {
                        let who = if to_all {
                            "some recipients (you targeted `all`)".to_string()
                        } else {
                            blocked.join(", ")
                        };
                        hint(format!(
                            "repo '{}' isn't accessible to {who}; they can't follow this pointer. Consider inlining the key content (condensed) so the message is self-contained.",
                            r.repo
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    // Body: --text, else stdin (multi-line / fenced Markdown). A literal
    // `--text -` means "read stdin" (Unix convention) — not the body text "-";
    // taking it literally silently wrote a bare "-" body and dropped real detail.
    let mut body = match a.text {
        Some(t) if t == "-" => String::new(),
        Some(t) => t,
        None => String::new(),
    };
    if body.is_empty() && !std::io::stdin().is_terminal() {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        body = s.trim_end().to_string();
    }
    // Fail loud on an empty / lone-sentinel body — the silent `-`/empty-body data
    // loss the fleet hit (a review finding). A genuine
    // summary-only note must opt in with --allow-empty-body — EXCEPT lifecycle
    // markers (claim/done/error/supersede), where the summary IS the payload, so
    // requiring a body just discourages closing requests.
    let lifecycle = matches!(
        a.msg_type.as_str(),
        "claim" | "done" | "error" | "supersede" | "blocked" | "defer"
    );
    if !a.allow_empty_body && !lifecycle && matches!(body.trim(), "" | "-" | ".") {
        return Err(anyhow!(
            "refusing to send an empty message body (got {:?}) — pass --text \"…\" or pipe stdin; \
             use --allow-empty-body for an intentional summary-only note",
            body.trim()
        ));
    }

    // Secret-shape lint (a review finding): the log is permanent + fleet-wide, so a pasted
    // token/key would leak forever. Block on a match unless explicitly overridden.
    if !a.allow_secret {
        let findings = secrets::scan(&format!("{}\n{body}", a.summary));
        if !findings.is_empty() {
            return Err(anyhow!(
                "refusing to send — the message looks like it contains a secret: {}. \
                 The hub history is permanent and cloned by every agent. Remove it, or pass \
                 --allow-secret if this is a false positive.",
                secrets::summarize(&findings)
            ));
        }
    }

    // Terminal-control lint (Fable review): a body/summary with raw ANSI/C0 escapes can
    // rewrite a reading agent's terminal, forge a fake envelope, or hide text. Render is
    // sanitized defensively (schema::sanitize_term), but block it at the source too so a
    // fleet message never carries them. `\n`/`\t` are fine in a body; the summary is a
    // one-liner so no control chars at all.
    let ctrl_body = body
        .chars()
        .find(|&c| c != '\n' && c != '\t' && c.is_control());
    if let Some(c) = ctrl_body {
        return Err(anyhow!(
            "refusing to send — the body contains a control character (U+{:04X}). \
             Strip terminal escape/control sequences; only newlines and tabs are allowed.",
            c as u32
        ));
    }
    if let Some(c) = a.summary.chars().find(|c| c.is_control()) {
        return Err(anyhow!(
            "refusing to send — the --summary contains a control character (U+{:04X}); \
             it must be a single clean line.",
            c as u32
        ));
    }

    let id = ulid::Ulid::new().to_string();
    let ts = now();
    let msg = Message {
        front: Frontmatter {
            id: id.clone(),
            from: role.clone(),
            msg_type: a.msg_type,
            ts: ts.clone(),
            host: config::hostname(),
            to,
            cc: a.cc,
            priority: a.priority,
            topic: Some(topic.clone()),
            reply_to,
            of,
            supersedes,
            resolution,
            defer: a.defer,
            via: None,
            src: a.src,
            summary: Some(a.summary),
            refs,
        },
        body,
    };

    let path = store::message_path(&root, &topic, &id, &role, &ts);
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(&path, msg.to_markdown()?)?;

    // Send receipt (stderr) so the sender SEES the body size immediately — a
    // 0-char body is now impossible, but the receipt makes content visible and
    // pairs with the drift/version checks.
    let synced = match gitcmd::commit_and_sync(
        &root,
        &role,
        &path,
        &format!("{role}: {} {}", msg.front.msg_type, id),
        config::signing_key(&root).is_some(),
    ) {
        // Pushed — nudge co-resident watchers instantly (they notify-watch this).
        Ok(gitcmd::Committed::Synced) => {
            config::touch_signal(&config::hub_key(&root));
            true
        }
        // Committed locally, push deferred — the message is SAFE and flushes on next sync.
        Ok(gitcmd::Committed::DeferredLocal) => {
            eprintln!(
                "confer: committed locally, hub push deferred; flushes on the next confer command"
            );
            false
        }
        // NOT committed (e.g. the clone was busy). Remove the orphaned working-tree file and
        // FAIL LOUDLY — never report "sent" for a message that didn't land (a review finding: a
        // backgrounded append must exit non-zero so the caller knows it did not go out).
        Err(e) => {
            let _ = std::fs::remove_file(&path);
            return Err(anyhow!(
                "did NOT send {} — not committed ({e}); the clone may be busy. Retry, e.g. `timeout 60 confer append …`.",
                short_id(&id)
            ));
        }
    };
    eprintln!(
        "confer: sent {} ({} type, summary {} chars, body {} chars){}",
        short_id(&id),
        msg.front.msg_type,
        msg.front.summary.as_deref().unwrap_or("").chars().count(),
        msg.body.chars().count(),
        if synced {
            ""
        } else {
            " [NOT synced — committed locally]"
        }
    );

    // Claim-race check: on a broadcast request two agents can both
    // claim. Resolution is by fold order — the earliest claim owns. After sync
    // (which pulls in any racing claim), warn the loser so they yield instead of
    // doing duplicate work, rather than both silently proceeding.
    if msg.front.msg_type == "claim" {
        if let Some(req) = &msg.front.of {
            if let Ok(after) = store::all_messages(&root) {
                let cs = claimants(&after, req);
                if cs.len() > 1 && cs.first().map(String::as_str) != Some(role.as_str()) {
                    eprintln!(
                        "confer: ⚠ contested claim — '{}' already claimed {} (owns by fold order). \
                         Yield (append a note and stand down) or coordinate to avoid duplicate work.",
                        cs[0],
                        short_id(req)
                    );
                }
            }
        }
    }
    println!("{id}"); // machine-readable id on stdout regardless of sync outcome
    if !synced {
        // Non-zero exit so a hook/loop can distinguish committed-locally from
        // reached-the-hub (audit S2) — the id above still identifies the message.
        return Err(anyhow!(
            "message {} committed locally but not synced to the hub",
            short_id(&id)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comma_targets_split_trim_and_drop_empties() {
        // `--to a,b,c` == `--to a --to b --to c`; trims whitespace and drops empties (field report).
        assert_eq!(split_comma_targets(vec!["a,b,c".into()]), vec!["a", "b", "c"]);
        assert_eq!(split_comma_targets(vec!["a".into(), "b, c".into()]), vec!["a", "b", "c"]);
        assert_eq!(split_comma_targets(vec!["a,,".into(), "".into()]), vec!["a"]);
        assert!(split_comma_targets(vec![]).is_empty());
        // a plain single target is unchanged
        assert_eq!(split_comma_targets(vec!["all".into()]), vec!["all"]);
    }

    #[test]
    fn parse_ref_handles_repo_path_sha_and_range() {
        let r = parse_ref("proj:docs/spec.md@6c513dca").unwrap();
        assert_eq!(r.repo, "proj");
        assert_eq!(r.path, "docs/spec.md");
        assert_eq!(r.sha, "6c513dca");
        assert_eq!(r.range, None);
        // sha defaults to HEAD when omitted
        let d = parse_ref("proj:docs/spec.md").unwrap();
        assert_eq!(d.sha, "HEAD");
        // line range, with and without the L prefix
        let ranged = parse_ref("app:src/main.rs@abc#L10-L42").unwrap();
        assert_eq!(ranged.path, "src/main.rs");
        assert_eq!(ranged.sha, "abc");
        assert_eq!(ranged.range, Some([10, 42]));
        // single-line ref (#L46) → degenerate range [46, 46]
        let one = parse_ref("app:src/main.rs@abc#L46").unwrap();
        assert_eq!(one.range, Some([46, 46]));
        // malformed → error, not panic
        assert!(parse_ref("no-colon").is_err());
        assert!(parse_ref("repo:").is_err());
        assert!(parse_ref(":path").is_err());
    }

    #[test]
    fn parse_range_errors_on_malformed() {
        assert_eq!(parse_range("10-42").unwrap(), [10, 42]);
        assert_eq!(parse_range("L10-L42").unwrap(), [10, 42]);
        // single line (#L46 / #46) → the degenerate range [n, n], not an error
        assert_eq!(parse_range("46").unwrap(), [46, 46]);
        assert_eq!(parse_range("L46").unwrap(), [46, 46]);
        assert!(parse_range("L10-Lx").is_err()); // nonnumeric
        assert!(parse_range("Lx").is_err()); // nonnumeric single
        assert!(parse_range("").is_err()); // empty
        assert!(parse_range("99999999999999999999-2").is_err()); // overflow
    }
}
