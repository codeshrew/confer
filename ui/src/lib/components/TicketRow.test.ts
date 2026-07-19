import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import TicketRow from './TicketRow.svelte';
import type { Agent, RequestRow } from '../types';

const reader: Agent = {
  id: 'reader',
  display: 'Reader',
  desc: null,
  expectedHost: null,
  lastTs: null,
  lastHost: null,
  live: true,
  verified: 'signed',
  version: null,
  watchState: null,
  keyFingerprint: null,
  profileMarkdown: null,
  color: 'var(--ag-reader)',
  abbr: 'RE',
  wip: [],
};

const request: RequestRow = {
  id: 'req_01JQ8f2',
  from: 'pipeline',
  to: ['reader'],
  summary: 'plate-bundle endpoint',
  status: 'CLAIMED',
  resolution: null,
  deferred: false,
  claimants: ['reader'],
  ageSecs: 2400,
  stale: false,
  topic: 'reader',
};

describe('TicketRow', () => {
  afterEach(() => {
    delete (navigator as { clipboard?: unknown }).clipboard;
  });

  it('renders id, summary, and assignee avatar — no action buttons on the row itself', () => {
    render(TicketRow, { request, agents: [reader] });

    expect(screen.getByText('req_01JQ8f2')).toBeInTheDocument();
    expect(screen.getByText('plate-bundle endpoint')).toBeInTheDocument();
    expect(screen.getByTitle('Reader')).toHaveTextContent('RE');
    // Two button-role elements: the row itself (the whole row is one
    // button) and the copy-id affordance — no claim/open buttons (a human
    // doesn't "claim" from a list row).
    expect(screen.getAllByRole('button')).toHaveLength(2);
  });

  it('shows the unclaimed placeholder when nobody has claimed it', () => {
    render(TicketRow, { request: { ...request, claimants: [] }, agents: [reader] });
    expect(screen.getByTitle('unclaimed')).toHaveTextContent('–');
  });

  it('fires onSelect with the request id when the row is clicked — opens the Full popover', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(TicketRow, { request, agents: [reader], onSelect });

    await user.click(screen.getByText('plate-bundle endpoint'));
    expect(onSelect).toHaveBeenCalledWith('req_01JQ8f2');
  });

  it('fires onSelect on Enter, for keyboard-only activation', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(TicketRow, { request, agents: [reader], onSelect });

    await user.tab();
    await user.keyboard('{Enter}');
    expect(onSelect).toHaveBeenCalledWith('req_01JQ8f2');
  });

  it('the copy-id control copies without also selecting the row', async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    const onSelect = vi.fn();
    render(TicketRow, { request, agents: [reader], onSelect });

    const copyBtn = screen.getByRole('button', { name: /copy id req_01JQ8f2/i });
    await user.click(copyBtn);

    expect(writeText).toHaveBeenCalledWith('req_01JQ8f2');
    expect(onSelect).not.toHaveBeenCalled();
  });
});
