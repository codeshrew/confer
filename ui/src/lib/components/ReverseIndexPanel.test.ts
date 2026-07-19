import { describe, expect, it, vi } from 'vitest';
import { render, screen, within } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import ReverseIndexPanel from './ReverseIndexPanel.svelte';
import type { Agent, RefHit } from '../types';

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

const thirdHit: RefHit = {
  repo: 'wealdlore',
  path: 'Sources/Reader/PlateBundle.swift',
  sha: 'a3f1c9',
  range: [44, 49],
  contentHash: null,
  staleness: 'current',
  msgId: 'msg_01JQi30',
  from: 'compositor',
  msgType: 'request',
  ts: '2026-07-12T10:00:00Z',
  topic: 'reader',
  summary: 'can the size guard be configurable?',
  threadRoot: 'msg_01JQi30',
  requestStatus: 'CLAIMED',
  hub: 'agent-coord',
  hubPrivate: false,
  refName: 'main',
  refType: 'branch',
  commitDate: '2026-07-17T14:40:00Z',
  dirty: false,
  untracked: false,
  baseRef: null,
  forkPoint: null,
};

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
  profileMarkdown: null,
  color: 'var(--ag-reader)',
  abbr: 'RE',
  wip: [],
};

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

describe('ReverseIndexPanel — piece 11 Phase 1: the anchored reader (anchored=true)', () => {
  const threeHits = [hits[0]!, hits[1]!, thirdHit];

  it('shows the scope header — ▐ + range when narrowed, ▤ whole-file when not', () => {
    const { rerender } = render(ReverseIndexPanel, {
      hits: threeHits,
      repo: 'wealdlore',
      path: 'Sources/Reader/PlateBundle.swift',
      range: [44, 49],
      anchored: true,
    });
    const scope = screen.getByTestId('anchor-scope');
    expect(scope).toHaveTextContent('▐');
    expect(scope).toHaveTextContent('PlateBundle.swift');

    rerender({ hits: threeHits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: null, anchored: true });
    expect(screen.getByTestId('anchor-scope')).toHaveTextContent('▤');
  });

  it('the FIRST hit is focused and expanded by default, the rest collapse into a "‹ N more" strip', () => {
    render(ReverseIndexPanel, { hits: threeHits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49], anchored: true });

    const expanded = screen.getByTestId('anchored-conv');
    expect(within(expanded).getByText('plate-bundle endpoint — the request these lines shipped for')).toBeInTheDocument();
    expect(screen.getAllByTestId('anchored-conv')).toHaveLength(1);
    expect(screen.getAllByTestId('anchored-more-pill')).toHaveLength(2);
    expect(screen.getByText('‹ 2 more')).toBeInTheDocument();
  });

  it('clicking a "more" pill focuses THAT conversation — it expands, the previously-expanded one joins the strip', async () => {
    const user = userEvent.setup();
    render(ReverseIndexPanel, { hits: threeHits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49], anchored: true });

    await user.click(screen.getAllByTestId('anchored-more-pill')[0]!);

    const expanded = screen.getByTestId('anchored-conv');
    expect(within(expanded).getByText('why not stream the regions?')).toBeInTheDocument();
    // The one that WAS expanded (reader's note) is now a pill instead.
    expect(screen.queryByText('plate-bundle endpoint — the request these lines shipped for')).not.toBeInTheDocument();
    expect(screen.getAllByTestId('anchored-more-pill')).toHaveLength(2);
  });

  it('j/k steps focus through the hits, wrapping neither past the first nor the last', async () => {
    const user = userEvent.setup();
    render(ReverseIndexPanel, { hits: threeHits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49], anchored: true });

    const panel = screen.getByRole('toolbar', { name: 'Conversations about this code' });
    panel.focus();
    await user.keyboard('j');
    expect(within(screen.getByTestId('anchored-conv')).getByText(/why not stream the regions/)).toBeInTheDocument();

    await user.keyboard('j');
    expect(within(screen.getByTestId('anchored-conv')).getByText(/size guard be configurable/)).toBeInTheDocument();

    // Already at the last hit — another 'j' stays put, doesn't overshoot.
    await user.keyboard('j');
    expect(within(screen.getByTestId('anchored-conv')).getByText(/size guard be configurable/)).toBeInTheDocument();

    await user.keyboard('k');
    expect(within(screen.getByTestId('anchored-conv')).getByText(/why not stream the regions/)).toBeInTheDocument();
  });

  it('clicking a row/pill NEVER fires onSelectHit or navigates away — only the expanded card\'s own link does', async () => {
    const user = userEvent.setup();
    const onSelectHit = vi.fn();
    const onOpenThread = vi.fn();
    render(ReverseIndexPanel, {
      hits: threeHits,
      repo: 'wealdlore',
      path: 'Sources/Reader/PlateBundle.swift',
      range: [44, 49],
      anchored: true,
      onSelectHit,
      onOpenThread,
    });

    await user.click(screen.getAllByTestId('anchored-more-pill')[0]!);
    expect(onSelectHit).not.toHaveBeenCalled();
    expect(onOpenThread).not.toHaveBeenCalled();

    await user.click(screen.getByTestId('open-full-thread'));
    expect(onSelectHit).not.toHaveBeenCalled();
    expect(onOpenThread).toHaveBeenCalledWith(hits[1]);
  });

  it('resolves a real agent color/display when the hit author is in `agents` — falls back honestly when not', () => {
    const { container } = render(ReverseIndexPanel, {
      hits: threeHits,
      repo: 'wealdlore',
      path: 'Sources/Reader/PlateBundle.swift',
      range: [44, 49],
      anchored: true,
      agents: [reader],
    });

    // hits[0] is from 'reader', who IS in `agents` — real display name + color.
    expect(screen.getByText('Reader')).toBeInTheDocument();
    expect(container.querySelector('.aav')?.getAttribute('style')).toContain('var(--ag-reader)');
  });

  it('falls back to a generic id-based treatment when the hit author is NOT in `agents` (no agents passed at all)', () => {
    render(ReverseIndexPanel, { hits: threeHits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49], anchored: true });
    // No real Agent for 'reader' here — falls back to the same cap()
    // treatment the non-anchored row form already uses.
    expect(screen.getByText('Reader')).toBeInTheDocument();
  });

  it('a scope with only ONE conversation shows no "more" strip at all', () => {
    render(ReverseIndexPanel, { hits: [hits[0]!], repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49], anchored: true });

    expect(screen.getByTestId('anchored-conv')).toBeInTheDocument();
    expect(screen.queryByTestId('anchored-more-pill')).not.toBeInTheDocument();
    expect(screen.queryByText(/more/)).not.toBeInTheDocument();
  });

  it('a new scope (different range) resets focus back to the first hit', async () => {
    const { rerender } = render(ReverseIndexPanel, {
      hits: threeHits,
      repo: 'wealdlore',
      path: 'Sources/Reader/PlateBundle.swift',
      range: [44, 49],
      anchored: true,
    });
    const user = userEvent.setup();
    await user.click(screen.getAllByTestId('anchored-more-pill')[1]!); // focus the 3rd hit

    await rerender({ hits: [otherFileHit], repo: 'wealdlore', path: 'pipeline/plates.py', range: [88, 102], anchored: true });
    expect(within(screen.getByTestId('anchored-conv')).getByText('restore chain context')).toBeInTheDocument();
  });

  it('anchored mode still shows the real empty state when there are no hits', () => {
    render(ReverseIndexPanel, { hits: [], repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [1, 2], anchored: true });
    expect(screen.getByText('No conversations yet')).toBeInTheDocument();
  });

  it('anchored=false (the default) is completely unaffected — the plain row list, unchanged', () => {
    render(ReverseIndexPanel, { hits: threeHits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49] });
    expect(screen.queryByTestId('anchored-conv')).not.toBeInTheDocument();
    expect(screen.queryByTestId('anchor-scope')).not.toBeInTheDocument();
    expect(screen.queryByTestId('anchored-more-pill')).not.toBeInTheDocument();
  });
});
