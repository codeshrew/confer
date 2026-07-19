import { describe, expect, it } from 'vitest';
import { aggregateAttention, agentPresence, deriveLiveness, deriveTrust, hubHealthReason } from './attention';
import type { Agent, Hub, Overview, RequestRow } from './types';

function hub(id: string, label = id, overrides: Partial<Hub> = {}): Hub {
  return { id, label, name: label, current: false, agentCount: 0, ...overrides };
}

function agent(overrides: Partial<Agent> = {}): Agent {
  return {
    id: 'reader',
    display: 'Reader',
    desc: null,
    expectedHost: 'reader',
    lastTs: '2026-07-18T10:00:00Z',
    lastHost: 'reader',
    live: true,
    verified: 'signed',
    color: 'var(--ag-reader)',
    abbr: 'RE',
    wip: [],
    ...overrides,
  };
}

function row(overrides: Partial<RequestRow> = {}): RequestRow {
  return {
    id: 'req_1',
    from: 'pipeline',
    to: ['reader'],
    summary: 'wire up search',
    status: 'OPEN',
    resolution: null,
    deferred: false,
    claimants: [],
    ageSecs: 100,
    stale: false,
    topic: 'general',
    ...overrides,
  };
}

function overview(fleet: Agent[], requests: RequestRow[], h: Hub): Overview {
  return {
    hub: h,
    topics: [],
    board: {
      requests,
      open: requests.filter((r) => r.status === 'OPEN').length,
      claimed: requests.filter((r) => r.status === 'CLAIMED').length,
      blocked: requests.filter((r) => r.status === 'BLOCKED').length,
      backlog: requests.filter((r) => r.deferred).length,
      closed: requests.filter((r) => r.status === 'DONE').length,
    },
    fleet,
  };
}

describe('deriveLiveness / deriveTrust — Phase 2 fallback', () => {
  it('uses the real liveness/trust fields when present', () => {
    expect(deriveLiveness(agent({ live: true, liveness: 'stale' }))).toBe('stale');
    expect(deriveTrust(agent({ verified: 'signed', trust: 'mismatch' }))).toBe('mismatch');
  });

  it('falls back to the lossy bool/enum when the backend has not landed Phase 2 yet', () => {
    expect(deriveLiveness(agent({ live: true }))).toBe('live');
    expect(deriveLiveness(agent({ live: false }))).toBe('down');
    expect(deriveTrust(agent({ verified: 'signed' }))).toBe('signed');
    expect(deriveTrust(agent({ verified: 'first-sight' }))).toBe('first-sight');
    expect(deriveTrust(agent({ verified: 'unverified' }))).toBe('unsigned');
  });
});

describe('aggregateAttention — empty input is the all-clear state', () => {
  it('returns empty lanes, not an error, for zero hubs', () => {
    const result = aggregateAttention([]);
    expect(result.needsYou).toEqual([]);
    expect(result.coordination).toEqual([]);
    expect(result.fleet).toEqual([]);
    expect(result.metrics.openRequests).toBe(0);
    expect(result.metrics.perHub).toEqual([]);
  });

  it('a fully nominal fleet (signed, live, no stuck requests) raises nothing', () => {
    const h = hub('calm-hub');
    const ov = overview([agent()], [row({ status: 'DONE', deferred: false })], h);
    const result = aggregateAttention([{ hub: h, overview: ov }]);
    expect(result.needsYou).toEqual([]);
    expect(result.coordination).toEqual([]);
    expect(result.fleet).toHaveLength(1);
    expect(result.fleet[0]!.severity).toBe('nominal');
    expect(result.fleet[0]!.fixVerb).toBeNull();
  });
});

