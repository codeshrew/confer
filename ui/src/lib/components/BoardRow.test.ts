import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import BoardRow from './BoardRow.svelte';
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

describe('BoardRow', () => {
  afterEach(() => {
    delete (navigator as { clipboard?: unknown }).clipboard;
  });

  it('fires onSelect with the request id when the row is clicked', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(BoardRow, { request, agents: [reader], statusVar: 'var(--claimed)', onSelect });

    await user.click(screen.getByText('plate-bundle endpoint'));

    expect(onSelect).toHaveBeenCalledWith('req_01JQ8f2');
  });

  it('shows the bare ticket id and exposes a copy-id control that copies without also selecting the row', async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    const onSelect = vi.fn();

    render(BoardRow, { request, agents: [reader], statusVar: 'var(--claimed)', onSelect });

    expect(screen.getByText('req_01JQ8f2')).toBeInTheDocument();
    const copyBtn = screen.getByRole('button', { name: /copy id req_01JQ8f2/i });
    await user.click(copyBtn);

    await vi.waitFor(() => {
      expect(writeText).toHaveBeenCalledWith('req_01JQ8f2');
    });
    expect(onSelect).not.toHaveBeenCalled();
  });
});
