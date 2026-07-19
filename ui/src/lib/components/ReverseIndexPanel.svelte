<script lang="ts">
  // "Conversations about this code" — ports `.cvx`/`.cvitem`/`.spanline` from
  // design/serve-dashboard-v2-mockup.html. Walks the `--ref` index backwards:
  // given a repo, a file, or a line range, list every thread that discussed
  // it, across hubs, public *and* private (private hubs get a distinct
  // badge). design/44 §6 item 2.4 — when `repo` is set and `path` is null,
  // this renders REPO-MODE instead: `hits` (a whole-repo getRefs(hub, repo)
  // fetch) grouped by file, each row drilling into that file.
  import type { Agent, RefHit } from '../types';
  import { formatAge, formatIsoDate } from '../format';
  import { groupRefHitsByFile } from '../codeTree';
  import { paneFocus } from '../paneFocus.svelte';
  import { hitColorVar } from '../codeGutter';
  import { isTypingTarget } from '../keys';
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
    /** Piece 11 Phase 1 (11-code-view-BUILD-BRIEF.md) — when true, this
     * panel is Code view's persistent ANCHORED READER: one conversation is
     * always focused and shown as a full card, the rest collapse into a
     * "‹ N more" strip, `j`/`k` moves focus — and clicking a row/pill only
     * ever changes FOCUS, never navigates away. `onOpenThread` (the
     * expanded card's own explicit link) is the ONLY thing that leaves
     * Code. `false` (default) keeps the EXISTING behavior Chat's own
     * inline-ref-chip lookup relies on: a plain flat row list, click a row
     * -> `onSelectHit` jumps straight there. */
    anchored?: boolean;
    /** Real agent color/display for the anchored reader's avatar+name — the
     * CURRENT hub's fleet, same list every other agent lookup in the app
     * already threads through. Falls back to a generic initials/id
     * treatment (matching the existing row form's own `cap(hit.from)`) when
     * the hit's author isn't in it — a cross-hub hit whose author isn't on
     * THIS hub's roster is a real, honest case, not an error. */
    agents?: Agent[];
    /** Anchored mode's ONLY navigate-away action — Code view's "stop
     * jumping to Chat" fix (piece 11 Phase 1). Wired to the SAME
     * `openHitInChat` Chat's own ref-chip flow already uses; it's just no
     * longer fired by a bare row click when anchored. */
    onOpenThread?: (hit: RefHit) => void;
    /** Piece 11 Phase 5 (11-code-view-BUILD-BRIEF.md) — the sha the code
     * pane is CURRENTLY rendering (`codeState.codeSha`, App.svelte's own
     * `codeCrumb.sha`/Phase 4's rev chip source) — drives the timeline's
     * per-node `this`/`older` state. `null` outside Code view, where the
     * timeline never renders anyway. */
    viewedSha?: string | null;
    /** Piece 11 Phase 5 — "↳ align code to this version": re-pins the code
     * pane to an OLDER node's exact sha. Opt-in only — never fired by
     * focusing/reading a node, only this explicit action. */
    onAlignToRevision?: (sha: string) => void;
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
    anchored = false,
    agents = [],
    onOpenThread,
    viewedSha = null,
    onAlignToRevision,
  }: Props = $props();

  const hubCount = $derived(new Set(hits.map((h) => h.hub)).size);
  const rangeLabel = $derived(range ? `L${range[0]}–${range[1]}` : '');
  /** design/44 §6 item 2.4 — repo scope: a repo without a path. */
  const repoMode = $derived(!!repo && !path);
  const fileGroups = $derived(repoMode ? groupRefHitsByFile(hits) : []);

  // Piece 11 Phase 5 — the sidebar conversation timeline (the range
  // biography MVP): the SAME scope's hits, laid oldest→newest, each pinned
  // to a real version. Law #3 — real `ts`, real `sha`; never a fabricated
  // node. Keeps the hit's ORIGINAL index into `hits` (not the sorted
  // position) so clicking a node can reuse `focusHit` below unchanged.
  const timelineNodes = $derived.by(() =>
    hits
      .map((hit, originalIndex) => ({ hit, originalIndex }))
      .sort((a, b) => (a.hit.ts < b.hit.ts ? -1 : a.hit.ts > b.hit.ts ? 1 : 0))
  );
  const timelineVersionCount = $derived(new Set(hits.map((h) => h.sha)).size);

  function cap(s: string): string {
    return s.length ? s[0]!.toUpperCase() + s.slice(1) : s;
  }

  function resolveAgent(id: string): Agent | undefined {
    return agents.find((a) => a.id === id);
  }

  // Piece 11 Phase 1 — the anchored reader's own focus: which hit is
  // currently shown EXPANDED (the rest render as compact rows). Reset to
  // the FIRST hit whenever the SCOPE changes (a different repo/path/range
  // selected) — never a stale index left over from whatever scope was
  // showing before. Keyed on the scope, not `hits` itself, so an unrelated
  // re-render that happens to pass a new-but-equivalent `hits` array
  // doesn't blow away focus the reader is mid-read on.
  let focusedIdx = $state(0);
  $effect(() => {
    void repo;
    void path;
    void range;
    focusedIdx = 0;
    showAllRows = false;
  });
  $effect(() => {
    if (focusedIdx >= hits.length) focusedIdx = Math.max(0, hits.length - 1);
  });

  // Jarvis's live-verify catch on Phase 1's own first cut: conversations
  // shouldn't hide behind a bare count — the pane's whole point is to SCAN
  // and flow through a range's discussion. All hits render as visible,
  // scannable ROWS (accordion: the focused one expands, collapses the
  // prior); only genuinely many (6+) fold the tail behind a "‹ N older"
  // reveal, matching the mock's own "few conversations, all shown" case
  // exactly while still keeping a huge hit list from blowing out the pane.
  const VISIBLE_ROW_CAP = 5;
  let showAllRows = $state(false);
  $effect(() => {
    // If focus ever lands beyond the initial cap (j/k stepping past it),
    // the visible window must include it — reveal rather than hide the
    // very thing that's focused.
    if (focusedIdx >= VISIBLE_ROW_CAP) showAllRows = true;
  });
  const visibleCount = $derived(showAllRows || hits.length <= VISIBLE_ROW_CAP ? hits.length : VISIBLE_ROW_CAP);
  const olderCount = $derived(hits.length - visibleCount);

  // A row click UNMOUNTS itself (the hit it named becomes the expanded
  // card, so it's no longer rendered as a row) — the browser silently
  // drops keyboard focus to <body> when its previously-focused element is
  // removed from the DOM. Without re-focusing the panel root here, a
  // SUBSEQUENT j/k press would never bubble back into
  // `handleAnchoredKeydown` (body isn't a descendant of `.cvx`, so nothing
  // reaches it). j/k's own moves don't need this — the panel root is
  // already what's focused when a keydown fires on it in the first place.
  function focusHit(i: number) {
    focusedIdx = i;
    cvxEl?.focus();
  }

  function handleAnchoredKeydown(e: KeyboardEvent) {
    if (!anchored || hits.length === 0) return;
    if (isTypingTarget(e.target)) return;
    if (e.key === 'j' || e.key === 'ArrowDown') {
      e.preventDefault();
      focusedIdx = Math.min(focusedIdx + 1, hits.length - 1);
    } else if (e.key === 'k' || e.key === 'ArrowUp') {
      e.preventDefault();
      focusedIdx = Math.max(focusedIdx - 1, 0);
    }
  }

  // keyboard-architecture pass — "refs", one of the 7 named Layer-1 panes.
  // Rows here are already all real, individually-focusable buttons
  // (Tab-reachable, click/Enter-activatable) — no pre-existing bare-key
  // vocab to retrofit, so this just registers the panel root as the
  // Ctrl+hjkl landing spot; native Tab then reaches every row same as ever.
  // Anchored mode ADDS the j/k bare-key vocab above, scoped to this pane
  // like every other pane's own bare keys (ChatStream, MetaThread, ...).
  let cvxEl: HTMLDivElement;
  $effect(() => {
    if (!cvxEl) return;
    return paneFocus.register({
      id: 'refs',
      label: 'Conversations',
      el: cvxEl,
      getRect: () => cvxEl.getBoundingClientRect(),
    });
  });
