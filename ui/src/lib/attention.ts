// design/47 — the Overview/Health view's aggregation logic, kept as pure
// functions (no fetch, no DOM) so the "who do I talk to" mapping and the
// severity ranking are unit-testable without mounting Overview.svelte.
//
// Phase 1 (this file): client-side fan-out over the EXISTING /api/overview
// per hub — no new backend endpoint (design/47 §5 Phase 1). `api.ts`'s
// `getAttention()` does the fetch (getHubs + getOverview per hub) and hands
// the result to `aggregateAttention` below. Phase 2 repoints that same
// caller at a real `/api/attention` endpoint; this module's shape is written
// to make that swap a no-op for Overview.svelte.
import type { Agent, Hub, Liveness, Overview, RequestRow, Trust } from './types';

export type Severity = 'critical' | 'attention' | 'info' | 'nominal';

const SEVERITY_WEIGHT: Record<Severity, number> = {
  critical: 0,
  attention: 1,
  info: 2,
  nominal: 3,
};

export type AttentionKind = 'mismatch' | 'unsigned' | 'first-sight' | 'stale-claimed' | 'stale-open' | 'unowned' | 'blocked';

/** One row in Lane 1 (Needs-you) or Lane 2 (Coordination). Every item names
 * an explicit target + verb — design/47 §2.3, the load-bearing "who do I
 * talk to" rule — even when the target is "the fleet" (nobody owns it yet). */
export interface AttentionItem {
  id: string;
  kind: AttentionKind;
  severity: Severity;
  hub: string;
  topic: string | null;
  /** Set on Lane 2 (board-row-derived) items — lets the card drill into the
   * request's thread. Null on Lane 1 (agent-derived) items. */
  reqId: string | null;
  summary: string;
  detail: string;
  /** The bare agent id/slug to copy-and-paste-address — null when nobody
   * single-handedly owns the item ("prompt an agent to claim, or assign
   * one" has no single target). */
  target: string | null;
  /** The full "who + what" instruction, verbatim, ready to act on. */
  verb: string;
  ageSecs: number | null;
}

/** Lane 3 — one card per agent, DEDUPED by signing identity (`Agent.id`)
 * across every hub it appears on (design/47 §2.2 Lane 3). */
export interface FleetCard {
  id: string;
  display: string;
  color: string;
  abbr: string;
  hubs: string[];
  liveness: Liveness;
  hbAgeSecs: number | null;
  trust: Trust;
  /** Total CLAIMED-request count across every hub this identity appears on. */
  wip: number;
  host: string | null;
  lastTs: string | null;
  severity: Severity;
  /** The one-line "→ tell <agent> to <fix>" hint (§2.2) — null when nothing
   * is off. */
  fixVerb: string | null;
}

export interface HubRollup {
  hub: string;
  label: string;
  /** Live-agent count on this hub (post-dedupe would double-count across
   * hubs, so this is intentionally per-hub-occurrence, matching the masthead
   * mini-rollup's own "agent-coord ●3" reading). */
  live: number;
  /** Needs-you + coordination item count scoped to this hub. */
  attention: number;
}

export interface AmbientMetrics {
  openRequests: number;
  perHub: HubRollup[];
}

export interface Attention {
  needsYou: AttentionItem[];
  coordination: AttentionItem[];
  fleet: FleetCard[];
  metrics: AmbientMetrics;
}

/** Three-state liveness — real signal when the backend provides it (design/47
 * Phase 2), else derived from the lossy `live` bool (Phase 1 fallback, per
 * the contract note in design/47's charge: "missing liveness as `live ?
 * 'live' : 'down'`"). */
export function deriveLiveness(agent: Agent): Liveness {
  return agent.liveness ?? (agent.live ? 'live' : 'down');
}

/** Trust state — real signal when present, else derived from the older
 * `verified` enum ("missing trust as derived from verified"). `unverified`
 * maps to `unsigned` — the closest existing meaning in the new vocabulary. */
