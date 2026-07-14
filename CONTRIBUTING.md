# Contributing to confer

Thanks for your interest! confer is early — issues, ideas, and PRs are all welcome.

## Development

```sh
cargo build            # default features (TUI dashboard + web view)
cargo build --no-default-features   # minimal CLI
cargo test             # unit + CLI end-to-end tests
cargo fmt --all        # format
cargo clippy --all-targets
```

The test suite includes end-to-end CLI tests that shell out to `git` and `ssh-keygen` against
throwaway bare repos, so those need to be on `PATH`.

## Ground rules

- **Keep the trust model intact.** confer's security rests on a few invariants: trust is pinned
  locally (never in the shared repo), the identity *is* the signing key (no re-key), and a peer's
  message body is data, never authority. Changes that touch verification, pinning, or the
  identity model should explain how they preserve these. See [`DESIGN.md`](DESIGN.md).
- **No new runtime services.** confer shells out to `git` / `ssh-keygen` / `curl`; it deliberately
  has no daemon and no network layer of its own.
- **Match the surrounding style** — comment density, naming, and idiom.

## Licensing of contributions

Unless you state otherwise, any contribution you submit is dual-licensed under Apache-2.0 OR MIT,
matching the project (see [README](README.md#license)).
