# Piece 8 — Fleet: the crew deck + the reusable agent dossier

**Status:** queued (build after piece 5, and reuses the piece-5 mini ticket card + piece-1 AgentNode).
**Specs:** `redesign-mockups/08-fleet-crew-deck.html` (the visual), this brief (the contract).
**Reviewer:** Jarvis, staged sub-commits as usual.

This piece establishes that an **agent is a composable type** (like ticket / note / code-ref): it has a
*mini* (the existing `AgentNode`) and a *full* (the **dossier**), and the dossier is a **reusable popover
reachable from anywhere an agent appears**. The Fleet *page* is the full deck; the dossier *popover* is the
per-agent inspector — they share one component.

---

## 8a — The crew deck (the Fleet page)

Organize the fleet **by machine** — agents are processes on hosts; grouping by host is the honest topology
(stable positions you learn; a whole machine going dark reads at once).

- **Machine bays** — one panel per host (`pop-os`, `Batman.local`, `Athena.local`, `Hestia.local`…), each
  holding its agent card(s). A bay whose agents are all down reads dimmed, with a "dark Nh" power indicator.
  A bay can hold >1 agent (build for it even though today's fleet is ~1:1).
- **Living presence card** per agent — REUSE `AgentNode`'s appearance-encoding, extended: avatar **breathes**
  when live (green glow), **fades** when stale (amber), goes **hollow/dashed** when down (red);
  `prefers-reduced-motion` kills the breathe. Card shows: name · role one-liner · liveness + heartbeat age ·
  trust chip · WIP chip · a small **activity sparkline** · tier-colored **hub-membership dots** (foreign reads
  foreign — law #2 on the deck). Clicking → the dossier popover.
- **Fleet vitals header:** N agents · M live · K down · H machines · trust posture ("all keys signed" / "N
  unverified").
- Semantic palette (settled): live=green, stale=amber, down=red, signed=green, mismatch=red. Trust framing
  (home/shared/foreign) stays its own axis.

## 8b — The agent dossier (reusable popover — the heart of this piece)

An overlay popover (like the ticket / focus-reader popovers) opened by clicking any agent **anywhere** — the
Fleet deck, an Overview `AgentNode`, a message author avatar, the seen-by roster, an @mention, the board's
fleet-filter rail. Same diagnose-this-agent view every time. `esc` closes → back where you were; `j`/`k`
prev/next agent when opened from a roster/deck.

**Content — be COMPLETE with everything confer actually exposes (operator directive):**
- **About** — render the agent's own **`roles/<role>.md` markdown profile** (their self-description) via the
  existing markdown pipeline. This is "the agent card in their own words."
- **On their plate** — REUSE the piece-5 **mini ticket card**:
  - **Carrying** (their claimed/in-flight requests, with mini progress track)
  - **Asking** (their open requests; unowned ones flagged amber — the *same* tickets that appear on the Board,
    for cross-view coherence)
  - Each mini card portals to its ticket popover. This is an **inspector, not a second board** — show their
    work + click-to-jump; full triage stays on the Board. Bounded/scrollable, not the whole backlog.
- **At-a-glance facts** — carrying count · closed-7d · hubs (real projections).
- **Activity chart** — real message-timestamp buckets over last N hours (see law-#3 flags).
- **Identity / presence side panel** — all real:
  - **role slug + aliases** (`confer whois`)
  - **confer version** they run — e.g. `0.6.9 (45a9c04)` (LAW-#3 FLAG below)
  - **signing key** fingerprint + trust state (confirmed / first-sight / mismatch)
  - **host** + whether it **matches expected** (roles file's expected host)
  - **watch** state — armed/reactive vs. not (LAW-#3 FLAG below)
  - **present in** — tier-colored hub list + last-seen per hub
  - **member since** · **last posted**

## Law #3 — real or honestly omitted, never faked

Real today (project from existing data): profile (`roles/*.md`), role/aliases, key + trust, host + expected,
hubs + last-seen, WIP/carrying/asking (RequestRows), last-posted, liveness/heartbeat.

**Verify the API serves these per-agent; if not, they become a small backend ask (ping Jarvis → Herald, like
the tier/sync fields) or are honestly omitted — NEVER fabricated:**
1. **`confer version` per agent** — confer knows it (`confer who` prints `confer X.Y.Z (sha)`), but confirm
   it's projected onto the agent shape in `/api/*`. If absent → omit or backend ask.
2. **Watch / armed state** — is "is this role actively watching" exposed? If not → omit or backend ask.
3. **Activity sparkline/chart** — needs message-timestamps bucketed per agent; same pattern as the Board's
   closure-throughput. Real if the messages are available to bucket; else backend ask.

## DoD

Reuse `AgentNode` + the piece-5 `TicketMiniCard` (do not reinvent). Dossier is one component used by the Fleet
page AND as the popover from every agent entry point (wire at least: Fleet deck, Overview node, seen-by roster).
Both themes (the palette), keyboard + mouse parity (`j`/`k` agents, `Enter`/click opens, `esc` closes; the
plate's mini cards are a focusable pane — `↑↓` select, `↵` opens the ticket popover), `prefers-reduced-motion`
(breathe + any pulse). Tests + e2e (bay grouping incl. a dark machine, living-state per liveness, dossier
opens from ≥2 entry points, plate mini-cards select→open, law-#3 fields real-or-omitted). Watch App.svelte size
— dossier is its own component. Commit per sub-item (8a deck, 8b dossier), report between.
