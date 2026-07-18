// Typed HTTP client for confer serve's backend. Same-origin base URL, since
// the built dist/index.html is served by the confer binary itself.
//
// In dev (`import.meta.env.DEV`) or when the page URL carries a `?mock`
// query param, requests are routed to `mockApi` (./mock.ts) instead of
// `fetch` — the backend doesn't exist yet, so this lets the whole UI be
// built and tested standalone. Fetch failures are NOT swallowed: a broken
// endpoint should surface as a thrown error, not a silent fallback, so a
// real backend bug is never hidden behind "well, it 404'd, guess mock time".

import type { Hub, Message, Overview, RefHit, ServerEvent, Snippet, ThreadNode } from './types';
import { mockApi } from './mock';

const BASE_URL = '';

function useMock(): boolean {
  if (typeof window === 'undefined') return import.meta.env.DEV;
  const params = new URLSearchParams(window.location.search);
  return import.meta.env.DEV || params.has('mock');
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
  subscribeEvents(onEvent: (event: ServerEvent) => void): () => void;
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

  subscribeEvents(onEvent) {
    const source = new EventSource(`${BASE_URL}/api/events`);
    source.onmessage = (ev) => {
      try {
        onEvent(JSON.parse(ev.data) as ServerEvent);
      } catch (err) {
        // A malformed event from the server is a real bug — surface it
        // instead of silently dropping the message.
        console.error('confer serve: malformed SSE payload', ev.data, err);
      }
    };
    return () => source.close();
  },
};

export const api: ConferApi = useMock() ? mockApi : httpApi;
