//! Read-path signature verification, anchored to the locally **pinned** key
//! (keyring/TOFU). DESIGN.md phase 1, the trust root.
//!
//! Before this, `confer verify` was a manual, per-message command that built its
//! allowed-signers list from the shared-repo role cards — so a rewritten card could
//! manufacture a "verified" (review #1/#2). Here verification (a) checks the ADD-commit
//! signature against the key we PINNED for that role, not the current card, and (b) is a
//! reusable, cached function every read path can call. A card whose key differs from the
//! pin is the loud `Mismatch`; the pin (the real key) is what signatures verify against.

use crate::schema::Message;
use crate::{crosshub, gitcmd, keyring, roster, store};
use std::collections::HashMap;
use std::path::Path;

/// The trust standing of a single message, from its ADD-commit signature vs the pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trust {
    /// Signature verified against the role's pinned, human-CONFIRMED key. `fpr` is its `SHA256:…`.
    Verified { fpr: String },
    /// Signed by the role's pinned key, but that key was just first-seen and NOT yet confirmed
    /// out-of-band. Provisional: an attacker who won the TOFU race on an
    /// un-pinned role also lands here, so treat as "confirm the fingerprint before trusting."
    FirstSight { fpr: String },
    /// No basis to trust: no published/pinned key, unsigned commit, or a signature by
    /// some key other than the pin. Neutral — not an alarm, just "can't vouch."
    Unverified { reason: String },
    /// The role's PUBLISHED key changed vs what we pinned (review #2) — a possible
    /// card-rewrite/impersonation. The loud state, and now **permanent**: the identity IS the
    /// key, so a changed key is never a legitimate rotation. There is no `--repin` (removed,
    /// DESIGN.md); the only honest resolution is a NEW role-id for what is a different agent.
    Mismatch { reason: String },
}

impl Trust {
    /// Compact one-line-feed marker. Unverified is a quiet middot (most fleet history is
    /// simply unsigned today), Verified a check, Mismatch a loud double-bang.
    pub fn glyph(&self) -> &'static str {
        match self {
            Trust::Verified { .. } => "✓",
            Trust::FirstSight { .. } => "⚠",
            Trust::Unverified { .. } => "·",
            Trust::Mismatch { .. } => "‼",
        }
    }
    /// Full status tag for `show`/`verify`/`inbox`.
    pub fn tag(&self) -> String {
        match self {
            Trust::Verified { fpr } => format!("✓ verified ({fpr})"),
            Trust::FirstSight { fpr } => format!("⚠ first-sight ({fpr}) — signed by a key not yet confirmed out-of-band; run `confer confirm-key`"),
            Trust::Unverified { reason } => format!("· unverified — {reason}"),
            Trust::Mismatch { reason } => format!("‼ KEY MISMATCH — {reason}"),
        }
    }
    pub fn is_mismatch(&self) -> bool { matches!(self, Trust::Mismatch { .. }) }
}

/// Per-invocation memo so each ADD-commit is located + signature-checked at most once,
/// even when a render pass touches many messages. A commit's signature is immutable, so
/// caching by sha is sound within (and across) a process run.
#[derive(Default)]
pub struct Cache {
    add_sha: HashMap<String, Option<String>>, // message id → ADD-commit sha
    card_sha: HashMap<String, Option<String>>, // role → latest roles/<id>.md commit sha
    gsig: HashMap<String, char>,              // sha → git `%G?`
    fpr: HashMap<String, String>,             // pubkey → SHA256 fingerprint
}

