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

    // Embed the built web dashboard — a single self-contained index.html from `ui/`
    // (vite-plugin-singlefile inlines all JS/CSS) — so `confer serve` ships it with no
    // runtime assets. If the UI isn't built, embed a placeholder that says how; cargo
    // build still works without node. Rebuild `serve` when the bundle changes.
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let html = std::fs::read_to_string("ui/dist/index.html").unwrap_or_else(|_| {
        "<!doctype html><meta charset=\"utf-8\"><title>confer serve</title>\
         <body style=\"font-family:system-ui;max-width:34rem;margin:3rem auto;padding:0 1rem\">\
         <h1>confer serve</h1><p>The web dashboard isn't built yet. Run \
         <code>npm --prefix ui install &amp;&amp; npm --prefix ui run build</code>, then rebuild confer.</p>\
         <p>The JSON API (<code>/api/*</code>) and the no-JS view (<code>/classic</code>) work regardless.</p>"
            .to_string()
    });
    std::fs::write(std::path::Path::new(&out).join("dashboard.html"), html).expect("write dashboard.html");
    println!("cargo:rerun-if-changed=ui/dist/index.html");
}
