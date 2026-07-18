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
  import FileIcon from './FileIcon.svelte';
  import Icon from './Icon.svelte';

  interface Props {
    hub: string;
    onOpenRefs?: (ctx: { repo: string; path: string; range: [number, number] | null }, hits: RefHit[]) => void;
    /** Fired whenever the active file's FULL reference list (whole-file `range:null`
     * hits included) is (re)loaded — lets the host surface a file-level conversation
     * list in the right rail without requiring the reader to click a hot line first.
     * Distinct from `onOpenRefs` (a deliberate click) so it never yanks open the
     * mobile details drawer just because a file was selected. */
    onFileRefs?: (ctx: { repo: string; path: string }, hits: RefHit[]) => void;
    /** Fired whenever "is there an active file at all" changes — App.svelte's
     * per-view right-rail visibility (design/43 Thread 1) needs this: the Code
     * view's rail is open whenever a file is active, hidden on a zero-files hub. */
    onActiveFileChange?: (hasActive: boolean) => void;
  }

  let { hub, onOpenRefs, onFileRefs, onActiveFileChange }: Props = $props();

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
  // ALL of the active file's hits (whole-file `range:null` ones included) —
  // the file-level "conversations about this code" list, distinct from
  // `hitsByLine` which only carries the ranged subset for the gutter.
  let fileRefs = $state<RefHit[]>([]);
  // The sha actually rendered — the newest hit's pinned sha, or 'HEAD' when
  // there are no hits (or the hit itself is an unpinned/legacy 'HEAD' ref).
  let codeSha = $state('HEAD');

  const active = $derived(codeFiles.find((f) => fileKey(f) === activeKey) ?? null);

  /** Most-recently-posted hit for the active file (ISO ts, string-sortable) —
   * drives both the sha `getCode` renders at and the empty-state copy. */
  const newestHit = $derived.by((): RefHit | null => {
    if (fileRefs.length === 0) return null;
    return [...fileRefs].sort((a, b) => (a.ts < b.ts ? 1 : a.ts > b.ts ? -1 : 0))[0]!;
  });

  const emptyCodeMessage = $derived.by((): { title: string; body: string } => {
    const hit = newestHit;
    const repo = active?.repo ?? hit?.repo ?? 'this repo';
    if (!hit || hit.sha === 'HEAD') {
      return {
        title: 'No code returned',
        body: `confer's index has no content for this file at HEAD — it may have been deleted, or never committed to ${repo}.`,
      };
    }
    const shortSha = hit.sha.slice(0, 10);
    const span = hit.range ? `lines ${hit.range[0]}–${hit.range[1]}` : 'whole file';
    if (hit.staleness === 'moved') {
      return {
        title: 'Referenced path not found at that revision',
        body: `Referenced at \`${shortSha}\` (${span}) — the path was moved or renamed since; it isn't at that path in ${repo} at \`${shortSha}\`.`,
      };
    }
    return {
      title: "Referenced revision isn't in your clone",
      body: `Referenced at \`${shortSha}\` (${span}) — that revision isn't in your local clone of ${repo}.`,
    };
  });

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

  // design/43 quick win: the `mod.rs` / `index.ts` problem. Basenames repeat
  // across repos (and sometimes within one, across directories) in a flat
  // list — count occurrences across ALL visible rows so a colliding pair
  // anywhere gets a distinguishing parent-dir suffix, not just within a
  // single repo group.
  const basenameCounts = $derived.by((): Map<string, number> => {
    const counts = new Map<string, number>();
    for (const f of codeFiles) {
      const b = basename(f.path);
      counts.set(b, (counts.get(b) ?? 0) + 1);
    }
    return counts;
  });

  /** The immediate parent directory, `like/this/` — null when the file sits
   * at its repo's root (nothing to disambiguate with). */
  function parentDir(path: string): string | null {
    const parts = path.split('/');
    parts.pop();
    const dir = parts.pop();
    return dir ? `${dir}/` : null;
  }

  function disambiguator(f: CodeFile): string | null {
    if ((basenameCounts.get(basename(f.path)) ?? 0) <= 1) return null;
    return parentDir(f.path);
  }

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

  function pickSha(hits: RefHit[]): string {
    if (hits.length === 0) return 'HEAD';
    return [...hits].sort((a, b) => (a.ts < b.ts ? 1 : a.ts > b.ts ? -1 : 0))[0]!.sha;
  }

  async function loadFile() {
    const f = active;
    if (!f || !f.mapped) {
      lines = [];
      hitsByLine = new Map();
      fileRefs = [];
      codeSha = 'HEAD';
      loading = false;
      return;
    }
    loading = true;
    try {
      // Refs first — the sha `getCode` renders at (the newest hit's pinned
      // sha, not a hardcoded 'HEAD') depends on knowing them.
      const hits = await api.getRefs(hub, `${f.repo}:${f.path}`, true);
      const relevant = hits.filter((h) => h.repo === f.repo && h.path === f.path);
      fileRefs = relevant;
      onFileRefs?.({ repo: f.repo, path: f.path }, relevant);
      const sha = pickSha(relevant);
      codeSha = sha;

      const snippet = await api.getCode(hub, f.repo, f.path, sha);
      lines = snippet.lines;
      lang = snippet.lang;
      highlighted = lines.length ? await highlightSnippetLines(lines, resolveLang(lang)) : [];

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

  $effect(() => {
    onActiveFileChange?.(activeKey !== null);
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
      {#if codeSha !== 'HEAD'}
        <span class="ct-sha" title={`Rendered at the newest reference's pinned sha`}>@{codeSha.slice(0, 10)}</span>
      {/if}
      <span class="ct-hint">conversation-density gutter · click a hot line to see who discussed it</span>
    </div>
    <div class="codepage">
      <div class="filetree">
        {#each groups as group (group.repo)}
          <div class="ft-group"><Icon name="folder" size={12} />{group.repo}</div>
          {#each group.files as f (fileKey(f))}
            {@const dis = disambiguator(f)}
            <button
              type="button"
              class="ftitem"
              class:active={fileKey(f) === activeKey}
              class:dim={!f.mapped}
              title={f.mapped ? undefined : 'unmapped — no local clone to read this file from'}
              onclick={() => (activeKey = fileKey(f))}
            >
              <FileIcon path={f.path} size={14} />
              <span class="ftname">{basename(f.path)}</span>
              {#if dis}<span class="ftdis">· {dis}</span>{/if}
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
            <EmptyState glyph="◇" title={emptyCodeMessage.title} body={emptyCodeMessage.body} />
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
  .ct-sha {
    font: 600 10.5px/1 var(--mono);
    color: var(--muted);
    background: var(--panel-3);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 6px;
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
    display: flex;
    align-items: center;
    gap: 6px;
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
  /* Unmapped (no local clone to read from) — dim the WHOLE row (icon
     included), not just a color-coded dot. Color-only state fails both
     scanning and accessibility; a `title` tooltip on the row carries the
     "unmapped" fact for anyone who wants it spelled out. */
  .ftitem.dim {
    opacity: 0.45;
  }
  .ftname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Basename-collision disambiguator (`mod.rs · fleet/`) — the parent dir,
     dim-styled, appended only for rows whose basename isn't unique among
     the currently-visible files. */
  .ftdis {
    flex: 0 0 auto;
    color: var(--faint);
    font-size: 11px;
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
