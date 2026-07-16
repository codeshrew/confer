//! Managed clone home. confer owns a hidden, organized tree of per-identity hub
//! clones under `~/.confer/clones/`, so agents never hand-place coordination clones in the
//! human's git workspace. Layout:
//!
//! ```text
//! ~/.confer/clones/<hub-slug>-<hubtag>/<role-slug>-<keytag>/
//!   e.g.  ~/.confer/clones/team-hub-3f9a1c024b8e/alice-1a2b3c4d5e6f/
//! ```
//!
//! The path is a pure function of `(hub_key, role-id, signing pubkey)`. **Resolution anchors
//! on the trailing tag** (`<hubtag>`/`<keytag>`) — the readable slug prefixes are *cosmetic*.
//! So a rename (which never touches role-id anyway), a hyphen inside a role-id
//! (`carol-role`), or any other weird chars can neither move a clone nor break the lookup:
//! we always "grab the last part" via [`split_tag`] and compare the stable tag.
//!
//! Why these anchors:
//! - `<keytag>` derives from the identity's **signing key** (the immutable identity — "identity
//!   IS the key"), so it survives rename AND `adopt` (a new session holding the same
//!   key resolves to the same folder). It is NOT the session id — that is ephemeral by design.
//! - `<hubtag>` derives from the machine-independent `hub_key` (root-commit sha), so the same
//!   hub maps to the same folder on every machine.

use crate::config;
use std::path::{Path, PathBuf};

/// Low bits of an FNV-1a-64 hash rendered as fixed-width lowercase hex. Deterministic,
/// dependency-free, and — critically — **hyphen-free and case-fold-safe** (lowercase hex
/// only), so the tag can never contain the `-` that [`split_tag`] splits on, and never
/// collides with itself on a case-insensitive filesystem (macOS APFS folds case). 48 bits is
/// collision-negligible for any real fleet, and resolution matches BOTH the hub and key tag,
/// so a false hit would need both to collide at once.
fn hash_tag(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:012x}", h & 0xffff_ffff_ffff)
}

/// Stable tag for a hub, from its machine-independent `hub_key` (root-commit sha). Pure.
pub fn hub_tag(hub_key: &str) -> String {
    hash_tag(hub_key)
}

/// Canonical key material for tagging: an ssh pubkey's algorithm + base64 body, dropping the
/// trailing comment (which varies). Mirrors `keyring::normalize` so the tag tracks the same
/// notion of key-identity the TOFU pin uses.
fn key_material(pubkey: &str) -> String {
    let mut it = pubkey.split_whitespace();
    match (it.next(), it.next()) {
        (Some(algo), Some(body)) => format!("{algo} {body}"),
        _ => pubkey.trim().to_string(),
    }
}

/// Stable tag for an identity, derived IN-PROCESS from its signing pubkey's canonical material.
/// Deliberately NOT derived from `ssh-keygen` output: that tool's format varies with its
/// availability/version and silently falls back to a different string on failure, which would
/// flip the tag and orphan the clone across machines/invocations (review finding). Immutable for
/// the life of the key; deterministic and environment-independent.
pub fn key_tag(pubkey: &str) -> String {
    hash_tag(&key_material(pubkey))
}

/// Human-readable, path-safe slug for the COSMETIC prefix of a segment. ASCII alnum + `_`
/// survive (lowercased); every other char — hyphen, `/`, whitespace, `.`, unicode/homoglyph —
/// becomes a single `-`; runs collapse; ends trimmed; length-capped. Never empty (`"x"`
/// fallback) and never leaks a `/` or `..`, so a hostile role-id can't escape the home dir.
/// Because resolution matches the trailing tag, this is best-effort labeling only.
pub fn slug(label: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in label.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    let s: String = out.trim_matches('-').chars().take(24).collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "x".into()
    } else {
        s
    }
}

/// The clones-home root: `~/.confer/clones/`.
pub fn root() -> anyhow::Result<PathBuf> {
    Ok(config::home()?.join(".confer").join("clones"))
}

/// Managed directory NAME for a hub: `<hub-slug>-<hubtag>`. Pure.
pub fn hub_dir_name(hub_slug: &str, hub_key: &str) -> String {
    format!("{}-{}", slug(hub_slug), hub_tag(hub_key))
}

