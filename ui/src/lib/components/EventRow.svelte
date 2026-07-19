<script lang="ts">
  // Piece 9 (ui/redesign-mockups/09-event-type-BRIEF.md) — the composable
  // EVENT type's Row: a compact, ambient line for a lifecycle/system message
  // (claim/done/error/blocked/defer/supersede) whose SUBJECT — the ticket,
  // agent, or thread it's about — is a distinct, clickable chip. An event
  // has NO popover of its own; clicking the chip opens the subject's own
  // popover via `onOpenSubject` (App.svelte's existing openTicketPopover/
  // openAgentDossier/thread-peek handlers, dispatched by ChatStream — see
  // its own `openEventSubject`). That's the composable insight: events are
  // pure portals.
  //
  // Row only for now — no Full (clicking always lands on the SUBJECT's
  // full view, never the event's own), but built reusable for the "mini"
  // context the brief calls out for later (e.g. an agent's recent-activity
  // list), the same Row/Mini/Full shape piece 5's ticket card trio settled.
  import type { Agent, Message } from '../types';
  import { EVENT_COLOR_VAR, EVENT_ICON, type EventKind, type EventSubject } from '../eventSubject';
  import { formatClock, formatIso8601 } from '../format';

  interface Props {
    message: Message;
    fromAgent?: Agent;
    /** Law #3 — `null` when the subject can't be resolved from real data
     * (a dangling `of`/`supersedes`/`replyTo`). Renders as plain text, never
     * a dead link. */
    subject: EventSubject | null;
    highlight?: boolean;
    onOpenSubject?: (subject: EventSubject) => void;
  }

  let { message, fromAgent, subject, highlight = false, onOpenSubject }: Props = $props();

  const kind = $derived(message.type as EventKind);
  const fromColor = $derived(fromAgent?.color ?? 'var(--muted)');
  const fromDisplay = $derived(fromAgent?.display ?? message.from);

  const SUBJECT_NOUN: Record<EventSubject['kind'], string> = { ticket: 'ticket', agent: 'agent', thread: 'thread' };
</script>

<div class="event-row" class:pulse={highlight} data-type={message.type} data-msg-id={message.id} data-testid="event-row">
  <span class="tick" style="color:{EVENT_COLOR_VAR[kind]}" aria-hidden="true">{EVENT_ICON[kind]}</span>
  <span class="evtext">
    <b style="color:{fromColor}">{fromDisplay}</b> {message.summary}
    {#if subject}
      <button
        type="button"
        class="subject-chip"
        title="Open the {SUBJECT_NOUN[subject.kind]}: {subject.label}"
        data-testid="event-subject-chip"
        onclick={(e) => {
          e.stopPropagation();
          onOpenSubject?.(subject);
        }}
      >
        {subject.label}
      </button>
    {/if}
  </span>
  <span class="ts" title={formatIso8601(message.ts)}>{formatClock(message.ts)} · {message.type.toUpperCase()}</span>
</div>

<style>
  .event-row {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 3px 10px;
    margin: 2px -10px;
    color: var(--muted);
    font-size: 12.5px;
  }
  .event-row .tick {
    width: 24px;
    display: grid;
    place-items: center;
    font-weight: 700;
  }
  .event-row b {
    color: var(--text);
    font-weight: 600;
  }
  .evtext {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .subject-chip {
    display: inline-flex;
    align-items: center;
    max-width: 260px;
    margin-left: 4px;
    padding: 1px 7px;
    border: 1px solid var(--border-2);
    border-radius: 999px;
    background: var(--panel-2);
    color: var(--accent);
    font: 600 11px/1.5 var(--mono);
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    vertical-align: middle;
  }
  .subject-chip:hover,
  .subject-chip:focus-visible {
    color: var(--text);
    border-color: var(--accent);
    background: var(--panel-3);
  }
  .event-row .ts {
    margin-left: auto;
    flex: 0 0 auto;
    font: 500 10.5px/1 var(--mono);
    color: var(--faint);
  }
  /* Same brief highlight pulse Message.svelte's own rows use (design/41
     Phase 0 item 4) — duplicated here rather than shared across the
     component boundary, since Svelte scoped `@keyframes` don't cross files;
     `prefers-reduced-motion` is respected upstream (ChatStream never sets
     `highlight` true in that case), so no separate override is needed. */
  @keyframes event-row-pulse {
    0% {
      box-shadow: inset 0 0 0 0px var(--accent);
      background: color-mix(in srgb, var(--accent) 20%, var(--panel));
    }
    100% {
      box-shadow: inset 0 0 0 0px var(--accent);
      background: transparent;
    }
  }
  .event-row.pulse {
    animation: event-row-pulse 2s ease-out;
    border-radius: 6px;
  }
</style>
