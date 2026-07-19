import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import ChatStream from './ChatStream.svelte';
import { readState } from '../readState.svelte';
import type { Agent, Message, RequestRow, SeenBy } from '../types';

const reader: Agent = {
  id: 'reader',
  display: 'Reader',
  desc: null,
  expectedHost: null,
  lastTs: null,
  lastHost: null,
  live: true,
  verified: 'signed',
  version: null,
  watchState: null,
  keyFingerprint: null,
  color: 'var(--ag-reader)',
  abbr: 'RE',
  wip: [],
};

function msg(id: string, ts: string, summary: string, seenBy: SeenBy[] = []): Message {
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
    seenBy,
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

describe('ChatStream — real seen-by (piece 4, item 2 — retires CONTRACT GAP #58\'s synthesized filler)', () => {
  const jarvis: Agent = {
    id: 'jarvis',
    display: 'Jarvis',
    desc: null,
    expectedHost: null,
    lastTs: '2026-07-14T09:00:00Z',
    lastHost: null,
    live: false,
    verified: 'signed',
    version: null,
    watchState: null,
    keyFingerprint: null,
    color: 'var(--ag-jarvis)',
    abbr: 'JV',
    wip: [],
  };

  it('a recipient absent from message.seenBy is unseen — regardless of Agent.live/lastTs (the old, now-retired heuristic)', () => {
    // jarvis is offline (live:false, stale lastTs) AND absent from seenBy —
    // the real signal (seenBy) is what must decide this now, not the old
    // online/offline guess. reader is ALSO absent from seenBy here, so
    // both are pending; the roster below asserts on jarvis specifically.
    const messages = [msg('m1', '2026-07-17T14:00:00Z', 'code-ref note')];
    const { container } = render(ChatStream, {
      messages,
      requests,
      agents: [reader, jarvis],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });

    const seen = container.querySelector('.seen');
    expect(seen).toBeInTheDocument();
    expect(seen?.className).not.toMatch(/done/);
    const jarvisRow = Array.from(container.querySelectorAll('.roster .rr')).find((el) => el.textContent?.includes('Jarvis'));
    expect(jarvisRow?.className).toMatch(/un/);
    expect(jarvisRow?.textContent).toContain('unseen');
  });

  it('a recipient IN message.seenBy is seen, with the REAL confirmed timestamp — even while offline by Agent.live/lastTs (real data overrides the old heuristic entirely)', () => {
    const messages = [
      msg('m1', '2026-07-17T14:00:00Z', 'code-ref note', [{ role: 'jarvis', ts: '2026-07-17T14:05:00Z' }]),
    ];
    const { container } = render(ChatStream, {
      messages,
      requests,
      agents: [jarvis],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });

    const seen = container.querySelector('.seen');
    expect(seen?.className).toMatch(/done/);
    const jarvisRow = Array.from(container.querySelectorAll('.roster .rr')).find((el) => el.textContent?.includes('Jarvis'));
    expect(jarvisRow?.textContent).not.toContain('unseen');
  });
});

describe('ChatStream — the real "since you last looked" watermark (piece 4, item 2)', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('a never-visited (hub, topic) shows NO "NEW" divider — nothing is flagged new on a first-ever visit', () => {
    const messages = [msg('m1', '2026-07-17T14:00:00Z', 'first'), msg('m2', '2026-07-17T14:05:00Z', 'second')];
    const { container } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'never-visited',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });

    expect(container.querySelector('.newmark')).not.toBeInTheDocument();
  });

  it('an existing watermark flags every message after it as NEW, and none before it', () => {
    readState.setWatermark('agent-coord', 'general', new Date('2026-07-17T14:02:00Z').getTime());
    const messages = [
      msg('m1', '2026-07-17T14:00:00Z', 'before the watermark'),
      msg('m2', '2026-07-17T14:05:00Z', 'after the watermark'),
    ];
    const { container } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });

    const newmark = container.querySelector('.newmark');
    expect(newmark).toBeInTheDocument();
    // The divider sits immediately before the first message past the
    // watermark, not the first message overall.
    expect(newmark?.nextElementSibling?.textContent).toContain('after the watermark');
  });

  it('leaving the topic (unmount) advances the watermark to now', () => {
    // A topic name unique to this test — readState is a real module
    // singleton shared across every test in this file (localStorage.clear()
    // in beforeEach wipes the persisted copy, but not whatever's already
    // in the singleton's own in-memory $state from an earlier test in this
    // same file), so reusing 'general'/'agent-coord' here would race
    // against the "existing watermark" test above.
    const messages = [msg('m1', '2026-07-17T14:00:00Z', 'first')];
    const before = Date.now();
    const { unmount } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'leave-test-topic',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });

    expect(readState.getWatermark('agent-coord', 'leave-test-topic')).toBeNull();
    unmount();
    const after = Date.now();

    const wm = readState.getWatermark('agent-coord', 'leave-test-topic');
    expect(wm).not.toBeNull();
    expect(wm as number).toBeGreaterThanOrEqual(before);
    expect(wm as number).toBeLessThanOrEqual(after);
  });

  it('switching topics (prop change, same mount) advances the OLD topic\'s watermark, not the new one\'s', async () => {
    const messages = [msg('m1', '2026-07-17T14:00:00Z', 'first')];
    const { rerender } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'topic-a',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });

    await rerender({
      messages,
      requests,
      agents: [reader],
      topic: 'topic-b',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });

    expect(readState.getWatermark('agent-coord', 'topic-a')).not.toBeNull();
    expect(readState.getWatermark('agent-coord', 'topic-b')).toBeNull();
  });
});

