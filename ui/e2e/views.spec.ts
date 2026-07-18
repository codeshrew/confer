import { test, expect } from '@playwright/test';

// Desktop (1440x900). Mock fixtures (src/lib/mock.ts) back every view in dev
// mode, so a fresh load renders a fully populated dashboard with no backend.

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  // Let the initial hub load settle before asserting.
  await expect(page.getByRole('tab', { name: 'Chat', exact: true })).toBeVisible();
});

test('Chat shows the message stream and a ticket card', async ({ page }) => {
  await expect(page.getByText('Shipping confer 0.7.3')).toBeVisible();
  // The filed request renders as a torn-stub TicketCard, not a plain note.
  await expect(page.getByText('Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader')).toBeVisible();
  await expect(page.getByText('req_01JQ8f2', { exact: true })).toBeVisible();
  // Its lifecycle track shows all four stages.
  await expect(page.getByText('filed', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('claim', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('blocked', { exact: true }).first()).toBeVisible();
  await expect(page.getByText('done', { exact: true }).first()).toBeVisible();
});

test('Board shows swimlanes and the group-by toggle regroups', async ({ page }) => {
  await page.getByRole('tab', { name: 'Board', exact: true }).click();

  // Default grouping is by status — the open/claimed/blocked/done lanes
  // (rendered visually uppercase via CSS text-transform; the underlying
  // lane-header text is lowercase). Scope to the lane-label testid since
  // e.g. "open"/"#reader" also legitimately appear elsewhere on the page
  // (row status stamps, topic tags, the meta-thread).
  const laneLabels = page.getByTestId('lane-label');
  await expect(laneLabels.filter({ hasText: /^open$/ })).toBeVisible();
  await expect(laneLabels.filter({ hasText: /^claimed$/ })).toBeVisible();
  await expect(laneLabels.filter({ hasText: /^blocked$/ })).toBeVisible();
  await expect(laneLabels.filter({ hasText: /^done$/ })).toBeVisible();
  await expect(page.getByText('Freeze the CSL schema — needs a decision from Herald')).toBeVisible();

  // Regroup by topic — lanes are now keyed by topic slug.
  await page.getByRole('button', { name: 'Topic', exact: true }).click();
  await expect(laneLabels.filter({ hasText: '#reader' })).toBeVisible();
  await expect(laneLabels.filter({ hasText: '#studio-markup' })).toBeVisible();

  // Regroup by claimant.
  await page.getByRole('button', { name: 'Claimant', exact: true }).click();
  await expect(laneLabels.filter({ hasText: 'unclaimed' })).toBeVisible();
});

test('Fleet shows agent identity cards', async ({ page }) => {
  await page.getByRole('tab', { name: 'Fleet', exact: true }).click();
  // Scoped to the Fleet pane itself — the left rail's own fleet roster also
  // lists agent names, which would otherwise make these matches ambiguous.
  const fleetView = page.getByTestId('fleet-view');

  await expect(fleetView.getByText('Herald', { exact: true })).toBeVisible();
  await expect(fleetView.getByText('Reader', { exact: true })).toBeVisible();
  await expect(fleetView.getByText('Pipeline', { exact: true })).toBeVisible();
  await expect(fleetView.getByText('Current WIP').first()).toBeVisible();
  // The dashboard viewer gets a first-class, non-claiming "You" card.
  await expect(fleetView.getByText('viewing this dashboard')).toBeVisible();
});

test('Code shows the file tree and a code view', async ({ page }) => {
  await page.getByRole('tab', { name: 'Code', exact: true }).click();

  // Scoped to the Code pane itself — with Chat kept alive (not destroyed)
  // in the background for instant tab-switching, a Chat message's own
  // CodeRefCard can render the exact same file path / highlighted token
  // text, which would otherwise make these matches ambiguous.
  const codeView = page.getByTestId('code-view');

  await expect(page.getByRole('button', { name: 'PlateBundle.swift' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'plates.py' })).toBeVisible();
  await expect(page.getByText('wealdlore', { exact: true })).toBeVisible();
  await expect(codeView.getByText('Sources/Reader/PlateBundle.swift')).toBeVisible();
  // A highlighted code line from the mock snippet.
  await expect(codeView.getByText('assembleBundle').first()).toBeVisible();

  // Switching files swaps the code view.
  await page.getByRole('button', { name: 'plates.py' }).click();
  await expect(codeView.getByText('pipeline/plates.py')).toBeVisible();
});
