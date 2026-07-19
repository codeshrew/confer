import { describe, expect, it } from 'vitest';
import { relatedRefs, relatedTickets, threadSummary } from './noteRelated';
import type { TrailNode } from './thread';
import type { CodeRef, RequestRow } from './types';

function node(overrides: Partial<TrailNode> = {}): TrailNode {
  return { msgId: 'msg_1', from: 'herald', type: 'note', topic: 'general', summary: 'hi', refs: [], ts: null, parentId: null, ...overrides };
}

function ref(overrides: Partial<CodeRef> = {}): CodeRef {
  return {
    repo: 'confer',
    path: 'src/patch.rs',
    sha: 'abc123',
    range: null,
    contentHash: null,
    refName: null,
    refType: null,
    commitDate: null,
    dirty: false,
    untracked: false,
    baseRef: null,
    forkPoint: null,
    ...overrides,
  };
}

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

describe('relatedTickets', () => {
  it('resolves REQUEST-type trail nodes to their real RequestRow', () => {
    const trail = [node({ msgId: 'msg_a', type: 'note' }), node({ msgId: 'msg_b', type: 'request' })];
    const requests = [request({ id: 'req_b' })];
    expect(relatedTickets(trail, requests)).toEqual([requests[0]]);
  });

  it('omits a request node whose row is not in the given hub\'s board — never a fabricated placeholder', () => {
    const trail = [node({ msgId: 'msg_b', type: 'request' })];
    expect(relatedTickets(trail, [])).toEqual([]);
  });

  it('de-duplicates if the same request somehow appears twice in the trail', () => {
    const trail = [node({ msgId: 'msg_b', type: 'request' }), node({ msgId: 'msg_b', type: 'request' })];
    const requests = [request({ id: 'req_b' })];
    expect(relatedTickets(trail, requests)).toHaveLength(1);
  });
});

describe('relatedRefs', () => {
  it('collects refs across the WHOLE trail, not just one message', () => {
    const refA = ref({ path: 'a.rs' });
    const refB = ref({ path: 'b.rs' });
    const trail = [node({ refs: [refA] }), node({ refs: [refB] })];
    expect(relatedRefs(trail)).toEqual([refA, refB]);
  });

  it('de-duplicates by repo:path:sha', () => {
    const shared = ref({ path: 'a.rs' });
    const trail = [node({ refs: [shared] }), node({ refs: [{ ...shared }] })];
    expect(relatedRefs(trail)).toHaveLength(1);
  });
});

describe('threadSummary', () => {
  it('counts real messages and distinct topics', () => {
    const trail = [node({ topic: 'reader' }), node({ topic: 'reader' }), node({ topic: 'studio' })];
    expect(threadSummary(trail)).toEqual({ messageCount: 3, topicCount: 2 });
  });

  it('an empty trail is honestly zero, not undefined', () => {
    expect(threadSummary([])).toEqual({ messageCount: 0, topicCount: 0 });
  });
});