describe('Lane 1 — needs-you from agent trust', () => {
  it('mismatch is critical and names the agent as the target', () => {
    const h = hub('agent-coord');
    const ov = overview([agent({ id: 'jarvis', display: 'Jarvis', verified: 'unverified', trust: 'mismatch' })], [], h);
    const [item] = aggregateAttention([{ hub: h, overview: ov }]).needsYou;
    expect(item!.severity).toBe('critical');
    expect(item!.kind).toBe('mismatch');
    expect(item!.target).toBe('jarvis');
    expect(item!.verb).toContain('verify Jarvis');
  });

  it('first-sight is info-level and suggests confirm-if-expected', () => {
    const h = hub('agent-coord');
    const ov = overview([agent({ verified: 'first-sight' })], [], h);
    const [item] = aggregateAttention([{ hub: h, overview: ov }]).needsYou;
    expect(item!.severity).toBe('info');
    expect(item!.verb).toMatch(/confirm/i);
  });

  it('unsigned is critical', () => {
    const h = hub('agent-coord');
    const ov = overview([agent({ trust: 'unsigned' })], [], h);
    const [item] = aggregateAttention([{ hub: h, overview: ov }]).needsYou;
    expect(item!.severity).toBe('critical');
  });

  it('signed agents raise no needs-you item', () => {
    const h = hub('agent-coord');
    const ov = overview([agent({ verified: 'signed' })], [], h);
    expect(aggregateAttention([{ hub: h, overview: ov }]).needsYou).toEqual([]);
  });
});

describe('Lane 2 — coordination from board rows', () => {
  it('a stale claimed request nudges the claimant', () => {
    const h = hub('agent-coord');
    const ov = overview([], [row({ stale: true, claimants: ['reader'] })], h);
    const [item] = aggregateAttention([{ hub: h, overview: ov }]).coordination;
    expect(item!.kind).toBe('stale-claimed');
    expect(item!.severity).toBe('attention');
    expect(item!.target).toBe('reader');
    expect(item!.verb).toContain('nudge reader');
  });

  it('a stale, unclaimed, addressed request nudges the addressee', () => {
    const h = hub('agent-coord');
    const ov = overview([], [row({ stale: true, claimants: [], to: ['jarvis'] })], h);
    const [item] = aggregateAttention([{ hub: h, overview: ov }]).coordination;
    expect(item!.kind).toBe('stale-open');
    expect(item!.target).toBe('jarvis');
  });

  it('a stale, unclaimed, --to-all request has no single target — "prompt an agent"', () => {
    const h = hub('agent-coord');
    const ov = overview([], [row({ stale: true, claimants: [], to: ['all'] })], h);
    const [item] = aggregateAttention([{ hub: h, overview: ov }]).coordination;
    expect(item!.kind).toBe('unowned');
    expect(item!.target).toBeNull();
    expect(item!.verb).toMatch(/prompt an agent/);
  });

  it('a blocked request targets the claimant with the stated reason', () => {
    const h = hub('agent-coord');
    const ov = overview([], [row({ status: 'BLOCKED', claimants: ['compositor'], resolution: 'needs the schema frozen' })], h);
    const [item] = aggregateAttention([{ hub: h, overview: ov }]).coordination;
    expect(item!.kind).toBe('blocked');
    expect(item!.target).toBe('compositor');
    expect(item!.verb).toContain('needs the schema frozen');
  });

  it('a fresh (< 1h), non-stale, unclaimed OPEN request raises nothing yet', () => {
    const h = hub('agent-coord');
    const ov = overview([], [row({ status: 'OPEN', claimants: [], ageSecs: 60, stale: false })], h);
    expect(aggregateAttention([{ hub: h, overview: ov }]).coordination).toEqual([]);
  });

  it('an aged (>= 1h) non-stale unclaimed OPEN request is info-level unowned', () => {
    const h = hub('agent-coord');
    const ov = overview([], [row({ status: 'OPEN', claimants: [], ageSecs: 3700, stale: false })], h);
    const [item] = aggregateAttention([{ hub: h, overview: ov }]).coordination;
    expect(item!.kind).toBe('unowned');
    expect(item!.severity).toBe('info');
  });

  it('deferred (backlog) rows never raise an item — already triaged', () => {
    const h = hub('agent-coord');
    const ov = overview([], [row({ status: 'OPEN', deferred: true, claimants: [], ageSecs: 999999, stale: false })], h);
    expect(aggregateAttention([{ hub: h, overview: ov }]).coordination).toEqual([]);
  });

  it('DONE/ERROR/SUPERSEDED rows never raise an item', () => {
    const h = hub('agent-coord');
    const ov = overview(
      [],
      [row({ status: 'DONE', stale: true }), row({ status: 'ERROR', stale: true }), row({ status: 'SUPERSEDED', stale: true })],
      h
    );
    expect(aggregateAttention([{ hub: h, overview: ov }]).coordination).toEqual([]);
  });
});

