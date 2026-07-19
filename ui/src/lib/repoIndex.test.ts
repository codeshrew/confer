import { describe, expect, it } from 'vitest';
import { buildRepoIndex, repoHealth } from './repoIndex';
import type { CodeFile, Repo } from './types';

function repo(overrides: Partial<Repo> = {}): Repo {
  return { slug: 'confer', role: 'tooling', url: 'github.com/codeshrew/confer', access: [], docs: null, owner: null, cloned: true, clonePath: '~/git/confer', rootSha: null, ...overrides };
}

function file(overrides: Partial<CodeFile> = {}): CodeFile {
  return { repo: 'confer', path: 'src/api.rs', refCount: 5, mapped: true, lastTs: '2026-07-18T10:00:00Z', ...overrides };
}

describe('buildRepoIndex', () => {
  it('a registered + cloned repo is tracked', () => {
    const index = buildRepoIndex([repo()], []);
    expect(index).toHaveLength(1);
    expect(index[0]!.tier).toBe('tracked');
  });

  it('a registered but uncloned repo is registered-not-local', () => {
    const index = buildRepoIndex([repo({ cloned: false, clonePath: null })], []);
    expect(index[0]!.tier).toBe('notlocal');
  });

  it('a repo referenced via --ref but never registered is shadow — a pure diff, no fetch of its own', () => {
    const files = [file({ repo: 'openjarvis', path: 'main.go', refCount: 2 })];
    const index = buildRepoIndex([], files);
    expect(index).toHaveLength(1);
    expect(index[0]).toMatchObject({ slug: 'openjarvis', tier: 'shadow', role: null, url: null, cloned: false, refCount: 2 });
  });

  it('a registered repo\'s refCount sums every referenced file\'s count, not just one', () => {
    const files = [file({ path: 'src/api.rs', refCount: 9 }), file({ path: 'src/patch.rs', refCount: 7 })];
    const index = buildRepoIndex([repo()], files);
    expect(index[0]!.refCount).toBe(16);
    expect(index[0]!.topFileCounts).toEqual([9, 7]);
  });

  it('a repo with no referenced files at all is honestly zero, not undefined', () => {
    const index = buildRepoIndex([repo()], []);
    expect(index[0]!.refCount).toBe(0);
    expect(index[0]!.topFileCounts).toEqual([]);
  });

  it('groups sort tracked, then registered-not-local, then shadow — hottest first within each', () => {
    const repos = [repo({ slug: 'quiet', cloned: true }), repo({ slug: 'loud', cloned: true }), repo({ slug: 'pointer-only', cloned: false, clonePath: null })];
    const files = [file({ repo: 'quiet', refCount: 1 }), file({ repo: 'loud', refCount: 50 }), file({ repo: 'shadow-repo', refCount: 3 })];
    const index = buildRepoIndex(repos, files);
    expect(index.map((e) => e.slug)).toEqual(['loud', 'quiet', 'pointer-only', 'shadow-repo']);
  });

  it('a repo appearing in both the registry AND codeFiles is never double-counted as shadow', () => {
    const index = buildRepoIndex([repo()], [file({ repo: 'confer' })]);
    expect(index).toHaveLength(1);
    expect(index[0]!.tier).toBe('tracked');
  });
});

describe('repoHealth', () => {
  it('counts each tier honestly, including the gap counts a flat list never surfaced', () => {
    const index = buildRepoIndex(
      [repo({ slug: 'tracked-1', cloned: true }), repo({ slug: 'notlocal-1', cloned: false, clonePath: null })],
      [file({ repo: 'shadow-1' })]
    );
    expect(repoHealth(index)).toEqual({ registeredCount: 2, trackedCount: 1, shadowCount: 1, notLocalCount: 1 });
  });

  it('an empty index is all zeros', () => {
    expect(repoHealth([])).toEqual({ registeredCount: 0, trackedCount: 0, shadowCount: 0, notLocalCount: 0 });
  });
});
