<script lang="ts">
  // A pinned `--ref` inline in a chat message, board row, or request detail.
  // Ports `.refcard`/`.ref-head`/`.code`/`.code-peek`/`.ref-foot` from
  // design/serve-dashboard-v2-mockup.html — a git-pinned code snippet with a
  // staleness badge, Shiki-highlighted lines, auto-collapse-on-size, and the
  // "N conversations reference these lines" reverse-index hook.
  import type { CodeRef, RefHit } from '../types';
  import { api } from '../api';
  import { highlightSnippetLines, type HighlightedLine, resolveLang } from '../highlight';
  import EmptyState from './EmptyState.svelte';
  import Skeleton from './Skeleton.svelte';

  interface Props {
    ref: CodeRef;
    hub: string;
    /** Lines at/above this count start collapsed to a one-line peek. */
    collapseThreshold?: number;
    onRevHook?: (ref: CodeRef, hits: RefHit[]) => void;
  }

  let { ref, hub, collapseThreshold = 8, onRevHook }: Props = $props();

  const STALENESS_LABEL: Record<string, string> = {
    current: 'current',
    changed: 'changed',
    moved: 'moved',
    unpinned: 'unpinned',
    unknown: 'unknown',
  };

  let loading = $state(true);
  let loadError = $state(false);
  let lines = $state<{ n: number; text: string }[]>([]);
  let staleness = $state<string>('unknown');
  let lang = $state<string | null>(null);
  let highlighted = $state<HighlightedLine[]>([]);
  let refHits = $state<RefHit[]>([]);
  let collapsedOverride = $state<boolean | null>(null);

  const rangeLabel = $derived(ref.range ? `L${ref.range[0]}–${ref.range[1]}` : null);
  const target = $derived(`${ref.repo}:${ref.path}${ref.range ? `@${ref.range[0]}-${ref.range[1]}` : ''}`);
  const autoCollapse = $derived(lines.length >= collapseThreshold);
  const collapsed = $derived(collapsedOverride ?? autoCollapse);
  const peekLine = $derived(lines[0] ?? null);

  function toggle() {
    collapsedOverride = !collapsed;
  }

  async function load() {
    loading = true;
    loadError = false;
    try {
      const range = ref.range ? `${ref.range[0]}-${ref.range[1]}` : undefined;
      const [snippet, hits] = await Promise.all([
        api.getCode(hub, ref.repo, ref.path, ref.sha, range),
        api.getRefs(hub, target, true),
      ]);
      lines = snippet.lines;
      staleness = snippet.staleness;
      lang = snippet.lang;
      refHits = hits;
      highlighted = lines.length ? await highlightSnippetLines(lines, resolveLang(lang)) : [];
    } catch {
      loadError = true;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    // Re-fetch whenever the ref identity (or hub) changes.
    void ref;
    void hub;
    void load();
  });

  function highlightedFor(n: number): HighlightedLine | undefined {
    return highlighted.find((h) => h.n === n);
  }

  function revHook(e: MouseEvent) {
    // Nested inside Message.svelte's own clickable `.msg` row (onclick ->
    // selectMessage, which resets the sidebar's contextMode to the
    // meta-thread pane) — stop propagation so opening the reverse index
    // doesn't get immediately stomped back to the meta-thread pane by the
    // bubbled selectMessage call.
    e.stopPropagation();
    onRevHook?.(ref, refHits);
  }

  function toggleFromClick(e: MouseEvent) {
    // Same nesting concern as revHook above — this is an in-card expand/
    // collapse, not "select this message".
    e.stopPropagation();
    toggle();
  }
</script>

