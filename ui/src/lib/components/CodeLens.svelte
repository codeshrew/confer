<script lang="ts">
  // The Code lens (Tier 3) — "conversations behind the code", browsable.
  // design/43 Phase B: the in-pane file list moved to CodeTree.svelte (the
  // Code view's left-rail navigator) — this component is now just the
  // Shiki-highlighted file view + the per-line conversation-density gutter
  // (heat via color-mix) + the "not cloned here" empty states. It reads
  // `files`/`activeKey` from the SAME shared `codeState` store CodeTree
  // writes to (stores.svelte.ts), rather than owning its own local file
  // list — clicking a file in the tree updates this pane with no prop or
  // callback plumbing between the two components.
  import type { RefHit } from '../types';
  import { api } from '../api';
  import { highlightSnippetLines, resolveLang, type HighlightedLine } from '../highlight';
  import { codeState } from '../stores.svelte';
  import { fileKey } from '../codeTree';
  import EmptyState from './EmptyState.svelte';
  import Skeleton from './Skeleton.svelte';

  interface Props {
    hub: string;
    onOpenRefs?: (ctx: { repo: string; path: string; range: [number, number] | null }, hits: RefHit[]) => void;
    /** Fired whenever the active file's FULL reference list (whole-file `range:null`
     * hits included) is (re)loaded — lets the host surface a file-level conversation
     * list in the right rail without requiring the reader to click a hot line first.
     * Distinct from `onOpenRefs` (a deliberate click) so it never yanks open the
     * mobile details drawer just because a file was selected. */
    onFileRefs?: (ctx: { repo: string; path: string }, hits: RefHit[]) => void;
  }

  let { hub, onOpenRefs, onFileRefs }: Props = $props();

  const s = $derived(codeState.forHub(hub));

  $effect(() => {
    const h = hub;
    void codeState.load(h);
  });

  const active = $derived(s.files.find((f) => fileKey(f) === s.activeKey) ?? null);

  let loading = $state(false);
  let lines = $state<{ n: number; text: string }[]>([]);
  let lang = $state<string | null>(null);
  let highlighted = $state<HighlightedLine[]>([]);
  let hitsByLine = $state<Map<number, RefHit[]>>(new Map());
  // ALL of the active file's hits (whole-file `range:null` ones included) —
  // the file-level "conversations about this code" list, distinct from
  // `hitsByLine` which only carries the ranged subset for the gutter.
  let fileRefs = $state<RefHit[]>([]);

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
      s.codeSha = 'HEAD';
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
      s.codeSha = sha;

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
  {#if !s.loaded}
    <div class="codepage">
      <div class="densitywrap"><Skeleton rows={6} /></div>
    </div>
  {:else if s.files.length === 0}
    <div class="clonestub">
      <EmptyState
        glyph="◇"
        title="No code referenced in this hub yet"
        body="Messages here haven't pinned any `--ref`s — once someone posts a note or request that references a line range, it'll show up here, browsable with its conversation-density gutter."
      />
    </div>
  {:else if active}
    <div class="codetool">
      <span class="ct-hint">conversation-density gutter · click a hot line to see who discussed it</span>
    </div>
    <div class="codepage">
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
    padding: 10px 20px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    flex: 0 0 auto;
    font-size: 13px;
  }
  .ct-hint {
    color: var(--faint);
    font-size: 11.5px;
  }
  .codepage {
    flex: 1;
    min-height: 0;
    display: flex;
    overflow: hidden;
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
</style>
