<script lang="ts">
  // The trust-tiered hub rail (ui/redesign-mockups/02-hub-nav.html, piece 2:
  // "Hub navigation & scale"). Replaces the horizontal hub tab-row (still
  // kept as TopBar's mobile fallback — see TopBar.svelte) with a persistent,
  // vertical, grouped-by-REAL-tier rail: Home (own) / Shared / Foreign /
  // Unclassified (null tier — its own bucket, never folded into Home; see
  // tierGroup below). Renders ONLY hubs `/api/hubs` actually returned —
  // never an illustrative/fake hub (REDESIGN.md law #3).
  //
  // Owns its own cross-hub `getAttention()` poll (same fan-out + cadence
  // Overview.svelte uses) so the health dot is the SAME real signal the
  // fleet map computes, not an independently-invented weaker one. Also owns
  // the ⌘K command palette (same data, natural single home for it) and
  // reports the current hub's tier up to App.svelte for the workspace tint.
  //
  // Keyboard (first slice of REDESIGN.md's keyboard-first model): j/k move
  // the roving-tabindex selection, `g g`/`G` jump to the first/last hub,
  // `Enter`/`l` opens the selected entry, `⌘K`/`Ctrl+K` opens the palette
  // from anywhere on the page (not just while the rail has focus).
  import { onDestroy, onMount } from 'svelte';
  import { getAttention } from '../api';
  import type { HubDomain } from '../attention';
  import { hubHealthReason } from '../attention';
  import type { HubTier } from '../types';
  import type { View } from '../stores.svelte';
  import { isCommandK, LEADER_TIMEOUT_MS } from '../keys';
  import CommandPalette from './CommandPalette.svelte';

  interface Props {
    currentHub: string;
    currentView: View;
    onHubChange?: (hubId: string) => void;
    /** The rail's "◆ All hubs" entry — switches to the cross-hub Overview
     * without changing `currentHub` (Overview doesn't have a single "current
     * hub" the way every other view does). */
    onAllHubs?: () => void;
    /** Fires whenever the resolved tier of `currentHub` changes (including
     * to `null`/unknown) — App.svelte's workspace tint reads this rather
     * than fetching its own copy of the same data. */
    onActiveTierChange?: (tier: HubTier | null) => void;
  }

  let { currentHub, currentView, onHubChange, onAllHubs, onActiveTierChange }: Props = $props();

  let loading = $state(true);
  let error = $state<string | null>(null);
  let domains = $state<HubDomain[]>([]);
  let paletteOpen = $state(false);

  const REFRESH_MS = 15000;
  let refreshTimer: ReturnType<typeof setInterval> | undefined;

  async function load() {
    try {
      const result = await getAttention();
      domains = result.domains;
      error = null;
    } catch (err) {
      console.error('confer serve: failed to load the hub rail', err);
      error = 'hubs unavailable';
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
    refreshTimer = setInterval(() => void load(), REFRESH_MS);
  });
  onDestroy(() => {
    clearInterval(refreshTimer);
  });

  $effect(() => {
    const active = domains.find((d) => d.hub === currentHub) ?? null;
    onActiveTierChange?.(active?.tier ?? null);
  });

  // ── grouping — own/shared/foreign/null, in this fixed display order ──────
  const GROUP_ORDER: { tier: HubTier | null; tierClass: string; label: string }[] = [
    { tier: 'own', tierClass: 'home', label: 'Home' },
    { tier: 'shared', tierClass: 'shared', label: 'Shared' },
    { tier: 'foreign', tierClass: 'foreign', label: 'Foreign' },
    { tier: null, tierClass: 'neutral', label: 'Unclassified' },
  ];

  type RailEntry =
    | { kind: 'all' }
    | { kind: 'group'; tierClass: string; label: string; count: number }
    | { kind: 'hub'; domain: HubDomain; tierClass: string };

  const railEntries = $derived.by((): RailEntry[] => {
    const entries: RailEntry[] = [{ kind: 'all' }];
    for (const g of GROUP_ORDER) {
      const inGroup = domains.filter((d) => d.tier === g.tier);
      if (inGroup.length === 0) continue;
      entries.push({ kind: 'group', tierClass: g.tierClass, label: g.label, count: inGroup.length });
      for (const domain of inGroup) entries.push({ kind: 'hub', domain, tierClass: g.tierClass });
    }
    return entries;
  });

  function entryKey(entry: RailEntry): string {
    if (entry.kind === 'all') return 'all';
    if (entry.kind === 'group') return `group:${entry.tierClass}`;
    return `hub:${entry.domain.hub}`;
  }

  // ── roving-tabindex keyboard nav (j/k, g g, G, Enter/l) ───────────────────
  let buttonEls = $state<(HTMLButtonElement | null)[]>([]);
  let focusedIdx = $state(0);
  let gArmed = false;
  let gTimer: ReturnType<typeof setTimeout> | undefined;

  function isNavigable(i: number): boolean {
    return railEntries[i]?.kind !== 'group' && railEntries[i] !== undefined;
  }

  function moveFocus(delta: number) {
    let i = focusedIdx;
    do {
      i += delta;
    } while (i >= 0 && i < railEntries.length && !isNavigable(i));
    if (i < 0 || i >= railEntries.length) return;
    focusedIdx = i;
    buttonEls[i]?.focus();
  }

  function moveToFirst() {
    const i = railEntries.findIndex((_, idx) => isNavigable(idx));
    if (i >= 0) {
      focusedIdx = i;
      buttonEls[i]?.focus();
    }
  }

  function moveToLast() {
    for (let i = railEntries.length - 1; i >= 0; i--) {
      if (isNavigable(i)) {
        focusedIdx = i;
        buttonEls[i]?.focus();
        return;
      }
    }
  }

  function activateEntry(entry: RailEntry) {
    if (entry.kind === 'all') onAllHubs?.();
    else if (entry.kind === 'hub') onHubChange?.(entry.domain.hub);
  }

  function activateFocused() {
    const entry = railEntries[focusedIdx];
    if (entry) activateEntry(entry);
  }

  function handleNavKeydown(e: KeyboardEvent) {
    const key = e.key;
    if (key === 'j' || key === 'ArrowDown') {
      e.preventDefault();
      gArmed = false;
      moveFocus(1);
      return;
    }
    if (key === 'k' || key === 'ArrowUp') {
      e.preventDefault();
      gArmed = false;
      moveFocus(-1);
      return;
    }
    if (key === 'G') {
      e.preventDefault();
      gArmed = false;
      moveToLast();
      return;
    }
    if (key === 'g') {
      if (gArmed) {
        e.preventDefault();
        moveToFirst();
        gArmed = false;
        clearTimeout(gTimer);
      } else {
        gArmed = true;
        clearTimeout(gTimer);
        gTimer = setTimeout(() => {
          gArmed = false;
        }, LEADER_TIMEOUT_MS);
      }
      return;
    }
    if (key === 'Enter' || key === 'l') {
      e.preventDefault();
      gArmed = false;
      activateFocused();
      return;
    }
    // Any other key cancels a half-armed `g g` — matches vim: a stray key
    // between the two g's just isn't the motion.
    gArmed = false;
  }

  onDestroy(() => clearTimeout(gTimer));

  // ⌘K works from anywhere on the page, not just while the rail has focus —
  // the whole point of a command palette (REDESIGN.md's macOS-native entry
  // point). Deliberately checked BEFORE any typing-target guard: opening the
  // palette while, say, composing a chat note is exactly when you'd reach
  // for it.
  function handleGlobalKeydown(e: KeyboardEvent) {
    if (isCommandK(e)) {
      e.preventDefault();
      paletteOpen = true;
    }
  }
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

