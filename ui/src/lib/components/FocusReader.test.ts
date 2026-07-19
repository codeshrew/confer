import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import FocusReader from './FocusReader.svelte';
import type { Agent, Message, ThreadNode } from '../types';

vi.mock('../api', () => ({
  api: { getRefs: vi.fn().mockResolvedValue([]) },
}));

function agent(id: string, display: string): Agent {
  return {
    id,
    display,
    desc: null,
    expectedHost: null,
    lastTs: null,
    lastHost: null,
    live: true,
    verified: 'signed',
    color: 'var(--ag-reader)',
    abbr: display.slice(0, 2).toUpperCase(),
    wip: [],
  };
}

function message(overrides: Partial<Message> & { id: string; from: string }): Message {
  return {
    type: 'note',
    ts: '2026-07-17T14:00:00Z',
    host: 'lab-01',
    to: [],
    cc: [],
    topic: 'general',
    summary: 'summary line',
    body: 'body text',
    of: null,
    replyTo: null,
    supersedes: null,
    refs: [],
    ...overrides,
  };
}

function node(msgId: string, from: string): ThreadNode {
  return { msgId, from, type: 'note', topic: 'general', summary: 'x', refs: [] };
}

const reader = agent('reader', 'Reader');
const pipeline = agent('pipeline', 'Pipeline');

