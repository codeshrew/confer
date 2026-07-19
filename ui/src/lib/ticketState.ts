// The shared ticket-lifecycle vocabulary for piece 5's card trio (Row/
// MiniCard/FullPopover) + Board's cockpit (ui/REDESIGN.md, "the composable
// card system" + "the semantic state palette", both settled 2026-07-19).
// One derivation, reused everywhere a ticket needs a color or a lifecycle
// stage — the same precedent as thread.ts's shared KIND_TAG (piece 4):
// duplicate the mapping in two components and it WILL drift.
import type { Agent, CodeRef, Message, RequestRow } from './types';
import { formatClock } from './format';

/** The six-hue work-state palette (`--state-*` in app.css). `unowned` is a
 * COMPUTED condition (open + no claimant), not a `RequestStatus` value —
 * it's the board's "needs an owner" stat, folded in here so every card
 * derives it the same way. */
export type TicketState = 'open' | 'flight' | 'unowned' | 'stuck' | 'done';

const STATE_VAR: Record<TicketState, string> = {
  open: 'var(--state-open)',
  flight: 'var(--state-flight)',
  unowned: 'var(--state-unowned)',
  stuck: 'var(--state-stuck)',
  done: 'var(--state-done)',
};

const STATE_LABEL: Record<TicketState, string> = {
  open: 'open',
  flight: 'claimed',
  unowned: 'needs owner',
  stuck: 'stuck',
  done: 'done',
};

/** Derives a ticket's work-state from its current `RequestRow`. `deferred`
 * (backlog) OPEN requests stay `open` (cyan) rather than `unowned` (amber)
 * — a deliberate park isn't the same as work quietly going stale, and the
 * amber "needs an owner" hue should mean the latter. */
export function ticketStateOf(request: RequestRow): TicketState {
  switch (request.status) {
    case 'CLAIMED':
      return 'flight';
    case 'BLOCKED':
    case 'ERROR':
      return 'stuck';
    case 'DONE':
    case 'SUPERSEDED':
      return 'done';
    case 'OPEN':
    default:
      if (request.deferred) return 'open';
      return request.claimants.length > 0 ? 'flight' : 'unowned';
  }
}

export function ticketStateVar(state: TicketState): string {
  return STATE_VAR[state];
}

export function ticketStateLabel(state: TicketState): string {
  return STATE_LABEL[state];
}

// ── the lifecycle track (Requested → Claimed → Done) ────────────────────
// RequestRow only carries the *current* status, not per-stage history — the
// same CONTRACT GAP RequestDetail.svelte's trail already worked around by
// walking the message stream via `of`/`replyTo` off the request's
// originating `req_id -> msg_id` (App.svelte/RequestDetail's existing
// convention, reused verbatim here rather than reinvented).

export type StageState = 'done' | 'current' | 'pending';

export interface LifecycleStage {
  label: 'Requested' | 'Claimed' | 'Done';
  state: StageState;
  who: string | null;
  ts: string | null;
}

export interface LifecycleTrack {
  stages: LifecycleStage[];
  /** Set only when the ticket is `stuck` — the reason branches off whatever
   * stage was current when it happened, per the mockup's "red branch off
   * current step + reason". */
  branch: { off: 'Requested' | 'Claimed'; who: string; ts: string; reason: string } | null;
  /** Done stage's resolution text, when the ticket is actually `done`. */
  resolution: string | null;
}

function displayName(agentId: string, agentsById: Map<string, Agent>): string {
  const agent = agentsById.get(agentId);
  if (agent) return agent.display;
  return agentId.length ? agentId[0]!.toUpperCase() + agentId.slice(1) : agentId;
}

/**
 * Builds the 3-node lifecycle track for the full popover, adapting per
 * state exactly as `05-board-cockpit.html` specifies: open (Req✓, rest
 * hollow) · in-flight (Claim pulsing) · done (all filled + resolution on
 * Done) · stuck (red branch off whichever step was current + reason).
 *
 * `messages` is the full, unpaginated per-hub array (same one
 * RequestDetail/MetaThread already require) — needed to attribute WHO
 * did each transition and WHEN, which `RequestRow` alone can't say.
 */
