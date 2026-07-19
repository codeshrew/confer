import { test, expect } from '@playwright/test';

// Piece 7 (ui/REDESIGN.md, `redesign-mockups/07-repos-integrity-gravity.html`)
// — Repos as integrity + gravity, not a flat list: tracked / registered-
// not-local / shadow tiers, real reference density, and a drill-in that
// jumps straight into the Code view.

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Chat', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Repos', exact: true }).click();
});

test('repos are grouped by real integrity tier, with a health line naming the real gap', async ({ page }) => {
  const reposView = page.getByTestId('repos-view');

  await expect(reposView.getByText('✓ tracked')).toBeVisible();
  await expect(reposView.getByText('◑ registered · not on this machine')).toBeVisible();
  await expect(reposView.getByText('◇ shadow · referenced, not registered')).toBeVisible();

  // wealdlore is registered but not cloned in the fixtures.
  await expect(reposView.getByTestId('repo-row-wealdlore')).toBeVisible();
  // openjarvis is --ref'd but never registered — a real shadow repo.
  await expect(reposView.getByTestId('repo-row-openjarvis')).toBeVisible();

  await expect(reposView.getByText(/1 not registered/)).toBeVisible();
  await expect(reposView.getByText(/1 not cloned/)).toBeVisible();
});

test('drilling into a repo shows its real hot files and identity, then jumps to the Code view', async ({ page }) => {
  const reposView = page.getByTestId('repos-view');
  await reposView.getByTestId('repo-row-wealdlore').click();

  const popover = page.getByTestId('repo-detail-popover');
  await expect(popover).toBeVisible();
  await expect(popover.getByText('wealdlore', { exact: true })).toBeVisible();
  await expect(popover.getByTestId('repo-hot-file').first()).toContainText('PlateBundle.swift');

  await popover.getByRole('button', { name: /open in code view/ }).click();
  await expect(popover).not.toBeVisible();

  // Landed on the Code view, showing that repo's rollup.
  await expect(page.getByTestId('code-view')).toBeVisible();
  const crumb = page.locator('.crumb');
  await expect(crumb.getByText('wealdlore', { exact: false })).toBeVisible();
});

test('renders correctly in both themes', async ({ page }) => {
  const reposView = page.getByTestId('repos-view');
  await expect(reposView.getByText('✓ tracked')).toBeVisible();

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(reposView.getByText('✓ tracked')).toBeVisible();
  await expect(reposView.getByText('◇ shadow · referenced, not registered')).toBeVisible();
});
