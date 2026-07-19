import { describe, expect, it } from 'vitest';
import { buildTrail, childrenOf, pathToRoot, trailRoot } from './thread';
import type { Message, ThreadNode } from './types';

function node(overrides: Partial<ThreadNode> = {}): ThreadNode {
  return { msgId: 'msg_1', from: 'herald', type: 'note', topic: 'general', summary: 'hi', refs: [], ...overrides };
}

function message(overrides: Partial<Message> = {}): Message {
  return {
    id: 'msg_1',
    from: 'herald',
    type: 'note',
    ts: '2026-07-18T10:00:00Z',
    host: null,
    to: [],
    cc: [],
    topic: 'general',
    summary: 'hi',
    body: 'hi',
    of: null,
    replyTo: null,
    supersedes: null,
    refs: [],
    ...overrides,
  };
}

describe('buildTrail', () => {
  it('recovers a real parent edge from Message.replyTo, cross-referenced by msgId', () => {
    const nodes = [node({ msgId: 'root' }), node({ msgId: 'reply' })];
    const messages = [message({ id: 'root' }), message({ id: 'reply', replyTo: 'root' })];
    const trail = buildTrail(nodes, messages);
    expect(trail.find((n) => n.msgId === 'reply')?.parentId).toBe('root');
    expect(trail.find((n) => n.msgId === 'root')?.parentId).toBeNull();
  });

  it('prefers `of` over `replyTo` when both are present (RequestDetail\'s own precedent)', () => {
    const nodes = [node({ msgId: 'root' }), node({ msgId: 'other' }), node({ msgId: 'child' })];
    const messages = [
      message({ id: 'root' }),
      message({ id: 'other' }),
      message({ id: 'child', of: 'root', replyTo: 'other' }),
    ];
    const trail = buildTrail(nodes, messages);
    expect(trail.find((n) => n.msgId === 'child')?.parentId).toBe('root');
  });

  it('never fabricates a parent — a pointer to a message OUTSIDE this thread stays null, not attached to something plausible', () => {
    const nodes = [node({ msgId: 'root' })];
    // `replyTo` points at a real message id, but that message isn't part of
    // THIS thread's node set (getThread only returned 'root') — the edge
    // must not be invented just because a plausible-looking id exists.
    const messages = [message({ id: 'root', replyTo: 'msg_from_a_different_thread' })];
    const trail = buildTrail(nodes, messages);
    expect(trail[0]!.parentId).toBeNull();
  });

  it('leaves parentId null when the message itself was not found in `messages` (honest "unknown", not a guess)', () => {
    const nodes = [node({ msgId: 'orphan' })];
    const trail = buildTrail(nodes, []);
    expect(trail[0]!.parentId).toBeNull();
    expect(trail[0]!.ts).toBeNull();
  });

  it('never lets a message claim itself as its own parent (defensive against bad/looped data)', () => {
    const nodes = [node({ msgId: 'a' })];
    const messages = [message({ id: 'a', replyTo: 'a' })];
    const trail = buildTrail(nodes, messages);
    expect(trail[0]!.parentId).toBeNull();
  });
});

describe('trailRoot', () => {
  it('finds the node with no parent', () => {
    const trail = buildTrail(
      [node({ msgId: 'root' }), node({ msgId: 'child' })],
      [message({ id: 'root' }), message({ id: 'child', replyTo: 'root' })]
    );
    expect(trailRoot(trail)?.msgId).toBe('root');
  });

  it('falls back to the first node if nothing resolves to a root (never returns nothing to render)', () => {
    const trail = buildTrail([node({ msgId: 'a' })], []);
    expect(trailRoot(trail)?.msgId).toBe('a');
  });

  it('returns null only for a genuinely empty trail', () => {
    expect(trailRoot([])).toBeNull();
  });
});

describe('pathToRoot', () => {
  it('walks the REAL parent chain, root first, however many hops deep', () => {
    const nodes = [node({ msgId: 'root' }), node({ msgId: 'mid' }), node({ msgId: 'leaf' })];
    const messages = [
      message({ id: 'root' }),
      message({ id: 'mid', replyTo: 'root' }),
      message({ id: 'leaf', replyTo: 'mid' }),
    ];
    const trail = buildTrail(nodes, messages);
    const path = pathToRoot(trail, 'leaf');
    expect(path.map((n) => n.msgId)).toEqual(['root', 'mid', 'leaf']);
  });

  it('a root-level focus returns a single-element path', () => {
    const trail = buildTrail([node({ msgId: 'root' })], [message({ id: 'root' })]);
    expect(pathToRoot(trail, 'root').map((n) => n.msgId)).toEqual(['root']);
  });

  it('does not infinite-loop on a cyclic parent chain (defensive, bad data)', () => {
    // a -> b -> a (shouldn't happen with real data, but must not hang)
    const nodes = [node({ msgId: 'a' }), node({ msgId: 'b' })];
    const messages = [message({ id: 'a', replyTo: 'b' }), message({ id: 'b', replyTo: 'a' })];
    const trail = buildTrail(nodes, messages);
    const path = pathToRoot(trail, 'a');
    expect(path.length).toBeLessThanOrEqual(2);
  });
});

describe('childrenOf', () => {
  it('returns direct children only, in trail order', () => {
    const nodes = [node({ msgId: 'root' }), node({ msgId: 'c1' }), node({ msgId: 'c2' }), node({ msgId: 'grandchild' })];
    const messages = [
      message({ id: 'root' }),
      message({ id: 'c1', replyTo: 'root' }),
      message({ id: 'c2', replyTo: 'root' }),
      message({ id: 'grandchild', replyTo: 'c1' }),
    ];
    const trail = buildTrail(nodes, messages);
    expect(childrenOf(trail, 'root').map((n) => n.msgId)).toEqual(['c1', 'c2']);
    expect(childrenOf(trail, 'c1').map((n) => n.msgId)).toEqual(['grandchild']);
  });

  it('returns an empty array for a leaf node', () => {
    const trail = buildTrail([node({ msgId: 'leaf' })], [message({ id: 'leaf' })]);
    expect(childrenOf(trail, 'leaf')).toEqual([]);
  });
});
