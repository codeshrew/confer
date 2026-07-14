//! Per-consumer read position: the last hub COMMIT this (hub, role) has
//! processed. Ordering is git's commit order (topological, skew-proof) — a
//! message is "new" iff it was added in a commit after the cursor. No wall-clock
//! comparison, no gaps. Keyed by (topology-proof hub, consuming role).
//! Local-only, never stored in the repo.

use crate::config;
use anyhow::Result;
use std::path::PathBuf;

fn path(hub_key: &str, role: &str) -> Result<PathBuf> {
    let role = if role.is_empty() { "_" } else { role };
    Ok(config::home()?
        .join(".confer")
        .join("cursor")
        .join(hub_key)
        .join(format!("{role}.json")))
}

/// The last processed hub commit sha, if any.
pub fn load(hub_key: &str, role: &str) -> Result<Option<String>> {
    let Ok(txt) = std::fs::read_to_string(path(hub_key, role)?) else {
        return Ok(None);
    };
    Ok(serde_json::from_str::<serde_json::Value>(&txt)
        .ok()
        .and_then(|v| v.get("commit").and_then(|c| c.as_str()).map(String::from)))
}

pub fn save(hub_key: &str, role: &str, commit: &str) -> Result<()> {
    let p = path(hub_key, role)?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&p, serde_json::json!({ "commit": commit }).to_string())?;
    Ok(())
}
