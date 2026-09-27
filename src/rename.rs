//! Verified role-rename links: fold an old role id into the role it became, but ONLY when
//! that fold is cryptographically earned — never from a bare self-declared claim.
//!
//! A role card is data any hub writer can rewrite (`roster.rs` reads it as such), so a naive
//! `renamed_from: [victim]` on an attacker's card would let it hijack `whois victim` and redirect
//! peers to the impostor. The fix mirrors how `status` is honored (`verify::card_trust`): a card
//! field only counts when the card's LATEST edit is signature-verified against that role's pinned
//! key. A link `old -> new` is VERIFIED iff either:
//!   1. `old` and `new` publish the SAME pubkey (both cards Verified) — the strongest proof, since
//!      a role can only publish a pubkey it can sign with (TOFU-pins on first sight, so a copied
//!      pubkey string that isn't actually held fails that role's OWN card-trust check); or
//!   2. BOTH sides agree: `new`'s (Verified) card has `renamed_from` containing `old`, AND `old`'s
//!      (Verified) card has `renamed_to: new`.
//!
//! A one-sided claim, or a claim riding an unverified/mismatched card, is surfaced only as an
//! unverified claim — never folded.

use crate::{gitcmd, roster, verify};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// One side made a rename claim that did not (yet, or ever) earn a verified link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedClaim {
    pub old: String,
    pub new: String,
    /// Which role's card carried the claim we couldn't corroborate.
    pub claimed_by: String,
}

/// Per-role resolution: only a role that is the TERMINUS of a verified chain carries
/// `renamed_from` (the verified predecessors that fold into it); only a role with a verified
/// outgoing link carries `renamed_to` (the FINAL role at the end of its chain, already chased
/// through any intermediate hops — `a -> b -> c` gives `renamed_to("a") == Some("c")`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolution {
    pub renamed_from: Vec<String>,
    pub renamed_to: Option<String>,
}

impl Resolution {
    pub fn verified(&self) -> bool {
        self.renamed_to.is_some() || !self.renamed_from.is_empty()
    }
}

pub struct RenameLinks {
    resolutions: HashMap<String, Resolution>,
    pub unverified: Vec<UnverifiedClaim>,
}

impl RenameLinks {
    pub fn get(&self, id: &str) -> Resolution {
        self.resolutions.get(id).cloned().unwrap_or_default()
    }

    /// Unverified claims that name `id` on either end (for a display-time footnote).
    pub fn claims_touching(&self, id: &str) -> Vec<&UnverifiedClaim> {
        self.unverified
            .iter()
            .filter(|c| c.old == id || c.new == id)
            .collect()
    }
}

/// The oldest commit timestamp (`%at`, unix seconds) touching a role's card — used ONLY to break
/// the direction of a pubkey-match link (rule 1 carries no old/new of its own): whichever card was
/// created first is `old`. An objective git-log fact, not a self-declared field, so it can't be
/// gamed by either side's card content.
fn card_first_seen(root: &Path, role: &str) -> Option<i64> {
    let rel = format!("roles/{role}.md");
    let out = gitcmd::output(root, &["log", "--format=%at", "--", &rel]).ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .last() // git log is newest-first; the LAST line is the oldest commit
        .and_then(|s| s.trim().parse::<i64>().ok())
}

/// Build the verified rename graph for every role in `roster`, plus the unverified claims left
/// over. Best-effort/read-only: any IO hiccup just drops a role from consideration, never panics.
pub fn resolve(
    root: &Path,
    hub_key: &str,
    ros: &roster::Roster,
    cache: &mut verify::Cache,
) -> RenameLinks {
    // Card-trust is per-role and shared by both rules below — compute each role's card trust
    // (and hence "is this role's own field data honored") exactly once.
    let verified: HashSet<String> = ros
        .keys()
        .filter(|id| matches!(verify::card_trust(root, hub_key, ros, cache, id), verify::Trust::Verified { .. }))
        .cloned()
        .collect();

    // Direct verified edges (old -> new), deduped.
    let mut edges: HashMap<String, String> = HashMap::new();
    let mut unverified = Vec::new();

    // Rule 1 — same pubkey, both cards Verified. Direction from first-seen commit timestamp.
    let ids: Vec<&String> = ros.keys().collect();
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let (a, b) = (ids[i], ids[j]);
            if !verified.contains(a) || !verified.contains(b) {
                continue;
            }
            let (Some(pa), Some(pb)) = (roster::pubkey(ros, a), roster::pubkey(ros, b)) else {
                continue;
            };
            if pa != pb {
                continue;
            }
            let (ta, tb) = (card_first_seen(root, a), card_first_seen(root, b));
            match (ta, tb) {
                (Some(ta), Some(tb)) if ta < tb => {
                    edges.insert(a.clone(), b.clone());
                }
                (Some(ta), Some(tb)) if tb < ta => {
                    edges.insert(b.clone(), a.clone());
                }
                _ => {} // equal/unknown timestamps: same-key link is real, but direction is
                        // ambiguous — skip rather than guess.
            }
        }
    }

    // Rule 2 — mutual, both-signed renamed_to/renamed_from agreement.
    for (old, role) in ros {
        let Some(new) = &role.renamed_to else { continue };
        if new == old || !ros.contains_key(new) {
            continue; // self-reference or a claim about a role that doesn't exist
        }
        let old_ok = verified.contains(old);
        let new_ok = verified.contains(new)
            && ros
                .get(new)
                .is_some_and(|r| r.renamed_from.iter().any(|o| o == old));
        if old_ok && new_ok {
            edges.entry(old.clone()).or_insert_with(|| new.clone());
        } else {
            unverified.push(UnverifiedClaim { old: old.clone(), new: new.clone(), claimed_by: old.clone() });
        }
    }
    // The reverse direction: a `new` card claiming `renamed_from` an `old` that never
    // reciprocated (or whose card doesn't verify) — surfaced as `new`'s own unverified claim.
    for (new, role) in ros {
        for old in &role.renamed_from {
            if old == new || !ros.contains_key(old) {
                continue;
            }
            if edges.get(old) == Some(new) {
                continue; // already a verified edge (rule 1 or the reciprocated rule 2 above)
            }
            unverified.push(UnverifiedClaim { old: old.clone(), new: new.clone(), claimed_by: new.clone() });
        }
    }

    // Chase each role to its FINAL target, cycle-safe (a malicious or looping claim chain stops
    // at the last node seen rather than spinning forever).
    let final_of = |mut cur: String| -> String {
        let mut seen = HashSet::new();
        while let Some(next) = edges.get(&cur) {
            if !seen.insert(cur.clone()) || next == &cur {
                break;
            }
            cur = next.clone();
        }
        cur
    };

    let mut resolutions: HashMap<String, Resolution> = HashMap::new();
    for id in ros.keys() {
        if edges.contains_key(id) {
            let target = final_of(id.clone());
            if target != *id {
                resolutions.entry(id.clone()).or_default().renamed_to = Some(target.clone());
                resolutions
                    .entry(target)
                    .or_default()
                    .renamed_from
                    .push(id.clone());
            }
        }
    }
    for r in resolutions.values_mut() {
        r.renamed_from.sort();
        r.renamed_from.dedup();
    }

    RenameLinks { resolutions, unverified }
}
