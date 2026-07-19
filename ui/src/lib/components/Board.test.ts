import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Board from './Board.svelte';
import { boardFilter } from '../boardFilter.svelte';
import type { Agent, Message, RequestRow } from '../types';

// `boardFilter` is a module singleton (piece 5c) — reset between tests so
// one test's stat/workload clicks can't leak into the next (same gotcha
// readState.svelte.ts's tests guard against).
beforeEach(() => {
  boardFilter.clearAll();
});

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
  color: 'var(--ag-reader)',
  abbr: 'RE',
  wip: [{ id: 'req_01JQa91', summary: 'alignment pass', status: 'CLAIMED' }],
};

const pipeline: Agent = {
  id: 'pipeline',
  display: 'Pipeline',
  desc: null,
  expectedHost: null,
  lastTs: null,
  lastHost: null,
  live: true,
  verified: 'signed',
  version: null,
  watchState: null,
  keyFingerprint: null,
  color: 'var(--ag-pipeline)',
  abbr: 'PI',
  wip: [],
};

const agents: Agent[] = [reader, pipeline];

function request(overrides: Partial<RequestRow>): RequestRow {
  return {
    id: 'req_x',
    from: 'pipeline',
    to: ['reader'],
    summary: 'a ticket',
    status: 'OPEN',
    resolution: null,
    deferred: false,
    claimants: [],
    ageSecs: 60,
    stale: false,
    topic: 'reader',
    ...overrides,
  };
}

const requests: RequestRow[] = [
  request({ id: 'req_01JQ8f2', summary: 'plate-bundle endpoint', status: 'DONE', resolution: 'shipped', claimants: ['reader'], ageSecs: 2400 }),
  request({ id: 'req_01JQa91', summary: 'alignment pass', status: 'CLAIMED', claimants: ['reader'], ageSecs: 3600 }),
  request({ id: 'req_01JQb22', summary: 'needs a home', status: 'OPEN', claimants: [], from: 'pipeline', ageSecs: 7200 }),
  request({ id: 'req_01JQc33', summary: 'stuck on review', status: 'BLOCKED', claimants: ['pipeline'], ageSecs: 10000 }),
];

describe('Board — cockpit header + stats', () => {
  it('shows the tier badge, hub name, and a real verdict line', () => {
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord', hubTier: 'shared' });

    expect(screen.getByText('shared')).toBeInTheDocument();
    expect(screen.getByText('agent-coord')).toBeInTheDocument();
    expect(screen.getByText(/1 needs an owner/)).toBeInTheDocument();
    expect(screen.getByText(/1 stuck/)).toBeInTheDocument();
  });

  it('activeWork ("Open") is the TOTAL of everything not done, not a disjoint fourth bucket', () => {
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });
    // 3 requests are live (claimed + needs-owner + stuck); the 4th is DONE.
    expect(screen.getByTestId('board-stat-open').querySelector('.n')).toHaveTextContent('3');
    expect(screen.getByTestId('board-stat-flight').querySelector('.n')).toHaveTextContent('1');
    expect(screen.getByTestId('board-stat-stuck').querySelector('.n')).toHaveTextContent('1');
    expect(screen.getByTestId('board-stat-unowned').querySelector('.n')).toHaveTextContent('1');
  });

  it('an empty board shows the empty state, not a wall of zeroed stat cards', () => {
    render(Board, { requests: [], agents: [], messages: [], hubName: 'agent-coord' });
    expect(screen.getByText('Nothing on the board yet')).toBeInTheDocument();
    expect(screen.queryByTestId('board-stat-open')).not.toBeInTheDocument();
  });
});

describe('Board — workload (carrying / asking)', () => {
  it('carrying reflects real Agent.wip CLAIMED counts', () => {
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });
    expect(screen.getByText('carrying')).toBeInTheDocument();
    // Reader is carrying (wip has one CLAIMED entry); Pipeline is not.
    const carryingSection = screen.getByText('carrying').closest('.card')! as HTMLElement;
    expect(carryingSection).toHaveTextContent('Reader');
  });

  it('asking groups live requests by requester, and the unowned key is present when relevant', () => {
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });
    const askingSection = screen.getByText('asking').closest('.card')! as HTMLElement;
    // pipeline filed 2 live requests here (alignment pass is claimant reader
    // but from pipeline is not set on it — check the fixture's `from`).
    expect(askingSection).toHaveTextContent('Pipeline');
  });
});

