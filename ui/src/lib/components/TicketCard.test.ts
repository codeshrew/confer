import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import TicketCard from './TicketCard.svelte';
import type { RequestRow } from '../types';

const doneRequest: RequestRow = {
  id: 'req_01JQ8f2',
  from: 'pipeline',
  to: ['reader'],
  summary: 'Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader',
  status: 'DONE',
  resolution: 'endpoint live, tests green',
  deferred: false,
  claimants: ['reader'],
  ageSecs: 2400,
  stale: false,
  topic: 'reader',
};

const blockedRequest: RequestRow = {
  id: 'req_01JQc4a',
  from: 'reader',
  to: ['compositor'],
  summary: 'Freeze the CSL schema — needs a decision from Herald',
  status: 'BLOCKED',
  resolution: null,
  deferred: false,
  claimants: ['compositor'],
  ageSecs: 7200,
  stale: false,
  topic: 'studio-markup',
};

describe('TicketCard', () => {
  it('renders the torn stub — serial, status stamp — and the title/route', () => {
    const { container } = render(TicketCard, { request: doneRequest });

    expect(screen.getByText('8F2')).toBeInTheDocument();
    expect(container.querySelector('.stub .stamp')?.textContent).toBe('done');
    expect(screen.getByText(doneRequest.summary)).toBeInTheDocument();
    expect(screen.getByText(/req_01JQ8f2/)).toBeInTheDocument();
  });

  it('renders the lifecycle track with filed/claim/done lit for a DONE request', () => {
    const { container } = render(TicketCard, { request: doneRequest });

    const litNodes = container.querySelectorAll('.track .n.done');
    // filed, claim, done all reached for a resolved ticket.
    expect(litNodes.length).toBe(3);
    expect(screen.getByText('filed')).toBeInTheDocument();
    expect(screen.getByText('claim')).toBeInTheDocument();
  });

  it('tints the stub/stamp for a BLOCKED request and marks "blocked" as current', () => {
    const { container } = render(TicketCard, { request: blockedRequest });

    expect(container.querySelector('.ticket.s-blocked')).toBeInTheDocument();
    expect(container.querySelector('.stub .stamp')?.textContent).toBe('blocked');
    expect(container.querySelector('.track .n.cur')).toBeInTheDocument();
  });

  it('fires onSelect with the request id when clicked', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(TicketCard, { request: doneRequest, onSelect });

    await user.click(screen.getByText(doneRequest.summary));

    expect(onSelect).toHaveBeenCalledWith('req_01JQ8f2');
  });

  it('stops propagation on click — a ticket is nested inside Message.svelte\'s own clickable row, so a ticket click must not also bubble up to it', async () => {
    // Full end-to-end regression coverage lives in App.test.ts ("selecting
    // a plain note after a ticket switches the sidebar OFF Request
    // detail"), which caught this exact bug via the real Message.svelte
    // nesting. Here: a listener on `document` stands in for that outer
    // ancestor — if the ticket's click reaches it, propagation wasn't
    // stopped.
    const user = userEvent.setup();
    const onSelect = vi.fn();
    const onDocumentClick = vi.fn();
    render(TicketCard, { request: doneRequest, onSelect });
    document.addEventListener('click', onDocumentClick);

    try {
      await user.click(screen.getByText(doneRequest.summary));
    } finally {
      document.removeEventListener('click', onDocumentClick);
    }

    expect(onSelect).toHaveBeenCalledWith('req_01JQ8f2');
    expect(onDocumentClick).not.toHaveBeenCalled();
  });
});
