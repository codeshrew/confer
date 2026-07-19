<script lang="ts">
  // The message feed for the current topic. Ports `.stream` from
  // design/serve-dashboard-v2-mockup.html: per-day dividers, notes/tickets/
  // syslines in order, a "NEW · since you last looked" divider, and an
  // empty-state for topics with nothing filed yet.
  //
  // CONTRACT GAP: `Message` carries no read-receipt / seen-by data, so the
  // per-message "seen" roster and the unseen/NEW cutoff are synthesized here
  // deterministically (see buildSeenEntries/NEW_CUTOFF) rather than sourced
  // from real state. If confer serve's backend grows a real seen-by
  // projection, this is the seam to wire it in.
  //
  // PAGINATION: `messages` is App.svelte's windowed per-(hub,topic) page —
  // most-recent CHAT_PAGE_SIZE on load, grown backward as the reader scrolls
  // up (see loadOlder below and App.svelte's loadOlderChatMessages). This
  // component owns the scroll-position bookkeeping (measure scrollHeight
  // before/after a prepend so the view doesn't jump) and the initial
  // scroll-to-bottom / stay-at-bottom-on-new-message behavior; the actual
  // fetching lives in the parent (onLoadOlder).
  import { tick } from 'svelte';
  import type { Agent, CodeRef, Message as MessageT, RefHit, RequestRow } from '../types';
  import { paneFocus } from '../paneFocus.svelte';
  import MessageComponent from './Message.svelte';
  import type { SeenEntry } from './SeenIndicator.svelte';
  import { formatClock, formatDayDivider, groupByDay } from '../format';

  interface Props {
    messages: MessageT[];
    requests: RequestRow[];
    agents: Agent[];
    topic: string | null;
    hub: string;
    notesOn: boolean;
    reqsOn: boolean;
    density?: 'summary' | 'full';
    selectedMessageId?: string | null;
    /** Whether an older page exists past what's currently loaded. */
    hasMore?: boolean;
    /** A fetch for an older page is already in flight. */
    loadingOlder?: boolean;
    /** Fetches and prepends the next older page; resolves with how many
     * messages were prepended (0 if none were available, or already
     * loading). Omitted entirely disables scroll-load (e.g. in tests that
     * don't care about pagination). */
    onLoadOlder?: () => Promise<number>;
    onSelectMessage?: (id: string) => void;
    onSelectTicket?: (id: string) => void;
    /** keyboard-architecture pass — the mouse path for the `f` shortcut,
     * passed straight through to each Message row. */
    onOpenFocus?: (id: string) => void;
    onOpenRefs?: (ref: CodeRef, hits: RefHit[]) => void;
    /** design/41 Phase 0 item 4 — the shared scroll-to + highlight-pulse
     * primitive that meta-thread-node clicks and lifecycle-trail-row clicks
     * both build on. `scrollToken` must be bumped by the caller on every
     * request (even a repeat of the same id) since Svelte's reactivity
     * otherwise can't tell "navigate here again" from "no change". */
    scrollToMessageId?: string | null;
    scrollToken?: number;
  }

  let {
    messages,
    requests,
    agents,
    topic,
    hub,
    notesOn,
    reqsOn,
    density = 'full',
    selectedMessageId = null,
    hasMore = false,
    loadingOlder = false,
    onLoadOlder,
    onSelectMessage,
    onSelectTicket,
    onOpenFocus,
    onOpenRefs,
    scrollToMessageId = null,
    scrollToken = 0,
  }: Props = $props();

  const agentsById = $derived(new Map(agents.map((a) => [a.id, a])));

  function findRequest(message: MessageT): RequestRow | null {
    const guess = message.id.replace(/^msg_/, 'req_');
    return requests.find((r) => r.id === guess) ?? requests.find((r) => r.summary === message.summary) ?? null;
  }

  function bucket(message: MessageT): 'note' | 'request' {
    return message.type === 'note' ? 'note' : 'request';
  }

  const topicMessages = $derived(
    messages
      .filter((m) => m.topic === topic)
      .filter((m) => (bucket(m) === 'note' ? notesOn : reqsOn))
      .sort((a, b) => new Date(a.ts).getTime() - new Date(b.ts).getTime())
  );

  const dayGroups = $derived(groupByDay(topicMessages));

  // Demo "since you last looked" cutoff — see the CONTRACT GAP note above.
  const NEW_CUTOFF = new Date('2026-07-17T14:53:00Z').getTime();

  function isUnseenByYou(message: MessageT): boolean {
    return new Date(message.ts).getTime() > NEW_CUTOFF;
  }

  // CONTRACT GAP (#58): `Message` carries no real per-recipient read
  // receipt at all — the "seen" timestamps below (bar this one check) are
  // synthesized filler, not sourced from the backend (see the CONTRACT GAP
  // note atop this file). That's fine for demo polish on ONLINE agents, but
  // it produced a real, reported bug: a recipient who is OFFLINE and has not
  // been active since before this message was even posted is being
  // unconditionally stamped "seen" anyway (confer-lab #code-refs showing
  // "✓ all seen" while Jarvis, last seen 3d ago, plainly cannot have read
  // it). That specific case IS decidable from data we already have —
  // `Agent.live` / `Agent.lastTs` — so it's fixed here: an agent is only
  // ever synthesized as "seen" if they were live, or demonstrably active
  // (lastTs) at or after the message's own timestamp. Anyone who fails that
  // check is marked `unseen` (rendered "pending <name>") regardless of the
  // demo NEW_CUTOFF below. A real fix still needs the backend to project an
  // actual per-message seen-by roster; until then this is the ceiling of
  // what the frontend can honestly claim.
  function couldHaveSeen(agent: Agent, messageTs: string): boolean {
    if (agent.live) return true;
    if (!agent.lastTs) return false;
    return new Date(agent.lastTs).getTime() >= new Date(messageTs).getTime();
  }

  function buildSeenEntries(message: MessageT): SeenEntry[] {
    const others = agents.filter((a) => a.id !== message.from);
    const baseMs = new Date(message.ts).getTime();
    if (!isUnseenByYou(message)) {
      return [
        ...others.map((a, i) =>
          couldHaveSeen(a, message.ts)
            ? {
                id: a.id,
                name: a.display,
                color: a.color,
                ts: formatClock(new Date(baseMs + (i + 1) * 90_000).toISOString()),
              }
            : { id: a.id, name: a.display, color: a.color, ts: null, unseen: true }
        ),
        {
          id: 'you',
          name: 'You',
          ts: formatClock(new Date(baseMs + (others.length + 1) * 90_000).toISOString()),
          isYou: true,
        },
      ];
    }
    return others.map((a, i) =>
      i === 0 && couldHaveSeen(a, message.ts)
        ? { id: a.id, name: a.display, color: a.color, ts: formatClock(message.ts) }
        : { id: a.id, name: a.display, color: a.color, ts: null, unseen: true }
    );
  }

  const firstUnseenId = $derived(topicMessages.find((m) => isUnseenByYou(m))?.id ?? null);

  // --- scroll behavior -------------------------------------------------
  let streamEl: HTMLDivElement | undefined = $state();
  // Whether the view should auto-follow new content at the bottom — true on
  // a fresh hub/topic load and while the reader hasn't scrolled away from
  // the bottom; false once they've scrolled up to read history (so a live
  // SSE-appended message doesn't yank them back down), and naturally false
  // while they're scrolled to the TOP loading older pages.
  let stickToBottom = $state(true);
  let lastKey: string | null = null;
  let loadingOlderNow = $state(false);
  // True while the pointer is anywhere over the stream — i.e. the reader is
  // mid-hover on a message, most likely lining up a click on a small,
  // hover-revealed affordance (CopyIdButton, the expand-toggle, a code-ref
  // card). A live SSE-appended note snapping `scrollTop` to the new bottom
  // at that exact moment shifts every row's on-screen position out from
  // under an already-positioned mouse cursor, so the click that lands next
  // hits whatever now happens to be under the pointer instead — often
  // nothing, reading to the reader as "the copy button didn't work" (see
  // design/41 bug report: chat's copy-id button appeared to silently fail
  // while MetaThread's identical affordance — a static, non-live-scrolling
  // list — never had this problem). Suspending the forced scroll while
  // hovering fixes that without giving up "stick to bottom" for a reader
  // who's just watching, not interacting: it catches up the moment the
  // pointer leaves (see handlePointerLeave below).
  let pointerOverStream = $state(false);

  $effect(() => {
    // A hub or topic switch is a fresh stream — reset to "follow the
    // bottom" regardless of where the reader had scrolled to previously.
    const key = `${hub} ${topic ?? ''}`;
    if (key !== lastKey) {
      lastKey = key;
      stickToBottom = true;
    }
  });

  $effect(() => {
    // Re-run whenever the rendered stream changes (initial load, SSE
    // append, or an older-page prepend). Only actually forces scroll when
    // `stickToBottom` — a prepend happens while the reader is scrolled up
    // (so this is a no-op then; the prepend's own scroll-compensation in
    // loadOlder handles that case instead) — AND the pointer isn't
    // currently over the stream (see pointerOverStream's own note above).
    void topicMessages;
    if (stickToBottom && streamEl && !pointerOverStream) {
      const el = streamEl;
      void tick().then(() => {
        el.scrollTop = el.scrollHeight;
      });
    }
  });

  function handlePointerEnter() {
    pointerOverStream = true;
  }

  function handlePointerLeave() {
    pointerOverStream = false;
    // Catch up on whatever arrived while the pointer was parked here, same
    // as the effect above would have done in the moment.
    if (stickToBottom && streamEl) {
      streamEl.scrollTop = streamEl.scrollHeight;
    }
  }

  const NEAR_TOP_PX = 60;
  const NEAR_BOTTOM_PX = 40;

  async function loadOlder() {
    if (!onLoadOlder || loadingOlderNow || loadingOlder || !hasMore || !streamEl) return;
    const el = streamEl;
    const prevScrollHeight = el.scrollHeight;
    const prevScrollTop = el.scrollTop;
    loadingOlderNow = true;
    try {
      const added = await onLoadOlder();
      if (added > 0) {
        await tick();
        if (streamEl) {
          const newScrollHeight = streamEl.scrollHeight;
          streamEl.scrollTop = prevScrollTop + (newScrollHeight - prevScrollHeight);
        }
      }
    } finally {
      loadingOlderNow = false;
    }
  }

  function handleScroll() {
    if (!streamEl) return;
    const el = streamEl;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    stickToBottom = distanceFromBottom < NEAR_BOTTOM_PX;
    if (el.scrollTop < NEAR_TOP_PX) void loadOlder();
  }

  const showLoadingOlder = $derived(loadingOlder || loadingOlderNow);

  // --- scroll-to + highlight-pulse (design/41 Phase 0 item 4) ------------
  // The shared primitive meta-thread-node clicks and lifecycle-trail-row
  // clicks both build on: scroll the target message into view and play a
  // brief highlight pulse. Must cope with the paginated window — if the
  // target isn't in the currently loaded page, load older pages (the same
  // mechanism as scroll-up pagination above) until it shows up or there's
  // nothing more to load; if it's truly unavailable, no-op gracefully.
  let pulseMessageId = $state<string | null>(null);
  // Bumped on every scroll-to request (see the $effect below) so an
  // in-flight load-older loop from a SUPERSEDED request can tell it's stale
  // and stop rather than fighting a newer request.
  let scrollGen = 0;
  const MAX_SCROLL_LOAD_ATTEMPTS = 25;

  function prefersReducedMotion(): boolean {
    return typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches === true;
  }

  function escapeForSelector(id: string): string {
    // CSS.escape isn't guaranteed to exist in every environment this runs
    // in (belt-and-suspenders for older embedders) — fall back to a minimal
    // manual escape of the two characters that would actually break the
    // attribute-selector string.
    return typeof CSS !== 'undefined' && typeof CSS.escape === 'function' ? CSS.escape(id) : id.replace(/["\\]/g, '\\$&');
  }

  async function performScrollTo(msgId: string, gen: number) {
    let attempts = 0;
    while (!topicMessages.some((m) => m.id === msgId) && hasMore && onLoadOlder && attempts < MAX_SCROLL_LOAD_ATTEMPTS) {
      if (gen !== scrollGen) return; // superseded by a newer scroll-to request
      const added = await onLoadOlder();
      if (gen !== scrollGen) return;
      if (added === 0) break; // nothing more to load
      attempts++;
    }
    await tick();
    if (gen !== scrollGen || !streamEl) return;
    if (!topicMessages.some((m) => m.id === msgId)) return; // truly unavailable — no-op
    const el = streamEl.querySelector(`[data-msg-id="${escapeForSelector(msgId)}"]`) as HTMLElement | null;
    if (!el) return;
    const reduced = prefersReducedMotion();
    el.scrollIntoView({ behavior: reduced ? 'auto' : 'smooth', block: 'center' });
    if (!reduced) {
      pulseMessageId = msgId;
      setTimeout(() => {
        if (pulseMessageId === msgId) pulseMessageId = null;
      }, 2000);
    }
  }

  $effect(() => {
    const target = scrollToMessageId;
    void scrollToken; // dependency only — forces a re-run even for a repeat id
    if (!target) return;
    const gen = ++scrollGen;
    void performScrollTo(target, gen);
  });

  // keyboard-architecture pass — "stream" is one of the 7 named Layer-1
  // panes; it had no bare-key vocab before this pass (selection was
  // click-only), so j/k here move `selectedMessageId` one message at a time
  // through the same ordered, filtered `topicMessages` list the stream
  // already renders — the natural bare-key pair given the app already has an
  // `f`-to-open-focus-reader affordance that reads that exact selection.
  function selectAdjacent(delta: number) {
    if (topicMessages.length === 0) return;
    const i = topicMessages.findIndex((m) => m.id === selectedMessageId);
    const next = i < 0 ? (delta > 0 ? 0 : topicMessages.length - 1) : Math.min(Math.max(i + delta, 0), topicMessages.length - 1);
    const msg = topicMessages[next];
    if (msg) onSelectMessage?.(msg.id);
  }

  function handleStreamKeydown(e: KeyboardEvent) {
    if (e.key === 'j' || e.key === 'ArrowDown') {
      e.preventDefault();
      selectAdjacent(1);
    } else if (e.key === 'k' || e.key === 'ArrowUp') {
      e.preventDefault();
      selectAdjacent(-1);
    }
  }

  $effect(() => {
    if (!streamEl) return;
    return paneFocus.register({
      id: 'stream',
      label: 'Chat stream',
      el: streamEl,
      getRect: () => streamEl!.getBoundingClientRect(),
    });
  });
</script>

<!-- keyboard-architecture pass: now a real Layer-1/2 pane (role="toolbar",
     same fit as HubRail/MetaThread's own roving-nav regions) — j/k move
     the message selection, Ctrl+hjkl/F6 land real focus here. The mouse
     enter/leave pair still only pauses/resumes stick-to-bottom auto-scroll
     while hovering (see pointerOverStream's note above). -->
<div
  class="stream"
  role="toolbar"
  aria-orientation="vertical"
  tabindex="-1"
  bind:this={streamEl}
  onscroll={handleScroll}
  onkeydown={handleStreamKeydown}
  onmouseenter={handlePointerEnter}
  onmouseleave={handlePointerLeave}
>
  {#if topicMessages.length === 0}
    <div class="emptystate">
      <div class="es-glyph">#</div>
      <div class="es-title">No messages yet</div>
      <div class="es-body">
        Nothing has been posted in <b class="mono">#{topic}</b>. Requests and notes filed here will show up in this
        stream — for anyone watching the hub.
      </div>
    </div>
  {:else}
    {#if hasMore}
      <div class="older-affordance">
        {#if showLoadingOlder}
          <span class="loading-older">loading older…</span>
        {:else}
          <button type="button" class="load-older-btn" onclick={() => void loadOlder()}> Load older messages </button>
        {/if}
      </div>
    {/if}
    {#each dayGroups as group (group.day)}
      <div class="daybreak">{formatDayDivider(group.day)}</div>
      {#each group.messages as message (message.id)}
        {#if message.id === firstUnseenId}
          <div class="newmark"><span class="t">NEW · SINCE YOU LAST LOOKED</span></div>
        {/if}
        <MessageComponent
          {message}
          {hub}
          fromAgent={agentsById.get(message.from)}
          request={message.type === 'request' ? findRequest(message) : null}
          selected={selectedMessageId === message.id}
          unseen={isUnseenByYou(message)}
          seenEntries={buildSeenEntries(message)}
          highlight={pulseMessageId === message.id}
          {density}
          onSelect={onSelectMessage}
          onSelectTicket={onSelectTicket}
          {onOpenFocus}
          {onOpenRefs}
        />
      {/each}
    {/each}
  {/if}
</div>

<style>
  .stream {
    overflow-y: auto;
    flex: 1;
    padding: 18px 20px 40px;
  }
  .older-affordance {
    display: flex;
    justify-content: center;
    margin: 0 0 14px;
  }
  .loading-older {
    font: 600 10.5px/1 var(--mono);
    color: var(--faint);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .load-older-btn {
    font: 600 11px/1 var(--mono);
    color: var(--muted);
    background: var(--panel-2);
    border: 1px solid var(--border-2);
    border-radius: 8px;
    padding: 6px 12px;
    cursor: pointer;
  }
  .load-older-btn:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .daybreak {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 6px 0 16px;
    color: var(--faint);
    font: 600 10.5px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .daybreak::before,
  .daybreak::after {
    content: '';
    height: 1px;
    background: var(--border);
    flex: 1;
  }
  .newmark {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 18px 0 8px;
  }
  .newmark .t {
    font: 800 9px/1 var(--mono);
    letter-spacing: 0.15em;
    color: var(--accent);
    white-space: nowrap;
  }
  .newmark::before {
    content: '';
    height: 1px;
    width: 22px;
    background: var(--accent);
    flex: 0 0 auto;
  }
  .newmark::after {
    content: '';
    height: 1px;
    flex: 1;
    background: linear-gradient(90deg, var(--accent), transparent);
  }
  .emptystate {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 9px;
    max-width: 420px;
    margin: 40px auto 0;
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
</style>
