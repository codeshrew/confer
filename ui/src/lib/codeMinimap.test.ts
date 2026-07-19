import { describe, expect, it } from 'vitest';
import { buildMinimapSegments, computeViewportIndicator } from './codeMinimap';
import { buildGutterEntries } from './codeGutter';
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

describe('buildMinimapSegments', () => {
  it('positions a segment proportionally to its range within the file\'s real first/last line span', () => {
    // A 1000-line file (lines 1..1000); a hit on [500,509] should sit
    // roughly halfway down, at ~1% of the total height.
    const entries = buildGutterEntries([hit({ range: [500, 509] })]);
    const [seg] = buildMinimapSegments(entries, 1, 1000);
    expect(seg!.top).toBeCloseTo(499 / 1000, 5);
    expect(seg!.height).toBeCloseTo(10 / 1000, 5);
  });

  it('respects a snippet that does NOT start at line 1 (real first/last, not an assumed 1-based file)', () => {
    const entries = buildGutterEntries([hit({ range: [48, 48] })]);
    const [seg] = buildMinimapSegments(entries, 44, 49);
    // 6-line span (44..49); line 48 is the 5th line in, so top = 4/6.
    expect(seg!.top).toBeCloseTo(4 / 6, 5);
  });

  it('floors a single-line hit\'s height so it stays a visible sliver in a large file, never rounding to zero', () => {
    const entries = buildGutterEntries([hit({ range: [500, 500] })]);
    const [seg] = buildMinimapSegments(entries, 1, 100000);
    expect(seg!.height).toBeGreaterThan(0);
    expect(seg!.height).toBeGreaterThanOrEqual(0.01);
  });

  it('one segment per gutter entry, colored by the SAME entryColorVar the gutter itself uses', () => {
    const entries = buildGutterEntries([
      hit({ msgId: 'a', range: [10, 12], msgType: 'note' }),
      hit({ msgId: 'b', range: [40, 40], msgType: 'request', requestStatus: 'BLOCKED' }),
      hit({ msgId: 'c', range: [80, 85], msgType: 'done' }),
    ]);
    const segs = buildMinimapSegments(entries, 1, 100);
    expect(segs).toHaveLength(3);
    const byRange = new Map(segs.map((s) => [`${s.range[0]}-${s.range[1]}`, s]));
    expect(byRange.get('10-12')!.color).toBe('var(--state-metric)'); // note
    expect(byRange.get('40-40')!.color).toBe('var(--state-stuck)'); // blocked
    expect(byRange.get('80-85')!.color).toBe('var(--state-done)'); // resolved
  });

  it('orders overlapping segments largest-first so a narrower one paints on top and stays clickable', () => {
    // [44,49] (6 lines) fully contains [44,46] (3 lines) and [48,48] (1 line)
    // — the exact real fixture shape (PlateBundle.swift in mock.ts). At the
    // minimap's 10px width there's no room for the gutter's own side-by-side
    // columns, so stacking order is what keeps the smaller ones clickable.
    const entries = buildGutterEntries([
      hit({ msgId: 'wide', range: [44, 49] }),
      hit({ msgId: 'mid', range: [44, 46] }),
      hit({ msgId: 'narrow', range: [48, 48] }),
    ]);
    const segs = buildMinimapSegments(entries, 44, 49);
    expect(segs.map((s) => `${s.range[0]}-${s.range[1]}`)).toEqual(['44-49', '44-46', '48-48']);
  });

  it('law #3 — no entries in, no segments out (never a fabricated band)', () => {
    expect(buildMinimapSegments([], 1, 500)).toEqual([]);
  });

  it('law #3 — an entry entirely outside the loaded line span is skipped, not clamped into a fake edge sliver', () => {
    // A genuinely `staleness: 'offline'` hit pinned to lines the currently
    // loaded snippet doesn't include (the per-line gutter already renders
    // nothing for it either — there's no `.cl` row at that line number).
    const entries = buildGutterEntries([hit({ range: [10, 15] })]);
    expect(buildMinimapSegments(entries, 44, 49)).toEqual([]);
  });

  it('an entry that PARTIALLY overlaps the loaded span is clamped to the real, visible portion', () => {
    const entries = buildGutterEntries([hit({ range: [40, 46] })]);
    const [seg] = buildMinimapSegments(entries, 44, 49);
    // Only lines 44-46 of this entry are actually loaded — clamp to that,
    // not the full [40,46] the hit itself claims.
    expect(seg!.top).toBeCloseTo(0, 5);
    expect(seg!.height).toBeCloseTo(3 / 6, 5);
  });

  it('a degenerate 0-line span (firstLine === lastLine) still positions without dividing by zero', () => {
    const entries = buildGutterEntries([hit({ range: [7, 7] })]);
    const segs = buildMinimapSegments(entries, 7, 7);
    expect(segs[0]!.top).toBe(0);
    expect(Number.isFinite(segs[0]!.height)).toBe(true);
  });
});

describe('computeViewportIndicator', () => {
  it('a scrolled-to-top, fully-visible file reads as top:0, height:1', () => {
    const v = computeViewportIndicator({ containerScrollTop: 0, containerClientHeight: 400, codeOffsetTop: 0, codeScrollHeight: 400 });
    expect(v.top).toBe(0);
    expect(v.height).toBe(1);
  });

  it('a long file scrolled halfway shows a proportionally small, offset indicator', () => {
    const v = computeViewportIndicator({ containerScrollTop: 1000, containerClientHeight: 200, codeOffsetTop: 0, codeScrollHeight: 2000 });
    expect(v.top).toBeCloseTo(0.5, 5);
    expect(v.height).toBeCloseTo(0.1, 5);
  });

  it('accounts for a non-zero codeOffsetTop (e.g. a file-lane pushing the code down within the scroll container)', () => {
    const v = computeViewportIndicator({ containerScrollTop: 40, containerClientHeight: 100, codeOffsetTop: 40, codeScrollHeight: 1000 });
    expect(v.top).toBeCloseTo(0, 5);
  });

  it('clamps to [0,1] rather than reading past either edge when scrolled beyond the measured range', () => {
    const v = computeViewportIndicator({ containerScrollTop: 5000, containerClientHeight: 200, codeOffsetTop: 0, codeScrollHeight: 1000 });
    expect(v.top).toBe(1);
    expect(v.top + v.height).toBeLessThanOrEqual(1.000001);
  });

  it('degrades honestly to "whole file in view" when nothing has been measured yet (codeScrollHeight <= 0) — not a divide-by-zero or a fabricated sliver', () => {
    const v = computeViewportIndicator({ containerScrollTop: 0, containerClientHeight: 0, codeOffsetTop: 0, codeScrollHeight: 0 });
    expect(v).toEqual({ top: 0, height: 1 });
  });
});
