import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import CopiedToast from './CopiedToast.svelte';

describe('CopiedToast', () => {
  it('renders nothing when text is null', () => {
    render(CopiedToast, { text: null });
    expect(screen.queryByTestId('copied-toast')).not.toBeInTheDocument();
  });

  it('renders the given text as a status region', () => {
    render(CopiedToast, { text: 'copied msg_01jq…' });
    const toast = screen.getByTestId('copied-toast');
    expect(toast).toHaveTextContent('copied msg_01jq…');
    expect(toast).toHaveAttribute('role', 'status');
  });
});
