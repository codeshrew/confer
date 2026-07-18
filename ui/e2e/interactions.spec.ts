import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Chat', exact: true })).toBeVisible();
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
  await page.getByText('Freeze the CSL schema — needs a decision from Herald').click();

  const drawer = page.getByTestId('right-drawer');
  await expect(drawer.getByText('Request detail')).toBeVisible();
  await expect(drawer.getByRole('heading', { name: 'Freeze the CSL schema — needs a decision from Herald' })).toBeVisible();
});

test('a code-ref reverse-index hook opens the reverse-index panel', async ({ page }) => {
  // Chat defaults to Summary density — expand this specific note to reveal
  // its full body (and the code-ref card within it) before interacting with
  // the revhook.
  const summaryLine = page.getByText('bundle assembly wired, pinned to a3f1c9');
  await expect(summaryLine).toBeVisible();
  const noteRow = page.locator('.msg').filter({ has: summaryLine });
  await noteRow.getByRole('button', { name: 'Expand message' }).click();
  await expect(page.getByText('Wired it — bundle assembly is here')).toBeVisible();

  // The revhook is the inner button carrying the "N conversations reference
  // these lines" hook — not the outer message row (also clickable, and its
  // accessible name also contains that text since it wraps the whole card).
  await page.getByTestId('revhook').first().click();

  const drawer = page.getByTestId('right-drawer');
  await expect(drawer.getByText('Reverse index')).toBeVisible();
  await expect(drawer.getByText('Conversations about this code')).toBeVisible();
  await expect(drawer.getByText('PlateBundle.swift')).toBeVisible();
});