describe('ChatStream — sticky day header (piece 4, item 3)', () => {
  it('shows a real message count alongside the day label', () => {
    const messages = [
      msg('m1', '2026-07-17T14:00:00Z', 'first'),
      msg('m2', '2026-07-17T15:00:00Z', 'second'),
      msg('m3', '2026-07-17T16:00:00Z', 'third'),
    ];
    const { container } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });

    const daybreak = container.querySelector('.daybreak');
    expect(daybreak).toBeInTheDocument();
    expect(daybreak?.textContent).toContain('3 messages');
  });

  it('one divider per real calendar day, each with its own count', () => {
    const messages = [
      msg('m1', '2026-07-17T14:00:00Z', 'first'),
      msg('m2', '2026-07-18T09:00:00Z', 'second'),
      msg('m3', '2026-07-18T10:00:00Z', 'third'),
    ];
    const { container } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });

    const dividers = container.querySelectorAll('.daybreak');
    expect(dividers).toHaveLength(2);
    expect(dividers[0]?.textContent).toContain('1 message');
    expect(dividers[0]?.textContent).not.toContain('1 messages');
    expect(dividers[1]?.textContent).toContain('2 messages');
  });

  // The actual `position: sticky` pinning is a real-layout/scroll concern
  // jsdom doesn't reliably resolve scoped component CSS for — verified live
  // instead, in e2e/read-state.spec.ts-style Playwright coverage (scrolling
  // the real stream and asserting the header's bounding box stays pinned
  // at the container's top).
});

