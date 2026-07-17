//! `confer join` — the signed-role onboarding command and its key/card plumbing.
//!
//! `cmd_join` clones-in-place is done by `init`; join proper mints/records the role's SIGNED card
//! (binding role ↔ ed25519 key 1:1), configures commit signing, and seeds the machine config +
//! hub-identity pin (design/35). The pubkey/card helpers here are the trust-critical write path;
//! several are pub(crate) because the reconnect / trust / roster / clonehome / keygen paths reuse them.

use crate::config_hub::{current_hub_name, short12};
use crate::identity::parse_card;
use crate::projection::request_status;
use crate::schema::{self, Message};
use crate::{
    alias, config, crosshub, gitcmd, groups, keyring, knownhubs, machineconfig, roster, store,
    tiers, verify,
};
use crate::{
    check_version, format_line, hint, is_reserved_name, now, ssh_keygen_path, valid_slug,
    warn_safety, warn_trust,
};
use anyhow::{anyhow, Result};

/// The public key (`ssh-… AAAA…`) for a signing key path: the `.pub` next to it,
/// or the path itself if it already is a public key.
pub(crate) fn read_pubkey(key: &std::path::Path) -> Result<String> {
    let pubpath = if key.extension().and_then(|e| e.to_str()) == Some("pub") {
        key.to_path_buf()
    } else {
        let mut s = key.as_os_str().to_os_string();
        s.push(".pub");
        std::path::PathBuf::from(s)
    };
    Ok(std::fs::read_to_string(&pubpath)
        .map_err(|e| anyhow!("cannot read public key {}: {e}", pubpath.display()))?
        .trim()
        .to_string())
}

/// Configure this clone to sign commits with the agent's key, overriding any
/// global signer. Returns the public key to publish in the role card.
pub(crate) fn configure_signing(root: &std::path::Path, key: &std::path::Path) -> Result<String> {
    if !key.exists() {
        return Err(anyhow!("signing key {} does not exist", key.display()));
    }
    let pubkey = read_pubkey(key)?;
    let keygen = ssh_keygen_path();
    let key_s = key.to_string_lossy();
    for (k, v) in [
        ("gpg.format", "ssh"),
        ("gpg.ssh.program", keygen.as_str()),
        ("user.signingkey", key_s.as_ref()),
        ("commit.gpgsign", "true"),
        ("rebase.gpgSign", "true"),
    ] {
        gitcmd::check(root, &["config", k, v])?;
    }
    Ok(pubkey)
}

