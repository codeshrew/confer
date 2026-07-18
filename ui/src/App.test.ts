import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import App from './App.svelte';
import { appState } from './lib/stores.svelte';

// jsdom has no real layout/viewport, so these tests exercise the responsive
// *structure* (the drawer/scrim elements exist, and their open/closed state
// is driven by appState.drawer) rather than anything computed-width based —
// the actual show/hide at each breakpoint is pure CSS (see App.svelte's
// media queries), which jsdom can't evaluate.

describe('App — responsive drawer structure', () => {
  it('resets to a known drawer state between tests', () => {
    appState.drawer = 'none';
    expect(appState.drawer).toBe('none');
  });

  it('renders the off-canvas scrim, left drawer, and right drawer, closed by default', () => {
    appState.drawer = 'none';
    render(App);

    const scrim = screen.getByTestId('drawer-scrim');
    const leftDrawer = screen.getByTestId('left-drawer');
    const rightDrawer = screen.getByTestId('right-drawer');

    expect(scrim).not.toHaveClass('show');
    expect(leftDrawer).not.toHaveClass('open');
    expect(rightDrawer).not.toHaveClass('open');
  });

  it('the TopBar hamburger opens the left drawer, and the scrim then closes it', async () => {
    appState.drawer = 'none';
    const user = userEvent.setup();
    render(App);

    await user.click(screen.getByRole('button', { name: /open menu/i }));
    expect(screen.getByTestId('left-drawer')).toHaveClass('open');
    expect(screen.getByTestId('drawer-scrim')).toHaveClass('show');

    await user.click(screen.getByTestId('drawer-scrim'));
    expect(screen.getByTestId('left-drawer')).not.toHaveClass('open');
    expect(screen.getByTestId('drawer-scrim')).not.toHaveClass('show');
  });

  it('the right-drawer toggle in the crumb opens the right drawer, and its own close button closes it', async () => {
    appState.drawer = 'none';
    const user = userEvent.setup();
    render(App);

    await user.click(screen.getByTestId('right-drawer-toggle'));
    expect(screen.getByTestId('right-drawer')).toHaveClass('open');

    await user.click(screen.getByTestId('right-drawer-close'));
    expect(screen.getByTestId('right-drawer')).not.toHaveClass('open');
  });

  it('opening one drawer closes the other — only one overlay open at a time', async () => {
    appState.drawer = 'none';
    const user = userEvent.setup();
    render(App);

    await user.click(screen.getByTestId('right-drawer-toggle'));
    expect(screen.getByTestId('right-drawer')).toHaveClass('open');

    await user.click(screen.getByRole('button', { name: /open menu/i }));
    expect(screen.getByTestId('left-drawer')).toHaveClass('open');
    expect(screen.getByTestId('right-drawer')).not.toHaveClass('open');
  });
});
