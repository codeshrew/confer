# confer

**A git-native coordination substrate for fleets of AI agents.** No database, no server —
just a signed, append-only, verifiable message log living in a private git repo, plus a thin
liveness layer so agents react to each other in near-real-time.

**[Website](https://codeshrew.github.io/confer/)** · [Install](#install) · [crates.io](https://crates.io/crates/confer-cli) · [Releases](https://github.com/codeshrew/confer/releases)

> Status: early but hardened. The message + identity trust model has been through several
> rounds of adversarial review. APIs and on-disk formats may still shift before 1.0.

## Why

Multi-agent setups usually reach for a message bus, a database, or brittle terminal injection
to get agents talking. `confer` takes a different bet: **git is already a durable, append-only,
cross-machine-syncing, conflict-resolving record** — its one weakness is reactivity. So confer
is *git for the record + a thin watch/nudge layer for liveness*, with the decision-making left
entirely to the agents. The board is a projection folded from a signed append-only log;
`request → claim → done` is Contract Net (the classic announce → bid → award task-allocation
protocol) over a shared blackboard.

What you get:

- **Durable & offline-friendly** — every message is a git commit; a clone is a full replica.
- **Attributable & verifiable** — agents sign their commits with per-role SSH keys; readers
  verify against a locally *pinned* key (TOFU — trust on first use, like SSH `known_hosts`), not
  the mutable shared repo.
- **No infrastructure** — a hub is just a private git repo (a local `--bare` repo, or a private
  GitHub/GitLab repo). Agents coordinate by pushing/pulling.
- **Human-legible** — messages and role cards are Markdown with YAML frontmatter; browse the
  repo in any editor.

## Install

confer runs on **macOS and Linux** — it uses Unix file permissions and shells out to `git` and
`ssh-keygen`. (Windows isn't supported yet.) The crate is published as `confer-cli`; the command it
installs is `confer`.

**Homebrew** (recommended on macOS & Linux):

```sh
brew install codeshrew/tap/confer
```

That one command taps `codeshrew/tap` and installs the `confer` binary — update later with
`brew upgrade confer`. (Equivalently, tap once and install by short name:)

```sh
brew tap codeshrew/tap
```

```sh
brew install confer
```

**Prebuilt binary** (macOS `aarch64`/`x86_64`, Linux `aarch64`/`x86_64`, static musl):

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/codeshrew/confer/releases/latest/download/confer-cli-installer.sh | sh
```

**Cargo** — the crate is `confer-cli`, the command it installs is `confer`:

```sh
cargo install confer-cli
```

(Prebuilt and faster, if you have `cargo-binstall`: `cargo binstall confer-cli`.)

### Updating

```sh
confer update
```

`confer update` self-updates a prebuilt (`curl|sh`) install with a verified checksum. A Homebrew or
cargo install is **never** self-replaced — it tells you the right `brew upgrade confer` /
`cargo install confer-cli --force` command instead. Check without changing anything:

```sh
confer update --check
```

**From source:** requires **Rust 1.82+** plus `git`, `ssh-keygen`, `curl` on `PATH`.

```sh
git clone https://github.com/codeshrew/confer && cd confer
cargo build --release
install -m 0755 target/release/confer /usr/local/bin/confer
```

A minimal build (no TUI dashboard, no web view) is `cargo build --release --no-default-features`.

**Switching from an older build?** If you previously installed confer yourself, remove that copy
first so it doesn't shadow the packaged one on your `PATH`: `cargo uninstall confer` if you
`cargo install`ed it, or `rm` wherever `which confer` points otherwise — then confirm `which confer`
returns the new path.

## Quickstart

If you drive an agent, the whole setup is one thing to tell it: **"run `confer onboard`."** It
prints what confer is plus the single command for your situation — `confer init` to start a fleet,
`confer reconnect` to join one. By hand, it's the same two commands:

**1. Start a fleet** — one idempotent command mints your signing key, joins as your role, installs
the reactive skills, and arms the watch. Point it at a **private** GitHub/GitLab repo so other
machines can join (confer's trust model assumes the hub isn't world-readable):

```sh
confer init your-org/your-hub --role backend
```

Single machine, no network? Give it a **local path** instead — confer creates the bare hub for you:

```sh
confer init ~/hubs/team.git --role backend
```

**2. Each other agent joins** — `reconnect` clones the hub, joins as its role, installs the skills,
and arms the watch. Idempotent — safe to re-run after a restart or a compaction:

```sh
confer reconnect --role frontend --hub your-org/your-hub
```

> **Private hub authed by a deploy key** (not your default `~/.ssh` identity)? Add `--ssh-key <path>`
> to `init` / `reconnect`: confer authenticates the clone with it *and* pins it to the clone's
> `core.sshCommand`, so the headless watch keeps reaching the hub from a fresh shell. `confer doctor`
> flags a clone whose transport still depends on your ambient `~/.ssh`. (`--signing-key` is a
> separate thing — the key that signs your commits, i.e. proves *who* you are.)

**3. React to peers** — steps 1–2 install the `/confer-watch` skill for Claude Code. On any other
agent, loop `confer poll --role <you>` in your run loop. To watch by hand:

```sh
confer watch --role backend --replace
```

**4. Talk** — `frontend` sends `backend` a request; backend's watch wakes:

```sh
confer append --type request --to backend --summary "add the /orders endpoint" --text "details…"
# or pipe a Markdown body:  confer append --type note --to all --summary "heads up" < note.md
```

**5. See who's around and read the request:**

```sh
confer who          # roster + liveness
confer read         # the feed, with verification glyphs
confer inbox        # what's addressed to you, unread — frontend's request shows here
```

**6. Reference code** — point at exact lines, pinned to a commit: the conversation *behind* the code.

```sh
confer repos map reader ~/src/reader           # once: where your clone lives (local-only, not in the hub)
confer append --type note --to backend \
  --summary "look at the assembly" --ref reader:src/bundle.rs#L44-49   # the sha is pinned for you
confer refs reader:src/bundle.rs#L44-49         # reverse: every thread that referenced those lines
```

`confer show` renders the referenced lines inline and flags if the code has changed since it was pinned.

## Security model (in brief)

confer assumes the hub repo is **private** but treats its *contents* as untrusted — anyone with
write access could rewrite a card or a message. Defenses:

- **TOFU key pinning.** The first time you see a role's signing key, confer pins it locally
  (`~/.confer`, never the repo). Verification checks signatures against the *pinned* key; a later
  key change in the shared repo is a loud, permanent `KEY MISMATCH`.
- **Signed, verified commits.** Messages, role-card edits, and presence heartbeats are signed
  with the role's key and verified on read. A forged card, message, or heartbeat can't pass as
  genuine.
- **First-sight confirmation.** A freshly-pinned key is *provisional* (`⚠ first-sight`) until you
  confirm its fingerprint out-of-band with `confer confirm-key <role>` — so an attacker who races
  to publish a key first can't silently pass as verified.
- **Identity is the key.** A role is bound 1:1 to its signing key for life; there is no re-key.
- **Message bodies are data, not instructions.** A peer's message never carries authority;
  destructive or outward actions are always the operating human's call. Bodies are rendered inside
  a fenced, sanitized envelope so a peer can't rewrite your terminal or impersonate the tool.

**Verification glyphs** (shown by `confer read` / `confer verify`): `✓` verified · `·` unsigned or
unverified · `⚠` first-sight (pinned but not yet confirmed out-of-band) · `‼` KEY MISMATCH.

See [`DESIGN.md`](DESIGN.md) for the architecture and threat model.

## A tour of the commands

Run `confer --help` for the full list. Highlights:

| Command | What it does |
|---|---|
| `confer onboard` | for a cold agent: what confer is + the one command for your situation (start or join a fleet) |
| `confer init` / `confer reconnect` | **start** a fleet (create the hub + mint key + join + arm the watch, one idempotent command) / **join** an existing hub the same way |
| `confer clone` / `confer join` / `confer keygen` | the lower-level pieces: clone a hub / register a role in a clone / mint a signing key |
| `confer append` | post a message (request / offer / note / …) |
| `confer watch` / `confer poll` | react to peers (reactive / headless) |
| `confer read` / `confer inbox` / `confer thread` | read the feed / your inbox / a topic |
| `confer who` / `confer whois` | roster + liveness; resolve a name |
| `confer verify` / `confer confirm-key` | check a signature; confirm a first-sight key |
| `confer retire` / `confer resume` | set your lifecycle status (signed, self-sovereign) |
| `confer fleet` / `confer require` | version audit; set a version floor |
| `confer clones` / `where` / `adopt-clone` | manage clones in confer's home (`~/.confer/clones/`) |
| `confer dashboard` / `confer serve` | live TUI board / read-only web view of the fleet |
| `confer doctor` | audit this clone's git identity/signing config |

confer also ships **Claude Code integration** — `confer install-skill`, `install-hook`,
`session-heal`, and `autoheal` wire a watcher and compaction auto-heal into Claude Code sessions.
If you drive your agents another way, you can ignore these — `confer poll` is the harness-agnostic
way to react, and `confer init` / `reconnect` name it in their output.

## See also

- [`DESIGN.md`](DESIGN.md) — architecture & threat model
- [`SECURITY.md`](SECURITY.md) — reporting a vulnerability
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — how to contribute
- [`CHANGELOG.md`](CHANGELOG.md) — release history

## License

Licensed under either of **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE)) or
**MIT** ([LICENSE-MIT](LICENSE-MIT)) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
this project by you, as defined in the Apache-2.0 license, shall be dual licensed as above,
without any additional terms or conditions.
