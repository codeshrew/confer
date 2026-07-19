<script lang="ts">
  // The persistent focus chip (ui/REDESIGN.md, keyboard-architecture pass):
  // "exactly one pane active, shown by a focus chip" — so the operator
  // always knows which pane's bare keys (Layer 2) will fire, without having
  // to guess from a border color alone. Reads `paneFocus` directly (a
  // module-level singleton, same pattern as `appState`) — no props needed,
  // it's always showing the one true focused pane.
  import { paneFocus } from '../paneFocus.svelte';
</script>

{#if paneFocus.focusedLabel}
  <span
    class="focus-chip"
    data-testid="focus-chip"
    title="Ctrl+h/j/k/l (or Ctrl+]/[, or F6/Shift+F6) moves focus between panes — only the focused pane's own keys fire"
  >
    <span class="fc-dot" aria-hidden="true"></span>
    {paneFocus.focusedLabel}
  </span>
{/if}

<style>
  .focus-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font: 600 10.5px/1 var(--mono);
    color: var(--muted);
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    border-radius: 999px;
    padding: 4px 9px 4px 7px;
  }
  .fc-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
    flex: 0 0 auto;
  }
</style>
