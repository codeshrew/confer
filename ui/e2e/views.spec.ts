import { test, expect } from '@playwright/test';

// Desktop (1440x900). Mock fixtures (src/lib/mock.ts) back every view in dev
// mode, so a fresh load renders a fully populated dashboard with no backend.

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  // Let the initial hub load settle before asserting. design/47: the
  // default landing is now Overview, not Chat — every test in this file
  // exercises a specific per-hub view, so it switches there explicitly.
  await expect(page.getByRole('tab', { name: 'Chat', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
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
  // Scoped to the Board pane itself — with Overview kept alive (not
  // destroyed) in the background for instant tab-switching, its own
  // Coordination-lane card for this same request ("BLOCKED · ...") repeats
  // the summary text, which would otherwise make this match ambiguous.
  const boardView = page.getByTestId('board-view');

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
  await expect(boardView.getByText('Freeze the CSL schema — needs a decision from Herald')).toBeVisible();

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

test('Code shows the file tree (design/43 Phase B — left rail) and a code view', async ({ page }) => {
  await page.getByRole('tab', { name: 'Code', exact: true }).click();

  // The file tree now lives in the left-rail slot (CodeTree.svelte), not
  // inside the code pane itself — scope to it via its own testid.
  const tree = page.getByTestId('code-tree');
  // Scoped to the Code pane itself — with Chat kept alive (not destroyed)
  // in the background for instant tab-switching, a Chat message's own
  // CodeRefCard can render the exact same file path / highlighted token
  // text, which would otherwise make these matches ambiguous.
  const codeView = page.getByTestId('code-view');

  await expect(tree.getByRole('button', { name: /PlateBundle\.swift/ })).toBeVisible();
  await expect(tree.getByRole('button', { name: /wealdlore/ })).toBeVisible();
  // The unified breadcrumb (absorbing the old codetool `repo › path` line)
  // now lives in the App crumb, above the code pane.
  const crumb = page.locator('.crumb');
  await expect(crumb.getByText('PlateBundle.swift', { exact: true })).toBeVisible();
  // A highlighted code line from the mock snippet.
  await expect(codeView.getByText('assembleBundle').first()).toBeVisible();

  // Switching files (via the tree, expanding pipeline/ first) swaps the
  // code view and the breadcrumb.
  await tree.getByRole('button', { name: 'pipeline/' }).click();
  await tree.getByRole('button', { name: /plates\.py/ }).click();
  await expect(crumb.getByText('plates.py', { exact: true })).toBeVisible();
});

test('Code: filtering the tree finds a file, and Tree|Active toggles the presentation', async ({ page }) => {
  await page.getByRole('tab', { name: 'Code', exact: true }).click();
  const tree = page.getByTestId('code-tree');

  await tree.getByLabel('Filter code files').fill('plates');
  await expect(tree.getByRole('listbox', { name: 'Filter matches' })).toBeVisible();
  // The match's highlighted substring splits the label across sibling
  // <span>s, so the browser's accname algorithm inserts spaces at each
  // boundary — match on the `title` (the plain, unsplit `repo/path`)
  // instead of the accessible name here.
  await expect(tree.locator('button[title="wealdlore/pipeline/plates.py"]')).toBeVisible();
  await expect(tree.getByRole('button', { name: /PlateBundle\.swift/ })).toHaveCount(0);

  await tree.getByLabel('Filter code files').press('Escape');
  await expect(tree.getByRole('listbox')).toHaveCount(0);

  await tree.getByRole('button', { name: 'Active', exact: true }).click();
  await expect(tree.getByRole('button', { name: /PlateBundle\.swift/ })).toBeVisible();
});
