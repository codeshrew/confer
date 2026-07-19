import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import CodeLens from './CodeLens.svelte';
import type { Agent, CodeFile, RefHit, Snippet } from '../types';
import { api } from '../api';
import { codeState } from '../stores.svelte';
import { fileKey } from '../codeTree';

// design/43 Phase B: CodeLens no longer owns the file list (that's
// CodeTree.svelte, in the left-rail slot — see CodeTree.test.ts) or fetches
// `/api/codefiles` directly — it reads `files`/`activeKey` off the shared
// `codeState` store (stores.svelte.ts), same as CodeTree does. These specs
// drive file selection by mutating `codeState.forHub(hub).activeKey`
// directly instead of clicking a file-tree row that no longer lives here.

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
  codeState.clear();
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
// one repo, one unmapped — now sourced from getCodeFiles via codeState.
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
    refName: null,
    refType: null,
    commitDate: null,
    dirty: false,
    untracked: false,
    baseRef: null,
    forkPoint: null,
  };
}

/** A whole-file reference — `range: null` — the shape nearly all real
 * `--ref` hits actually take. */
function wholeFileHit(overrides: Partial<RefHit> = {}, path = 'Sources/Reader/PlateBundle.swift'): RefHit {
  return {
    repo: 'wealdlore',
    path,
    sha: 'HEAD',
    range: null,
    contentHash: null,
    staleness: 'current',
    msgId: 'msg_done_1',
    from: 'studio',
    msgType: 'done',
    ts: '2026-07-17T15:00:00Z',
    topic: 'general',
    summary: 'shipped the taxonomy pass',
    threadRoot: 'msg_root_1',
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
    ...overrides,
  };
}

