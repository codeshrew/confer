# Tweak — the composable EVENT type in the chat stream

**Goal:** elevate lifecycle/system events in the chat stream into the composable card system — a fourth
"type" (alongside ticket / note / code-ref) whose defining trait is that **an event is ABOUT another
entity, and clicking its subject opens that subject's popover.** Build end-to-end.

## What an event is

The lifecycle/system message types — already rendered as compact `.sysline`s in `Message.svelte`
(`SYSLINE_TYPES = claim / done / error / defer / supersede`, plus `blocked`). Each names a **subject**:

- `claim` / `done` / `error` / `blocked` / `defer` → the **request/ticket** they're `--of` → **TicketFullPopover**
- an event that references an **agent** (e.g. a join/roster event, or "X did Y") → the **AgentDossier**
- an event that references a **thread/message** (`supersede`, reply-ref) → the **thread peek**

## Build

1. **Event row (elevate the sysline):** keep it compact/ambient — an icon by kind, the actor, the verb, and
   the **subject as a distinct clickable chip**. Color by the semantic state palette:
   - `done` → green ✓ · `claim` → green ● (in-flight) · `blocked`/`error` → red ⊘ · `defer` → amber · `supersede` → muted.
   - e.g. `✓ Jarvis closed [98XCNF] 0.8.0 review` where `[98XCNF]` is a clickable pill.
2. **Clickable subject → the subject's popover.** This is the core ask. Clicking the subject chip opens the
   RIGHT popover for the subject type: ticket→TicketFullPopover, agent→AgentDossier, thread→peek. Reuse the
   existing open handlers (App.svelte's `openTicketPopover`/`openDossier`/peek) — an event has **no popover of
   its own**; its "full view" IS its subject's. That's the composable insight: events are pure portals.
3. **Subject resolution (law #3):** resolve the subject from real data — `of`/`replyTo`/refs. If the subject
   can't be resolved (dangling), render the event WITHOUT a clickable chip (plain text) — never a dead link.
4. **Row / mini / (no full):** the event has a **row** (in the stream) and can appear as a **mini** in related
   contexts later (e.g. an agent's recent-activity), but NO full-of-its-own — clicking always lands on the
   subject. Keep the row component reusable for that future mini use.

## Keyboard (SETTLED — my call on the operator's open question)

Events are **NOT primary `j`/`k` stops** in the stream — keeping the message-reading flow on substantive
notes/requests (the operator flagged that making every event a keyboard stop "might not be the best flow" —
agreed). BUT the subject chip is **focusable + Enter-activatable** (Tab-reachable, `:focus-visible`), so
events are fully keyboard-operable without cluttering the `j`/`k` rhythm. If the operator later wants events
in the j/k ring, it's a one-line inclusion — build it so that's easy, but default OFF.

## Note — the popover-BACK behavior is a SEPARATE research topic

Opening a subject popover FROM the stream (event → ticket) esc-closes back to the stream fine. The
popover→popover back-stacking bug (dossier → ticket → can't get back) is being handled separately as the
navigation/state-history research topic — don't try to solve that here; just open the subject popover with
the existing handlers.

## DoD

Both themes, semantic palette, law #3 (dangling subject → plain, not fake), subject chip keyboard-reachable
(Tab/Enter) but not a j/k stop, tests + e2e (each event kind → correct popover; dangling → plain text;
keyboard reach). Reuse the existing popover open handlers + TicketMiniCard/dossier. Build end-to-end, verify
live both themes, report. I'll screenshot-verify against this spec.
