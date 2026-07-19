// Piece 7 (ui/REDESIGN.md, `redesign-mockups/07-repos-integrity-gravity.html`)
// — pure fold behind the Repos view's two real questions: INTEGRITY (is
// every referenced repo registered and cloned so refs resolve?) and
// GRAVITY (which repos does the fleet actually talk about?). Same
// convention as boardStats.ts/thread.ts — kept out of Repos.svelte so the
// tiering/shadow-detection logic is unit-testable without mounting Svelte.
//
// Law #3: every number here folds from data already served —
// `getRepos()` (the hub's registered inventory) cross-referenced against
// `getCodeFiles()` (every repo:path a `--ref` has actually pointed at).
// SHADOW detection is a pure diff of those two real lists, not a fetch of
// its own. No new backend field needed.
import type { CodeFile, Repo } from './types';

export type RepoTier = 'tracked' | 'notlocal' | 'shadow';

export interface RepoIndexEntry {
  slug: string;
  tier: RepoTier;
  /** `null` for a shadow repo — it has no inventory card to read a role
   * from (that's the whole point: the hub doesn't know about it yet). */
  role: string | null;
  url: string | null;
  access: string[];
  cloned: boolean;
  clonePath: string | null;
  /** Sum of every referenced file's `refCount` in this repo — the
   * "gravity" number (how much the fleet actually talks about it). */
  refCount: number;
  /** Per-file ref counts, desc — the density sparkline's real data,
   * capped to the top few files (the visual is a glance, not a report). */
  topFileCounts: number[];
}

export interface RepoHealth {
  registeredCount: number;
  trackedCount: number;
  shadowCount: number;
  /** Registered but not cloned here — refs stay pointer-only. */
  notLocalCount: number;
}

/** Builds the tiered repo index — tracked (registered + cloned) first,
 * then registered-not-local, then shadow (referenced but unregistered) —
 * each group sorted by real reference gravity (`refCount` desc, hottest
 * first), matching the mockup's own ordering. */
export function buildRepoIndex(repos: Repo[], codeFiles: CodeFile[]): RepoIndexEntry[] {
  const filesByRepo = new Map<string, CodeFile[]>();
  for (const f of codeFiles) {
    const arr = filesByRepo.get(f.repo) ?? [];
    arr.push(f);
    filesByRepo.set(f.repo, arr);
  }

  function densityFor(slug: string): { refCount: number; topFileCounts: number[] } {
    const files = filesByRepo.get(slug) ?? [];
    const counts = files.map((f) => f.refCount).sort((a, b) => b - a);
    return { refCount: counts.reduce((sum, c) => sum + c, 0), topFileCounts: counts.slice(0, 5) };
  }

  const registeredSlugs = new Set(repos.map((r) => r.slug));

  const known: RepoIndexEntry[] = repos.map((r) => {
    const { refCount, topFileCounts } = densityFor(r.slug);
    return {
      slug: r.slug,
      tier: r.cloned ? 'tracked' : 'notlocal',
      role: r.role,
      url: r.url,
      access: r.access,
      cloned: r.cloned,
      clonePath: r.clonePath,
      refCount,
      topFileCounts,
    };
  });

  const shadowSlugs = [...filesByRepo.keys()].filter((slug) => !registeredSlugs.has(slug));
  const shadow: RepoIndexEntry[] = shadowSlugs.map((slug) => {
    const { refCount, topFileCounts } = densityFor(slug);
    return { slug, tier: 'shadow', role: null, url: null, access: [], cloned: false, clonePath: null, refCount, topFileCounts };
  });

  const byTier = (tier: RepoTier) => [...known, ...shadow].filter((e) => e.tier === tier).sort((a, b) => b.refCount - a.refCount || a.slug.localeCompare(b.slug));

  return [...byTier('tracked'), ...byTier('notlocal'), ...byTier('shadow')];
}

export function repoHealth(index: RepoIndexEntry[]): RepoHealth {
  return {
    registeredCount: index.filter((e) => e.tier !== 'shadow').length,
    trackedCount: index.filter((e) => e.tier === 'tracked').length,
    shadowCount: index.filter((e) => e.tier === 'shadow').length,
    notLocalCount: index.filter((e) => e.tier === 'notlocal').length,
  };
}
