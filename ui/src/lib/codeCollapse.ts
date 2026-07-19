// Piece 11 Phase 3 (ui/redesign-mockups/11-code-view-BUILD-BRIEF.md +
// 11-code-collapse-RESEARCH.md + mock 11's `.fold` rows) — PR-style
// collapse: referenced ranges + a little context stay open, everything else
// folds to an "⋯ expand N lines" row. Roll-our-own per the research doc
// (Shiki has no fold logic; CodeMirror 6 would discard the custom line
// rendering + density gutter for +93KB). Pure span/gap math here, kept
// separate from `CodeLens.svelte`'s DOM/scroll concerns so it's
// unit-testable without a layout engine — same split as `codeGutter.ts`/
// `codeMinimap.ts`.
import type { GutterEntry } from './codeGutter';

/** Lines of real code kept visible immediately around every referenced
 * range even when collapsed, per the research doc's own recommendation —
 * enough to read a signature or closing brace without expanding. */
export const COLLAPSE_CONTEXT = 2;

/** How many lines a single ↑/↓ click reveals from a middle gap's edge —
 * mock 11's own `↑8`/`↓8`. */
export const PARTIAL_EXPAND_STEP = 8;

export type Span = [number, number]; // inclusive line range

/** Merges each gutter entry's range (expanded by `context` lines on both
 * sides, clamped to the loaded file) into the spans that stay open by
 * default. Adjacent/overlapping expanded ranges coalesce into one span so
 * two nearby conversations don't leave a pointless sliver of a fold between
 * them. An entry whose range doesn't overlap `[firstLine, lastLine]` at all
 * (same law-#3 case `codeMinimap.ts` already handles — e.g. a genuinely
 * `staleness: 'offline'` ref to lines this file's loaded content doesn't
 * have) contributes no span rather than a degenerate inverted one. */
export function computeOpenSpans(entries: GutterEntry[], firstLine: number, lastLine: number, context = COLLAPSE_CONTEXT): Span[] {
  const raw = entries
    .map((e): Span => [Math.max(firstLine, e.range[0] - context), Math.min(lastLine, e.range[1] + context)])
    .filter(([start, end]) => start <= end)
    .sort((a, b) => a[0] - b[0]);

  const merged: Span[] = [];
  for (const [start, end] of raw) {
    const last = merged[merged.length - 1];
    if (last && start <= last[1] + 1) {
      last[1] = Math.max(last[1], end);
    } else {
      merged.push([start, end]);
    }
  }
  return merged;
}

export interface CollapsedGap {
  start: number;
  end: number;
  /** `top`/`bottom` gaps touch a real file edge — there's no adjacent open
   * content to step toward, so a single button reveals the whole gap
   * (mock 11's bare `↑ top` / `↓ bottom`, no `↑8`/`↓8`/`⤢all`). `middle`
   * gaps sit between two open spans and get the full partial-reveal set. */
  edge: 'top' | 'bottom' | 'middle';
}

/** The complement of `openSpans` within `[firstLine, lastLine]` — the
 * collapsible gaps. Deliberately returns NO gaps at all when `openSpans` is
 * empty (nothing referenced in the loaded file) rather than one giant
 * whole-file fold — a file nobody's discussed yet should just show its
 * code, not demand a click before you can read a single line. Collapse
 * only kicks in once there's at least one referenced range to collapse
 * AROUND. */
export function computeGaps(openSpans: Span[], firstLine: number, lastLine: number): CollapsedGap[] {
  if (openSpans.length === 0) return [];
  const gaps: CollapsedGap[] = [];
  let cursor = firstLine;
  for (const [start, end] of openSpans) {
    if (start > cursor) gaps.push(makeGap(cursor, start - 1, firstLine, lastLine));
    cursor = Math.max(cursor, end + 1);
  }
  if (cursor <= lastLine) gaps.push(makeGap(cursor, lastLine, firstLine, lastLine));
  return gaps;
}

function makeGap(start: number, end: number, firstLine: number, lastLine: number): CollapsedGap {
  const edge: CollapsedGap['edge'] = start === firstLine ? 'top' : end === lastLine ? 'bottom' : 'middle';
  return { start, end, edge };
}

