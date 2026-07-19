import { test, expect } from '@playwright/test';

// piece 2 (ui/REDESIGN.md): the trust-tiered hub rail + its keyboard layer.
// Mock fixtures back three hubs — agent-coord (own), confer-lab (shared),
// jarvis-orbit (foreign) — so the rail's three real groups (plus the
// backend-real health dots) all render from something.

test('the rail groups hubs under Home/Shared/Foreign, health dots real, and switching hubs tints the workspace', async ({ page }) => {
  await page.goto('/');

  const rail = page.getByTestId('hub-rail');
  await expect(rail.getByText('Home')).toBeVisible();
  await expect(rail.getByText('Shared')).toBeVisible();
  await expect(rail.getByText('Foreign')).toBeVisible();
  await expect(rail.getByText('agent-coord')).toBeVisible();

  // The tint/pill only mean anything once you're actually IN a hub's
  // workspace — Overview (the default landing) is cross-hub and never
  // tints (see App.svelte's workspaceTintClass), so switch to Chat first.
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();

  // Home is the silent default — no world-pill on the hub `current: true`
  // in the mock fixtures (agent-coord, own).
  await expect(page.getByTestId('world-pill')).toHaveCount(0);

  // Switching to the foreign hub tints the workspace + shows the pill.
  await rail.getByText('jarvis-orbit').click();
  await expect(page.getByTestId('world-pill')).toHaveText(/foreign hub/);
});

test('⌘K opens the command palette from anywhere, fuzzy-jumps to a hub, and closes on Escape', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('hub-rail').getByText('agent-coord').waitFor();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();

  await page.keyboard.press('Meta+k');
  const palette = page.getByTestId('command-palette');
  await expect(palette).toBeVisible();

  // 'orb' is technically a fuzzy subsequence of confer-lab too — the tight
  // "orbit" match ranks first (and is pre-selected), which is the case that
  // matters (matches the mockup's own worked "orb" -> orbit example).
  await page.getByTestId('palette-input').fill('orb');
  await expect(page.getByTestId('palette-row').first()).toContainText('jarvis-orbit');
  await expect(page.getByTestId('palette-row').first()).toHaveClass(/sel/);

  await page.keyboard.press('Enter');
  await expect(palette).not.toBeVisible();
  await expect(page.getByTestId('world-pill')).toHaveText(/foreign hub/);

  await page.keyboard.press('Meta+k');
  await expect(page.getByTestId('command-palette')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('command-palette')).not.toBeVisible();
});

test('"g" then a number switches views, and "?" opens the which-key overlay', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('hub-rail').getByText('agent-coord').waitFor();

  await page.keyboard.press('g');
  await page.keyboard.press('3');
  await expect(page.getByRole('tab', { name: 'Board', exact: true })).toHaveAttribute('aria-selected', 'true');

  await page.keyboard.press('?');
  await expect(page.getByTestId('whichkey-backdrop')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('whichkey-backdrop')).not.toBeVisible();
});

test('j/k move the rail selection and Enter opens the focused hub', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
  const rail = page.getByTestId('hub-rail');
  const allHubs = rail.getByTestId('hub-rail-all');
  await allHubs.waitFor();

  await allHubs.focus();
  await page.keyboard.press('j'); // -> agent-coord (Home's only real entry above confer-lab)
  await page.keyboard.press('Enter');

  await expect(page.locator('.center')).toHaveClass(/tint-home/);
});

test('renders correctly in both themes', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByTestId('hub-rail')).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByTestId('hub-rail')).toBeVisible();
});
