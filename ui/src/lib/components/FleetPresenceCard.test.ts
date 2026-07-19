import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import FleetPresenceCard from './FleetPresenceCard.svelte';
import type { Agent } from '../types';

const jarvis: Agent = {
  id: 'jarvis',
  display: 'Jarvis',
  desc: 'OpenJarvis · design-review partner',
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
  wip: [{ id: 'req_1', summary: 'x', status: 'CLAIMED' }],
};

describe('FleetPresenceCard', () => {
  it('renders name, role, liveness, trust chip, and a real WIP chip', () => {
    render(FleetPresenceCard, { agent: jarvis, sparkline: [1, 2, 3], hubs: [] });
    expect(screen.getByText('Jarvis')).toBeInTheDocument();
    expect(screen.getByText('OpenJarvis · design-review partner')).toBeInTheDocument();
    expect(screen.getByText('⬤ live')).toBeInTheDocument();
    expect(screen.getByText('✓ signed')).toBeInTheDocument();
    expect(screen.getByText('● 1 WIP')).toBeInTheDocument();
  });

  it('a down agent shows a hollow avatar and "○ down"', () => {
    const { container } = render(FleetPresenceCard, {
      agent: { ...jarvis, liveness: 'down', live: false },
      sparkline: [],
      hubs: [],
    });
    expect(screen.getByText('○ down')).toBeInTheDocument();
    expect(container.querySelector('.fp-avatar.down')).toBeInTheDocument();
  });

  it('shows real tier-colored hub-membership dots when presence is known', () => {
    render(FleetPresenceCard, {
      agent: jarvis,
      sparkline: [],
      hubs: [
        { hub: 'confer-lab', tier: 'shared', lastTs: null },
        { hub: 'confer-jarvis-orbit', tier: 'foreign', lastTs: null },
      ],
    });
    expect(screen.getByText(/confer-lab/)).toBeInTheDocument();
    expect(screen.getByText(/confer-jarvis-orbit/)).toBeInTheDocument();
  });

  it('fires onOpen with the agent id when clicked', async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    render(FleetPresenceCard, { agent: jarvis, sparkline: [], hubs: [], onOpen });
    await user.click(screen.getByTestId('fleet-presence-card'));
    expect(onOpen).toHaveBeenCalledWith('jarvis');
  });
});