describe('severity ranking — critical sorts before attention before info', () => {
  it('needsYou is sorted mismatch (critical) before first-sight (info)', () => {
    const h = hub('agent-coord');
    const ov = overview(
      [agent({ id: 'jarvis', verified: 'first-sight' }), agent({ id: 'orbit', trust: 'mismatch' })],
      [],
      h
    );
    const items = aggregateAttention([{ hub: h, overview: ov }]).needsYou;
    expect(items.map((i) => i.kind)).toEqual(['mismatch', 'first-sight']);
  });
});

describe('Lane 3 — fleet dedupe across hubs', () => {
  it('an agent on two hubs appears once, hubs merged', () => {
    const hA = hub('agent-coord');
    const hB = hub('confer-lab');
    const a = agent({ id: 'herald', wip: [{ id: 'r1', summary: 's', status: 'CLAIMED' }] });
    const ovA = overview([a], [], hA);
    const ovB = overview([a], [], hB);
    const fleet = aggregateAttention([
      { hub: hA, overview: ovA },
      { hub: hB, overview: ovB },
    ]).fleet;
    expect(fleet).toHaveLength(1);
    expect(fleet[0]!.hubs.sort()).toEqual(['agent-coord', 'confer-lab']);
    // WIP sums across both occurrences (each carried the same CLAIMED item
    // in this fixture, but the point is the accumulator adds, not replaces).
    expect(fleet[0]!.wip).toBe(2);
  });

  it('the WORSE trust/liveness across occurrences wins — an alarm on one hub is never hidden', () => {
    const hA = hub('agent-coord');
    const hB = hub('confer-lab');
    const healthy = agent({ id: 'orbit', live: true, verified: 'signed' });
    const spoofed = agent({ id: 'orbit', live: true, trust: 'mismatch' });
    const fleet = aggregateAttention([
      { hub: hA, overview: overview([healthy], [], hA) },
      { hub: hB, overview: overview([spoofed], [], hB) },
    ]).fleet;
    expect(fleet).toHaveLength(1);
    expect(fleet[0]!.trust).toBe('mismatch');
    expect(fleet[0]!.severity).toBe('critical');
  });

  it('a down agent is critical with a restart-hint fixVerb', () => {
    const h = hub('agent-coord');
    const ov = overview([agent({ id: 'orbit', display: 'Orbit', live: false })], [], h);
    const fleet = aggregateAttention([{ hub: h, overview: ov }]).fleet;
    expect(fleet[0]!.severity).toBe('critical');
    expect(fleet[0]!.fixVerb).toMatch(/Orbit is down/);
  });
});

describe('metrics — ambient context strip', () => {
  it('counts open+claimed requests across hubs, and rolls up live/attention per hub', () => {
    const hA = hub('agent-coord', 'agent-coord');
    const hB = hub('confer-lab', 'confer-lab');
    const ovA = overview([agent({ id: 'a1' })], [row({ status: 'OPEN' }), row({ status: 'CLAIMED', id: 'req_2' })], hA);
    const ovB = overview([agent({ id: 'a2', live: false })], [row({ status: 'DONE', id: 'req_3' })], hB);
    const result = aggregateAttention([
      { hub: hA, overview: ovA },
      { hub: hB, overview: ovB },
    ]);
    expect(result.metrics.openRequests).toBe(2);
    const rollupA = result.metrics.perHub.find((r) => r.hub === 'agent-coord')!;
    const rollupB = result.metrics.perHub.find((r) => r.hub === 'confer-lab')!;
    expect(rollupA.live).toBe(1);
    expect(rollupB.live).toBe(0);
  });
});