export function buildLifecycleTrack(request: RequestRow, messages: Message[], agents: Agent[]): LifecycleTrack {
  const agentsById = new Map(agents.map((a) => [a.id, a]));
  const originMsgId = request.id.replace(/^req_/, 'msg_');
  const originMsg = messages.find((m) => m.id === originMsgId) ?? null;
  const related = messages
    .filter((m) => m.id !== originMsgId && (m.of === originMsgId || m.replyTo === originMsgId))
    .sort((a, b) => new Date(a.ts).getTime() - new Date(b.ts).getTime());

  const claimMsg = related.find((m) => m.type === 'claim') ?? null;
  const doneMsg = related.filter((m) => m.type === 'done').at(-1) ?? null;
  const blockedOrErrorMsg = related.filter((m) => m.type === 'blocked' || m.type === 'error').at(-1) ?? null;

  const state = ticketStateOf(request);

  const requestedStage: LifecycleStage = {
    label: 'Requested',
    state: 'done',
    who: originMsg ? displayName(originMsg.from, agentsById) : displayName(request.from, agentsById),
    ts: originMsg ? formatClock(originMsg.ts) : null,
  };

  const claimed = claimMsg !== null || request.claimants.length > 0;
  const claimedStage: LifecycleStage = {
    label: 'Claimed',
    state: state === 'flight' ? 'current' : claimed ? 'done' : 'pending',
    who: claimMsg ? displayName(claimMsg.from, agentsById) : request.claimants[0] ? displayName(request.claimants[0], agentsById) : null,
    ts: claimMsg ? formatClock(claimMsg.ts) : null,
  };

  const doneStage: LifecycleStage = {
    label: 'Done',
    state: state === 'done' ? 'done' : 'pending',
    who: doneMsg ? displayName(doneMsg.from, agentsById) : null,
    ts: doneMsg ? formatClock(doneMsg.ts) : null,
  };

  let branch: LifecycleTrack['branch'] = null;
  if (state === 'stuck' && blockedOrErrorMsg) {
    branch = {
      off: claimed ? 'Claimed' : 'Requested',
      who: displayName(blockedOrErrorMsg.from, agentsById),
      ts: formatClock(blockedOrErrorMsg.ts),
      reason: blockedOrErrorMsg.summary,
    };
  }

  return {
    stages: [requestedStage, claimedStage, doneStage],
    branch,
    resolution: state === 'done' ? request.resolution : null,
  };
}

/** Any code refs collected across the request's own trail (origin message +
 * every reply) — same de-duplication RequestDetail.svelte used, ported
 * verbatim since the full popover's "Refs" meta row needs the same set. */
export function ticketRefs(request: RequestRow, messages: Message[]): CodeRef[] {
  const originMsgId = request.id.replace(/^req_/, 'msg_');
  const originMsg = messages.find((m) => m.id === originMsgId) ?? null;
  const related = messages.filter((m) => m.id !== originMsgId && (m.of === originMsgId || m.replyTo === originMsgId));
  const trailMsgs = originMsg ? [originMsg, ...related] : related;
  const seen = new Set<string>();
  const out: CodeRef[] = [];
  for (const m of trailMsgs) {
    for (const r of m.refs) {
      const key = `${r.repo}:${r.path}:${r.sha}`;
      if (!seen.has(key)) {
        seen.add(key);
        out.push(r);
      }
    }
  }
  return out;
}

/** The origin chat message's own topic — the "open thread" footer jump
 * target. Null when the origin message isn't loaded (shouldn't happen in
 * practice; same seam as `buildLifecycleTrack`'s `originMsg` lookup). */
export function ticketOriginMessage(request: RequestRow, messages: Message[]): Message | null {
  const originMsgId = request.id.replace(/^req_/, 'msg_');
  return messages.find((m) => m.id === originMsgId) ?? null;
}
