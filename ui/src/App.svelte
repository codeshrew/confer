<script lang="ts">
  import { onMount } from 'svelte';
  import TopBar from './lib/components/TopBar.svelte';
  import type { ConnStatus } from './lib/components/TopBar.svelte';
  import LeftRail from './lib/components/LeftRail.svelte';
  import FilterBar, { type StatusFilter } from './lib/components/FilterBar.svelte';
  import ChatStream from './lib/components/ChatStream.svelte';
  import Board from './lib/components/Board.svelte';
  import Fleet from './lib/components/Fleet.svelte';
  import CodeLens from './lib/components/CodeLens.svelte';
  import MetaThread from './lib/components/MetaThread.svelte';
  import RequestDetail from './lib/components/RequestDetail.svelte';
  import ReverseIndexPanel from './lib/components/ReverseIndexPanel.svelte';
  import { api } from './lib/api';
  import { appState, hubDataCache } from './lib/stores.svelte';
  import { selectDefaultHub, selectDefaultTopic } from './lib/hydrate';
  import type { CodeRef, Hub, Message, Overview, RefHit, ThreadNode } from './lib/types';

  let hubs = $state<Hub[]>([]);
  let overview = $state<Overview | null>(null);
  let messages = $state<Message[]>([]);
  let thread = $state<ThreadNode[]>([]);
  let connStatus = $state<ConnStatus>('loading');

  let statusFilter = $state<StatusFilter>('all');
  let notesOn = $state(true);
  let reqsOn = $state(true);
  let selectedRequestId = $state<string | null>(null);

  // Keep-alive for the four main view panes: switching Chat/Board/Fleet/Code
  // via `{#if appState.view === ...}` would destroy and recreate whichever
  // component you're leaving, then rebuild it from scratch on return — for
  // Chat that means re-running renderMarkdown/DOMPurify.sanitize (mitigated
  // above by markdown.ts's own cache, but still real DOM-rebuild work) and
  // for Code a full Shiki re-tokenize + re-fetch. Instead, each pane mounts
  // ONCE on first visit (`*Mounted` flips true and never back) and then
  // stays alive — hidden with CSS (`.view-pane` / `.active`, see the style
  // block below) rather than removed — so tabbing away and back is instant
  // flip, not a re-mount. Panes never visited yet aren't mounted at all, so
  // first load doesn't pay for a Code fetch/tokenize the user hasn't asked
  // for.
  let chatMounted = $state(true); // 'chat' is the initial view
  let boardMounted = $state(false);
  let fleetMounted = $state(false);
  let codeMounted = $state(false);

  $effect(() => {
    switch (appState.view) {
      case 'chat':
        chatMounted = true;
        break;
      case 'board':
        boardMounted = true;
        break;
      case 'fleet':
        fleetMounted = true;
        break;
      case 'code':
        codeMounted = true;
        break;
    }
  });

  // The right rail is a single context panel that switches between the
  // reference graph (default), a request's lifecycle detail (ticket/board
  // row clicked), and the reverse index (a --ref's "N conversations
  // reference these lines" hook, from a chat ref, request detail, or the
  // Code lens's density gutter).
  type ContextMode = 'meta' | 'request' | 'refs';
  let contextMode = $state<ContextMode>('meta');
  let refHits = $state<RefHit[]>([]);
  let refContext = $state<{ repo: string; path: string; range: [number, number] | null } | null>(null);

  // Applies a fetched (or cached) hub's overview/messages to the view state.
  // Shared by the cache-hit and cache-miss paths in loadHub below so the
  // "keep current topic if still valid, else pick a default" logic can't
  // drift between them.
  function applyHubData(data: { overview: Overview; messages: Message[] }) {
    overview = data.overview;
    messages = data.messages;
    // The meta-thread panel is per-message (it's a reply-hash walk rooted
    // at a specific msgId — the real backend 400s on a blank id, there is
    // no "the hub's thread"), so it resets to empty on a hub switch and is
    // populated lazily by selectMessage/selectTicket below, not fetched
    // here.
    thread = [];
    connStatus = 'live';
    // Keep the current topic selection if it's still valid for this hub
    // (e.g. a same-named topic exists in both); otherwise — including the
    // very first load, where appState.topic starts null — pick a sensible
    // default from what this hub's overview actually has. Never falls
    // back to a hardcoded mock slug.
    const validSlugs = new Set(data.overview.topics.map((t) => t.slug));
    if (!appState.topic || !validSlugs.has(appState.topic)) {
      appState.topic = selectDefaultTopic(data.overview);
    }
  }

  async function loadHub(hubId: string) {
    // Cache hit: render instantly from memory, no fetch, no loading flicker.
    // A hub only ever gets fetched once per session unless its cache entry
    // is invalidated by a live SSE event (see the subscribeEvents effect
    // below) — that keeps the CURRENT hub fresh while still making
    // revisiting an already-loaded hub instant.
    const cached = hubDataCache.get(hubId);
    if (cached) {
      applyHubData(cached);
      return;
    }

    connStatus = 'loading';
    try {
      const [ov, msgs] = await Promise.all([api.getOverview(hubId), api.getMessages(hubId)]);
      hubDataCache.set(hubId, { overview: ov, messages: msgs });
      applyHubData({ overview: ov, messages: msgs });
    } catch (err) {
      console.error('confer serve: failed to load hub', hubId, err);
      connStatus = 'reconnecting';
    }
  }

  onMount(() => {
    document.documentElement.setAttribute('data-theme', appState.theme);

    api.getHubs().then((result) => {
      hubs = result;
      // No hardcoded hub id: pick the one the backend marked `current`,
      // else the first hub it returned. This also kicks off the first
      // loadHub, via the $effect below reacting to appState.hub changing
      // from '' to a real id.
      const defaultHub = selectDefaultHub(result);
      if (defaultHub) appState.hub = defaultHub.id;
    });
  });

  $effect(() => {
    // Reload the hub's overview/messages/thread whenever the current hub
    // changes (initial hydration, or a TopBar hub-pill click). Guarded on
    // a non-empty id since appState.hub starts '' until /api/hubs resolves.
    const hubId = appState.hub;
    if (hubId) void loadHub(hubId);
  });

  $effect(() => {
    // (Re)connect the SSE channel whenever the selected hub changes. The
    // indicator starts 'loading' (see connStatus's initial state) and only
    // becomes 'reconnecting' on a genuine transport error from the source
    // itself — never as a default guess.
    const hubId = appState.hub;
    if (!hubId) return;
    const unsubscribe = api.subscribeEvents(
      hubId,
      (event) => {
        if (event.event === 'ping') return;
        if (event.hub !== appState.hub) return;
        // A real message/presence event means the hub-data cache entry is
        // now stale — drop it so loadHub does a real fetch instead of
        // replaying the (now outdated) cached snapshot.
        hubDataCache.invalidate(event.hub);
        void loadHub(appState.hub);
      },
      (status) => {
        connStatus = status;
      }
    );
    return unsubscribe;
  });

  const currentTopic = $derived(overview?.topics.find((t) => t.slug === appState.topic) ?? null);

  function loadThread(msgId: string) {
    api.getThread(appState.hub, msgId).then(
      (th) => {
        thread = th;
      },
      (err) => {
        console.error('confer serve: failed to load thread', msgId, err);
      }
    );
  }

  function selectMessage(id: string) {
    const found = messages.find((m) => m.id === id);
    appState.selectedMessage = found ?? null;
    loadThread(id);
  }

  function selectTicket(id: string) {
    selectedRequestId = id;
    contextMode = 'request';
    // A ticket's originating message shares the `msg_`/`req_` id suffix
    // convention used across the mock fixtures (see ChatStream.findRequest).
    const asMsgId = id.replace(/^req_/, 'msg_');
    const found = messages.find((m) => m.id === asMsgId);
    appState.selectedMessage = found ?? null;
    loadThread(asMsgId);
    // On tablet/phone the right rail is a drawer — a "thread" affordance
    // (this one) is exactly what should surface it. No-op at desktop widths,
    // where the right rail is always visible regardless of drawer state.
    appState.drawer = 'right';
  }

  function selectBoardRow(id: string) {
    selectedRequestId = id;
    contextMode = 'request';
    appState.drawer = 'right';
  }

  function openRefs(ref: CodeRef, hits: RefHit[]) {
    refContext = { repo: ref.repo, path: ref.path, range: ref.range };
    refHits = hits;
    contextMode = 'refs';
    appState.drawer = 'right';
  }

  function openRefsFromCode(ctx: { repo: string; path: string; range: [number, number] | null }, hits: RefHit[]) {
    refContext = ctx;
    refHits = hits;
    contextMode = 'refs';
    appState.drawer = 'right';
  }

  function selectTopic(slug: string) {
    appState.topic = slug;
    // Choosing a topic from the left drawer is "done with the menu" on
    // tablet/phone — close it so the chat underneath is revealed.
    appState.drawer = 'none';
  }

  const selectedRequest = $derived(overview?.board.requests.find((r) => r.id === selectedRequestId) ?? null);
