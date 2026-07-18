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

describe('App — right-rail context mode', () => {
  it('selecting a plain note after a ticket switches the sidebar OFF "Request detail" — not stuck showing the previous ticket', async () => {
    appState.drawer = 'none';
    const user = userEvent.setup();
    render(App);

    // First: a ticket — the mock fixture's filed request card.
    await user.click(
      await screen.findByText('Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader')
    );
    expect(screen.getByText('Request detail')).toBeInTheDocument();

    // Then: a plain note (Jarvis's "canaried 0.7.3", same #reader topic).
    await user.click(await screen.findByText(/canaried 0.7.3/));

    // The sidebar must have moved off "Request detail" — it's now the
    // meta-thread/reference-graph pane for the note just clicked, not stuck
    // showing the earlier ticket.
    expect(screen.queryByText('Request detail')).not.toBeInTheDocument();
    expect(screen.getByText('Meta-thread')).toBeInTheDocument();
  });
});
