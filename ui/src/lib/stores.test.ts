import { describe, expect, it } from 'vitest';
import { appState } from './stores.svelte';

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
