import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import WhichKeyOverlay from './WhichKeyOverlay.svelte';

describe('WhichKeyOverlay', () => {
  it('renders nothing when closed', () => {
    render(WhichKeyOverlay, { open: false, onClose: vi.fn() });
    expect(screen.queryByTestId('whichkey-backdrop')).not.toBeInTheDocument();
  });

  it('lists the real bound keys — pane focus, command palette, rail nav, and Cmd+number view switches', () => {
    render(WhichKeyOverlay, { open: true, onClose: vi.fn() });

    expect(screen.getByText('Ctrl+h/j/k/l')).toBeInTheDocument();
    expect(screen.getByText('⌘K')).toBeInTheDocument();
    expect(screen.getByText('g g')).toBeInTheDocument();
    expect(screen.getByText('⌘1')).toBeInTheDocument();
    expect(screen.getByText('⌘5')).toBeInTheDocument();
  });

  it('states the three-layer model footnote', () => {
    render(WhichKeyOverlay, { open: true, onClose: vi.fn() });
    expect(
      screen.getByText("Ctrl = panes · bare = content · Cmd = app · only the focused pane's keys fire")
    ).toBeInTheDocument();
  });

  it('the close button closes it', async () => {
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(WhichKeyOverlay, { open: true, onClose });

    await user.click(screen.getByRole('button', { name: 'Close' }));
    expect(onClose).toHaveBeenCalled();
  });

  it('Escape closes it', async () => {
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(WhichKeyOverlay, { open: true, onClose });

    await user.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalled();
  });

  it('clicking the backdrop closes; clicking inside the panel does not', async () => {
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(WhichKeyOverlay, { open: true, onClose });

    await user.click(screen.getByText('Keyboard shortcuts'));
    expect(onClose).not.toHaveBeenCalled();

    await user.click(screen.getByTestId('whichkey-backdrop'));
    expect(onClose).toHaveBeenCalled();
  });
});
