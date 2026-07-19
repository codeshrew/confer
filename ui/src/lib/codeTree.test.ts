import { describe, expect, it } from 'vitest';
import {
  activeFlatten,
  ancestorIdsFor,
  breadcrumbFromTree,
  buildTree,
  collapseBreadcrumb,
  computeDisambiguators,
  countVisibleRows,
  defaultExpandedIds,
  fileKey,
  filterFiles,
  groupRefHitsByFile,
  truncateMiddle,
} from './codeTree';
import type { CodeFile, RefHit } from './types';

function f(repo: string, path: string, overrides: Partial<CodeFile> = {}): CodeFile {
  return { repo, path, refCount: 1, mapped: true, lastTs: '2026-07-17T10:00:00Z', ...overrides };
}

function hit(path: string, ts: string, overrides: Partial<RefHit> = {}): RefHit {
  return {
    repo: 'wealdlore',
    path,
    sha: 'a3f1c9',
    range: null,
    contentHash: null,
    staleness: 'current',
    msgId: `msg_${path}_${ts}`,
    from: 'reader',
    msgType: 'note',
    ts,
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
    ...overrides,
  };
}

describe('buildTree — fold + path compaction', () => {
  it('groups files by repo, sorted A-Z', () => {
    const tree = buildTree([f('wealdlore', 'a.rs'), f('agent-coord', 'b.rs')]);
    expect(tree.map((r) => r.repo)).toEqual(['agent-coord', 'wealdlore']);
  });

  it('sums refCount up to the repo node', () => {
    const tree = buildTree([f('r', 'a.rs', { refCount: 3 }), f('r', 'b.rs', { refCount: 2 })]);
    expect(tree[0]!.refCount).toBe(5);
  });

  it('compacts a chain of single-child directories into one row', () => {
    const tree = buildTree([f('r', 'src/lib/components/CodeTree.svelte')]);
    // repo -> one dir child, compacted all the way to the file's parent.
    expect(tree[0]!.children).toHaveLength(1);
    const dir = tree[0]!.children[0]!;
    expect(dir.kind).toBe('dir');
    if (dir.kind === 'dir') {
      expect(dir.label).toBe('src/lib/components/');
      expect(dir.fullPath).toBe('src/lib/components');
      expect(dir.children).toHaveLength(1);
      expect(dir.children[0]!.kind).toBe('file');
    }
  });

  it('does NOT fold a directory together with its one file (only dir chains compact)', () => {
    const tree = buildTree([f('r', 'api/api.rs')]);
    const dir = tree[0]!.children[0]!;
    expect(dir.kind).toBe('dir');
    if (dir.kind === 'dir') {
      expect(dir.label).toBe('api/');
      expect(dir.children[0]!.kind).toBe('file');
    }
  });

  it('stops compaction at a branch point (multiple children)', () => {
    const tree = buildTree([f('r', 'src/lib/a.rs'), f('r', 'src/lib/b.rs'), f('r', 'src/other/c.rs')]);
    // src/ has two children (lib/, other/) so it can't fold past itself.
    const src = tree[0]!.children.find((n) => n.kind === 'dir' && n.label === 'src/');
    expect(src).toBeDefined();
    if (src && src.kind === 'dir') {
      expect(src.children).toHaveLength(2); // lib/, other/
    }
  });

  it('sorts dirs before files, both A-Z, within a level', () => {
    const tree = buildTree([f('r', 'zeta.rs'), f('r', 'alpha/x.rs')]);
    const names = tree[0]!.children.map((n) => (n.kind === 'file' ? n.name : n.kind === 'dir' ? n.label : ''));
    expect(names).toEqual(['alpha/', 'zeta.rs']);
  });
});

