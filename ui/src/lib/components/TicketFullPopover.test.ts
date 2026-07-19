import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { api } from '../api';
import TicketFullPopover from './TicketFullPopover.svelte';
import type { Agent, CodeRef, Message, RequestRow } from '../types';

vi.mock('../api', () => ({
  api: { getRefs: vi.fn().mockResolvedValue([]) },
}));

const herald: Agent = {
  id: 'herald',
  display: 'Herald',
  desc: null,
  expectedHost: null,
  lastTs: null,
  lastHost: null,
  live: true,
  verified: 'signed',
  version: null,
  watchState: null,
  keyFingerprint: null,
  color: 'var(--ag-herald)',
  abbr: 'HE',
  wip: [],
};

const jarvis: Agent = {
  id: 'jarvis',
  display: 'Jarvis',
  desc: null,
  expectedHost: null,
  lastTs: null,
  lastHost: null,
  live: true,
  verified: 'signed',
  version: null,
  watchState: null,
  keyFingerprint: null,
  color: 'var(--ag-jarvis)',
  abbr: 'JA',
  wip: [],
};

const ref: CodeRef = {
  repo: 'confer',
  path: 'src/patch.rs',
  sha: 'abc123',
  range: null,
  contentHash: null,
  refName: null,
  refType: null,
  commitDate: null,
  dirty: false,
  untracked: false,
  baseRef: null,
  forkPoint: null,
};

function message(overrides: Partial<Message> = {}): Message {
  return {
    id: 'msg_abc',
    from: 'herald',
    type: 'request',
    ts: '2026-07-18T10:00:00Z',
    host: null,
    to: [],
    cc: [],
    topic: 'review-080',
    summary: 'review please',
    body: 'A longer body with the actual detail, spread across more than two lines of real prose.',
    of: null,
    replyTo: null,
    supersedes: null,
    refs: [ref],
    seenBy: [],
    ...overrides,
  };
}

function request(overrides: Partial<RequestRow> = {}): RequestRow {
  return {
    id: 'req_abc',
    from: 'herald',
    to: ['jarvis'],
    summary: '0.8.0 review + test',
    status: 'CLAIMED',
    resolution: null,
    deferred: false,
    claimants: ['jarvis'],
    ageSecs: 3600,
    stale: false,
    topic: 'review-080',
    ...overrides,
  };
}

const claimMsg = message({ id: 'msg_c1', type: 'claim', from: 'jarvis', of: 'msg_abc', ts: '2026-07-18T10:05:00Z' });

