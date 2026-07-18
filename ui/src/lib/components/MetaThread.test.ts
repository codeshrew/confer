import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
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

function message(id: string, from: string, topic: string, summary: string, body: string): Message {
  return {
    id,
    from,
    type: 'note',
    ts: '2026-07-17T14:00:00Z',
    host: null,
    to: [],
    cc: [],
    topic,
    summary,
    body,
    of: null,
    replyTo: null,
    supersedes: null,
    refs: [],
  };
}

const reader = agent('reader', 'Reader', 'var(--ag-reader)');
const pipeline = agent('pipeline', 'Pipeline', 'var(--ag-pipeline)');

describe('MetaThread', () => {
  it('a single-topic thread has no cross-topic hop and no "weaves across" note', () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
    render(MetaThread, { thread, agents: [reader, pipeline] });

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
    render(MetaThread, { thread, agents: [reader, pipeline] });

    expect(screen.getByText(/This thread weaves across 2 topics\./)).toBeInTheDocument();
    expect(screen.getByText(/↗ thread crosses into #studio-markup/)).toBeInTheDocument();
    expect(screen.getByText(/↩ resolves back in #reader/)).toBeInTheDocument();
  });

  it('does not flag a hop between two consecutive nodes that share a topic', () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'reader', 'reader'), node('m3', 'pipeline', 'reader')];
    render(MetaThread, { thread, agents: [reader, pipeline] });

    expect(screen.queryByText(/hop-in|hop-back/)).not.toBeInTheDocument();
    expect(screen.queryByText(/crosses into|resolves back/)).not.toBeInTheDocument();
  });

  it('reports message/topic/agent counts', () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'studio'), node('m3', 'reader', 'reader')];
    render(MetaThread, { thread, agents: [reader, pipeline] });

    expect(screen.getByText('3')).toBeInTheDocument(); // messages
    // 2 unique topics, 2 unique agents (reader, pipeline) both render as "2"
    const twos = screen.getAllByText('2');
    expect(twos.length).toBeGreaterThanOrEqual(2);
  });

  it('omits the span stat entirely when messages aren\'t supplied (no ts to compute from)', () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
    const { container } = render(MetaThread, { thread, agents: [reader, pipeline] });

    expect(container.querySelector('.mt-stats')?.textContent).not.toMatch(/span/);
  });

  it('computes a minutes span from the first/last node\'s message timestamps when messages are supplied', () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
    const messages: Message[] = [
      {
        id: 'm1',
        from: 'reader',
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
      },
      {
        id: 'm2',
        from: 'pipeline',
        type: 'note',
        ts: '2026-07-17T14:25:00Z',
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
      },
    ];
    render(MetaThread, { thread, agents: [reader, pipeline], messages });

    expect(screen.getByText('25m')).toBeInTheDocument();
  });

  it('shows hours once the span passes 60 minutes', () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
    const messages: Message[] = [
      {
        id: 'm1',
        from: 'reader',
        type: 'note',
        ts: '2026-07-17T10:00:00Z',
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
      },
      {
        id: 'm2',
        from: 'pipeline',
        type: 'note',
        ts: '2026-07-17T13:00:00Z',
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
      },
    ];
    render(MetaThread, { thread, agents: [reader, pipeline], messages });

    expect(screen.getByText('3h')).toBeInTheDocument();
  });

  it('renders the node\'s agent display name when known, and falls back to the raw `from` id for an unknown agent', () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'ghost-agent', 'reader')];
    render(MetaThread, { thread, agents: [reader] });

    expect(screen.getByText('Reader')).toBeInTheDocument();
    expect(screen.getByText('ghost-agent')).toBeInTheDocument();
  });

  it('clicking a node card fires onSelectNode with that node\'s msgId', async () => {
    const thread = [node('m1', 'reader', 'reader'), node('m2', 'pipeline', 'reader')];
    const onSelectNode = vi.fn();
    render(MetaThread, { thread, agents: [reader, pipeline], onSelectNode });

    const user = userEvent.setup();
    await user.click(screen.getByText('m2'));

    expect(onSelectNode).toHaveBeenCalledWith('m2');
  });

  describe('full-body rendering (density + expand)', () => {
    it('shows only the summary by default (summary density), with a chevron to expand', () => {
      const thread = [node('m1', 'reader', 'reader', 'Short summary')];
      const messages = [message('m1', 'reader', 'reader', 'Short summary', '**Full** rendered body text.')];
      const { container } = render(MetaThread, { thread, agents: [reader], messages });

      expect(screen.getByText('Short summary')).toBeInTheDocument();
      expect(container.querySelector('.gbody')).not.toBeInTheDocument();
      expect(container.querySelector('.node-expand-toggle')).toBeInTheDocument();
    });

    it('expands to the full sanitized/rendered body on chevron click, and collapses again on a second click', async () => {
      const user = userEvent.setup();
      const thread = [node('m1', 'reader', 'reader', 'Short summary')];
      const messages = [message('m1', 'reader', 'reader', 'Short summary', '**Full** rendered body text.')];
      const { container } = render(MetaThread, { thread, agents: [reader], messages });

      const chevron = container.querySelector('.node-expand-toggle') as HTMLButtonElement;
      await user.click(chevron);

      const body = container.querySelector('.gbody');
      expect(body).toBeInTheDocument();
      expect(body?.querySelector('strong')?.textContent).toBe('Full');

      await user.click(chevron);
      expect(container.querySelector('.gbody')).not.toBeInTheDocument();
    });

    it('clicking the chevron does not also fire onSelectNode (no double-action)', async () => {
      const user = userEvent.setup();
      const thread = [node('m1', 'reader', 'reader', 'Short summary')];
      const messages = [message('m1', 'reader', 'reader', 'Short summary', 'Full body.')];
      const onSelectNode = vi.fn();
      const { container } = render(MetaThread, { thread, agents: [reader], messages, onSelectNode });

      const chevron = container.querySelector('.node-expand-toggle') as HTMLButtonElement;
      await user.click(chevron);

      expect(onSelectNode).not.toHaveBeenCalled();
    });

    it('falls back to summary-only, no chevron, when the node\'s message is not in the loaded window', () => {
      // Simulates the pagination CONTRACT GAP: an older thread node whose
      // message page has scrolled out of App.svelte's windowed `messages`.
      const thread = [node('m1', 'reader', 'reader', 'Only a summary — body not loaded')];
      const { container } = render(MetaThread, { thread, agents: [reader], messages: [] });

      expect(screen.getByText('Only a summary — body not loaded')).toBeInTheDocument();
      expect(container.querySelector('.node-expand-toggle')).not.toBeInTheDocument();
      expect(container.querySelector('.gbody')).not.toBeInTheDocument();
    });

    it('in full density, the body is shown by default with no click needed', () => {
      const thread = [node('m1', 'reader', 'reader', 'Short summary')];
      const messages = [message('m1', 'reader', 'reader', 'Short summary', 'Body shown by default.')];
      const { container } = render(MetaThread, { thread, agents: [reader], messages, density: 'full' });

      expect(container.querySelector('.gbody')).toBeInTheDocument();
      expect(container.textContent).toContain('Body shown by default.');
    });
  });

  it('renders an empty thread without throwing (no nodes, no legend, zero counts)', () => {
    const { container } = render(MetaThread, { thread: [], agents: [] });

    expect(container.querySelector('.gn')).not.toBeInTheDocument();
    const zeros = screen.getAllByText('0');
    expect(zeros.length).toBe(3); // topics, messages, agents all zero
    expect(zeros[0]!.closest('.stat')).toBeInTheDocument();
  });
});
