// Svelte 5 runes-based app state. Small and typed — later agents extend
// this as more panes come online (chat stream selection, filters, etc).

import { SvelteSet } from 'svelte/reactivity';
import { api } from './api';
import { buildTree, defaultExpandedIds, fileKey } from './codeTree';
import type { CodeFile, Message, Overview } from './types';

// --- per-hub data cache --------------------------------------------------
// Switching hubs in the TopBar re-fetches /api/overview + /api/messages
// every time — including re-visiting a hub already loaded this session.
// This cache lets App.svelte's loadHub render a previously-seen hub
// instantly from memory instead of re-fetching. It is NOT reactive state on
// its own (plain Map, not $state) — callers own displaying the data via
// their own $state fields; this is purely "have we already fetched this
// hub" bookkeeping.
//
// Live updates: the current hub's SSE channel invalidates this hub's entry
// on every message/presence event (see App.svelte's subscribeEvents
// handler), so the next loadHub() call for that hub does a real fetch
// again — a stale cache entry never lingers for the hub you're actively
// watching. A hub you're NOT currently on has no live channel open (the
// backend's /api/events is scoped to one hub), so its cache entry can go
// stale until you revisit it; that's an accepted tradeoff for "instant
// hub-switch," not a bug.
export interface HubData {
  overview: Overview;
  messages: Message[];
}

function createHubDataCache() {
  const entries = new Map<string, HubData>();
  return {
    get(hubId: string): HubData | undefined {
      return entries.get(hubId);
    },
    set(hubId: string, data: HubData): void {
      entries.set(hubId, data);
    },
    has(hubId: string): boolean {
      return entries.has(hubId);
    },
    invalidate(hubId: string): void {
      entries.delete(hubId);
    },
    clear(): void {
      entries.clear();
    },
    get size(): number {
      return entries.size;
    },
  };
}

export type HubDataCache = ReturnType<typeof createHubDataCache>;

export const hubDataCache: HubDataCache = createHubDataCache();

// --- per-(hub,topic) chat window cache ------------------------------------
// ChatStream no longer renders off the (unpaginated, whole-hub) HubData.messages
// above — it renders a windowed page, fetched most-recent-first and grown
// backward as the reader scrolls up (see App.svelte's loadChatWindow /
// loadOlderChatMessages). This cache lets switching back to a hub+topic
// already visited this session restore that window instantly (same
// "instant hub-switch" contract as hubDataCache), rather than re-fetching
// page 1 and losing however far back the reader had scrolled.
export interface ChatWindow {
  /** Loaded messages, oldest first (chronological) — the pages fetched so far. */
  messages: Message[];
  /** False once a page came back shorter than the page size — nothing older left. */
  hasMore: boolean;
}

function chatKey(hubId: string, topic: string): string {
  return `${hubId} ${topic}`;
}

function createChatWindowCache() {
  const entries = new Map<string, ChatWindow>();
  return {
    get(hubId: string, topic: string): ChatWindow | undefined {
      return entries.get(chatKey(hubId, topic));
    },
    set(hubId: string, topic: string, data: ChatWindow): void {
      entries.set(chatKey(hubId, topic), data);
    },
    has(hubId: string, topic: string): boolean {
      return entries.has(chatKey(hubId, topic));
    },
    clear(): void {
      entries.clear();
    },
    get size(): number {
      return entries.size;
    },
  };
}

export type ChatWindowCache = ReturnType<typeof createChatWindowCache>;

export const chatWindowCache: ChatWindowCache = createChatWindowCache();

// --- per-hub Code-view state (design/43 Phase B) -------------------------
// Lifting the Code view's file/tree/selection state out of CodeLens and
// into a shared, per-hub-keyed store is the prerequisite for splitting the
// old single "file list + code pane" component into two: CodeTree (the
// left-rail navigator) and CodeLens (the center code pane + gutter). Both
// read/write the SAME per-hub record, so clicking a file in the tree and
// seeing the code pane update (or vice versa) needs no prop-drilling or
// callback plumbing between them — just two components pointed at the same
// reactive object. Keyed per hub like `chatWindowCache` above: switching
// hubs and back restores the previously-loaded file list, expansion state,
// filter, and sort instantly instead of re-fetching and re-collapsing.
export interface CodeFileState {
  /** False until this hub's `/api/codefiles` fetch has resolved once. */
  loaded: boolean;
  files: CodeFile[];
  activeKey: string | null;
  /** Tree/dir node ids currently expanded — a `SvelteSet` so `.add`/`.delete`
   * are independently reactive without reassigning the whole set. */
  expanded: SvelteSet<string>;
  filter: string;
  sort: 'tree' | 'active';
  /** The sha CodeLens is actually rendering at (the newest ref's pinned
   * sha, or 'HEAD') — lives here too so the unified breadcrumb (built in
   * App.svelte from this same store) doesn't need a callback prop just to
   * learn what CodeLens decided to render. */
  codeSha: string;
  /** Set by a breadcrumb-segment click (or any other "reveal this node"
   * affordance); CodeTree's effect consumes it (expand ancestors + scroll)
   * and clears it. Selection-only — no routing implication. */
  pendingReveal: string | null;
}

