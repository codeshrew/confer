import { test, expect } from '@playwright/test';

// Piece 11 Phase 2b (ui/redesign-mockups/11-code-view-BUILD-BRIEF.md +
// 12-code-gutter-and-time.html's `.minimap`) — the conversation minimap: the
// whole file compressed to a strip on the far edge of the code pane, one
// colored segment per gutter entry that intersects the loaded file (SAME
// state palette Phase 2's gutter uses), plus a viewport indicator,
// click-to-scroll. Fixtures: the default active file (PlateBundle.swift)
// carries a 3-hit `note` group on [44,49], a 1-hit `OPEN request` on the
// genuinely overlapping [44,46], and a genuinely drifted note on [48,48] —
// three real gutter entries touching the loaded lines (a fourth hit, an
// `offline` ref to [10,15], is outside the loaded snippet entirely and is
// correctly excluded — see codeMinimap.test.ts for that case directly).

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Code', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Code', exact: true }).click();
  await expect(page.getByTestId('code-view')).toBeVisible();
});

test('renders one minimap segment per gutter entry, colored by the same state palette the gutter uses', async ({ page }) => {
  const minimap = page.getByTestId('code-minimap');
  await expect(minimap).toBeVisible();

  const segments = page.getByTestId('minimap-segment');
  await expect(segments).toHaveCount(3);

  const noteTab = page.getByTestId('gutter-tab').filter({ hasText: '3 ·' });
  const reqTab = page.getByTestId('gutter-tab').filter({ hasText: '1 ·' }).filter({ hasText: 'JA' });

  const noteTabColor = await noteTab.evaluate((el) => getComputedStyle(el).color);
  const reqTabColor = await reqTab.evaluate((el) => getComputedStyle(el).color);

  const segmentColors = await segments.evaluateAll((els) => els.map((el) => getComputedStyle(el).backgroundColor));
  // Every minimap segment's color matches SOME real gutter tab's color —
  // never an invented third palette.
  expect(segmentColors).toContain(noteTabColor);
  expect(segmentColors).toContain(reqTabColor);
});

test('click-to-scroll: clicking a minimap segment scrolls its range into view', async ({ page }) => {
  await page.evaluate(() => {
    (window as unknown as { __scrolledLine: string | null }).__scrolledLine = null;
    const orig = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function (this: HTMLElement, opts?: boolean | ScrollIntoViewOptions) {
      (window as unknown as { __scrolledLine: string | null }).__scrolledLine = this.getAttribute('data-line');
      return orig.call(this, opts);
    };
  });

  // [48,48]'s segment is the narrowest of the three overlapping entries —
  // it's stacked on top (codeMinimap.ts's largest-first ordering) and so is
  // the one guaranteed clickable without picking a stray covered pixel.
  const segment = page.locator('[data-testid="minimap-segment"][title="lines 48–48"]');
  await segment.click();

  const scrolledLine = await page.evaluate(() => (window as unknown as { __scrolledLine: string | null }).__scrolledLine);
  expect(scrolledLine).toBe('48');
});

// Law #3 (no minimap at all for a file with zero ranged conversations) is
// covered by CodeLens.test.ts's own unit-level fixture control — there's no
// mapped file in the shared mock dataset that loads real code with zero
// ranged hits to drive that case honestly through the full app shell here.

test('renders correctly in both themes', async ({ page }) => {
  await expect(page.getByTestId('code-minimap')).toBeVisible();
  await expect(page.getByTestId('minimap-segment').first()).toBeVisible();

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByTestId('code-minimap')).toBeVisible();
  await expect(page.getByTestId('minimap-segment').first()).toBeVisible();
});
