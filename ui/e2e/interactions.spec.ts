import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  // design/47: Overview is the default landing now, not Chat — most of this
  // file's tests need Chat's content on screen, so switch there up front.
  await expect(page.getByRole('tab', { name: 'Chat', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
});

test('theme toggle flips data-theme and the rendered background', async ({ page }) => {
  const before = await page.evaluate(() => ({
    theme: document.documentElement.getAttribute('data-theme'),
    bg: getComputedStyle(document.body).backgroundColor,
  }));
  expect(before.theme).toBe('dark');

  await page.getByRole('button', { name: 'Toggle theme' }).click();

  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  const after = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  expect(after).not.toBe(before.bg);

  // Flips back.
  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
});

test('opening a ticket from Chat (its Mini card) surfaces the Full popover — lifecycle track, not a right-rail panel', async ({ page }) => {
  // piece 5 retired RequestDetail's right-rail role — a ticket's Mini card
  // (embedded inline in the stream) now portals into the Full popover
  // overlay instead. This fixture (req_01JQ8f2) is DONE, so the track's
  // three stages are all filled and the resolution line shows.
  await page.getByText('Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader').click();

  const popover = page.getByTestId('ticket-popover');
  await expect(popover).toBeVisible();
  await expect(popover.getByRole('heading', { name: 'Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader' })).toBeVisible();
  const track = popover.locator('.track3');
  await expect(track.getByText('Requested', { exact: true })).toBeVisible();
  await expect(track.getByText('Claimed', { exact: true })).toBeVisible();
  await expect(track.getByText('Done', { exact: true })).toBeVisible();
  await expect(popover.getByText(/resolved: endpoint live, tests green/)).toBeVisible();

  // The right rail, meanwhile, opened the SAME meta-thread pane a plain
  // click would — piece 5 dropped the bespoke "Request detail" mode.
  await expect(page.getByTestId('right-drawer').getByText('Meta-thread')).toBeVisible();

  // Esc closes the popover without losing the underlying selection.
  await page.keyboard.press('Escape');
  await expect(popover).not.toBeVisible();
});

test('opening a board row surfaces the Full popover, stuck-state branch included', async ({ page }) => {
  await page.getByRole('tab', { name: 'Board', exact: true }).click();
  // Scoped to the Board pane — Overview stays mounted in the background and
  // repeats this same request's summary in its own Coordination-lane card.
  await page.getByTestId('board-view').getByText('Freeze the CSL schema — needs a decision from Herald').click();

  const popover = page.getByTestId('ticket-popover');
  await expect(popover).toBeVisible();
  await expect(popover.getByRole('heading', { name: 'Freeze the CSL schema — needs a decision from Herald' })).toBeVisible();
  // This fixture (req_01JQc4a) is BLOCKED — the state shows in the meta
  // grid. It has no chat message correctly linked as its own origin (a
  // pre-existing mock-data gap — the ticket exists only as board data), so
  // there's no real blocked-message text to attribute a reason to; law #3
  // means the popover correctly shows NO branch/reason line rather than
  // fabricating one (see ticketState.test.ts for the branch-reason case,
  // covered there with a fixture that DOES have a real trail).
  await expect(popover.getByText(/stuck/)).toBeVisible();
  await expect(popover.locator('.tk-branch')).toHaveCount(0);
});

test('a code-ref reverse-index hook opens the reverse-index panel', async ({ page }) => {
  // Chat defaults to Summary density (piece 4, item 3: Summary means
  // summary now — one line + chips, no rendered code/refs, no per-message
  // expand into them anymore). Switch the whole stream to Full density to
  // reveal the note's full body and its code-ref card.
  //
  // "restore chain context" specifically (not "bundle assembly wired") —
  // its body has no inline `--ref` mention, so its ref stays a full
  // trailing CodeRefCard (with a real revhook button) rather than being
  // anchored inline as a compact chip (piece 4, item 3.3 — see
  // chat-glance.spec.ts for that path's own reverse-index coverage).
  await page.getByRole('button', { name: 'Full', exact: true }).click();
  await expect(page.getByText('For context, the restore chain')).toBeVisible();

  // The revhook is the inner button carrying the "N conversations reference
  // these lines" hook — not the outer message row (also clickable, and its
  // accessible name also contains that text since it wraps the whole card).
  await page.getByTestId('revhook').first().click();

  const drawer = page.getByTestId('right-drawer');
  await expect(drawer.getByText('Reverse index')).toBeVisible();
  await expect(drawer.getByText('Conversations about this code')).toBeVisible();
  await expect(drawer.getByText('plates.py')).toBeVisible();
});

test('the ticket Full popover renders correctly in both themes', async ({ page }) => {
  const openTicket = () => page.getByText('Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader').click();
  const popover = page.getByTestId('ticket-popover');

  await openTicket();
  await expect(popover).toBeVisible();
  await expect(popover.getByText(/resolved: endpoint live, tests green/)).toBeVisible();

  // The popover's backdrop blocks clicks to the chrome underneath (by
  // design, a modal) — close it before reaching the theme toggle.
  await page.keyboard.press('Escape');
  await expect(popover).not.toBeVisible();
  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');

  await openTicket();
  await expect(popover).toBeVisible();
  await expect(popover.getByText(/resolved: endpoint live, tests green/)).toBeVisible();
});
