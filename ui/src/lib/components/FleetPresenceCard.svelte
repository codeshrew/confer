<script lang="ts">
  // Piece 8a's crew-deck presence card (ui/REDESIGN.md, `08-fleet-crew-
  // deck.html`) — the SAME appearance-encoding AgentNode.svelte established
  // (breathe when live, fade when stale, hollow/dashed when down) EXTENDED
  // with the richer content the deck wants: role one-liner, a WIP chip, a
  // real activity sparkline, and tier-colored hub-membership dots. Kept as
  // its own component rather than bolting these onto AgentNode itself —
  // AgentNode's `agent: DomainAgent` (attention.ts's per-hub-occurrence
  // shape, used by Overview) doesn't carry a role line, sparkline data, or
  // multi-hub presence, and forcing those in would mean Overview's much
  // simpler node either fetches data it doesn't need or gets optional props
  // it never fills. Same CSS values/breathing keyframes, same trust-color
  // mapping — the visual LANGUAGE is reused, verbatim in spirit.
  import type { Agent } from '../types';
  import type { AgentHubPresence } from '../attention';
  import { deriveLiveness, deriveTrust } from '../attention';
  import { formatAgeFromSecs } from '../format';

  interface Props {
    agent: Agent;
    /** Real per-hour message counts, oldest first — `fleetDossier.ts`'s
     * `activityBuckets`, same real data the dossier's own chart uses. */
    sparkline: number[];
    /** Real cross-hub presence — omitted (empty) while still loading
     * rather than showing a fabricated single-hub guess. */
    hubs: AgentHubPresence[];
    onOpen?: (agentId: string) => void;
  }

  let { agent, sparkline, hubs, onOpen }: Props = $props();

  const liveness = $derived(deriveLiveness(agent));
  const trust = $derived(deriveTrust(agent));
  const wipCount = $derived(agent.wip.filter((w) => w.status === 'CLAIMED').length);
  const maxSpark = $derived(Math.max(1, ...sparkline));

  const LIVE_LABEL: Record<string, string> = { live: '⬤ live', stale: '◐ stale', down: '○ down' };
</script>

