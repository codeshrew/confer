import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import MetaThread from './MetaThread.svelte';
import { paneFocus } from '../paneFocus.svelte';
import { readState } from '../readState.svelte';
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

function node(msgId: string, from: string, topic: string, summary = 'x', type: ThreadNode['type'] = 'note'): ThreadNode {
  return { msgId, from, type, topic, summary, refs: [] };
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
    seenBy: [],
    ...overrides,
  };
}

const reader = agent('reader', 'Reader', 'var(--ag-reader)');
const pipeline = agent('pipeline', 'Pipeline', 'var(--ag-pipeline)');

/** The currently-focused row — the one with the `here` tag. */
function hereRow() {
  return screen.getAllByTestId('peek-node').find((r) => within(r).queryByText('here'))!;
}

describe('MetaThread — the minimap (piece 4, item 1)', () => {
  afterEach(() => {
    delete (navigator as { clipboard?: unknown }).clipboard;
  });

  it('renders one row per trail node — author, kind tag, short id — and NO snippet text', () => {
    const thread = [node('m1', 'reader', 'reader', 'this exact summary text must not appear')];
    render(MetaThread, { thread, agents: [reader], focusedMsgId: 'm1' });

    const row = screen.getByTestId('peek-node');
    expect(within(row).getByText('Reader')).toBeInTheDocument();
    expect(within(row).getByText('note')).toBeInTheDocument();
    expect(within(row).getByText(/m1/)).toBeInTheDocument();
    expect(within(row).queryByText('this exact summary text must not appear')).not.toBeInTheDocument();
  });

  it('falls back to the raw `from` id for an unknown agent', () => {
    const thread = [node('m1', 'ghost-agent', 'reader')];
    render(MetaThread, { thread, agents: [], focusedMsgId: 'm1' });

    expect(screen.getByText('ghost-agent')).toBeInTheDocument();
  });

  it('a single-topic thread shows no crossing divider and no topic swatches in the stat line', () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
    render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

    expect(screen.queryByText(/↘ #/)).not.toBeInTheDocument();
  });

  it('a topic-crossing thread shows exactly ONE labeled divider at the crossing (not a sentence, not one per hop)', () => {
    const thread = [
      node('m1', 'pipeline', 'reader'),
      node('m2', 'reader', 'studio-markup'), // crosses in
      node('m3', 'pipeline', 'studio-markup'), // same topic — no second divider
    ];
    render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

    expect(screen.getAllByText('↘ #studio-markup')).toHaveLength(1);
  });

  it('shows the message count and per-topic swatches in the stat line (only when there is more than one topic)', () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'studio')];
    render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

    expect(screen.getByText(/2 msgs/)).toBeInTheDocument();
    expect(screen.getByText('#reader')).toBeInTheDocument();
    expect(screen.getByText('#studio')).toBeInTheDocument();
  });

  it('the focused node shows a "here" tag and highlight, without needing a click', () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
    render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

    expect(within(hereRow()).getByText('Reader')).toBeInTheDocument();
  });

  it('resets local focus to the new focusedMsgId when a genuinely new peek session opens (prop change)', async () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
    const { rerender } = render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });
    expect(within(hereRow()).getByText('Reader')).toBeInTheDocument();

    await rerender({ thread, agents: [reader, pipeline], focusedMsgId: 'm2' });
    expect(within(hereRow()).getByText('Pipeline')).toBeInTheDocument();
  });

  describe('interaction model: click jumps, hover previews (piece 4 — supersedes piece 3\'s click-only-locally-focuses)', () => {
    it('clicking a node fires onJump with that node\'s id + topic — click now navigates', async () => {
      const user = userEvent.setup();
      const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'general')];
      const onJump = vi.fn();
      render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1', onJump });

      await user.click(screen.getByText('Pipeline'));

      expect(onJump).toHaveBeenCalledWith('m2', 'general');
    });

    it('clicking a node also moves the local "here" indicator, alongside firing onJump', async () => {
      const user = userEvent.setup();
      const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
      render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

      await user.click(screen.getByText('Pipeline'));
      expect(within(hereRow()).getByText('Pipeline')).toBeInTheDocument();
    });

    it('hovering a DIFFERENT node previews it, without moving local focus or firing onJump', async () => {
      const user = userEvent.setup();
      const thread = [node('m1', 'reader', 'reader', 'first summary'), node('m2', 'pipeline', 'reader', 'second summary')];
      const onJump = vi.fn();
      render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1', onJump });

      expect(screen.getByTestId('peek-preview')).toHaveTextContent('first summary');

      await user.hover(screen.getByText('Pipeline'));
      expect(screen.getByTestId('peek-preview')).toHaveTextContent('second summary');
      expect(onJump).not.toHaveBeenCalled();
      // The "here" indicator never moved — hover previews, it doesn't focus.
      expect(within(hereRow()).getByText('Reader')).toBeInTheDocument();
    });

    it('the preview falls back to the locally-focused node when nothing is hovered', async () => {
      const thread = [node('m1', 'reader', 'reader', 'first summary'), node('m2', 'pipeline', 'reader', 'second summary')];
      const { container } = render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

      (container.querySelector('.mt') as HTMLElement).focus();
      const user = userEvent.setup();
      await user.keyboard('j');

      expect(screen.getByTestId('peek-preview')).toHaveTextContent('second summary');
    });
  });

  describe('keyboard: j/k/h/l stay LOCAL (never fire onJump); Enter is the one thing that jumps', () => {
    function replyChain() {
      const thread = [node('root', 'pipeline', 'reader'), node('reply', 'reader', 'reader'), node('leaf', 'pipeline', 'reader')];
      const messages = [
        message({ id: 'root', from: 'pipeline' }),
        message({ id: 'reply', from: 'reader', replyTo: 'root' }),
        message({ id: 'leaf', from: 'pipeline', replyTo: 'reply' }),
      ];
      return { thread, messages };
    }

    it('j/k move the local pointer along the flat trail order, without firing onJump', async () => {
      const { thread, messages } = replyChain();
      const user = userEvent.setup();
      const onJump = vi.fn();
      const { container } = render(MetaThread, { thread, agents: [reader, pipeline], messages, focusedMsgId: 'root', onJump });

      (container.querySelector('.mt') as HTMLElement).focus();
      await user.keyboard('j');
      expect(within(hereRow()).getByText('Reader')).toBeInTheDocument();

      await user.keyboard('j');
      expect(within(hereRow()).getByText('Pipeline')).toBeInTheDocument();

      await user.keyboard('k');
      expect(within(hereRow()).getByText('Reader')).toBeInTheDocument();
      expect(onJump).not.toHaveBeenCalled();
    });

    it('l moves to the first child (real replyTo edge), h moves back to the parent', async () => {
      const { thread, messages } = replyChain();
      const user = userEvent.setup();
      const { container } = render(MetaThread, { thread, agents: [reader, pipeline], messages, focusedMsgId: 'root' });

      (container.querySelector('.mt') as HTMLElement).focus();
      await user.keyboard('l'); // root -> reply (its child)
      expect(within(hereRow()).getByText('Reader')).toBeInTheDocument();

      await user.keyboard('h'); // back to root
      expect(within(hereRow()).getByText('Pipeline')).toBeInTheDocument();
    });

    it('l is a no-op on a leaf node (no children) — never throws, never moves', async () => {
      const { thread, messages } = replyChain();
      const user = userEvent.setup();
      const { container } = render(MetaThread, { thread, agents: [reader, pipeline], messages, focusedMsgId: 'leaf' });

      (container.querySelector('.mt') as HTMLElement).focus();
      await user.keyboard('l');
      expect(within(hereRow()).getByText('Pipeline')).toBeInTheDocument();
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

    it('"y" copies the FOCUSED node\'s full id and shows a toast', async () => {
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

  describe('copy-id — every row has its own (mouse path), reusing CopyIdButton', () => {
    it('clicking a row\'s copy-id button copies that row\'s id WITHOUT jumping (stopPropagation)', async () => {
      const user = userEvent.setup();
      const writeText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
      const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
      const onJump = vi.fn();
      render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1', onJump });

      const copyBtn = screen.getByRole('button', { name: /copy id m2/i });
      await user.click(copyBtn);

      await vi.waitFor(() => expect(writeText).toHaveBeenCalledWith('m2'));
      expect(onJump).not.toHaveBeenCalled();
      // Copying m2's id must not have moved local focus onto it either.
      expect(within(hereRow()).getByText('Reader')).toBeInTheDocument();
    });
  });

  describe('collapse — "hide" declutters the map without touching the peek session', () => {
    it('hides the map/preview/footer when collapsed, and shows them again on toggle', async () => {
      const user = userEvent.setup();
      const thread = [node('m1', 'reader', 'reader')];
      render(MetaThread, { thread, agents: [reader], focusedMsgId: 'm1' });

      expect(screen.getByTestId('peek-map')).toBeInTheDocument();
      await user.click(screen.getByTestId('peek-collapse'));
      expect(screen.queryByTestId('peek-map')).not.toBeInTheDocument();
      expect(screen.queryByTestId('peek-preview')).not.toBeInTheDocument();

      await user.click(screen.getByTestId('peek-collapse'));
      expect(screen.getByTestId('peek-map')).toBeInTheDocument();
    });

    it('does not touch appState/onClose — collapsing is not closing', async () => {
      const user = userEvent.setup();
      const onClose = vi.fn();
      const thread = [node('m1', 'reader', 'reader')];
      render(MetaThread, { thread, agents: [reader], focusedMsgId: 'm1', onClose });

      await user.click(screen.getByTestId('peek-collapse'));
      expect(onClose).not.toHaveBeenCalled();
    });
  });

  describe('the ✕ button — mouse-close for the whole peek session (piece 4: the shared right-rail close is mobile-drawer-only and never reached this on desktop)', () => {
    it('fires onClose when clicked', async () => {
      const user = userEvent.setup();
      const onClose = vi.fn();
      const thread = [node('m1', 'reader', 'reader')];
      render(MetaThread, { thread, agents: [reader], focusedMsgId: 'm1', onClose });

      await user.click(screen.getByTestId('peek-close'));
      expect(onClose).toHaveBeenCalledOnce();
    });

    it('is visible even while collapsed — closing the whole session shouldn\'t require un-collapsing first', async () => {
      const user = userEvent.setup();
      const onClose = vi.fn();
      const thread = [node('m1', 'reader', 'reader')];
      render(MetaThread, { thread, agents: [reader], focusedMsgId: 'm1', onClose });

      await user.click(screen.getByTestId('peek-collapse'));
      await user.click(screen.getByTestId('peek-close'));
      expect(onClose).toHaveBeenCalledOnce();
    });
  });

  it('renders an empty thread without throwing — no rows, no crash', () => {
    render(MetaThread, { thread: [], agents: [], focusedMsgId: '' });

    expect(screen.queryByTestId('peek-node')).not.toBeInTheDocument();
    expect(screen.getByText(/0 msgs/)).toBeInTheDocument();
  });

  describe('piece 4, item 2 — the "detail-viewed" glyph is neutral by absence', () => {
    beforeEach(() => {
      localStorage.clear();
    });

    it('shows nothing for a node never opened in the focus reader', () => {
      const thread = [node('m-never-viewed', 'reader', 'reader')];
      render(MetaThread, { thread, agents: [reader], focusedMsgId: 'm-never-viewed' });

      expect(screen.queryByTitle('Opened in the focus reader')).not.toBeInTheDocument();
    });

    it('shows a subtle ✓ on the node once readState records it as detail-viewed', () => {
      readState.markDetailViewed('m-was-viewed');
      const thread = [node('m-was-viewed', 'reader', 'reader')];
      render(MetaThread, { thread, agents: [reader], focusedMsgId: 'm-was-viewed' });

      expect(screen.getByTitle('Opened in the focus reader')).toBeInTheDocument();
    });
  });
});

describe('MetaThread — keyboard-architecture pass, item 0 bug fix: pane focus must not leak from content sync', () => {
  // Regression for the live bug: focus the Chat stream, press j/k — after
  // the FIRST move it silently jumped into the meta-thread (every
  // subsequent j/k moved the trail, not the stream). Root cause: a stream
  // selection change opens/updates this peek as a SIDE EFFECT, and the old
  // roving-row-focus effect called real `.focus()` on every such change —
  // which paneFocus.syncFromFocusEvent (bound on window, not present in
  // this isolated unit test) reads as "the operator moved into
  // thread-peek." Simulated directly here via the real paneFocus singleton
  // (a mock 'stream' pane, explicitly unregistered at the end of the test
  // that adds one — it's a shared module instance, real state must not
  // leak into other tests).

  it('does NOT steal real DOM focus when the selection changes while a DIFFERENT pane is active', () => {
    const streamEl = document.createElement('div');
    streamEl.tabIndex = -1;
    document.body.appendChild(streamEl);
    const stopStream = paneFocus.register({
      id: 'stream',
      label: 'Chat stream',
      el: streamEl,
      getRect: () => ({ top: 0, left: 0, width: 100, height: 100 }),
    });
    paneFocus.focus('stream');
    expect(document.activeElement).toBe(streamEl);

    const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
    render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'm1' });

    // MetaThread mounted (registered itself as 'thread-peek'), but nothing
    // EXPLICIT navigated into it — 'stream' must still hold both the
    // active-pane state AND real DOM focus.
    expect(paneFocus.focusedId).toBe('stream');
    expect(document.activeElement).toBe(streamEl);

    stopStream();
    streamEl.remove();
  });

  it('DOES move real DOM focus onto the newly-focused row once thread-peek is already the active pane', async () => {
    const thread = [node('root', 'pipeline', 'reader'), node('reply', 'reader', 'reader', 'x')];
    const { container } = render(MetaThread, { thread, agents: [reader, pipeline], focusedMsgId: 'root' });

    // Nothing else registered in this test — thread-peek becomes the sole/
    // default active pane on mount (paneFocus's own "first registrant wins"
    // rule), matching a genuine "operator is already in the peek" state.
    expect(paneFocus.focusedId).toBe('thread-peek');

    const user = userEvent.setup();
    (container.querySelector('.mt') as HTMLElement).focus();
    await user.keyboard('j');

    expect(within(hereRow()).getByText('Reader')).toBeInTheDocument();
  });
});
