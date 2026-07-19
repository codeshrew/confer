import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import App from './App.svelte';
import { appState, codeState } from './lib/stores.svelte';
import { overlayStack } from './lib/overlayStack.svelte';
import { fileKey } from './lib/codeTree';

// jsdom has no real layout/viewport, so these tests exercise the responsive
// *structure* (the drawer/scrim elements exist, and their open/closed state
// is driven by appState.drawer) rather than anything computed-width based —
// the actual show/hide at each breakpoint is pure CSS (see App.svelte's
// media queries), which jsdom can't evaluate.

// `overlayStack` (piece 10 Phase A) is a module singleton, like `appState` —
// a test that opens a dossier/ticket/note and doesn't explicitly esc back
// out (several pre-existing tests below don't, since closing wasn't their
// point) would otherwise leak a frame into the NEXT test's stack, throwing
// off any depth-dependent assertion (same gotcha boardFilter.test.ts/
// readState.test.ts already guard their own singletons against).
beforeEach(() => {
  overlayStack.clear();
  // Piece 11 Phase 4 — same singleton-leak concern: a Code-view test that
  // sets `activeKey` (below) would otherwise leave it pointed at whatever
  // file it last selected for the next test that touches Code.
  codeState.clear();
});

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

  it('hides the FilterBar entirely in Board view (Board is its own cockpit, not a chat filter target)', async () => {
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

// TopBar's hub-pill row is still in the DOM below 1024px (see TopBar.svelte)
// — jsdom has no real viewport, so BOTH it and HubRail render simultaneously
// here, and a plain screen.getByText('agent-coord') is ambiguous between
// them. Every test below that needs to find/click a hub scopes to HubRail
// specifically via this helper.
async function findHubRail() {
  return within(await screen.findByTestId('hub-rail'));
}

describe('App — keyboard-architecture pass: Layer 3 (Cmd+number), ? which-key overlay', () => {
  it('"?" opens the which-key overlay; its own Escape handling closes it', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = ''; // fresh-app default — see stores.svelte.ts
    const user = userEvent.setup();
    render(App);
    // Wait for HubRail's own fetch to land — avoids a flaky race where a
    // keypress fires before anything meaningful is mounted.
    await findHubRail();

    expect(screen.queryByTestId('whichkey-backdrop')).not.toBeInTheDocument();
    await user.keyboard('?');
    expect(await screen.findByTestId('whichkey-backdrop')).toBeInTheDocument();

    await user.keyboard('{Escape}');
    expect(screen.queryByTestId('whichkey-backdrop')).not.toBeInTheDocument();
  });

  it('the TopBar "?" button also opens it — mouse parity, not keyboard-only', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);
    await findHubRail();

    expect(screen.queryByTestId('whichkey-backdrop')).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Keyboard shortcuts' }));
    expect(await screen.findByTestId('whichkey-backdrop')).toBeInTheDocument();
  });

  it('Cmd+3 switches to Board (the g-leader is retired — views are Cmd+number now)', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);
    await findHubRail();

    await user.keyboard('{Meta>}3{/Meta}');
    expect(appState.view).toBe('board');
  });

  it('Cmd+4 switches to Fleet', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);
    await findHubRail();

    await user.keyboard('{Meta>}4{/Meta}');
    expect(appState.view).toBe('fleet');
  });

  it('plain "g" (no modifier) does nothing — the g-leader is retired, not aliased', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);
    await findHubRail();

    await user.keyboard('g3');
    expect(appState.view).toBe('chat');
  });

  it('Ctrl+3 does NOT switch views — Ctrl+number is reserved for browser tab-switching, deliberately not a Layer-3 alias', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);
    await findHubRail();

    await user.keyboard('{Control>}3{/Control}');
    expect(appState.view).toBe('chat');
  });

  it('never fires while typing — Cmd+3 typed while the ⌘K palette search has focus does not switch views', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);
    await findHubRail();

    await user.keyboard('{Meta>}k{/Meta}');
    const input = await screen.findByTestId('palette-input');
    input.focus();
    await user.keyboard('{Meta>}3{/Meta}');

    expect(appState.view).toBe('chat');
  });
});