impl Cache {
    /// The LATEST commit touching a message's file — the one whose signature authorizes the
    /// content confer actually renders. NOT the add-commit: the rendered body is read fresh from
    /// the working tree, so a later commit that rewrites the body (or any frontmatter field) is
    /// what must be verified. Otherwise a hub writer could tamper with an already-`✓ verified`
    /// message and the stamp would still show — a forged-verified on attacker text (a review
    /// finding). Same discipline as `card_commit`; messages are append-only, so for a legit message the
    /// latest commit IS the add-commit.
    fn msg_commit(&mut self, root: &Path, m: &Message) -> Option<String> {
        if let Some(v) = self.add_sha.get(&m.front.id) {
            return v.clone();
        }
        let topic = m.front.topic.as_deref().unwrap_or("general");
        let file = store::message_path(root, topic, &m.front.id, &m.front.from, &m.front.ts);
        let rel = file.strip_prefix(root).unwrap_or(&file).to_string_lossy().to_string();
        let sha = gitcmd::output(root, &["log", "--format=%H", "-1", "--", &rel])
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty());
        self.add_sha.insert(m.front.id.clone(), sha.clone());
        sha
    }

    /// git `%G?` for `sha`, verified against a one-line allowed-signers file mapping the
    /// role's committer email to its PINNED key. `G` good, `N` unsigned, else signed by
    /// a non-pinned key (`U`/`B`/`E`).
    fn gsig(&mut self, root: &Path, role: &str, pinned: &str, sha: &str) -> char {
        if let Some(c) = self.gsig.get(sha) {
            return *c;
        }
        let c = (|| {
            // The signers file must hold EXACTLY the one pinned key — the whole `%G?` check
            // (which binds to any listed signer, not the committer) rests on that. `role` is a
            // card filename stem (untrusted); a newline in it would inject a second signer line,
            // so reject anything that isn't a clean slug.
            if !crate::valid_slug(role) {
                return None;
            }
            let dir = root.join(".confer");
            std::fs::create_dir_all(&dir).ok()?;
            // Per-invocation file (pid + counter): a shared path raced across concurrent
            // verifiers (a `watch`/`integrate` checking role B while `who` checks role A),
            // flipping the one-line file mid-check → a spurious Unverified/Mismatch flap.
            use std::sync::atomic::{AtomicU64, Ordering};
            static N: AtomicU64 = AtomicU64::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let sf = dir.join(format!("verify_signers.{}.{n}", std::process::id()));
            std::fs::write(&sf, format!("{role}@confer.local {pinned}\n")).ok()?;
            let keygen = crate::ssh_keygen_path();
            let out = gitcmd::output(
                root,
                &[
                    "-c", "gpg.format=ssh",
                    "-c", &format!("gpg.ssh.program={keygen}"),
                    "-c", &format!("gpg.ssh.allowedSignersFile={}", sf.display()),
                    "show", "-s", "--format=%G?", sha,
                ],
            );
            let _ = std::fs::remove_file(&sf); // clean up regardless of the result
            let out = out.ok()?;
            String::from_utf8_lossy(&out.stdout).trim().chars().next()
        })()
        .unwrap_or('E');
        self.gsig.insert(sha.to_string(), c);
        c
    }

    /// The latest commit that touched a role's card (`roles/<id>.md`) — the edit whose
    /// signature authorizes the card's CURRENT fields.
    fn card_commit(&mut self, root: &Path, role: &str) -> Option<String> {
        if let Some(v) = self.card_sha.get(role) {
            return v.clone();
        }
        let rel = format!("roles/{role}.md");
        let sha = gitcmd::output(root, &["log", "--format=%H", "-1", "--", &rel])
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty());
        self.card_sha.insert(role.to_string(), sha.clone());
        sha
    }

    fn fingerprint(&mut self, pubkey: &str) -> String {
        if let Some(f) = self.fpr.get(pubkey) {
            return f.clone();
        }
        let f = crosshub::fingerprint(pubkey);
        self.fpr.insert(pubkey.to_string(), f.clone());
        f
    }
}

/// The trust standing of a role's CARD — whether its CURRENT fields (display / host / desc /
/// aliases, and the forthcoming `status`) were authored by the role's PINNED key. This is the
/// DESIGN.md fix that extends message-grade verification to card mutations: `roster.rs` reads
/// those fields as data, so without this a hub writer could rewrite another role's card (flip a
/// status, redirect a display to an impostor). Same TOFU + `%G?` machinery as [`status`], applied
/// to `roles/<id>.md`. Best-effort; degrades to `Unverified`, never a crash or a false `Verified`.
///
/// Rollout is graceful: **`Mismatch`** (the published key changed vs the pin) is the loud,
/// act-on-it state — do NOT trust the card's fields. **`Unverified`** (a legacy `roles.toml`-only
/// card, or an unsigned `roles/<id>.md` — common on pre-signing fleet history) is advisory: fields
/// are still shown, just not vouched for. **`Verified`** means the current card was signed by the
/// pinned key.
pub fn card_trust(
    root: &Path,
    hub_key: &str,
    ros: &roster::Roster,
    cache: &mut Cache,
    role: &str,
) -> Trust {
    // TOFU + the loud key-change alarm is handled inside `commit_trust`; here we just locate the
    // card's latest edit and hand it off. No .md commit (legacy roles.toml) → still surface a
    // Mismatch if the published key changed, else Unverified.
    let Some(sha) = cache.card_commit(root, role) else {
        if roster::pubkey(ros, role).is_none() {
            return Trust::Unverified { reason: "no published signing key".into() };
        }
        // fall through commit_trust with an empty sha only to reuse its mismatch check
        return match commit_trust(root, hub_key, ros, cache, role, "") {
            m @ Trust::Mismatch { .. } => m,
            _ => Trust::Unverified { reason: "role card has no signed commit (legacy/unsigned)".into() },
        };
    };
    commit_trust(root, hub_key, ros, cache, role, &sha)
}

