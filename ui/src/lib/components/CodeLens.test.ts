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

  it('post-verify fix — a mixed-type entry (a resolved `done` hit alongside an open note) does NOT render green "in-flight"', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    // Same identical range, two hits: a resolved `done` (would be
    // state-flight GREEN under the OLD piece-9-reused mapping) and a
    // note (state-metric teal, and the more-actionable of the two per the
    // priority order since 'note' outranks 'resolved'). The entry must
    // read teal, never the done hit's green.
    vi.mocked(api.getRefs).mockResolvedValue([{ ...hitOn(44), msgType: 'done' }, { ...hitOn(44), msgId: 'msg_44b', msgType: 'note' }]);

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(container.querySelector('[data-testid="gutter-tab"]')).toBeInTheDocument());
    const tick = container.querySelector('.tick') as HTMLElement;
    expect(tick.style.getPropertyValue('--bc')).toBe('var(--state-metric)');
  });

  describe('post-verify fix — the active-range highlight (`activeScope`)', () => {
    it('the range tab and its bracket/tick show `.act` when `activeScope` matches this file + range exactly', async () => {
      vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
      vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
      vi.mocked(api.getRefs).mockResolvedValue([hitOn(44)]);

      const { container } = render(CodeLens, {
        hub: 'agent-coord',
        activeScope: { repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [44, 44] },
      });

      await waitFor(() => expect(container.querySelector('[data-testid="gutter-tab"]')).toBeInTheDocument());
      expect(container.querySelector('[data-testid="gutter-tab"]')).toHaveClass('act');
      expect(container.querySelector('.tick')).toHaveClass('act');
    });

    it('NO `.act` when activeScope names a different range in the same file', async () => {
      vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
      vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
      vi.mocked(api.getRefs).mockResolvedValue([hitOn(44)]);

      const { container } = render(CodeLens, {
        hub: 'agent-coord',
        activeScope: { repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: [45, 45] },
      });

      await waitFor(() => expect(container.querySelector('[data-testid="gutter-tab"]')).toBeInTheDocument());
      expect(container.querySelector('[data-testid="gutter-tab"]')).not.toHaveClass('act');
    });

    it('NO `.act` when activeScope names a DIFFERENT file entirely', async () => {
      vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
      vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
      vi.mocked(api.getRefs).mockResolvedValue([hitOn(44)]);

      const { container } = render(CodeLens, {
        hub: 'agent-coord',
        activeScope: { repo: 'wealdlore', path: 'pipeline/plates.py', range: [44, 44] },
      });

      await waitFor(() => expect(container.querySelector('[data-testid="gutter-tab"]')).toBeInTheDocument());
      expect(container.querySelector('[data-testid="gutter-tab"]')).not.toHaveClass('act');
    });

    it('the file-lane shows `.act` when activeScope is this file with range:null (whole-file scope open)', async () => {
      vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
      vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
      vi.mocked(api.getRefs).mockResolvedValue([wholeFileHit()]);

      const { container } = render(CodeLens, {
        hub: 'agent-coord',
        activeScope: { repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', range: null },
      });

      await waitFor(() => expect(container.querySelector('[data-testid="file-lane"]')).toBeInTheDocument());
      expect(container.querySelector('[data-testid="file-lane"]')).toHaveClass('act');
    });

    it('no activeScope at all (reader closed) means nothing is active', async () => {
      vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
      vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
      vi.mocked(api.getRefs).mockResolvedValue([hitOn(44), wholeFileHit()]);

      const { container } = render(CodeLens, { hub: 'agent-coord' });

      await waitFor(() => expect(container.querySelector('[data-testid="gutter-tab"]')).toBeInTheDocument());
      expect(container.querySelector('[data-testid="gutter-tab"]')).not.toHaveClass('act');
      expect(container.querySelector('[data-testid="file-lane"]')).not.toHaveClass('act');
    });
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

describe('CodeLens — piece 11 Phase 2b: the conversation minimap', () => {
  beforeEach(() => {
    // jsdom has no real layout engine and doesn't implement scrollIntoView
    // at all — stub it so click-to-scroll is observable.
    Element.prototype.scrollIntoView = vi.fn();
  });

  it('renders one minimap segment per gutter entry, colored by the SAME entryColorVar the gutter itself uses', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    const noteHit = hitOn(44); // msgType 'note' -> --state-metric
    const blockedHit = { ...hitOn(46), msgType: 'request' as const, requestStatus: 'BLOCKED' as const }; // -> --state-stuck
    vi.mocked(api.getRefs).mockResolvedValue([noteHit, blockedHit]);

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(container.querySelectorAll('[data-testid="minimap-segment"]').length).toBe(2));
    const bcValues = Array.from(container.querySelectorAll<HTMLElement>('[data-testid="minimap-segment"]')).map((s) =>
      s.style.getPropertyValue('--bc').trim()
    );
    expect(bcValues).toContain('var(--state-metric)');
    expect(bcValues).toContain('var(--state-stuck)');
  });

  it('law #3 — no minimap at all when the file has no ranged conversations (never a fabricated empty strip)', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([wholeFileHit()]); // whole-file only, no ranged hits

    render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(screen.getByTestId('file-lane')).toBeInTheDocument());
    expect(screen.queryByTestId('code-minimap')).not.toBeInTheDocument();
  });

  it('click-to-scroll: clicking a segment scrolls its entry\'s start line into view, not just anywhere', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([hitOn(45)]);

    const { container } = render(CodeLens, { hub: 'agent-coord' });

    await waitFor(() => expect(container.querySelector('[data-testid="minimap-segment"]')).toBeInTheDocument());
    const user = userEvent.setup();
    await user.click(container.querySelector('[data-testid="minimap-segment"]') as HTMLElement);

    const line45 = container.querySelector('[data-line="45"]');
    expect(line45).not.toBeNull();
    expect(vi.mocked(Element.prototype.scrollIntoView)).toHaveBeenCalledTimes(1);
    expect(vi.mocked(Element.prototype.scrollIntoView).mock.instances[0]).toBe(line45);
  });
});

