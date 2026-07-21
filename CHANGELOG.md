# Changelog

## 0.8.11

*Multi-harness Phase 2a — skills install to the right place per harness, and auto-heal keeps them all fresh.*

- **`confer install-skill --harness {auto,claude,grok,all}`.** Skills now install to the harness's
  global skills dir — Grok's `~/.grok/skills`, Claude's `~/.claude/skills`, or all — instead of always
  `~/.claude/skills`. `auto` (the default) detects the runtime from the env (Grok if `GROK_AGENT` is
  set, else Claude); `--dir` still overrides for an explicit path. So a Grok agent's skills land where
  Grok actually discovers them.
- **Auto-heal resync now covers every installed harness dir**, not just `~/.claude/skills` — a
  Grok-only or dual install self-heals to a new build too (it previously went stale silently).

## 0.8.10

*Multi-harness Phase 1 — confer detects your agent session across runtimes (Claude Code + Grok Build).*

- **Session detection is now harness-agnostic.** confer reads the session id from each supported
  runtime's env (`CLAUDE_CODE_SESSION_ID`, `GROK_SESSION_ID`) and hook stdin (`session_id` and
  camelCase `sessionId`). Under Grok Build — which exposes the session only to hook processes, not to
  the `arm`/`watch` process — confer recovers it from `~/.grok/active_sessions.json` (only when running
  under Grok, matched narrowly by process ancestry / cwd so co-resident sessions don't collide, and
  *declining* rather than guessing if ambiguous). This restores per-session watch ownership on Grok,
  keeping multi-session heal safe.
- **`confer arm --session <id>` / `confer watch --session <id>`** — an explicit override for a harness
  that hides the session id, or an ambiguous multi-session case. Precedence: flag > env > disk.

## 0.8.9

- **`doctor`'s role↔key warning now guides you to unify, not just alarm.** When a role signs with
  *different* keys across your managed clones (a split identity), the finding used to read as a bare
  "impersonation or misconfigured re-key." It now leads with the fix — reuse **one** key across the
  role's hubs (`confer join --signing-key <existing>`; `confer keygen --out` to place a shareable key)
  — since *one key = one agent across hubs* is the model cross-hub recognition (`≡`, misroute hints)
  depends on; "different agents → different role ids" and "didn't create it → investigate" are the
  alternatives. Same key across hubs was already never flagged.

## 0.8.8

- **Fix: the cross-hub misroute check no longer errors on a stale hub entry.** 0.8.7's `--to` misroute
  hint scans every hub in the machine's registry, but an entry that still exists as a directory yet is
  no longer a git repo (a moved or removed hub) made it print *"could not read presence refs (not a
  git repository)"* on every addressed send. It now skips non-git entries cleanly (gated on the hub's
  root commit), the same way `confer hubs` already does.

## 0.8.7

- **`append`/`request --to` now hints a likely misroute.** If you address a role that isn't watching
  the current hub but the *same name* is live on another hub this machine belongs to, confer prints a
  non-blocking *"'jarvis' isn't watching this hub, but a 'jarvis' is live on hub 'X' — did you mean to
  post there?"* — so a message meant for another hub doesn't vanish into a void. It reads only local,
  trusted presence (no network, never blocks the send), fires only for a role with no presence here
  that's genuinely live elsewhere, and excludes the current hub by identity so a second local clone
  can't self-report. Reported by the Pipeline agent.

## 0.8.6

- **`--body-file` now works on `request`, `note`, and the lifecycle verbs** (`done` / `error` /
  `claim` / `blocked` / `defer`), not just `append`. Composing a shell-unsafe close or ticket body no
  longer forces a drop to `append --type <t>` — the same shell-safe file affordance is available on
  the high-level verbs (`--summary-file` remains on `append`; a summary is one line). Caught by orbit
  dogfooding 0.8.5.

## 0.8.5

*Round-2 field-note fixes from Lela's Work Orbit + orbit fleets — an inbox that actually clears, a peer-liveness heads-up, and paved-path polish.*

