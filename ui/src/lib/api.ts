// Typed HTTP client for confer serve's backend. Same-origin base URL, since
// the built dist/index.html is served by the confer binary itself.
//
// In dev (`import.meta.env.DEV`) or when the page URL carries a `?mock`
// query param, requests are routed to `mockApi` (./mock.ts) instead of
// `fetch` — the backend doesn't exist yet (or, in dev, isn't running), so
// this lets the whole UI be built and tested standalone. Fetch failures are
// NOT swallowed: a broken endpoint should surface as a thrown error, not a
// silent fallback, so a real backend bug is never hidden behind "well, it
// 404'd, guess mock time".
//
// Dev mode defaults to mock (existing Vitest/Playwright specs are written
// against mock.ts's fixtures and must keep passing unattended). To exercise
// a REAL `confer serve` backend from `npm run dev` instead — e.g. against
// the dev proxy configured in vite.config.ts — either load the page with a
// `?live` query param, or set `VITE_LIVE=1` when starting the dev server
// (`VITE_LIVE=1 npm run dev`). `?mock` always wins over `?live` if both are
// somehow present, and always wins outside dev too.

import type { Hub, Message, Overview, RefHit, ServerEvent, Snippet, ThreadNode } from './types';
import { mockApi } from './mock';

const BASE_URL = '';

function useMock(): boolean {
  if (typeof window !== 'undefined') {
    const params = new URLSearchParams(window.location.search);
    if (params.has('mock')) return true;
    if (params.has('live')) return false;
  }
  if (import.meta.env.VITE_LIVE) return false;
  return import.meta.env.DEV;
}

async function getJson<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE_URL}${path}`);
  if (!res.ok) {
    throw new Error(`GET ${path} failed: ${res.status} ${res.statusText}`);
  }
  return (await res.json()) as T;
}

export interface ConferApi {
  getHubs(): Promise<Hub[]>;
  getOverview(hub: string): Promise<Overview>;
  getMessages(hub: string, topic?: string): Promise<Message[]>;
  getThread(hub: string, id: string): Promise<ThreadNode[]>;
  getRefs(hub: string, target: string, allHubs?: boolean): Promise<RefHit[]>;
  getCode(hub: string, repo: string, path: string, sha: string, range?: string): Promise<Snippet>;
  /**
   * Opens a live-update channel scoped to `hub`. `onEvent` gets each parsed
   * `message`/`presence`/`ping` event; `onStatus` reports the connection's
   * own health ('live' once open, 'reconnecting' only on a real transport
   * error — never as a default/initial guess). Returns an unsubscribe fn.
   */
  subscribeEvents(hub: string, onEvent: (event: ServerEvent) => void, onStatus: (status: 'live' | 'reconnecting') => void): () => void;
}

const httpApi: ConferApi = {
  async getHubs() {
    return getJson<Hub[]>('/api/hubs');
  },

  async getOverview(hub) {
    const qs = new URLSearchParams({ hub });
    return getJson<Overview>(`/api/overview?${qs}`);
  },

  async getMessages(hub, topic) {
    const qs = new URLSearchParams({ hub });
    if (topic) qs.set('topic', topic);
    return getJson<Message[]>(`/api/messages?${qs}`);
  },

  async getThread(hub, id) {
    const qs = new URLSearchParams({ hub, id });
    return getJson<ThreadNode[]>(`/api/thread?${qs}`);
  },

  async getRefs(hub, target, allHubs) {
    const qs = new URLSearchParams({ hub, target });
    if (allHubs) qs.set('allHubs', 'true');
    return getJson<RefHit[]>(`/api/refs?${qs}`);
  },

  async getCode(hub, repo, path, sha, range) {
    const qs = new URLSearchParams({ hub, repo, path, sha });
    if (range) qs.set('range', range);
    return getJson<Snippet>(`/api/code?${qs}`);
  },

  subscribeEvents(hub, onEvent, onStatus) {
    const qs = new URLSearchParams({ hub });
    const source = new EventSource(`${BASE_URL}/api/events?${qs}`);
    // The connection is genuinely live once the browser has the response
    // headers — don't wait for the first data frame (the backend's SSE loop
    // only writes on a hub change or a ~30s keepalive, so a stream that's
    // healthy but quiet must not read as "reconnecting").
    source.onopen = () => onStatus('live');
    source.onmessage = (ev) => {
      try {
        onEvent(JSON.parse(ev.data) as ServerEvent);
        onStatus('live');
      } catch (err) {
        // A malformed event from the server is a real bug — surface it
        // instead of silently dropping the message.
        console.error('confer serve: malformed SSE payload', ev.data, err);
      }
    };
    // EventSource auto-retries on error; report the transient state but
    // don't tear anything down ourselves.
    source.onerror = () => onStatus('reconnecting');
    return () => source.close();
  },
};

export const api: ConferApi = useMock() ? mockApi : httpApi;
