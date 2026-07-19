import { test, expect } from '@playwright/test';

// Piece 10 Phase A (ui/redesign-mockups/10-navigation-history-RESEARCH.md) —
// the overlay back-stack fixing the reported bug: Fleet -> click an agent
// (AgentDossier opens) -> click one of their tickets used to REPLACE the
// dossier instead of stacking over it, so `esc` landed nowhere. Now it
// `push`es on top; `esc`/"‹ back" pops exactly one layer, landing back on
// the dossier. Pipeline carries `req_01JQa91` ("Run the alignment pass over
// the restored plate set") in the mock fixtures.

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('tab', { name: 'Fleet', exact: true })).toBeVisible();
  await page.getByRole('tab', { name: 'Fleet', exact: true }).click();
});

async function openPipelineDossierAndItsTicket(page: import('@playwright/test').Page) {
  const fleetView = page.getByTestId('fleet-view');
  const cards = fleetView.getByTestId('fleet-presence-card');
  const pipelineCard = cards.filter({ hasText: 'Pipeline' });
  await pipelineCard.click();

  const dossier = page.getByTestId('agent-dossier');
  await expect(dossier).toBeVisible();

  await dossier.getByText('Run the alignment pass over the restored plate set').click();
  await expect(page.getByTestId('ticket-popover')).toBeVisible();
}

test('the exact reported repro: Fleet → agent dossier → one of their tickets → esc returns to the dossier, not nowhere', async ({ page }) => {
  await openPipelineDossierAndItsTicket(page);

  // Only the TOP of the stack renders — the ticket covers the dossier
  // (a real card stack, not two overlapping modals) — but its CONTEXT
  // survived, proven by a single esc bringing it straight back.
  await expect(page.getByTestId('agent-dossier')).not.toBeVisible();

  await page.keyboard.press('Escape');
  await expect(page.getByTestId('ticket-popover')).not.toBeVisible();
  await expect(page.getByTestId('agent-dossier')).toBeVisible();

  // A second esc closes the dossier too, back to nothing — normal for a
  // top-level (non-nested) popover.
  await page.keyboard.press('Escape');
  await expect(page.getByTestId('agent-dossier')).not.toBeVisible();
});

test('the nested ticket popover shows a "‹ back" affordance, which pops exactly the same as esc', async ({ page }) => {
  await openPipelineDossierAndItsTicket(page);

  const ticketPopover = page.getByTestId('ticket-popover');
  const back = ticketPopover.getByRole('button', { name: 'Back' });
  await expect(back).toBeVisible();

  await back.click();
  await expect(ticketPopover).not.toBeVisible();
  await expect(page.getByTestId('agent-dossier')).toBeVisible();
});

test('a TOP-LEVEL ticket open (Chat, no dossier involved) shows NO back affordance — it is not nested', async ({ page }) => {
  await page.getByRole('tab', { name: 'Chat', exact: true }).click();
  await page.getByText('Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader').click();

  const ticketPopover = page.getByTestId('ticket-popover');
  await expect(ticketPopover).toBeVisible();
  await expect(ticketPopover.getByRole('button', { name: 'Back' })).not.toBeVisible();
});

test('j/k navigating between tickets while nested under the dossier stays nested — one esc still reaches the dossier', async ({ page }) => {
  await openPipelineDossierAndItsTicket(page);

  const ticketPopover = page.getByTestId('ticket-popover');
  await ticketPopover.focus();
  await page.keyboard.press('k');
  await expect(ticketPopover).toBeVisible();

  // A SINGLE esc (not two) must already be back at the dossier — proof j/k
  // swapped content in place rather than pushing a second ticket frame.
  await page.keyboard.press('Escape');
  await expect(ticketPopover).not.toBeVisible();
  await expect(page.getByTestId('agent-dossier')).toBeVisible();
});

test('renders correctly in both themes', async ({ page }) => {
  // Every overlay here (dossier, ticket) has its own full-viewport backdrop
  // that blocks the TopBar's theme toggle while showing — esc all the way
  // back out before toggling, then re-open to confirm the nested view (and
  // its "‹ back" affordance) still renders correctly in light.
  await page.getByRole('button', { name: 'Toggle theme' }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');

  await openPipelineDossierAndItsTicket(page);
  const ticketPopover = page.getByTestId('ticket-popover');
  await expect(ticketPopover.getByRole('button', { name: 'Back' })).toBeVisible();

  await page.keyboard.press('Escape');
  await expect(page.getByTestId('agent-dossier')).toBeVisible();
});
