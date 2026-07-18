import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Overview from './Overview.svelte';
import type { Attention } from '../attention';
import { getAttention } from '../api';

vi.mock('../api', () => ({
  getAttention: vi.fn(),
}));

const EMPTY: Attention = {
  needsYou: [],
  coordination: [],
  fleet: [
    {
      id: 'reader',
      display: 'Reader',
      color: 'var(--ag-reader)',
      abbr: 'RE',
      hubs: ['agent-coord'],
      liveness: 'live',
      hbAgeSecs: null,
      trust: 'signed',
      wip: 0,
      host: 'reader',
      lastTs: '2026-07-18T10:00:00Z',
      severity: 'nominal',
      fixVerb: null,
    },
  ],
  metrics: { openRequests: 0, perHub: [{ hub: 'agent-coord', label: 'agent-coord', live: 1, attention: 0 }] },
};

const BUSY: Attention = {
  needsYou: [
    {
      id: 'mismatch:agent-coord:jarvis',
      kind: 'mismatch',
      severity: 'critical',
      hub: 'agent-coord',
      topic: null,
      reqId: null,
      summary: 'KEY MISMATCH — Jarvis @ agent-coord',
      detail: 'card key ≠ pinned key (possible spoof)',
      target: 'jarvis',
      verb: 'verify Jarvis locally before trusting its posts',
      ageSecs: null,
    },
  ],
  coordination: [
    {
      id: 'stale-claimed:agent-coord:req_1',
      kind: 'stale-claimed',
      severity: 'attention',
      hub: 'agent-coord',
      topic: 'reader',
      reqId: 'req_1',
      summary: 'STALE · "wire up search" · agent-coord/#reader',
      detail: 'claimed by reader, no movement',
      target: 'reader',
      verb: 'nudge reader — claimed, no movement',
      ageSecs: 432000,
    },
  ],
  fleet: EMPTY.fleet,
  metrics: { openRequests: 3, perHub: [{ hub: 'agent-coord', label: 'agent-coord', live: 1, attention: 2 }] },
};

describe('Overview', () => {
  it('renders the all-clear state when there is nothing to do', async () => {
    vi.mocked(getAttention).mockResolvedValue(EMPTY);
    render(Overview);

    await waitFor(() => expect(screen.getByTestId('needs-you-clear')).toBeInTheDocument());
    expect(screen.getByTestId('coordination-clear')).toBeInTheDocument();
    expect(screen.getByText('✓ all clear')).toBeInTheDocument();
    // The calm state still renders the fleet grid — it's the always-present
    // reassurance, not something that collapses away too.
    expect(screen.getByText('Reader')).toBeInTheDocument();
  });

  it('renders a mismatch item in Needs-you and a stale item in Coordination, each with a target + verb', async () => {
    vi.mocked(getAttention).mockResolvedValue(BUSY);
    render(Overview);

    await waitFor(() => expect(screen.getByText(/KEY MISMATCH/)).toBeInTheDocument());
    expect(screen.getByText(/verify Jarvis locally/)).toBeInTheDocument();

    expect(screen.getByText(/STALE/)).toBeInTheDocument();
    expect(screen.getByText(/nudge reader/)).toBeInTheDocument();
  });

  it('clicking "open thread" on a coordination card fires onDrillRequest with hub + reqId', async () => {
    vi.mocked(getAttention).mockResolvedValue(BUSY);
    const onDrillRequest = vi.fn();
    const user = userEvent.setup();
    render(Overview, { onDrillRequest });

    const openBtn = await screen.findByRole('button', { name: /open thread/i });
    await user.click(openBtn);

    expect(onDrillRequest).toHaveBeenCalledWith('agent-coord', 'req_1');
  });

  it('clicking a per-hub rollup chip fires onDrillHub', async () => {
    vi.mocked(getAttention).mockResolvedValue(EMPTY);
    const onDrillHub = vi.fn();
    const user = userEvent.setup();
    render(Overview, { onDrillHub });

    await waitFor(() => expect(screen.getByTestId('ov-context-strip')).toBeInTheDocument());
    await user.click(screen.getByText('agent-coord'));

    expect(onDrillHub).toHaveBeenCalledWith('agent-coord');
  });

  it('shows a retry-able error state when the fetch fails', async () => {
    vi.mocked(getAttention).mockRejectedValue(new Error('network down'));
    render(Overview);

    expect(await screen.findByText('Overview unavailable')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /retry/i })).toBeInTheDocument();
  });
});
