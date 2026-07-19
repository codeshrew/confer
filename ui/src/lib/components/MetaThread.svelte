<script lang="ts">
  // The reference graph — a git-native commit graph woven across topics.
  // Ports `.mt`/`.gn`/`.gcard` from design/serve-dashboard-v2-mockup.html: a
  // vertical rail whose color encodes the topic, gradienting at cross-topic
  // crossings, with agent-colored dots ringed in the topic color.
  //
  // Redesigned 2026-07-18 (ui/REDESIGN.md piece 3 — side-peek + trail):
  // "peeking != navigating". Previously every row click called straight into
  // App.svelte's navigateToMessageInChat — the exact bug piece 3 exists to
  // fix (a peek that silently teleports your stream). Now a row click (or
  // j/k/h/l) only moves a LOCAL "focused" pointer among nodes already loaded
  // in `thread` — free, reversible, no App involvement, no re-fetch (backend-
  // confirmed: src/api.rs's `thread()` handler returns every message sharing
  // one `thread_root`, so ONE getThread() call already covers the whole
  // connected graph any node in it belongs to — moving focus among them is
  // just picking a different array entry). Only `onJump` (Enter, or the
  // Focused card's explicit button) is allowed to move the real stream.
  //
  // The breadcrumb + h/l ("deeper"/"back") need REAL parent/child edges,
  // which `ThreadNode` itself doesn't carry — `thread.ts`'s `buildTrail`
  // recovers them by cross-referencing the (already-received) `messages`
  // prop's `of`/`replyTo` fields, the same precedence RequestDetail.svelte's
  // own lifecycle-trail reconstruction already uses. A node whose pointer
  // doesn't resolve within THIS thread's own node set gets `parentId: null`
  // (an honest root/orphan), never an invented edge — see thread.ts's header.
  //
  // CONTRACT GAP: `ThreadNode` carries no timestamp, so the mockup's "40m
  // span" stat can't be reproduced from the thread alone — an optional
  // `messages` prop is accepted to recover `ts` by `msgId` lookup; without it
  // the span stat is omitted.
  //
  // CONTRACT GAP (body): `ThreadNode` also carries no message BODY, only
  // `summary` — /api/thread doesn't return it. We recover the full body the
  // same way we recover `ts` above: by looking the node's `msgId` up in the
  // (optional) `messages` prop. But `messages` is now App.svelte's WINDOWED
  // per-topic chat page (see ChatStream's own pagination notes), not every
  // message ever posted — an older node's message may simply not be loaded.
  // When that happens we fall back to `summary` (graceful degradation, not a
  // bug). If bodies need to be reliably available here, the clean fix is
  // having the backend's `/api/thread` include the body per node directly.
  //
  // DESIGN DECISION (piece 3, logged in ui/REDESIGN.md — not a backend gap):
  // the mockup tinted one trail node "foreign" (pulled in from a different
  // hub), but that overreached past what this graph can mean — a confer
  // thread is a reply-hash root within ONE hub's own git log, and two hubs
  // are two separate repos, so a reply chain literally cannot span hubs
  // today (confirmed in src/api.rs::thread — hub-scoped, one `thread_root`
  // grouping). No foreign-tint rendering here, on purpose; what's real and
  // IS kept is cross-TOPIC hops (one hub, many topics) — see the `hop`
  // rows below. Revisit only if cross-hub reply-threading ever becomes a
  // real capability.
  import { renderMarkdown, highlightRenderedCodeBlocks } from '../markdown';
  import { copyToClipboard } from '../clipboard';
  import { formatClock, formatIso8601 } from '../format';
  import { buildTrail, childrenOf, pathToRoot, trailRoot, type TrailNode } from '../thread';
  import { paneFocus } from '../paneFocus.svelte';
  import Icon from './Icon.svelte';
  import CodeRefCard from './CodeRefCard.svelte';
  import type { Agent, CodeRef, Message as MessageT, MsgType, RefHit, ThreadNode } from '../types';

  interface Props {
    thread: ThreadNode[];
    agents: Agent[];
    messages?: MessageT[];
    hub?: string;
    /** The message this peek session was opened on — the trail's initial
     * focus. Resets local navigation whenever it changes (a genuinely NEW
     * peek session, not a within-panel move). */
    focusedMsgId: string;
    /** Enter (or the Focused card's explicit button) — the ONE deliberate
     * action that actually moves the stream, per "peeking != navigating". */
    onJump?: (msgId: string, topic: string | null) => void;
    /** Esc — close the whole peek. */
    onClose?: () => void;
    onOpenRefs?: (ref: CodeRef, hits: RefHit[]) => void;
  }

  let { thread, agents, messages = [], hub = '', focusedMsgId, onJump, onClose, onOpenRefs }: Props = $props();

  // git-log-style "short sha" — msgIds are 26-char ULIDs, far too wide for
  // a dense one-line row (the reference-graph rail is a narrow sidebar
  // panel). Truncate the DISPLAYED text only; copyNodeId below still copies
  // the full id, and the full id remains in the title/aria-label.
  function shortId(msgId: string): string {
    return msgId.length > 10 ? `${msgId.slice(0, 8)}…` : msgId;
  }

  const agentsById = $derived(new Map(agents.map((a) => [a.id, a])));
  const messagesById = $derived(new Map(messages.map((m) => [m.id, m])));

  // The real parent/child trail (thread.ts) — recomputed whenever the
  // fetched thread or the message cross-reference changes.
  const trail = $derived(buildTrail(thread, messages));

  // Local focus — moves freely via click/j/k/h/l, never touching anything
  // outside this panel. Resets to the prop whenever a NEW peek session opens
  // (the prop itself changing means App.svelte loaded a different thread).
  let localFocusedId = $state('');
  $effect(() => {
    localFocusedId = focusedMsgId;
  });

  const focusedNode = $derived(trail.find((n) => n.msgId === localFocusedId) ?? trailRoot(trail));
  const breadcrumbPath = $derived(focusedNode ? pathToRoot(trail, focusedNode.msgId) : []);
  const rootTopic = $derived(trailRoot(trail)?.topic ?? null);
  // `{@const}` can't sit inside a plain element (only inside a block like
  // {#if}/{#each}) — these live here instead of inline in the Focused card.
  const focusedAgent = $derived(focusedNode ? agentsById.get(focusedNode.from) : undefined);
  const focusedBody = $derived(focusedNode ? renderedBodyFor(focusedNode.msgId) : null);

  // Roving DOM focus, one element per trail row — keeps real keyboard focus
  // in sync with `localFocusedId` so j/k/h/l work the instant a peek opens
  // (or the focused node changes) without the reader having to Tab in or
  // click first.
  let rowEls = $state<Record<string, HTMLElement | null>>({});
  $effect(() => {
    const id = focusedNode?.msgId;
    if (id) rowEls[id]?.focus();
  });

  function focusNode(msgId: string) {
    if (trail.some((n) => n.msgId === msgId)) localFocusedId = msgId;
  }

  function jump() {
    if (focusedNode) onJump?.(focusedNode.msgId, focusedNode.topic);
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
      case 'Escape':
        e.preventDefault();
        onClose?.();
        break;
    }
  }

  // keyboard-architecture pass — "thread-peek", registered only while this
  // panel is actually mounted (App.svelte renders it conditionally on the
  // peek being open), so register/unregister just rides the component's own
  // mount/destroy — no separate open/close bookkeeping needed here.
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

  // Copy-id affordance for the `.gid` line (design/41 Phase 0, §4) — the id
  // is already displayed per node; this just makes it click-to-copy, with
  // the same "swap to a check for ~1.2s" feedback as CopyIdButton, keyed by
  // msgId since it's one row among many.
  let copiedIds = $state(new Set<string>());
  const copyResetTimers = new Map<string, ReturnType<typeof setTimeout>>();
  async function copyNodeId(e: MouseEvent, msgId: string) {
    e.stopPropagation();
    const ok = await copyToClipboard(msgId);
    if (!ok) return;
    copiedIds = new Set(copiedIds).add(msgId);
    clearTimeout(copyResetTimers.get(msgId));
    copyResetTimers.set(
      msgId,
      setTimeout(() => {
        const next = new Set(copiedIds);
        next.delete(msgId);
        copiedIds = next;
      }, 1200)
    );
  }

  // renderMarkdown sanitizes with DOMPurify — message bodies are untrusted,
  // peer-authored content (see markdown.ts's own header note) — same
  // sanitize path Message.svelte uses.
  function renderedBodyFor(msgId: string): string | null {
    const body = messagesById.get(msgId)?.body;
    return body ? renderMarkdown(body) : null;
  }

  // A Svelte action instead of a per-row $effect: {#each} rows don't have
  // their own component instance to hang bind:this/$effect off of, but an
  // action naturally runs once per rendered element and re-runs on update.
  function highlightBody(el: HTMLElement) {
    void highlightRenderedCodeBlocks(el);
    return {
      update() {
        void highlightRenderedCodeBlocks(el);
      },
    };
  }

  const TOPIC_PALETTE = ['var(--accent)', '#8b8bf0', 'var(--ag-jarvis)', 'var(--ag-orbit)', 'var(--ag-compositor)'];

  function topicOf(node: ThreadNode | TrailNode): string {
    return node.topic ?? '—';
  }

  const uniqueTopics = $derived([...new Set(thread.map(topicOf))]);
  function colorFor(topic: string): string {
    const idx = uniqueTopics.indexOf(topic);
    return TOPIC_PALETTE[idx % TOPIC_PALETTE.length] ?? 'var(--muted)';
  }

  const BADGE_CLASS: Record<MsgType, string> = {
    request: 'b-request',
    claim: 'b-claim',
    blocked: 'b-blocked',
    done: 'b-done',
    note: 'b-note',
    error: 'b-blocked',
    defer: 'b-note',
    supersede: 'b-note',
  };

  interface Row {
    node: TrailNode;
    topColor: string;
    botColor: string;
    hop: { label: string; cls: string } | null;
  }

  const rows = $derived.by((): Row[] => {
    const rootTopicLocal = trail.length ? topicOf(trail[0]!) : null;
    return trail.map((node, i) => {
      const topic = topicOf(node);
      const color = colorFor(topic);
      const topColor = i === 0 ? 'transparent' : color;
      const nextTopic = i < trail.length - 1 ? topicOf(trail[i + 1]!) : null;
      const botColor =
        i === trail.length - 1
          ? 'transparent'
          : nextTopic === topic
            ? color
            : `linear-gradient(180deg, ${color}, ${colorFor(nextTopic!)})`;
      const prevTopic = i > 0 ? topicOf(trail[i - 1]!) : null;
      let hop: Row['hop'] = null;
      if (i > 0 && topic !== prevTopic) {
        hop =
          topic === rootTopicLocal
            ? { label: `↩ resolves back in #${topic}`, cls: 'hop-back' }
            : { label: `↗ thread crosses into #${topic}`, cls: 'hop-in' };
      }
      return { node, topColor, botColor, hop };
    });
  });

  const agentCount = $derived(new Set(trail.map((n) => n.from)).size);
  const span = $derived.by(() => {
    if (!trail.length) return null;
    const first = messagesById.get(trail[0]!.msgId);
    const last = messagesById.get(trail[trail.length - 1]!.msgId);
    if (!first || !last) return null;
    const mins = Math.round((new Date(last.ts).getTime() - new Date(first.ts).getTime()) / 60_000);
    return mins < 60 ? `${mins}m` : `${Math.round(mins / 60)}h`;
  });

  const focusedRefs = $derived(focusedNode?.refs ?? []);
