import { test, expect } from '@playwright/test';

// Piece 11 Phase 5 (ui/redesign-mockups/11-code-view-BUILD-BRIEF.md + mock
// 12's `.tl`/`.spine`/`.node`) — the sidebar conversation timeline, the
// range-biography (mock 13) MVP: the current scope's real conversations,
// oldest→newest, each pinned to a real version — green "this" on the
// viewed sha, amber "older" otherwise, with an opt-in "↳ align code to
// this version" action that composes with Phase 4's rev chip. Fixture:
// the default whole-file scope's real hits include mock.ts's pre-44
// legacy note (`sha: 'HEAD'`), which genuinely doesn't match the file's
// normally-pinned sha — a real "older" node, no synthetic data needed.

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Code', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Code', exact: true }).click();
  await expect(page.getByTestId('code-view')).toBeVisible();
});

test('the timeline shows real nodes oldest→newest, one genuinely "older" against the pinned view', async ({ page }) => {
  await expect(page.getByTestId('rev-chip')).toHaveClass(/pinned/);

  const timeline = page.getByTestId('conversation-timeline');
  await expect(timeline).toBeVisible();

  const nodes = page.getByTestId('timeline-node');
  const count = await nodes.count();
  expect(count).toBeGreaterThan(1);

  // Real ordering: the FIRST node is the oldest — mock.ts's pre-44 legacy
  // note (2026-06-02), earlier than every other PlateBundle.swift hit.
  await expect(nodes.first()).toContainText('early note, pinned before confer captured a real sha');
  await expect(nodes.first()).toHaveClass(/old/);
  await expect(nodes.first()).toContainText('HEAD');

  // Most other real hits share the file's currently-pinned sha — "this".
  const curCount = await page.locator('[data-testid="timeline-node"].cur').count();
  expect(curCount).toBeGreaterThan(0);
});

test('align-to-revision re-pins the code AND flips Phase 4\'s rev chip live', async ({ page }) => {
  const headNode = page.locator('[data-testid="timeline-node"]', { hasText: 'HEAD' });
  await expect(headNode).toHaveClass(/old/);

  await headNode.getByTestId('align-to-revision').click();

  await expect(page.getByTestId('rev-chip')).toHaveClass(/head/);
  await expect(page.getByTestId('rev-chip')).toContainText('HEAD');
  // The node that was just aligned to now reads "this" itself — the SAME
  // store both the rev chip and the timeline read from.
  await expect(headNode).toHaveClass(/cur/);
});

test('reading a node (clicking its body) focuses the accordion but does NOT move the code — opt-in only', async ({ page }) => {
  // Wait for the settled pinned state before capturing a baseline — the rev
  // chip briefly shows the store's transient default ('HEAD') before the
  // real fetch resolves.
  await expect(page.getByTestId('rev-chip')).toHaveClass(/pinned/);
  const chipBefore = await page.getByTestId('rev-chip').textContent();

  const nodes = page.getByTestId('timeline-node-focus');
  await nodes.nth(1).click();

  // A real, visible effect of reading: the accordion's expanded card changed.
  await expect(page.getByTestId('anchored-conv')).toBeVisible();
  // But the code pane's own revision did NOT move.
  await expect(page.getByTestId('rev-chip')).toHaveText(chipBefore ?? '');
});

test('renders correctly in both themes', async ({ page }) => {
  await expect(page.getByTestId('conversation-timeline')).toBeVisible();
  await expect(page.getByTestId('timeline-node').first()).toBeVisible();

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByTestId('conversation-timeline')).toBeVisible();
  await expect(page.getByTestId('timeline-node').first()).toBeVisible();
});
