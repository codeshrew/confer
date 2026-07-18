import { describe, expect, it } from 'vitest';
import {
  defaultContextMode,
  leftRailVisible,
  rightRailToggleVisible,
  rightRailVisible,
  showFleetSection,
} from './railLayout';
import type { View } from './stores.svelte';

const ALL_VIEWS: View[] = ['overview', 'chat', 'board', 'fleet', 'code', 'repos'];

describe('defaultContextMode', () => {
  it('defaults Board to request', () => {
    expect(defaultContextMode('board')).toBe('request');
  });
  it('defaults Code to refs', () => {
    expect(defaultContextMode('code')).toBe('refs');
  });
  it('defaults Chat, Fleet, and Repos to meta', () => {
    expect(defaultContextMode('chat')).toBe('meta');
    expect(defaultContextMode('fleet')).toBe('meta');
    expect(defaultContextMode('repos')).toBe('meta');
  });
});

describe('leftRailVisible', () => {
  it('hides only on Repos and Overview — both full-width, cross-hub-or-collection views', () => {
    for (const view of ALL_VIEWS) {
      expect(leftRailVisible(view)).toBe(view !== 'repos' && view !== 'overview');
    }
  });
});

describe('showFleetSection', () => {
  it('drops the fleet roster only on Fleet — the center already is the roster', () => {
    for (const view of ALL_VIEWS) {
      expect(showFleetSection(view)).toBe(view !== 'fleet');
    }
  });
});

describe('rightRailToggleVisible', () => {
  it('hides the ⓘ toggle on Fleet, Repos, and Overview only', () => {
    for (const view of ALL_VIEWS) {
      expect(rightRailToggleVisible(view)).toBe(view !== 'fleet' && view !== 'repos' && view !== 'overview');
    }
  });
});

describe('rightRailVisible', () => {
  it('Chat is always open, selection or not', () => {
    expect(rightRailVisible({ view: 'chat', hasSelection: false })).toBe(true);
    expect(rightRailVisible({ view: 'chat', hasSelection: true })).toBe(true);
  });

  it('Board is collapsed until a row/request is selected', () => {
    expect(rightRailVisible({ view: 'board', hasSelection: false })).toBe(false);
    expect(rightRailVisible({ view: 'board', hasSelection: true })).toBe(true);
  });

  it('Code is collapsed until a file is active', () => {
    expect(rightRailVisible({ view: 'code', hasSelection: false })).toBe(false);
    expect(rightRailVisible({ view: 'code', hasSelection: true })).toBe(true);
  });

  it('Fleet, Repos, and Overview never show it, regardless of hasSelection', () => {
    expect(rightRailVisible({ view: 'fleet', hasSelection: true })).toBe(false);
    expect(rightRailVisible({ view: 'repos', hasSelection: true })).toBe(false);
    expect(rightRailVisible({ view: 'overview', hasSelection: true })).toBe(false);
  });
});
