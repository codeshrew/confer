<script lang="ts">
  import type { Agent } from '../types';

  export type StatusFilter = 'all' | 'active' | 'attention' | 'blocked' | 'done' | 'backlog';

  interface Props {
    statusFilter: StatusFilter;
    notesOn: boolean;
    reqsOn: boolean;
    agents: Agent[];
    /** Chat density (Summary/Full segmented control) — omitted (undefined)
     * hides the control entirely, e.g. while viewing the Board. */
    chatDensity?: 'summary' | 'full';
    onStatusFilterChange?: (filter: StatusFilter) => void;
    onToggleNotes?: () => void;
    onToggleReqs?: () => void;
    onChatDensityChange?: (density: 'summary' | 'full') => void;
  }

  let {
    statusFilter,
    notesOn,
    reqsOn,
    agents,
    chatDensity,
    onStatusFilterChange,
    onToggleNotes,
    onToggleReqs,
    onChatDensityChange,
  }: Props = $props();

  const statuses: { key: StatusFilter; label: string; dot?: string }[] = [
    { key: 'all', label: 'All' },
    { key: 'active', label: 'Active', dot: 'var(--open)' },
    { key: 'attention', label: 'Needs attention', dot: 'var(--blocked)' },
    { key: 'blocked', label: 'Blocked', dot: 'var(--blocked)' },
    { key: 'done', label: 'Done', dot: 'var(--done)' },
    { key: 'backlog', label: 'Backlog', dot: 'var(--deferred)' },
  ];
</script>

<div class="filterbar">
  <span class="flabel">Status</span>
  {#each statuses as s (s.key)}
    <button
      type="button"
      class="chip"
      class:on={statusFilter === s.key}
      onclick={() => onStatusFilterChange?.(s.key)}
    >
      {#if s.dot}<span class="dot" style="background:{s.dot}"></span>{/if}
      {s.label}
    </button>
  {/each}

  <span class="divider"></span>
  <span class="flabel">Type</span>
  <button type="button" class="chip" class:on={notesOn} onclick={() => onToggleNotes?.()}>Notes</button>
  <button type="button" class="chip" class:on={reqsOn} onclick={() => onToggleReqs?.()}>Requests</button>

  {#if chatDensity}
    <span class="divider"></span>
    <span class="flabel">Density</span>
    <div class="segctl" role="group" aria-label="Chat density" data-testid="density-toggle">
      <button
        type="button"
        class="segbtn"
        class:on={chatDensity === 'summary'}
        aria-pressed={chatDensity === 'summary'}
        onclick={() => onChatDensityChange?.('summary')}
      >
        Summary
      </button>
      <button
        type="button"
        class="segbtn"
        class:on={chatDensity === 'full'}
        aria-pressed={chatDensity === 'full'}
        onclick={() => onChatDensityChange?.('full')}
      >
        Full
      </button>
    </div>
  {/if}

  <span class="divider"></span>
  <span class="flabel">Who</span>
  {#each agents as agent (agent.id)}
    <button type="button" class="chip ag" style="color:{agent.color}">{agent.abbr}</button>
  {/each}
</div>

<style>
  .filterbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 9px 16px;
    flex: 0 0 auto;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
  }
  .flabel {
    font: 600 10px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.09em;
    color: var(--faint);
    margin-right: 2px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
    padding: 5px 10px;
    border-radius: 999px;
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--muted);
    font-size: 12px;
    font-weight: 550;
  }
  .chip:hover {
    color: var(--text);
    border-color: var(--faint);
  }
  .chip.on {
    background: color-mix(in srgb, var(--accent) 16%, var(--panel-2));
    border-color: var(--accent);
    color: var(--text);
  }
  .chip .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .chip.ag {
    font-family: var(--mono);
    font-size: 11px;
  }
  .divider {
    width: 1px;
    height: 20px;
    background: var(--border-2);
    margin: 0 4px;
    flex: 0 0 auto;
  }
  .segctl {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--border-2);
    border-radius: 999px;
    padding: 2px;
    background: var(--panel-2);
    flex: 0 0 auto;
  }
  .segbtn {
    border: 0;
    background: transparent;
    color: var(--muted);
    font-size: 12px;
    font-weight: 550;
    padding: 4px 10px;
    border-radius: 999px;
    white-space: nowrap;
  }
  .segbtn.on {
    background: color-mix(in srgb, var(--accent) 16%, var(--panel-2));
    color: var(--text);
  }
  .segbtn:hover:not(.on) {
    color: var(--text);
  }
</style>
