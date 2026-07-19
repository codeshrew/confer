// Pure projections behind Board's cockpit (piece 5b, ui/REDESIGN.md,
// `redesign-mockups/05-board-cockpit.html`) — kept out of Board.svelte so
// the "answer the four questions" math is unit-testable without mounting
// Svelte, same convention as railLayout.ts/thread.ts/ticketState.ts.
//
// Law #3 runs through every function here: every number is folded from
// REAL `RequestRow`/`Message` data already on hand. The one field that
// looked like it might need a backend ask — closure throughput's per-day
// close history — doesn't: `type: 'done'` messages already carry the exact
// closure timestamp (and `type: 'request'` the exact open timestamp), so
// `computeThroughput` buckets those directly rather than inventing a
// synthetic "closed at" field. No fabricated bars, no backend gap filed.
import type { Agent, Message, RequestRow } from './types';
import { formatIsoDate } from './format';
import { ticketStateOf, type TicketState } from './ticketState';

export interface BoardStats {
  /** The mockup's "Open" stat — sub-labeled "active work": every request
   * NOT done/superseded (open + needs-owner + in-flight + stuck combined),
   * not a bucket exclusive of the other three. `inFlight`/`stuck`/
   * `needsOwner` are real SUB-counts of this same total, not siblings of
   * it — clicking "Open" is "show all live work" (equivalent to clearing
   * a state filter), not a fourth disjoint slice. */
  activeWork: number;
  inFlight: number;
  stuck: number;
  needsOwner: number;
  done: number;
}

/** The four questions as numbers (mockup's stat strip) + `done`, folded
 * away behind the disclosure line rather than a fifth stat card. */
export function computeBoardStats(requests: RequestRow[]): BoardStats {
  const stats: BoardStats = { activeWork: 0, inFlight: 0, stuck: 0, needsOwner: 0, done: 0 };
  for (const r of requests) {
    const state = ticketStateOf(r);
    if (state === 'done') {
      stats.done++;
      continue;
    }
    stats.activeWork++;
    if (state === 'flight') stats.inFlight++;
    else if (state === 'stuck') stats.stuck++;
    else if (state === 'unowned') stats.needsOwner++;
    // state === 'open' (a deferred/parked ticket) counts toward
    // activeWork only — it has no dedicated stat card of its own.
  }
  return stats;
}

export interface FlowSegment {
  state: 'open' | 'unowned' | 'flight' | 'stuck';
  count: number;
  pct: number;
}

/** The thin status flow bar under the stat strip — every LIVE (non-done)
 * ticket's state, as a proportion of live work. Empty when there's no live
 * work at all (never divides by zero into a fabricated 100%-of-nothing
 * bar). */
export function computeFlowBar(requests: RequestRow[]): FlowSegment[] {
  const stats = computeBoardStats(requests);
  // `stats.activeWork` is the TOTAL of these four, not a fifth sibling —
  // the "parked" (deferred) count is derived directly here since it has
  // no dedicated field on BoardStats (no stat card of its own either).
  const parked = requests.filter((r) => ticketStateOf(r) === 'open').length;
  const live: { state: FlowSegment['state']; count: number }[] = [
    { state: 'open', count: parked },
    { state: 'unowned', count: stats.needsOwner },
    { state: 'flight', count: stats.inFlight },
    { state: 'stuck', count: stats.stuck },
  ];
  const total = live.reduce((sum, s) => sum + s.count, 0);
  if (total === 0) return [];
  return live.filter((s) => s.count > 0).map((s) => ({ ...s, pct: (s.count / total) * 100 }));
}

export interface LoadRow {
  agentId: string;
  count: number;
  /** Only meaningful for the "asking" load — the portion of that
   * requester's live requests with no claimant yet. */
  unownedCount?: number;
}

/** "Carrying" — who's doing the work. Reuses `Agent.wip` (the real
 * currently-CLAIMED count already server/mock-projected — the same
 * aggregation `attention.ts`'s `deriveDomainAgent` uses) rather than
 * re-deriving a second, possibly-drifting count from `requests`. Sorted
 * desc by count; agents carrying nothing are omitted (nothing to show). */
export function computeCarrying(agents: Agent[]): LoadRow[] {
  return agents
    .map((a) => ({ agentId: a.id, count: a.wip.filter((w) => w.status === 'CLAIMED').length }))
    .filter((r) => r.count > 0)
    .sort((a, b) => b.count - a.count);
}

