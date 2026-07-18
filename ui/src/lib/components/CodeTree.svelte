<script lang="ts">
  // design/43 Phase B — the Code view's navigator: a real collapsible tree
  // (repo -> compacted directories -> files) living in the LEFT-RAIL slot,
  // replacing CodeLens's old in-pane 200px flat list. Pure fold over
  // `/api/codefiles` (see ../codeTree.ts for the tree-building/filter/sort
  // logic — kept out of this component so it's unit-testable without
  // mounting Svelte). State (files/activeKey/expanded/filter/sort) lives in
  // `codeState` (stores.svelte.ts), shared with CodeLens so clicking a file
  // here updates the code pane with no callback plumbing between them.
  import type { CodeFile } from '../types';
  import { codeState } from '../stores.svelte';
  import {
    activeFlatten,
    ancestorIdsFor,
    basename,
    buildTree,
    computeDisambiguators,
    countVisibleRows,
    fileKey,
    filterFiles,
    truncateMiddle,
    type TreeNode,
  } from '../codeTree';
  import FileIcon from './FileIcon.svelte';
  import Icon from './Icon.svelte';
  import Skeleton from './Skeleton.svelte';

  interface Props {
    hub: string;
    /** Fired on an explicit file activation (click, or Enter in the filter
     * list) — App.svelte doesn't strictly need this (activeKey already
     * lives in the shared store), but it's the natural hook for "close the
     * mobile drawer on selection", matching selectTopic's contract. */
    onActivate?: (file: CodeFile) => void;
    /** design/44 §6 item 2.4 — fired when the repo node's SELECT affordance
     * (distinct from its expand/collapse chevron) is clicked: the repo
     * itself becomes the view target (main pane -> rollup). */
    onActivateRepo?: (repo: string) => void;
  }

  let { hub, onActivate, onActivateRepo }: Props = $props();

  const s = $derived(codeState.forHub(hub));

  $effect(() => {
    const h = hub;
    void codeState.load(h);
  });

  const repos = $derived(buildTree(s.files));
  const trimmedFilter = $derived(s.filter.trim());
  const matches = $derived(trimmedFilter ? filterFiles(s.files, s.filter) : []);
  const activeList = $derived(s.sort === 'active' ? activeFlatten(s.files) : []);
  const disambiguators = $derived(
    computeDisambiguators(trimmedFilter ? matches.map((m) => m.file) : s.sort === 'active' ? activeList : [])
  );

  // design/43 §2.3 — lazy render already keeps visible rows in the low
  // hundreds (collapsed subtrees render nothing). This is the pre-decided
  // virtualization trigger: if it ever fires, swap the plain tree
  // `{#each}` below for a hand-rolled fixed-height windowed slice
  // (scrollTop -> index range -> padded spacers). No dependency, deferred
  // to Phase C until this is a real, observed problem.
  $effect(() => {
    if (trimmedFilter || s.sort === 'active') return; // those are already flat lists, not the tree
    const visible = countVisibleRows(repos, s.expanded);
    if (visible > 500) {
      console.debug(`confer serve: CodeTree visible rows (${visible}) exceeded the virtualization trigger (~500) — see codeTree.ts's countVisibleRows doc comment`);
    }
  });

  let filterInputEl = $state<HTMLInputElement | null>(null);
  let scrollEl = $state<HTMLDivElement | null>(null);
  let filterSelectedIndex = $state(0);

  $effect(() => {
    // Reset keyboard selection whenever the match set changes shape.
    void matches.length;
    filterSelectedIndex = 0;
  });

  function toggleExpand(id: string) {
    if (s.expanded.has(id)) s.expanded.delete(id);
    else s.expanded.add(id);
  }

  function activate(file: CodeFile) {
    s.activeKey = fileKey(file);
    s.viewMode = 'file';
    onActivate?.(file);
  }

  /** design/44 §6 item 2.4 — selects the repo itself as the Code view's
   * target (distinct from `toggleExpand`, which only opens/closes the tree
   * node). Fired by the repo row's separate "view rollup" affordance. */
  function selectRepo(repo: string) {
    s.activeRepo = repo;
    s.viewMode = 'repo';
    onActivateRepo?.(repo);
  }

  function scrollToNode(id: string) {
    const el = scrollEl?.querySelector<HTMLElement>(`[data-node-id="${CSS.escape(id)}"]`);
    // jsdom (unit tests) has no real layout and doesn't implement
    // scrollIntoView at all — guard so tests can drive reveal/expand
    // behavior without a full browser.
    el?.scrollIntoView?.({ block: 'nearest' });
  }

  // Auto-reveal: whenever the active file changes (a tree click, a filter
  // Enter, or a breadcrumb-click elsewhere), expand its ancestors and
  // scroll it into view — the reader should never have to manually
  // re-expand to see what's now active.
  $effect(() => {
    const key = s.activeKey;
    if (!key) return;
    for (const id of ancestorIdsFor(repos, key)) s.expanded.add(id);
    queueMicrotask(() => scrollToNode(key));
  });

  // design/43 §2.2 — a breadcrumb-segment click (App.svelte) sets
  // `pendingReveal` to a tree node id (file or compacted dir); reveal it
  // here and clear the flag. Selection-only, no routing implication yet.
  $effect(() => {
    const target = s.pendingReveal;
    if (!target) return;
    for (const id of ancestorIdsFor(repos, target)) s.expanded.add(id);
    queueMicrotask(() => scrollToNode(target));
    s.pendingReveal = null;
  });

  function onFilterKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      s.filter = '';
      return;
    }
    if (matches.length === 0) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      filterSelectedIndex = (filterSelectedIndex + 1) % matches.length;
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      filterSelectedIndex = (filterSelectedIndex - 1 + matches.length) % matches.length;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const hit = matches[filterSelectedIndex] ?? matches[0];
      if (hit) activate(hit.file);
    }
  }

  function onGlobalKeydown(e: KeyboardEvent) {
    if (e.key !== '/') return;
    const active = document.activeElement;
    const isEditable =
      active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement || (active as HTMLElement | null)?.isContentEditable;
    if (isEditable) return;
    e.preventDefault();
    filterInputEl?.focus();
  }

  $effect(() => {
    window.addEventListener('keydown', onGlobalKeydown);
    return () => window.removeEventListener('keydown', onGlobalKeydown);
  });

  function secondaryLabel(path: string): string {
    const dir = path.split('/').slice(0, -1).join('/');
    return dir ? truncateMiddle(dir, 34) : '';
  }

  function highlightParts(combined: string, start: number, end: number): { pre: string; hit: string; post: string } {
    return { pre: combined.slice(0, start), hit: combined.slice(start, end), post: combined.slice(end) };
  }
