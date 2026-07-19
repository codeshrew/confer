import { beforeEach, describe, expect, it, vi } from 'vitest';

// readState is a module-level singleton constructed from localStorage AT
// IMPORT TIME — so each test that needs a specific starting localStorage
// state must reset the module registry (vi.resetModules) and re-import
// fresh, rather than relying on the already-imported instance reflecting a
// localStorage write made after import.
async function freshReadState() {
  vi.resetModules();
  const mod = await import('./readState.svelte');
  return mod.readState;
}

describe('readState — watermarks ("since you last looked")', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('a never-visited (hub, topic) has no watermark at all — null, not zero', async () => {
    const readState = await freshReadState();
    expect(readState.getWatermark('agent-coord', 'reader')).toBeNull();
  });

  it('setWatermark records a real value, retrievable for that exact (hub, topic)', async () => {
    const readState = await freshReadState();
    readState.setWatermark('agent-coord', 'reader', 1_700_000_000_000);
    expect(readState.getWatermark('agent-coord', 'reader')).toBe(1_700_000_000_000);
  });

  it('watermarks are scoped per (hub, topic) — same topic name in a different hub is independent', async () => {
    const readState = await freshReadState();
    readState.setWatermark('agent-coord', 'reader', 1000);
    readState.setWatermark('confer-lab', 'reader', 2000);

    expect(readState.getWatermark('agent-coord', 'reader')).toBe(1000);
    expect(readState.getWatermark('confer-lab', 'reader')).toBe(2000);
  });

  it('markAllRead moves the watermark to (approximately) now', async () => {
    const readState = await freshReadState();
    const before = Date.now();
    readState.markAllRead('agent-coord', 'reader');
    const after = Date.now();

    const wm = readState.getWatermark('agent-coord', 'reader');
    expect(wm).not.toBeNull();
    expect(wm as number).toBeGreaterThanOrEqual(before);
    expect(wm as number).toBeLessThanOrEqual(after);
  });

  it('survives a fresh module load (persisted to localStorage, not just in-memory)', async () => {
    const first = await freshReadState();
    first.setWatermark('agent-coord', 'reader', 5000);

    const second = await freshReadState();
    expect(second.getWatermark('agent-coord', 'reader')).toBe(5000);
  });

  it('degrades to "no watermarks" (never throws) when localStorage is unavailable', async () => {
    const original = Object.getOwnPropertyDescriptor(window, 'localStorage');
    Object.defineProperty(window, 'localStorage', {
      configurable: true,
      get() {
        throw new Error('storage disabled');
      },
    });

    const readState = await freshReadState();
    expect(() => readState.getWatermark('agent-coord', 'reader')).not.toThrow();
    expect(readState.getWatermark('agent-coord', 'reader')).toBeNull();
    expect(() => readState.setWatermark('agent-coord', 'reader', 123)).not.toThrow();

    if (original) Object.defineProperty(window, 'localStorage', original);
  });
});

describe('readState — detail-viewed (completionist-safe: marks READ, never UNREAD)', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('a never-opened message is not detail-viewed — absence is neutral, not a debt', async () => {
    const readState = await freshReadState();
    expect(readState.isDetailViewed('msg_never_opened')).toBe(false);
  });

  it('markDetailViewed records it, and it stays recorded', async () => {
    const readState = await freshReadState();
    readState.markDetailViewed('msg_01JQ001');
    expect(readState.isDetailViewed('msg_01JQ001')).toBe(true);
  });

  it('marking the same message twice is a no-op, not a duplicate write', async () => {
    const readState = await freshReadState();
    readState.markDetailViewed('msg_01JQ001');
    readState.markDetailViewed('msg_01JQ001');
    expect(readState.isDetailViewed('msg_01JQ001')).toBe(true);
  });

  it('survives a fresh module load', async () => {
    const first = await freshReadState();
    first.markDetailViewed('msg_persisted');

    const second = await freshReadState();
    expect(second.isDetailViewed('msg_persisted')).toBe(true);
  });

  it('detail-viewed state for one message never affects another', async () => {
    const readState = await freshReadState();
    readState.markDetailViewed('msg_a');
    expect(readState.isDetailViewed('msg_b')).toBe(false);
  });
});
