// Svelte 5 runes-based app state. Small and typed — later agents extend
// this as more panes come online (chat stream selection, filters, etc).

import type { Message, Overview } from './types';

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
