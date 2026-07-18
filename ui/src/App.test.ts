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

describe('App — default landing (design/47 §3)', () => {
  it('a fresh load lands on Overview, not a hub Chat — no appState.view set beforehand', async () => {
    // Deliberately NOT setting appState.view here — this is the one place
    // that exercises the untouched module default (every other describe
    // block in this file explicitly pins its own view first).
    render(App);
    await screen.findByTestId('overview-view');
    expect(screen.queryByTestId('overview-view')).toBeInTheDocument();
  });
});

describe('App — responsive drawer structure', () => {
  // These tests exercise the drawer/scrim CHROME generically — not anything
  // Overview-specific — so they pin appState.view to 'chat' (Overview, per
  // design/47, has no left rail/right-rail-toggle at all, which would make
  // "open menu"/"right-drawer-toggle" not exist to click).
  it('resets to a known drawer state between tests', () => {
    appState.drawer = 'none';
    expect(appState.drawer).toBe('none');
  });

  it('renders the off-canvas scrim, left drawer, and right drawer, closed by default', () => {
    appState.drawer = 'none';
    appState.view = 'chat';
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
    appState.view = 'chat';
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
    appState.view = 'chat';
    const user = userEvent.setup();
    render(App);

    await user.click(screen.getByTestId('right-drawer-toggle'));
    expect(screen.getByTestId('right-drawer')).toHaveClass('open');

    await user.click(screen.getByTestId('right-drawer-close'));
    expect(screen.getByTestId('right-drawer')).not.toHaveClass('open');
  });

  it('opening one drawer closes the other — only one overlay open at a time', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    const user = userEvent.setup();
    render(App);

    await user.click(screen.getByTestId('right-drawer-toggle'));
    expect(screen.getByTestId('right-drawer')).toHaveClass('open');

    await user.click(screen.getByRole('button', { name: /open menu/i }));
    expect(screen.getByTestId('left-drawer')).toHaveClass('open');
    expect(screen.getByTestId('right-drawer')).not.toHaveClass('open');
  });
});

describe('App — FilterBar is Chat-only', () => {
  it('shows the FilterBar (Type + Density) in Chat view', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    render(App);

    await screen.findByText('Notes');
    expect(screen.getByText('Requests')).toBeInTheDocument();
    expect(screen.getByTestId('density-toggle')).toBeInTheDocument();
  });

  it('hides the FilterBar entirely in Board view (Board keeps its own group-by control)', async () => {
    appState.drawer = 'none';
    appState.view = 'board';
    render(App);

    // Give the view a tick to settle, then assert the FilterBar's Type
    // chips are gone — Board no longer gets a (dead) FilterBar at all.
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.queryByTestId('density-toggle')).not.toBeInTheDocument();
    expect(screen.queryByText('Requests')).not.toBeInTheDocument();
  });
});

describe('App — right-rail context mode', () => {
  it('selecting a plain note after a ticket switches the sidebar OFF "Request detail" — not stuck showing the previous ticket', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
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
