//! Embed the commit confer was built from, so it can report its build and detect version
//! drift against a hub's expected version.

use std::process::Command;

fn main() {
    // Prefer an explicitly provided sha (CI/release can inject the tag or commit), then git,
    // then "unknown" — so `cargo build` works out of the box AND a build from a source tarball
    // with no `.git` still compiles.
    let sha = std::env::var("CONFER_GIT_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "unknown".to_string())
        });
    println!("cargo:rustc-env=CONFER_GIT_SHA={sha}");
    // Re-stamp when HEAD moves or the override changes.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-env-changed=CONFER_GIT_SHA");
}
