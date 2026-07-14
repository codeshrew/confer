# confer tests

Three layers, run top-to-bottom by cost. **`cargo test` runs layers 1 and 2**;
the shell script (layer 3) is a separate command.

## 1. Unit tests — pure/near-pure logic (`src/**/*.rs`, `#[cfg(test)]`)
In-process, microsecond-fast, no I/O. Cover the fiddly pure logic:
- fold/id: `id_matches`, `id_ref_matches`, `resolve_unique`, `request_status`,
  `claimants`, `superseded_set`, empty/short-reference guards (C1/C2).
- parsing: `parse_ref`, `parse_range`, `parse_remote`, slug/reserved-name rules.
- URL scheme selection: `clone_url_candidates` / `clone_candidates`.
- schema round-trip (CRLF, fenced code, malformed YAML, `summary_line` fallback).
- git plumbing (`gitcmd::tests`): these build **real throwaway git repos** in
  `std::env::temp_dir()` (local only, no network) to test `cursor_anchor`,
  `added_message_files` ranges/fallbacks, and gitlock stale-reclaim.

Add a unit test here when the logic can be exercised without spawning `confer`.

## 2. CLI integration tests (`tests/cli.rs`) — the real binary end-to-end
Drives the freshly-built binary (`env!("CARGO_BIN_EXE_confer")`, so never a stale
build) against **local bare hubs**. The `Hub`/`Clone` fixture spins up a seeded bare
hub and clones; `Clone::confer(args)` runs the binary with `CONFER_HUB`/`CONFER_ROLE`
scoped **per-subprocess** (no process-global env → parallel-safe). Covers the flows
unit tests can't reach: `append` validation + commit + sync + receipt + exit codes,
empty-body/`--text -` handling, unreadable-file tolerance, malformed-card logging,
forced-signing resilience, `join` role-card registration, cross-clone delivery, and
the git-subprocess timeout (via a fake hanging `git` on `PATH` + `CONFER_GIT_TIMEOUT_SECS`).

**Add a CLI test here** for any new command or any behavior that involves git/fs or
exit codes. This is the primary surface as confer grows; prefer it over a new shell
script. It's parallel-safe and needs no network/auth (bare hubs use file transport).

## 2b. Gated real-remote E2E (`tests/cli.rs`, `#[ignore]`d)
`e2e_real_remote_roundtrip` covers the one seam local bare hubs can't: **actual
network + auth** (SSH/HTTPS, credential helpers). It does `init`/clone → `append`
(push) → fresh clone → `read` against a REAL remote and asserts the message
round-trips. It's `#[ignore]`d (out of the default suite — needs a repo + working
git credentials) and reads the target from `CONFER_E2E_REMOTE`:

```sh
CONFER_E2E_REMOTE=https://github.com/codeshrew/confer-e2e.git \
  cargo test --release --test cli -- --ignored e2e_real_remote
```

`codeshrew/confer-e2e` is a **private throwaway hub** (safe to delete/recreate);
each run tags a unique marker and messages accumulate harmlessly. Use the URL
scheme whose auth you want to smoke — HTTPS (gh credential helper) or SSH. This is
a pre-release / environment check, not a per-commit gate; the live fleet
(`team-hub`) is the continuous real-remote validation.

## 3. Cross-clone delivery script (`tests/two_clone.sh`)
The original two-clone-through-a-bare-hub scenarios (B1–B4 cursor/divergence, H1/H2/H6
delivery/routing/traversal, init split-brain). Run with `bash tests/two_clone.sh`
(defaults to the debug binary; override with `CONFER=…/target/release/confer`). Kept
because it's a compact, readable end-to-end narrative; new cases generally belong in
layer 2 instead (integrated into `cargo test`, structured assertions, parallel).

## Conventions
- Never touch the user's real git config/signing: test git runs with
  `-c user.name/email` and `-c commit.gpgsign=false`; confer itself already forces
  `commit.gpgsign=false`.
- Temp dirs are unique per test (`pid` + atomic counter) → safe under parallelism.
- No network: everything uses local bare-hub file transport.
