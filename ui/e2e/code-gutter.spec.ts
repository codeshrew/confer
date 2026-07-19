import { test, expect } from '@playwright/test';

// Piece 11 Phase 2 (ui/redesign-mockups/11-code-view-BUILD-BRIEF.md +
// 12-code-gutter-and-time.html) — the powered gutter: shape = scope
// (file-lane / bracket / tick), color = meaning (the real per-kind
// palette), column = overlap (side-by-side brackets for genuinely
// different overlapping ranges), and a drift marker only on a REAL sha
// mismatch. Mock fixtures: PlateBundle.swift (the default active file)
// carries a whole-file hit, a 3-hit identical range [44,49], and a
// genuinely overlapping different range [44,46] (1 hit) — real column
// overlap without any synthetic setup. pipeline/plates.py carries a real
// `staleness: 'changed'` hit — the drift case.

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Code', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Code', exact: true }).click();
  await expect(page.getByTestId('code-view')).toBeVisible();
});

test('shape = scope: the file-lane shows whole-file conversations, separate from any line', async ({ page }) => {
  const lane = page.getByTestId('file-lane');
  await expect(lane).toBeVisible();
  await expect(lane).toContainText('conversation');

  await lane.click();
  const drawer = page.getByTestId('right-drawer');
  await expect(drawer.getByTestId('anchor-scope')).toContainText('▤');
  await expect(drawer.getByTestId('anchored-conv')).toBeVisible();
});

test('color = meaning: a claimed/open request tab and a note tab read distinct real colors, not one bare count', async ({ page }) => {
  // The 3-hit note group and the 1-hit OPEN request group both start on
  // line 44 — two real, differently-colored tabs, not a single number.
  const noteTab = page.getByTestId('gutter-tab').filter({ hasText: '3 ·' });
  const reqTab = page.getByTestId('gutter-tab').filter({ hasText: '1 ·' }).filter({ hasText: 'JA' });
  await expect(noteTab).toBeVisible();
  await expect(reqTab).toBeVisible();

  const noteColor = await noteTab.evaluate((el) => getComputedStyle(el).color);
  const reqColor = await reqTab.evaluate((el) => getComputedStyle(el).color);
  expect(noteColor).not.toBe(reqColor);
});

test('column = overlap: two genuinely different overlapping ranges render as two distinct bracket columns on their shared line', async ({ page }) => {
  // Lines 44-46 are covered by BOTH the [44,49] note group and the
  // [44,46] open-request hit — two real, side-by-side bracket segments.
  const line44 = page.locator('.cl').filter({ has: page.locator('.ln', { hasText: '44' }) });
  await expect(line44.locator('.br')).toHaveCount(2);
});

test('the range tab opens the anchored reader scoped to its REAL full range, not just the clicked line', async ({ page }) => {
  const noteTab = page.getByTestId('gutter-tab').filter({ hasText: '3 ·' });
  await noteTab.click();

  const drawer = page.getByTestId('right-drawer');
  await expect(drawer.getByTestId('anchor-scope')).toContainText('44');
  await expect(drawer.getByTestId('anchor-scope')).toContainText('49');
  await expect(drawer.getByTestId('anchored-conv')).toBeVisible();
});

test('law #3 — the drift marker shows ONLY on a real staleness "changed" hit, elsewhere it never appears', async ({ page }) => {
  // Line 48's note has genuinely drifted (staleness: 'changed') — a real
  // dashed bracket + a "◷" tab, distinct from the ordinary (non-drifted)
  // tabs on lines 44/44 right above it.
  const driftTab = page.getByTestId('gutter-tab').filter({ hasText: '◷' });
  await expect(driftTab).toBeVisible();
  await expect(driftTab).toContainText('1 · CO');
  // A single-line hit renders as a TICK, not a bracket — drift still
  // shows (a dashed outline ring, the tick's own version of the bracket's
  // dashed edge).
  await expect(page.locator('.tick.drift')).toBeVisible();

  // The other, ordinary tabs on this same file show NO drift marker.
  const noteTab = page.getByTestId('gutter-tab').filter({ hasText: '3 ·' });
  await expect(noteTab).not.toHaveClass(/drift/);
});

test('renders correctly in both themes', async ({ page }) => {
  await expect(page.getByTestId('file-lane')).toBeVisible();
  await expect(page.getByTestId('gutter-tab').filter({ hasText: '3 ·' })).toBeVisible();

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByTestId('file-lane')).toBeVisible();
  await expect(page.getByTestId('gutter-tab').filter({ hasText: '3 ·' })).toBeVisible();
  const line44 = page.locator('.cl').filter({ has: page.locator('.ln', { hasText: '44' }) });
  await expect(line44.locator('.br')).toHaveCount(2);
});
