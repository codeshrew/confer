import { test, expect } from '@playwright/test';

// design/47 — the cross-hub Overview/Health view. Mock fixtures
// (src/lib/mock.ts) back every hub in dev mode, so a fresh load exercises
// the real fan-out (getHubs + getOverview per hub) with no backend.

test('Overview is the default landing — a fresh load shows it, not a hub Chat', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Overview', exact: true })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByTestId('overview-view')).toBeVisible();
});

test('shows the three lanes with a real key-mismatch and a stale request, each naming who to talk to', async ({ page }) => {
  await page.goto('/');

  const needsYou = page.getByTestId('lane-needs-you');
  await expect(needsYou.getByText(/KEY MISMATCH/)).toBeVisible();
  await expect(needsYou.getByText(/verify Sentinel locally/)).toBeVisible();

  const coordination = page.getByTestId('lane-coordination');
  await expect(coordination.getByText(/STALE/).first()).toBeVisible();
  await expect(coordination.getByText(/nudge herald/)).toBeVisible();

  const fleetHealth = page.getByTestId('lane-fleet-health');
  await expect(fleetHealth.getByText('Reader', { exact: true })).toBeVisible();

  // The context strip's per-hub rollup.
  await expect(page.getByTestId('ov-context-strip')).toContainText('agent-coord');
});

test('opening a coordination card\'s thread drills into that hub\'s Board', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('lane-coordination').getByRole('button', { name: /open thread/i }).first().click();

  await expect(page.getByRole('tab', { name: 'Board', exact: true })).toHaveAttribute('aria-selected', 'true');
});

test('the all-clear scenario collapses every lane to a single calm line', async ({ page }) => {
  await page.goto('/?clear');

  await expect(page.getByTestId('needs-you-clear')).toHaveText('✓ nothing needs you');
  await expect(page.getByTestId('coordination-clear')).toHaveText('✓ nothing stuck or unowned');
  await expect(page.getByText('✓ all clear')).toBeVisible();
});

test('renders correctly in both themes', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('overview-view')).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByTestId('overview-view')).toBeVisible();
});
