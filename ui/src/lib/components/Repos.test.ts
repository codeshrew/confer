import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import Repos from './Repos.svelte';
import type { Repo } from '../types';
import { api } from '../api';

vi.mock('../api', () => ({
  api: {
    getRepos: vi.fn(),
  },
}));

const clonedRepo: Repo = {
  slug: 'git-conversations',
  role: 'tooling',
  url: 'git@github.com:codeshrew/git-conversations.git',
  access: [],
  docs: 'design/',
  owner: 'sk',
  cloned: true,
  clonePath: '/Users/sk/git/git-conversations',
  rootSha: 'a3f1c9d',
};

const notClonedRepo: Repo = {
  slug: 'wealdlore',
  role: 'code',
  url: 'git@github.com:codeshrew/wealdlore.git',
  access: ['reader', 'gitconv'],
  docs: 'docs/',
  owner: 'sk',
  cloned: false,
  clonePath: null,
  rootSha: null,
};

describe('Repos', () => {
  it('renders a cloned repo with its clone status + path, and an uncloned repo with the map hint', async () => {
    vi.mocked(api.getRepos).mockResolvedValue([clonedRepo, notClonedRepo]);

    render(Repos, { hub: 'agent-coord' });

    await waitFor(() => expect(screen.getByTestId('repos-view')).toBeInTheDocument());

    // Cloned repo: slug, role chip, url, clone status + path + short sha.
    expect(await screen.findByText('git-conversations')).toBeInTheDocument();
    expect(screen.getByText('tooling')).toBeInTheDocument();
    expect(screen.getByText('git@github.com:codeshrew/git-conversations.git')).toBeInTheDocument();
    expect(screen.getByTestId('clone-status-git-conversations')).toHaveTextContent('✓ cloned');
    expect(screen.getByText('/Users/sk/git/git-conversations')).toBeInTheDocument();
    expect(screen.getByText('a3f1c9d')).toBeInTheDocument();

    // Not-cloned repo: muted status + the `confer repos map` hint, access list shown.
    expect(screen.getByText('wealdlore')).toBeInTheDocument();
    expect(screen.getByTestId('clone-status-wealdlore')).toHaveTextContent('not cloned here');
    expect(screen.getByText('confer repos map wealdlore <path>')).toBeInTheDocument();
    expect(screen.getByText('reader, gitconv')).toBeInTheDocument();
  });

  it('shows "all" for empty access and an empty state when there are no repos', async () => {
    vi.mocked(api.getRepos).mockResolvedValue([clonedRepo]);
    render(Repos, { hub: 'agent-coord' });

    expect(await screen.findByText('all')).toBeInTheDocument();

    vi.mocked(api.getRepos).mockResolvedValue([]);
    render(Repos, { hub: 'confer-lab' });

    expect(await screen.findByText('No repos registered')).toBeInTheDocument();
  });
});
