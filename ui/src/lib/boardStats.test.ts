import { describe, expect, it } from 'vitest';
import { computeAsking, computeBoardStats, computeCarrying, computeFlowBar, computeThroughput, filterRequests, summarizeThroughput, verdictParts } from './boardStats';
import type { Agent, Message, RequestRow } from './types';

function request(overrides: Partial<RequestRow> = {}): RequestRow {
  return {
    id: 'req_1',
    from: 'herald',
    to: [],
    summary: 'do the thing',
    status: 'OPEN',
    resolution: null,
    deferred: false,
    claimants: [],
    ageSecs: 60,
    stale: false,
    topic: 'general',
    ...overrides,
  };
}

function agent(overrides: Partial<Agent> = {}): Agent {
  return {
    id: 'jarvis',
    display: 'Jarvis',
    desc: null,
    expectedHost: null,
    lastTs: null,
    lastHost: null,
    live: true,
    verified: 'signed',
    color: '#7dcfff',
    abbr: 'JA',
    wip: [],
    ...overrides,
  };
}

function message(overrides: Partial<Message> = {}): Message {
  return {
    id: 'msg_1',
    from: 'herald',
    type: 'note',
    ts: '2026-07-18T10:00:00Z',
    host: null,
    to: [],
    cc: [],
    topic: 'general',
    summary: 'hi',
    body: 'hi',
    of: null,
    replyTo: null,
    supersedes: null,
    refs: [],
    seenBy: [],
    ...overrides,
  };
}

describe('computeBoardStats', () => {
  it('activeWork is the TOTAL of everything not done — inFlight/stuck/needsOwner are sub-counts of it, not siblings', () => {
    const requests = [
      request({ id: 'a', status: 'OPEN', claimants: [] }), // needsOwner
      request({ id: 'b', status: 'CLAIMED', claimants: ['jarvis'] }), // inFlight
      request({ id: 'c', status: 'BLOCKED' }), // stuck
      request({ id: 'd', status: 'DONE' }), // done — excluded from activeWork
      request({ id: 'e', status: 'OPEN', deferred: true }), // parked — counts toward activeWork only
    ];
    expect(computeBoardStats(requests)).toEqual({ activeWork: 4, inFlight: 1, stuck: 1, needsOwner: 1, done: 1 });
  });

  it('an empty board is all zeros, not undefined/NaN', () => {
    expect(computeBoardStats([])).toEqual({ activeWork: 0, inFlight: 0, stuck: 0, needsOwner: 0, done: 0 });
  });
});

describe('computeFlowBar', () => {
  it('excludes done and expresses live work as percentages summing to ~100', () => {
    const requests = [request({ id: 'a', status: 'CLAIMED', claimants: ['jarvis'] }), request({ id: 'b', status: 'BLOCKED' }), request({ id: 'c', status: 'DONE' })];
    const bar = computeFlowBar(requests);
    expect(bar.map((s) => s.state)).toEqual(['flight', 'stuck']);
    expect(bar.reduce((sum, s) => sum + s.pct, 0)).toBeCloseTo(100);
  });

  it('returns an empty array (not a divide-by-zero NaN bar) when there is no live work', () => {
    expect(computeFlowBar([request({ status: 'DONE' })])).toEqual([]);
    expect(computeFlowBar([])).toEqual([]);
  });
});

describe('computeCarrying', () => {
  it('reuses the real Agent.wip CLAIMED count, sorted desc, omitting idle agents', () => {
    const agents = [
      agent({ id: 'jarvis', wip: [{ id: 'req_a', summary: '', status: 'CLAIMED' }] }),
      agent({ id: 'herald', wip: [{ id: 'req_b', summary: '', status: 'CLAIMED' }, { id: 'req_c', summary: '', status: 'CLAIMED' }] }),
      agent({ id: 'idle', wip: [] }),
    ];
    expect(computeCarrying(agents)).toEqual([
      { agentId: 'herald', count: 2 },
      { agentId: 'jarvis', count: 1 },
    ]);
  });

  it('a non-CLAIMED wip entry (defensive — should not happen in real data) does not count as carrying', () => {
    const agents = [agent({ id: 'jarvis', wip: [{ id: 'req_a', summary: '', status: 'DONE' }] })];
    expect(computeCarrying(agents)).toEqual([]);
  });
});

