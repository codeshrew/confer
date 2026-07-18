<script lang="ts">
  // The fleet map's per-agent node — the reusable half of the redesign's
  // appearance-encoding vocabulary (ui/REDESIGN.md law #1: position tells
  // you who/where, appearance tells you how they're doing right now).
  // Introduced for Overview's domain cards (design 2026-07-18, piece 1);
  // Fleet's per-agent detail (piece 7, the drill target of a node here) is
  // expected to reuse this same avatar/liveness/trust language rather than
  // reinvent it.
  //
  // Liveness is the only animated channel (a live node breathes), and only
  // when the platform allows it — prefers-reduced-motion turns the breathe
  // keyframe off below, matching Skeleton's shimmer.
  import type { DomainAgent } from '../attention';
  import { formatAgeFromSecs } from '../format';

  interface Props {
    agent: DomainAgent;
    /** Fires on click/Enter — the caller drills into that agent's Fleet
     * detail (design/47 §2.6's drill contract, carried into the redesign). */
    onOpen?: (agentId: string) => void;
  }

  let { agent, onOpen }: Props = $props();

  const LIVENESS_LABEL: Record<DomainAgent['liveness'], string> = {
    live: '⬤ live',
    stale: '◐ stale',
    down: '○ down',
  };
  const TRUST_LABEL: Record<DomainAgent['trust'], string> = {
    signed: '✓ signed',
    mismatch: '‼ mismatch',
    'first-sight': '⚠ first-sight',
    unsigned: '⚠ unsigned',
  };

  const livenessSuffix = $derived(agent.hbAgeSecs !== null ? ` · ${formatAgeFromSecs(agent.hbAgeSecs)}` : '');
</script>

<button
  type="button"
  class="an-node an-{agent.liveness}"
  onclick={() => onOpen?.(agent.id)}
  aria-label={`${agent.display}${agent.host ? ` on ${agent.host}` : ''} — ${LIVENESS_LABEL[agent.liveness]}`}
  data-testid="agent-node"
>
  <div class="an-top">
    <span class="an-avatar" style={agent.liveness === 'down' ? undefined : `--ac:${agent.color};background:${agent.color}`}>{agent.abbr}</span>
    <span class="an-who">
      <span class="an-nm">{agent.display}</span>
      <span class="an-host">{agent.host ?? '—'}</span>
    </span>
  </div>
  <div class="an-state">
    <span class="an-chip an-trust-{agent.trust}">{TRUST_LABEL[agent.trust]}</span>
    <span class="an-chip an-live-{agent.liveness}">{LIVENESS_LABEL[agent.liveness]}{livenessSuffix}</span>
    {#if agent.wip > 0}
      <span class="an-chip an-wip">● {agent.wip} WIP</span>
    {/if}
  </div>
</button>

<style>
  .an-node {
    flex: 1 1 160px;
    min-width: 148px;
    max-width: 220px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 10px 11px;
    text-align: left;
    font: inherit;
    color: inherit;
    transition:
      transform 0.15s ease,
      border-color 0.15s ease;
  }
  .an-node:hover {
    transform: translateY(-2px);
    border-color: var(--accent);
  }
  .an-node:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .an-node.an-down {
    opacity: 0.62;
  }
  .an-top {
    display: flex;
    align-items: center;
    gap: 9px;
  }
  .an-avatar {
    width: 32px;
    height: 32px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    font: 700 12px/1 var(--mono);
    color: #0b0e17;
    flex: 0 0 auto;
  }
  .an-down .an-avatar {
    background: transparent !important;
    color: var(--faint) !important;
    border: 1.5px dashed var(--border-2);
  }
  /* Liveness in the appearance channel (law #1): a live node breathes with a
     soft glow keyed to its own avatar color; a down node is already hollow
     via .an-down above, so it never breathes. */
  .an-live .an-avatar {
    box-shadow:
      0 0 0 2px var(--panel-2),
      0 0 12px -1px color-mix(in srgb, var(--ac, var(--accent)) 55%, transparent);
    animation: an-breathe 3.4s ease-in-out infinite;
  }
  @media (prefers-reduced-motion: reduce) {
    .an-live .an-avatar {
      animation: none;
    }
  }
  @keyframes an-breathe {
    0%,
    100% {
      box-shadow:
        0 0 0 2px var(--panel-2),
        0 0 8px -2px color-mix(in srgb, var(--ac, var(--accent)) 55%, transparent);
    }
    50% {
      box-shadow:
        0 0 0 2px var(--panel-2),
        0 0 16px 1px color-mix(in srgb, var(--ac, var(--accent)) 55%, transparent);
    }
  }
  .an-who {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .an-nm {
    font-weight: 640;
    font-size: 12.5px;
    line-height: 1.15;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .an-host {
    font: 500 10px/1.3 var(--mono);
    color: var(--faint);
  }
  .an-state {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-wrap: wrap;
  }
  .an-chip {
    font: 600 10px/1 var(--mono);
    padding: 3px 6px;
    border-radius: 5px;
    border: 1px solid var(--border);
    color: var(--muted);
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .an-trust-signed {
    color: var(--done);
    border-color: color-mix(in srgb, var(--done) 35%, transparent);
  }
  .an-trust-mismatch,
  .an-trust-unsigned {
    color: var(--error);
    border-color: color-mix(in srgb, var(--error) 40%, transparent);
  }
  .an-trust-first-sight {
    color: var(--blocked);
    border-color: color-mix(in srgb, var(--blocked) 40%, transparent);
  }
  .an-live-live {
    color: var(--done);
  }
  .an-live-stale {
    color: var(--blocked);
  }
  .an-live-down {
    color: var(--faint);
  }
  .an-wip {
    color: var(--claimed);
    border-color: color-mix(in srgb, var(--claimed) 40%, transparent);
    background: color-mix(in srgb, var(--claimed) 12%, transparent);
  }
</style>
