import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import LeftRail from './LeftRail.svelte';
import { appState } from '../stores.svelte';
import type { Agent, Topic } from '../types';

const topics: Topic[] = [
  { slug: 'general', messages: 3, open: 0, requests: 0, status: 'discussion', stale: false, lastTs: null },
  { slug: 'reader', messages: 7, open: 1, requests: 1, status: 'open', stale: false, lastTs: null },
  { slug: 'plate-pipeline', messages: 2, open: 1, requests: 1, status: 'open', stale: true, lastTs: null },
];

const agents: Agent[] = [
  {
    id: 'herald',
    display: 'Herald',
    desc: null,
    expectedHost: null,
    lastTs: '2026-07-17T14:57:00Z',
    lastHost: null,
    live: true,
    verified: 'signed',
    color: 'var(--ag-herald)',
    abbr: 'HE',
    wip: [],
  },
];

describe('LeftRail', () => {
  it('renders a topic entry for each topic, with its status chip', () => {
    render(LeftRail, { hubName: 'agent-coord', topics, currentTopic: 'reader', agents });

    expect(screen.getByText('general')).toBeInTheDocument();
    expect(screen.getByText('reader')).toBeInTheDocument();
    expect(screen.getByText('stale')).toBeInTheDocument();
    expect(screen.getByText('disc')).toBeInTheDocument();
  });

  it('renders the fleet mini-panel with agent color pip + abbreviation', () => {
    render(LeftRail, { hubName: 'agent-coord', topics, currentTopic: 'reader', agents });

    expect(screen.getByText('HE')).toBeInTheDocument();
    expect(screen.getByText('Herald')).toBeInTheDocument();
  });

  it('fires onTopicSelect (and updates appState.topic) when a topic is clicked', async () => {
    const user = userEvent.setup();
    const onTopicSelect = vi.fn((slug: string) => {
      appState.topic = slug;
    });
    render(LeftRail, { hubName: 'agent-coord', topics, currentTopic: 'reader', agents, onTopicSelect });

    await user.click(screen.getByText('plate-pipeline'));

    expect(onTopicSelect).toHaveBeenCalledWith('plate-pipeline');
    expect(appState.topic).toBe('plate-pipeline');
  });

  it('drops the fleet roster section when showFleet is false (Fleet view — the center already is the roster)', () => {
    render(LeftRail, { hubName: 'agent-coord', topics, currentTopic: 'reader', agents, showFleet: false });

    expect(screen.queryByText('Herald')).not.toBeInTheDocument();
    expect(screen.queryByText(/Fleet · you/)).not.toBeInTheDocument();
    // Topics stay — only the fleet section is dropped.
    expect(screen.getByText('reader')).toBeInTheDocument();
  });

  it('shows the fleet roster section by default', () => {
    render(LeftRail, { hubName: 'agent-coord', topics, currentTopic: 'reader', agents });

    expect(screen.getByText('Herald')).toBeInTheDocument();
  });
});