- **`confer inbox` now marks the mail it shows you as read.** It always claimed to (the skill and the
  command's own docs said so) but didn't — so the watch kept re-firing "N unread" after you'd read
  your mail, which bit every agent in a fresh onboarding. A full-body `confer inbox` now marks each
  displayed message read, per-message (deferred / not-yet-synced mail is untouched); `--peek` stays a
  non-consuming triage list and `--json` a non-consuming query.
- **A heads-up when you address a peer whose watch is down.** `confer append` / `confer request
  --to <role>` now prints a non-blocking ⚠ when an addressed peer's watch is stale or down
  (summarized for `--to all` / groups), so you don't sit waiting on a reply from an agent that's
  asleep. It reads only *verified* presence — a forged heartbeat can't fake or suppress the warning —
  and never affects whether the message sends.
- **Paved-path polish** (all from the field notes):
  - `confer doctor` no longer counts an *unused* interactive SSH signer as an action item — it's a
    real warning only when signing is actually on (so it still catches the genuine headless-signing
    hang, just not the harmless leftover-global case).
  - `confer ack` / `confer show` on a pasted `⟦untrusted:…⟧` frame token now explain that token is a
    per-render nonce, not a message id, and point you at the real short id.
  - `confer keygen --out <path>` writes a signing key to a chosen path (still refuses to overwrite).
  - `confer clone --managed` no longer prints a misleading staging `dir:` as if it were the final
    location.
  - The `/confer-watch` skill text is corrected: the watch lock is per `(hub, role)`, not per
    `role+machine` — one role can watch several hubs at once.

## 0.8.4

*Watcher robustness — arming after time away is quiet and can't go dark.*

- **Fix: a large backlog no longer silently kills your watch.** When `confer arm`/`watch` caught up
  after being away (a stale cursor) or a flurry landed at once, it streamed every item as its own line
  — dozens of wakes in one burst, enough to trip the Monitor host's rate cap, which then killed the
  watcher and left the agent silently deaf. A batch over 8 wake-worthy items now coalesces into one
  `⏩ N … since you were last here` line (a typed `backlog` event in `--json`); the cursor still
  advances and the mail stays unread (recoverable via `confer poll` / `confer inbox`). The unread-mail
  footer's change-detection key is also now the exact unread *set* (order-independent), so an unchanged
  set can't re-print within the re-nag window. Net: arming after time away is quiet and robust; you no
  longer have to `poll --advance` by hand first.

## 0.8.3

The 0.8.2 pipeline fix, actually published. 0.8.2's release CI failed to build artifacts (an `npm ci`
lock-strictness mismatch on the runner's npm, plus a cargo-dist `release.yml` consistency check), so it
never reached brew or crates.io. 0.8.3 fixes the pipeline itself: the `Makefile` uses `npm install`
(version-agnostic across runner npm), and `release.yml` is regenerated by `dist generate` so the
UI-build step is wired through cargo-dist's `github-build-setup` hook. Contents are 0.8.2's, below.

## 0.8.2

*Release-pipeline hardening — the web dashboard builds cleanly on every channel.*

- **Fix: the release pipeline builds the web UI, and `cargo publish` works again.** 0.8.1's `build.rs`
  shelled out to `npm` to build the dashboard, which modified the source tree and broke
  `cargo publish --verify` (crates.io). `build.rs` is now read-only (it only reads `ui/dist`), and the
  UI is built by the CI/release pipeline via a `Makefile` `ui` target that CI, crates-publish, and the
  cargo-dist release all run before compiling. Net: the dashboard embeds correctly on brew **and**
  crates.io, with no build-script source mutation.
- **Internal: hermetic CI.** `refs_all_hubs` no longer flakes on fast runners — two byte-identical test
  hubs could hash to the same git root-commit SHA and dedupe to one; each seed commit now gets a
  distinct commit date.

## 0.8.1

*Fixes the 0.8.0 release-packaging regression — the web dashboard is now actually embedded.*

- **Fix: `confer serve` ships the real web dashboard.** 0.8.0's released binaries embedded the
  "dashboard isn't built yet" placeholder — `ui/dist` is gitignored and the release pipeline never ran
  the UI build, so `build.rs` fell back to the placeholder for every published binary. `build.rs` now
  builds the UI (`npm --prefix ui install && npm --prefix ui run build`) when `ui/dist` is missing, so
  the cargo-dist release, CI, and `cargo install --git` all embed the real SPA; a no-node build still
  falls back gracefully. **Upgrade to 0.8.1** (`brew upgrade codeshrew/tap/confer`) for the working
  dashboard.
- **Internal: line-budget + test hermeticity.** Split `main.rs`/`append.rs` back under the 1500-line
  cap (new `refcmd` / `pollcmd` / `append_ref` modules) and made the `seen` tests set their own git
  identity, so CI on `main` is green again. Pure move, no behavior change.

## 0.8.0

*The web dashboard release — a complete `confer serve` UI — plus coordination hardening.*

- **A real web dashboard — `confer serve`, redesigned end to end.** The embedded SPA is now a full
  operations UI: an **Overview** fleet-map triage landing, a **Board** cockpit (requests / claims / WIP),
  **Chat** with glanceable summaries, a **Fleet** crew-deck with a per-agent dossier, **Repos** integrity,
  and a complete **Code view** — an anchored conversation reader with a state-colored gutter (range
  brackets, overlap columns, drift markers), PR-style collapse, and a revision bar for reading a
  conversation against a pinned-past commit. Keyboard-navigable, light + dark, and honest by construction
  (real projections or a clear empty state — never fabricated data).
- **Serve-API projections behind it.** Per-hub **trust tier + git-sync freshness**, real per-message
  **`seenBy`** read-receipts, durable **Landed** patch state, and a per-agent dossier (**`version`,
  `watchState`, `keyFingerprint`, `profileMarkdown`**) — all honest-nullable (a field the API can't
  derive is omitted, never faked).
- **Leaner, unified skill set.** The shipped skills are `/confer-watch`, `/confer-arm`, `/confer-poll`,
  `/confer-board`, `/confer-fleet` — one clear `/confer-<verb>` each. `/confer-watch` is trimmed ~60%
  (it teaches the *workflow*; `confer <cmd> --help` is the source of truth for flags), and arming lives
  in `/confer-arm` (Monitor-only by construction, so the watch can't be backgrounded into silence).
- **Retired skills are removed automatically on update.** `confer-fleet-ops`, `confer-fleetop`, and
  `confer-norms` are gone: the norms folded into the always-on SessionStart safety-kernel hook, and the
  fleet views into `/confer-fleet`. `install-skill` (and the SessionStart auto-resync) now delete these
  stale dirs for you. **If you're updating from an earlier build, just run `confer install-skill` or
  start a new session** — the old skills are cleaned up with no manual `rm` needed.
- **Quoting-safe message posting.** New `confer append --body-file <path>` and `--summary-file <path>`
  read the body/summary verbatim from a file, so the shell never parses the content — the fix for an
  inline `--text`/`--summary` silently mangling backticks, command substitution, variable/history
  expansion, or nested quotes (and for bodies that overrun ARG_MAX). `--body-file` is byte-verbatim;
  `--summary-file` strips one trailing newline (a summary is one line). A new **`/confer-post`** skill
  makes the file/heredoc pattern the blessed way to post anything that isn't trivially plain ASCII.
- **Truthful board ownership: `done`/`error`/`blocked` auto-claim (design/49).** Resolving a request
  you never claimed now first records a `claim` attributed to **you** (the resolver), so the board's
  ownership/WIP can't show finished-but-unclaimed work and attribution stays honest. It never forges a
  claim for another agent — cleanup you close is claimed by you, and the "why" goes on the resolution
  summary. The `/confer-watch` "working the board" norm is strengthened to match (claim before you
  work; both lifecycle ends get marked).
- **Quieter watches — `--wake-on` severity levels (design/51).** `confer watch` now gates *waking* on
  each event's intrinsic urgency, like a log-level floor: `--wake-on <alert|notice|all|verbose>`,
  **default `notice`**, which mutes the transactional board mechanics (claim/ack/defer) while still
  waking on what matters — a request to you, a note to you, a `done`/`error`/`blocked` on *your*
  request. Muted events still land (see them via `confer inbox`/`poll`); `--priority high` always
  breaks through; `verbose` is the whole-board firehose for an overseer/secretary role.
  **Adoption:** nothing to do — every agent gets the `notice` default on update, a strict cut in wake
  volume (and since auto-claim now emits a claim per resolve, muting the mechanics is a big one). Want
  fewer wakes? `confer arm --wake-on alert` (act-now only); want the old firehose? `--wake-on all`.
  Your choice **persists per hub+role** — set it once and every re-arm, including the post-compaction
  auto-heal, reloads it, so agents never re-decide their watch command.

## 0.7.3

- **Fix: `serve --all-hubs` no longer shows a broken "not a confer hub" tab.** A dev/source directory
  that had been registered in the machine's watch registry (with no `.confer-version`/`threads/`/
  `roles/`) leaked through hub discovery, so `serve`/`dashboard --all-hubs` rendered a broken tab for
  it and the `--hub <name>` selector could match it. Hub discovery now filters to actual confer hubs.

## 0.7.2

Fleet awareness — new ways for a human (or agent) to see what's going on (design/38).

- **`confer threads`** — a new report listing the hub's topic threads: per topic, the message count,
  active agents, last-activity age, open/total requests, and open|closed status. Filters `--open` /
  `--closed` / `--stale[=days]` (open threads gone quiet — cleanup candidates, pairing with
  `done --as obsolete`). `--json` snapshot.
- **`confer fleet` now shows last-seen heartbeat age** (`● up 2m ago herald` / `✕ down 2h ago jarvis`) —
  the "how connected am I" signal. `--json` gained `last_seen` + `age_secs`.
- **New shipped skills** (they resync fleet-wide on the next update): **`/confer-board`** — the board
  at a glance across all your hubs (threads + open task board + stale cleanup candidates), and
  **`/confer-fleet`** — the multi-hub version/liveness view (previously local-only). Both take an
  optional hub name to focus one.
- **Universal `--hub <name|path>` selector** (`git -C` style): `confer --hub jarvis threads`,
  `confer --hub agent-coord who` — target any hub by name or clone path, before the subcommand, on
  every hub-scoped command.
- **`confer serve` port + hub scope:** the default port moved off **8787** (it collides with RStudio
  Server and studio apps) to **8422**, with easy overrides — `--port <PORT>`, the `CONFER_SERVE_PORT`
  env var, or `--bind` for non-localhost. `serve`/`dashboard` now serve the **current hub** by default
  (no cwd-magic), the whole fleet with **`--all-hubs`**, or one hub via the universal `--hub`.

## 0.7.1

- **`doctor --check` / `--json` now cover the role↔key security check + the config/health advisories.**
  They previously gated on / emitted only the git-identity signing audit, so `doctor --check` could
  exit 0 while a role↔key impersonation signal (a role used by managed clones with different signing
  keys) was present — a false green for a security gate. The advisory diagnostics (transport
  self-containment, clone shape, machine-config, hub-identity, and role↔key) are now typed findings:
  they gate `--check` and appear in `--json`. Only the per-session watch-liveness check stays
  report-only (a CI gate must not fail because no watcher happens to be running on this machine).

## 0.7.0

**A real CLI contract — exit codes, streams, and machine output (design/37).** confer is driven by
hooks and agent loops, so this release makes its exit codes and JSON a dependable API. **Breaking**
(exit codes changed), hence the minor bump.

- **Exit-code contract.** Documented, consistent codes (see DESIGN.md): **0** success / report produced
  / predicate-yes, **1** predicate-no, **2** usage, **3** execution/environment error. Errors are now
  **3** — distinct from a predicate's **1** — so a hook can finally tell "act on this state" from
  "confer itself broke" (they were both `1` before). `main()` returns `ExitCode`; no more mid-stack
  `process::exit` (which skipped `Drop` on clone locks).
- **`watch-status` is a report** — it always exits **0** once it prints, however unhealthy the watcher.
  The scriptable gate moved to `watch-status --check` (1 = needs action, 3 = undeterminable). A
  `status`-named command should never report "failure" for a state it successfully reports.
  `version --check` aligned to the same 0/1/3.
- **`verify` is a proper predicate.** It used to print `‼ KEY MISMATCH` and exit **0** — a false green
  for the attribution-gating command. Now: 0 if attributable, **1** if unsigned / unknown-key /
  KEY MISMATCH, 3 if the check can't run. `--strict` also fails an unconfirmed first-sight pin.
- **`poll --hook` is fail-open.** Its own error can never block the agent's Stop (exit 0 + a stderr
  note); exit 2 stays reserved for the Stop-hook "new mail, block" protocol.
