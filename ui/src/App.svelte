<script lang="ts">
  import { onMount } from 'svelte';
  import TopBar from './lib/components/TopBar.svelte';
  import type { ConnStatus } from './lib/components/TopBar.svelte';
  import LeftRail from './lib/components/LeftRail.svelte';
  import CodeTree from './lib/components/CodeTree.svelte';
  import FilterBar, { type StatusFilter } from './lib/components/FilterBar.svelte';
  import ChatStream from './lib/components/ChatStream.svelte';
  import Board from './lib/components/Board.svelte';
  import Fleet from './lib/components/Fleet.svelte';
  import CodeLens from './lib/components/CodeLens.svelte';
  import Repos from './lib/components/Repos.svelte';
  import MetaThread from './lib/components/MetaThread.svelte';
  import RequestDetail from './lib/components/RequestDetail.svelte';
  import ReverseIndexPanel from './lib/components/ReverseIndexPanel.svelte';
  import EmptyState from './lib/components/EmptyState.svelte';
  import CopyIdButton from './lib/components/CopyIdButton.svelte';
  import { api } from './lib/api';
  import { appState, chatWindowCache, codeState, hubDataCache } from './lib/stores.svelte';
  import { selectDefaultHub, selectDefaultTopic } from './lib/hydrate';
  import { breadcrumbFromTree, buildTree, collapseBreadcrumb, fileKey, type BreadcrumbNode } from './lib/codeTree';
  import { formatIsoDate } from './lib/format';
  import {
    defaultContextMode,
    leftRailVisible,
    rightRailToggleVisible,
    rightRailVisible as computeRightRailVisible,
    showFleetSection,
  } from './lib/railLayout';
  import type { CodeRef, Hub, Message, Overview, RefHit, ThreadNode } from './lib/types';

  let hubs = $state<Hub[]>([]);
  let overview = $state<Overview | null>(null);
  let messages = $state<Message[]>([]);
  let thread = $state<ThreadNode[]>([]);
  let connStatus = $state<ConnStatus>('loading');

  // --- ChatStream's windowed message page ---------------------------------
  // Distinct from `messages` above (the full, unpaginated hub fetch still
  // used by RequestDetail/MetaThread's cross-topic trail reconstruction —
  // see their own CONTRACT GAP notes). ChatStream instead renders this
  // per-(hub,topic) window: most-recent CHAT_PAGE_SIZE on load, grown
  // backward on scroll-up (loadOlderChatMessages), cached across hub/topic
  // switches by chatWindowCache so revisiting one already loaded this
  // session is instant.
  const CHAT_PAGE_SIZE = 50;
  let chatMessages = $state<Message[]>([]);
  let chatHasMore = $state(false);
  let chatLoadingOlder = $state(false);

  async function loadChatWindow(hubId: string, topic: string) {
    const cached = chatWindowCache.get(hubId, topic);
    if (cached) {
      chatMessages = cached.messages;
      chatHasMore = cached.hasMore;
      return;
    }
    try {
      const page = await api.getMessages(hubId, topic, { limit: CHAT_PAGE_SIZE });
      // Still the current hub/topic once the fetch resolves? A quick
      // hub/topic switch mid-flight must not stomp the newer selection's
      // (possibly already-cached) window with this now-stale one.
      if (appState.hub !== hubId || appState.topic !== topic) return;
      const hasMore = page.length === CHAT_PAGE_SIZE;
      chatWindowCache.set(hubId, topic, { messages: page, hasMore });
      chatMessages = page;
      chatHasMore = hasMore;
    } catch (err) {
      console.error('confer serve: failed to load chat window', hubId, topic, err);
    }
  }

  /** Scroll-load: fetch the next older page and prepend it. Returns the
   * number of messages prepended (0 if there was nothing older, or a fetch
   * was already in flight) so ChatStream knows whether to keep listening
   * for more scroll-up. */
  async function loadOlderChatMessages(): Promise<number> {
    if (chatLoadingOlder || !chatHasMore) return 0;
    const hubId = appState.hub;
    const topic = appState.topic;
    const oldest = chatMessages[0];
    if (!hubId || !topic || !oldest) return 0;
    chatLoadingOlder = true;
    try {
      const older = await api.getMessages(hubId, topic, { limit: CHAT_PAGE_SIZE, before: oldest.id });
      if (appState.hub !== hubId || appState.topic !== topic) return 0;
      const hasMore = older.length === CHAT_PAGE_SIZE;
      chatMessages = [...older, ...chatMessages];
      chatHasMore = hasMore;
      chatWindowCache.set(hubId, topic, { messages: chatMessages, hasMore });
      return older.length;
    } catch (err) {
      console.error('confer serve: failed to load older chat messages', hubId, topic, err);
      return 0;
    } finally {
      chatLoadingOlder = false;
    }
  }

  /** SSE landed a `message` event for the topic currently on screen — fetch
   * just the newest page and append whatever isn't already loaded, instead
   * of invalidating/re-fetching the whole window (which would both be
   * wasteful on a large hub and reset the reader's scroll position). */
  async function appendNewestChatMessages(hubId: string, topic: string) {
    try {
      const page = await api.getMessages(hubId, topic, { limit: CHAT_PAGE_SIZE });
      if (appState.hub !== hubId || appState.topic !== topic) return;
      const known = new Set(chatMessages.map((m) => m.id));
      const fresh = page.filter((m) => !known.has(m.id));
      if (fresh.length === 0) return;
      chatMessages = [...chatMessages, ...fresh];
      chatWindowCache.set(hubId, topic, { messages: chatMessages, hasMore: chatHasMore });
    } catch (err) {
      console.error('confer serve: failed to append newest chat messages', hubId, topic, err);
    }
  }

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
  let reposMounted = $state(false);

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
      case 'repos':
        reposMounted = true;
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
  // design/44 §6 item 2.4 — `path: null` means repo-mode (a whole-repo
  // rollup, no single file selected yet).
  let refContext = $state<{ repo: string; path: string | null; range: [number, number] | null } | null>(null);
  // The active Code file's FULL (whole-file, range:null-included) hit list —
  // kept separate from refHits/refContext above because a hot-line click
  // narrows those to a single range. The "↩ whole file" chip (design/43
  // quick win) needs somewhere to return TO that isn't itself overwritten by
  // the narrowing click.
  let fileLevelRefs = $state<{ ctx: { repo: string; path: string }; hits: RefHit[] } | null>(null);

  // design/43 Thread 1 — Code view's right rail is open whenever a file is
  // active (there's always one on a non-empty hub). design/43 Phase B: this
  // now reads straight off the shared `codeState` store instead of a
  // callback CodeLens used to fire (`onActiveFileChange`) — CodeTree and
  // CodeLens both write/read the same per-hub record, so App can just look.
  const codeHasActiveFile = $derived(codeState.forHub(appState.hub).activeKey !== null);

  // Whether the right rail has anything to inspect for the CURRENT view —
  // the single source of truth for both the grid-column collapse and the
  // pane's own visibility (see the `.main`/`.rail-r` markup below).
  const rightRailOpen = $derived(
    computeRightRailVisible({
      view: appState.view,
      hasSelection: appState.view === 'board' ? selectedRequestId !== null : appState.view === 'code' ? codeHasActiveFile : false,
    })
  );
  const leftRailHidden = $derived(!leftRailVisible(appState.view));
  const showFleetInRail = $derived(showFleetSection(appState.view));
  const showRightRailToggle = $derived(rightRailToggleVisible(appState.view));

  $effect(() => {
    // Reset the inspector to the view's own legal default whenever the view
    // changes — kills the "Request detail leaks into Code" bug family,
    // where a mode picked on one view lingered visually into the next.
    contextMode = defaultContextMode(appState.view);
  });

  // A reverse-index hit clicked from the right rail (Code's file-level list,
  // or a line's hot-line drill-in) navigates to Chat, at that message — even
  // across hubs. Cross-hub, `messages` isn't populated yet the instant
  // appState.hub flips, so the jump is deferred here and resolved by the
  // $effect below once that hub's messages actually land.
  let pendingHit = $state<{ msgId: string; topic: string | null } | null>(null);

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
    // Load (or restore from cache) the ChatStream's windowed page whenever
    // the current hub OR topic changes — a topic switch alone (no hub
    // change) must also refetch, since the window is scoped per-topic.
    // Guarded on both being resolved: topic starts null until loadHub's
    // applyHubData picks a default (see selectDefaultTopic).
    const hubId = appState.hub;
    const topic = appState.topic;
    if (hubId && topic) void loadChatWindow(hubId, topic);
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
        // replaying the (now outdated) cached snapshot. This still refetches
        // the FULL messages list (unpaginated) that RequestDetail/MetaThread
        // rely on for cross-topic trail reconstruction.
        hubDataCache.invalidate(event.hub);
        codeState.invalidate(event.hub);
        void loadHub(appState.hub);
        // The ChatStream window, in contrast, must NOT redo a full refetch
        // here — that would both hammer a large hub's history on every tick
        // and reset whatever the reader has scrolled back to. If the event
        // is for the topic currently on screen, just fetch the newest page
        // and append what's missing.
        const currentTopicId = appState.topic;
        if (event.event === 'message' && currentTopicId && event.topic === currentTopicId) {
          void appendNewestChatMessages(appState.hub, currentTopicId);
        }
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

  // design/41 Phase 0 items 2-4: the shared "jump to a message in Chat"
  // navigation used by MetaThread's onSelectNode (a thread node click) and
  // RequestDetail's lifecycle-trail row clicks. Always lands in the Chat
  // view; switches topic first (awaiting that topic's window so the
  // pagination-chase in ChatStream's scrollToMessageId effect starts from
  // the RIGHT topic's data, not a stale one) when the target message lives
  // in a different topic than whatever's currently showing.
  let scrollTargetId = $state<string | null>(null);
  let scrollToken = $state(0);

  async function navigateToMessageInChat(msgId: string, topic?: string | null) {
    appState.view = 'chat';
    if (topic && topic !== appState.topic) {
      appState.topic = topic;
      await loadChatWindow(appState.hub, topic);
    }
    selectMessage(msgId);
    scrollTargetId = msgId;
    scrollToken++;
  }

  function onMetaThreadSelectNode(msgId: string) {
    // MetaThread's onSelectNode only carries the msgId — recover the
    // node's own topic from the currently-loaded `thread` (the same
    // ThreadNode[] MetaThread itself renders from) so a cross-topic node
    // click switches topic before scrolling.
    const node = thread.find((n) => n.msgId === msgId);
    void navigateToMessageInChat(msgId, node?.topic ?? null);
  }

  function selectMessage(id: string) {
    const found = messages.find((m) => m.id === id);
    appState.selectedMessage = found ?? null;
    // A plain note/thread click is the meta-thread pane, not "Request
    // detail" — without this, clicking a ticket first (which sets
    // contextMode = 'request') then a plain note left the sidebar stuck
    // showing the PREVIOUS ticket's "Request detail" heading and content
    // over the newly-selected note's own thread.
    contextMode = 'meta';
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

  // Fired by CodeLens whenever the SELECTED FILE's full reference list loads
  // (whole-file `range:null` hits included) — not a click, just "a file is
  // now showing", so the right rail's reverse index stays in sync with the
  // Code pane without forcing the mobile drawer open the way an explicit
  // hot-line click does.
  function onCodeFileRefs(ctx: { repo: string; path: string }, hits: RefHit[]) {
    refContext = { repo: ctx.repo, path: ctx.path, range: null };
    refHits = hits;
    contextMode = 'refs';
    fileLevelRefs = { ctx, hits };
  }

  // The "↩ whole file" chip — returns the inspector from a hot-line-narrowed
  // range back to the active file's full hit list (design/43 quick win).
  function backToWholeFile() {
    if (!fileLevelRefs) return;
    refContext = { ...fileLevelRefs.ctx, range: null };
    refHits = fileLevelRefs.hits;
  }

  // A reverse-index entry (file-level list or line drill-in) was clicked —
  // jump to that message in Chat, switching hub/topic first if the hit came
  // from elsewhere (getRefs(allHubs=1) can span hubs).
  function openHitInChat(hit: RefHit) {
    appState.view = 'chat';
    // Defensive: a hit's `hub` is documented as always-populated (see
    // RefHit in types.ts), but a live backend has been observed to omit it
    // (a server-side /api/refs contract gap, not this UI's to fix) — falling
    // back to the CURRENT hub instead of hub `undefined` keeps navigation
    // working rather than 404ing every subsequent fetch.
    const targetHub = hit.hub || appState.hub;
    if (targetHub && targetHub !== appState.hub) {
      pendingHit = { msgId: hit.msgId, topic: hit.topic };
      appState.hub = targetHub;
      return;
    }
    if (hit.topic) appState.topic = hit.topic;
    selectMessage(hit.msgId);
  }

  $effect(() => {
    // Resolves openHitInChat's cross-hub jump once the target hub's
    // messages have actually loaded (loadHub is async).
    const p = pendingHit;
    if (!p) return;
    const found = messages.find((m) => m.id === p.msgId);
    if (!found) return;
    if (p.topic) appState.topic = p.topic;
    selectMessage(p.msgId);
    pendingHit = null;
  });

  function selectTopic(slug: string) {
    appState.topic = slug;
    // Choosing a topic from the left drawer is "done with the menu" on
    // tablet/phone — close it so the chat underneath is revealed.
    appState.drawer = 'none';
  }

  const selectedRequest = $derived(overview?.board.requests.find((r) => r.id === selectedRequestId) ?? null);

  // design/43 Phase B — the unified Code breadcrumb: `hub › Code › repo ›
  // dir › … › file @sha`, absorbing CodeLens's old standalone `repo ›
  // path` crumb line entirely. Built from the SAME tree CodeTree renders
  // (walking the actual compacted structure, not a raw path split) so a
  // crumb segment always corresponds to exactly one real tree row —
  // clicking it can reveal that exact row (see onCrumbSegmentClick below).
  const codeCrumb = $derived.by((): { segments: BreadcrumbNode[]; full: string; sha: string | null } => {
    const cs = codeState.forHub(appState.hub);
    // design/44 §6 item 2.4 — the repo node was selected as the view target
    // itself (a repo rollup, not a single file): the crumb is just the repo.
    if (cs.viewMode === 'repo' && cs.activeRepo) {
      return { segments: [{ label: cs.activeRepo, nodeId: cs.activeRepo }], full: cs.activeRepo, sha: null };
    }
    const active = cs.files.find((f) => fileKey(f) === cs.activeKey);
    if (!active) return { segments: [], full: '', sha: null };
    const tree = buildTree(cs.files);
    const chain = breadcrumbFromTree(tree, active.repo, fileKey(active));
    const full = `${active.repo}/${active.path}`;
    return { segments: chain, full, sha: cs.codeSha !== 'HEAD' ? cs.codeSha : null };
  });
  const codeCrumbDisplay = $derived(collapseBreadcrumb(codeCrumb.segments, 4));

  // design/44 §5.1 — "Web (... Code view header): a branch/tag chip + the
  // commit date beside the sha chip." Sourced from the active file's newest
  // hit (the same one `codeSha` above is pinned at) — no extra fetch, this
  // is the same whole-file hit list CodeLens already reports via onFileRefs.
  const codeCrumbMeta = $derived.by((): { refName: string | null; commitDate: string | null } => {
    // Only meaningful in single-file scope — a repo rollup has no one
    // "newest hit" to label, and `fileLevelRefs` can otherwise be a stale
    // leftover from whichever file was active before a repo-rollup selection.
    if (codeState.forHub(appState.hub).viewMode !== 'file') return { refName: null, commitDate: null };
    const hits = fileLevelRefs?.hits ?? [];
    if (hits.length === 0) return { refName: null, commitDate: null };
    const newest = [...hits].sort((a, b) => (a.ts < b.ts ? 1 : a.ts > b.ts ? -1 : 0))[0]!;
    return { refName: newest.refName, commitDate: newest.commitDate };
  });

  /** A breadcrumb segment (repo/dir/file) click reveals + scrolls that node
   * in CodeTree — selection-only, no routing implication yet (design/41's
   * `code?repo=&path=` will plug in here later). */
  function onCrumbSegmentClick(nodeId: string | null) {
    if (!nodeId) return;
    codeState.forHub(appState.hub).pendingReveal = nodeId;
  }

  /** CodeTree's onActivate — a file click (or filter Enter). Closes the
   * mobile left drawer, same contract as selectTopic: choosing something
   * from the drawer menu means the reader is "done with the menu". */
  function onCodeFileActivate() {
    if (appState.drawer === 'left') appState.drawer = 'none';
  }

  /** design/44 §6 item 2.4 — CodeTree's repo-select affordance. Closes the
   * mobile left drawer (same contract as onCodeFileActivate); the actual
   * repo-rollup fetch + right-rail sync happens in CodeLens (onRepoRefs
   * below), the same one-fetch-one-callback shape onFileRefs already uses. */
  function onCodeRepoActivate() {
    if (appState.drawer === 'left') appState.drawer = 'none';
  }

  /** Fired by CodeLens whenever a repo rollup's hit list (re)loads — mirrors
   * onCodeFileRefs, keeping the right rail's ReverseIndexPanel in repo-mode
   * (`path: null`) in sync with whichever repo is the active view target. */
  function onCodeRepoRefs(repo: string, hits: RefHit[]) {
    refContext = { repo, path: null, range: null };
    refHits = hits;
    contextMode = 'refs';
  }

  /** The reverse-index panel's "widen to repo" breadcrumb segment — routes
   * through the SAME codeState the CodeTree repo-select affordance uses, so
   * CodeLens's own effect does the fetch and calls onCodeRepoRefs above. */
  function widenToRepo(repo: string) {
    const cs = codeState.forHub(appState.hub);
    cs.activeRepo = repo;
    cs.viewMode = 'repo';
  }

  /** A repo-mode file-group row was clicked — narrow back down into that
   * file. Sets the shared codeState (CodeLens's `active` effect does the
   * fetch and calls onCodeFileRefs, which updates this same right rail). */
  function selectFileFromRepoMode(path: string) {
    const repo = refContext?.repo;
    if (!repo) return;
    const cs = codeState.forHub(appState.hub);
    cs.activeKey = fileKey({ repo, path });
    cs.viewMode = 'file';
  }
</script>

<div class="app">
  <TopBar
    {hubs}
    currentHub={appState.hub}
    currentView={appState.view}
    {connStatus}
    theme={appState.theme}
    menuOpen={appState.drawer === 'left'}
    showMenu={!leftRailHidden}
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
      chatDensity={appState.view === 'chat' ? appState.chatDensity : undefined}
      onStatusFilterChange={(f) => (statusFilter = f)}
      onToggleNotes={() => (notesOn = !notesOn)}
      onToggleReqs={() => (reqsOn = !reqsOn)}
      onChatDensityChange={(d) => (appState.chatDensity = d)}
    />
  {/if}

  <div
    class="main"
    data-view={appState.view}
    style={`${leftRailHidden ? '--rail-l-w:0px;' : ''}${!rightRailOpen ? '--rail-r-w:0px;' : ''}`}
  >
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

    <div
      class="rail-l-wrap"
      class:open={appState.drawer === 'left'}
      style={leftRailHidden ? 'visibility:hidden' : undefined}
      data-testid="left-drawer"
    >
      <button
        type="button"
        class="drawer-close"
        aria-label="Close menu"
        onclick={() => appState.closeDrawer()}
        data-testid="left-drawer-close"
      >
        ✕
      </button>
      {#if appState.view === 'code'}
        <!-- design/43 Thread 1/2: Code's navigator IS the file tree, not
             topics/fleet — replaces LeftRail entirely in this view (also
             becomes the mobile left drawer's content for free, since this
             wrapper's drawer CSS doesn't care what's inside it). -->
        <CodeTree hub={appState.hub} onActivate={onCodeFileActivate} onActivateRepo={onCodeRepoActivate} />
      {:else}
        <LeftRail
          hubName={appState.hub}
          topics={overview?.topics ?? []}
          currentTopic={appState.topic}
          agents={overview?.fleet ?? []}
          showFleet={showFleetInRail}
          onTopicSelect={selectTopic}
        />
      {/if}
    </div>

    <div class="center">
      <div class="crumb" title={appState.view === 'code' && codeCrumb.full ? codeCrumb.full : undefined}>
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
        {:else if appState.view === 'code'}
          <span class="c strong">Code</span>
          {#each codeCrumbDisplay as seg, i (i)}
            <span class="sep">›</span>
            {#if seg.nodeId}
              <button type="button" class="c crumb-seg" onclick={() => onCrumbSegmentClick(seg.nodeId)}>{seg.label}</button>
            {:else}
              <span class="c crumb-ellipsis">{seg.label}</span>
            {/if}
          {/each}
          {#if codeCrumb.sha}
            <span class="ct-sha" title="Rendered at the newest reference's pinned sha">@{codeCrumb.sha.slice(0, 10)}</span>
          {/if}
          {#if codeCrumbMeta.refName}
            <span class="ct-refname" data-testid="code-header-refname">{codeCrumbMeta.refName}</span>
          {/if}
          {#if codeCrumbMeta.commitDate}
            <span class="ct-commit-date" data-testid="code-header-date">{formatIsoDate(codeCrumbMeta.commitDate)}</span>
          {/if}
        {:else}
          <span class="c strong">Repos</span>
        {/if}
        {#if showRightRailToggle}
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
        {/if}
      </div>

      <div class="view-pane" class:active={appState.view === 'chat'}>
        {#if chatMounted}
          <ChatStream
            messages={chatMessages}
            hasMore={chatHasMore}
            loadingOlder={chatLoadingOlder}
            onLoadOlder={loadOlderChatMessages}
            requests={overview?.board.requests ?? []}
            agents={overview?.fleet ?? []}
            topic={appState.topic}
            hub={appState.hub}
            {notesOn}
            {reqsOn}
            density={appState.chatDensity}
            selectedMessageId={appState.selectedMessage?.id ?? null}
            scrollToMessageId={scrollTargetId}
            {scrollToken}
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
          <CodeLens hub={appState.hub} onOpenRefs={openRefsFromCode} onFileRefs={onCodeFileRefs} onRepoRefs={onCodeRepoRefs} />
        {/if}
      </div>
      <div class="view-pane" class:active={appState.view === 'repos'}>
        {#if reposMounted}
          <Repos hub={appState.hub} />
        {/if}
      </div>
    </div>

    <div
      class="rail-r"
      class:open={appState.drawer === 'right'}
      style={!rightRailOpen ? 'visibility:hidden' : undefined}
      data-testid="right-drawer"
    >
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
          <h2>
            {selectedRequest?.summary ?? selectedRequestId ?? 'Request'}
            {#if selectedRequestId}
              <CopyIdButton id={selectedRequestId} class="ctx-copy-id" />
            {/if}
          </h2>
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
          <RequestDetail
            request={selectedRequest}
            {messages}
            agents={overview?.fleet ?? []}
            hub={appState.hub}
            onOpenRefs={openRefs}
            onSelectMessage={navigateToMessageInChat}
          />
        {:else if contextMode === 'refs'}
          <ReverseIndexPanel
            hits={refHits}
            repo={refContext?.repo ?? null}
            path={refContext?.path ?? null}
            range={refContext?.range ?? null}
            onSelectHit={openHitInChat}
            onWholeFile={backToWholeFile}
            onWidenToRepo={() => refContext?.repo && widenToRepo(refContext.repo)}
            onSelectFile={selectFileFromRepoMode}
          />
        {:else if appState.selectedMessage}
          <MetaThread {thread} agents={overview?.fleet ?? []} {messages} density={appState.chatDensity} onSelectNode={onMetaThreadSelectNode} />
        {:else}
          <EmptyState
            glyph="↩"
            title="Select a message to trace its thread"
            body="Click any note or request in the stream — its reference graph (the reply-hash trail reconstructing its conversation, across topics) shows up here."
          />
        {/if}
      </div>
      {#if contextMode === 'meta'}
        <div class="foothint">↩ discovered via reply-hashes — no extra state, pure projection</div>
      {/if}
    </div>
  </div>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  /* design/43 Thread 1: both rail widths are driven per-view by
     `--rail-l-w`/`--rail-r-w` custom properties set inline on `.main` (see
     App.svelte's script) — `0px` when a view's rail is hidden/collapsed,
     otherwise unset so each breakpoint's own fallback below applies. The
     transition animates Board's collapsed→open slide and any view switch's
     rail appearance/disappearance; reduced-motion gets a hard snap instead. */
  .main {
    flex: 1;
    display: grid;
    grid-template-columns: var(--rail-l-w, 248px) 1fr var(--rail-r-w, 320px);
    min-height: 0;
    position: relative;
    transition: grid-template-columns 0.2s ease;
  }
  @media (prefers-reduced-motion: reduce) {
    .main {
      transition: none;
    }
  }

  /* ── Tablet (768–1023px): the tri-pane's desktop column layout is
     unchanged above 1024px. Below it, the right rail (meta-thread / request
     detail / reverse-index) becomes an off-canvas drawer (so it drops out of
     this grid entirely — only the left rail's track matters here); the left
     rail (topics/fleet) stays put — only its width shrinks slightly, unless
     the current view hides it (`--rail-l-w: 0px` overrides the 220px fallback
     the same as it does 248px at desktop). ── */
  @media (max-width: 1023.98px) {
    .main {
      grid-template-columns: var(--rail-l-w, 220px) 1fr;
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
  /* One wrapper per Chat/Board/Fleet/Code/Repos pane. Only the active view's
     wrapper participates in layout (display:flex, matching what `.center`'s
     direct child used to be); the rest are `display:none` — kept mounted
     (see chatMounted/boardMounted/fleetMounted/codeMounted/reposMounted
     above) but out of the flow entirely, not just visually hidden, so they can't be
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
  /* design/43 Phase B — Code view's unified breadcrumb segments (repo/dir/
     .../file), clickable to reveal + scroll that node in CodeTree. */
  .crumb-seg {
    background: transparent;
    border: 0;
    color: var(--muted);
    font: inherit;
    font-family: var(--mono);
    font-size: 12.5px;
    padding: 0;
    cursor: pointer;
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .crumb-seg:hover {
    color: var(--accent);
    text-decoration: underline;
  }
  .crumb-ellipsis {
    color: var(--faint);
    font-family: var(--mono);
    font-size: 12.5px;
  }
  .ct-sha {
    font: 600 10.5px/1 var(--mono);
    color: var(--muted);
    background: var(--panel-3);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 6px;
  }
  .ct-refname {
    font: 600 10.5px/1 var(--mono);
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 13%, transparent);
    border-radius: 5px;
    padding: 3px 6px;
  }
  .ct-commit-date {
    font: 500 10.5px/1 var(--mono);
    color: var(--faint);
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
    display: flex;
    align-items: center;
    gap: 7px;
  }
  /* Copy-id affordance (design/41 Phase 0) on the request-detail header —
     reveal on hover/focus of the whole header (desktop); CopyIdButton's own
     `(hover: none)` query keeps it always-visible on touch. */
  .ctx-head:hover :global(.ctx-copy-id),
  .ctx-head:focus-within :global(.ctx-copy-id) {
    opacity: 1;
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