/// Write `contents` to `path` atomically: write a sibling temp file, fsync it, then rename over the
/// target. A crash / OOM-kill / disk-full mid-write leaves the PREVIOUS file intact (or none),
/// never a half-written one — so a reader (e.g. the re-role guard, which must fail closed on a
/// corrupt identity) can trust the file is either the old valid state or the new one. Mirrors how
/// `tiers`/`presence`/`keyring` persist state; the pid-suffixed temp name avoids collisions.
fn write_atomic(path: &std::path::Path, contents: &str) -> Result<()> {
    use std::io::Write;
    let dir = path
        .parent()
        .ok_or_else(|| anyhow!("no parent dir for {}", path.display()))?;
    std::fs::create_dir_all(dir)?;
    let fname = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("state");
    let tmp = dir.join(format!(".{fname}.tmp.{}", std::process::id()));
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(contents.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// The `pubkey:` value published in a role card's FRONTMATTER, if any. Parses via `parse_card`
/// exactly like the read side (`roster::parse_role`) — never a raw line-scan, so the write-side
/// 1:1 check can't diverge from what verification actually reads (a `pubkey:` in the body, a
/// `pubkey : x` with a space, or a missing fence would otherwise disagree — red-team).
/// Read a card frontmatter map's published `pubkey`, FAILING CLOSED on a present-but-unusable value.
/// Ok(None) ONLY when the field is genuinely absent. A present `pubkey` that isn't a non-empty
/// string — `null`, a list, a number, `""` — is a tamper/degenerate signal and returns Err, NEVER
/// "no key". Treating a non-string pubkey as "no key" is a type-confusion bypass of the write-side
/// 1:1 guard: a hub writer sets `pubkey: null`, the guard reads "keyless", and the role is re-keyed
/// (silent identity hijack — red-team). Both the join guard and `ensure_card_pubkey` go through here
/// so they can't disagree on what "already published" means.
/// The write-side view of a card's published key — delegates to `roster::classify_pubkey`, the
/// SINGLE shared classifier the read/pin side uses too, so the guard can't diverge from what gets
/// pinned. Absent/null/"" → None (a legit placeholder — the re-key path still gates filling it via
/// the git-history "ever keyed?" check). A present non-string value → hard refusal (type-confusion
/// bypass — red-team).
pub(crate) fn published_pubkey(map: &serde_yaml::Mapping) -> Result<Option<String>> {
    roster::classify_pubkey(map).map_err(|kind| {
        anyhow!(
            "role card's `pubkey` is present but is a {kind} where a key string was expected — \
             refusing to treat that as 'no key published' (a role-id's identity IS its key; this \
             shape can't be verified — possible tampering). Inspect the roles/*.md card."
        )
    })
}

/// Did this role EVER publish a real `pubkey: ssh-…` line in the hub's history? One git call over a
/// tiny file. Used to gate re-keying a card that currently shows NO key: a fresh, never-keyed role
/// may publish its first key, but a role whose key was nulled/emptied (tamper) must NOT be re-keyed
/// through that placeholder — "once keyed, never re-keyed." (Absolute prevention is impossible when
/// the attacker fully controls the hub — that's what read-side TOFU + out-of-band confirm are for —
/// but this raises the bar from "one edited line" to "rewrite + force-push the whole hub history".)
fn role_ever_published_a_key(root: &std::path::Path, role: &str) -> Result<bool> {
    if !valid_slug(role) {
        return Ok(false);
    }
    let path = format!("roles/{role}.md");
    // Enumerate every commit that touched this card, then PARSE each historical blob through the
    // SAME `parse_card`/`published_pubkey` the current-state check uses — never a diff-text grep.
    // A line-oriented grep for `+pubkey:...ssh-` is defeated by any non-literal representation a
    // YAML parser still resolves to a real key: an anchor/alias (`pubkey: *realkey`), a folded/
    // continued scalar, rename-detection collapsing the diff, etc. (red-team). Reusing the parser
    // per revision closes that text/semantics divergence by construction.
    let log = gitcmd::output(root, &["log", "--format=%H", "--", &path])?;
    if !log.status.success() {
        return Ok(false);
    }
    let shas = String::from_utf8_lossy(&log.stdout);
    for sha in shas.lines().map(str::trim).filter(|s| !s.is_empty()) {
        let blob = gitcmd::output(root, &["show", &format!("{sha}:{path}")])?;
        if !blob.status.success() {
            continue; // the card didn't exist at this revision
        }
        let txt = String::from_utf8_lossy(&blob.stdout);
        match parse_card(&txt).and_then(|(m, _)| published_pubkey(&m)) {
            Ok(Some(_)) => return Ok(true),
            // A historical revision that is itself unparsable or type-confused is suspicious — treat
            // it as "had a key" (fail closed), never as a reason to allow a re-key.
            Err(_) => return Ok(true),
            Ok(None) => {}
        }
    }
    Ok(false)
}

fn card_pubkey(card_text: &str) -> Result<Option<String>> {
    let (map, _body) = parse_card(card_text)?;
    published_pubkey(&map)
}

/// Compare two ssh pubkeys by algorithm + key material only (ignore the trailing comment) —
/// the same notion of key-identity the pin uses.
pub(crate) fn pubkey_material_eq(a: &str, b: &str) -> bool {
    let material = |s: &str| {
        let mut it = s.split_whitespace();
        match (it.next(), it.next()) {
            (Some(x), Some(y)) => format!("{x} {y}"),
            _ => s.trim().to_string(),
        }
    };
    material(a) == material(b)
}

/// Publish the signing `pubkey` into a role card's frontmatter if it lacks one, via the SAME
/// serde round-trip the read side uses (`parse_card` → set key → reserialize) — never a raw
/// line-insert, which could produce a DUPLICATE `pubkey:` and make the card unparseable (the role
/// then vanishes fleet-wide — red-team). Returns true if it changed.
fn ensure_card_pubkey(root: &std::path::Path, role: &str, pubkey: &str) -> Result<bool> {
    let path = root.join("roles").join(format!("{role}.md"));
    // `?` here is load-bearing: a card whose frontmatter won't parse must ABORT the write, never
    // fall through to `map.get("pubkey") == None` and insert this key over a corrupt card.
    let (mut map, body) = parse_card(&std::fs::read_to_string(&path)?)?;
    // Write-side 1:1: a role-id may never publish a SECOND, different key. Same key
    // re-joining is a harmless no-op; a different key is refused (the read-side MISMATCH is the
    // suspenders — the hub is not server-validated, so this is a source-side UX guard, not a
    // boundary).
    // Same fail-closed classifier the join guard uses: a present-but-non-string `pubkey` (null, a
    // list, "") is refused here rather than read as "no key" and overwritten — that type confusion
    // was the residual identity-hijack the first cut missed (red-team).
    if let Some(existing) = published_pubkey(&map)? {
        return if pubkey_material_eq(&existing, pubkey) {
            Ok(false)
        } else {
            Err(anyhow!(
                "role '{role}' already publishes a DIFFERENT signing key — the identity IS the key, so a role-id cannot be re-keyed. For a new agent use your OWN role-id; to drive THIS identity, join with its existing key."
            ))
        };
    }
    // The card shows no key. Only a role that has NEVER published one may key itself here — else this
    // is a re-key through a nulled/emptied card (the type-confusion hijack: attacker overwrites
    // `pubkey:` to null so the guard reads "keyless", then this would fill their key).
    if role_ever_published_a_key(root, role)? {
        return Err(anyhow!(
            "role '{role}' has published a signing key before, but its card now shows none — refusing \
             to re-key it. Its card may have been tampered (its `pubkey` nulled/removed); recover the \
             card from git history rather than re-keying. The identity IS the key."
        ));
    }
    map.insert("pubkey".into(), pubkey.into());
    let yaml = serde_yaml::to_string(&map)?;
    let content = if body.trim().is_empty() {
        format!("---\n{yaml}---\n")
    } else {
        format!("---\n{yaml}---\n\n{}\n", body.trim())
    };
    std::fs::write(&path, content)?;
    Ok(true)
}

/// Warn (non-fatal) if the hub clone sits INSIDE another git repo — a repo-in-a-repo
/// that the outer repo sees as a stray untracked dir, inviting accidental commits.
/// The hub belongs as a SIBLING to work repos, not nested.
/// Would a clone at `dir` nest inside another git work tree? (Any ancestor holds a `.git`.)
pub(crate) fn is_nested_path(dir: &std::path::Path) -> bool {
    let abs = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|c| c.join(dir))
            .unwrap_or_else(|_| dir.to_path_buf())
    };
    let mut p = abs.parent();
    while let Some(a) = p {
        if a.join(".git").exists() {
            return true;
        }
        p = a.parent();
    }
    false
}

