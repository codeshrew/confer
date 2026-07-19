//! The command-line surface: the clap `Parser`/`Subcommand` definitions.
//!
//! This is pure argument *data* — derive macros only, no handler logic (the handlers live in
//! `main.rs` and the command modules). The doc-comments on `Cmd`'s variants and their fields ARE
//! the `--help` text, so edits here change user-facing help verbatim. `LifecycleArgs` (the shared
//! claim/done/error/blocked/defer flag block) still lives in `main.rs`; it's referenced here via
//! `crate::LifecycleArgs` for the `#[command(flatten)]`.

use clap::{Parser, Subcommand, ValueEnum};
use crate::{CreateArgs, LifecycleArgs, VERSION};

/// `confer autoheal <action>` — was a freeform `String` (a typo exited as a runtime error, code 3,
/// with no shell completion); a `ValueEnum` makes clap reject a bad value itself (usage error, code
/// 2) and enables completion. Same accepted values as before, `enable`/`disable` kept as aliases.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutohealAction {
    #[value(alias = "enable")]
    On,
    #[value(alias = "disable")]
    Off,
    Status,
    Prune,
}

/// `confer config <action>` — see `AutohealAction` (design/37 item 9): a typed value so a bad
/// action is a clap usage error, not a runtime one. Same accepted values as before.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConfigAction {
    Show,
    Get,
    Set,
    Validate,
    Schema,
}

/// `confer hub <action>` — see `AutohealAction` (design/37 item 9). `show` kept as an alias of
/// `status` (the handler already treated them as synonyms).
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HubAction {
    #[value(alias = "show")]
    Status,
    Repin,
    Prune,
}

/// `confer repos <action>` — `map` records THIS machine's clone of a repo (design/40 layer 2:
/// `~/.confer/repos.json`, local-only, never in the hub) so `--ref <slug>:…` resolves to real
/// code here. No action = the listing.
#[derive(Subcommand, Clone)]
pub(crate) enum ReposAction {
    /// Point a repo slug at its clone on THIS machine (path defaults to the current dir).
    Map {
        /// the repos/<slug> key (matches `--ref <slug>:…`)
        slug: String,
        /// clone path (default: the current directory)
        path: Option<String>,
    },
    /// Backfill the clone map: match every repo registered in a hub you follow to a git
    /// clone already on this machine (by canonical remote or root-commit SHA) and record
    /// it, without touching any hub card or committing. Idempotent — a slug already
    /// mapped is left alone. A REPORT (exit 0): prints `mapped <slug> → <path>` for each
    /// match found and `unmatched <slug> (<url>)` for a registered repo with no local
    /// clone.
    Discover {
        /// dev root to scan (its IMMEDIATE children only); repeatable. Default: ~/git,
        /// ~/src, ~/code, ~/Projects, ~/dev, ~/repos, $HOME, plus the parent dir of every
        /// hub clone you already follow.
        #[arg(long = "root")]
        root: Vec<String>,
    },
}