</script>

<div class="code-tree" data-testid="code-tree">
  <div class="ct-head">
    <div class="ct-search">
      <Icon name="search" size={13} class="ct-search-icon" />
      <input
        bind:this={filterInputEl}
        type="text"
        placeholder="Filter files… (/)"
        aria-label="Filter code files"
        value={s.filter}
        oninput={(e) => (s.filter = e.currentTarget.value)}
        onkeydown={onFilterKeydown}
      />
    </div>
    <div class="ct-toggle" role="group" aria-label="Tree or Active sort">
      <button type="button" class:active={s.sort === 'tree'} onclick={() => (s.sort = 'tree')}>Tree</button>
      <button type="button" class:active={s.sort === 'active'} onclick={() => (s.sort = 'active')}>Active</button>
    </div>
  </div>

  <div class="ct-scroll" bind:this={scrollEl}>
    {#if !s.loaded}
      <Skeleton rows={4} />
    {:else if s.files.length === 0}
      <div class="ct-empty">No code referenced yet</div>
    {:else if trimmedFilter}
      <!-- Findability escape hatch: flat, case-insensitive substring match
           over repo/path, match highlighted, repo as a dim prefix. -->
      <div class="ct-list" role="listbox" aria-label="Filter matches">
        {#each matches as m, i (m.key)}
          {@const combined = `${m.file.repo}/${m.file.path}`}
          {@const parts = highlightParts(combined, m.matchStart, m.matchEnd)}
          {@const dis = disambiguators.get(m.key)}
          <button
            type="button"
            class="row file flat"
            class:selected={i === filterSelectedIndex}
            class:unmapped={!m.file.mapped}
            class:active={m.key === s.activeKey}
            title={m.file.mapped ? combined : `${combined} — unmapped, no local clone to read this file from`}
            onclick={() => activate(m.file)}
            onmouseenter={() => (filterSelectedIndex = i)}
          >
            <FileIcon path={m.file.path} size={14} />
            <span class="label">
              <span class="dim">{parts.pre}</span><span class="hit">{parts.hit}</span><span class="dim">{parts.post}</span>
            </span>
            {#if dis}<span class="dis">· {dis}</span>{/if}
            <span class="badge">{m.file.refCount}</span>
          </button>
        {:else}
          <div class="ct-empty">No files match "{s.filter}"</div>
        {/each}
      </div>
    {:else if s.sort === 'active'}
      <!-- Same dataset as the tree, ranked by refCount desc then lastTs
           desc (the backend's own order) instead of hierarchy A-Z. -->
      <div class="ct-list">
        {#each activeList as file (fileKey(file))}
          {@const dis = disambiguators.get(fileKey(file))}
          {@const secondary = secondaryLabel(file.path)}
          <button
            type="button"
            class="row file flat"
            class:unmapped={!file.mapped}
            class:active={fileKey(file) === s.activeKey}
            title={file.mapped ? `${file.repo}/${file.path}` : `${file.repo}/${file.path} — unmapped`}
            onclick={() => activate(file)}
          >
            <FileIcon path={file.path} size={14} />
            <span class="label-stack">
              <span class="label">{basename(file.path)}</span>
              {#if secondary}<span class="secondary">{secondary}</span>{/if}
            </span>
            {#if dis}<span class="dis">· {dis}</span>{/if}
            <span class="badge">{file.refCount}</span>
          </button>
        {/each}
      </div>
    {:else}
      <!-- The real tree: repo -> compacted dirs -> files. -->
      <div class="ct-list">
        {#each repos as repoNode (repoNode.id)}
          {@render node(repoNode)}
        {/each}
      </div>
    {/if}
  </div>
</div>

{#snippet node(n: TreeNode)}
  {#if n.kind === 'repo'}
    <div class="row-wrap repo-wrap" data-node-id={n.id} class:selected={s.viewMode === 'repo' && s.activeRepo === n.repo}>
      <button
        type="button"
        class="row repo"
        aria-expanded={s.expanded.has(n.id)}
        onclick={() => toggleExpand(n.id)}
      >
        <Icon name="chevron-right" size={12} class={s.expanded.has(n.id) ? 'chev open' : 'chev'} />
        <span class="glyph">◆</span>
        <span class="label">{n.repo}</span>
        <span class="badge">Σ{n.refCount}</span>
      </button>
      <button
        type="button"
        class="repo-select"
        title="View every conversation referencing this repo"
        aria-label="View repo rollup"
        data-testid={`repo-select-${n.repo}`}
        onclick={() => selectRepo(n.repo)}
      >
        <Icon name="arrow-up-right" size={12} />
      </button>
    </div>
    {#if s.expanded.has(n.id)}
      {#each n.children as child (child.id)}
        {@render node(child)}
      {/each}
    {/if}
  {:else if n.kind === 'dir'}
    <button
      type="button"
      class="row dir"
      data-node-id={n.id}
      aria-expanded={s.expanded.has(n.id)}
      onclick={() => toggleExpand(n.id)}
    >
      {#each Array.from({ length: n.depth - 1 }) as _, i (i)}<span class="guide"></span>{/each}
      <Icon name="chevron-right" size={12} class={s.expanded.has(n.id) ? 'chev open' : 'chev'} />
      <Icon name={s.expanded.has(n.id) ? 'folder-open' : 'folder'} size={13} class="dir-icon" />
      <span class="label" title={n.fullPath}>{n.label}</span>
    </button>
    {#if s.expanded.has(n.id)}
      {#each n.children as child (child.id)}
        {@render node(child)}
      {/each}
    {/if}
  {:else}
    <button
      type="button"
      class="row file"
      class:active={n.id === s.activeKey}
      class:unmapped={!n.file.mapped}
      data-node-id={n.id}
      title={n.file.mapped ? `${n.repo}/${n.path}` : `${n.repo}/${n.path} — unmapped, no local clone to read this file from`}
      onclick={() => activate(n.file)}
    >
      {#each Array.from({ length: n.depth }) as _, i (i)}<span class="guide"></span>{/each}
      <FileIcon path={n.path} size={14} />
      <span class="label">{n.name}</span>
      <span class="badge">{n.file.refCount}</span>
    </button>
  {/if}
{/snippet}

<style>
  .code-tree {
    background: var(--panel);
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
  }
  .ct-head {
    flex: 0 0 auto;
    padding: 10px 8px 8px;
    border-bottom: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .ct-search {
    position: relative;
    display: flex;
    align-items: center;
  }
  .ct-search :global(.ct-search-icon) {
    position: absolute;
    left: 8px;
    color: var(--faint);
    pointer-events: none;
  }
  .ct-search input {
    width: 100%;
    background: var(--panel-2);
    border: 1px solid var(--border-2);
    border-radius: 7px;
    padding: 6px 8px 6px 27px;
    color: var(--text);
    font-size: 12.5px;
  }
  .ct-search input:focus {
    outline: 1.5px solid var(--accent);
    outline-offset: -1px;
  }
  .ct-toggle {
    display: flex;
    border: 1px solid var(--border-2);
    border-radius: 7px;
    overflow: hidden;
  }
  .ct-toggle button {
    flex: 1;
    border: 0;
    background: transparent;
    color: var(--muted);
    font: 600 10.5px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 6px 0;
  }
  .ct-toggle button.active {
    background: var(--panel-3);
    color: var(--accent);
  }
  .ct-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 6px;
    min-height: 0;
  }
  .ct-empty {
    padding: 16px 10px;
    color: var(--faint);
    font-size: 12px;
  }
  .ct-list {
    display: flex;
    flex-direction: column;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    border: 0;
    background: transparent;
    text-align: left;
    padding: 5px 7px;
    border-radius: 6px;
    color: var(--muted);
    font-size: 12.5px;
    border-left: 2px solid transparent;
  }
  .row:hover {
    background: var(--panel-2);
    color: var(--text);
  }
  .row.repo {
    font-weight: 650;
    color: var(--text);
    flex: 1;
    min-width: 0;
  }
  .row-wrap.repo-wrap {
    display: flex;
    align-items: center;
    gap: 2px;
    border-radius: 6px;
  }
  .row-wrap.repo-wrap.selected {
    background: var(--panel-3);
  }
  .row-wrap.repo-wrap.selected .row.repo {
    color: var(--accent);
  }
  .repo-select {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--faint);
    cursor: pointer;
  }
  .repo-select:hover {
    color: var(--accent);
    background: var(--panel-2);
  }
  .row .glyph {
    color: var(--faint);
    font-size: 11px;
  }
  .row .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row.file.flat .label {
    display: inline-flex;
    white-space: nowrap;
  }
  .row .label .dim {
    color: var(--faint);
  }
  .row .label .hit {
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 30%, transparent);
    border-radius: 2px;
  }
  .label-stack {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
  }
  .label-stack .label {
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .label-stack .secondary {
    color: var(--faint);
    font-size: 10.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dis {
    flex: 0 0 auto;
    color: var(--faint);
    font-size: 11px;
  }
  .badge {
    flex: 0 0 auto;
    font: 600 9.5px/1 var(--mono);
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    padding: 3px 5px;
    border-radius: 5px;
    margin-left: auto;
  }
  .guide {
    flex: 0 0 auto;
    width: 12px;
    align-self: stretch;
    position: relative;
  }
  .guide::before {
    content: '';
    position: absolute;
    left: 5px;
    top: -5px;
    bottom: -5px;
    width: 1px;
    background: var(--border);
  }
  :global(.chev) {
    flex: 0 0 auto;
    color: var(--faint);
    transition: transform 0.15s ease;
  }
  :global(.chev.open) {
    transform: rotate(90deg);
  }
  :global(.dir-icon) {
    flex: 0 0 auto;
    color: var(--muted);
  }
  /* Active file: same active language as `.topic.active` in LeftRail —
     panel-3 background + a 2px accent left edge. */
  .row.file.active {
    background: var(--panel-3);
    color: var(--text);
    border-left-color: var(--accent);
    font-weight: 600;
  }
  .row.file.flat.selected {
    background: var(--panel-2);
  }
  /* Unmapped — dim the whole row (icon included), not a color-only dot. */
  .row.file.unmapped {
    opacity: 0.45;
  }
</style>
