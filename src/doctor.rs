//! `confer doctor` — audit a repo's git identity/signing config so an agent and its
//! human never clobber each other.
//!
//! The hazard: git config resolves system → global → local, last wins. The human's
//! GLOBAL config is theirs (their key, their 1Password signer). If an agent inherits it
//! it signs *as the human* (masquerade, and blocks headless on an interactive signer);
//! if an agent writes LOCAL signing config into a clone the human ALSO commits in, it
//! rewrites the human's commits. The safe shape: agents get their OWN clone + OWN key,
//! configured LOCAL-only; confer never touches global. This audit checks exactly that
//! and is the guardrail for rolling an agent key across a fleet.

use crate::gitcmd;
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Info,
}

impl Level {
    fn glyph(&self) -> &'static str {
        match self {
            Level::Ok => "✓",
            Level::Warn => "⚠",
            Level::Info => "ℹ",
        }
    }
    /// The machine-readable severity string for `doctor --json` (design/37 item 10).
    pub fn severity(&self) -> &'static str {
        match self {
            Level::Ok => "ok",
            Level::Warn => "warn",
            Level::Info => "info",
        }
    }
}

pub struct Finding {
    pub level: Level,
    pub title: String,
    pub fix: Option<String>,
}

/// git config scope a key is actually set at, from `--show-origin`.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Scope {
    Local,
    Global,
    System,
    Unknown,
}

impl Scope {
    pub fn from_origin(path: &str) -> Scope {
        // Local origins come back RELATIVE to the repo (`.git/config`), so match without
        // requiring a leading slash. (`.gitconfig`/`.config/git/config` are global and
        // don't contain the literal `.git/config`.)
        if path.contains(".git/config") {
            Scope::Local
        } else if path.contains("/etc/") {
            Scope::System
        } else if path.contains(".gitconfig") || path.contains("/git/config") {
            Scope::Global
        } else {
            Scope::Unknown
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Scope::Local => "local",
            Scope::Global => "global",
            Scope::System => "system",
            Scope::Unknown => "?",
        }
    }
}

/// The effective value of a git config key and the scope it came from.
fn scoped(root: &Path, key: &str) -> Option<(Scope, String)> {
    let out = gitcmd::output(root, &["config", "--show-origin", "--get", key]).ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let line = s.lines().next()?; // "file:/path\tvalue"
    let (origin, value) = line.split_once('\t')?;
    let path = origin.strip_prefix("file:").unwrap_or(origin);
    Some((Scope::from_origin(path), value.trim().to_string()))
}

fn is_interactive_signer(program: &str) -> bool {
    let p = program.to_lowercase();
    p.contains("op-ssh-sign") || p.contains("1password") || p.contains("gpg-agent")
}

