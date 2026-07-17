# Changelog

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
