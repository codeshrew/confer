# Code Collapse / Range-Based Folding — Library Evaluation

## Research Summary

Evaluated whether to adopt an existing library for GitHub-PR-style line collapsing in CodeLens.svelte, or implement range-based collapse natively.

**Findings:**
- **Shiki transformers** provide line-level visual annotations (diff notation, focus, highlight) but NOT collapse/expand logic. Not applicable.
- **CodeMirror 6** offers full code folding but requires loading entire editor machinery (93KB gzipped basic-setup), would discard custom line-div rendering and the per-line conversation-density gutter. **Overkill.**
- **diff2html / react-diff-view** work at hunk/file level, designed for diff viewing, not ref-range-driven folding within a single file. UX pattern (expandable hunks) is reference-worthy but not the tool.
- **Roll-our-own:** CodeLens already owns line-by-line rendering, full ref ranges (`RefHit[]` with `range: [start,end] | null`), and density gutter. Collapse logic is straightforward: compute collapsed gaps, render "⋯ expand N lines" affordances, toggle show-all flag.

---

## Options Table

| Option | Gives PR-Collapse? | Keeps Shiki + Density Gutter? | Svelte 5 Fit | Bundle Cost | Effort | Notes |
|--------|:--:|:--:|:--:|:--:|:--:|---------|
| **Shiki transformers** | ❌ (annotations only) | ✅ | ✅ | 0 | 0h | Transformers highlight lines; no collapse logic. Ruled out. |
| **CodeMirror 6 folding** | ✅ | ❌ (full editor replacement) | ⚠️ (need adapter) | +93KB | 16h+ | Loses custom line rendering, gutter. Bloat for read-only view. |
| **diff2html** | ❌ (file/hunk level) | ✅ | ⚠️ (JS lib, manual bind) | +40KB | 12h | Designed for diffs, not ref ranges. UX pattern useful, tool overkill. |
| **react-diff-view** | ❌ (hunk-based gaps) | ✅ | ❌ (React, need port) | +15KB | 20h+ | Hunk-based line counting, not ref-driven. React-to-Svelte port cost high. |
| **Roll-our-own** | ✅ | ✅ | ✅ | 0 | 4-6h | Own the line list + ref ranges already. Simple gap detection + expand affordances. |

---

## Recommendation: Roll Our Own

**Rationale:**

1. **You already own the data:** CodeLens has `hitsByLine: Map<number, RefHit[]>` and the full `fileRefs: RefHit[]` (including whole-file `range:null` hits). Each `RefHit` carries a `range: [start, end] | null`. No need to decode hunks or re-parse diffs.

2. **Custom features are worth keeping:** The per-line conversation-density gutter (heat coloring by ref count) is unique to confer and adds signal. CodeMirror would discard it; diff viewers don't support it.

3. **Bundle stays lean:** Zero dependency addition. Shiki/markdown-it/dompurify remain the only runtime deps.

4. **Svelte 5 runes fit naturally:** Collapse state is a single `$state(showAllLines: boolean)`, ref-range-derived collapsed spans are a `$derived` computation, affordance click handlers are local event listeners.

5. **Bounded scope:** The operator wants both focused and full-code views. Toggle is trivial; all the logic lives in line-render loop.

---

## Algorithm Sketch (10 lines)

```typescript
// 1. Compute collapsed spans (gaps between ref ranges + context buffer)
function getCollapsedSpans(refs: RefHit[], context = 2): Array<[start, end]> {
  const covered = refs.flatMap(r => r.range ? [r.range] : []);
  const withContext = covered.flatMap(([a, b]) => [[a - context, b + context]]);
  const merged = mergeRanges(withContext);  // coalesce overlapping ranges
  const allLines = Math.max(...covered.map(r => r[1]));
  return gaps(merged, allLines);  // inverse: gaps between merged ranges
}

// 2. Render loop decision (per line):
// if (showAllLines || isInRefRange || isInContextRange) { render line }
// else if (isGapStart) { render `<button>⋯ expand N lines</button>` }
// else { skip (collapsed) }

// 3. Click handler: toggle range in local Set<[start,end]> to expand that gap
// 4. Show-all toggle: set showAllLines = !showAllLines (single flag flip)
```

**Gotchas to handle:**

- **Scroll anchor:** When expanding a gap, preserve viewport scroll position (measure element offset pre-expand, re-focus post-expand).
- **Keyboard nav:** If user tabs into an "expand" button inside a collapsed section, auto-scroll that section into view.
- **Density gutter line numbers:** When gaps are collapsed, gutter must show line numbers that skip the hidden range (e.g., 10–15 hidden → next visible is 16). Track this in the render loop.
- **Very large files (10K+ lines):** Batching/windowing not needed for initial MVP; if profiling shows lag, virtualize the line list (only render visible viewport + small buffer).

---

## Next Steps

1. Add `showAllLines: boolean` and `expandedGaps: Set<[number, number]>` runes to CodeLens.svelte.
2. Derive `collapsedSpans` from `fileRefs` (above algorithm).
3. Update line-render loop to conditionally skip collapsed lines and inject expand affordances.
4. Density gutter: adjust line-number rendering to account for collapsed ranges.
5. E2E test: expand a gap, verify lines appear; toggle show-all, verify all lines appear; scroll into expanded section on keyboard focus.

