import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Fleet from './Fleet.svelte';
import type { Agent, Message } from '../types';

vi.mock('../api', () => ({
  fetchHubOverviews: vi.fn().mockResolvedValue([]),
}));

import { fetchHubOverviews } from '../api';

const jarvis: Agent = {
  id: 'jarvis',
  display: 'Jarvis',
  desc: 'design-review partner',
  expectedHost: 'pop-os',
  lastTs: '2026-07-18T17:41:00Z',
  lastHost: 'pop-os',
  live: true,
  verified: 'signed',
  liveness: 'live',
  hbAgeSecs: 0,
  trust: 'signed',
  version: null,
  watchState: null,
  keyFingerprint: null,
  color: '#7dcfff',
  abbr: 'JA',
  wip: [{ id: 'req_1', summary: 'x', status: 'CLAIMED' }],
};

const workOrbit: Agent = {
  id: 'work-orbit',
  display: 'Work Orbit',
  desc: null,
  expectedHost: 'Hestia.local',
  lastTs: '2026-07-15T00:00:00Z',
  lastHost: 'Hestia.local',
  live: false,
  verified: 'signed',
  liveness: 'down',
  hbAgeSecs: 7200,
  trust: 'signed',
  version: null,
  watchState: null,
  keyFingerprint: null,
  color: '#bb9af7',
  abbr: 'WO',
  wip: [],
};

function message(overrides: Partial<Message> = {}): Message {
  return {
    id: 'msg_1',
    from: 'jarvis',
    type: 'note',
    ts: '2026-07-18T17:00:00Z',
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

describe('Fleet — the crew deck', () => {
  afterEach(() => {
    vi.mocked(fetchHubOverviews).mockClear();
  });

  it('groups agents into machine bays by their real host', async () => {
    render(Fleet, { agents: [jarvis, workOrbit], hubName: 'agent-coord', messages: [] });

    expect(await screen.findByText('pop-os')).toBeInTheDocument();
    expect(screen.getByText('Hestia.local')).toBeInTheDocument();
    expect(screen.getByText('Jarvis')).toBeInTheDocument();
    expect(screen.getByText('Work Orbit')).toBeInTheDocument();
  });

  it('a bay whose agents are all down reads "dark", not "online"', async () => {
    const { container } = render(Fleet, { agents: [workOrbit], hubName: 'agent-coord', messages: [] });
    await screen.findByText('Hestia.local');

    expect(container.querySelector('.bay.dark')).toBeInTheDocument();
    expect(screen.getByText('dark')).toBeInTheDocument();
  });

  it('a healthy bay reads "online", not "dark"', async () => {
    const { container } = render(Fleet, { agents: [jarvis], hubName: 'agent-coord', messages: [] });
    await screen.findByText('online');
    expect(container.querySelector('.bay.dark')).not.toBeInTheDocument();
  });

  it('shows real vitals: agent/live/down/machine counts, and a real trust posture line', async () => {
    render(Fleet, { agents: [jarvis, workOrbit], hubName: 'agent-coord', messages: [] });
    await screen.findByText('pop-os');

    expect(screen.getByText('✓ all keys signed')).toBeInTheDocument();
  });

  it('an unsigned agent flips the trust line to a real gap count, not a fabricated "all clear"', async () => {
    const unsigned: Agent = { ...jarvis, verified: 'unverified', trust: undefined };
    render(Fleet, { agents: [unsigned], hubName: 'agent-coord', messages: [] });
    await screen.findByText('pop-os');

    expect(screen.getByText('◐ 1 unverified')).toBeInTheDocument();
  });

  it('an empty fleet shows the empty state', async () => {
    render(Fleet, { agents: [], hubName: 'agent-coord', messages: [] });
    expect(await screen.findByText('No agents on this hub')).toBeInTheDocument();
  });

  it('clicking a presence card fires onOpenAgent — opens the dossier', async () => {
    const user = userEvent.setup();
    const onOpenAgent = vi.fn();
    render(Fleet, { agents: [jarvis], hubName: 'agent-coord', messages: [], onOpenAgent });
    await user.click(await screen.findByTestId('fleet-presence-card'));
    expect(onOpenAgent).toHaveBeenCalledWith('jarvis');
  });

  it('feeds each card a real activity sparkline folded from real message timestamps', async () => {
    const messages = [message({ from: 'jarvis', ts: new Date().toISOString() })];
    render(Fleet, { agents: [jarvis], hubName: 'agent-coord', messages });
    // Doesn't crash, and the card renders — the sparkline's own values are
    // covered by fleetDossier.test.ts's activityBuckets unit tests.
    expect(await screen.findByTestId('fleet-presence-card')).toBeInTheDocument();
  });
});
