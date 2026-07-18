<script lang="ts">
  import type { Hub } from '../types';
  import type { View } from '../stores.svelte';

  export type ConnStatus = 'live' | 'reconnecting' | 'loading';

  interface Props {
    hubs: Hub[];
    currentHub: string;
    currentView: View;
    connStatus?: ConnStatus;
    theme?: 'dark' | 'light';
    onHubChange?: (hubId: string) => void;
    onViewChange?: (view: View) => void;
    onThemeToggle?: () => void;
  }

  let {
    hubs,
    currentHub,
    currentView,
    connStatus = 'live',
    theme = 'dark',
    onHubChange,
    onViewChange,
    onThemeToggle,
  }: Props = $props();

  const CONN_LABEL: Record<ConnStatus, string> = {
    live: 'live · SSE',
    reconnecting: 'reconnecting…',
    loading: 'loading…',
  };

  const views: View[] = ['chat', 'board', 'fleet', 'code'];
  const viewLabel: Record<View, string> = {
    chat: 'Chat',
    board: 'Board',
    fleet: 'Fleet',
    code: 'Code',
  };
</script>

<div class="topbar">
  <div class="brand">
    <span class="glyph">c</span>
    <span>confer</span>
    <span class="tag">serve</span>
  </div>

  <div class="hubs" role="tablist" aria-label="Hubs">
    {#each hubs as hub (hub.id)}
      <button
        type="button"
        class="hub"
        class:active={hub.id === currentHub}
        role="tab"
        aria-selected={hub.id === currentHub}
        onclick={() => onHubChange?.(hub.id)}
      >
        <span>{hub.label}</span>
        <span class="cnt">{hub.agentCount}</span>
      </button>
    {/each}
  </div>

  <div class="spacer"></div>

  <div
    class="live"
    class:reconnect={connStatus === 'reconnecting'}
    class:loading={connStatus === 'loading'}
    data-testid="live-indicator"
  >
    <span class="pip"></span>
    <span>{CONN_LABEL[connStatus]}</span>
  </div>

  <div class="seg" role="tablist" aria-label="View">
    {#each views as v (v)}
      <button
        type="button"
        class:on={currentView === v}
        role="tab"
        aria-selected={currentView === v}
        onclick={() => onViewChange?.(v)}
      >
        {viewLabel[v]}
      </button>
    {/each}
  </div>

  <button
    type="button"
    class="icon-btn"
    title="Toggle theme"
    aria-label="Toggle theme"
    onclick={() => onThemeToggle?.()}
  >
    {theme === 'dark' ? '◐' : '◑'}
  </button>
</div>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 0 16px;
    height: 52px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
    flex: 0 0 auto;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .brand .glyph {
    width: 22px;
    height: 22px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    background: linear-gradient(140deg, var(--accent), #1c8f83);
    color: #04120f;
    font-family: var(--mono);
    font-weight: 800;
    font-size: 13px;
  }

  .brand .tag {
    font: 600 10px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--faint);
    border: 1px solid var(--border-2);
    border-radius: 5px;
    padding: 3px 5px;
  }

  .hubs {
    display: flex;
    gap: 4px;
    margin-left: 6px;
  }

  .hub {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 5px 11px;
    border-radius: 8px;
    background: transparent;
    border: 1px solid transparent;
    color: var(--muted);
    font-size: 12.5px;
    font-weight: 550;
  }

  .hub:hover {
    background: var(--panel-2);
    color: var(--text);
  }

  .hub.active {
    background: var(--panel-3);
    border-color: var(--border-2);
    color: var(--text);
  }

  .hub .cnt {
    font: 600 10.5px/1 var(--mono);
    color: var(--faint);
  }

  .hub.active .cnt {
    color: var(--accent);
  }

  .spacer {
    flex: 1;
  }

  .live {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--muted);
    font-size: 12px;
  }

  .live .pip {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--done);
    box-shadow: 0 0 0 0 rgba(70, 198, 107, 0.5);
    animation: pulse 2.4s infinite;
  }

  @keyframes pulse {
    0% {
      box-shadow: 0 0 0 0 rgba(70, 198, 107, 0.45);
    }
    70% {
      box-shadow: 0 0 0 7px rgba(70, 198, 107, 0);
    }
    100% {
      box-shadow: 0 0 0 0 rgba(70, 198, 107, 0);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .live .pip {
      animation: none;
    }
  }

  .live.reconnect .pip {
    background: var(--blocked);
    animation: none;
    box-shadow: none;
  }

  .live.reconnect {
    color: var(--blocked);
  }

  .live.loading .pip {
    background: var(--claimed);
    animation: none;
    box-shadow: none;
  }

  .seg {
    display: flex;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 2px;
  }

  .seg button {
    border: 0;
    background: transparent;
    color: var(--muted);
    font-size: 12.5px;
    font-weight: 600;
    padding: 5px 12px;
    border-radius: 6px;
  }

  .seg button.on {
    background: var(--panel-3);
    color: var(--text);
    box-shadow: inset 0 0 0 1px var(--border-2);
  }

  .icon-btn {
    border: 1px solid var(--border);
    background: var(--panel-2);
    color: var(--muted);
    width: 32px;
    height: 32px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    font-size: 14px;
  }

  .icon-btn:hover {
    color: var(--text);
  }
</style>
