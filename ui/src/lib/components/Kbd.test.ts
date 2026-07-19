import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import Kbd from './Kbd.svelte';

describe('Kbd', () => {
  it('renders the literal keys label', () => {
    render(Kbd, { keys: '⌘1' });
    expect(screen.getByText('⌘1')).toBeInTheDocument();
  });

  it('renders as a <kbd> element (semantic, not a generic span)', () => {
    render(Kbd, { keys: 'Ctrl+K' });
    expect(screen.getByText('Ctrl+K').tagName).toBe('KBD');
  });
});