describe('defaultExpandedIds — cold-render policy', () => {
  it('expands every repo when there are <=2 total', () => {
    const files = [f('a', 'x.rs'), f('b', 'y.rs')];
    const tree = buildTree(files);
    const expanded = defaultExpandedIds(tree, null);
    expect(expanded.has('a')).toBe(true);
    expect(expanded.has('b')).toBe(true);
  });

  it('collapses repos by default once there are >2, except the one holding the active file', () => {
    const files = [f('a', 'x.rs'), f('b', 'y.rs'), f('c', 'z.rs')];
    const tree = buildTree(files);
    const expanded = defaultExpandedIds(tree, fileKey(files[1]!));
    expect(expanded.has('a')).toBe(false);
    expect(expanded.has('b')).toBe(true);
    expect(expanded.has('c')).toBe(false);
  });

  it('also expands the compacted dir ancestors of the active file', () => {
    const files = [f('a', 'x.rs'), f('b', 'src/lib/y.rs'), f('c', 'z.rs')];
    const tree = buildTree(files);
    const active = fileKey(files[1]!);
    const expanded = defaultExpandedIds(tree, active);
    const bRepo = tree.find((r) => r.repo === 'b')!;
    const dirId = (bRepo.children[0] as { id: string }).id;
    expect(expanded.has(dirId)).toBe(true);
  });
});

describe('ancestorIdsFor', () => {
  it('returns the repo + dir chain ids for a nested file (crumb-click reveal)', () => {
    const files = [f('r', 'src/lib/y.rs')];
    const tree = buildTree(files);
    const target = fileKey(files[0]!);
    const ids = ancestorIdsFor(tree, target);
    expect(ids).toContain('r');
    const dirId = (tree[0]!.children[0] as { id: string }).id;
    expect(ids).toContain(dirId);
  });
});

describe('countVisibleRows', () => {
  it('counts only rows under expanded ancestors (lazy render == O(visible))', () => {
    const files = [f('a', 'x.rs'), f('b', 'y.rs')];
    const tree = buildTree(files);
    // Nothing expanded: only the two repo rows are "visible".
    expect(countVisibleRows(tree, new Set())).toBe(2);
    expect(countVisibleRows(tree, new Set(['a', 'b']))).toBe(4);
  });
});

describe('filterFiles', () => {
  const files = [f('wealdlore', 'Sources/Reader/PlateBundle.swift'), f('wealdlore', 'pipeline/plates.py'), f('other', 'README.md')];

  it('matches case-insensitively over repo/path', () => {
    const matches = filterFiles(files, 'PLATES');
    expect(matches.map((m) => m.file.path)).toEqual(['pipeline/plates.py']);
  });

  it('returns nothing for a blank query', () => {
    expect(filterFiles(files, '   ')).toEqual([]);
  });

  it('matches on the repo name too', () => {
    const matches = filterFiles(files, 'wealdlore');
    expect(matches).toHaveLength(2);
  });

  it('sorts by match position, then path, so Enter opens a stable top hit', () => {
    const matches = filterFiles(files, 'p');
    expect(matches[0]!.file.path).toBe('pipeline/plates.py'); // "p" at index 0 of "wealdlore/pipeline..." combined? actually repo/path
  });
});

describe('activeFlatten', () => {
  it('ranks by refCount desc, then lastTs desc — the backend order', () => {
    const files = [
      f('r', 'a.rs', { refCount: 1, lastTs: '2026-07-01T00:00:00Z' }),
      f('r', 'b.rs', { refCount: 5, lastTs: '2026-07-01T00:00:00Z' }),
      f('r', 'c.rs', { refCount: 5, lastTs: '2026-07-10T00:00:00Z' }),
    ];
    const ranked = activeFlatten(files);
    expect(ranked.map((r) => r.path)).toEqual(['c.rs', 'b.rs', 'a.rs']);
  });
});

describe('computeDisambiguators', () => {
  it('flags colliding basenames with a distinguishing parent dir', () => {
    const files = [f('r', 'fleet/mod.rs'), f('r', 'reader/mod.rs'), f('r', 'pipeline/plates.py')];
    const map = computeDisambiguators(files);
    expect(map.get(fileKey(files[0]!))).toBe('fleet/');
    expect(map.get(fileKey(files[1]!))).toBe('reader/');
    expect(map.get(fileKey(files[2]!))).toBeNull();
  });
});