describe('FocusReader', () => {
  it('renders nothing when closed', () => {
    const messages = [message({ id: 'm1', from: 'reader' })];
    render(FocusReader, { open: false, msgId: 'm1', messages, agents: [reader], thread: [node('m1', 'reader')], hub: 'lab' });
    expect(screen.queryByTestId('focus-reader')).not.toBeInTheDocument();
  });

  it('renders nothing when msgId is null (nothing focused anywhere)', () => {
    render(FocusReader, { open: true, msgId: null, messages: [], agents: [], thread: [], hub: 'lab' });
    expect(screen.queryByTestId('focus-reader')).not.toBeInTheDocument();
  });

  it('shows the message body prose-typeset (rendered markdown), author, and topic', async () => {
    const messages = [message({ id: 'm1', from: 'reader', body: '**Bold** prose.', topic: 'design-review' })];
    render(FocusReader, { open: true, msgId: 'm1', messages, agents: [reader], thread: [node('m1', 'reader')], hub: 'lab' });

    expect(await screen.findByText('Reader')).toBeInTheDocument();
    expect(screen.getByText('#design-review')).toBeInTheDocument();
    const reading = document.querySelector('.fr-reading');
    expect(reading?.querySelector('strong')?.textContent).toBe('Bold');
  });

  it('j/k walk prev/next in the REAL thread order and fire onNavigate', async () => {
    const messages = [
      message({ id: 'root', from: 'pipeline' }),
      message({ id: 'reply', from: 'reader', replyTo: 'root' }),
    ];
    const thread = [node('root', 'pipeline'), node('reply', 'reader')];
    const onNavigate = vi.fn();
    const user = userEvent.setup();
    render(FocusReader, { open: true, msgId: 'root', messages, agents: [reader, pipeline], thread, hub: 'lab', onNavigate });

    await user.keyboard('j');
    expect(onNavigate).toHaveBeenCalledWith('reply');
  });

  it('k on the first message in the thread is a no-op (nothing before it)', async () => {
    const messages = [message({ id: 'root', from: 'pipeline' })];
    const thread = [node('root', 'pipeline')];
    const onNavigate = vi.fn();
    const user = userEvent.setup();
    render(FocusReader, { open: true, msgId: 'root', messages, agents: [pipeline], thread, hub: 'lab', onNavigate });

    await user.keyboard('k');
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it('the prev/next nav buttons fire onNavigate directly (mouse path)', async () => {
    const messages = [
      message({ id: 'root', from: 'pipeline' }),
      message({ id: 'reply', from: 'reader', replyTo: 'root' }),
    ];
    const thread = [node('root', 'pipeline'), node('reply', 'reader')];
    const onNavigate = vi.fn();
    const user = userEvent.setup();
    render(FocusReader, { open: true, msgId: 'root', messages, agents: [reader, pipeline], thread, hub: 'lab', onNavigate });

    await user.click(screen.getByTestId('reader-next'));
    expect(onNavigate).toHaveBeenCalledWith('reply');
  });

  it('Escape fires onClose', async () => {
    const messages = [message({ id: 'm1', from: 'reader' })];
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(FocusReader, { open: true, msgId: 'm1', messages, agents: [reader], thread: [node('m1', 'reader')], hub: 'lab', onClose });

    await user.keyboard('{Escape}');
    expect(onClose).toHaveBeenCalled();
  });

  it('clicking the backdrop closes; clicking inside the panel does not', async () => {
    const messages = [message({ id: 'm1', from: 'reader' })];
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(FocusReader, { open: true, msgId: 'm1', messages, agents: [reader], thread: [node('m1', 'reader')], hub: 'lab', onClose });

    await user.click(screen.getByTestId('focus-reader'));
    expect(onClose).not.toHaveBeenCalled();

    await user.click(screen.getByTestId('reader-backdrop'));
    expect(onClose).toHaveBeenCalled();
  });

  it('does not fire j/k navigation while the window keydown target is a typing field', async () => {
    // Regression guard for the shared isTypingTarget gate — simulates an
    // input elsewhere on the page (e.g. a chat compose box) still having
    // focus while the reader is open.
    const input = document.createElement('input');
    document.body.appendChild(input);
    input.focus();

    const messages = [
      message({ id: 'root', from: 'pipeline' }),
      message({ id: 'reply', from: 'reader', replyTo: 'root' }),
    ];
    const thread = [node('root', 'pipeline'), node('reply', 'reader')];
    const onNavigate = vi.fn();
    const user = userEvent.setup();
    render(FocusReader, { open: true, msgId: 'root', messages, agents: [reader, pipeline], thread, hub: 'lab', onNavigate });

    await user.keyboard('j');
    expect(onNavigate).not.toHaveBeenCalled();
    input.remove();
  });

  it('renders real refs in the gutter, and clicking one fetches real reverse-index hits via api.getRefs', async () => {
    const { api } = await import('../api');
    vi.mocked(api.getRefs).mockResolvedValue([
      {
        repo: 'confer',
        path: 'src/api.rs',
        sha: 'abc123',
        range: null,
        contentHash: null,
        refName: null,
        refType: null,
        commitDate: null,
        dirty: false,
        untracked: false,
        baseRef: null,
        forkPoint: null,
        staleness: 'current',
        msgId: 'm1',
        from: 'reader',
        msgType: 'note',
        ts: '2026-07-17T14:00:00Z',
        topic: 'general',
        summary: 'x',
        threadRoot: 'm1',
        requestStatus: null,
        hub: 'lab',
        hubPrivate: false,
      },
    ]);
    const messages = [
      message({ id: 'm1', from: 'reader', refs: [{ repo: 'confer', path: 'src/api.rs', sha: 'abc123', range: null, contentHash: null, refName: null, refType: null, commitDate: null, dirty: false, untracked: false, baseRef: null, forkPoint: null }] }),
    ];
    const onOpenRefs = vi.fn();
    const user = userEvent.setup();
    render(FocusReader, { open: true, msgId: 'm1', messages, agents: [reader], thread: [node('m1', 'reader')], hub: 'lab', onOpenRefs });

    await user.click(await screen.findByText('src/api.rs'));

    await vi.waitFor(() => expect(onOpenRefs).toHaveBeenCalled());
    const [ref, hits] = onOpenRefs.mock.calls[0]!;
    expect(ref.path).toBe('src/api.rs');
    expect(hits).toHaveLength(1); // real hits, never a fabricated empty array
  });

  describe('keyboard-architecture pass: "y" copies the full message id', () => {
    afterEach(() => {
      delete (navigator as { clipboard?: unknown }).clipboard;
    });

    it('copies the FULL id (not the shortened display id) and shows a toast', async () => {
      // userEvent.setup() installs its own navigator.clipboard stub, so the
      // mock must be defined AFTER setup() or it gets clobbered — same
      // ordering Message.test.ts's copy-id test uses.
      const user = userEvent.setup();
      const writeText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });

      const messages = [message({ id: 'msg_01JQ00000000000000000001', from: 'reader' })];
      render(FocusReader, {
        open: true,
        msgId: 'msg_01JQ00000000000000000001',
        messages,
        agents: [reader],
        thread: [node('msg_01JQ00000000000000000001', 'reader')],
        hub: 'lab',
      });

      expect(screen.queryByTestId('copied-toast')).not.toBeInTheDocument();
      await user.keyboard('y');

      await vi.waitFor(() => {
        expect(writeText).toHaveBeenCalledWith('msg_01JQ00000000000000000001');
      });
      expect(await screen.findByTestId('copied-toast')).toHaveTextContent(/copied/);
    });

    it('does not fire while a typing field has focus', async () => {
      const input = document.createElement('input');
      document.body.appendChild(input);
      input.focus();

      const user = userEvent.setup();
      const writeText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });

      const messages = [message({ id: 'm1', from: 'reader' })];
      render(FocusReader, { open: true, msgId: 'm1', messages, agents: [reader], thread: [node('m1', 'reader')], hub: 'lab' });

      await user.keyboard('y');
      expect(writeText).not.toHaveBeenCalled();
      input.remove();
    });
  });
});
