<script lang="ts">
  // The "Mini" tier for CODE refs in piece 6's Related column
  // (ui/REDESIGN.md, "the composable card system" — `05-composable-cards.html`'s
  // `.c-mini`). Deliberately lighter than CodeRefCard.svelte: no snippet
  // fetch, no Shiki highlight — just the ref's own static metadata (path,
  // range, repo), which is already on hand. Portals to the SAME reverse-
  // index destination CodeRefCard's own revhook does (one fetch, on
  // click, not on mount) — a mini card that never fetches until the
  // reader actually asks for more, matching "mini = a portal" from the
  // composable-card rationale.
  import type { CodeRef, RefHit } from '../types';
  import { api } from '../api';

  interface Props {
    ref: CodeRef;
    hub: string;
    selected?: boolean;
    onOpenRefs?: (ref: CodeRef, hits: RefHit[]) => void;
  }

  let { ref, hub, selected = false, onOpenRefs }: Props = $props();

  const rangeLabel = $derived(ref.range ? `L${ref.range[0]}–${ref.range[1]}` : null);

  async function open() {
    const target = `${ref.repo}:${ref.path}${ref.range ? `@${ref.range[0]}-${ref.range[1]}` : ''}`;
    try {
      const hits = await api.getRefs(hub, target, true);
      onOpenRefs?.(ref, hits);
    } catch (err) {
      console.error('confer serve: failed to load reverse-index hits', target, err);
    }
  }
</script>

<button type="button" class="c-mini" class:sel={selected} onclick={open} data-testid="code-ref-mini">
  <div class="top">
    <span class="ic" aria-hidden="true">◆</span>
    <span class="path mono">{ref.path}</span>
    {#if rangeLabel}<span class="ln mono">{rangeLabel}</span>{/if}
  </div>
  <div class="sub mono">{ref.repo}</div>
</button>

<style>
  .c-mini {
    width: 100%;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px 9px;
    text-align: left;
    font: inherit;
    color: inherit;
    cursor: pointer;
    transition: border-color 0.12s ease;
  }
  .c-mini:hover,
  .c-mini:focus-visible {
    border-color: var(--accent);
  }
  .c-mini.sel {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 28%, transparent);
  }
  .c-mini .top {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--text);
  }
  .c-mini .ic {
    color: var(--accent);
  }
  .c-mini .path {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .c-mini .ln {
    margin-left: auto;
    color: var(--muted);
    font-size: 10px;
    flex: 0 0 auto;
  }
  .c-mini .sub {
    margin-top: 3px;
    font-size: 10px;
    color: var(--faint);
  }
</style>
