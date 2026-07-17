# Working in this repo

`confer` is a git-native coordination blackboard for AI agents — a Rust CLI (clap-derive). Source of
truth for the architecture and threat model is `DESIGN.md`. This file is the maintenance contract:
how to keep the codebase from rotting as it grows.

## Module taxonomy — where new code goes

The crate is a `main.rs` dispatcher plus focused modules. Keep it that way. When you add code, it
belongs in one of three places — **never** by default in `main.rs`:

- **Domain modules** (`config`, `watch`, `roster`, `gitcmd`, `keyring`, `knownhubs`, `machineconfig`,
  `projection`, `schema`, …): logic + data for one concern. Own state and pure functions live here.
- **Command modules** (`cli`, `init`, `join`, `trust`, `fleet`, `identity`, `inbox`, `append`,
  `reconnect`, `config_hub`, `hooks`, `skills`, `keygen_release`, `transport`, `templates`): the
  `cmd_*` handlers, grouped by family. A command handler lives in its family's module.
- **`main.rs`**: `mod`/`use` wiring, the dispatch `match`, `main()`, the small cross-cutting helpers
  used by many families (`warn_safety`/`hint`/`warn_trust`, `now`, `truncate`, `short_id`, the
  build/version consts), and the shared `#[cfg(test)] mod tests`.

### Rules that keep this from rotting
- **A new `cmd_X` goes in its family's command module, not `main.rs`.** If it starts a new family,
  make a new module. `main.rs` is dispatch, not a handler dumping ground — that's exactly how it grew
  to 7,666 lines once.
- **A helper used by ≥2 families gets a named home** (the most-owning domain module, or a clearly
  named util), never an accretion in `main.rs`. Scattered shared helpers were the hardest part of the
  decomposition — don't recreate the problem.
- **Prefer a `pub(crate)` fn in the owning module + a `use` at the call site** over copy-paste. If you
  find yourself writing the same block twice (e.g. a `.find(...).expect(...)` lookup), extract it.

## Size budgets (enforced)

- **Per file: hard cap 1,500 lines** (CI job `file-size budget`, **blocking**; soft warning at 1,000).
  Over the cap fails CI — split the file into a focused module. clippy has no per-file lint, so this
  script is the forcing function.
- **Per function: ~150 lines** (`clippy::too_many_lines`, threshold in `clippy.toml`, **advisory** in
  the CI lint job). Split a handler that outgrows it; if it's irreducibly linear (a big `match`), add
  `#[allow(clippy::too_many_lines)]` with a one-line reason.

Don't wait for the cap. Split as you touch — see below for why that's cheap here.

## Refactor safely (the technique that makes incremental splits routine)

A pure move (relocating code with no behavior change) is provably safe when you:
1. Build the pre-change binary once as a golden reference (e.g. from the last release tag).
2. Move the code; make only cross-module refs `pub(crate)`; fix imports (the compiler lists them).
3. **Byte-diff the full `--help` surface** — top-level **and every subcommand** — new binary vs golden.
   The clap doc-comments ARE the help text, so an identical `--help` proves the command surface is
   unchanged. Add a functional diff of read-only commands for handler moves.
4. `cargo test` stays green. For a moved fn that a `#[cfg(test)]` test still uses, re-import it in the
   test module as `#[cfg(test)] use crate::<mod>::<fn>;`.

Watch the **doc-comment boundary**: a fn's `///` block (and any `#[cfg]`/derive) can start several
lines above the `fn` — capture the whole run, or you drop a line from `--help` and orphan it onto the
neighbor.

## Build / test conventions

- Build/test out of tree to avoid clobbering the dev target: `CARGO_TARGET_DIR=/tmp/confer-build cargo build`.
- `cargo test` — the suite must stay green; the count must not silently drop when you move tests.
- **Do not run `cargo fmt`.** (The CI `fmt --check` is advisory; formatting churn is not wanted here.)
- clippy is advisory in CI — heed `too_many_lines`, but a wall of pedantic lints won't block a merge.
- No global mutable state / statics; that's why module extraction is low-risk. Keep it that way.
