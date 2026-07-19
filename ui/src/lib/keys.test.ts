import { describe, expect, it } from 'vitest';
import { isCommandK, isTypingTarget, nearestPaneId, viewForCmdNumber, type GeometryPane } from './keys';

function el(tag: string, opts: { contentEditable?: boolean } = {}): HTMLElement {
  const e = document.createElement(tag);
  if (opts.contentEditable) e.contentEditable = 'true';
  document.body.appendChild(e);
  return e;
}

describe('isTypingTarget', () => {
  it('is true for input/textarea/select and contenteditable elements', () => {
    expect(isTypingTarget(el('input'))).toBe(true);
    expect(isTypingTarget(el('textarea'))).toBe(true);
    expect(isTypingTarget(el('select'))).toBe(true);
    expect(isTypingTarget(el('div', { contentEditable: true }))).toBe(true);
  });

  it('is false for ordinary elements and null', () => {
    expect(isTypingTarget(el('div'))).toBe(false);
    expect(isTypingTarget(el('button'))).toBe(false);
    expect(isTypingTarget(null)).toBe(false);
  });
});

describe('isCommandK', () => {
  it('matches Meta+K (mac) and Ctrl+K (everywhere else)', () => {
    expect(isCommandK(new KeyboardEvent('keydown', { key: 'k', metaKey: true }))).toBe(true);
    expect(isCommandK(new KeyboardEvent('keydown', { key: 'K', ctrlKey: true }))).toBe(true);
  });

  it('does not fire on a bare "k" or with Shift/Alt also held', () => {
    expect(isCommandK(new KeyboardEvent('keydown', { key: 'k' }))).toBe(false);
    expect(isCommandK(new KeyboardEvent('keydown', { key: 'k', metaKey: true, shiftKey: true }))).toBe(false);
    expect(isCommandK(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, altKey: true }))).toBe(false);
  });
});

describe('viewForCmdNumber — Layer 3\'s Cmd+number (the retired g-leader\'s replacement)', () => {
  it('maps 1-5 to the five views, in tmux-window-number order', () => {
    expect(viewForCmdNumber('1')).toBe('overview');
    expect(viewForCmdNumber('2')).toBe('chat');
    expect(viewForCmdNumber('3')).toBe('board');
    expect(viewForCmdNumber('4')).toBe('fleet');
    expect(viewForCmdNumber('5')).toBe('code');
  });

  it('returns null for an unbound key — no letter aliases at this layer anymore', () => {
    expect(viewForCmdNumber('6')).toBeNull();
    expect(viewForCmdNumber('o')).toBeNull();
    expect(viewForCmdNumber('b')).toBeNull();
  });
});

describe('nearestPaneId — Layer 1\'s Ctrl+hjkl geometry scorer', () => {
  // A rough 3-column layout: rail (far left) | center (stream) | right rail
  // — real widths/positions, not a hardcoded adjacency list (gotcha #4).
  const panes: GeometryPane[] = [
    { id: 'rail', rect: { top: 0, left: 0, width: 200, height: 800 } },
    { id: 'stream', rect: { top: 0, left: 200, width: 800, height: 800 } },
    { id: 'peek', rect: { top: 0, left: 1000, width: 400, height: 800 } },
  ];

  it('l (right) from rail goes to stream, then l again from stream goes to peek', () => {
    expect(nearestPaneId(panes, 'rail', 'l')).toBe('stream');
    expect(nearestPaneId(panes, 'stream', 'l')).toBe('peek');
  });

  it('h (left) from peek goes to stream, then h again from stream goes to rail', () => {
    expect(nearestPaneId(panes, 'peek', 'h')).toBe('stream');
    expect(nearestPaneId(panes, 'stream', 'h')).toBe('rail');
  });

  it('returns null at the edge — l from the rightmost pane has nowhere to go', () => {
    expect(nearestPaneId(panes, 'peek', 'l')).toBeNull();
  });

  it('returns null for j/k when nothing shares that axis (all panes side by side here)', () => {
    expect(nearestPaneId(panes, 'stream', 'j')).toBeNull();
    expect(nearestPaneId(panes, 'stream', 'k')).toBeNull();
  });

  it('picks the geometrically nearest pane on the pressed side, not registration order', () => {
    // Three stacked panes, top-to-bottom: header (small), body (tall), footer.
    // Pressing j from header should land on body (the nearest one down),
    // even though footer is also "below" — order in the array is
    // deliberately scrambled to prove this isn't reading list position.
    const stacked: GeometryPane[] = [
      { id: 'footer', rect: { top: 700, left: 0, width: 400, height: 100 } },
      { id: 'header', rect: { top: 0, left: 0, width: 400, height: 100 } },
      { id: 'body', rect: { top: 100, left: 0, width: 400, height: 600 } },
    ];
    expect(nearestPaneId(stacked, 'header', 'j')).toBe('body');
  });

  it('weighs cross-axis misalignment against primary-axis distance (2x), preferring a closer-but-slightly-offset pane over a far-but-perfectly-aligned one', () => {
    // 'near' is right below and slightly to the right (small cross-axis
    // offset); 'far' is directly below but much further away. The near
    // pane should win even though it's not perfectly column-aligned.
    const layout: GeometryPane[] = [
      { id: 'from', rect: { top: 0, left: 0, width: 100, height: 100 } },
      { id: 'near', rect: { top: 110, left: 30, width: 100, height: 100 } },
      { id: 'far', rect: { top: 500, left: 0, width: 100, height: 100 } },
    ];
    expect(nearestPaneId(layout, 'from', 'j')).toBe('near');
  });

  it('returns null when fromId is not a known pane', () => {
    expect(nearestPaneId(panes, 'ghost', 'l')).toBeNull();
  });
});
