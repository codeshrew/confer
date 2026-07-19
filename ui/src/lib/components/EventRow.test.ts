import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import EventRow from './EventRow.svelte';
import type { Agent, Message } from '../types';
import type { EventSubject } from '../eventSubject';

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

function claimMessage(overrides: Partial<Message> = {}): Message {
  return {
    id: 'msg_01JQa10',
    from: 'reader',
    type: 'claim',
    ts: '2026-07-17T14:11:00Z',
    host: 'reader',
    to: ['pipeline'],
    cc: [],
    topic: 'reader',
    summary: 'claimed it — taking the endpoint',
    body: 'Reader claimed this request.',
    of: 'msg_01JQ8f2',
    replyTo: 'msg_01JQ8f2',
    supersedes: null,
    refs: [],
    seenBy: [],
    ...overrides,
  };
}

const ticketSubject: EventSubject = { kind: 'ticket', id: 'req_01JQ8f2', label: 'req_01JQ8f2' };
const threadSubject: EventSubject = { kind: 'thread', msgId: 'msg_01JQf01', label: 'restore chain context' };

describe('EventRow', () => {
  it('renders the actor and summary, with a clickable subject chip when the subject resolves', () => {
    render(EventRow, { message: claimMessage(), fromAgent: reader, subject: ticketSubject });

    expect(screen.getByText('Reader')).toBeInTheDocument();
    expect(screen.getByText(/claimed it — taking the endpoint/)).toBeInTheDocument();
    const chip = screen.getByTestId('event-subject-chip');
    expect(chip).toBeInTheDocument();
    expect(chip).toHaveTextContent('req_01JQ8f2');
  });

  it('clicking the subject chip fires onOpenSubject with the resolved subject, not the row', () => {
    const onOpenSubject = vi.fn();
    render(EventRow, { message: claimMessage(), fromAgent: reader, subject: ticketSubject, onOpenSubject });

    screen.getByTestId('event-subject-chip').click();
    expect(onOpenSubject).toHaveBeenCalledWith(ticketSubject);
  });

  it('renders a thread subject\'s label the same way — one chip, one dispatch, regardless of subject kind', () => {
    const onOpenSubject = vi.fn();
    const evt = claimMessage({ type: 'supersede', of: null, supersedes: 'msg_01JQf01', summary: 'superseded the restore-chain note' });
    render(EventRow, { message: evt, fromAgent: reader, subject: threadSubject, onOpenSubject });

    const chip = screen.getByTestId('event-subject-chip');
    expect(chip).toHaveTextContent('restore chain context');
    chip.click();
    expect(onOpenSubject).toHaveBeenCalledWith(threadSubject);
  });

  it('law #3 — a dangling (null) subject renders NO chip, just plain text', () => {
    const evt = claimMessage({ summary: 'claimed req_purged — ticket no longer on the board', of: 'msg_purged' });
    render(EventRow, { message: evt, fromAgent: reader, subject: null });

    expect(screen.queryByTestId('event-subject-chip')).not.toBeInTheDocument();
    expect(screen.getByText(/claimed req_purged/)).toBeInTheDocument();
  });

  it('is keyboard-reachable: the subject chip is a real, Tab-focusable, Enter-activatable button', async () => {
    const user = userEvent.setup();
    const onOpenSubject = vi.fn();
    render(EventRow, { message: claimMessage(), fromAgent: reader, subject: ticketSubject, onOpenSubject });

    const chip = screen.getByTestId('event-subject-chip');
    expect(chip.tagName).toBe('BUTTON');
    chip.focus();
    expect(chip).toHaveFocus();
    await user.keyboard('{Enter}');
    expect(onOpenSubject).toHaveBeenCalledWith(ticketSubject);
  });

  it('the row itself carries no tabindex/role=button of its own — it is not a j/k-equivalent stop', () => {
    const { container } = render(EventRow, { message: claimMessage(), fromAgent: reader, subject: ticketSubject });
    const row = container.querySelector('[data-testid="event-row"]');
    expect(row).not.toHaveAttribute('role', 'button');
    expect(row).not.toHaveAttribute('tabindex');
  });

  it('colors the icon by the semantic event palette — done is green, blocked is red, distinctly', () => {
    const { container: done } = render(EventRow, { message: claimMessage({ type: 'done', summary: 'endpoint live' }), fromAgent: reader, subject: null });
    const { container: blocked } = render(EventRow, { message: claimMessage({ type: 'blocked', summary: 'blocked on x' }), fromAgent: reader, subject: null });

    const doneTick = done.querySelector('.tick') as HTMLElement;
    const blockedTick = blocked.querySelector('.tick') as HTMLElement;
    expect(doneTick.style.color).toBe('var(--state-flight)');
    expect(blockedTick.style.color).toBe('var(--state-stuck)');
  });
});