<nav class="hr-rail" aria-label="hubs" data-testid="hub-rail">
  <button type="button" class="hr-jump" onclick={() => (paletteOpen = true)} data-testid="hub-rail-jump">
    <span>Jump to hub…</span>
    <span class="hr-kbd mono">⌘K</span>
  </button>

  <!-- role="toolbar", not "listbox": the WAI-ARIA pattern for exactly what's
       built here — a set of buttons with roving tabindex + arrow/vim-style
       keyboard navigation among them — without claiming selection-state
       semantics (aria-selected, role="option") this component doesn't
       implement. -->
  <div class="hr-list" role="toolbar" aria-orientation="vertical" aria-label="hubs" tabindex="-1" onkeydown={handleNavKeydown}>
    {#if loading && domains.length === 0}
      <div class="hr-status">loading hubs…</div>
    {:else if error && domains.length === 0}
      <div class="hr-status hr-status-err">{error}</div>
    {:else}
      {#each railEntries as entry, i (entryKey(entry))}
        {#if entry.kind === 'all'}
          <button
            type="button"
            class="hr-all"
            class:active={currentView === 'overview'}
            tabindex={i === focusedIdx ? 0 : -1}
            bind:this={buttonEls[i]}
            onfocus={() => (focusedIdx = i)}
            onclick={() => activateEntry(entry)}
            data-testid="hub-rail-all"
          >
            <span class="hr-di">◆</span>
            <span class="hr-allname">All hubs</span>
            <span class="hr-allmeta mono">fleet</span>
          </button>
        {:else if entry.kind === 'group'}
          <div class="hr-glab hr-glab-{entry.tierClass}">
            <span class="hr-gname">{entry.label}</span>
            <span class="hr-gcount mono">{entry.count}</span>
          </div>
        {:else}
          <button
            type="button"
            class="hr-hub hr-hub-{entry.tierClass}"
            class:active={entry.domain.hub === currentHub}
            tabindex={i === focusedIdx ? 0 : -1}
            bind:this={buttonEls[i]}
            onfocus={() => (focusedIdx = i)}
            onclick={() => activateEntry(entry)}
            title={hubHealthReason(entry.domain)}
            data-testid="hub-rail-hub"
          >
            <span class="hr-hname">{entry.domain.label}</span>
            <span class="hr-hbadge mono">{entry.domain.agents.length}</span>
            <span class="hr-hdot hr-hdot-{entry.domain.health}" aria-hidden="true"></span>
          </button>
        {/if}
      {/each}
    {/if}
  </div>
</nav>

<CommandPalette
  open={paletteOpen}
  {domains}
  onSelect={(hubId) => onHubChange?.(hubId)}
  onClose={() => (paletteOpen = false)}
/>

<style>
  .hr-rail {
    background: var(--panel);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 10px 8px;
  }
  .hr-jump {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 2px 10px;
    padding: 7px 9px;
    border-radius: 8px;
    border: 1px solid var(--border-2);
    background: var(--bg);
    color: var(--muted);
    font-size: 12px;
    text-align: left;
  }
  .hr-jump:hover {
    color: var(--text);
    border-color: var(--faint);
  }
  .hr-jump:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .hr-kbd {
    margin-left: auto;
    font-size: 10px;
    border: 1px solid var(--border-2);
    border-radius: 4px;
    padding: 1px 5px;
    color: var(--faint);
  }
  .hr-list {
    overflow-y: auto;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .hr-status {
    padding: 10px 8px;
    font-size: 12px;
    color: var(--faint);
    font-style: italic;
  }
  .hr-status-err {
    color: var(--blocked);
  }

  .hr-all {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 8px;
    border-radius: 8px;
    border: 0;
    background: transparent;
    color: var(--muted);
    font-size: 13px;
    font-weight: 600;
    margin-bottom: 4px;
  }
  .hr-all:hover {
    background: var(--panel-2);
  }
  .hr-all.active {
    background: var(--panel-3);
    color: var(--text);
  }
  .hr-all:focus-visible,
  .hr-hub:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .hr-di {
    color: var(--accent);
    font-family: var(--mono);
  }
  .hr-allmeta {
    margin-left: auto;
    font-size: 10px;
    color: var(--faint);
  }

  .hr-glab {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 8px 5px;
  }
  .hr-gname {
    font: 700 10px/1 var(--mono);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .hr-gcount {
    font-size: 10px;
    color: var(--faint);
  }
  .hr-glab-home .hr-gname {
    color: var(--home-frame);
  }
  .hr-glab-shared .hr-gname {
    color: var(--shared-frame);
  }
  .hr-glab-foreign .hr-gname {
    color: var(--foreign-frame);
  }
  .hr-glab-neutral .hr-gname {
    color: var(--neutral-frame);
  }

  .hr-hub {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px 6px 12px;
    border-radius: 7px;
    border: 0;
    background: transparent;
    color: var(--muted);
    font-size: 12.5px;
    text-align: left;
  }
  .hr-hub::before {
    content: '';
    position: absolute;
    left: 0;
    top: 5px;
    bottom: 5px;
    width: 2.5px;
    border-radius: 2px;
    opacity: 0.55;
  }
  .hr-hub-home::before {
    background: var(--home-frame);
  }
  .hr-hub-shared::before {
    background: var(--shared-frame);
  }
  .hr-hub-foreign::before {
    background: repeating-linear-gradient(var(--foreign-frame) 0 3px, transparent 3px 5px);
    opacity: 0.85;
  }
  .hr-hub-neutral::before {
    background: var(--neutral-frame);
  }
  .hr-hub:hover {
    background: var(--panel-2);
  }
  .hr-hub.active {
    background: var(--panel-3);
    color: var(--text);
    font-weight: 640;
  }
  .hr-hub.active::before {
    opacity: 1;
    width: 3px;
  }
  .hr-hname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hr-hbadge {
    font-size: 10px;
    color: var(--faint);
    background: var(--bg);
    border: 1px solid var(--border-2);
    border-radius: 5px;
    padding: 1px 5px;
    flex: 0 0 auto;
  }
  .hr-hdot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: 0 0 auto;
  }
  .hr-hdot-ok {
    background: var(--done);
  }
  .hr-hdot-warn {
    background: var(--blocked);
  }
  .hr-hdot-critical {
    background: var(--error);
  }
  .hr-hdot-unknown {
    background: var(--border-2);
  }

  @media (max-width: 1023.98px) {
    .hr-rail {
      display: none;
    }
  }
</style>
