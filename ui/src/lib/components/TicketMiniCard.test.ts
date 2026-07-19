import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import TicketMiniCard from './TicketMiniCard.svelte';
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

describe('TicketMiniCard', () => {
  it('renders the kind tag, id, age, assignee avatar, and a 2-line summary', () => {
    render(TicketMiniCard, { request, agents: [reader] });

    expect(screen.getByText('req')).toBeInTheDocument();
    expect(screen.getByText('req_01JQ8f2')).toBeInTheDocument();
    expect(screen.getByText('plate-bundle endpoint')).toBeInTheDocument();
    expect(screen.getByTitle('Reader')).toHaveTextContent('RE');
  });

  it('labels a claimed ticket "claimed" with two filled progress knobs (Requested + Claimed)', () => {
    const { container } = render(TicketMiniCard, { request, agents: [reader] });
    expect(screen.getByText('claimed')).toBeInTheDocument();
    expect(container.querySelectorAll('.mini-prog .k.on')).toHaveLength(1);
    expect(container.querySelectorAll('.mini-prog .k.cur')).toHaveLength(1);
  });

  it('labels an unclaimed OPEN ticket "needs owner" with only Requested filled', () => {
    const open: RequestRow = { ...request, status: 'OPEN', claimants: [] };
    const { container } = render(TicketMiniCard, { request: open, agents: [reader] });
    expect(screen.getByText('needs owner')).toBeInTheDocument();
    expect(container.querySelectorAll('.mini-prog .k.on')).toHaveLength(1);
    expect(container.querySelectorAll('.mini-prog .k.cur')).toHaveLength(0);
  });

  it('labels a DONE ticket "done" with all three knobs filled', () => {
    const done: RequestRow = { ...request, status: 'DONE' };
    const { container } = render(TicketMiniCard, { request: done, agents: [reader] });
    expect(screen.getByText('done')).toBeInTheDocument();
    expect(container.querySelectorAll('.mini-prog .k.on')).toHaveLength(3);
  });

  it('shows the unclaimed placeholder when nobody has claimed it', () => {
    const open: RequestRow = { ...request, status: 'OPEN', claimants: [] };
    render(TicketMiniCard, { request: open, agents: [reader] });
    expect(screen.getByTitle('unclaimed')).toHaveTextContent('–');
  });

  it('fires onSelect with the request id when clicked — portals to the Full popover', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(TicketMiniCard, { request, agents: [reader], onSelect });

    await user.click(screen.getByText('plate-bundle endpoint'));
    expect(onSelect).toHaveBeenCalledWith('req_01JQ8f2');
  });

  it('gets a `.sel` ring when selected', () => {
    const { container } = render(TicketMiniCard, { request, agents: [reader], selected: true });
    expect(container.querySelector('.t-mini.sel')).toBeInTheDocument();
  });
});