describe('breadcrumbFromTree + collapseBreadcrumb', () => {
  it('walks the compacted tree, so a folded dir chain is ONE crumb segment', () => {
    const files = [f('confer-public', 'ui/src/lib/components/CodeTree.svelte')];
    const tree = buildTree(files);
    const crumb = breadcrumbFromTree(tree, 'confer-public', fileKey(files[0]!));
    expect(crumb.map((c) => c.label)).toEqual(['confer-public', 'ui/src/lib/components', 'CodeTree.svelte']);
  });

  it('collapses a long crumb to first-two + ellipsis + last, keeping the full chain available for title', () => {
    // Branch points at every level (a sibling file/dir) prevent compaction,
    // so this really does produce a 5-segment chain to collapse.
    const files = [
      f('r', 'a/b/c/d/e/file.rs'),
      f('r', 'a/sibling.rs'),
      f('r', 'a/b/sibling.rs'),
      f('r', 'a/b/c/sibling.rs'),
      f('r', 'a/b/c/d/sibling.rs'),
    ];
    const tree = buildTree(files);
    const crumb = breadcrumbFromTree(tree, 'r', fileKey(files[0]!));
    const collapsed = collapseBreadcrumb(crumb, 4);
    expect(collapsed).toHaveLength(4);
    expect(collapsed[2]!.label).toBe('…');
    expect(collapsed[2]!.nodeId).toBeNull();
    expect(collapsed[0]!.label).toBe('r');
    expect(collapsed[collapsed.length - 1]!.label).toBe('file.rs');
  });

  it('leaves short crumbs untouched', () => {
    const files = [f('r', 'a.rs')];
    const tree = buildTree(files);
    const crumb = breadcrumbFromTree(tree, 'r', fileKey(files[0]!));
    expect(collapseBreadcrumb(crumb, 4)).toEqual(crumb);
  });
});

describe('truncateMiddle', () => {
  it('leaves short text alone', () => {
    expect(truncateMiddle('short.rs', 40)).toBe('short.rs');
  });

  it('middle-truncates long text, preserving an ellipsis', () => {
    const long = 'a'.repeat(60);
    const out = truncateMiddle(long, 40);
    expect(out.length).toBeLessThanOrEqual(40);
    expect(out).toContain('…');
  });
});

describe('groupRefHitsByFile — repo rollup (design/44 §6 item 2.4)', () => {
  it('groups hits by path, counting each group and keeping the newest ts', () => {
    const hits = [
      hit('a.rs', '2026-07-10T00:00:00Z'),
      hit('a.rs', '2026-07-17T00:00:00Z'),
      hit('b.rs', '2026-07-12T00:00:00Z'),
    ];

    const groups = groupRefHitsByFile(hits);

    expect(groups).toHaveLength(2);
    const a = groups.find((g) => g.path === 'a.rs')!;
    expect(a.count).toBe(2);
    expect(a.lastTs).toBe('2026-07-17T00:00:00Z');
    const b = groups.find((g) => g.path === 'b.rs')!;
    expect(b.count).toBe(1);
    expect(b.lastTs).toBe('2026-07-12T00:00:00Z');
  });

  it('sorts by count desc, then path asc for ties', () => {
    const hits = [hit('z.rs', '2026-07-10T00:00:00Z'), hit('a.rs', '2026-07-10T00:00:00Z'), hit('m.rs', '2026-07-10T00:00:00Z'), hit('m.rs', '2026-07-11T00:00:00Z')];

    const groups = groupRefHitsByFile(hits);

    expect(groups.map((g) => g.path)).toEqual(['m.rs', 'a.rs', 'z.rs']);
  });

  it('returns an empty list for no hits', () => {
    expect(groupRefHitsByFile([])).toEqual([]);
  });
});
