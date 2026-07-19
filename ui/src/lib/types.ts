// The API contract — mirrors the Rust projections (Board::fold, RefIndex::fold,
// agents, Snapshot) that confer serve's backend will serialize as JSON.

// design/44 §3 + Addenda 1/2 — extends the original current/changed/moved/
// unpinned/unknown vocabulary with three ancestry/squash-aware values:
// `reachable` (still an ancestor of HEAD, just not the tip — a robust
// alternative to the fragile "not HEAD ⇒ stale" read), `offline` (not
// reachable — rebased away/abandoned/GC'd), and `squashed` (offline, but the
// captured `baseRef`/`forkPoint` shows it was merged/squashed away rather
// than simply lost).
export type Staleness = 'current' | 'changed' | 'moved' | 'reachable' | 'offline' | 'squashed' | 'unpinned' | 'unknown';

/** `ref_type` classification (design/44 §1.2) — null for a legacy/unresolved ref. */
export type RefType = 'branch' | 'tag' | 'detached';
export type RequestStatus = 'OPEN' | 'CLAIMED' | 'BLOCKED' | 'DONE' | 'ERROR' | 'SUPERSEDED';
export type MsgType = 'note' | 'request' | 'claim' | 'done' | 'error' | 'blocked' | 'defer' | 'supersede';

// design/48 §2-3 — the server's OWN trust classification for a hub
// (`confer trust own|shared|foreign`, local-only `~/.confer/tiers.json`,
// never client/peer-derived) and its git-sync freshness, both projected onto
// `/api/hubs` + `/api/overview` (src/api.rs's `hub_json`). `null` means the
// server genuinely doesn't know — `tier: null` is "never classified" (NOT
// the same as "own"/trusted), and `sync: null` (or any field inside it) is
// "unknown" (NOT a calm all-clear). A frontend consumer must render both
// honestly rather than defaulting a null into a reassuring value (redesign
// law #3 — see ui/REDESIGN.md).
export type HubTier = 'own' | 'shared' | 'foreign';

export interface HubSync {
  /** Age (seconds) since the hub's clone last successfully fetched — null =
   * unknown (no FETCH_HEAD mtime on record yet). */
  lastFetchedSecs: number | null;
  /** Commits behind the tracked upstream — null = no upstream tracking. */
  behind: number | null;
  /** Local unpushed/uncommitted changes — null = unknown (not yet probed). */
  pending: number | null;
  /** Whether the last background sweep could reach the hub — null = not
   * probed yet (pre-first-sweep). */
  reachable: boolean | null;
}

export interface Hub {
  id: string;
  label: string;
  name: string;
  current: boolean;
  agentCount: number;
  tier?: HubTier | null;
  sync?: HubSync | null;
}

export interface Topic {
  slug: string;
  messages: number;
  open: number;
  requests: number;
  status: 'open' | 'closed' | 'discussion';
  stale: boolean;
  lastTs: string | null;
}

// design/44 §3 — the temporal-identity fields every pinned ref/code-ref now
// carries: `refName`/`refType` (branch/tag label), `commitDate` (ISO8601,
// placement without a clone), `dirty`/`untracked` (the write-time integrity
// gate's flags — a working-tree snapshot, not a clean commit), and the
// Addendum-2 fork-point pair (`baseRef`/`forkPoint`) that makes a squashed-
// away branch's history recoverable. All best-effort/nullable — a legacy
// pre-44 ref (or one where a git call was undeterminable) omits them.
export interface RefTemporalFields {
  refName: string | null;
  refType: RefType | null;
  commitDate: string | null;
  dirty: boolean;
  untracked: boolean;
  baseRef: string | null;
  forkPoint: string | null;
}

export interface CodeRef extends RefTemporalFields {
  repo: string;
  path: string;
  sha: string;
  range: [number, number] | null;
  contentHash: string | null;
}

/** One real, cryptographically-confirmed read-receipt (src/seen.rs) — a
 * role appears here ONLY once their signed presence cursor has actually
 * consumed past this message. Honest by omission: a role who hasn't
 * confirmed (or can't be confirmed — unsigned beat, unresolvable cursor)
 * is simply ABSENT from the array, never present with a null/guessed `ts`. */
export interface SeenBy {
  role: string;
  ts: string;
}

export interface Message {
  id: string;
  from: string;
  type: MsgType;
  ts: string;
  host: string | null;
  to: string[];
  cc: string[];
  topic: string | null;
  summary: string;
  body: string;
  of: string | null;
  replyTo: string | null;
  supersedes: string | null;
  refs: CodeRef[];
  seenBy: SeenBy[];
}

