import { test, expect } from '@playwright/test';

// piece 4, item 3 (ui/REDESIGN.md, redesign-mockups/04-chat-glance.html):
// sticky day header, true summary density (chips only, no rendered blocks),
// inline refs anchored to prose in Full, and minimap-styled prev/next.

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Chat', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
});

test('Summary density shows one line + chips, never the rendered code/refs — Full reveals them, anchored inline', async ({ page }) => {
  // "bundle assembly wired, pinned to a3f1c9" has both a fenced code block
  // and a real code ref in the mock fixtures.
  const summaryLine = page.getByText('bundle assembly wired, pinned to a3f1c9');
  await expect(summaryLine).toBeVisible();
  const noteRow = page.locator('.msg').filter({ has: summaryLine });

  // Summary: chips present, nothing rendered.
  await expect(noteRow.locator('.chip.ref, .chip.code').first()).toBeVisible();
  await expect(page.getByText('Wired it — bundle assembly is here')).not.toBeVisible();

  // Full: the real body renders, and the code-ref card no longer trails
  // separately — the same ref is anchored inline as a chip within the text.
  await page.getByRole('button', { name: 'Full', exact: true }).click();
  await expect(page.getByText('Wired it — bundle assembly is here')).toBeVisible();
  await expect(page.locator('.inline-ref-chip').first()).toBeVisible();
});

test('clicking the anchored inline ref chip opens the reverse-index panel with real data', async ({ page }) => {
  await page.getByRole('button', { name: 'Full', exact: true }).click();
  const chip = page.locator('.inline-ref-chip').first();
  await expect(chip).toBeVisible();
  await chip.click();

  const drawer = page.getByTestId('right-drawer');
  await expect(drawer.getByText('Reverse index')).toBeVisible();
  await expect(drawer.getByText('PlateBundle.swift')).toBeVisible();
});

test('the sticky day header stays pinned to the top of the stream while scrolling', async ({ page }) => {
  const stream = page.locator('.stream');
  await stream.waitFor();
  const dayHeader = page.locator('.daybreak').first();
  await expect(dayHeader).toBeVisible();

  const beforeScroll = await dayHeader.boundingBox();
  // A regular (non-sticky) element would move up by roughly the scroll
  // distance; a genuinely PINNED one stays at the same screen position —
  // that's the whole test, no need to reason about the container's own
  // padding offset.
  await stream.evaluate((el) => el.scrollBy(0, 300));
  await page.waitForTimeout(150);
  const afterScroll = await dayHeader.boundingBox();

  expect(afterScroll?.y).toBeCloseTo(beforeScroll?.y ?? -1, 0);
});

test('the sticky day bar sits flush — no gap above it (crumb header) or to its right (the scrollbar track), with a REAL scrollbar present', async ({ page }) => {
  // A short viewport forces the stream to overflow reliably, and
  // Playwright's bundled Chromium renders real (non-overlay) scrollbars —
  // exercising exactly the gap this polish pass fixed (Stefan spotted it
  // live: the bar's negative right margin only cancelled .stream's own
  // padding, not the scrollbar TRACK sitting inside it too).
  await page.setViewportSize({ width: 1200, height: 500 });
  await page.getByRole('button', { name: 'Full', exact: true }).click();
  await page.getByText('Wired it').first().waitFor();

  const metrics = await page.evaluate(() => {
    const stream = document.querySelector('.stream')!;
    const bar = document.querySelector('.daybreak')!;
    const sRect = stream.getBoundingClientRect();
    const bRect = bar.getBoundingClientRect();
    return {
      hasScrollbar: stream.scrollHeight > stream.clientHeight,
      topGap: bRect.top - sRect.top,
      rightGap: sRect.right - bRect.right,
    };
  });

  expect(metrics.hasScrollbar).toBe(true); // sanity — the scenario this test exists for
  expect(metrics.topGap).toBeCloseTo(0, 0);
  expect(metrics.rightGap).toBeCloseTo(0, 0);
});

test('the focus reader\'s prev/next uses the minimap\'s dot + kind vocabulary', async ({ page }) => {
  await page.getByText(/canaried 0.7.3/).first().click();
  await page.keyboard.press('f');

  const reader = page.getByTestId('focus-reader');
  await expect(reader).toBeVisible();
  const current = reader.locator('.pn-step.cur');
  await expect(current).toBeVisible();
  await expect(current.locator('.kn')).toBeVisible();
  await expect(current.locator('.kd2')).toBeVisible();
});

test('renders correctly in both themes', async ({ page }) => {
  await page.getByRole('button', { name: 'Full', exact: true }).click();
  await expect(page.getByText('Wired it — bundle assembly is here')).toBeVisible();

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.locator('.daybreak').first()).toBeVisible();
});