describe('Board — grouped work lists + done fold', () => {
  it('groups live tickets by state, each rendered as a TicketRow', () => {
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });

    expect(screen.getByText('▲ needs an owner')).toBeInTheDocument();
    expect(screen.getByText('● in flight')).toBeInTheDocument();
    expect(screen.getByText('▲ blocked / stale')).toBeInTheDocument();
    expect(screen.getByText('needs a home')).toBeInTheDocument();
    expect(screen.getByText('alignment pass')).toBeInTheDocument();
    expect(screen.getByText('stuck on review')).toBeInTheDocument();
  });

  it('DONE tickets are collapsed behind a fold, not shown by default', () => {
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });

    expect(screen.queryByText('plate-bundle endpoint')).not.toBeInTheDocument();
    expect(screen.getByTestId('board-done-fold')).toHaveTextContent('1 closed');
  });

  it('clicking the done fold reveals the DONE tickets', async () => {
    const user = userEvent.setup();
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });

    await user.click(screen.getByTestId('board-done-fold'));
    expect(screen.getByText('plate-bundle endpoint')).toBeInTheDocument();
  });

  it('fires onSelectRequest when a ticket row is clicked', async () => {
    const user = userEvent.setup();
    const onSelectRequest = vi.fn();
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord', onSelectRequest });

    await user.click(screen.getByText('alignment pass'));
    expect(onSelectRequest).toHaveBeenCalledWith('req_01JQa91');
  });
});

describe('Board — stat click-to-filter', () => {
  it('clicking "Stuck" narrows the work lists to just the stuck group', async () => {
    const user = userEvent.setup();
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });

    await user.click(screen.getByTestId('board-stat-stuck'));

    expect(screen.getByText('stuck on review')).toBeInTheDocument();
    expect(screen.queryByText('needs a home')).not.toBeInTheDocument();
    expect(screen.queryByText('alignment pass')).not.toBeInTheDocument();
    expect(screen.getByTestId('board-filter-chips')).toHaveTextContent('blocked / stale');
  });

  it('"clear all" clears the filter', async () => {
    const user = userEvent.setup();
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });

    await user.click(screen.getByTestId('board-stat-stuck'));
    await user.click(screen.getByRole('button', { name: /clear all/ }));

    expect(screen.getByText('needs a home')).toBeInTheDocument();
    expect(screen.getByText('alignment pass')).toBeInTheDocument();
    expect(screen.getByText('stuck on review')).toBeInTheDocument();
    expect(screen.queryByTestId('board-filter-chips')).not.toBeInTheDocument();
  });

  it('clicking a second stat swaps the filter; clicking the SAME stat again clears it', async () => {
    const user = userEvent.setup();
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });

    await user.click(screen.getByTestId('board-stat-stuck'));
    await user.click(screen.getByTestId('board-stat-unowned'));
    expect(screen.getByText('needs a home')).toBeInTheDocument();
    expect(screen.queryByText('stuck on review')).not.toBeInTheDocument();

    await user.click(screen.getByTestId('board-stat-unowned'));
    expect(screen.queryByTestId('board-filter-chips')).not.toBeInTheDocument();
  });

  it('clicking "Open" (the total, not a disjoint bucket) clears any active filter', async () => {
    const user = userEvent.setup();
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });

    await user.click(screen.getByTestId('board-stat-stuck'));
    await user.click(screen.getByTestId('board-stat-open'));
    expect(screen.queryByTestId('board-filter-chips')).not.toBeInTheDocument();
  });
});