describe('CodeLens', () => {
  it('loads codeState (the SAME store CodeTree writes to) and auto-activates the first file, calling getCode/getRefs with the right args', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    expect(api.getCodeFiles).toHaveBeenCalledWith('agent-coord');
    await waitFor(() =>
      expect(api.getCode).toHaveBeenCalledWith('agent-coord', 'wealdlore', 'Sources/Reader/PlateBundle.swift', 'HEAD')
    );
    expect(api.getRefs).toHaveBeenCalledWith('agent-coord', 'wealdlore:Sources/Reader/PlateBundle.swift', true);
    expect(codeState.forHub('agent-coord').activeKey).toBe(fileKey(threeFiles[0]!));

    // Shiki tokenizes the line into several <span> pieces, so match on the
    // reassembled text content of the code block rather than a single node.
    await waitFor(() => expect(container.querySelector('.densefile .code')?.textContent).toContain('func assembleBundle'));
  });

  it('shows the new empty state when the hub has no referenced code files', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue([]);

    render(CodeLens, { hub: 'confer-jarvis-orbit' });

    expect(await screen.findByText('No code referenced in this hub yet')).toBeInTheDocument();
    // No per-file fetches when there's nothing to select.
    expect(api.getCode).not.toHaveBeenCalled();
    expect(api.getRefs).not.toHaveBeenCalled();
  });

  it('shows a loading skeleton before the codeState fetch resolves', async () => {
    let resolveFiles!: (files: CodeFile[]) => void;
    vi.mocked(api.getCodeFiles).mockReturnValue(new Promise((res) => (resolveFiles = res)));

    render(CodeLens, { hub: 'agent-coord' });

    expect(screen.getAllByTestId('skeleton').length).toBeGreaterThan(0);
    resolveFiles(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    await waitFor(() => expect(api.getCode).toHaveBeenCalled());
  });

  it('re-fetches when switching to a different active file (as CodeTree would drive via the shared store)', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeLens, { hub: 'agent-coord' });
    await waitFor(() => expect(api.getCode).toHaveBeenCalledTimes(1));

    codeState.forHub('agent-coord').activeKey = fileKey(threeFiles[1]!);

    await waitFor(() =>
      expect(api.getCode).toHaveBeenLastCalledWith('agent-coord', 'wealdlore', 'pipeline/plates.py', 'HEAD')
    );
    expect(api.getCode).toHaveBeenCalledTimes(2);
  });

  it('switching the active key to an unmapped file shows the "no clone mapped" empty state, disabled, without calling getCode again', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeLens, { hub: 'agent-coord' });
    await waitFor(() => expect(api.getCode).toHaveBeenCalledTimes(1));

    codeState.forHub('agent-coord').activeKey = fileKey(threeFiles[2]!); // citations.py, unmapped

    expect(await screen.findByText('No clone mapped for this repo')).toBeInTheDocument();
    expect(screen.getByText('＋ map a clone to see the code')).toBeDisabled();
    expect(api.getCode).toHaveBeenCalledTimes(1);
  });

  it('shows "No code returned" when the backend responds with an empty line set for a mapped file', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue({ lang: 'swift', staleness: 'current', lines: [] });
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeLens, { hub: 'agent-coord' });

    expect(await screen.findByText('No code returned')).toBeInTheDocument();
  });

  it('piece 11 Phase 2 — renders a range tab only for lines with reference hits, filtered to the active file (repo+path)', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([hitOn(44), hitOn(45), hitOn(999, 'some/other/file.swift')]); // different path — must be filtered out

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(container.querySelectorAll('[data-testid="gutter-tab"]').length).toBe(2));
    const tabs = container.querySelectorAll('[data-testid="gutter-tab"]');
    // hitOn(44)/hitOn(45) are single-line hits from 'reader' — a 1-count
    // tick tab, real initials (no `agents` passed here, so the honest
    // id-derived fallback: 'reader' -> 'RE').
    expect(tabs[0]!.textContent).toBe('1 · RE');
  });

  it('piece 11 Phase 2 — pluralizes the tab title correctly for multiple hits on one identical range', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([hitOn(44), hitOn(44), hitOn(44), hitOn(44), hitOn(44), hitOn(44)]); // 6 hits, identical range -> ONE entry

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(container.querySelector('[data-testid="gutter-tab"]')).toBeInTheDocument());
    const tab = container.querySelector('[data-testid="gutter-tab"]') as HTMLElement;
    expect(tab.textContent).toBe('6 · RE');
    expect(tab.getAttribute('title')).toMatch(/^6 conversations · Reader · latest/);
  });

  it('piece 11 Phase 2 — clicking a range tab fires onOpenRefs with the active file context and the entry\'s REAL range + hits', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    const hit = hitOn(45);
    vi.mocked(api.getRefs).mockResolvedValue([hit]);
    const onOpenRefs = vi.fn();

    const { container } = render(CodeLens, { hub: 'agent-coord', onOpenRefs });

    await waitFor(() => expect(container.querySelector('[data-testid="gutter-tab"]')).toBeInTheDocument());
    const user = userEvent.setup();
    await user.click(container.querySelector('[data-testid="gutter-tab"]') as HTMLElement);

    expect(onOpenRefs).toHaveBeenCalledWith(
      { repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [45, 45] },
      [hit]
    );
  });

  it('a cold (non-referenced) line has no gutter tab and does not fire onOpenRefs', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);
    const onOpenRefs = vi.fn();

    const { container } = render(CodeLens, { hub: 'agent-coord', onOpenRefs });

    await waitFor(() => expect(screen.queryByTestId('skeleton')).not.toBeInTheDocument());
    expect(document.querySelectorAll('[data-testid="gutter-tab"]').length).toBe(0);
    // The empty gutter column slots still render (reserving the layout),
    // just with no bracket/tick inside any of them.
    expect(container.querySelectorAll('.gcol').length).toBeGreaterThan(0);
    expect(container.querySelectorAll('.br, .tick').length).toBe(0);
    expect(onOpenRefs).not.toHaveBeenCalled();
  });

  it('fires onFileRefs with the whole-file (range:null) hits too — not just the ones that light the gutter', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    const ranged = hitOn(45);
    const wholeFile = wholeFileHit();
    vi.mocked(api.getRefs).mockResolvedValue([ranged, wholeFile]);
    const onFileRefs = vi.fn();

    render(CodeLens, { hub: 'agent-coord', onFileRefs });

    await waitFor(() =>
      expect(onFileRefs).toHaveBeenCalledWith(
        { repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift' },
        expect.arrayContaining([ranged, wholeFile])
      )
    );
    // The gutter only ever sees the ranged hit — a range:null hit lights no
    // line — but it must still be present in what onFileRefs reports.
    const [, hits] = onFileRefs.mock.calls[onFileRefs.mock.calls.length - 1]!;
    expect(hits).toHaveLength(2);
  });

  it('renders at the newest hit\'s pinned sha instead of a hardcoded HEAD, and records it on the shared store (for App\'s breadcrumb)', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    const older = wholeFileHit({ msgId: 'msg_old', sha: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', ts: '2026-07-10T09:00:00Z' });
    const newest = wholeFileHit({ msgId: 'msg_new', sha: '6c513dcaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', ts: '2026-07-17T15:00:00Z' });
    vi.mocked(api.getRefs).mockResolvedValue([older, newest]);

    render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() =>
      expect(api.getCode).toHaveBeenCalledWith(
        'agent-coord',
        'wealdlore',
        'Sources/Reader/PlateBundle.swift',
        '6c513dcaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
      )
    );
    expect(codeState.forHub('agent-coord').codeSha).toBe('6c513dcaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');
  });

  it('falls back to HEAD when there are no hits at all (unchanged behavior)', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() =>
      expect(api.getCode).toHaveBeenCalledWith('agent-coord', 'wealdlore', 'Sources/Reader/PlateBundle.swift', 'HEAD')
    );
  });

  it('empty-state: a dangling pinned-sha ref (not in the local clone) gets a precise message, not the generic HEAD one', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    // getCode resolves nothing at that sha — as it would for a real dangling ref.
    vi.mocked(api.getCode).mockResolvedValue({ lang: 'swift', staleness: 'unknown', lines: [] });
    const dangling = wholeFileHit({
      sha: '6c513dcaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
      staleness: 'unknown',
    });
    vi.mocked(api.getRefs).mockResolvedValue([dangling]);

    render(CodeLens, { hub: 'agent-coord' });

    expect(await screen.findByText("Referenced revision isn't in your clone")).toBeInTheDocument();
    expect(screen.getByText(/6c513dcaa.*isn't in your local clone of wealdlore/)).toBeInTheDocument();
  });

  it('empty-state: a "moved" ref (path renamed since the pin) is distinguished from an unresolvable sha', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue({ lang: 'swift', staleness: 'moved', lines: [] });
    const moved = wholeFileHit({
      sha: 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeef',
      staleness: 'moved',
    });
    vi.mocked(api.getRefs).mockResolvedValue([moved]);

    render(CodeLens, { hub: 'agent-coord' });

    expect(await screen.findByText('Referenced path not found at that revision')).toBeInTheDocument();
    expect(screen.getByText(/moved or renamed since/)).toBeInTheDocument();
  });

  it('empty-state: no hits at all keeps the original generic "no content at HEAD" message', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue({ lang: 'swift', staleness: 'unknown', lines: [] });
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeLens, { hub: 'agent-coord' });

    expect(await screen.findByText('No code returned')).toBeInTheDocument();
  });

  it('the no-referenced-code empty-state copy invites a repo, a file, or a line range — not just a line range', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue([]);

    render(CodeLens, { hub: 'confer-jarvis-orbit' });

    expect(await screen.findByText(/references this repo — a file, a line range, or the repo itself/)).toBeInTheDocument();
  });
});

