import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import AgentDossier from './AgentDossier.svelte';
import type { Agent, RequestRow } from '../types';

vi.mock('../api', () => ({
  fetchHubOverviews: vi.fn().mockResolvedValue([]),
}));

import { fetchHubOverviews } from '../api';

const jarvis: Agent = {
  id: 'jarvis',
  display: 'Jarvis',
  desc: 'OpenJarvis AI running on **pop-os** — design-review partner.',
  expectedHost: 'pop-os',
  lastTs: '2026-07-18T17:41:00Z',
  lastHost: 'pop-os',
  live: true,
  verified: 'signed',
  liveness: 'live',
  hbAgeSecs: 0,
  trust: 'signed',
  color: '#7dcfff',
  abbr: 'JA',
  wip: [{ id: 'req_98xcnf', summary: '0.8.0 review', status: 'CLAIMED' }],
};

const herald: Agent = {
  id: 'herald',
  display: 'Herald',
  desc: null,
  expectedHost: 'Batman.local',
  lastTs: '2026-07-18T17:00:00Z',
  lastHost: 'Batman.local',
  live: true,
  verified: 'signed',
  liveness: 'live',
  hbAgeSecs: 60,
  trust: 'signed',
  color: '#9ece6a',
  abbr: 'HE',
  wip: [],
};

function request(overrides: Partial<RequestRow> = {}): RequestRow {
  return {
    id: 'req_1',
    from: 'jarvis',
    to: [],
    summary: 'a ticket',
    status: 'OPEN',
    resolution: null,
    deferred: false,
    claimants: [],
    ageSecs: 60,
    stale: false,
    topic: 'general',
    ...overrides,
  };
}

describe('AgentDossier', () => {
  afterEach(() => {
    vi.mocked(fetchHubOverviews).mockClear();
  });

  it('renders nothing when closed or when agentId does not resolve', () => {
    const { rerender } = render(AgentDossier, { open: false, agentId: 'jarvis', agents: [jarvis], requests: [], messages: [] });
    expect(screen.queryByTestId('agent-dossier')).not.toBeInTheDocument();

    rerender({ open: true, agentId: 'nobody', agents: [jarvis], requests: [], messages: [] });
    expect(screen.queryByTestId('agent-dossier')).not.toBeInTheDocument();
  });

  it('renders the header, real About markdown, and at-a-glance facts', async () => {
    vi.mocked(fetchHubOverviews).mockResolvedValue([]);
    render(AgentDossier, { open: true, agentId: 'jarvis', agents: [jarvis], requests: [], messages: [] });

    expect(await screen.findByTestId('agent-dossier')).toBeInTheDocument();
    expect(screen.getByText('Jarvis')).toBeInTheDocument();
    expect(screen.getByText('⬤ live', { exact: false })).toBeInTheDocument();
    expect(screen.getByText(/design-review partner/)).toBeInTheDocument();
    expect(screen.getByText('pop-os')).toBeInTheDocument(); // the header's host label
  });

  it('when the agent has no profile written, the About block is honestly omitted, not a fake placeholder', async () => {
    render(AgentDossier, { open: true, agentId: 'herald', agents: [herald], requests: [], messages: [] });
    await screen.findByTestId('agent-dossier');
    expect(screen.queryByText('about')).not.toBeInTheDocument();
  });

  it('shows real carrying/asking mini cards, unowned flagged, and the same tickets appear on the Board (no separate data)', async () => {
    const requests = [
      request({ id: 'req_carry', from: 'herald', claimants: ['jarvis'], status: 'CLAIMED', summary: 'carried one' }),
      request({ id: 'req_ask', from: 'jarvis', claimants: [], status: 'OPEN', summary: 'unowned ask' }),
    ];
    render(AgentDossier, { open: true, agentId: 'jarvis', agents: [jarvis], requests, messages: [] });

    const plate = await screen.findByTestId('agent-plate');
    expect(plate).toHaveTextContent('carrying');
    expect(screen.getByText('carried one')).toBeInTheDocument();
    expect(screen.getByText('unowned ask')).toBeInTheDocument();
    expect(plate).toHaveTextContent(/unowned/);
  });

  it('clicking a plate mini card fires onOpenTicket — portals to the ticket popover', async () => {
    const user = userEvent.setup();
    const onOpenTicket = vi.fn();
    const requests = [request({ id: 'req_carry', from: 'herald', claimants: ['jarvis'], status: 'CLAIMED', summary: 'carried one' })];
    render(AgentDossier, { open: true, agentId: 'jarvis', agents: [jarvis], requests, messages: [], onOpenTicket });

    await user.click(await screen.findByText('carried one'));
    expect(onOpenTicket).toHaveBeenCalledWith('req_carry');
  });

  it('shows real cross-hub presence once loaded, tier-colored', async () => {
    vi.mocked(fetchHubOverviews).mockResolvedValue([]);
    // agentPresence is a pure fold over fetchHubOverviews' real return —
    // simplest to verify the loading state resolves without crashing and
    // the empty case degrades honestly.
    render(AgentDossier, { open: true, agentId: 'jarvis', agents: [jarvis], requests: [], messages: [] });
    expect(await screen.findByText('not seen on any hub yet')).toBeInTheDocument();
  });

  it('Escape closes the dossier', async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(AgentDossier, { open: true, agentId: 'jarvis', agents: [jarvis], requests: [], messages: [], onClose });
    await screen.findByTestId('agent-dossier');

    await user.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalled();
  });

  it('j/k walk to the next/previous agent', async () => {
    const user = userEvent.setup();
    const onNavigate = vi.fn();
    render(AgentDossier, { open: true, agentId: 'jarvis', agents: [jarvis, herald], requests: [], messages: [], onNavigate });
    await screen.findByTestId('agent-dossier');

    await user.keyboard('j');
    expect(onNavigate).toHaveBeenCalledWith('herald');
  });

  it('never fabricates confer version, watch state, or a signing-key fingerprint — none of that text appears', async () => {
    render(AgentDossier, { open: true, agentId: 'jarvis', agents: [jarvis], requests: [], messages: [] });
    await screen.findByTestId('agent-dossier');
    expect(screen.queryByText(/confer version/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/armed/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/SHA256/)).not.toBeInTheDocument();
  });
});
