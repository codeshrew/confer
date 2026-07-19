import { test, expect } from '@playwright/test';

// Piece 11 Phase 1 (ui/redesign-mockups/11-code-view-BUILD-BRIEF.md) — the
// anchored reader: clicking a code conversation from WITHIN Code view no
// longer yanks the operator into Chat (the reported "#1 fix"). Code stays
// put, the clicked range's conversations read in the persistent right-rail
// reader, and "open full thread ›" is the ONLY opt-in path back to Chat.
// Mock fixtures: PlateBundle.swift's L44–49 carries 3 real hits (reader,
// pipeline, and a private compositor note).

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Code', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Code', exact: true }).click();
  await expect(page.getByTestId('code-view')).toBeVisible();
});

test('clicking a hot line opens the anchored reader in place — Code stays put, does not jump to Chat', async ({ page }) => {
  const hotLine = page.locator('.dens.hit').first();
  await expect(hotLine).toBeVisible();
  await hotLine.click();

  // Still Code — the tab, and the code pane itself, both stayed exactly
  // where they were. This is the actual bug fix.
  await expect(page.getByRole('tab', { name: 'Code', exact: true })).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByTestId('code-view')).toBeVisible();

  const drawer = page.getByTestId('right-drawer');
  await expect(drawer.getByTestId('anchor-scope')).toBeVisible();
  await expect(drawer.getByTestId('anchored-conv')).toBeVisible();
});

test('the scope header locks to the clicked range, with "↩ whole file" to widen', async ({ page }) => {
  const hotLine = page.locator('.dens.hit').first();
  await hotLine.click();

  const drawer = page.getByTestId('right-drawer');
  const scope = drawer.getByTestId('anchor-scope');
  await expect(scope).toContainText('PlateBundle.swift');
  // The whole-file glyph (▤) only appears once widened — narrowed to a
  // range shows the range glyph (▐) instead.
  await expect(scope).toContainText('▐');

  await drawer.getByRole('button', { name: 'whole file' }).click();
  await expect(scope).toContainText('▤');
  await expect(scope).not.toContainText('▐');
});

test('multiple conversations on the range: one focused + expanded, the rest a "‹ N more" strip, j/k steps between them', async ({ page }) => {
  const hotLine = page.locator('.dens.hit').first();
  await hotLine.click();

  const drawer = page.getByTestId('right-drawer');
  await expect(drawer.getByTestId('anchored-conv')).toBeVisible();
  const morePills = drawer.getByTestId('anchored-more-pill');
  await expect(morePills).toHaveCount(2);

  const firstBody = await drawer.getByTestId('anchored-conv').textContent();

  // Clicking a "more" pill swaps the focus — a DIFFERENT conversation now
  // expands, and the pill count stays at 2 (still one focused, N-1 more).
  await morePills.first().click();
  const secondBody = await drawer.getByTestId('anchored-conv').textContent();
  expect(secondBody).not.toBe(firstBody);
  await expect(drawer.getByTestId('anchored-more-pill')).toHaveCount(2);

  // j/k also steps focus — the keydown bubbles up from whatever's already
  // focused (the pill just clicked) to the panel's own handler, no extra
  // focus() call needed.
  await page.keyboard.press('j');
  const thirdBody = await drawer.getByTestId('anchored-conv').textContent();
  expect(thirdBody).not.toBe(secondBody);
});

test('"open full thread ›" is the ONLY thing that leaves Code — never a bare row/pill click', async ({ page }) => {
  const hotLine = page.locator('.dens.hit').first();
  await hotLine.click();

  const drawer = page.getByTestId('right-drawer');
  // Clicking the "more" strip stayed in Code (proven by the previous test);
  // only the expanded card's own explicit link navigates.
  await drawer.getByTestId('open-full-thread').click();

  await expect(page.getByRole('tab', { name: 'Chat', exact: true })).toHaveAttribute('aria-selected', 'true');
});

test('renders correctly in both themes', async ({ page }) => {
  const hotLine = page.locator('.dens.hit').first();
  await hotLine.click();
  const drawer = page.getByTestId('right-drawer');
  await expect(drawer.getByTestId('anchored-conv')).toBeVisible();

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(drawer.getByTestId('anchored-conv')).toBeVisible();
  await expect(drawer.getByTestId('anchored-more-pill').first()).toBeVisible();
});
