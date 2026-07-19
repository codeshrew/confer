import { describe, expect, it } from 'vitest';
import { buildGutterEntries, entryColorVar, gutterColumnCount, hitColorVar } from './codeGutter';
import type { RefHit } from './types';

function hit(overrides: Partial<RefHit> = {}): RefHit {
  return {
    repo: 'wealdlore',
    path: 'src/refcode.rs',
    sha: 'a41f0c2',
    range: [267, 285],
    contentHash: null,
    staleness: 'current',
    msgId: 'msg_1',
    from: 'jarvis',
    msgType: 'note',
    ts: '2026-07-12T10:00:00Z',
    topic: 'review-080',
    summary: 'the point-in-time read',
    threadRoot: 'msg_1',
    requestStatus: null,
    hub: 'agent-coord',
    hubPrivate: false,
    refName: 'main',
    refType: 'branch',
    commitDate: '2026-07-12T09:00:00Z',
    dirty: false,
    untracked: false,
    baseRef: null,
    forkPoint: null,
    ...overrides,
  };
}

describe('hitColorVar', () => {
  it('colors a note the metric hue', () => {
    expect(hitColorVar(hit({ msgType: 'note' }))).toBe('var(--state-metric)');
  });

  it('colors a request by its REAL requestStatus, not a fixed hue', () => {
    expect(hitColorVar(hit({ msgType: 'request', requestStatus: 'OPEN' }))).toBe('var(--state-open)');
    expect(hitColorVar(hit({ msgType: 'request', requestStatus: 'CLAIMED' }))).toBe('var(--state-flight)');
    expect(hitColorVar(hit({ msgType: 'request', requestStatus: 'BLOCKED' }))).toBe('var(--state-stuck)');
    expect(hitColorVar(hit({ msgType: 'request', requestStatus: 'ERROR' }))).toBe('var(--state-stuck)');
    expect(hitColorVar(hit({ msgType: 'request', requestStatus: 'DONE' }))).toBe('var(--state-done)');
    expect(hitColorVar(hit({ msgType: 'request', requestStatus: 'SUPERSEDED' }))).toBe('var(--state-done)');
  });

  it('a request with no requestStatus falls back to open, never a fabricated status', () => {
    expect(hitColorVar(hit({ msgType: 'request', requestStatus: null }))).toBe('var(--state-open)');
  });

  it('reuses the SAME event palette (piece 9) for claim/error/blocked/defer/supersede — but NOT done', () => {
    expect(hitColorVar(hit({ msgType: 'claim' }))).toBe('var(--state-flight)');
    expect(hitColorVar(hit({ msgType: 'blocked' }))).toBe('var(--state-stuck)');
    expect(hitColorVar(hit({ msgType: 'error' }))).toBe('var(--state-stuck)');
    expect(hitColorVar(hit({ msgType: 'defer' }))).toBe('var(--state-unowned)');
    expect(hitColorVar(hit({ msgType: 'supersede' }))).toBe('var(--muted)');
  });

  it('post-verify fix (Jarvis) — a `done` hit reads GREY (resolved), not piece 9\'s "just happened" GREEN', () => {
    expect(hitColorVar(hit({ msgType: 'done' }))).toBe('var(--state-done)');
    // Same grey a resolved REQUEST already gets — one consistent
    // "resolved" meaning across msgTypes, not two different hues.
    expect(hitColorVar(hit({ msgType: 'request', requestStatus: 'DONE' }))).toBe('var(--state-done)');
  });
});

describe('entryColorVar', () => {
  it('a single-hit entry just uses that hit\'s own color', () => {
    const entry = { hits: [hit({ msgType: 'note' })], range: [44, 49] as [number, number], isTick: false, column: 0, drift: false };
    expect(entryColorVar(entry)).toBe('var(--state-metric)');
  });

  it('post-verify fix (Jarvis) — a mixed-type entry reads the MOST-ACTIONABLE color, not hits[0]\'s', () => {
    // [done, resolved-request, note] — nothing here is genuinely OPEN, but
    // the note outranks the two resolved hits, so the entry reads the
    // note's teal — never the resolved grey those two alone would give.
    const entry = {
      hits: [
        hit({ msgId: 'a', msgType: 'done' }),
        hit({ msgId: 'b', msgType: 'request', requestStatus: 'DONE' }),
        hit({ msgId: 'c', msgType: 'note' }),
      ],
      range: [44, 49] as [number, number],
      isTick: false,
      column: 0,
      drift: false,
    };
    expect(entryColorVar(entry)).toBe('var(--state-metric)');
  });

  it('a blocked hit ALWAYS wins the entry color, regardless of hit order', () => {
    const withBlockedFirst = {
      hits: [hit({ msgId: 'a', msgType: 'blocked' }), hit({ msgId: 'b', msgType: 'note' }), hit({ msgId: 'c', msgType: 'done' })],
      range: [1, 5] as [number, number],
      isTick: false,
      column: 0,
      drift: false,
    };
    const withBlockedLast = {
      hits: [hit({ msgId: 'c', msgType: 'done' }), hit({ msgId: 'b', msgType: 'note' }), hit({ msgId: 'a', msgType: 'blocked' })],
      range: [1, 5] as [number, number],
      isTick: false,
      column: 0,
      drift: false,
    };
    expect(entryColorVar(withBlockedFirst)).toBe('var(--state-stuck)');
    expect(entryColorVar(withBlockedLast)).toBe('var(--state-stuck)');
  });

  it('the exact bug Jarvis caught: [note, note, resolved-request, done] never reads green "in-flight" from the done hit', () => {
    const entry = {
      hits: [
        hit({ msgId: 'a', msgType: 'note' }),
        hit({ msgId: 'b', msgType: 'note' }),
        hit({ msgId: 'c', msgType: 'request', requestStatus: 'DONE' }),
        hit({ msgId: 'd', msgType: 'done' }),
      ],
      range: [1, 5] as [number, number],
      isTick: false,
      column: 0,
      drift: false,
    };
    expect(entryColorVar(entry)).not.toBe('var(--state-flight)');
    expect(entryColorVar(entry)).toBe('var(--state-metric)');
  });

  it('an entry with genuinely open work reads that color even alongside resolved/note hits', () => {
    const entry = {
      hits: [
        hit({ msgId: 'a', msgType: 'note' }),
        hit({ msgId: 'b', msgType: 'done' }),
        hit({ msgId: 'c', msgType: 'request', requestStatus: 'OPEN' }),
      ],
      range: [1, 5] as [number, number],
      isTick: false,
      column: 0,
      drift: false,
    };
    expect(entryColorVar(entry)).toBe('var(--state-open)');
  });
});

