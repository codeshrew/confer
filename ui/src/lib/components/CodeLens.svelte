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
  import type { Agent, RefHit } from '../types';
  import { api } from '../api';
  import { highlightSnippetLines, resolveLang, type HighlightedLine } from '../highlight';
  import { codeState } from '../stores.svelte';
  import { fileKey, groupRefHitsByFile } from '../codeTree';
  import { formatAge } from '../format';
  import { buildGutterEntries, entryColorVar, gutterColumnCount, type GutterEntry } from '../codeGutter';
  import { buildMinimapSegments, computeViewportIndicator, type ViewportIndicator } from '../codeMinimap';
  import {
    computeGaps,
    computeOpenSpans,
    PARTIAL_EXPAND_STEP,
    planRows,
    revealAll,
    revealFromBottom,
    revealFromTop,
    type RevealState,
    type Row,
  } from '../codeCollapse';
  import EmptyState from './EmptyState.svelte';
  import Skeleton from './Skeleton.svelte';
  import FileIcon from './FileIcon.svelte';

  interface Props {
    hub: string;
    /** Piece 11 Phase 2 — the current hub's fleet, for the gutter tab's
     * real initials (falls back to a generic id-based treatment when a
     * hit's author isn't in it, same honest-fallback convention the
     * anchored reader's own `agents` prop already established). */
    agents?: Agent[];
    onOpenRefs?: (ctx: { repo: string; path: string; range: [number, number] | null }, hits: RefHit[]) => void;
    /** Fired whenever the active file's FULL reference list (whole-file `range:null`
     * hits included) is (re)loaded — lets the host surface a file-level conversation
     * list in the right rail without requiring the reader to click a hot line first.
     * Distinct from `onOpenRefs` (a deliberate click) so it never yanks open the
     * mobile details drawer just because a file was selected. */
    onFileRefs?: (ctx: { repo: string; path: string }, hits: RefHit[]) => void;
    /** design/44 §6 item 2.4 — fired whenever the repo rollup's hit list
     * (re)loads, so the host can mirror it into the right rail's
     * `ReverseIndexPanel` repo-mode, same contract as `onFileRefs`. */
    onRepoRefs?: (repo: string, hits: RefHit[]) => void;
    /** Piece 11 Phase 2 (post-verify fix) — the scope currently open in the
     * Phase-1 anchored reader (App.svelte's own `refContext`, passed straight
     * through), so the gutter can show WHICH range is active — mock 11/12's
     * `.cl.r.act`/`.br.act` treatment. `null`/a different repo+path than the
     * active file means nothing here is active (the reader is showing a
     * DIFFERENT file, or nothing at all). */
    activeScope?: { repo: string; path: string | null; range: [number, number] | null } | null;
  }

  let { hub, agents = [], onOpenRefs, onFileRefs, onRepoRefs, activeScope = null }: Props = $props();

  const s = $derived(codeState.forHub(hub));

  $effect(() => {
    const h = hub;
    void codeState.load(h);
  });

  const active = $derived(s.files.find((f) => fileKey(f) === s.activeKey) ?? null);

  // design/44 §6 item 2.4 — repo rollup: the repo node in CodeTree was
  // selected as the view target instead of a single file.
  const repoTarget = $derived(s.viewMode === 'repo' ? s.activeRepo : null);
  let repoLoading = $state(false);
  let repoHits = $state<RefHit[]>([]);
  const repoGroups = $derived(groupRefHitsByFile(repoHits));

  async function loadRepoRollup(repo: string, hubId: string) {
    repoLoading = true;
    try {
      const hits = await api.getRefs(hubId, repo, true);
      repoHits = hits;
      onRepoRefs?.(repo, hits);
    } finally {
      repoLoading = false;
    }
  }

  $effect(() => {
    const repo = repoTarget;
    const h = hub;
    if (repo) void loadRepoRollup(repo, h);
  });

  /** A repo-rollup row was clicked — drill into that file, same as clicking
   * it directly in CodeTree (design/43's existing behavior, unchanged). */
  function selectFileFromRollup(repo: string, path: string) {
    s.activeKey = fileKey({ repo, path });
    s.viewMode = 'file';
  }

  let loading = $state(false);
  let lines = $state<{ n: number; text: string }[]>([]);
  let lang = $state<string | null>(null);
  let highlighted = $state<HighlightedLine[]>([]);
  // ALL of the active file's hits (whole-file `range:null` ones included) —
  // the file-level "conversations about this code" list. Piece 11 Phase 2's
  // gutter (`gutterEntries` below) and file-lane (`wholeFileHits`) are both
  // pure derivations off this ONE fetched list, not separate state to keep
  // in sync.
  let fileRefs = $state<RefHit[]>([]);

  // Piece 11 Phase 2 (11-code-view-BUILD-BRIEF.md) — the powered gutter:
  // "shape = scope" (file-lane / bracket / tick), "color = meaning" (real
  // per-kind palette, `codeGutter.ts`'s `hitColorVar` — shared with the
  // anchored reader, piece 9's own event palette underneath), "column =
  // overlap" (`buildGutterEntries`'s greedy interval coloring). Whole-file
  // hits (`range: null`) never touch the gutter — they live in the
  // file-lane above line 1 instead.
  const wholeFileHits = $derived(fileRefs.filter((h) => h.range === null));
  const gutterEntries = $derived(buildGutterEntries(fileRefs));
  const gutterColumns = $derived(gutterColumnCount(gutterEntries));

  // Piece 11 Phase 3 (11-code-collapse-RESEARCH.md, roll-our-own) — PR-style
  // collapse: referenced ranges + context stay open, everything else folds.
  // `showAll` is the rev-bar `referenced`/`show all` toggle; `reveal` holds
  // each gap's own partial-expand progress (`↑8`/`↓8`), keyed by the gap's
  // ORIGINAL bounds (`codeCollapse.ts`'s `gapKey`) so it survives re-renders
  // of the SAME file. Both reset on a file switch, in `loadFile` below.
  let showAll = $state(false);
  let reveal = $state(new Map<string, RevealState>());

  const firstLine = $derived(lines[0]?.n ?? null);
  const lastLine = $derived(lines[lines.length - 1]?.n ?? null);
  const openSpans = $derived.by(() => {
    if (firstLine === null || lastLine === null) return [];
    return computeOpenSpans(gutterEntries, firstLine, lastLine);
  });
  const collapseGaps = $derived.by(() => {
    if (firstLine === null || lastLine === null) return [];
    return computeGaps(openSpans, firstLine, lastLine);
  });
  const rowPlan = $derived.by((): Row[] => planRows(lines.map((l) => l.n), collapseGaps, reveal, showAll));
  const lineByN = $derived(new Map(lines.map((l) => [l.n, l])));

  /** Every fold-expand click goes through this — it keeps the viewport's
   * visual position stable across the DOM height change a reveal causes
   * (the research doc's own "scroll anchoring" gotcha). Anchors on whatever
   * REAL line is currently topmost in view (not the clicked fold row
   * itself — a full `⤢all`/edge reveal removes that row entirely), so it
   * works uniformly for a partial OR a full expand. */
  function withScrollAnchor(mutate: () => void) {
    const el = scrollEl;
    if (!el) {
      mutate();
      return;
    }
    const containerTop = el.getBoundingClientRect().top;
    let anchor: { line: string; offset: number } | null = null;
    for (const row of el.querySelectorAll<HTMLElement>('[data-line]')) {
      const r = row.getBoundingClientRect();
      if (r.bottom > containerTop) {
        anchor = { line: row.getAttribute('data-line')!, offset: r.top - containerTop };
        break;
      }
    }
    mutate();
    if (!anchor) return;
    const { line, offset } = anchor;
    requestAnimationFrame(() => {
      const row = el.querySelector<HTMLElement>(`[data-line="${line}"]`);
      if (!row) return;
      const newOffset = row.getBoundingClientRect().top - containerTop;
      el.scrollTop += newOffset - offset;
    });
  }

  function setReveal(key: string, next: RevealState) {
    const map = new Map(reveal);
    map.set(key, next);
    reveal = map;
  }

  function expandTop(row: Extract<Row, { kind: 'fold' }>) {
    withScrollAnchor(() => setReveal(row.key, revealFromTop(row.hiddenStart, row.hiddenEnd)));
  }
  function expandBottom(row: Extract<Row, { kind: 'fold' }>) {
    withScrollAnchor(() => setReveal(row.key, revealFromBottom(row.hiddenStart, row.hiddenEnd)));
  }
  function expandAllOf(row: Extract<Row, { kind: 'fold' }>) {
    withScrollAnchor(() => setReveal(row.key, revealAll(row.hiddenStart, row.hiddenEnd)));
  }
  function setShowAll(next: boolean) {
    if (next === showAll) return;
    withScrollAnchor(() => (showAll = next));
  }

  // Piece 11 Phase 2b — the conversation minimap: the SAME gutter entries,
  // compressed to a proportional strip. Built off the file's real first/last
  // line numbers (a snippet doesn't necessarily start at line 1), never off
  // whatever's currently SCROLLED into view — clusters in a Phase 3 fold
  // still show up here (it's built off the full hit set, same as
  // `collapseGaps` above, not off `rowPlan`'s visible subset). Now that
  // Phase 3 exists: this SAME data is what draws the fold rows too — one
  // source, two views. (`buildMinimapSegments` still honestly drops an
  // entry that doesn't intersect the LOADED file at all, e.g. a dangling
  // `offline` ref to lines that no longer exist here — same as the
  // per-line gutter already does.)
  const minimapSegments = $derived.by(() => {
    if (firstLine === null || lastLine === null) return [];
    return buildMinimapSegments(gutterEntries, firstLine, lastLine);
  });

  // The viewport indicator (mock 12's `.mmview`) — real scroll geometry, not
  // a decorative fixed band. `scrollEl`/`codeEl` are bound below; `undefined`
  // (not yet mounted, or a different view branch is showing) degrades to
  // "whole file in view" via `computeViewportIndicator`'s own 0-height guard.
  let scrollEl = $state<HTMLElement | undefined>();
  let codeEl = $state<HTMLElement | undefined>();
  let viewport = $state<ViewportIndicator>({ top: 0, height: 1 });

  function updateViewport() {
    viewport = computeViewportIndicator({
      containerScrollTop: scrollEl?.scrollTop ?? 0,
      containerClientHeight: scrollEl?.clientHeight ?? 0,
      codeOffsetTop: codeEl?.offsetTop ?? 0,
      codeScrollHeight: codeEl?.scrollHeight ?? 0,
    });
  }

  $effect(() => {
    // Re-run when a new file's lines land (the code element's scrollHeight
    // changes) as well as on real scroll/resize.
    void lines;
    const el = scrollEl;
    if (!el) return;
    updateViewport();
    el.addEventListener('scroll', updateViewport, { passive: true });
    window.addEventListener('resize', updateViewport);
    return () => {
      el.removeEventListener('scroll', updateViewport);
      window.removeEventListener('resize', updateViewport);
    };
  });

  /** Click-to-scroll (the brief's own bullet) — jump the code pane to an
   * entry's first line. Queries by the real `data-line` attribute each `.cl`
   * row carries, rather than keeping a second line->element map in sync. */
  function scrollToRange(range: [number, number]) {
    const target = scrollEl?.querySelector<HTMLElement>(`[data-line="${range[0]}"]`);
    target?.scrollIntoView({ block: 'center' });
  }

  // Piece 11 Phase 2 (post-verify fix) — is the Phase-1 reader currently
  // showing THIS file's scope at all? (A reader open on a different
  // file/repo means nothing here is active.)
  const isActiveFileScope = $derived(!!active && !!activeScope && activeScope.repo === active.repo && activeScope.path === active.path);
  const fileLaneActive = $derived(isActiveFileScope && activeScope!.range === null);
  /** The exact range (as a "start-end" key, matching `buildGutterEntries`'
   * own grouping key) currently open in the reader — `null` when the
   * reader isn't scoped to a specific range in this file (whole-file, a
   * different file, or closed). */
  const activeEntryKey = $derived(
    isActiveFileScope && activeScope!.range ? `${activeScope!.range[0]}-${activeScope!.range[1]}` : null
  );

  interface LineSegment {
    entry: GutterEntry;
    pos: 'start' | 'middle' | 'end' | 'tick';
  }
  /** Per line, one slot per gutter column — `null` where nothing crosses
   * that line in that column. Rebuilt whenever the entries change (a new
   * file, or its refs reloading), not per-render. */
  const lineColumns = $derived.by((): Map<number, (LineSegment | null)[]> => {
    const map = new Map<number, (LineSegment | null)[]>();
    for (const line of lines) map.set(line.n, new Array(gutterColumns).fill(null));
    for (const entry of gutterEntries) {
      for (let n = entry.range[0]; n <= entry.range[1]; n++) {
        const slots = map.get(n);
        if (!slots) continue;
        const pos: LineSegment['pos'] = entry.isTick ? 'tick' : n === entry.range[0] ? 'start' : n === entry.range[1] ? 'end' : 'middle';
        slots[entry.column] = { entry, pos };
      }
    }
    return map;
  });

  /** The clickable "range tab" renders once per entry, on its START
   * line — the click target into the Phase-1 anchored reader. More than
   * one entry can rarely start on the same line; render both rather than
   * silently dropping one. */
  const tabsByLine = $derived.by((): Map<number, GutterEntry[]> => {
    const map = new Map<number, GutterEntry[]>();
    for (const entry of gutterEntries) {
      const arr = map.get(entry.range[0]) ?? [];
      arr.push(entry);
      map.set(entry.range[0], arr);
    }
    return map;
  });

  function resolveAgent(id: string): Agent | undefined {
    return agents.find((a) => a.id === id);
  }

  function cap(s: string): string {
    return s.length ? s[0]!.toUpperCase() + s.slice(1) : s;
  }

  /** The tab's own label — "2 · JV HE" (mock 12) — real initials for every
   * DISTINCT author on the entry, falling back to a generic id-derived
   * treatment when an author isn't on this hub's roster (a real cross-hub
   * case, not an error). */
  function tabInitials(entry: GutterEntry): string {
    const seen = new Set<string>();
    const initials: string[] = [];
    for (const h of entry.hits) {
      if (seen.has(h.from)) continue;
      seen.add(h.from);
      initials.push(resolveAgent(h.from)?.abbr ?? h.from.slice(0, 2).toUpperCase());
    }
    return initials.join(' ');
  }

  /** Hover = peek (the brief's own bullet) — a native title tooltip naming
   * who and how many, real data, no custom hover UI needed for this. */
  function tabTitle(entry: GutterEntry): string {
    const count = entry.hits.length;
    const latest = [...entry.hits].sort((a, b) => (a.ts < b.ts ? 1 : a.ts > b.ts ? -1 : 0))[0]!;
    const who = [...new Set(entry.hits.map((h) => resolveAgent(h.from)?.display ?? cap(h.from)))].join(', ');
    return `${count} conversation${count === 1 ? '' : 's'} · ${who} · latest ${formatAge(latest.ts)}${entry.drift ? ' · drifted from the pinned lines' : ''}`;
  }

  function clickEntry(entry: GutterEntry) {
    if (!active) return;
    onOpenRefs?.({ repo: active.repo, path: active.path, range: entry.range }, entry.hits);
  }

  function clickFileLane() {
    if (!active || wholeFileHits.length === 0) return;
    onOpenRefs?.({ repo: active.repo, path: active.path, range: null }, wholeFileHits);
  }

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
    // Piece 11 Phase 3 — a new file starts fresh: default "referenced"
    // collapse, no carried-over partial reveals from whatever the operator
    // was just looking at (a gap key coinciding across two different files
    // by pure line-number chance would otherwise silently reuse the wrong
    // reveal state).
    showAll = false;
    reveal = new Map();
    const f = active;
    if (!f || !f.mapped) {
      lines = [];
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
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void active;
    void hub;
    // design/44 §6 item 2.4 — a repo rollup is selected (not a single file):
    // skip the per-file fetch entirely, even though `active` may still be
    // set (codeState auto-activates the first file on load regardless of
    // viewMode) — `loadRepoRollup` above is the only fetch that should run.
    if (s.viewMode === 'repo') return;
    void loadFile();
  });

  function highlightedFor(n: number): HighlightedLine | undefined {
    return highlighted.find((h) => h.n === n);
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
        body="Messages here haven't pinned any `--ref`s — once someone posts a note or request that references this repo — a file, a line range, or the repo itself — it'll show up here, browsable with its conversation-density gutter."
      />
    </div>
  {:else if repoTarget}
    <div class="codetool">
      <span class="ct-hint">repo rollup · every conversation referencing {repoTarget} · click a file to open it</span>
    </div>
    <div class="codepage">
      <div class="densitywrap">
        {#if repoLoading}
          <Skeleton rows={4} />
        {:else if repoGroups.length === 0}
          <div class="clonestub">
            <EmptyState
              glyph="◇"
              title="No conversations reference this repo yet"
              body={`Nobody has referenced anything in ${repoTarget} via --ref yet.`}
            />
          </div>
        {:else}
          <div class="repo-rollup" data-testid="repo-rollup">
            {#each repoGroups as g (g.path)}
              <button type="button" class="rollup-row" onclick={() => selectFileFromRollup(repoTarget, g.path)}>
                <FileIcon path={g.path} size={14} />
                <span class="rollup-path">{g.path}</span>
                <span class="rollup-count">{g.count} ref{g.count === 1 ? '' : 's'}</span>
                <span class="rollup-ts">{formatAge(g.lastTs)}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {:else if active}
    <div class="codetool">
      <span class="ct-hint">conversation gutter · shape=scope, color=meaning, column=overlap · click a range tab to read it</span>
      {#if collapseGaps.length > 0}
        <!-- Piece 11 Phase 3 — "referenced" (default) folds everything not
             near a conversation; "show all" renders the real whole file.
             Hidden entirely when there's nothing TO collapse (no gaps) —
             a toggle with no effect is just clutter. -->
        <span class="rtoggle" data-testid="collapse-toggle">
          <button type="button" class:on={!showAll} onclick={() => setShowAll(false)} data-testid="collapse-toggle-referenced">referenced</button>
          <button type="button" class:on={showAll} onclick={() => setShowAll(true)} data-testid="collapse-toggle-showall">show all</button>
        </span>
      {/if}
    </div>
    <div class="codepage">
      <div class="densitywrap" bind:this={scrollEl} onscroll={updateViewport}>
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
            {#if wholeFileHits.length > 0}
              <!-- Piece 11 Phase 2 — the file-lane: whole-file conversations
                   live HERE, never pinned to (or cluttering) any one line.
                   `act` (post-verify fix) when the Phase-1 reader is
                   currently showing this whole-file scope. -->
              <button type="button" class="filelane" class:act={fileLaneActive} onclick={clickFileLane} data-testid="file-lane">
                <span class="fl-ico">▤</span>
                <span class="fl-tx"
                  ><b>{wholeFileHits.length} conversation{wholeFileHits.length === 1 ? '' : 's'}</b> about the whole file</span
                >
              </button>
            {/if}
            <div class="code" data-lang={lang} bind:this={codeEl} style="--gutter-cols:{gutterColumns}">
              {#each rowPlan as row (row.kind === 'line' ? 'l' + row.n : 'f' + row.key)}
                {#if row.kind === 'fold'}
                  {@const hiddenCount = row.hiddenEnd - row.hiddenStart + 1}
                  <!-- Piece 11 Phase 3 — a real collapsed span, never lines
                       rendered-then-hidden: everything between row.hiddenStart
                       and row.hiddenEnd is simply absent from `rowPlan`. -->
                  <div class="fold" data-testid="fold-row" title={`lines ${row.hiddenStart}–${row.hiddenEnd}`}>
                    <span class="exp">⋯</span>
                    <span class="n">expand {hiddenCount} line{hiddenCount === 1 ? '' : 's'}</span>
                    <span class="updown">
                      {#if row.edge === 'top'}
                        <button type="button" onclick={() => expandAllOf(row)} data-testid="fold-expand-edge">↑ top</button>
                      {:else if row.edge === 'bottom'}
                        <button type="button" onclick={() => expandAllOf(row)} data-testid="fold-expand-edge">↓ bottom</button>
                      {:else}
                        <button type="button" onclick={() => expandTop(row)} data-testid="fold-expand-up"
                          >↑ {Math.min(PARTIAL_EXPAND_STEP, hiddenCount)}</button
                        >
                        <button type="button" onclick={() => expandBottom(row)} data-testid="fold-expand-down"
                          >↓ {Math.min(PARTIAL_EXPAND_STEP, hiddenCount)}</button
                        >
                        <button type="button" onclick={() => expandAllOf(row)} data-testid="fold-expand-all">⤢ all</button>
                      {/if}
                    </span>
                  </div>
                {:else}
                  {@const n = row.n}
                  {@const text = lineByN.get(n)?.text ?? ''}
                  {@const cols = lineColumns.get(n) ?? []}
                  {@const tabs = tabsByLine.get(n) ?? []}
                  {@const rowActive = cols.some((seg) => !!seg && activeEntryKey === `${seg.entry.range[0]}-${seg.entry.range[1]}`)}
                  <div class="cl" class:hot={cols.some((c) => c !== null)} class:act={rowActive} data-line={n}>
                    <span class="g">
                      {#each cols as seg, ci (ci)}
                        {@const segActive = !!seg && activeEntryKey === `${seg.entry.range[0]}-${seg.entry.range[1]}`}
                        <span class="gcol">
                          {#if seg}
                            {#if seg.pos === 'tick'}
                              <span class="tick" class:drift={seg.entry.drift} class:act={segActive} style="--bc:{entryColorVar(seg.entry)}"></span>
                            {:else}
                              <span
                                class="br"
                                class:s={seg.pos === 'start'}
                                class:e={seg.pos === 'end'}
                                class:drift={seg.entry.drift}
                                class:act={segActive}
                                style="--bc:{entryColorVar(seg.entry)}"
                              ></span>
                            {/if}
                          {/if}
                        </span>
                      {/each}
                    </span>
                    <span class="ln">{n}</span>
                    <span class="cc">
                      {#each highlightedFor(n)?.tokens ?? [{ text, style: '' }] as tok, i (i)}
                        <span class="shiki-tok" style={tok.style}>{tok.text}</span>
                      {/each}
                    </span>
                    {#if tabs.length > 0}
                      <span class="tabs-wrap">
                        {#each tabs as entry (entry.range[0] + '-' + entry.range[1])}
                          <button
                            type="button"
                            class="tab"
                            class:drift={entry.drift}
                            class:act={activeEntryKey === `${entry.range[0]}-${entry.range[1]}`}
                            style="--tc:{entryColorVar(entry)}"
                            title={tabTitle(entry)}
                            onclick={() => clickEntry(entry)}
                            data-testid="gutter-tab"
                          >{entry.drift ? '◷ ' : ''}{entry.hits.length} · {tabInitials(entry)}</button>
                        {/each}
                      </span>
                    {/if}
                  </div>
                {/if}
              {/each}
            </div>
          </div>
        {/if}
      </div>
      {#if minimapSegments.length > 0}
        <!-- Piece 11 Phase 2b — the conversation minimap: the whole file
             compressed, folded regions included (there's nothing to fold
             yet — Phase 3 — but this is already built off the FULL hit set,
             not the viewport, so it's correct the moment collapse lands).
             Law #3: omitted entirely when the file has no ranged
             conversations, same convention as the file-lane above. -->
        <div class="minimap" data-testid="code-minimap" title="conversation minimap — whole file">
          {#each minimapSegments as seg (seg.range[0] + '-' + seg.range[1])}
            <button
              type="button"
              class="mm"
              style="top:{seg.top * 100}%;height:{seg.height * 100}%;--bc:{seg.color}"
              title={`lines ${seg.range[0]}–${seg.range[1]}`}
              onclick={() => scrollToRange(seg.range)}
              data-testid="minimap-segment"
            ></button>
          {/each}
          <div class="mmview" style="top:{viewport.top * 100}%;height:{viewport.height * 100}%" data-testid="minimap-viewport"></div>
        </div>
      {/if}
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
  /* Piece 11 Phase 3 — the "referenced / show all" collapse toggle
     (mock 11's `.rtoggle`), pushed to the far right of the toolbar. */
  .rtoggle {
    margin-left: auto;
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 7px;
    overflow: hidden;
    font: 600 10.5px/1 var(--mono);
  }
  .rtoggle button {
    background: transparent;
    border: 0;
    color: var(--muted);
    padding: 4px 9px;
    cursor: pointer;
    font: inherit;
  }
  .rtoggle button.on {
    background: var(--panel-3);
    color: var(--text);
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
  .repo-rollup {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 20px;
  }
  .rollup-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    text-align: left;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel-2);
    padding: 9px 12px;
    color: var(--text);
    cursor: pointer;
    font-size: 12.5px;
  }
  .rollup-row:hover {
    border-color: var(--border-2);
    background: var(--panel-3);
  }
  .rollup-path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font: 600 12px/1 var(--mono);
  }
  .rollup-count {
    flex: 0 0 auto;
    font: 600 10px/1 var(--mono);
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    padding: 3px 6px;
    border-radius: 5px;
  }
  .rollup-ts {
    flex: 0 0 auto;
    font: 500 10.5px/1 var(--mono);
    color: var(--faint);
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
    align-items: stretch;
    padding: 0 14px;
    white-space: pre;
    position: relative;
  }
  .densefile .cl:hover,
  .densefile .cl.hot:hover {
    background: var(--panel-2);
  }
  .densefile .cl.hot {
    background: color-mix(in srgb, var(--accent) 4%, transparent);
  }
  /* Piece 11 Phase 2 (post-verify fix) — the range currently open in the
     Phase-1 reader (mock 11/12's `.cl.r.act`): a stronger tint than the
     plain "this line has a conversation" `.hot` state. */
  .densefile .cl.act {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .densefile .cl.act:hover {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }

  /* Piece 11 Phase 2 — the file-lane: whole-file conversations, never
     pinned to (or cluttering) any one line. */
  .filelane {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
    padding: 7px 20px;
    background: color-mix(in srgb, var(--state-metric) 9%, transparent);
    border: 0;
    border-bottom: 1px dashed color-mix(in srgb, var(--state-metric) 35%, transparent);
    cursor: pointer;
    font: inherit;
    color: inherit;
  }
  .filelane:hover {
    background: color-mix(in srgb, var(--state-metric) 15%, transparent);
  }
  .filelane.act {
    background: color-mix(in srgb, var(--state-metric) 22%, transparent);
    box-shadow: inset 3px 0 0 var(--state-metric);
  }
  .filelane .fl-ico {
    font: 700 11px/1 var(--mono);
    color: var(--state-metric);
  }
  .filelane .fl-tx {
    font-size: 11.5px;
    color: var(--muted);
  }
  .filelane .fl-tx b {
    color: var(--text);
  }

  /* The gutter itself: `--gutter-cols` columns wide (piece 11 Phase 2's
     "column = overlap" — set once per file on `.code`, read by every row
     so a single-column file doesn't reserve space for overlap it doesn't
     have, and a deeply-overlapping one gets exactly the width it needs). */
  .g {
    flex: 0 0 auto;
    display: flex;
    width: calc(var(--gutter-cols, 1) * 13px + 6px);
    margin-right: 10px;
  }
  .gcol {
    width: 13px;
    flex: 0 0 auto;
    position: relative;
  }
  /* A range bracket — "shape = scope": a contiguous band spans exactly the
     lines the conversation is pinned to. `--bc` (bracket color) is set
     per-segment from `codeGutter.ts`'s `hitColorVar` — "color = meaning". */
  .br {
    position: absolute;
    left: 3px;
    right: 2px;
    top: 0;
    bottom: 0;
  }
  .br.s {
    top: 3px;
    border-top-left-radius: 3px;
    border-top-right-radius: 3px;
  }
  .br.e {
    bottom: 3px;
    border-bottom-left-radius: 3px;
    border-bottom-right-radius: 3px;
  }
  .br::before {
    content: '';
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--bc) 20%, transparent);
  }
  .br::after {
    content: '';
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 2.5px;
    background: var(--bc);
  }
  /* Drift — law #3: dashed only when a REAL hit in this entry has
     genuinely drifted (`staleness === 'changed'`), never decorative. */
  .br.drift::after {
    background: repeating-linear-gradient(0deg, var(--bc) 0 4px, transparent 4px 7px);
  }
  /* The range currently open in the reader (mock's `.br.act`) — a
     stronger fill, same as `.cl.act` above. */
  .br.act::before {
    background: color-mix(in srgb, var(--bc) 40%, transparent);
  }
  /* A single-line hit — "shape = scope": a tick, not a bracket. */
  .tick {
    position: absolute;
    left: 3px;
    width: 7px;
    height: 7px;
    border-radius: 2px;
    top: 50%;
    transform: translateY(-50%);
    background: var(--bc);
  }
  .tick.act {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--bc) 50%, transparent);
  }
  /* A tick's own drift treatment — a bracket has an edge to dash, a tick
     is a single point, so drift reads as a dashed OUTLINE ring instead of
     a filled square (same law-#3 "real mismatch only" rule as `.br.drift`). */
  .tick.drift {
    background: transparent;
    border: 1.5px dashed var(--bc);
    box-sizing: border-box;
  }
  /* The range tab — the click target into the Phase-1 anchored reader.
     Sits on the entry's START line; more than one rarely starting on the
     same line stack via flex `gap` rather than overlapping. */
  .tabs-wrap {
    position: absolute;
    right: 14px;
    top: -1px;
    display: flex;
    gap: 3px;
    z-index: 2;
  }
  .tab {
    font: 700 9px/1 var(--mono);
    color: var(--tc, var(--accent));
    background: var(--panel);
    border: 1px solid color-mix(in srgb, var(--tc, var(--accent)) 42%, transparent);
    border-radius: 0 0 5px 5px;
    padding: 1px 5px;
    cursor: pointer;
  }
  .tab:hover {
    background: color-mix(in srgb, var(--tc, var(--accent)) 16%, var(--panel));
  }
  .tab.drift {
    border-style: dashed;
  }
  .tab.act {
    background: color-mix(in srgb, var(--tc, var(--accent)) 28%, var(--panel));
    color: var(--text);
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

  /* Piece 11 Phase 3 — a collapsed-span row (mock 11's `.fold`): the whole
     row is chrome, not code, so it doesn't carry `data-line` and sits
     outside the gutter-column grid entirely (its own full-width flex row). */
  .fold {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 14px;
    margin: 2px 0;
    color: var(--muted);
    cursor: default;
    background: color-mix(in srgb, var(--accent) 6%, transparent);
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    font: 500 10.5px/1 var(--mono);
  }
  .fold .exp {
    color: var(--accent);
  }
  .fold .updown {
    margin-left: auto;
    display: flex;
    gap: 10px;
  }
  .fold .updown button {
    background: transparent;
    border: 0;
    padding: 0;
    color: var(--accent);
    cursor: pointer;
    font: inherit;
  }
  .fold .updown button:hover {
    text-decoration: underline;
  }

  /* Piece 11 Phase 2b — the conversation minimap: a thin strip on the far
     edge of the code pane, `.codepage`'s second flex child (`.densitywrap`
     stays the scrollable one). Reuses `entryColorVar`'s SAME state palette
     as the gutter — one meaning, one hue, everywhere it shows up. */
  .minimap {
    flex: 0 0 auto;
    width: 10px;
    position: relative;
    background: var(--panel-2);
    border-left: 1px solid var(--border);
  }
  .mm {
    position: absolute;
    left: 2px;
    right: 2px;
    border: 0;
    border-radius: 2px;
    padding: 0;
    cursor: pointer;
    background: var(--bc);
  }
  .mm:hover {
    outline: 1.5px solid color-mix(in srgb, var(--bc) 60%, transparent);
    outline-offset: 0.5px;
  }
  /* The viewport indicator (mock 12's `.mmview`) — real scroll geometry,
     not decorative; see `computeViewportIndicator` in `codeMinimap.ts`. */
  .mmview {
    position: absolute;
    left: -1px;
    right: -1px;
    border: 1.5px solid var(--faint);
    border-radius: 3px;
    background: color-mix(in srgb, var(--text) 8%, transparent);
    pointer-events: none;
  }
</style>
