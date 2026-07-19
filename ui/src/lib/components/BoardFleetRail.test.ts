import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import BoardFleetRail from './BoardFleetRail.svelte';
import { boardFilter } from '../boardFilter.svelte';
import type { Agent } from '../types';

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
  color: 'var(--ag-reader)',
  abbr: 'RE',
  wip: [{ id: 'req_1', summary: 'x', status: 'CLAIMED' }],
};

const pipeline: Agent = {
  id: 'pipeline',
  display: 'Pipeline',
  desc: null,
  expectedHost: null,
  lastTs: null,
  lastHost: null,
  live: false,
  verified: 'signed',
  color: 'var(--ag-pipeline)',
  abbr: 'PI',
  wip: [],
};

describe('BoardFleetRail', () => {
  it('lists every agent with a real WIP count where it has claimed work', () => {
    render(BoardFleetRail, { agents: [reader, pipeline] });

    expect(screen.getByText('Reader')).toBeInTheDocument();
    expect(screen.getByText('Pipeline')).toBeInTheDocument();
    const rows = screen.getAllByTestId('fleet-rail-agent');
    expect(rows[0]).toHaveTextContent('1'); // reader's one CLAIMED item
  });

  it('clicking an agent toggles the shared boardFilter.agentFilter', async () => {
    const user = userEvent.setup();
    render(BoardFleetRail, { agents: [reader, pipeline] });

    await user.click(screen.getByText('Reader'));
    expect(boardFilter.agentFilter).toBe('reader');

    await user.click(screen.getByText('Reader'));
    expect(boardFilter.agentFilter).toBeNull();
  });

  it('a "clear" affordance appears only while a filter is active, and clears it', async () => {
    const user = userEvent.setup();
    render(BoardFleetRail, { agents: [reader, pipeline] });

    expect(screen.queryByRole('button', { name: /clear/ })).not.toBeInTheDocument();

    await user.click(screen.getByText('Reader'));
    await user.click(screen.getByRole('button', { name: /clear/ }));
    expect(boardFilter.agentFilter).toBeNull();
  });

  it('the collapse toggle hides and reshows the agent list', async () => {
    const user = userEvent.setup();
    render(BoardFleetRail, { agents: [reader, pipeline] });

    await user.click(screen.getByRole('button', { name: /collapse fleet filter/i }));
    expect(screen.queryByText('Reader')).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /expand fleet filter/i }));
    expect(screen.getByText('Reader')).toBeInTheDocument();
  });

  it('j/k move the roving selection and Enter toggles the filter', async () => {
    const user = userEvent.setup();
    render(BoardFleetRail, { agents: [reader, pipeline] });

    screen.getAllByTestId('fleet-rail-agent')[0]!.focus();
    await user.keyboard('j');
    await user.keyboard('{Enter}');
    expect(boardFilter.agentFilter).toBe('pipeline');
  });
});
