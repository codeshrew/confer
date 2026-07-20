//! Architecture boundary tests (design/48 §1.3) — grep-grade assertions that the
//! receiver-side "doors" keep their exclusive rights, so the class of bug Jarvis found
//! in the 0.8.0 review (an invariant enforced at the honest write path, not at the
//! receiver) cannot be reintroduced by merely forgetting a check.
//!
//! The rule (design/48 §1.1): any datum crossing the peer->receiver boundary is
//! untrusted until it has passed THE ONE function that owns the invariant, and the code
//! is organized so that function is the only door. These tests turn "we agreed not to"
//! into "it doesn't merge." If one fails, a call site bypassed a door — route it through
//! the door; do NOT just add the file to the allowlist.

use std::fs;
use std::path::{Path, PathBuf};

/// Every `src/**/*.rs` file (the crate source). Tests are exempt — they build fixtures.
fn source_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(Path::new("src"), &mut out);
    out
}

/// A line that is purely a `//`/`///`/`*` comment carries no call — exclude it.
fn is_comment(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with('*')
}

fn scan(exempt: &[&str], needles: &[&str]) -> Vec<String> {
    let mut hits = Vec::new();
    for f in source_files() {
        let name = f.file_name().unwrap().to_str().unwrap().to_string();
        if exempt.contains(&name.as_str()) {
            continue;
        }
        let src = fs::read_to_string(&f).unwrap();
        for (i, line) in src.lines().enumerate() {
            if is_comment(line) {
                continue;
            }
            if needles.iter().any(|n| line.contains(n)) {
                hits.push(format!("  {}:{}  {}", f.display(), i + 1, line.trim()));
            }
        }
    }
    hits
}

/// design/48 §1.2a — the `ValidatedPatch` door. `git apply` against a work repo may be
/// invoked ONLY from `patch.rs`. Everywhere else, an apply must go through a
/// `ValidatedPatch`, whose sole constructor runs the receiver-side gates (binary refusal,
/// byte ceiling, temp-index apply). Anyone applying a raw diff elsewhere has bypassed
/// the gate for any patch that didn't come from the honest write path.
#[test]
fn git_apply_is_confined_to_patch_rs() {
    let offenders = scan(&["patch.rs"], &["[\"apply\"", "vec![\"apply\""]);
    assert!(
        offenders.is_empty(),
        "`git apply` invoked outside patch.rs — route the apply through a ValidatedPatch \
         (design/48 §1.2a), don't allowlist:\n{}",
        offenders.join("\n")
    );
}

/// design/48 §1.2b — the `ServeScope` door. `crosshub::hub_dirs()` (the machine's ENTIRE
/// hub registry, ignoring operator scope) may be *called* only from `allowed_hubs()` (the
/// scoped resolver, in api.rs), from the operator/CLI layer where the operator IS the
/// scope (main.rs — the `serve` command builds its served set here, and operator commands
/// like `fleet` enumerate directly; refcmd.rs — the `refs --all-hubs` command handler,
/// moved out of main.rs), and from the `repos discover` CLI (reposdiscover.rs).
/// No HTTP *handler* may reach it, so a future endpoint cannot repeat the `?allHubs=1`
/// cross-hub leak — a handler that wants many hubs must go through `allowed_hubs(dirs,
/// all_hubs)`, which is the only request-path caller.
#[test]
fn hub_dirs_is_reached_only_through_the_scoped_resolver() {
    let offenders = scan(&["api.rs", "main.rs", "reposdiscover.rs", "refcmd.rs"], &["crosshub::hub_dirs()"]);
    assert!(
        offenders.is_empty(),
        "crosshub::hub_dirs() called outside allowed_hubs()/CLI — route it through ServeScope \
         (design/48 §1.2b), don't allowlist:\n{}",
        offenders.join("\n")
    );
}
