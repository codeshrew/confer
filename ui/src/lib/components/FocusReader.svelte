<script lang="ts">
  // The focus reader (ui/redesign-mockups/03-thread-nav.html, piece 3, "C" —
  // three depths: skim (stream) -> explore (peek) -> read (this)). `f`
  // anywhere on a focused message drops it into a distraction-free,
  // prose-typeset single-message view — the stream/graph/chrome stripped,
  // the words getting real reading typography instead of the stream's
  // clamped/compact treatment.
  //
  // Reuses the SAME real data the peek uses (thread.ts's buildTrail over the
  // already-fetched `thread`/`messages`) for j/k "prev · next in thread" —
  // one shared mental model of "what thread is this," not a second one.
  // Code blocks/refs inside the body go through the SAME markdown +
  // highlight.ts pipeline every other message body uses (no reinvented
  // renderer) — "code blocks render in the code view" means the same Shiki
  // tokenizer, not a bespoke one.
  //
  // DEFERRED TO PIECE 4 BY DESIGN (piece 3, logged in ui/REDESIGN.md — not
  // blocked): the mockup's gutter shows a "seen" line. Real per-message
  // `seenBy` now EXISTS (Herald shipped it, b776c94, design/48 #62, from the
  // published read-frontier) — so this isn't ChatStream's old CONTRACT GAP
  // #58 synthesized-filler problem. It's simply not wired HERE: read-state
  // (this line, Chat's synthesis, any "since you last looked" watermark)
  // belongs together in piece 4, done holistically rather than scattered as
  // a one-off. The gutter keeps author + refs (both real); "seen" lands for
  // free once piece 4 wires the real projection app-wide.
  import { renderMarkdown, highlightRenderedCodeBlocks } from '../markdown';
  import { formatClock, formatIso8601, formatLocalDateTime } from '../format';
  import { buildTrail, type TrailNode } from '../thread';
  import { isTypingTarget } from '../keys';
  import { api } from '../api';
  import type { Agent, CodeRef, Message as MessageT, RefHit, ThreadNode } from '../types';

  interface Props {
    open: boolean;
    msgId: string | null;
    messages: MessageT[];
    agents: Agent[];
    thread: ThreadNode[];
    hub: string;
    /** j/k — move to the prev/next message in the SAME thread. A pure focus
     * move (like the peek's own h/l/j/k) — never refetches, since the trail
     * covering `msgId` already covers every node it could walk to. */
    onNavigate?: (msgId: string) => void;
    onOpenRefs?: (ref: CodeRef, hits: RefHit[]) => void;
    onClose?: () => void;
  }

  let { open, msgId, messages, agents, thread, hub, onNavigate, onOpenRefs, onClose }: Props = $props();

  const agentsById = $derived(new Map(agents.map((a) => [a.id, a])));
  const message = $derived(messages.find((m) => m.id === msgId) ?? null);
  const agent = $derived(message ? agentsById.get(message.from) : undefined);

  // Same real trail the peek renders — reused, not recomputed differently,
  // so "next in thread" here means the exact same thing it means there.
  const trail = $derived(buildTrail(thread, messages));
  const index = $derived(msgId ? trail.findIndex((n) => n.msgId === msgId) : -1);
  const prevNode = $derived(index > 0 ? (trail[index - 1] ?? null) : null);
  const nextNode = $derived(index >= 0 && index < trail.length - 1 ? (trail[index + 1] ?? null) : null);

  const renderedBody = $derived(message ? renderMarkdown(message.body) : null);
  let bodyEl: HTMLElement | undefined = $state();
  $effect(() => {
    void renderedBody;
    if (bodyEl) void highlightRenderedCodeBlocks(bodyEl);
  });

  function shortId(id: string): string {
    return id.length > 10 ? `${id.slice(0, 8)}…` : id;
  }

  function goTo(node: TrailNode | null) {
    if (node) onNavigate?.(node.msgId);
  }

  // The gutter's compact ref chips don't render CodeRefCard's own full
  // preview (too wide for a narrow gutter column) — but the reverse-index
  // hook still needs REAL hit data, not a fabricated empty array implying
  // "nothing references this." Same `getRefs(hub, target, true)` call
  // CodeRefCard itself makes before firing its own onRevHook.
  async function openRefHits(ref: CodeRef) {
    const target = `${ref.repo}:${ref.path}${ref.range ? `@${ref.range[0]}-${ref.range[1]}` : ''}`;
    try {
      const hits = await api.getRefs(hub, target, true);
      onOpenRefs?.(ref, hits);
    } catch (err) {
      console.error('confer serve: failed to load reverse-index hits', target, err);
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!open) return;
    if (isTypingTarget(e.target)) return;
    switch (e.key) {
      case 'j':
        e.preventDefault();
        goTo(nextNode);
        break;
      case 'k':
        e.preventDefault();
        goTo(prevNode);
        break;
      case 'Escape':
        // `f` itself is deliberately NOT handled here — App.svelte owns the
        // open/close toggle for it (the single global handler that also
        // decides whether `f` should open the reader in the first place),
        // so there's exactly one source of truth instead of two listeners
        // racing on the same keypress.
        e.preventDefault();
        onClose?.();
        break;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open && message}
  <div class="fr-overlay">
    <div class="fr-backdrop" onclick={onClose} aria-hidden="true" data-testid="reader-backdrop"></div>
    <div class="fr-panel" role="dialog" aria-modal="true" aria-label="Focus reader" tabindex="-1" data-testid="focus-reader">
      <div class="fr-head">
        <div class="fr-crumbs mono">
          {#if message.topic}<span class="cz">#{message.topic}</span><span class="sep">›</span>{/if}
          <span class="cz here">{shortId(message.id)}</span>
        </div>
        <span class="fr-badge">◉ focus read</span>
        <div class="fr-nav mono">
          {#if prevNode}
            <button type="button" class="fr-hop" onclick={() => goTo(prevNode)} data-testid="reader-prev">◂ {shortId(prevNode.msgId)}</button>
          {/if}
          <b>{shortId(message.id)}</b>
          {#if nextNode}
            <button type="button" class="fr-hop" onclick={() => goTo(nextNode)} data-testid="reader-next">{shortId(nextNode.msgId)} ▸</button>
          {/if}
          <span class="fr-kk-inline"><span class="kk">j</span><span class="kk">k</span> prev·next</span>
        </div>
        <button type="button" class="fr-close" aria-label="Close focus reader" onclick={onClose}>✕</button>
      </div>

      <div class="fr-body">
        <aside class="fr-gutter">
          <div class="glab">from</div>
          <div class="who">{agent?.display ?? message.from}</div>
          <div class="whosub">{message.host ?? '—'} · {formatClock(message.ts)}</div>
          {#if message.refs.length}
            <div class="glab">refs</div>
            {#each message.refs as ref (ref.repo + ':' + ref.path + '@' + ref.sha)}
              <button type="button" class="gref" onclick={() => void openRefHits(ref)} title={`${ref.repo}/${ref.path}`}>
                {ref.path}
              </button>
            {/each}
          {/if}
          <div class="glab">timestamp</div>
          <!-- Local primary, UTC alongside it — "both, for clarity" (design/48).
               Local is what the operator actually reads at a glance; the ISO
               instant is the unambiguous wire-format fact underneath it. -->
          <div class="whosub">{formatLocalDateTime(message.ts)}</div>
          <div class="whosub mono dim">{formatIso8601(message.ts)}</div>
        </aside>
        <article class="fr-reading prose md" bind:this={bodyEl}>
          {#if renderedBody}
            {@html renderedBody}
          {:else}
            <p>{message.summary}</p>
          {/if}
        </article>
      </div>

      <div class="fr-keys">
        <span><span class="kk">j</span><span class="kk">k</span> prev · next in thread</span>
        <span><span class="kk">f</span> exit focus</span>
        <span><span class="kk">esc</span> exit focus</span>
      </div>
    </div>
  </div>
{/if}

<style>
  .fr-overlay {
    position: fixed;
    inset: 0;
    z-index: 65;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--phi2, 24px);
  }
  .fr-backdrop {
    position: absolute;
    inset: 0;
    background: rgba(4, 6, 10, 0.6);
  }
  .fr-panel {
    position: relative;
    z-index: 1;
    width: 100%;
    /* Widened per design feedback (2026-07-18) — the original 880px/63ch
       read cramped for actual reading. 72ch stays under the ~75ch upper
       bound where a line starts hurting readability; the panel itself has
       enough headroom past that for the gutter + margins to breathe. */
    max-width: 1080px;
    max-height: 88vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    border: 1px solid var(--border-2);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .fr-head {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    flex-wrap: wrap;
  }
  .fr-crumbs {
    font-size: 11.5px;
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .fr-crumbs .cz {
    color: var(--muted);
  }
  .fr-crumbs .cz.here {
    color: var(--text);
    font-weight: 650;
  }
  .fr-crumbs .sep {
    color: var(--faint);
  }
  .fr-badge {
    font: 700 9.5px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    border-radius: 5px;
    padding: 3px 7px;
  }
  .fr-nav {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--muted);
  }
  .fr-nav b {
    color: var(--text);
  }
  .fr-hop {
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--muted);
    font: 600 11px/1 var(--mono);
    padding: 4px 8px;
    border-radius: 6px;
  }
  .fr-hop:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .fr-hop:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .fr-kk-inline {
    display: flex;
    align-items: center;
    gap: 3px;
  }
  .fr-close {
    width: 26px;
    height: 26px;
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--muted);
    border-radius: 7px;
    flex: 0 0 auto;
  }
  .fr-close:hover {
    color: var(--text);
  }
  .fr-close:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .fr-body {
    overflow-y: auto;
    display: grid;
    /* 72ch — the reading measure asked for, kept just under the ~75ch point
       where a line stops being comfortable to track. */
    grid-template-columns: 160px minmax(0, 72ch);
    justify-content: center;
    gap: var(--phi2, 24px);
    padding: var(--phi2, 24px) var(--phi2, 24px) var(--phi3, 40px);
  }
  .fr-gutter {
    font: 500 11px/1.5 var(--mono);
    color: var(--muted);
    border-right: 1px solid var(--border-soft, var(--border));
    padding-right: 16px;
  }
  .fr-gutter .glab {
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-size: 9.5px;
    color: var(--faint);
    margin: 15px 0 5px;
  }
  .fr-gutter .glab:first-child {
    margin-top: 0;
  }
  .fr-gutter .who {
    color: var(--text);
    font-size: 12px;
    font-weight: 650;
  }
  .fr-gutter .whosub {
    color: var(--faint);
    font-size: 10.5px;
  }
  /* The secondary UTC line under the primary local timestamp — present
     (never hidden — "both, for clarity"), just visually quieter. */
  .fr-gutter .whosub.dim {
    opacity: 0.68;
    margin-top: 1px;
  }
  .fr-gutter .gref {
    display: block;
    width: 100%;
    text-align: left;
    color: var(--claimed);
    border: 0;
    background: transparent;
    padding: 2px 0;
    font: inherit;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fr-gutter .gref:hover {
    text-decoration: underline;
  }
  /* Prose-typeset reading column — deliberately NOT the stream's compact
     clamp: real line-height, a comfortable measure (63ch, matching the
     mockup), sans body text. Headings/lists/code/etc are app.css's shared
     `.prose.md` rules (same ones Message.svelte/RequestDetail use) — no
     bespoke typography system just for this view. */
  .fr-reading {
    font-size: 15px;
    line-height: 1.7;
  }
  .fr-keys {
    border-top: 1px solid var(--border);
    padding: 9px 16px;
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    font: 500 10.5px/1 var(--mono);
    color: var(--muted);
    flex: 0 0 auto;
  }
  .kk {
    color: var(--text);
    border: 1px solid var(--border-2);
    border-radius: 4px;
    padding: 1px 5px;
    margin-right: 2px;
  }

  @media (max-width: 700px) {
    .fr-body {
      grid-template-columns: 1fr;
    }
    .fr-gutter {
      border-right: none;
      border-bottom: 1px solid var(--border-soft, var(--border));
      padding-right: 0;
      padding-bottom: 12px;
    }
  }
</style>