</script>

<!-- role="toolbar": the same WAI-ARIA fit piece 2's HubRail used for a
     roving-tabindex set of keyboard-navigable controls (j/k/h/l/Enter/Esc
     over the trail's buttons) — see HubRail.svelte's own note on why this
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
  {#if focusedNode}
    <div class="crumbs" data-testid="peek-crumbs">
      {#if rootTopic}<span class="cz mono">#{rootTopic}</span><span class="sep">›</span>{/if}
      {#each breadcrumbPath as seg, i (seg.msgId)}
        {#if i > 0}<span class="sep">›</span>{/if}
        <button type="button" class="cz mono" class:here={seg.msgId === focusedNode.msgId} onclick={() => focusNode(seg.msgId)}>
          {shortId(seg.msgId)}
        </button>
      {/each}
    </div>

    <div class="focused" data-testid="peek-focused">
      <div class="ft">
        <span class="av" style="color:{focusedAgent?.color ?? 'var(--muted)'};background:color-mix(in srgb, {focusedAgent?.color ?? 'var(--muted)'} 18%, transparent)">
          {focusedAgent?.abbr ?? focusedNode.from.slice(0, 2).toUpperCase()}
        </span>
        <span class="fwho">{focusedAgent?.display ?? focusedNode.from}</span>
        {#if messagesById.get(focusedNode.msgId)}
          <span class="fts mono" title={formatIso8601(messagesById.get(focusedNode.msgId)!.ts)}>{formatClock(messagesById.get(focusedNode.msgId)!.ts)}</span>
        {/if}
        <button type="button" class="jumpbtn" onclick={jump} data-testid="peek-jump">↵ open here ›</button>
      </div>
      <div class="fx prose md" use:highlightBody>
        {#if focusedBody}
          {@html focusedBody}
        {:else}
          {focusedNode.summary}
        {/if}
      </div>
      {#if focusedRefs.length}
        <div class="frefs">
          {#each focusedRefs as ref (ref.repo + ':' + ref.path + '@' + ref.sha)}
            <CodeRefCard {ref} {hub} onRevHook={onOpenRefs} />
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <p class="ctx-note">
    Walked from <code class="mono">of</code>/<code class="mono">reply_to</code> hashes — every node is a signed commit.
    {#if uniqueTopics.length > 1}This thread weaves across {uniqueTopics.length} topics.{/if}
  </p>
  <div class="mt-stats">
    <span class="stat"><b>{uniqueTopics.length}</b> topics</span>
    <span class="stat"><b>{trail.length}</b> messages</span>
    <span class="stat"><b>{agentCount}</b> agents</span>
    {#if span}<span class="stat"><b>{span}</b> span</span>{/if}
  </div>
  <div class="mt-legend">
    {#each uniqueTopics as topic (topic)}
      <span class="lg"><i style="background:{colorFor(topic)}"></i>#{topic}</span>
    {/each}
    {#if uniqueTopics.length > 1}<span class="lg xr">↗ topic crossing</span>{/if}
  </div>

  {#each rows as row (row.node.msgId)}
    {@const agent = agentsById.get(row.node.from)}
    {@const isHere = row.node.msgId === focusedNode?.msgId}
    <div class="gn" class:here={isHere}>
      <div class="rail">
        <span class="seg top" style="background:{row.topColor}"></span>
        <span class="cd" style="background:{agent?.color ?? 'var(--muted)'};box-shadow:0 0 0 2px var(--panel), 0 0 0 3.5px {colorFor(topicOf(row.node))}"></span>
        <span class="seg bot" style="background:{row.botColor}"></span>
      </div>
      <!-- A `div`, not a `button` — it nests the copy-id button below;
           real buttons can't nest. role=button + Enter/Space keydown makes
           it just as keyboard-reachable (matches the original row's own
           pattern, kept unchanged by this redesign). -->
      <div
        class="gcard"
        class:here={isHere}
        role="button"
        tabindex="0"
        bind:this={rowEls[row.node.msgId]}
        onclick={() => focusNode(row.node.msgId)}
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') focusNode(row.node.msgId);
        }}
        data-testid="peek-node"
      >
        <!-- `git log --oneline`-style density, two tight lines: (1) colored
             author + type badge + a one-line ellipsized summary — the full
             body only ever shows in the Focused card above, clicking a row
             just moves focus there; (2) a small meta row — the topic (only
             called out on a cross-topic hop; the rail color already encodes
             topic on every row), the short id, and the time. -->
        <div class="gline">
          <span class="gwho" style="color:{agent?.color ?? 'var(--muted)'}">{agent?.display ?? row.node.from}</span>
          <span class="gbadge {BADGE_CLASS[row.node.type]}">{row.node.type}</span>
          <span class="gtx">{row.node.summary}</span>
          {#if isHere}<span class="heretag">◂ here</span>{/if}
        </div>
        <div class="gmeta">
          {#if row.hop}<span class="gtp cross">#{topicOf(row.node)}</span>{/if}
          <button
            type="button"
            class="gid"
            class:copied={copiedIds.has(row.node.msgId)}
            onclick={(e) => copyNodeId(e, row.node.msgId)}
            aria-label={copiedIds.has(row.node.msgId) ? `Copied ${row.node.msgId}` : `Copy id ${row.node.msgId}`}
            title={row.node.msgId}
          >
            <Icon name={copiedIds.has(row.node.msgId) ? 'check' : 'copy'} size={10} />
            {shortId(row.node.msgId)}
          </button>
          {#if row.node.ts}<span class="gts" title={formatIso8601(row.node.ts)}>{formatClock(row.node.ts)}</span>{/if}
        </div>
        {#if row.hop}<div class="hop {row.hop.cls}">{row.hop.label}</div>{/if}
      </div>
    </div>
  {/each}

  <div class="pk-keys">
    <span><span class="kk">j</span><span class="kk">k</span> move</span>
    <span><span class="kk">l</span> deeper</span>
    <span><span class="kk">h</span> back</span>
    <span><span class="kk">↵</span> jump to it</span>
    <span><span class="kk">esc</span> close</span>
  </div>
</div>

<style>
  .mt {
    text-align: left;
  }

  /* ── breadcrumb — the REAL root->focused path (thread.ts's pathToRoot),
       however many hops deep it actually is ── */
  .crumbs {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 12px;
    font-size: 11px;
  }
  .crumbs .cz {
    color: var(--muted);
    white-space: nowrap;
    border: 0;
    background: transparent;
    padding: 2px 4px;
    border-radius: 4px;
    font: inherit;
  }
  button.cz:hover {
    color: var(--text);
    background: var(--panel-2);
  }
  button.cz:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .crumbs .cz.here {
    color: var(--text);
    font-weight: 650;
  }
  .crumbs .sep {
    color: var(--faint);
  }

  /* ── the Focused card — the currently-focused node's full content ── */
  .focused {
    background: var(--panel);
    border: 1px solid var(--border-2);
    border-radius: 10px;
    padding: 11px 12px;
    margin-bottom: 16px;
  }
  .focused .ft {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }
  .focused .av {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    font: 700 10px/1 var(--mono);
  }
  .focused .fwho {
    font-weight: 650;
    font-size: 13px;
  }
  .focused .fts {
    font-size: 10.5px;
    color: var(--faint);
  }
  .focused .jumpbtn {
    margin-left: auto;
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--accent);
    font: 600 11px/1 var(--mono);
    padding: 5px 9px;
    border-radius: 7px;
  }
  .focused .jumpbtn:hover {
    background: var(--panel-3);
    border-color: var(--accent);
  }
  .focused .jumpbtn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .focused .fx {
    font-size: 13px;
    color: var(--text);
    line-height: 1.55;
  }
  .focused .frefs {
    margin-top: 8px;
  }

  .ctx-note {
    color: var(--muted);
    font-size: 12.5px;
    margin: 0 0 14px;
  }
  .mt-stats {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 13px;
  }
  .mt-stats .stat {
    font: 600 11px/1 var(--mono);
    color: var(--muted);
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 7px;
    padding: 6px 8px;
  }
  .mt-stats .stat b {
    color: var(--text);
  }
  .mt-legend {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
    margin-bottom: 16px;
    font: 600 10.5px/1 var(--mono);
    color: var(--muted);
  }
  .mt-legend .lg {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .mt-legend .lg i {
    width: 9px;
    height: 9px;
    border-radius: 3px;
  }
  .mt-legend .xr {
    color: var(--faint);
  }
  .gn {
    display: flex;
    gap: 12px;
    align-items: stretch;
  }
  .gn .rail {
    position: relative;
    width: 14px;
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .gn .seg {
    width: 2px;
    border-radius: 2px;
  }
  .gn .seg.top {
    height: 9px;
    flex: 0 0 auto;
  }
  .gn .seg.bot {
    flex: 1;
    min-height: 9px;
  }
  .gn .cd {
    width: 11px;
    height: 11px;
    border-radius: 50%;
    flex: 0 0 auto;
    z-index: 1;
  }
  /* `git log --oneline` density: one tight row per node, not a padded
     "card". Rows are only as tall as their line-height plus a hairline of
     breathing room — the hop indicator (optional) adds its own margin when
     present. */
  .gn .gcard {
    flex: 1;
    min-width: 0;
    padding: 3px 0;
    display: block;
    text-align: left;
    cursor: pointer;
    border-radius: 6px;
  }
  .gn .gcard:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  /* The currently-focused node — appearance-encodes state the same way
     piece 1's agent nodes do (position is stable, appearance shows "how/
     which one right now"). */
  .gn.here .gcard {
    background: color-mix(in srgb, var(--accent) 9%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 35%, transparent);
    padding-left: 6px;
    padding-right: 6px;
  }
  .gn:last-child .gcard {
    padding-bottom: 2px;
  }
  .gcard :global(.gline) {
    display: flex;
    align-items: baseline;
    gap: 7px;
    min-width: 0;
  }
  .gcard :global(.gmeta) {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px 7px;
    min-width: 0;
    margin-top: 3px;
  }
  .gcard :global(.gwho) {
    flex: 0 0 auto;
    font-weight: 650;
    font-size: 12.5px;
  }
  .gcard :global(.gtp) {
    flex: 0 0 auto;
    font: 600 9.5px/1 var(--mono);
    color: var(--faint);
  }
  .gcard :global(.gtp.cross) {
    color: var(--claimed);
  }
  .gcard :global(.gbadge) {
    flex: 0 0 auto;
    font: 800 8.5px/1 var(--mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 3px 5px;
    border-radius: 5px;
  }
  .gcard :global(.b-request) {
    color: var(--open);
    background: color-mix(in srgb, var(--open) 15%, transparent);
  }
  .gcard :global(.b-claim) {
    color: var(--claimed);
    background: color-mix(in srgb, var(--claimed) 15%, transparent);
  }
  .gcard :global(.b-blocked) {
    color: var(--blocked);
    background: color-mix(in srgb, var(--blocked) 15%, transparent);
  }
  .gcard :global(.b-note) {
    color: var(--muted);
    background: color-mix(in srgb, var(--muted) 18%, transparent);
  }
  .gcard :global(.b-done) {
    color: var(--done);
    background: color-mix(in srgb, var(--done) 15%, transparent);
  }
  .gcard :global(.gtx) {
    min-width: 0;
    flex: 1 1 auto;
    font-size: 12.5px;
    color: var(--muted);
    line-height: 1.4;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .gcard :global(.heretag) {
    flex: 0 0 auto;
    font: 700 9.5px/1 var(--mono);
    color: var(--accent);
  }
  .gcard :global(.gts) {
    flex: 0 0 auto;
    font: 500 10.5px/1 var(--mono);
    color: var(--faint);
  }
  .gcard :global(.gbody) {
    margin: 6px 0 2px;
    font-size: 12.5px;
    color: var(--text);
    line-height: 1.55;
    font-weight: 400;
  }
  .gcard :global(.gid) {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px 2px 5px;
    border: 1px solid transparent;
    border-radius: 5px;
    background: transparent;
    font: 500 10px/1 var(--mono);
    color: var(--faint);
    cursor: pointer;
  }
  .gcard :global(.gid:hover),
  .gcard :global(.gid:focus-visible) {
    color: var(--text);
    border-color: var(--border-2);
    background: var(--panel-2);
  }
  .gcard :global(.gid.copied) {
    color: var(--done);
    border-color: var(--done);
    background: color-mix(in srgb, var(--done) 12%, transparent);
  }
  .gcard :global(.hop) {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-top: 4px;
    font: 700 9.5px/1 var(--mono);
    letter-spacing: 0.03em;
    padding: 5px 8px;
    border-radius: 7px;
  }
  .gcard :global(.hop-in) {
    color: #8b8bf0;
    background: color-mix(in srgb, #8b8bf0 14%, transparent);
    border: 1px solid color-mix(in srgb, #8b8bf0 38%, transparent);
  }
  .gcard :global(.hop-back) {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 38%, transparent);
  }

  /* ── keyboard legend ── */
  .pk-keys {
    margin-top: 16px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-wrap: wrap;
    gap: 11px;
    font: 500 10.5px/1 var(--mono);
    color: var(--muted);
  }
  .pk-keys .kk {
    color: var(--text);
    border: 1px solid var(--border-2);
    border-radius: 4px;
    padding: 1px 5px;
    margin-right: 2px;
  }
</style>