describe('App — keyboard-architecture pass: Layer 1 (Ctrl pane focus)', () => {
  it('shows a persistent focus chip naming the currently-focused pane', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    render(App);
    await findHubRail();

    expect(await screen.findByTestId('focus-chip')).toBeInTheDocument();
  });

  it('clicking a pane focuses it — the chip updates to name it', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);
    const rail = await findHubRail();

    await user.click(await rail.findByText('agent-coord'));
    await waitFor(() => expect(screen.getByTestId('focus-chip')).toHaveTextContent('Hubs'));
  });

  it('Ctrl+] cycles pane focus forward through the registered panes', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);
    const rail = await findHubRail();
    await user.click(await rail.findByText('agent-coord'));
    await waitFor(() => expect(screen.getByTestId('focus-chip')).toHaveTextContent('Hubs'));

    await user.keyboard('{Control>}]{/Control}');
    await waitFor(() => expect(screen.getByTestId('focus-chip')).not.toHaveTextContent('Hubs'));
  });

  it('F6 also cycles pane focus — the browser-safe fallback for reserved Ctrl+hjkl chords', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);
    const rail = await findHubRail();
    await user.click(await rail.findByText('agent-coord'));
    await waitFor(() => expect(screen.getByTestId('focus-chip')).toHaveTextContent('Hubs'));

    await user.keyboard('{F6}');
    await waitFor(() => expect(screen.getByTestId('focus-chip')).not.toHaveTextContent('Hubs'));
  });
});

describe('App — keyboard-architecture pass, item 0 bug fix: pane focus must not leak from stream into the peek', () => {
  it('clicking a stream message opens the peek (a content-sync side effect), but the active pane stays "stream" — j/k×3 keeps moving the stream selection, not the trail', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);
    await findHubRail();

    // Clicking a message both selects it AND is a genuine trusted click —
    // real focus lands on the row (inside .stream), and per "clicking a
    // pane focuses it" the active pane becomes "stream". This ALSO opens
    // the peek (MetaThread mounts) as a side effect of selection — exactly
    // the scenario that used to silently steal focus (MetaThread's old
    // roving-row-focus effect firing unconditionally on mount).
    const noteText = /canaried 0.7.3/;
    await user.click(await screen.findByText(noteText));
    await screen.findByTestId('thread-peek');
    await waitFor(() => expect(screen.getByTestId('focus-chip')).toHaveTextContent('Chat stream'));

    const firstSelected = appState.selectedMessage?.id;
    await user.keyboard('j');
    // Still "stream" — the peek update from the new selection (each j
    // press re-opens/updates the peek on the newly-selected message) must
    // not have flipped the active pane. This is THE regression: before the
    // fix, this first `j` was exactly where focus silently jumped away.
    expect(screen.getByTestId('focus-chip')).toHaveTextContent('Chat stream');

    await user.keyboard('j');
    await user.keyboard('j');
    // The pane held for all three presses — not just the first one.
    expect(screen.getByTestId('focus-chip')).toHaveTextContent('Chat stream');

    // And it's genuinely the STREAM's own bare-key vocab firing (not the
    // peek's j/k, which would move the trail instead): the clicked note is
    // the last message in the topic, so `j` clamped at the end — `k` (still
    // within the stream's own keydown, still real focus never having left
    // it) proves the selection actually moves, not just that the chip text
    // didn't change.
    await user.keyboard('k');
    expect(appState.selectedMessage?.id).not.toBe(firstSelected);
    expect(screen.getByTestId('focus-chip')).toHaveTextContent('Chat stream');
  });
});

describe('App — piece 2 workspace tint: the active hub\'s real tier', () => {
  it('tints the workspace "home" for the own-tier default hub, with no world-pill (home is the silent default)', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    render(App);

    await findHubRail();
    const center = document.querySelector('.center')!;
    await waitFor(() => expect(center).toHaveClass('tint-home'));
    expect(screen.queryByTestId('world-pill')).not.toBeInTheDocument();
  });

  it('switching to the foreign hub (via HubRail) tints the workspace foreign and shows the world-pill', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);

    const rail = await findHubRail();
    await user.click(await rail.findByText('jarvis-orbit'));

    const center = document.querySelector('.center')!;
    await waitFor(() => expect(center).toHaveClass('tint-foreign'));
    expect(await screen.findByTestId('world-pill')).toHaveTextContent('foreign hub');

    // Leave the shared appState.hub singleton as later tests in this file
    // expect it (unset — resolved fresh by whichever test runs next).
    appState.hub = '';
  });

  it('Overview never tints — it has no single current hub', async () => {
    appState.drawer = 'none';
    appState.view = 'overview';
    appState.hub = '';
    render(App);
    await screen.findByTestId('overview-view');

    const center = document.querySelector('.center')!;
    expect(center).not.toHaveClass('tint-home');
    expect(center).not.toHaveClass('tint-foreign');
    expect(center).not.toHaveClass('tint-neutral');
  });
});

