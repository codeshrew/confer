<script lang="ts">
  // The reference graph — a git-native commit graph woven across topics.
  // Ports `.mt`/`.gn`/`.gcard` from design/serve-dashboard-v2-mockup.html:
  // a vertical rail whose color encodes the topic, gradienting at
  // cross-topic crossings, with agent-colored dots ringed in the topic color.
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
  import { renderMarkdown, highlightRenderedCodeBlocks } from '../markdown';
  import type { Agent, Message as MessageT, MsgType, ThreadNode } from '../types';

  interface Props {
    thread: ThreadNode[];
    agents: Agent[];
    messages?: MessageT[];
    /** `'summary'` (default) shows each node's summary line, collapsed, with
     * a chevron to expand the full rendered body when it's loaded (see the
     * CONTRACT GAP note above); `'full'` shows the body by default. */
    density?: 'summary' | 'full';
    onSelectNode?: (msgId: string) => void;
  }

  let { thread, agents, messages = [], density = 'summary', onSelectNode }: Props = $props();

  const agentsById = $derived(new Map(agents.map((a) => [a.id, a])));
  const messagesById = $derived(new Map(messages.map((m) => [m.id, m])));

  // Per-node expand state, independent of every other node and of the
  // global `density` toggle — mirrors Message.svelte's own `expanded`
  // pattern (keyed by msgId here since this is a list, not a single node).
  let expandedIds = $state(new Set<string>());
  function toggleExpanded(msgId: string) {
    const next = new Set(expandedIds);
    if (next.has(msgId)) next.delete(msgId);
    else next.add(msgId);
    expandedIds = next;
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

  function topicOf(node: ThreadNode): string {
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
    node: ThreadNode;
    topColor: string;
    botColor: string;
    hop: { label: string; cls: string } | null;
  }

  const rows = $derived.by((): Row[] => {
    const rootTopic = thread.length ? topicOf(thread[0]!) : null;
    return thread.map((node, i) => {
      const topic = topicOf(node);
      const color = colorFor(topic);
      const topColor = i === 0 ? 'transparent' : color;
      const nextTopic = i < thread.length - 1 ? topicOf(thread[i + 1]!) : null;
      const botColor =
        i === thread.length - 1
          ? 'transparent'
          : nextTopic === topic
            ? color
            : `linear-gradient(180deg, ${color}, ${colorFor(nextTopic!)})`;
      const prevTopic = i > 0 ? topicOf(thread[i - 1]!) : null;
      let hop: Row['hop'] = null;
      if (i > 0 && topic !== prevTopic) {
        hop =
          topic === rootTopic
            ? { label: `↩ resolves back in #${topic}`, cls: 'hop-back' }
            : { label: `↗ thread crosses into #${topic}`, cls: 'hop-in' };
      }
      return { node, topColor, botColor, hop };
    });
  });

  const agentCount = $derived(new Set(thread.map((n) => n.from)).size);
  const span = $derived.by(() => {
    if (!thread.length) return null;
    const first = messagesById.get(thread[0]!.msgId);
    const last = messagesById.get(thread[thread.length - 1]!.msgId);
    if (!first || !last) return null;
    const mins = Math.round((new Date(last.ts).getTime() - new Date(first.ts).getTime()) / 60_000);
    return mins < 60 ? `${mins}m` : `${Math.round(mins / 60)}h`;
  });
</script>

<div class="mt">
  <p class="ctx-note">
    Walked from <code class="mono">of</code>/<code class="mono">reply_to</code> hashes — every node is a signed commit.
    {#if uniqueTopics.length > 1}This thread weaves across {uniqueTopics.length} topics.{/if}
  </p>
  <div class="mt-stats">
    <span class="stat"><b>{uniqueTopics.length}</b> topics</span>
    <span class="stat"><b>{thread.length}</b> messages</span>
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
    {@const renderedBody = renderedBodyFor(row.node.msgId)}
    {@const expanded = density === 'full' || expandedIds.has(row.node.msgId)}
    <div class="gn">
      <div class="rail">
        <span class="seg top" style="background:{row.topColor}"></span>
        <span class="cd" style="background:{agent?.color ?? 'var(--muted)'};box-shadow:0 0 0 2px var(--panel), 0 0 0 3.5px {colorFor(topicOf(row.node))}"></span>
        <span class="seg bot" style="background:{row.botColor}"></span>
      </div>
      <div
        class="gcard"
        role="button"
        tabindex="0"
        onclick={() => onSelectNode?.(row.node.msgId)}
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') onSelectNode?.(row.node.msgId);
        }}
      >
        <div class="grow">
          <span class="gwho" style="color:{agent?.color ?? 'var(--muted)'}">{agent?.display ?? row.node.from}</span>
          <span class="gbadge {BADGE_CLASS[row.node.type]}">{row.node.type}</span>
          <span class="gtp" class:cross={row.hop !== null}>#{topicOf(row.node)}</span>
        </div>
        <div class="gtx-row">
          {#if renderedBody}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <button
              type="button"
              class="node-expand-chevron"
              class:open={expanded}
              aria-expanded={expanded}
              aria-label={expanded ? 'Collapse message' : 'Expand message'}
              onclick={(e) => {
                e.stopPropagation();
                toggleExpanded(row.node.msgId);
              }}
            >
              ▸
            </button>
          {/if}
          <div class="gtx">{row.node.summary}</div>
        </div>
        {#if renderedBody && expanded}
          <div class="gbody prose md" use:highlightBody>
            {@html renderedBody}
          </div>
        {/if}
        {#if row.hop}<div class="hop {row.hop.cls}">{row.hop.label}</div>{/if}
        <div class="gid">{row.node.msgId}</div>
      </div>
    </div>
  {/each}
</div>

<style>
  .mt {
    text-align: left;
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
    height: 13px;
    flex: 0 0 auto;
  }
  .gn .seg.bot {
    flex: 1;
    min-height: 14px;
  }
  .gn .cd {
    width: 13px;
    height: 13px;
    border-radius: 50%;
    flex: 0 0 auto;
    z-index: 1;
  }
  .gn .gcard {
    flex: 1;
    min-width: 0;
    padding: 0 0 16px;
    display: block;
    text-align: left;
    border: 0;
    background: transparent;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .gn:last-child .gcard {
    padding-bottom: 2px;
  }
  .gcard :global(.grow) {
    display: flex;
    align-items: center;
    gap: 7px;
    flex-wrap: wrap;
    margin-bottom: 5px;
  }
  .gcard :global(.gwho) {
    font-weight: 650;
    font-size: 12.5px;
  }
  .gcard :global(.gtp) {
    margin-left: auto;
    font: 600 9.5px/1 var(--mono);
    color: var(--faint);
  }
  .gcard :global(.gtp.cross) {
    color: var(--claimed);
  }
  .gcard :global(.gbadge) {
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
  .gcard :global(.gtx-row) {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .gcard :global(.gtx) {
    font-size: 12.5px;
    color: var(--muted);
    line-height: 1.45;
  }
  /* Same real, obvious tap-target treatment as Message.svelte's own
     `.expand-chevron` — ≥24×24px with a hover/focus background, not a bare
     unstyled glyph. */
  .gcard :global(.node-expand-chevron) {
    flex: 0 0 auto;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    margin: -3px 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    padding: 0;
    color: var(--muted);
    font-size: 14px;
    line-height: 1;
    cursor: pointer;
    transform: rotate(0deg);
    transition:
      transform 0.12s ease,
      background 0.12s ease,
      color 0.12s ease;
  }
  .gcard :global(.node-expand-chevron.open) {
    transform: rotate(90deg);
  }
  .gcard :global(.node-expand-chevron:hover),
  .gcard :global(.node-expand-chevron:focus-visible) {
    color: var(--text);
    background: var(--panel-2);
  }
  .gcard :global(.gbody) {
    margin: 6px 0 0;
    font-size: 12.5px;
    color: var(--text);
    line-height: 1.5;
  }
  .gcard :global(.gid) {
    font: 500 10px/1 var(--mono);
    color: var(--faint);
    margin-top: 7px;
  }
  .gcard :global(.hop) {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-top: 8px;
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
</style>
