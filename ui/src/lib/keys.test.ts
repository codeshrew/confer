import { describe, expect, it } from 'vitest';
import { isCommandK, isTypingTarget, viewForLeaderKey } from './keys';

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

describe('viewForLeaderKey — g-leader\'s second key', () => {
  it('maps 1-5 to the five views, in tmux-window-number order', () => {
    expect(viewForLeaderKey('1')).toBe('overview');
    expect(viewForLeaderKey('2')).toBe('chat');
    expect(viewForLeaderKey('3')).toBe('board');
    expect(viewForLeaderKey('4')).toBe('fleet');
    expect(viewForLeaderKey('5')).toBe('code');
  });

  it('maps unambiguous first-letter aliases (o/b/f), case-insensitively', () => {
    expect(viewForLeaderKey('o')).toBe('overview');
    expect(viewForLeaderKey('O')).toBe('overview');
    expect(viewForLeaderKey('b')).toBe('board');
    expect(viewForLeaderKey('f')).toBe('fleet');
  });

  it('has NO letter alias for Chat or Code — both start with "c", so neither is unambiguous', () => {
    expect(viewForLeaderKey('c')).toBeNull();
    expect(viewForLeaderKey('C')).toBeNull();
  });

  it('returns null for an unbound key', () => {
    expect(viewForLeaderKey('6')).toBeNull();
    expect(viewForLeaderKey('x')).toBeNull();
  });
});
