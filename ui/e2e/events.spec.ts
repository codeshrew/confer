import { test, expect } from '@playwright/test';

// Piece 9 (ui/redesign-mockups/09-event-type-BRIEF.md) — the composable
// EVENT type: a lifecycle/system message (claim/done/error/blocked/defer/
// supersede) renders as a compact row whose SUBJECT is a clickable chip
// that opens the subject's OWN popover (ticket/agent/thread) — the event
// has none of its own. Mock fixtures (src/lib/mock.ts) back real ticket
// events (claim/done/blocked in #reader), a real thread-superseding event,
// and one deliberately dangling claim (law #3's "plain text, never a dead
// link" case).

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Chat', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
});

test('a claim event\'s subject chip opens the ticket it claimed — Full popover, not the event\'s own', async ({ page }) => {
  const row = page.locator('[data-testid="event-row"][data-msg-id="msg_01JQa10"]');
  await expect(row).toBeVisible();

  const chip = row.getByTestId('event-subject-chip');
  await expect(chip).toHaveText('req_01JQ8f2');
  await chip.click();

  const popover = page.getByTestId('ticket-popover');
  await expect(popover).toBeVisible();
  await expect(popover).toContainText('req_01JQ8f2');
});

test('a blocked event renders as a compact event row, not a full message bubble (the real gap the type fixes)', async ({ page }) => {
  const row = page.locator('[data-testid="event-row"][data-msg-id="msg_01JQc4a"]');
  await expect(row).toBeVisible();
  await expect(page.locator('.msg[data-msg-id="msg_01JQc4a"]')).toHaveCount(0);
});

test('a defer event\'s subject chip opens its ticket — Board\'s parked/deferred one, in a different topic', async ({ page }) => {
  await page.getByRole('button', { name: /studio-markup/ }).click();

  const row = page.locator('[data-testid="event-row"][data-type="defer"]');
  await expect(row).toBeVisible();
  const chip = row.getByTestId('event-subject-chip');
  await expect(chip).toHaveText('req_01JQd21');
  await chip.click();

  const popover = page.getByTestId('ticket-popover');
  await expect(popover).toBeVisible();
  await expect(popover).toContainText('stream regions instead of assembling the full bundle');
});

test('an error event\'s subject chip opens its ticket', async ({ page }) => {
  await page.getByRole('button', { name: /plate-pipeline/ }).click();

  const row = page.locator('[data-testid="event-row"][data-type="error"]');
  await expect(row).toBeVisible();
  const chip = row.getByTestId('event-subject-chip');
  await expect(chip).toHaveText('req_01JQe88');
  await chip.click();

  const popover = page.getByTestId('ticket-popover');
  await expect(popover).toBeVisible();
  await expect(popover).toContainText('Revisit eager per-region loop');
});

test('a supersede event\'s subject chip peeks the thread it replaces — not a popover, the same peek a note click gives', async ({ page }) => {
  const row = page.locator('[data-testid="event-row"][data-type="supersede"]');
  await expect(row).toBeVisible();
  const chip = row.getByTestId('event-subject-chip');
  await expect(chip).toHaveText('restore chain context');
  await chip.click();

  await expect(page.getByTestId('thread-peek')).toBeVisible();
  // esc closes the peek back to the stream, same as clicking a note row —
  // proving event->thread is stream->popover-adjacent, not a stacked popover.
  await page.getByTestId('thread-peek').focus();
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('thread-peek')).not.toBeVisible();
});

test('law #3 — a dangling event (its ticket was pruned) renders as plain text with NO clickable chip', async ({ page }) => {
  const row = page.locator('[data-testid="event-row"]').filter({ hasText: 'claimed req_purged' });
  await expect(row).toBeVisible();
  await expect(row.getByTestId('event-subject-chip')).toHaveCount(0);
});

test('the subject chip is keyboard-reachable — focusable and Enter-activates it', async ({ page }) => {
  const row = page.locator('[data-testid="event-row"][data-msg-id="msg_01JQa10"]');
  const chip = row.getByTestId('event-subject-chip');
  await chip.focus();
  await expect(chip).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('ticket-popover')).toBeVisible();
});

test('events are NOT primary j/k stops — pressing j walks only real notes/tickets, skipping event rows', async ({ page }) => {
  // #reader (the default topic) has 5 real (non-event) messages and 5
  // event rows interleaved among them; if events were counted as stops,
  // reaching the last real message would take more than 5 presses.
  const stream = page.locator('.stream');
  for (let i = 0; i < 5; i++) await stream.press('j');

  const selected = page.locator('.msg.sel');
  await expect(selected).toHaveAttribute('data-msg-id', 'msg_01JQg44');
});

test('renders correctly in both themes', async ({ page }) => {
  const row = page.locator('[data-testid="event-row"][data-msg-id="msg_01JQa10"]');
  await expect(row.getByTestId('event-subject-chip')).toBeVisible();

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(row.getByTestId('event-subject-chip')).toBeVisible();
  await expect(page.locator('[data-testid="event-row"]').filter({ hasText: 'claimed req_purged' }).getByTestId('event-subject-chip')).toHaveCount(0);
});