</script>

<div class="app">
  <TopBar
    {hubs}
    currentHub={appState.hub}
    currentView={appState.view}
    {connStatus}
    theme={appState.theme}
    menuOpen={appState.drawer === 'left'}
    onHubChange={(hubId) => (appState.hub = hubId)}
    onViewChange={(view) => (appState.view = view)}
    onThemeToggle={() => appState.toggleTheme()}
    onMenuToggle={() => appState.toggleDrawer('left')}
  />

  {#if appState.view === 'chat' || appState.view === 'board'}
    <FilterBar
      {statusFilter}
      {notesOn}
      {reqsOn}
      agents={overview?.fleet ?? []}
      onStatusFilterChange={(f) => (statusFilter = f)}
      onToggleNotes={() => (notesOn = !notesOn)}
      onToggleReqs={() => (reqsOn = !reqsOn)}
    />
  {/if}

  <div class="main">
    <!-- Scrim: only rendered visually (via CSS) below 1024px, dims + blocks
         clicks through to the tri-pane while a drawer is open, and closes
         whichever drawer is open when tapped. -->
    <div
      class="scrim"
      class:show={appState.drawer !== 'none'}
      onclick={() => appState.closeDrawer()}
      aria-hidden={appState.drawer === 'none'}
      data-testid="drawer-scrim"
    ></div>

    <div class="rail-l-wrap" class:open={appState.drawer === 'left'} data-testid="left-drawer">
      <button
        type="button"
        class="drawer-close"
        aria-label="Close menu"
        onclick={() => appState.closeDrawer()}
        data-testid="left-drawer-close"
      >
        ✕
      </button>
      <LeftRail
        hubName={appState.hub}
        topics={overview?.topics ?? []}
        currentTopic={appState.topic}
        agents={overview?.fleet ?? []}
        onTopicSelect={selectTopic}
      />
    </div>

    <div class="center">
      <div class="crumb">
        <span class="c">{appState.hub}</span>
        <span class="sep">›</span>
        {#if appState.view === 'chat'}
          <span class="c strong hash">#{appState.topic}</span>
          {#if currentTopic}
            <span class="meta">{currentTopic.messages} messages · {currentTopic.requests} requests</span>
          {/if}
        {:else if appState.view === 'board'}
          <span class="c strong">Board</span>
        {:else if appState.view === 'fleet'}
          <span class="c strong">Fleet</span>
        {:else}
          <span class="c strong">Code</span>
        {/if}
        <button
          type="button"
          class="rail-r-toggle"
          aria-label={appState.drawer === 'right' ? 'Close details panel' : 'Open details panel'}
          aria-expanded={appState.drawer === 'right'}
          onclick={() => appState.toggleDrawer('right')}
          data-testid="right-drawer-toggle"
        >
          ⓘ
        </button>
      </div>

      <div class="view-pane" class:active={appState.view === 'chat'}>
        {#if chatMounted}
          <ChatStream
            {messages}
            requests={overview?.board.requests ?? []}
            agents={overview?.fleet ?? []}
            topic={appState.topic}
            hub={appState.hub}
            {notesOn}
            {reqsOn}
            selectedMessageId={appState.selectedMessage?.id ?? null}
            onSelectMessage={selectMessage}
            onSelectTicket={selectTicket}
            onOpenRefs={openRefs}
          />
        {/if}
      </div>
      <div class="view-pane" class:active={appState.view === 'board'}>
        {#if boardMounted}
          <Board
            requests={overview?.board.requests ?? []}
            agents={overview?.fleet ?? []}
            hubName={appState.hub}
            {selectedRequestId}
            onSelectRequest={selectBoardRow}
          />
        {/if}
      </div>
      <div class="view-pane" class:active={appState.view === 'fleet'}>
        {#if fleetMounted}
          <Fleet agents={overview?.fleet ?? []} hubName={appState.hub} />
        {/if}
      </div>
      <div class="view-pane" class:active={appState.view === 'code'}>
        {#if codeMounted}
          <CodeLens hub={appState.hub} onOpenRefs={openRefsFromCode} />
        {/if}
      </div>
    </div>

    <div class="rail-r" class:open={appState.drawer === 'right'} data-testid="right-drawer">
      <div class="ctx-head">
        <button
          type="button"
          class="drawer-close"
          aria-label="Close details panel"
          onclick={() => appState.closeDrawer()}
          data-testid="right-drawer-close"
        >
          ✕
        </button>
        {#if contextMode === 'request'}
          <div class="k">Request detail</div>
          <h2>{selectedRequest?.summary ?? selectedRequestId ?? 'Request'}</h2>
        {:else if contextMode === 'refs'}
          <div class="k">Reverse index</div>
          <h2>Conversations about this code</h2>
        {:else}
          <div class="k">Reference graph</div>
          <h2>Meta-thread</h2>
        {/if}
      </div>
      <div class="ctx-body">
        {#if contextMode === 'request' && selectedRequest}
          <RequestDetail request={selectedRequest} {messages} agents={overview?.fleet ?? []} hub={appState.hub} onOpenRefs={openRefs} />
        {:else if contextMode === 'refs'}
          <ReverseIndexPanel hits={refHits} repo={refContext?.repo ?? null} path={refContext?.path ?? null} range={refContext?.range ?? null} />
        {:else}
          <MetaThread {thread} agents={overview?.fleet ?? []} {messages} />
        {/if}
      </div>
      <div class="foothint">↩ discovered via reply-hashes — no extra state, pure projection</div>
    </div>
  </div>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .main {
    flex: 1;
    display: grid;
    grid-template-columns: 248px 1fr 320px;
    min-height: 0;
    position: relative;
  }

  /* ── Tablet (768–1023px): the tri-pane's desktop column layout is
     unchanged above 1024px. Below it, the right rail (meta-thread / request
     detail / reverse-index) becomes an off-canvas drawer; the left rail
     (topics/fleet) stays put — only its width shrinks slightly. ── */
  @media (max-width: 1023.98px) {
    .main {
      grid-template-columns: 220px 1fr;
    }
  }

  /* ── Phone (<768px): single column. Both rails become off-canvas
     drawers — left opened via the TopBar hamburger, right via the
     in-content "details" toggle or a thread/board-row/ref tap. ── */
  @media (max-width: 767.98px) {
    .main {
      grid-template-columns: 1fr;
    }
  }

  .scrim {
    display: none;
  }
  @media (max-width: 1023.98px) {
    .scrim {
      display: block;
      position: fixed;
      inset: 0;
      background: rgba(4, 6, 10, 0.55);
      z-index: 35;
      opacity: 0;
      pointer-events: none;
      transition: opacity 0.2s ease;
    }
    .scrim.show {
      opacity: 1;
      pointer-events: auto;
    }
  }

  /* Left rail wrapper: a no-op at desktop (`display: contents` keeps
     LeftRail as the direct 248px/220px grid-column item it already was).
     Below 1024px it becomes a fixed off-canvas panel sliding in from the
     left, toggled by the TopBar hamburger. */
  .rail-l-wrap {
    display: contents;
  }
  @media (max-width: 1023.98px) {
    .rail-l-wrap {
      display: block;
      position: fixed;
      top: 0;
      bottom: 0;
      left: 0;
      width: 280px;
      max-width: 82vw;
      z-index: 40;
      transform: translateX(-100%);
      transition: transform 0.22s ease;
      box-shadow: var(--shadow);
    }
    .rail-l-wrap.open {
      transform: translateX(0);
    }
    .rail-l-wrap :global(.rail-l) {
      height: 100%;
    }
  }

  .drawer-close {
    display: none;
  }
  @media (max-width: 1023.98px) {
    .drawer-close {
      display: flex;
      align-items: center;
      justify-content: center;
      position: absolute;
      top: 8px;
      right: 8px;
      width: 40px;
      height: 40px;
      border: 1px solid var(--border-2);
      background: var(--panel-2);
      color: var(--muted);
      border-radius: 8px;
      font-size: 14px;
      z-index: 1;
    }
    .rail-l-wrap .drawer-close {
      top: 8px;
      right: 8px;
    }
  }

  .rail-r-toggle {
    display: none;
  }
  @media (max-width: 1023.98px) {
    .rail-r-toggle {
      display: flex;
      align-items: center;
      justify-content: center;
      margin-left: auto;
      width: 40px;
      height: 40px;
      border: 1px solid var(--border-2);
      background: var(--panel-2);
      color: var(--muted);
      border-radius: 8px;
      font-size: 15px;
      flex: 0 0 auto;
    }
  }
  .center {
    display: flex;
    flex-direction: column;
    min-height: 0;
    /* Grid items default to `min-width: auto`, which lets a descendant's
       min-content size (e.g. an unwrapped ticket track, a long code line)
       win over the 1fr track and blow the whole row out past the viewport
       on phone. min-width: 0 makes `.center` actually shrink to the track
       it's given, so its children's own overflow/wrap rules (TicketCard,
       BoardRow, CodeLens, etc.) are what's left to decide, not this. */
    min-width: 0;
    background: var(--bg);
  }
  /* One wrapper per Chat/Board/Fleet/Code pane. Only the active view's
     wrapper participates in layout (display:flex, matching what `.center`'s
     direct child used to be); the rest are `display:none` — kept mounted
     (see chatMounted/boardMounted/fleetMounted/codeMounted above) but out
     of the flow entirely, not just visually hidden, so they can't be
     tabbed/clicked into and don't affect layout. */
  .view-pane {
    display: none;
    flex: 1;
    flex-direction: column;
    min-height: 0;
  }
  .view-pane.active {
    display: flex;
  }
  .crumb {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
    flex: 0 0 auto;
    background: var(--panel);
  }
  .crumb .c {
    color: var(--muted);
    font-size: 13px;
  }
  .crumb .c.strong {
    color: var(--text);
    font-weight: 600;
  }
  .crumb .sep {
    color: var(--faint);
  }
  .crumb .hash {
    font-family: var(--mono);
    color: var(--accent);
  }
  .crumb .meta {
    margin-left: auto;
    color: var(--faint);
    font: 500 11.5px/1 var(--mono);
  }
  .rail-r {
    background: var(--panel);
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  /* Below 1024px the right rail leaves the grid flow and becomes a fixed
     slide-over panel from the right, toggled by the ⓘ crumb button or by
     selecting a request/ref (see selectTicket/selectBoardRow/openRefs*). */
  @media (max-width: 1023.98px) {
    .rail-r {
      position: fixed;
      top: 0;
      bottom: 0;
      right: 0;
      width: 360px;
      max-width: 88vw;
      z-index: 40;
      border-left: 1px solid var(--border-2);
      transform: translateX(100%);
      transition: transform 0.22s ease;
      box-shadow: var(--shadow);
    }
    .rail-r.open {
      transform: translateX(0);
    }
  }
  .ctx-head {
    position: relative;
    padding: 14px 16px 12px;
    border-bottom: 1px solid var(--border);
    flex: 0 0 auto;
  }
  @media (max-width: 1023.98px) {
    .ctx-head {
      padding-right: 56px;
    }
  }
  .ctx-head .k {
    font: 700 10px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--faint);
  }
  .ctx-head h2 {
    margin: 8px 0 0;
    font-size: 14.5px;
    font-weight: 650;
  }
  .ctx-body {
    overflow-y: auto;
    flex: 1;
    padding: 14px 16px;
  }
  .foothint {
    padding: 10px 16px;
    border-top: 1px solid var(--border);
    color: var(--faint);
    font-size: 11px;
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 0 0 auto;
  }

  @media (max-width: 767.98px) {
    .crumb {
      flex-wrap: wrap;
      row-gap: 6px;
      padding: 10px 14px;
    }
    .crumb .meta {
      margin-left: 0;
      flex-basis: 100%;
    }
  }
</style>