/** "Asking" — who's waiting on work, i.e. every LIVE (non-done, non-
 * superseded) request grouped by its `from` (the requester), with the
 * unowned (unclaimed) portion of each requester's pile broken out — "a
 * full unowned bar is who to route" (the mockup's own framing). */
export function computeAsking(requests: RequestRow[]): LoadRow[] {
  const byRequester = new Map<string, { count: number; unowned: number }>();
  for (const r of requests) {
    const state = ticketStateOf(r);
    if (state === 'done') continue;
    const entry = byRequester.get(r.from) ?? { count: 0, unowned: 0 };
    entry.count++;
    if (state === 'unowned') entry.unowned++;
    byRequester.set(r.from, entry);
  }
  return [...byRequester.entries()]
    .map(([agentId, { count, unowned }]) => ({ agentId, count, unownedCount: unowned }))
    .sort((a, b) => b.count - a.count);
}

export interface ThroughputDay {
  day: string;
  opened: number;
  closed: number;
}

/** Buckets real `request`/`done` message timestamps into the last `days`
 * calendar days (oldest first, today last) — the closure throughput
 * chart's real data source. `nowMs` is injectable for deterministic tests
 * (mirrors `formatAge`'s own `nowMs` parameter convention). */
export function computeThroughput(messages: Message[], days: number, nowMs: number = Date.now()): ThroughputDay[] {
  const buckets = new Map<string, ThroughputDay>();
  const dayKeys: string[] = [];
  for (let i = days - 1; i >= 0; i--) {
    const key = formatIsoDate(new Date(nowMs - i * 24 * 60 * 60 * 1000).toISOString());
    dayKeys.push(key);
    buckets.set(key, { day: key, opened: 0, closed: 0 });
  }
  const earliest = dayKeys[0];
  for (const m of messages) {
    if (m.type !== 'request' && m.type !== 'done') continue;
    const day = formatIsoDate(m.ts);
    if (!earliest || day < earliest) continue;
    const bucket = buckets.get(day);
    if (!bucket) continue; // outside the window (a future-dated fixture, etc.)
    if (m.type === 'request') bucket.opened++;
    else bucket.closed++;
  }
  return dayKeys.map((k) => buckets.get(k)!);
}

export interface ThroughputSummary {
  closed: number;
  opened: number;
  net: number;
}

export function summarizeThroughput(days: ThroughputDay[]): ThroughputSummary {
  const closed = days.reduce((sum, d) => sum + d.closed, 0);
  const opened = days.reduce((sum, d) => sum + d.opened, 0);
  return { closed, opened, net: closed - opened };
}

export interface VerdictParts {
  needsOwner: string | null;
  stuck: string | null;
  trend: string | null;
}

/** The header's one-line verdict — "answer the four questions before
 * showing a single ticket." Any clause with nothing to report is `null`
 * (the template omits it) rather than a padded "0 stuck". */
export function verdictParts(stats: BoardStats, throughput: ThroughputSummary): VerdictParts {
  return {
    needsOwner: stats.needsOwner > 0 ? `${stats.needsOwner} need${stats.needsOwner === 1 ? 's' : ''} an owner` : null,
    stuck: stats.stuck > 0 ? `${stats.stuck} stuck` : null,
    trend: throughput.net > 0 ? 'closing faster than opening ↗' : throughput.net < 0 ? 'opening faster than closing ↘' : 'holding steady →',
  };
}

/** Piece 5c's combined filter — a request matches when it satisfies BOTH
 * dimensions that are actually set (a `null` dimension always matches,
 * "no opinion"). `agentFilter` reads as "this agent is involved" — either
 * the requester (`from`) or the current claimant — the broadest honest
 * reading of "filter the board to their work" that doesn't force a
 * separate carrying-vs-asking choice onto the filter itself (the
 * workload visuals already show that distinction; the filter just
 * narrows which tickets are in view). */
export function filterRequests(requests: RequestRow[], stateFilter: TicketState | null, agentFilter: string | null): RequestRow[] {
  return requests.filter((r) => {
    if (stateFilter && ticketStateOf(r) !== stateFilter) return false;
    if (agentFilter && r.from !== agentFilter && !r.claimants.includes(agentFilter)) return false;
    return true;
  });
}
