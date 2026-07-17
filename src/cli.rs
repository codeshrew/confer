//! The command-line surface: the clap `Parser`/`Subcommand` definitions.
//!
//! This is pure argument *data* — derive macros only, no handler logic (the handlers live in
//! `main.rs` and the command modules). The doc-comments on `Cmd`'s variants and their fields ARE
//! the `--help` text, so edits here change user-facing help verbatim. `LifecycleArgs` (the shared
//! claim/done/error/blocked/defer flag block) still lives in `main.rs`; it's referenced here via
//! `crate::LifecycleArgs` for the `#[command(flatten)]`.

use clap::{Parser, Subcommand};
use crate::{LifecycleArgs, VERSION};

#[derive(Parser)]
#[command(
    name = "confer",
    version = VERSION,
    about = "git-native coordination blackboard for AI agents"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) cmd: Cmd,
}

#[derive(Subcommand)]
pub(crate) enum Cmd {
    /// Claim a role for this session; prints the role's open assignments (resume).
    Join {
        #[arg(long)]
        role: String,
        #[arg(long)]
        host: Option<String>,
        /// human-friendly display name to register on the hub (default: the role id)
        #[arg(long)]
        display: Option<String>,
        /// one-line description of what this role does (shown in the role card)
        #[arg(long)]
        desc: Option<String>,
        /// SSH key to sign this agent's commits (overrides a global 1Password
        /// signer); its pubkey is published in the role card for verification
        #[arg(long = "signing-key")]
        signing_key: Option<String>,
        /// re-role a clone that already belongs to a DIFFERENT role. Off by default: doing so keeps
        /// this clone's signing key, so one key would back two role-ids (they become linked). For a
        /// new role, prefer a SEPARATE clone.
        #[arg(long)]
        force: bool,
    },
    /// Release the role's lease (record a clean handoff). [stub]
    Leave,
    /// List known roles and their presence.
    Who,
    /// Append one message (Markdown body via --text or stdin).
    Append {
        /// message type: note | request | claim | done | error | supersede
        #[arg(long = "type")]
        msg_type: String,
        /// REQUIRED one-line summary — the triage field peers read before opening the body.
        #[arg(long)]
        summary: String,
        /// message body; if omitted, read from stdin (supports multi-line/fenced)
        #[arg(long)]
        text: Option<String>,
        /// primary addressee target(s) — role id, group, or `all`; repeatable
        /// (--to a --to b). REQUIRED for `request`.
        #[arg(long = "to")]
        to: Vec<String>,
        /// secondary audience target(s) — role id, group, or `all`; repeatable
        #[arg(long = "cc")]
        cc: Vec<String>,
        /// triage hint: low | normal | high
        #[arg(long)]
        priority: Option<String>,
        /// thread/topic slug (folder); defaults to "general"
        #[arg(long)]
        topic: Option<String>,
        /// id of the message this replies to (threading)
        #[arg(long = "reply-to")]
        reply_to: Option<String>,
        /// request id (for claim/done/error) — short id ok, resolved to canonical
        #[arg(long)]
        of: Option<String>,
        /// id of the message this supersedes (required for type supersede)
        #[arg(long)]
        supersedes: Option<String>,
        /// override the writing role (defaults to the joined role)
        #[arg(long)]
        from: Option<String>,
        /// content provenance: agent | web | human (external → downweight)
        #[arg(long)]
        src: Option<String>,
        /// point at a durable doc/spec instead of re-transmitting it:
        /// `repo:path[@sha][#Lstart-Lend]` (repo resolves against `confer repos`);
        /// repeatable. sha defaults to HEAD.
        #[arg(long = "ref")]
        refs: Vec<String>,
        /// allow a summary-only message (empty body) — otherwise an empty/`-` body
        /// is rejected, so content isn't silently lost. (claim/done/error/supersede
        /// are summary-only by default — their summary IS the payload.)
        #[arg(long)]
        allow_empty_body: bool,
        /// resolution for a terminal `done`: wont-do | duplicate | obsolete
        /// (default: done) — lets a request close WITHOUT being completed.
        #[arg(long = "as")]
        resolution: Option<String>,
        /// mark a request as backlog/someday — captured but kept OFF the active
        /// `requests` board until promoted.
        #[arg(long)]
        defer: bool,
        /// post anyway even if the body looks like it contains a secret (the lint
        /// blocks common token/key shapes — history is permanent + fleet-wide).
        #[arg(long = "allow-secret")]
        allow_secret: bool,
    },
    /// Claim a request (you're taking it). Sugar for `append --type claim --of`.
    Claim {
        #[command(flatten)]
        args: LifecycleArgs,
    },
    /// Mark a request done. `--as wont-do|duplicate|obsolete` closes it *without*
    /// completion (a conscious drop). Sugar for `append --type done --of`.
    Done {
        #[command(flatten)]
        args: LifecycleArgs,
        #[arg(long = "as")]
        resolution: Option<String>,
    },
    /// Mark a request failed. Sugar for `append --type error --of`.
    Error {
        #[command(flatten)]
        args: LifecycleArgs,
    },
    /// Mark a request blocked/waiting (off the active board → `requests --blocked`);
    /// re-`claim` when unblocked. Sugar for `append --type blocked --of`.
    Blocked {
        #[command(flatten)]
        args: LifecycleArgs,
    },
    /// Backlog a request (someday) — anyone can, incl. the addressee. Sugar for
    /// `append --type defer --of`.
    Defer {
        #[command(flatten)]
        args: LifecycleArgs,
    },
    /// Print what's new since the cursor, then exit (for /loop and hooks).
    Poll {
        #[arg(long = "since-cursor", default_value_t = true)]
        since_cursor: bool,
        #[arg(long)]
        advance: bool,
        #[arg(long)]
        topic: Option<String>,
        #[arg(long)]
        hook: bool,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        role: Option<String>,
        /// firehose: include notes and everything else, not just actionable items
        #[arg(long)]
        all: bool,
        /// only messages where I am the addressee (to/cc)
        #[arg(long = "to-me")]
        to_me: bool,
    },
    /// Print one full message (by id or id-prefix) — triage a summary, then open it.
    Show { id: String },
    /// List requests with their derived status (open/claimed/done/error).
    Requests {
        /// only requests not yet done/errored (open or claimed) — the active board
        #[arg(long)]
        open: bool,
        /// only requests I sent or am assigned
        #[arg(long)]
        mine: bool,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        json: bool,
        /// show the deferred/someday BACKLOG instead of the active board
        #[arg(long)]
        backlog: bool,
        /// show only BLOCKED requests (waiting on a dependency/human)
        #[arg(long)]
        blocked: bool,
    },
    /// Assemble a request's lifecycle: the message + everything referencing it
    /// (claims/dones/errors/replies/supersedes), transitively.
    Thread {
        id: String,
        #[arg(long)]
        full: bool,
    },
    /// Browse/catch-up (does not touch the cursor).
    Read {
        #[arg(long)]
        last: Option<usize>,
        #[arg(long)]
        topic: Option<String>,
        /// print full message bodies (Markdown), not one-line summaries
        #[arg(long)]
        full: bool,
        #[arg(long)]
        json: bool,
    },
    /// Stream new actionable events until stopped (drives the Monitor tool).
    Watch {
        #[arg(long, default_value_t = true)]
        follow: bool,
        #[arg(long = "since-cursor", default_value_t = true)]
        since_cursor: bool,
        #[arg(long)]
        topic: Option<String>,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long = "poll", default_value_t = 10)]
        poll_secs: u64,
        #[arg(long = "no-advance")]
        no_advance: bool,
        /// take over a watcher already running for this (hub, role) — kills the
        /// old one (e.g. an orphan left by a compacted session) instead of refusing
        #[arg(long)]
        replace: bool,
        /// firehose: wake on ALL actionable board traffic, not just what's
        /// addressed to me (for an overseer role). Default: only my own mail.
        #[arg(long)]
        all: bool,
        /// only wake on messages at/above this priority: low (default, all) |
        /// normal | high. Lower-priority items still land — seen via `poll`.
        #[arg(long = "min-priority", default_value = "low")]
        min_priority: String,
        /// don't emit the one-shot "a newer confer is on this hub — update" wake (it's on by
        /// default; version drift is otherwise only seen at watch startup / `confer status`).
        #[arg(long = "no-version-notice")]
        no_version_notice: bool,
        /// how this watcher delivers wakes (stamped for `watch-status`): the `/confer-watch` skill
        /// passes `monitor`; a poll loop `poll`. Any harness passes its own label. Omit for a manual
        /// run — `watch-status` then can't confirm you're actually receiving events.
        #[arg(long)]
        delivery: Option<String>,
    },
    /// Is a watcher running for your role on THIS machine — and is it yours and on
    /// the current build? Run this first thing after a compaction to decide whether
    /// to re-arm (`watch --replace`). A REPORT: exits 0 whenever it produces the report
    /// (even when the watcher is unhealthy). For a scriptable gate, add `--check`.
    WatchStatus {
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        json: bool,
        /// scriptable gate: exit 1 if the watcher needs action (re-arm), 3 if it can't be
        /// determined, 0 if healthy. Without this, the command always exits 0 (it's a report).
        #[arg(long)]
        check: bool,
    },
    /// Queryable health: hub reachability, unpushed/unintegrated commits, watch
    /// state, disk headroom. Pull-not-push — confer handles transient degradation
    /// quietly and self-heals; this is how you check on demand if you care.
    Status,
    /// Live read-only TUI: agents (liveness + cross-hub identity), the task board +
    /// flow, health, and a live activity tail — folded from local hub clones, no
    /// server. `--hub <dir>` (repeatable) follows specific hubs; defaults to the
    /// current hub. Keys: q quit · r refresh · c toggle closed · Tab switch.
    #[cfg(feature = "dashboard")]
    Dashboard {
        #[arg(long = "hub")]
        hub: Vec<String>,
    },
    /// Read-only web view of the fleet (same data as `dashboard`) — a pure-Rust
    /// server rendering the board/agents/health/activity as HTML, auto-refreshing.
    /// Binds to LOCALHOST by default (the board is unauthenticated); to open it on
    /// your phone, deliberately expose it with `--bind 0.0.0.0:8787`. `--hub`
    /// (repeatable) or the current hub. Read-only: never posts, never locks.
    #[cfg(feature = "serve")]
    Serve {
        #[arg(long = "hub")]
        hub: Vec<String>,
        /// Address to bind. Default 127.0.0.1:8787 — LOCALHOST only, because the board is
        /// served WITHOUT auth. Pass e.g. `--bind 0.0.0.0:8787` to expose it on your LAN on
        /// purpose (anyone who can reach that address can read your fleet's coordination board).
        #[arg(long, default_value = "127.0.0.1:8787")]
        bind: String,
    },
    /// Install the SessionStart auto-heal hook into Claude Code settings so a
    /// compacted session is told to re-arm a stale watcher. User scope by default;
    /// `--project <dir>` for project scope. Inert until `autoheal on`.
    InstallHook {
        #[arg(long)]
        project: Option<String>,
    },
    /// Remove confer's SessionStart auto-heal hook from Claude Code settings.
    UninstallHook {
        #[arg(long)]
        project: Option<String>,
    },
    /// Hook target (run by Claude Code on SessionStart) — checks watcher health and
    /// injects a re-arm nudge if needed. Not meant to be run by hand.
    SessionHeal,
    /// Toggle/inspect auto-heal: `on` | `off` | `status` | `prune`.
    Autoheal {
        /// on | off | status | prune
        action: String,
        /// with `prune`: actually remove stale targets (default is a dry-run listing).
        #[arg(long)]
        yes: bool,
    },
    /// Inspect or set this machine's policy config (`~/.confer/config.json`: clone location,
    /// per-hub transport/auth, tuning, update posture — design/35). NOT the shared repo contract
    /// and NOT trust state. Confer-managed; don't hand-edit the JSON.
    Config {
        /// show | get | set | validate | schema  (omit → show)
        #[arg(default_value = "show")]
        action: String,
        /// dotted key for get/set, e.g. `machine.clone_root`, `hubs.<name>.scheme`, `tuning.git_timeout_secs`
        key: Option<String>,
        /// value, for `set`
        value: Option<String>,
        /// confirm a security-gated `set` (url / auth / auto_update / clone_root / a new hub block)
        #[arg(long)]
        yes: bool,
    },
    /// Hub-identity pins (`~/.confer/known_hubs.json` — confer's `known_hosts` for hubs, design/35):
    /// `status` shows the pins + verifies this hub against its pin; `repin` deliberately re-points
    /// this hub's pin (human-gated, verify out-of-band first); `prune` forgets pins for hubs no
    /// longer in your machine config.
    Hub {
        /// status | repin | prune
        #[arg(default_value = "status")]
        action: String,
        /// confirm a `repin`, or actually apply a `prune` (default is a dry-run listing)
        #[arg(long)]
        yes: bool,
    },
    /// Emit a re-arm plan for ALL your hubs' watches at once, honoring each hub's `watch` mode in your
    /// config (`reactive` → arm a Monitor watch; `poll` → loop; `off` → skip). confer PLANS the set;
    /// your harness HOSTS the watch (confer can't spawn a persistent Monitor). Scoped to YOUR own
    /// registered watch targets — never a co-resident peer's.
    Rewatch {
        /// limit to one hub (by name, or a clone-path substring)
        #[arg(long)]
        only: Option<String>,
    },
    /// Your cross-hub identity: your signing-key fingerprint and where else the
    /// SAME key appears (the same agent across hubs you've joined). F3.
    Identity {
        #[arg(long)]
        role: Option<String>,
    },
    /// Resolve a loose phrase ("my iOS agent", "the book one") to a role — fuzzy
    /// match against ids / displays / descriptions / aliases / hosts.
    Whois {
        /// the phrase to resolve (quotes optional: `confer whois my ios agent`)
        #[arg(required = true, num_args = 1..)]
        phrase: Vec<String>,
    },
    /// Update your OWN role card: set a description and add/remove the aliases the
    /// owner uses for you. Alias adds are collision-checked against every other
    /// role's names/aliases.
    Describe {
        #[arg(long)]
        role: Option<String>,
        /// one-line "what I am / do"
        #[arg(long)]
        desc: Option<String>,
        /// set the display name peers see (the rename; homoglyph/collision checked).
        #[arg(long)]
        display: Option<String>,
        /// add a nickname/phrase (repeatable); rejected if it collides.
        #[arg(long = "add-alias")]
        add_alias: Vec<String>,
        /// remove an alias (repeatable).
        #[arg(long = "remove-alias")]
        remove_alias: Vec<String>,
        /// add even if it looks confusingly close to another role's name.
        #[arg(long)]
        force: bool,
    },
    /// Retire yourself: mark your card `dormant` (paused, resurrectable) or, with
    /// `--permanent`, `retired` (a tombstone — key-lost/gone-for-good). A self-sovereign,
    /// signed edit of YOUR card; peers can't set it. Intent only — liveness/aging
    /// still comes from the presence heartbeat. `confer resume` returns to active.
    Retire {
        #[arg(long)]
        role: Option<String>,
        /// permanent tombstone (`retired`) instead of the resurrectable `dormant`.
        #[arg(long)]
        permanent: bool,
    },
    /// Resume: return your card to `active` (undo a `retire`). Self-sovereign + signed.
    Resume {
        #[arg(long)]
        role: Option<String>,
    },
    /// Rename yourself: set a short, voice-friendly display name (and register it as an
    /// alias so the owner can refer to you by it). Sugar over `describe --display`. The
    /// role ID never changes — history/attribution are stable.
    Rename {
        /// the new display name, e.g. `confer rename Gil`
        #[arg(required = true, num_args = 1..)]
        name: Vec<String>,
        #[arg(long)]
        role: Option<String>,
        /// allow a name that looks confusingly close to another role's.
        #[arg(long)]
        force: bool,
    },
    /// Clone a hub and set up a working copy (pins the `main` branch; scaffolds
    /// a fresh/empty hub). Idempotent — safe on an already-initialized hub.
    Init {
        /// hub git URL (ssh/https) or the `owner/repo` GitHub shorthand
        url: String,
        /// target directory (default: the repo name from the URL)
        dir: Option<String>,
        /// also `join` this role after setup
        #[arg(long)]
        role: Option<String>,
        /// force the SSH URL scheme (default: autodetect, fall back on auth failure)
        #[arg(long)]
        ssh: bool,
        /// force the HTTPS URL scheme
        #[arg(long, conflicts_with = "ssh")]
        https: bool,
        /// display name to register for --role (default: the role id)
        #[arg(long)]
        display: Option<String>,
        /// one-line role description to register for --role
        #[arg(long)]
        desc: Option<String>,
        /// SSH key to sign --role's commits — the IDENTITY key (its pubkey is published in the
        /// card). Proves WHO you are; see --ssh-key for the key that lets you REACH the repo.
        #[arg(long = "signing-key")]
        signing_key: Option<String>,
        /// SSH key that AUTHENTICATES git transport to a PRIVATE hub (push/fetch), e.g. a deploy
        /// key. Used for the clone AND written to the clone's local `core.sshCommand`, so a fresh
        /// shell or the headless watch keeps working without depending on your ambient ~/.ssh.
        #[arg(long = "ssh-key")]
        ssh_key: Option<String>,
        /// place the clone in confer's managed home (~/.confer/clones/…) instead of `dir`.
        #[arg(long)]
        managed: bool,
    },
    /// Alias of `init` for joining an existing hub (accepts `owner/repo` shorthand).
    Clone {
        url: String,
        dir: Option<String>,
        #[arg(long)]
        role: Option<String>,
        #[arg(long)]
        ssh: bool,
        #[arg(long, conflicts_with = "ssh")]
        https: bool,
        #[arg(long)]
        display: Option<String>,
        #[arg(long)]
        desc: Option<String>,
        #[arg(long = "signing-key")]
        signing_key: Option<String>,
        /// SSH key that authenticates git transport to a PRIVATE hub (see `init --ssh-key`).
        #[arg(long = "ssh-key")]
        ssh_key: Option<String>,
        /// place the clone in confer's managed home (~/.confer/clones/…) instead of `dir`,
        /// keeping it out of your git workspace.
        #[arg(long)]
        managed: bool,
    },
    /// List the clones confer manages under `~/.confer/clones/`.
    Clones,
    /// Print one clone path per DISTINCT hub (deduped), one per line — the discovery primitive for
    /// portable multi-hub scripts/skills: `for h in $(confer hubs); do CONFER_HUB=$h confer fleet; done`.
    /// Managed clones only; `adopt-clone` an ad-hoc clone to have it listed.
    Hubs,
    /// Print the managed-home path for THIS clone's identity — resolves it by key (verified),
    /// or shows where it would live if you `adopt-clone` it. Handy for scripts.
    Where,
    /// Move an existing clone into confer's managed home (~/.confer/clones/…). Migrates a
    /// hand-placed clone; refuses one with unpushed/uncommitted work unless `--force`.
    AdoptClone {
        /// path to the clone to migrate.
        path: String,
        #[arg(long)]
        force: bool,
    },
    /// Mint a dedicated ed25519 signing key for a role at the fleet-standard location
    /// (`~/.confer/keys/<role>`, comment `<role>@confer`), so a keyless agent gets a
    /// verifiable identity without hand-rolling `ssh-keygen`. REFUSES to overwrite an existing
    /// key — the identity IS the key. Prints the `join --signing-key` line to publish it.
    Keygen {
        /// the role to key (default: this clone's role)
        #[arg(long)]
        role: Option<String>,
    },
    /// Update confer to the latest release. Self-updates ONLY a standalone install (the
    /// `curl … | sh` installer / a GitHub-release binary, which carries a dist install receipt);
    /// a package-manager install (Homebrew / cargo) is never self-replaced — it prints the right
    /// `brew upgrade` / `cargo install --force` command instead. `--check` just reports.
    Update {
        /// only report whether a newer release exists; don't replace anything
        #[arg(long)]
        check: bool,
    },
    /// Print a paste-ready onboarding invite for a cold agent (no comms path yet):
    /// install line + `clone --role` + `install-skill` + hello, filled from live
    /// hub state (origin URL, version pin, role-collision check).
    Invite {
        /// role the newcomer will take (collision-checked; omit to leave a placeholder)
        #[arg(long)]
        role: Option<String>,
        /// host to record for the newcomer (cosmetic hint in the invite)
        #[arg(long)]
        host: Option<String>,
        /// embed SSH URLs (default: emit the credential-agnostic `owner/repo` shorthand)
        #[arg(long)]
        ssh: bool,
        /// embed HTTPS URLs
        #[arg(long, conflicts_with = "ssh")]
        https: bool,
    },
    /// Write the canonical `/confer-watch` + `/confer-poll` skills, adapted to this
    /// machine (resolved confer binary path, hub working copy, role) — so agents
    /// track the canonical skill instead of hand-forking it.
    InstallSkill {
        /// where to write the skills (default: ~/.claude/skills — global, so
        /// /watch is discovered from any project, not just the hub repo).
        /// Pass e.g. `--dir .claude/skills` to scope them to one project.
        #[arg(long)]
        dir: Option<String>,
        /// hub working copy (default: the current repo)
        #[arg(long)]
        hub: Option<String>,
        /// role (default: from the hub's .confer/identity.json)
        #[arg(long)]
        role: Option<String>,
        /// skip installing + enabling the SessionStart auto-heal hook.
        /// By default `install-skill` sets up the full reactive stack.
        #[arg(long = "no-autoheal")]
        no_autoheal: bool,
    },
    /// Bulletproof (re)connect: clone the hub if missing, (re)join your role,
    /// install the skills + auto-heal hook, and tell you to arm the watch. Idempotent
    /// — safe to paste whether an agent is cold or just stale after a compaction.
    Reconnect {
        /// your role id
        #[arg(long)]
        role: Option<String>,
        /// hub to clone if not present (git URL / `owner/repo`), OR a path to an
        /// existing clone. Omit to use the current repo / $CONFER_HUB.
        #[arg(long)]
        hub: Option<String>,
        /// clone into / find the hub here (default: derived from the URL)
        #[arg(long)]
        dir: Option<String>,
        /// host to record for this role (default: autodetected hostname)
        #[arg(long)]
        host: Option<String>,
        /// SSH key that authenticates git transport to a PRIVATE hub (deploy key etc.) — used for
        /// the clone if the hub isn't present yet, and pinned to the clone's `core.sshCommand` so
        /// the headless watch keeps reaching the hub. See `init --ssh-key`.
        #[arg(long = "ssh-key")]
        ssh_key: Option<String>,
        /// re-role a clone that already belongs to a DIFFERENT role (see `join --force`). Off by
        /// default: it keeps the current clone's signing key, linking two role-ids to one key.
        #[arg(long)]
        force: bool,
    },
    /// Bootstrap literacy for a cold agent: three lines on what confer is + the SINGLE next
    /// command for your situation — `init` to START a fleet, `reconnect` to JOIN one. The one
    /// thing to tell a fresh agent is "run `confer onboard`"; from there it's literate and has an
    /// idempotent command to run. Agent-agnostic — needs no skill/plugin, works on any harness.
    Onboard {
        /// the role you'll take (default: a `<your-role>` placeholder in the printed command)
        #[arg(long)]
        role: Option<String>,
        /// a hub to JOIN (git URL / `owner/repo`). Omit to be told how to START a new fleet.
        #[arg(long)]
        hub: Option<String>,
    },
    /// List the repos this hub is "about" (role, access, url, docs) — the
    /// inventory that `--ref` points into. See DESIGN.md.
    Repos {
        #[arg(long)]
        json: bool,
    },
    /// Verify a message's commit signature against the sender role's LOCALLY PINNED
    /// key (TOFU, ~/.confer) — attribution / anti-spoof. A PREDICATE: prints the verdict and
    /// exits 0 if the sender is cryptographically attributed (verified, or an unconfirmed
    /// first-sight pin — see `--strict`), 1 if NOT attributable (unsigned / unknown key / KEY
    /// MISMATCH), 3 if the check couldn't run. So `confer verify <id> && act` is a safe gate.
    /// See DESIGN.md.
    Verify {
        id: String,
        /// stricter gate: also exit 1 for an unconfirmed first-sight pin (only a human-CONFIRMED
        /// key passes). Use for high-stakes attribution decisions.
        #[arg(long)]
        strict: bool,
    },
    /// Confirm a role's first-seen key OUT-OF-BAND: after checking its
    /// fingerprint by a trusted channel, mark the pin confirmed so it verifies as ✓ instead of
    /// the provisional ⚠ first-sight. Shows the pinned fingerprint if you pass no role.
    ConfirmKey {
        /// the role whose pinned key you've verified out-of-band.
        role: Option<String>,
    },
    /// Audit this clone's git identity/signing config so an agent and its human don't
    /// clobber each other's settings (scope conflicts, masquerade, headless signer).
    /// Read-only. See DESIGN.md.
    Doctor {
        /// Repo to audit (default: the current hub/repo).
        dir: Option<String>,
        /// apply the auto-fixable repairs (e.g. turn on signing when a key is set but
        /// commit.gpgsign is off) — LOCAL config only, agent clones only.
        #[arg(long)]
        fix: bool,
    },
    /// Show or set this hub's TRUST TIER — how much to trust its peers.
    /// Local-only; a peer can't set its own. `own` (your fleet) | `shared` (co-owned
    /// with a trusted collaborator) | `foreign` (someone else's hub you joined).
    Trust {
        /// own | shared | foreign — omit to show the current tier.
        tier: Option<String>,
    },
    /// Heuristic injection screen: classify a message body as
    /// allow/screen, or score the heuristic against an adversarial corpus. Screen-level
    /// only — it annotates, never blocks (that needs the model screen).
    Screen {
        /// Score the heuristic against a corpus JSON.
        #[arg(long)]
        corpus: Option<String>,
        /// Classify a single body inline (else read from stdin).
        #[arg(long)]
        text: Option<String>,
    },
    /// Read-receipts: who among a message's audience has consumed it, derived from
    /// each peer's published cursor (presence). "Seen" = the message's commit is an
    /// ancestor of that peer's cursor. See DESIGN.md.
    Seen {
        /// message id (short or full)
        id: String,
    },
    /// Your unread inbox: directly-addressed mail (`--to` you) you haven't CONSUMED
    /// yet — the watch shows only summaries, so a resolution/answer re-surfaces here
    /// until read. Prints the full messages and marks them read (advances your read
    /// frontier). `--peek` to view without marking. This is the "did I actually see
    /// it" backstop, separate from the delivery cursor. See inbox.rs / DESIGN.md.
    Inbox {
        #[arg(long)]
        role: Option<String>,
        /// view without marking as read
        #[arg(long)]
        peek: bool,
    },
    /// Acknowledge mail as read without re-opening it: advances your read frontier to
    /// <id> (or to the latest message if omitted), clearing the unread nag.
    Ack {
        /// mark read up to this message id (default: everything so far)
        id: Option<String>,
        #[arg(long)]
        role: Option<String>,
    },
    /// git credential helper — mints/serves GitHub App installation tokens over
    /// HTTPS so push/fetch auto-authenticate (no SSH agent / 1Password). Wire with:
    /// `git config credential.https://github.com.helper "!confer credential"`.
    Credential {
        /// git passes the operation: get | store | erase
        op: String,
    },
    /// Print a fresh GitHub App installation token (debug / manual `git clone`).
    AppToken,
    /// Set or show the GitHub App config (app id / key path / installation id).
    AppConfig {
        #[arg(long = "app-id")]
        app_id: Option<String>,
        #[arg(long = "key")]
        key: Option<String>,
        #[arg(long = "installation-id")]
        installation_id: Option<u64>,
        /// look up and store the installation id via the API (App must be installed)
        #[arg(long = "find-installation")]
        find_installation: bool,
    },
    /// Print the confer build version and grade drift against the hub's pin
    /// (major/minor/patch/rebuild). `--check` exits non-zero when an update is
    /// available (scriptable — like `watch-status`); `--json` for machine parsing.
    /// Shows the three build layers (running watcher / installed binary / hub pin)
    /// when they diverge.
    Version {
        #[arg(long)]
        json: bool,
        /// exit non-zero if this build is behind the hub pin (an update is available)
        #[arg(long)]
        check: bool,
        /// (maintainer) move the hub pin to THIS build + commit/push — cuts a release
        /// so the fleet detects the update. Bump Cargo.toml's version first for a
        /// graded (major/minor/patch) signal; otherwise it's a same-version rebuild.
        #[arg(long)]
        pin: bool,
    },
    /// Show the release notes baked into THIS binary — what shipped in the build you're running.
    /// After `confer update` + a watch re-arm, run this to see what changed and whether the diff
    /// asks anything of you (a new flag, a setup cleanup). Defaults to the newest entry; `--since
    /// <version>` shows everything newer than a version you were on; `--all` dumps the whole log.
    Changelog {
        /// show every entry strictly newer than this version (e.g. the build you updated FROM)
        #[arg(long, value_name = "VERSION")]
        since: Option<String>,
        /// show the entire changelog, not just the newest entry
        #[arg(long)]
        all: bool,
    },
    /// Fleet version audit: every agent's published build (from presence) vs the hub pin
    /// and the requirement floor — the "are we all up to date / compatible" view.
    Fleet {
        #[arg(long)]
        json: bool,
    },
    /// Show or set the hub's version REQUIREMENT floor (a semver range like `>=0.1.0`) —
    /// the fuzzy repo-level compatibility contract each agent's exact build is audited
    /// against. `--bump` raises it to the lowest live-agent version (auto-bump).
    Require {
        /// a semver requirement, e.g. `>=0.2.0` — omit to show the current floor.
        req: Option<String>,
        /// raise the floor to `>=<lowest live-agent version>` (advances only).
        #[arg(long)]
        bump: bool,
    },
}