describe('computeAsking', () => {
  it('groups live requests by requester, breaking out the unowned portion', () => {
    const requests = [
      request({ id: 'a', from: 'jarvis', status: 'OPEN', claimants: [] }),
      request({ id: 'b', from: 'jarvis', status: 'CLAIMED', claimants: ['herald'] }),
      request({ id: 'c', from: 'herald', status: 'DONE' }), // excluded — not live
    ];
    expect(computeAsking(requests)).toEqual([{ agentId: 'jarvis', count: 2, unownedCount: 1 }]);
  });

  it('an empty board asks nothing', () => {
    expect(computeAsking([])).toEqual([]);
  });
});

describe('computeThroughput / summarizeThroughput', () => {
  const NOW = new Date('2026-07-19T12:00:00Z').getTime();

  it('buckets real request/done message timestamps into the last N calendar days, today last', () => {
    const messages = [
      message({ type: 'request', ts: '2026-07-17T10:00:00Z' }),
      message({ type: 'done', ts: '2026-07-19T09:00:00Z' }),
      message({ type: 'done', ts: '2026-07-19T11:00:00Z' }),
      message({ type: 'note', ts: '2026-07-19T10:30:00Z' }), // ignored — not open/close
    ];
    const days = computeThroughput(messages, 3, NOW);
    expect(days.map((d) => d.day)).toEqual(['2026-07-17', '2026-07-18', '2026-07-19']);
    expect(days[0]).toEqual({ day: '2026-07-17', opened: 1, closed: 0 });
    expect(days[2]).toEqual({ day: '2026-07-19', opened: 0, closed: 2 });
  });

  it('a message older than the window is dropped, not misattributed to the earliest bucket', () => {
    const messages = [message({ type: 'done', ts: '2026-06-01T00:00:00Z' })];
    const days = computeThroughput(messages, 3, NOW);
    expect(days.every((d) => d.closed === 0)).toBe(true);
  });

  it('summarizeThroughput totals and nets the window', () => {
    const days = computeThroughput(
      [message({ type: 'done', ts: '2026-07-19T09:00:00Z' }), message({ type: 'request', ts: '2026-07-19T09:00:00Z' })],
      1,
      NOW
    );
    expect(summarizeThroughput(days)).toEqual({ closed: 1, opened: 1, net: 0 });
  });
});

describe('verdictParts', () => {
  it('omits a clause with nothing to report rather than padding "0 stuck"', () => {
    const stats = computeBoardStats([]);
    const v = verdictParts(stats, { closed: 0, opened: 0, net: 0 });
    expect(v.needsOwner).toBeNull();
    expect(v.stuck).toBeNull();
    expect(v.trend).toBe('holding steady →');
  });

  it('reports real needs-owner/stuck counts, singular-aware', () => {
    const stats = computeBoardStats([request({ status: 'OPEN', claimants: [] })]);
    expect(verdictParts(stats, { closed: 0, opened: 0, net: 0 }).needsOwner).toBe('1 needs an owner');
  });

  it('trend reads the real net — closing faster, opening faster, or steady', () => {
    const stats = computeBoardStats([]);
    expect(verdictParts(stats, { closed: 5, opened: 2, net: 3 }).trend).toBe('closing faster than opening ↗');
    expect(verdictParts(stats, { closed: 2, opened: 5, net: -3 }).trend).toBe('opening faster than closing ↘');
  });
});

describe('filterRequests', () => {
  const a = request({ id: 'a', from: 'jarvis', status: 'OPEN', claimants: [] }); // unowned, jarvis asking
  const b = request({ id: 'b', from: 'herald', status: 'CLAIMED', claimants: ['jarvis'] }); // flight, jarvis carrying
  const c = request({ id: 'c', from: 'herald', status: 'BLOCKED', claimants: ['herald'] }); // stuck, herald

  it('a null dimension matches everything ("no opinion")', () => {
    expect(filterRequests([a, b, c], null, null)).toEqual([a, b, c]);
  });

  it('state alone narrows to that state', () => {
    expect(filterRequests([a, b, c], 'stuck', null)).toEqual([c]);
  });

  it('agent alone matches either the requester OR the claimant — "their work"', () => {
    expect(filterRequests([a, b, c], null, 'jarvis')).toEqual([a, b]);
  });

  it('both dimensions combine with AND', () => {
    expect(filterRequests([a, b, c], 'flight', 'jarvis')).toEqual([b]);
    expect(filterRequests([a, b, c], 'unowned', 'herald')).toEqual([]);
  });
});
