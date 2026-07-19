// Piece 11 (ui/redesign-mockups/11-code-view-BUILD-BRIEF.md) — shared
// derivations between the anchored reader (Phase 1, ReverseIndexPanel.svelte)
// and the powered gutter (Phase 2, CodeLens.svelte), so "color = meaning"
// means the exact same thing in both places — one definition, not two
// drifting copies (the same discipline `ticketState.ts`/`thread.ts`/
// `eventSubject.ts` already hold for their own shared vocabularies).
import type { RefHit } from './types';
import { EVENT_COLOR_VAR } from './eventSubject';

/** A `RefHit`'s accent color, by real CONVERSATION-STATE meaning (mock 12's
 * gutter palette: open=cyan, note=teal, needs-owner=amber, blocked=red,
 * resolved=grey) — NOT piece 9's lifecycle-EVENT palette, which answers a
 * different question ("what just happened" — a `done` event there is
 * "just happened, in the moment" GREEN). Code-view color means "what
 * state is this conversation in RIGHT NOW", so a resolved/closed
 * conversation is grey regardless of how it got there. `note` gets its
 * own muted teal (not an event kind — no subject to resolve); `request`
 * is colored by its REAL `requestStatus` (a simplified peer of
 * `ticketStateOf`'s own state->color story — a `RefHit` carries only the
 * raw status enum, not a full `RequestRow`); `claim`/`blocked`/`error`/
 * `defer`/`supersede` reuse `EVENT_COLOR_VAR` (piece 9) since those DO mean
 * the same thing in both palettes (in-flight/blocked/needs-owner/muted);
 * `done` is the one deliberate divergence — see below. Verified against
 * mock 12 live (Jarvis's catch: a `done` hit was reading green, making a
 * fully-resolved range look like open in-flight work). */
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
    case 'done':
      // Deliberately NOT `EVENT_COLOR_VAR.done` (green, "just happened") —
      // a resolved conversation reads the SAME grey a resolved request's
      // own DONE/SUPERSEDED status gets above.
      return 'var(--state-done)';
    default:
      return EVENT_COLOR_VAR[hit.msgType];
  }
}

/** How urgently a color demands attention — used ONLY to pick the single
 * most-actionable color when several conversations collapse into one
 * gutter entry (below). Higher wins. Unlisted colors (e.g. `--muted`,
 * `supersede`'s) default to the lowest tier via `?? 0`. */
const STATE_PRIORITY: Record<string, number> = {
  'var(--state-stuck)': 4, // blocked/error
  'var(--state-unowned)': 3, // needs-owner/defer
  'var(--state-open)': 3, // open question
  'var(--state-flight)': 2, // in-flight/claimed
  'var(--state-metric)': 1, // note
  'var(--state-done)': 0, // resolved
};

/** Piece 11 Phase 2 (post-verify fix) — the color a gutter ENTRY renders
 * as, once its hits are collapsed into one bracket/tick. Picking
 * `hits[0]`'s color alone let hit ORDER decide the entry's color — a
 * range that's really `[note, note, resolved-request, done]` (nothing
 * open) could read as green "in-flight" just because a `done` hit
 * happened to sort first. Instead: the MOST-ACTIONABLE hit wins — a range
 * NEVER reads as calm/resolved while ANY of its conversations still need
 * attention (blocked > open/needs-owner > in-flight > note > resolved). */
export function entryColorVar(entry: GutterEntry): string {
  let best = entry.hits[0]!;
  let bestPriority = STATE_PRIORITY[hitColorVar(best)] ?? 0;
  for (const hit of entry.hits.slice(1)) {
    const color = hitColorVar(hit);
    const priority = STATE_PRIORITY[color] ?? 0;
    if (priority > bestPriority) {
      best = hit;
      bestPriority = priority;
    }
  }
  return hitColorVar(best);
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
