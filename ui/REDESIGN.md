# confer dashboard — redesign roadmap

**Branch:** `feat/dashboard-redesign` (off `review/0.8.0`, merges back for the 0.8.0 release)
**Owner:** Jarvis (design direction + review) · implementation delegated per piece
**Living doc:** each piece updates its status here as it lands. Document as we go.

---

## North star — the fleet is a place, not a list

The current dashboard ranks everything into flat lists and drops the fleet's structure on the
floor. A fleet has structure: hubs you trust differently, agents that live on fixed machines, work
that flows between them. This redesign makes that structure the interface. Three laws govern every
page:

1. **Position encodes identity, appearance encodes state.** *Where* a thing sits tells you what it
   is / who owns it / where it lives (stable — you navigate by memory). *How* it looks tells you how
   it's doing right now (live glow, down hollow, stale fade, WIP badge, trust border). Borrowed
   straight from Astrolabos' tier-2 "Instrument" model.
2. **Trust domains stay separate.** An internal hub and a foreign hub are different worlds and must
   read as different worlds — distinct framing, and a foreign hub can never mint a top-priority
   alarm without local corroboration. (0.8.0 review finding #1: the queue flattened trust.)
3. **Never fabricate state.** Every mark is a real fact folded from the log. No synthesized "seen"
   rosters, no poll-age masquerading as sync-freshness. If we don't know, we say so. (Lacuna
   empty-state contract + the 0.8.0 review's stale-lie / fake-overlay findings.)

Design system: **Tokyo Night** palette (matches the operator's whole environment + confer's theme),
PHI-scaled spacing, a shared appearance-encoding vocabulary (see `ui/redesign-mockups/01-overview.html`
for the canonical reference — tokens, agent-node states, trust framing, attention overlay).

---

## The human-experience problems this must solve

Named by the operator (2026-07-18), in priority order:

- **P1 · Hub switching is confusing, and won't scale.** The hub tab-row breaks down past a few hubs
  and it's disorienting to jump between them. Needs a model that scales to *many* hubs and keeps you
  oriented ("which world am I in?") — the trust-tiered domains from the Overview are the seed.
- **P2 · Getting lost in threads / meta-threads.** Clicking through a thread, then a meta-thread,
  then jumping to another place, the operator loses the thread (literally). Reconsider the
  **affordance**: is a thread a full page-swap, an inline expand, a side-peek panel, a pop-over? The
  goal is *never lose your place* — keep context while you look at detail, and always have a way back.
- **P3 · (review) trust-flattening, stale-"all-clear" lie, fabricated read-state.** Addressed by
  laws #2 and #3 above; each page enforces them.

---

## Sequence — one piece at a time (design → build → self-review → commit → tick here)

| # | Piece | What it establishes / fixes | Status |
|---|-------|------------------------------|--------|
| 1 | **Foundation + Overview** | The Tokyo Night token layer + appearance-encoding vocabulary as shared CSS/components; the fleet-map Overview (laws #1–3). Reference: `redesign-mockups/01-overview.html`. | ✅ done |
| 2 | **Hub navigation & scale (P1)** | Replace the tab-row with a model that scales to N hubs and keeps trust-domain orientation. Design the switcher (grouped by tier? command-palette? persistent rail?). | ○ queued |
| 3 | **Thread / meta-thread nav (P2)** | Redesign the affordance so you never lose your place — side-peek / pop-over / inline expand + breadcrumb + back. The meta-thread (reference-graph) becomes legible. | ○ queued |
| 4 | **Chat** | Real read-state: client-side "since you last looked" watermark (localStorage) + real seen-by projected from agents' `ack` read-frontiers (kill the synthesized filler). Inline refs anchored to prose. | ○ queued |
| 5 | **Board** | Claim / WIP / lifecycle made spatial and honest; the ack-story + claim-norm surfaced (design/48/49). | ○ queued |
| 6 | **Code** | The ref/patch view under the shared language; density gutter, reverse index. | ○ queued |
| 7 | **Fleet** | Per-agent detail as the drill target of an Overview node. | ○ queued |

Later phases (backend, out of this branch's frontend scope but noted): real `/api/attention`,
per-hub sync-health projection, seen-by projection, shadow-repo surfacing.

---

## Backend gaps (found while building piece 1, resolved 2026-07-18)

The mockup (`redesign-mockups/01-overview.html`) showed two things the web API didn't project when
piece 1 first shipped. Both are now RESOLVED — Herald landed them at `071e027` (request MRCKY3):
per-hub trust tier + git-sync freshness on `/api/hubs` and `/api/overview` (design/48 §2-3). Kept
here for history; see `types.ts`'s `Hub.tier`/`Hub.sync` for the shipped shape.

- **Per-hub trust tier (home vs. foreign).** `confer trust own|shared|foreign` (`src/tiers.rs`) is
  still stored LOCAL-only (`~/.confer/tiers.json`, by design — a peer can't declare itself trusted),
  but the SERVER's own configured tier for the hub it's serving is now projected as `Hub.tier: 'own'
  | 'shared' | 'foreign' | null`. Overview renders `own`/`shared` with the solid home frame,
  `foreign` with the dashed foreign frame, and `null` (never classified) with a distinct neutral
  frame — not folded into "home" just because it isn't explicitly foreign.
- **Per-hub sync-freshness.** `Hub.sync: { lastFetchedSecs, behind, pending, reachable } | null` —
  every field independently nullable, straight from Herald's comment on `hub_json`: "a null is
  'unknown', never a fabricated zero/true — so a stale hub can't render as a calm all-clear." Each
  domain card now shows a real per-hub sync line; the DASHBOARD's own poll age ("dashboard polled Ns
  ago" in the masthead) is kept as a separate, clearly-labeled fact — it answers "did the browser
  last ask recently", not "is this hub's picture current".

Law #3 enforcement in the render, not just the fetch: `sync === null` → "sync unknown" (no other
line shown); any individual null field inside a present `sync` → that field's own "… unknown" text,
never silently dropped or defaulted; `reachable === false` or `behind > 0` → amber warn styling
(can't read as healthy). `tier === null` → "unclassified" label, neutral frame, never defaulted to
"home".

---

## Process

- **Each piece:** design intent (grounded in the three laws) → implement in real Svelte on this
  branch → self-review (build clean, `npm --prefix ui run check`, renders, no console errors,
  both themes, reduced-motion, keyboard focus) → commit → tick the table above with the commit.
- **Reviewable increments:** each piece is its own commit(s) so Herald can review/merge piecewise
  for 0.8.0. Never fabricate data — if the backend doesn't project something yet, degrade honestly
  and note the backend gap here rather than faking it.
- **Law #3 is a HARD REVIEW GATE, every piece (operator directive, 2026-07-18).** No piece is
  approved until the reviewer has verified *in source* that every interface element is bound to real
  data — no hardcoded placeholder state, no plausible-but-invented value. When the backend can't
  provide something, the pattern is: (1) degrade honestly in the UI, (2) log it under "Backend gaps"
  above, (3) turn it into a real backend ask. That loop is working — piece 1's deferred trust-tier
  became request `MRCKY3` to Herald, who is now projecting a real `tier` field rather than us faking
  one. A fake pixel is a bug, not a shortcut.
- **Design fidelity:** the mockup(s) under `redesign-mockups/` are the visual spec. Match the token
  values, the appearance-encoding vocabulary, and the spacing exactly; deviate only with a noted reason.

## Build / test

```
npm --prefix ui install
npm --prefix ui run build      # produces ui/dist/index.html (embedded by build.rs)
npm --prefix ui run check      # svelte-check / types
npm --prefix ui run test       # vitest component tests
```