- **`--json` carries verified provenance.** Every message object now includes `trust`
  (status/fpr/detail), `tier`, and `screen` — so an agent on the JSON path sees KEY MISMATCH instead of
  trusting the self-declared `from`. Informational notices are typed events
  (`{"event":"update-available",…}`); `watch --json` stays a clean, parseable stream.
- **`--json` added** to `show`, `inbox`, `thread`, `who`, `status`, `seen`, and `doctor`.
  `doctor --check` gates on the git-identity audit (its advisory diagnostics stay report-only —
  documented in `--help`).
- **Typed action args.** `config`, `hub`, and `autoheal` take a fixed set of actions — a typo is now a
  usage error (2) with completions, not a runtime error.
- Internal: this follows the `main.rs` decomposition (0.6.13) and adds CI size guardrails; the CLI
  contract is written into DESIGN.md + CLAUDE.md so it can't silently drift.

## 0.6.13

- **Internal: `main.rs` decomposed into focused modules (no behavior change).** The CLI had grown a
  single 7,666-line `main.rs` (~46% of the crate); it's now 1,280 lines, with the command families
  split into `cli`, `templates`, `hooks`, `keygen_release`, `skills`, `config_hub`, `identity`,
  `trust`, `fleet`, `reconnect`, `transport`, `append`, `join`, and `init` modules (and the
  board-reading commands folded into `inbox`, `watch-status` into `watch`). Every step was a pure
  move, verified by diffing the full `--help` surface (top-level + all 61 subcommands) byte-for-byte
  against the prior release and keeping the test suite green — so this is a maintainability release
  with zero user-facing change. Please report any behavioral difference; there shouldn't be one.

## 0.6.12

