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

  // keyboard-architecture pass (ui/REDESIGN.md, 2026-07-19) — the three-
  // layer model. "Panes" (Layer 1) and "App" (Layer 3) work everywhere,
  // regardless of which pane is focused; every other group is Layer 2 —
  // bare keys that only fire while THAT pane holds real focus (the focus
  // chip in the crumb bar always shows which one that is).
  const GROUPS: KeyGroup[] = [
    {
      title: 'Panes (Ctrl)',
      rows: [
        { keys: 'Ctrl+h/j/k/l', desc: 'move focus between panes, by position' },
        { keys: 'Ctrl+] / Ctrl+[', desc: 'cycle to the next / previous pane' },
        { keys: 'F6 / Shift+F6', desc: 'same, browser-safe fallback' },
      ],
    },
    {
      title: 'App (Cmd)',
      rows: [
        { keys: '⌘K', desc: 'command palette — fuzzy jump to a hub' },
        { keys: '⌘1', desc: 'Overview' },
        { keys: '⌘2', desc: 'Chat' },
        { keys: '⌘3', desc: 'Board' },
        { keys: '⌘4', desc: 'Fleet' },
        { keys: '⌘5', desc: 'Code' },
        { keys: '?', desc: 'this overlay' },
      ],
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
      title: 'Topic list',
      rows: [
        { keys: 'j / k', desc: 'move selection down / up' },
        { keys: '↵ / l', desc: 'open the selected topic' },
      ],
    },
    {
      title: 'Chat stream',
      rows: [{ keys: 'j / k', desc: 'select next / previous message' }],
    },
    {
      title: 'Thread map',
      rows: [
        { keys: 'click / ↵', desc: 'jump the stream to that message' },
        { keys: 'hover', desc: 'preview the text without jumping' },
        { keys: 'j / k', desc: 'move the local pointer along the map' },
        { keys: 'l', desc: 'deeper — focus a reply' },
        { keys: 'h', desc: 'back — focus the parent' },
        { keys: 'y', desc: 'copy the focused node\'s full id' },
        { keys: 'Esc', desc: 'close the peek' },
      ],
    },
    {
      title: 'Focus reader',
      rows: [
        { keys: 'f', desc: 'open/close — reads the focused message' },
        { keys: 'j / k', desc: 'prev / next message in thread' },
        { keys: 'y', desc: 'copy the message\'s full id' },
        { keys: 'Esc', desc: 'exit focus' },
      ],
    },
    {
      title: 'General',
      rows: [{ keys: 'Esc', desc: 'close any overlay' }],
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
      <div class="wk-foot">
        <p>Ctrl = panes · bare = content · Cmd = app · only the focused pane's keys fire</p>
        <p>Never fires while typing in a field — this overlay itself included.</p>
      </div>
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
  .wk-foot p {
    margin: 0;
  }
  .wk-foot p + p {
    margin-top: 4px;
  }
  @media (max-width: 620px) {
    .wk-body {
      grid-template-columns: 1fr;
    }
  }
</style>
