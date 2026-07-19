<script lang="ts">
  // A tiny inline confirmation for a copy triggered from the KEYBOARD (`y`
  // — FocusReader, MetaThread's peek), where there's no button under the
  // pointer for CopyIdButton's own hover-swap feedback to live on. The
  // parent owns the timer (sets `text`, clears it back to `null` after
  // ~1.5s) — this component is purely presentational, so it stays trivial
  // to reuse from both call sites without duplicating timer logic here.
  interface Props {
    /** The message to show, e.g. "copied msg_01jq…" — `null` renders
     * nothing. */
    text: string | null;
  }

  let { text }: Props = $props();
</script>

{#if text}
  <span class="copied-toast" role="status" data-testid="copied-toast">{text}</span>
{/if}

<style>
  .copied-toast {
    display: inline-flex;
    align-items: center;
    font: 600 10.5px/1 var(--mono);
    color: var(--done);
    border: 1px solid color-mix(in srgb, var(--done) 40%, var(--border-2));
    background: color-mix(in srgb, var(--done) 12%, transparent);
    border-radius: 999px;
    padding: 4px 9px;
    white-space: nowrap;
  }
</style>