/// Audit `root`. Read-only — never writes config.
pub fn audit(root: &Path) -> Vec<Finding> {
    let mut f = Vec::new();
    let agent_clone = root.join(".confer").join("identity.json").exists();

    f.push(Finding {
        level: Level::Info,
        title: if agent_clone {
            "This is an agent clone (has .confer/identity.json) — local signing config is correct here.".into()
        } else {
            "This looks like a human-used clone (no .confer/identity.json) — an agent must NOT write signing config here.".into()
        },
        fix: None,
    });

    let gpgsign = scoped(root, "commit.gpgsign");
    let program = scoped(root, "gpg.ssh.program");
    let signingkey = scoped(root, "user.signingkey");
    let email = scoped(root, "user.email");
    let signing_on = gpgsign.as_ref().map(|(_, v)| v == "true").unwrap_or(false);

    // Interactive signer. It only BLOCKS a headless agent when signing is actually ON — then every
    // commit invokes the interactive program and hangs, a real problem (Warn). With signing OFF the
    // program is never invoked (an inherited-but-unused global), so it's informational, not a counted
    // action item — that's the round-2 cry-wolf fix, WITHOUT hiding the genuine headless-signing
    // blocker a full downgrade would (red-team: local key + interactive gpg.ssh.program + gpgsign=on
    // still hangs, and `doctor --check` must keep catching it for onboarding automation).
    if let Some((scope, prog)) = &program {
        if is_interactive_signer(prog) {
            f.push(Finding {
                level: if agent_clone && signing_on { Level::Warn } else { Level::Info },
                title: format!(
                    "gpg.ssh.program ({}) is an interactive signer ({prog}) — it prompts/blocks in a headless agent.",
                    scope.label()
                ),
                fix: agent_clone.then(|| {
                    "give this clone its own on-disk key: `confer join --role <you> --signing-key <path>` (overrides the program to ssh-keygen, no prompt).".into()
                }),
            });
        }
    }

    if agent_clone {
        // Agent clone should sign with an agent key set LOCALLY, not the human's inherited key.
        // A key alone isn't enough — `commit.gpgsign` must actually be ON, or messages go out
        // unsigned and no peer can verify them (this exact gap shipped an unverifiable fleet-op).
        match &signingkey {
            Some((Scope::Local, _)) if signing_on => f.push(Finding {
                level: Level::Ok,
                title: "Signs with a LOCAL signing key (agent-scoped, not the human's).".into(),
                fix: None,
            }),
            Some((Scope::Local, _)) => f.push(Finding {
                level: Level::Warn,
                title: "A LOCAL signing key is set but commit.gpgsign is OFF — this agent's messages go out UNSIGNED and peers can't verify them.".into(),
                fix: Some("turn signing on: `confer join --role <you> --signing-key <path>` (sets commit.gpgsign=true), or `git config commit.gpgsign true`.".into()),
            }),
            Some((scope, _)) => f.push(Finding {
                level: Level::Warn,
                title: format!("user.signingkey is inherited from {} config — this agent would sign with the HUMAN's key.", scope.label()),
                fix: Some("adopt an agent key: `confer join --role <you> --signing-key <path>`.".into()),
            }),
            None => f.push(Finding {
                level: Level::Info,
                title: "No signing key configured — messages will be unverifiable.".into(),
                fix: Some("adopt an agent key: `confer join --role <you> --signing-key <path>`.".into()),
            }),
        }
        if let Some((scope, mail)) = &email {
            if *scope != Scope::Local {
                f.push(Finding {
                    level: Level::Warn,
                    title: format!("user.email ({mail}) comes from {} config — agent commits are attributed to the human, not the role.", scope.label()),
                    fix: Some("join sets a role identity (<role>@confer.local) locally.".into()),
                });
            }
        }
    } else {
        // Human-used clone: any LOCAL signing/identity override would hit the human's commits.
        for (key, val) in [("commit.gpgsign", &gpgsign), ("user.signingkey", &signingkey), ("user.email", &email)] {
            if let Some((Scope::Local, v)) = val {
                f.push(Finding {
                    level: Level::Warn,
                    title: format!("LOCAL override `{key} = {v}` in a human-used clone — this changes how the HUMAN's commits here behave."),
                    fix: Some("agents should use a SEPARATE clone, or per-invocation `git -c …` overrides — never local config in a shared clone.".into()),
                });
            }
        }
        f.push(Finding {
            level: Level::Info,
            title: "Agents should not commit here directly — give each agent its own sibling clone with its own key.".into(),
            fix: None,
        });
    }

    // Hub visibility: confer's whole trust model assumes a PRIVATE hub, so an
    // anonymously-readable (public) remote silently exposes every message, role, and the
    // full history to the world. Warn ONLY on a positive confirmation.
    let (url, vis) = remote_visibility(root);
    match vis {
        Visibility::Public => f.push(Finding {
            level: Level::Warn,
            title: format!(
                "the hub remote appears PUBLIC (anonymously readable): {} — confer assumes a PRIVATE hub, so this exposes all coordination traffic (every message, role, and the full history) and anyone can clone it.",
                url.as_deref().unwrap_or("origin")
            ),
            fix: Some("make the repo private, or confirm this is a deliberate public/read-only mirror.".into()),
        }),
        Visibility::Private => f.push(Finding {
            level: Level::Ok,
            title: "The hub remote is not anonymously readable (appears private).".into(),
            fix: None,
        }),
        Visibility::Unknown => f.push(Finding {
            level: Level::Info,
            title: "Couldn't verify hub visibility (offline, SSH-only, or a self-hosted host) — can't confirm it's private; check manually if unsure.".into(),
            fix: None,
        }),
        Visibility::NotApplicable => {} // local/file remote → not a public-exposure concern
    }

    f.push(Finding {
        level: Level::Info,
        title: "confer only ever writes LOCAL git config; it never modifies your global ~/.gitconfig.".into(),
        fix: None,
    });
    f
}

