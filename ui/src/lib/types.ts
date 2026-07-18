// The API contract — mirrors the Rust projections (Board::fold, RefIndex::fold,
// agents, Snapshot) that confer serve's backend will serialize as JSON.

export type Staleness = 'current' | 'changed' | 'moved' | 'unpinned' | 'unknown';
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

export interface CodeRef {
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

export interface RefHit {
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

export type ServerEvent =
  | { event: 'message'; hub: string; topic: string | null }
  | { event: 'presence'; hub: string }
  | { event: 'ping' };
