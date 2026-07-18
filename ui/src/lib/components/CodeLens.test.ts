import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import CodeLens from './CodeLens.svelte';
import type { CodeFile, RefHit, Snippet } from '../types';
import { api } from '../api';

vi.mock('../api', () => ({
  api: {
    getCode: vi.fn(),
    getRefs: vi.fn(),
    getCodeFiles: vi.fn(),
  },
}));

beforeEach(() => {
  vi.mocked(api.getCode).mockReset();
  vi.mocked(api.getRefs).mockReset();
  vi.mocked(api.getCodeFiles).mockReset();
});

const plateBundleSnippet: Snippet = {
  lang: 'swift',
  staleness: 'current',
  lines: [
    { n: 44, text: 'func assembleBundle(uid: UID) throws -> PlateBundle {' },
    { n: 45, text: '  let plate = try store.restoredPlate(uid)' },
    { n: 46, text: '}' },
  ],
};

// Same three-file shape the component used to hardcode: two mapped files in
// one repo, one unmapped — now sourced from getCodeFiles instead.
const threeFiles: CodeFile[] = [
  { repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', refCount: 3, mapped: true, lastTs: '2026-07-17T14:46:00Z' },
  { repo: 'wealdlore', path: 'pipeline/plates.py', refCount: 2, mapped: true, lastTs: '2026-07-17T14:52:00Z' },
  { repo: 'wealdlore', path: 'studio-markup/citations.py', refCount: 1, mapped: false, lastTs: '2026-07-10T09:00:00Z' },
];

function hitOn(line: number, path = 'Sources/Reader/PlateBundle.swift'): RefHit {
  return {
    repo: 'wealdlore',
    path,
    sha: 'a3f1c9',
    range: [line, line],
    contentHash: null,
    staleness: 'current',
    msgId: `msg_${line}`,
    from: 'reader',
    msgType: 'note',
    ts: '2026-07-17T14:46:00Z',
    topic: 'reader',
    summary: 'discussion',
    threadRoot: 'msg_root',
    requestStatus: null,
    hub: 'agent-coord',
    hubPrivate: false,
  };
}

describe('CodeLens', () => {
  it('fetches getCodeFiles(hub) and loads the first (mapped) file, calling getCode/getRefs with the right args', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    expect(api.getCodeFiles).toHaveBeenCalledWith('agent-coord');
    await waitFor(() =>
      expect(api.getCode).toHaveBeenCalledWith('agent-coord', 'wealdlore', 'Sources/Reader/PlateBundle.swift', 'HEAD')
    );
    expect(api.getRefs).toHaveBeenCalledWith('agent-coord', 'wealdlore:Sources/Reader/PlateBundle.swift', true);

    // Shiki tokenizes the line into several <span> pieces, so match on the
    // reassembled text content of the code block rather than a single node.
    await waitFor(() => expect(container.querySelector('.densefile .code')?.textContent).toContain('func assembleBundle'));
  });

  it('renders the fetched files in the tree by basename, grouped under their repo', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeLens, { hub: 'agent-coord' });

    expect(await screen.findByRole('button', { name: 'PlateBundle.swift' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'plates.py' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'citations.py' })).toBeInTheDocument();
    expect(screen.getByText('wealdlore')).toBeInTheDocument(); // the repo group header
  });

  it('marks mapped vs unmapped files with a different dot color', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeLens, { hub: 'agent-coord' });
    await waitFor(() => expect(screen.getByRole('button', { name: 'citations.py' })).toBeInTheDocument());

    const mappedDot = screen.getByRole('button', { name: 'PlateBundle.swift' }).querySelector('.fdot') as HTMLElement;
    const unmappedDot = screen.getByRole('button', { name: 'citations.py' }).querySelector('.fdot') as HTMLElement;

    expect(mappedDot.getAttribute('style')).toContain('var(--done)');
    expect(unmappedDot.getAttribute('style')).toContain('var(--faint)');
  });

  it('shows the new empty state — not the file tree — when the hub has no referenced code files', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue([]);

    render(CodeLens, { hub: 'confer-jarvis-orbit' });

    expect(await screen.findByText('No code referenced in this hub yet')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'PlateBundle.swift' })).not.toBeInTheDocument();
    // No per-file fetches when there's nothing to select.
    expect(api.getCode).not.toHaveBeenCalled();
    expect(api.getRefs).not.toHaveBeenCalled();
  });

  it('re-fetches getCodeFiles and resets selection when the hub changes', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    const { rerender } = render(CodeLens, { hub: 'agent-coord' });
    await waitFor(() => expect(api.getCodeFiles).toHaveBeenCalledWith('agent-coord'));

    const otherHubFiles: CodeFile[] = [
      { repo: 'jarvis-repo', path: 'src/main.rs', refCount: 1, mapped: true, lastTs: '2026-07-17T10:00:00Z' },
    ];
    vi.mocked(api.getCodeFiles).mockResolvedValue(otherHubFiles);

    await rerender({ hub: 'confer-jarvis-orbit' });

    await waitFor(() => expect(api.getCodeFiles).toHaveBeenCalledWith('confer-jarvis-orbit'));
    expect(await screen.findByRole('button', { name: 'main.rs' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'PlateBundle.swift' })).not.toBeInTheDocument();
  });

  it('shows a loading skeleton before the file-list fetch resolves', async () => {
    let resolveFiles!: (files: CodeFile[]) => void;
    vi.mocked(api.getCodeFiles).mockReturnValue(new Promise((res) => (resolveFiles = res)));

    render(CodeLens, { hub: 'agent-coord' });

    expect(screen.getAllByTestId('skeleton').length).toBeGreaterThan(0);
    resolveFiles(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    await waitFor(() => expect(screen.getByRole('button', { name: 'PlateBundle.swift' })).toBeInTheDocument());
  });

  it('switching to an unmapped file shows the "no clone mapped" empty state, disabled, without calling the API', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeLens, { hub: 'agent-coord' });
    await waitFor(() => expect(api.getCode).toHaveBeenCalledTimes(1));

    const user = userEvent.setup();
    await user.click(screen.getByText('citations.py'));

    expect(await screen.findByText('No clone mapped for this repo')).toBeInTheDocument();
    // The empty-state action is disabled (mapping a clone isn't wired up yet).
    expect(screen.getByText('＋ map a clone to see the code')).toBeDisabled();
    // No new fetch happens for the unmapped file.
    expect(api.getCode).toHaveBeenCalledTimes(1);
  });

  it('shows "No code returned" when the backend responds with an empty line set for a mapped file', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue({ lang: 'swift', staleness: 'current', lines: [] });
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeLens, { hub: 'agent-coord' });

    expect(await screen.findByText('No code returned')).toBeInTheDocument();
  });

  it('renders a density hook only for lines with reference hits, filtered to the active file (repo+path)', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([
      hitOn(44),
      hitOn(45),
      hitOn(999, 'some/other/file.swift'), // different path — must be filtered out
    ]);

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(container.querySelectorAll('.dens.hit').length).toBe(2));
    const hitButtons = container.querySelectorAll('.dens.hit');
    expect(hitButtons[0]!.textContent).toBe('1');
    expect(hitButtons[0]!.getAttribute('title')).toBe('1 conversation reference this line');
  });

  it('pluralizes the hit-count title correctly, and clamps the heat variable at 42% for >=5 refs on one line', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([hitOn(44), hitOn(44), hitOn(44), hitOn(44), hitOn(44), hitOn(44)]); // 6 hits, same line

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(container.querySelector('.dens.hit')).toBeInTheDocument());
    const hitButton = container.querySelector('.dens.hit') as HTMLElement;
    expect(hitButton.textContent).toBe('6');
    expect(hitButton.getAttribute('title')).toBe('6 conversations reference this line');
    // 6 * 10 = 60, clamped to 42
    expect(hitButton.getAttribute('style')).toMatch(/--heat:\s*42%/);
  });

  it('clicking a hot line fires onOpenRefs with the active file context and that line\'s hits', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    const hit = hitOn(45);
    vi.mocked(api.getRefs).mockResolvedValue([hit]);
    const onOpenRefs = vi.fn();

    const { container } = render(CodeLens, { hub: 'agent-coord', onOpenRefs });

    await waitFor(() => expect(container.querySelector('.dens.hit')).toBeInTheDocument());
    const user = userEvent.setup();
    await user.click(container.querySelector('.dens.hit') as HTMLElement);

    expect(onOpenRefs).toHaveBeenCalledWith(
      { repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [45, 45] },
      [hit]
    );
  });

  it('a cold (non-referenced) line is not clickable and does not fire onOpenRefs', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);
    const onOpenRefs = vi.fn();

    const { container } = render(CodeLens, { hub: 'agent-coord', onOpenRefs });

    await waitFor(() => expect(screen.queryByTestId('skeleton')).not.toBeInTheDocument());
    expect(container.querySelectorAll('.dens.hit').length).toBe(0);
    expect(container.querySelectorAll('.dens').length).toBeGreaterThan(0);
    expect(onOpenRefs).not.toHaveBeenCalled();
  });

  it('re-fetches when switching between two mapped files', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeLens, { hub: 'agent-coord' });
    await waitFor(() => expect(api.getCode).toHaveBeenCalledTimes(1));

    const user = userEvent.setup();
    await user.click(screen.getByText('plates.py'));

    await waitFor(() =>
      expect(api.getCode).toHaveBeenLastCalledWith('agent-coord', 'wealdlore', 'pipeline/plates.py', 'HEAD')
    );
    expect(api.getCode).toHaveBeenCalledTimes(2);
  });
});
