import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import CodeRefCard from './CodeRefCard.svelte';
import type { CodeRef, RefHit, Snippet } from '../types';
import { api } from '../api';

vi.mock('../api', () => ({
  api: {
    getCode: vi.fn(),
    getRefs: vi.fn(),
  },
}));

const ref: CodeRef = {
  repo: 'wealdlore',
  path: 'Sources/Reader/PlateBundle.swift',
  sha: 'a3f1c9',
  range: [44, 49],
  contentHash: 'sha256:9f2c...e01a',
  refName: null,
  refType: null,
  commitDate: null,
  dirty: false,
  untracked: false,
  baseRef: null,
  forkPoint: null,
};

const smallSnippet: Snippet = {
  lang: 'swift',
  staleness: 'current',
  lines: [
    { n: 44, text: 'func assembleBundle(uid: UID) throws -> PlateBundle {' },
    { n: 45, text: '  let plate = try store.restoredPlate(uid)' },
  ],
};

const bigSnippet: Snippet = {
  lang: 'python',
  staleness: 'changed',
  lines: Array.from({ length: 12 }, (_, i) => ({ n: 88 + i, text: `line ${i}` })),
};

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
    summary: 'plate-bundle endpoint',
    threadRoot: 'msg_01JQ8f2',
    requestStatus: 'DONE',
    hub: 'agent-coord',
    hubPrivate: false,
    refName: null,
    refType: null,
    commitDate: null,
    dirty: false,
    untracked: false,
    baseRef: null,
    forkPoint: null,
  },
];

