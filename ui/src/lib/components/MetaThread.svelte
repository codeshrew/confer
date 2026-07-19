<script lang="ts">
  // The conversation MINIMAP (ui/REDESIGN.md piece 4, item 1 — 2026-07-19,
  // redesign-mockups/04-metathread-minimap.html). Replaces piece 3's
  // "wall of text" sidebar — full snippets + a narrated sentence per
  // topic-crossing — with a glanceable MAP: shape, not content. Per node:
  // an author-colored dot on a topic-colored lane, a kind tag, short-id +
  // time. NO snippet. Full text is one hover away (a small preview card);
  // a topic crossing is ONE labeled divider, not a sentence.
  //
  // INTERACTION MODEL CHANGE from piece 3: click now JUMPS straight to the
  // stream. Piece 3's "peeking != navigating" existed because click was
  // the ONLY way to explore a node without committing — clicking had to be
  // safe-by-default, so it only moved a local pointer, and only Enter (or
  // an explicit button) actually navigated. HOVER now serves that "explore
  // without committing" role (the preview card), so click is free to mean
  // what it looks like it means. Keyboard stays exactly as it was: j/k/h/l
  // move a LOCAL pointer (never navigates), Enter jumps — hover has no
  // keyboard equivalent, so the preview falls back to showing whichever
  // node is currently locally-focused.
  //
  // Reading (full markdown body, refs) is explicitly OUT of scope here —
  // that's the stream's and the focus reader's job (mockup rationale:
  // "the map should point, not re-print"). `messages` is still accepted —
  // buildTrail still needs it to recover real `ts`/parent edges — but this
  // component no longer renders message bodies or CodeRefCard from it.
  import { formatClock, formatIso8601 } from '../format';
  import { buildTrail, childrenOf, trailRoot, type TrailNode } from '../thread';
  import { copyToClipboard } from '../clipboard';
  import { paneFocus } from '../paneFocus.svelte';
  import { readState } from '../readState.svelte';
  import CopyIdButton from './CopyIdButton.svelte';
  import CopiedToast from './CopiedToast.svelte';
  import type { Agent, Message as MessageT, MsgType, ThreadNode } from '../types';

  interface Props {
    thread: ThreadNode[];
    agents: Agent[];
    messages?: MessageT[];
    /** The message this peek session was opened on — the map's initial
     * local focus. Resets local navigation whenever it changes (a
     * genuinely NEW peek session, not a within-panel move). */
    focusedMsgId: string;
    /** Click a node, or Enter on the locally-focused one — the two paths
     * that actually move the stream. */
    onJump?: (msgId: string, topic: string | null) => void;
    /** Esc — close the whole peek (mouse equivalent lives in App.svelte's
     * shared `.ctx-head` close button). */
    onClose?: () => void;
  }

  let { thread, agents, messages = [], focusedMsgId, onJump, onClose }: Props = $props();

  // git-log-style "short sha" — msgIds are 26-char ULIDs, far too wide for
  // this narrow rail. Truncate the DISPLAYED text only; CopyIdButton still
  // copies the full id.
  function shortId(msgId: string): string {
    return msgId.length > 10 ? `${msgId.slice(0, 8)}…` : msgId;
  }

  const agentsById = $derived(new Map(agents.map((a) => [a.id, a])));

  // The real parent/child trail (thread.ts) — recomputed whenever the
  // fetched thread or the message cross-reference changes.
  const trail = $derived(buildTrail(thread, messages));

  // Local focus — moves freely via j/k/h/l, never touching anything
  // outside this panel. Resets to the prop whenever a NEW peek session
  // opens (the prop itself changing means App.svelte loaded a different
  // thread).
  let localFocusedId = $state('');
  $effect(() => {
    localFocusedId = focusedMsgId;
  });

  const focusedNode = $derived(trail.find((n) => n.msgId === localFocusedId) ?? trailRoot(trail));

  // keyboard-architecture-pass BUG FIX (piece 4 item 0, found live: focus
  // the Chat stream, press j/k — after the FIRST move it silently jumped
  // into the meta-thread). Root cause: a stream selection change opens/
  // updates this peek as a SIDE EFFECT of content sync, not the operator
  // asking to navigate here — but this effect used to call real `.focus()`
  // unconditionally on every `focusedNode` change, which paneFocus reads
  // as "the operator moved into thread-peek", stealing the active pane.
  // Fix: only move real focus onto a row when thread-peek is ALREADY the
  // active pane (genuine within-peek navigation, Ctrl+hjkl/click already
  // got it there) — an externally-driven change just updates the `here`
  // highlight, which is entirely `class:here={isHere}` state already.
  let rowEls = $state<Record<string, HTMLElement | null>>({});
  $effect(() => {
    const id = focusedNode?.msgId;
    if (id && paneFocus.focusedId === 'thread-peek') rowEls[id]?.focus();
  });

  function focusNode(msgId: string) {
    if (trail.some((n) => n.msgId === msgId)) localFocusedId = msgId;
  }

  function jump() {
    if (focusedNode) onJump?.(focusedNode.msgId, focusedNode.topic);
  }

  /** The mouse path: focus + jump in one action (see the interaction-model
   * note above — click is a direct "go there", not a two-step). Reads the
   * topic straight off the trail node being clicked rather than the
   * (possibly not-yet-updated) `focusedNode` derived. */
  function jumpTo(msgId: string) {
    focusNode(msgId);
    const node = trail.find((n) => n.msgId === msgId);
    if (node) onJump?.(msgId, node.topic);
  }

  function moveFlat(delta: number) {
    const i = trail.findIndex((n) => n.msgId === localFocusedId);
    const next = trail[i + delta];
    if (next) localFocusedId = next.msgId;
  }

  function stepDeeper() {
    if (!focusedNode) return;
    const kids = childrenOf(trail, focusedNode.msgId);
    if (kids.length > 0) localFocusedId = kids[0]!.msgId;
  }

  function stepBack() {
    if (focusedNode?.parentId) localFocusedId = focusedNode.parentId;
  }

  function handlePeekKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case 'j':
      case 'ArrowDown':
        e.preventDefault();
        moveFlat(1);
        break;
      case 'k':
      case 'ArrowUp':
        e.preventDefault();
        moveFlat(-1);
        break;
      case 'l':
        e.preventDefault();
        stepDeeper();
        break;
      case 'h':
        e.preventDefault();
        stepBack();
        break;
      case 'Enter':
        e.preventDefault();
        jump();
        break;
      case 'y':
        e.preventDefault();
        if (focusedNode) void copyFocusedId(focusedNode.msgId);
        break;
      case 'Escape':
        e.preventDefault();
        onClose?.();
        break;
    }
  }

  // `y` (vim yank) copies the focused node's FULL id — the keyboard path;
  // hovering any row reveals that row's own CopyIdButton for the mouse
  // path (below). Same toast feedback FocusReader's `y` uses, since
  // there's no button under the pointer when this fires from the keyboard.
  let toastText = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  async function copyFocusedId(msgId: string) {
    const ok = await copyToClipboard(msgId);
    if (!ok) return;
    toastText = `copied ${shortId(msgId)}`;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toastText = null;
    }, 1500);
  }

  // keyboard-architecture pass — "thread-peek", registered only while this
  // panel is actually mounted (App.svelte renders it conditionally on the
  // peek being open), so register/unregister just rides the component's
  // own mount/destroy — no separate open/close bookkeeping needed here.
  let mtEl: HTMLDivElement;
  $effect(() => {
    if (!mtEl) return;
    return paneFocus.register({
      id: 'thread-peek',
      label: 'Reference graph',
      el: mtEl,
      getRect: () => mtEl.getBoundingClientRect(),
    });
  });

  // Hover (mouse) or native focus (Tab) previews a DIFFERENT node than the
  // local one without moving it — the map's "explore without committing"
  // affordance. Falls back to the locally-focused node so the preview
  // always shows something relevant, including via keyboard-only j/k/h/l
  // nav (which has no hover equivalent).
  let hoveredId = $state<string | null>(null);
  const previewNode = $derived(trail.find((n) => n.msgId === hoveredId) ?? focusedNode);
  const previewAgent = $derived(previewNode ? agentsById.get(previewNode.from) : undefined);

  // "hide" — declutters the map back to just the header, without touching
  // the peek SESSION itself (appState.selectedMessage stays set; Esc/the
  // shared ✕ close that). Local, resets naturally each time this
  // component mounts fresh.
  let collapsed = $state(false);

  const TOPIC_PALETTE = ['var(--accent)', '#8b8bf0', 'var(--ag-jarvis)', 'var(--ag-orbit)', 'var(--ag-compositor)'];

  function topicOf(node: ThreadNode | TrailNode): string {
    return node.topic ?? '—';
  }

  const uniqueTopics = $derived([...new Set(thread.map(topicOf))]);
  function colorFor(topic: string): string {
    const idx = uniqueTopics.indexOf(topic);
    return TOPIC_PALETTE[idx % TOPIC_PALETTE.length] ?? 'var(--muted)';
  }

  const KIND_TAG: Record<MsgType, string> = {
    request: 'req',
    claim: 'claim',
    blocked: 'blocked',
    done: 'done',
    note: 'note',
    error: 'error',
    defer: 'defer',
    supersede: 'super',
  };

  interface Row {
    node: TrailNode;
    laneColor: string;
    /** The topic this row crosses INTO, shown as one labeled divider
     * before the row — null on every row that doesn't cross (including
     * the first, which has nothing before it to cross from). */
    crossing: string | null;
  }

  const rows = $derived.by((): Row[] =>
    trail.map((node, i) => {
      const topic = topicOf(node);
      const prevTopic = i > 0 ? topicOf(trail[i - 1]!) : null;
      return {
        node,
        laneColor: colorFor(topic),
        crossing: i > 0 && topic !== prevTopic ? topic : null,
      };
    })
  );

  const agentCount = $derived(new Set(trail.map((n) => n.from)).size);
  const span = $derived.by(() => {
    if (trail.length < 2) return null;
    const first = trail[0]!;
    const last = trail[trail.length - 1]!;
    if (!first.ts || !last.ts) return null;
    const mins = Math.round((new Date(last.ts).getTime() - new Date(first.ts).getTime()) / 60_000);
    return mins < 60 ? `${mins}m` : `${Math.round(mins / 60)}h`;
  });
