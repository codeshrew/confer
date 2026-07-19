// The Layer-1 pane-focus engine (Ctrl+h/j/k/l, Ctrl+]/[, F6/Shift+F6) —
// ui/REDESIGN.md's keyboard-architecture pass, 2026-07-19. A single
// MODULE-level registry (same singleton pattern as stores.svelte.ts's
// `appState`), so any pane-hosting component can register/unregister itself
// without App.svelte ever needing to re-bind anything.
//
// This directly satisfies gotcha #1 (REDESIGN.md): the global keydown
// listener that calls `moveDirection`/`cycle` is bound ONCE in App.svelte's
// `<svelte:window>` and always reads `panes` live off this module — never a
// snapshot closed over at bind time — because `panes` is reassigned via the
// functional updater (`panes = [...panes, pane]`) rather than mutated
// in place, and every reader goes through the getter, not a captured local.
//
// Real DOM focus is the actual source of truth for "which pane's bare keys
// fire" (each pane's own onkeydown, scoped to its own subtree, already only
// fires when focus is inside it — pieces 2-3 already built that). This
// registry is a thin index over that: `focus(id)` moves real DOM focus onto
// the pane's root element (gotcha #3: blurs whatever had it first, so the
// OLD pane's handlers stop intercepting keys), and a single `focusin`
// listener (also bound once, in App.svelte) keeps `focusedId` in sync
// whenever focus moves for any OTHER reason (Tab, a mouse click inside a
// pane) — "clicking a pane focuses it" falls out of that for free, no
// per-pane onclick needed.
import { untrack } from 'svelte';
import { nearestPaneId, type Direction, type PaneRect } from './keys';

export interface PaneHandle {
  id: string;
  /** Shown in the focus chip. */
  label: string;
  /** The pane's own root element — focus target, and the container
   * `syncFromFocusEvent` checks `.contains()` against. */
  el: HTMLElement;
  /** Measured at CALL TIME (not cached at registration) — a pane's size
   * changes with the viewport/responsive collapse, and gotcha #4 requires
   * the geometry to reflect reality, not a stale snapshot. */
  getRect: () => PaneRect;
}

function rectOf(pane: PaneHandle) {
  return { id: pane.id, rect: pane.getRect() };
}

function createPaneFocus() {
  let panes = $state<PaneHandle[]>([]);
  let focusedId = $state<string | null>(null);

  /** Registers a pane; returns an unregister function — call from an
   * `$effect`'s cleanup (`$effect(() => { const stop = paneFocus.register(...); return stop; })`)
   * so a pane that unmounts (a view switch, a closed peek) can't be
   * Ctrl+hjkl'd into after it's gone.
   *
   * Every pane calls this from ITS OWN `$effect` (that's the whole point —
   * self-registration, App.svelte never re-binds anything). Without
   * `untrack`, Svelte's auto-tracking sees `[...panes, pane]` READ `panes`
   * inside that effect's callback, making THAT effect depend on `panes` —
   * then the very next line WRITES `panes`, which reruns every OTHER pane's
   * registration effect (they all did the same read), which writes `panes`
   * again, forever. `untrack` is exactly "read/write state without making
   * the calling effect depend on it" — it was that ping-pong that made every
   * `render(App)` in App.test.ts spin (confirmed: killed a >10min hung
   * `vitest run` that was quietly retriggering all 7 panes' effects in a
   * loop the whole time). */
  function register(pane: PaneHandle): () => void {
    untrack(() => {
      panes = [...panes, pane];
      if (focusedId === null) focusedId = pane.id;
    });
    return () => {
      untrack(() => {
        panes = panes.filter((p) => p.id !== pane.id);
        if (focusedId === pane.id) {
          focusedId = panes[0]?.id ?? null;
        }
      });
    };
  }

  function focus(id: string) {
    untrack(() => {
      const pane = panes.find((p) => p.id === id);
      if (!pane) return;
      focusedId = id;
      if (document.activeElement instanceof HTMLElement && document.activeElement !== pane.el) {
        document.activeElement.blur();
      }
      pane.el.focus();
    });
  }

  function moveDirection(direction: Direction) {
    untrack(() => {
      if (!focusedId) {
        if (panes[0]) focus(panes[0].id);
        return;
      }
      const next = nearestPaneId(panes.map(rectOf), focusedId, direction);
      if (next) focus(next);
    });
  }

  /** Ctrl+]/[ and F6/Shift+F6 — a strict ring cycle (registration order),
   * the reliable fallback when geometry has nowhere to go (e.g. only one
   * pane on an axis) or when Ctrl+hjkl itself got eaten by browser chrome. */
  function cycle(forward: boolean) {
    untrack(() => {
      if (panes.length === 0) return;
      const i = panes.findIndex((p) => p.id === focusedId);
      const nextIndex = ((i < 0 ? 0 : i) + (forward ? 1 : -1) + panes.length) % panes.length;
      focus(panes[nextIndex]!.id);
    });
  }

  /** Bound once, on `window`'s `focusin` — keeps `focusedId` truthful
   * whenever DOM focus moves for a reason THIS module didn't cause (Tab, a
   * mouse click on something inside a pane). */
  function syncFromFocusEvent(e: FocusEvent) {
    untrack(() => {
      const target = e.target;
      if (!(target instanceof HTMLElement)) return;
      const pane = panes.find((p) => p.el.contains(target));
      if (pane && pane.id !== focusedId) focusedId = pane.id;
    });
  }

  return {
    get panes() {
      return panes;
    },
    get focusedId() {
      return focusedId;
    },
    get focusedLabel(): string | null {
      return panes.find((p) => p.id === focusedId)?.label ?? null;
    },
    register,
    focus,
    moveDirection,
    cycle,
    syncFromFocusEvent,
  };
}

export type PaneFocusStore = ReturnType<typeof createPaneFocus>;

export const paneFocus: PaneFocusStore = createPaneFocus();
