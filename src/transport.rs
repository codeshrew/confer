//! Remote-transport plumbing: scheme detection, GitHub-style remote parsing/URL
//! candidates, and SSH-key wiring for `init`/`clone`/`invite`.

use crate::config;
use anyhow::{anyhow, Result};

/// Which URL scheme to use when a remote is available in both forms.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Scheme {
    Auto,
    Ssh,
    Https,
}

pub(crate) fn scheme_from(ssh: bool, https: bool) -> Scheme {
    if ssh {
        Scheme::Ssh
    } else if https {
        Scheme::Https
    } else {
        Scheme::Auto
    }
}

/// One GitHub-style remote in both URL forms, so `clone` can fall back
/// scheme→scheme and `invite` can emit a credential-agnostic shorthand.
pub(crate) struct Remote {
    /// the input verbatim (used as-is for unrecognized / non-GitHub / local remotes)
    pub(crate) raw: String,
    pub(crate) https: Option<String>,
    pub(crate) ssh: Option<String>,
    /// `owner/repo` when the host is github.com (scheme-agnostic shorthand)
    pub(crate) shorthand: Option<String>,
}

/// Parse `git@host:owner/repo(.git)`, `scheme://host/owner/repo(.git)`, or the bare
/// `owner/repo` shorthand (→ github.com). Unrecognized inputs (self-hosted git,
/// local paths) pass through as `raw` with no alternate scheme.
pub(crate) fn parse_remote(input: &str) -> Remote {
    let raw = input.to_string();
    if let Some(rest) = input.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            return gh_remote(raw, host, path.trim_end_matches(".git"));
        }
    }
    if let Some((_scheme, after)) = input.split_once("://") {
        let after = after.rsplit_once('@').map_or(after, |(_, h)| h); // strip user@
        if let Some((host, path)) = after.split_once('/') {
            return gh_remote(
                raw,
                host,
                path.trim_end_matches('/').trim_end_matches(".git"),
            );
        }
    }
    // bare owner/repo: exactly one slash, no scheme/colon, not a path
    if !input.contains("://")
        && !input.contains(':')
        && input.matches('/').count() == 1
        && !input.starts_with(['/', '.', '~'])
    {
        return gh_remote(raw, "github.com", input.trim_end_matches(".git"));
    }
    Remote {
        raw,
        https: None,
        ssh: None,
        shorthand: None,
    }
}

fn gh_remote(raw: String, host: &str, path: &str) -> Remote {
    Remote {
        raw,
        https: Some(format!("https://{host}/{path}.git")),
        ssh: Some(format!("git@{host}:{path}.git")),
        shorthand: (host == "github.com").then(|| path.to_string()),
    }
}

