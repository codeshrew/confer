// Piece 8b (ui/REDESIGN.md, `redesign-mockups/08-fleet-crew-deck.html` +
// `08-fleet-BRIEF.md`) — pure projections behind the agent dossier's "on
// their plate" and "activity" blocks. Same convention as boardStats.ts/
// noteRelated.ts — kept out of AgentDossier.svelte so the derivations are
// unit-testable without mounting Svelte.
//
// Law #3, per the brief's own flags: carrying/asking/closed-recent/activity
// are all real folds of `RequestRow[]`/`Message[]` already served for the
// current hub (the SAME data Board's cockpit and NotePopover's Related
// column already trust) — no fabrication. `agentPresence` (cross-hub
// "present in") lives in attention.ts, next to the OTHER cross-hub fold
// (`aggregateAttention`) it shares a fetch shape with.
import type { Message, RequestRow } from './types';
import { ticketStateOf } from './ticketState';

/** "Carrying" — this agent's own claimed/in-flight work (mirrors
 * boardStats.ts's `computeCarrying`, but returns the actual rows — the
 * dossier renders them as real TicketMiniCards, not just a count). */
export function carryingFor(agentId: string, requests: RequestRow[]): RequestRow[] {
  return requests.filter((r) => r.claimants.includes(agentId) && ticketStateOf(r) === 'flight').sort((a, b) => b.ageSecs - a.ageSecs);
}

/** "Asking" — this agent's own live (non-done) requests, unowned ones
 * flagged by the caller via `ticketStateOf(row) === 'unowned'` — the SAME
 * tickets that appear on the Board, "for cross-view coherence" (brief's
 * own words). Excludes anything the agent is ALREADY carrying (they filed
 * it and also claimed it themselves) — real data can produce that overlap,
 * but showing the identical ticket twice on one plate reads as a bug, not
 * two facts; "carrying" is the more specific truth once they hold it. */
export function askingFor(agentId: string, requests: RequestRow[]): RequestRow[] {
  return requests
    .filter((r) => r.from === agentId && ticketStateOf(r) !== 'done' && !r.claimants.includes(agentId))
    .sort((a, b) => b.ageSecs - a.ageSecs);
}

/** "closed · Nd" — tickets THIS agent personally closed (`type: 'done'`
 * messages they authored) within the window. Real, from the same
 * message-timestamp data Board's closure throughput already trusts —
 * not a RequestRow field (RequestRow carries no close timestamp). */
export function closedRecentCount(agentId: string, messages: Message[], days: number, nowMs: number = Date.now()): number {
  const cutoff = nowMs - days * 24 * 60 * 60 * 1000;
  return messages.filter((m) => m.type === 'done' && m.from === agentId && new Date(m.ts).getTime() >= cutoff).length;
}

export interface ActivityHour {
  hour: string; // ISO hour bucket start, for a11y/title use
  count: number;
}

/** The activity chart's real data — this agent's own message count per
 * hour over the last `hours` hours, oldest first, now last. Same bucketing
 * shape as boardStats.ts's `computeThroughput`, just hourly and scoped to
 * one agent instead of the whole board. */
export function activityBuckets(agentId: string, messages: Message[], hours: number, nowMs: number = Date.now()): ActivityHour[] {
  const HOUR_MS = 60 * 60 * 1000;
  const buckets: ActivityHour[] = [];
  const startOfHour = (ms: number) => Math.floor(ms / HOUR_MS) * HOUR_MS;
  const nowHour = startOfHour(nowMs);
  for (let i = hours - 1; i >= 0; i--) {
    const hourStart = nowHour - i * HOUR_MS;
    buckets.push({ hour: new Date(hourStart).toISOString(), count: 0 });
  }
  const earliestHour = nowHour - (hours - 1) * HOUR_MS;
  for (const m of messages) {
    if (m.from !== agentId) continue;
    const t = new Date(m.ts).getTime();
    const bucketStart = startOfHour(t);
    if (bucketStart < earliestHour || bucketStart > nowHour) continue;
    const idx = Math.round((bucketStart - earliestHour) / HOUR_MS);
    const bucket = buckets[idx];
    if (bucket) bucket.count++;
  }
  return buckets;
}
