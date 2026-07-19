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

  it('the FIRST hit is focused and expanded by default, the rest render as VISIBLE scannable rows (not hidden)', () => {
    render(ReverseIndexPanel, { hits: threeHits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49], anchored: true });

    const expanded = screen.getByTestId('anchored-conv');
    expect(within(expanded).getByText('plate-bundle endpoint — the request these lines shipped for')).toBeInTheDocument();
    expect(screen.getAllByTestId('anchored-conv')).toHaveLength(1);
    // Both OTHER hits are real, visible rows — not folded behind a count
    // (only 6+ hits trigger the "‹ N older" overflow, see below). Scoped to
    // the rows themselves (piece 11 Phase 5's own timeline reuses the SAME
    // summary text in its `.snip`, so a bare `screen.getByText` would now
    // be ambiguous).
    const rows = screen.getAllByTestId('anchored-row');
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent('why not stream the regions?');
    expect(rows[1]).toHaveTextContent('can the size guard be configurable?');
    expect(screen.queryByTestId('anchored-older')).not.toBeInTheDocument();
  });

  it('clicking a row focuses THAT conversation — it expands (accordion), the previously-expanded one becomes a row', async () => {
    const user = userEvent.setup();
    render(ReverseIndexPanel, { hits: threeHits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49], anchored: true });

    await user.click(screen.getAllByTestId('anchored-row')[0]!);

    const expanded = screen.getByTestId('anchored-conv');
    expect(within(expanded).getByText('why not stream the regions?')).toBeInTheDocument();
    // The one that WAS expanded (reader's note) is now a row instead — its
    // teaser text is still visible (rows are scannable, not hidden), just
    // no longer inside the expanded `.aconv` card.
    expect(within(expanded).queryByText('plate-bundle endpoint — the request these lines shipped for')).not.toBeInTheDocument();
    const rows = screen.getAllByTestId('anchored-row');
    expect(rows).toHaveLength(2);
    expect(rows.some((r) => r.textContent?.includes('plate-bundle endpoint — the request these lines shipped for'))).toBe(true);
  });

  it('6+ hits: only the visible cap renders as rows, the rest fold behind "‹ N older" — clicking it reveals them all', async () => {
    const user = userEvent.setup();
    const manyHits = [hits[0]!, hits[1]!, thirdHit, otherFileHit, { ...thirdHit, msgId: 'msg_extra1' }, { ...thirdHit, msgId: 'msg_extra2' }];
    render(ReverseIndexPanel, { hits: manyHits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49], anchored: true });

    // 1 expanded + 4 rows = the 5-item visible cap; 1 more hit folds away.
    expect(screen.getAllByTestId('anchored-row')).toHaveLength(4);
    const older = screen.getByTestId('anchored-older');
    expect(older).toHaveTextContent('‹ 1 older');

    await user.click(older);
    expect(screen.getAllByTestId('anchored-row')).toHaveLength(5);
    expect(screen.queryByTestId('anchored-older')).not.toBeInTheDocument();
  });

  it('j/k stepping past the visible cap auto-reveals the rest — the focused hit is never hidden', async () => {
    const user = userEvent.setup();
    const manyHits = [hits[0]!, hits[1]!, thirdHit, otherFileHit, { ...thirdHit, msgId: 'msg_extra1' }, { ...thirdHit, msgId: 'msg_extra2', summary: 'the sixth one' }];
    render(ReverseIndexPanel, { hits: manyHits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49], anchored: true });

    expect(screen.getByTestId('anchored-older')).toBeInTheDocument();
    const panel = screen.getByRole('toolbar', { name: 'Conversations about this code' });
    panel.focus();
    for (let i = 0; i < 5; i++) await user.keyboard('j');

    expect(within(screen.getByTestId('anchored-conv')).getByText('the sixth one')).toBeInTheDocument();
    expect(screen.queryByTestId('anchored-older')).not.toBeInTheDocument();
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

    await user.click(screen.getAllByTestId('anchored-row')[0]!);
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

    // hits[0] is from 'reader', who IS in `agents` — real display name +
    // color, in the expanded card specifically (piece 11 Phase 5's own
    // timeline also names 'Reader' once, in its own `.who` — scope to the
    // accordion's avatar to avoid that ambiguity).
    expect(within(screen.getByTestId('anchored-conv')).getByText('Reader')).toBeInTheDocument();
    expect(container.querySelector('.aav')?.getAttribute('style')).toContain('var(--ag-reader)');
  });

  it('falls back to a generic id-based treatment when the hit author is NOT in `agents` (no agents passed at all)', () => {
    render(ReverseIndexPanel, { hits: threeHits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49], anchored: true });
    // No real Agent for 'reader' here — falls back to the same cap()
    // treatment the non-anchored row form already uses. Scoped to the
    // expanded card — the timeline names 'Reader' too (its own `.who`).
    expect(within(screen.getByTestId('anchored-conv')).getByText('Reader')).toBeInTheDocument();
  });

  it('a scope with only ONE conversation shows no rows and no overflow at all', () => {
    render(ReverseIndexPanel, { hits: [hits[0]!], repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49], anchored: true });

    expect(screen.getByTestId('anchored-conv')).toBeInTheDocument();
    expect(screen.queryByTestId('anchored-row')).not.toBeInTheDocument();
    expect(screen.queryByTestId('anchored-older')).not.toBeInTheDocument();
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
    await user.click(screen.getAllByTestId('anchored-row')[1]!); // focus the 3rd hit

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
    expect(screen.queryByTestId('anchored-row')).not.toBeInTheDocument();
  });
});

