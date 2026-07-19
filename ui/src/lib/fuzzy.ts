// A small fzf-style subsequence matcher for the ⌘K command palette
// (ui/redesign-mockups/02-hub-nav.html: "fuzzy jump ... fzf-like"). Pure and
// dependency-free — every query character must appear in `target`, in
// order, but not necessarily contiguous ("orb" matches "confer-jarvis-orbit").
// Not a full fzf port (no multi-term, no smart-case config) — just enough to
// make hub-jump feel fuzzy; extend here if a later piece needs threads/
// actions in the same palette.

export interface FuzzyMatch {
  /** Higher is a better match. Consecutive-character runs and an early match
   * start score higher, so "orb" ranks "orbit" above "co-orb-ordinate". */
  score: number;
  /** Index of each matched character in `target`, for highlighting. */
  positions: number[];
}

/** Returns `null` when `query` isn't a subsequence of `target` at all. An
 * empty query matches everything with a neutral (zero) score, so a blank
 * palette input shows the full list rather than nothing.
 *
 * Two passes, the standard fzf-lite trick: a forward greedy pass finds SOME
 * valid subsequence, then a backward pass re-finds each character as late
 * as possible (working right-to-left from the forward match's end) —
 * "compacting" the match into its tightest possible window. Without this,
 * plain leftmost-greedy matching picks the FIRST "o" it finds ("codeshrew"'s
 * o) over a perfect contiguous "orb" run sitting right there in "-orbit"
 * later in the same string, and ranks the loose match as if it were tight. */
export function fuzzyMatch(query: string, target: string): FuzzyMatch | null {
  if (query.length === 0) return { score: 0, positions: [] };

  const q = query.toLowerCase();
  const t = target.toLowerCase();

  // Forward pass: is `query` a subsequence at all, and if so, roughly where
  // does it end? (Only `forward`'s last index is actually used below — as
  // the backward pass's starting bound.)
  let ti = 0;
  let forwardEnd = -1;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found === -1) return null;
    forwardEnd = found;
    ti = found + 1;
  }

  // Backward pass: walk the query in reverse, taking the LATEST occurrence
  // of each character at-or-before the current bound — pulls every match as
  // far right (and thus as tight against its neighbors) as the forward
  // match's span allows.
  const positions = new Array<number>(q.length);
  let bound = forwardEnd;
  for (let qi = q.length - 1; qi >= 0; qi--) {
    const found = t.lastIndexOf(q[qi]!, bound);
    positions[qi] = found;
    bound = found - 1;
  }

  let score = 0;
  let lastMatch = -1;
  for (const pos of positions) {
    // A contiguous run is the dominant signal — weighted well above the
    // early-start bonus below, so a tight match later in the string always
    // beats a scattered match that merely starts sooner.
    score += pos === lastMatch + 1 ? 5 : 1;
    lastMatch = pos;
  }
  // A small tie-breaking nudge for an earlier match start, capped low
  // enough that it can never outweigh even a single consecutive pair.
  score += Math.max(0, 3 - positions[0]!);

  return { score, positions };
}

/** Filters + ranks `items` by fuzzy-matching `query` against each one's
 * `text(item)`, best score first; ties keep their original relative order
 * (`Array.sort` is stable). */
export function fuzzyFilter<T>(items: T[], query: string, text: (item: T) => string): T[] {
  const scored = items
    .map((item) => ({ item, match: fuzzyMatch(query, text(item)) }))
    .filter((s): s is { item: T; match: FuzzyMatch } => s.match !== null);
  scored.sort((a, b) => b.match.score - a.match.score);
  return scored.map((s) => s.item);
}
