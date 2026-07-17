//! Key generation and the self-update / changelog commands.
//!
//! `cmd_keygen` mints a role's signing key (the identity). The rest is release-adjacent: `cmd_update`
//! self-updates the binary (delegating to brew/cargo when confer was installed that way), and
//! `cmd_changelog` renders sections of the embedded CHANGELOG. Small shared helpers (`which`,
//! `changelog_sections`, `parse_semver_prefix`, `delegate_to_package_manager`) stay private here.

use crate::join::read_pubkey;
use crate::{config, valid_slug, CHANGELOG_MD};
use anyhow::{anyhow, Result};

/// Mint a dedicated ed25519 signing key for a role at the fleet-standard location
/// (`~/.confer/keys/<role>`, comment `<role>@confer`) — `confer keygen`. Refuses to clobber an
/// existing key (the identity IS the key, so overwriting one destroys an identity), and prints
/// the `join --signing-key` line so a keyless agent can go from no-key to a verifiable, keyed
/// identity (and thus a managed clone) without guessing the ssh-keygen convention.
pub(crate) fn cmd_keygen(role: Option<String>, print_publish_hint: bool) -> Result<()> {
    // Role from --role, else the current clone's role (so `confer keygen` "just works" in a hub).
    let role = match role {
        Some(r) => r,
        None => config::repo_root()
            .ok()
            .and_then(|r| config::resolve_role(None, &r).ok())
            .ok_or_else(|| anyhow!("no role — pass --role <id>, or run inside your hub clone"))?,
    };
    if !valid_slug(&role) {
        return Err(anyhow!(
            "invalid role id '{role}' — a role is lowercase letters/digits/'-' (same rule as `join`)"
        ));
    }
    let keydir = config::home()?.join(".confer").join("keys");
    let keypath = keydir.join(&role);
    if keypath.exists() {
        return Err(anyhow!(
            "a signing key already exists for '{role}' at {} — the identity IS the key, so confer \
             will not overwrite it. If this identity is truly dead, remove the key by hand first.",
            keypath.display()
        ));
    }
    std::fs::create_dir_all(&keydir)?;
    // Lock the key dir to the owner (0o700) — the key material lives here (defense-in-depth nit).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&keydir, std::fs::Permissions::from_mode(0o700));
    }
    let out = std::process::Command::new("ssh-keygen")
        .args(["-t", "ed25519", "-C", &format!("{role}@confer"), "-N", ""])
        .arg("-f")
        .arg(&keypath)
        // Close the child's stdin explicitly: if a key somehow appeared at keypath between the
        // exists() gate and here (TOCTOU), ssh-keygen's "Overwrite? (y/n)" prompt hits EOF and
        // ABORTS rather than clobbering — make that fail-closed OURS, not incidental (review nit).
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| anyhow!("could not run ssh-keygen: {e}"))?;
    if !out.status.success() {
        return Err(anyhow!(
            "ssh-keygen failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    // ssh-keygen already writes the private key 0600, but be explicit — and surface a failure
    // rather than swallow it (a silent perm-set failure would leave the key too open).
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&keypath, std::fs::Permissions::from_mode(0o600)) {
            eprintln!(
                "warning: could not set 0600 on {} ({e}) — tighten it by hand",
                keypath.display()
            );
        }
    }
    println!("minted an ed25519 signing key for '{role}':");
    println!("  private: {}", keypath.display());
    println!("  public:  {}.pub", keypath.display());
    if let Ok(pk) = read_pubkey(&keypath) {
        println!("  {pk}");
    }
    // Suppress the "now publish it with `confer join`" hint when a caller (init's one-command
    // create) is about to join immediately — printing it there reads as if the join were still
    // pending when it isn't. Standalone `confer keygen` still shows it.
    if print_publish_hint {
        println!();
        println!("publish it (from your hub clone) to get a verifiable, keyed identity:");
        println!(
            "  confer join --role {role} --signing-key {}",
            keypath.display()
        );
        println!(
            "then your messages sign + verify, and `confer adopt-clone` (managed home) will work."
        );
    }
    Ok(())
}

