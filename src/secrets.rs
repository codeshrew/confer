//! Secret-shape lint for `append` — cheap insurance against a pasted token/key
//! becoming a PERMANENT, fleet-wide leak in the message history (a review finding:
//! messages are immutable markdown in a repo many agents clone). A hit blocks the
//! append unless `--allow-secret` is passed.
//!
//! Deliberately pattern-based (prefix + charset + length) on the highest-impact,
//! low-false-positive shapes — NOT a generic entropy scan (which cries wolf and gets
//! reflexively overridden). Hand-rolled to keep confer dependency-light.

pub struct Finding {
    pub kind: &'static str,
    /// A redacted preview safe to show in the warning (never the full secret).
    pub preview: String,
}

/// A token character (what a secret is made of); used to slice candidate tokens out
/// of surrounding punctuation/quotes/markdown.
fn is_tok(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

fn redact(tok: &str) -> String {
    let n = tok.chars().count();
    if n <= 10 {
        format!("{}…", &tok[..tok.len().min(4)])
    } else {
        let head: String = tok.chars().take(6).collect();
        format!("{head}…({n} chars)")
    }
}

/// Classify a single candidate token as a known secret shape, if it is one.
fn classify(tok: &str) -> Option<&'static str> {
    let n = tok.len();
    let after = |p: &str| tok.strip_prefix(p);
    let alnum = |s: &str| s.chars().all(|c| c.is_ascii_alphanumeric());
    let alnum_us = |s: &str| s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');

    // AWS access key id: AKIA + 16 uppercase/digits (exactly 20).
    if let Some(rest) = after("AKIA") {
        if rest.len() == 16 && rest.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
            return Some("aws-access-key-id");
        }
    }
    // GitHub tokens: ghp_/gho_/ghu_/ghs_/ghr_ + >=30 alnum.
    for p in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        if let Some(rest) = after(p) {
            if rest.len() >= 30 && alnum(rest) {
                return Some("github-token");
            }
        }
    }
    // GitHub fine-grained PAT: github_pat_ + long body (letters/digits/underscore).
    // This is the credential DESIGN.md recommends per agent, i.e. the one most likely
    // to be pasted into a message (Fable review) — the ghp_ family above missed it.
    if let Some(rest) = after("github_pat_") {
        if rest.len() >= 40 && alnum_us(rest) {
            return Some("github-token");
        }
    }
    // Slack: xoxb-/xoxp-/xoxa-/xoxr-/xoxs- + token body.
    if tok.starts_with("xox") && n >= 15 && tok.as_bytes().get(4) == Some(&b'-') {
        return Some("slack-token");
    }
    // Google API key: AIza + 35 token chars.
    if let Some(rest) = after("AIza") {
        if rest.len() == 35 && alnum_us(rest) {
            return Some("google-api-key");
        }
    }
    // Stripe live secret/restricted key.
    if (tok.starts_with("sk_live_") || tok.starts_with("rk_live_")) && n >= 20 && alnum_us(&tok[8..]) {
        return Some("stripe-live-key");
    }
    // OpenAI-style secret key: sk- + long token (>=32) — narrower than Stripe test keys.
    if let Some(rest) = after("sk-") {
        if rest.len() >= 32 && alnum_us(rest) {
            return Some("api-secret-key");
        }
    }
    // NB: no generic "long hex" rule — git SHAs (40 hex) and SHA-256 (64 hex) appear
    // constantly in confer messages, so it was pure false-positive (a review finding).
    // We only flag PREFIXED, high-signal shapes above. A bare hex secret with no
    // prefix (e.g. a raw AWS secret access key) is a known miss — the accompanying
    // AKIA id usually trips the block, and prefix-free detection is too noisy.
    None
}

/// Scan text for secret shapes. Returns one finding per distinct match (deduped by
/// kind+preview). A non-empty result should block an append unless overridden.
pub fn scan(text: &str) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    let mut push = |kind: &'static str, tok: &str| {
        let preview = redact(tok);
        if !out.iter().any(|f| f.kind == kind && f.preview == preview) {
            out.push(Finding { kind, preview });
        }
    };

    // Whole-text markers first: PEM/OpenSSH private key blocks.
    if text.contains("PRIVATE KEY-----") && text.contains("-----BEGIN") {
        push("private-key-block", "-----BEGIN…PRIVATE KEY-----");
    }

    // Then per-token classification.
    for raw in text.split(|c: char| !is_tok(c)) {
        if raw.len() < 8 {
            continue;
        }
        if let Some(kind) = classify(raw) {
            push(kind, raw);
        }
    }
    out
}

/// One-line summary of findings for an error message.
pub fn summarize(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| format!("{} ({})", f.kind, f.preview))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<&'static str> {
        let mut k: Vec<&'static str> = scan(text).into_iter().map(|f| f.kind).collect();
        k.sort();
        k.dedup();
        k
    }

    #[test]
    fn catches_common_secret_shapes() {
        assert_eq!(kinds("key AKIAIOSFODNN7EXAMPLE here"), vec!["aws-access-key-id"]);
        assert!(kinds(&format!("token=ghp_{}", "a".repeat(36))).contains(&"github-token"));
        assert!(kinds(&format!("pat github_pat_{}", "A1_".repeat(20))).contains(&"github-token"));
        assert!(kinds("slack xoxb-2401-abc-def123").contains(&"slack-token"));
        assert!(kinds(&format!("g AIza{}", "b".repeat(35))).contains(&"google-api-key")); // real keys are AIza + 35
        assert!(kinds(&format!("stripe sk_live_{}", "c".repeat(24))).contains(&"stripe-live-key"));
        assert!(kinds(&format!("openai sk-{}", "d".repeat(36))).contains(&"api-secret-key"));
        assert!(kinds("-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END").contains(&"private-key-block"));
    }

    #[test]
    fn ignores_ordinary_prose_and_ids() {
        // Normal words, short ids, ULIDs, git shas (full 40-hex + SHA-256 64-hex),
        // and urls → no findings. Git SHAs are everywhere in confer, so they must NOT
        // be flagged (a review finding).
        for clean in [
            "please rebake the plate gallery and reply when done",
            "see request 01KXA3TBY1G859FHKNFZ7Q91D1 on topic general",
            "commit b2308ab fixes the watch loop; see DESIGN.md",
            "full sha a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4 landed on main",
            "sha256 e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 digest",
            "https://github.com/codeshrew/team-hub.git",
            "the AKIA prefix alone is not a key",
        ] {
            assert!(scan(clean).is_empty(), "false positive on: {clean:?} -> {:?}", kinds(clean));
        }
    }

    #[test]
    fn dedupes_repeats() {
        let t = "ghp_0123456789abcdefghijklmnopqrstuvwxyz and again ghp_0123456789abcdefghijklmnopqrstuvwxyz";
        assert_eq!(scan(t).len(), 1);
    }
}