/// Choose a working-clone location that won't nest inside a work repo (#4 field feedback). An
/// explicit `dir` is honored verbatim; otherwise, if the default `CWD/<basename>` would nest
/// (agents run from their project dir), clone into `$HOME/<basename>` and say so.
pub(crate) fn safe_clone_dir(dir: Option<String>, basename: &str) -> String {
    if let Some(d) = dir {
        return d;
    }
    if is_nested_path(std::path::Path::new(basename)) {
        if let Ok(home) = config::home() {
            let target = home.join(basename);
            eprintln!(
                "confer: inside a git repo — cloning to {} so it isn't nested in your working tree.",
                target.display()
            );
            return target.to_string_lossy().into_owned();
        }
    }
    basename.to_string()
}

pub(crate) fn warn_if_nested(hub: &std::path::Path) {
    let hub_abs = hub.canonicalize().unwrap_or_else(|_| hub.to_path_buf());
    let mut p = hub_abs.parent();
    while let Some(dir) = p {
        if dir.join(".git").exists() {
            eprintln!(
                "confer: ⚠ this hub clone is nested inside another git repo ({}). \
                 Keep the hub as a SIBLING (e.g. ~/git/<hub>), not inside a work repo — \
                 the outer repo sees it as an untracked dir and it's easy to commit by \
                 accident. Move it and `confer reconnect --dir <new-path>` when convenient.",
                dir.display()
            );
            return;
        }
        p = dir.parent();
    }
}

