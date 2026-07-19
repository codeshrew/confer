<script lang="ts">
  // Piece 6 (ui/REDESIGN.md, "the composable card system") — the enriched
  // note/thread popover: a note's body + a keyboard-selectable Related
  // column composed from the SAME mini cards piece 5 already built
  // (TicketMiniCard, portable by design) plus two new ones this piece adds
  // (CodeRefMini, a thread pill). Per the doc's own framing: "the f reader
  // stays pure reading; this is the inspect view" — the FULL body renders
  // here (unlike TicketFullPopover's deliberate 2-line-teaser restraint —
  // that one is a launchpad OFF a ticket; this one IS the note's own
  // content), but no thread graph/lifecycle chrome — those stay MetaThread's
  // and FocusReader's own jobs, one click away via the footer/thread pill.
  //
  // Same overlay convention as FocusReader/TicketFullPopover
  // (`.fr-overlay`'s precedent) — reuses the SAME `thread`/
  // `appState.selectedMessage` state those two already populate (the
  // trigger calls `selectMessage` first, exactly like `openInFocusReader`
  // does), so opening this never means a second fetch for data already on
  // hand.
  import type { Agent, CodeRef, Message, RefHit, RequestRow, ThreadNode } from '../types';
  import { api } from '../api';
  import { renderMarkdown, highlightRenderedCodeBlocks } from '../markdown';
  import { formatClock } from '../format';
  import { buildTrail } from '../thread';
  import { relatedRefs, relatedTickets, threadSummary } from '../noteRelated';
  import { isTypingTarget } from '../keys';
  import TicketMiniCard from './TicketMiniCard.svelte';
  import CodeRefMini from './CodeRefMini.svelte';
  import CopyIdButton from './CopyIdButton.svelte';

  interface Props {
    open: boolean;
    msgId: string | null;
    messages: Message[];
    agents: Agent[];
    requests: RequestRow[];
    thread: ThreadNode[];
    hub: string;
    /** True while the focus reader is ALSO open — auto-closes this popover,
     * same "launchpad, don't stack" rule TicketFullPopover follows. */
    focusReaderOpen?: boolean;
    onOpenTicket?: (id: string) => void;
    onOpenRefs?: (ref: CodeRef, hits: RefHit[]) => void;
    onOpenThread?: (msgId: string, topic: string | null) => void;
    onClose?: () => void;
  }

  let { open, msgId, messages, agents, requests, thread, hub, focusReaderOpen = false, onOpenTicket, onOpenRefs, onOpenThread, onClose }: Props = $props();

  const agentsById = $derived(new Map(agents.map((a) => [a.id, a])));
  const message = $derived(msgId ? (messages.find((m) => m.id === msgId) ?? null) : null);
  const showing = $derived(open && message !== null);
  const agent = $derived(message ? agentsById.get(message.from) : undefined);

  const trail = $derived(buildTrail(thread, messages));
  const tickets = $derived(requests.length && trail.length ? relatedTickets(trail, requests) : []);
  const refs = $derived(relatedRefs(trail));
  const summary = $derived(threadSummary(trail));

  const renderedBody = $derived(message ? renderMarkdown(message.body) : null);
  let bodyEl: HTMLElement | undefined = $state();
  $effect(() => {
    void renderedBody;
    if (bodyEl) void highlightRenderedCodeBlocks(bodyEl);
  });

  function display(agentId: string): string {
    const a = agentsById.get(agentId);
    if (a) return a.display;
    return agentId.length ? agentId[0]!.toUpperCase() + agentId.slice(1) : agentId;
  }

  // The Related column as ONE flat, keyboard-selectable list — tickets,
  // then code, then the thread pill last — matching the mockup's own
  // top-to-bottom reading order (rel-grp tickets → code → thread).
  type RelatedItem = { kind: 'ticket'; key: string; row: RequestRow } | { kind: 'ref'; key: string; ref: CodeRef } | { kind: 'thread'; key: string };
  const relatedItems = $derived.by((): RelatedItem[] => [
    ...tickets.map((row): RelatedItem => ({ kind: 'ticket', key: `t:${row.id}`, row })),
    ...refs.map((ref): RelatedItem => ({ kind: 'ref', key: `r:${ref.repo}:${ref.path}:${ref.sha}`, ref })),
    ...(summary.messageCount > 1 ? [{ kind: 'thread' as const, key: 'thread' }] : []),
  ]);

  let selectedIdx = $state(0);
  $effect(() => {
    if (selectedIdx >= relatedItems.length) selectedIdx = Math.max(0, relatedItems.length - 1);
  });

  function activate(item: RelatedItem) {
    if (item.kind === 'ticket') onOpenTicket?.(item.row.id);
    else if (item.kind === 'ref') void openRef(item.ref);
    else if (message) onOpenThread?.(message.id, message.topic);
  }

  async function openRef(ref: CodeRef) {
    // CodeRefMini already fetches + calls onOpenRefs on its own click; a
    // keyboard Enter on the SAME item does the identical thing here so
    // both input paths land on the same real reverse-index data, not two
    // slightly-different code paths.
    const target = `${ref.repo}:${ref.path}${ref.range ? `@${ref.range[0]}-${ref.range[1]}` : ''}`;
    try {
      const hits = await api.getRefs(hub, target, true);
      onOpenRefs?.(ref, hits);
    } catch (err) {
      console.error('confer serve: failed to load reverse-index hits', target, err);
    }
  }

  $effect(() => {
    if (focusReaderOpen) onClose?.();
  });

  function handleRelatedKeydown(e: KeyboardEvent) {
    if (relatedItems.length === 0) return;
    if (e.key === 'j' || e.key === 'ArrowDown') {
      e.preventDefault();
      selectedIdx = Math.min(selectedIdx + 1, relatedItems.length - 1);
      return;
    }
    if (e.key === 'k' || e.key === 'ArrowUp') {
      e.preventDefault();
      selectedIdx = Math.max(selectedIdx - 1, 0);
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const item = relatedItems[selectedIdx];
      if (item) activate(item);
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!showing) return;
    if (isTypingTarget(e.target)) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose?.();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if showing && message}
  <div class="np-overlay">
    <div class="np-backdrop" onclick={onClose} aria-hidden="true" data-testid="note-popover-backdrop"></div>
    <div class="np-panel" role="dialog" aria-modal="true" aria-label="Note detail" tabindex="-1" data-testid="note-popover">
      <div class="np-head">
        <span class="np-kind">{message.type}</span>
        <span class="np-from" style="color:{agent?.color ?? 'var(--muted)'}">{display(message.from)}</span>
        <span class="np-meta mono">{message.host ?? '—'} · {formatClock(message.ts)} · #{message.topic ?? '—'}</span>
        <CopyIdButton id={message.id} class="np-copy-id" />
        <button type="button" class="np-close" aria-label="Close note" onclick={onClose}>esc ✕</button>
      </div>

      <div class="np-grid">
        <div class="np-body">
          <h3 class="np-title">{message.summary}</h3>
          <article class="prose md" bind:this={bodyEl}>
            {#if renderedBody}
              {@html renderedBody}
            {/if}
          </article>
        </div>

        <div
          class="np-related"
          role="listbox"
          aria-label="related tickets, code, and thread"
          tabindex="-1"
          onkeydown={handleRelatedKeydown}
          data-testid="note-related"
        >
          {#if tickets.length}
            <div class="rel-grp">
              <div class="rel-lab">tickets <span class="c mono">{tickets.length}</span></div>
              {#each tickets as row (row.id)}
                {@const idx = relatedItems.findIndex((i) => i.kind === 'ticket' && i.row.id === row.id)}
                <TicketMiniCard request={row} {agents} selected={idx === selectedIdx} onSelect={() => onOpenTicket?.(row.id)} />
              {/each}
            </div>
          {/if}

          {#if refs.length}
            <div class="rel-grp">
              <div class="rel-lab">code <span class="c mono">{refs.length}</span></div>
              {#each refs as ref (ref.repo + ':' + ref.path + '@' + ref.sha)}
                {@const idx = relatedItems.findIndex((i) => i.kind === 'ref' && i.ref === ref)}
                <CodeRefMini {ref} {hub} selected={idx === selectedIdx} {onOpenRefs} />
              {/each}
            </div>
          {/if}

          {#if summary.messageCount > 1}
            <div class="rel-grp">
              <div class="rel-lab">thread</div>
              <button
                type="button"
                class="thr-mini"
                class:sel={relatedItems[selectedIdx]?.kind === 'thread'}
                onclick={() => message && onOpenThread?.(message.id, message.topic)}
                data-testid="note-thread-pill"
              >
                <span class="g" aria-hidden="true">⋔</span>
                {summary.messageCount} msgs · {summary.topicCount} topic{summary.topicCount === 1 ? '' : 's'}
                <span class="go">open ›</span>
              </button>
            </div>
          {/if}

          {#if relatedItems.length === 0}
            <p class="rel-empty">nothing else connects to this note yet</p>
          {/if}

          {#if relatedItems.length}
            <div class="rel-foot mono">
              <span class="kk">↑</span><span class="kk">↓</span> select
              <span class="kk">↵</span> open
            </div>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .np-overlay {
    position: fixed;
    inset: 0;
    z-index: 61;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--phi2, 24px);
  }
  .np-backdrop {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--bg) 72%, transparent);
    backdrop-filter: blur(2px);
  }
  .np-panel {
    position: relative;
    width: min(760px, 100%);
    max-height: 88vh;
    overflow: hidden;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
  }
  .np-head {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 12px 15px;
    border-bottom: 1px solid var(--border);
    background: var(--panel-2);
  }
  .np-kind {
    font: 700 9.5px/1 var(--mono);
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--muted);
    border: 1px solid var(--border-2);
    border-radius: 4px;
    padding: 2px 6px;
  }
  .np-from {
    font-weight: 640;
    font-size: 13px;
  }
  .np-meta {
    font-size: 11px;
    color: var(--faint);
  }
  .np-close {
    margin-left: auto;
    font: 500 11px/1 var(--mono);
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 7px;
    background: transparent;
    cursor: pointer;
  }
  .np-close:hover {
    color: var(--text);
    border-color: var(--faint);
  }
  .np-head :global(.np-copy-id) {
    opacity: 1;
  }

  .np-grid {
    display: grid;
    grid-template-columns: 1fr 268px;
    min-height: 0;
    overflow: hidden;
  }
  .np-body {
    padding: 18px 20px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
  }
  .np-title {
    font-size: 15px;
    font-weight: 640;
    line-height: 1.4;
    margin: 0 0 12px;
    color: var(--text);
  }

  .np-related {
    padding: 12px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
    background: color-mix(in srgb, var(--panel-2) 55%, var(--panel));
  }
  .rel-grp {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .rel-lab {
    font: 700 9.5px/1 var(--mono);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .rel-lab .c {
    color: var(--faint);
    text-transform: none;
    letter-spacing: normal;
  }
  .rel-empty {
    margin: 0;
    font-size: 11.5px;
    color: var(--faint);
    font-style: italic;
  }

  .thr-mini {
    display: flex;
    align-items: center;
    gap: 7px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px 9px;
    cursor: pointer;
    font-size: 12px;
    color: var(--muted);
    text-align: left;
    font-family: inherit;
    width: 100%;
  }
  .thr-mini:hover,
  .thr-mini.sel {
    border-color: var(--accent);
  }
  .thr-mini .g {
    color: var(--faint);
    font-family: var(--mono);
  }
  .thr-mini .go {
    margin-left: auto;
    font: 500 10px/1 var(--mono);
    color: var(--faint);
  }

  .rel-foot {
    margin-top: auto;
    padding-top: 6px;
    border-top: 1px solid var(--border-2);
    font-size: 9.5px;
    color: var(--faint);
    display: flex;
    gap: 5px;
    align-items: center;
  }
  .rel-foot .kk {
    color: var(--muted);
    border: 1px solid var(--border-2);
    border-radius: 3px;
    padding: 0 4px;
  }

  @media (max-width: 720px) {
    .np-grid {
      grid-template-columns: 1fr;
    }
    .np-body {
      border-right: none;
      border-bottom: 1px solid var(--border);
    }
  }
</style>