</script>

<!-- role="toolbar": the same WAI-ARIA fit piece 2's HubRail used for a
     roving-tabindex set of keyboard-navigable controls (j/k/h/l/Enter/Esc
     over the trail's rows) — see HubRail.svelte's own note on why this
     role rather than an unclaimed listbox/group. -->
<div
  class="mt"
  role="toolbar"
  aria-orientation="vertical"
  tabindex="-1"
  bind:this={mtEl}
  onkeydown={handlePeekKeydown}
  data-testid="thread-peek"
>
  <div class="mh2">
    <div class="mtitle">
      <span aria-hidden="true">⋔</span> Thread map
      <button
        type="button"
        class="collapse"
        onclick={() => (collapsed = !collapsed)}
        aria-expanded={!collapsed}
        data-testid="peek-collapse"
      >
        {collapsed ? '⌃ show' : '⌄ hide'}
      </button>
      <!-- The real mouse-close for the peek SESSION (Esc's mouse
           equivalent — flagged as missing during the keyboard-
           architecture pass's mouse-parity audit). App.svelte's shared
           `.ctx-head` ✕ looked like it should cover this but doesn't: it's
           `display:none` above 1024px (mobile-drawer-only, design/43) and
           `.rail-r` is always visible on desktop — so there was no
           DESKTOP mouse-close at all. This one is always visible, self-
           contained to the peek's own header, and calls the same
           `onClose` Esc already does. -->
      <button type="button" class="close" onclick={() => onClose?.()} aria-label="Close peek" title="Close (Esc)" data-testid="peek-close">
        ✕
      </button>
    </div>
    {#if !collapsed}
      <div class="mstat mono">
        <span>{trail.length} msg{trail.length === 1 ? '' : 's'}{#if span} · {span}{/if}{#if agentCount} · {agentCount} agent{agentCount === 1 ? '' : 's'}{/if}</span>
        {#if uniqueTopics.length > 1}
          {#each uniqueTopics as topic (topic)}
            <span class="tp"><span class="swatch" style="background:{colorFor(topic)}"></span>#{topic}</span>
          {/each}
        {/if}
      </div>
    {/if}
  </div>

  <CopiedToast text={toastText} />

  {#if !collapsed}
    <div class="map" data-testid="peek-map">
      {#each rows as row (row.node.msgId)}
        {@const agent = agentsById.get(row.node.from)}
        {@const isHere = row.node.msgId === focusedNode?.msgId}
        {#if row.crossing}
          <div class="cross"><span class="ln"></span><span class="chip mono">↘ #{row.crossing}</span><span class="ln"></span></div>
        {/if}
        <div
          class="node"
          class:req={row.node.type === 'request'}
          class:done={row.node.type === 'done'}
          class:here={isHere}
          role="button"
          tabindex="0"
          bind:this={rowEls[row.node.msgId]}
          onclick={() => jumpTo(row.node.msgId)}
          onkeydown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') jumpTo(row.node.msgId);
          }}
          onmouseenter={() => (hoveredId = row.node.msgId)}
          onmouseleave={() => (hoveredId = null)}
          onfocus={() => (hoveredId = row.node.msgId)}
          onblur={() => (hoveredId = null)}
          data-testid="peek-node"
        >
          <span class="lane" style="background:{row.laneColor}"></span>
          <span class="spine"><span class="knob" style="background:{agent?.color ?? 'var(--muted)'}"></span></span>
          <span class="who">
            <span class="kd">{KIND_TAG[row.node.type]}</span>
            <span class="nm">{agent?.display ?? row.node.from}</span>
            {#if isHere}<span class="here-tag">here</span>{/if}
            {#if readState.isDetailViewed(row.node.msgId)}
              <!-- Completionist-safe (piece 4, item 2) — same neutral-by-
                   absence glyph as the stream's own Message.svelte. -->
              <span class="detail-viewed" title="Opened in the focus reader" aria-label="Opened in the focus reader">✓</span>
            {/if}
          </span>
          <span class="meta mono">
            <CopyIdButton id={row.node.msgId} class="node-copy-id" />
            <span title={row.node.ts ? formatIso8601(row.node.ts) : undefined}>
              {shortId(row.node.msgId)}{#if row.node.ts} · {formatClock(row.node.ts)}{/if}
            </span>
          </span>
        </div>
      {/each}
    </div>

    <div class="preview" data-testid="peek-preview">
      {#if previewNode}
        <div class="pvh mono">
          ▸ {hoveredId ? 'hover' : 'focused'} — {previewAgent?.display ?? previewNode.from}{#if previewNode.ts} · {formatClock(previewNode.ts)}{/if}
        </div>
        {previewNode.summary}
      {/if}
    </div>

    <div class="mfoot">
      <span><span class="kk">click</span> jump</span>
      <span><span class="kk">j</span><span class="kk">k</span> move</span>
      <span><span class="kk">l</span> deeper · <span class="kk">h</span> back</span>
      <span><span class="kk">y</span> copy id</span>
      <span><span class="kk">esc</span> close</span>
    </div>
  {/if}
</div>

<style>
  .mt {
    text-align: left;
  }

  .mh2 {
    padding-bottom: 10px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 10px;
  }
  .mtitle {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    font-weight: 640;
  }
  .mtitle .collapse {
    margin-left: auto;
    font: 600 10px/1 var(--mono);
    color: var(--muted);
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    border-radius: 5px;
    padding: 3px 6px;
  }
  .mtitle .collapse:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .mtitle .collapse:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .mtitle .close {
    width: 20px;
    height: 20px;
    display: grid;
    place-items: center;
    font-size: 11px;
    color: var(--muted);
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    border-radius: 5px;
  }
  .mtitle .close:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .mtitle .close:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .mstat {
    margin-top: 6px;
    font-size: 10.5px;
    color: var(--muted);
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .mstat .tp {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .mstat .swatch {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }

  /* ── the map — one line per node, NO snippet ── */
  .map {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .node {
    position: relative;
    display: grid;
    grid-template-columns: 3px 14px 1fr auto;
    align-items: center;
    gap: 8px;
    padding: 4px 6px;
    border-radius: 6px;
    cursor: pointer;
  }
  .node:hover {
    background: var(--panel-2);
  }
  .node:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .node .lane {
    align-self: stretch;
    border-radius: 2px;
  }
  .node .spine {
    position: relative;
    display: flex;
    justify-content: center;
  }
  .node .knob {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: 0 0 auto;
  }
  .node.req .knob {
    border-radius: 2px;
  }
  .node .who {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .node .kd {
    font: 700 8.5px/1 var(--mono);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    padding: 2px 4px;
    border-radius: 4px;
    border: 1px solid var(--border-2);
    color: var(--muted);
    flex: 0 0 auto;
  }
  .node.req .kd {
    color: var(--open);
    border-color: color-mix(in srgb, var(--open) 40%, transparent);
  }
  .node.done .kd {
    color: var(--done);
    border-color: color-mix(in srgb, var(--done) 40%, transparent);
  }
  .node .nm {
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .node .here-tag {
    font: 700 9px/1 var(--mono);
    color: var(--accent);
    flex: 0 0 auto;
  }
  /* Subtle, positive-only — same convention as Message.svelte's own
     detail-viewed glyph (never a colored/urgent badge; absence is
     neutral, not a debt). */
  .node .detail-viewed {
    font: 700 9px/1 var(--mono);
    color: var(--done);
    opacity: 0.75;
    flex: 0 0 auto;
  }
  .node .meta {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    color: var(--faint);
    white-space: nowrap;
    justify-self: end;
  }
  /* CopyIdButton defaults to opacity:0 (design/41) — reveal it on the
     row's own hover/focus, same convention as Message's own copy-id. */
  .node:hover :global(.node-copy-id),
  .node:focus-within :global(.node-copy-id) {
    opacity: 1;
  }
  .node.here {
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .node.here .knob {
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .node.here .nm {
    color: var(--text);
  }

  /* topic-crossing — ONE labeled divider, not a sentence per hop */
  .cross {
    display: flex;
    align-items: center;
    gap: 7px;
    margin: 4px 4px;
  }
  .cross .ln {
    height: 1px;
    flex: 1;
    background: var(--border);
  }
  .cross .chip {
    font-size: 9.5px;
    letter-spacing: 0.03em;
    color: var(--muted);
    padding: 1px 6px;
    border-radius: 5px;
    border: 1px solid var(--border);
  }

  /* ── preview — the text is one hover/focus away, on-demand ── */
  .preview {
    margin-top: 8px;
    padding: 8px 10px;
    background: var(--panel);
    border: 1px solid var(--border-2);
    border-radius: 8px;
    font-size: 12px;
    color: var(--muted);
    line-height: 1.5;
  }
  .preview .pvh {
    font-size: 10px;
    color: var(--faint);
    margin-bottom: 3px;
  }

  /* ── keyboard footer ── */
  .mfoot {
    margin-top: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    font: 500 10px/1 var(--mono);
    color: var(--muted);
  }
  .mfoot .kk {
    color: var(--text);
    border: 1px solid var(--border-2);
    border-radius: 4px;
    padding: 1px 5px;
    margin-right: 2px;
  }
</style>
