import { beforeEach, describe, expect, it } from 'vitest';
import { frameData, overlayStack } from './overlayStack.svelte';

// A module singleton — reset between tests so one test's pushes can't leak
// into the next (same gotcha boardFilter.test.ts/readState.test.ts already
// guard against).
beforeEach(() => {
  overlayStack.clear();
});

describe('overlayStack', () => {
  it('starts empty', () => {
    expect(overlayStack.stack).toEqual([]);
    expect(overlayStack.top).toBeNull();
  });

  it('push adds a frame on top, growing the stack', () => {
    overlayStack.push({ id: 'agent-dossier', type: 'popover', data: { agentId: 'herald' } });
    expect(overlayStack.stack).toHaveLength(1);
    expect(overlayStack.top).toEqual({ id: 'agent-dossier', type: 'popover', data: { agentId: 'herald' } });
  });

  it('push on top of an existing frame NESTS it — both frames survive, the piece 10 bug fix', () => {
    overlayStack.push({ id: 'agent-dossier', type: 'popover', data: { agentId: 'herald' } });
    overlayStack.push({ id: 'ticket', type: 'popover', data: { ticketId: 'req_01JQ8f2' } });

    expect(overlayStack.stack).toHaveLength(2);
    expect(overlayStack.top?.id).toBe('ticket');
    // The dossier frame is still THERE, underneath — not destroyed.
    expect(overlayStack.stack[0]).toEqual({ id: 'agent-dossier', type: 'popover', data: { agentId: 'herald' } });
  });

  it('pop unwinds exactly one layer, revealing the frame beneath', () => {
    overlayStack.push({ id: 'agent-dossier', type: 'popover', data: { agentId: 'herald' } });
    overlayStack.push({ id: 'ticket', type: 'popover', data: { ticketId: 'req_01JQ8f2' } });

    overlayStack.pop();
    expect(overlayStack.stack).toHaveLength(1);
    expect(overlayStack.top?.id).toBe('agent-dossier');
    expect(frameData(overlayStack.top, 'agentId')).toBe('herald');
  });

  it('pop on an empty stack is a no-op, not an error', () => {
    expect(() => overlayStack.pop()).not.toThrow();
    expect(overlayStack.stack).toEqual([]);
  });

  it('replace on an EMPTY stack behaves like push — a fresh top-level open', () => {
    overlayStack.replace({ id: 'ticket', type: 'popover', data: { ticketId: 'req_01JQ8f2' } });
    expect(overlayStack.stack).toHaveLength(1);
    expect(overlayStack.top?.id).toBe('ticket');
  });

  it('replace swaps the TOP frame in place — same depth, no nesting (j/k navigating within one popover)', () => {
    overlayStack.push({ id: 'ticket', type: 'popover', data: { ticketId: 'req_01JQ8f2' } });
    overlayStack.replace({ id: 'ticket', type: 'popover', data: { ticketId: 'req_01JQd21' } });

    expect(overlayStack.stack).toHaveLength(1);
    expect(frameData(overlayStack.top, 'ticketId')).toBe('req_01JQd21');
  });

  it('replace on the TOP of a nested stack swaps only the top, leaving the parent frame intact', () => {
    overlayStack.push({ id: 'agent-dossier', type: 'popover', data: { agentId: 'herald' } });
    overlayStack.push({ id: 'ticket', type: 'popover', data: { ticketId: 'req_01JQ8f2' } });
    // j/k-navigating to a different ticket while nested under the dossier.
    overlayStack.replace({ id: 'ticket', type: 'popover', data: { ticketId: 'req_01JQd21' } });

    expect(overlayStack.stack).toHaveLength(2);
    expect(frameData(overlayStack.top, 'ticketId')).toBe('req_01JQd21');
    expect(overlayStack.stack[0]?.id).toBe('agent-dossier');
    expect(frameData(overlayStack.stack[0] ?? null, 'agentId')).toBe('herald');
  });

  it('clear empties the whole stack regardless of depth', () => {
    overlayStack.push({ id: 'agent-dossier', type: 'popover', data: { agentId: 'herald' } });
    overlayStack.push({ id: 'ticket', type: 'popover', data: { ticketId: 'req_01JQ8f2' } });
    overlayStack.clear();

    expect(overlayStack.stack).toEqual([]);
    expect(overlayStack.top).toBeNull();
  });

  it('top always reflects the LAST pushed frame', () => {
    overlayStack.push({ id: 'note', type: 'popover', data: { msgId: 'msg_1' } });
    overlayStack.push({ id: 'ticket', type: 'popover', data: { ticketId: 'req_1' } });
    expect(overlayStack.top?.id).toBe('ticket');
    overlayStack.pop();
    expect(overlayStack.top?.id).toBe('note');
    overlayStack.pop();
    expect(overlayStack.top).toBeNull();
  });
});

describe('frameData', () => {
  it('reads a string value off a frame', () => {
    const frame = { id: 'ticket', type: 'popover' as const, data: { ticketId: 'req_01JQ8f2' } };
    expect(frameData(frame, 'ticketId')).toBe('req_01JQ8f2');
  });

  it('returns null for a null frame', () => {
    expect(frameData(null, 'ticketId')).toBeNull();
  });

  it('returns null for a missing key', () => {
    const frame = { id: 'ticket', type: 'popover' as const, data: {} };
    expect(frameData(frame, 'ticketId')).toBeNull();
  });

  it('returns null for a non-string value (e.g. a number) — Phase-B\'s URL-serializable shape allows those, but callers here always want a string id', () => {
    const frame = { id: 'ticket', type: 'popover' as const, data: { ticketId: 123 } };
    expect(frameData(frame, 'ticketId')).toBeNull();
  });

  it('returns null when the frame has no data at all', () => {
    const frame = { id: 'ticket', type: 'popover' as const };
    expect(frameData(frame, 'ticketId')).toBeNull();
  });
});
