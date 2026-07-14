//! GitHub App auth: mint short-lived, scoped installation tokens from a locally
//! stored App private key, and serve them to git as a credential helper — so
//! `git push/fetch` over HTTPS "just works" with auto-rotating (1-hour) tokens and
//! no SSH agent / 1Password. See DESIGN.md. Shells out to `openssl` (RS256 JWT)
//! and `curl` (API) — no new Rust deps.

use crate::config;
use anyhow::{anyhow, Result};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// `~/.confer/app/` — where the App config + private key + token cache live.
fn app_dir() -> Result<PathBuf> {
    Ok(config::home()?.join(".confer").join("app"))
}

#[derive(serde::Serialize, serde::Deserialize, Default, Clone)]
pub struct AppConfig {
    /// numeric App id (the JWT issuer).
    pub app_id: String,
    /// path to the App private key (.pem), 0600.
    pub key_path: String,
    /// the installation to mint tokens for (found via `GET /app/installations`).
    #[serde(default)]
    pub installation_id: Option<u64>,
    /// App slug (for the install URL), cosmetic.
    #[serde(default)]
    pub slug: Option<String>,
}

pub fn load_config() -> Result<AppConfig> {
    let p = app_dir()?.join("config.json");
    let txt = std::fs::read_to_string(&p)
        .map_err(|_| anyhow!("no GitHub App configured — run `confer app-setup` or `confer app-config` first"))?;
    Ok(serde_json::from_str(&txt)?)
}

pub fn save_config(c: &AppConfig) -> Result<()> {
    let dir = app_dir()?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("config.json"), serde_json::to_string_pretty(c)?)?;
    Ok(())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// URL-safe base64 without padding (JWT segment encoding).
