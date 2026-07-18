<script lang="ts">
  // The Code lens (Tier 3) — "conversations behind the code", browsable.
  // Ports `.code-wrap`/`.codetool`/`.filetree`/`.densitywrap`/`.densefile`/
  // `.dens` from design/serve-dashboard-v2-mockup.html: a file-tree stub,
  // Shiki-highlighted file view, a per-line conversation-density gutter
  // (heat via color-mix), and the "not cloned here" empty state for files
  // confer can't read from a local clone.
  //
  // The file tree hydrates from `/api/codefiles?hub=` — the distinct files
  // THIS hub's messages actually reference via `--ref` — instead of the
  // hardcoded fixture this component used to ship with (which rendered the
  // same fake wealdlore paths on every hub, regardless of what that hub
  // actually talked about).
  import type { CodeFile, RefHit } from '../types';
  import { api } from '../api';
  import { highlightSnippetLines, resolveLang, type HighlightedLine } from '../highlight';
  import EmptyState from './EmptyState.svelte';
  import Skeleton from './Skeleton.svelte';

  interface Props {
    hub: string;
    onOpenRefs?: (ctx: { repo: string; path: string; range: [number, number] | null }, hits: RefHit[]) => void;
  }

  let { hub, onOpenRefs }: Props = $props();

  function fileKey(f: { repo: string; path: string }): string {
    return `${f.repo} ${f.path}`;
  }

  /** The file tree shows just the basename (matches the prior fixture's
   * short `key` labels, and what the e2e specs click by accessible name) —
   * the full path is always visible in the codetool breadcrumb once a file
   * is selected. */
  function basename(path: string): string {
    return path.split('/').pop() || path;
  }

  let codeFiles = $state<CodeFile[]>([]);
  let filesLoading = $state(true);
  let activeKey = $state<string | null>(null);

  let loading = $state(false);
  let lines = $state<{ n: number; text: string }[]>([]);
  let lang = $state<string | null>(null);
  let highlighted = $state<HighlightedLine[]>([]);
  let hitsByLine = $state<Map<number, RefHit[]>>(new Map());

  const active = $derived(codeFiles.find((f) => fileKey(f) === activeKey) ?? null);

  // Group by repo, preserving the backend's own order (refCount desc, path
  // asc) within and across groups — files may span more than one repo.
  const groups = $derived.by((): { repo: string; files: CodeFile[] }[] => {
    const byRepo = new Map<string, CodeFile[]>();
    for (const f of codeFiles) {
      const list = byRepo.get(f.repo) ?? [];
      list.push(f);
      byRepo.set(f.repo, list);
    }
    return [...byRepo.entries()].map(([repo, files]) => ({ repo, files }));
  });

  async function loadFiles() {
    filesLoading = true;
    activeKey = null;
    codeFiles = [];
    try {
      const files = await api.getCodeFiles(hub);
      codeFiles = files;
      activeKey = files[0] ? fileKey(files[0]) : null;
    } catch (err) {
      console.error('confer serve: failed to load code files', hub, err);
    } finally {
      filesLoading = false;
    }
  }

  async function loadFile() {
    const f = active;
    if (!f || !f.mapped) {
      lines = [];
      hitsByLine = new Map();
      loading = false;
      return;
    }
    loading = true;
    try {
      const [snippet, hits] = await Promise.all([
        api.getCode(hub, f.repo, f.path, 'HEAD'),
        api.getRefs(hub, `${f.repo}:${f.path}`, true),
      ]);
      lines = snippet.lines;
      lang = snippet.lang;
      highlighted = lines.length ? await highlightSnippetLines(lines, resolveLang(lang)) : [];

      const relevant = hits.filter((h) => h.repo === f.repo && h.path === f.path);
      const byLine = new Map<number, RefHit[]>();
      for (const h of relevant) {
        if (!h.range) continue;
        for (let n = h.range[0]; n <= h.range[1]; n++) {
          const arr = byLine.get(n) ?? [];
          arr.push(h);
          byLine.set(n, arr);
        }
      }
      hitsByLine = byLine;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    // Reset selection and re-fetch the file tree whenever the hub changes —
    // a file from the PREVIOUS hub must not linger selected.
    void hub;
    void loadFiles();
  });

  $effect(() => {
    void active;
    void hub;
    void loadFile();
  });

  function highlightedFor(n: number): HighlightedLine | undefined {
    return highlighted.find((h) => h.n === n);
  }

  function heatStyle(n: number): string | undefined {
    const refs = hitsByLine.get(n)?.length ?? 0;
    if (!refs) return undefined;
    const heat = Math.min(refs * 10, 42);
    return `--heat:${heat}%`;
  }

  function clickLine(n: number) {
    if (!active) return;
    const hits = hitsByLine.get(n);
    if (!hits || hits.length === 0) return;
    onOpenRefs?.({ repo: active.repo, path: active.path, range: [n, n] }, hits);
  }
</script>

