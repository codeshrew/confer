// design/43 Thread 1 — per-view rail matrix, as pure functions so the
// visibility/default-mode logic is unit-testable without mounting App.svelte.
// App.svelte is the only caller; it wires these against live state
// (appState.view, selectedRequestId, whether Code has an active file).
import type { View } from './stores.svelte';

// piece 5 (ui/REDESIGN.md) retired 'request': the ticket detail view moved
// from a right-rail panel (RequestDetail.svelte) to a centered overlay
// (TicketFullPopover.svelte, reachable from Chat and Board alike) that's
// entirely independent of the right rail's contextMode. Selecting a ticket
// on Board now shows the SAME meta-thread reference graph Chat shows for
// any selection — one fewer bespoke mode, and Board gets the reference
// graph for free.
export type ContextMode = 'meta' | 'refs';

/** The right rail's legal default `contextMode` per view. App.svelte resets
 * to this whenever `appState.view` changes — killing the "stale mode leaks
 * into the next view" class of bug. */
export function defaultContextMode(view: View): ContextMode {
  switch (view) {
    case 'code':
      return 'refs';
    default:
      return 'meta';
  }
}

/** Left rail (navigator): visible on every view except Repos and Overview,
 * whose centers ARE the collection (a full-width card grid / triage lanes)
 * — a duplicate topic list next to it buys nothing. Overview is also
 * cross-hub, so a single-hub topic list would be actively wrong there. */
export function leftRailVisible(view: View): boolean {
  return view !== 'repos' && view !== 'overview';
}

/** Whether the left rail's fleet-roster section should render. False only
 * on Fleet, where the center pane already IS the roster 20px away — Chat,
 * Board, and Code all keep it as primary nav (Code's own tree replacement
 * is Phase B, out of scope here). */
export function showFleetSection(view: View): boolean {
  return view !== 'fleet';
}

/** The ⓘ crumb toggle that opens the right rail as a mobile drawer — hidden
 * entirely only on views with no inspector concept at all (Fleet/Repos/
 * Overview). Board and Code keep the toggle even before anything is
 * selected. Overview drills OUT to a hub's own views instead of opening an
 * inspector alongside itself. */
export function rightRailToggleVisible(view: View): boolean {
  return view !== 'fleet' && view !== 'repos' && view !== 'overview';
}

export interface RightRailParams {
  view: View;
  /** Board: a request/row is selected. Code: a file is active. Ignored for
   * `chat` (always open) and `fleet`/`repos` (always hidden). */
  hasSelection: boolean;
}

/** Whether the right rail should occupy layout space at all — drives both
 * the grid-column collapse (`--rail-r-w: 0px`) and the pane's own
 * `visibility`. Chat is always open (with an EmptyState when nothing's
 * selected); Board/Code stay collapsed until there's something to inspect,
 * then slide open; Fleet/Repos never show it. */
export function rightRailVisible({ view, hasSelection }: RightRailParams): boolean {
  switch (view) {
    case 'chat':
      return true;
    case 'board':
    case 'code':
      return hasSelection;
    case 'fleet':
    case 'repos':
    case 'overview':
    default:
      return false;
  }
}
