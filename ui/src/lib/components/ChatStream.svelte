<script lang="ts">
  // The message feed for the current topic. Ports `.stream` from
  // design/serve-dashboard-v2-mockup.html: per-day dividers, notes/tickets/
  // syslines in order, a "NEW · since you last looked" divider, and an
  // empty-state for topics with nothing filed yet.
  //
  // READ-STATE (ui/REDESIGN.md piece 4, item 2 — 2026-07-19): both halves
  // of CONTRACT GAP #58 are now retired.
  //   - "seen by" was entirely synthesized (a fake per-agent clock).
  //     Herald shipped a REAL per-message read-receipt index (src/seen.rs,
  //     signed presence cursors, honest-by-omission) — `message.seenBy`.
  //     `buildSeenEntries` below now reads that directly; no more filler.
  //   - the "since you last looked" cutoff was a hardcoded demo constant
  //     (`NEW_CUTOFF`). It's now a REAL per-(hub,topic) watermark in the
  //     operator's own localStorage (readState.svelte.ts) — genuinely
  //     "since YOU last looked," not a fixed demo date.
  // "You" have seen a message the same way everyone else has "seen" is
  // defined here: your own local watermark for this (hub, topic) having
  // passed the message's timestamp — see `isUnseenByYou`.
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
  import { readState } from '../readState.svelte';
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

  // "Since you last looked" — a REAL per-(hub,topic) watermark (see the
  // header note), not a hardcoded demo date. `null` means this (hub,
  // topic) has never been visited before — readState.svelte.ts's own note
  // explains why that means "nothing flagged new," not "everything is
  // new."
  const watermark = $derived(topic ? readState.getWatermark(hub, topic) : null);

  function isUnseenByYou(message: MessageT): boolean {
    if (watermark === null) return false;
    return new Date(message.ts).getTime() > watermark;
  }

  // Real seen-by (Herald's src/seen.rs, merged) — a role appears in
  // `message.seenBy` ONLY once their signed presence cursor has actually
  // consumed past this message; honest by omission, so "not in the array"
  // already means exactly "unseen (or unconfirmable)," no heuristic
  // needed. "You" — the local operator, not a confer role with a presence
  // beat — is derived from the SAME watermark that drives the NEW divider
  // above: you've seen a message once your watermark has passed it.
  function buildSeenEntries(message: MessageT): SeenEntry[] {
    const seenByRole = new Map(message.seenBy.map((s) => [s.role, s.ts]));
    const others = agents.filter((a) => a.id !== message.from);
    const entries: SeenEntry[] = others.map((a) => {
      const ts = seenByRole.get(a.id);
      return ts
        ? { id: a.id, name: a.display, color: a.color, ts: formatClock(ts) }
        : { id: a.id, name: a.display, color: a.color, ts: null, unseen: true };
    });
    entries.push(
      isUnseenByYou(message)
        ? { id: 'you', name: 'You', ts: null, isYou: true, unseen: true }
        : {
            id: 'you',
            name: 'You',
            // Bounded, not exact — the watermark is when this (hub,topic)
            // was last marked caught-up, so it's a real upper bound on
            // when you saw this message, not a fabricated per-message
            // instant (the old synthesized filler's actual bug).
            ts: watermark !== null ? formatClock(new Date(watermark).toISOString()) : null,
            isYou: true,
          }
    );
    return entries;
  }

  const firstUnseenId = $derived(topicMessages.find((m) => isUnseenByYou(m))?.id ?? null);

  // "advance it when the view is seen" — moves the watermark to now when
  // the operator LEAVES this (hub, topic), so a later visit only flags
  // what arrived after this one. Runs on topic/hub change AND on unmount
  // (component destroy is also "leaving"). The explicit "mark all read"
  // control (FilterBar) does the same update on demand, for a long
  // absence the operator wants to catch up on immediately rather than by
  // scrolling through it.
  $effect(() => {
    const h = hub;
    const t = topic;
    return () => {
      if (t) readState.setWatermark(h, t, Date.now());
    };
  });

  // --- scroll behavior -------------------------------------------------
  let streamEl: HTMLDivElement | undefined = $state();

  // piece 4, item 3 polish — the sticky day bar's negative right margin
  // cancels .stream's own 20px padding to go full-bleed, but a REAL
  // scrollbar (when content overflows) sits INSIDE that padding too and
  // was left uncovered — a visible gap to the bar's right that only
  // showed up once there was actually enough to scroll. Scrollbar width
  // varies by OS/browser (0 on macOS overlay scrollbars, ~15-17px on
  // Windows/Linux) — there's no reliable CSS constant for it, so it's
  // MEASURED (offsetWidth includes the scrollbar track, clientWidth
  // doesn't) and exposed as a CSS var the bar's own margin reads.
  // ResizeObserver's content-box reporting reflects the scrollbar-adjusted
  // box, so this re-fires both on window resize AND whenever new content
  // toggles the scrollbar on/off — no manual dependency list needed.
  let scrollbarWidth = $state(0);
  $effect(() => {
    // jsdom (vitest) has no ResizeObserver — degrade to "no measurement,
    // 0px" rather than crash; --sb-w simply falls back to its own 0px
    // default in that environment, same as it would on a scrollbar-less
    // (e.g. touch) device.
    if (!streamEl || typeof ResizeObserver === 'undefined') return;
    const el = streamEl;
    const measure = () => {
      scrollbarWidth = el.offsetWidth - el.clientWidth;
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  });
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
  style="--sb-w: {scrollbarWidth}px"
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
      <!-- piece 4, item 3 — STICKY: pins to the top of .stream as the
           operator scrolls past it, so "which day am I looking at" is
           always answered without scrolling back to check (the previous
           day's divider naturally slides under this one). -->
      <div class="daybreak">
        <span class="d">{formatDayDivider(group.day)}</span>
        <span class="count">{group.messages.length} message{group.messages.length === 1 ? '' : 's'}</span>
      </div>
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
    /* No top padding (piece 4, item 3 polish) — that gap used to sit
       between the crumb header and the sticky day bar (and reappear
       above it once pinned, since sticky respects the scrollport's own
       padding). The bar's own margin-bottom (16px) already gives the
       first row its breathing room from below, so nothing above is lost. */
    padding: 0 20px 40px;
    /* Reserves the scrollbar's track consistently (no layout shift when
       content grows past the fold) — the bar's own right margin below
       still needs the MEASURED width (--sb-w) on top of this, since the
       reserved gutter is what makes a scrollbar possible here at all,
       not a fixed, known-in-advance pixel amount. */
    scrollbar-gutter: stable;
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
  /* piece 4, item 3 — a solid, full-bleed STICKY bar (not the old
     rule-flanked divider, which would show scrolled content through its
     gaps once pinned) — negative margins cancel .stream's own horizontal
     padding so it spans edge-to-edge while sticky, matching padding
     re-applied to keep the label where it visually sat before. The RIGHT
     margin also cancels the measured scrollbar width (--sb-w, set on
     .stream from streamEl.offsetWidth - clientWidth) — the scrollbar's
     own track sits INSIDE that padding too, so -20px alone only reached
     its left edge, leaving the track itself as an uncovered gap whenever
     content actually overflowed enough to show one. */
  .daybreak {
    position: sticky;
    top: 0;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 0 calc(-20px - var(--sb-w, 0px)) 16px -20px;
    padding: 7px calc(20px + var(--sb-w, 0px)) 7px 20px;
    background: color-mix(in srgb, var(--panel) 94%, transparent);
    backdrop-filter: blur(6px);
    border-bottom: 1px solid var(--border);
    color: var(--faint);
    font: 600 10.5px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .daybreak .count {
    margin-left: auto;
    color: var(--muted);
    text-transform: none;
    letter-spacing: normal;
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
