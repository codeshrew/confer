// Piece 6 (ui/REDESIGN.md, "the composable card system") — the enriched
// note popover's "Related" column: a note's own tickets/code/thread,
// derived from the SAME real trail data MetaThread's peek already walks
// (thread.ts's `buildTrail`, over `/api/thread` + the full `messages`
// array) — no new fetch, no invented relationship. Kept pure/testable,
// same convention as boardStats.ts/ticketState.ts.
import type { CodeRef, RequestRow } from './types';
import type { TrailNode } from './thread';

/** Every REQUEST-type node in the note's own trail, resolved to its real
 * `RequestRow` (via the same `req_`/`msg_` id-suffix convention every
 * other ticket lookup in this app already uses). A trail node whose
 * request isn't in `requests` (a different hub's board, a stale window)
 * is simply omitted — never a fabricated placeholder row. */
export function relatedTickets(trail: TrailNode[], requests: RequestRow[]): RequestRow[] {
  const requestsById = new Map(requests.map((r) => [r.id, r]));
  const out: RequestRow[] = [];
  const seen = new Set<string>();
  for (const node of trail) {
    if (node.type !== 'request') continue;
    const reqId = node.msgId.replace(/^msg_/, 'req_');
    const row = requestsById.get(reqId);
    if (row && !seen.has(row.id)) {
      seen.add(row.id);
      out.push(row);
    }
  }
  return out;
}

/** Every code ref mentioned ANYWHERE in the note's trail (not just the
 * focused message itself — a reply's `--ref` is just as "related"),
 * de-duplicated by repo+path+sha, same key shape `ticketRefs` (piece 5)
 * uses. */
export function relatedRefs(trail: TrailNode[]): CodeRef[] {
  const seen = new Set<string>();
  const out: CodeRef[] = [];
  for (const node of trail) {
    for (const ref of node.refs) {
      const key = `${ref.repo}:${ref.path}:${ref.sha}`;
      if (!seen.has(key)) {
        seen.add(key);
        out.push(ref);
      }
    }
  }
  return out;
}

export interface ThreadSummary {
  messageCount: number;
  topicCount: number;
}

/** The "thread" pill's real numbers — how many messages, across how many
 * topics, this note's conversation actually spans. */
export function threadSummary(trail: TrailNode[]): ThreadSummary {
  const topics = new Set(trail.map((n) => n.topic).filter((t): t is string => t !== null));
  return { messageCount: trail.length, topicCount: topics.size };
}
