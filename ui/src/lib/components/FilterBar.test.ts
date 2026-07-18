import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import FilterBar from './FilterBar.svelte';

// Regression coverage for the false-affordance cleanup: the STATUS chip row
// (wrote to `statusFilter` state that App.svelte never read back into
// Chat/Board) and the WHO chip row (no onclick at all — a dead click target)
// were both confirmed-dead and removed. Only Type + Density remain.
describe('FilterBar', () => {
  it('renders Notes/Requests chips and fires their toggles', async () => {
    const user = userEvent.setup();
    const onToggleNotes = vi.fn();
    const onToggleReqs = vi.fn();
    render(FilterBar, { notesOn: true, reqsOn: true, onToggleNotes, onToggleReqs });

    await user.click(screen.getByText('Notes'));
    expect(onToggleNotes).toHaveBeenCalled();

    await user.click(screen.getByText('Requests'));
    expect(onToggleReqs).toHaveBeenCalled();
  });

  it('does not render a Status chip row', () => {
    render(FilterBar, { notesOn: true, reqsOn: true });

    expect(screen.queryByText('Status')).not.toBeInTheDocument();
    expect(screen.queryByText('Needs attention')).not.toBeInTheDocument();
    expect(screen.queryByText('Backlog')).not.toBeInTheDocument();
  });

  it('does not render a Who chip row', () => {
    const { container } = render(FilterBar, { notesOn: true, reqsOn: true });

    expect(screen.queryByText('Who')).not.toBeInTheDocument();
    expect(container.querySelector('.chip.ag')).not.toBeInTheDocument();
  });

  it('shows the density segmented control only when chatDensity is supplied', () => {
    const { rerender, container } = render(FilterBar, { notesOn: true, reqsOn: true });
    expect(container.querySelector('[data-testid="density-toggle"]')).not.toBeInTheDocument();

    rerender({ notesOn: true, reqsOn: true, chatDensity: 'summary' });
    expect(container.querySelector('[data-testid="density-toggle"]')).toBeInTheDocument();
  });

  it('fires onChatDensityChange when a density option is clicked', async () => {
    const user = userEvent.setup();
    const onChatDensityChange = vi.fn();
    render(FilterBar, { notesOn: true, reqsOn: true, chatDensity: 'summary', onChatDensityChange });

    await user.click(screen.getByText('Full'));
    expect(onChatDensityChange).toHaveBeenCalledWith('full');
  });
});
