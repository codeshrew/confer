import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import HubRail from './HubRail.svelte';
import type { Attention } from '../attention';
import { getAttention } from '../api';

vi.mock('../api', () => ({
  getAttention: vi.fn(),
}));

function domain(overrides: Partial<Attention['domains'][number]> = {}): Attention['domains'][number] {
  return {
    hub: 'lab',
    label: 'lab',
    tier: 'own',
    sync: { lastFetchedSecs: 5, behind: 0, pending: 0, reachable: true },
    health: 'ok',
    agents: [],
    workInFlight: [],
    ...overrides,
  };
}

const FOUR_TIERS: Attention = {
  needsYou: [],
  coordination: [],
  fleet: [],
  domains: [
    domain({ hub: 'agent-coord', label: 'agent-coord', tier: 'own', health: 'ok' }),
    domain({ hub: 'confer-lab', label: 'confer-lab', tier: 'shared', health: 'warn' }),
    domain({ hub: 'confer-jarvis-orbit', label: 'confer-jarvis-orbit', tier: 'foreign', health: 'critical' }),
    domain({ hub: 'sandbox', label: 'sandbox', tier: null, sync: null, health: 'unknown' }),
  ],
  metrics: { openRequests: 0, perHub: [] },
};

describe('HubRail — tier grouping', () => {
  it('groups hubs under Home/Shared/Foreign/Unclassified headers, in that order, by REAL Hub.tier', async () => {
    vi.mocked(getAttention).mockResolvedValue(FOUR_TIERS);
    render(HubRail, { currentHub: 'agent-coord', currentView: 'chat' });

    await screen.findByText('agent-coord');
    const rail = screen.getByTestId('hub-rail');
    const text = rail.textContent ?? '';
    // Order matters — Home, then Shared, then Foreign, then Unclassified.
    expect(text.indexOf('Home')).toBeGreaterThanOrEqual(0);
    expect(text.indexOf('Home')).toBeLessThan(text.indexOf('Shared'));
    expect(text.indexOf('Shared')).toBeLessThan(text.indexOf('Foreign'));
    expect(text.indexOf('Foreign')).toBeLessThan(text.indexOf('Unclassified'));

    expect(within(rail).getByText('agent-coord')).toBeInTheDocument();
    expect(within(rail).getByText('sandbox')).toBeInTheDocument();
  });

  it('never renders an empty group — a fixture with only "own" hubs shows just Home', async () => {
    vi.mocked(getAttention).mockResolvedValue({
      ...FOUR_TIERS,
      domains: [domain({ hub: 'agent-coord', label: 'agent-coord', tier: 'own' })],
    });
    render(HubRail, { currentHub: 'agent-coord', currentView: 'chat' });

    await screen.findByText('agent-coord');
    const rail = screen.getByTestId('hub-rail');
    expect(within(rail).getByText('Home')).toBeInTheDocument();
    expect(within(rail).queryByText('Shared')).not.toBeInTheDocument();
    expect(within(rail).queryByText('Foreign')).not.toBeInTheDocument();
    expect(within(rail).queryByText('Unclassified')).not.toBeInTheDocument();
  });
});

describe('HubRail — health dots are the real, folded signal (law #3)', () => {
  it('renders a distinct dot class for ok/warn/critical/unknown — unknown is its OWN state, not a fake ok', async () => {
    vi.mocked(getAttention).mockResolvedValue(FOUR_TIERS);
    render(HubRail, { currentHub: 'agent-coord', currentView: 'chat' });

    const hubs = await screen.findAllByTestId('hub-rail-hub');
    const byLabel = (label: string) => hubs.find((h) => h.textContent?.includes(label))!;

    expect(byLabel('agent-coord').querySelector('.hr-hdot-ok')).toBeInTheDocument();
    expect(byLabel('confer-lab').querySelector('.hr-hdot-warn')).toBeInTheDocument();
    expect(byLabel('confer-jarvis-orbit').querySelector('.hr-hdot-critical')).toBeInTheDocument();
    expect(byLabel('sandbox').querySelector('.hr-hdot-unknown')).toBeInTheDocument();
  });

  it('a null-tier hub shows "unclassified", never defaulted to a tier label', async () => {
    vi.mocked(getAttention).mockResolvedValue(FOUR_TIERS);
    render(HubRail, { currentHub: 'agent-coord', currentView: 'chat' });

    await screen.findByText('sandbox');
    expect(screen.getByText('Unclassified')).toBeInTheDocument();
  });
});

describe('HubRail — clicks', () => {
  it('clicking a hub fires onHubChange with its id', async () => {
    vi.mocked(getAttention).mockResolvedValue(FOUR_TIERS);
    const onHubChange = vi.fn();
    const user = userEvent.setup();
    render(HubRail, { currentHub: 'agent-coord', currentView: 'chat', onHubChange });

    await user.click(await screen.findByText('confer-lab'));
    expect(onHubChange).toHaveBeenCalledWith('confer-lab');
  });

  it('clicking "All hubs" fires onAllHubs', async () => {
    vi.mocked(getAttention).mockResolvedValue(FOUR_TIERS);
    const onAllHubs = vi.fn();
    const user = userEvent.setup();
    render(HubRail, { currentHub: 'agent-coord', currentView: 'chat', onAllHubs });

    await user.click(await screen.findByTestId('hub-rail-all'));
    expect(onAllHubs).toHaveBeenCalled();
  });

  it('reports the current hub\'s real tier up via onActiveTierChange, including null', async () => {
    vi.mocked(getAttention).mockResolvedValue(FOUR_TIERS);
    const onActiveTierChange = vi.fn();
    render(HubRail, { currentHub: 'sandbox', currentView: 'chat', onActiveTierChange });

    await waitFor(() => expect(onActiveTierChange).toHaveBeenCalledWith(null));
  });
});

describe('HubRail — keyboard: j/k + Enter, ⌘K', () => {
  it('j moves the roving-tabindex focus forward, k moves it back, Enter activates it', async () => {
    vi.mocked(getAttention).mockResolvedValue(FOUR_TIERS);
    const onHubChange = vi.fn();
    const user = userEvent.setup();
    render(HubRail, { currentHub: 'agent-coord', currentView: 'chat', onHubChange });

    const allEntry = await screen.findByTestId('hub-rail-all');
    const hubButton = (label: string) =>
      screen.getAllByTestId('hub-rail-hub').find((h) => h.textContent?.includes(label))!;

    allEntry.focus();
    expect(allEntry).toHaveFocus();

    // All hubs -> (Home group header, skipped) -> agent-coord
    await user.keyboard('j');
    expect(hubButton('agent-coord')).toHaveFocus();

    await user.keyboard('j');
    // agent-coord -> (Shared header, skipped) -> confer-lab
    expect(hubButton('confer-lab')).toHaveFocus();

    await user.keyboard('k');
    expect(hubButton('agent-coord')).toHaveFocus();

    await user.keyboard('{Enter}');
    expect(onHubChange).toHaveBeenCalledWith('agent-coord');
  });

  it('⌘K opens the command palette from anywhere — not just while the rail has focus', async () => {
    vi.mocked(getAttention).mockResolvedValue(FOUR_TIERS);
    render(HubRail, { currentHub: 'agent-coord', currentView: 'chat' });
    await screen.findByTestId('hub-rail');

    expect(screen.queryByTestId('command-palette')).not.toBeInTheDocument();
    await userEvent.keyboard('{Meta>}k{/Meta}');
    expect(await screen.findByTestId('command-palette')).toBeInTheDocument();
  });
});