<div class="code-wrap" data-testid="code-view">
  {#if filesLoading}
    <div class="codetool"><span class="ct-hint">Loading files…</span></div>
    <div class="codepage">
      <div class="filetree"><Skeleton rows={4} /></div>
      <div class="densitywrap"><Skeleton rows={6} /></div>
    </div>
  {:else if codeFiles.length === 0}
    <div class="clonestub">
      <EmptyState
        glyph="◇"
        title="No code referenced in this hub yet"
        body="Messages here haven't pinned any `--ref`s — once someone posts a note or request that references a line range, it'll show up here, browsable with its conversation-density gutter."
      />
    </div>
  {:else if active}
    <div class="codetool">
      <span class="ct-crumb">◆ {active.repo}</span>
      <span class="ct-sep">›</span>
      <span class="ct-crumb">{active.path}</span>
      <span class="ct-hint">conversation-density gutter · click a hot line to see who discussed it</span>
    </div>
    <div class="codepage">
      <div class="filetree">
        {#each groups as group (group.repo)}
          <div class="ft-group">{group.repo}</div>
          {#each group.files as f (fileKey(f))}
            <button
              type="button"
              class="ftitem"
              class:active={fileKey(f) === activeKey}
              onclick={() => (activeKey = fileKey(f))}
            >
              <span class="fdot" style="background:{f.mapped ? 'var(--done)' : 'var(--faint)'}"></span>
              {basename(f.path)}
            </button>
          {/each}
        {/each}
      </div>
      <div class="densitywrap">
        {#if loading}
          <Skeleton rows={4} />
        {:else if !active.mapped}
          <div class="clonestub">
            <EmptyState
              glyph="◇"
              title="No clone mapped for this repo"
              body={`confer can only show code it can read from a local clone. Map a clone of ${active.repo} to see ${active.path} with its conversation-density gutter.`}
              actionLabel="＋ map a clone to see the code"
              disabled
            />
          </div>
        {:else if lines.length === 0}
          <div class="clonestub">
            <EmptyState glyph="◇" title="No code returned" body="confer's index has no content for this file at HEAD." />
          </div>
        {:else}
          <div class="densefile">
            <div class="code" data-lang={lang}>
              {#each lines as line (line.n)}
                {@const refCount = hitsByLine.get(line.n)?.length ?? 0}
                <div class="cl">
                  {#if refCount > 0}
                    <button
                      type="button"
                      class="dens hit"
                      style={heatStyle(line.n)}
                      title={`${refCount} conversation${refCount === 1 ? '' : 's'} reference this line`}
                      onclick={() => clickLine(line.n)}
                    >{refCount}</button>
                  {:else}
                    <span class="dens">·</span>
                  {/if}
                  <span class="ln">{line.n}</span>
                  <span class="cc">
                    {#each highlightedFor(line.n)?.tokens ?? [{ text: line.text, style: '' }] as tok, i (i)}
                      <span class="shiki-tok" style={tok.style}>{tok.text}</span>
                    {/each}
                  </span>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .code-wrap {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  .codetool {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    flex: 0 0 auto;
    font-size: 13px;
  }
  .ct-crumb {
    color: var(--text);
    font-weight: 600;
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .ct-sep {
    color: var(--faint);
  }
  .ct-hint {
    margin-left: auto;
    color: var(--faint);
    font-size: 11.5px;
  }
  .codepage {
    flex: 1;
    min-height: 0;
    display: flex;
    overflow: hidden;
  }
  .filetree {
    width: 200px;
    flex: 0 0 auto;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    padding: 12px 8px;
    background: var(--panel);
  }
  .ft-group {
    font: 700 9px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--faint);
    padding: 6px 8px 5px;
  }
  .ftitem {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    border: 0;
    background: transparent;
    text-align: left;
    padding: 6px 8px;
    border-radius: 7px;
    color: var(--muted);
    font-size: 12.5px;
  }
  .ftitem:hover {
    background: var(--panel-2);
    color: var(--text);
  }
  .ftitem.active {
    background: var(--panel-3);
    color: var(--text);
    font-weight: 600;
  }
  .ftitem .fdot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex: 0 0 auto;
  }
  .densitywrap {
    flex: 1;
    overflow: auto;
    padding: 14px 0 30px;
  }
  .clonestub {
    margin: 40px 20px 0;
  }
  .densefile .code {
    padding: 0;
    font: 500 12px/1.65 var(--mono);
  }
  .densefile .cl {
    display: flex;
    padding: 0 14px;
    white-space: pre;
  }
  .densefile .cl:hover {
    background: var(--panel-2);
  }
  .dens {
    width: 26px;
    flex: 0 0 auto;
    text-align: center;
    margin-right: 10px;
    border-radius: 4px;
    font: 700 9.5px/1 var(--mono);
    color: var(--faint);
    cursor: default;
    border: 0;
    background: transparent;
    padding: 0;
  }
  .dens.hit {
    cursor: pointer;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) var(--heat, 14%), transparent);
  }
  .dens.hit:hover {
    background: color-mix(in srgb, var(--accent) calc(var(--heat, 14%) + 12%), transparent);
    text-decoration: underline;
  }
  .ln {
    width: 24px;
    flex: 0 0 auto;
    text-align: right;
    margin-right: 14px;
    color: var(--faint);
    user-select: none;
  }
  .cc {
    color: var(--text);
  }

  /* On phone, stack the file tree above the code instead of side-by-side —
     200px off a ~360px viewport leaves the code view uncomfortably narrow.
     The file tree becomes a horizontally-scrolling strip of chips so it
     never forces the page to scroll sideways. */
  @media (max-width: 767.98px) {
    .codepage {
      flex-direction: column;
    }
    .filetree {
      width: 100%;
      max-height: none;
      display: flex;
      flex-direction: row;
      gap: 6px;
      overflow-x: auto;
      overflow-y: hidden;
      border-right: 0;
      border-bottom: 1px solid var(--border);
      padding: 8px;
    }
    .ft-group {
      display: none;
    }
    .ftitem {
      width: auto;
      flex: 0 0 auto;
      white-space: nowrap;
      min-height: 40px;
    }
  }
</style>
