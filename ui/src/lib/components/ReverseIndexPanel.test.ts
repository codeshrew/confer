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
    refName: 'main',
    refType: 'branch',
    commitDate: '2026-07-17T14:40:00Z',
    dirty: false,
    untracked: false,
    baseRef: null,
    forkPoint: null,
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
    refName: 'main',
    refType: 'branch',
    commitDate: '2026-07-17T14:40:00Z',
    dirty: false,
    untracked: false,
    baseRef: null,
    forkPoint: null,
  },
];

const otherFileHit: RefHit = {
  repo: 'wealdlore',
  path: 'pipeline/plates.py',
  sha: 'b7e2a4',
  range: [88, 102],
  contentHash: null,
  staleness: 'current',
  msgId: 'msg_01JQf01',
  from: 'pipeline',
  msgType: 'note',
  ts: '2026-07-17T14:52:00Z',
  topic: 'studio',
  summary: 'restore chain context',
  threadRoot: 'msg_01JQf01',
  requestStatus: null,
  hub: 'agent-coord',
  hubPrivate: false,
  refName: null,
  refType: null,
  commitDate: null,
  dirty: false,
  untracked: false,
  baseRef: null,
  forkPoint: null,
};

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

  it('shows the "whole file" chip only when narrowed to a line range, and fires onWholeFile', async () => {
    const user = userEvent.setup();
    const onWholeFile = vi.fn();
    const { rerender } = render(ReverseIndexPanel, { hits, range: [44, 49], onWholeFile });

    const chip = screen.getByRole('button', { name: /whole file/ });
    expect(chip).toBeInTheDocument();
    await user.click(chip);
    expect(onWholeFile).toHaveBeenCalledOnce();

    await rerender({ hits, range: null, onWholeFile });
    expect(screen.queryByRole('button', { name: /whole file/ })).not.toBeInTheDocument();
  });

  it('the header copy invites a repo, a file, or a line range — not just a line range', () => {
    render(ReverseIndexPanel, { hits });
    expect(screen.getByText(/given a repo, a file, or a line range/)).toBeInTheDocument();
  });

  it('repo-mode (repo set, path null) groups hits by file instead of listing every message', () => {
    render(ReverseIndexPanel, { hits: [...hits, otherFileHit], repo: 'wealdlore', path: null });

    expect(screen.getByTestId('crumb-repo-mode')).toBeInTheDocument();
    // Two distinct files -> two group rows, not three per-message rows.
    expect(screen.getByText('Sources/Reader/PlateBundle.swift')).toBeInTheDocument();
    expect(screen.getByText('pipeline/plates.py')).toBeInTheDocument();
    expect(screen.getByText(/2 conversations/)).toBeInTheDocument();
    expect(screen.getByText(/1 conversation\b/)).toBeInTheDocument();
    // The individual message summaries are NOT rendered in repo-mode.
    expect(screen.queryByText('plate-bundle endpoint — the request these lines shipped for')).not.toBeInTheDocument();
  });

  it('repo-mode: clicking a file-group row fires onSelectFile with that path', async () => {
    const user = userEvent.setup();
    const onSelectFile = vi.fn();
    render(ReverseIndexPanel, { hits: [...hits, otherFileHit], repo: 'wealdlore', path: null, onSelectFile });

    await user.click(screen.getByText('pipeline/plates.py'));

    expect(onSelectFile).toHaveBeenCalledWith('pipeline/plates.py');
  });

  it('repo-mode shows its own empty state when the repo has no hits', () => {
    render(ReverseIndexPanel, { hits: [], repo: 'wealdlore', path: null });
    expect(screen.getByText('No conversations yet')).toBeInTheDocument();
  });

  it('bidirectional breadcrumb: the repo segment widens (onWidenToRepo) and the file segment narrows back (onWholeFile)', async () => {
    const user = userEvent.setup();
    const onWidenToRepo = vi.fn();
    const onWholeFile = vi.fn();
    render(ReverseIndexPanel, {
      hits,
      repo: 'wealdlore',
      path: 'Sources/Reader/PlateBundle.swift',
      range: [44, 49],
      onWidenToRepo,
      onWholeFile,
    });

    expect(screen.getByTestId('crumb-hits-mode')).toBeInTheDocument();
    await user.click(screen.getByTestId('crumb-repo-seg'));
    expect(onWidenToRepo).toHaveBeenCalledOnce();

    await user.click(screen.getByTestId('crumb-file-seg'));
    expect(onWholeFile).toHaveBeenCalledOnce();
  });

  it('file scope with no range shows the file as a non-clickable breadcrumb leaf (nothing to widen down to)', () => {
    render(ReverseIndexPanel, { hits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: null });

    expect(screen.getByTestId('crumb-repo-seg')).toBeInTheDocument();
    expect(screen.queryByTestId('crumb-file-seg')).not.toBeInTheDocument();
    expect(screen.getByText('PlateBundle.swift')).toBeInTheDocument();
  });
});
