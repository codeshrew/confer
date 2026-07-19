import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import CopyIdButton from './CopyIdButton.svelte';

describe('CopyIdButton', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    delete (navigator as { clipboard?: unknown }).clipboard;
  });

  it('copies the bare id via navigator.clipboard and shows check feedback', async () => {
    // NOTE: userEvent.setup() installs its OWN navigator.clipboard stub, so
    // our mock must be defined AFTER setup() or it gets clobbered.
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });

    render(CopyIdButton, { id: 'msg_a1b2' });

    const btn = screen.getByTestId('copy-id-btn');
    expect(btn).toHaveAttribute('aria-label', 'Copy id msg_a1b2');

    await user.click(btn);

    // The click handler is async (awaits the clipboard write before flipping
    // the icon) — its completion isn't guaranteed to have flushed by the
    // time `user.click` itself resolves, so wait for the observable effect
    // rather than asserting immediately.
    await vi.waitFor(() => {
      expect(writeText).toHaveBeenCalledWith('msg_a1b2');
      expect(btn).toHaveAttribute('aria-label', 'Copied msg_a1b2');
    });
    expect(btn.className).toMatch(/copied/);
  });

  it('reverts the check feedback back to the copy icon after ~1.2s', async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });

    render(CopyIdButton, { id: 'msg_a1b2' });
    const btn = screen.getByTestId('copy-id-btn');

    await user.click(btn);
    await vi.waitFor(() => {
      expect(btn).toHaveAttribute('aria-label', 'Copied msg_a1b2');
    });

    await vi.waitFor(
      () => {
        expect(btn).toHaveAttribute('aria-label', 'Copy id msg_a1b2');
      },
      { timeout: 2000, interval: 50 }
    );
  });

  it('falls back to execCommand copy when clipboard API is unavailable (the plain-HTTP LAN-IP case)', async () => {
    const user = userEvent.setup();
    // Explicitly undefined AFTER setup() — overrides user-event's own
    // clipboard stub to simulate a browser with no Clipboard API at all.
    Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true });
    const execCommand = vi.fn().mockReturnValue(true);
    document.execCommand = execCommand;

    render(CopyIdButton, { id: 'msg_e5f6' });
    await user.click(screen.getByTestId('copy-id-btn'));

    await vi.waitFor(() => {
      expect(execCommand).toHaveBeenCalledWith('copy');
    });
  });
});
