import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import Repos from './Repos.svelte';
import type { CodeFile, Repo, RefHit } from '../types';
import { api } from '../api';

vi.mock('../api', () => ({
  api: {
    getRepos: vi.fn(),
    getCodeFiles: vi.fn().mockResolvedValue([]),
    getRefs: vi.fn().mockResolvedValue([]),
  },
}));

const clonedRepo: Repo = {
  slug: 'confer',
  role: 'tooling',
  url: 'github.com/codeshrew/confer',
  access: [],
  docs: 'design/',
  owner: 'sk',
  cloned: true,
  clonePath: '~/git/confer',
  rootSha: 'a3f1c9d',
};

const notClonedRepo: Repo = {
  slug: 'lelas-knowledge-base',
  role: 'shared',
  url: 'github.com/codeshrew/lelas-knowledge-base',
  access: ['reader', 'gitconv'],
  docs: null,
  owner: null,
  cloned: false,
  clonePath: null,
  rootSha: null,
};

const files: CodeFile[] = [
  { repo: 'confer', path: 'src/api.rs', refCount: 9, mapped: true, lastTs: '2026-07-18T10:00:00Z' },
  { repo: 'confer', path: 'src/patch.rs', refCount: 7, mapped: true, lastTs: '2026-07-18T09:00:00Z' },
  // openjarvis is referenced but never registered — a shadow repo.
  { repo: 'openjarvis', path: 'main.go', refCount: 2, mapped: false, lastTs: '2026-07-17T10:00:00Z' },
];

describe('Repos — tiered integrity groups', () => {
  it('groups a registered+cloned repo as tracked, registered-not-local separately, and an unregistered --ref as shadow', async () => {
    vi.mocked(api.getRepos).mockResolvedValue([clonedRepo, notClonedRepo]);
    vi.mocked(api.getCodeFiles).mockResolvedValue(files);

    render(Repos, { hub: 'agent-coord' });
    await waitFor(() => expect(screen.getByTestId('repos-view')).toBeInTheDocument());

    expect(await screen.findByText('✓ tracked')).toBeInTheDocument();
    expect(screen.getByTestId('repo-row-confer')).toHaveTextContent('confer');
    expect(screen.getByTestId('repo-row-confer')).toHaveTextContent('16 refs'); // 9 + 7

    expect(screen.getByText('◑ registered · not on this machine')).toBeInTheDocument();
    expect(screen.getByTestId('repo-row-lelas-knowledge-base')).toBeInTheDocument();

    expect(screen.getByText('◇ shadow · referenced, not registered')).toBeInTheDocument();
    expect(screen.getByTestId('repo-row-openjarvis')).toHaveTextContent('2 refs');
  });

  it('shows a real health line naming the gaps, not a fabricated "all clear"', async () => {
    vi.mocked(api.getRepos).mockResolvedValue([clonedRepo, notClonedRepo]);
    vi.mocked(api.getCodeFiles).mockResolvedValue(files);
    render(Repos, { hub: 'agent-coord' });

    expect(await screen.findByText(/1 not registered/)).toBeInTheDocument();
    expect(screen.getByText(/1 not cloned/)).toBeInTheDocument();
  });

  it('an all-tracked hub shows the calm "all mapped" line honestly, not omitted', async () => {
    vi.mocked(api.getRepos).mockResolvedValue([clonedRepo]);
    vi.mocked(api.getCodeFiles).mockResolvedValue([]);
    render(Repos, { hub: 'agent-coord' });

    expect(await screen.findByText('✓ all mapped')).toBeInTheDocument();
  });

  it('an empty hub shows the empty state', async () => {
    vi.mocked(api.getRepos).mockResolvedValue([]);
    vi.mocked(api.getCodeFiles).mockResolvedValue([]);
    render(Repos, { hub: 'confer-lab' });

    expect(await screen.findByText('No repos registered')).toBeInTheDocument();
  });
});

describe('Repos — drill-in', () => {
  const hit: RefHit = {
    repo: 'confer',
    path: 'src/api.rs',
    sha: 'abc123',
    range: null,
    contentHash: null,
    staleness: 'current',
    msgId: 'msg_1',
    from: 'herald',
    msgType: 'note',
    ts: '2026-07-18T10:00:00Z',
    topic: 'review-080',
    summary: 'ref',
    threadRoot: 'msg_1',
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

  it('clicking a repo opens the detail popover with real hot files from a single getRefs(hub, repo, true) call', async () => {
    const user = userEvent.setup();
    vi.mocked(api.getRepos).mockResolvedValue([clonedRepo]);
    vi.mocked(api.getCodeFiles).mockResolvedValue(files);
    vi.mocked(api.getRefs).mockResolvedValue([hit]);

    render(Repos, { hub: 'agent-coord' });
    await user.click(await screen.findByTestId('repo-row-confer'));

    expect(await screen.findByTestId('repo-detail-popover')).toBeInTheDocument();
    expect(api.getRefs).toHaveBeenCalledWith('agent-coord', 'confer', true);
    expect(await screen.findByTestId('repo-hot-file')).toHaveTextContent('src/api.rs');
  });

  it('"open in code view" fires onOpenCode with the repo (no path) and closes the popover', async () => {
    const user = userEvent.setup();
    const onOpenCode = vi.fn();
    vi.mocked(api.getRepos).mockResolvedValue([clonedRepo]);
    vi.mocked(api.getCodeFiles).mockResolvedValue(files);
    vi.mocked(api.getRefs).mockResolvedValue([hit]);

    render(Repos, { hub: 'agent-coord', onOpenCode });
    await user.click(await screen.findByTestId('repo-row-confer'));
    await user.click(await screen.findByRole('button', { name: /open in code view/ }));

    expect(onOpenCode).toHaveBeenCalledWith('confer', undefined);
    expect(screen.queryByTestId('repo-detail-popover')).not.toBeInTheDocument();
  });

  it('clicking a hot file fires onOpenCode with the specific path', async () => {
    const user = userEvent.setup();
    const onOpenCode = vi.fn();
    vi.mocked(api.getRepos).mockResolvedValue([clonedRepo]);
    vi.mocked(api.getCodeFiles).mockResolvedValue(files);
    vi.mocked(api.getRefs).mockResolvedValue([hit]);

    render(Repos, { hub: 'agent-coord', onOpenCode });
    await user.click(await screen.findByTestId('repo-row-confer'));
    await user.click(await screen.findByTestId('repo-hot-file'));

    expect(onOpenCode).toHaveBeenCalledWith('confer', 'src/api.rs');
  });

  it('Escape closes the popover', async () => {
    const user = userEvent.setup();
    vi.mocked(api.getRepos).mockResolvedValue([clonedRepo]);
    vi.mocked(api.getCodeFiles).mockResolvedValue(files);
    vi.mocked(api.getRefs).mockResolvedValue([hit]);

    render(Repos, { hub: 'agent-coord' });
    await user.click(await screen.findByTestId('repo-row-confer'));
    await screen.findByTestId('repo-detail-popover');

    await user.keyboard('{Escape}');
    expect(screen.queryByTestId('repo-detail-popover')).not.toBeInTheDocument();
  });
});
