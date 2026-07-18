import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import ChatStream from './ChatStream.svelte';
import type { Agent, Message, RequestRow } from '../types';

const reader: Agent = {
  id: 'reader',
  display: 'Reader',
  desc: null,
  expectedHost: null,
  lastTs: null,
  lastHost: null,
  live: true,
  verified: 'signed',
  color: 'var(--ag-reader)',
  abbr: 'RE',
  wip: [],
};

function msg(id: string, ts: string, summary: string): Message {
  return {
    id,
    from: 'reader',
    type: 'note',
    ts,
    host: null,
    to: [],
    cc: [],
    topic: 'general',
    summary,
    body: summary,
    of: null,
    replyTo: null,
    supersedes: null,
    refs: [],
  };
}

const requests: RequestRow[] = [];

describe('ChatStream — scrollToMessageId + highlight pulse (design/41 Phase 0)', () => {
  const originalMatchMedia = window.matchMedia;
  const originalScrollIntoView = Element.prototype.scrollIntoView;
  let scrollIntoView: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView as unknown as typeof Element.prototype.scrollIntoView;
    // Default: no reduced-motion preference.
    window.matchMedia = vi.fn().mockReturnValue({ matches: false }) as unknown as typeof window.matchMedia;
  });

  afterEach(() => {
    Element.prototype.scrollIntoView = originalScrollIntoView;
    window.matchMedia = originalMatchMedia;
    vi.restoreAllMocks();
  });

  it('scrolls to a message already in the loaded window and applies the pulse class', async () => {
    const messages = [msg('m1', '2026-07-17T14:00:00Z', 'first'), msg('m2', '2026-07-17T14:01:00Z', 'second')];
    const { container } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
      scrollToMessageId: 'm2',
      scrollToken: 1,
    });

    await vi.waitFor(() => {
      expect(scrollIntoView).toHaveBeenCalled();
    });
    const target = container.querySelector('[data-msg-id="m2"]');
    expect(target).toBeInTheDocument();
    await vi.waitFor(() => {
      expect(target?.className).toMatch(/pulse/);
    });
  });

  it('loads older pages until the target message is found, then scrolls to it', async () => {
    const initial = [msg('m5', '2026-07-17T14:05:00Z', 'fifth')];
    let resolveLoadOlder: (() => void) | undefined;
    // Mirrors App.svelte's real onLoadOlder: the parent's fetch resolves,
    // it prepends the page (so `messages` updates), THEN the promise
    // returned to ChatStream resolves with the count added.
    const onLoadOlder = vi.fn().mockImplementation(
      () =>
        new Promise<number>((resolve) => {
          resolveLoadOlder = () => resolve(1);
        })
    );

    const { rerender, container } = render(ChatStream, {
      messages: initial,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
      hasMore: true,
      onLoadOlder,
      scrollToMessageId: 'm4',
      scrollToken: 1,
    });

    await vi.waitFor(() => {
      expect(onLoadOlder).toHaveBeenCalled();
    });

    await rerender({
      messages: [msg('m4', '2026-07-17T14:04:00Z', 'fourth'), ...initial],
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
      hasMore: false,
      onLoadOlder,
      scrollToMessageId: 'm4',
      scrollToken: 1,
    });
    resolveLoadOlder?.();

    await vi.waitFor(() => {
      expect(container.querySelector('[data-msg-id="m4"]')).toBeInTheDocument();
    });
    await vi.waitFor(() => {
      expect(scrollIntoView).toHaveBeenCalled();
    });
  });

  it('no-ops gracefully when the target message is truly unavailable (older pages exhausted)', async () => {
    const messages = [msg('m1', '2026-07-17T14:00:00Z', 'first')];
    const onLoadOlder = vi.fn().mockResolvedValue(0); // nothing more to load, ever
    render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
      hasMore: true,
      onLoadOlder,
      scrollToMessageId: 'ghost_msg',
      scrollToken: 1,
    });

    await vi.waitFor(() => {
      expect(onLoadOlder).toHaveBeenCalled();
    });
    // Give any pending microtasks a moment to settle, then assert no crash
    // and no scroll happened for a message that was never found.
    await new Promise((r) => setTimeout(r, 50));
    expect(scrollIntoView).not.toHaveBeenCalled();
  });

  it('respects prefers-reduced-motion: scrolls without smooth behavior and skips the pulse', async () => {
    window.matchMedia = vi.fn().mockReturnValue({ matches: true }) as unknown as typeof window.matchMedia;
    const messages = [msg('m1', '2026-07-17T14:00:00Z', 'first')];
    const { container } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
      scrollToMessageId: 'm1',
      scrollToken: 1,
    });

    await vi.waitFor(() => {
      expect(scrollIntoView).toHaveBeenCalledWith(expect.objectContaining({ behavior: 'auto' }));
    });
    const target = container.querySelector('[data-msg-id="m1"]');
    expect(target?.className).not.toMatch(/pulse/);
  });

  it('a repeat scroll to the SAME message id re-triggers when scrollToken bumps', async () => {
    const messages = [msg('m1', '2026-07-17T14:00:00Z', 'first')];
    const { rerender } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
      scrollToMessageId: 'm1',
      scrollToken: 1,
    });
    await vi.waitFor(() => expect(scrollIntoView).toHaveBeenCalledTimes(1));

    await rerender({
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
      scrollToMessageId: 'm1',
      scrollToken: 2,
    });

    await vi.waitFor(() => expect(scrollIntoView).toHaveBeenCalledTimes(2));
  });
});