pub(crate) fn cmd_join(
    role: String,
    host: Option<String>,
    display: Option<String>,
    desc: Option<String>,
    signing_key: Option<String>,
    force: bool,
) -> Result<()> {
    let root = config::repo_root()?;
    if !valid_slug(&role) {
        return Err(anyhow!(
            "invalid role '{role}': must match [a-z0-9][a-z0-9-]* (≤64 chars)"
        ));
    }
    if is_reserved_name(&role) {
        return Err(anyhow!(
            "role '{role}' is reserved (the broadcast target); choose another role id"
        ));
    }
    // Guard the free-form display name against homoglyph impersonation:
    // a `gitcоnv` (Cyrillic о) would render in every wake line and impersonate a peer.
    if let Some(d) = &display {
        if alias::homoglyph_risk(d) {
            return Err(anyhow!(
                "display name '{d}' mixes Latin with Cyrillic/Greek look-alike characters \
                 (homoglyph impersonation risk); use a plain-ASCII display name"
            ));
        }
    }
    if let Err(e) = gitcmd::integrate(&root) {
        eprintln!("confer: could not sync hub ({e}); resuming from local state");
    }
    check_version(&root);
    // Write-side 1:1: refuse EARLY — before any signing config or pin side effects —
    // if this role already publishes a DIFFERENT key. The identity IS the key; a role-id can't be
    // re-keyed. (ensure_card_pubkey re-checks as suspenders.)
    if let Some(kp) = &signing_key {
        let my_pub = read_pubkey(std::path::Path::new(kp))?;
        let card_path = root.join("roles").join(format!("{role}.md"));
        if let Ok(txt) = std::fs::read_to_string(&card_path) {
            // card_pubkey now FAILS CLOSED (`?`): a corrupt card can no longer read as "no key
            // published" and slip past this guard — that was the identity-hijack (a hub writer
            // commits one malformed line, then re-keys the role). A card in an unknown state aborts
            // the join rather than letting a re-key through.
            if let Some(existing) = card_pubkey(&txt)? {
                if !pubkey_material_eq(&existing, &my_pub) {
                    return Err(anyhow!(
                        "role '{role}' already publishes a DIFFERENT signing key — the identity IS the key, so a role-id cannot be re-keyed. Use your OWN role-id for a new agent, or join with this identity's existing key."
                    ));
                }
            }
        }
    }
    let roster = roster::load(&root);
    let session = ulid::Ulid::new().to_string();
    let host = host.or_else(config::hostname).unwrap_or_default();
    let confer_dir = root.join(".confer");
    let identity_path = confer_dir.join("identity.json");

    // Serialize the read-check-write of identity.json against a concurrent join on the SAME clone
    // (the SessionStart auto-heal fires `reconnect` while a manual reconnect may also run) — a
    // bounded flock; best-effort like presence/keyring (proceed if it times out). Held until the
    // atomic identity write below so the guard's decision can't be raced.
    let _idlock = config::state_lock(&confer_dir.join("identity.lock"));

    // One clone = one role, permanently. If this working copy is ALREADY bound to a DIFFERENT
    // role, re-roling it here is an identity clobber: the clone keeps its CURRENT signing key, so
    // that one key would back two role-ids on the hub and the prior role's future posts from this
    // clone would surface under the new label — silently. (Field-reported on 0.6.0.) Refuse by
    // default; a deliberate re-role takes --force. The clean path for a new role is a SEPARATE
    // clone, not relabeling this one.
    //
    // FAIL CLOSED: a control whose whole point is "refuse by default" must not default to PROCEED
    // when it can't determine the bound role. Only a genuinely ABSENT identity.json is a fresh
    // clone; an unreadable / corrupt / role-less file (e.g. a torn write from a crash) is refused,
    // not fallen through. (Red-team, Jarvis: the old if-let/if-let/if-let skipped the guard on any
    // read/parse failure and re-roled silently, with not even the --force warning.)
    match std::fs::read_to_string(&identity_path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {} // fresh clone — nothing bound yet
        Err(e) => {
            return Err(anyhow!(
                "cannot read this clone's identity (.confer/identity.json: {e}) — refusing to \
                 (re-)role it, since I can't verify it isn't already bound to another role. \
                 Inspect the file, or pass --force to override."
            ));
        }
        Ok(txt) => {
            let prev = serde_json::from_str::<serde_json::Value>(&txt)
                .ok()
                .and_then(|v| v.get("role").and_then(|r| r.as_str()).map(str::to_string));
            match prev {
                None if !force => {
                    return Err(anyhow!(
                        ".confer/identity.json exists but names no role (corrupt or partial write?) \
                         — refusing to (re-)role this clone without --force. Inspect the file, or \
                         re-create the clone."
                    ));
                }
                Some(prev) if prev != role && !force => {
                    return Err(anyhow!(
                        "this clone already belongs to role '{prev}' — refusing to re-role it to \
                         '{role}'. It would keep {prev}'s signing key, binding one key to two roles \
                         and making {prev}'s posts from here appear as '{role}'. For a new role, \
                         make a SEPARATE clone: `confer clone <hub> --role {role} --managed`. To \
                         re-role THIS clone anyway (it keeps the current key), pass --force."
                    ));
                }
                Some(prev) if prev != role => {
                    eprintln!(
                        "confer: --force re-roling this clone from '{prev}' to '{role}' — it keeps \
                         the current signing key, so both role-ids are backed by the same identity \
                         (they are now linked; see DESIGN.md)."
                    );
                }
                _ => {} // same role (idempotent re-join), or --force over a role-less file
            }
        }
    }

    // Compute the signing pubkey with a PURE read (no git-config side effect) so it can go into
    // identity.json — which we write FIRST, before any git-config mutation. #2 (red-team, Jarvis):
    // configure_signing + the user.name/email sets used to run BEFORE the identity write with no
    // rollback, so a failed join left the clone committing as a role confer never recorded. The
    // durable identity record must land before the reconfiguration.
    let pubkey: Option<String> = match &signing_key {
        Some(kp) => Some(read_pubkey(std::path::Path::new(kp))?),
        None => None,
    };
    let mut identity = serde_json::json!({
        "role": role, "session": session, "host": host, "joined_at": now(),
    });
    if let Some(kp) = &signing_key {
        identity["signing_key"] = serde_json::Value::String(kp.clone());
    }
    // Record the pubkey so the managed-clone-home resolver can verify a clone's identity by KEY,
    // not just its (public, replayable) path tag.
    if let Some(pk) = &pubkey {
        identity["pubkey"] = serde_json::Value::String(pk.clone());
    }
    // Atomic (temp+rename): a crash mid-write leaves the PREVIOUS valid identity.json intact, never
    // a torn file — so the fail-closed guard above can always trust what it reads (mirrors how
    // tiers/presence/keyring persist state). The plain fs::write here was the root cause that let a
    // corrupt file blind the guard.
    write_atomic(&identity_path, &serde_json::to_string_pretty(&identity)?)?;

    // NOW the git-config mutations (signing + committer identity), AFTER the identity is durable.
    match &signing_key {
        Some(kp) => {
            configure_signing(&root, std::path::Path::new(kp))?;
            // Pin the committer identity in the clone config so a rebase re-commits (and re-signs)
            // as this role — otherwise the committer email wouldn't match the allowed_signers
            // principal and verification would fail.
            gitcmd::check(&root, &["config", "user.name", &role])?;
            gitcmd::check(
                &root,
                &["config", "user.email", &format!("{role}@confer.local")],
            )?;
            println!("signing: commits from this clone will be signed with {kp}");
        }
        None => {
            // No agent key → do NOT inherit the human's personal git signer (wrong identity, and it
            // breaks the moment their 1Password locks). Turn commit signing OFF for this clone and
            // attribute commits to the role. confer's message-level attribution / verification is
            // the identity model; git commit signatures are orthogonal and must never be the
            // human's personal key.
            let _ = gitcmd::check(&root, &["config", "commit.gpgsign", "false"]);
            let _ = gitcmd::check(&root, &["config", "gpg.format", "ssh"]); // harmless; avoids gpg fallback
            let _ = gitcmd::check(&root, &["config", "user.name", &role]);
            let _ = gitcmd::check(
                &root,
                &["config", "user.email", &format!("{role}@confer.local")],
            );
        }
    }
    warn_if_nested(&root);
    let sign = signing_key.is_some();

    // Pin + CONFIRM our OWN key locally: an agent doesn't
    // out-of-band-confirm itself — only a PEER's first-seen key stays provisional (⚠ first-sight)
    // until `confer confirm-key`. Confirm ONLY when the pin IS this key (we just pinned it, or it
    // already matches) — NEVER on a Mismatch, so `join --role <peer>` can't auto-confirm a
    // peer's/attacker's pinned key (red-team).
    if let Some(pk) = &pubkey {
        let hk = config::hub_key(&root);
        if matches!(
            keyring::pin_or_check(&hk, &role, pk, &now()),
            Ok(keyring::Pin::First) | Ok(keyring::Pin::Match)
        ) {
            let _ = keyring::confirm(&hk, &role);
        }
    }
    // Joining an existing hub defaults it to `foreign` — but only if no tier
    // is set, so `init`'s `own` (set before it calls join) and an explicit `confer trust`
    // both win.
    let _ = tiers::set_default(&config::hub_key(&root), tiers::Tier::Foreign);
    println!(
        "joined as {} [{role}] (session {session})",
        schema::sanitize_term(roster::display(&roster, &role), false)
    );

    // Register the role on the hub so peers see it — roles are shared as
    // roles/<id>.md cards (display name + host + pubkey), not just the local
    // identity. Create-if-absent so a hand-authored display name is never
    // clobbered; but ensure the signing pubkey gets published either way.
    let card_path = root.join("roles").join(format!("{role}.md"));
    if card_path.exists() {
        let msg = match &pubkey {
            Some(pk) if ensure_card_pubkey(&root, &role, pk)? => {
                Some("join: publish signing pubkey")
            }
            _ => None,
        };
        match msg {
            Some(m) => match gitcmd::commit_and_sync(&root, &role, &card_path, m, sign) {
                Ok(_) => println!("published signing pubkey to roles/{role}.md."),
                Err(e) => eprintln!("confer: pubkey written locally but hub sync failed ({e})."),
            },
            None => println!("role already registered on the hub (roles/{role}.md)."),
        }
    } else {
        let display = display.unwrap_or_else(|| role.clone());
        let mut card = serde_yaml::Mapping::new();
        card.insert("display".into(), display.clone().into());
        card.insert("host".into(), host.clone().into());
        if let Some(d) = &desc {
            card.insert("desc".into(), d.clone().into());
        }
        if let Some(pk) = &pubkey {
            card.insert("pubkey".into(), pk.clone().into());
        }
        let yaml = serde_yaml::to_string(&card)?;
        std::fs::create_dir_all(root.join("roles"))?;
        std::fs::write(&card_path, format!("---\n{yaml}---\n"))?;
        match gitcmd::commit_and_sync(&root, &role, &card_path, &format!("join: register role {role}"), sign) {
            Ok(_) => println!("registered on the hub: roles/{role}.md (display '{display}', host '{host}')."),
            Err(e) => eprintln!(
                "confer: role card written locally but hub sync failed ({e}); it will reach the hub on your next append."
            ),
        }
    }

    let msgs = store::all_messages(&root)?;
    let grps = groups::load(&root);
    let open: Vec<&Message> = msgs
        .iter()
        .filter(|m| {
            m.front.msg_type == "request"
                && groups::addressed(m, &role, &grps)
                && matches!(request_status(&msgs, &m.front.id), "OPEN" | "CLAIMED")
        })
        .collect();
    if open.is_empty() {
        println!("no open requests assigned to '{role}'.");
    } else {
        println!("open requests for '{role}':");
        let hub_key = config::hub_key(&root);
        let mut vc = verify::Cache::default();
        for m in open {
            let t = verify::status(&root, &hub_key, &roster, &mut vc, m);
            println!("{}", format_line(&roster, m, false, Some(&t)));
        }
    }
    crosshub::record(&root, &role); // remember this hub for cross-hub recognition (F3)
    seed_hub_on_join(&root); // design/35 phase 2: record routing + TOFU-pin the hub identity
    Ok(())
}

