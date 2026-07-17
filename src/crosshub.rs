//! Cross-hub identity recognition — F3 Phase 0 (see docs/06-identity-unity-review.md).
//!
//! The per-agent SSH signing key is the durable cross-hub anchor: the
//! SAME pubkey published in two hubs' role cards ⇒ the same agent. This module
//! records which hubs THIS machine's agent belongs to and matches pubkeys across
//! them. Recognition ONLY — no trust, no authorization (that's F4), and no
//! cross-hub data flow: matching reads only the agent's own local clones, never a
//! remote, so hubs stay isolated. Key reuse is the consent act; a fresh key in
//! someone else's hub means no linkage — the privacy is physics, not policy.

use crate::{config, gitcmd, roster};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Membership {
    pub dir: String,
    pub role: String,
}

#[derive(Serialize, Deserialize, Default)]
struct Registry {
    #[serde(default)]
    hubs: Vec<Membership>,
}

fn registry_path() -> Option<PathBuf> {
    config::home().ok().map(|h| h.join(".confer").join("hubs.json"))
}

fn load() -> Vec<Membership> {
    registry_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Registry>(&s).ok())
        .map(|r| r.hubs)
        .unwrap_or_default()
}

fn write_registry(hubs: &[Membership]) {
    if let Some(p) = registry_path() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(s) = serde_json::to_string_pretty(&Registry { hubs: hubs.to_vec() }) {
            let _ = std::fs::write(p, s);
        }
    }
}

/// A throwaway clone that must not persist in the registry: scratchpad/session
/// test hubs under `…/scratchpad/…` in a tempdir. (Deleted temp clones — including
/// the CLI test harness's own `confer-cli-…` dirs — are already dropped by the
/// gone-dir check in `prune`; matching *live* confer-cli dirs would wrongly delete
/// the clones an in-flight cross-hub test is actively using, so we don't.)
fn is_ephemeral(dir: &str) -> bool {
    let temp = dir.contains("/tmp/") || dir.contains("/T/") || dir.contains("/var/folders/");
    temp && dir.contains("/scratchpad/")
}

/// Drop registry entries whose dir is gone or is an ephemeral test clone, rewriting
/// hubs.json only if something changed. Best-effort GC so the registry (and every
/// consumer — `who`, `identity`, the dashboard's tab list) stays clean over time.
/// Fixes the accumulation papercut where CLI-test clones under $TMPDIR piled up.
pub fn prune() -> Vec<Membership> {
    let hubs = load();
    let kept: Vec<Membership> = hubs
        .iter()
        .filter(|m| Path::new(&m.dir).is_dir() && !is_ephemeral(&m.dir))
        .cloned()
        .collect();
    if kept.len() != hubs.len() {
        write_registry(&kept);
    }
    kept
}

/// Distinct hubs this agent follows (pruned), first-seen order, ONE dir per hub —
/// multiple local clones of the same hub (same root SHA) collapse to a single
/// entry. For the dashboard to enumerate followed hubs with no explicit `--hub`.
pub fn hub_dirs() -> Vec<PathBuf> {
    let mut seen_dir = std::collections::HashSet::new();
    let mut seen_hub = std::collections::HashSet::new();
    let mut out = Vec::new();
    for m in prune() {
        let dir = PathBuf::from(&m.dir);
        if !seen_dir.insert(dir.clone()) {
            continue;
        }
        // Skip a registered dir that ISN'T actually a confer hub — a dev/source dir that leaked into
        // the watch registry, or a clone since replaced by something else. Without this, `serve
        // --all-hubs` renders a broken "not a confer hub" tab and `--hub <name>` can match a non-hub.
        // A real hub carries the `.confer-version` scaffold marker (threads/roles are the fallback).
        if !(dir.join(".confer-version").exists()
            || dir.join("threads").is_dir()
            || dir.join("roles").is_dir())
        {
            continue;
        }
        // Collapse sibling clones of the SAME hub to one tab (root SHA = F3 anchor).
        let hub_key = root_sha(&dir).unwrap_or_else(|| m.dir.clone());
        if !seen_hub.insert(hub_key) {
            continue;
        }
        out.push(dir);
    }
    out
}

/// Record that this machine's agent belongs to `(dir, role)`. Idempotent,
/// best-effort — called on join and on `who`/`identity` so the set of hubs the
/// agent participates in builds up naturally. Prunes stale entries as it goes.
pub fn record(dir: &Path, role: &str) {
    if role.is_empty() {
        return;
    }
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()).to_string_lossy().to_string();
    let mut hubs = prune();
    let m = Membership { dir, role: role.to_string() };
    if !hubs.contains(&m) {
        hubs.push(m);
        write_registry(&hubs);
    }
}