describe('CodeLens — piece 11 Phase 2: the powered gutter', () => {
  const readerAgent: Agent = {
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

  it('shape = scope: the file-lane renders for whole-file hits, clicking it fires onOpenRefs with range:null', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    const wholeFile = wholeFileHit();
    vi.mocked(api.getRefs).mockResolvedValue([wholeFile]);
    const onOpenRefs = vi.fn();

    render(CodeLens, { hub: 'agent-coord', onOpenRefs });

    const lane = await screen.findByTestId('file-lane');
    expect(lane).toHaveTextContent('1 conversation');
    // Whole-file hits never light a line's gutter — no bracket/tick.
    expect(document.querySelectorAll('.br, .tick').length).toBe(0);

    const user = userEvent.setup();
    await user.click(lane);
    expect(onOpenRefs).toHaveBeenCalledWith(
      { repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: null },
      [wholeFile]
    );
  });

  it('no file-lane at all when there are no whole-file hits', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([hitOn(44)]);

    render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(screen.getByTestId('gutter-tab')).toBeInTheDocument());
    expect(screen.queryByTestId('file-lane')).not.toBeInTheDocument();
  });

  it('shape = scope: a single-line hit is a TICK, a multi-line hit is a BRACKET (both/either real, never guessed)', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([hitOn(44), { ...hitOn(45), range: [44, 46] }]);

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(container.querySelectorAll('[data-testid="gutter-tab"]').length).toBe(2));
    expect(container.querySelector('.tick')).toBeInTheDocument();
    expect(container.querySelector('.br')).toBeInTheDocument();
  });

  it('column = overlap: two DIFFERENT overlapping ranges render two distinct bracket segments on a shared line, not one blurred together', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([
      { ...hitOn(44), msgId: 'a', range: [44, 45] },
      { ...hitOn(45), msgId: 'b', range: [45, 46] },
    ]);

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(container.querySelectorAll('[data-testid="gutter-tab"]').length).toBe(2));
    // Line 45 is covered by BOTH ranges — two separate bracket segments,
    // one per column, not a single merged one.
    const line45 = [...container.querySelectorAll('.cl')].find((cl) => cl.querySelector('.ln')?.textContent === '45')!;
    expect(line45.querySelectorAll('.br').length).toBe(2);
  });

  it('law #3: the drift marker shows ONLY when a real hit has staleness "changed" — never decorative', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([{ ...hitOn(44), range: [44, 45], staleness: 'changed' }]);

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(container.querySelector('[data-testid="gutter-tab"]')).toBeInTheDocument());
    expect(container.querySelector('.br.drift')).toBeInTheDocument();
    expect(container.querySelector('.tab.drift')).toBeInTheDocument();
    expect(screen.getByTestId('gutter-tab').textContent).toContain('◷');
  });

  it('law #3: a single-line (TICK) hit shows its own drift treatment too — a dashed outline, not a bracket edge', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([{ ...hitOn(44), staleness: 'changed' as const }]);

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(container.querySelector('[data-testid="gutter-tab"]')).toBeInTheDocument());
    expect(container.querySelector('.tick.drift')).toBeInTheDocument();
    expect(container.querySelector('.br.drift')).not.toBeInTheDocument();
  });

  it('law #3: an ordinary current hit shows NO drift marker', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([{ ...hitOn(44), range: [44, 45], staleness: 'current' }]);

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(container.querySelector('[data-testid="gutter-tab"]')).toBeInTheDocument());
    expect(container.querySelector('.br.drift')).not.toBeInTheDocument();
    expect(container.querySelector('.tab.drift')).not.toBeInTheDocument();
  });

  it('color = meaning: resolves a real agent\'s initials in the tab when `agents` is passed', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([hitOn(44)]);

    const { container } = render(CodeLens, { hub: 'agent-coord', agents: [readerAgent] });

    await waitFor(() => expect(container.querySelector('[data-testid="gutter-tab"]')).toBeInTheDocument());
    const tab = container.querySelector('[data-testid="gutter-tab"]') as HTMLElement;
    expect(tab.textContent).toBe('1 · RE');
    expect(tab.getAttribute('title')).toContain('Reader');
  });
});