/// Best-effort seed-on-join (design/35 phase 2). A human ran `join`, which IS the first-sight
/// confirmation — so record this hub's routing into the machine config and TOFU-pin its identity in
/// `known_hubs` (`confirmed=true`). Additive + best-effort: NEVER fails the join. A mismatch against an
/// EXISTING pin is surfaced loudly (`‼`) but the pin is NOT silently re-pointed — a deliberate move is
/// `confer hub repin`. (Phase-3 auto-join will hard-fail on a mismatch; here a human is present.)
fn seed_hub_on_join(root: &std::path::Path) {
    let name = match current_hub_name(root) {
        Ok(n) => n,
        Err(_) => return, // no origin / underivable name → nothing to seed
    };
    // Routing: remember url + scheme (create-if-absent so we never clobber an explicit config).
    if let Ok(o) = gitcmd::output(root, &["config", "--get", "remote.origin.url"]) {
        if o.status.success() {
            let url = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let scheme = if url.starts_with("http") { "https" } else { "ssh" };
            let (n, u, s) = (name.clone(), url.clone(), scheme.to_string());
            if machineconfig::update_with(move |cfg| {
                let hub = cfg.hubs.entry(n).or_default();
                if hub.url.is_none() {
                    hub.url = Some(u);
                }
                if hub.scheme.is_none() {
                    hub.scheme = Some(s);
                }
                Ok(())
            })
            .is_err()
            {
                hint(format!("couldn't record routing for '{name}' (set it with `confer config set hubs.{name}.url <url>`)."));
            }
        }
    }
    // Identity: TOFU-RECORD the pin (or advance the tip). NOTE: recorded UNCONFIRMED — a `confer join`
    // can be run by an agent/script/reconnect chain, so it is NOT a human first-sight confirmation
    // (design/35: the pin-write must block on a human, which a bare join doesn't). A human confirms
    // out-of-band with `confer hub repin` (which shows root+tip and is --yes-gated). Phase-3 auto-join
    // will only trust a `confirmed:true` pin.
    match knownhubs::verify(&name, root) {
        knownhubs::Verdict::FirstSight { root: r, tip } => {
            if knownhubs::record(&name, &r, &tip, false).is_ok() {
                hint(format!(
                    "recorded (UNCONFIRMED) hub identity for '{name}' (root {}). Verify + confirm with `confer hub repin`.",
                    short12(&r)
                ));
            } else {
                warn_safety(format!("couldn't record the hub-identity pin for '{name}' — run `confer hub repin` once ~/.confer is writable."));
            }
        }
        knownhubs::Verdict::Match { new_tip } => knownhubs::advance_tip(&name, &new_tip),
        knownhubs::Verdict::RootMismatch { pinned, got } => warn_trust(format!(
            "hub '{name}': ROOT MISMATCH — pinned {} but this repo's root is {}. NOT re-pinning; \
             investigate, then `confer hub repin` if this is a legitimate move.",
            short12(&pinned),
            short12(&got)
        )),
        knownhubs::Verdict::TipUnreachable { pinned_tip } => warn_trust(format!(
            "hub '{name}': confirmed-good tip {} not reachable from HEAD (history rewritten?). NOT \
             advancing the pin; investigate.",
            short12(&pinned_tip)
        )),
        knownhubs::Verdict::NotVerifiable(_) => {}
    }
}
