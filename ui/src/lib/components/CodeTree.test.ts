import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import CodeTree from './CodeTree.svelte';
import type { CodeFile } from '../types';
import { api } from '../api';
import { codeState } from '../stores.svelte';
import { fileKey } from '../codeTree';

vi.mock('../api', () => ({
  api: { getCodeFiles: vi.fn() },
}));

beforeEach(() => {
  vi.mocked(api.getCodeFiles).mockReset();
  codeState.clear();
});

const threeFiles: CodeFile[] = [
  { repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', refCount: 3, mapped: true, lastTs: '2026-07-17T14:46:00Z' },
  { repo: 'wealdlore', path: 'pipeline/plates.py', refCount: 2, mapped: true, lastTs: '2026-07-17T14:52:00Z' },
  { repo: 'wealdlore', path: 'studio-markup/citations.py', refCount: 1, mapped: false, lastTs: '2026-07-10T09:00:00Z' },
];

describe('CodeTree — fetch + default expansion', () => {
  it('fetches via codeState.load and renders the tree with repo + file rows', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);

    render(CodeTree, { hub: 'agent-coord' });

    expect(api.getCodeFiles).toHaveBeenCalledWith('agent-coord');
    expect(await screen.findByRole('button', { name: /wealdlore/ })).toBeInTheDocument();
    // <=2 repos total (just wealdlore here) — the repo is expanded by
    // default, and PlateBundle.swift auto-activated (so its own dir
    // ancestor auto-expands too) — but sibling dirs (pipeline/,
    // studio-markup/) start collapsed, per the cold-render policy.
    expect(await screen.findByRole('button', { name: /PlateBundle\.swift/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'pipeline/' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /plates\.py/ })).not.toBeInTheDocument();
  });

  it('expanding a collapsed dir reveals its file rows', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    render(CodeTree, { hub: 'agent-coord' });
    await screen.findByRole('button', { name: /wealdlore/ });

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'pipeline/' }));

    expect(await screen.findByRole('button', { name: /plates\.py/ })).toBeInTheDocument();
  });

  it('shows a loading skeleton before the fetch resolves', () => {
    vi.mocked(api.getCodeFiles).mockReturnValue(new Promise(() => {}));
    render(CodeTree, { hub: 'agent-coord' });
    expect(screen.getAllByTestId('skeleton').length).toBeGreaterThan(0);
  });

  it('shows an empty state for a hub with no referenced files', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue([]);
    render(CodeTree, { hub: 'confer-jarvis-orbit' });
    expect(await screen.findByText('No code referenced yet')).toBeInTheDocument();
  });

  it('dims unmapped file rows and gives them a title, without a color-only dot', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    render(CodeTree, { hub: 'agent-coord' });
    await screen.findByRole('button', { name: /wealdlore/ });

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'studio-markup/' }));

    const unmappedRow = await screen.findByRole('button', { name: /citations\.py/ });
    expect(unmappedRow.className).toContain('unmapped');
    expect(unmappedRow.getAttribute('title')).toMatch(/unmapped/);
  });

  it('shows a refCount badge on file rows', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    render(CodeTree, { hub: 'agent-coord' });
    const row = await screen.findByRole('button', { name: /PlateBundle\.swift/ });
    expect(row.textContent).toContain('3');
  });
});

describe('CodeTree — expand/collapse + selection', () => {
  it('clicking a file row activates it in the shared codeState store and fires onActivate', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    const onActivate = vi.fn();
    render(CodeTree, { hub: 'agent-coord', onActivate });
    await screen.findByRole('button', { name: /wealdlore/ });

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'pipeline/' }));
    await user.click(await screen.findByRole('button', { name: /plates\.py/ }));

    expect(codeState.forHub('agent-coord').activeKey).toBe(fileKey(threeFiles[1]!));
    expect(onActivate).toHaveBeenCalledWith(threeFiles[1]);
  });

  it('collapses/expands a repo node on click, toggling its children out of the DOM (lazy render)', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    render(CodeTree, { hub: 'agent-coord' });

    const repoRow = await screen.findByRole('button', { name: /wealdlore/ });
    expect(screen.getByRole('button', { name: /PlateBundle\.swift/ })).toBeInTheDocument();

    const user = userEvent.setup();
    await user.click(repoRow);
    expect(screen.queryByRole('button', { name: /PlateBundle\.swift/ })).not.toBeInTheDocument();

    await user.click(repoRow);
    expect(await screen.findByRole('button', { name: /PlateBundle\.swift/ })).toBeInTheDocument();
  });

  it('collapses repos beyond the first two by default when there are >2 repos, unless they hold the active file', async () => {
    const manyRepoFiles: CodeFile[] = [
      { repo: 'repo-a', path: 'a.rs', refCount: 1, mapped: true, lastTs: '2026-07-17T10:00:00Z' },
      { repo: 'repo-b', path: 'b.rs', refCount: 1, mapped: true, lastTs: '2026-07-17T10:00:00Z' },
      { repo: 'repo-c', path: 'c.rs', refCount: 1, mapped: true, lastTs: '2026-07-17T10:00:00Z' },
    ];
    vi.mocked(api.getCodeFiles).mockResolvedValue(manyRepoFiles);
    render(CodeTree, { hub: 'agent-coord' });

    await screen.findByRole('button', { name: /repo-a/ });
    // repo-a holds the auto-activated first file, so it's expanded; repo-b
    // and repo-c are not — collapsed-by-default keeps cold render O(repos).
    expect(screen.getByRole('button', { name: /a\.rs/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^b\.rs$/ })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /^c\.rs$/ })).not.toBeInTheDocument();
  });
});

