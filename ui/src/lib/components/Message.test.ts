import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Message from './Message.svelte';
import type { Agent, Message as MessageT } from '../types';

const herald: Agent = {
  id: 'herald',
  display: 'Herald',
  desc: 'gitconv',
  expectedHost: 'gitconv',
  lastTs: null,
  lastHost: null,
  live: true,
  verified: 'signed',
  color: 'var(--ag-herald)',
  abbr: 'HE',
  wip: [],
};

const noteMessage: MessageT = {
  id: 'msg_01JQ001',
  from: 'herald',
  type: 'note',
  ts: '2026-07-17T14:02:00Z',
  host: 'gitconv',
  to: ['all'],
  cc: [],
  topic: 'reader',
  summary: 'Shipping confer 0.7.3',
  body: 'Shipping confer 0.7.3 — @all the `serve --all-hubs` broken-tab fix is in.',
  of: null,
  replyTo: null,
  supersedes: null,
  refs: [],
};

const claimMessage: MessageT = {
  ...noteMessage,
  id: 'msg_01JQa10',
  type: 'claim',
  summary: 'claimed req_01JQ8f2',
  body: 'Reader claimed this request.',
};

describe('Message', () => {
  it('renders a note with the who/role/ts head and the body text', () => {
    render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [] });

    expect(screen.getByText('Herald')).toBeInTheDocument();
    expect(screen.getByText('gitconv')).toBeInTheDocument();
    expect(screen.getByText(/Shipping confer 0.7.3/)).toBeInTheDocument();
  });

  it('highlights @mentions and inline code distinctly from the surrounding text', () => {
    const { container } = render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [] });

    expect(container.querySelector('.mention')?.textContent).toBe('@all');
    expect(container.querySelector('code.mono')?.textContent).toBe('serve --all-hubs');
  });

  it('renders lifecycle types (claim/done/blocked) as an inline sysline, not a message bubble', () => {
    const { container } = render(Message, { message: claimMessage, fromAgent: herald, seenEntries: [] });

    expect(container.querySelector('.sysline')).toBeInTheDocument();
    expect(container.querySelector('.msg')).not.toBeInTheDocument();
    expect(screen.getByText('claimed req_01JQ8f2')).toBeInTheDocument();
  });

  it('shows the seen indicator reflecting all-seen vs partial state', () => {
    render(Message, {
      message: noteMessage,
      fromAgent: herald,
      seenEntries: [{ id: 'reader', name: 'Reader', color: 'var(--ag-reader)', ts: '14:03' }],
    });

    expect(screen.getByText('all seen')).toBeInTheDocument();
  });

  it('fires onSelect when the message body is clicked', async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(Message, { message: noteMessage, fromAgent: herald, seenEntries: [], onSelect });

    await user.click(screen.getByText('Herald'));

    expect(onSelect).toHaveBeenCalledWith('msg_01JQ001');
  });
});