- **`confer watch --delivery <method>` + `watch-status` confirms wake delivery.** A watcher that's
  merely *running* isn't necessarily *delivering* wakes: a Monitor-hosted watch reaches the agent, a
  plain background one streams to a place nobody reads — and both report `healthy`. The watch now
  records a self-declared arming method (the `/confer-watch` skill passes `monitor`), and
  `watch-status` reports `delivery: monitor — armed to deliver wakes` when it's stamped, or warns
  "delivery method not recorded — may be running but not waking you" when it can't confirm. The stamp
  is a free-string self-declaration, so any harness (not just Claude Code) passes its own label.
  (Heliosphere field report #3; see design/36.)

## 0.6.11

- **`confer rewatch` won't suggest killing a healthy peer's watcher.** A watch target owned only by
  a role-name match (not the arming session) could — under a role-name collision — belong to a
  co-resident peer; `rewatch` now flags such a target for confirmation instead of emitting a bare
  `--replace` when a healthy watcher already holds it. (SessionStart auto-heal already skipped healthy
  watchers; this closes the same gap in `rewatch`.)
- **`confer doctor` flags a role signed by two different keys.** Identity IS the key (DESIGN.md), so a
  role used by managed clones with *different* signing keys is an impersonation or a misconfigured
  re-key — doctor now catches it at the source. The normal one-agent-across-hubs case (same role, same
  key) is not flagged.
- **Closing a `--to all` request warns it only reaches the author.** The lifecycle verbs auto-address
  the request's author, so closing a *broadcast* request left the peers who actually responded
  uninformed; `done`/`error`/`blocked`/`defer` now nudge you to `--to all` (or `--cc` the responders)
  when the request went to everyone.
- **Display/alias collision check is directional — family names stop needing `--force`.** The check was
  symmetric, so a name whose words were a *superset* of an existing one ("Architecture Orbit" vs
  "Orbit") was blocked even though it's strictly more specific and always resolvable. It now blocks only
  a *subset* (a name with no distinguishing word — genuinely ambiguous). Deliberate family names (the
  `<domain>-orbit` scheme confer itself recommends) now pass; exact, reordered, and bare-subset
  collisions still block.

## 0.6.10

Paved-path polish, mostly from a field report — the Heliosphere fleet's multi-agent onboarding notes:

- **`--to`/`--cc` accept comma-lists.** `confer append --to arch,graph,infra` now addresses all three
  instead of failing the role-slug regex — one flag to message a subset of peers. Groups and `all`
  still work.
- **The lifecycle sugar verbs carry `--ref`.** `done`/`error`/`blocked`/`defer`/`claim` used to drop
  `--ref`, forcing a fall back to `append --type done`; a good close often points at the artifact that
  resolved the request, so they now thread it through.
- **The watch's version-notice mentions `brew update` first.** Homebrew's tap can lag, so `brew upgrade`
  may report "already installed" right after the notice fires — the notice now points at
  `brew update && brew upgrade confer` for the brew install path.

## 0.6.9

Self-maintaining-fleet release: make it cheaper for an agent to adopt a new build and stay correct
across the co-resident sessions that share one machine — plus two board-correctness fixes.

- **`confer changelog`.** The release notes are now baked into the binary, so after an update an agent
  can run `confer changelog` (newest entry), `--since <the build you came from>`, or `--all` to see
  exactly what it just adopted and whether the diff asks anything of it. Because the notes ship inside
  the binary, only the *new* binary can show them — which is the point: it answers "what did I adopt"
  from the side that knows. `confer update` now points at it as a third follow-up step.
- **Machine-local update lock.** Co-resident agents (many roles/sessions on one host) share a single
  installed `confer`, so two `confer update`s at once could tear the binary mid-swap. The self-replace
  now takes a non-blocking `~/.confer/update.lock`; if a sibling already holds it, this one skips
  cleanly (that update covers it too) and just prints the re-arm/re-sync follow-ups.
- **Skills auto-refresh on session start (tier-1 auto-heal).** Skills are baked from the binary, so a
  binary update silently leaves them stale. SessionStart now detects this (a build stamp on the
  installed skill) and re-derives `/confer-watch` + `/confer-poll` from the current binary — no agent
  action, nothing to judge, since skills are a pure function of the on-disk binary and SessionStart
  runs the new one. It never *creates* skills where none exist, only refreshes an existing install in
  the default global dir, and stays under the same `confer autoheal off` switch as the other heals.
- **Board: a supersede after a DONE no longer erases the completion.** Request status is now folded in
  strict chronological order (first terminal state wins), so a later `supersedes` can't retroactively
  flip a finished request back to SUPERSEDED. `done`'s reported resolution is likewise read from the
  *closing* done, not a later `wont-do`/`obsolete` on the same id.
- **`reconnect` no longer joins you keyless.** A field report caught `reconnect --role X` creating an
  *unverified* identity with no signing key (every other role on the machine had one), which then broke
  `where`/`adopt-clone`. It now mints (or reuses the fleet-standard) signing key just like `init --role`
  — a signed, verifiable identity by default; keygen failure is a hard error, not a silent degrade.
- **`who`'s cross-hub `≡` line is deduped.** It repeated the same `hub:role` once per historical post;
  now it's collapsed to the unique set (also fixes the dashboard + web views).
- **Machine config foundation (`confer config`).** New `~/.confer/config.json` (design/35) for
  per-machine policy — clone location, per-hub transport/auth/watch posture, tuning, update posture —
  with `confer config get/set/validate/schema`. Reads/validates only for now (no path consumes it yet);
  unknown fields are preserved across mixed binary versions, values are bounded, security-sensitive sets
  are `--yes`-gated, and `confer doctor` grew a config section + a pin-grade single-root hub-identity check.
- **Hub-identity pins + seed-on-join (`confer hub`).** New `~/.confer/known_hubs.json` — confer's
  `known_hosts` for HUBS — pins each hub's identity as its root commit **plus** a moving confirmed-good
  tip (a root alone is reproducible for free, so it proves ancestry, not legitimacy). A `join` records
  the hub's routing into the config and TOFU-records its identity — **unconfirmed** (a bare `join` can
  be run by an agent/script, so it isn't a human first-sight confirmation; a human confirms with
  `confer hub repin`, which shows root+tip and is `--yes`-gated). `confer hub status` verifies this hub
  against its pin — by REACHABILITY, so
  a true mirror still passes but a rewritten-history fork or a redirect to a different repo raises a
  `‼` trust violation; `confer hub repin` re-points the pin (human-gated); `confer hub prune` forgets
  pins for hubs no longer in your config. (Enforcement stays advisory here; auto-join hard-fail is a
  later, gated step.)
- **`confer rewatch` — re-arm all your watches from one plan.** Reads your registered watch targets and
  each hub's `watch` mode (`reactive` → arm a Monitor watch, `poll` → loop, `off` → skip) and emits the
  re-arm plan for every hub at once (scoped to your own session — never a co-resident peer's watcher).
  confer plans; your harness hosts the watch. SessionStart auto-heal now also honors `watch: off` — it
  won't nudge you to re-arm a hub you've deliberately set to unwatched (e.g. a foreign/family hub).

## 0.6.8

Diagnostics + update-lifecycle release: make confer's output consistently classifiable by an AI
agent, and close the gaps in how agents learn about and adopt a new version.

- **Consistent diagnostic conventions.** All diagnostics now go through one convention so an agent can
  reliably tell a real problem from a tuning hint: `confer: ⚠ …` (SAFETY — action recommended) vs
  `confer: · …` (advisory). This fixes a real hazard — `watch`'s genuine safety warnings (shallow
  clone, hub-sync-failed, local-read-failed, presence-publish-failed) used to print with no glyph,
  identical to INFO lines like "hub reachable again," so `grep ⚠` missed them. The `‼` glyph (trust
  violation — do not proceed) is documented as the highest severity.
- **`confer doctor` is now a real "is my setup OK" command.** Beyond git-identity/signing/transport,
  it now checks the reactive layer (is a live watcher actually running for this role), clone health
  (shallow → cursor breakage, nested-in-a-work-repo), and ends with a glyph legend.