<button type="button" class="fp-card {liveness}" onclick={() => onOpen?.(agent.id)} data-testid="fleet-presence-card">
  <div class="fp-top">
    <span class="fp-avatar {liveness}" style={liveness === 'down' ? undefined : `background:${agent.color}`}>{agent.abbr}</span>
    <span class="fp-who">
      <span class="fp-nm">{agent.display}</span>
      {#if agent.desc}<span class="fp-role">{agent.desc}</span>{/if}
    </span>
    <span class="fp-status">
      <span class="liv {liveness}">{LIVE_LABEL[liveness]}</span>
      {#if agent.hbAgeSecs != null}<span class="hb">hb {formatAgeFromSecs(agent.hbAgeSecs)}</span>{/if}
    </span>
  </div>
  <div class="fp-mid">
    <span class="chip trust-{trust}">{trust === 'signed' ? '✓ signed' : trust === 'mismatch' ? '‼ mismatch' : trust === 'first-sight' ? '⚠ first-sight' : '⚠ unsigned'}</span>
    {#if wipCount > 0}<span class="chip wip">● {wipCount} WIP</span>{:else}<span class="chip">0 WIP</span>{/if}
    {#if sparkline.length}
      <span class="fp-spark" aria-hidden="true">
        {#each sparkline as c, i (i)}<i style="height:{Math.max(2, (c / maxSpark) * 20)}px"></i>{/each}
      </span>
    {/if}
  </div>
  {#if hubs.length}
    <div class="fp-hubs">
      {#each hubs as h (h.hub)}
        <span class="hd tier-{h.tier ?? 'unclassified'}"></span>{h.hub}
      {/each}
    </div>
  {/if}
</button>

<style>
  .fp-card {
    width: 100%;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 11px 12px;
    text-align: left;
    font: inherit;
    color: inherit;
    cursor: pointer;
    transition:
      border-color 0.12s,
      transform 0.12s;
  }
  .fp-card:hover {
    border-color: var(--accent);
    transform: translateY(-1px);
  }
  .fp-card.down {
    opacity: 0.72;
  }
  .fp-top {
    display: flex;
    align-items: center;
    gap: 9px;
  }
  .fp-avatar {
    width: 34px;
    height: 34px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    font: 700 12px/1 var(--mono);
    color: #0b0e17;
    flex: 0 0 auto;
  }
  .fp-avatar.down {
    background: transparent !important;
    color: var(--faint) !important;
    border: 1.5px dashed var(--border-2);
  }
  .fp-avatar.live {
    box-shadow:
      0 0 0 2px var(--panel-2),
      0 0 12px -1px color-mix(in srgb, var(--state-flight) 55%, transparent);
    animation: fp-breathe 3.4s ease-in-out infinite;
  }
  @media (prefers-reduced-motion: reduce) {
    .fp-avatar.live {
      animation: none;
    }
  }
  @keyframes fp-breathe {
    0%,
    100% {
      box-shadow:
        0 0 0 2px var(--panel-2),
        0 0 8px -2px color-mix(in srgb, var(--state-flight) 55%, transparent);
    }
    50% {
      box-shadow:
        0 0 0 2px var(--panel-2),
        0 0 16px 1px color-mix(in srgb, var(--state-flight) 55%, transparent);
    }
  }
  .fp-who {
    min-width: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  .fp-nm {
    font-weight: 640;
    font-size: 13px;
    line-height: 1.2;
  }
  .fp-role {
    font-size: 10px;
    color: var(--faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .fp-status {
    text-align: right;
    flex: 0 0 auto;
    font-size: 10px;
  }
  .fp-status .liv {
    font-weight: 700;
  }
  .fp-status .liv.live {
    color: var(--state-flight);
  }
  .fp-status .liv.stale {
    color: var(--state-unowned);
  }
  .fp-status .liv.down {
    color: var(--state-stuck);
  }
  .fp-status .hb {
    display: block;
    color: var(--muted-2, var(--faint));
    margin-top: 1px;
  }
  .fp-mid {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 9px;
  }
  .chip {
    font: 600 9.5px/1 var(--mono);
    padding: 2px 6px;
    border-radius: 4px;
    border: 1px solid var(--border);
    color: var(--muted);
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .chip.trust-signed {
    color: var(--state-flight);
    border-color: color-mix(in srgb, var(--state-flight) 35%, transparent);
  }
  .chip.trust-mismatch,
  .chip.trust-unsigned {
    color: var(--state-stuck);
    border-color: color-mix(in srgb, var(--state-stuck) 40%, transparent);
  }
  .chip.trust-first-sight {
    color: var(--state-unowned);
    border-color: color-mix(in srgb, var(--state-unowned) 40%, transparent);
  }
  .chip.wip {
    color: var(--state-flight);
    border-color: color-mix(in srgb, var(--state-flight) 40%, transparent);
  }
  .fp-spark {
    display: flex;
    align-items: flex-end;
    gap: 1.5px;
    height: 20px;
    margin-left: auto;
  }
  .fp-spark i {
    width: 3px;
    background: color-mix(in srgb, var(--accent) 45%, transparent);
    border-radius: 1px;
    min-height: 2px;
  }
  .fp-card.down .fp-spark i {
    background: var(--border-2);
  }
  .fp-hubs {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-top: 8px;
    font: 500 9.5px/1 var(--mono);
    color: var(--muted);
    flex-wrap: wrap;
  }
  .fp-hubs .hd {
    width: 7px;
    height: 7px;
    border-radius: 2px;
    margin-right: -2px;
  }
  .fp-hubs .hd.tier-own {
    background: var(--home-frame, var(--accent));
  }
  .fp-hubs .hd.tier-shared {
    background: var(--shared-frame, var(--accent));
  }
  .fp-hubs .hd.tier-foreign {
    background: var(--foreign-frame, var(--muted));
  }
  .fp-hubs .hd.tier-unclassified {
    background: var(--neutral-frame, var(--faint));
  }
</style>
