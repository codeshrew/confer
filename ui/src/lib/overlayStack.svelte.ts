// Piece 10 Phase A (ui/redesign-mockups/10-navigation-history-RESEARCH.md) —
// the overlay back-stack fixing the reported bug: Fleet -> click an agent
// (AgentDossier opens) -> click one of their tickets used to REPLACE the
// dossier instead of stacking over it, so `esc` landed nowhere. A stack of
// frames instead of independent booleans lets a popover open FROM WITHIN
// another one without destroying the parent's context: `push` nests a new
// layer, `pop` unwinds exactly one.
//
// Phase A is IN-MEMORY ONLY, by design (Jarvis's task) — no URL sync
// (`syncToUrl`/`pushState`/`hydrateFromUrl`) yet; that's Phase B, which will
// ALSO need a `popstate` listener re-syncing `stack` on browser-back — the
// doc's own risk table already flags this, then its prose contradicts
// itself claiming `pushState` alone makes the back button "work
// out-of-the-box, no extra wiring needed." That claim is wrong; don't
// implement Phase B as literally written.
//
// Only NESTED overlays live here: agent-dossier / ticket / note
// (App.svelte owns the id -> component mapping). `whichKeyOpen` (help) and
// `focusReaderOpen` (the read peek) stay plain booleans — they're
// top-level/independent, not stackable layers, per the doc's own "Phase 4:
// Selective Migration" call.
export interface OverlayFrame {
  /** Which overlay this frame renders — 'agent-dossier' / 'ticket' / 'note'
   * today. */
  id: string;
  /** Visual category — only 'popover' exists in the app today; carried
   * through from the research doc's shape for when a 'drawer'-style overlay
   * joins the stack. */
  type: 'popover' | 'drawer';
  /** Context for the frame, e.g. `{ agentId }` / `{ ticketId }` / `{ msgId }`.
   * Kept to primitives (not object refs) since Phase B's URL sync needs to
   * serialize this. App.svelte still reads its OWN dedicated state
   * (`selectedRequestId`, `appState.selectedMessage`) for content that has
   * OTHER consumers beyond the popover itself — e.g. Board's row highlight
   * persists after the ticket popover closes, which the stack (emptied once
   * popped) can't hold — so `data` here is the forward-compatible mirror,
   * not yet the single source of truth for that particular content. */
  data?: Record<string, string | number | null>;
}

function createOverlayStack() {
  let stack = $state<OverlayFrame[]>([]);

  return {
    get stack(): OverlayFrame[] {
      return stack;
    },
    get top(): OverlayFrame | null {
      return stack[stack.length - 1] ?? null;
    },
    /** Nests a NEW layer on top of whatever's already showing — the "open
     * FROM WITHIN another overlay" case (dossier -> ticket, note -> ticket).
     * The frame(s) beneath stay in the stack, untouched — this is the actual
     * bug fix: the parent's context survives. */
    push(frame: OverlayFrame): void {
      stack = [...stack, frame];
    },
    /** Swaps the TOP frame's content in place — same stack depth. For "I am
     * already AT this overlay, show different content" (j/k navigating
     * between tickets/agents within the same popover) and for a fresh
     * TOP-LEVEL open (Chat/Board/Fleet — nothing else was showing, so this
     * is equivalent to `push`, matching `push`-on-an-empty-stack below). */
    replace(frame: OverlayFrame): void {
      stack = stack.length === 0 ? [frame] : [...stack.slice(0, -1), frame];
    },
    /** Unwinds exactly ONE layer — `esc`, a close button, and the "‹ back"
     * affordance all call this. A no-op on an already-empty stack. */
    pop(): void {
      if (stack.length > 0) stack = stack.slice(0, -1);
    },
    /** Empties the whole stack. Not wired to any trigger yet in Phase A —
     * part of the store's required surface (and unit-tested), available for
     * whenever a "every overlay's context just went stale" trigger (a hub
     * switch, say) needs it. */
    clear(): void {
      stack = [];
    },
  };
}

export const overlayStack = createOverlayStack();

/** Reads a string field off a frame's `data` — the small type-narrowing
 * callers need since `data`'s values are `string | number | null` (the
 * Phase-B URL-serializable shape), not always `string`. Returns `null` for
 * a missing frame, a missing key, or a non-string value. */
export function frameData(frame: OverlayFrame | null, key: string): string | null {
  const v = frame?.data?.[key];
  return typeof v === 'string' ? v : null;
}
