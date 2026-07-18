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

export interface Hub {
  id: string;
  label: string;
  name: string;
  current: boolean;
  agentCount: number;
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

export interface Agent {
  id: string;
  display: string;
  desc: string | null;
  expectedHost: string | null;
  lastTs: string | null;
  lastHost: string | null;
  live: boolean;
  verified: 'signed' | 'first-sight' | 'unverified';
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
