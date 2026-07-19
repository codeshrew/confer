import { test, expect } from '@playwright/test';

// design/47 — the cross-hub Overview/Health view. Mock fixtures
// (src/lib/mock.ts) back every hub in dev mode, so a fresh load exercises
// the real fan-out (getHubs + getOverview per hub) with no backend.
//
// Redesigned 2026-07-18 (ui/REDESIGN.md piece 1) around the fleet map: hub
// domain cards with agent nodes (position=identity, appearance=state) plus
// a merged "needs you" overlay anchored to the map, replacing the old
// three-flat-lanes layout.

test('Overview is the default landing — a fresh load shows it, not a hub Chat', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Overview', exact: true })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByTestId('overview-view')).toBeVisible();
});

test('shows the fleet map with a real key-mismatch agent and a stale request, each naming who to talk to', async ({ page }) => {
  await page.goto('/');

  const attn = page.getByTestId('ov-attention');
  await expect(attn.getByText(/KEY MISMATCH/)).toBeVisible();
  await expect(attn.getByText(/verify Sentinel locally/)).toBeVisible();
  await expect(attn.getByText(/STALE/).first()).toBeVisible();
  await expect(attn.getByText(/nudge herald/)).toBeVisible();

  // The domain map shows the same agents the overlay points back at.
  const map = page.getByTestId('ov-map');
  await expect(map.getByTestId('agent-node').filter({ hasText: 'Reader' })).toBeVisible();

  // The context strip's per-hub rollup.
  await expect(page.getByTestId('ov-context-strip')).toContainText('agent-coord');
});

test('opening an overlay row\'s thread drills into that hub\'s Board', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('ov-attention').getByRole('button', { name: /open thread/i }).first().click();

  await expect(page.getByRole('tab', { name: 'Board', exact: true })).toHaveAttribute('aria-selected', 'true');
});

test('the all-clear scenario collapses the overlay to a single calm line', async ({ page }) => {
  await page.goto('/?clear');

  await expect(page.getByTestId('attention-clear')).toHaveText('✓ nothing needs you');
  await expect(page.getByText('✓ Steady')).toBeVisible();
});

test('renders correctly in both themes', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('overview-view')).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByTestId('overview-view')).toBeVisible();
});
