import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import ReverseIndexPanel from './ReverseIndexPanel.svelte';
import type { RefHit } from '../types';

const hits: RefHit[] = [
  {
    repo: 'wealdlore',
    path: 'Sources/Reader/PlateBundle.swift',
    sha: 'a3f1c9',
    range: [44, 49],
    contentHash: null,
    staleness: 'current',
    msgId: 'msg_01JQe73',
    from: 'reader',
    msgType: 'note',
    ts: '2026-07-17T14:46:00Z',
    topic: 'reader',
    summary: 'plate-bundle endpoint — the request these lines shipped for',
    threadRoot: 'msg_01JQ8f2',
    requestStatus: 'DONE',
    hub: 'agent-coord',
    hubPrivate: false,
  },
  {
    repo: 'wealdlore',
    path: 'Sources/Reader/PlateBundle.swift',
    sha: 'a3f1c9',
    range: [44, 49],
    contentHash: null,
    staleness: 'current',
    msgId: 'msg_01JQh12',
    from: 'compositor',
    msgType: 'note',
    ts: '2026-07-10T09:00:00Z',
    topic: 'design',
    summary: 'why not stream the regions?',
    threadRoot: 'msg_01JQh12',
    requestStatus: null,
    hub: 'wealdlore-internal',
    hubPrivate: true,
  },
];

describe('ReverseIndexPanel', () => {
  it('lists each hit — hub, topic, from/type, summary', () => {
    render(ReverseIndexPanel, { hits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49] });

    expect(screen.getByText('plate-bundle endpoint — the request these lines shipped for')).toBeInTheDocument();
    expect(screen.getByText('why not stream the regions?')).toBeInTheDocument();
    expect(screen.getByText('#reader')).toBeInTheDocument();
    expect(screen.getByText('#design')).toBeInTheDocument();
    expect(screen.getByText(/2 refs · 2 hubs/)).toBeInTheDocument();
  });

  it('badges a private hub distinctly from a public one', () => {
    const { container } = render(ReverseIndexPanel, { hits });

    const badges = [...container.querySelectorAll('.cvhub')];
    expect(badges.some((b) => b.textContent === 'agent-coord' && !b.classList.contains('priv'))).toBe(true);
    expect(badges.some((b) => b.textContent === 'wealdlore-internal · priv' && b.classList.contains('priv'))).toBe(true);
  });

  it('renders an empty state when there are no hits', () => {
    render(ReverseIndexPanel, { hits: [] });

    expect(screen.getByText('No conversations yet')).toBeInTheDocument();
  });

  it('fires onSelectHit when a conversation item is clicked', async () => {
    const user = userEvent.setup();
    const onSelectHit = vi.fn();
    render(ReverseIndexPanel, { hits, onSelectHit });

    await user.click(screen.getByText('plate-bundle endpoint — the request these lines shipped for'));

    expect(onSelectHit).toHaveBeenCalledWith(hits[0]);
  });
});
