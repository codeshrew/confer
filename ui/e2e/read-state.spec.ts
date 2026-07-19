import { test, expect } from '@playwright/test';

// piece 4, item 2 (ui/REDESIGN.md): real per-(hub,topic) watermark, real
// seen-by (Herald's src/seen.rs), and the completionist-safe detail-viewed
// marker. Each Playwright test gets a fresh browser context, so localStorage
// starts empty — every (hub, topic) here is genuinely "never visited" at
// the start of each test.

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Chat', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
});

test('real seen-by: a message with a confirmed seenBy entry shows real names/times in the roster, not synthesized ones', async ({ page }) => {
  // msg_01JQ001 ("Shipping confer 0.7.3") has real mock seenBy entries for
  // reader/pipeline/compositor — see mock.ts.
  const note = page.getByText('Shipping confer 0.7.3');
  await note.waitFor();

  const seenIndicator = page.locator('.msg', { hasText: 'Shipping confer 0.7.3' }).locator('.seen');
  await seenIndicator.hover();
  const roster = seenIndicator.locator('.roster');
  await expect(roster).toBeVisible();
  await expect(roster.getByText('Reader', { exact: true })).toBeVisible();
});

test('"mark all read" is available in Chat and clicking it does not error', async ({ page }) => {
  const button = page.getByTestId('mark-all-read');
  await expect(button).toBeVisible();
  await button.click();
  // Still on Chat, stream still rendered — a plain, safe catch-up action.
  await expect(page.getByText('Shipping confer 0.7.3')).toBeVisible();
});

test('detail-viewed: opening the focus reader past the dwell threshold marks the message, showing a subtle ✓ on its stream row', async ({ page }) => {
  const note = page.getByText(/canaried 0.7.3/).first();
  await note.click();
  await page.keyboard.press('f');

  const reader = page.getByTestId('focus-reader');
  await expect(reader).toBeVisible();
  // The dwell threshold (~2.5s) — real wait, no fake timers available at
  // the Playwright/real-browser layer the way vitest's unit tests use.
  await page.waitForTimeout(2700);

  await page.keyboard.press('Escape');
  await expect(reader).not.toBeVisible();

  const row = page.locator('.msg', { hasText: 'canaried 0.7.3' });
  await expect(row.getByTitle('Opened in the focus reader')).toBeVisible();
});

test('renders correctly in both themes', async ({ page }) => {
  await expect(page.getByTestId('mark-all-read')).toBeVisible();
  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByTestId('mark-all-read')).toBeVisible();
});