describe('App — piece 3: side-peek preserves the stream, Esc closes it', () => {
  // A plain note (not a ticket) opens the meta-thread PEEK — same as a
  // ticket does now (piece 5 retired the bespoke "Request detail" mode; a
  // ticket ALSO opens the Full popover overlay, see the "right-rail context
  // mode" describe block below). Reuses the same fixture note that block
  // already relies on.
  const noteText = /canaried 0.7.3/;

  it('opening a peek does not remove the stream from the DOM — it stays mounted, untouched, behind the peek', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);

    await user.click(await screen.findByText(noteText));

    // The peek opened (meta-thread pane) AND the exact same stream row is
    // still right there in the DOM — this is the whole point of a
    // side-peek over the old page-swap.
    expect(await screen.findByTestId('thread-peek')).toBeInTheDocument();
    expect(screen.getByText(noteText)).toBeInTheDocument();
  });

  it('Escape in the peek closes it back to the empty state, without touching the stream', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);

    await user.click(await screen.findByText(noteText));
    const peek = await screen.findByTestId('thread-peek');
    peek.focus();

    await user.keyboard('{Escape}');

    expect(screen.queryByTestId('thread-peek')).not.toBeInTheDocument();
    expect(screen.getByText('Select a message to trace its thread')).toBeInTheDocument();
    // The stream itself never moved.
    expect(screen.getByText(noteText)).toBeInTheDocument();
  });
});

describe('App — piece 3: the focus reader, reachable from anywhere', () => {
  it('"f" does nothing when nothing is focused yet (a fresh Chat load, no click)', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);
    await screen.findByTestId('hub-rail');

    await user.keyboard('f');
    expect(screen.queryByTestId('focus-reader')).not.toBeInTheDocument();
  });

  it('"f" opens the focus reader on the currently-selected Chat message, and "f" again closes it', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);

    const ticketText = 'Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader';
    await user.click(await screen.findByText(ticketText));

    await user.keyboard('f');
    expect(await screen.findByTestId('focus-reader')).toBeInTheDocument();

    await user.keyboard('f');
    expect(screen.queryByTestId('focus-reader')).not.toBeInTheDocument();
  });

  it('"f" from a selected Board ticket also opens the reader — selectBoardRow sets the same focus appState.selectedMessage does', async () => {
    appState.drawer = 'none';
    appState.view = 'board';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);

    // A live (BLOCKED) ticket — piece 5's cockpit collapses DONE tickets
    // behind a fold by default, so a DONE fixture wouldn't be in the DOM
    // to click here at all.
    const ticketText = 'Freeze the CSL schema — needs a decision from Herald';
    await user.click(await screen.findByText(ticketText));

    await user.keyboard('f');
    expect(await screen.findByTestId('focus-reader')).toBeInTheDocument();
  });
});

describe('App — right-rail context mode', () => {
  it('selecting a plain note after a ticket switches the sidebar to that note\'s own thread — not stuck showing the previous ticket', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    const user = userEvent.setup();
    render(App);

    // First: a ticket — the mock fixture's filed request card (its Mini
    // card, embedded inline in the stream). Selecting it opens the Full
    // popover AND puts the right rail on the SAME meta-thread pane a plain
    // click would (piece 5 retired the bespoke "Request detail" mode).
    await user.click(
      await screen.findByText('Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader')
    );
    expect(await screen.findByTestId('ticket-popover')).toBeInTheDocument();
    expect(screen.getByText('Meta-thread')).toBeInTheDocument();

    // The popover's backdrop covers the stream — close it (Esc) before the
    // next click can reach a different row underneath, same as any modal.
    await user.keyboard('{Escape}');
    expect(screen.queryByTestId('ticket-popover')).not.toBeInTheDocument();

    // Then: a plain note (Jarvis's "canaried 0.7.3", same #reader topic).
    await user.click(await screen.findByText(/canaried 0.7.3/));

    // Still the meta-thread pane, but now for the note just clicked, not
    // stuck showing the earlier ticket's thread.
    expect(screen.queryByTestId('ticket-popover')).not.toBeInTheDocument();
    expect(screen.getByText('Meta-thread')).toBeInTheDocument();
  });
});

