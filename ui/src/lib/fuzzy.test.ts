import { describe, expect, it } from 'vitest';
import { fuzzyFilter, fuzzyMatch } from './fuzzy';

describe('fuzzyMatch', () => {
  it('matches a scattered subsequence, case-insensitively', () => {
    expect(fuzzyMatch('orb', 'codeshrew/confer-jarvis-orbit')).not.toBeNull();
    expect(fuzzyMatch('ORB', 'codeshrew/confer-jarvis-orbit')).not.toBeNull();
  });

  it('returns null when the query is not a subsequence at all', () => {
    expect(fuzzyMatch('xyz', 'confer-lab')).toBeNull();
  });

  it('an empty query matches everything with a neutral score', () => {
    expect(fuzzyMatch('', 'anything')).toEqual({ score: 0, positions: [] });
  });

  it('scores a contiguous run higher than the same letters scattered', () => {
    const contiguous = fuzzyMatch('orb', 'orbit')!;
    const scattered = fuzzyMatch('orb', 'o-r-consulting-b')!;
    expect(contiguous.score).toBeGreaterThan(scattered.score);
  });

  it('scores an earlier match start higher than the same query starting later', () => {
    const early = fuzzyMatch('lab', 'lab-notes')!;
    const late = fuzzyMatch('lab', 'confer-lab-notes')!;
    expect(early.score).toBeGreaterThan(late.score);
  });
});

describe('fuzzyFilter', () => {
  const hubs = ['agent-coord', 'confer-lab', 'codeshrew/confer-jarvis-orbit'];

  it('filters out non-matches (agent-coord has no o-r-b subsequence) and ranks the tighter match first', () => {
    // 'orb' is technically a subsequence of BOTH remaining hubs (confer-lab
    // has an o...r...b spread across "conf-e-r---b"), but "orbit"'s
    // contiguous "orb" run scores well above that scattered one.
    const result = fuzzyFilter(hubs, 'orb', (h) => h);
    expect(result[0]).toBe('codeshrew/confer-jarvis-orbit');
    expect(result).not.toContain('agent-coord');
  });

  it('an empty query returns every item, order preserved (stable sort, equal scores)', () => {
    expect(fuzzyFilter(hubs, '', (h) => h)).toEqual(hubs);
  });

  it('ranks a hub whose name starts with the query above one where it appears mid-string', () => {
    const result = fuzzyFilter(['confer-lab', 'lab-partners'], 'lab', (h) => h);
    expect(result[0]).toBe('lab-partners');
  });
});
