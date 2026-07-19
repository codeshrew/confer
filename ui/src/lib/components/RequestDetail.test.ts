import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import RequestDetail from './RequestDetail.svelte';
import type { Agent, Message, RequestRow } from '../types';

vi.mock('../api', () => ({
  api: {
    getCode: vi.fn().mockResolvedValue({ lang: 'swift', staleness: 'current', lines: [] }),
    getRefs: vi.fn().mockResolvedValue([]),
  },
}));

const reader: Agent = {
  id: 'reader',
  display: 'Reader',
  desc: 'reader',
  expectedHost: 'reader',
  lastTs: null,
  lastHost: null,
  live: true,
  verified: 'signed',
  color: 'var(--ag-reader)',
  abbr: 'RE',
  wip: [],
};

const pipeline: Agent = {
  id: 'pipeline',
  display: 'Pipeline',
  desc: 'studio',
  expectedHost: 'studio',
  lastTs: null,
  lastHost: null,
  live: true,
  verified: 'signed',
  color: 'var(--ag-pipeline)',
  abbr: 'PI',
  wip: [],
};

const request: RequestRow = {
  id: 'req_01JQ8f2',
  from: 'pipeline',
  to: ['reader'],
  summary: 'Wire up /plate-bundle/:uid',
  status: 'DONE',
  resolution: 'endpoint live, tests green',
  deferred: false,
  claimants: ['reader'],
  ageSecs: 2400,
  stale: false,
  topic: 'reader',
};

const messages: Message[] = [
  {
    id: 'msg_01JQ8f2',
    from: 'pipeline',
    type: 'request',
    ts: '2026-07-17T14:08:00Z',
    host: 'studio',
    to: ['reader'],
    cc: [],
    topic: 'reader',
    summary: 'Wire up /plate-bundle/:uid',
    body: '...',
    of: null,
    replyTo: null,
    supersedes: null,
    refs: [],
    seenBy: [],
  },
  {
    id: 'msg_01JQa10',
    from: 'reader',
    type: 'claim',
    ts: '2026-07-17T14:11:00Z',
    host: 'reader',
    to: ['pipeline'],
    cc: [],
    topic: 'reader',
    summary: 'claimed req_01JQ8f2',
    body: '...',
    of: 'msg_01JQ8f2',
    replyTo: 'msg_01JQ8f2',
    supersedes: null,
    refs: [],
    seenBy: [],
  },
  {
    id: 'msg_01JQc4a',
    from: 'reader',
    type: 'blocked',
    ts: '2026-07-17T14:20:00Z',
    host: 'reader',
    to: ['pipeline'],
    cc: [],
    topic: 'reader',
    summary: 'blocked on uid-spine contract',
    body: '...',
    of: 'msg_01JQ8f2',
    replyTo: 'msg_01JQ8f2',
    supersedes: null,
    refs: [],
    seenBy: [],
  },
  {
    id: 'msg_01JQe80',
    from: 'reader',
    type: 'done',
    ts: '2026-07-17T14:48:00Z',
    host: 'reader',
    to: ['pipeline'],
    cc: [],
    topic: 'reader',
    summary: 'endpoint live, tests green',
    body: '...',
    of: 'msg_01JQ8f2',
    replyTo: 'msg_01JQe73',
    supersedes: null,
    refs: [],
    seenBy: [],
  },
];

describe('RequestDetail', () => {
  it('renders the request kv summary (status/ticket/addressee/claimant/topic/age)', () => {
    render(RequestDetail, { request, messages, agents: [reader, pipeline], hub: 'agent-coord' });

    expect(screen.getByText('DONE')).toBeInTheDocument();
    expect(screen.getByText('req_01JQ8f2')).toBeInTheDocument();
    expect(screen.getByText('@Reader')).toBeInTheDocument();
    expect(screen.getByText('Reader', { selector: '.v2' })).toBeInTheDocument();
    expect(screen.getByText('#reader')).toBeInTheDocument();
  });

  it('renders a lifecycle trail event per related message (filed, claim, blocked, done)', () => {
    const { container } = render(RequestDetail, { request, messages, agents: [reader, pipeline], hub: 'agent-coord' });

    const events = container.querySelectorAll('.lcev');
    // origin message (filed) + claim + blocked + done
    expect(events.length).toBe(4);
    expect(screen.getByText('filed')).toBeInTheDocument();
    expect(screen.getByText('claim')).toBeInTheDocument();
    expect(screen.getByText('blocked')).toBeInTheDocument();
    expect(screen.getByText('done')).toBeInTheDocument();
    expect(screen.getByText('blocked on uid-spine contract')).toBeInTheDocument();
  });

  describe('clickable lifecycle-trail rows (design/41 Phase 0)', () => {
    it('clicking a trail row navigates to its underlying message, passing that message\'s own topic', async () => {
      const user = userEvent.setup();
      const onSelectMessage = vi.fn();
      render(RequestDetail, { request, messages, agents: [reader, pipeline], hub: 'agent-coord', onSelectMessage });

      await user.click(screen.getByText('blocked on uid-spine contract'));

      expect(onSelectMessage).toHaveBeenCalledWith('msg_01JQc4a', 'reader');
    });

    it('clicking the origin ("filed") row navigates to the request\'s originating message', async () => {
      const user = userEvent.setup();
      const onSelectMessage = vi.fn();
      render(RequestDetail, { request, messages, agents: [reader, pipeline], hub: 'agent-coord', onSelectMessage });

      await user.click(screen.getByText('filed'));

      expect(onSelectMessage).toHaveBeenCalledWith('msg_01JQ8f2', 'reader');
    });

    it('renders each trail row as a real, keyboard-reachable button (interactive affordance)', () => {
      const { container } = render(RequestDetail, { request, messages, agents: [reader, pipeline], hub: 'agent-coord' });

      const buttons = container.querySelectorAll('.lccard');
      expect(buttons.length).toBe(4);
      buttons.forEach((b) => expect(b.tagName).toBe('BUTTON'));
    });
  });
});
