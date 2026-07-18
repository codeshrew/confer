<script lang="ts">
  // Fleet / agent-identity view. Ports `.fleetgrid`/`.agentcard`/`.ac-*` from
  // design/serve-dashboard-v2-mockup.html: identity cards (color/host/
  // heartbeat/verification) and a "You · viewing" card.
  //
  // Identity (display name / abbreviation / color) is READ-ONLY here: it's
  // self-declared in the agent's signed role card and synced across the hub.
  // There's no editor in this view — appearance is set by the agent via the
  // CLI (not yet built), not by clicking around a dashboard.
  import type { Agent } from '../types';
  import { formatAge } from '../format';

  interface Props {
    agents: Agent[];
    hubName: string;
  }

  let { agents, hubName }: Props = $props();

  const staleCount = $derived(agents.filter((a) => !a.live).length);

  const you = { display: 'You', abbr: '◉', color: 'var(--accent)' };

  const VERIFY_GLYPH: Record<Agent['verified'], { g: string; cls: string; title: string }> = {
    signed: { g: '✓', cls: 'ok', title: 'verified — key matches the role card' },
    'first-sight': { g: '⚠', cls: 'warn', title: 'first-sight — no history to verify against yet' },
    unverified: { g: '⚠', cls: 'warn', title: 'unverified — heads up before trusting claims' },
  };
</script>

<div class="fleet-wrap" data-testid="fleet-view">
  <div class="board-head">
    <div class="board-topline">
      <h2>Fleet · {hubName}</h2>
      <span class="flabel" style="margin-left:auto">{agents.length} agents · {staleCount} stale heartbeat{staleCount === 1 ? '' : 's'}</span>
    </div>
  </div>

  <p class="ac-note fleet-note">
    Appearance is self-declared in each agent's role card and synced across the hub — set by the agent, not editable
    from here.
  </p>

  <div class="fleetgrid">
    <!-- "You" — the dashboard viewer, a first-class but non-claiming peer -->
    <div class="agentcard you">
      <div class="ac-top">
        <span class="ac-av" style="color:{you.color};background:color-mix(in srgb, {you.color} 18%, transparent)">{you.abbr}</span>
        <div class="ac-id">
          <div class="ac-nm">{you.display}</div>
          <div class="ac-host">viewing this dashboard</div>
        </div>
      </div>
      <div class="ac-hb">watching · not a claiming peer</div>
    </div>

    {#each agents as agent (agent.id)}
      {@const stale = !agent.live}
      {@const vm = VERIFY_GLYPH[agent.verified]}
      <div class="agentcard" class:stale>
        <div class="ac-top">
          <span class="ac-av" style="color:{agent.color};background:color-mix(in srgb, {agent.color} 18%, transparent)">{agent.abbr}</span>
          <div class="ac-id">
            <div class="ac-nm">{agent.display}</div>
            <div class="ac-host">{agent.lastHost ?? agent.expectedHost ?? '—'}</div>
          </div>
          <span class="ac-verify {vm.cls}" title={vm.title}>{vm.g}</span>
        </div>
        <div class="ac-hb">{stale ? 'heartbeat stale' : 'live'} · last posted {formatAge(agent.lastTs)} ago</div>
        {#if agent.verified === 'unverified'}
          <div class="ac-warnline">⚠ unverified peer — if this hub's remote allows anonymous read, verify the key before trusting claims</div>
        {/if}
        <div class="ac-wip">
          <div class="ac-wiplab">Current WIP</div>
          {#if agent.wip.length === 0}
            <div class="ac-idle">no active claims</div>
          {:else}
            {#each agent.wip as w (w.id)}
              <div class="ac-wipitem">
                <span class="pill p-{w.status.toLowerCase()}">{w.status}</span>
                <span>{w.summary}</span>
                <span class="mono">{w.id}</span>
              </div>
            {/each}
          {/if}
        </div>
      </div>
    {/each}
  </div>
</div>

<style>
  .fleet-wrap {
    overflow: auto;
    flex: 1;
    padding: 16px 20px;
  }
  .board-head {
    margin-bottom: 6px;
  }
  .board-topline {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 12px;
  }
  .board-topline h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 650;
  }
  .flabel {
    font: 600 10px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.09em;
    color: var(--faint);
  }
  .fleetgrid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 14px;
  }
  .agentcard {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 14px 15px;
    display: flex;
    flex-direction: column;
    gap: 11px;
  }
  .agentcard.you {
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
  }
  .ac-top {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .ac-av {
    width: 36px;
    height: 36px;
    border-radius: 9px;
    display: grid;
    place-items: center;
    font: 700 13px/1 var(--mono);
    flex: 0 0 auto;
  }
  .ac-id {
    min-width: 0;
    flex: 1;
  }
  .ac-nm {
    font-weight: 650;
    font-size: 13.5px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .ac-host {
    font: 500 10.5px/1 var(--mono);
    color: var(--faint);
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ac-verify {
    font-size: 13px;
    flex: 0 0 auto;
  }
  .ac-verify.ok {
    color: var(--done);
  }
  .ac-verify.warn {
    color: var(--blocked);
  }
  .ac-verify.unk {
    color: var(--faint);
  }
  .ac-hb {
    display: flex;
    align-items: center;
    gap: 6px;
    font: 500 11px/1 var(--mono);
    color: var(--muted);
  }
  .ac-hb::before {
    content: '';
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--done);
  }
  .agentcard.stale .ac-hb {
    color: var(--blocked);
  }
  .agentcard.stale .ac-hb::before {
    background: var(--blocked);
  }
  .ac-wip {
    border-top: 1px solid var(--border);
    padding-top: 9px;
  }
  .ac-wiplab {
    font: 700 9px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--faint);
    margin-bottom: 6px;
  }
  .ac-wipitem {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: var(--text);
    padding: 2px 0;
  }
  .ac-wipitem .mono {
    color: var(--faint);
    font-size: 10.5px;
  }
  .ac-wipitem .pill {
    font: 700 8.5px/1 var(--mono);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    padding: 2px 5px;
    border-radius: 5px;
  }
  .p-open {
    color: var(--open);
    background: color-mix(in srgb, var(--open) 16%, transparent);
  }
  .p-claimed {
    color: var(--claimed);
    background: color-mix(in srgb, var(--claimed) 16%, transparent);
  }
  .p-blocked {
    color: var(--blocked);
    background: color-mix(in srgb, var(--blocked) 16%, transparent);
  }
  .p-done {
    color: var(--done);
    background: color-mix(in srgb, var(--done) 16%, transparent);
  }
  .p-error {
    color: var(--error);
    background: color-mix(in srgb, var(--error) 16%, transparent);
  }
  .ac-idle {
    font-size: 12px;
    color: var(--faint);
    font-style: italic;
  }
  .ac-warnline {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 2px;
    font-size: 11px;
    color: var(--blocked);
  }
  .ac-note {
    font-size: 11px;
    color: var(--faint);
    margin: 0;
  }
  .fleet-note {
    margin: 0 0 12px;
  }
</style>