describe('ChatStream — stick-to-bottom vs. an active hover (design/41 copy-id bug)', () => {
  // Bug: a busy topic's live SSE-appended note snaps `scrollTop` to the new
  // bottom via the stick-to-bottom effect. If that snap lands while the
  // reader is mid-hover lining up a click on a small hover-revealed
  // affordance (CopyIdButton, expand-toggle, ...), every row shifts out from
  // under the stationary cursor and the click lands on whatever is now
  // there instead — silently, with no error. MetaThread (a static,
  // non-live-scrolling list) never had this failure mode, which is why "the
  // same copy affordance works there." Fix: suspend the forced scroll while
  // the pointer is over `.stream`, and catch up the instant it leaves.
  function spyOnScrollTop(el: Element): ReturnType<typeof vi.fn> {
    const setter = vi.fn();
    Object.defineProperty(el, 'scrollTop', {
      configurable: true,
      get: () => 0,
      set: setter,
    });
    return setter;
  }

  it('does not force-scroll while the pointer is hovering the stream, even when new messages arrive', async () => {
    const messages = [msg('m1', '2026-07-17T14:00:00Z', 'first')];
    const { container, rerender } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });
    const streamEl = container.querySelector('.stream') as HTMLElement;

    await fireEvent.mouseEnter(streamEl);
    const scrollTopSetter = spyOnScrollTop(streamEl);

    // A new note lands (mirrors App.svelte's SSE-driven appendNewestChatMessages).
    await rerender({
      messages: [...messages, msg('m2', '2026-07-17T14:01:00Z', 'second')],
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });
    await new Promise((r) => setTimeout(r, 20));

    expect(scrollTopSetter).not.toHaveBeenCalled();
  });

  it('catches up to the bottom the instant the pointer leaves the stream', async () => {
    const messages = [msg('m1', '2026-07-17T14:00:00Z', 'first')];
    const { container, rerender } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });
    const streamEl = container.querySelector('.stream') as HTMLElement;

    await fireEvent.mouseEnter(streamEl);
    await rerender({
      messages: [...messages, msg('m2', '2026-07-17T14:01:00Z', 'second')],
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });
    await new Promise((r) => setTimeout(r, 20));

    const scrollTopSetter = spyOnScrollTop(streamEl);
    await fireEvent.mouseLeave(streamEl);

    expect(scrollTopSetter).toHaveBeenCalled();
  });

  it('still auto-scrolls on new messages when the pointer was never over the stream', async () => {
    const messages = [msg('m1', '2026-07-17T14:00:00Z', 'first')];
    const { container, rerender } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });
    const streamEl = container.querySelector('.stream') as HTMLElement;
    const scrollTopSetter = spyOnScrollTop(streamEl);

    await rerender({
      messages: [...messages, msg('m2', '2026-07-17T14:01:00Z', 'second')],
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
    });
    await new Promise((r) => setTimeout(r, 20));

    expect(scrollTopSetter).toHaveBeenCalled();
  });
});

describe('ChatStream — keyboard-architecture pass: j/k select the next/previous message', () => {
  const messages = [
    msg('m1', '2026-07-17T14:00:00Z', 'first'),
    msg('m2', '2026-07-17T14:01:00Z', 'second'),
    msg('m3', '2026-07-17T14:02:00Z', 'third'),
  ];

  it('j selects the first message when nothing is selected yet, then moves forward; k moves back', async () => {
    const onSelectMessage = vi.fn();
    const { container, rerender } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
      onSelectMessage,
    });
    const streamEl = container.querySelector('.stream') as HTMLElement;

    await fireEvent.keyDown(streamEl, { key: 'j' });
    expect(onSelectMessage).toHaveBeenLastCalledWith('m1');

    // The real app re-renders with the new selectedMessageId — mirror that.
    await rerender({
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
      onSelectMessage,
      selectedMessageId: 'm1',
    });

    await fireEvent.keyDown(streamEl, { key: 'j' });
    expect(onSelectMessage).toHaveBeenLastCalledWith('m2');

    await rerender({
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
      onSelectMessage,
      selectedMessageId: 'm2',
    });

    await fireEvent.keyDown(streamEl, { key: 'k' });
    expect(onSelectMessage).toHaveBeenLastCalledWith('m1');
  });

  it('does not overrun the ends of the list', async () => {
    const onSelectMessage = vi.fn();
    const { container } = render(ChatStream, {
      messages,
      requests,
      agents: [reader],
      topic: 'general',
      hub: 'agent-coord',
      notesOn: true,
      reqsOn: true,
      selectedMessageId: 'm3',
      onSelectMessage,
    });
    const streamEl = container.querySelector('.stream') as HTMLElement;

    await fireEvent.keyDown(streamEl, { key: 'j' });
    expect(onSelectMessage).toHaveBeenLastCalledWith('m3');
  });
});