describe('App — piece 8b: the agent dossier, reachable from multiple entry points', () => {
  it('clicking an Overview AgentNode opens the dossier WITHOUT navigating away — esc returns to the exact same place', async () => {
    appState.drawer = 'none';
    appState.view = 'overview';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);

    const nodes = await screen.findAllByTestId('agent-node');
    await user.click(nodes[0]!);

    expect(await screen.findByTestId('agent-dossier')).toBeInTheDocument();
    // Never navigated — still on Overview, not forced into the Fleet tab.
    expect(appState.view).toBe('overview');
    expect(screen.getByTestId('overview-view')).toBeInTheDocument();

    await user.keyboard('{Escape}');
    expect(screen.queryByTestId('agent-dossier')).not.toBeInTheDocument();
    expect(appState.view).toBe('overview');
  });

  it('the Fleet deck also opens the dossier — a second real entry point for the same popover', async () => {
    appState.drawer = 'none';
    appState.view = 'fleet';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);

    const cards = await screen.findAllByTestId('fleet-presence-card');
    await user.click(cards[0]!);

    expect(await screen.findByTestId('agent-dossier')).toBeInTheDocument();
  });
});

describe('App — piece 10 Phase A: the overlay stack fixes the dossier→ticket back-nav bug', () => {
  // The exact reported repro: Fleet → click an agent (dossier opens) →
  // click one of their tickets (used to REPLACE the dossier — `esc` landed
  // nowhere). Pipeline carries `req_01JQa91` in the mock fixtures.
  async function openPipelineDossierAndItsTicket(user: ReturnType<typeof userEvent.setup>) {
    const cards = await screen.findAllByTestId('fleet-presence-card');
    const pipelineCard = cards.find((c) => within(c).queryByText('Pipeline'));
    if (!pipelineCard) throw new Error('Pipeline fleet-presence-card not found in mock fixtures');
    await user.click(pipelineCard);
    const dossier = await screen.findByTestId('agent-dossier');

    // Scoped to the dossier itself — Overview's own cross-hub worklist
    // (kept mounted underneath, just CSS-hidden, per App.svelte's
    // keep-alive-pane architecture) shows the SAME real ticket summary
    // text, which would otherwise make an unscoped query ambiguous.
    await user.click(within(dossier).getByText('Run the alignment pass over the restored plate set'));
    await screen.findByTestId('ticket-popover');
  }

  it('opening a ticket FROM the dossier pushes it on top — the dossier survives underneath, and esc returns to it', async () => {
    appState.drawer = 'none';
    appState.view = 'fleet';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);

    await openPipelineDossierAndItsTicket(user);
    // Only the TOP of the stack ever renders (the ticket now covers the
    // dossier, same as a real card stack — showing both at once would look
    // like two overlapping modals). The dossier's CONTEXT survives in
    // `overlayStack`, not its DOM presence — proven by esc bringing it
    // back below, not by it staying visible underneath.
    expect(screen.queryByTestId('agent-dossier')).not.toBeInTheDocument();

    // esc pops exactly ONE layer — the ticket closes, the dossier reappears.
    await user.keyboard('{Escape}');
    expect(screen.queryByTestId('ticket-popover')).not.toBeInTheDocument();
    expect(await screen.findByTestId('agent-dossier')).toBeInTheDocument();

    // A second esc closes the dossier too — back to nothing, as normal for
    // a top-level (non-nested) popover.
    await user.keyboard('{Escape}');
    expect(screen.queryByTestId('agent-dossier')).not.toBeInTheDocument();
  });

  it('the nested ticket popover shows a "‹ back" affordance, and it pops exactly the same as esc', async () => {
    appState.drawer = 'none';
    appState.view = 'fleet';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);

    await openPipelineDossierAndItsTicket(user);
    const ticketPopover = screen.getByTestId('ticket-popover');
    const back = within(ticketPopover).getByRole('button', { name: 'Back' });

    await user.click(back);
    expect(screen.queryByTestId('ticket-popover')).not.toBeInTheDocument();
    expect(await screen.findByTestId('agent-dossier')).toBeInTheDocument();
  });

  it('a TOP-LEVEL ticket open (no dossier involved) shows NO back affordance — it is not nested', async () => {
    appState.drawer = 'none';
    appState.view = 'chat';
    const user = userEvent.setup();
    render(App);

    await user.click(await screen.findByText('Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader'));
    const ticketPopover = await screen.findByTestId('ticket-popover');
    expect(within(ticketPopover).queryByRole('button', { name: 'Back' })).not.toBeInTheDocument();
  });

  it('j/k navigating between tickets while nested under the dossier stays nested — replace, not push', async () => {
    appState.drawer = 'none';
    appState.view = 'fleet';
    appState.hub = '';
    const user = userEvent.setup();
    render(App);

    await openPipelineDossierAndItsTicket(user);
    // j/k inside the ticket popover swaps content in place — still exactly
    // one layer nested under the dossier, not a second ticket frame pushed
    // on top of the first.
    await user.keyboard('k');
    expect(screen.getByTestId('ticket-popover')).toBeInTheDocument();

    await user.keyboard('{Escape}');
    // A SINGLE esc (not two) must already be back at the dossier — proof
    // j/k didn't grow the stack.
    expect(screen.queryByTestId('ticket-popover')).not.toBeInTheDocument();
    expect(await screen.findByTestId('agent-dossier')).toBeInTheDocument();
  });
});