#[derive(Parser)]
#[command(
    name = "confer",
    version = VERSION,
    about = "git-native coordination blackboard for AI agents"
)]
pub(crate) struct Cli {
    /// Operate on a specific hub — like `git -C`, before the subcommand: `confer --hub jarvis topics`,
    /// `confer --hub agent-coord who`. Works with EVERY hub-scoped command. Give a hub NAME (matched
    /// against `confer hubs` — a case-insensitive substring of the hub's name) or a clone PATH. Sets
    /// the effective hub for this one invocation (overrides $CONFER_HUB / the current directory).
    #[arg(long)]
    pub(crate) hub: Option<String>,
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
    Who {
        /// machine output: a JSON ARRAY of agent objects (one per role), mirroring the text
        /// columns — `role`/`display`/`desc`/`host`/`expected_host`/`live`/`liveness`/
        /// `last_posted`/`aliases`/`trust`/`status`/`xhub`. `[]` if no roles yet.
        #[arg(long)]
        json: bool,
    },
    /// Append one message (Markdown body via --text, --body-file, or stdin).
    Append {
        /// message type: note | request | claim | done | error | supersede
        #[arg(long = "type")]
        msg_type: String,
        /// REQUIRED one-line summary — the triage field peers read before opening the body.
        /// Exactly one of --summary / --summary-file.
        #[arg(long)]
        summary: Option<String>,
        /// read the summary verbatim from a file (raw bytes, no shell expansion) — the
        /// shell-free affordance for a summary containing backticks/$()/$VAR/quotes/etc.
        /// Mutually exclusive with --summary. A single trailing newline is stripped (a
        /// summary is one line).
        #[arg(long = "summary-file")]
        summary_file: Option<String>,
        /// message body; if omitted, read from stdin (supports multi-line/fenced)
        #[arg(long)]
        text: Option<String>,
        /// read the message body verbatim from a file (raw bytes, no shell expansion,
        /// no interpretation) — the shell-free affordance for a body containing
        /// backticks/$()/$VAR/!/quotes/fenced code/etc. Mutually exclusive with --text
        /// and bypasses the stdin fallback (never combine with piped stdin).
        #[arg(long = "body-file")]
        body_file: Option<String>,
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
        /// capture EVERY `--ref`'s identity from this directory's worktree instead of the
        /// mapped clone (message-wide) — the escape hatch when the agent's cwd isn't the
        /// tree it means; only applies to refs whose repo identity matches this dir.
        #[arg(long = "ref-from")]
        ref_from: Option<String>,
        /// allow a `--ref` whose content is uncommitted/untracked at capture: embeds the
        /// working-tree lines into the message body (a `confer-ref` fence) instead of
        /// refusing to pin. Default is to FAIL — a ref is a promise peers can retrieve
        /// what's pinned; use this only when that promise can't hold yet.
        #[arg(long = "allow-dirty")]
        allow_dirty: bool,
        /// attach a prepared unified diff (file path, or `-` for stdin) as a `confer-patch`
        /// (design/45): pinned to a base sha, write-time apply-gated via a temp index, one
        /// derived `patch: true` ref per touched file with its `result_hash`. Requires --repo.
        #[arg(long)]
        patch: Option<String>,
        /// the `repos/<slug>` --patch is against — resolves its base sha's capture dir the same
        /// way --ref-from does (--ref-from itself, if given, is reused as that capture dir).
        #[arg(long = "repo")]
        patch_repo: Option<String>,
        /// raise --patch's size gate from refuse-above-400-changed-lines to a hard ~2000-line
        /// cap. Above THAT, the change is branch-scale: push a branch and --ref the commit.
        #[arg(long = "allow-large-patch")]
        allow_large_patch: bool,
    },
    /// Open a new tracked request (a ticket). Sugar for `append --type request`.
    /// note = chat, request = ticket: use this when the thing needs someone to DO
    /// something and show up on `requests`; use `note` for a plain message.
    Request {
        #[command(flatten)]
        args: CreateArgs,
        /// promote a prior `note` (or any message) into a tracked request that
        /// references it — the "escalation" idiom: `note` first, `request
        /// --reply-to <note-id>` once it needs to be tracked.
        #[arg(long = "reply-to")]
        reply_to: Option<String>,
    },
    /// Post a plain message (chat, no lifecycle tracking). Sugar for `append --type note`.
    /// note = chat, request = ticket: use `request` instead when someone needs to DO
    /// something about it.
    Note {
        #[command(flatten)]
        args: CreateArgs,
    },
    /// Claim a request (you're taking it). Sugar for `append --type claim --of`.
    /// Takes the request id positionally (`confer claim <id>`) or via `--of`.
    Claim {
        #[command(flatten)]
        args: LifecycleArgs,
    },
    /// Mark a request done. `--as wont-do|duplicate|obsolete` closes it *without*
    /// completion (a conscious drop). Sugar for `append --type done --of`.
    /// Takes the request id positionally (`confer done <id>`) or via `--of`.
    Done {
        #[command(flatten)]
        args: LifecycleArgs,
        #[arg(long = "as")]
        resolution: Option<String>,
    },
    /// Mark a request failed. Sugar for `append --type error --of`.
    /// Takes the request id positionally (`confer error <id>`) or via `--of`.
    Error {
        #[command(flatten)]
        args: LifecycleArgs,
    },
    /// Mark a request blocked/waiting (off the active board → `requests --blocked`);
    /// re-`claim` when unblocked. Sugar for `append --type blocked --of`.
    /// Takes the request id positionally (`confer blocked <id>`) or via `--of`.
    Blocked {
        #[command(flatten)]
        args: LifecycleArgs,
    },
    /// Backlog a request (someday) — anyone can, incl. the addressee. Sugar for
    /// `append --type defer --of`.
    /// Takes the request id positionally (`confer defer <id>`) or via `--of`.
    Defer {
        #[command(flatten)]
        args: LifecycleArgs,
    },
    /// Propose a change: sugar for `append --type request --patch …` (design/45 §1.3) — the
    /// reviewer's/refactorer's concrete output, applicable by the author with one `confer apply`
    /// command, traceable forever via the request lifecycle (claim/done/wont-do/supersede). An
    /// alternative with no expectation of action is the Talk-side `note --patch` instead. `--to`
    /// is required, exactly as for any request. Requires `--patch <file|->` (the `--worktree`
    /// capture flow is design/45's M-phase, not yet implemented).
    Suggest {
        #[command(flatten)]
        args: CreateArgs,
    },
    /// Apply a `confer-patch` from a message to its target repo's WORKING TREE (design/45 §1.5).
    /// EXPLICIT and never automatic: confer stops at `git apply`/`git apply --3way`, leaving
    /// review, staging, committing, and attribution to YOU in your own repo — it never commits
    /// or pushes to a work repo. `--check` is the predicate form (design/37): exit 0 applies
    /// cleanly, 1 conflicts/drift, 2 already landed (HEAD already has this change), 3
    /// unresolvable here (no mapped clone, or the base commit isn't present locally).
    Apply {
        /// the message id (or id-prefix) carrying the `confer-patch` fence.
        id: String,
        /// predicate mode: report the verdict via exit code only (0/1/2/3); never touches the
        /// working tree.
        #[arg(long)]
        check: bool,
        /// resolve the target repo here instead of the mapped clone (the `--ref-from` analogue —
        /// design/44 §1.1).
        #[arg(long = "repo-dir")]
        repo_dir: Option<String>,
        /// apply even though a touched path has uncommitted changes (default: refuse — never
        /// stack a proposal onto unsaved work).
        #[arg(long)]
        force: bool,
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
    /// A REPORT: exits 0 once the message is printed.
    Show {
        id: String,
        /// machine output: the message as one `to_json` object (carries `trust`/`tier`/
        /// `screen`), no supersession/edit-notice decoration. Still marks the message read.
        #[arg(long)]
        json: bool,
    },
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
    /// (claims/dones/errors/replies/supersedes), transitively. A REPORT: exits 0
    /// once the thread is printed (even a single-message thread).
    Thread {
        id: String,
        #[arg(long)]
        full: bool,
        /// machine output: one `to_json` object per line (NDJSON), oldest first.
        #[arg(long)]
        json: bool,
    },
    /// List the hub's TOPICS (the folders under `threads/`) with activity + open-work state —
    /// the board at a glance, newest-active first, and a way to find stale open topics to clean up.
    /// `thread <id>` (singular) drills into one request's lifecycle; this lists them all. A REPORT
    /// (exits 0). Combine `--stale` with lifecycle `done --as obsolete` to close what's gone dead.
    #[command(alias = "threads")]
    Topics {
        /// only topics with unresolved work (an open/claimed/blocked request)
        #[arg(long)]
        open: bool,
        /// only topics with no open work (all requests resolved, or discussion-only)
        #[arg(long)]
        closed: bool,
        /// only OPEN topics gone quiet longer than N days (default 14) — cleanup candidates
        #[arg(long, num_args = 0..=1, default_missing_value = "14")]
        stale: Option<u64>,
        /// machine output: a JSON array of topic objects — topic, messages, participants,
        /// last_activity, age_secs, requests, open_requests, status ("open"|"closed"), stale.
        #[arg(long)]
        json: bool,
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
    /// Arm (or re-arm) your watcher the ONE correct way, then stream wakes: self-locate your
    /// role's clone, take over any orphan (`--replace`), and stamp `--delivery monitor` so
    /// `watch-status` can confirm delivery — none of which you can forget. Host it under the
    /// Monitor tool (the `/confer-arm` skill); it's a long-lived streamer like `watch`. The
    /// paved path over `watch --replace --delivery monitor` (design/49).
    Arm {
        /// arm this role explicitly. Default: resolve from the current clone, or the single
        /// watch target this session owns (disambiguate a multi-role machine with this).
        #[arg(long)]
        role: Option<String>,
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
    /// quietly and self-heals; this is how you check on demand if you care. A REPORT:
    /// exits 0 once the report is produced, however bad the news.
    Status {
        /// machine output: one JSON object with the same fields the text report shows.
        #[arg(long)]
        json: bool,
    },
    /// Live read-only TUI: agents (liveness + cross-hub identity), the task board +
    /// flow, health, and a live activity tail — folded from local hub clones, no
    /// server. Current hub by default; `--all-hubs` for the whole fleet, or the
    /// top-level `--hub <name>` to focus one. Keys: q quit · r refresh · c toggle closed · Tab switch.
    #[cfg(feature = "dashboard")]
    Dashboard {
        /// Show EVERY hub on this machine (the full fleet view). Without it, shows just the current
        /// hub — narrow to a specific one with the top-level selector: `confer --hub jarvis dashboard`.
        #[arg(long = "all-hubs")]
        all_hubs: bool,
    },
    /// Read-only web view of the fleet (same data as `dashboard`) — a pure-Rust
    /// server rendering the board/agents/health/activity as HTML, auto-refreshing.
    /// Binds to LOCALHOST (127.0.0.1, this machine only) by default — the board is
    /// unauthenticated, so nothing else on your network can reach it unless you ask.
    /// Pass `--lan` to deliberately expose it on your network (e.g. for phone access).
    /// Current hub by default; `--all-hubs` for the whole fleet, or top-level
    /// `--hub <name>` to focus one. Read-only: never posts, never locks.
    #[cfg(feature = "serve")]
    Serve {
        /// Serve EVERY hub on this machine (the full fleet view). Without it, serves just the current
        /// hub — narrow to a specific one with the top-level selector: `confer --hub jarvis serve`.
        #[arg(long = "all-hubs")]
        all_hubs: bool,
        /// Expose the server to your WHOLE NETWORK (binds 0.0.0.0), for phone/LAN access.
        /// UNAUTHENTICATED: anyone who can reach this machine on the network can then read
        /// all hub content and code. Ignored if `--bind` is also given (explicit `--bind`
        /// always wins). Without this flag, serve binds loopback (127.0.0.1) only.
        #[arg(long)]
        lan: bool,
        /// Localhost port to serve on — the easy override (shorthand for `--bind 127.0.0.1:PORT`,
        /// or `--bind 0.0.0.0:PORT` if combined with `--lan`). Also settable via the
        /// `CONFER_SERVE_PORT` env var. Default 8422 (8787 collides with RStudio Server and
        /// some studio apps).
        #[arg(long)]
        port: Option<u16>,
        /// Full bind address override — for power users. ALWAYS wins over `--lan`/`--port` when
        /// given. Loopback (127.0.0.1 / ::1 / localhost) is treated as private; anything else
        /// (e.g. `0.0.0.0:8422`) is UNAUTHENTICATED network exposure — confer will warn loudly
        /// at startup, but won't refuse it.
        #[arg(long)]
        bind: Option<String>,
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
        #[arg(value_enum)]
        action: AutohealAction,
        /// with `prune`: actually remove stale targets (default is a dry-run listing).
        #[arg(long)]
        yes: bool,
    },
    /// Inspect or set this machine's policy config (`~/.confer/config.json`: clone location,
    /// per-hub transport/auth, tuning, update posture — design/35). NOT the shared repo contract
    /// and NOT trust state. Confer-managed; don't hand-edit the JSON.
    Config {
        /// show | get | set | validate | schema  (omit → show)
        #[arg(value_enum, default_value = "show")]
        action: ConfigAction,
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
        #[arg(value_enum, default_value = "status")]
        action: HubAction,
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
    /// inventory that `--ref` points into — plus whether each is cloned on THIS
    /// machine. `repos map <slug> [path]` records where your clone lives (local-only,
    /// never in the hub) so `--ref <slug>:…` resolves to real code here. See DESIGN.md.
    Repos {
        #[command(subcommand)]
        action: Option<ReposAction>,
        #[arg(long)]
        json: bool,
    },
    /// Reverse index — "given this code, what conversations reference it?" Folds every
    /// `--ref` in the log by (repo, path) and lists the threads that touched it. A
    /// REPORT (exits 0). `--check` makes it a PREDICATE: exit 1 if nothing references the
    /// target — a scriptable "is there conversation behind this code?" gate. See design/40.
    Refs {
        /// `<repo>` | `<repo>:<path>` | `<repo>:<path>#L44-49` (or `#L46`) — what to look up.
        target: String,
        /// predicate mode: exit 1 (not 0) if nothing references the target.
        #[arg(long)]
        check: bool,
        /// fold every hub you follow (~/.confer/hubs.json), not just the current one.
        #[arg(long = "all-hubs")]
        all_hubs: bool,
        /// machine output: NDJSON, one `ref-hit` event per line (versioned, additive).
        #[arg(long)]
        json: bool,
    },
    /// Plumbing predicate (design/44 Addendum 1): is `<sha>` reachable from `<ref>` —
    /// `git merge-base --is-ancestor <sha> <ref>`? Exit 0 if yes, 1 if no. A more robust
    /// liveness check than "is it still HEAD" (HEAD moves constantly; ancestry doesn't
    /// go stale on every further commit). Runs in `--repo <slug>`'s mapped clone if
    /// given, else the git working tree at the current directory. No fetch.
    RefContains {
        /// the commit to test (full or short hex; anything `git rev-parse` accepts).
        sha: String,
        /// the ref to test against (branch, tag, sha…) — default `HEAD`.
        #[arg(value_name = "ref", default_value = "HEAD")]
        against: String,
        /// resolve the repo via the machine-local clone map instead of the cwd's
        /// working tree (for a script that isn't standing inside the code repo).
        #[arg(long)]
        repo: Option<String>,
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
    /// Read-only. A REPORT by default: exits 0 once the audit is printed, however bad the
    /// news; add `--check` for a scriptable gate. See DESIGN.md.
    Doctor {
        /// Repo to audit (default: the current hub/repo).
        dir: Option<String>,
        /// apply the auto-fixable repairs (e.g. turn on signing when a key is set but
        /// commit.gpgsign is off) — LOCAL config only, agent clones only.
        #[arg(long)]
        fix: bool,
        /// machine output: `{"findings":[{"severity","title","fix"}],"ok":bool}` — the git
        /// identity/signing `audit` findings PLUS the config/security/health advisories
        /// (severity: ok | warn | info): transport self-containment, clone shape, machine config,
        /// hub identity, and the peer role↔key check. SCOPE: only the per-session watch-liveness
        /// check (is a watcher running on THIS machine right now) is report-only and excluded —
        /// everything else that `doctor` prints is in this array.
        #[arg(long)]
        json: bool,
        /// scriptable gate on the full finding set: exit 1 if any finding — audit OR advisory,
        /// INCLUDING the role↔key impersonation check — is `warn`-severity, 0 if clean, 3 on
        /// error. NOTE: the per-session watch-liveness check is deliberately excluded (a CI gate
        /// must not fail just because no watcher happens to be running right now). Without
        /// `--check`, `doctor` always exits 0 (it's a report).
        #[arg(long)]
        check: bool,
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
        /// machine output: one object `{"event":"seen","id","seen_by","pending_by",
        /// "no_heartbeat_by"}` (role ids, not display names).
        #[arg(long)]
        json: bool,
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
        /// machine output: one `to_json` object per unread message (NDJSON); empty inbox
        /// emits nothing. A REPORT: exits 0 whether or not there's unread mail.
        #[arg(long)]
        json: bool,
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
