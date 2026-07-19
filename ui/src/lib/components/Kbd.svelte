<script lang="ts">
  // The "glanceable shortcuts" chip (ui/REDESIGN.md, keyboard-architecture
  // pass, operator directive 2026-07-19): every actionable control shows its
  // own shortcut inline, so the three-layer model is learned passively just
  // by using the UI, not by memorizing the `?` overlay. One component, used
  // everywhere a shortcut needs a label — a button's own inline hint, a tab,
  // a menu row.
  interface Props {
    /** The literal label to render, e.g. "⌘1", "Ctrl+K", "j". Not parsed —
     * callers spell out exactly what the reader should press. */
    keys: string;
    class?: string;
  }

  let { keys, class: klass = '' }: Props = $props();
</script>

<!-- aria-hidden: purely a visual hint layered on top of a control that
     already has its own accessible name (button text, aria-label) — without
     this, the chip's own text would get appended into that name (e.g. a
     "Board" tab becoming "Board ⌘3"), breaking name-based queries/matchers
     and doubling up what a screen reader announces. -->
<kbd class="kbd {klass}" aria-hidden="true">{keys}</kbd>

<style>
  .kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font: 600 10px/1 var(--mono);
    color: var(--faint);
    border: 1px solid var(--border-2);
    border-radius: 4px;
    padding: 2px 5px;
    background: var(--panel-2);
    white-space: nowrap;
  }
</style>