describe('CodeLens — repo rollup (design/44 §6 item 2.4)', () => {
  it('renders a grouped-by-file rollup when codeState.viewMode is "repo", fetching via getRefs(hub, repo)', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getRefs).mockResolvedValue([
      hitOn(44),
      hitOn(45),
      hitOn(1, 'pipeline/plates.py'),
    ]);

    render(CodeLens, { hub: 'agent-coord' });
    codeState.forHub('agent-coord').activeRepo = 'wealdlore';
    codeState.forHub('agent-coord').viewMode = 'repo';

    expect(await screen.findByTestId('repo-rollup')).toBeInTheDocument();
    expect(api.getRefs).toHaveBeenCalledWith('agent-coord', 'wealdlore', true);
    expect(screen.getByText('Sources/Reader/PlateBundle.swift')).toBeInTheDocument();
    expect(screen.getByText('pipeline/plates.py')).toBeInTheDocument();
    expect(screen.getByText('2 refs')).toBeInTheDocument();
    expect(screen.getByText('1 ref')).toBeInTheDocument();
  });

  it('clicking a rollup row drills into that file (sets activeKey + viewMode back to "file")', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([hitOn(1, 'pipeline/plates.py')]);

    render(CodeLens, { hub: 'agent-coord' });
    codeState.forHub('agent-coord').activeRepo = 'wealdlore';
    codeState.forHub('agent-coord').viewMode = 'repo';

    const row = await screen.findByText('pipeline/plates.py');
    const user = userEvent.setup();
    await user.click(row);

    expect(codeState.forHub('agent-coord').viewMode).toBe('file');
    expect(codeState.forHub('agent-coord').activeKey).toBe(fileKey({ repo: 'wealdlore', path: 'pipeline/plates.py' }));
    // Let the now-triggered file-load effect settle before the test (and its
    // mocks) tear down — otherwise the in-flight api.getCode promise resolves
    // after a later test's beforeEach has reset the mock.
    await waitFor(() => expect(api.getCode).toHaveBeenCalledWith('agent-coord', 'wealdlore', 'pipeline/plates.py', 'a3f1c9'));
  });

  it('shows a repo-scoped empty state when the repo has no hits', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    render(CodeLens, { hub: 'agent-coord' });
    codeState.forHub('agent-coord').activeRepo = 'wealdlore';
    codeState.forHub('agent-coord').viewMode = 'repo';

    expect(await screen.findByText('No conversations reference this repo yet')).toBeInTheDocument();
  });
});