describe('buildGutterEntries', () => {
  it('collapses hits sharing an IDENTICAL range into one entry with multiple hits', () => {
    const hits = [hit({ msgId: 'a', range: [44, 49] }), hit({ msgId: 'b', range: [44, 49] })];
    const entries = buildGutterEntries(hits);
    expect(entries).toHaveLength(1);
    expect(entries[0]!.hits.map((h) => h.msgId)).toEqual(['a', 'b']);
    expect(entries[0]!.range).toEqual([44, 49]);
  });

  it('a single-line range (start === end) is a tick, a multi-line range is not', () => {
    const hits = [hit({ msgId: 'a', range: [312, 312] }), hit({ msgId: 'b', range: [267, 285] })];
    const entries = buildGutterEntries(hits);
    const tick = entries.find((e) => e.hits[0]!.msgId === 'a')!;
    const bracket = entries.find((e) => e.hits[0]!.msgId === 'b')!;
    expect(tick.isTick).toBe(true);
    expect(bracket.isTick).toBe(false);
  });

  it('whole-file hits (range: null) are excluded entirely — the file-lane is a separate concern', () => {
    const hits = [hit({ msgId: 'a', range: null }), hit({ msgId: 'b', range: [1, 5] })];
    const entries = buildGutterEntries(hits);
    expect(entries).toHaveLength(1);
    expect(entries[0]!.hits[0]!.msgId).toBe('b');
  });

  it('non-overlapping ranges share column 0 — no need to spread them out', () => {
    const hits = [hit({ msgId: 'a', range: [10, 15] }), hit({ msgId: 'b', range: [20, 25] })];
    const entries = buildGutterEntries(hits);
    expect(entries.every((e) => e.column === 0)).toBe(true);
  });

  it('two DIFFERENT overlapping ranges get distinct columns — never blurred into one bracket', () => {
    const hits = [hit({ msgId: 'a', range: [10, 20] }), hit({ msgId: 'b', range: [15, 25] })];
    const entries = buildGutterEntries(hits);
    const cols = new Set(entries.map((e) => e.column));
    expect(cols.size).toBe(2);
  });

  it('a third range overlapping BOTH existing columns opens a third column', () => {
    const hits = [
      hit({ msgId: 'a', range: [10, 20] }),
      hit({ msgId: 'b', range: [15, 25] }),
      hit({ msgId: 'c', range: [12, 22] }),
    ];
    const entries = buildGutterEntries(hits);
    const cols = new Set(entries.map((e) => e.column));
    expect(cols.size).toBe(3);
  });

  it('a range that starts after an earlier one ENDS reuses that column — no needless spreading', () => {
    const hits = [hit({ msgId: 'a', range: [10, 15] }), hit({ msgId: 'b', range: [16, 20] })];
    const entries = buildGutterEntries(hits);
    expect(entries.every((e) => e.column === 0)).toBe(true);
  });

  it('law #3 — drift is true only when a REAL hit in the entry has staleness "changed"', () => {
    const drifted = buildGutterEntries([hit({ msgId: 'a', range: [88, 102], staleness: 'changed' })]);
    expect(drifted[0]!.drift).toBe(true);

    const current = buildGutterEntries([hit({ msgId: 'a', range: [88, 102], staleness: 'current' })]);
    expect(current[0]!.drift).toBe(false);

    // Other staleness values (moved/offline/squashed/unpinned/unknown) are
    // real facts too, but NOT "the lines drifted" — only 'changed' means
    // that specifically, so none of them should flip the drift marker.
    const offline = buildGutterEntries([hit({ msgId: 'a', range: [88, 102], staleness: 'offline' })]);
    expect(offline[0]!.drift).toBe(false);
  });

  it('an entry drifts if ANY of its (identical-range) hits genuinely drifted, even if not all did', () => {
    const hits = [
      hit({ msgId: 'a', range: [1, 5], staleness: 'changed' }),
      hit({ msgId: 'b', range: [1, 5], staleness: 'current' }),
    ];
    const entries = buildGutterEntries(hits);
    expect(entries).toHaveLength(1);
    expect(entries[0]!.drift).toBe(true);
  });

  it('no ranged hits at all produces no entries', () => {
    expect(buildGutterEntries([])).toEqual([]);
    expect(buildGutterEntries([hit({ range: null })])).toEqual([]);
  });
});

describe('gutterColumnCount', () => {
  it('is always at least 1, even with no entries', () => {
    expect(gutterColumnCount([])).toBe(1);
  });

  it('matches the highest column index used, +1', () => {
    const hits = [hit({ msgId: 'a', range: [10, 20] }), hit({ msgId: 'b', range: [15, 25] })];
    const entries = buildGutterEntries(hits);
    expect(gutterColumnCount(entries)).toBe(2);
  });
});