</script>

<div class="cvx" role="toolbar" aria-orientation="vertical" aria-label="Conversations about this code" tabindex="-1" bind:this={cvxEl} onkeydown={handleAnchoredKeydown}>
  <p class="ctx-note">
    Every <code class="mono">--ref</code> is indexed, so you can walk it backwards: given a repo, a file, or a line range,
    find every thread that discussed it — across hubs, public <b>and</b> private.
  </p>

  {#if anchored && !repoMode && (repo || path)}
    <!-- Piece 11 Phase 1 — the scope header locked to the selection: ▤
         whole file vs ▐ a specific range, so it's never ambiguous what
         the reader below is anchored to. -->
    <div class="ascope" data-testid="anchor-scope">
      <span class="aglyph">{range ? '▐' : '▤'}</span>
      <span class="alabel">{range ? `${path?.split('/').pop()}:${rangeLabel}` : path?.split('/').pop()}</span>
    </div>
  {/if}

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

  {#if anchored && !repoMode && hits.length > 0}
    <!-- Piece 11 Phase 5 — the sidebar timeline: this scope's real
         conversations, oldest→newest, each pinned to a real version. The
         range-biography (mock 13) MVP this later expands from. Reading/
         focusing a node is just that — focusing; only the explicit
         "↳ align" action on an OLDER node ever moves the code. -->
    <div class="timeline" data-testid="conversation-timeline">
      <div class="tl-h">
        this {range ? 'range' : 'file'}, over time — {hits.length} conversation{hits.length === 1 ? '' : 's'} across {timelineVersionCount}
        version{timelineVersionCount === 1 ? '' : 's'}
      </div>
      <div class="spine">
        {#each timelineNodes as { hit, originalIndex } (hit.msgId)}
          {@const agent = resolveAgent(hit.from)}
          {@const isCurrent = viewedSha !== null && hit.sha === viewedSha}
          <div class="node" class:cur={isCurrent} class:old={!isCurrent} data-testid="timeline-node">
            <span class="dot"></span>
            <button type="button" class="node-main" onclick={() => focusHit(originalIndex)} data-testid="timeline-node-focus">
              <span class="who">{agent?.display ?? cap(hit.from)}</span>
              <span class="kind">{hit.msgType}</span>
              <span class="rev-chip" class:cur={isCurrent}>
                {#if isCurrent}
                  <span class="glyph">●</span><span class="lbl">{hit.sha.slice(0, 10)} · this</span>
                {:else}
                  <span class="glyph">◷</span><span class="lbl"
                    >{hit.sha.slice(0, 10)}{hit.commitDate ? ` · ${formatIsoDate(hit.commitDate)}` : ''}</span
                  >
                {/if}
              </span>
            </button>
            <div class="snip">{hit.summary}</div>
            {#if !isCurrent}
              <button type="button" class="align" onclick={() => onAlignToRevision?.(hit.sha)} data-testid="align-to-revision"
                >↳ align code to this version</button
              >
            {/if}
          </div>
        {/each}
      </div>
    </div>
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
  {:else if anchored}
    <!-- Piece 11 Phase 1 — the anchored reader: ONE conversation focused +
         expanded (the full card), the rest a compact "‹ N more" strip.
         Clicking a row/pill only ever moves FOCUS — `onOpenThread` (the
         expanded card's own explicit link) is the ONLY way out to Chat. -->
    {#each hits as hit, i (hit.msgId)}
      {#if i < visibleCount}
        {#if i === focusedIdx}
          {@const agent = resolveAgent(hit.from)}
          {@const c = hitColorVar(hit)}
          <div class="aconv" style="--c:{c}" data-testid="anchored-conv">
            <div class="ach">
              <span class="aav" style="background:{agent?.color ?? 'var(--muted-2)'}">{agent?.abbr ?? hit.from.slice(0, 2).toUpperCase()}</span>
              <span class="awho">{agent?.display ?? cap(hit.from)}</span>
              <span class="akind">{hit.msgType}</span>
              <span class="ats">{formatAge(hit.ts)}</span>
            </div>
            <div class="abody">{hit.summary}</div>
            <div class="afoot">
              <button type="button" class="aopenlink" onclick={() => onOpenThread?.(hit)} data-testid="open-full-thread">open full thread ›</button>
              {#if hit.topic}<span class="asep">·</span><span class="atopic">#{hit.topic}</span>{/if}
              <span class="ahub" class:priv={hit.hubPrivate}>{hit.hub}{hit.hubPrivate ? ' · priv' : ''}</span>
            </div>
          </div>
        {:else}
          {@const agent = resolveAgent(hit.from)}
          <!-- Visible, scannable — NOT hidden behind a count. Clicking
               moves focus (accordion: this expands, the current one
               collapses back to a row); it never navigates away. -->
          <button type="button" class="arow" style="--c:{hitColorVar(hit)}" onclick={() => focusHit(i)} data-testid="anchored-row">
            <span class="arow-who">{agent?.display ?? cap(hit.from)}</span>
            <span class="arow-kind">{hit.msgType}</span>
            <span class="arow-teaser">{hit.summary}</span>
          </button>
        {/if}
      {/if}
    {/each}
    {#if olderCount > 0}
      <button type="button" class="aolder" onclick={() => (showAllRows = true)} data-testid="anchored-older">‹ {olderCount} older</button>
    {/if}
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

  /* Piece 11 Phase 1 — the anchored reader (Code view's persistent
     conversation pane, evolved from the plain hit-list above). */
  .ascope {
    display: flex;
    align-items: center;
    gap: 7px;
    font: 600 12px/1 var(--mono);
    color: var(--fg-dim, var(--text));
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 7px 10px;
    margin-bottom: 12px;
  }
  .ascope .aglyph {
    color: var(--accent);
    font-size: 13px;
  }
  .aconv {
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 11px 12px;
    margin-bottom: 10px;
    position: relative;
  }
  .aconv::before {
    content: '';
    position: absolute;
    left: 0;
    top: 9px;
    bottom: 9px;
    width: 3px;
    border-radius: 2px;
    background: var(--c, var(--accent));
  }
  .ach {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-bottom: 6px;
  }
  .aav {
    width: 19px;
    height: 19px;
    border-radius: 5px;
    display: grid;
    place-items: center;
    font: 700 9px/1 var(--mono);
    color: #12131a;
    flex: 0 0 auto;
  }
  .awho {
    font-weight: 600;
    font-size: 12.5px;
  }
  .akind {
    font: 700 9px/1 var(--mono);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--c, var(--accent));
    border: 1px solid color-mix(in srgb, var(--c, var(--accent)) 40%, transparent);
    border-radius: 4px;
    padding: 2px 5px;
  }
  .ats {
    margin-left: auto;
    font: 500 10px/1 var(--mono);
    color: var(--faint);
  }
  .abody {
    font-size: 12.5px;
    color: var(--fg-dim, var(--muted));
    line-height: 1.55;
  }
  .afoot {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    font: 600 10.5px/1 var(--mono);
  }
  .aopenlink {
    color: var(--accent);
    background: transparent;
    border: 0;
    padding: 0;
    cursor: pointer;
    font: inherit;
  }
  .aopenlink:hover {
    text-decoration: underline;
  }
  .asep {
    color: var(--muted-2, var(--faint));
  }
  .atopic {
    color: var(--faint);
  }
  .ahub {
    margin-left: auto;
    color: var(--faint);
  }
  .ahub.priv {
    color: var(--deferred);
  }
  /* Piece 11 Phase 1 (post-verify tweak) — visible, scannable collapsed
     rows, not a hidden pill strip: the pane's whole point is to scan and
     flow through a range's conversations. Clicking one is the accordion's
     "expand this instead" action — the currently-expanded `.aconv` becomes
     a row exactly like this the moment a DIFFERENT one is focused. */
  .arow {
    display: flex;
    align-items: baseline;
    gap: 7px;
    width: 100%;
    text-align: left;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-left: 3px solid var(--c, var(--accent));
    border-radius: 0 8px 8px 0;
    padding: 6px 10px;
    margin-bottom: 6px;
    cursor: pointer;
    font: inherit;
    color: inherit;
  }
  .arow:hover,
  .arow:focus-visible {
    border-color: var(--c, var(--accent));
    background: var(--panel-3);
  }
  .arow-who {
    font-weight: 600;
    font-size: 11.5px;
    flex: 0 0 auto;
  }
  .arow-kind {
    font: 600 9px/1 var(--mono);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--c, var(--accent));
    flex: 0 0 auto;
  }
  .arow-teaser {
    font-size: 11.5px;
    color: var(--muted);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .aolder {
    display: block;
    width: 100%;
    text-align: center;
    font: 600 10.5px/1 var(--mono);
    color: var(--muted);
    background: transparent;
    border: 1px dashed var(--border-2);
    border-radius: 8px;
    padding: 6px;
    cursor: pointer;
  }
  .aolder:hover {
    color: var(--text);
    border-color: var(--accent);
  }

  /* Piece 11 Phase 5 — the sidebar conversation timeline (mock 12's `.tl`/
     `.spine`/`.node`). `cur`/`old` reuse the SAME `--state-flight`/
     `--state-unowned` tokens Phase 4's rev chip uses for head/pinned — one
     "what version is this" palette everywhere it shows up, not a second one. */
  .timeline {
    margin-bottom: 14px;
  }
  .tl-h {
    font: 600 10px/1 var(--mono);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--faint);
    margin-bottom: 10px;
  }
  .spine {
    position: relative;
    padding-left: 16px;
  }
  .spine::before {
    content: '';
    position: absolute;
    left: 4px;
    top: 4px;
    bottom: 4px;
    width: 2px;
    background: var(--border-2);
  }
  .node {
    position: relative;
    margin-bottom: 10px;
  }
  .node .dot {
    position: absolute;
    left: -16px;
    top: 3px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 2px solid var(--panel);
    background: var(--muted);
  }
  .node.cur .dot {
    background: var(--state-flight);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--state-flight) 25%, transparent);
  }
  .node.old .dot {
    background: var(--state-unowned);
  }
  .node-main {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    text-align: left;
    background: transparent;
    border: 0;
    padding: 0;
    cursor: pointer;
    font: inherit;
    color: inherit;
  }
  .node-main .who {
    font-weight: 600;
    font-size: 12px;
  }
  .node-main .kind {
    font: 700 9px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--faint);
  }
  .node-main .rev-chip {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font: 600 10px/1 var(--mono);
    padding: 2px 6px;
    border-radius: 5px;
    border: 1px solid var(--border);
    color: var(--state-unowned);
    flex: 0 0 auto;
    white-space: nowrap;
  }
  .node-main .rev-chip.cur {
    color: var(--state-flight);
    border-color: color-mix(in srgb, var(--state-flight) 40%, transparent);
  }
  .node .snip {
    font-size: 11.5px;
    color: var(--muted);
    line-height: 1.45;
    margin-top: 2px;
  }
  .node .align {
    font: 600 10.5px/1 var(--mono);
    color: var(--state-unowned);
    background: transparent;
    border: 0;
    padding: 0;
    margin-top: 4px;
    cursor: pointer;
  }
  .node .align:hover {
    text-decoration: underline;
  }
</style>
