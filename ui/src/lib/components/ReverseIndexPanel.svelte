<script lang="ts">
  // "Conversations about this code" — ports `.cvx`/`.cvitem`/`.spanline` from
  // design/serve-dashboard-v2-mockup.html. Walks the `--ref` index backwards:
  // given a repo, a file, or a line range, list every thread that discussed
  // it, across hubs, public *and* private (private hubs get a distinct
  // badge). design/44 §6 item 2.4 — when `repo` is set and `path` is null,
  // this renders REPO-MODE instead: `hits` (a whole-repo getRefs(hub, repo)
  // fetch) grouped by file, each row drilling into that file.
  import type { RefHit } from '../types';
  import { formatAge, formatIsoDate } from '../format';
  import { groupRefHitsByFile } from '../codeTree';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import FileIcon from './FileIcon.svelte';

  interface Props {
    hits: RefHit[];
    /** Optional header context — repo/path/range this reverse index is for.
     * `repo` set with `path` null means REPO-MODE (design/44 §6 2.4). */
    repo?: string | null;
    path?: string | null;
    range?: [number, number] | null;
    loading?: boolean;
    onSelectHit?: (hit: RefHit) => void;
    /** Fired when the "↩ whole file" chip is clicked — only rendered when
     * `range` narrows this panel to a specific line range (a hot-line click
     * in CodeLens). Returns the panel to the file's full hit list. */
    onWholeFile?: () => void;
    /** design/44 §6 item 2.4 — fired when the breadcrumb's repo segment is
     * clicked (widens from file/range scope out to the whole-repo rollup). */
    onWidenToRepo?: () => void;
    /** design/44 §6 item 2.4 — fired when a repo-mode file-group row is
     * clicked (narrows from repo scope down into that one file). */
    onSelectFile?: (path: string) => void;
  }

  let {
    hits,
    repo = null,
    path = null,
    range = null,
    loading = false,
    onSelectHit,
    onWholeFile,
    onWidenToRepo,
    onSelectFile,
  }: Props = $props();

  const hubCount = $derived(new Set(hits.map((h) => h.hub)).size);
  const rangeLabel = $derived(range ? `L${range[0]}–${range[1]}` : '');
  /** design/44 §6 item 2.4 — repo scope: a repo without a path. */
  const repoMode = $derived(!!repo && !path);
  const fileGroups = $derived(repoMode ? groupRefHitsByFile(hits) : []);

  function cap(s: string): string {
    return s.length ? s[0]!.toUpperCase() + s.slice(1) : s;
  }
</script>

<div class="cvx">
  <p class="ctx-note">
    Every <code class="mono">--ref</code> is indexed, so you can walk it backwards: given a repo, a file, or a line range,
    find every thread that discussed it — across hubs, public <b>and</b> private.
  </p>

  {#if repoMode}
    <!-- design/44 §6 item 2.4 — repo rollup breadcrumb: just the repo name,
         already at the widest scope (nothing further to widen to here). -->
    <div class="spanline" data-testid="crumb-repo-mode">
      <span class="crumb-leaf">{repo}</span> · {fileGroups.length} file{fileGroups.length === 1 ? '' : 's'} ·
      {hits.length} ref{hits.length === 1 ? '' : 's'} · {hubCount} hub{hubCount === 1 ? '' : 's'}
    </div>
  {:else if repo || path}
    <!-- design/44 §6 item 2.4 — bidirectional breadcrumb: `repo ▸ file ▸
         L44-49`, each segment (but the terminal one) clickable to widen the
         scope back out. Narrowing happens by clicking a hit/file row. -->
    <div class="spanline" data-testid="crumb-hits-mode">
      {#if repo}
        <button type="button" class="crumb-seg" onclick={() => onWidenToRepo?.()} data-testid="crumb-repo-seg">{repo}</button>
      {/if}
      {#if repo && path}<span class="crumb-sep">▸</span>{/if}
      {#if path}
        {#if range}
          <button type="button" class="crumb-seg" onclick={() => onWholeFile?.()} data-testid="crumb-file-seg">{path.split('/').pop()}</button>
          <span class="crumb-sep">▸</span>
          <span class="crumb-leaf">{rangeLabel}</span>
        {:else}
          <span class="crumb-leaf tp">{path.split('/').pop()}</span>
        {/if}
      {/if}
      <span class="crumb-counts">· {hits.length} ref{hits.length === 1 ? '' : 's'} · {hubCount} hub{hubCount === 1 ? '' : 's'}</span>
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
  {:else if repoMode}
    {#if fileGroups.length === 0}
      <EmptyState
        glyph="↩"
        title="No conversations yet"
        body="Nobody has referenced this repo in a --ref yet. Once they do, the files show up here — backwards, from code to conversation."
      />
    {:else}
      {#each fileGroups as g (g.path)}
        <button type="button" class="cvitem filegroup" onclick={() => onSelectFile?.(g.path)}>
          <div class="cvtop">
            <FileIcon path={g.path} size={13} />
            <span class="cvtitle filepath">{g.path}</span>
            <span class="cvtime">{formatAge(g.lastTs)}</span>
          </div>
          <div class="cvsnip">{g.count} conversation{g.count === 1 ? '' : 's'} · last {formatIsoDate(g.lastTs)}</div>
        </button>
      {/each}
    {/if}
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
  .crumb-seg {
    font-family: var(--mono);
    font-size: 11.5px;
    font-weight: 600;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    padding: 2px 6px;
    border-radius: 5px;
    border: 0;
    cursor: pointer;
  }
  .crumb-seg:hover {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .crumb-sep {
    color: var(--faint);
  }
  .crumb-leaf {
    font-family: var(--mono);
    color: var(--muted);
    font-size: 11.5px;
  }
  .crumb-counts {
    color: var(--muted);
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
  .cvitem.filegroup .cvtop {
    gap: 8px;
  }
  .cvitem.filegroup .filepath {
    font: 600 12.5px/1 var(--mono);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
