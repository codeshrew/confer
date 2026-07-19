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
    /** Whether the off-canvas left-rail drawer is currently open (tablet/phone only). */
    menuOpen?: boolean;
    /** design/43: Repos has no left rail at all (full-width card grid) — the
     * hamburger that would open it is hidden entirely, not just a no-op.
     * Every other view keeps it (default true). */
    showMenu?: boolean;
    onHubChange?: (hubId: string) => void;
    onViewChange?: (view: View) => void;
    onThemeToggle?: () => void;
    /** Hamburger tap — toggles the left-rail drawer. Only rendered/visible below 1024px. */
    onMenuToggle?: () => void;
  }

  let {
    hubs,
    currentHub,
    currentView,
    connStatus = 'live',
    theme = 'dark',
    menuOpen = false,
    showMenu = true,
    onHubChange,
    onViewChange,
    onThemeToggle,
    onMenuToggle,
  }: Props = $props();

  const CONN_LABEL: Record<ConnStatus, string> = {
    live: 'live · SSE',
    reconnecting: 'reconnecting…',
    loading: 'loading…',
  };

  // Overview is first — design/47 §3: it's the only cross-hub view (the
  // triage front door), everything else answers "what's happening in THIS
  // hub."
  const views: View[] = ['overview', 'chat', 'board', 'fleet', 'code', 'repos'];
  const viewLabel: Record<View, string> = {
    overview: 'Overview',
    chat: 'Chat',
    board: 'Board',
    fleet: 'Fleet',
    code: 'Code',
    repos: 'Repos',
  };
</script>

<div class="topbar">
  {#if showMenu}
    <button
      type="button"
      class="hamburger"
      class:open={menuOpen}
      aria-label={menuOpen ? 'Close menu' : 'Open menu'}
      aria-expanded={menuOpen}
      onclick={() => onMenuToggle?.()}
      data-testid="hamburger"
    >
      <span></span>
      <span></span>
      <span></span>
    </button>
  {/if}

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

  /* piece 2 (ui/REDESIGN.md): the persistent, trust-tiered HubRail replaces
     this row at desktop widths (≥1024px, where HubRail is visible). Below
     that HubRail hides itself (no room for a whole extra rail on tablet/
     phone — see HubRail.svelte's own media query), so this row stays as the
     mobile hub-switch fallback rather than leaving mobile with no way to
     change hubs at all. */
  .hubs {
    display: none;
    gap: 4px;
    margin-left: 6px;
  }
  @media (max-width: 1023.98px) {
    .hubs {
      display: flex;
    }
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

  /* Hamburger — hidden on desktop (≥1024px), where the left rail is always
     visible; surfaces at tablet/phone widths to open the off-canvas drawer. */
  .hamburger {
    display: none;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    gap: 4px;
    width: 36px;
    height: 36px;
    flex: 0 0 auto;
    border: 1px solid var(--border);
    background: var(--panel-2);
    border-radius: 8px;
  }

  .hamburger span {
    display: block;
    width: 16px;
    height: 2px;
    border-radius: 1px;
    background: var(--muted);
    transition: transform 0.18s ease, opacity 0.18s ease;
  }

  .hamburger.open span:nth-child(1) {
    transform: translateY(6px) rotate(45deg);
  }
  .hamburger.open span:nth-child(2) {
    opacity: 0;
  }
  .hamburger.open span:nth-child(3) {
    transform: translateY(-6px) rotate(-45deg);
  }

  @media (max-width: 1023.98px) {
    .hamburger {
      display: flex;
    }
  }

  @media (max-width: 767.98px) {
    .topbar {
      flex-wrap: wrap;
      height: auto;
      min-height: 52px;
      padding: 8px 10px;
      gap: 8px 10px;
    }
    .hubs {
      order: 3;
      width: 100%;
      margin-left: 0;
      overflow-x: auto;
    }
    .live {
      display: none;
    }
    .spacer {
      display: none;
    }
    .seg {
      margin-left: auto;
    }
    .hamburger,
    .icon-btn {
      width: 40px;
      height: 40px;
    }
    .seg button {
      min-height: 40px;
    }
  }
</style>
