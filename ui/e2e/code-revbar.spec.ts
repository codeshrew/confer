import { test, expect } from '@playwright/test';

// Piece 11 Phase 4 (ui/redesign-mockups/11-code-view-BUILD-BRIEF.md + mock
// 11/12's rev bar) — revision orientation: the crumb always shows sha, and
// whether it's HEAD (green) or a pinned past commit (amber), plus ref/date
// when the API provides them. Data already served (refcode.rs::snippet at
// the pinned sha, identity_paren for ref/date) — this is a surfacing pass,
// no backend change. `⇄ compare to HEAD` is a stub this phase. Fixtures:
// PlateBundle.swift (real hits -> pinned) and EmptyView.swift (mock.ts's
// new zero-refs fixture -> HEAD).

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Code', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Code', exact: true }).click();
  await expect(page.getByTestId('code-view')).toBeVisible();
});

test('pinned (amber): a file with real hits shows the pinned sha, real ref/date, and a disabled compare-to-HEAD', async ({
  page,
}) => {
  const chip = page.getByTestId('rev-chip');
  await expect(chip).toHaveClass(/pinned/);
  await expect(chip).not.toHaveClass(/head/);
  await expect(chip).toContainText('◷');
  await expect(chip).toContainText('@a3f1c9');

  await expect(page.getByTestId('code-header-refname')).toHaveText('main');
  await expect(page.getByTestId('code-header-date')).toBeVisible();

  const compare = page.getByTestId('compare-to-head');
  await expect(compare).toBeVisible();
  await expect(compare).toBeDisabled();
  await expect(compare).toContainText('compare to HEAD');
});

test('HEAD (green): a file with zero real hits shows an explicit HEAD chip, not an omitted one — no ref/date, no compare stub', async ({
  page,
}) => {
  const tree = page.getByTestId('code-tree');
  await tree.getByRole('button', { name: /EmptyView\.swift/ }).click();

  const chip = page.getByTestId('rev-chip');
  await expect(chip).toHaveClass(/head/);
  await expect(chip).not.toHaveClass(/pinned/);
  await expect(chip).toContainText('●');
  await expect(chip).toContainText('HEAD');

  // law #3 — nothing fabricated when there's genuinely nothing pinned.
  await expect(page.getByTestId('code-header-refname')).not.toBeAttached();
  await expect(page.getByTestId('code-header-date')).not.toBeAttached();
  await expect(page.getByTestId('compare-to-head')).not.toBeAttached();
});

test('switching files flips the chip between HEAD and pinned live, in place', async ({ page }) => {
  const chip = page.getByTestId('rev-chip');
  await expect(chip).toHaveClass(/pinned/);

  const tree = page.getByTestId('code-tree');
  await tree.getByRole('button', { name: /EmptyView\.swift/ }).click();
  await expect(chip).toHaveClass(/head/);

  await tree.getByRole('button', { name: /PlateBundle\.swift/ }).click();
  await expect(chip).toHaveClass(/pinned/);
});

test('renders correctly in both themes', async ({ page }) => {
  await expect(page.getByTestId('rev-chip')).toBeVisible();
  await expect(page.getByTestId('compare-to-head')).toBeVisible();

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByTestId('rev-chip')).toBeVisible();
  await expect(page.getByTestId('compare-to-head')).toBeVisible();
});
