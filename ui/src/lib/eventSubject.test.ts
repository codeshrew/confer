import { describe, expect, it } from 'vitest';
import { EVENT_COLOR_VAR, EVENT_ICON, resolveEventSubject, SYSLINE_TYPES } from './eventSubject';
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

describe('SYSLINE_TYPES', () => {
  it('includes blocked — a real gap the old ad-hoc set missed', () => {
    expect(SYSLINE_TYPES.has('blocked')).toBe(true);
  });
  it('includes every lifecycle/system type the brief names', () => {
    for (const t of ['claim', 'done', 'error', 'blocked', 'defer', 'supersede'] as const) {
      expect(SYSLINE_TYPES.has(t)).toBe(true);
    }
  });
  it('excludes note/request — those are full conversational messages', () => {
    expect(SYSLINE_TYPES.has('note')).toBe(false);
    expect(SYSLINE_TYPES.has('request')).toBe(false);
  });
});

describe('EVENT_ICON / EVENT_COLOR_VAR', () => {
  it('gives done and claim the same green (--state-flight), per the brief', () => {
    expect(EVENT_COLOR_VAR.done).toBe('var(--state-flight)');
    expect(EVENT_COLOR_VAR.claim).toBe('var(--state-flight)');
  });
  it('gives blocked and error the same red (--state-stuck)', () => {
    expect(EVENT_COLOR_VAR.blocked).toBe('var(--state-stuck)');
    expect(EVENT_COLOR_VAR.error).toBe('var(--state-stuck)');
  });
  it('gives defer amber and supersede muted, distinctly from the rest', () => {
    expect(EVENT_COLOR_VAR.defer).toBe('var(--state-unowned)');
    expect(EVENT_COLOR_VAR.supersede).toBe('var(--muted)');
  });
  it('has a real icon glyph for every kind', () => {
    for (const kind of ['claim', 'done', 'error', 'blocked', 'defer', 'supersede'] as const) {
      expect(EVENT_ICON[kind]).toBeTruthy();
    }
  });
});

describe('resolveEventSubject — ticket-kind events (claim/done/error/blocked/defer)', () => {
  const req = request({ id: 'req_8f2', summary: 'wire the endpoint' });

  it('resolves via `of` to a real ticket', () => {
    const evt = message({ type: 'claim', of: 'msg_8f2' });
    expect(resolveEventSubject(evt, [req], [], [])).toEqual({ kind: 'ticket', id: 'req_8f2', label: 'req_8f2' });
  });

  it('resolves the same way for done/error/blocked/defer — all ticket-kind', () => {
    for (const type of ['done', 'error', 'blocked', 'defer'] as const) {
      const evt = message({ type, of: 'msg_8f2' });
      expect(resolveEventSubject(evt, [req], [], [])).toEqual({ kind: 'ticket', id: 'req_8f2', label: 'req_8f2' });
    }
  });

  it('law #3 — a null `of` never resolves (no fabricated subject)', () => {
    const evt = message({ type: 'claim', of: null });
    expect(resolveEventSubject(evt, [req], [], [])).toBeNull();
  });

  it('law #3 — an `of` that maps to a ticket NOT in `requests` is dangling, not resolved', () => {
    const evt = message({ type: 'claim', of: 'msg_purged' });
    expect(resolveEventSubject(evt, [req], [], [])).toBeNull();
  });
});

describe('resolveEventSubject — thread-kind events (supersede)', () => {
  const original = message({ id: 'msg_orig', type: 'note', summary: 'restore chain context' });

  it('resolves via `supersedes` to a real message, labeled with its summary', () => {
    const evt = message({ type: 'supersede', supersedes: 'msg_orig' });
    expect(resolveEventSubject(evt, [], [original], [])).toEqual({ kind: 'thread', msgId: 'msg_orig', label: 'restore chain context' });
  });

  it('falls back to `replyTo` when `supersedes` is null', () => {
    const evt = message({ type: 'supersede', supersedes: null, replyTo: 'msg_orig' });
    expect(resolveEventSubject(evt, [], [original], [])).toEqual({ kind: 'thread', msgId: 'msg_orig', label: 'restore chain context' });
  });

  it('law #3 — neither `supersedes` nor `replyTo` set never resolves', () => {
    const evt = message({ type: 'supersede', supersedes: null, replyTo: null });
    expect(resolveEventSubject(evt, [], [original], [])).toBeNull();
  });

  it('law #3 — a `supersedes` pointing at a message not in `messages` is dangling', () => {
    const evt = message({ type: 'supersede', supersedes: 'msg_never_loaded' });
    expect(resolveEventSubject(evt, [], [original], [])).toBeNull();
  });
});

describe('resolveEventSubject — non-event types never produce a subject', () => {
  it('returns null for note/request — those are not events', () => {
    expect(resolveEventSubject(message({ type: 'note' }), [], [], [])).toBeNull();
    expect(resolveEventSubject(message({ type: 'request' }), [], [], [])).toBeNull();
  });
});

// Piece 9's `agent` subject kind has no real MsgType producing it today (no
// join/roster event exists) — nothing to test against real data without
// fabricating a message type the backend doesn't send.
