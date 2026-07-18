import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/svelte';
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
  domains: [
    {
      hub: 'agent-coord',
      label: 'agent-coord',
      agents: [
        {
          id: 'reader',
          display: 'Reader',
          color: 'var(--ag-reader)',
          abbr: 'RE',
          host: 'reader',
          liveness: 'live',
          hbAgeSecs: null,
          trust: 'signed',
          wip: 0,
        },
      ],
      workInFlight: [],
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
  domains: [
    {
      hub: 'agent-coord',
      label: 'agent-coord',
      agents: [
        {
          id: 'jarvis',
          display: 'Jarvis',
          color: 'var(--ag-jarvis)',
          abbr: 'JA',
          host: 'pop-os',
          liveness: 'live',
          hbAgeSecs: null,
          trust: 'mismatch',
          wip: 0,
        },
        {
          id: 'reader',
          display: 'Reader',
          color: 'var(--ag-reader)',
          abbr: 'RE',
          host: 'reader',
          liveness: 'live',
          hbAgeSecs: null,
          trust: 'signed',
          wip: 1,
        },
      ],
      workInFlight: [
        { id: 'req_1', summary: 'wire up search', status: 'CLAIMED', stale: true, claimants: ['reader'], ageSecs: 432000 },
      ],
    },
  ],
  metrics: { openRequests: 3, perHub: [{ hub: 'agent-coord', label: 'agent-coord', live: 2, attention: 2 }] },
};

// A down agent — real liveness folded into BOTH the map (hollow node) and
// the needs-you overlay (verb pointing back at it), the two-places-at-once
// pattern the redesign's mockup calls for.
const DOWN: Attention = {
  needsYou: [
    {
      id: 'down:jarvis-orbit:work-orbit',
      kind: 'down',
      severity: 'critical',
      hub: 'jarvis-orbit',
      topic: null,
      reqId: null,
      summary: 'DOWN — Work Orbit @ jarvis-orbit',
      detail: 'last seen on Hestia',
      target: 'work-orbit',
      verb: 'Work Orbit is down — check the host / restart the session',
      ageSecs: 7200,
    },
  ],
  coordination: [],
  fleet: [],
  domains: [
    {
      hub: 'jarvis-orbit',
      label: 'jarvis-orbit',
      agents: [
        {
          id: 'work-orbit',
          display: 'Work Orbit',
          color: 'var(--ag-orbit)',
          abbr: 'WO',
          host: 'Hestia',
          liveness: 'down',
          hbAgeSecs: 7200,
          trust: 'signed',
          wip: 0,
        },
      ],
      workInFlight: [],
    },
  ],
  metrics: { openRequests: 0, perHub: [{ hub: 'jarvis-orbit', label: 'jarvis-orbit', live: 0, attention: 1 }] },
};

describe('Overview', () => {
  it('renders the all-clear state — a calm map, no fabricated freshness line', async () => {
    vi.mocked(getAttention).mockResolvedValue(EMPTY);
    render(Overview);

    await waitFor(() => expect(screen.getByTestId('attention-clear')).toBeInTheDocument());
    expect(screen.getByText('✓ Steady')).toBeInTheDocument();
    // The calm state still renders the domain map — it's the always-present
    // reassurance, not something that collapses away too.
    expect(screen.getByText('Reader')).toBeInTheDocument();
    expect(screen.getByTestId('ov-domain')).toBeInTheDocument();
  });

  it('renders a mismatch item and a stale item in the merged needs-you overlay, each with a verb', async () => {
    vi.mocked(getAttention).mockResolvedValue(BUSY);
    render(Overview);

    const rows = await screen.findAllByTestId('ov-attention-row');
    expect(rows).toHaveLength(2);
    expect(within(rows[0]!).getByText(/verify Jarvis locally/)).toBeInTheDocument();
    expect(within(rows[1]!).getByText(/nudge reader/)).toBeInTheDocument();

    // The agent nodes on the map carry the same real state the overlay
    // points back at (Jarvis's mismatch trust chip).
    const nodes = screen.getAllByTestId('agent-node');
    expect(nodes.some((n) => n.textContent?.includes('Jarvis'))).toBe(true);
  });

  it('a down agent shows hollow on the map AND raises a needs-you overlay row naming it', async () => {
    vi.mocked(getAttention).mockResolvedValue(DOWN);
    render(Overview);

    await waitFor(() => expect(screen.getByText(/Work Orbit is down/)).toBeInTheDocument());
    const node = await screen.findByRole('button', { name: /Work Orbit.*Hestia.*down/ });
    expect(node).toBeInTheDocument();
  });

  it('clicking "open thread" on an overlay row fires onDrillRequest with hub + reqId', async () => {
    vi.mocked(getAttention).mockResolvedValue(BUSY);
    const onDrillRequest = vi.fn();
    const user = userEvent.setup();
    render(Overview, { onDrillRequest });

    const openBtn = await screen.findByRole('button', { name: /open thread/i });
    await user.click(openBtn);

    expect(onDrillRequest).toHaveBeenCalledWith('agent-coord', 'req_1');
  });

  it('clicking a domain name fires onDrillHub', async () => {
    vi.mocked(getAttention).mockResolvedValue(EMPTY);
    const onDrillHub = vi.fn();
    const user = userEvent.setup();
    render(Overview, { onDrillHub });

    await waitFor(() => expect(screen.getByTestId('ov-domain')).toBeInTheDocument());
    await user.click(screen.getByRole('button', { name: 'agent-coord' }));

    expect(onDrillHub).toHaveBeenCalledWith('agent-coord');
  });

  it('clicking an agent node fires onDrillFleet with hub + agent id', async () => {
    vi.mocked(getAttention).mockResolvedValue(EMPTY);
    const onDrillFleet = vi.fn();
    const user = userEvent.setup();
    render(Overview, { onDrillFleet });

    const node = await screen.findByTestId('agent-node');
    await user.click(node);

    expect(onDrillFleet).toHaveBeenCalledWith('agent-coord', 'reader');
  });

  it('clicking a work-in-flight chip fires onDrillRequest with hub + reqId', async () => {
    vi.mocked(getAttention).mockResolvedValue(BUSY);
    const onDrillRequest = vi.fn();
    const user = userEvent.setup();
    render(Overview, { onDrillRequest });

    const chip = await screen.findByText('wire up search');
    await user.click(chip);

    expect(onDrillRequest).toHaveBeenCalledWith('agent-coord', 'req_1');
  });

  it('shows a retry-able error state when the fetch fails', async () => {
    vi.mocked(getAttention).mockRejectedValue(new Error('network down'));
    render(Overview);

    expect(await screen.findByText('Overview unavailable')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /retry/i })).toBeInTheDocument();
  });

  it('shows a loading skeleton before the first response lands', () => {
    vi.mocked(getAttention).mockReturnValue(new Promise(() => {}));
    render(Overview);

    expect(screen.getByTestId('skeleton')).toBeInTheDocument();
  });
});