/// Is `cmd` on PATH? (used to prefer the fast `cargo binstall` over a from-source `cargo install`.)
fn which(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// `confer update` — self-update a STANDALONE install (the `curl … | sh` installer / a GitHub
/// release binary, which carries a dist install receipt). A package-manager install (Homebrew /
/// cargo) has no receipt and is NEVER self-replaced — self-replacing a pm binary fights the
/// package manager and gets silently clobbered on its next upgrade — so we delegate to it instead.
/// Exit 0 on every branch: "defer to your package manager" is a valid outcome, not an error.
/// Split the embedded CHANGELOG into `(version_heading, section_text)` pairs, newest first. Sections
/// are delimited by `## <heading>` lines; anything before the first `##` (the `# Changelog` title) is
/// dropped. The heading is kept verbatim (e.g. "0.6.8" or "Unreleased") for display + comparison.
fn changelog_sections() -> Vec<(String, String)> {
    let mut sections: Vec<(String, String)> = Vec::new();
    let mut cur: Option<(String, String)> = None;
    for line in CHANGELOG_MD.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            if let Some(prev) = cur.take() {
                sections.push(prev);
            }
            cur = Some((h.trim().to_string(), format!("{line}\n")));
        } else if let Some((_, body)) = cur.as_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some(prev) = cur.take() {
        sections.push(prev);
    }
    sections
}

