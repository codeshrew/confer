import { describe, expect, it } from 'vitest';
import { buildLifecycleTrack, ticketOriginMessage, ticketRefs, ticketStateLabel, ticketStateOf, ticketStateVar } from './ticketState';
import type { Agent, CodeRef, Message, RequestRow } from './types';

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

function agent(overrides: Partial<Agent> = {}): Agent {
  return {
    id: 'herald',
    display: 'Herald',
    desc: null,
    expectedHost: null,
    lastTs: null,
    lastHost: null,
    live: true,
    verified: 'signed',
    version: null,
    watchState: null,
    keyFingerprint: null,
    color: '#7dcfff',
    abbr: 'HE',
    wip: [],
    ...overrides,
  };
}

describe('ticketStateOf', () => {
  it('maps CLAIMED to flight (in-flight/green)', () => {
    expect(ticketStateOf(request({ status: 'CLAIMED', claimants: ['jarvis'] }))).toBe('flight');
  });
  it('maps BLOCKED and ERROR to stuck (red)', () => {
    expect(ticketStateOf(request({ status: 'BLOCKED' }))).toBe('stuck');
    expect(ticketStateOf(request({ status: 'ERROR' }))).toBe('stuck');
  });
  it('maps DONE and SUPERSEDED to done (grey)', () => {
    expect(ticketStateOf(request({ status: 'DONE' }))).toBe('done');
    expect(ticketStateOf(request({ status: 'SUPERSEDED' }))).toBe('done');
  });
  it('maps unclaimed OPEN to unowned (amber, needs an owner)', () => {
    expect(ticketStateOf(request({ status: 'OPEN', claimants: [] }))).toBe('unowned');
  });
  it('keeps a deferred OPEN request as open (cyan) — parked is not stuck/unowned', () => {
    expect(ticketStateOf(request({ status: 'OPEN', deferred: true }))).toBe('open');
  });
  it('an OPEN request that somehow already has a claimant reads as flight, not unowned', () => {
    expect(ticketStateOf(request({ status: 'OPEN', claimants: ['jarvis'] }))).toBe('flight');
  });
});

describe('ticketStateVar / ticketStateLabel', () => {
  it('resolves to the dedicated --state-* tokens, not the older ad-hoc ones', () => {
    expect(ticketStateVar('unowned')).toBe('var(--state-unowned)');
    expect(ticketStateVar('flight')).toBe('var(--state-flight)');
  });
  it('has a human label for every state', () => {
    expect(ticketStateLabel('unowned')).toBe('needs owner');
    expect(ticketStateLabel('flight')).toBe('claimed');
  });
});

describe('buildLifecycleTrack', () => {
  it('open: only Requested is done, Claimed/Done stay pending', () => {
    const req = request({ id: 'req_abc', status: 'OPEN' });
    const messages = [message({ id: 'msg_abc', type: 'request', from: 'herald' })];
    const track = buildLifecycleTrack(req, messages, [agent()]);
    expect(track.stages.map((s) => s.state)).toEqual(['done', 'pending', 'pending']);
    expect(track.stages[0]!.who).toBe('Herald');
    expect(track.branch).toBeNull();
  });

  it('flight: Claimed is current (pulsing), attributed to the claimant', () => {
    const req = request({ id: 'req_abc', status: 'CLAIMED', claimants: ['jarvis'] });
    const messages = [
      message({ id: 'msg_abc', type: 'request', from: 'herald', ts: '2026-07-18T10:00:00Z' }),
      message({ id: 'msg_c1', type: 'claim', from: 'jarvis', of: 'msg_abc', ts: '2026-07-18T10:05:00Z' }),
    ];
    const track = buildLifecycleTrack(req, messages, [agent(), agent({ id: 'jarvis', display: 'Jarvis' })]);
    expect(track.stages.map((s) => s.state)).toEqual(['done', 'current', 'pending']);
    expect(track.stages[1]!.who).toBe('Jarvis');
  });

  it('done: all three stages fill, resolution carried from the request', () => {
    const req = request({ id: 'req_abc', status: 'DONE', resolution: 'shipped it', claimants: ['jarvis'] });
    const messages = [
      message({ id: 'msg_abc', type: 'request', from: 'herald', ts: '2026-07-18T10:00:00Z' }),
      message({ id: 'msg_c1', type: 'claim', from: 'jarvis', of: 'msg_abc', ts: '2026-07-18T10:05:00Z' }),
      message({ id: 'msg_d1', type: 'done', from: 'jarvis', of: 'msg_abc', ts: '2026-07-18T11:00:00Z' }),
    ];
    const track = buildLifecycleTrack(req, messages, [agent(), agent({ id: 'jarvis', display: 'Jarvis' })]);
    expect(track.stages.map((s) => s.state)).toEqual(['done', 'done', 'done']);
    expect(track.resolution).toBe('shipped it');
    expect(track.branch).toBeNull();
  });

  it('stuck: a red branch off Claimed, carrying the blocking message as the reason', () => {
    const req = request({ id: 'req_abc', status: 'BLOCKED', claimants: ['jarvis'] });
    const messages = [
      message({ id: 'msg_abc', type: 'request', from: 'herald', ts: '2026-07-18T10:00:00Z' }),
      message({ id: 'msg_c1', type: 'claim', from: 'jarvis', of: 'msg_abc', ts: '2026-07-18T10:05:00Z' }),
      message({ id: 'msg_b1', type: 'blocked', from: 'jarvis', of: 'msg_abc', ts: '2026-07-18T10:30:00Z', summary: 'waiting on design review' }),
    ];
    const track = buildLifecycleTrack(req, messages, [agent(), agent({ id: 'jarvis', display: 'Jarvis' })]);
    expect(track.branch).toEqual({ off: 'Claimed', who: 'Jarvis', ts: '10:30', reason: 'waiting on design review' });
  });

  it('stuck before any claim: the branch is off Requested, not Claimed', () => {
    const req = request({ id: 'req_abc', status: 'ERROR' });
    const messages = [
      message({ id: 'msg_abc', type: 'request', from: 'herald', ts: '2026-07-18T10:00:00Z' }),
      message({ id: 'msg_e1', type: 'error', from: 'herald', of: 'msg_abc', ts: '2026-07-18T10:10:00Z', summary: 'malformed patch' }),
    ];
    const track = buildLifecycleTrack(req, messages, [agent()]);
    expect(track.branch?.off).toBe('Requested');
  });
});

describe('ticketRefs', () => {
  it('de-duplicates refs across the origin message and its trail', () => {
    const ref: CodeRef = {
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
    };
    const messages = [
      message({ id: 'msg_abc', refs: [ref] }),
      message({ id: 'msg_c1', of: 'msg_abc', refs: [ref] }),
    ];
    const refs = ticketRefs(request({ id: 'req_abc' }), messages);
    expect(refs).toHaveLength(1);
  });
});

describe('ticketOriginMessage', () => {
  it('resolves the req_ id to its originating msg_ id', () => {
    const messages = [message({ id: 'msg_abc', summary: 'the origin' })];
    expect(ticketOriginMessage(request({ id: 'req_abc' }), messages)?.summary).toBe('the origin');
  });
  it('returns null when the origin message is not loaded', () => {
    expect(ticketOriginMessage(request({ id: 'req_missing' }), [])).toBeNull();
  });
});