<div class="refcard" class:collapsed>
  <div class="ref-head">
    <span class="repo">◆ {ref.repo}</span>
    <span class="path">{ref.path}</span>
    <span class="sha">@{ref.sha}</span>
    {#if rangeLabel}<span class="lines">{rangeLabel}</span>{/if}
    <span class="stale-badge stale-{staleness}" data-testid="staleness-badge">{STALENESS_LABEL[staleness] ?? staleness}</span>
    {#if lines.length > 0}
      <button type="button" class="ref-toggle" onclick={toggleFromClick} data-testid="ref-toggle">
        <span class="lbl">{collapsed ? 'Expand' : 'Collapse'}</span>
        <span class="chev">{collapsed ? '▾' : '▴'}</span>
      </button>
    {/if}
  </div>

  {#if loading}
    <Skeleton rows={2} />
  {:else if loadError || lines.length === 0}
    <EmptyState
      glyph="◇"
      title="No code available"
      body={`confer can only show code it can read from a local clone. Map a clone of ${ref.repo} to see ${ref.path}${rangeLabel ? ` ${rangeLabel}` : ''}.`}
      actionLabel="＋ map a clone to see the code"
      disabled
    />
  {:else}
    {#if collapsed && peekLine}
      <div
        class="code-peek"
        onclick={toggleFromClick}
        role="button"
        tabindex="0"
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            e.stopPropagation();
            toggle();
          }
        }}
      >
        <span class="peek-ln mono">{peekLine.n}</span>
        <span class="peek-code mono">{peekLine.text}</span>
        <span class="peek-more">{lines.length} lines · expand</span>
      </div>
    {:else}
      <div class="code" data-lang={lang}>
        {#each lines as line (line.n)}
          <div class="cl">
            <span class="ln">{line.n}</span>
            <span class="cc">
              {#each highlightedFor(line.n)?.tokens ?? [{ text: line.text, style: '' }] as tok, i (i)}
                <span class="shiki-tok" style={tok.style}>{tok.text}</span>
              {/each}
            </span>
          </div>
        {/each}
      </div>
    {/if}
  {/if}

  <div class="ref-foot">
    <span>pinned to <b>{ref.sha}</b> · immutable</span>
    {#if refHits.length > 0}
      <button type="button" class="revhook" onclick={revHook} data-testid="revhook">
        ↩ {refHits.length} conversation{refHits.length === 1 ? '' : 's'} reference these lines
      </button>
    {/if}
  </div>
</div>

<style>
  .refcard {
    margin-top: 10px;
    border: 1px solid var(--border-2);
    border-radius: 10px;
    overflow: hidden;
    background: var(--panel-2);
  }
  .ref-head {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 9px 12px;
    background: var(--panel-3);
    border-bottom: 1px solid var(--border);
  }
  .ref-head .repo {
    font: 700 11px/1 var(--mono);
    color: var(--accent);
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .ref-head .path {
    font: 600 11.5px/1 var(--mono);
    color: var(--text);
  }
  .ref-head .sha {
    font: 600 10.5px/1 var(--mono);
    color: var(--muted);
    background: var(--panel);
    border: 1px solid var(--border-2);
    border-radius: 5px;
    padding: 3px 6px;
  }
  .ref-head .lines {
    font: 700 10.5px/1 var(--mono);
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 13%, transparent);
    border-radius: 5px;
    padding: 3px 7px;
  }
  .stale-badge {
    font: 700 9.5px/1 var(--mono);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    border-radius: 5px;
    padding: 3px 6px;
  }
  .stale-current {
    color: var(--done);
    background: color-mix(in srgb, var(--done) 16%, transparent);
  }
  .stale-changed {
    color: var(--blocked);
    background: color-mix(in srgb, var(--blocked) 16%, transparent);
  }
  .stale-moved {
    color: var(--deferred);
    background: color-mix(in srgb, var(--deferred) 16%, transparent);
  }
  .stale-unpinned {
    color: var(--muted);
    background: color-mix(in srgb, var(--muted) 16%, transparent);
  }
  .stale-unknown {
    color: var(--faint);
    background: color-mix(in srgb, var(--faint) 16%, transparent);
  }
  .ref-head .ref-toggle {
    margin-left: auto;
    border: 1px solid var(--border-2);
    background: var(--panel);
    color: var(--muted);
    cursor: pointer;
    font: 700 12px/1 var(--mono);
    height: 26px;
    padding: 0 10px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border-radius: 7px;
    letter-spacing: 0.03em;
  }
  .ref-head .ref-toggle:hover {
    color: var(--text);
    border-color: var(--faint);
    background: var(--panel-3);
  }
  .ref-head .ref-toggle .chev {
    font-size: 10px;
  }
  .code {
    font: 500 12px/1.65 var(--mono);
    padding: 8px 0;
    overflow-x: auto;
    max-height: 264px;
    overflow-y: auto;
  }
  .code .cl {
    display: flex;
    padding: 0 12px;
    white-space: pre;
  }
  .code .ln {
    width: 24px;
    flex: 0 0 auto;
    text-align: right;
    margin-right: 14px;
    color: var(--faint);
    user-select: none;
  }
  .code .cc {
    color: var(--text);
  }
  .code-peek {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    cursor: pointer;
  }
  .code-peek .peek-ln {
    color: var(--faint);
    flex: 0 0 auto;
  }
  .code-peek .peek-code {
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .code-peek .peek-more {
    color: var(--accent);
    font: 600 11px/1 var(--mono);
    white-space: nowrap;
    flex: 0 0 auto;
  }
  .code-peek:hover .peek-more {
    text-decoration: underline;
  }
  .ref-foot {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    font: 500 10.5px/1 var(--mono);
    color: var(--faint);
  }
  .ref-foot b {
    color: var(--muted);
  }
  .ref-foot .revhook {
    margin-left: auto;
    color: var(--accent);
    cursor: pointer;
    font-weight: 600;
    border: 0;
    background: transparent;
    font-family: var(--mono);
    font-size: inherit;
    padding: 0;
  }
  .ref-foot .revhook:hover {
    text-decoration: underline;
  }
</style>
