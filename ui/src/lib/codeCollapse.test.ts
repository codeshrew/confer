import { describe, expect, it } from 'vitest';
import {
  computeGaps,
  computeOpenSpans,
  gapKey,
  planRows,
  revealAll,
  revealFromBottom,
  revealFromTop,
  type CollapsedGap,
  type RevealState,
} from './codeCollapse';
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

describe('computeOpenSpans', () => {
  it('expands a single entry by the context on both sides, clamped to the file', () => {
    const entries = buildGutterEntries([hit({ range: [50, 60] })]);
    expect(computeOpenSpans(entries, 1, 1000, 2)).toEqual([[48, 62]]);
  });

  it('clamps context at the file edges rather than reading past them', () => {
    const entries = buildGutterEntries([hit({ range: [1, 3] })]);
    expect(computeOpenSpans(entries, 1, 1000, 2)).toEqual([[1, 5]]);
  });

  it('merges two entries whose CONTEXT-expanded ranges touch or overlap into one span', () => {
    const entries = buildGutterEntries([
      hit({ msgId: 'a', range: [10, 15] }),
      hit({ msgId: 'b', range: [18, 20] }), // gap of 2 lines (16,17) — context=2 bridges it exactly
    ]);
    expect(computeOpenSpans(entries, 1, 100, 2)).toEqual([[8, 22]]);
  });

  it('keeps two entries separate when a real gap remains after context', () => {
    const entries = buildGutterEntries([
      hit({ msgId: 'a', range: [10, 12] }),
      hit({ msgId: 'b', range: [30, 32] }),
    ]);
    expect(computeOpenSpans(entries, 1, 100, 2)).toEqual([
      [8, 14],
      [28, 34],
    ]);
  });

  it('law #3 — an entry entirely outside the loaded file contributes no span (never an inverted one)', () => {
    const entries = buildGutterEntries([hit({ range: [10, 15] })]);
    expect(computeOpenSpans(entries, 44, 49, 2)).toEqual([]);
  });

  it('no entries at all -> no open spans', () => {
    expect(computeOpenSpans([], 1, 100)).toEqual([]);
  });
});

describe('computeGaps', () => {
  it('a file with no open spans (nothing referenced) has NO gaps — show the code, not one giant fold', () => {
    expect(computeGaps([], 1, 100)).toEqual([]);
  });

  it('a single open span in the middle produces a top gap and a bottom gap', () => {
    const gaps = computeGaps([[40, 60]], 1, 100);
    expect(gaps).toEqual([
      { start: 1, end: 39, edge: 'top' },
      { start: 61, end: 100, edge: 'bottom' },
    ]);
  });

  it('an open span touching line 1 produces no top gap', () => {
    const gaps = computeGaps([[1, 20]], 1, 100);
    expect(gaps).toEqual([{ start: 21, end: 100, edge: 'bottom' }]);
  });

  it('an open span touching the last line produces no bottom gap', () => {
    const gaps = computeGaps([[80, 100]], 1, 100);
    expect(gaps).toEqual([{ start: 1, end: 79, edge: 'top' }]);
  });

  it('two open spans produce a real MIDDLE gap between them', () => {
    const gaps = computeGaps(
      [
        [1, 20],
        [80, 100],
      ],
      1,
      100
    );
    expect(gaps).toEqual([{ start: 21, end: 79, edge: 'middle' }]);
  });

  it('an open span covering the whole file leaves no gaps at all', () => {
    expect(computeGaps([[1, 100]], 1, 100)).toEqual([]);
  });
});

