import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import SeenIndicator, { type SeenEntry } from './SeenIndicator.svelte';

describe('SeenIndicator', () => {
  it('collapses to "all seen" when every entry is seen', () => {
    const entries: SeenEntry[] = [
      { id: 'reader', name: 'Reader', color: 'var(--ag-reader)', ts: '14:03' },
      { id: 'you', name: 'You', ts: '14:40', isYou: true },
    ];
    render(SeenIndicator, { entries });

    expect(screen.getByText('all seen')).toBeInTheDocument();
    expect(screen.getByText('✓')).toBeInTheDocument();
  });

  it('shows a partial "seen" label with dots when some entries are unseen', () => {
    const entries: SeenEntry[] = [
      { id: 'pipeline', name: 'Pipeline', color: 'var(--ag-pipeline)', ts: '14:21' },
      { id: 'compositor', name: 'Compositor', color: 'var(--ag-compositor)', ts: null, unseen: true },
    ];
    render(SeenIndicator, { entries });

    expect(screen.getByText('seen')).toBeInTheDocument();
    expect(screen.queryByText('all seen')).not.toBeInTheDocument();
  });

  it('reveals the roster popover on click, listing who saw it and when', async () => {
    const user = userEvent.setup();
    const entries: SeenEntry[] = [
      { id: 'pipeline', name: 'Pipeline', color: 'var(--ag-pipeline)', ts: '14:21' },
      { id: 'compositor', name: 'Compositor', color: 'var(--ag-compositor)', ts: null, unseen: true },
    ];
    render(SeenIndicator, { entries });

    const button = screen.getByRole('button', { name: /seen roster/i });
    expect(button).toHaveAttribute('aria-expanded', 'false');

    await user.click(button);

    expect(button).toHaveAttribute('aria-expanded', 'true');
    expect(screen.getByText('Pipeline')).toBeInTheDocument();
    expect(screen.getByText('14:21')).toBeInTheDocument();
    expect(screen.getByText('Compositor')).toBeInTheDocument();
    expect(screen.getByText('unseen')).toBeInTheDocument();
  });

  describe('the "you" entry (piece 4, item 2 — read-state)', () => {
    it('an unseen "you" is NOT rendered as "all seen", and shows "unseen" in the roster like anyone else', async () => {
      const user = userEvent.setup();
      const entries: SeenEntry[] = [
        { id: 'pipeline', name: 'Pipeline', color: 'var(--ag-pipeline)', ts: '14:21' },
        { id: 'you', name: 'You', ts: null, isYou: true, unseen: true },
      ];
      render(SeenIndicator, { entries });

      expect(screen.queryByText('all seen')).not.toBeInTheDocument();

      await user.click(screen.getByRole('button', { name: /seen roster/i }));
      expect(screen.getByText('You')).toBeInTheDocument();
      expect(screen.getByText('unseen')).toBeInTheDocument();
    });

    it('a seen "you" still collapses to "all seen" alongside other real seen entries', () => {
      const entries: SeenEntry[] = [
        { id: 'pipeline', name: 'Pipeline', color: 'var(--ag-pipeline)', ts: '14:21' },
        { id: 'you', name: 'You', ts: '14:40', isYou: true },
      ];
      render(SeenIndicator, { entries });

      expect(screen.getByText('all seen')).toBeInTheDocument();
    });
  });
});

describe('piece 8b — the roster is an agent-dossier entry point', () => {
  it('clicking a real roster row fires onOpenAgent with that agent\'s id, not the toggle', async () => {
    const user = userEvent.setup();
    const onOpenAgent = vi.fn();
    const entries: SeenEntry[] = [{ id: 'pipeline', name: 'Pipeline', color: 'var(--ag-pipeline)', ts: '14:21' }];
    render(SeenIndicator, { entries, onOpenAgent });

    await user.click(screen.getByRole('button', { name: /seen roster/i }));
    await user.click(screen.getByRole('button', { name: /open pipeline's dossier/i }));

    expect(onOpenAgent).toHaveBeenCalledWith('pipeline');
  });

  it('the synthesized "You" row has no dossier — it never fires onOpenAgent', async () => {
    const user = userEvent.setup();
    const onOpenAgent = vi.fn();
    const entries: SeenEntry[] = [{ id: 'you', name: 'You', ts: '14:40', isYou: true }];
    render(SeenIndicator, { entries, onOpenAgent });

    await user.click(screen.getByRole('button', { name: /seen roster/i }));
    expect(screen.queryByRole('button', { name: /open you's dossier/i })).not.toBeInTheDocument();
  });
});
