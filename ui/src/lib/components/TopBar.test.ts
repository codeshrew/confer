import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import TopBar from './TopBar.svelte';
import type { Hub } from '../types';

const hubs: Hub[] = [
  { id: 'agent-coord', label: 'agent-coord', name: 'agent-coord', current: true, agentCount: 6 },
  { id: 'confer-lab', label: 'confer-lab', name: 'confer-lab', current: false, agentCount: 2 },
];

describe('TopBar', () => {
  it('renders a hub pill for each hub in props, with its agent count', () => {
    render(TopBar, { hubs, currentHub: 'agent-coord', currentView: 'chat' });

    expect(screen.getByRole('tab', { name: /agent-coord/ })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: /confer-lab/ })).toBeInTheDocument();
    expect(screen.getByText('6')).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
  });

  it('fires onHubChange when a hub pill is clicked', async () => {
    const user = userEvent.setup();
    const onHubChange = vi.fn();
    render(TopBar, { hubs, currentHub: 'agent-coord', currentView: 'chat', onHubChange });

    await user.click(screen.getByRole('tab', { name: /confer-lab/ }));

    expect(onHubChange).toHaveBeenCalledWith('confer-lab');
  });

  it('fires onViewChange when a view nav item is clicked', async () => {
    const user = userEvent.setup();
    const onViewChange = vi.fn();
    render(TopBar, { hubs, currentHub: 'agent-coord', currentView: 'chat', onViewChange });

    await user.click(screen.getByRole('tab', { name: 'Board' }));

    expect(onViewChange).toHaveBeenCalledWith('board');
  });

  it('renders a hamburger button that fires onMenuToggle when tapped', async () => {
    const user = userEvent.setup();
    const onMenuToggle = vi.fn();
    render(TopBar, { hubs, currentHub: 'agent-coord', currentView: 'chat', onMenuToggle });

    await user.click(screen.getByRole('button', { name: /open menu/i }));

    expect(onMenuToggle).toHaveBeenCalledOnce();
  });

  it('reflects menuOpen in the hamburger label/aria-expanded state', () => {
    const { rerender } = render(TopBar, { hubs, currentHub: 'agent-coord', currentView: 'chat', menuOpen: false });

    const closedBtn = screen.getByRole('button', { name: /open menu/i });
    expect(closedBtn).toHaveAttribute('aria-expanded', 'false');

    rerender({ hubs, currentHub: 'agent-coord', currentView: 'chat', menuOpen: true });

    const openBtn = screen.getByRole('button', { name: /close menu/i });
    expect(openBtn).toHaveAttribute('aria-expanded', 'true');
  });

  it('hides the hamburger entirely when showMenu is false (Repos view: no left rail to open)', () => {
    render(TopBar, { hubs, currentHub: 'agent-coord', currentView: 'repos', showMenu: false });

    expect(screen.queryByTestId('hamburger')).not.toBeInTheDocument();
  });

  it('fires onHelp when the "?" button is clicked — the mouse path for the which-key overlay', async () => {
    const user = userEvent.setup();
    const onHelp = vi.fn();
    render(TopBar, { hubs, currentHub: 'agent-coord', currentView: 'chat', onHelp });

    await user.click(screen.getByRole('button', { name: 'Keyboard shortcuts' }));

    expect(onHelp).toHaveBeenCalledOnce();
  });

  it('calls onThemeToggle and reflects the flipped data-theme on <html>', async () => {
    const user = userEvent.setup();
    document.documentElement.setAttribute('data-theme', 'dark');
    const onThemeToggle = vi.fn(() => {
      document.documentElement.setAttribute('data-theme', 'light');
    });
    render(TopBar, { hubs, currentHub: 'agent-coord', currentView: 'chat', theme: 'dark', onThemeToggle });

    await user.click(screen.getByRole('button', { name: /toggle theme/i }));

    expect(onThemeToggle).toHaveBeenCalled();
    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
  });
});