export interface RequestRow {
  id: string;
  from: string;
  to: string[];
  summary: string;
  status: RequestStatus;
  resolution: string | null;
  deferred: boolean;
  claimants: string[];
  ageSecs: number;
  stale: boolean;
  topic: string | null;
}

// design/47 §5 Phase 2 — the health/trust signals `confer fleet`/`doctor`
// already compute but the web API doesn't serve yet. Optional on `Agent`
// (and nullable where the backend may know "no beat yet") so the frontend
// compiles and degrades gracefully whether or not a given `confer serve` has
// landed them — see attention.ts's `deriveLiveness`/`deriveTrust`, which
// fall back to `live`/`verified` when these are absent.
export type Liveness = 'live' | 'stale' | 'down';
/** Supersedes `verified` when present: adds `mismatch` (a card key that
 * doesn't match the pinned key — possible spoof) and `unsigned` (posting
 * without a signature at all), both folded away in the older enum. */
export type Trust = 'signed' | 'mismatch' | 'first-sight' | 'unsigned';

export interface Agent {
  id: string;
  display: string;
  desc: string | null;
  expectedHost: string | null;
  lastTs: string | null;
  lastHost: string | null;
  live: boolean;
  verified: 'signed' | 'first-sight' | 'unverified';
  /** Three-state liveness + heartbeat age (design/47) — absent until the
   * backend lands it. */
  liveness?: Liveness;
  hbAgeSecs?: number | null;
  trust?: Trust;
  // Piece 8b's Fleet dossier fields (Herald, src/api.rs's `agent_row_json`,
  // commit 32ef9a4) — same honest-nullable pattern as tier/sync/seenBy:
  // always-present keys, `null` when the underlying signal genuinely isn't
  // known, never a guessed value.
  /** The confer build the agent's own TRUST-GATED heartbeat carries, the
   * fleet pin-form `"0.6.9 (45a9c04)"` — `null` when no trusted beat
   * publishes a build (an untrusted/forged/replayed beat must not assert a
   * believed version either). */
  version: string | null;
  /** `liveness` re-mapped to the narrower "is a watcher actually armed"
   * vocabulary: `'armed'` (fresh beat) / `'idle'` (stale — seen recently but
   * past cadence) / `null` (down, or no trusted beat at all — no basis to
   * claim either state). A pure fold of `liveness`, not a new signal. */
  watchState: 'armed' | 'idle' | null;
  /** The pinned signing key's SHA256 fingerprint from the verified role
   * card (`"SHA256:…"`) — `null` for unverified/mismatch/no card. */
  keyFingerprint: string | null;
  color: string;
  abbr: string;
  wip: { id: string; summary: string; status: RequestStatus }[];
}

export interface RefHit extends RefTemporalFields {
  repo: string;
  path: string;
  sha: string;
  range: [number, number] | null;
  contentHash: string | null;
  staleness: Staleness;
  msgId: string;
  from: string;
  msgType: MsgType;
  ts: string;
  topic: string | null;
  summary: string;
  threadRoot: string;
  requestStatus: RequestStatus | null;
  /** Which hub this hit was found in (getRefs can span hubs via allHubs). */
  hub: string;
  /** Whether that hub is a private (non-anonymous-read) hub — surfaced as a badge in the reverse index. */
  hubPrivate: boolean;
}

export interface ThreadNode {
  msgId: string;
  from: string;
  type: MsgType;
  topic: string | null;
  summary: string;
  refs: CodeRef[];
}

export interface Snippet {
  lines: { n: number; text: string }[];
  staleness: Staleness;
  lang: string;
}

export interface Overview {
  hub: Hub;
  topics: Topic[];
  board: {
    requests: RequestRow[];
    open: number;
    claimed: number;
    blocked: number;
    backlog: number;
    closed: number;
  };
  fleet: Agent[];
}

export interface Repo {
  slug: string;
  role: string;
  url: string | null;
  access: string[];
  docs: string | null;
  owner: string | null;
  cloned: boolean;
  clonePath: string | null;
  rootSha: string | null;
}

/** A distinct code file referenced (via `--ref`) by messages in a hub — the
 * Code view's file tree hydrates from these instead of a hardcoded fixture.
 * `mapped` reuses `/api/code`'s own clone resolution, so the file tree can
 * show the mapped/unmapped dot without a failed `getCode` round-trip. */
export interface CodeFile {
  repo: string;
  path: string;
  refCount: number;
  mapped: boolean;
  lastTs: string;
}

export type ServerEvent =
  | { event: 'message'; hub: string; topic: string | null }
  | { event: 'presence'; hub: string }
  | { event: 'ping' };
