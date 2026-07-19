import { beforeEach, describe, expect, it } from 'vitest';
import { boardFilter } from './boardFilter.svelte';

// A module singleton — reset between tests so one test's clicks can't
// leak into the next (same gotcha readState.svelte.ts's own tests guard
// against). Unlike paneFocus's registration-object identity problem, a
// plain clearAll() is enough here — there's no per-instance state to
// re-import around.
beforeEach(() => {
  boardFilter.clearAll();
});

describe('boardFilter', () => {
  it('starts with no filter active', () => {
    expect(boardFilter.stateFilter).toBeNull();
    expect(boardFilter.agentFilter).toBeNull();
    expect(boardFilter.active).toBe(false);
  });

  it('toggleState sets, and clicking the SAME state again clears it', () => {
    boardFilter.toggleState('stuck');
    expect(boardFilter.stateFilter).toBe('stuck');
    expect(boardFilter.active).toBe(true);

    boardFilter.toggleState('stuck');
    expect(boardFilter.stateFilter).toBeNull();
  });

  it('toggleState with a DIFFERENT state swaps it, not toggles it off', () => {
    boardFilter.toggleState('stuck');
    boardFilter.toggleState('unowned');
    expect(boardFilter.stateFilter).toBe('unowned');
  });

  it('"activeWork" (the Open stat) always clears the state filter', () => {
    boardFilter.toggleState('stuck');
    boardFilter.toggleState('activeWork');
    expect(boardFilter.stateFilter).toBeNull();
  });

  it('toggleAgent sets and un-sets independently of the state filter', () => {
    boardFilter.toggleState('flight');
    boardFilter.toggleAgent('jarvis');
    expect(boardFilter.stateFilter).toBe('flight');
    expect(boardFilter.agentFilter).toBe('jarvis');

    boardFilter.toggleAgent('jarvis');
    expect(boardFilter.agentFilter).toBeNull();
    expect(boardFilter.stateFilter).toBe('flight');
  });

  it('clearAll resets both dimensions', () => {
    boardFilter.toggleState('stuck');
    boardFilter.toggleAgent('jarvis');
    boardFilter.clearAll();
    expect(boardFilter.stateFilter).toBeNull();
    expect(boardFilter.agentFilter).toBeNull();
    expect(boardFilter.active).toBe(false);
  });
});
