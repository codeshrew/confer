import { beforeEach, describe, expect, it, vi } from 'vitest';
import { appState, chatWindowCache, codeState, hubDataCache } from './stores.svelte';
import { api } from './api';
import type { CodeFile, Message, Overview } from './types';

vi.mock('./api', () => ({
  api: { getCodeFiles: vi.fn() },
}));

describe('appState.view', () => {
  it('defaults to overview — design/47 §3\'s new landing, not a hub Chat', () => {
    expect(appState.view).toBe('overview');
  });
});

describe('appState.drawer', () => {
  it('starts closed', () => {
    expect(appState.drawer).toBe('none');
  });

  it('toggleDrawer("left") opens the left drawer, and toggling it again closes it', () => {
    appState.drawer = 'none';

    appState.toggleDrawer('left');
    expect(appState.drawer).toBe('left');

    appState.toggleDrawer('left');
    expect(appState.drawer).toBe('none');
  });

  it('opening the right drawer while the left is open closes the left — only one at a time', () => {
    appState.drawer = 'none';

    appState.toggleDrawer('left');
    expect(appState.drawer).toBe('left');

    appState.toggleDrawer('right');
    expect(appState.drawer).toBe('right');
  });

  it('closeDrawer() closes whichever drawer is open', () => {
    appState.drawer = 'none';
    appState.toggleDrawer('right');
    expect(appState.drawer).toBe('right');

    appState.closeDrawer();
    expect(appState.drawer).toBe('none');
  });

  it('the drawer setter accepts direct assignment too', () => {
    appState.drawer = 'left';
    expect(appState.drawer).toBe('left');
    appState.drawer = 'none';
  });
});

describe('appState.theme', () => {
  it('defaults to dark', () => {
    expect(appState.theme).toBe('dark');
  });

  it('toggleTheme() flips dark <-> light', () => {
    appState.theme = 'dark';
    appState.toggleTheme();
    expect(appState.theme).toBe('light');

    appState.toggleTheme();
    expect(appState.theme).toBe('dark');
  });

  it('setting the theme mirrors it onto <html data-theme> so app.css can key off it', () => {
    appState.theme = 'light';
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');

    appState.theme = 'dark';
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
  });
});

describe('appState.chatDensity', () => {
  it('defaults to summary', () => {
    expect(appState.chatDensity).toBe('summary');
  });

  it('flips to full and back via direct assignment', () => {
    appState.chatDensity = 'full';
    expect(appState.chatDensity).toBe('full');

    appState.chatDensity = 'summary';
    expect(appState.chatDensity).toBe('summary');
  });
});

describe('hubDataCache', () => {
  function fixture(hub: string): { overview: Overview; messages: [] } {
    return {
      overview: {
        hub: { id: hub, label: hub, name: hub, current: true, agentCount: 1 },
        topics: [],
        board: { requests: [], open: 0, claimed: 0, blocked: 0, backlog: 0, closed: 0 },
        fleet: [],
      },
      messages: [],
    };
  }

  it('is empty for a hub that has never been set', () => {
    expect(hubDataCache.get('never-seen-hub')).toBeUndefined();
    expect(hubDataCache.has('never-seen-hub')).toBe(false);
  });

  it('set() then get() returns the same data — switching back to a cached hub renders instantly, no re-fetch', () => {
    const data = fixture('confer-lab');
    hubDataCache.set('confer-lab', data);

    expect(hubDataCache.has('confer-lab')).toBe(true);
    expect(hubDataCache.get('confer-lab')).toBe(data);
  });

  it('invalidate() drops just that hub entry — a live SSE event for one hub must not evict another', () => {
    hubDataCache.set('hub-a', fixture('hub-a'));
    hubDataCache.set('hub-b', fixture('hub-b'));

    hubDataCache.invalidate('hub-a');

    expect(hubDataCache.has('hub-a')).toBe(false);
    expect(hubDataCache.has('hub-b')).toBe(true);
  });

  it('clear() empties every entry', () => {
    hubDataCache.set('hub-x', fixture('hub-x'));
    hubDataCache.set('hub-y', fixture('hub-y'));
    expect(hubDataCache.size).toBeGreaterThanOrEqual(2);

    hubDataCache.clear();

    expect(hubDataCache.size).toBe(0);
    expect(hubDataCache.has('hub-x')).toBe(false);
  });
});

