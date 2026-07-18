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
  import { copyToClipboard } from '../clipboard';
  import Icon from './Icon.svelte';
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
          <div class="gtx">{row.node.summary}</div>
          {#if renderedBody}
            <button
              type="button"
              class="node-expand-toggle"
              class:open={expanded}
              aria-expanded={expanded}
              aria-label={expanded ? 'Collapse message' : 'Expand message'}
              onclick={(e) => {
                e.stopPropagation();
                toggleExpanded(row.node.msgId);
              }}
            >
              <span>{expanded ? 'Show less' : 'Show more'}</span>
              <svg class="chev" viewBox="0 0 16 16" width="11" height="11" aria-hidden="true">
                <polyline points="4 6 8 10 12 6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </button>
          {/if}
        </div>
        {#if renderedBody && expanded}
          <div class="gbody prose md" use:highlightBody>
            {@html renderedBody}
          </div>
        {/if}
        {#if row.hop}<div class="hop {row.hop.cls}">{row.hop.label}</div>{/if}
        <button
          type="button"
          class="gid"
          class:copied={copiedIds.has(row.node.msgId)}
          onclick={(e) => copyNodeId(e, row.node.msgId)}
          aria-label={copiedIds.has(row.node.msgId) ? `Copied ${row.node.msgId}` : `Copy id ${row.node.msgId}`}
          title="Click to copy id"
        >
          <Icon name={copiedIds.has(row.node.msgId) ? 'check' : 'copy'} size={10} />
          {row.node.msgId}
        </button>
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
    gap: 8px;
  }
  .gcard :global(.gtx) {
    min-width: 0;
    flex: 1 1 auto;
    font-size: 12.5px;
    color: var(--muted);
    line-height: 1.45;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Same unified expand/collapse pill as Message.svelte's `.expand-toggle` —
     a text label ("Show more"/"Show less") plus a chevron that flips, not a
     bare unlabeled glyph. Duplicated (not shared via app.css) because this
     is real component markup, not `{@html}` content — but it's the same
     visual language everywhere a note/node can be expanded. */
  .gcard :global(.node-expand-toggle) {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 1px solid var(--border-2);
    border-radius: 999px;
    background: var(--panel-2);
    padding: 3px 9px 3px 10px;
    color: var(--muted);
    font: 600 11px/1.2 var(--mono);
    letter-spacing: 0.01em;
    cursor: pointer;
    transition:
      background 0.12s ease,
      color 0.12s ease,
      border-color 0.12s ease;
  }
  .gcard :global(.node-expand-toggle .chev) {
    transform: rotate(0deg);
    transition: transform 0.15s ease;
  }
  .gcard :global(.node-expand-toggle.open .chev) {
    transform: rotate(180deg);
  }
  .gcard :global(.node-expand-toggle:hover),
  .gcard :global(.node-expand-toggle:focus-visible) {
    color: var(--text);
    background: var(--panel-3);
    border-color: var(--accent);
  }
  .gcard :global(.gbody) {
    margin: 8px 0 0;
    font-size: 12.5px;
    color: var(--text);
    line-height: 1.55;
    font-weight: 400;
  }
  .gcard :global(.gid) {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-top: 7px;
    padding: 3px 6px 3px 5px;
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
