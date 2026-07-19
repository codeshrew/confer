//! Fuzzy identity: resolve a human's loose phrase ("my iOS agent", "the book one")
//! to a role, and guard alias additions against collisions. All the
//! matching is normalized + token-based so casual phrasing still lands.

use crate::roster::Roster;

/// Lowercase, strip punctuation to spaces, collapse whitespace.
pub fn normalize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Homoglyph / mixed-script red flag for a DISPLAY name. Role *ids* are
/// already ASCII-only (`valid_slug`), but a free-form display like `gitcоnv` (Cyrillic
/// `о`) renders in every wake line and can impersonate a fleet agent. We flag a name that
/// mixes ASCII-Latin letters with letters from the Cyrillic or Greek blocks — the classic
/// Latin-lookalike sources — which catches the attack without tripping on accented-Latin
/// names (`José`) or a wholly non-Latin display. Dependency-free (no full TR39).
pub fn homoglyph_risk(s: &str) -> bool {
    let has_ascii_letter = s.chars().any(|c| c.is_ascii_alphabetic());
    let has_confusable = s.chars().any(|c| {
        let cp = c as u32;
        (0x0370..=0x03FF).contains(&cp) // Greek
            || (0x0400..=0x04FF).contains(&cp) // Cyrillic
    });
    has_ascii_letter && has_confusable
}

/// Significant tokens (drop filler the human sprinkles in).
fn tokens(s: &str) -> Vec<String> {
    const STOP: &[&str] = &["the", "a", "an", "my", "our", "your", "that", "one", "agent", "is"];
    normalize(s)
        .split_whitespace()
        .filter(|t| !STOP.contains(t))
        .map(String::from)
        .collect()
}