describe('chatWindowCache', () => {
  function msg(id: string, ts: string): Message {
    return {
      id,
      from: 'reader',
      type: 'note',
      ts,
      host: null,
      to: [],
      cc: [],
      topic: 'reader',
      summary: id,
      body: id,
      of: null,
      replyTo: null,
      supersedes: null,
      refs: [],
    };
  }

  it('is empty for a (hub, topic) that has never been set', () => {
    expect(chatWindowCache.get('never-seen', 'general')).toBeUndefined();
    expect(chatWindowCache.has('never-seen', 'general')).toBe(false);
  });

  it('set() then get() returns the same window, keyed by BOTH hub and topic', () => {
    const window = { messages: [msg('m1', '2026-07-17T10:00:00Z')], hasMore: true };
    chatWindowCache.set('confer-lab', 'reader', window);

    expect(chatWindowCache.get('confer-lab', 'reader')).toBe(window);
    // Same hub, different topic — no bleed-through.
    expect(chatWindowCache.get('confer-lab', 'studio')).toBeUndefined();
    // Different hub, same topic slug — also no bleed-through.
    expect(chatWindowCache.get('agent-coord', 'reader')).toBeUndefined();
  });

  it('clear() empties every entry', () => {
    chatWindowCache.set('hub-x', 'general', { messages: [], hasMore: false });
    chatWindowCache.set('hub-y', 'general', { messages: [], hasMore: false });
    expect(chatWindowCache.size).toBeGreaterThanOrEqual(2);

    chatWindowCache.clear();

    expect(chatWindowCache.size).toBe(0);
    expect(chatWindowCache.has('hub-x', 'general')).toBe(false);
  });
});

describe('codeState (design/43 Phase B — the shared CodeTree/CodeLens store)', () => {
  const files: CodeFile[] = [
    { repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', refCount: 3, mapped: true, lastTs: '2026-07-17T14:46:00Z' },
    { repo: 'wealdlore', path: 'pipeline/plates.py', refCount: 2, mapped: true, lastTs: '2026-07-17T14:52:00Z' },
  ];

  beforeEach(() => {
    vi.mocked(api.getCodeFiles).mockReset();
    codeState.clear();
  });

  it('forHub() creates an empty, unloaded record on first access', () => {
    const s = codeState.forHub('never-seen-hub');
    expect(s.loaded).toBe(false);
    expect(s.files).toEqual([]);
    expect(s.activeKey).toBeNull();
    expect(s.expanded.size).toBe(0);
    expect(s.filter).toBe('');
    expect(s.sort).toBe('tree');
    expect(s.codeSha).toBe('HEAD');
  });

  it('forHub() returns the SAME object on repeat calls — CodeTree and CodeLens share one record', () => {
    expect(codeState.forHub('hub-a')).toBe(codeState.forHub('hub-a'));
  });

  it('load() fetches once, populates files, and defaults activeKey to the first file', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(files);

    await codeState.load('agent-coord');

    const s = codeState.forHub('agent-coord');
    expect(s.loaded).toBe(true);
    expect(s.files).toEqual(files);
    expect(s.activeKey).toBe('wealdlore Sources/Reader/PlateBundle.swift');
    expect(api.getCodeFiles).toHaveBeenCalledTimes(1);
  });

  it('load() is a no-op once loaded — revisiting a hub is instant, no re-fetch', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(files);
    await codeState.load('agent-coord');
    await codeState.load('agent-coord');
    expect(api.getCodeFiles).toHaveBeenCalledTimes(1);
  });

  it('concurrent load() calls for the same unloaded hub share one in-flight fetch', async () => {
    let resolve!: (v: CodeFile[]) => void;
    vi.mocked(api.getCodeFiles).mockReturnValue(new Promise((res) => (resolve = res)));

    const p1 = codeState.load('agent-coord');
    const p2 = codeState.load('agent-coord');
    resolve(files);
    await Promise.all([p1, p2]);

    expect(api.getCodeFiles).toHaveBeenCalledTimes(1);
  });

  it('invalidate() flips loaded off (so the next load() refetches) without resetting expansion/filter/sort', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(files);
    await codeState.load('agent-coord');
    const s = codeState.forHub('agent-coord');
    s.filter = 'plates';
    s.sort = 'active';
    s.expanded.add('wealdlore');

    codeState.invalidate('agent-coord');
    expect(s.loaded).toBe(false);
    expect(s.filter).toBe('plates');
    expect(s.sort).toBe('active');
    expect(s.expanded.has('wealdlore')).toBe(true);

    vi.mocked(api.getCodeFiles).mockResolvedValue(files);
    await codeState.load('agent-coord');
    expect(api.getCodeFiles).toHaveBeenCalledTimes(2);
  });

  it('keeps two hubs completely separate', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(files);
    await codeState.load('hub-a');
    const otherFiles: CodeFile[] = [{ repo: 'r', path: 'x.rs', refCount: 1, mapped: true, lastTs: '2026-07-17T10:00:00Z' }];
    vi.mocked(api.getCodeFiles).mockResolvedValue(otherFiles);
    await codeState.load('hub-b');

    expect(codeState.forHub('hub-a').files).toEqual(files);
    expect(codeState.forHub('hub-b').files).toEqual(otherFiles);
  });
});
