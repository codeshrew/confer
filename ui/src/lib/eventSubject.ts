// Piece 9 (ui/redesign-mockups/09-event-type-BRIEF.md) — the composable
// EVENT type: a lifecycle/system message (claim/done/error/blocked/defer/
// supersede — Message.svelte's `.sysline`s) is a pure PORTAL. It has no
// popover of its own; clicking it opens whatever it's ABOUT — a ticket
// (TicketFullPopover), an agent (AgentDossier), or a thread (the peek).
// This module resolves that subject from real data ONLY — the same law-#3
// discipline `thread.ts`/`noteRelated.ts` already hold: an `of`/`replyTo`/
// `supersedes` pointer that doesn't resolve to something actually loaded
// renders the event as plain text, never a dead link.
import type { Agent, Message, MsgType, RequestRow } from './types';

/** The lifecycle/system message types rendered as compact rows, not full
 * conversational messages — Message.svelte's `isSysline`. `blocked` was
 * missing from the old ad-hoc set (a real gap: a BLOCKED event rendered as
 * a full note instead of a compact sysline) — this is now the one place
 * that list lives. */
export const SYSLINE_TYPES: ReadonlySet<MsgType> = new Set(['claim', 'done', 'error', 'blocked', 'defer', 'supersede']);

/** Event kinds whose subject is the REQUEST/TICKET they're `--of`. */
const TICKET_EVENT_TYPES: ReadonlySet<MsgType> = new Set(['claim', 'done', 'error', 'blocked', 'defer']);

export type EventKind = 'claim' | 'done' | 'error' | 'blocked' | 'defer' | 'supersede';

/** Icon + semantic-palette color var per event kind, straight off the
 * brief: done/claim green, blocked/error red, defer amber, supersede
 * muted — reusing the SAME `--state-*` tokens the ticket cards use (piece
 * 5), not a second invented palette. `--state-flight` (green) covers both
 * done and claim per the brief's literal wording; `--state-done` itself is
 * deliberately NOT used here — it's the Board's muted "already folded away"
 * gray, the opposite of the in-the-moment "this just happened" green an
 * event wants. */
export const EVENT_ICON: Record<EventKind, string> = {
  claim: '●',
  done: '✓',
  blocked: '⊘',
  error: '⊘',
  defer: '⏸',
  supersede: '↻',
};

export const EVENT_COLOR_VAR: Record<EventKind, string> = {
  claim: 'var(--state-flight)',
  done: 'var(--state-flight)',
  blocked: 'var(--state-stuck)',
  error: 'var(--state-stuck)',
  defer: 'var(--state-unowned)',
  supersede: 'var(--muted)',
};

export type EventSubject =
  | { kind: 'ticket'; id: string; label: string }
  | { kind: 'agent'; id: string; label: string }
  | { kind: 'thread'; msgId: string; label: string };

/**
 * Resolves an event message's subject, or `null` when it's dangling (law
 * #3 — never a fake/dead chip).
 *
 * - claim/done/error/blocked/defer -> the ticket they're `--of` (the same
 *   `msg_`/`req_` id-suffix convention `ticketState.ts`/`noteRelated.ts`
 *   already use). Only resolves if that ticket is actually in `requests`.
 * - supersede -> the thread/message it replaces (`supersedes`, falling
 *   back to `replyTo` per the same `of ?? replyTo` precedence
 *   `thread.ts`'s `buildTrail` already establishes). Only resolves if that
 *   message is actually in `messages`.
 * - No `MsgType` today represents an agent-referencing event (a join/
 *   roster event) — the `agent` variant stays part of the type (and
 *   Message.svelte's open-handler wiring) for when such an event type
 *   ships, rather than a case fabricated here for data that doesn't exist.
 */
export function resolveEventSubject(message: Message, requests: RequestRow[], messages: Message[], agents: Agent[]): EventSubject | null {
  if (TICKET_EVENT_TYPES.has(message.type)) {
    if (!message.of) return null;
    const reqId = message.of.replace(/^msg_/, 'req_');
    const req = requests.find((r) => r.id === reqId);
    return req ? { kind: 'ticket', id: req.id, label: req.id } : null;
  }
  if (message.type === 'supersede') {
    const targetId = message.supersedes ?? message.replyTo ?? null;
    if (!targetId) return null;
    const target = messages.find((m) => m.id === targetId);
    return target ? { kind: 'thread', msgId: target.id, label: target.summary } : null;
  }
  void agents; // reserved for the future agent-subject case (see doc comment above)
  return null;
}
