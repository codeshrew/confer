import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Board from './Board.svelte';
import type { Agent, RequestRow } from '../types';

const agents: Agent[] = [
  {
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
  },
  {
    id: 'pipeline',
    display: 'Pipeline',
    desc: null,
    expectedHost: null,
    lastTs: null,
    lastHost: null,
    live: true,
    verified: 'signed',
    color: 'var(--ag-pipeline)',
    abbr: 'PI',
    wip: [],
  },
];

const requests: RequestRow[] = [
  {
    id: 'req_01JQ8f2',
    from: 'pipeline',
    to: ['reader'],
    summary: 'plate-bundle endpoint',
    status: 'DONE',
    resolution: 'shipped',
    deferred: false,
    claimants: ['reader'],
    ageSecs: 2400,
    stale: false,
    topic: 'reader',
  },
  {
    id: 'req_01JQa91',
    from: 'pipeline',
    to: ['pipeline'],
    summary: 'alignment pass',
    status: 'CLAIMED',
    resolution: null,
    deferred: false,
    claimants: ['pipeline'],
    ageSecs: 3600,
    stale: false,
    topic: 'studio',
  },
];

describe('Board', () => {
  it('renders a swimlane per status with its rows', () => {
    const { container } = render(Board, { requests, agents, hubName: 'agent-coord' });

    const laneLabels = [...container.querySelectorAll('.lane-head .ln')].map((el) => el.textContent);
    expect(laneLabels).toEqual(['claimed', 'done']);
    expect(screen.getByText('plate-bundle endpoint')).toBeInTheDocument();
    expect(screen.getByText('alignment pass')).toBeInTheDocument();
  });

  it('renders the distribution bar + status counts', () => {
    const { container } = render(Board, { requests, agents, hubName: 'agent-coord' });

    expect(container.querySelectorAll('.distbar i').length).toBe(2);
    expect(container.querySelectorAll('.board-counts .bc').length).toBe(2);
  });

  it('regroups the rows into topic swimlanes when the group-by toggle changes', async () => {
    const user = userEvent.setup();
    const { container } = render(Board, { requests, agents, hubName: 'agent-coord' });

    await user.click(screen.getByRole('button', { name: 'Topic' }));

    const laneLabels = [...container.querySelectorAll('.lane-head .ln')].map((el) => el.textContent);
    expect(laneLabels).toEqual(['#reader', '#studio']);
    expect(container.querySelector('.lane-head .ln')?.textContent).not.toBe('done');
  });

  it('regroups into claimant swimlanes and fires onSelectRequest on a row click', async () => {
    const user = userEvent.setup();
    const onSelectRequest = vi.fn();
    render(Board, { requests, agents, hubName: 'agent-coord', onSelectRequest });

    await user.click(screen.getByRole('button', { name: 'Claimant' }));
    expect(screen.getByText('Reader')).toBeInTheDocument();
    expect(screen.getByText('Pipeline')).toBeInTheDocument();

    await user.click(screen.getByText('plate-bundle endpoint'));
    expect(onSelectRequest).toHaveBeenCalledWith('req_01JQ8f2');
  });
});