describe('ReverseIndexPanel — piece 11 Phase 5: the sidebar conversation timeline', () => {
  // hits[0] (reader, Jul 17) / hits[1] (compositor, Jul 10) / thirdHit
  // (compositor, Jul 12) all share ONE sha in the base fixtures — clone
  // thirdHit onto a genuinely OLDER sha so `viewedSha` comparisons and
  // "align" actually have something real to distinguish.
  const olderHit: RefHit = { ...thirdHit, msgId: 'msg_older', sha: 'deadbeef01', commitDate: '2026-06-01T08:00:00Z', ts: '2026-06-01T08:00:00Z' };
  const timelineHits = [hits[0]!, hits[1]!, olderHit]; // deliberately NOT chronological order

  it('orders nodes oldest→newest by real ts, not by the order `hits` was given in', () => {
    render(ReverseIndexPanel, {
      hits: timelineHits,
      repo: 'wealdlore',
      path: 'Sources/Reader/PlateBundle.swift',
      range: [44, 49],
      anchored: true,
      viewedSha: 'a3f1c9',
    });

    const nodes = screen.getAllByTestId('timeline-node');
    expect(nodes).toHaveLength(3);
    // Real chronological order: olderHit (Jun 1) -> hits[1] (Jul 10) ->
    // hits[0] (Jul 17) — NOT the `[hits[0], hits[1], olderHit]` order the
    // `hits` prop was actually given in.
    expect(nodes[0]).toHaveTextContent('can the size guard be configurable?'); // olderHit
    expect(nodes[1]).toHaveTextContent('why not stream the regions?'); // hits[1]
    expect(nodes[2]).toHaveTextContent('plate-bundle endpoint — the request these lines shipped for'); // hits[0]
  });

  it('the node matching `viewedSha` reads "this" (green), every other real sha reads "older" (amber)', () => {
    render(ReverseIndexPanel, {
      hits: timelineHits,
      repo: 'wealdlore',
      path: 'Sources/Reader/PlateBundle.swift',
      range: [44, 49],
      anchored: true,
      viewedSha: 'a3f1c9',
    });

    const nodes = screen.getAllByTestId('timeline-node');
    const cur = nodes.filter((n) => n.classList.contains('cur'));
    const old = nodes.filter((n) => n.classList.contains('old'));
    // Two real hits share the viewed sha 'a3f1c9' (hits[0] and hits[1]) —
    // both read "this"; only the genuinely older-sha'd node reads "older".
    expect(cur).toHaveLength(2);
    expect(old).toHaveLength(1);
    expect(cur.every((n) => n.textContent?.includes('this'))).toBe(true);
    expect(old[0]).toHaveTextContent('deadbeef01');
  });

  it('an OLDER node offers "↳ align code to this version" with its REAL sha; a "this" node offers no align button at all', async () => {
    const onAlignToRevision = vi.fn();
    render(ReverseIndexPanel, {
      hits: timelineHits,
      repo: 'wealdlore',
      path: 'Sources/Reader/PlateBundle.swift',
      range: [44, 49],
      anchored: true,
      viewedSha: 'a3f1c9',
      onAlignToRevision,
    });

    expect(screen.getAllByTestId('align-to-revision')).toHaveLength(1); // only the older-sha'd node
    const user = userEvent.setup();
    await user.click(screen.getByTestId('align-to-revision'));
    expect(onAlignToRevision).toHaveBeenCalledWith('deadbeef01');
  });

  it('reading/focusing a node is NOT the align action — clicking the node body only moves accordion focus, never calls onAlignToRevision', async () => {
    const onAlignToRevision = vi.fn();
    render(ReverseIndexPanel, {
      hits: timelineHits,
      repo: 'wealdlore',
      path: 'Sources/Reader/PlateBundle.swift',
      range: [44, 49],
      anchored: true,
      viewedSha: 'a3f1c9',
      onAlignToRevision,
    });

    const user = userEvent.setup();
    await user.click(screen.getAllByTestId('timeline-node-focus')[0]!);

    expect(onAlignToRevision).not.toHaveBeenCalled();
    // It DID move accordion focus — a real, visible effect of reading.
    expect(screen.getByTestId('anchored-conv')).toBeInTheDocument();
  });

  it('the header names real counts: N conversations across M distinct versions', () => {
    render(ReverseIndexPanel, {
      hits: timelineHits,
      repo: 'wealdlore',
      path: 'Sources/Reader/PlateBundle.swift',
      range: [44, 49],
      anchored: true,
      viewedSha: 'a3f1c9',
    });
    expect(screen.getByText(/3 conversations across 2 versions/)).toBeInTheDocument();
  });

  it('law #3 — honest fallback when an older node genuinely has no commitDate, never a fabricated one', () => {
    const undated: RefHit = { ...olderHit, msgId: 'msg_undated', commitDate: null };
    render(ReverseIndexPanel, {
      hits: [hits[0]!, undated],
      repo: 'wealdlore',
      path: 'Sources/Reader/PlateBundle.swift',
      range: [44, 49],
      anchored: true,
      viewedSha: 'a3f1c9',
    });
    const node = screen.getAllByTestId('timeline-node').find((n) => n.classList.contains('old'))!;
    expect(node).toHaveTextContent('deadbeef01');
    // No date rendered for it — not even an empty/undefined string leaking through.
    expect(node.textContent).not.toMatch(/undefined|NaN|null/);
  });

  it('no timeline at all in repo-mode, or when the scope has zero hits', () => {
    render(ReverseIndexPanel, { hits: [], repo: 'wealdlore', path: null, anchored: true }); // repo-mode
    expect(screen.queryByTestId('conversation-timeline')).not.toBeInTheDocument();
  });

  it('no timeline when anchored is false — the plain row list has no version concept', () => {
    render(ReverseIndexPanel, { hits: timelineHits, repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 49] });
    expect(screen.queryByTestId('conversation-timeline')).not.toBeInTheDocument();
  });
});
