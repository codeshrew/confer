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

**Cross-cutting: keyboard-first (operator directive, 2026-07-18).** The operator lives in vim, modern
tmux, and macOS — the dashboard must be operable end-to-end from the keyboard, designed in from each
piece, never retrofitted. The model (build up across pieces, discoverable via a `?` / which-key overlay):
- **`⌘K` command palette** — fuzzy jump to any hub / thread / action (fzf-like), the macOS-native entry point.
- **vim motions wherever there's a list** — `j`/`k` move selection, `g g`/`G` top/bottom, `/` filter-in-place,
  `Enter` or `l` open/drill, `Esc` or `h` back/up-a-level. (Rail hubs, chat messages, board rows, code refs.)
- **`g` leader for views + actions (SETTLED with operator 2026-07-18)** — the operator's tmux prefix is
  `Ctrl+Space`, which clashes with browser chords, so the web dashboard uses a vim-native **`g` leader**.
  View switching: **`g` then a number `1`–`5`** (Overview/Chat/Board/Fleet/Code — mirrors tmux window-number
  muscle memory) is primary, with first-letter aliases where unambiguous. Action keys (`c` claim / `d` done)
  where they apply.
- **`?`** opens a which-key-style overlay of available keys, so nothing has to be memorized blind.
- Every affordance reachable without a mouse — this composes with the `:focus-visible` work already in place.
First slice lands in piece 2: rail `j`/`k` navigation + `⌘K` palette + `g`+number view switching + `?` overlay.

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
| 2 | **Hub navigation & scale (P1)** | Replace the tab-row with a persistent, trust-tiered rail (Home/Shared/Foreign/Unclassified, real health dots) + workspace tint; first slice of the keyboard-first layer (⌘K palette, rail j/k/gg/G, g+number views, ? which-key). Reference: `redesign-mockups/02-hub-nav.html`. | ✅ done |
| 3 | **Thread / meta-thread nav + focus reader (P2)** | SETTLED affordance = **side-peek** (stream stays the anchor, never a page-swap) + a REAL breadcrumb trail (from `Message.of`/`replyTo`, walked with h/l/j/k) + the meta-thread drawn as a legible reference-graph trail (cross-topic hops marked). Plus a **focus reader** (`f`, from anywhere) for deep single-message reading. Reference: `redesign-mockups/03-thread-nav.html`. | ✅ done |
| 4 | **Chat** | Real read-state: client-side "since you last looked" watermark (localStorage) + real seen-by projected from agents' `ack` read-frontiers (kill the synthesized filler). Inline refs anchored to prose. | ○ queued |
| 5 | **Board** | Claim / WIP / lifecycle made spatial and honest; the ack-story + claim-norm surfaced (design/48/49). | ○ queued |
| 6 | **Code** | The ref/patch view under the shared language; density gutter, reverse index. | ○ queued |
| 7 | **Fleet** | Per-agent detail as the drill target of an Overview node. | ○ queued |

Later phases (backend, out of this branch's frontend scope but noted): real `/api/attention`,
per-hub sync-health projection, seen-by projection, shadow-repo surfacing.

**Piece 2 scope note:** HubRail (the persistent rail) and the workspace tint are desktop-only
(≥1024px) — below that, TopBar's original horizontal hub-pill row is kept as-is, unredesigned, as
the mobile fallback (see TopBar.svelte). Mobile hub-switching wasn't in piece 2's brief; revisit if
a later piece needs it.

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

## Backend gaps (found while building piece 3, open)

Two things the mockup (`redesign-mockups/03-thread-nav.html`) shows that the current data model
can't honestly back yet. Both degrade to an honest omission rather than a fabricated pixel — see
the top-of-file comments in `MetaThread.svelte` and `FocusReader.svelte`.

- **Foreign-hub trail nodes.** The mockup tints one trail node "foreign" (pulled in from
  `confer-jarvis-orbit`, mid-trail). `/api/thread` (`src/api.rs::thread`) is hub-scoped — one `hub`
  query param, walks only that hub's own message log via `thread_root` grouping — and neither
  `ThreadNode` nor `Message` carries a `hub` field. A node in one hub's thread literally cannot be
  from another hub with the current contract, so no foreign-tint rendering exists in the trail. The
  fix would need either cross-hub reply-threading (a much bigger backend change) or, more cheaply, a
  `hub` field on `/api/thread`'s response IF confer ever does grow cross-hub replies. Piece 2's
  `--foreign-frame`/`--foreign-glow` tokens are already in place to receive this the moment there's a
  real signal.
- **The focus reader's "seen" line.** The mockup's gutter shows `seen ✓ all`. The only seen-data
  anywhere in this app is ChatStream's own `buildSeenEntries`, which its own code comment already
  flags as CONTRACT GAP #58 — synthesized filler, explicitly the thing piece 4 ("real seen-by
  projected from agents' `ack` read-frontiers") is scoped to fix for real. Extending that same
  synthesis into a brand-new piece-3 surface would be the wrong direction under law #3, so the
  reader's gutter omits "seen" entirely for now — it becomes a small, free addition once piece 4
  lands a real per-message roster.

Piece 3's breadcrumb/h-l-j-k navigation, by contrast, needed NO new backend work: `Message.of`/
`replyTo` (already served) plus the fact that `/api/thread` already returns the WHOLE connected
reply-hash graph for any anchor within it (confirmed in `src/api.rs`) meant one fetch per peek
session was enough — `thread.ts`'s `buildTrail`/`pathToRoot`/`childrenOf` do the rest client-side,
purely, from data that already exists.

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