describe('Board — closure throughput', () => {
  function msg(overrides: Partial<Message>): Message {
    return {
      id: 'msg_1',
      from: 'herald',
      type: 'done',
      ts: '2026-07-18T10:00:00Z',
      host: null,
      to: [],
      cc: [],
      topic: 'general',
      summary: 'hi',
      body: 'hi',
      of: null,
      replyTo: null,
      supersedes: null,
      refs: [],
      seenBy: [],
      ...overrides,
    };
  }

  it('renders real closed/opened totals from request/done message timestamps — no fabricated chart', () => {
    const messages = [msg({ id: 'm1', type: 'done', ts: new Date().toISOString() }), msg({ id: 'm2', type: 'request', ts: new Date().toISOString() })];
    render(Board, { requests, agents, messages, hubName: 'agent-coord' });

    expect(screen.getByText('closure throughput')).toBeInTheDocument();
    expect(screen.getByText('1', { selector: '.chart-meta b' })).toBeInTheDocument();
  });
});

describe('Board — agent filter (workload rows) + combined filters', () => {
  it('clicking a workload row (carrying) filters the work lists to that agent\'s work', async () => {
    const user = userEvent.setup();
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });

    const carryingSection = screen.getByText('carrying').closest('.card')! as HTMLElement;
    const readerRow = within(carryingSection).getByText('Reader').closest('button')! as HTMLElement;
    await user.click(readerRow);

    // Reader carries "alignment pass" (claimant) — visible. "needs a home"
    // (from pipeline, unclaimed) and "stuck on review" (pipeline's) are not.
    expect(screen.getByText('alignment pass')).toBeInTheDocument();
    expect(screen.queryByText('needs a home')).not.toBeInTheDocument();
    expect(screen.queryByText('stuck on review')).not.toBeInTheDocument();
    expect(screen.getByTestId('board-filter-chips')).toHaveTextContent('Reader');
  });

  it('clicking the SAME workload row again clears the agent filter', async () => {
    const user = userEvent.setup();
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });

    const carryingSection = screen.getByText('carrying').closest('.card')! as HTMLElement;
    const readerRow = within(carryingSection).getByText('Reader').closest('button')! as HTMLElement;
    await user.click(readerRow);
    await user.click(readerRow);

    expect(screen.queryByTestId('board-filter-chips')).not.toBeInTheDocument();
    expect(screen.getByText('needs a home')).toBeInTheDocument();
  });

  it('state + agent filters combine with AND, both shown as separate chips', async () => {
    const user = userEvent.setup();
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });

    await user.click(screen.getByTestId('board-stat-stuck'));
    const carryingSection = screen.getByText('carrying').closest('.card')! as HTMLElement;
    // Pipeline isn't in the "carrying" list (no CLAIMED wip), so use the
    // asking section instead, where pipeline does appear.
    const askingSection = screen.getByText('asking').closest('.card')! as HTMLElement;
    const pipelineRow = within(askingSection).getByText('Pipeline').closest('button')! as HTMLElement;
    await user.click(pipelineRow);

    const chips = screen.getByTestId('board-filter-chips');
    expect(chips).toHaveTextContent('blocked / stale');
    expect(chips).toHaveTextContent('Pipeline');
    // "stuck on review" is BLOCKED and pipeline is its claimant — matches both.
    expect(screen.getByText('stuck on review')).toBeInTheDocument();
    void carryingSection;
  });

  it('"clear all" resets both dimensions at once', async () => {
    const user = userEvent.setup();
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });

    await user.click(screen.getByTestId('board-stat-stuck'));
    const askingSection = screen.getByText('asking').closest('.card')! as HTMLElement;
    await user.click(within(askingSection).getByText('Pipeline').closest('button')! as HTMLElement);
    await user.click(screen.getByRole('button', { name: /clear all/ }));

    expect(screen.queryByTestId('board-filter-chips')).not.toBeInTheDocument();
    expect(screen.getByText('needs a home')).toBeInTheDocument();
    expect(screen.getByText('alignment pass')).toBeInTheDocument();
  });

  it('the done fold\'s own count respects the active agent filter', async () => {
    const user = userEvent.setup();
    render(Board, { requests, agents, messages: [], hubName: 'agent-coord' });

    const carryingSection = screen.getByText('carrying').closest('.card')! as HTMLElement;
    await user.click(within(carryingSection).getByText('Reader').closest('button')! as HTMLElement);

    // Reader is the claimant on the one DONE ticket too.
    expect(screen.getByTestId('board-done-fold')).toHaveTextContent('1 closed');
  });
});