/// Human-friendly label for a hub clone: `owner/repo` from its remote, else the
/// dir name marked `(local)` (a purely-local hub has no remote — still supported).
pub fn hub_label(dir: &Path) -> String {
    if let Ok(o) = gitcmd::output(dir, &["config", "--get", "remote.origin.url"]) {
        if o.status.success() {
            let url = String::from_utf8_lossy(&o.stdout).trim().trim_end_matches(".git").to_string();
            let normalized = url.replace(':', "/");
            let segs: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();
            if segs.len() >= 2 {
                return format!("{}/{}", segs[segs.len() - 2], segs[segs.len() - 1]);
            }
        }
    }
    // No remote (a purely-local hub) — label by dir name + the stable root-SHA so
    // it's still uniquely identifiable and re-host-proof (the F3 anchor).
    let anchor = root_sha(dir).map(|s| format!(" {}", &s[..s.len().min(8)])).unwrap_or_default();
    format!("{} (local{anchor})", dir.file_name().and_then(|s| s.to_str()).unwrap_or("hub"))
}

/// The root-commit SHA — identical across all clones of a repo, so it names a hub
/// even with no remote (the F3 namespacing anchor). Best-effort.
pub fn root_sha(dir: &Path) -> Option<String> {
    let o = gitcmd::output(dir, &["rev-list", "--max-parents=0", "HEAD"]).ok()?;
    if !o.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&o.stdout);
    s.split_whitespace().next().map(String::from)
}

/// An SSH pubkey's `SHA256:…` fingerprint (via ssh-keygen), for display. Falls back
/// to a short tail of the key body if ssh-keygen isn't available.
pub fn fingerprint(pubkey: &str) -> String {
    if let Ok(home) = config::home() {
        // A PER-CALL temp name (pid + monotonic counter) — a shared `fp.tmp` let concurrent
        // callers (e.g. parallel `join`/verify) race: one's `remove_file` could fire before
        // another's `ssh-keygen` read, silently forcing the fallback below.
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let tmp = home
            .join(".confer")
            .join(format!("fp.{}.{n}.tmp", std::process::id()));
        if std::fs::write(&tmp, pubkey).is_ok() {
            let out = std::process::Command::new("ssh-keygen").arg("-lf").arg(&tmp).output();
            let _ = std::fs::remove_file(&tmp);
            if let Ok(o) = out {
                if o.status.success() {
                    if let Some(fp) = String::from_utf8_lossy(&o.stdout)
                        .split_whitespace()
                        .find(|t| t.starts_with("SHA256:"))
                    {
                        return fp.to_string();
                    }
                }
            }
        }
    }
    // Fallback: last 8 CHARS (not bytes) — slicing bytes could split a multibyte UTF-8
    // sequence and panic on a hostile/malformed pubkey.
    let body = pubkey.split_whitespace().nth(1).unwrap_or(pubkey);
    let tail: String = {
        let chars: Vec<char> = body.chars().collect();
        chars[chars.len().saturating_sub(8)..].iter().collect()
    };
    format!("key:…{tail}")
}

/// pubkey → [(hub_label, role)] across every OTHER hub the agent belongs to (its
/// own local clones), excluding `exclude` (the current hub, canonicalized) so we
/// never self-match. Only role cards that publish a pubkey participate.
pub fn appearances(exclude: &Path) -> HashMap<String, Vec<(String, String)>> {
    let exclude = exclude.canonicalize().unwrap_or_else(|_| exclude.to_path_buf());
    let mut idx: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for m in prune() {
        let dir = PathBuf::from(&m.dir);
        if dir.canonicalize().map(|d| d == exclude).unwrap_or(false) || !dir.is_dir() {
            continue;
        }
        let label = hub_label(&dir);
        for (rid, role) in roster::load(&dir) {
            if let Some(pk) = role.pubkey {
                idx.entry(pk).or_default().push((label.clone(), rid));
            }
        }
    }
    // Dedupe each identity's appearances: the registry can list the same hub dir many times (one per
    // historical post/session), which otherwise repeats the same `hub:role` in the `≡` line and makes
    // `who`/dashboard hard to scan (field report). Collapse to unique, first-seen order.
    for v in idx.values_mut() {
        let mut seen = std::collections::HashSet::new();
        v.retain(|pair| seen.insert(pair.clone()));
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::is_ephemeral;

    #[test]
    fn ephemeral_matches_scratchpad_test_hubs_only() {
        // scratchpad/session test hubs under a tempdir → ephemeral.
        assert!(is_ephemeral("/private/tmp/claude-501/x/scratchpad/tasklayer/wd"));
        assert!(is_ephemeral("/var/folders/tw/x/T/sess/scratchpad/hub"));
        // Live CLI test clones are NOT force-pruned (a running cross-hub test needs
        // them); they get dropped only once deleted, by prune's gone-dir check.
        assert!(!is_ephemeral("/private/var/folders/tw/x/T/confer-cli-5118-clone-carol-41"));
        // Real hubs are kept.
        assert!(!is_ephemeral("/Users/me/git/team-hub"));
        assert!(!is_ephemeral("/Users/me/git/proj/team-hub"));
        assert!(!is_ephemeral("/home/me/scratchpad-notes")); // marker but not under a tempdir
    }
}
