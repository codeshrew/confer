import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import CommandPalette from './CommandPalette.svelte';
import type { HubDomain } from '../attention';

function domain(overrides: Partial<HubDomain> = {}): HubDomain {
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

const DOMAINS: HubDomain[] = [
  domain({ hub: 'agent-coord', label: 'agent-coord', tier: 'own', health: 'ok' }),
  domain({ hub: 'confer-lab', label: 'confer-lab', tier: 'shared', health: 'warn' }),
  domain({
    hub: 'confer-jarvis-orbit',
    label: 'codeshrew/confer-jarvis-orbit',
    tier: 'foreign',
    health: 'critical',
    agents: [
      {
        id: 'orbit-work',
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
  }),
];

describe('CommandPalette', () => {
  it('does not render when closed', () => {
    render(CommandPalette, { open: false, domains: DOMAINS, onSelect: vi.fn(), onClose: vi.fn() });
    expect(screen.queryByTestId('command-palette')).not.toBeInTheDocument();
  });

  it('lists every domain when the query is empty, each with its real tier + health hint', async () => {
    render(CommandPalette, { open: true, domains: DOMAINS, onSelect: vi.fn(), onClose: vi.fn() });

    const rows = await screen.findAllByTestId('palette-row');
    expect(rows).toHaveLength(3);
    expect(screen.getByText('healthy')).toBeInTheDocument();
    // Real per-agent detail, not a generic "critical" gloss — hubHealthReason
    // names the actual down agent.
    expect(screen.getByText('Work Orbit down')).toBeInTheDocument();
  });

  it('fuzzy-filters as you type — "orb" ranks the orbit hub\'s tight match first, tier-labeled', async () => {
    const user = userEvent.setup();
    render(CommandPalette, { open: true, domains: DOMAINS, onSelect: vi.fn(), onClose: vi.fn() });

    await user.type(screen.getByTestId('palette-input'), 'orb');

    const rows = await screen.findAllByTestId('palette-row');
    expect(rows[0]).toHaveTextContent('codeshrew/confer-jarvis-orbit');
    expect(rows[0]).toHaveTextContent('foreign');
    expect(rows[0]).toHaveClass('sel');
    // agent-coord has no o-r-b subsequence at all — excluded outright.
    expect(screen.queryByText('agent-coord')).not.toBeInTheDocument();
  });

  it('a query matching nothing shows the empty state, not a blank list', async () => {
    const user = userEvent.setup();
    render(CommandPalette, { open: true, domains: DOMAINS, onSelect: vi.fn(), onClose: vi.fn() });

    await user.type(screen.getByTestId('palette-input'), 'zzz-no-match');
    expect(await screen.findByText(/no hub matches/)).toBeInTheDocument();
  });

  it('ArrowDown/ArrowUp move the selection, Enter selects it and closes', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(CommandPalette, { open: true, domains: DOMAINS, onSelect, onClose });

    const input = screen.getByTestId('palette-input');
    await user.click(input);
    await user.keyboard('{ArrowDown}{ArrowDown}{Enter}');

    expect(onSelect).toHaveBeenCalledWith('confer-jarvis-orbit');
    expect(onClose).toHaveBeenCalled();
  });

  it('Escape closes without selecting', async () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(CommandPalette, { open: true, domains: DOMAINS, onSelect, onClose });

    await user.click(screen.getByTestId('palette-input'));
    await user.keyboard('{Escape}');

    expect(onSelect).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it('clicking a row selects that hub', async () => {
    const onSelect = vi.fn();
    const user = userEvent.setup();
    render(CommandPalette, { open: true, domains: DOMAINS, onSelect, onClose: vi.fn() });

    const rows = await screen.findAllByTestId('palette-row');
    await user.click(rows[1]!); // confer-lab
    expect(onSelect).toHaveBeenCalledWith('confer-lab');
  });

  it('clicking the backdrop closes; clicking inside the panel does not', async () => {
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(CommandPalette, { open: true, domains: DOMAINS, onSelect: vi.fn(), onClose });

    await user.click(screen.getByTestId('command-palette'));
    expect(onClose).not.toHaveBeenCalled();

    await user.click(screen.getByTestId('palette-backdrop'));
    expect(onClose).toHaveBeenCalled();
  });
});