export function deriveTrust(agent: Agent): Trust {
  if (agent.trust) return agent.trust;
  switch (agent.verified) {
    case 'signed':
      return 'signed';
    case 'first-sight':
      return 'first-sight';
    default:
      return 'unsigned';
  }
}

function itemId(...parts: (string | null | undefined)[]): string {
  return parts.filter((p) => p !== null && p !== undefined).join(':');
}

// A request left OPEN with no claimant is only worth surfacing once it's had
// a little time to sit — a request filed a minute ago isn't "unowned," it's
// just new. (The 3-day `stale` mark already covers the long-unclaimed case;
// this threshold is for the shorter "nobody's picked this up yet" nudge.)
const UNOWNED_THRESHOLD_SECS = 60 * 60; // 1h

/** Lane 1 — the integrity/trust items a single agent's card can raise
 * (design/47 §2.2 Lane 1, minus the hub-visibility alarm, which needs a
 * `doctor` probe this Phase-1 client-side fan-out has no access to). */
function needsYouForAgent(agent: Agent, hub: Hub): AttentionItem[] {
  const trust = deriveTrust(agent);
  const base = { hub: hub.id, topic: null, reqId: null, ageSecs: null } as const;
  switch (trust) {
    case 'mismatch':
      return [
        {
          ...base,
          id: itemId('mismatch', hub.id, agent.id),
          kind: 'mismatch',
          severity: 'critical',
          summary: `KEY MISMATCH — ${agent.display} @ ${hub.label}`,
          detail: 'card key ≠ pinned key (possible spoof)',
          target: agent.id,
          verb: `verify ${agent.display} locally before trusting its posts`,
        },
      ];
    case 'unsigned':
      return [
        {
          ...base,
          id: itemId('unsigned', hub.id, agent.id),
          kind: 'unsigned',
          severity: 'critical',
          summary: `UNSIGNED POSTER — ${agent.display} @ ${hub.label}`,
          detail: 'this hub expects signing, but this agent is posting unsigned',
          target: agent.id,
          verb: `verify ${agent.display} — posting unsigned`,
        },
      ];
    case 'first-sight':
      return [
        {
          ...base,
          id: itemId('first-sight', hub.id, agent.id),
          kind: 'first-sight',
          severity: 'info',
          summary: `FIRST-SIGHT KEY — ${agent.display} @ ${hub.label}`,
          detail: 'newly-pinned key, pending confirmation',
          target: agent.id,
          verb: `confirm ${agent.display}'s key if you expected it to (re)join`,
        },
      ];
    default:
      return [];
  }
}

/** Lane 2 — the request-lifecycle attention set for one board row (design/47
 * §2.2 Lane 2): stale (sub-typed claimed vs. unclaimed), unowned-but-fresh,
 * and blocked. DONE/ERROR/SUPERSEDED never raise an item; deferred (backlog)
 * rows are skipped too — a human already triaged those out of the urgent
 * set on purpose. */
