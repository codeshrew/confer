<script lang="ts">
  // Chat-only now (see App.svelte's `appState.view === 'chat'` gate on this
  // component). Two rows were removed after a false-affordance audit found
  // both were dead: the STATUS chip row wrote to `statusFilter` state that
  // App.svelte never read back into Chat/Board, and the WHO chip row had no
  // onclick at all — clicking either did nothing. What's left (Type +
  // Density) are genuinely Chat concepts, hence the render-only-in-Chat
  // change. Board keeps its own group-by control instead.

  interface Props {
    notesOn: boolean;
    reqsOn: boolean;
    /** Chat density (Summary/Full segmented control) — omitted (undefined)
     * hides the control entirely. */
    chatDensity?: 'summary' | 'full';
    onToggleNotes?: () => void;
    onToggleReqs?: () => void;
    onChatDensityChange?: (density: 'summary' | 'full') => void;
  }

  let { notesOn, reqsOn, chatDensity, onToggleNotes, onToggleReqs, onChatDensityChange }: Props = $props();
</script>

<div class="filterbar">
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
