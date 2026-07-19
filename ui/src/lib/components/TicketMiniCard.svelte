<script lang="ts">
  // The "Mini card" tier of piece 5's ticket card trio (ui/REDESIGN.md, "the
  // composable card system") — `redesign-mockups/05-composable-cards.html`'s
  // `.t-mini`: a rich but small embed (kind tag + id + age + assignee, a
  // 2-line summary, a mini 3-node progress + state label). Replaces
  // TicketCard.svelte's old role embedding a request inline in the chat
  // stream (Message.svelte) — same integration point, new shape. Selectable
  // (`.sel` ring) and PORTALS into the Full popover on click — never a dead
  // end, per the composable-card rationale ("mini = a portal").
  import type { Agent, RequestRow } from '../types';
  import { formatAgeFromSecs } from '../format';
  import { ticketStateLabel, ticketStateOf, ticketStateVar } from '../ticketState';

  interface Props {
    request: RequestRow;
    agents: Agent[];
    selected?: boolean;
    onSelect?: (id: string) => void;
  }

  let { request, agents, selected = false, onSelect }: Props = $props();

  const agentsById = $derived(new Map(agents.map((a) => [a.id, a])));
  const assignee = $derived(request.claimants[0] ? agentsById.get(request.claimants[0]) : undefined);
  const state = $derived(ticketStateOf(request));
  const stateVar = $derived(ticketStateVar(state));
  const stateLabel = $derived(ticketStateLabel(state));

  // The mini 3-node progress: Requested is always reached (a RequestRow
  // only exists once filed); Claimed/Done fill in from the current state —
  // same "no history, just current status" projection limit the old
  // TicketCard's track had, now expressed through the shared TicketState
  // rather than a locally-reinvented mapping.
  type Knob = 'on' | 'cur' | 'off';
  const knobs = $derived.by((): [Knob, Knob, Knob] => {
    switch (state) {
      case 'unowned':
      case 'open':
        return ['on', 'off', 'off'];
      case 'flight':
        return ['on', 'cur', 'off'];
      case 'done':
        return ['on', 'on', 'on'];
      case 'stuck':
        // A stuck ticket's branch point depends on whether it was ever
        // claimed — request.claimants is the only signal a RequestRow
        // carries for that without a full trail fetch (the Full popover's
        // buildLifecycleTrack does the real per-message reconstruction;
        // this mini progress is deliberately cheaper).
        return request.claimants.length > 0 ? ['on', 'cur', 'off'] : ['on', 'off', 'off'];
    }
  });
</script>

<button
  type="button"
  class="t-mini"
  class:sel={selected}
  style="--c:{stateVar}"
  onclick={() => onSelect?.(request.id)}
  data-testid="ticket-mini"
>
  <div class="top">
    <span class="kd">req</span>
    <span class="id mono">{request.id}</span>
    <span class="age mono">{formatAgeFromSecs(request.ageSecs)}</span>
    {#if assignee}
      <span class="av" style="background:{assignee.color}" title={assignee.display}>{assignee.abbr}</span>
    {:else}
      <span class="av none" title="unclaimed">–</span>
    {/if}
  </div>
  <div class="sum">{request.summary}</div>
  <div class="mini-prog">
    {#each knobs as knob, i (i)}
      <span class="k" class:on={knob === 'on'} class:cur={knob === 'cur'}></span>
      {#if i < knobs.length - 1}
        <span class="seg" class:on={knob === 'on'}></span>
      {/if}
    {/each}
    <span class="st mono">{stateLabel}</span>
  </div>
</button>

<style>
  .t-mini {
    width: 100%;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 9px;
    padding: 9px 10px;
    display: flex;
    flex-direction: column;
    gap: 7px;
    text-align: left;
    font: inherit;
    color: inherit;
    cursor: pointer;
    transition: border-color 0.12s ease;
  }
  .t-mini:hover,
  .t-mini:focus-visible {
    border-color: var(--accent);
  }
  .t-mini.sel {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .t-mini:focus-visible {
    outline: none;
  }
  .t-mini .top {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .t-mini .kd {
    font: 700 8px/1 var(--mono);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    padding: 2px 5px;
    border-radius: 4px;
    border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
    color: var(--c);
  }
  .t-mini .id {
    font-size: 10px;
    color: var(--faint);
  }
  .t-mini .age {
    margin-left: auto;
    font-size: 9.5px;
    color: var(--muted);
  }
  .t-mini .av {
    width: 17px;
    height: 17px;
    border-radius: 4px;
    display: grid;
    place-items: center;
    font: 700 8px/1 var(--mono);
    color: #0b0e17;
    flex: 0 0 auto;
  }
  .t-mini .av.none {
    color: var(--faint);
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--border-2);
  }
  .t-mini .sum {
    font-size: 12.5px;
    color: var(--text);
    line-height: 1.35;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .mini-prog {
    display: flex;
    align-items: center;
    gap: 0;
  }
  .mini-prog .k {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    border: 1.5px solid var(--border-2);
    background: var(--bg);
    flex: 0 0 auto;
  }
  .mini-prog .k.on {
    background: var(--c);
    border-color: var(--c);
  }
  .mini-prog .k.cur {
    border-color: var(--c);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--c) 25%, transparent);
  }
  .mini-prog .seg {
    height: 1.5px;
    flex: 1;
    background: var(--border-2);
    margin: 0 1px;
  }
  .mini-prog .seg.on {
    background: var(--c);
  }
  .mini-prog .st {
    margin-left: 7px;
    font-size: 9.5px;
    color: var(--c);
  }
</style>
