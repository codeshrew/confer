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
});
