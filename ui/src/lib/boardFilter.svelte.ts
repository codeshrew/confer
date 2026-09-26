// Board's combined filter state (piece 5c, ui/REDESIGN.md) — a module
// singleton (same pattern as paneFocus.svelte.ts/readState.svelte.ts) so
// two SIBLING components in App.svelte's template — Board.svelte (the
// center cockpit, owns the stat-card and workload-bar clicks) and
// BoardFleetRail.svelte (the left rail replacing the topic list on Board,
// owns the per-agent clicks) — can toggle and read the SAME two filter
// dimensions without App.svelte threading state between them.
//
// Three dimensions, combined with AND (piece 5c: "filters combine"):
// `stateFilter` (from a stat card — piece 5b), `agentFilter` (from a
// workload-bar row OR the fleet rail — piece 5c), and `projectFilter` (the
// effective-`project` dropdown on Board.svelte). Deliberately simple
// module-level `$state` — no `untrack()` needed here (unlike paneFocus's
// registration effects): every mutation below is a plain onclick/onchange
// handler, never an `$effect` reading the same state it writes, so there's
// no feedback-loop risk to guard against.
import type { TicketState } from './ticketState';

function createBoardFilter() {
  let stateFilter = $state<TicketState | null>(null);
  let agentFilter = $state<string | null>(null);
  let projectFilter = $state<string | null>(null);

  /** Clicking "Open" (`'activeWork'`, the total-live stat, not a disjoint
   * bucket) clears the state filter — it has no narrower group to drill
   * to. Clicking the ALREADY-active state clears it too (a toggle, not a
   * one-way drill). */
  function toggleState(state: TicketState | 'activeWork'): void {
    stateFilter = state === 'activeWork' ? null : stateFilter === state ? null : state;
  }

  function toggleAgent(agentId: string): void {
    agentFilter = agentFilter === agentId ? null : agentId;
  }

  /** Sets the project dimension directly (a `<select>`'s value, not a
   * click-toggle) — `null` (or the empty-option value) clears it. */
  function setProject(project: string | null): void {
    projectFilter = project;
  }

  function clearAll(): void {
    stateFilter = null;
    agentFilter = null;
    projectFilter = null;
  }

  return {
    get stateFilter() {
      return stateFilter;
    },
    get agentFilter() {
      return agentFilter;
    },
    get projectFilter() {
      return projectFilter;
    },
    get active(): boolean {
      return stateFilter !== null || agentFilter !== null || projectFilter !== null;
    },
    toggleState,
    toggleAgent,
    setProject,
    clearAll,
  };
}

export type BoardFilterStore = ReturnType<typeof createBoardFilter>;

export const boardFilter: BoardFilterStore = createBoardFilter();
