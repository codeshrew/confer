// Client-side read-state (ui/REDESIGN.md piece 4, item 2 — 2026-07-19). A
// module-level singleton (same pattern as paneFocus.svelte.ts/appState),
// backed by localStorage so it survives reloads. Two INDEPENDENT tracks,
// both honest-by-construction (the operator's own browser, never a guess
// about anyone else):
//
// 1. WATERMARK — per (hub, topic), "the moment you last looked at this
//    topic." Drives the stream's "NEW · since you last looked" divider.
//    Advances to now when the operator LEAVES a topic (so returning later
//    only flags what arrived after that visit) or via the explicit
//    "mark all read" catch-up. A topic never visited has no watermark at
//    all — nothing is flagged new on a first-ever visit (there's nothing
//    real to compare against yet), not "everything is new".
//
// 2. DETAIL-VIEWED — a per-message SET, completionist-SAFE by design
//    (Stefan's framing, load-bearing): this marks what you HAVE deep-read
//    (opened in the focus reader, past a dwell/scroll threshold), never
//    what you HAVEN'T. There is no unread counter, no red badge derived
//    from its absence — a message simply not being in this set is neutral,
//    not a debt. Label it "opened", not a comprehension claim.
const WATERMARK_KEY = 'confer.readState.watermarks.v1';
const DETAIL_VIEWED_KEY = 'confer.readState.detailViewed.v1';

function watermarkKey(hub: string, topic: string): string {
  return `${hub}␟${topic}`;
}

function loadWatermarks(): Record<string, number> {
  try {
    const raw = localStorage.getItem(WATERMARK_KEY);
    return raw ? (JSON.parse(raw) as Record<string, number>) : {};
  } catch {
    // Private browsing / storage disabled / corrupt JSON — degrade to "no
    // watermarks recorded" rather than throwing. Never fabricate a value.
    return {};
  }
}

function persistWatermarks(map: Record<string, number>): void {
  try {
    localStorage.setItem(WATERMARK_KEY, JSON.stringify(map));
  } catch {
    // Same degrade-quietly stance — a failed write just means the
    // watermark won't survive a reload, not a crash.
  }
}

function loadDetailViewed(): Set<string> {
  try {
    const raw = localStorage.getItem(DETAIL_VIEWED_KEY);
    return raw ? new Set(JSON.parse(raw) as string[]) : new Set();
  } catch {
    return new Set();
  }
}

function persistDetailViewed(set: Set<string>): void {
  try {
    localStorage.setItem(DETAIL_VIEWED_KEY, JSON.stringify([...set]));
  } catch {
    // ignore — see loadWatermarks' note
  }
}

function createReadState() {
  let watermarks = $state<Record<string, number>>(loadWatermarks());
  let detailViewed = $state<Set<string>>(loadDetailViewed());

  /** `null` — never visited (or storage unavailable) — means "don't flag
   * anything as new," not "everything is new." Callers must treat the two
   * differently; see ChatStream's `isUnseenByYou`. */
  function getWatermark(hub: string, topic: string): number | null {
    return watermarks[watermarkKey(hub, topic)] ?? null;
  }

  function setWatermark(hub: string, topic: string, ms: number): void {
    watermarks = { ...watermarks, [watermarkKey(hub, topic)]: ms };
    persistWatermarks(watermarks);
  }

  /** The explicit "mark all read" catch-up — moves the watermark to now,
   * same effect as leaving the topic, available on demand so a long
   * absence is never a trap the operator has to scroll their way out of. */
  function markAllRead(hub: string, topic: string): void {
    setWatermark(hub, topic, Date.now());
  }

  function isDetailViewed(msgId: string): boolean {
    return detailViewed.has(msgId);
  }

  function markDetailViewed(msgId: string): void {
    if (detailViewed.has(msgId)) return; // already recorded — no-op, no redundant write
    const next = new Set(detailViewed);
    next.add(msgId);
    detailViewed = next;
    persistDetailViewed(next);
  }

  return {
    getWatermark,
    setWatermark,
    markAllRead,
    isDetailViewed,
    markDetailViewed,
  };
}

export type ReadState = ReturnType<typeof createReadState>;

export const readState: ReadState = createReadState();
