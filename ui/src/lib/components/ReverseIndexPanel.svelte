<script lang="ts">
  // "Conversations about this code" — ports `.cvx`/`.cvitem`/`.spanline` from
  // design/serve-dashboard-v2-mockup.html. Walks the `--ref` index backwards:
  // given a file + line range, list every thread that discussed it, across
  // hubs, public *and* private (private hubs get a distinct badge).
  import type { RefHit } from '../types';
  import { formatAge } from '../format';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    hits: RefHit[];
    /** Optional header context — repo/path/range this reverse index is for. */
    repo?: string | null;
    path?: string | null;
    range?: [number, number] | null;
    loading?: boolean;
    onSelectHit?: (hit: RefHit) => void;
    /** Fired when the "↩ whole file" chip is clicked — only rendered when
     * `range` narrows this panel to a specific line range (a hot-line click
     * in CodeLens). Returns the panel to the file's full hit list. */
    onWholeFile?: () => void;
  }

  let { hits, repo = null, path = null, range = null, loading = false, onSelectHit, onWholeFile }: Props = $props();

  const hubCount = $derived(new Set(hits.map((h) => h.hub)).size);
  const rangeLabel = $derived(range ? ` L${range[0]}–${range[1]}` : '');

  function cap(s: string): string {
    return s.length ? s[0]!.toUpperCase() + s.slice(1) : s;
  }
</script>

<div class="cvx">
  <p class="ctx-note">
    Every <code class="mono">--ref</code> is indexed, so you can walk it backwards: given a file + line range, find every
    thread that discussed it — across hubs, public <b>and</b> private.
  </p>

  {#if repo && path}
    <div class="spanline">
      {repo} · <span class="tp">{path.split('/').pop()}</span>{rangeLabel} · {hits.length} ref{hits.length === 1 ? '' : 's'} · {hubCount}
      hub{hubCount === 1 ? '' : 's'}
    </div>
  {/if}

  {#if range}
    <button type="button" class="whole-file-chip" onclick={() => onWholeFile?.()}>
      <Icon name="corner-down-left" size={12} />
      <span>whole file</span>
    </button>
  {/if}

  {#if loading}
    <p class="ctx-note">Loading…</p>
  {:else if hits.length === 0}
    <EmptyState
      glyph="↩"
      title="No conversations yet"
      body="Nobody has referenced these exact lines in a --ref yet. Once they do, the thread shows up here — backwards, from code to conversation."
    />
  {:else}
    {#each hits as hit (hit.msgId)}
      <button type="button" class="cvitem" onclick={() => onSelectHit?.(hit)}>
        <div class="cvtop">
          <span class="cvhub" class:priv={hit.hubPrivate}>{hit.hub}{hit.hubPrivate ? ' · priv' : ''}</span>
          {#if hit.topic}<span class="cvtopic">#{hit.topic}</span>{/if}
          <span class="cvtime">{formatAge(hit.ts)}</span>
        </div>
        <div class="cvtitle">{cap(hit.from)} · {hit.msgType}</div>
        <div class="cvsnip">{hit.summary}</div>
      </button>
    {/each}
  {/if}
</div>

<style>
  .cvx {
    text-align: left;
  }
  .ctx-note {
    color: var(--muted);
    font-size: 12.5px;
    margin: 0 0 14px;
  }
  .spanline {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 14px;
    flex-wrap: wrap;
  }
  .spanline .tp {
    font-family: var(--mono);
    color: var(--accent);
    font-size: 11.5px;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    padding: 2px 6px;
    border-radius: 5px;
  }
  .whole-file-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--muted);
    font: 600 11px/1 var(--mono);
    padding: 5px 10px;
    border-radius: 999px;
    margin-bottom: 14px;
  }
  .whole-file-chip:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .cvitem {
    display: block;
    width: 100%;
    padding: 11px 12px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--panel-2);
    margin-bottom: 10px;
    cursor: pointer;
    text-align: left;
    font: inherit;
    color: inherit;
  }
  .cvitem:hover {
    border-color: var(--border-2);
  }
  .cvtop {
    display: flex;
    align-items: center;
    gap: 7px;
    flex-wrap: wrap;
    margin-bottom: 5px;
  }
  .cvhub {
    font: 600 9.5px/1 var(--mono);
    color: var(--muted);
    background: var(--panel-3);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 6px;
  }
  .cvhub.priv {
    color: var(--deferred);
    border-color: color-mix(in srgb, var(--deferred) 40%, var(--border));
  }
  .cvtopic {
    font: 600 10px/1 var(--mono);
    color: var(--faint);
  }
  .cvtime {
    margin-left: auto;
    font: 500 10px/1 var(--mono);
    color: var(--faint);
  }
  .cvtitle {
    font-weight: 600;
    font-size: 13px;
  }
  .cvsnip {
    font-size: 12px;
    color: var(--muted);
    margin-top: 3px;
    line-height: 1.45;
  }
</style>