describe('CodeTree — filter (findability escape hatch)', () => {
  it('typing into the filter box switches to a flat match list', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    render(CodeTree, { hub: 'agent-coord' });
    await screen.findByRole('button', { name: /wealdlore/ });

    const user = userEvent.setup();
    await user.type(screen.getByLabelText('Filter code files'), 'plates');

    expect(await screen.findByRole('listbox', { name: 'Filter matches' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /plates\.py/ })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /PlateBundle\.swift/ })).not.toBeInTheDocument();
    // The repo tree itself is gone while filtering — flat list only (its
    // repo nodes carry a "Σ" aggregate badge unique to tree rows).
    expect(screen.queryByRole('button', { name: /Σ/ })).not.toBeInTheDocument();
  });

  it('Escape clears the filter and returns to the tree', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    render(CodeTree, { hub: 'agent-coord' });
    await screen.findByRole('button', { name: /wealdlore/ });

    const user = userEvent.setup();
    const input = screen.getByLabelText('Filter code files');
    await user.type(input, 'plates');
    expect(screen.getByRole('listbox')).toBeInTheDocument();

    await user.type(input, '{Escape}');
    expect(screen.queryByRole('listbox')).not.toBeInTheDocument();
    expect(await screen.findByRole('button', { name: /wealdlore/ })).toBeInTheDocument();
  });

  it('Enter opens the top match and activates it', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    const onActivate = vi.fn();
    render(CodeTree, { hub: 'agent-coord', onActivate });
    await screen.findByRole('button', { name: /wealdlore/ });

    const user = userEvent.setup();
    await user.type(screen.getByLabelText('Filter code files'), 'plates.py{Enter}');

    expect(codeState.forHub('agent-coord').activeKey).toBe(fileKey(threeFiles[1]!));
    expect(onActivate).toHaveBeenCalledWith(threeFiles[1]);
  });
});

describe('CodeTree — Tree | Active toggle', () => {
  it('switches to a ranked flat list (refCount desc) when Active is selected', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    render(CodeTree, { hub: 'agent-coord' });
    await screen.findByRole('button', { name: /wealdlore/ });

    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Active' }));

    const rows = screen.getAllByRole('button').filter((b) => /\.(swift|py)\b/.test(b.textContent ?? ''));
    // PlateBundle.swift (refCount 3) should rank ahead of plates.py (2) and
    // citations.py (1) — the backend's own refCount-desc order.
    const order = rows.map((r) => r.textContent);
    const idxBundle = order.findIndex((t) => t?.includes('PlateBundle'));
    const idxPlates = order.findIndex((t) => t?.includes('plates.py'));
    expect(idxBundle).toBeGreaterThanOrEqual(0);
    expect(idxPlates).toBeGreaterThanOrEqual(0);
    expect(idxBundle).toBeLessThan(idxPlates);
  });
});

describe('CodeTree — active-file auto-reveal', () => {
  it('expanding to a nested active file reveals its ancestor dir automatically', async () => {
    const nested: CodeFile[] = [{ repo: 'wealdlore', path: 'src/lib/deep/Thing.swift', refCount: 1, mapped: true, lastTs: '2026-07-17T10:00:00Z' }];
    vi.mocked(api.getCodeFiles).mockResolvedValue(nested);
    render(CodeTree, { hub: 'agent-coord' });

    // Single repo -> expanded by default; the file's compacted dir ancestor
    // is auto-expanded too since it's the active file's own repo.
    await waitFor(() => expect(screen.getByRole('button', { name: /Thing\.swift/ })).toBeInTheDocument());
  });

  it('setting pendingReveal on the store expands ancestors and does not throw', async () => {
    const nested: CodeFile[] = [
      { repo: 'wealdlore', path: 'src/lib/deep/Thing.swift', refCount: 1, mapped: true, lastTs: '2026-07-17T10:00:00Z' },
      { repo: 'other-repo', path: 'a.rs', refCount: 1, mapped: true, lastTs: '2026-07-17T10:00:00Z' },
      { repo: 'third-repo', path: 'b.rs', refCount: 1, mapped: true, lastTs: '2026-07-17T10:00:00Z' },
    ];
    vi.mocked(api.getCodeFiles).mockResolvedValue(nested);
    render(CodeTree, { hub: 'agent-coord' });
    await screen.findByRole('button', { name: /wealdlore/ });

    codeState.forHub('agent-coord').pendingReveal = fileKey(nested[0]!);

    await waitFor(() => expect(codeState.forHub('agent-coord').pendingReveal).toBeNull());
    expect(screen.getByRole('button', { name: /Thing\.swift/ })).toBeInTheDocument();
  });
});
