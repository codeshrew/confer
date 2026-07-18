<script lang="ts">
  // Shared click-to-copy affordance for the bare `msg_…`/`req_…` id — used
  // on Message's header, BoardRow, RequestDetail's ctx-head (App.svelte),
  // and MetaThread's `.gid` line (design/41 Phase 0, §4 "copy-id cluster").
  //
  // Hidden by default (opacity: 0) so it doesn't visually compete with the
  // rest of the row; each HOST component reveals it on the row's own
  // `:hover`/`:focus-within` via `:global(.copy-id-btn)` in ITS OWN scoped
  // style (Svelte scoped CSS can't reach into a child component's markup
  // otherwise). `(hover: none)` below (touch/no-pointer devices) always
  // shows it, per design/41.
  import Icon from './Icon.svelte';
  import { copyToClipboard } from '../clipboard';

  interface Props {
    /** The bare id to copy — e.g. `msg_a1b2` or `req_c3d4`. */
    id: string;
    /** Extra classes for host-specific placement/sizing. */
    class?: string;
  }

  let { id, class: klass = '' }: Props = $props();

  let copied = $state(false);
  let resetTimer: ReturnType<typeof setTimeout> | undefined;

  async function handleClick(e: MouseEvent) {
    // This button is nearly always nested inside a clickable row (a note,
    // board row, meta-thread node) — never let the copy action ALSO fire
    // the row's own onclick (select/navigate).
    e.stopPropagation();
    const ok = await copyToClipboard(id);
    if (!ok) return;
    copied = true;
    clearTimeout(resetTimer);
    resetTimer = setTimeout(() => {
      copied = false;
    }, 1200);
  }
</script>

<button
  type="button"
  class="copy-id-btn {klass}"
  class:copied
  onclick={handleClick}
  aria-label={copied ? `Copied ${id}` : `Copy id ${id}`}
  title={id}
  data-testid="copy-id-btn"
>
  <Icon name={copied ? 'check' : 'copy'} size={12} />
</button>

<style>
  .copy-id-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: 20px;
    height: 20px;
    padding: 0;
    border: 1px solid var(--border-2);
    border-radius: 5px;
    background: var(--panel-2);
    color: var(--muted);
    cursor: pointer;
    opacity: 0;
    transition:
      opacity 0.12s ease,
      color 0.12s ease,
      border-color 0.12s ease,
      background 0.12s ease;
  }
  .copy-id-btn:hover,
  .copy-id-btn:focus-visible {
    opacity: 1;
    color: var(--text);
    border-color: var(--accent);
    background: var(--panel-3);
  }
  .copy-id-btn.copied {
    opacity: 1;
    color: var(--done);
    border-color: var(--done);
  }
  /* Touch/no-hover devices (design/41: "always-visible (touch)") — there is
     no hover state to reveal it via, so it must default to visible. */
  @media (hover: none) {
    .copy-id-btn {
      opacity: 1;
    }
  }
</style>