/// Apply the auto-fixable repairs and return a description of each. Currently: an agent
/// clone that has a signing key but isn't actually signing (commit.gpgsign off, or an
/// interactive signer that blocks headless) — turn on non-interactive ssh signing with the
/// on-disk key so its messages verify. Only ever touches LOCAL config, only in an agent
/// clone with a key already set; never writes global, never invents a key.
pub fn fix(root: &Path, ssh_keygen: &str) -> anyhow::Result<Vec<String>> {
    let mut applied = Vec::new();
    let agent_clone = root.join(".confer").join("identity.json").exists();
    if !agent_clone {
        return Ok(applied); // never auto-touch a human-used clone
    }
    let signingkey = scoped(root, "user.signingkey");
    if signingkey.is_none() {
        return Ok(applied); // no key to sign with — adopting one is a deliberate `join`
    }
    let signing_off = scoped(root, "commit.gpgsign").map(|(_, v)| v != "true").unwrap_or(true);
    let interactive = scoped(root, "gpg.ssh.program").map(|(_, p)| is_interactive_signer(&p)).unwrap_or(false);
    if signing_off || interactive {
        for (k, v) in [("gpg.format", "ssh"), ("gpg.ssh.program", ssh_keygen), ("commit.gpgsign", "true")] {
            gitcmd::check(root, &["config", k, v])?;
        }
        applied.push(
            "enabled commit signing with the on-disk agent key (gpg.format=ssh, ssh-keygen program, commit.gpgsign=true) — this clone's messages were going out UNSIGNED/unverifiable".to_string(),
        );
    }
    Ok(applied)
}

/// Whether the hub's origin remote is anonymously readable (i.e. public). confer's trust
/// model assumes a PRIVATE hub, so a public one is a warning — but we only ever warn on a
/// POSITIVE confirmation; anything we can't confirm is left alone (no false alarms).
#[derive(Debug, PartialEq, Eq)]
pub enum Visibility {
    /// Anonymous read works → the repo is public.
    Public,
    /// Anonymous read is refused (auth required / 404) → appears private.
    Private,
    /// A network remote we couldn't reach anonymously (offline, SSH-only host, self-hosted).
    Unknown,
    /// No network remote to check (a local `file://`/path clone, or no origin).
    NotApplicable,
}

