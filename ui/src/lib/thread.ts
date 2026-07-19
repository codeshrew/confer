// Real parent/child structure for the reply-hash reference graph (piece 3,
// ui/REDESIGN.md — side-peek + trail). `/api/thread` (src/api.rs::thread)
// returns every message sharing one `thread_root` — confirmed in the Rust
// source: calling it with ANY msgId inside a thread returns the SAME array,
// so one fetch already covers the whole connected conversation the peek
// needs, however many topics it crosses. What it does NOT carry is the real
// `of`/`replyTo` parent pointer (ThreadNode has no such field) — those live
// on the full `Message` record (App.svelte's already-loaded, unpaginated
// per-hub array). This module cross-references the two to recover REAL
// parent/child edges — never inventing a relationship the data doesn't
// support (law #3): a node whose `of`/`replyTo` doesn't resolve to another
// node in the SAME thread is simply a root (or an orphan-of-this-view),
// not silently attached to something plausible-looking.
import type { CodeRef, Message, MsgType, ThreadNode } from './types';

export interface TrailNode {
  msgId: string;
  from: string;
  type: MsgType;
  topic: string | null;
  summary: string;
  refs: CodeRef[];
  /** Recovered from the `messages` cross-reference — null if that message
   * isn't loaded (shouldn't happen in practice: `/api/thread` and the
   * per-hub `messages` array are the same underlying log, but the type
   * stays honest about the possibility rather than assuming). */
  ts: string | null;
  /** The real parent (whichever of `of`/`replyTo` resolves to another node
   * in THIS thread) — null for the root, or when neither pointer resolves
   * within this thread's own node set (RequestDetail.svelte uses the same
   * `of ?? replyTo` precedence for its own lifecycle-trail reconstruction —
   * matching established precedent, not inventing a new convention). */
  parentId: string | null;
}

/** Cross-references `nodes` (a `getThread()` response) against the fuller
 * `messages` array to recover real parent pointers. Nodes are returned in
 * the SAME order `nodes` arrived in (already chronological — the backend
 * sorts by ULID id). */
export function buildTrail(nodes: ThreadNode[], messages: Message[]): TrailNode[] {
  const idsInThread = new Set(nodes.map((n) => n.msgId));
  const byId = new Map(messages.map((m) => [m.id, m]));
  return nodes.map((n) => {
    const msg = byId.get(n.msgId);
    let parentId: string | null = null;
    if (msg) {
      const candidate = msg.of ?? msg.replyTo ?? null;
      if (candidate && idsInThread.has(candidate) && candidate !== n.msgId) parentId = candidate;
    }
    return { msgId: n.msgId, from: n.from, type: n.type, topic: n.topic, summary: n.summary, refs: n.refs, ts: msg?.ts ?? null, parentId };
  });
}

/** The thread's root — the one node with no resolvable parent. Falls back to
 * the first (chronologically earliest) node if every node claims a parent
 * (shouldn't happen with real data — a cycle or all-orphans case — but a
 * trail must always have SOME anchor to render rather than nothing). */
export function trailRoot(trail: TrailNode[]): TrailNode | null {
  return trail.find((n) => n.parentId === null) ?? trail[0] ?? null;
}

/** The real path from root to `focusedId`, root first — built by walking
 * `parentId` pointers, not by assuming a fixed depth. A cycle (bad data)
 * can't loop forever: `seen` bounds the walk to the thread's own size. */
export function pathToRoot(trail: TrailNode[], focusedId: string): TrailNode[] {
  const byId = new Map(trail.map((n) => [n.msgId, n]));
  const path: TrailNode[] = [];
  let current = byId.get(focusedId) ?? null;
  const seen = new Set<string>();
  while (current && !seen.has(current.msgId)) {
    path.unshift(current);
    seen.add(current.msgId);
    current = current.parentId ? (byId.get(current.parentId) ?? null) : null;
  }
  return path;
}

/** Direct children of `msgId`, in the trail's own (chronological) order —
 * `l` ("deeper") moves to the first of these. */
export function childrenOf(trail: TrailNode[], msgId: string): TrailNode[] {
  return trail.filter((n) => n.parentId === msgId);
}
