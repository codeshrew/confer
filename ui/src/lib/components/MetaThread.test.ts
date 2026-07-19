import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import MetaThread from './MetaThread.svelte';
import type { Agent, Message, ThreadNode } from '../types';

function agent(id: string, display: string, color: string): Agent {
  return {
    id,
    display,
    desc: null,
    expectedHost: null,
    lastTs: null,
    lastHost: null,
    live: true,
    verified: 'signed',
    color,
    abbr: display.slice(0, 2).toUpperCase(),
    wip: [],
  };
}

function node(msgId: string, from: string, topic: string, summary = 'x'): ThreadNode {
  return { msgId, from, type: 'note', topic, summary, refs: [] };
}

function message(overrides: Partial<Message> & { id: string; from: string }): Message {
  return {
    type: 'note',
    ts: '2026-07-17T14:00:00Z',
    host: null,
    to: [],
    cc: [],
    topic: 'reader',
    summary: 'x',
    body: 'x',
    of: null,
    replyTo: null,
    supersedes: null,
    refs: [],
    ...overrides,
  };
}

const reader = agent('reader', 'Reader', 'var(--ag-reader)');
const pipeline = agent('pipeline', 'Pipeline', 'var(--ag-pipeline)');

describe('MetaThread', () => {
  afterEach(() => {
    delete (navigator as { clipboard?: unknown }).clipboard;
  });

  it('a single-topic thread has no cross-topic hop and no "weaves across" note', () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
    render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

    expect(screen.queryByText(/weaves across/)).not.toBeInTheDocument();
    expect(screen.queryByText(/thread crosses into/)).not.toBeInTheDocument();
    expect(screen.queryByText(/resolves back in/)).not.toBeInTheDocument();
  });

  it('flags a cross-topic hop when the thread leaves the root topic, and a "resolves back" hop on return', () => {
    const thread = [
      node('m1', 'pipeline', 'reader'), // root topic: reader
      node('m2', 'reader', 'studio-markup'), // crosses into studio-markup
      node('m3', 'pipeline', 'reader'), // resolves back into reader
    ];
    render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

    expect(screen.getByText(/This thread weaves across 2 topics\./)).toBeInTheDocument();
    expect(screen.getByText(/↗ thread crosses into #studio-markup/)).toBeInTheDocument();
    expect(screen.getByText(/↩ resolves back in #reader/)).toBeInTheDocument();
  });

  it('reports message/topic/agent counts', () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'studio'), node('m3', 'reader', 'reader')];
    render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

    expect(screen.getByText('3')).toBeInTheDocument(); // messages
    const twos = screen.getAllByText('2');
    expect(twos.length).toBeGreaterThanOrEqual(2); // topics + agents
  });

  it('renders the node\'s agent display name when known, and falls back to the raw `from` id for an unknown agent', () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'ghost-agent', 'reader')];
    render(MetaThread, { thread, agents: [reader], focusedMsgId: 'm1' });

    // "Reader" appears both as the row's author AND (since m1 is focused) as
    // the Focused card's author — assert presence, not uniqueness.
    expect(screen.getAllByText('Reader').length).toBeGreaterThan(0);
    expect(screen.getByText('ghost-agent')).toBeInTheDocument();
  });

  describe('Focused card + breadcrumb (peek redesign, piece 3)', () => {
    it('opens focused on the focusedMsgId prop, shown in both the breadcrumb and the Focused card', () => {
      const thread = [node('root', 'pipeline', 'reader', 'root summary'), node('reply', 'reader', 'reader', 'reply summary')];
      const messages = [message({ id: 'root', from: 'pipeline' }), message({ id: 'reply', from: 'reader', replyTo: 'root' })];
      render(MetaThread, { thread, agents: [reader, pipeline], messages, focusedMsgId: 'reply' });

      const crumbs = screen.getByTestId('peek-crumbs');
      expect(crumbs).toHaveTextContent('#reader');
      const focused = screen.getByTestId('peek-focused');
      expect(within(focused).getByText('Reader')).toBeInTheDocument();
    });

    it('the breadcrumb shows the REAL root->focused path, however many replyTo hops deep', () => {
      const thread = [node('root', 'pipeline', 'reader'), node('mid', 'reader', 'reader'), node('leaf', 'pipeline', 'reader')];
      const messages = [
        message({ id: 'root', from: 'pipeline' }),
        message({ id: 'mid', from: 'reader', replyTo: 'root' }),
        message({ id: 'leaf', from: 'pipeline', replyTo: 'mid' }),
      ];
      render(MetaThread, { thread, agents: [reader, pipeline], messages, focusedMsgId: 'leaf' });

      const crumbs = screen.getByTestId('peek-crumbs');
      // 3-hop path: root, mid, leaf — all present, in that order.
      const buttons = within(crumbs).getAllByRole('button');
      expect(buttons.map((b) => b.textContent?.trim())).toEqual(['root', 'mid', 'leaf']);
    });

    it('a click on a trail row moves focus WITHOUT firing onJump — peeking != navigating', async () => {
      const user = userEvent.setup();
      const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
      const onJump = vi.fn();
      render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1', onJump });

      await user.click(screen.getByText('Pipeline'));

      expect(onJump).not.toHaveBeenCalled();
      const focused = screen.getByTestId('peek-focused');
      expect(within(focused).getByText('Pipeline')).toBeInTheDocument();
    });

    it('the focused row shows a "here" tag and highlight, moved on click', async () => {
      const user = userEvent.setup();
      const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
      render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

      const rows = screen.getAllByTestId('peek-node');
      expect(within(rows[0]!).getByText('◂ here')).toBeInTheDocument();

      await user.click(screen.getByText('Pipeline'));
      expect(within(rows[1]!).getByText('◂ here')).toBeInTheDocument();
    });

    it('the Focused card\'s "open here" button fires onJump with the focused node\'s id + topic', async () => {
      const user = userEvent.setup();
      const thread = [node('m1', 'reader', 'general')];
      const onJump = vi.fn();
      render(MetaThread, { thread, agents: [reader], focusedMsgId: 'm1', onJump });

      await user.click(screen.getByTestId('peek-jump'));
      expect(onJump).toHaveBeenCalledWith('m1', 'general');
    });

    it('resets local focus to the new focusedMsgId when a genuinely new peek session opens (prop change)', async () => {
      const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
      const { rerender } = render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

      let focused = screen.getByTestId('peek-focused');
      expect(within(focused).getByText('Reader')).toBeInTheDocument();

      await rerender({ thread, agents: [reader, pipeline], focusedMsgId: 'm2' });
      focused = screen.getByTestId('peek-focused');
      expect(within(focused).getByText('Pipeline')).toBeInTheDocument();
    });
  });

  describe('keyboard: j/k/h/l/Enter/Esc', () => {
    function replyChain() {
      const thread = [node('root', 'pipeline', 'reader'), node('reply', 'reader', 'reader'), node('leaf', 'pipeline', 'reader')];
      const messages = [
        message({ id: 'root', from: 'pipeline' }),
        message({ id: 'reply', from: 'reader', replyTo: 'root' }),
        message({ id: 'leaf', from: 'pipeline', replyTo: 'reply' }),
      ];
      return { thread, messages };
    }

    it('j/k move focus along the flat trail order', async () => {
      const { thread, messages } = replyChain();
      const user = userEvent.setup();
      const { container } = render(MetaThread, { thread, agents: [reader, pipeline], messages, focusedMsgId: 'root' });

      (container.querySelector('.mt') as HTMLElement).focus();
      await user.keyboard('j');
      expect(within(screen.getByTestId('peek-focused')).getByText('Reader')).toBeInTheDocument();

      await user.keyboard('j');
      expect(within(screen.getByTestId('peek-focused')).getByText('Pipeline')).toBeInTheDocument();

      await user.keyboard('k');
      expect(within(screen.getByTestId('peek-focused')).getByText('Reader')).toBeInTheDocument();
    });

    it('l moves to the first child (real replyTo edge), h moves back to the parent', async () => {
      const { thread, messages } = replyChain();
      const user = userEvent.setup();
      const { container } = render(MetaThread, { thread, agents: [reader, pipeline], messages, focusedMsgId: 'root' });

      (container.querySelector('.mt') as HTMLElement).focus();
      await user.keyboard('l'); // root -> reply (its child)
      expect(within(screen.getByTestId('peek-focused')).getByText('Reader')).toBeInTheDocument();

      await user.keyboard('h'); // back to root
      expect(within(screen.getByTestId('peek-focused')).getByText('Pipeline')).toBeInTheDocument();
    });

    it('l is a no-op on a leaf node (no children) — never throws, never moves', async () => {
      const { thread, messages } = replyChain();
      const user = userEvent.setup();
      const { container } = render(MetaThread, { thread, agents: [reader, pipeline], messages, focusedMsgId: 'leaf' });

      (container.querySelector('.mt') as HTMLElement).focus();
      await user.keyboard('l');
      expect(within(screen.getByTestId('peek-focused')).getByText('Pipeline')).toBeInTheDocument();
    });

    it('Enter fires onJump with the focused node', async () => {
      const { thread, messages } = replyChain();
      const user = userEvent.setup();
      const onJump = vi.fn();
      const { container } = render(MetaThread, { thread, agents: [reader, pipeline], messages, focusedMsgId: 'root', onJump });

      (container.querySelector('.mt') as HTMLElement).focus();
      await user.keyboard('{Enter}');
      expect(onJump).toHaveBeenCalledWith('root', 'reader');
    });

    it('Escape fires onClose', async () => {
      const { thread, messages } = replyChain();
      const user = userEvent.setup();
      const onClose = vi.fn();
      const { container } = render(MetaThread, { thread, agents: [reader, pipeline], messages, focusedMsgId: 'root', onClose });

      (container.querySelector('.mt') as HTMLElement).focus();
      await user.keyboard('{Escape}');
      expect(onClose).toHaveBeenCalled();
    });
  });

  describe('copy-id on the .gid line (design/41 Phase 0)', () => {
    it('clicking the id copies it and swaps the icon to a check, without moving focus', async () => {
      // userEvent.setup() installs its own navigator.clipboard stub, so our
      // mock must be defined AFTER setup() or it gets clobbered.
      const user = userEvent.setup();
      Object.defineProperty(navigator, 'clipboard', {
        value: { writeText: vi.fn().mockResolvedValue(undefined) },
        configurable: true,
      });
      const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
      render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

      const gidBtn = screen.getByRole('button', { name: /copy id m2/i });
      await user.click(gidBtn);

      await vi.waitFor(() => {
        expect(gidBtn).toHaveAttribute('aria-label', 'Copied m2');
      });
      // Copying m2's id must not have refocused the panel onto m2.
      expect(within(screen.getByTestId('peek-focused')).getByText('Reader')).toBeInTheDocument();
    });
  });

  describe('keyboard-architecture pass: the Focused card also has a copy-id control, and "y" copies it', () => {
    afterEach(() => {
      delete (navigator as { clipboard?: unknown }).clipboard;
    });

    it('the Focused card shows its own copy-id button (not just the trail rows)', async () => {
      const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
      render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

      const focused = within(screen.getByTestId('peek-focused'));
      expect(focused.getByRole('button', { name: /copy id m1/i })).toBeInTheDocument();
    });

    it('"y" copies the FULL id of the currently-focused node and shows a toast', async () => {
      const user = userEvent.setup();
      const writeText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });

      const thread = [node('msg_01JQ00000000000000000001', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
      render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'msg_01JQ00000000000000000001' });

      screen.getByTestId('thread-peek').focus();
      expect(screen.queryByTestId('copied-toast')).not.toBeInTheDocument();
      await user.keyboard('y');

      await vi.waitFor(() => {
        expect(writeText).toHaveBeenCalledWith('msg_01JQ00000000000000000001');
      });
      expect(await screen.findByTestId('copied-toast')).toHaveTextContent(/copied/);
    });
  });

  describe('git-log-style row (time + hop chip)', () => {
    it('shows a per-node time when the node\'s message is loaded, formatted via formatClock', () => {
      const thread = [node('m1', 'reader', 'reader', 'Short summary')];
      const messages = [message({ id: 'm1', from: 'reader' })];
      const { container } = render(MetaThread, { thread, agents: [reader], messages, focusedMsgId: 'm1' });

      expect(container.querySelector('.gts')).toBeInTheDocument();
    });

    it('omits the per-node time when the node\'s message is not loaded', () => {
      const thread = [node('m1', 'reader', 'reader', 'Short summary')];
      const { container } = render(MetaThread, { thread, agents: [reader], messages: [], focusedMsgId: 'm1' });

      expect(container.querySelector('.gts')).not.toBeInTheDocument();
    });

    it('only shows the inline #topic chip on a hop (cross-topic) row, not on every row', () => {
      const thread = [
        node('m1', 'pipeline', 'reader'),
        node('m2', 'reader', 'studio-markup'), // crosses in — should show #studio-markup
      ];
      const { container } = render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

      expect(container.querySelectorAll('.gtp')).toHaveLength(1);
    });
  });

  it('renders an empty thread without throwing (no nodes, no legend, zero counts, no Focused card)', () => {
    const { container } = render(MetaThread, { thread: [], agents: [], focusedMsgId: '' });

    expect(container.querySelector('.gn')).not.toBeInTheDocument();
    expect(screen.queryByTestId('peek-focused')).not.toBeInTheDocument();
    const zeros = screen.getAllByText('0');
    expect(zeros.length).toBe(3); // topics, messages, agents all zero
    expect(zeros[0]!.closest('.stat')).toBeInTheDocument();
  });
});
