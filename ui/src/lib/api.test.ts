// Tests for the real HTTP client (httpApi) that api.ts switches to outside
// mock mode, plus the useMock() routing logic itself. `api.ts` picks its
// implementation once, at module-eval time, based on the page URL and
// import.meta.env — so getting at httpApi (rather than the mockApi every
// other spec in this repo exercises) means: set window.location.search
// BEFORE importing, via vi.resetModules() + a fresh dynamic import.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

async function freshApiForcedLive() {
  // '?live' beats DEV unconditionally (see api.ts's useMock()), so this
  // reliably selects httpApi regardless of how the test runner's own
  // import.meta.env.DEV happens to be set.
  window.history.replaceState(null, '', '/?live');
  vi.resetModules();
  const mod = await import('./api');
  return mod.api;
}

async function freshApiForcedMock(search: string) {
  window.history.replaceState(null, '', `/${search}`);
  vi.resetModules();
  const mod = await import('./api');
  return mod.api;
}

describe('useMock() routing (via the exported `api` singleton)', () => {
  afterEach(() => {
    window.history.replaceState(null, '', '/');
  });

  it('?live forces the real HTTP client even though the test runner is in DEV mode', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify([]), { status: 200 })
    );
    const api = await freshApiForcedLive();

    await api.getHubs();
    expect(fetchSpy).toHaveBeenCalledWith('/api/hubs');
    fetchSpy.mockRestore();
  });

  it('?mock always wins over ?live if both are present', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const api = await freshApiForcedMock('?mock&live');

    const hubs = await api.getHubs();
    // mockApi never calls fetch; if this were httpApi it would.
    expect(fetchSpy).not.toHaveBeenCalled();
    expect(Array.isArray(hubs)).toBe(true);
    expect(hubs.length).toBeGreaterThan(0);
    fetchSpy.mockRestore();
  });

  it('with no query params at all, dev mode (the test runner) defaults to mock — no fetch happens', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch');
    const api = await freshApiForcedMock('');

    await api.getHubs();
    expect(fetchSpy).not.toHaveBeenCalled();
    fetchSpy.mockRestore();
  });
});

describe('httpApi URL construction (forced via ?live)', () => {
  let fetchSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementation(async () => new Response(JSON.stringify({}), { status: 200 }));
  });

  afterEach(() => {
    fetchSpy.mockRestore();
    window.history.replaceState(null, '', '/');
  });

  it('getHubs hits /api/hubs with no query string', async () => {
    const api = await freshApiForcedLive();
    await api.getHubs();
    expect(fetchSpy).toHaveBeenCalledWith('/api/hubs');
  });

  it('getOverview encodes the hub id', async () => {
    const api = await freshApiForcedLive();
    await api.getOverview('codeshrew/agent-coord');
    expect(fetchSpy).toHaveBeenCalledWith('/api/overview?hub=codeshrew%2Fagent-coord');
  });

  it('getMessages omits the topic param when not given, includes it when given', async () => {
    const api = await freshApiForcedLive();
    await api.getMessages('agent-coord');
    expect(fetchSpy).toHaveBeenLastCalledWith('/api/messages?hub=agent-coord');

    await api.getMessages('agent-coord', 'reader');
    expect(fetchSpy).toHaveBeenLastCalledWith('/api/messages?hub=agent-coord&topic=reader');
  });

  it('getMessages passes limit and before through when given, omits them entirely otherwise', async () => {
    const api = await freshApiForcedLive();
    await api.getMessages('agent-coord', 'reader', { limit: 50 });
    expect(fetchSpy).toHaveBeenLastCalledWith('/api/messages?hub=agent-coord&topic=reader&limit=50');

    await api.getMessages('agent-coord', 'reader', { limit: 50, before: 'msg_01JQ001' });
    expect(fetchSpy).toHaveBeenLastCalledWith(
      '/api/messages?hub=agent-coord&topic=reader&limit=50&before=msg_01JQ001'
    );

    await api.getMessages('agent-coord', 'reader', {});
    expect(fetchSpy).toHaveBeenLastCalledWith('/api/messages?hub=agent-coord&topic=reader');
  });

  it('getThread includes both hub and id', async () => {
    const api = await freshApiForcedLive();
    await api.getThread('agent-coord', 'msg_01JQ8f2');
    expect(fetchSpy).toHaveBeenLastCalledWith('/api/thread?hub=agent-coord&id=msg_01JQ8f2');
  });

  it('getRefs omits allHubs unless explicitly true, and never sends allHubs=false', async () => {
    const api = await freshApiForcedLive();
    await api.getRefs('agent-coord', 'wealdlore:Foo.swift');
    expect(fetchSpy).toHaveBeenLastCalledWith('/api/refs?hub=agent-coord&target=wealdlore%3AFoo.swift');

    await api.getRefs('agent-coord', 'wealdlore:Foo.swift', false);
    expect(fetchSpy).toHaveBeenLastCalledWith('/api/refs?hub=agent-coord&target=wealdlore%3AFoo.swift');

    await api.getRefs('agent-coord', 'wealdlore:Foo.swift', true);
    expect(fetchSpy).toHaveBeenLastCalledWith('/api/refs?hub=agent-coord&target=wealdlore%3AFoo.swift&allHubs=true');
  });

  it('getCode omits range unless given', async () => {
    const api = await freshApiForcedLive();
    await api.getCode('agent-coord', 'wealdlore', 'Sources/Foo.swift', 'a3f1c9');
    expect(fetchSpy).toHaveBeenLastCalledWith(
      '/api/code?hub=agent-coord&repo=wealdlore&path=Sources%2FFoo.swift&sha=a3f1c9'
    );

    await api.getCode('agent-coord', 'wealdlore', 'Sources/Foo.swift', 'a3f1c9', '44-49');
    expect(fetchSpy).toHaveBeenLastCalledWith(
      '/api/code?hub=agent-coord&repo=wealdlore&path=Sources%2FFoo.swift&sha=a3f1c9&range=44-49'
    );
  });

  it('getRepos hits /api/repos with the hub', async () => {
    const api = await freshApiForcedLive();
    await api.getRepos('agent-coord');
    expect(fetchSpy).toHaveBeenLastCalledWith('/api/repos?hub=agent-coord');
  });

  it('getCodeFiles hits /api/codefiles with the hub', async () => {
    const api = await freshApiForcedLive();
    await api.getCodeFiles('agent-coord');
    expect(fetchSpy).toHaveBeenLastCalledWith('/api/codefiles?hub=agent-coord');
  });
});

