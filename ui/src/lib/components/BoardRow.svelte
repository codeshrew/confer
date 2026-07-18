<script lang="ts">
  import type { Agent, RequestRow } from '../types';
  import { agePct, ageColorVar, formatAgeFromSecs } from '../format';

  interface Props {
    request: RequestRow;
    agents: Agent[];
    statusVar: string;
    selected?: boolean;
    onSelect?: (id: string) => void;
  }

  let { request, agents, statusVar, selected = false, onSelect }: Props = $props();

  const agentsById = $derived(new Map(agents.map((a) => [a.id, a])));
  const claimant = $derived(request.claimants[0] ?? null);

  function cap(s: string): string {
    return s.length ? s[0]!.toUpperCase() + s.slice(1) : s;
  }
</script>

{#snippet pip(agentId: string | null)}
  {#if !agentId}
    <span class="bp none" title="unclaimed">–</span>
  {:else}
    {@const agent = agentsById.get(agentId)}
    <span
      class="bp"
      style="color:{agent?.color ?? 'var(--muted)'};background:color-mix(in srgb, {agent?.color ?? 'var(--muted)'} 18%, transparent)"
      title={agent?.display ?? agentId}>{agent?.abbr ?? cap(agentId).slice(0, 2).toUpperCase()}</span
    >
  {/if}
{/snippet}

<div
  class="brow"
  class:sel={selected}
  style="--st:{statusVar}"
  role="button"
  tabindex="0"
  onclick={() => onSelect?.(request.id)}
  onkeydown={(e) => {
    if (e.key === 'Enter' || e.key === ' ') onSelect?.(request.id);
  }}
>
  <span class="bstripe"></span>
  <div class="btitle">
    <span class="bt">{request.summary}</span>
    <span class="bid">{request.id}</span>
  </div>
  <span class="btopic">#{request.topic ?? '—'}</span>
  <div class="bhand">
    {@render pip(request.from)}
    <span class="barr">→</span>
    {@render pip(claimant)}
  </div>
  <div class="bage">
    <span class="agebar"><i style="width:{agePct(request.ageSecs)}%;background:{ageColorVar(request.ageSecs, request.stale)}"></i></span>
    <span class="agelab" class:warn={request.stale}>{formatAgeFromSecs(request.ageSecs)}{request.stale ? ' ⚠' : ''}</span>
  </div>
  <span class="bstamp">{request.status.toLowerCase()}</span>
</div>

<style>
  .brow {
    display: grid;
    grid-template-columns: 3px minmax(0, 1fr) auto auto 108px auto;
    align-items: center;
    gap: 13px;
    padding: 8px 12px 8px 0;
    border-radius: 9px;
    width: 100%;
    border: 0;
    background: transparent;
    text-align: left;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .brow:hover {
    background: var(--panel);
  }
  .brow.sel {
    background: var(--panel);
    box-shadow: inset 0 0 0 1px var(--border-2);
  }
  .brow .bstripe {
    width: 3px;
    align-self: stretch;
    min-height: 30px;
    border-radius: 3px;
    background: var(--st);
  }
  .brow .btitle {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .brow .bt {
    font-weight: 600;
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .brow .bid {
    font: 500 10px/1 var(--mono);
    color: var(--faint);
  }
  .brow .btopic {
    font: 600 10.5px/1 var(--mono);
    color: var(--muted);
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px 7px;
    white-space: nowrap;
  }
  .brow .bhand {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .brow .bp {
    width: 21px;
    height: 21px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    font: 700 8.5px/1 var(--mono);
    flex: 0 0 auto;
  }
  .brow .bp.none {
    color: var(--faint);
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--border-2);
  }
  .brow .barr {
    color: var(--faint);
    font-size: 11px;
  }
  .brow .bage {
    display: flex;
    align-items: center;
    gap: 9px;
    justify-content: flex-end;
  }
  .brow .agebar {
    width: 52px;
    height: 5px;
    border-radius: 3px;
    background: var(--panel-3);
    overflow: hidden;
    flex: 0 0 auto;
  }
  .brow .agebar i {
    display: block;
    height: 100%;
    border-radius: 3px;
  }
  .brow .agelab {
    font: 500 10.5px/1 var(--mono);
    color: var(--muted);
    min-width: 34px;
    text-align: right;
    white-space: nowrap;
  }
  .brow .agelab.warn {
    color: var(--blocked);
  }
  .brow .bstamp {
    font: 800 8.5px/1 var(--mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--st);
    border: 1.5px solid var(--st);
    border-radius: 5px;
    padding: 3px 6px;
    transform: rotate(-3deg);
  }

  /* ── Phone (<768px): the desktop grid packs 6 columns (stripe / title /
     topic / claim-handoff / age / stamp) side by side, several of them
     fixed-width — together they need more room than a ~360-390px viewport
     has. Below 768px it becomes a 3-column/3-row grid (stripe stays a full-
     height rail; title+stamp share row 1, topic gets its own row, the
     claim-handoff + age share the last row) so every cell wraps to a size
     the phone actually has. ── */
  @media (max-width: 767.98px) {
    .brow {
      grid-template-columns: 3px 1fr auto;
      grid-template-areas:
        'stripe title stamp'
        'stripe topic topic'
        'stripe hand age';
      row-gap: 7px;
      align-items: start;
    }
    .brow .bstripe {
      grid-area: stripe;
    }
    .brow .btitle {
      grid-area: title;
      min-width: 0;
    }
    .brow .bt {
      white-space: normal;
      overflow-wrap: anywhere;
    }
    .brow .bstamp {
      grid-area: stamp;
      align-self: start;
    }
    .brow .btopic {
      grid-area: topic;
      justify-self: start;
    }
    .brow .bhand {
      grid-area: hand;
    }
    .brow .bage {
      grid-area: age;
      justify-content: flex-start;
    }
  }
</style>
