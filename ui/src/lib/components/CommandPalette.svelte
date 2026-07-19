<script lang="ts">
  // The ⌘K command palette (ui/redesign-mockups/02-hub-nav.html, piece 2's
  // keyboard-first slice) — fuzzy jump to any REGISTERED hub. Scope for now
  // is hub-jump only; structured so a later piece can add threads/actions to
  // the same result list without reshaping this component (see `Result`).
  //
  // Data comes from HubRail's own `domains` (real per-hub tier/health, the
  // same projection Overview's fleet map renders from) — never a hardcoded
  // or illustrative hub list (law #3 / REDESIGN.md's hub-nav footnote: "Only
  // [N] are live today ... the others illustrate ... a null would read
  // unknown, never a fake green").
  import { tick } from 'svelte';
  import type { HubDomain } from '../attention';
  import { hubHealthReason } from '../attention';
  import { fuzzyFilter } from '../fuzzy';

  interface Props {
    open: boolean;
    domains: HubDomain[];
    onSelect: (hubId: string) => void;
    onClose: () => void;
  }

  let { open, domains, onSelect, onClose }: Props = $props();

  let query = $state('');
  let selected = $state(0);
  let inputEl = $state<HTMLInputElement | null>(null);

  const results = $derived(fuzzyFilter(domains, query, (d) => d.label));

  // Reopening always starts from a clean slate — a stale query/selection
  // from the last time it was open would be confusing, not a "recent
  // search" feature anyone asked for.
  $effect(() => {
    if (open) {
      query = '';
      selected = 0;
      void tick().then(() => inputEl?.focus());
    }
  });

  // Clamp `selected` whenever the filtered result count changes (typing
  // narrows the list out from under whatever index was selected).
  $effect(() => {
    if (selected >= results.length) selected = Math.max(0, results.length - 1);
  });

  function tierLabel(tier: HubDomain['tier']): string {
    return tier ?? 'unclassified';
  }

  function choose(domain: HubDomain) {
    onSelect(domain.hub);
    onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (results.length > 0) selected = (selected + 1) % results.length;
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (results.length > 0) selected = (selected - 1 + results.length) % results.length;
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const hit = results[selected];
      if (hit) choose(hit);
    }
  }
</script>

{#if open}
  <div class="cp-overlay">
    <!-- aria-hidden: a bare click-outside-to-dismiss layer — a SIBLING of
       the dialog, not its parent (the dialog is real content that must stay
       in the accessibility tree; aria-hidden on an ANCESTOR of it would
       have hidden the whole dialog from assistive tech, not just this
       backdrop — matches App.svelte's `.scrim`, which is a sibling of the
       tri-pane for the same reason). -->
    <div class="cp-backdrop" onclick={onClose} aria-hidden="true" data-testid="palette-backdrop"></div>
    <div class="cp-panel" role="dialog" aria-modal="true" aria-label="Jump to hub" tabindex="-1" data-testid="command-palette">
      <div class="cp-q">
        <span class="cp-cur mono">⌘K</span>
        <input
          bind:this={inputEl}
          bind:value={query}
          onkeydown={handleKeydown}
          type="text"
          placeholder="Jump to hub…"
          aria-label="Jump to hub"
          autocomplete="off"
          spellcheck="false"
          data-testid="palette-input"
        />
        <span class="cp-hint mono">↑↓ move · ↵ open · esc close</span>
      </div>
      <div class="cp-results">
        {#if results.length === 0}
          <div class="cp-empty">no hub matches "{query}"</div>
        {:else}
          {#each results as domain, i (domain.hub)}
            <button
              type="button"
              class="cp-row"
              class:sel={i === selected}
              onmouseenter={() => (selected = i)}
              onclick={() => choose(domain)}
              data-testid="palette-row"
            >
              <span class="cp-tier cp-tier-{domain.tier ?? 'none'}">{tierLabel(domain.tier)}</span>
              <span class="cp-name">{domain.label}</span>
              <span class="cp-health cp-health-{domain.health}">{hubHealthReason(domain)}</span>
            </button>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .cp-overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 14vh;
  }
  .cp-backdrop {
    position: absolute;
    inset: 0;
    background: rgba(4, 6, 10, 0.55);
  }
  .cp-panel {
    position: relative;
    z-index: 1;
    width: 520px;
    max-width: 92vw;
    max-height: 60vh;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .cp-q {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
    flex: 0 0 auto;
  }
  .cp-cur {
    color: var(--accent);
    font-size: 11.5px;
    font-weight: 700;
  }
  .cp-q input {
    all: unset;
    flex: 1;
    color: var(--text);
    font-size: 14px;
    min-width: 0;
  }
  .cp-hint {
    font-size: 10.5px;
    color: var(--faint);
    white-space: nowrap;
    flex: 0 0 auto;
  }
  .cp-results {
    overflow-y: auto;
    padding: 6px;
  }
  .cp-empty {
    padding: 14px 10px;
    font-size: 12.5px;
    color: var(--faint);
    font-style: italic;
    text-align: center;
  }
  .cp-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 8px 9px;
    border-radius: 7px;
    border: 0;
    background: transparent;
    color: var(--muted);
    text-align: left;
    font-size: 13px;
  }
  .cp-row.sel {
    background: var(--panel-2);
  }
  .cp-row.sel .cp-name {
    color: var(--text);
  }
  .cp-row:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .cp-tier {
    font: 700 9.5px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    padding: 3px 6px;
    border-radius: 4px;
    flex: 0 0 auto;
    color: var(--faint);
    background: var(--panel-3);
  }
  .cp-tier-own,
  .cp-tier-shared {
    color: var(--home-frame);
    background: color-mix(in srgb, var(--home-frame) 15%, transparent);
  }
  .cp-tier-foreign {
    color: var(--foreign-frame);
    background: color-mix(in srgb, var(--foreign-frame) 15%, transparent);
  }
  .cp-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cp-health {
    flex: 0 0 auto;
    font: 500 10.5px/1 var(--mono);
    color: var(--faint);
  }
  .cp-health-warn {
    color: var(--blocked);
  }
  .cp-health-critical {
    color: var(--error);
  }
</style>