describe('TicketFullPopover', () => {
  afterEach(() => {
    delete (navigator as { clipboard?: unknown }).clipboard;
    vi.mocked(api.getRefs).mockClear();
  });

  it('renders nothing when closed, or when requestId does not resolve', () => {
    const r = request();
    const { rerender } = render(TicketFullPopover, {
      open: false,
      requestId: r.id,
      requests: [r],
      messages: [message()],
      agents: [herald, jarvis],
      hub: 'confer-lab',
    });
    expect(screen.queryByTestId('ticket-popover')).not.toBeInTheDocument();

    rerender({ open: true, requestId: 'req_missing', requests: [r], messages: [message()], agents: [herald, jarvis], hub: 'confer-lab' });
    expect(screen.queryByTestId('ticket-popover')).not.toBeInTheDocument();
  });

  it('renders the lifecycle track, meta grid, and a teaser — never the full body', async () => {
    const r = request();
    render(TicketFullPopover, {
      open: true,
      requestId: r.id,
      requests: [r],
      messages: [message(), claimMsg],
      agents: [herald, jarvis],
      hub: 'confer-lab',
    });

    expect(await screen.findByTestId('ticket-popover')).toBeInTheDocument();
    expect(screen.getByText('0.8.0 review + test')).toBeInTheDocument();
    expect(screen.getByText('Requested')).toBeInTheDocument();
    expect(screen.getByText('Claimed')).toBeInTheDocument();
    expect(screen.getByText('Done')).toBeInTheDocument();
    // Meta grid — requester, assignee, real ref chip.
    expect(screen.getByText('Herald')).toBeInTheDocument();
    expect(screen.getByText('Jarvis')).toBeInTheDocument();
    expect(screen.getByText('src/patch.rs')).toBeInTheDocument();
    // The teaser, but no full-body reading surface (no rendered prose block).
    expect(screen.getByText(/A longer body with the actual detail/)).toBeInTheDocument();
  });

  it('shows a red branch + reason when the ticket is stuck', async () => {
    const r = request({ status: 'BLOCKED' });
    const blockedMsg = message({ id: 'msg_b1', type: 'blocked', from: 'jarvis', of: 'msg_abc', ts: '2026-07-18T10:30:00Z', summary: 'waiting on design review' });
    render(TicketFullPopover, {
      open: true,
      requestId: r.id,
      requests: [r],
      messages: [message(), claimMsg, blockedMsg],
      agents: [herald, jarvis],
      hub: 'confer-lab',
    });

    expect(await screen.findByText(/stuck at/)).toHaveTextContent('waiting on design review');
  });

  it('Escape closes the popover', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const r = request();
    render(TicketFullPopover, {
      open: true,
      requestId: r.id,
      requests: [r],
      messages: [message(), claimMsg],
      agents: [herald, jarvis],
      hub: 'confer-lab',
      onClose,
    });
    await screen.findByTestId('ticket-popover');

    await user.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalled();
  });

  it('the ✕ button also closes it', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const r = request();
    render(TicketFullPopover, {
      open: true,
      requestId: r.id,
      requests: [r],
      messages: [message(), claimMsg],
      agents: [herald, jarvis],
      hub: 'confer-lab',
      onClose,
    });
    await user.click(await screen.findByRole('button', { name: /close ticket/i }));
    expect(onClose).toHaveBeenCalled();
  });

  it('j/k walk to the next/previous ticket in the navigable list', async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    const r1 = request({ id: 'req_a' });
    const r2 = request({ id: 'req_b' });
    const r3 = request({ id: 'req_c' });
    render(TicketFullPopover, {
      open: true,
      requestId: 'req_b',
      requests: [r1, r2, r3],
      messages: [message({ id: 'msg_a' }), message({ id: 'msg_b' }), message({ id: 'msg_c' })],
      agents: [herald, jarvis],
      hub: 'confer-lab',
      onNavigate,
    });
    await screen.findByTestId('ticket-popover');

    await user.keyboard('j');
    expect(onNavigate).toHaveBeenCalledWith('req_c');
    await user.keyboard('k');
    expect(onNavigate).toHaveBeenCalledWith('req_a');
  });

  it('`y` copies the ticket id and shows a toast', async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    const r = request();
    render(TicketFullPopover, {
      open: true,
      requestId: r.id,
      requests: [r],
      messages: [message(), claimMsg],
      agents: [herald, jarvis],
      hub: 'confer-lab',
    });
    await screen.findByTestId('ticket-popover');

    await user.keyboard('y');
    expect(writeText).toHaveBeenCalledWith('req_abc');
    expect(await screen.findByTestId('copied-toast')).toBeInTheDocument();
  });

  it('"open thread" fires onOpenThread with the origin message id and topic — a launchpad jump, not a container', async () => {
    const user = userEvent.setup();
    const onOpenThread = vi.fn();
    const r = request();
    render(TicketFullPopover, {
      open: true,
      requestId: r.id,
      requests: [r],
      messages: [message(), claimMsg],
      agents: [herald, jarvis],
      hub: 'confer-lab',
      onOpenThread,
    });
    await user.click(await screen.findByText('open thread ›'));
    expect(onOpenThread).toHaveBeenCalledWith('msg_abc', 'review-080');
  });

  it('"focus read" fires onFocusRead with the origin message id', async () => {
    const user = userEvent.setup();
    const onFocusRead = vi.fn();
    const r = request();
    render(TicketFullPopover, {
      open: true,
      requestId: r.id,
      requests: [r],
      messages: [message(), claimMsg],
      agents: [herald, jarvis],
      hub: 'confer-lab',
      onFocusRead,
    });
    await user.click(await screen.findByText(/focus read/));
    expect(onFocusRead).toHaveBeenCalledWith('msg_abc');
  });

  it('clicking a ref chip fetches real reverse-index hits via api.getRefs, then calls onOpenRefs', async () => {
    const user = userEvent.setup();
    const onOpenRefs = vi.fn();
    vi.mocked(api.getRefs).mockResolvedValue([]);
    const r = request();
    render(TicketFullPopover, {
      open: true,
      requestId: r.id,
      requests: [r],
      messages: [message(), claimMsg],
      agents: [herald, jarvis],
      hub: 'confer-lab',
      onOpenRefs,
    });
    await user.click(await screen.findByText('src/patch.rs'));

    expect(api.getRefs).toHaveBeenCalledWith('confer-lab', 'confer:src/patch.rs', true);
    expect(onOpenRefs).toHaveBeenCalledWith(ref, []);
  });

  it('auto-closes when the focus reader opens on top of it — launchpad, not a stack', () => {
    const onClose = vi.fn();
    const r = request();
    render(TicketFullPopover, {
      open: true,
      requestId: r.id,
      requests: [r],
      messages: [message(), claimMsg],
      agents: [herald, jarvis],
      hub: 'confer-lab',
      focusReaderOpen: true,
      onClose,
    });
    expect(onClose).toHaveBeenCalled();
  });
});