/** A stable key for a gap's ORIGINAL bounds — `computeGaps` always
 * recomputes the same base gaps from the real refs on every render,
 * independent of how much has been revealed, so this is safe to use as a
 * `Map` key across renders (and as the identity a click handler reports
 * back through). */
export function gapKey(gap: Pick<CollapsedGap, 'start' | 'end'>): string {
  return `${gap.start}-${gap.end}`;
}

/** The currently-hidden sub-range of a gap, after any partial reveal.
 * `hiddenStart > hiddenEnd` means the gap has been fully revealed (chipped
 * away from both edges, or a single `⤢all`/edge click) — a valid empty
 * range, not a sentinel to special-case. Absent from the reveal map means
 * "nothing revealed yet" (the gap's own original bounds). */
export interface RevealState {
  hiddenStart: number;
  hiddenEnd: number;
}

export type Row =
  | { kind: 'line'; n: number }
  | {
      kind: 'fold';
      /** `gapKey` of the gap's ORIGINAL bounds — pass this straight back
       * into `revealFromTop`/`revealFromBottom`/`revealAll` below. */
      key: string;
      edge: CollapsedGap['edge'];
      hiddenStart: number;
      hiddenEnd: number;
    };

/** The render loop's row plan: every loaded line is either shown (`line`)
 * or folded into a single collapsed-span row (`fold`) — a gap fully
 * revealed (via repeated `↑8`/`↓8`, or one `⤢all`/edge click) produces NO
 * fold row at all, its lines rendered like any other. Skipped lines are
 * simply never added to the plan (never rendered then hidden via CSS) —
 * the large-file gotcha the research doc flags. `showAll` (or no gaps at
 * all) bypasses collapse entirely. */
export function planRows(lineNumbers: number[], gaps: CollapsedGap[], reveal: Map<string, RevealState>, showAll: boolean): Row[] {
  if (showAll || gaps.length === 0) return lineNumbers.map((n) => ({ kind: 'line', n }));

  const hiddenByStart = new Map<number, { key: string; start: number; end: number; edge: CollapsedGap['edge'] }>();
  for (const gap of gaps) {
    const key = gapKey(gap);
    const r = reveal.get(key);
    const start = r ? r.hiddenStart : gap.start;
    const end = r ? r.hiddenEnd : gap.end;
    if (start <= end) hiddenByStart.set(start, { key, start, end, edge: gap.edge });
  }

  const rows: Row[] = [];
  let skipUntil = -1;
  for (const n of lineNumbers) {
    if (n <= skipUntil) continue;
    const hidden = hiddenByStart.get(n);
    if (hidden) {
      rows.push({ kind: 'fold', key: hidden.key, edge: hidden.edge, hiddenStart: hidden.start, hiddenEnd: hidden.end });
      skipUntil = hidden.end;
    } else {
      rows.push({ kind: 'line', n });
    }
  }
  return rows;
}

/** Applies an `↑8`-style click: reveal up to `step` more lines from the TOP
 * of whatever's still hidden — pure row-relative math, no need for the
 * gap's original bounds. Capped at what's actually left: clicking `↑8` on
 * a 3-line remainder reveals exactly 3 (`hiddenStart` pushed past
 * `hiddenEnd`, the "fully revealed" empty range), never a phantom 5 more. */
export function revealFromTop(hiddenStart: number, hiddenEnd: number, step = PARTIAL_EXPAND_STEP): RevealState {
  return { hiddenStart: Math.min(hiddenStart + step, hiddenEnd + 1), hiddenEnd };
}

/** The `↓8` mirror of `revealFromTop`. */
export function revealFromBottom(hiddenStart: number, hiddenEnd: number, step = PARTIAL_EXPAND_STEP): RevealState {
  return { hiddenStart, hiddenEnd: Math.max(hiddenEnd - step, hiddenStart - 1) };
}

/** `⤢all` (a middle gap), or the single button on a `top`/`bottom` edge gap
 * — reveal everything left in one step. Takes `hiddenStart` too (unused)
 * purely to keep the same call shape as `revealFromTop`/`revealFromBottom`
 * at every call site. */
export function revealAll(_hiddenStart: number, hiddenEnd: number): RevealState {
  return { hiddenStart: hiddenEnd + 1, hiddenEnd };
}