/// Managed directory NAME for an identity within a hub: `<role-slug>-<keytag>`.
pub fn role_dir_name(role_id: &str, pubkey: &str) -> String {
    format!("{}-{}", slug(role_id), key_tag(pubkey))
}

/// "Grab the last part": split a managed dir NAME into `(cosmetic-slug, stable-tag)` on the
/// LAST `-`. Returns `None` when there is no separator (so foreign dirs are ignored). This is
/// the ONE parse used to match by tag — robust to any number of hyphens in the slug prefix.
pub fn split_tag(dir_name: &str) -> Option<(&str, &str)> {
    dir_name.rsplit_once('-')
}

/// Full managed clone path for `(hub, role-id, pubkey)`. Pure; does not touch disk. Used to
/// place a NEW clone (`join`/`adopt`).
pub fn clone_dir(
    hub_slug: &str,
    hub_key: &str,
    role_id: &str,
    pubkey: &str,
) -> anyhow::Result<PathBuf> {
    Ok(root()?
        .join(hub_dir_name(hub_slug, hub_key))
        .join(role_dir_name(role_id, pubkey)))
}

/// Find an EXISTING managed clone under `root` by matching the stable tags, tolerant of any
/// drift in the cosmetic slug prefixes. Pure filesystem walk — the part that must be
/// bullet-proof, so it's tested directly with literal tags.
pub fn resolve_by_tags(
    root: &Path,
    hub_tag: &str,
    key_tag: &str,
    expect_pubkey: Option<&str>,
) -> Option<PathBuf> {
    let Ok(hubs) = std::fs::read_dir(root) else {
        return None;
    };
    let mut found: Option<PathBuf> = None;
    for h in hubs.flatten() {
        // Only real directories — `file_type()` does not follow symlinks, so a symlinked entry
        // (is_symlink) is rejected here rather than followed.
        if !h.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        if !tag_matches(&h.file_name().to_string_lossy(), hub_tag) {
            continue;
        }
        let Ok(roles) = std::fs::read_dir(h.path()) else {
            continue;
        };
        for r in roles.flatten() {
            if !r.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            if !tag_matches(&r.file_name().to_string_lossy(), key_tag) {
                continue;
            }
            // A REAL confer clone has `.confer/identity.json` as a regular file (symlink_metadata
            // rejects a symlink pointing at a victim's file — borrowed legitimacy).
            let idf = r.path().join(".confer").join("identity.json");
            if !std::fs::symlink_metadata(&idf).map(|m| m.is_file()).unwrap_or(false) {
                continue;
            }
            // PUBKEY-EQUALITY (fail CLOSED): `key_tag` derives from the PUBLIC key, so a collision
            // is replayable. When the caller knows the expected key, the candidate MUST record a
            // MATCHING pubkey — a clone that omits/empties `pubkey` is NOT a match, because an
            // attacker with local write to the clone home could plant a pubkey-less dir at the
            // precomputed tag before the real clone exists (a review finding). Callers that pass no
            // expected key (`None`) still fall back to the tag + the ambiguity guard below.
            if let Some(want) = expect_pubkey {
                match identity_pubkey(&r.path()) {
                    Some(have) if same_key(&have, want) => {}
                    _ => continue,
                }
            }
            if found.is_some() {
                // Two clones match the same (hub, key) tag — nondeterministic which `read_dir`
                // yields first, and one could be attacker-planted. Refuse rather than guess.
                return None;
            }
            found = Some(r.path());
        }
    }
    found
}

/// Same ssh key by algorithm + material (the trailing comment is ignored) — the notion of
/// key-identity the TOFU pin uses.
pub fn same_key(a: &str, b: &str) -> bool {
    key_material(a) == key_material(b)
}

