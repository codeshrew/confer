import { afterEach, describe, expect, it, vi } from 'vitest';
import { copyToClipboard } from './clipboard';

describe('copyToClipboard', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    delete (navigator as { clipboard?: unknown }).clipboard;
  });

  it('uses navigator.clipboard.writeText when available (secure context)', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });

    const ok = await copyToClipboard('msg_a1b2');

    expect(ok).toBe(true);
    expect(writeText).toHaveBeenCalledWith('msg_a1b2');
  });

  it('falls back to execCommand when navigator.clipboard is undefined (the plain-HTTP LAN-IP case)', async () => {
    Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true });
    const execCommand = vi.fn().mockReturnValue(true);
    document.execCommand = execCommand;

    const ok = await copyToClipboard('req_c3d4');

    expect(ok).toBe(true);
    expect(execCommand).toHaveBeenCalledWith('copy');
  });

  it('falls back to execCommand when navigator.clipboard.writeText rejects', async () => {
    const writeText = vi.fn().mockRejectedValue(new Error('not allowed'));
    Object.defineProperty(navigator, 'clipboard', {
      value: { writeText },
      configurable: true,
    });
    const execCommand = vi.fn().mockReturnValue(true);
    document.execCommand = execCommand;

    const ok = await copyToClipboard('msg_e5f6');

    expect(ok).toBe(true);
    expect(execCommand).toHaveBeenCalledWith('copy');
  });

  it('the fallback textarea is added and removed from the DOM, and carries the exact text', async () => {
    Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true });
    let seenValue: string | null = null;
    document.execCommand = vi.fn().mockImplementation(() => {
      const ta = document.querySelector('textarea');
      seenValue = ta?.value ?? null;
      return true;
    });

    await copyToClipboard('req_deadbeef');

    expect(seenValue).toBe('req_deadbeef');
    expect(document.querySelector('textarea')).not.toBeInTheDocument();
  });

  it('returns false (does not throw) when both mechanisms are unavailable', async () => {
    Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true });
    // @ts-expect-error -- simulate an execCommand-less environment
    document.execCommand = undefined;

    const ok = await copyToClipboard('msg_nope');

    expect(ok).toBe(false);
  });
});
