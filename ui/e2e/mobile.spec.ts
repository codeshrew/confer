import { test, expect } from '@playwright/test';

// Mobile project only (390x844 — see playwright.config.ts). This is the
// regression guard for the TicketCard/BoardRow/CodeLens narrow-viewport
// fixes: nothing in the app may force the document itself to scroll
// sideways, on any of the four views.

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  // design/47: Overview (the new default landing) has no left rail at all,
  // so the hamburger/drawer regression guards below need Chat on screen —
  // switch there up front (a bare page load now lands on Overview instead).
  await expect(page.getByRole('tab', { name: 'Chat', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
});

async function expectNoHorizontalScroll(page: import('@playwright/test').Page) {
  await expect
    .poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth))
    .toBe(true);
}

test('the hamburger opens the left drawer and the scrim closes it', async ({ page }) => {
  const drawer = page.getByTestId('left-drawer');
  const hamburger = page.getByTestId('hamburger');

  // Closed: the drawer sits off-canvas, to the left of the viewport.
  const closedBox = await drawer.boundingBox();
  expect(closedBox?.x).toBeLessThan(0);
  await expect(hamburger).toHaveAttribute('aria-expanded', 'false');

  await hamburger.click();
  await expect(hamburger).toHaveAttribute('aria-expanded', 'true');
  await expect.poll(async () => (await drawer.boundingBox())?.x).toBeGreaterThanOrEqual(0);
  // The topic list inside the drawer is now reachable.
  await expect(page.getByTestId('left-drawer').getByText('reader', { exact: true })).toBeVisible();

  // Click a point the drawer panel itself doesn't cover (it's only 280px
  // wide) — the scrim spans the full viewport, but at the drawer's own
  // width it sits underneath the (higher z-index) drawer panel.
  await page.getByTestId('drawer-scrim').click({ position: { x: 370, y: 400 } });
  await expect(hamburger).toHaveAttribute('aria-expanded', 'false');
  await expect.poll(async () => (await drawer.boundingBox())?.x).toBeLessThan(0);
});

test('Chat has no horizontal overflow', async ({ page }) => {
  await expectNoHorizontalScroll(page);
  // The regression case: a ticket card's stub + lifecycle track must not
  // force the card (and therefore the page) wider than the viewport.
  await expect(page.getByText('Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader')).toBeVisible();
  await expectNoHorizontalScroll(page);
});

test('Board has no horizontal overflow', async ({ page }) => {
  await page.getByRole('tab', { name: 'Board', exact: true }).click();
  // Scoped to the Board pane — Overview stays mounted in the background and
  // repeats this same request's summary in its own Coordination-lane card.
  await expect(page.getByTestId('board-view').getByText('Freeze the CSL schema — needs a decision from Herald')).toBeVisible();
  await expectNoHorizontalScroll(page);
});

test('Code has no horizontal overflow', async ({ page }) => {
  await page.getByRole('tab', { name: 'Code', exact: true }).click();
  // Scoped to the Code pane — Chat stays mounted (kept alive, just hidden)
  // in the background for instant tab-switching, and a Chat message's own
  // CodeRefCard can render this same highlighted token text, which would
  // otherwise make this match ambiguous.
  await expect(page.getByTestId('code-view').getByText('assembleBundle').first()).toBeVisible();
  await expectNoHorizontalScroll(page);
});