function coordinationForRow(row: RequestRow, hub: Hub): AttentionItem[] {
  const base = { hub: hub.id, topic: row.topic, reqId: row.id, ageSecs: row.ageSecs } as const;
  const where = `${hub.label}${row.topic ? `/#${row.topic}` : ''}`;

  if (row.status === 'BLOCKED') {
    const claimant = row.claimants[0] ?? row.from;
    return [
      {
        ...base,
        id: itemId('blocked', hub.id, row.id),
        kind: 'blocked',
        severity: 'attention',
        summary: `BLOCKED · "${row.summary}" · ${where}`,
        detail: row.resolution ? `blocked — waiting on: ${row.resolution}` : 'blocked, no stated reason yet',
        target: claimant,
        verb: row.resolution ? `unblock ${claimant} — waiting on: ${row.resolution}` : `unblock ${claimant}`,
      },
    ];
  }

  if (row.status === 'DONE' || row.status === 'ERROR' || row.status === 'SUPERSEDED') return [];
  if (row.deferred) return [];

  if (row.stale) {
    if (row.claimants.length > 0) {
      const claimant = row.claimants[0]!;
      return [
        {
          ...base,
          id: itemId('stale-claimed', hub.id, row.id),
          kind: 'stale-claimed',
          severity: 'attention',
          summary: `STALE · "${row.summary}" · ${where}`,
          detail: `claimed by ${claimant}, no movement`,
          target: claimant,
          verb: `nudge ${claimant} — claimed, no movement`,
        },
      ];
    }
    const addressees = row.to.filter((t) => t !== 'all');
    if (addressees.length > 0) {
      const target = addressees.join(', ');
      return [
        {
          ...base,
          id: itemId('stale-open', hub.id, row.id),
          kind: 'stale-open',
          severity: 'attention',
          summary: `STALE · "${row.summary}" · ${where}`,
          detail: `open, addressed to ${target}, unanswered`,
          target,
          verb: `nudge ${target} — unanswered`,
        },
      ];
    }
    return [
      {
        ...base,
        id: itemId('unowned', hub.id, row.id),
        kind: 'unowned',
        severity: 'attention',
        summary: `UNOWNED · "${row.summary}" · ${where}`,
        detail: 'no claimant, stale',
        target: null,
        verb: 'prompt an agent to claim, or assign one',
      },
    ];
  }

  if (row.status === 'OPEN' && row.claimants.length === 0 && row.ageSecs >= UNOWNED_THRESHOLD_SECS) {
    return [
      {
        ...base,
        id: itemId('unowned', hub.id, row.id),
        kind: 'unowned',
        severity: 'info',
        summary: `UNOWNED · "${row.summary}" · ${where}`,
        detail: 'no claimant yet',
        target: null,
        verb: 'prompt an agent to claim, or assign one',
      },
    ];
  }

  return [];
}

function fleetCardSeverity(trust: Trust, liveness: Liveness): Severity {
  if (trust === 'mismatch') return 'critical';
  if (liveness === 'down') return 'critical';
  if (liveness === 'stale' || trust === 'first-sight' || trust === 'unsigned') return 'attention';
  return 'nominal';
}

function fleetCardFixVerb(display: string, trust: Trust, liveness: Liveness): string | null {
  if (trust === 'mismatch') return `verify ${display} locally before trusting its posts`;
  if (liveness === 'down') return `${display} is down — check the host / restart the session`;
  if (trust === 'first-sight') return `confirm ${display}'s key if you expected it to (re)join`;
  if (trust === 'unsigned') return `verify ${display} — posting unsigned`;
  if (liveness === 'stale') return `nudge ${display} — heartbeat stale`;
  return null;
}

const LIVENESS_RANK: Record<Liveness, number> = { down: 0, stale: 1, live: 2 };
const TRUST_RANK: Record<Trust, number> = { mismatch: 0, unsigned: 1, 'first-sight': 2, signed: 3 };

interface FleetAcc {
  display: string;
  color: string;
  abbr: string;
  hubs: Set<string>;
  liveness: Liveness;
  hbAgeSecs: number | null;
  trust: Trust;
  wip: number;
  host: string | null;
  lastTs: string | null;
}

/** One agent identity can appear on several hubs (design/47 §2.2 Lane 3: "an
 * agent on two hubs appears once, keyed by signing identity"). When
 * occurrences disagree, the WORSE liveness/trust wins — a real alarm on one
 * hub must not be hidden by a healthy showing on another. */
