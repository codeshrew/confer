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

test('Chat shows the message stream and a ticket mini card', async ({ page }) => {
  await expect(page.getByText('Shipping confer 0.7.3')).toBeVisible();
  // The filed request renders as a TicketMiniCard (piece 5's card trio),
  // not a plain note — kind tag, id, mini progress + state label.
  const mini = page.getByTestId('ticket-mini').filter({ hasText: 'req_01JQ8f2' });
  await expect(mini).toBeVisible();
  await expect(mini.getByText('req', { exact: true })).toBeVisible();
  await expect(mini.getByText('Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader')).toBeVisible();
  // req_01JQ8f2 is DONE in the fixtures — all three progress knobs fill.
  await expect(mini.getByText('done', { exact: true })).toBeVisible();

  // Clicking it portals to the Full popover.
  await mini.click();
  await expect(page.getByTestId('ticket-popover')).toBeVisible();
});

test('Board is a triage cockpit — real stats, workload, throughput, grouped work, DONE folded', async ({ page }) => {
  await page.getByRole('tab', { name: 'Board', exact: true }).click();
  const boardView = page.getByTestId('board-view');

  // The four questions, as real numbers.
  await expect(boardView.getByTestId('board-stat-open')).toBeVisible();
  await expect(boardView.getByTestId('board-stat-flight')).toBeVisible();
  await expect(boardView.getByTestId('board-stat-stuck')).toBeVisible();
  await expect(boardView.getByTestId('board-stat-unowned')).toBeVisible();

  // The real visuals.
  await expect(boardView.getByText('carrying')).toBeVisible();
  await expect(boardView.getByText('asking')).toBeVisible();
  await expect(boardView.getByText('closure throughput')).toBeVisible();

  // The actionable work, grouped — a stuck fixture is visible by default.
  await expect(boardView.getByText('Freeze the CSL schema — needs a decision from Herald')).toBeVisible();

  // DONE is collapsed behind a fold, not dominating the page.
  const fold = boardView.getByTestId('board-done-fold');
  await expect(fold).toBeVisible();
  await expect(boardView.getByText('Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader')).not.toBeVisible();
  await fold.click();
  await expect(boardView.getByText('Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader')).toBeVisible();

  // A stat card filters the work lists down to just that state.
  await page.getByTestId('board-stat-stuck').click();
  await expect(boardView.getByTestId('board-filter-chips')).toBeVisible();
  await expect(boardView.getByText('Freeze the CSL schema — needs a decision from Herald')).toBeVisible();
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

test('Board cockpit renders correctly in both themes', async ({ page }) => {
  await page.getByRole('tab', { name: 'Board', exact: true }).click();
  const boardView = page.getByTestId('board-view');
  await expect(boardView.getByTestId('board-stat-open')).toBeVisible();

  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(boardView.getByTestId('board-stat-open')).toBeVisible();
  await expect(boardView.getByText('closure throughput')).toBeVisible();
});

test('Board\'s left rail is a Fleet-as-filter (not the chat channel list) — click an agent to filter', async ({ page }) => {
  await page.getByRole('tab', { name: 'Board', exact: true }).click();
  const boardView = page.getByTestId('board-view');
  const fleetRail = page.getByTestId('board-fleet-rail');

  await expect(fleetRail).toBeVisible();
  await expect(fleetRail.getByText('Compositor')).toBeVisible();
  // Not the chat topic list — no "#" topic entries here.
  await expect(fleetRail.getByText(/^#/)).toHaveCount(0);

  await fleetRail.getByText('Compositor').click();
  await expect(boardView.getByTestId('board-filter-chips')).toContainText('Compositor');
  // Compositor is the claimant on the "Freeze the CSL schema" ticket.
  await expect(boardView.getByText('Freeze the CSL schema — needs a decision from Herald')).toBeVisible();

  // Collapsing hides the agent list without losing the active filter.
  await page.getByRole('button', { name: /collapse fleet filter/i }).click();
  await expect(fleetRail.getByText('Compositor')).not.toBeVisible();
  await expect(boardView.getByTestId('board-filter-chips')).toContainText('Compositor');
});
