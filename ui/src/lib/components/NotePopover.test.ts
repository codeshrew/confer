import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { api } from '../api';
import NotePopover from './NotePopover.svelte';
import type { Agent, CodeRef, Message, RequestRow, ThreadNode } from '../types';

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
  range: [47, 73],
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
    id: 'msg_root',
    from: 'herald',
    type: 'note',
    ts: '2026-07-18T17:09:00Z',
    host: 'Batman.local',
    to: [],
    cc: [],
    topic: 'review-080',
    summary: 'Boundary set',
    body: 'Clean division, works for me — no stomp risk.',
    of: null,
    replyTo: null,
    supersedes: null,
    refs: [],
    seenBy: [],
    ...overrides,
  };
}

function node(overrides: Partial<ThreadNode> = {}): ThreadNode {
  return { msgId: 'msg_root', from: 'herald', type: 'note', topic: 'review-080', summary: 'hi', refs: [], ...overrides };
}

function request(overrides: Partial<RequestRow> = {}): RequestRow {
  return {
    id: 'req_mrcky3',
    from: 'herald',
    to: ['jarvis'],
    summary: 'Project per-hub tier + sync on /api/hubs',
    status: 'DONE',
    resolution: 'shipped',
    deferred: false,
    claimants: ['jarvis'],
    ageSecs: 7200,
    stale: false,
    topic: 'review-080',
    ...overrides,
  };
}

describe('NotePopover', () => {
  afterEach(() => {
    vi.mocked(api.getRefs).mockClear();
  });

  it('renders nothing when closed or when msgId does not resolve', () => {
    const { rerender } = render(NotePopover, { open: false, msgId: 'msg_root', messages: [message()], agents: [herald], requests: [], thread: [node()], hub: 'confer-lab' });
    expect(screen.queryByTestId('note-popover')).not.toBeInTheDocument();

    rerender({ open: true, msgId: 'msg_missing', messages: [message()], agents: [herald], requests: [], thread: [node()], hub: 'confer-lab' });
    expect(screen.queryByTestId('note-popover')).not.toBeInTheDocument();
  });

  it('renders the note body, author, and topic', async () => {
    render(NotePopover, { open: true, msgId: 'msg_root', messages: [message()], agents: [herald], requests: [], thread: [node()], hub: 'confer-lab' });
    expect(await screen.findByTestId('note-popover')).toBeInTheDocument();
    expect(screen.getByText('Herald')).toBeInTheDocument();
    expect(screen.getByText('Boundary set')).toBeInTheDocument();
    expect(screen.getByText(/Clean division, works for me/)).toBeInTheDocument();
  });

  it('shows related tickets from the SAME trail, as portable TicketMiniCards', async () => {
    const msgs = [message(), message({ id: 'msg_req', type: 'request', of: 'msg_root', replyTo: 'msg_root', summary: 'Project per-hub tier + sync on /api/hubs' })];
    const thread = [node(), node({ msgId: 'msg_req', type: 'request' })];
    render(NotePopover, { open: true, msgId: 'msg_root', messages: msgs, agents: [herald, jarvis], requests: [request({ id: 'req_req' })], thread, hub: 'confer-lab' });

    expect(await screen.findByText('tickets')).toBeInTheDocument();
    expect(screen.getByTestId('ticket-mini')).toBeInTheDocument();
  });

  it('shows related code refs collected across the whole trail', async () => {
    const msgs = [message(), message({ id: 'msg_reply', replyTo: 'msg_root', refs: [ref] })];
    const thread = [node(), node({ msgId: 'msg_reply', refs: [ref] })];
    render(NotePopover, { open: true, msgId: 'msg_root', messages: msgs, agents: [herald], requests: [], thread, hub: 'confer-lab' });

    expect(await screen.findByText('code')).toBeInTheDocument();
    expect(screen.getByTestId('code-ref-mini')).toBeInTheDocument();
  });

  it('shows the thread pill with a real message/topic count, and it fires onOpenThread', async () => {
    const user = userEvent.setup();
    const onOpenThread = vi.fn();
    const msgs = [message(), message({ id: 'msg_reply', replyTo: 'msg_root', topic: 'studio' })];
    const thread = [node(), node({ msgId: 'msg_reply', topic: 'studio' })];
    render(NotePopover, { open: true, msgId: 'msg_root', messages: msgs, agents: [herald], requests: [], thread, hub: 'confer-lab', onOpenThread });

    const pill = await screen.findByTestId('note-thread-pill');
    expect(pill).toHaveTextContent('2 msgs · 2 topics');
    await user.click(pill);
    expect(onOpenThread).toHaveBeenCalledWith('msg_root', 'review-080');
  });

  it('Escape closes the popover', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(NotePopover, { open: true, msgId: 'msg_root', messages: [message()], agents: [herald], requests: [], thread: [node()], hub: 'confer-lab', onClose });
    await screen.findByTestId('note-popover');

    await user.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalled();
  });

  it('auto-closes when the focus reader opens on top of it', () => {
    const onClose = vi.fn();
    render(NotePopover, {
      open: true,
      msgId: 'msg_root',
      messages: [message()],
      agents: [herald],
      requests: [],
      thread: [node()],
      hub: 'confer-lab',
      focusReaderOpen: true,
      onClose,
    });
    expect(onClose).toHaveBeenCalled();
  });

  it('j/k move the related-column selection and Enter activates the selected ticket', async () => {
    const user = userEvent.setup();
    const onOpenTicket = vi.fn();
    const msgs = [message(), message({ id: 'msg_req', type: 'request', of: 'msg_root', replyTo: 'msg_root' })];
    const thread = [node(), node({ msgId: 'msg_req', type: 'request' })];
    render(NotePopover, {
      open: true,
      msgId: 'msg_root',
      messages: msgs,
      agents: [herald, jarvis],
      requests: [request({ id: 'req_req' })],
      thread,
      hub: 'confer-lab',
      onOpenTicket,
    });

    const related = await screen.findByTestId('note-related');
    related.focus();
    await user.keyboard('{Enter}');
    expect(onOpenTicket).toHaveBeenCalledWith('req_req');
  });

  it('clicking a code mini fetches real hits and calls onOpenRefs', async () => {
    const user = userEvent.setup();
    const onOpenRefs = vi.fn();
    vi.mocked(api.getRefs).mockResolvedValue([]);
    const msgs = [message({ refs: [ref] })];
    const thread = [node({ refs: [ref] })];
    render(NotePopover, { open: true, msgId: 'msg_root', messages: msgs, agents: [herald], requests: [], thread, hub: 'confer-lab', onOpenRefs });

    await user.click(await screen.findByTestId('code-ref-mini'));
    expect(api.getRefs).toHaveBeenCalledWith('confer-lab', 'confer:src/patch.rs@47-73', true);
    expect(onOpenRefs).toHaveBeenCalledWith(ref, []);
  });
});
