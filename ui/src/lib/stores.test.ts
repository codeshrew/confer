import { describe, expect, it } from 'vitest';
import { appState, hubDataCache } from './stores.svelte';
import type { Overview } from './types';

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