function mergeFleetOccurrence(acc: FleetAcc | undefined, agent: Agent, hub: Hub): FleetAcc {
  const liveness = deriveLiveness(agent);
  const trust = deriveTrust(agent);
  const wip = agent.wip.filter((w) => w.status === 'CLAIMED').length;
  if (!acc) {
    return {
      display: agent.display,
      color: agent.color,
      abbr: agent.abbr,
      hubs: new Set([hub.id]),
      liveness,
      hbAgeSecs: null,
      trust,
      wip,
      host: agent.lastHost ?? agent.expectedHost ?? null,
      lastTs: agent.lastTs,
    };
  }
  acc.hubs.add(hub.id);
  acc.wip += wip;
  if (LIVENESS_RANK[liveness] < LIVENESS_RANK[acc.liveness]) acc.liveness = liveness;
  if (TRUST_RANK[trust] < TRUST_RANK[acc.trust]) acc.trust = trust;
  if (!acc.lastTs || (agent.lastTs && agent.lastTs > acc.lastTs)) {
    acc.lastTs = agent.lastTs;
    acc.host = agent.lastHost ?? agent.expectedHost ?? acc.host;
  }
  return acc;
}

function buildFleetCards(hubOverviews: { hub: Hub; overview: Overview }[]): FleetCard[] {
  const byId = new Map<string, FleetAcc>();
  for (const { hub, overview } of hubOverviews) {
    for (const agent of overview.fleet) {
      byId.set(agent.id, mergeFleetOccurrence(byId.get(agent.id), agent, hub));
    }
  }
  const cards: FleetCard[] = [...byId.entries()].map(([id, acc]) => {
    const severity = fleetCardSeverity(acc.trust, acc.liveness);
    return {
      id,
      display: acc.display,
      color: acc.color,
      abbr: acc.abbr,
      hubs: [...acc.hubs],
      liveness: acc.liveness,
      hbAgeSecs: acc.hbAgeSecs,
      trust: acc.trust,
      wip: acc.wip,
      host: acc.host,
      lastTs: acc.lastTs,
      severity,
      fixVerb: fleetCardFixVerb(acc.display, acc.trust, acc.liveness),
    };
  });
  cards.sort((a, b) => SEVERITY_WEIGHT[a.severity] - SEVERITY_WEIGHT[b.severity] || a.display.localeCompare(b.display));
  return cards;
}

function bySeverityThenAge(a: AttentionItem, b: AttentionItem): number {
  const w = SEVERITY_WEIGHT[a.severity] - SEVERITY_WEIGHT[b.severity];
  if (w !== 0) return w;
  return (b.ageSecs ?? 0) - (a.ageSecs ?? 0);
}

/**
 * The Overview view's whole data model, computed from a per-hub fan-out of
 * `/api/overview` (design/47 §5 Phase 1 — no new backend endpoint yet).
 * Pure and synchronous: `api.ts`'s `getAttention()` does the fetching, this
 * does the folding/ranking, so the ranking rules are testable without a
 * network or a DOM.
 */
export function aggregateAttention(hubOverviews: { hub: Hub; overview: Overview }[]): Attention {
  const needsYou: AttentionItem[] = [];
  const coordination: AttentionItem[] = [];

  for (const { hub, overview } of hubOverviews) {
    for (const agent of overview.fleet) needsYou.push(...needsYouForAgent(agent, hub));
    for (const row of overview.board.requests) coordination.push(...coordinationForRow(row, hub));
  }

  needsYou.sort(bySeverityThenAge);
  coordination.sort(bySeverityThenAge);

  const fleet = buildFleetCards(hubOverviews);

  const perHub: HubRollup[] = hubOverviews.map(({ hub, overview }) => ({
    hub: hub.id,
    label: hub.label,
    live: overview.fleet.filter((a) => deriveLiveness(a) === 'live').length,
    attention: needsYou.filter((i) => i.hub === hub.id).length + coordination.filter((i) => i.hub === hub.id).length,
  }));

  const openRequests = hubOverviews.reduce(
    (sum, { overview }) => sum + overview.board.requests.filter((r) => r.status === 'OPEN' || r.status === 'CLAIMED').length,
    0
  );

  return { needsYou, coordination, fleet, metrics: { openRequests, perHub } };
}