function createCodeFileState(): CodeFileState {
  return {
    loaded: false,
    files: [],
    activeKey: null,
    expanded: new SvelteSet<string>(),
    filter: '',
    sort: 'tree',
    codeSha: 'HEAD',
    pendingReveal: null,
  };
}

function createCodeStateStore() {
  const perHub = new Map<string, CodeFileState>();
  const inFlight = new Map<string, Promise<void>>();

  function ensure(hubId: string): CodeFileState {
    const existing = perHub.get(hubId);
    if (existing) return existing;
    const created = $state(createCodeFileState());
    perHub.set(hubId, created);
    return created;
  }

  return {
    /** The reactive per-hub record — create it (empty, unloaded) on first
     * access so callers never have to null-check before reading `.files`. */
    forHub(hubId: string): CodeFileState {
      return ensure(hubId);
    },
    /** Fetches `/api/codefiles` once per hub (cache-hit skips the refetch,
     * same "instant revisit" contract as `hubDataCache`/`chatWindowCache`).
     * Concurrent callers (CodeTree + CodeLens both mount together on first
     * visit to Code view) share one in-flight request instead of firing two. */
    async load(hubId: string): Promise<void> {
      const s = ensure(hubId);
      if (s.loaded) return;
      let p = inFlight.get(hubId);
      if (!p) {
        p = api
          .getCodeFiles(hubId)
          .then((files) => {
            s.files = files;
            if (s.activeKey === null && files[0]) s.activeKey = fileKey(files[0]);
            s.expanded = new SvelteSet(defaultExpandedIds(buildTree(files), s.activeKey));
            s.loaded = true;
          })
          .finally(() => inFlight.delete(hubId));
        inFlight.set(hubId, p);
      }
      return p;
    },
    /** Marks a hub's file list stale (e.g. a live SSE message/presence event
     * for it) so the next `load()` call re-fetches — mirrors
     * `hubDataCache.invalidate`. Expansion/filter/sort/activeKey are left
     * alone; only `loaded` flips, so a live update doesn't reset the
     * reader's tree state out from under them. */
    invalidate(hubId: string): void {
      const s = perHub.get(hubId);
      if (s) s.loaded = false;
    },
    clear(): void {
      perHub.clear();
      inFlight.clear();
    },
  };
}

export type CodeStateStore = ReturnType<typeof createCodeStateStore>;

export const codeState: CodeStateStore = createCodeStateStore();

export type View = 'chat' | 'board' | 'fleet' | 'code' | 'repos';
export type Theme = 'dark' | 'light';

// Which off-canvas drawer is open on tablet/phone widths (≤1023px). Only one
// may be open at a time — opening one implicitly closes the other. Desktop
// (≥1024px) ignores this entirely; the tri-pane there is always fully visible.
export type Drawer = 'none' | 'left' | 'right';

// Chat stream density: 'summary' shows each note/message's one-liner
// (Message.summary) collapsed, expandable per-message to the full rendered
// body; 'full' is the pre-existing always-full-body behavior. Global toggle
// only sets the default collapsed/expanded state — an individually-expanded
// message stays expanded until the reader collapses it (see Message.svelte).
export type ChatDensity = 'summary' | 'full';

function createAppState() {
  // No hub/topic default here: these were mock fixtures (agent-coord/reader)
  // that only exist in mock.ts. Against the real API they're hydrated in
  // App.svelte's onMount from /api/hubs + /api/overview (see selectDefaultHub
  // / selectDefaultTopic below) — starting empty avoids a mismatched initial
  // fetch against a hub id that doesn't exist on the real backend.
  let hub = $state('');
  let view = $state<View>('chat');
  let topic = $state<string | null>(null);
  let selectedMessage = $state<Message | null>(null);
  let theme = $state<Theme>('dark');
  let drawer = $state<Drawer>('none');
  let chatDensity = $state<ChatDensity>('summary');

  return {
    get hub() {
      return hub;
    },
    set hub(value: string) {
      hub = value;
    },
    get view() {
      return view;
    },
    set view(value: View) {
      view = value;
    },
    get topic() {
      return topic;
    },
    set topic(value: string | null) {
      topic = value;
    },
    get selectedMessage() {
      return selectedMessage;
    },
    set selectedMessage(value: Message | null) {
      selectedMessage = value;
    },
    get theme() {
      return theme;
    },
    set theme(value: Theme) {
      theme = value;
      if (typeof document !== 'undefined') {
        document.documentElement.setAttribute('data-theme', value);
      }
    },
    toggleTheme() {
      this.theme = this.theme === 'dark' ? 'light' : 'dark';
    },
    get drawer() {
      return drawer;
    },
    set drawer(value: Drawer) {
      drawer = value;
    },
    /** Opens `which` drawer, closing any other — or closes it if it's already open. */
    toggleDrawer(which: Exclude<Drawer, 'none'>) {
      drawer = drawer === which ? 'none' : which;
    },
    closeDrawer() {
      drawer = 'none';
    },
    get chatDensity() {
      return chatDensity;
    },
    set chatDensity(value: ChatDensity) {
      chatDensity = value;
    },
  };
}

export const appState = createAppState();