describe('CodeLens — piece 11 Phase 3: PR-style collapse', () => {
  // A 40-line file with two far-apart single-hit ranges — real top, middle,
  // AND bottom gaps in one fixture (context=2 around each hit):
  //   entry [5,6]  -> open span [3,8]
  //   entry [30,31] -> open span [28,33]
  //   gap [1,2]   edge=top    (2 lines)
  //   gap [9,27]  edge=middle (19 lines)
  //   gap [34,40] edge=bottom (7 lines)
  const longSnippet: Snippet = {
    lang: 'swift',
    staleness: 'current',
    lines: Array.from({ length: 40 }, (_, i) => ({ n: i + 1, text: `line ${i + 1}` })),
  };

  function rangeHit(range: [number, number], msgId: string): RefHit {
    return { ...hitOn(range[0]), msgId, range };
  }

  const longFileFixture = [rangeHit([5, 6], 'msg_a'), rangeHit([30, 31], 'msg_b')];

  async function renderCollapsed() {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(longSnippet);
    vi.mocked(api.getRefs).mockResolvedValue(longFileFixture);
    const { container } = render(CodeLens, { hub: 'agent-coord' });
    await waitFor(() => expect(container.querySelectorAll('[data-testid="fold-row"]')).toHaveLength(3));
    return container;
  }

  function visibleLineNumbers(container: HTMLElement): number[] {
    return Array.from(container.querySelectorAll<HTMLElement>('[data-line]')).map((el) => Number(el.getAttribute('data-line')));
  }

  it('defaults to "referenced": only referenced ranges + context are visible, three real folds for top/middle/bottom', async () => {
    const container = await renderCollapsed();

    const visible = visibleLineNumbers(container);
    expect(visible).toEqual([3, 4, 5, 6, 7, 8, 28, 29, 30, 31, 32, 33]);

    const folds = container.querySelectorAll<HTMLElement>('[data-testid="fold-row"]');
    expect(folds[0]!.getAttribute('title')).toBe('lines 1–2');
    expect(folds[0]!.textContent).toContain('expand 2 lines');
    expect(folds[1]!.getAttribute('title')).toBe('lines 9–27');
    expect(folds[1]!.textContent).toContain('expand 19 lines');
    expect(folds[2]!.getAttribute('title')).toBe('lines 34–40');
    expect(folds[2]!.textContent).toContain('expand 7 lines');
  });

  it('gutter line numbers stay real and correct across a fold — never renumbered', async () => {
    // Line 30 (inside the second open span) keeps reading "30", not "13" or
    // any other position-in-DOM count — the whole point of never rendering
    // collapsed lines instead of hiding them.
    const container = await renderCollapsed();
    const row = container.querySelector('[data-line="30"]');
    expect(row?.querySelector('.ln')?.textContent).toBe('30');
  });

  it('a top-edge fold has ONE button ("↑ top") that fully reveals in one click', async () => {
    const container = await renderCollapsed();
    const folds = container.querySelectorAll<HTMLElement>('[data-testid="fold-row"]');
    const topFold = folds[0]!;
    expect(topFold.querySelectorAll('[data-testid="fold-expand-up"], [data-testid="fold-expand-down"], [data-testid="fold-expand-all"]')).toHaveLength(0);
    const edgeBtn = topFold.querySelector<HTMLElement>('[data-testid="fold-expand-edge"]');
    expect(edgeBtn?.textContent).toBe('↑ top');

    const user = userEvent.setup();
    await user.click(edgeBtn!);

    await waitFor(() => expect(container.querySelectorAll('[data-testid="fold-row"]')).toHaveLength(2));
    expect(visibleLineNumbers(container)).toEqual(expect.arrayContaining([1, 2]));
  });

  it('a bottom-edge fold\'s single "↓ bottom" button fully reveals it too', async () => {
    const container = await renderCollapsed();
    const folds = container.querySelectorAll<HTMLElement>('[data-testid="fold-row"]');
    const bottomFold = folds[2]!;
    const edgeBtn = bottomFold.querySelector<HTMLElement>('[data-testid="fold-expand-edge"]');
    expect(edgeBtn?.textContent).toBe('↓ bottom');

    const user = userEvent.setup();
    await user.click(edgeBtn!);

    await waitFor(() => expect(visibleLineNumbers(container)).toEqual(expect.arrayContaining([34, 40])));
    expect(container.querySelectorAll('[data-testid="fold-row"]')).toHaveLength(2);
  });

  it('a middle gap has all three buttons (↑8, ↓8, ⤢all) and ↑8 reveals exactly 8 lines from the top, shrinking the fold', async () => {
    const container = await renderCollapsed();
    const folds = container.querySelectorAll<HTMLElement>('[data-testid="fold-row"]');
    const middleFold = folds[1]!;
    expect(middleFold.querySelector('[data-testid="fold-expand-up"]')?.textContent).toBe('↑ 8');
    expect(middleFold.querySelector('[data-testid="fold-expand-down"]')?.textContent).toBe('↓ 8');
    expect(middleFold.querySelector('[data-testid="fold-expand-all"]')?.textContent).toBe('⤢ all');

    const user = userEvent.setup();
    await user.click(middleFold.querySelector('[data-testid="fold-expand-up"]')!);

    // Lines 9-16 (8 lines) now real rows; the SAME fold row shrinks to
    // "expand 11 lines" (17-27) instead of a second fold appearing.
    await waitFor(() => expect(visibleLineNumbers(container)).toEqual(expect.arrayContaining([9, 10, 11, 12, 13, 14, 15, 16])));
    expect(container.querySelectorAll('[data-testid="fold-row"]')).toHaveLength(3);
    const shrunk = container.querySelectorAll<HTMLElement>('[data-testid="fold-row"]')[1]!;
    expect(shrunk.getAttribute('title')).toBe('lines 17–27');
    expect(shrunk.textContent).toContain('expand 11 lines');
  });

  it('↑8 then ↓8 on the same middle gap chip away from both edges without double-counting or overlapping', async () => {
    const container = await renderCollapsed();
    const user = userEvent.setup();
    let middleFold = container.querySelectorAll<HTMLElement>('[data-testid="fold-row"]')[1]!;
    await user.click(middleFold.querySelector('[data-testid="fold-expand-up"]')!); // hidden 17-27
    middleFold = container.querySelectorAll<HTMLElement>('[data-testid="fold-row"]')[1]!;
    await user.click(middleFold.querySelector('[data-testid="fold-expand-down"]')!); // reveal 20-27 -> hidden 17-19

    await waitFor(() => {
      middleFold = container.querySelectorAll<HTMLElement>('[data-testid="fold-row"]')[1]!;
      expect(middleFold.getAttribute('title')).toBe('lines 17–19');
    });
    expect(visibleLineNumbers(container)).toEqual(expect.arrayContaining([9, 10, 11, 12, 13, 14, 15, 16, 20, 21, 22, 23, 24, 25, 26, 27]));
  });

  it('⤢all on the middle gap reveals it entirely in one click — no fold row left for it', async () => {
    const container = await renderCollapsed();
    const middleFold = container.querySelectorAll<HTMLElement>('[data-testid="fold-row"]')[1]!;
    const user = userEvent.setup();
    await user.click(middleFold.querySelector('[data-testid="fold-expand-all"]')!);

    await waitFor(() => expect(container.querySelectorAll('[data-testid="fold-row"]')).toHaveLength(2));
    expect(visibleLineNumbers(container)).toEqual(expect.arrayContaining(Array.from({ length: 19 }, (_, i) => i + 9)));
  });

  it('"show all" reveals every real line and hides every fold row; "referenced" re-collapses', async () => {
    const container = await renderCollapsed();
    const user = userEvent.setup();

    await user.click(screen.getByTestId('collapse-toggle-showall'));
    await waitFor(() => expect(container.querySelectorAll('[data-testid="fold-row"]')).toHaveLength(0));
    expect(visibleLineNumbers(container)).toEqual(Array.from({ length: 40 }, (_, i) => i + 1));
    expect(screen.getByTestId('collapse-toggle-showall')).toHaveClass('on');

    await user.click(screen.getByTestId('collapse-toggle-referenced'));
    await waitFor(() => expect(container.querySelectorAll('[data-testid="fold-row"]')).toHaveLength(3));
    expect(screen.getByTestId('collapse-toggle-referenced')).toHaveClass('on');
  });

  it('the collapse toggle is hidden entirely when there\'s nothing to collapse (no gaps)', async () => {
    // plateBundleSnippet's 3 lines are all within [44,46]'s own context —
    // no gap exists, so a toggle with no effect would just be clutter.
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([hitOn(45)]);

    render(CodeLens, { hub: 'agent-coord' });
    await waitFor(() => expect(screen.getByTestId('gutter-tab')).toBeInTheDocument());
    expect(screen.queryByTestId('collapse-toggle')).not.toBeInTheDocument();
  });

  it('a file with zero ranged conversations shows every line — collapse never kicks in with nothing to anchor around', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(longSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([]);

    const { container } = render(CodeLens, { hub: 'agent-coord' });
    await waitFor(() => expect(container.querySelectorAll('.cl')).toHaveLength(40));
    expect(container.querySelectorAll('[data-testid="fold-row"]')).toHaveLength(0);
  });

  it('switching to a different file resets collapse state — no carried-over reveals or "show all"', async () => {
    const container = await renderCollapsed();
    const user = userEvent.setup();
    await user.click(screen.getByTestId('collapse-toggle-showall'));
    await waitFor(() => expect(container.querySelectorAll('[data-testid="fold-row"]')).toHaveLength(0));

    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([hitOn(45, 'pipeline/plates.py')]);
    codeState.forHub('agent-coord').activeKey = fileKey(threeFiles[1]!);

    await waitFor(() => expect(vi.mocked(api.getCode)).toHaveBeenLastCalledWith('agent-coord', 'wealdlore', 'pipeline/plates.py', 'a3f1c9'));
    // The new file has no gaps at all (fixture is fully covered by context),
    // so the absence of a toggle here also confirms showAll didn't leak.
    expect(screen.queryByTestId('collapse-toggle')).not.toBeInTheDocument();
  });
});