/// Weak preference hint: which scheme to *try first*. Detection is unreliable
/// (keychain/1Password SSH agents report no `ssh-add` identities yet work), so
/// this only orders attempts — the clone fallback is what guarantees correctness.
fn prefer_ssh() -> bool {
    match std::env::var("CONFER_SCHEME").ok().as_deref() {
        Some("ssh") => return true,
        Some("https") => return false,
        _ => {}
    }
    if let Ok(home) = config::home() {
        let sshdir = home.join(".ssh");
        if sshdir.join("config").exists() {
            return true;
        }
        if let Ok(rd) = std::fs::read_dir(&sshdir) {
            for e in rd.flatten() {
                let n = e.file_name();
                let n = n.to_string_lossy();
                if n.starts_with("id_") && !n.ends_with(".pub") {
                    return true;
                }
            }
        }
    }
    std::process::Command::new("ssh-add")
        .arg("-l")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Ordered clone URLs to try, honoring the scheme the user TYPED: an explicit
/// `https://`/`ssh` URL puts that scheme first (so origin ends up on it — a no-SSH
/// agent needs a fetchable HTTPS origin); only the bare `owner/repo` shorthand
/// falls back to prefer-ssh ordering. An explicit `--ssh`/`--https` flag overrides.
pub(crate) fn clone_url_candidates(url: &str, remote: &Remote, scheme: Scheme) -> Vec<String> {
    if scheme != Scheme::Auto {
        return clone_candidates(remote, scheme);
    }
    if url.starts_with("https://") || url.starts_with("http://") {
        clone_candidates(remote, Scheme::Https)
            .into_iter()
            .chain(remote.ssh.clone())
            .collect()
    } else if url.starts_with("git@") || url.starts_with("ssh://") {
        clone_candidates(remote, Scheme::Ssh)
            .into_iter()
            .chain(remote.https.clone())
            .collect()
    } else {
        clone_candidates(remote, Scheme::Auto)
    }
}

/// Ordered clone URLs to try for a remote under a scheme choice (with fallback).
pub(crate) fn clone_candidates(r: &Remote, scheme: Scheme) -> Vec<String> {
    match (scheme, &r.ssh, &r.https) {
        (Scheme::Ssh, Some(s), _) => vec![s.clone()],
        (Scheme::Https, _, Some(h)) => vec![h.clone()],
        (Scheme::Auto, Some(s), Some(h)) => {
            if prefer_ssh() {
                vec![s.clone(), h.clone()]
            } else {
                vec![h.clone(), s.clone()]
            }
        }
        _ => vec![r.raw.clone()],
    }
}

/// Build a `GIT_SSH_COMMAND` / `core.sshCommand` value from a transport key path: force THIS key
/// only (`IdentitiesOnly=yes`) and ignore any ssh-agent / 1Password identity (`IdentityAgent=none`)
/// so a deploy key works headlessly regardless of the ambient agent. Expands a leading `~`, and
/// single-quotes the path for the shell git runs the value through.
/// Expand a leading `~`/`~/` in a key path to $HOME. Shared by validate + git_ssh_command so the
/// string that is VALIDATED is exactly the string that gets single-quoted into the ssh command.
fn expand_key_path(path: &str) -> std::path::PathBuf {
    if path == "~" {
        config::home().unwrap_or_else(|_| std::path::PathBuf::from(path))
    } else if let Some(rest) = path.strip_prefix("~/") {
        config::home()
            .map(|h| h.join(rest))
            .unwrap_or_else(|_| std::path::PathBuf::from(path))
    } else {
        std::path::PathBuf::from(path)
    }
}

/// Build a `GIT_SSH_COMMAND` / `core.sshCommand` value from a transport key: force THIS key only
/// (`IdentitiesOnly=yes`), ignore any ssh-agent / 1Password identity (`IdentityAgent=none`), and
/// stay non-interactive (`BatchMode=yes`) so a passphrase / host-key prompt FAILS FAST instead of
/// hanging a headless clone (#3). The expanded path is single-quoted for the shell git runs it in.
pub(crate) fn git_ssh_command(key: &str) -> String {
    let expanded = expand_key_path(key);
    format!(
        "ssh -i '{}' -o IdentitiesOnly=yes -o IdentityAgent=none -o BatchMode=yes -o ConnectTimeout=30",
        expanded.display()
    )
}

/// Reject a transport-key path that isn't a real key file or that carries a character which would
/// break out of the single-quoted `core.sshCommand` / `GIT_SSH_COMMAND` value git runs through a
/// shell — a `'` (or a control char) is a command-injection vector (cf. the 0.5.0 clone RCE).
/// Reject a transport-key path whose EXPANDED string (what actually gets single-quoted into
/// `core.sshCommand` / `GIT_SSH_COMMAND`) carries a `'` or control char — a `'` can enter via
/// `$HOME` expansion AFTER the raw arg passed, so validate the same string `git_ssh_command`
/// quotes, not the raw arg (#1, red-team). Also require the key to be a real file.
pub(crate) fn validate_transport_key(path: &str) -> Result<()> {
    let expanded = expand_key_path(path);
    let s = expanded.to_string_lossy();
    if s.contains('\'') || s.chars().any(|c| c.is_control()) {
        return Err(anyhow!(
            "--ssh-key path (expanded: {s}) contains a single-quote or control character — use a plain filesystem path"
        ));
    }
    if !expanded.is_file() {
        return Err(anyhow!("--ssh-key {s}: not a readable key file"));
    }
    Ok(())
}
