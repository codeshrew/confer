<script lang="ts">
  // `?` — the which-key overlay (ui/REDESIGN.md: "opens a which-key-style
  // overlay of available keys, so nothing has to be memorized blind"). Pure
  // reference list, no state of its own beyond open/close.
  interface Props {
    open: boolean;
    onClose: () => void;
  }

  let { open, onClose }: Props = $props();

  let closeBtn = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    if (open) closeBtn?.focus();
  });

  // `<svelte:window>` must live at the component's top level (Svelte doesn't
  // allow it inside a block), so the open-guard moves into the handler
  // instead of around the tag.
  function handleKeydown(e: KeyboardEvent) {
    if (!open) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  interface KeyGroup {
    title: string;
    rows: { keys: string; desc: string }[];
  }

  const GROUPS: KeyGroup[] = [
    {
      title: 'Jump',
      rows: [{ keys: '⌘K / Ctrl+K', desc: 'command palette — fuzzy jump to a hub' }],
    },
    {
      title: 'Hub rail',
      rows: [
        { keys: 'j / k', desc: 'move selection down / up' },
        { keys: 'g g', desc: 'jump to first hub' },
        { keys: 'G', desc: 'jump to last hub' },
        { keys: '↵ / l', desc: 'open the selected hub' },
      ],
    },
    {
      title: 'Views',
      rows: [
        { keys: 'g 1', desc: 'Overview' },
        { keys: 'g 2', desc: 'Chat' },
        { keys: 'g 3', desc: 'Board' },
        { keys: 'g 4', desc: 'Fleet' },
        { keys: 'g 5', desc: 'Code' },
        { keys: 'g o / g b / g f', desc: 'letter aliases — Overview / Board / Fleet' },
      ],
    },
    {
      title: 'General',
      rows: [
        { keys: '?', desc: 'this overlay' },
        { keys: 'Esc', desc: 'close any overlay' },
      ],
    },
  ];
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div class="wk-overlay">
    <!-- aria-hidden: a bare click-outside-to-dismiss layer — a SIBLING of
       the dialog, not its parent (the dialog is real content that must stay
       in the accessibility tree; aria-hidden on an ANCESTOR of it would
       have hidden the whole dialog from assistive tech — matches
       App.svelte's `.scrim`, a sibling of the tri-pane for the same
       reason). -->
    <div class="wk-backdrop" onclick={onClose} aria-hidden="true" data-testid="whichkey-backdrop"></div>
    <div class="wk-panel" role="dialog" aria-modal="true" aria-label="Keyboard shortcuts" tabindex="-1">
      <div class="wk-head">
        <h2>Keyboard shortcuts</h2>
        <button type="button" class="wk-close" aria-label="Close" onclick={onClose} bind:this={closeBtn}>✕</button>
      </div>
      <div class="wk-body">
        {#each GROUPS as group (group.title)}
          <div class="wk-group">
            <div class="wk-gtitle">{group.title}</div>
            {#each group.rows as row (row.keys)}
              <div class="wk-row">
                <span class="wk-keys mono">{row.keys}</span>
                <span class="wk-desc">{row.desc}</span>
              </div>
            {/each}
          </div>
        {/each}
      </div>
      <div class="wk-foot">Never fires while typing in a field — this overlay itself included.</div>
    </div>
  </div>
{/if}

<style>
  .wk-overlay {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .wk-backdrop {
    position: absolute;
    inset: 0;
    background: rgba(4, 6, 10, 0.55);
  }
  .wk-panel {
    position: relative;
    z-index: 1;
    width: 560px;
    max-width: 92vw;
    max-height: 78vh;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .wk-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
    flex: 0 0 auto;
  }
  .wk-head h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 650;
  }
  .wk-close {
    width: 28px;
    height: 28px;
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--muted);
    border-radius: 7px;
  }
  .wk-close:hover {
    color: var(--text);
  }
  .wk-close:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .wk-body {
    overflow-y: auto;
    padding: 12px 16px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }
  .wk-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .wk-gtitle {
    font: 700 10.5px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--faint);
    margin-bottom: 2px;
  }
  .wk-row {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .wk-keys {
    flex: 0 0 auto;
    min-width: 92px;
    font-size: 11.5px;
    color: var(--accent);
    background: var(--panel-3);
    border: 1px solid var(--border-2);
    border-radius: 5px;
    padding: 2px 6px;
    text-align: center;
  }
  .wk-desc {
    font-size: 12px;
    color: var(--muted);
  }
  .wk-foot {
    padding: 10px 16px;
    border-top: 1px solid var(--border);
    font-size: 11px;
    color: var(--faint);
    flex: 0 0 auto;
  }
  @media (max-width: 620px) {
    .wk-body {
      grid-template-columns: 1fr;
    }
  }
</style>
