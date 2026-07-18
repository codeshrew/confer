// Svelte 5 runes-based app state. Small and typed — later agents extend
// this as more panes come online (chat stream selection, filters, etc).

import type { Message } from './types';

export type View = 'chat' | 'board' | 'fleet' | 'code';
export type Theme = 'dark' | 'light';

// Which off-canvas drawer is open on tablet/phone widths (≤1023px). Only one
// may be open at a time — opening one implicitly closes the other. Desktop
// (≥1024px) ignores this entirely; the tri-pane there is always fully visible.
export type Drawer = 'none' | 'left' | 'right';

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
  };
}

export const appState = createAppState();