describe('planRows', () => {
  const lineNumbers = Array.from({ length: 20 }, (_, i) => i + 1); // 1..20

  it('showAll bypasses collapse entirely, real lines, no fold rows', () => {
    const gaps: CollapsedGap[] = [{ start: 5, end: 15, edge: 'middle' }];
    const rows = planRows(lineNumbers, gaps, new Map(), true);
    expect(rows).toEqual(lineNumbers.map((n) => ({ kind: 'line', n })));
  });

  it('no gaps at all -> every line renders (never rendered-then-hidden)', () => {
    const rows = planRows(lineNumbers, [], new Map(), false);
    expect(rows).toHaveLength(20);
    expect(rows.every((r) => r.kind === 'line')).toBe(true);
  });

  it('a fresh gap (no reveal state yet) folds to exactly ONE row for the whole hidden span', () => {
    const gaps: CollapsedGap[] = [{ start: 5, end: 15, edge: 'middle' }];
    const rows = planRows(lineNumbers, gaps, new Map(), false);
    // Lines 1-4 visible, one fold row for 5-15, lines 16-20 visible.
    expect(rows.filter((r) => r.kind === 'line').map((r) => (r as { n: number }).n)).toEqual([1, 2, 3, 4, 16, 17, 18, 19, 20]);
    const fold = rows.find((r) => r.kind === 'fold');
    expect(fold).toMatchObject({ kind: 'fold', edge: 'middle', hiddenStart: 5, hiddenEnd: 15 });
  });

  it('never renders a collapsed line then hides it — collapsed lines are simply absent from the plan', () => {
    const gaps: CollapsedGap[] = [{ start: 5, end: 15, edge: 'middle' }];
    const rows = planRows(lineNumbers, gaps, new Map(), false);
    const lineNs = rows.filter((r) => r.kind === 'line').map((r) => (r as { n: number }).n);
    for (let n = 5; n <= 15; n++) expect(lineNs).not.toContain(n);
  });

  it('a fully-revealed gap (reveal state empty range) produces no fold row — its lines render normally', () => {
    const gaps: CollapsedGap[] = [{ start: 5, end: 15, edge: 'middle' }];
    const reveal = new Map<string, RevealState>([[gapKey(gaps[0]!), { hiddenStart: 16, hiddenEnd: 15 }]]); // start > end
    const rows = planRows(lineNumbers, gaps, reveal, false);
    expect(rows.filter((r) => r.kind === 'fold')).toHaveLength(0);
    expect(rows).toHaveLength(20);
  });

  it('a partially-revealed gap shows the newly-visible edge lines as real rows plus a SHRUNK fold', () => {
    const gaps: CollapsedGap[] = [{ start: 5, end: 15, edge: 'middle' }];
    const reveal = new Map<string, RevealState>([[gapKey(gaps[0]!), { hiddenStart: 9, hiddenEnd: 15 }]]); // top 4 lines (5-8) revealed
    const rows = planRows(lineNumbers, gaps, reveal, false);
    const lineNs = rows.filter((r) => r.kind === 'line').map((r) => (r as { n: number }).n);
    expect(lineNs).toEqual(expect.arrayContaining([5, 6, 7, 8]));
    const fold = rows.find((r) => r.kind === 'fold');
    expect(fold).toMatchObject({ hiddenStart: 9, hiddenEnd: 15 });
  });

  it('multiple independent gaps each fold separately, keyed by their own bounds', () => {
    const gaps: CollapsedGap[] = [
      { start: 1, end: 3, edge: 'top' },
      { start: 10, end: 12, edge: 'middle' },
      { start: 18, end: 20, edge: 'bottom' },
    ];
    const rows = planRows(lineNumbers, gaps, new Map(), false);
    expect(rows.filter((r) => r.kind === 'fold')).toHaveLength(3);
  });
});

describe('revealFromTop / revealFromBottom / revealAll', () => {
  it('revealFromTop chips the step off the top edge, shrinking the hidden range', () => {
    expect(revealFromTop(5, 15, 8)).toEqual({ hiddenStart: 13, hiddenEnd: 15 });
  });

  it('revealFromTop caps at what remains — never reveals past the gap', () => {
    expect(revealFromTop(13, 15, 8)).toEqual({ hiddenStart: 16, hiddenEnd: 15 }); // 3 lines left -> fully revealed
  });

  it('revealFromBottom chips the step off the bottom edge', () => {
    expect(revealFromBottom(5, 15, 8)).toEqual({ hiddenStart: 5, hiddenEnd: 7 });
  });

  it('revealFromBottom caps at what remains', () => {
    expect(revealFromBottom(5, 7, 8)).toEqual({ hiddenStart: 5, hiddenEnd: 4 }); // fully revealed
  });

  it('revealAll empties the hidden range in one step regardless of size', () => {
    expect(revealAll(5, 15)).toEqual({ hiddenStart: 16, hiddenEnd: 15 });
  });

  it('alternating top/bottom reveals converge to fully revealed without overlapping or double-counting', () => {
    let state = revealFromTop(1, 20, 8); // 1..8 revealed -> hidden 9..20
    expect(state).toEqual({ hiddenStart: 9, hiddenEnd: 20 });
    state = revealFromBottom(state.hiddenStart, state.hiddenEnd, 8); // hidden 9..20 -> reveal 13..20 -> hidden 9..12
    expect(state).toEqual({ hiddenStart: 9, hiddenEnd: 12 });
    state = revealFromTop(state.hiddenStart, state.hiddenEnd, 8); // only 4 left -> fully revealed
    expect(state.hiddenStart).toBeGreaterThan(state.hiddenEnd);
  });
});
