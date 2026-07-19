import { afterEach, describe, expect, it } from 'vitest';
import { paneFocus } from './paneFocus.svelte';
import type { PaneRect } from './keys';

// The module-level `paneFocus` singleton persists across tests in this
// file (same reason App.test.ts resets appState.hub between tests) — every
// test unregisters what it registered so state never bleeds forward.

function makePane(id: string, label: string, rect: PaneRect) {
  const el = document.createElement('div');
  el.tabIndex = -1;
  document.body.appendChild(el);
  // jsdom never computes real layout, so getBoundingClientRect always
  // returns zeros — the registry takes a `getRect` callback specifically so
  // callers (and tests) supply the real numbers instead of relying on that.
  const stop = paneFocus.register({ id, label, el, getRect: () => rect });
  return { el, stop };
}

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const stop of cleanups.splice(0)) stop();
  document.body.innerHTML = '';
});

describe('paneFocus.register', () => {
  it('the first registered pane becomes focused automatically', () => {
    const a = makePane('a', 'Pane A', { top: 0, left: 0, width: 100, height: 100 });
    cleanups.push(a.stop);

    expect(paneFocus.focusedId).toBe('a');
    expect(paneFocus.focusedLabel).toBe('Pane A');
  });

  it('unregistering the focused pane falls back to whatever is left', () => {
    const a = makePane('a', 'A', { top: 0, left: 0, width: 100, height: 100 });
    const b = makePane('b', 'B', { top: 0, left: 100, width: 100, height: 100 });
    cleanups.push(b.stop);

    expect(paneFocus.focusedId).toBe('a');
    a.stop();
    expect(paneFocus.focusedId).toBe('b');
  });

  it('unregistering a NON-focused pane leaves focus alone', () => {
    const a = makePane('a', 'A', { top: 0, left: 0, width: 100, height: 100 });
    cleanups.push(a.stop);
    const b = makePane('b', 'B', { top: 0, left: 100, width: 100, height: 100 });

    expect(paneFocus.focusedId).toBe('a');
    b.stop();
    expect(paneFocus.focusedId).toBe('a');
  });
});

describe('paneFocus.focus', () => {
  it('moves focus to the pane, blurring whatever had it (gotcha #3)', () => {
    const a = makePane('a', 'A', { top: 0, left: 0, width: 100, height: 100 });
    cleanups.push(a.stop);
    const b = makePane('b', 'B', { top: 0, left: 100, width: 100, height: 100 });
    cleanups.push(b.stop);

    a.el.focus();
    expect(document.activeElement).toBe(a.el);

    paneFocus.focus('b');
    expect(paneFocus.focusedId).toBe('b');
    expect(document.activeElement).toBe(b.el);
  });

  it('is a no-op for an unknown pane id', () => {
    const a = makePane('a', 'A', { top: 0, left: 0, width: 100, height: 100 });
    cleanups.push(a.stop);

    paneFocus.focus('ghost');
    expect(paneFocus.focusedId).toBe('a');
  });
});

describe('paneFocus.moveDirection', () => {
  it('moves via real geometry — l goes right, h goes back left', () => {
    const rail = makePane('rail', 'Rail', { top: 0, left: 0, width: 200, height: 800 });
    cleanups.push(rail.stop);
    const stream = makePane('stream', 'Stream', { top: 0, left: 200, width: 800, height: 800 });
    cleanups.push(stream.stop);

    paneFocus.focus('rail');
    paneFocus.moveDirection('l');
    expect(paneFocus.focusedId).toBe('stream');

    paneFocus.moveDirection('h');
    expect(paneFocus.focusedId).toBe('rail');
  });

  it('is a no-op at the edge (nowhere to go in that direction)', () => {
    const rail = makePane('rail', 'Rail', { top: 0, left: 0, width: 200, height: 800 });
    cleanups.push(rail.stop);

    paneFocus.focus('rail');
    paneFocus.moveDirection('h');
    expect(paneFocus.focusedId).toBe('rail');
  });

  it('focuses the first pane when nothing is focused yet', () => {
    const a = makePane('a', 'A', { top: 0, left: 0, width: 100, height: 100 });
    cleanups.push(a.stop);
    // Simulate "nothing focused" by cycling to a state where focusedId
    // could plausibly be null — register/unregister a lone pane.
    a.stop();
    cleanups.length = 0;
    const b = makePane('b', 'B', { top: 0, left: 0, width: 100, height: 100 });
    cleanups.push(b.stop);
    expect(paneFocus.focusedId).toBe('b');
  });
});

describe('paneFocus.cycle — Ctrl+]/[ and F6/Shift+F6\'s reliable ring fallback', () => {
  it('cycles forward through registration order, wrapping at the end', () => {
    const a = makePane('a', 'A', { top: 0, left: 0, width: 100, height: 100 });
    cleanups.push(a.stop);
    const b = makePane('b', 'B', { top: 0, left: 200, width: 100, height: 100 });
    cleanups.push(b.stop);
    const c = makePane('c', 'C', { top: 0, left: 400, width: 100, height: 100 });
    cleanups.push(c.stop);

    paneFocus.focus('a');
    paneFocus.cycle(true);
    expect(paneFocus.focusedId).toBe('b');
    paneFocus.cycle(true);
    expect(paneFocus.focusedId).toBe('c');
    paneFocus.cycle(true);
    expect(paneFocus.focusedId).toBe('a');
  });

  it('cycles backward, wrapping at the start', () => {
    const a = makePane('a', 'A', { top: 0, left: 0, width: 100, height: 100 });
    cleanups.push(a.stop);
    const b = makePane('b', 'B', { top: 0, left: 200, width: 100, height: 100 });
    cleanups.push(b.stop);

    paneFocus.focus('a');
    paneFocus.cycle(false);
    expect(paneFocus.focusedId).toBe('b');
  });
});

describe('paneFocus.syncFromFocusEvent — mouse-click-focuses-a-pane, for free', () => {
  it('updates focusedId when a descendant of a DIFFERENT pane receives focus', () => {
    const a = makePane('a', 'A', { top: 0, left: 0, width: 100, height: 100 });
    cleanups.push(a.stop);
    const b = makePane('b', 'B', { top: 0, left: 200, width: 100, height: 100 });
    cleanups.push(b.stop);

    const button = document.createElement('button');
    b.el.appendChild(button);

    expect(paneFocus.focusedId).toBe('a');
    paneFocus.syncFromFocusEvent({ target: button } as unknown as FocusEvent);
    expect(paneFocus.focusedId).toBe('b');
  });

  it('ignores a focus event with a non-Element target', () => {
    const a = makePane('a', 'A', { top: 0, left: 0, width: 100, height: 100 });
    cleanups.push(a.stop);
    paneFocus.syncFromFocusEvent({ target: null } as unknown as FocusEvent);
    expect(paneFocus.focusedId).toBe('a');
  });
});