fn b64url(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        let take = chunk.len() + 1; // 2 bytes→3 chars, 3 bytes→4 chars, 1 byte→2 chars
        for i in 0..take {
            out.push(A[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
        }
    }
    out
}

/// A GitHub App JWT (RS256), issued now, expiring in ~9 minutes. Signed with the
/// App private key via `openssl` (no JWT crate).
fn app_jwt(cfg: &AppConfig) -> Result<String> {
    let header = b64url(br#"{"alg":"RS256","typ":"JWT"}"#);
    let iat = now().saturating_sub(60); // clock-skew buffer
    let exp = now() + 540;
    let payload = b64url(format!(r#"{{"iat":{iat},"exp":{exp},"iss":"{}"}}"#, cfg.app_id).as_bytes());
    let signing_input = format!("{header}.{payload}");

    let mut child = Command::new("openssl")
        .args(["dgst", "-sha256", "-sign", &cfg.key_path])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("openssl not available for JWT signing: {e}"))?;
    child.stdin.take().unwrap().write_all(signing_input.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(anyhow!(
            "openssl signing failed (check key {}): {}",
            cfg.key_path,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(format!("{signing_input}.{}", b64url(&out.stdout)))
}

/// Minimal JSON string-field extractor for curl responses (avoids a JSON dep for
/// the couple of fields we read; the values are simple GitHub tokens/ids/timestamps).
fn json_str<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let i = body.find(&needle)? + needle.len();
    let rest = &body[i..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let after = after.strip_prefix('"')?;
    Some(&after[..after.find('"')?])
}

/// The first UNQUOTED numeric field with this key (e.g. installation `id`, cache
/// `expires`) — GitHub returns ids as bare numbers, not strings.
fn json_num(body: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\"");
    let i = body.find(&needle)? + needle.len();
    let rest = &body[i..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let end = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
    after[..end].parse().ok()
}

/// GET/POST a GitHub API path with the App JWT as Bearer. `body` (Some) → POST.
fn api(cfg: &AppConfig, method: &str, path: &str, body: Option<&str>) -> Result<String> {
    let jwt = app_jwt(cfg)?;
    let mut args = vec![
        "-sS".to_string(),
        "-X".into(), method.into(),
        "-H".into(), format!("Authorization: Bearer {jwt}"),
        "-H".into(), "Accept: application/vnd.github+json".into(),
        "-H".into(), "X-GitHub-Api-Version: 2022-11-28".into(),
        format!("https://api.github.com{path}"),
    ];
    if let Some(b) = body {
        args.push("-d".into());
        args.push(b.to_string());
    }
    let out = Command::new("curl").args(&args).output().map_err(|e| anyhow!("curl not available: {e}"))?;
    if !out.status.success() {
        return Err(anyhow!("GitHub API {method} {path} failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Discover the installation id for this App (first installation).
pub fn find_installation(cfg: &AppConfig) -> Result<u64> {
    let body = api(cfg, "GET", "/app/installations", None)?;
    json_num(&body, "id")
        .ok_or_else(|| anyhow!("no installation found — install the App on your repos first (see `confer app-setup`).\n{body}"))
}

/// Mint a fresh installation access token (1-hour), returning (token, expires_at_unix).
fn mint_token(cfg: &AppConfig) -> Result<(String, u64)> {
    let inst = cfg
        .installation_id
        .ok_or_else(|| anyhow!("no installation_id configured — run `confer app-config --installation-id <id>` (or app-setup)"))?;
    let body = api(cfg, "POST", &format!("/app/installations/{inst}/access_tokens"), Some("{}"))?;
    let token = json_str(&body, "token").ok_or_else(|| anyhow!("no token in response: {body}"))?;
    // "expires_at":"2026-...Z" — parse just to bound the cache; fall back to +55m.
    let exp = now() + 55 * 60;
    Ok((token.to_string(), exp))
}

/// A cached-or-fresh installation token (cache is a 0600 file; reused while >5 min remain).
pub fn token(cfg: &AppConfig) -> Result<String> {
    let cache = app_dir()?.join("token.json");
    if let Ok(txt) = std::fs::read_to_string(&cache) {
        if let (Some(t), Some(e)) = (json_str(&txt, "token"), json_num(&txt, "expires")) {
            if e > now() + 300 {
                return Ok(t.to_string());
            }
        }
    }
    let (t, exp) = mint_token(cfg)?;
    // store cache 0600
    let dir = app_dir()?;
    std::fs::create_dir_all(&dir)?;
    let f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(&cache)?;
    set_0600(&f);
    let mut f = f;
    writeln!(f, "{{\"token\":\"{t}\",\"expires\":{exp}}}")?;
    Ok(t)
}

#[cfg(unix)]
fn set_0600(f: &std::fs::File) {
    use std::os::unix::fs::PermissionsExt;
    let _ = f.set_permissions(std::fs::Permissions::from_mode(0o600));
}
#[cfg(not(unix))]
fn set_0600(_f: &std::fs::File) {}

/// The git credential-helper protocol: on `get`, print `x-access-token:<token>`;
/// `store`/`erase` are no-ops (tokens are minted, never user-supplied).
pub fn credential(op: &str) -> Result<()> {
    // git sends key=value lines on stdin; we only need to answer `get`.
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok();
    if op != "get" {
        return Ok(()); // store / erase → nothing to persist
    }
    // Only answer for github.com HTTPS.
    let host = input.lines().find_map(|l| l.strip_prefix("host=")).unwrap_or("");
    if !host.is_empty() && host != "github.com" {
        return Ok(()); // let another helper handle non-GitHub hosts
    }
    let tok = token(&load_config()?)?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "username=x-access-token")?;
    writeln!(out, "password={tok}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b64url_matches_known_vectors() {
        assert_eq!(b64url(b""), "");
        assert_eq!(b64url(b"f"), "Zg");
        assert_eq!(b64url(b"fo"), "Zm8");
        assert_eq!(b64url(b"foo"), "Zm9v");
        assert_eq!(b64url(b"foob"), "Zm9vYg");
        assert_eq!(b64url(b"fooba"), "Zm9vYmE");
        assert_eq!(b64url(b"foobar"), "Zm9vYmFy");
        // url-safe alphabet: bytes that would be + / in standard base64 → - _
        assert_eq!(b64url(&[0xfb, 0xff, 0xbf]), "-_-_");
    }

    #[test]
    fn app_jwt_signs_a_three_segment_token() {
        // generate a throwaway RSA key and sign a JWT with it (validates the
        // openssl signing path + segment assembly). Skips if openssl is absent.
        let dir = std::env::temp_dir().join(format!("confer-jwt-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let key = dir.join("app.pem");
        let gen = Command::new("openssl")
            .args(["genrsa", "-out", key.to_str().unwrap(), "2048"])
            .output();
        if gen.map(|o| !o.status.success()).unwrap_or(true) {
            return; // no openssl → skip
        }
        let cfg = AppConfig {
            app_id: "123456".into(),
            key_path: key.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let jwt = app_jwt(&cfg).expect("sign jwt");
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT must have header.payload.signature");
        assert!(parts.iter().all(|p| !p.is_empty()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_str_extracts_simple_fields() {
        let body = r#"{"token":"ghs_abc123","expires_at":"2026-07-09T21:00:00Z","id":42}"#;
        assert_eq!(json_str(body, "token"), Some("ghs_abc123"));
        assert_eq!(json_str(body, "expires_at"), Some("2026-07-09T21:00:00Z"));
        assert_eq!(json_str(body, "missing"), None);
    }

    #[test]
    fn json_num_extracts_bare_numbers() {
        // GitHub returns ids as unquoted numbers; the FIRST id (installation) wins.
        let list = r#"[{"id": 145536049, "app_id": 4259398, "account": {"id": 155389}}]"#;
        assert_eq!(json_num(list, "id"), Some(145536049));
        assert_eq!(json_num(list, "app_id"), Some(4259398));
        assert_eq!(json_num(r#"{"expires":1799999999}"#, "expires"), Some(1799999999));
        assert_eq!(json_num(r#"{"x":"str"}"#, "x"), None);
    }
}