describe('domains — real per-hub tier/sync carried through unchanged, health computed honestly', () => {
  it('carries a null tier/sync through as null, never defaulting to "own" or a fake healthy sync', () => {
    const h = hub('sandbox', 'sandbox', { tier: null, sync: null });
    const ov = overview([agent()], [], h);
    const [domain] = aggregateAttention([{ hub: h, overview: ov }]).domains;
    expect(domain!.tier).toBeNull();
    expect(domain!.sync).toBeNull();
  });

  it('health: a known problem (down agent) outranks an unknown sync — never demoted to "unknown"', () => {
    const h = hub('orbit', 'orbit', { tier: 'foreign', sync: null });
    const ov = overview([agent({ live: false })], [], h);
    const [domain] = aggregateAttention([{ hub: h, overview: ov }]).domains;
    expect(domain!.health).toBe('critical');
    expect(hubHealthReason(domain!)).toBe('Reader down');
  });

  it('health: a mismatched trust is critical even with a fully healthy sync', () => {
    const h = hub('lab', 'lab', { sync: { lastFetchedSecs: 5, behind: 0, pending: 0, reachable: true } });
    const ov = overview([agent({ trust: 'mismatch' })], [], h);
    const [domain] = aggregateAttention([{ hub: h, overview: ov }]).domains;
    expect(domain!.health).toBe('critical');
    expect(hubHealthReason(domain!)).toBe('Reader key mismatch');
  });

  it('health: an unreachable sync is critical even with an otherwise-clean fleet', () => {
    const h = hub('lab', 'lab', { sync: { lastFetchedSecs: 5, behind: 0, pending: 0, reachable: false } });
    const ov = overview([agent()], [], h);
    const [domain] = aggregateAttention([{ hub: h, overview: ov }]).domains;
    expect(domain!.health).toBe('critical');
  });

  it('health: a stuck/coordination item (not agent-derived) is warn, not critical', () => {
    const h = hub('lab', 'lab', { sync: { lastFetchedSecs: 5, behind: 0, pending: 0, reachable: true } });
    const ov = overview([agent()], [row({ status: 'CLAIMED', claimants: ['reader'], stale: true })], h);
    const [domain] = aggregateAttention([{ hub: h, overview: ov }]).domains;
    expect(domain!.health).toBe('warn');
  });

  it('health: nothing known wrong + unprobed sync reads as UNKNOWN, never a fake "ok"', () => {
    const h = hub('sandbox', 'sandbox', { tier: null, sync: null });
    const ov = overview([agent()], [], h);
    const [domain] = aggregateAttention([{ hub: h, overview: ov }]).domains;
    expect(domain!.health).toBe('unknown');
    expect(hubHealthReason(domain!)).toBe('sync unknown');
  });

  it('health: reachable:null (probed for other fields, but reachability itself unknown) also reads unknown', () => {
    const h = hub('lab', 'lab', { sync: { lastFetchedSecs: 5, behind: 0, pending: 0, reachable: null } });
    const ov = overview([agent()], [], h);
    const [domain] = aggregateAttention([{ hub: h, overview: ov }]).domains;
    expect(domain!.health).toBe('unknown');
  });

  it('health: a fully healthy fleet + fully known, clean sync is ok — and only then', () => {
    const h = hub('lab', 'lab', { sync: { lastFetchedSecs: 5, behind: 0, pending: 0, reachable: true } });
    const ov = overview([agent()], [], h);
    const [domain] = aggregateAttention([{ hub: h, overview: ov }]).domains;
    expect(domain!.health).toBe('ok');
    expect(hubHealthReason(domain!)).toBe('healthy');
  });
});

describe('agentPresence — per-hub last-seen for the piece-8 dossier', () => {
  it('lists every hub an agent identity occurs on, each with that hub\'s OWN lastTs (not collapsed)', () => {
    const hubA = hub('lab', 'Lab', { tier: 'own' });
    const hubB = hub('orbit', 'Orbit', { tier: 'foreign' });
    const hubOverviews = [
      { hub: hubA, overview: overview([agent({ id: 'jarvis', lastTs: '2026-07-18T10:00:00Z' })], [], hubA) },
      { hub: hubB, overview: overview([agent({ id: 'jarvis', lastTs: '2026-07-18T09:00:00Z' })], [], hubB) },
    ];
    expect(agentPresence(hubOverviews, 'jarvis')).toEqual([
      { hub: 'lab', tier: 'own', lastTs: '2026-07-18T10:00:00Z' },
      { hub: 'orbit', tier: 'foreign', lastTs: '2026-07-18T09:00:00Z' },
    ]);
  });

  it('an agent not present on a hub is simply absent from that hub\'s row — never a fabricated entry', () => {
    const hubA = hub('lab');
    const hubOverviews = [{ hub: hubA, overview: overview([agent({ id: 'jarvis' })], [], hubA) }];
    expect(agentPresence(hubOverviews, 'herald')).toEqual([]);
  });
});
