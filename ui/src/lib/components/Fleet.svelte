<script lang="ts">
  // Fleet — piece 8a, the crew deck (ui/REDESIGN.md, `08-fleet-crew-
  // deck.html` + `08-fleet-BRIEF.md`): "agents are processes that live on
  // machines... grouping by machine is the honest topology." Replaces the
  // old flat `.fleetgrid` with machine bays (fleetBays.ts) holding living
  // presence cards (FleetPresenceCard.svelte, extending AgentNode's own
  // appearance-encoding). Clicking a card opens the reusable agent dossier
  // (piece 8b, AgentDossier.svelte) — wired one level up in App.svelte,
  // since the dossier is reachable from more than just this page.
  import type { Agent, Message } from '../types';
  import { fetchHubOverviews } from '../api';
  import { agentPresence, type AgentHubPresence } from '../attention';
  import { buildBays, fleetVitals } from '../fleetBays';
  import { activityBuckets } from '../fleetDossier';
  import FleetPresenceCard from './FleetPresenceCard.svelte';
  import EmptyState from './EmptyState.svelte';
  import Skeleton from './Skeleton.svelte';

  interface Props {
    agents: Agent[];
    hubName: string;
    /** For each card's real activity sparkline — same message data the
     * dossier's own (bigger) activity chart buckets. */
    messages: Message[];
    onOpenAgent?: (id: string) => void;
  }

  let { agents, hubName, messages, onOpenAgent }: Props = $props();

  const SPARK_HOURS = 7;
  function sparklineFor(agentId: string): number[] {
    return activityBuckets(agentId, messages, SPARK_HOURS).map((b) => b.count);
  }

  // Cross-hub presence (the hub-membership dots) — fetched once per hub
  // visit, not eagerly re-polled like HubRail's own health dots; the deck
  // doesn't need second-by-second freshness on "which hubs is this agent
  // on," just an honest real answer.
  let presenceLoading = $state(true);
  let presenceByAgent = $state<Map<string, AgentHubPresence[]>>(new Map());
  let loadedForHub = $state<string | null>(null);

  async function loadPresence(hub: string) {
    presenceLoading = true;
    try {
      const hubOverviews = await fetchHubOverviews();
      const map = new Map<string, AgentHubPresence[]>();
      for (const agent of agents) map.set(agent.id, agentPresence(hubOverviews, agent.id));
      presenceByAgent = map;
      loadedForHub = hub;
    } catch (err) {
      console.error('confer serve: failed to load fleet presence', hub, err);
      presenceByAgent = new Map();
      loadedForHub = hub;
    } finally {
      presenceLoading = false;
    }
  }

  $effect(() => {
    if (hubName && (hubName !== loadedForHub || agents)) void loadPresence(hubName);
  });

  const bays = $derived(buildBays(agents));
  const vitals = $derived(fleetVitals(agents, bays));
  const trustLine = $derived(vitals.unsignedCount === 0 ? '✓ all keys signed' : `◐ ${vitals.unsignedCount} unverified`);
</script>

<div class="fleet-wrap" data-testid="fleet-view">
  {#if agents.length === 0}
    <EmptyState glyph="◇" title="No agents on this hub" body="Agents post to this hub's confer log — none have shown up yet." />
  {:else}
    <div class="vitals">
      <h2>fleet · {hubName}</h2>
      <span class="v mono"><b>{vitals.agentCount}</b> agents</span>
      <span class="v mono live"><b>{vitals.liveCount}</b> live</span>
      {#if vitals.downCount > 0}<span class="v mono down"><b>{vitals.downCount}</b> down</span>{/if}
      <span class="v mono"><b>{vitals.machineCount}</b> machine{vitals.machineCount === 1 ? '' : 's'}</span>
      <span class="you mono" title="You're viewing this dashboard — not a claiming peer">◉ you're watching</span>
      <span class="trust" class:warn={vitals.unsignedCount > 0}>{trustLine}</span>
    </div>

    {#if presenceLoading}
      <Skeleton rows={3} />
    {:else}
      <div class="bays">
        {#each bays as bay (bay.host)}
          <div class="bay" class:dark={bay.dark}>
            <div class="bay-h">
              <span class="host mono">{bay.host}</span>
              <span class="pw mono"><span class="pip"></span>{bay.dark ? 'dark' : 'online'}</span>
            </div>
            <div class="bay-agents">
              {#each bay.agents as agent (agent.id)}
                <FleetPresenceCard {agent} sparkline={sparklineFor(agent.id)} hubs={presenceByAgent.get(agent.id) ?? []} onOpen={onOpenAgent} />
              {/each}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .fleet-wrap {
    overflow: auto;
    flex: 1;
    padding: 16px 20px 40px;
  }

  .vitals {
    display: flex;
    align-items: baseline;
    gap: 16px;
    flex-wrap: wrap;
    margin-bottom: 14px;
  }
  .vitals h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 650;
  }
  .vitals .v {
    font-size: 11px;
    color: var(--muted);
  }
  .vitals .v b {
    font-size: 13px;
    color: var(--text);
  }
  .vitals .v.live b {
    color: var(--state-flight);
  }
  .vitals .v.down b {
    color: var(--state-stuck);
  }
  .vitals .you {
    font-size: 10px;
    color: var(--faint);
  }
  .vitals .trust {
    margin-left: auto;
    font: 600 11px/1 var(--mono);
    color: var(--state-flight);
  }
  .vitals .trust.warn {
    color: var(--state-unowned);
  }

  .bays {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 14px;
  }
  .bay {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 11px;
  }
  .bay.dark {
    opacity: 0.72;
  }
  .bay-h {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 4px 9px;
  }
  .bay-h .host {
    font-size: 12.5px;
    font-weight: 640;
    color: var(--text);
  }
  .bay-h .pw {
    margin-left: auto;
    font-size: 10px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--state-flight);
  }
  .bay-h .pw .pip {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--state-flight);
    box-shadow: 0 0 6px var(--state-flight);
  }
  .bay.dark .bay-h .pw {
    color: var(--state-stuck);
  }
  .bay.dark .bay-h .pw .pip {
    background: var(--state-stuck);
    box-shadow: none;
  }
  .bay-agents {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  @media (max-width: 767.98px) {
    .bays {
      grid-template-columns: 1fr;
    }
  }
</style>