/// Parse a leading `X.Y.Z` (ignoring any trailing text) into a comparable tuple. Non-numeric
/// headings like "Unreleased" return None — the caller treats those as "always newer" so a
/// pending, not-yet-versioned entry is never hidden by a `--since` filter.
fn parse_semver_prefix(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.trim().split_whitespace().next()?;
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next()?.parse().ok()?;
    let patch = it.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// `confer changelog` — show the release notes compiled into this binary. Because the changelog is
/// embedded at build time, a freshly-updated binary shows the new notes and an old one can't; that's
/// the whole point — it answers "what did I just adopt" from the side that actually knows.
pub(crate) fn cmd_changelog(since: Option<String>, all: bool) -> Result<()> {
    let sections = changelog_sections();
    if sections.is_empty() {
        println!("(no changelog embedded in this build)");
        return Ok(());
    }
    let selected: Vec<&(String, String)> = if all {
        sections.iter().collect()
    } else if let Some(since) = since.as_deref() {
        // Everything strictly newer than `since`. A heading that doesn't parse as semver
        // (e.g. "Unreleased") is treated as newer so it's never filtered out.
        let floor = parse_semver_prefix(since);
        sections
            .iter()
            .filter(|(v, _)| match (parse_semver_prefix(v), floor) {
                (Some(vv), Some(f)) => vv > f,
                _ => true,
            })
            .collect()
    } else {
        // Default: just the newest entry — the "what's new right now" view.
        sections.iter().take(1).collect()
    };
    if selected.is_empty() {
        let base = since.as_deref().unwrap_or("");
        println!("confer {} — no changelog entries newer than {base}.", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    for (i, (_, body)) in selected.iter().enumerate() {
        if i > 0 {
            println!();
        }
        print!("{}", body.trim_end());
        println!();
    }
    Ok(())
}

pub(crate) fn cmd_update(check_only: bool) -> Result<()> {
    use axoupdater::AxoUpdater;

    // new_for(the dist APP/PACKAGE name "confer-cli") — dist writes the install receipt keyed on
    // the package name (`~/.config/confer-cli/…-receipt.json`), NOT the binary name. Using the
    // binary "confer" here made load_receipt() always miss, so a standalone curl|sh install never
    // self-updated and fell through to the package-manager delegate (a standalone-canary finding).
    // load_receipt() still Errs for a real brew/cargo install (no receipt) → we delegate; a dist
    // install HAS a receipt → we self-replace. The receipt is the discriminator, so we must look
    // for it under the right name.
    let mut updater = AxoUpdater::new_for("confer-cli");
    if updater.load_receipt().is_err() {
        return delegate_to_package_manager();
    }
    // Optionally use a token to dodge GitHub API rate limits for agents that update often.
    if let Ok(tok) = std::env::var("GITHUB_TOKEN").or_else(|_| std::env::var("GH_TOKEN")) {
        if !tok.is_empty() {
            updater.set_github_token(&tok);
        }
    }
    // STANDALONE: the only self-replace path. axoupdater fetches the latest GH Release, verifies
    // the checksum dist embedded, and swaps atomically.
    if check_only {
        if updater.is_update_needed_sync()? {
            println!("a newer confer is available — run `confer update`.");
        } else {
            println!("confer is up to date.");
        }
        return Ok(());
    }
    // Serialize the self-replace across co-resident agents: they share one installed binary, so two
    // processes swapping it at once could tear the file. Non-blocking — if a sibling already holds
    // the lock, that update covers this binary too, so we skip and just point at the follow-up steps.
    let _guard = match config::try_update_lock() {
        Some(g) => g,
        None => {
            println!("another confer on this machine is updating the shared binary — skipping.");
            println!("once it finishes, pick up the new build:");
            print_post_update_steps();
            return Ok(());
        }
    };
    if updater.run_sync()?.is_some() {
        println!("confer updated to the latest release.");
        print_post_update_steps();
    } else {
        println!("confer is already up to date.");
    }
    Ok(())
}

/// After a binary update, two things are stale and easy to forget — say so explicitly. The RUNNING
/// watch is still the old binary (it must re-arm to run the new build), and the deployed skills were
/// baked from the old binary (they must be re-synced). Neither is automatic (the watch is a separate
/// process the agent hosts; the skill templates live in the NEW on-disk binary, not this process).
fn print_post_update_steps() {
    println!("\nnext — pick up the new build (both are needed):");
    println!("  1. re-arm your watch so it runs the new binary:  /confer-watch   (or `confer watch --replace`)");
    println!("  2. re-sync your skills from the new binary:       confer install-skill");
    println!("  3. see what changed (may ask something of you):   confer changelog");
}

/// No dist receipt → a package manager owns this binary. Detect which from the running exe and
/// print the precise upgrade command; never self-replace.
fn delegate_to_package_manager() -> Result<()> {
    // Canonicalize: a Homebrew install exposes the binary as a SYMLINK (e.g.
    // /usr/local/bin/confer -> ../Cellar/confer/<v>/bin/confer on Intel macOS), and
    // `current_exe()` returns the unresolved symlink, which wouldn't contain `/Cellar/`.
    // Resolve it so the package-manager path detection actually fires (a dogfood finding).
    let exe = std::env::current_exe().unwrap_or_default();
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let p = exe.to_string_lossy();
    if p.contains("/Cellar/") || p.contains("/homebrew/") || p.contains("/usr/local/opt/") {
        println!("confer was installed via Homebrew — `confer update` won't replace it.");
        println!("update with:  brew upgrade confer");
    } else if p.contains("/.cargo/bin/")
        || std::env::var("CARGO_HOME")
            .map(|c| !c.is_empty() && p.contains(&c))
            .unwrap_or(false)
    {
        println!("confer was installed via cargo — `confer update` won't replace it.");
        if which("cargo-binstall") {
            println!("update with:  cargo binstall confer-cli --force   (prebuilt; fast)");
            println!("         or:  cargo install  confer-cli --force   (from source)");
        } else {
            println!("update with:  cargo install confer-cli --force");
            println!("  (tip: `cargo binstall confer-cli --force` is much faster, if you install cargo-binstall)");
        }
    } else {
        println!(
            "confer has no dist install receipt and isn't in a recognized package-manager path,"
        );
        println!("so `confer update` can't safely replace it. Reinstall via the shell installer");
        println!("(curl … | sh) for self-update, or update through your package manager.");
    }
    // Whichever way they update the binary, the watch + skills must be refreshed afterward.
    print_post_update_steps();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changelog_parses_into_newest_first_sections() {
        let sections = changelog_sections();
        assert!(!sections.is_empty(), "the embedded CHANGELOG should parse into sections");
        // Newest-first: the top heading sorts >= every other numeric heading below it.
        let top = parse_semver_prefix(&sections[0].0);
        if let Some(top) = top {
            for (v, _) in &sections[1..] {
                if let Some(vv) = parse_semver_prefix(v) {
                    assert!(top >= vv, "sections must be newest-first ({top:?} vs {vv:?})");
                }
            }
        }
        // Each section body starts with its own `## ` heading and nothing leaks from the file title.
        for (heading, body) in &sections {
            assert!(body.starts_with("## "), "section body must start with its heading");
            assert!(!body.contains("# Changelog\n"), "the file title must not leak into a section");
            assert!(!heading.is_empty());
        }
    }

    #[test]
    fn semver_prefix_orders_versions_and_ignores_non_numeric() {
        assert!(parse_semver_prefix("0.6.8") > parse_semver_prefix("0.6.7"));
        assert!(parse_semver_prefix("0.7.0") > parse_semver_prefix("0.6.99"));
        assert!(parse_semver_prefix("1.0.0") > parse_semver_prefix("0.99.99"));
        // trailing text after the version core is ignored
        assert_eq!(parse_semver_prefix("0.6.8 (abc123)"), Some((0, 6, 8)));
        // a two-component version fills patch with 0
        assert_eq!(parse_semver_prefix("0.6"), Some((0, 6, 0)));
        // non-numeric headings (e.g. "Unreleased") don't parse — the filter treats them as newer
        assert_eq!(parse_semver_prefix("Unreleased"), None);
    }
}
