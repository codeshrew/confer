// Piece 11 Phase 2b (ui/redesign-mockups/11-code-view-BUILD-BRIEF.md +
// 12-code-gutter-and-time.html's `.minimap`) — the conversation minimap: the
// whole file compressed to a thin strip, one colored segment per gutter
// entry, positioned proportionally. Pairs with Phase 2's `codeGutter.ts`
// (reuses `GutterEntry`/`entryColorVar` verbatim — "color = meaning" must
// stay ONE definition, not a second palette drifting from the gutter's) and
// with the not-yet-built Phase 3 collapse: this is built off the FULL file's
// hit set, not whatever's currently scrolled into view, so folded regions
// are never invisible — the minimap's whole reason to exist.
import { entryColorVar, type GutterEntry } from './codeGutter';

export interface MinimapSegment {
  range: [number, number];
  /** 0–1 fraction of the file's line span (`firstLine`..`lastLine`). */
  top: number;
  /** 0–1 fraction, floored so a single-line hit stays a visible sliver even
   * in a very long file, rather than rounding away to nothing. */
  height: number;
  color: string;
}

/** A one-line hit would round to ~0% height in a large file — floor it to a
 * hairline that's still clickable/visible, same spirit as the gutter's own
 * tick (a single line still gets real screen space, not zero). */
const MIN_SEGMENT_HEIGHT = 0.01;

/** Piece 11 Phase 2b — one minimap segment per gutter entry that actually
 * intersects the loaded file (the SAME entries the per-line gutter renders
 * a bracket/tick for, `codeGutter.ts`'s `buildGutterEntries` output),
 * positioned by real line numbers against the file's real line span.
 *
 * An entry whose range falls entirely outside `[firstLine, lastLine]` is
 * skipped, not clamped into a fake edge sliver — that's a hit pinned to
 * lines this file's currently-loaded content doesn't have (e.g. a
 * genuinely `staleness: 'offline'` ref to a since-discarded branch), and
 * the per-line gutter already renders it as nothing for the same reason
 * (no `.cl` row exists at that line number to hang a bracket off). A
 * PARTIALLY overlapping entry is clamped to the visible span rather than
 * dropped — law #3 is "don't fabricate", not "don't show the part that's
 * real". */
export function buildMinimapSegments(entries: GutterEntry[], firstLine: number, lastLine: number): MinimapSegment[] {
  const span = Math.max(1, lastLine - firstLine + 1);
  const segments: MinimapSegment[] = [];
  for (const entry of entries) {
    const start = Math.max(entry.range[0], firstLine);
    const end = Math.min(entry.range[1], lastLine);
    if (start > end) continue; // no real overlap with what's actually loaded
    const top = (start - firstLine) / span;
    const rawHeight = (end - start + 1) / span;
    segments.push({ range: entry.range, top, height: Math.max(rawHeight, MIN_SEGMENT_HEIGHT), color: entryColorVar(entry) });
  }
  // Overlapping ranges (the same "column = overlap" case the gutter renders
  // as side-by-side brackets) collapse onto ONE narrow strip here — there's
  // no room for columns at 10px wide. Render largest-first, smallest-last
  // instead: the smaller segment then paints on top and stays clickable,
  // rather than a wide entry silently swallowing every click meant for a
  // shorter one nested inside it.
  return segments.sort((a, b) => b.height - a.height);
}

export interface ViewportIndicator {
  /** 0–1 fraction of the code's total scrollable height. */
  top: number;
  /** 0–1 fraction. */
  height: number;
}

/** Pure geometry — kept separate from the DOM measurements that feed it
 * (`CodeLens.svelte`'s scroll/resize listeners) so the math itself is
 * unit-testable without a real layout engine. `codeScrollHeight <= 0`
 * (nothing measured yet, e.g. before mount, or in a test environment with
 * no real layout) degrades honestly to "the whole file is in view" rather
 * than dividing by zero or fabricating a sliver. */
export function computeViewportIndicator(params: {
  containerScrollTop: number;
  containerClientHeight: number;
  codeOffsetTop: number;
  codeScrollHeight: number;
}): ViewportIndicator {
  const { containerScrollTop, containerClientHeight, codeOffsetTop, codeScrollHeight } = params;
  if (codeScrollHeight <= 0) return { top: 0, height: 1 };
  const rawTop = (containerScrollTop - codeOffsetTop) / codeScrollHeight;
  const top = Math.min(Math.max(rawTop, 0), 1);
  const rawHeight = containerClientHeight / codeScrollHeight;
  const height = Math.min(Math.max(rawHeight, 0), 1 - top);
  return { top, height };
}
