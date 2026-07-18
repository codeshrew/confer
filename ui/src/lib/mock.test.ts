// Tests for mockApi.getMessages's pagination handling — it must mirror the
// real backend's /api/messages semantics (src/api.rs's `messages()`) closely
// enough that dev/tests exercise the same page-boundary behavior App.svelte
// relies on: most-recent `limit` messages, `before` for paging backward, and
// the "fewer than limit means no more" stop signal.
import { describe, expect, it } from 'vitest';
import { mockApi, mockMessages } from './mock';

describe('mockApi.getMessages pagination', () => {
  it('with no opts, returns every message for the topic (back-compat)', async () => {
    const all = await mockApi.getMessages('agent-coord', 'reader');
    const expectedCount = mockMessages.filter((m) => m.topic === 'reader').length;
    expect(all).toHaveLength(expectedCount);
  });

  it('with limit, returns the most-recent N messages in chronological order', async () => {
    const page = await mockApi.getMessages('agent-coord', 'reader', { limit: 3 });
    expect(page).toHaveLength(3);
    // Chronological (ascending) order.
    for (let i = 1; i < page.length; i++) {
      expect(new Date(page[i]!.ts).getTime()).toBeGreaterThanOrEqual(new Date(page[i - 1]!.ts).getTime());
    }
    // These are the LAST 3 (most recent), not the first 3.
    const all = await mockApi.getMessages('agent-coord', 'reader');
    expect(page.map((m) => m.id)).toEqual(all.slice(-3).map((m) => m.id));
  });

  it('a limit greater than the available count just returns everything', async () => {
    const all = await mockApi.getMessages('agent-coord', 'reader');
    const page = await mockApi.getMessages('agent-coord', 'reader', { limit: 999 });
    expect(page).toHaveLength(all.length);
  });

  it('before= pages backward: only messages strictly older than that id come back', async () => {
    const all = await mockApi.getMessages('agent-coord', 'reader');
    const cursor = all[all.length - 1]!.id; // the newest message
    const older = await mockApi.getMessages('agent-coord', 'reader', { before: cursor });
    expect(older).toHaveLength(all.length - 1);
    expect(older.some((m) => m.id === cursor)).toBe(false);
  });

  it('before combined with limit takes the most-recent `limit` of what is older than the cursor', async () => {
    const all = await mockApi.getMessages('agent-coord', 'reader');
    const cursor = all[all.length - 1]!.id;
    const page = await mockApi.getMessages('agent-coord', 'reader', { before: cursor, limit: 2 });
    expect(page.map((m) => m.id)).toEqual(all.slice(-3, -1).map((m) => m.id));
  });

  it('a full walk backward via repeated before= reconstructs the whole topic, oldest-first, no dupes', async () => {
    const all = await mockApi.getMessages('agent-coord', 'reader');
    const PAGE = 2;
    let cursor: string | undefined;
    const collected: string[] = [];
    for (;;) {
      const page = await mockApi.getMessages('agent-coord', 'reader', { limit: PAGE, before: cursor });
      if (page.length === 0) break;
      collected.unshift(...page.map((m) => m.id));
      cursor = page[0]!.id;
      if (page.length < PAGE) break; // stop-at-end signal
    }
    expect(collected).toEqual(all.map((m) => m.id));
  });

  it('paging backward past the oldest message returns an empty page (the stop condition)', async () => {
    const all = await mockApi.getMessages('agent-coord', 'reader');
    const oldest = all[0]!.id;
    const page = await mockApi.getMessages('agent-coord', 'reader', { before: oldest, limit: 10 });
    expect(page).toEqual([]);
  });
});
