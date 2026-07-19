import { afterEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import { tick } from 'svelte';
import FocusChip from './FocusChip.svelte';
import { paneFocus } from '../paneFocus.svelte';

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const stop of cleanups.splice(0)) stop();
  document.body.innerHTML = '';
});

function registerPane(id: string, label: string) {
  const el = document.createElement('div');
  document.body.appendChild(el);
  const stop = paneFocus.register({ id, label, el, getRect: () => ({ top: 0, left: 0, width: 100, height: 100 }) });
  cleanups.push(stop);
  return el;
}

describe('FocusChip', () => {
  it('renders nothing when no pane is registered', () => {
    render(FocusChip);
    expect(screen.queryByTestId('focus-chip')).not.toBeInTheDocument();
  });

  it('shows the currently-focused pane\'s real label', () => {
    registerPane('rail', 'Hubs');
    render(FocusChip);
    expect(screen.getByTestId('focus-chip')).toHaveTextContent('Hubs');
  });

  it('updates when focus moves to a different pane', async () => {
    registerPane('rail', 'Hubs');
    registerPane('stream', 'Chat stream');
    render(FocusChip);

    expect(screen.getByTestId('focus-chip')).toHaveTextContent('Hubs');
    paneFocus.focus('stream');
    await tick();
    expect(screen.getByTestId('focus-chip')).toHaveTextContent('Chat stream');
  });
});