fn origin_url(root: &Path) -> Option<String> {
    let out = gitcmd::output(root, &["config", "--get", "remote.origin.url"]).ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Convert a git remote URL to an anonymous HTTPS URL we can probe without credentials.
/// `None` for schemes we can't (or shouldn't) anon-check — a local path or `file://`.
pub fn anon_https(url: &str) -> Option<String> {
    let u = url.trim();
    if let Some(r) = u.strip_prefix("git@") {
        // scp-like: git@host:owner/repo.git → https://host/owner/repo.git
        Some(format!("https://{}", r.replacen(':', "/", 1)))
    } else if let Some(r) = u.strip_prefix("ssh://git@") {
        Some(format!("https://{}", r.replacen(':', "/", 1)))
    } else if u.starts_with("https://") || u.starts_with("http://") {
        Some(u.to_string())
    } else {
        None // file://, bare path, or an unknown scheme → not a public-repo concern
    }
}

/// Probe whether `anon_url` is anonymously readable by an UNAUTHENTICATED HTTPS request
/// to its git smart-HTTP endpoint (`…/info/refs?service=git-upload-pack`) via `curl`.
/// 200 = public; 401/403/404 = not anon-readable (private/absent); no answer = unknown.
///
/// We deliberately use curl, not `git`: the user's config commonly rewrites HTTPS→SSH
/// (`insteadOf`), which would turn an "anonymous" probe into an authenticated SSH connect
/// (and hang on the key agent). A raw HTTP GET bypasses all git URL rewriting + auth.
fn probe_public(anon_url: &str) -> Visibility {
    use std::process::{Command, Stdio};
    let endpoint = format!("{}/info/refs?service=git-upload-pack", anon_url.trim_end_matches('/'));
    let mut cmd = Command::new("curl");
    cmd.args([
        "-sS", "-o", "/dev/null", "-w", "%{http_code}",
        "--max-time", "6", "-L", "--max-redirs", "3",
        "-A", "confer-doctor",
        &endpoint,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    let Ok(child) = cmd.spawn() else {
        return Visibility::Unknown; // no curl available
    };
    let pid = child.id();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let out = match rx.recv_timeout(std::time::Duration::from_secs(8)) {
        Ok(Ok(o)) => o,
        _ => {
            let _ = Command::new("kill").arg("-9").arg(pid.to_string()).status();
            return Visibility::Unknown;
        }
    };
    match String::from_utf8_lossy(&out.stdout).trim() {
        "200" => Visibility::Public,
        "000" | "" => Visibility::Unknown, // couldn't connect (offline)
        _ => Visibility::Private,          // 401/403/404/… → not anonymously readable
    }
}

/// Best-effort: is the hub's origin anonymously readable? Returns the url + verdict.
pub fn remote_visibility(root: &Path) -> (Option<String>, Visibility) {
    let Some(url) = origin_url(root) else {
        return (None, Visibility::NotApplicable);
    };
    match anon_https(&url) {
        Some(anon) => (Some(url), probe_public(&anon)),
        None => (Some(url), Visibility::NotApplicable),
    }
}

/// Render an audit as a human-readable report.
pub fn render(findings: &[Finding]) -> String {
    let mut out = String::from("confer doctor — health & identity audit\n");
    for f in findings {
        out.push_str(&format!("  {} {}\n", f.level.glyph(), f.title));
        if let Some(fix) = &f.fix {
            out.push_str(&format!("      → {fix}\n"));
        }
    }
    let warns = findings.iter().filter(|f| f.level == Level::Warn).count();
    out.push_str(&if warns == 0 {
        "\nNo scope conflicts found.\n".to_string()
    } else {
        format!("\n{warns} thing(s) to address above.\n")
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_from_origin_classifies_paths() {
        assert_eq!(Scope::from_origin("/Users/x/proj/.git/config"), Scope::Local);
        assert_eq!(Scope::from_origin(".git/config"), Scope::Local); // git reports local relative
        assert_eq!(Scope::from_origin("/Users/x/.gitconfig"), Scope::Global);
        assert_eq!(Scope::from_origin("/Users/x/.config/git/config"), Scope::Global);
        assert_eq!(Scope::from_origin("/etc/gitconfig"), Scope::System);
    }

    #[test]
    fn anon_https_derivation() {
        assert_eq!(anon_https("git@github.com:codeshrew/x.git").unwrap(), "https://github.com/codeshrew/x.git");
        assert_eq!(anon_https("https://github.com/codeshrew/x.git").unwrap(), "https://github.com/codeshrew/x.git");
        assert_eq!(anon_https("ssh://git@gitlab.com/o/x.git").unwrap(), "https://gitlab.com/o/x.git");
        assert!(anon_https("file:///tmp/hub.git").is_none()); // local → not a public concern
        assert!(anon_https("/tmp/hub.git").is_none());
    }

    #[test]
    fn interactive_signer_detection() {
        assert!(is_interactive_signer("/Applications/1Password.app/Contents/MacOS/op-ssh-sign"));
        assert!(!is_interactive_signer("/usr/bin/ssh-keygen"));
    }

    #[test]
    fn severity_strings_are_the_json_contract_for_doctor_json() {
        // `confer doctor --json` maps each Finding through `Level::severity()` (design/37 item 10);
        // pin the exact strings so the JSON contract can't silently drift.
        assert_eq!(Level::Ok.severity(), "ok");
        assert_eq!(Level::Warn.severity(), "warn");
        assert_eq!(Level::Info.severity(), "info");
    }

    #[test]
    fn doctor_json_shape_round_trips_through_serde_json() {
        let findings = vec![
            Finding { level: Level::Ok, title: "fine".into(), fix: None },
            Finding { level: Level::Warn, title: "uh oh".into(), fix: Some("do X".into()) },
        ];
        let any_hard = findings.iter().any(|f| f.level == Level::Warn);
        let arr: Vec<serde_json::Value> = findings
            .iter()
            .map(|f| serde_json::json!({ "severity": f.level.severity(), "title": f.title, "fix": f.fix }))
            .collect();
        let v = serde_json::json!({ "findings": arr, "ok": !any_hard });
        let s = serde_json::to_string(&v).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&s)
            .unwrap_or_else(|e| panic!("doctor --json shape must parse ({e}): {s}"));
        assert_eq!(parsed["ok"], false);
        assert_eq!(parsed["findings"][1]["severity"], "warn");
        assert_eq!(parsed["findings"][1]["fix"], "do X");
        assert_eq!(parsed["findings"][0]["fix"], serde_json::Value::Null);
    }
}