/// Levenshtein edit distance (for typo-closeness on short handles).
fn lev(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            cur.push((prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Do the significant tokens of the shorter string fully sit inside the longer's?
/// ("reader" ⊆ "mobile reader" → true; "design studio" vs "design review" → false)
/// Are ALL of `a`'s significant words already in `b` (a ⊆ b)? DIRECTIONAL on purpose. A proposed name
/// that is a subset of an existing one has NO distinguishing word — a loose reference to it also matches
/// the existing name, so it's genuinely ambiguous and must be blocked. A proposed name that ADDS a
/// distinguisher (a superset, or a partial overlap with a new word) is always resolvable, so it's
/// allowed — otherwise deliberate family names would each need `--force` for no ambiguity benefit: e.g.
/// "Architecture Orbit" ⊃ "Orbit" is strictly *more* specific, never confusable with the base "Orbit"
/// (field report). The exact-match + typo checks (`conflict`) still catch the genuinely confusable cases.
fn token_subset(a: &str, b: &str) -> bool {
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.is_empty() || tb.is_empty() {
        return false;
    }
    ta.iter().all(|t| tb.contains(t))
}

/// Every identifier string for a role: its id, display, and aliases.
fn identifiers<'a>(roster: &'a Roster, id: &'a str) -> Vec<String> {
    let mut v = vec![id.to_string()];
    if let Some(r) = roster.get(id) {
        if let Some(d) = &r.display {
            v.push(d.clone());
        }
        v.extend(r.aliases.iter().cloned());
        if let Some(h) = &r.host {
            v.push(h.clone());
        }
    }
    v
}

/// Is `proposed` too close to some OTHER role's identifier to be a safe alias for
/// `me`? Returns the conflicting role + string + reason. `None` = clear to add.
pub fn conflict(roster: &Roster, me: &str, proposed: &str) -> Option<(String, String, String)> {
    let np = normalize(proposed);
    if np.is_empty() {
        return Some((String::new(), proposed.to_string(), "empty after normalizing".into()));
    }
    for id in roster.keys() {
        if id == me {
            continue;
        }
        for ident in identifiers(roster, id) {
            let ni = normalize(&ident);
            if ni.is_empty() {
                continue;
            }
            if ni == np {
                return Some((id.clone(), ident, "already identifies".into()));
            }
            if token_subset(&np, &ni) {
                return Some((id.clone(), ident, "shares all significant words with".into()));
            }
            // typo-closeness, but only for handles long enough that a 1-edit
            // difference is likely a mistake (avoid blocking distinct short tokens).
            if np.len() >= 4 && ni.len() >= 4 && lev(&np, &ni) <= 1 {
                return Some((id.clone(), ident, "is one keystroke from".into()));
            }
        }
    }
    None
}

/// A scored `whois` match.
pub struct Match {
    pub id: String,
    pub score: i32,
}

/// Rank every role by how well it answers the loose phrase. Empty = no signal.
pub fn resolve(roster: &Roster, phrase: &str) -> Vec<Match> {
    let np = normalize(phrase);
    let pt = tokens(phrase);
    let mut out: Vec<Match> = Vec::new();
    for id in roster.keys() {
        let mut score = 0i32;
        // Match against handles (id/display/aliases/host) AND the freeform desc —
        // the description is often the richest signal ("mobile reader app" → "mobile").
        let mut targets = identifiers(roster, id);
        if let Some(d) = roster.get(id).and_then(|r| r.desc.as_deref()) {
            targets.push(d.to_string());
        }
        for ident in &targets {
            let ni = normalize(ident);
            if ni.is_empty() {
                continue;
            }
            let it = tokens(ident);
            if ni == np {
                score = score.max(100);
            } else if !pt.is_empty() && pt.iter().all(|t| it.contains(t)) {
                score = score.max(75); // everything you said is in this identifier
            } else if !it.is_empty() && it.iter().all(|t| pt.contains(t)) {
                score = score.max(65); // this identifier is fully within what you said
            } else if ni.contains(&np) || np.contains(&ni) {
                score = score.max(45);
            } else {
                let shared = pt.iter().filter(|t| it.contains(*t)).count() as i32;
                if shared > 0 {
                    score = score.max(25 + 10 * shared);
                }
            }
        }
        if score > 0 {
            out.push(Match { id: id.clone(), score });
        }
    }
    out.sort_by(|a, b| b.score.cmp(&a.score).then(a.id.cmp(&b.id)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roster::Role;

    fn role(display: &str, host: &str, desc: &str, aliases: &[&str]) -> Role {
        Role {
            display: Some(display.into()),
            host: Some(host.into()),
            desc: (!desc.is_empty()).then(|| desc.to_string()),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
            status: None,
            pubkey: None,
            profile: None,
        }
    }

    fn fixture() -> Roster {
        let mut m = Roster::new();
        m.insert("bob".into(), role("Mobile Reader", "host-b.local", "mobile reader app", &["mobile agent", "spare box"]));
        m.insert("carol".into(), role("Design Studio", "host-a.local", "document layout + typesetting", &["design agent"]));
        m
    }

    #[test]
    fn homoglyph_risk_flags_mixed_latin_cyrillic_not_legit_names() {
        assert!(homoglyph_risk("ali\u{0441}e")); // Cyrillic с among Latin — the attack
        assert!(homoglyph_risk("A\u{0430}ron")); // Cyrillic а
        assert!(!homoglyph_risk("alice")); // pure ASCII
        assert!(!homoglyph_risk("José")); // accented Latin — fine
        assert!(!homoglyph_risk("Confer / confer repo")); // punctuation/spaces — fine
        assert!(!homoglyph_risk("Алиса")); // wholly Cyrillic (no Latin to confuse) — fine
    }

    #[test]
    fn resolve_loose_phrases() {
        let r = fixture();
        assert_eq!(resolve(&r, "my mobile agent")[0].id, "bob");
        assert_eq!(resolve(&r, "the spare box one")[0].id, "bob");
        assert_eq!(resolve(&r, "design")[0].id, "carol");
        // matches on the free-form description, not just handles:
        assert_eq!(resolve(&r, "the reader app")[0].id, "bob");
        assert_eq!(resolve(&r, "typesetting")[0].id, "carol");
    }

    #[test]
    fn conflict_blocks_collisions_not_distinct() {
        let r = fixture();
        // exact + subset + typo collisions with an OTHER role are blocked:
        assert!(conflict(&r, "carol", "mobile agent").is_some()); // bob's alias
        assert!(conflict(&r, "carol", "bob").is_some()); // bob's id
        assert!(conflict(&r, "carol", "spare  box").is_some()); // normalizes to bob's alias
        // a genuinely distinct alias for carol is allowed:
        assert!(conflict(&r, "carol", "printer").is_none());
        // re-adding my OWN identifier is not a conflict (me is skipped):
        assert!(conflict(&r, "bob", "mobile agent").is_none());
    }

    #[test]
    fn collision_is_directional_superset_ok_subset_still_blocked() {
        // A name that ADDS a distinguishing word (superset / partial overlap with a new word) is
        // always resolvable → allowed. A name that is a bare SUBSET (drops all distinguishers) is
        // still ambiguous → blocked. This dissolves the family-naming friction WITHOUT weakening the
        // guard (field report): confer recommends `<domain>-orbit`, whose display is a superset of the
        // base `orbit`, so every member used to need `--force` for no ambiguity benefit.
        let mut r = Roster::new();
        r.insert("arch-orbit".into(), role("Architecture Orbit", "athena.local", "architecture", &[]));

        // supersets / siblings — a distinguishing word makes them resolvable → allowed:
        assert!(conflict(&r, "orbit", "Orbit Prime").is_none()); // {orbit,prime} ⊄ {architecture,orbit}
        assert!(conflict(&r, "graph-orbit", "Graph Orbit").is_none()); // {graph,orbit} ⊄ {architecture,orbit}

        // bare subsets — no distinguisher → still blocked (a loose ref matches "Architecture Orbit"):
        assert!(conflict(&r, "orbit", "Orbit").is_some()); // {orbit} ⊆ {architecture,orbit}
        assert!(conflict(&r, "x", "Architecture").is_some()); // {architecture} ⊆ {architecture,orbit}

        // exact + reordered duplicates still blocked:
        assert!(conflict(&r, "x", "architecture orbit").is_some());
        assert!(conflict(&r, "x", "Orbit Architecture").is_some()); // same words, reordered
    }
}
