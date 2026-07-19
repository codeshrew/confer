<script lang="ts">
  // Ports `.emptystate`/`.es-*` from design/serve-dashboard-v2-mockup.html —
  // the shared empty/unmapped-clone/no-refs affordance used across the chat
  // stream, CodeRefCard's unresolved-snippet fallback, the Code lens's
  // "map a clone" stub, and the reverse-index panel's "no conversations yet".
  interface Props {
    glyph: string;
    title: string;
    body: string;
    actionLabel?: string | null;
    disabled?: boolean;
    onAction?: () => void;
  }

  let { glyph, title, body, actionLabel = null, disabled = false, onAction }: Props = $props();
</script>

<div class="emptystate">
  <div class="es-glyph">{glyph}</div>
  <div class="es-title">{title}</div>
  <div class="es-body">{body}</div>
  {#if actionLabel}
    <button type="button" class="es-btn" {disabled} onclick={() => onAction?.()}>{actionLabel}</button>
  {/if}
</div>

<style>
  .emptystate {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 9px;
    max-width: 420px;
    margin: 20px auto 0;
    padding: 0 20px;
    text-align: left;
  }
  .es-glyph {
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--panel-2);
    border: 1px solid var(--border-2);
    display: grid;
    place-items: center;
    font: 700 16px/1 var(--mono);
    color: var(--faint);
    margin-bottom: 4px;
  }
  .es-title {
    font-weight: 650;
    font-size: 14px;
    color: var(--text);
  }
  .es-body {
    font-size: 12.5px;
    color: var(--muted);
    line-height: 1.55;
  }
  .es-btn {
    margin-top: 6px;
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--muted);
    font: 600 11.5px/1 var(--sans);
    padding: 7px 12px;
    border-radius: 7px;
  }
  .es-btn:hover {
    color: var(--text);
    border-color: var(--faint);
  }
  .es-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