describe('App — piece 11 Phase 4: the revision orientation chip', () => {
  it('pinned (amber): the default file has real hits, so the crumb shows the pinned sha + real ref/date, and a disabled compare-to-HEAD stub', async () => {
    appState.drawer = 'none';
    appState.view = 'code';
    appState.hub = '';
    render(App);

    // The chip renders immediately at the store's initial `codeSha: 'HEAD'`
    // default, then flips once the file's real refs finish loading — wait
    // for the settled state rather than the first paint.
    await screen.findByTestId('rev-chip');
    await waitFor(() => expect(screen.getByTestId('rev-chip')).toHaveClass('pinned'));
    const chip = screen.getByTestId('rev-chip');
    expect(chip).not.toHaveClass('head');
    expect(chip).toHaveTextContent('◷');
    expect(chip).toHaveTextContent('@a3f1c9'); // the mock's real pinned sha, unpadded (6 chars < the 10-char slice)

    expect(await screen.findByTestId('code-header-refname')).toHaveTextContent('main');
    expect(screen.getByTestId('code-header-date')).toBeInTheDocument();

    const compare = screen.getByTestId('compare-to-head');
    expect(compare).toBeDisabled();
    expect(compare).toHaveTextContent('compare to HEAD');
  });

  it('HEAD (green): a mapped file with zero real hits falls back to the literal HEAD sha — explicit, not omitted, no ref/date, no compare stub', async () => {
    appState.drawer = 'none';
    appState.view = 'code';
    appState.hub = '';
    render(App);

    // Let the default file's fetch settle first, then switch to the
    // fixture with genuinely zero conversations (mock.ts's EmptyView.swift).
    await screen.findByTestId('rev-chip');
    await waitFor(() => expect(screen.getByTestId('rev-chip')).toHaveClass('pinned'));
    codeState.forHub('agent-coord').activeKey = fileKey({ repo: 'wealdlore', path: 'Sources/Reader/EmptyView.swift' });

    await waitFor(() => expect(screen.getByTestId('rev-chip')).toHaveClass('head'));
    const chip = screen.getByTestId('rev-chip');
    expect(chip).not.toHaveClass('pinned');
    expect(chip).toHaveTextContent('●');
    expect(chip).toHaveTextContent('HEAD');

    // Law #3 — no fabricated ref/date when nothing real is pinned, and no
    // compare-to-HEAD stub (comparing HEAD to itself is meaningless).
    expect(screen.queryByTestId('code-header-refname')).not.toBeInTheDocument();
    expect(screen.queryByTestId('code-header-date')).not.toBeInTheDocument();
    expect(screen.queryByTestId('compare-to-head')).not.toBeInTheDocument();
  });
});
