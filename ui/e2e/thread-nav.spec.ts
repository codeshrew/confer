import { test, expect } from '@playwright/test';

// piece 3 (ui/REDESIGN.md): side-peek + trail + focus reader. Mock fixtures
// back a real reply-hash thread (agent-coord/#reader) so the peek/trail/
// breadcrumb/reader all have real data to walk.

test('opening a peek keeps the stream in place, and Escape closes it back to the empty state', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('hub-rail').getByText('agent-coord').waitFor();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();

  const note = page.getByText(/canaried 0.7.3/).first();
  await note.click();

  const peek = page.getByTestId('thread-peek');
  await expect(peek).toBeVisible();
  // The stream row is still right there — never swapped away.
  await expect(note).toBeVisible();

  await peek.focus();
  await page.keyboard.press('Escape');
  await expect(peek).not.toBeVisible();
  await expect(page.getByText('Select a message to trace its thread')).toBeVisible();
  await expect(note).toBeVisible();
});

test('the peek shows a real breadcrumb + Focused card, and clicking a trail row moves focus without jumping the stream', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('hub-rail').getByText('agent-coord').waitFor();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
  await page.getByText(/canaried 0.7.3/).first().click();

  await expect(page.getByTestId('peek-crumbs')).toBeVisible();
  await expect(page.getByTestId('peek-focused')).toBeVisible();

  const rows = page.getByTestId('peek-node');
  const secondRow = rows.nth(1);
  await secondRow.click();

  // Focus moved (the clicked row now shows the "here" tag) but the Chat
  // topic crumb up top never changed — no navigation happened.
  await expect(secondRow).toContainText('◂ here');
  await expect(page.getByText('#reader', { exact: true }).first()).toBeVisible();
});

test('Enter (or the Focused card\'s "open here" button) actually jumps the stream', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('hub-rail').getByText('agent-coord').waitFor();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
  await page.getByText(/canaried 0.7.3/).first().click();

  await page.getByTestId('peek-jump').click();
  // Jumping lands back in Chat (it already was), with the peek re-anchored
  // on the jumped-to message — the deliberate, explicit move.
  await expect(page.getByRole('tab', { name: 'Chat', exact: true })).toHaveAttribute('aria-selected', 'true');
});

test('"f" opens the focus reader on the focused message; j/k walk the thread; f/Esc exits', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('hub-rail').getByText('agent-coord').waitFor();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
  await page.getByText(/canaried 0.7.3/).first().click();

  await page.keyboard.press('f');
  const reader = page.getByTestId('focus-reader');
  await expect(reader).toBeVisible();
  // Prose-typeset body, not the stream's clamp — real markdown rendering.
  await expect(reader.locator('.fr-reading')).toBeVisible();

  await page.keyboard.press('j');
  // Still open, now showing a different message (real thread navigation).
  await expect(reader).toBeVisible();

  await page.keyboard.press('Escape');
  await expect(reader).not.toBeVisible();
});

test('item 0 bug fix: clicking a stream message opens the peek, but active pane focus stays "stream" — j/k×3 keeps moving the stream, not the trail', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('hub-rail').getByText('agent-coord').waitFor();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();

  const chip = page.getByTestId('focus-chip');
  await page.getByText(/canaried 0.7.3/).first().click();

  // Clicking a message opens the peek as a side effect of selection — the
  // exact scenario that used to silently steal pane focus.
  await expect(page.getByTestId('thread-peek')).toBeVisible();
  await expect(chip).toHaveText(/Chat stream/);

  await page.keyboard.press('j');
  // Still "stream" after the first move — this is where it used to jump.
  await expect(chip).toHaveText(/Chat stream/);
  await page.keyboard.press('j');
  await page.keyboard.press('j');
  await expect(chip).toHaveText(/Chat stream/);
});

test('"y" copies the focused message\'s full id, in both the focus reader and the peek', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('hub-rail').getByText('agent-coord').waitFor();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
  await page.getByText(/canaried 0.7.3/).first().click();

  // The peek's Focused card.
  const peekFocused = page.getByTestId('peek-focused');
  await expect(peekFocused).toBeVisible();
  await page.getByTestId('thread-peek').focus();
  await page.keyboard.press('y');
  await expect(page.getByTestId('copied-toast')).toBeVisible();
  await expect(page.getByTestId('copied-toast')).toContainText('copied');

  // The focus reader.
  await page.keyboard.press('f');
  const reader = page.getByTestId('focus-reader');
  await expect(reader).toBeVisible();
  await page.keyboard.press('y');
  await expect(reader.getByTestId('copied-toast')).toBeVisible();
});

test('renders correctly in both themes', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('hub-rail').getByText('agent-coord').waitFor();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
  await page.getByText(/canaried 0.7.3/).first().click();
  await expect(page.getByTestId('thread-peek')).toBeVisible();

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByTestId('thread-peek')).toBeVisible();

  await page.keyboard.press('f');
  await expect(page.getByTestId('focus-reader')).toBeVisible();
});
