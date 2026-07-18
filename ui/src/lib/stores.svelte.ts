// Svelte 5 runes-based app state. Small and typed — later agents extend
// this as more panes come online (chat stream selection, filters, etc).

import type { Message } from './types';

export type View = 'chat' | 'board' | 'fleet' | 'code';
export type Theme = 'dark' | 'light';

function createAppState() {
  let hub = $state('agent-coord');
  let view = $state<View>('chat');
  let topic = $state<string | null>('reader');
  let selectedMessage = $state<Message | null>(null);
  let theme = $state<Theme>('dark');

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
  };
}

export const appState = createAppState();
