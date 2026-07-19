import { test, expect } from '@playwright/test';

// Piece 6 (ui/REDESIGN.md, "the composable card system") — the enriched
// note popover: a plain note's full body + a keyboard-selectable Related
// column (tickets/code/thread), reached via each message row's own "open
// note" button (mouse) — notes only, a ticket already has its own richer
// destination (TicketFullPopover, piece 5).

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Chat', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
});

test('opening a note surfaces the enriched popover — body + real related tickets/code/thread', async ({ page }) => {
  const noteRow = page.locator('.msg').filter({ hasText: 'canaried 0.7.3' });
  await noteRow.hover();
  await noteRow.getByRole('button', { name: 'Open note detail' }).click();

  const popover = page.getByTestId('note-popover');
  await expect(popover).toBeVisible();
  await expect(popover.getByRole('heading', { name: 'canaried 0.7.3' })).toBeVisible();
  await expect(popover.getByText(/nice\. Canaried 0\.7\.3 on pop-os/)).toBeVisible();

  // The related column — real data pulled from the same thread the peek
  // uses (mock's shared thread fixture: one ticket, one code ref, 5 msgs
  // across 2 topics).
  await expect(popover.getByText('tickets')).toBeVisible();
  await expect(popover.getByTestId('ticket-mini')).toBeVisible();
  await expect(popover.getByText('code')).toBeVisible();
  await expect(popover.getByTestId('code-ref-mini')).toBeVisible();
  const threadPill = popover.getByTestId('note-thread-pill');
  await expect(threadPill).toContainText('msgs');
  await expect(threadPill).toContainText('topic');

  await page.keyboard.press('Escape');
  await expect(popover).not.toBeVisible();
});

test('clicking a related ticket mini card portals to the Full ticket popover', async ({ page }) => {
  const noteRow = page.locator('.msg').filter({ hasText: 'canaried 0.7.3' });
  await noteRow.hover();
  await noteRow.getByRole('button', { name: 'Open note detail' }).click();

  const popover = page.getByTestId('note-popover');
  await popover.getByTestId('ticket-mini').click();

  await expect(page.getByTestId('ticket-popover')).toBeVisible();
  await expect(popover).not.toBeVisible();
});

test('the note popover renders correctly in both themes', async ({ page }) => {
  const noteRow = page.locator('.msg').filter({ hasText: 'canaried 0.7.3' });
  await noteRow.hover();
  await noteRow.getByRole('button', { name: 'Open note detail' }).click();
  const popover = page.getByTestId('note-popover');
  await expect(popover).toBeVisible();

  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');

  await noteRow.hover();
  await noteRow.getByRole('button', { name: 'Open note detail' }).click();
  await expect(popover).toBeVisible();
  await expect(popover.getByText('tickets')).toBeVisible();
});
