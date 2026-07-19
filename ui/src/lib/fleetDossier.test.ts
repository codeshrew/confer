import { describe, expect, it } from 'vitest';
import { activityBuckets, askingFor, carryingFor, closedRecentCount } from './fleetDossier';
import type { Message, RequestRow } from './types';

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

describe('carryingFor', () => {
  it('only this agent\'s CLAIMED requests, sorted oldest first', () => {
    const requests = [
      request({ id: 'a', claimants: ['jarvis'], status: 'CLAIMED', ageSecs: 100 }),
      request({ id: 'b', claimants: ['jarvis'], status: 'CLAIMED', ageSecs: 500 }),
      request({ id: 'c', claimants: ['herald'], status: 'CLAIMED' }),
      request({ id: 'd', claimants: ['jarvis'], status: 'DONE' }), // closed — not flight, excluded
    ];
    expect(carryingFor('jarvis', requests).map((r) => r.id)).toEqual(['b', 'a']);
  });
});

describe('askingFor', () => {
  it('only this agent\'s LIVE (non-done) filed requests', () => {
    const requests = [
      request({ id: 'a', from: 'jarvis', status: 'OPEN' }),
      request({ id: 'b', from: 'jarvis', status: 'CLAIMED', claimants: ['herald'] }),
      request({ id: 'c', from: 'jarvis', status: 'DONE' }), // excluded — closed
      request({ id: 'd', from: 'herald', status: 'OPEN' }), // someone else's
    ];
    expect(askingFor('jarvis', requests).map((r) => r.id).sort()).toEqual(['a', 'b']);
  });

  it('excludes a ticket the agent filed AND already claimed themselves — no duplicate with carryingFor', () => {
    // Real data can produce this (an agent self-assigns), but it must not
    // show up on both halves of the same plate — regression for a real
    // duplicate caught in a live screenshot review.
    const requests = [request({ id: 'self', from: 'pipeline', claimants: ['pipeline'], status: 'CLAIMED' })];
    expect(carryingFor('pipeline', requests).map((r) => r.id)).toEqual(['self']);
    expect(askingFor('pipeline', requests)).toEqual([]);
  });
});

describe('closedRecentCount', () => {
  const NOW = new Date('2026-07-19T12:00:00Z').getTime();

  it('counts only THIS agent\'s own done messages within the window', () => {
    const messages = [
      message({ type: 'done', from: 'jarvis', ts: '2026-07-18T10:00:00Z' }), // within 7d
      message({ type: 'done', from: 'herald', ts: '2026-07-18T10:00:00Z' }), // someone else
      message({ type: 'done', from: 'jarvis', ts: '2026-06-01T10:00:00Z' }), // too old
      message({ type: 'note', from: 'jarvis', ts: '2026-07-18T10:00:00Z' }), // not a close
    ];
    expect(closedRecentCount('jarvis', messages, 7, NOW)).toBe(1);
  });

  it('an agent with no closes is honestly zero', () => {
    expect(closedRecentCount('jarvis', [], 7, NOW)).toBe(0);
  });
});

describe('activityBuckets', () => {
  const NOW = new Date('2026-07-19T12:30:00Z').getTime();

  it('buckets real message timestamps into hourly counts, oldest first, now last', () => {
    const messages = [
      message({ from: 'jarvis', ts: '2026-07-19T10:15:00Z' }),
      message({ from: 'jarvis', ts: '2026-07-19T10:45:00Z' }),
      message({ from: 'jarvis', ts: '2026-07-19T12:05:00Z' }),
      message({ from: 'herald', ts: '2026-07-19T12:05:00Z' }), // someone else — excluded
    ];
    const buckets = activityBuckets('jarvis', messages, 3, NOW);
    expect(buckets).toHaveLength(3);
    expect(buckets.map((b) => b.count)).toEqual([2, 0, 1]);
  });

  it('an agent with no activity in the window is all zeros, not undefined', () => {
    const buckets = activityBuckets('jarvis', [], 4, NOW);
    expect(buckets.every((b) => b.count === 0)).toBe(true);
    expect(buckets).toHaveLength(4);
  });
});