describe('CodeRefCard', () => {
  it('renders the header (repo/path/sha/range) and the staleness badge', async () => {
    vi.mocked(api.getCode).mockResolvedValue(smallSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeRefCard, { ref, hub: 'agent-coord' });

    await waitFor(() => expect(api.getCode).toHaveBeenCalledWith('agent-coord', 'wealdlore', ref.path, 'a3f1c9', '44-49'));
    expect(screen.getByText('wealdlore', { exact: false })).toBeInTheDocument();
    expect(screen.getByText(ref.path)).toBeInTheDocument();
    expect(screen.getByText('@a3f1c9')).toBeInTheDocument();
    expect(screen.getByText('L44–49')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId('staleness-badge').textContent).toBe('current'));
  });

  it('auto-collapses a large snippet to a peek, and Expand reveals the full code', async () => {
    vi.mocked(api.getCode).mockResolvedValue(bigSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    const { container } = render(CodeRefCard, { ref, hub: 'agent-coord', collapseThreshold: 8 });

    await waitFor(() => expect(screen.getByText(/12 lines · expand/)).toBeInTheDocument());
    expect(container.querySelector('.refcard.collapsed')).toBeInTheDocument();
    expect(screen.getByTestId('staleness-badge').textContent).toBe('changed');

    const user = userEvent.setup();
    await user.click(screen.getByTestId('ref-toggle'));

    expect(container.querySelector('.refcard.collapsed')).not.toBeInTheDocument();
  });

  it('shows the reverse-index hook with the reference count, and fires onRevHook when clicked', async () => {
    vi.mocked(api.getCode).mockResolvedValue(smallSnippet);
    vi.mocked(api.getRefs).mockResolvedValue(hits);
    const onRevHook = vi.fn();

    render(CodeRefCard, { ref, hub: 'agent-coord', onRevHook });

    await waitFor(() => expect(screen.getByTestId('revhook')).toBeInTheDocument());
    expect(screen.getByTestId('revhook').textContent).toContain('1 conversation reference');

    const user = userEvent.setup();
    await user.click(screen.getByTestId('revhook'));

    expect(onRevHook).toHaveBeenCalledWith(ref, hits);
  });

  it('renders a branch chip + commit date beside the sha chip when refName/commitDate are present', async () => {
    vi.mocked(api.getCode).mockResolvedValue(smallSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);
    const branchRef: CodeRef = { ...ref, refName: 'main', refType: 'branch', commitDate: '2026-07-12T09:00:00Z' };

    render(CodeRefCard, { ref: branchRef, hub: 'agent-coord' });

    await waitFor(() => expect(screen.getByTestId('ref-branch-chip')).toBeInTheDocument());
    expect(screen.getByTestId('ref-branch-chip').textContent).toContain('main');
    expect(screen.getByTestId('commit-date').textContent).toBe('2026-07-12');
    expect(screen.getByTestId('ref-foot-pin').textContent).toContain('main');
    expect(screen.getByTestId('ref-foot-pin').textContent).toContain('2026-07-12');
  });

  it('renders a tag chip distinctly (refType tag) with no branch icon confusion', async () => {
    vi.mocked(api.getCode).mockResolvedValue(smallSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);
    const tagRef: CodeRef = { ...ref, refName: 'v1.2.0', refType: 'tag', commitDate: '2026-06-01T00:00:00Z' };

    render(CodeRefCard, { ref: tagRef, hub: 'agent-coord' });

    await waitFor(() => expect(screen.getByTestId('ref-branch-chip').textContent).toContain('v1.2.0'));
  });

  it('renders a dirty/untracked warning chip when the ref carries a working-tree snapshot', async () => {
    vi.mocked(api.getCode).mockResolvedValue(smallSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);
    const dirtyRef: CodeRef = { ...ref, dirty: true };

    render(CodeRefCard, { ref: dirtyRef, hub: 'agent-coord' });

    await waitFor(() => expect(screen.getByTestId('dirty-chip')).toBeInTheDocument());
    expect(screen.getByTestId('dirty-chip').textContent).toContain('working-tree snapshot');
  });

  it('does not render the dirty chip when neither dirty nor untracked is set', async () => {
    vi.mocked(api.getCode).mockResolvedValue(smallSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeRefCard, { ref, hub: 'agent-coord' });

    await waitFor(() => expect(screen.getByTestId('staleness-badge')).toBeInTheDocument());
    expect(screen.queryByTestId('dirty-chip')).not.toBeInTheDocument();
  });

  it.each([
    ['reachable', 'in history'],
    ['offline', 'offline'],
    ['unpinned', 'unpinned (legacy)'],
    ['unknown', 'not in local clone'],
  ] as const)('labels staleness %s as %s', async (staleness, label) => {
    vi.mocked(api.getCode).mockResolvedValue({ ...smallSnippet, staleness });
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeRefCard, { ref, hub: 'agent-coord' });

    await waitFor(() => expect(screen.getByTestId('staleness-badge').textContent).toBe(label));
  });

  it('renders a distinct "merged/squashed away" chip using baseRef/forkPoint when staleness is squashed', async () => {
    vi.mocked(api.getCode).mockResolvedValue({ ...smallSnippet, staleness: 'squashed' });
    vi.mocked(api.getRefs).mockResolvedValue([]);
    const squashedRef: CodeRef = {
      ...ref,
      refName: 'alignment-pass',
      refType: 'branch',
      baseRef: 'main',
      forkPoint: 'cafebabe0011223344556677889900aabbccddee',
    };

    render(CodeRefCard, { ref: squashedRef, hub: 'agent-coord' });

    await waitFor(() => expect(screen.getByTestId('squash-chip')).toBeInTheDocument());
    expect(screen.getByTestId('squash-chip').textContent).toContain('merged/squashed away');
    expect(screen.getByTestId('squash-chip').textContent).toContain('main@cafebab');
  });

  it('stops the revhook click from bubbling — CodeRefCard is nested inside Message.svelte\'s own clickable row, and opening the reverse index must not also fire that row\'s onSelect', async () => {
    vi.mocked(api.getCode).mockResolvedValue(smallSnippet);
    vi.mocked(api.getRefs).mockResolvedValue(hits);
    const onRevHook = vi.fn();
    const onDocumentClick = vi.fn();

    render(CodeRefCard, { ref, hub: 'agent-coord', onRevHook });
    document.addEventListener('click', onDocumentClick);

    try {
      await waitFor(() => expect(screen.getByTestId('revhook')).toBeInTheDocument());
      const user = userEvent.setup();
      await user.click(screen.getByTestId('revhook'));
    } finally {
      document.removeEventListener('click', onDocumentClick);
    }

    expect(onRevHook).toHaveBeenCalledWith(ref, hits);
    expect(onDocumentClick).not.toHaveBeenCalled();
  });
});