/// The pubkey a clone's identity commits to: the recorded `pubkey`, or — for clones joined
/// before `pubkey` was recorded (every pre-0.4.0 `identity.json` has `signing_key` but no
/// `pubkey`) — the pubkey DERIVED from the recorded `signing_key`'s `.pub`. The resolver still
/// checks whatever this returns against the caller's expected/pinned key (`same_key`), so a
/// wrong or attacker-chosen key is rejected exactly as before; this only recovers the key the
/// identity ALREADY names via `signing_key`, so `where`/resolve stop disagreeing with `clones`
/// on a legacy migrated clone (a fleet-migration finding, reviewer-endorsed).
pub fn identity_pubkey(clone: &Path) -> Option<String> {
    let txt = std::fs::read_to_string(clone.join(".confer").join("identity.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&txt).ok()?;
    if let Some(pk) = v.get("pubkey").and_then(|x| x.as_str()).filter(|s| !s.is_empty()) {
        return Some(pk.to_string());
    }
    // Fallback: derive from the recorded signing key's `.pub`.
    let sk = v.get("signing_key").and_then(|x| x.as_str()).filter(|s| !s.is_empty())?;
    pubkey_of_key(Path::new(sk))
}

/// Read the ssh public key for a signing-key path (the `.pub` beside it, or the path itself if it
/// already is a `.pub`). Mirrors `main::read_pubkey`, kept local so the resolver is self-contained.
fn pubkey_of_key(key: &Path) -> Option<String> {
    let pubpath = if key.extension().and_then(|e| e.to_str()) == Some("pub") {
        key.to_path_buf()
    } else {
        let mut s = key.as_os_str().to_os_string();
        s.push(".pub");
        PathBuf::from(s)
    };
    let pk = std::fs::read_to_string(&pubpath).ok()?.trim().to_string();
    (!pk.is_empty()).then_some(pk)
}

/// Persist `pubkey` into a clone's `identity.json` if it isn't already recorded — so the resolver
/// can verify the clone by key without re-deriving from `signing_key` each time. Called by
/// `adopt-clone` after a move. Best-effort: a failure is not fatal (the derive fallback in
/// [`identity_pubkey`] still resolves the clone).
pub fn backfill_pubkey(clone: &Path, pubkey: &str) {
    let idf = clone.join(".confer").join("identity.json");
    let Ok(txt) = std::fs::read_to_string(&idf) else { return };
    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&txt) else { return };
    if v.get("pubkey").and_then(|x| x.as_str()).filter(|s| !s.is_empty()).is_some() {
        return;
    }
    if let Some(obj) = v.as_object_mut() {
        obj.insert("pubkey".into(), serde_json::Value::String(pubkey.to_string()));
        if let Ok(s) = serde_json::to_string_pretty(&v) {
            let _ = std::fs::write(&idf, s);
        }
    }
}

/// Does a managed dir NAME carry exactly this stable tag as its trailing segment? Requires the tag
/// to be WELL-FORMED (12 lowercase hex), so a foreign `backup-2024`-style dir is never mistaken for
/// a managed clone.
fn tag_matches(dir_name: &str, want: &str) -> bool {
    matches!(split_tag(dir_name), Some((_, t)) if t == want && is_tag(t))
}

fn is_tag(s: &str) -> bool {
    s.len() == 12 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Resolve "my clone of this hub as this identity" without trusting any name — matches on the
/// hub_key + key tags AND verifies the resident pubkey. Returns the on-disk path if present.
pub fn resolve(hub_key: &str, pubkey: &str) -> anyhow::Result<Option<PathBuf>> {
    Ok(resolve_by_tags(&root()?, &hub_tag(hub_key), &key_tag(pubkey), Some(pubkey)))
}

/// One managed clone on disk (for `confer clones`).
pub struct ManagedClone {
    pub path: PathBuf,
    pub role: String,
    pub hub_slug: String,
}

/// Enumerate the managed clones under `~/.confer/clones/`. Best-effort; skips non-clone dirs.
pub fn list() -> Vec<ManagedClone> {
    let mut out = Vec::new();
    let Ok(root) = root() else {
        return out;
    };
    // A MISSING managed home is the legit "no managed clones yet" case (silent). Any OTHER read
    // failure (permissions, an unmounted volume) means the list is INCOMPLETE — surface it loudly
    // rather than returning a confident-but-partial empty, which reads as "no clones" downstream
    // (`confer hubs`/`clones`) — the same silent-omission class the hubs command was written to fix.
    let hubs = match std::fs::read_dir(&root) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return out,
        Err(e) => {
            eprintln!(
                "confer: ⚠ cannot read the managed-clone home {} ({e}) — the managed-clone list may be INCOMPLETE.",
                root.display()
            );
            return out;
        }
    };
    for h in hubs.flatten() {
        if !h.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let hname = h.file_name().to_string_lossy().to_string();
        let hub_slug = split_tag(&hname).map(|(s, _)| s.to_string()).unwrap_or(hname);
        let roles = match std::fs::read_dir(h.path()) {
            Ok(rd) => rd,
            Err(e) => {
                // The hub dir exists but its contents can't be read — its clones are dropped. Say so
                // rather than silently omitting a whole hub from `confer hubs`.
                eprintln!(
                    "confer: ⚠ cannot read managed hub dir {} ({e}) — its clones are OMITTED from the list.",
                    h.path().display()
                );
                continue;
            }
        };
        for r in roles.flatten() {
            if !r.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            if !r.path().join(".confer").join("identity.json").is_file() {
                continue;
            }
            let rname = r.file_name().to_string_lossy().to_string();
            let role = std::fs::read_to_string(r.path().join(".confer").join("identity.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| v.get("role").and_then(|x| x.as_str()).map(String::from))
                .unwrap_or_else(|| split_tag(&rname).map(|(s, _)| s.to_string()).unwrap_or(rname));
            out.push(ManagedClone { path: r.path(), role, hub_slug: hub_slug.clone() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn tmp() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("confer-clonehome-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn slug_is_readable_but_path_safe() {
        assert_eq!(slug("alice"), "alice");
        assert_eq!(slug("carol-role"), "carol-role"); // internal hyphen kept as-is
        assert_eq!(slug("Carol_Role"), "carol_role"); // lowercased, `_` survives
        // hostile / weird inputs can never escape the dir or smuggle separators:
        assert_eq!(slug("../../etc/passwd"), "etc-passwd"); // no `/`, no `..`
        assert_eq!(slug("a  b\tc"), "a-b-c"); // whitespace runs collapse
        assert_eq!(slug("héllo"), "h-llo"); // non-ascii → dash
        assert_eq!(slug("--weird--"), "weird"); // trimmed
        assert_eq!(slug(""), "x"); // never empty
        assert_eq!(slug("!!!"), "x"); // all-symbol → fallback
        assert!(slug(&"z".repeat(200)).len() <= 24); // capped
        assert!(!slug("../x").contains('/'));
    }

    #[test]
    fn tag_is_deterministic_hyphen_free_lowercase_hex() {
        let a = hub_tag("root-commit-sha-abc");
        assert_eq!(a, hub_tag("root-commit-sha-abc")); // stable
        assert_ne!(a, hub_tag("root-commit-sha-xyz")); // distinguishes
        assert_eq!(a.len(), 12);
        assert!(!a.contains('-')); // MUST be hyphen-free so split_tag is unambiguous
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn split_tag_grabs_the_last_part_even_with_hyphenated_slug() {
        // the case the human flagged: a role-id WITH hyphens + the tag appended.
        assert_eq!(split_tag("carol-role-1a2b3c4d5e6f"), Some(("carol-role", "1a2b3c4d5e6f")));
        assert_eq!(split_tag("alice-aabbccddeeff"), Some(("alice", "aabbccddeeff")));
        assert_eq!(split_tag("x-000000000000"), Some(("x", "000000000000")));
        assert_eq!(split_tag("notagseparator"), None); // foreign dir → ignored
    }

    #[test]
    fn resolve_matches_by_tag_despite_a_relabeled_prefix() {
        // Simulates a rename/relabel: the identity's clone was created as `alice-<tag>`,
        // but the on-disk prefix later reads differently. Resolution must STILL find it by the
        // stable key tag — that's the whole point of anchoring on the tag, not the name.
        let root = tmp();
        let hub_t = "3f9a1c024b8e";
        let key_t = "1a2b3c4d5e6f";
        let hub_dir = root.join(format!("team-hub-{hub_t}"));
        let role_dir = hub_dir.join(format!("helper-renamed-{key_t}")); // prefix != original
        // a real clone has .confer/identity.json — required for resolution to accept it.
        std::fs::create_dir_all(role_dir.join(".confer")).unwrap();
        std::fs::write(role_dir.join(".confer").join("identity.json"), "{}").unwrap();
        // also drop a co-resident PEER identity in the same hub — must NOT be returned.
        std::fs::create_dir_all(hub_dir.join("carol-999999999999")).unwrap();

        assert_eq!(resolve_by_tags(&root, hub_t, key_t, None), Some(role_dir));
        // wrong key tag / wrong hub tag → no match
        assert_eq!(resolve_by_tags(&root, hub_t, "ffffffffffff", None), None);
        assert_eq!(resolve_by_tags(&root, "ffffffffffff", key_t, None), None);
        // empty/absent home → None, never a panic
        assert_eq!(resolve_by_tags(&tmp(), hub_t, key_t, None), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_fails_closed_when_two_clones_share_a_key_tag() {
        // Two dirs matching the same (hub, key) tag, both real clones — resolution must refuse
        // (return None) rather than pick a read_dir-order-dependent, possibly-planted one.
        let root = tmp();
        let hub_t = "3f9a1c024b8e";
        let key_t = "1a2b3c4d5e6f";
        let hub_dir = root.join(format!("team-hub-{hub_t}"));
        for slug in ["real", "imposter"] {
            let d = hub_dir.join(format!("{slug}-{key_t}"));
            std::fs::create_dir_all(d.join(".confer")).unwrap();
            std::fs::write(d.join(".confer").join("identity.json"), "{}").unwrap();
        }
        assert_eq!(resolve_by_tags(&root, hub_t, key_t, None), None, "ambiguous tag must fail closed");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_verifies_the_pubkey_not_just_the_tag() {
        // Two clones whose dir names carry the SAME (hub, key) tag but DIFFERENT recorded pubkeys
        // (a replayed collision — the tag comes from the PUBLIC key). resolve must return the one
        // whose identity.json pubkey MATCHES the expected key, not a read_dir-order guess.
        let root = tmp();
        let hub_t = "aaaaaaaaaaaa";
        let key_t = "bbbbbbbbbbbb";
        let hub_dir = root.join(format!("h-{hub_t}"));
        for (slug, pk) in [("real", "ssh-ed25519 REALKEY x"), ("evil", "ssh-ed25519 EVILKEY y")] {
            let d = hub_dir.join(format!("{slug}-{key_t}"));
            std::fs::create_dir_all(d.join(".confer")).unwrap();
            std::fs::write(d.join(".confer").join("identity.json"), format!("{{\"pubkey\":\"{pk}\"}}")).unwrap();
        }
        // same key MATERIAL as REAL (different comment) → resolves the real clone, never the evil one.
        let hit = resolve_by_tags(&root, hub_t, key_t, Some("ssh-ed25519 REALKEY other-comment"));
        assert_eq!(hit, Some(hub_dir.join(format!("real-{key_t}"))));
        // a key matching NEITHER recorded pubkey → no match (not a blind tag hit).
        assert_eq!(resolve_by_tags(&root, hub_t, key_t, Some("ssh-ed25519 STRANGER z")), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_rejects_a_pubkeyless_clone_when_a_key_is_expected() {
        // Fail closed — a planted dir whose identity.json OMITS `pubkey` must NOT resolve as
        // authoritative just because its tag matches (a review finding).
        let root = tmp();
        let hub_t = "cccccccccccc";
        let key_t = "dddddddddddd";
        let d = root.join(format!("h-{hub_t}")).join(format!("planted-{key_t}"));
        std::fs::create_dir_all(d.join(".confer")).unwrap();
        std::fs::write(d.join(".confer").join("identity.json"), "{}").unwrap(); // no pubkey
        assert_eq!(resolve_by_tags(&root, hub_t, key_t, Some("ssh-ed25519 SOMEKEY x")), None);
        // a caller with NO expected key still falls back to the tag (back-compat, e.g. `clones`).
        assert!(resolve_by_tags(&root, hub_t, key_t, None).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_recovers_pubkey_from_signing_key_for_a_legacy_clone() {
        // A clone joined BEFORE `pubkey` was recorded: identity.json has `signing_key` but no
        // `pubkey`. resolve must recover the key from `<signing_key>.pub` and match it, so `where`
        // stops reporting "not managed yet" for an already-adopted clone (a fleet-migration
        // finding). Still key-verified — a signing_key whose pub doesn't match is rejected.
        let root = tmp();
        let hub_t = "eeeeeeeeeeee";
        let key_t = "ffffffffffff";
        let keydir = tmp();
        std::fs::create_dir_all(&keydir).unwrap();
        let keypath = keydir.join("agentkey");
        std::fs::write(keydir.join("agentkey.pub"), "ssh-ed25519 LEGACYKEY host\n").unwrap();
        let d = root.join(format!("h-{hub_t}")).join(format!("legacy-{key_t}"));
        std::fs::create_dir_all(d.join(".confer")).unwrap();
        std::fs::write(
            d.join(".confer").join("identity.json"),
            format!("{{\"role\":\"x\",\"signing_key\":\"{}\"}}", keypath.display()),
        )
        .unwrap();
        // identity_pubkey recovers the key from signing_key.pub
        assert_eq!(identity_pubkey(&d).as_deref(), Some("ssh-ed25519 LEGACYKEY host"));
        // and resolve matches it (comment ignored), but a DIFFERENT expected key fails closed
        assert_eq!(resolve_by_tags(&root, hub_t, key_t, Some("ssh-ed25519 LEGACYKEY other")), Some(d));
        assert_eq!(resolve_by_tags(&root, hub_t, key_t, Some("ssh-ed25519 OTHERKEY z")), None);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&keydir);
    }

    #[test]
    fn backfill_pubkey_writes_missing_and_preserves_present() {
        let root = tmp();
        let d = root.join("c");
        std::fs::create_dir_all(d.join(".confer")).unwrap();
        let idf = d.join(".confer").join("identity.json");
        std::fs::write(&idf, "{\"role\":\"x\"}").unwrap();
        backfill_pubkey(&d, "ssh-ed25519 KKK x");
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&idf).unwrap()).unwrap();
        assert_eq!(v.get("pubkey").and_then(|x| x.as_str()), Some("ssh-ed25519 KKK x"));
        assert_eq!(v.get("role").and_then(|x| x.as_str()), Some("x"), "existing fields preserved");
        // does NOT overwrite an already-recorded pubkey
        backfill_pubkey(&d, "ssh-ed25519 DIFFERENT y");
        let v2: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&idf).unwrap()).unwrap();
        assert_eq!(v2.get("pubkey").and_then(|x| x.as_str()), Some("ssh-ed25519 KKK x"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_ignores_a_foreign_dir_with_a_non_tag_suffix() {
        // A dir like `backup-2024` has a trailing segment but it isn't a 12-hex tag → never
        // treated as a managed clone.
        let root = tmp();
        let hub_dir = root.join("backup-2024");
        std::fs::create_dir_all(hub_dir.join("stuff-notahextag")).unwrap();
        assert_eq!(resolve_by_tags(&root, "2024", "notahextag", None), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_skips_a_bare_tag_colliding_dir_that_is_not_a_clone() {
        // A dir whose name matches both tags but is NOT a real confer clone (no
        // .confer/identity.json) must be ignored, not returned — the first line of defence
        // against a tag collision landing on a random directory.
        let root = tmp();
        let hub_t = "aaaaaaaaaaaa";
        let key_t = "bbbbbbbbbbbb";
        let hub_dir = root.join(format!("h-{hub_t}"));
        std::fs::create_dir_all(hub_dir.join(format!("imposter-{key_t}"))).unwrap(); // no .confer
        assert_eq!(resolve_by_tags(&root, hub_t, key_t, None), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn key_tag_is_environment_stable_and_ignores_the_comment() {
        // Same key material → same tag regardless of the trailing comment, and derived purely
        // in-process (no ssh-keygen), so it never flips across machines.
        let a = key_tag("ssh-ed25519 AAAAC3NzBODYMATERIAL helper@host-a");
        let b = key_tag("ssh-ed25519 AAAAC3NzBODYMATERIAL someone@else");
        assert_eq!(a, b); // comment (3rd field) ignored
        assert_ne!(a, key_tag("ssh-ed25519 DIFFERENTBODYMATERIAL x")); // body (2nd field) matters
        assert!(!a.contains('-') && a.len() == 12);
    }

    #[test]
    fn dir_names_compose_and_round_trip() {
        let name = hub_dir_name("Team Hub!", "some-root-sha");
        let (prefix, tag) = split_tag(&name).unwrap();
        assert_eq!(prefix, "team-hub"); // slugged
        assert_eq!(tag, hub_tag("some-root-sha")); // stable tag recoverable
    }
}
