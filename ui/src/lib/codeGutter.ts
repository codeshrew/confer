// Piece 11 (ui/redesign-mockups/11-code-view-BUILD-BRIEF.md) — shared
// derivations between the anchored reader (Phase 1, ReverseIndexPanel.svelte)
// and the powered gutter (Phase 2, CodeLens.svelte), so "color = meaning"
// means the exact same thing in both places — one definition, not two
// drifting copies (the same discipline `ticketState.ts`/`thread.ts`/
// `eventSubject.ts` already hold for their own shared vocabularies).
import type { RefHit } from './types';
import { EVENT_COLOR_VAR } from './eventSubject';

/** A `RefHit`'s accent color, by real meaning: `note` gets its own muted
 * teal (not an event kind — no subject to resolve); `request` is colored
 * by its REAL `requestStatus` (a simplified peer of `ticketStateOf`'s own
 * state->color story — a `RefHit` carries only the raw status enum, not a
 * full `RequestRow`); everything else (claim/done/error/blocked/defer/
 * supersede) reuses the SAME `EVENT_COLOR_VAR` palette piece 9 already
 * established — not a second invented mapping. */
export function hitColorVar(hit: RefHit): string {
  switch (hit.msgType) {
    case 'note':
      return 'var(--state-metric)';
    case 'request':
      switch (hit.requestStatus) {
        case 'CLAIMED':
          return 'var(--state-flight)';
        case 'BLOCKED':
        case 'ERROR':
          return 'var(--state-stuck)';
        case 'DONE':
        case 'SUPERSEDED':
          return 'var(--state-done)';
        default:
          return 'var(--state-open)';
      }
    default:
      return EVENT_COLOR_VAR[hit.msgType];
  }
}

/** Piece 11 Phase 2 — one gutter entry per REAL, distinct line range: hits
 * that share the EXACT same `[start, end]` collapse into ONE entry (several
 * conversations, one bracket) — never merged with a genuinely different
 * range just because it happens to touch the same lines (that's what
 * `column` is for). */
export interface GutterEntry {
  hits: RefHit[];
  range: [number, number];
  /** A single-line range (`range[0] === range[1]`) renders as a tick, not
   * a bracket — "shape = scope" (piece 11 Phase 2). */
  isTick: boolean;
  /** 0-based side-by-side column: two entries whose line spans overlap
   * never share a column, so they render as visually distinct brackets
   * instead of blurring into one. */
  column: number;
  /** Law #3 — true only when a REAL hit in this entry has drifted
   * (`staleness === 'changed'`: the pinned sha's lines have genuinely
   * moved since) — never a decorative guess. */
  drift: boolean;
}

/** Groups a file's RANGED hits (whole-file `range: null` hits are a
 * separate concern — the file-lane, not the per-line gutter) into gutter
 * entries, with overlapping-but-DIFFERENT ranges assigned distinct
 * columns via classic greedy interval coloring: sort by start line, place
 * each entry in the first column whose last-placed end line is already
 * behind it; open a new column only when none free. */
export function buildGutterEntries(hits: RefHit[]): GutterEntry[] {
  const ranged = hits.filter((h): h is RefHit & { range: [number, number] } => h.range !== null);

  const byRange = new Map<string, RefHit[]>();
  for (const h of ranged) {
    const key = `${h.range[0]}-${h.range[1]}`;
    const arr = byRange.get(key) ?? [];
    arr.push(h);
    byRange.set(key, arr);
  }

  const groups = [...byRange.values()]
    .map((groupHits) => {
      const range = groupHits[0]!.range as [number, number];
      return {
        hits: groupHits,
        range,
        isTick: range[0] === range[1],
        drift: groupHits.some((h) => h.staleness === 'changed'),
      };
    })
    .sort((a, b) => a.range[0] - b.range[0] || a.range[1] - b.range[1]);

  const columnEnds: number[] = [];
  return groups.map((g) => {
    let column = columnEnds.findIndex((end) => end < g.range[0]);
    if (column === -1) {
      column = columnEnds.length;
      columnEnds.push(g.range[1]);
    } else {
      columnEnds[column] = g.range[1];
    }
    return { ...g, column };
  });
}

/** How many side-by-side columns the gutter needs to render every entry
 * without two overlapping ranges colliding. Always at least 1 so the
 * gutter reserves its base width even for an empty/no-range file. */
export function gutterColumnCount(entries: GutterEntry[]): number {
  return Math.max(1, ...entries.map((e) => e.column + 1));
}
