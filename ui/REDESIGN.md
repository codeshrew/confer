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
**The model — three collision-free modifier layers** (adopted 2026-07-19 from Compositor's studio
proofread-UI spec, relayed by Herald `P73QG8`; **supersedes the earlier `g`-leader**):

    Ctrl  = PANES     move focus between regions (the operator's vim-tmux-navigator reflex)
    bare  = CONTENT   vim motion INSIDE the focused pane (only its keys fire)
    Cmd   = APP       global — palette, views, help — works regardless of focused pane

- **Layer 1 · `Ctrl`+`h`/`j`/`k`/`l`** — move pane focus by BOUNDING-BOX GEOMETRY (nearest region on the
  pressed side, scored by primary-axis distance + 2× cross-axis misalignment) so it survives any responsive
  layout with no keymap edits. `Ctrl`+`]`/`[` cycle (ring); `F6`/`Shift+F6` a11y alias. Exactly one pane
  active, shown by a **focus chip**; clicking a pane focuses it.
- **Layer 2 · bare keys = the FOCUSED pane's own vocabulary** — only the focused pane's keys fire, so the
  same key means different things in different panes with zero conflict (the tmux model). Stream: `j`/`k`
  messages · `Enter` open peek · `f` focus reader; rail: `j`/`k` hubs; trail: `j`/`k`/`h`/`l` nodes;
  `gg`/`G` top/bottom within any list.
- **Layer 3 · `Cmd`** — `⌘K` palette · `⌘1`–`⌘5` views (Overview/Chat/Board/Fleet/Code, tmux-window feel) ·
  `?` grouped keyboard-help. Global.
- **The `g` leader is RETIRED** — a global `g`-prefix collides with per-pane bare keys (`gg`). Views moved to
  the `Cmd` layer. **Browser caveat:** some `Ctrl` chords are reserved (`Ctrl+L` = address bar); provide a
  `Cmd` alias where a `Ctrl` binding can't be reliably `preventDefault`ed.

**Robustness — the 4 gotchas, enforced in `keys.ts`:** (1) bind the keydown listener ONCE, read the pane
list through a ref, mutate via the functional updater — a listener that re-subscribes on pane-list change
DROPS EVERY OTHER KEYPRESS (the biggest footgun); (2) `isTypingTarget` guard — INPUT/TEXTAREA/SELECT/
`contentEditable` → let the key through, don't fire nav; (3) blur the active element on pane switch — else the
old pane keeps eating keys; (4) neighbour from geometry, never a hardcoded order.

**Everything button-reachable + glanceable shortcuts (operator directive 2026-07-19):** every action has an
on-screen control — full mouse parity, nothing keyboard-only — and **each control displays its own shortcut
inline** (a small `kbd` chip on the button / tab / menu item), so the model is learned passively just by using
the UI. `?` gives the full grouped cheatsheet; a persistent focus chip shows the active pane; a one-line
footnote states the model verbatim: *"Ctrl = panes · bare = content · Cmd = app · only the focused pane's keys
fire."* It must stay **learnable in one sentence** — if it can't be, the layering is wrong.

**Keyboard-architecture pass — done (2026-07-19).** `paneFocus.svelte.ts` is the Layer-1 engine (module
singleton, same pattern as `appState`) — 7 named panes registered: `rail` (HubRail) · `topic-list`
(LeftRail) · `stream` (ChatStream) · `thread-peek` (MetaThread) · `code-tree` (CodeTree) · `refs`
(ReverseIndexPanel) · `board` (Board). `topic-list` and `stream` gained NEW bare-key vocab in this pass
(`j`/`k`/`Enter`, matching the rail's existing pattern — neither had one before); `code-tree`/`refs`/`board`
register as Layer-1 focus targets only (no pre-existing bare-key nav to retrofit, native Tab/scroll +
mouse cover them, so nothing was invented from scratch there). The `g`-leader is fully retired — views are
`⌘1`–`⌘5` (`keys.ts`'s `viewForCmdNumber`); HubRail's own *local* `g g`/`G` "first/last hub" chord is
unaffected (it was never the retired leader, just reuses the same timeout constant). A persistent
`FocusChip` (crumb bar, right edge) names the focused pane. `Kbd.svelte` is the shared shortcut-chip
component — used on TopBar's view tabs (⌘1–⌘5) and a new "open in focus reader" button on each chat message
(the mouse-parity fix for `f`, which was keyboard-only before this pass).

**Browser-caveat resolution:** `Ctrl+K`/`Ctrl+L` are unconditionally reserved (address bar) in every
tested browser — `preventDefault()` never reaches them, so `F6`/`Shift+F6` is the ONLY reliable pane-cycle
fallback (`Ctrl+]`/`[` also works everywhere tested). `Ctrl+H`/`Ctrl+J` are reserved in Chrome (history/
downloads) but not universally — `e.preventDefault()` is still called (works in Firefox and in Chrome once
the page has focus in many configurations) with `F6`/`Shift+F6` as the documented fallback either way.
`Ctrl+1`–`9` is unconditionally reserved for tab-switching, which is exactly why `viewForCmdNumber` has NO
`Ctrl` alias — `⌘`/Cmd is the only binding for Layer 3 view switches, by design, not oversight.

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

## Piece 3 open items (one design question, one deferred-by-design)

Two things the mockup (`redesign-mockups/03-thread-nav.html`) shows that piece 3 doesn't render —
neither is a missing backend field to request; see the resolution below each, and the top-of-file
comments in `MetaThread.svelte`/`FocusReader.svelte`.

- **Foreign-hub trail nodes — a DESIGN question, not a backend gap.** The mockup tinted one trail
  node "foreign" (pulled in from `confer-jarvis-orbit`, mid-trail) — but on reflection this was the
  mockup overreaching past what the model can mean: a confer thread is a reply-hash root within ONE
  hub's own git log, and two hubs are two separate repos, so a reply chain literally cannot span
  hubs today (confirmed in `src/api.rs::thread` — hub-scoped, walks one hub's `thread_root`
  grouping). `--ref` code references already link across hubs (`RefHit.hub`), but a THREAD that
  spans hubs would be a genuinely new capability, not a missing projection. **Resolution: omit
  foreign-tint entirely — it isn't a real dimension of this graph.** What piece 3 DOES keep, and is
  real: cross-TOPIC hops (one hub, many topics) — those are the actual legibility win the mockup was
  reaching for, and they're fully wired (`↗ thread crosses into #topic` / `↩ resolves back in
  #topic`). Revisit foreign-tint only if cross-hub reply-threading ever becomes a real capability.
- **The focus reader's "seen" line — deferred to piece 4 by design, not blocked.** The mockup's
  gutter shows `seen ✓ all`. Real per-message `seenBy` now EXISTS — Herald shipped it (`b776c94`,
  design/48 #62, "real per-message seen-by on /api/messages from the published read-frontier") — so
  this isn't the CONTRACT GAP #58 synthesized-filler problem it looked like when piece 3 started.
  It's simply not wired here: read-state (the reader's "seen" line, Chat's synthesized filler, any
  "since you last looked" watermark) belongs together in piece 4, done holistically, not scattered
  as a one-off in piece 3. The gutter keeps author + refs (both real); "seen" lands for free once
  piece 4 wires the real projection app-wide.

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