describe('CodeLens — piece 11 Phase 5: align to revision (codeState.pinnedSha)', () => {
  it("setting pinnedSha (the timeline's align action) re-fetches getCode at that sha, without re-fetching getRefs", async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([hitOn(44), hitOn(45)]);

    render(CodeLens, { hub: 'agent-coord' });
    await waitFor(() => expect(api.getCode).toHaveBeenCalledTimes(1));
    expect(api.getRefs).toHaveBeenCalledTimes(1);

    codeState.forHub('agent-coord').pinnedSha = 'deadbeef01';

    await waitFor(() => expect(api.getCode).toHaveBeenCalledTimes(2));
    expect(api.getCode).toHaveBeenLastCalledWith('agent-coord', 'wealdlore', 'Sources/Reader/PlateBundle.swift', 'deadbeef01');
    // The ref list itself hasn't changed — only which historical content is
    // displayed — so getRefs must NOT have been called again.
    expect(api.getRefs).toHaveBeenCalledTimes(1);
    // This is what drives Phase 4's rev chip (App.svelte reads codeSha
    // straight off this same store) — confirms the linkage without needing
    // App.svelte in this test.
    expect(codeState.forHub('agent-coord').codeSha).toBe('deadbeef01');
  });

  it('switching files resets pinnedSha — an alignment never carries over to a DIFFERENT file', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([hitOn(44)]);

    render(CodeLens, { hub: 'agent-coord' });
    await waitFor(() => expect(api.getCode).toHaveBeenCalledTimes(1));

    codeState.forHub('agent-coord').pinnedSha = 'deadbeef01';
    await waitFor(() => expect(codeState.forHub('agent-coord').codeSha).toBe('deadbeef01'));

    codeState.forHub('agent-coord').activeKey = fileKey(threeFiles[1]!); // pipeline/plates.py — no hits in this fixture
    await waitFor(() => expect(api.getCode).toHaveBeenLastCalledWith('agent-coord', 'wealdlore', 'pipeline/plates.py', 'HEAD'));
    expect(codeState.forHub('agent-coord').pinnedSha).toBeNull();
  });

  it('setting pinnedSha to the sha already rendering is a no-op — no extra fetch', async () => {
    vi.mocked(api.getCodeFiles).mockResolvedValue(threeFiles);
    vi.mocked(api.getCode).mockResolvedValue(plateBundleSnippet);
    vi.mocked(api.getRefs).mockResolvedValue([hitOn(44)]);

    render(CodeLens, { hub: 'agent-coord' });
    await waitFor(() => expect(api.getCode).toHaveBeenCalledTimes(1));
    const currentSha = codeState.forHub('agent-coord').codeSha;

    codeState.forHub('agent-coord').pinnedSha = currentSha;
    await waitFor(() => expect(codeState.forHub('agent-coord').pinnedSha).toBe(currentSha));
    expect(api.getCode).toHaveBeenCalledTimes(1);
  });
});