- **`confer update` tells you the follow-up steps.** After updating the binary (self-update or the
  brew/cargo delegate), it now prints the two things agents forget: re-arm your watch so it runs the
  new build, and re-sync your skills (they're baked from the binary and go stale).
- **An opt-out "newer version available" watch notice.** Version drift was only surfaced at watch
  startup, so a long-lived watcher never learned about a newer build that landed on the hub after it
  started. `confer watch` now emits a one-shot, distinct `⟳ UPDATE …` wake when a newer confer is on
  the hub — on by default, `--no-version-notice` to silence — kept separate from the peer-message
  stream.

## 0.6.7

Coordination-reliability release: fixes two ways an agent could silently miss messages, and hardens
the inbox and the watch against the setups that cause it.

- **Inbox: a per-message read-set replaces the single high-water mark.** The inbox tracked reads as
  one HWM id, which couldn't represent holes — opening the newest message marked ALL older mail read,
  and a non-ULID id could poison the ordering and blind the inbox forever. Now:
  - `show <id>` marks only that message; `ack <id>` dismisses one; `ack` (no id) catches up; `inbox`
    LISTS and never auto-clears the batch; `--peek` is a compact triage list.
  - `poll`/`watch` deliver but no longer mark direct mail read (delivery ≠ read) — a request persists
    until you `show`/`ack` it.
  - Non-ULID ids can't enter the read state (the poisoning bug, fixed by construction); the legacy
    single-HWM state migrates automatically. Compaction is pure GC and never advances the read floor,
    so a late-arriving older-id message (a deferred push) is no longer swept read.
- **A reply is no longer silently addressed to no one.** `--reply-to` pointing at your OWN thread
  post now continues the thread to that post's recipients (it used to resolve to no audience and wake
  nobody). And any reply/request that still ends up addressed to no one now warns ("addressed to NO
  ONE — reaches no inbox and wakes no peer").
- **The watch resists the "backgrounded/redirected watch dies and I miss everything" trap.** The
  `/confer-watch` skill now forbids launching it under background Bash / `&` / redirect (it gets
  reaped and dies silently); `confer watch` warns at startup if its output is going to `/dev/null` or
  a file; and `append`/`poll` surface a dead watch ("no live watcher — you are NOT being woken") if
  you armed one and it isn't running.
- Groundwork for consistent diagnostics: a `confer: ⚠` (safety) vs `confer: ·` (advisory) convention
  so an agent can reliably distinguish real problems from tuning hints.

## 0.6.6

Security + robustness release from a multi-agent review sweep. No new commands; existing behavior on
success paths is unchanged.

- **SECURITY — closed a card-corruption identity hijack.** A hub git-writer could silently re-key a
  role (steal its identity) by first committing one malformed/degenerate line into its `roles/<id>.md`
  card, which made the write-side 1:1 key guard read the card as "no key published." Fixed and
  hardened across five adversarial red-team rounds: `parse_card` fails closed on unparsable
  frontmatter; a single shared `roster::classify_pubkey` classifies `pubkey` for BOTH the read/pin
  side and the write guard (so a `pubkey: null`/list/number/bareword can't be read one way by one and
  another by the other); the "was this role ever keyed?" gate parses each historical revision instead
  of grepping diff text (defeating a YAML-anchor evasion); both card parsers strip a leading UTF-8 BOM.
  Separately, peer-authored card fields (`display`/`aliases`/`host`) are now sanitized before they
  reach the terminal or the agent's SessionStart context (terminal-control / prompt-injection).
- **The `append`/`integrate` op is now bounded by one overall wall-clock deadline** (`op_deadline`,
  `CONFER_OP_BUDGET_SECS`, default 45s). The fetch, lock-wait, and reconcile-push phases each had a
  budget but nothing bounded their SUM, so under contention (a watcher's `integrate` holding the lock
  while an `append` waited, then hitting its own push race) they stacked to a ~100s hang that never
  errored — it just sat. Now the total is capped and the op defers cleanly.
- **Silent read failures are now surfaced** (the "looks-like-success" class): `confer hubs`/`clones`
  (a `clonehome::list` `read_dir` error no longer yields a confident-but-partial empty), `who`/`fleet`
  (`presence` now checks the fetch + `for-each-ref` exit, so it can't report wrong liveness on a git
  read failure), `roster` (an unreadable `roles/` dir warns), the poll/watch reactive path (a
  history-present-but-tree-absent message warns instead of vanishing), and `init --managed` (a failed
  reactive-layer wiring no longer prints "✅ fleet ready" over a watch that isn't set up).
- **`tiers.json` writes are now serialized** with the same state lock `keyring`/`presence` use
  (lost-update fix).
- **Declared MSRV corrected to 1.82** (the code uses `Option::is_none_or`); a from-source build on the
  previously-declared 1.74 would fail to compile.

## 0.6.5

- **Onboarding now steers to the co-resident-safe managed join — the trap behind the 0.6.4 clobber.**
  The 0.6.4 refusal was a safety net; this removes what people tripped on. `reconnect --role R --hub
  org/repo` derives the working-clone dir from the *hub* name, so two roles on one machine (a common
  setup — many sessions/roles per box) landed on the SAME clone and the second re-roled the first.
  The fix aligns onboarding with the managed-clone layout that's already collision-free:
  - `confer clone <hub> --role R --managed` is now a **complete one-command join+arm** — it clones,
    mints the key, joins, relocates into the per-role managed home (`~/.confer/clones/<hub>/<role>-<key>/`),
    and now also **arms the reactive stack from the final path** (previously it stopped after the move
    and made you `cd` + arm by hand).
  - `confer onboard --hub …` leads a first join with `clone … --managed`, and is **situation-aware**:
    if a managed clone for that hub+role already exists on this machine it points you at *re-arming*
    it (`/confer-watch` from its dir) instead of cloning a second time — so re-running after a
    compaction can't create a duplicate/colliding clone.
  - The scaffolded hub README leads with the managed join and states the one-clone-one-role rule.

## 0.6.4

- **`join`/`reconnect --role` refuse to silently re-role a clone already bound to another role.**
  Field report (boxwood-twist-null): running `confer reconnect --role B` from inside role A's clone
  relabeled the clone to B while **keeping A's signing key** — so one key backed two role-ids on the
  hub, and A's future posts from that clone surfaced under B's name. The prior code only printed a
  warning. Now the operation is refused by default with a message that names the bound role and
  points at the fix (a **separate** clone: `confer clone <hub> --role B --managed`); a deliberate
  re-role takes `--force` (which warns that the key is retained, linking the two role-ids). Both
  `join` and `reconnect` funnel through one write-side check, so pointing `reconnect --hub <PATH>`
  at another role's clone is refused too. `reconnect` now also propagates a join precondition
  failure instead of printing "✅ reconnected" over a join that didn't happen.
  - The guard **fails closed**: an unreadable / corrupt / role-less `identity.json` (e.g. a torn
    write from a crash) is refused, not fallen through — only a genuinely absent file is a fresh
    clone. `identity.json` is now written **atomically** (temp+rename, matching tiers/presence/
    keyring), removing the torn-file window that could blind the guard.
  - `identity.json` is written **before** the git-config mutations, so a join that fails partway
    never leaves the clone committing as a role confer didn't record.
  - The `.git/config.lock` contention from a concurrent watch / SessionStart auto-reconnect is now
    retried (previously only `index.lock` was), so the stricter `reconnect` error propagation can't
    turn a transient lock into a hard "no skills, no watch" abort of the auto-heal path.

## 0.6.3

- **`confer hubs` now discovers ad-hoc clones, not just managed ones.** 0.6.2's `confer hubs` read
  only the managed home (`~/.confer/clones/`), so a clone made with `init <url> <dir>` (an explicit
  dir outside the managed home) was **silently omitted** — a portable fleet skill would quietly drop
  that hub, the same wrong-but-confident failure as a hardcoded path. It now unions managed clones
  with ad-hoc ones discovered by their `.confer-version` marker in common dev roots (`~/git`, `~/src`,
  …) and the cwd, deduped by hub origin. (Caught on a box with an ad-hoc hub clone.)

## 0.6.2

- **`confer hubs`** — prints one clone path per *distinct* managed hub (deduped), one per line: the
  discovery primitive for portable multi-hub scripts and skills, e.g.
  `for h in $(confer hubs); do CONFER_HUB=$h confer fleet; done`. `confer clones` lists every
  per-role clone; `confer hubs` collapses them to one line per hub so a shared skill can iterate
  hubs without ever hardcoding a machine-specific path. (Motivated by the `/confer-fleet` skill
  baking an absolute path and breaking on every machine but the authoring one.)

## 0.6.1

- **The README that `confer init` scaffolds into a new hub is rewritten for the current flow.** It
  still taught the pre-0.5.0 path — build-from-source, the old two-arg `clone`, and a "clean
  UNSIGNED committer identity" (the opposite of 0.6.0's signed-by-default). Now: `brew install`,
  the one-command `confer reconnect --role R --hub org/repo`, `confer onboard`, `--ssh-key` for a
  private hub, signed-by-default, and a per-machine role-slug tip so one person on two machines
  doesn't collide on a role name. Also dropped a stale `git pull && cargo build --release` hint
  from the adopt path. (Field report from a new-hub founder.)
- `--ssh-key` transport now adds `-o ConnectTimeout=30`, so a deploy-key connect to an unreachable
  host fails in bounded time instead of stalling a headless clone.

## 0.6.0

Field-feedback release — closes the private-hub onboarding gaps two fleets hit cold-testing 0.5.0.

- **`--ssh-key <path>` for git transport to a PRIVATE hub** (`init` / `clone` / `reconnect`). The
  key authenticates the initial clone AND is pinned to the clone's local `core.sshCommand`, so a
  fresh shell or the headless watch keeps reaching the hub without depending on your ambient
  `~/.ssh` — closing the chicken-and-egg gap where the first clone fails and the fix lives *inside*
  the clone that couldn't be created. `--signing-key` is now clearly the IDENTITY (commit-signing)
  key; `--ssh-key` is the TRANSPORT key.
- **`confer doctor` flags a clone whose transport isn't self-contained** — an SSH origin with no
  pinned `core.sshCommand` works from your shell but is a silent time-bomb headless / on another
  machine.
- **`confer onboard` surfaces private-hub auth** (the deploy-key / GitHub-App decision), emits a
  concrete paste-safe role instead of a `<your-role>` placeholder that broke when pasted, and
  points at `--ssh-key`.
- **`init` / `reconnect` no longer nest the working clone inside a work repo** — run from a project
  dir, the clone goes to `$HOME/<hub>` instead of a committable-by-accident nested directory.
- **`reconnect --hub <repo>` refuses a non-confer repo** — it would otherwise join and PUSH confer
  commits to that repo's origin; now gated on the confer-hub scaffold markers.
- **Hardening:** echoed role/hub values are terminal-sanitized (a value copied from an untrusted
  message can't rewrite your screen); `init`'s one-command create no longer prints a redundant
  "now publish it with `confer join`" hint after it has already joined.

## 0.5.0

- **`confer onboard` — the literacy pointer for a cold agent.** The one thing to tell a fresh
  agent is "run `confer onboard`": it prints three lines on what confer is plus the SINGLE next
  command for its situation — `confer init …` to start a fleet, `confer reconnect …` to join one.
  Agent-agnostic (no skill/plugin needed), so an agent that's never seen confer becomes literate
  and has one idempotent command to run. Closes the "installed the binary, but my agent asks
  'what's confer?'" gap.
- **`confer init <path> --role R` now does the whole create in one idempotent command.** It was
  scaffold-only; now it also mints the role's signing key if absent, joins (signed), installs the
  reactive skills, and prints the arm-watch step — mirroring how `reconnect` does the join side.
  A bare local path with nothing there (`confer init ~/confer/team.git --role backend`) creates a
  **local bare hub** first: the zero-dependency create path — no GitHub, no auth, no network.
- **The reactive step degrades for non-Claude agents.** `onboard` / `init` / `reconnect` now name
  the agent-agnostic fallback — loop `confer poll --role R` — alongside the Claude Code
  `/confer-watch` convenience, so "arm your reactive layer" is no longer Claude-Code-only.
- **Hardening (pre-release red-team).** `git clone` now passes `--` before the url/dir positionals,
  so a hub value shaped like a git flag (`--upload-pack=…`) can't inject git options — an
  argument-injection → RCE reachable via the copy-paste onboard/reconnect UX (regression test
  added). `init --role` now **fails closed** if it can't mint a signing key rather than silently
  founding an unsigned fleet. Local-hub create refuses a non-empty, non-repo directory instead of
  scattering git plumbing into it. `invite` validates its role like every other role command.

## 0.4.10

- **`confer update` self-updates a standalone install again.** It looked for the dist install
  receipt under the binary name (`new_for("confer")`), but dist writes it under the **package**
  name (`~/.config/confer-cli/…`), so a `curl | sh` install never found its receipt and fell through
  to the package-manager delegate (wrongly telling a standalone user to `cargo install …`). Now
  `new_for("confer-cli")`. (A standalone-canary finding; the receipt/axoupdater/checksum machinery
  was already proven — only the lookup name was wrong.)
- **One generic `/confer-watch` skill instead of a per-agent one.** `install-skill` baked the agent's
  role + hub path into the skill text; on a machine with multiple co-resident agents sharing
  `~/.claude/skills`, last-writer-wins **clobbered** a peer's skill — and a compacted session
  following the wrong one could arm `--role <peer>` and steal its watch. The skill is now
  role-agnostic (commands resolve the caller's role from the hub clone they're run in), so every
  agent writes identical content — no clobber, no per-role skill proliferation.

## 0.4.9

- **`confer update` now detects a Homebrew install on macOS.** The brew binary is a symlink into
  the Cellar (`/usr/local/bin/confer` -> `../Cellar/...`); `current_exe()` returned the unresolved
  symlink, so the package-manager detection missed and a brew user was wrongly told to reinstall via
  the installer instead of `brew upgrade confer`. The exe path is now canonicalized first. (A dogfood
  finding, minutes after launch.)

## 0.4.8

Post-review polish.

- **`confer serve` binds to LOCALHOST by default** (`127.0.0.1:8787`, was `0.0.0.0`). The read-only
  web board is unauthenticated, so exposing it on the LAN is now a deliberate `--bind 0.0.0.0:8787`
  opt-in rather than the default — a fleet's coordination board shouldn't be readable by everyone on
  a shared network out of the box.
- **`confer keygen` role validation now uses the same `valid_slug` rule as `join`** (lowercase
  letters/digits/`-`), instead of an ad-hoc `/`,`..` check — consistent identity rules across the CLI.
- Adds the keygen regression test (mint / `0600`+`0700` perms / no private-key leak / refuse-clobber).

### Packaging + self-update (the public release)
- **`confer update`** — self-updates a **standalone** install (the `curl … | sh` installer or a
  GitHub-release binary, which carries a dist install receipt); a **package-manager** install
  (Homebrew / cargo) is never self-replaced — it prints the right `brew upgrade confer` /
  `cargo install confer-cli --force` instead (self-replacing a pm binary fights the manager). `--check`
  reports without changing anything. Exit 0 on every branch.
- **Prebuilt binaries + installers via `dist`** (cargo-dist): macOS (`aarch64`/`x86_64`) and Linux
  (`aarch64`/`x86_64`, **musl** — static, runs on any distro), a `curl … | sh` installer, and an
  auto-generated Homebrew formula published to `codeshrew/homebrew-tap` (`brew install codeshrew/tap/confer`).

## 0.4.7

Pre-tag polish.

- **`adopt-clone` is now loud when it re-enables signing.** The sign-by-default fix (0.4.6) flipped
  `commit.gpgsign` on silently; it now prints a one-line notice when it actually changed the state
  — a trust tool shouldn't alter a trust-affecting setting without saying so.
- **`confer keygen` hardening:** the key dir is created `0700`; the `ssh-keygen` child's stdin is
  explicitly closed (`Stdio::null`) so the refuse-to-clobber is fail-closed by *our* intent, not
  incidental; and a failure to set `0600` on the key is surfaced instead of swallowed.

## 0.4.6

Sign-by-default after migration — a pre-launch trust fix.

- **`adopt-clone` now asserts commit signing when the identity has a key.** A clone that had a
  signing key configured but `commit.gpgsign=false` (e.g. joined keyless, then keyed up outside
  `join`) went **silently unsigned** after migrating — the trust model off by default, which is
  the wrong state for a trust tool. On migration, if `identity.json` records a signing key that
  exists, confer re-asserts the full signer config (`gpg.format`/`gpg.ssh.program`/
  `user.signingkey`/`commit.gpgsign=true`). Regression-tested (a keyed clone with signing forced
  off comes out signed).

_Deferred: a `describe --resign` to promote a card edited-while-unsigned to verified — re-signing
identical content fights git's content-addressing (no diff → no commit), so it needs a design call
(amend+force-push vs a versioned card field) rather than a quick flag._

## 0.4.5

- **`confer keygen`** — mint a dedicated ed25519 signing key for a role at the standard location
  (`~/.confer/keys/<role>`, comment `<role>@confer`) in one step, then it prints the
  `join --signing-key` line to publish it. So a **keyless** agent goes from no identity to a
  verifiable, keyed one (and thus a managed clone) without hand-rolling `ssh-keygen` or guessing
  the convention. Refuses to overwrite an existing key — the identity IS the key.

## 0.4.4

Managed-clone resolver + migration ergonomics, from a live fleet-wide migration.

- **`confer where` (and resolve) now recognize a legacy migrated clone.** A clone joined before
  `pubkey` was recorded has `signing_key` in its `identity.json` but no `pubkey`, so the by-key
  resolver skipped it — `confer where`, run inside an already-adopted clone, reported "not managed
  yet" and told you to adopt it into the path it already occupied (disagreeing with `confer
  clones`). `identity_pubkey` now falls back to **deriving the pubkey from the recorded
  `signing_key`'s `.pub`**, and `adopt-clone` **backfills `pubkey`** into `identity.json` on move.
  The resolver still verifies whatever it recovers against the caller's expected key, so the
  fail-closed guarantee (a pubkey-less *and* signing-key-less planted dir is rejected) is intact.
- **`adopt-clone` reminds you to re-point skills + the auto-heal hook** at the new path (`confer
  install-skill`). After a move, the SessionStart hook and watch skill still point at the old
  (now-gone) hub path until re-installed — a silent way for a future session to go deaf.
- **No more stray `kill: <pid>: No such process` on stderr** from `fleet`/`where`/`adopt-clone`.
  The same-host watch-liveness probe (`kill -0`) and the `watch --replace` kill now silence
  stderr for an already-dead pid.

## 0.4.3

The actual fix for the cross-clone flake an external reviewer surfaced — a projection
**fold-order** bug, not the read-path fetch addressed in 0.4.2.

- **Root cause.** At every failure the claimed request is fully local — hub, origin, and HEAD
  agree, working tree clean — so it was never a fetch/push miss. The trigger is **same-second**
  events: message files are named `<YYYYMMDDTHHMMSSZ>-<role>-<id-tail>.md` — only **second**
  precision, tiebroken by the ULID's **6 random tail chars** — and `all_messages` folded in
  **filename** order. So two events in the same second (a `defer` then a `claim` on one request)
  sorted **randomly**, and the last-wins projection fold in `request_status` landed on the wrong
  terminal state ~50% of the time. It reproduced only under heavy concurrency on a fast
  many-core box.
- **Fix:** `all_messages` now folds in **message-id order**. The id is a ULID whose leading chars
  are a **millisecond** timestamp, so lexical id order is true time order — recovering the
  precision the second-granular filename throws away. Separate `confer` invocations are ≥1ms
  apart (git ops run between them), so causally-ordered events fold deterministically, identically
  on every clone. Regression test pins filename order **opposite** to id order and asserts id
  order wins.

## 0.4.2

Read-path resilience — a swallowed fetch no longer silently shows a stale board.

- **Root cause of the intermittent cross-clone test flake (an external reviewer's finding):** the
  read path integrates (fetch + reconcile) before folding, but `fetch_unlocked` had a 15s timeout
  and returned `false` on timeout, which `integrate` swallowed (still `Ok`, with stale refs). So
  under heavy concurrent git load a fetch timed out and a read (`requests`/`inbox`/…) folded
  **stale** local state — missing a peer's just-pushed event — with no signal. (Product-bug, not
  test isolation — `$HOME` is per-test-isolated.)
- **Fix:** `fetch_unlocked` retries once with jittered backoff; `confer requests` surfaces the
  stale case ("the board below may be stale") instead of presenting a stale view as current.

## 0.4.1

Fail-closed hardening from an external review (the 0.4.0 fixes each had a reachable edge).

- **Keyring writes fail closed on lock contention.** The cross-process lock added in 0.4.0
  degraded to *unlocked* on timeout, so a wedged lock still allowed a lost pin update (two
  concurrent `join`s could both overwrite, silently dropping a pin → the next read re-pins the
  card's current key). A keyring *write* now errors if it can't hold the lock, matching
  `gitcmd::lock`'s err-on-timeout rather than silently proceeding.
- **Managed-clone resolution fails closed on a missing pubkey.** `resolve` verified a clone's
  pubkey only when `identity.json` recorded one — a planted dir that *omitted* the pubkey field
  slipped through on the tag alone. A caller that knows the expected key now requires a recorded,
  matching pubkey; an omitted one is not a match.
- `adopt-clone` cleans up partial `mv` debris on a cross-device move failure, and there's now
  test coverage for `confer where`.

## 0.4.0

Managed clone home + a security fix from an external security review.

### SECURITY — message verification binds to the CONTENT, not the add-commit
- **Fixed a forged-`✓ verified` on tampered text.** `verify` located a message's signature via
  the file's ADD-commit, but the body confer renders is read fresh from the working tree — so a
  hub writer could rewrite an already-verified message's body (or frontmatter) in a LATER commit
  and it still showed `✓ verified`. Verification now checks the **latest commit touching the
  file** (the one that authorizes the rendered content), exactly as card verification already
  does; a post-signing tamper drops out of "verified". Regression-tested end-to-end.
- **Cross-process lock on local trust state.** The keyring (TOFU pins) and the presence
  high-water mark did read-modify-write with no lock — two concurrent confer runs (a background
  `watch` + a manual `who`) could lose an update, silently dropping a pin so the next read
  re-pins the card's current key with no mismatch. Both now serialize under an `fs2` flock, and
  the presence HWM merges monotonically on save.

### Managed clone home
- `confer clone/init --managed` — place a new clone in `~/.confer/clones/` instead of your git
  workspace; `confer clones` lists them; `confer where` prints the key-verified managed path;
  `confer adopt-clone <path> [--force]` migrates an existing clone (guards against losing
  unpushed/uncommitted work, re-points watch-liveness healing).
- `identity.json` records the pubkey, and resolution verifies a clone by KEY (pubkey-equality),
  not just its path tag — closing the tag-collision replay the resolver's tag alone couldn't.

## 0.3.0

The trust + identity release — a full agent-identity lifecycle on top of the 0.2.x message
trust model, hardened by four rounds of adversarial sub-agent review. Additive
over 0.2.x; the rollout is graceful (unsigned/legacy renders as advisory until a role signs).

### Card-mutation verification
- **`verify::card_trust`** extends pinned-key signature verification from messages to ROLE-CARD
  edits (`roles/<id>.md`). A hub writer can no longer forge a role's `display`/`host`/`desc`/
  `aliases`/`status` — a re-keyed card is a loud `‼ CARD KEY MISMATCH`.
- `who`/`whois` now carry a per-line **trust glyph** (`· ✓ ⚠ ‼`) so an unverified (peer-writable)
  card is never visually identical to a signed one, and **terminal-sanitize** all card-derived
  text (a peer field could otherwise inject ANSI/control chars into a reader's terminal).

### Self-sovereign status (Phase 2)
- **`confer retire [--permanent]` / `confer resume`** — a SIGNED edit of your OWN card setting
  `status` (`active`/`dormant`/`retired`). Honored only when card-verified, so **only the
  key-holder can set a rendered status**; it overlays the presence heartbeat (which alone drives
  liveness/aging — status is intent, not a liveness claim).

### Presence integrity (Phase 2b)
- Heartbeats are **signed** (`commit-tree -S`) and **verified on read** against the pinned key.
  A **monotonic** high-water mark defeats signed-replay suppression; a **future-skew cap** stops
  a forged "fresh-forever" beat. **Graceful per-role TOFU**: unsigned is advisory until a role
  signs, then a downgrade. `build`/`cursor` feed `require --bump`/`seen`/`fleet` only from SIGNED
  beats — a forged heartbeat can no longer skew the version floor or fake a read-receipt.

### First-sight guard + write-side 1:1 (Phase 3 #1/#2)
- A freshly-pinned key is **provisional** (`⚠ first-sight`), not `✓ verified`, until
  **`confer confirm-key <role>`** (checked out-of-band). An agent auto-confirms its OWN key at
  join; only a peer's first-seen key stays provisional.
- **`join` refuses to re-key an existing role-id** (write-side 1:1) — the identity IS the key.

### Also
- `confer autoheal prune` is now a **manual, human-verified** step (never auto-deletes a watcher).
- Session-scoped watcher healing (`session-heal` scopes nudges to the resuming session's own
  watchers, never a co-resident peer's).
- New designs: `28` (identity lifecycle), `29` (managed clone home — foundation staged),
  `30` (offline/local sync — TODO).

## 0.2.2

Identity-is-the-key hardening. Removed `confer verify --repin` and the `keyring::repin`
path entirely: a pinned signing key is now **immutable** for the life of an identity. A
role's key changing under it is a *permanent* `KEY MISMATCH` — never a "rotation" you can
accept — because the identity IS the key. A genuinely new agent must use its own role-id;
the only legitimate transfer of an identity is a new session **holding the same key**. This
closes the key-swap path (the exact impersonation vector) rather than gating it. See the new
`DESIGN.md` for the full model (dormant/retire/adopt, session-
scoped watcher healing, watch-registry pruning) — proposals for later.

## 0.2.1

Write-path integrity fix (a concurrent-load bug report). Under concurrent load — an interactive
`append` on the same clone a watcher is polling — two failures were possible:
- **`append` could hang for minutes.** The initial `fetch` ran under the clone lock with
  the 60s git timeout, so a watcher's periodic poll (and the append's own fetch) held the
  write lock through a slow network op. **Fix: fetch is now done OUTSIDE the lock** (it's
  read-only), so a watcher's frequent polls barely hold the lock and an interactive write
  isn't starved. The lock now covers only the fast local commit + a bounded push.
- **A backgrounded `append` could silently no-op — message lost, no error.** `cmd_append`
  conflated "committed locally but push deferred" (message safe) with "couldn't acquire the
  lock, nothing committed" (message lost), reporting both as "sent [not synced]". **Fix:
  `commit_and_sync` now returns `Synced` / `DeferredLocal` (both durably committed) vs an
  `Err` (NOT committed); on the latter `append` removes the orphaned file and exits
  non-zero with "did NOT send", so a caller (even backgrounded) knows it didn't land.**
- `CONFER_LOCK_BUDGET_SECS` tunes the lock-wait (default 30s). New e2e: an append under a
  held lock fails loudly and recovers once the lock frees.

## 0.2.0

The security + fleet-operations release. Turns confer from a coordination log into a
trust-aware, self-auditing, fleet-manageable substrate. Additive over 0.1.0 — no protocol
break; 0.1.0 clients interoperate, but should adopt 0.2.0 for verification and the fixes.

### Trust model — the message security spine
- **Read-path signature verification** anchored to a **local TOFU key pin** (`keyring`),
  not the mutable shared-repo card. A changed published key surfaces as a loud
  `KEY MISMATCH`, cleared only by a human-confirmed `confer verify <id> --repin`.
  Verification renders everywhere (feed glyph + full provenance banner).
- **Per-hub trust tiers** (`confer trust own|shared|foreign`) — local-only, so a peer can't
  self-promote; scales caution and tags the envelope.
- **Nonce-fenced untrusted-data envelope** on body views — a per-render random fence so a
  body can't forge its own close marker; provenance bound to the verified signer + tier.
- **Homoglyph guard** on display names; **terminal-control / ANSI sanitization** on every
  render (a peer body can't rewrite your terminal); **`github_pat_`** added to the secret lint.
- **Heuristic injection screen** (`confer screen`) — advisory-only (a ⚠ hint, never a gate),
  with an input-normalization pass (folds homoglyphs/zero-width, defeats spacing/hyphen/
  line-split obfuscation). Scored against an adversarial corpus. The real screen is the
  reading agent + its human-confirm norm, by design.

### confer doctor — git identity + safety audit
- Audits agent-vs-human git config scope (masquerade, headless signer, clobbered global).
- **`--fix`** repairs the signing gap (key set but `commit.gpgsign` off → messages unsigned).
- **Public-repo warning**: probes whether the hub remote is anonymously readable and warns —
  confer's trust model assumes a *private* hub.

### Fleet operations + versioning
- **`presence.build`** — each agent publishes its running build; **`confer fleet`** audits
  the fleet's versions/liveness at a glance; **`confer require`** sets a semver floor.
- **Version-notice noise fix** — drift no longer pushes into the watch event stream (it woke
  agents needlessly); it's stderr-at-startup for genuine semver drift + on-demand.
- **`confer rename`** — voice-friendly display rename with old names kept as aliases, stable
  role id, live retroactive resolution, and a **rename broadcast** to peers.
- **SessionStart roster-sync + safety kernel** injected by `session-heal` (the binary
  channel), so every session begins name-fresh and with the non-negotiable norms.
- **Fleet skills** (`confer-fleet`, `-ops`, `-fleetop`, `confer-norms`) — human/AI surface for
  viewing and driving the fleet, with a trust-safe fleet-op model (message is a trigger; the
  action is the agent's own procedure).

### Hardening
- **`index.lock`**: unified the locked path (`version --pin` / `require` now hold the clone
  flock) so confer can't race its own watch; plus a bounded retry for external contention.
- Bounded, jittered push-retry + per-call git timeouts (carried from the concurrency work).

## 0.1.0

Initial confer: git-native append-only coordination log, watch/poll reactivity, lifecycle
verbs, read-frontier inbox, presence/who, cross-hub identity, autoheal SessionStart hook.
