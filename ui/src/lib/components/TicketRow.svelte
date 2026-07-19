<script lang="ts">
  // The "Row" tier of piece 5's ticket card trio (ui/REDESIGN.md, "the
  // composable card system") — `redesign-mockups/05-composable-cards.html`'s
  // `.t-row`: a compact scan line for a list. Deliberately minimal — dot,
  // id, summary, assignee, age, NO action buttons (a human doesn't "claim" a
  // ticket from a list row, that's an agent op) and no status stamp/topic
  // chip the way the older BoardRow carried. The whole row is one button;
  // clicking it opens the Full popover (never a dead end).
  import type { Agent, RequestRow } from '../types';
  import { formatAgeFromSecs } from '../format';
  import { ticketStateOf, ticketStateVar } from '../ticketState';
  import CopyIdButton from './CopyIdButton.svelte';

  interface Props {
    request: RequestRow;
    agents: Agent[];
    selected?: boolean;
    onSelect?: (id: string) => void;
  }

  let { request, agents, selected = false, onSelect }: Props = $props();

  const agentsById = $derived(new Map(agents.map((a) => [a.id, a])));
  const assignee = $derived(request.claimants[0] ? agentsById.get(request.claimants[0]) : undefined);
  const stateVar = $derived(ticketStateVar(ticketStateOf(request)));
</script>

<div
  class="t-row"
  class:sel={selected}
  style="--c:{stateVar}"
  role="button"
  tabindex="0"
  onclick={() => onSelect?.(request.id)}
  onkeydown={(e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onSelect?.(request.id);
    }
  }}
  data-testid="ticket-row"
>
  <span class="dot" aria-hidden="true"></span>
  <span class="id mono">{request.id}</span>
  <span class="sum">{request.summary}</span>
  {#if assignee}
    <span class="av" style="background:{assignee.color}" title={assignee.display}>{assignee.abbr}</span>
  {:else}
    <span class="av none" title="unclaimed">–</span>
  {/if}
  <span class="age mono" class:stale={request.stale}>{formatAgeFromSecs(request.ageSecs)}</span>
  <CopyIdButton id={request.id} class="t-row-copy-id" />
</div>

<style>
  .t-row {
    display: grid;
    grid-template-columns: 7px auto 1fr auto auto auto;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 7px 9px;
    border-radius: 8px;
    border: 1px solid transparent;
    background: transparent;
    text-align: left;
    font: inherit;
    font-size: 12.5px;
    color: inherit;
    cursor: pointer;
  }
  .t-row:hover,
  .t-row:focus-visible {
    background: var(--panel-2);
    border-color: var(--border);
  }
  .t-row.sel {
    background: var(--panel-2);
    border-color: var(--accent);
  }
  .t-row:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  .t-row .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--c);
    flex: 0 0 auto;
  }
  .t-row .id {
    font-size: 10.5px;
    color: var(--faint);
  }
  .t-row .sum {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
  }
  .t-row .av {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    display: grid;
    place-items: center;
    font: 700 8px/1 var(--mono);
    color: #0b0e17;
    flex: 0 0 auto;
  }
  .t-row .av.none {
    color: var(--faint);
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--border-2);
  }
  .t-row .age {
    font-size: 10.5px;
    color: var(--muted);
    white-space: nowrap;
  }
  .t-row .age.stale {
    color: var(--state-stuck);
  }
  .t-row:hover :global(.t-row-copy-id),
  .t-row:focus-within :global(.t-row-copy-id) {
    opacity: 1;
  }
</style>
