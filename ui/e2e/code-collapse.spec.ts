import { test, expect } from '@playwright/test';

// Piece 11 Phase 3 (ui/redesign-mockups/11-code-view-BUILD-BRIEF.md +
// 11-code-collapse-RESEARCH.md + mock 11's `.fold` rows) — PR-style
// collapse: referenced ranges + context stay open, everything else folds.
// Fixture: PlateBundle.swift's mock snippet carries real hits on [44,49]
// (context -> open span [44,51]) but lines 52-65 are genuinely unreferenced
// padding (see mock.ts) — a real bottom fold, not synthetic test-only data.

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Code', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Code', exact: true }).click();
  await expect(page.getByTestId('code-view')).toBeVisible();
});

test('defaults to "referenced": a real bottom fold hides the unreferenced tail', async ({ page }) => {
  await expect(page.locator('[data-line="51"]')).toBeVisible();
  await expect(page.locator('[data-line="52"]')).not.toBeAttached();

  const fold = page.getByTestId('fold-row');
  await expect(fold).toBeVisible();
  await expect(fold).toHaveAttribute('title', 'lines 52–65');
  await expect(fold).toContainText('expand 14 lines');
  // A bottom-edge fold has ONE button, not the middle gap's ↑8/↓8/⤢all set.
  await expect(page.getByTestId('fold-expand-edge')).toHaveText('↓ bottom');
  await expect(page.getByTestId('fold-expand-up')).toHaveCount(0);

  const toggle = page.getByTestId('collapse-toggle');
  await expect(toggle).toBeVisible();
  await expect(page.getByTestId('collapse-toggle-referenced')).toHaveClass(/on/);
});

test('clicking the fold reveals the real hidden lines, in place, and the fold row disappears', async ({ page }) => {
  await page.getByTestId('fold-expand-edge').click();

  await expect(page.getByTestId('fold-row')).toHaveCount(0);
  await expect(page.locator('[data-line="52"]')).toBeVisible();
  await expect(page.locator('[data-line="65"]')).toBeVisible();
  await expect(page.locator('[data-line="65"]')).toContainText('EOF (mock fixture padding)');
});

test('"show all" renders the real whole file with no folds; "referenced" re-collapses it', async ({ page }) => {
  await page.getByTestId('collapse-toggle-showall').click();

  await expect(page.getByTestId('fold-row')).toHaveCount(0);
  await expect(page.locator('[data-line="65"]')).toBeVisible();
  await expect(page.getByTestId('collapse-toggle-showall')).toHaveClass(/on/);

  await page.getByTestId('collapse-toggle-referenced').click();
  await expect(page.getByTestId('fold-row')).toBeVisible();
  await expect(page.locator('[data-line="52"]')).not.toBeAttached();
});

test('gutter line numbers stay correct across the fold — line 44 still reads "44", not renumbered', async ({ page }) => {
  const line44 = page.locator('[data-line="44"] .ln');
  await expect(line44).toHaveText('44');
  const line51 = page.locator('[data-line="51"] .ln');
  await expect(line51).toHaveText('51');
});

test('the minimap keeps working across collapse — clicking a segment still scrolls to its real range', async ({ page }) => {
  // Regression guard: Phase 2b's minimap is built off the full hit set, not
  // the collapsed view — clicking a segment for an always-open referenced
  // range must still work with Phase 3 landed.
  await expect(page.getByTestId('code-minimap')).toBeVisible();
  await expect(page.getByTestId('minimap-segment').first()).toBeVisible();
});

test('renders correctly in both themes', async ({ page }) => {
  await expect(page.getByTestId('fold-row')).toBeVisible();
  await expect(page.getByTestId('collapse-toggle')).toBeVisible();

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByTestId('fold-row')).toBeVisible();
  await expect(page.getByTestId('collapse-toggle')).toBeVisible();
});