/// Verify that a specific commit `sha` was signed by `role`'s PINNED key (TOFU-pinning the card
/// key on first sight, and firing the loud `Mismatch` if the published key changed). Generalises
/// the card check to any commit — used for presence heartbeats. Best-effort;
/// degrades to `Unverified`, never a false `Verified` or a crash.
pub fn commit_trust(
    root: &Path,
    hub_key: &str,
    ros: &roster::Roster,
    cache: &mut Cache,
    role: &str,
    sha: &str,
) -> Trust {
    let Some(card) = roster::pubkey(ros, role) else {
        return Trust::Unverified { reason: "no published signing key".into() };
    };
    if let Ok(keyring::Pin::Mismatch { .. }) = keyring::pin_or_check(hub_key, role, card, "") {
        return Trust::Mismatch {
            reason: format!(
                "{role}'s published key differs from the one first pinned — the identity IS the key, so a changed key is never a legitimate rotation"
            ),
        };
    }
    let pinned = keyring::pinned(hub_key, role).unwrap_or_else(|| card.to_string());
    if sha.is_empty() {
        return Trust::Unverified { reason: "no commit to verify".into() };
    }
    match cache.gsig(root, role, &pinned, sha) {
        'G' => good_verdict(hub_key, role, cache.fingerprint(&pinned)),
        'N' => Trust::Unverified { reason: "unsigned commit".into() },
        _ => Trust::Unverified { reason: format!("not signed by {role}'s pinned key") },
    }
}

/// A good signature against the pin resolves to `Verified` only once the human has confirmed the
/// key out-of-band; before that it's `FirstSight`.
fn good_verdict(hub_key: &str, role: &str, fpr: String) -> Trust {
    if keyring::confirmed(hub_key, role) {
        Trust::Verified { fpr }
    } else {
        Trust::FirstSight { fpr }
    }
}

/// The trust standing of one message: TOFU-pin the role's published key, then verify the
/// ADD-commit signature against the pin. Best-effort — any IO hiccup degrades to
/// `Unverified`, never a crash or a false `Verified`.
pub fn status(root: &Path, hub_key: &str, ros: &roster::Roster, cache: &mut Cache, m: &Message) -> Trust {
    let role = m.front.from.as_str();
    let Some(card) = roster::pubkey(ros, role) else {
        return Trust::Unverified { reason: "no published signing key".into() };
    };
    // TOFU: pin on first sight; a card key that changed under us is the review-#2 alarm.
    if let Ok(keyring::Pin::Mismatch { .. }) = keyring::pin_or_check(hub_key, role, card, &m.front.ts) {
        return Trust::Mismatch {
            reason: format!(
                "{role}'s published key differs from the one first pinned — the identity IS the key, so this is never a legitimate rotation (impersonation, or a new agent that must use its own role-id)"
            ),
        };
    }
    // Verify against the PINNED key (falls back to the card only if the pin write failed).
    let pinned = keyring::pinned(hub_key, role).unwrap_or_else(|| card.to_string());
    let Some(sha) = cache.msg_commit(root, m) else {
        return Trust::Unverified { reason: "could not locate the message's commit".into() };
    };
    match cache.gsig(root, role, &pinned, &sha) {
        'G' => good_verdict(hub_key, role, cache.fingerprint(&pinned)),
        'N' => Trust::Unverified { reason: "unsigned commit".into() },
        _ => Trust::Unverified { reason: format!("signed, but not by {role}'s pinned key") },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_and_tag_shapes() {
        assert_eq!(Trust::Verified { fpr: "SHA256:x".into() }.glyph(), "✓");
        assert_eq!(Trust::Unverified { reason: "unsigned commit".into() }.glyph(), "·");
        assert_eq!(Trust::Mismatch { reason: "changed".into() }.glyph(), "‼");
        assert!(Trust::Verified { fpr: "SHA256:x".into() }.tag().contains("verified"));
        assert!(Trust::Mismatch { reason: "changed".into() }.tag().contains("MISMATCH"));
        assert!(Trust::Mismatch { reason: "c".into() }.is_mismatch());
        assert!(!Trust::Verified { fpr: "SHA256:x".into() }.is_mismatch());
    }
}