describe('httpApi error handling — a failed request must throw, never resolve to undefined/null', () => {
  afterEach(() => {
    window.history.replaceState(null, '', '/');
  });

  it('a non-ok response (e.g. 404, 500) throws with the status in the message', async () => {
    const fetchSpy = vi
      .spyOn(globalThis, 'fetch')
      .mockImplementation(async () => new Response('not found', { status: 404, statusText: 'Not Found' }));
    const api = await freshApiForcedLive();

    await expect(api.getHubs()).rejects.toThrow(/404/);
    await expect(api.getHubs()).rejects.toThrow(/Not Found/);
    fetchSpy.mockRestore();
  });

  it('a network-level failure (fetch itself rejects) propagates, not swallowed into a fallback', async () => {
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValue(new TypeError('Failed to fetch'));
    const api = await freshApiForcedLive();

    await expect(api.getOverview('agent-coord')).rejects.toThrow('Failed to fetch');
    fetchSpy.mockRestore();
  });
});

// --- subscribeEvents (SSE) --------------------------------------------------
// EventSource doesn't exist in jsdom, so these tests install a small fake
// that captures the handlers httpApi wires up (onopen/onmessage/onerror)
// and lets the test drive them directly.
class FakeEventSource {
  static instances: FakeEventSource[] = [];
  url: string;
  onopen: (() => void) | null = null;
  onmessage: ((ev: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;

  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }

  close() {
    this.closed = true;
  }
}

describe('httpApi.subscribeEvents (SSE)', () => {
  let originalEventSource: unknown;

  beforeEach(() => {
    originalEventSource = (globalThis as { EventSource?: unknown }).EventSource;
    FakeEventSource.instances = [];
    (globalThis as { EventSource?: unknown }).EventSource = FakeEventSource;
  });

  afterEach(() => {
    (globalThis as { EventSource?: unknown }).EventSource = originalEventSource;
    window.history.replaceState(null, '', '/');
  });

  it('opens an EventSource scoped to the given hub', async () => {
    const api = await freshApiForcedLive();
    api.subscribeEvents('agent-coord', vi.fn(), vi.fn());

    expect(FakeEventSource.instances).toHaveLength(1);
    expect(FakeEventSource.instances[0]!.url).toBe('/api/events?hub=agent-coord');
  });

  it('reports "live" as soon as the connection opens — not before, and not merely by default', async () => {
    const api = await freshApiForcedLive();
    const onStatus = vi.fn();
    api.subscribeEvents('agent-coord', vi.fn(), onStatus);

    expect(onStatus).not.toHaveBeenCalled();
    FakeEventSource.instances[0]!.onopen?.();
    expect(onStatus).toHaveBeenCalledWith('live');
  });

  it('reports "reconnecting" only on a real transport error', async () => {
    const api = await freshApiForcedLive();
    const onStatus = vi.fn();
    api.subscribeEvents('agent-coord', vi.fn(), onStatus);

    FakeEventSource.instances[0]!.onerror?.();
    expect(onStatus).toHaveBeenCalledWith('reconnecting');
  });

  it('dispatches a well-formed event to onEvent and marks the connection live again', async () => {
    const api = await freshApiForcedLive();
    const onEvent = vi.fn();
    const onStatus = vi.fn();
    api.subscribeEvents('agent-coord', onEvent, onStatus);

    const payload = { event: 'message', hub: 'agent-coord', topic: 'reader' };
    FakeEventSource.instances[0]!.onmessage?.({ data: JSON.stringify(payload) });

    expect(onEvent).toHaveBeenCalledWith(payload);
    expect(onStatus).toHaveBeenCalledWith('live');
  });

  it('a malformed (non-JSON) event payload is logged, not dispatched, and does not throw', async () => {
    const api = await freshApiForcedLive();
    const onEvent = vi.fn();
    const onStatus = vi.fn();
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    api.subscribeEvents('agent-coord', onEvent, onStatus);

    expect(() => {
      FakeEventSource.instances[0]!.onmessage?.({ data: 'not json {' });
    }).not.toThrow();

    expect(onEvent).not.toHaveBeenCalled();
    expect(errorSpy).toHaveBeenCalled();
    errorSpy.mockRestore();
  });

  it('the returned unsubscribe function closes the EventSource', async () => {
    const api = await freshApiForcedLive();
    const unsubscribe = api.subscribeEvents('agent-coord', vi.fn(), vi.fn());

    expect(FakeEventSource.instances[0]!.closed).toBe(false);
    unsubscribe();
    expect(FakeEventSource.instances[0]!.closed).toBe(true);
  });
});
