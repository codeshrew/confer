<script lang="ts">
  // Piece 5c (ui/REDESIGN.md) — the Board view's left column: "NOT the chat
  // channel list — a Fleet-as-filter rail (collapsible): click an agent →
  // filter the board to their work." Replaces LeftRail in the `rail-l-wrap`
  // slot on Board specifically (App.svelte), the same way CodeTree replaces
  // it on Code — each view's left column is whatever navigator actually
  // fits it, not a one-size-fits-all topic list.
  //
  // Clicking an agent toggles the SHARED `boardFilter` singleton's
  // `agentFilter` (piece 5c: also settable from Board's own workload-bar
  // rows) — this rail and Board.svelte are siblings in App.svelte's
  // template, so the module singleton (same pattern as paneFocus/
  // readState) is how they agree on "which agent is filtered" without
  // App.svelte threading state between them.
  import type { Agent } from '../types';
  import { formatAge } from '../format';
  import { paneFocus } from '../paneFocus.svelte';
  import { boardFilter } from '../boardFilter.svelte';

  interface Props {
    agents: Agent[];
    now?: number;
  }

  let { agents, now = Date.now() }: Props = $props();

  let collapsed = $state(false);

  function wipCount(agent: Agent): number {
    return agent.wip.filter((w) => w.status === 'CLAIMED').length;
  }

  // roving-tabindex j/k/Enter — the same shape as LeftRail's topic-list and
  // HubRail's hub list (piece 2's established pattern for this UI shape).
  let focusedIdx = $state(0);
  // `$state` (not a plain `let`, unlike LeftRail's always-rendered
  // topic-list): this one's `bind:this` lives inside the `{#if !collapsed}`
  // block below, so it genuinely un-/re-mounts — Svelte needs the reactive
  // wrapper to track that correctly.
  let listEl: HTMLDivElement | undefined = $state();
  let railEl: HTMLDivElement;
  let buttonEls = $state<(HTMLButtonElement | null)[]>([]);

  $effect(() => {
    if (focusedIdx >= agents.length) focusedIdx = Math.max(0, agents.length - 1);
  });

  $effect(() => {
    if (!railEl) return;
    return paneFocus.register({
      id: 'fleet-rail',
      label: 'Fleet filter',
      el: listEl ?? railEl,
      getRect: () => railEl.getBoundingClientRect(),
    });
  });

  function forwardContainerFocus(e: FocusEvent) {
    if (e.target === listEl) buttonEls[focusedIdx]?.focus();
  }

  function handleListKeydown(e: KeyboardEvent) {
    if (agents.length === 0) return;
    if (e.key === 'j' || e.key === 'ArrowDown') {
      e.preventDefault();
      focusedIdx = Math.min(focusedIdx + 1, agents.length - 1);
      buttonEls[focusedIdx]?.focus();
      return;
    }
    if (e.key === 'k' || e.key === 'ArrowUp') {
      e.preventDefault();
      focusedIdx = Math.max(focusedIdx - 1, 0);
      buttonEls[focusedIdx]?.focus();
      return;
    }
    if (e.key === 'Enter' || e.key === 'l') {
      e.preventDefault();
      const agent = agents[focusedIdx];
      if (agent) boardFilter.toggleAgent(agent.id);
    }
  }
</script>

<div class="rail-l" bind:this={railEl} data-testid="board-fleet-rail">
  <div class="rail-scroll">
    <div class="rail-head">
      <h3>Fleet · filter</h3>
      <div class="rail-head-actions">
        {#if boardFilter.agentFilter}
          <button type="button" class="clear-btn" onclick={() => boardFilter.toggleAgent(boardFilter.agentFilter!)}>✕ clear</button>
        {/if}
        <button
          type="button"
          class="collapse-btn"
          onclick={() => (collapsed = !collapsed)}
          aria-expanded={!collapsed}
          aria-label={collapsed ? 'Expand fleet filter' : 'Collapse fleet filter'}
        >
          {collapsed ? '▸' : '▾'}
        </button>
      </div>
    </div>

    {#if !collapsed}
      <div
        class="agent-list"
        role="toolbar"
        aria-orientation="vertical"
        aria-label="filter by agent"
        tabindex="-1"
        bind:this={listEl}
        onkeydown={handleListKeydown}
        onfocus={forwardContainerFocus}
      >
        {#each agents as agent, i (agent.id)}
          <button
            type="button"
            class="agent"
            class:active={boardFilter.agentFilter === agent.id}
            class:stale={!agent.live}
            tabindex={i === focusedIdx ? 0 : -1}
            bind:this={buttonEls[i]}
            onfocus={() => (focusedIdx = i)}
            onclick={() => boardFilter.toggleAgent(agent.id)}
            title={`${agent.live ? 'live' : 'heartbeat stale'} · filter the board to ${agent.display}'s work`}
            data-testid="fleet-rail-agent"
          >
            <span class="av" style="color:{agent.color};background:color-mix(in srgb, {agent.color} 18%, transparent)">{agent.abbr}</span>
            <span class="nm">{agent.display}</span>
            {#if wipCount(agent) > 0}
              <span class="wip mono">{wipCount(agent)}</span>
            {/if}
            <span class="hb mono">{formatAge(agent.lastTs, now)}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .rail-l {
    background: var(--panel);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .rail-scroll {
    overflow-y: auto;
    flex: 1;
    padding: 12px 10px;
  }
  .rail-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 8px 8px;
  }
  .rail-head h3 {
    margin: 0;
    font: 700 11px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--faint);
  }
  .rail-head-actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .clear-btn {
    font: 600 10px/1 var(--mono);
    color: var(--accent);
    background: transparent;
    border: 1px solid var(--border-2);
    border-radius: 5px;
    padding: 3px 6px;
    cursor: pointer;
  }
  .collapse-btn {
    color: var(--faint);
    background: transparent;
    border: 0;
    cursor: pointer;
    font-size: 11px;
    padding: 3px 4px;
  }
  .collapse-btn:hover {
    color: var(--text);
  }

  .agent-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 2px 0;
  }
  .agent {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 6px 8px;
    border-radius: 7px;
    border: 1px solid transparent;
    width: 100%;
    background: transparent;
    text-align: left;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .agent:hover {
    background: var(--panel-2);
  }
  .agent.active {
    background: var(--panel-3);
    border-color: var(--accent);
  }
  .agent:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .agent .av {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    font: 700 10px/1 var(--mono);
    letter-spacing: 0.02em;
  }
  .agent .nm {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12.5px;
    color: var(--text);
    font-weight: 500;
  }
  .agent .wip {
    font-size: 10px;
    color: var(--state-flight);
    background: color-mix(in srgb, var(--state-flight) 15%, transparent);
    border-radius: 4px;
    padding: 1px 5px;
    flex: 0 0 auto;
  }
  .agent .hb {
    font-size: 10px;
    color: var(--faint);
    flex: 0 0 auto;
  }
  .agent.stale .hb {
    color: var(--blocked);
  }
</style>
