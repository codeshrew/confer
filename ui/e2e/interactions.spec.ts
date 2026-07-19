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

test('opening a ticket from Chat surfaces the RequestDetail lifecycle trail', async ({ page }) => {
  await page.getByText('Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader').click();

  const drawer = page.getByTestId('right-drawer');
  await expect(drawer.getByText('Request detail')).toBeVisible();
  await expect(drawer.getByRole('heading', { name: 'Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader' })).toBeVisible();
  // The reconstructed lifecycle trail (walked from reply-hashes).
  await expect(drawer.getByText('blocked on uid-spine contract')).toBeVisible();
});

test('opening a board row surfaces the RequestDetail lifecycle trail', async ({ page }) => {
  await page.getByRole('tab', { name: 'Board', exact: true }).click();
  // Scoped to the Board pane — Overview stays mounted in the background and
  // repeats this same request's summary in its own Coordination-lane card.
  await page.getByTestId('board-view').getByText('Freeze the CSL schema — needs a decision from Herald').click();

  const drawer = page.getByTestId('right-drawer');
  await expect(drawer.getByText('Request detail')).toBeVisible();
  await expect(drawer.getByRole('heading', { name: 'Freeze the CSL schema — needs a decision from Herald' })).toBeVisible();
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
