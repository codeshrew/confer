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

## Cross-cutting: the semantic state palette (settled 2026-07-19)

Work/agent state colors must be **consistent tool-wide AND mutually distinct** (operator flagged open≈in-flight
and unowned≈stuck as too-close). One meaning per hue, spread across the wheel:

| state | hue | dark | light | means |
|-------|-----|------|-------|-------|
| **open / available / fresh** | cyan | `#7dcfff` | `#2959aa` | unclaimed, not a problem |
| **in-flight / claimed / active / live** | green | `#9ece6a` | `#4f6d2e` | being worked / healthy (= live agent) |
| **needs-owner / attention / warn / stale** | amber | `#e0af68` | `#8a6c2f` | soft alert — pick up / getting old |
| **stuck / blocked / critical / down / mismatch** | red | `#f7768e` | `#d81f57` | hard problem |
| **done / resolved** | muted grey | `#565f89` | `#7a80a0` | closed, collapsed |
| **metrics (throughput etc.)** | teal | `#2ac3de` | `#0f7c93` | neutral data-viz — NOT a state, keeps green = in-flight only |

Trust framing keeps its own axis (home/shared vs foreign) — that's orthogonal to work-state. When a piece
touches an older component whose colors predate this (e.g. an AgentNode WIP badge that was cyan), realign it.

## Cross-cutting: the composable card system (settled 2026-07-19)

Every content **type** (ticket, code-ref, note) exists at THREE zoom levels — one identity, one color/lifecycle
language, three densities:

1. **Row** — one line in a list/stream (`scan` many). What the board/chat lists already use.
2. **Mini card** — rich but small: a glanceable summary + **mini progress/state** + who/age; **embeds inside
   other views and PORTALS into the full view** (never a dead-end link). `CodeRefCard` is the existing
   precedent (mini code view → full code view); generalize it to ticket + note.
3. **Full popover** — the whole item: the ticket **lifecycle popover** (Requested→Claimed→Done, adapts per
   state: open/in-flight/done-with-resolution/blocked-with-red-branch), meta, teaser — a **launchpad, not a
   container** (jumps out to thread / focus reader / code; shows no thread list or full body itself).

**Composition:** a detail popover shows a **Related column** of mini cards (its tickets · code · thread) as a
**focusable pane** — `↑↓`/`j k` select, `↵` opens that item's full view. This is the connective tissue that
makes the enriched note/thread popover (piece 6) rich: body + related mini cards, assembled from the shared set.

**Four zoom levels of a request:** Board (all work) → Ticket card (one item's status) → Thread/peek (the
conversation) → Focus reader (the words). Each owns one level; no duplication. Refs:
`redesign-mockups/05-board-cockpit.html` (cockpit + ticket popover), `05-composable-cards.html` (the card system).

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
| 4 | **Chat + meta-thread** | Item 0 (pane-focus leak bug, fixed). Item 1 (meta-thread minimap, `redesign-mockups/04-metathread-minimap.html`). Item 2 (real read-state: watermark + real seenBy + completionist-safe detail-viewed — see the CONTRACT GAP #58 note above). Item 3 (sticky day header, true summary density — chips only, no rendered blocks — inline refs anchored to prose, minimap-styled focus-reader prev/next; `redesign-mockups/04-chat-glance.html`). All four done. | ✅ done |
| 5 | **Board = triage cockpit + the shared card system** | Reframe the flat ticket list as a **cockpit**: stat strip (Open/In-flight/Stuck/Needs-owner) → real visuals (assignee+requester load, closure throughput) → actionable work grouped by state, **DONE collapsed**. Establish the **card system** (ticket row/mini/full) + the **ticket lifecycle popover** (Requested→Claimed→Done, adapts per state). Left column → **Fleet-as-filter** rail (collapsible), no channel list. Refs: `redesign-mockups/05-board-cockpit.html`, `05-composable-cards.html`. Item 5a (Row/MiniCard/FullPopover, `152c768`), 5b (the cockpit rewrite, `d2bf56c`), 5c (Fleet-as-filter rail + combined stat/agent filters + chip row/clear-all) all done. | ✅ done |
| 6 | **Enriched note / thread popover** | Make the note/thread popover rich by **composing** the card system: the note body + a keyboard-selectable **Related** column of mini cards (tickets · code · thread). The `f` reader stays pure reading; this is the *inspect* view. Ref: `05-composable-cards.html`. `NotePopover.svelte` (overlay, same convention as TicketFullPopover) + new `CodeRefMini.svelte`; reuses piece 5's `TicketMiniCard` verbatim, proving it portable as designed. Reached via each note's own "open note" button (Message.svelte) — notes only, a ticket keeps its own Full popover. | ✅ done |
| 7 | **Repos — integrity + gravity** | Repos redesigned around two real questions: **integrity** (tracked / registered-not-local / shadow tiers — `repoIndex.ts`'s pure diff of `getRepos()` vs `getCodeFiles()`, no new fetch) and **gravity** (real per-repo reference density). Drill-in (`RepoDetailPopover.svelte`) shows real hot files (one `getRefs(hub, repo, true)` call, the SAME repo-rollup shape CodeLens's own item 2.4 already established) + identity + "open in code view", routing through the shared `codeState` store straight into the existing Code view. Mutating actions (map/register/pull) kept as deferred, dashed intent — `serve` is read-only. Ref: `redesign-mockups/07-repos-integrity-gravity.html`. | ✅ done |
| 8 | **Fleet — the crew deck + the reusable agent dossier** | An agent is a composable type too: a *mini* (`FleetPresenceCard`, extending `AgentNode`'s appearance-encoding) and a *full* (`AgentDossier.svelte`, a reusable popover reachable from anywhere an agent appears). 8a: machine bays (`fleetBays.ts` — real `expectedHost`/`lastHost` grouping, a bay dark only when every real occupant is down), living presence cards (breathe/fade/hollow, real trust+WIP chips, a real activity sparkline, real cross-hub hub-membership dots). 8b: About (real `Agent.desc`, honestly NOT claimed as a confirmed `roles/*.md` source — flagged, not guessed), On their plate (real carrying/asking via `fleetDossier.ts`, reusing `TicketMiniCard` verbatim), a real activity chart, and an identity/presence side panel (trust, host-match, real cross-hub last-seen via `attention.ts`'s new `agentPresence`). `confer version`, watch/armed state, and a signing-key fingerprint were honestly omitted at first (not on the `Agent` shape yet) — Herald shipped all three onto `agent_row_json` same-night (commit `32ef9a4`, request D56E1E) and they're now wired into the identity panel (confer version row, watch armed/idle row, shortened key fingerprint under signing key), each still honest-nullable — a `null` row is genuinely omitted, never shown as a fake "unknown". Herald then shipped the real `roles/<id>.md` body too (`profileMarkdown`, commit `1318664`) — the dossier's About now renders it through the same markdown pipeline a message body uses, captioned `roles/<id>.md`, falling back to the one-line `desc` (captioned plain "about") when no profile is written, and rendering nothing at all when neither exists. Every field the brief originally flagged as a backend ask is now real and wired. Wired at 3 entry points: the Fleet deck, an Overview `AgentNode` (opens the dossier in place, no navigation), and a message's seen-by roster. Refs: `redesign-mockups/08-fleet-crew-deck.html`, `08-fleet-BRIEF.md`. | ✅ done |

| 9 | **The composable EVENT type** | The lifecycle/system message types (claim/done/error/blocked/defer/supersede — Message.svelte's `.sysline`s) elevated into the card system as a fourth type: an event is ABOUT another entity, and has no popover of its own — clicking its subject opens the SUBJECT's popover. New `eventSubject.ts` (pure, law-#3: resolves a ticket subject via the real `of` pointer, a thread subject via `supersedes`/`replyTo`, returning `null` — no chip, plain text — when it's dangling) + new `EventRow.svelte` (Row-only per the card trio's shape; icon+color by the semantic state palette — done/claim green, blocked/error red, defer amber, supersede muted — reusing the SAME `--state-*` tokens piece 5 established, not a second palette). Dispatches to the EXISTING open handlers (`onSelectTicket`/`onOpenAgent`/`onSelectMessage`) via ChatStream's new `openEventSubject` — an event never gets its own popover. Fixed a real pre-existing gap along the way: `blocked` was missing from the old ad-hoc sysline-type set and rendered as a full message bubble instead of a compact row. Keyboard: the subject chip is a real `<button>` (Tab-focusable, Enter-activatable) but events are deliberately NOT primary `j`/`k` stops in the stream (a single flag, `EVENTS_ARE_KEYNAV_STOPS`, folds them back in later if wanted). Ref: `redesign-mockups/09-event-type-BRIEF.md`. | ✅ done |

| 10a | **The overlay back-stack (Phase A — in-memory)** | Fixed the reported bug: Fleet → click an agent (dossier) → click one of their tickets used to REPLACE the dossier instead of stacking over it, so `esc` landed nowhere. New `overlayStack.svelte.ts` (Svelte 5 runes: `push`/`replace`/`pop`/`clear`/`top`, in-memory only — no URL sync yet, that's Phase B) replaces the `dossierOpen`/`ticketPopoverOpen`/`notePopoverOpen` booleans: opening a ticket FROM WITHIN the dossier or a note now `push`es a new frame on top (the parent survives underneath — only the TOP of the stack ever renders, like a real card stack), while a top-level open or `j`/`k`-navigating within the same popover `replace`s in place (no nesting). `esc`, each popover's close button, and a new visible "‹ back" affordance (shown only when nested) all `pop` exactly one layer. `whichKeyOpen`/`focusReaderOpen` stay plain booleans, per the doc's own call — not nested, not migrated. Caught and fixed a real cascading-listener bug along the way: popping a frame can synchronously reveal a parent whose OWN always-attached `<svelte:window onkeydown>` Escape handler would otherwise process the SAME keydown too (`stopImmediatePropagation` in all three popovers' Escape case). Ref: `redesign-mockups/10-navigation-history-RESEARCH.md`. Phase B (hash-based URL sync + deep-linking + a real `popstate` listener — the doc's own claim that browser-back "just works" from `pushState` alone is wrong) is a separate, not-yet-started follow-up. | ✅ done |

| 11.1 | **Code view — Phase 1: the anchored reader** | The #1 reported Code-view fix: clicking a code conversation used to yank the operator into Chat, abandoning the code. Killed that. `ReverseIndexPanel.svelte` evolved (not replaced — "reuse the existing composable pieces") with a new `anchored` mode, on ONLY while actually in Code view: one conversation is always focused and shown as a full card (real avatar/color via a new `agents` prop, kind badge colored by the shared per-kind palette). The rest render as VISIBLE, scannable accordion rows (post-verify tweak — Jarvis's call: hiding them behind a bare count fights the pane's whole point, which is to scan and flow through the discussion; only 6+ hits fold the tail behind a "‹ N older" reveal) — `j`/`k` steps focus between them, clicking a row expands it and collapses the prior. A locked scope header (`▤ whole file` / `▐ L44–49`) uses the existing `backToWholeFile` handler, unchanged. "open full thread ›" — the expanded card's own explicit link, wired to the SAME `openHitInChat` Chat's own ref-chip flow already uses — is the ONLY thing that still navigates to Chat; every other click (a row) only ever changes FOCUS. Chat's own inline-ref-chip lookup is completely unaffected (`anchored` defaults `false`). Caught and fixed a real focus-loss bug along the way: clicking a row/pill unmounts it (it becomes the expanded card), silently dropping keyboard focus to `<body>` — a subsequent `j`/`k` press would never bubble back into the panel's own handler without explicitly re-focusing the panel root after the click. Ref: `redesign-mockups/11-code-view-BUILD-BRIEF.md`, `11-code-anchored-conversations.html`. | ✅ done |
| 11.2 | **Code view — Phase 2: the powered gutter** | Replaced the bare per-line density count with the three-channel map (mock 12): **shape = scope** — a new `codeGutter.ts` (`buildGutterEntries`) collapses hits sharing an IDENTICAL range into one entry (a bracket spanning the lines, or a tick for a single line), whole-file hits get their own file-lane above line 1, never a line's gutter. **Color = meaning** — `hitColorVar` (moved out of `ReverseIndexPanel.svelte` into `codeGutter.ts` so Phase 1's reader and Phase 2's gutter share ONE definition) reuses piece 9's `EVENT_COLOR_VAR` palette for claim/blocked/error/defer/supersede, but — **post-verify fix (Jarvis's live-verify catch)** — NOT for `done`: piece 9's palette answers "what just happened" (`done` = green, in-the-moment), but the gutter needs "what STATE is this conversation in right now" (`done`/resolved = grey `--state-done`, same as a resolved request), a different semantic axis on the same tokens. Also fixed: when several hits with different colors collapse into one entry (identical range), the entry used to just take `hits[0]`'s color — hit order deciding the color let a range that was really `[note, note, resolved-request, done]` (nothing open) read as green "in-flight" purely because a `done` hit happened to sort first. New `entryColorVar` picks the MOST-ACTIONABLE hit's color instead (`STATE_PRIORITY`: blocked/error > open/needs-owner > in-flight > note > resolved) — a range never reads calm/resolved while any of its conversations still need attention, covered by a named unit test reproducing Jarvis's exact scenario. Also added (this pass): the active-range highlight (mock 11/12's `.cl.act`/`.br.act`/`.tab.act`) — a new `activeScope` prop (App.svelte's existing `refContext`, passed straight through, no new state) drives `.act` on the file-lane/row/bracket-or-tick/tab currently open in the Phase-1 reader; confirmed live in both themes via a new e2e test (`code-gutter.spec.ts`) that clicks a range tab and asserts the highlight actually appears in the rendered DOM — it was genuinely missing before, not a display bug. **Column = overlap** — genuinely DIFFERENT overlapping ranges get distinct side-by-side bracket columns via classic greedy interval coloring, never blurred into one. The range tab (`3 · RE PI CO`) is the click target into the Phase 1 reader, now opening the entry's REAL full range (a correctness fix over the old per-line click, which narrowed the reader to just the clicked line even when the actual ref spanned many). Hover = a native title tooltip (who/count/latest, law #3 — real data, no custom UI needed). Drift marker — law #3: a dashed bracket edge (or, for a single-line tick, a dashed outline ring) shows ONLY when a real hit's `staleness === 'changed'`, never decorative. Verified against REAL mock fixtures, not just synthetic unit ranges — added a genuinely overlapping second range and a genuinely drifted hit to `mock.ts`'s existing PlateBundle.swift fixtures (discovered along the way: mock `getCode` always renders the same fixed lines regardless of file/sha, so a pre-existing plates.py 'changed' fixture never actually intersected visible code — demonstrating drift needed a hit on lines that are really shown). Ref: `redesign-mockups/11-code-view-BUILD-BRIEF.md`, `12-code-gutter-and-time.html`. Phases 2b (minimap) through 5 (sidebar timeline) are queued, one phase at a time, each screenshot-verified before the next. | ✅ done |
| 11.2b | **Code view — Phase 2b: the conversation minimap** | The far-edge strip (mock 12's `.minimap`): the whole file compressed, one colored segment per gutter entry, a real viewport indicator, click-to-scroll. New `codeMinimap.ts` — `buildMinimapSegments` reuses Phase 2's `GutterEntry`/`entryColorVar` VERBATIM (one state palette, not a second one drifting from the gutter's), positioned against the file's real first/last line numbers (a snippet doesn't necessarily start at line 1). Built off the FULL gutter-entry set, not whatever's scrolled into view — the point is that Phase 3's future collapse (not built yet) never makes a folded cluster invisible here. Law #3 caught two real edge cases during build: (1) an entry whose range falls ENTIRELY outside the loaded file (a genuinely `staleness: 'offline'` ref to lines that no longer exist here) is skipped outright rather than clamped into a fake edge sliver — verified against the mock's own PlateBundle.swift `[10,15]` offline fixture, which produced a wildly negative, off-page position before the fix; a PARTIALLY overlapping entry is clamped to the real visible portion instead of dropped. (2) The minimap has no room for the gutter's own side-by-side overlap columns at 10px wide — overlapping segments (PlateBundle.swift's real `[44,49]`/`[44,46]`/`[48,48]` fixtures) used to let the widest one silently swallow clicks meant for a narrower one nested inside it; segments now render largest-first/smallest-last so a narrower one always paints on top and stays clickable. The viewport indicator (`computeViewportIndicator`, kept as pure geometry separate from the DOM measurements that feed it) degrades honestly to "whole file in view" when nothing's been measured yet, rather than a divide-by-zero or a fabricated sliver — confirmed for real via `scrollEl`/`codeEl` bind:this + scroll/resize listeners in `CodeLens.svelte`. Confirmed live in both themes (screenshot-verified: 3 real segments positioned/colored correctly, viewport indicator reading real 0–100% coverage on the mock's short fixture file). Ref: `redesign-mockups/11-code-view-BUILD-BRIEF.md`, `12-code-gutter-and-time.html`. Phase 3 (collapse) is next. | ✅ done |
| 11.3 | **Code view — Phase 3: PR-style collapse** | Roll-our-own collapse per `11-code-collapse-RESEARCH.md`'s recommendation (Shiki has no fold logic; CodeMirror 6 would cost +93KB and discard the custom density gutter). New `codeCollapse.ts` — pure span/gap math, same split as `codeGutter.ts`/`codeMinimap.ts`: `computeOpenSpans` expands each gutter entry's range by `COLLAPSE_CONTEXT` (2 lines) on both sides and merges overlapping/adjacent results; `computeGaps` takes the complement within the loaded file, classifying each gap `top`/`bottom` (touches a real file edge — one button, `↑ top`/`↓ bottom`, full reveal) or `middle` (between two open spans — the full `↑8`/`↓8`/`⤢all` set, mock 11's own labels); `planRows` turns that into the render loop's actual row plan, where a collapsed line is simply ABSENT from the plan — never rendered then hidden via CSS, the research doc's large-file gotcha. Two deliberate product calls made explicit in code comments: (1) a file with ZERO referenced ranges gets NO collapse at all (not one giant whole-file fold) — collapse only kicks in once there's something real to collapse AROUND, so an undiscussed file still just shows its code; (2) an entry outside the loaded file (same law-#3 case Phase 2b's minimap already handles) contributes no open span rather than a degenerate inverted one. Reveal state (`Map<gapKey, {hiddenStart,hiddenEnd}>`) is pure row-relative math — `revealFromTop`/`revealFromBottom`/`revealAll` cap at what's actually left, `hiddenStart > hiddenEnd` is a valid "fully revealed" empty range rather than a sentinel — and resets on every file switch (`loadFile`) so a gap key coinciding across two different files by line-number chance never leaks state. **Scroll anchoring** (the research doc's #1 gotcha): every expand goes through `withScrollAnchor`, which measures the topmost visible line's viewport offset before mutating state and restores that exact offset after — works uniformly whether the click was a partial reveal (fold row shrinks) or a full one (fold row disappears entirely), verified live (expanding the fixture's bottom fold keeps line 44 pinned at the same screen position). **Gutter line-number continuity**: since collapsed lines are never in the row plan (not hidden, simply absent), every visible line keeps its REAL number — no renumbering math needed, covered by a named test. **The minimap keeps working**: Phase 2b was already built off the full gutter-entry set rather than the viewport, so folded clusters were correct here before Phase 3 even existed — a regression e2e test confirms it still is. Real fixture, not just synthetic: `mock.ts`'s PlateBundle.swift snippet gained genuinely unreferenced trailing lines 50–65 (open spans from its real hits already close naturally at line 51) — a real bottom fold (`⋯ expand 14 lines · ↓ bottom`) exercised end-to-end in `code-collapse.spec.ts`, appending-only so every already-shipped Phase 1/2/2b e2e assertion (lines 44–49's exact content/behavior) stayed untouched — confirmed by the full existing suite staying green. `referenced`/`show all` toggle (mock 11/12's `.rtoggle`) lives in the code toolbar, hidden entirely when a file has no gaps (a toggle with no effect is clutter, not a feature). Both themes screenshot-verified live (default fold, expanded, toggle, in dark and light). 25 new unit tests (`codeCollapse.test.ts`) + 13 new component tests (`CodeLens.test.ts`, a bespoke 40-line fixture exercising all three edge types — top/middle/bottom — in one shot, plus partial-reveal sequencing, show-all, toggle visibility, and file-switch reset) + 6 new e2e tests. Ref: `redesign-mockups/11-code-view-BUILD-BRIEF.md`, `11-code-collapse-RESEARCH.md`, `11-code-anchored-conversations.html`'s `.fold` rows. Phase 4 (revision bar) is next, held per Stefan's release cadence. | ✅ done |
| 11.4 | **Code view — Phase 4: revision orientation** | Surfacing pass only, no backend change — the data was already served (`refcode.rs::snippet()` renders at the ref's pinned sha, `identity_paren()` gives the ref/date; the frontend already had `codeState.codeSha` + `RefTemporalFields` on every `RefHit`). The unified Code breadcrumb (App.svelte's `codeCrumb`/`codeCrumbMeta`, design/43/44) previously showed a bare `@sha` chip ONLY when pinned and omitted itself entirely at HEAD — the exact ambiguity the brief flags ("so you know for sure you're seeing exactly what the agent referenced"). `codeCrumb.sha` now always resolves to the real sha (the literal `'HEAD'` string included) and a new `isHead` flag drives an explicit `.rev-chip` that's ALWAYS rendered: `● HEAD` (green) when nothing's pinned, `◷ @<sha>` (amber) otherwise — real ref/date chips beside it when the API provides them (`codeCrumbMeta`, unchanged), honestly absent when it doesn't (never a fabricated "main · today"). **Reused the state palette, not a second color system** — `head`/`pinned` are literally `--state-flight`/`--state-unowned`, the SAME hex values mock 12's own dedicated `--head`/`--pinned` tokens already used, confirmed by inspection before wiring (the brief's own instruction). `⇄ compare to HEAD` renders as a genuine STUB this phase — present, disabled, titled "coming soon" — shown only when NOT at HEAD (comparing HEAD to itself is meaningless), omitted at HEAD same as the ref/date chips. New mock fixture: `EmptyView.swift` (mapped, genuinely zero conversations referencing it) — the only file in the fixture set that naturally drives the HEAD state, since every other mapped file has real pinned hits; every other file (PlateBundle.swift, plates.py) already exercised the pinned/amber path via Phase 1-3's existing fixtures, untouched. Fixed a real layout bug caught during live verify: the `⇄ compare to HEAD` button's text wrapped mid-word inside the already-crowded crumb row, inflating its height — `white-space: nowrap` on both the compare button and the rev chip. Confirmed live in both themes (default pinned state, HEAD state via file switch, compare stub). 2 new unit-style component tests in `App.test.ts` (pinned + HEAD, including the store's async initial-`'HEAD'`-then-settles race) + 4 new e2e tests (`code-revbar.spec.ts`: pinned, HEAD, live file-switch flip, both themes). Ref: `redesign-mockups/11-code-view-BUILD-BRIEF.md`, `11-code-anchored-conversations.html`/`12-code-gutter-and-time.html`'s `.rev`/`.rcompare`. Phase 5 (sidebar conversation timeline) held per Stefan's release cadence. | ✅ done |
| 11.5 | **Code view — Phase 5: the sidebar conversation timeline (LAST Code-view phase)** | The range-biography (mock 13) MVP, built into the anchored reader (`ReverseIndexPanel.svelte`) it already had — the scope's real hits laid oldest→newest as a spine, each node pinned to a real version. `viewedSha`/`onAlignToRevision` new props; a `timelineNodes` derivation sorts `hits` by real `ts` (keeping each hit's ORIGINAL index so a node click reuses the existing `focusHit` unchanged — reading a node just moves accordion focus, a real, harmless effect; only the explicit "↳ align" action moves the code). Node state is real: `hit.sha === viewedSha` reads green `● … · this` (reusing Phase 4's exact `--state-flight`/`--state-unowned` tokens, not a third palette); every other real sha reads amber `◷ … · <date>`, with the align action — omitted entirely on `this` nodes (aligning to where you already are is meaningless). Law #3: a node with no `commitDate` falls back to sha-only, never a fabricated date. **The align mechanism** (`codeState.pinnedSha`, new): CodeLens's file-load effect used to be one function that always both re-fetched refs AND picked a sha; splitting "is this a genuine file switch" from "is this the SAME file at a newly-pinned sha" needed to be race-proof against two effects both reacting to `active` — solved by folding both concerns into `loadFile` itself, deciding synchronously via a plain (non-`$state`) `lastLoadedKey` local rather than trusting `$effect` execution order across two separate effects (documented in code as a genuine pitfall avoided, not a hypothetical one). A file switch always resets `pinnedSha` to `null` — an alignment never silently carries over to a different file. **Composes with Phase 4 by construction, not by wiring**: aligning sets `codeState.codeSha` (the SAME field Phase 4's rev chip already reads straight from the store), so the chip updates live with zero new plumbing between the two phases — confirmed on REAL data, not a synthetic fixture: the default whole-file scope's real hit set already contains mock.ts's pre-44 legacy note (`sha: 'HEAD'`), which genuinely doesn't match the file's normally-pinned sha, so it renders as a real "older" node out of the box; aligning to it flips the rev chip from amber pinned to green HEAD live, screenshot-verified in both themes (and — after aligning — every hit that used to read "this" correctly re-classifies to "older" against the NEW viewed sha, each growing its own align link). Existing `ReverseIndexPanel.test.ts` specs needed scoping fixes (`within(...)`, not bare `screen.getByText`) where the new timeline's own `.snip`/`.who` text legitimately duplicates the accordion's — a real, expected consequence of composing two views over the same data, not a bug. 8 new `ReverseIndexPanel.test.ts` tests (ordering, this/older classification, align wiring, opt-in reading, header counts, law #3 date fallback, repo-mode/anchored-off omission) + 3 new `CodeLens.test.ts` tests (pinnedSha triggers a code-only refetch with getRefs NOT called again, file-switch reset, same-sha no-op) + 1 new `App.test.ts` integration test (the full align→rev-chip composition, on real data) + 4 new e2e tests (`code-timeline.spec.ts`). Ref: `redesign-mockups/11-code-view-BUILD-BRIEF.md`, `12-code-gutter-and-time.html`'s `.tl`/`.spine`/`.node`. **This completes the Code view epic (Phases 1–5) — ready for merge review.** | ✅ done |

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
- **The focus reader's "seen" line — RESOLVED, piece 4 item 2 (2026-07-19).** Real per-message
  `seenBy` (Herald, `b776c94`, design/48 #62) is now wired everywhere: the reader's own gutter shows
  it directly; ChatStream's `buildSeenEntries` reads it instead of synthesizing — CONTRACT GAP #58
  is retired. The "since you last looked" cutoff is now a real per-(hub,topic) localStorage
  watermark (`readState.svelte.ts`), not a hardcoded demo date, with an explicit "mark all read"
  catch-up. A completionist-safe "detail-viewed" marker (dwell/scroll in the focus reader → a
  subtle, absence-is-neutral ✓ on the stream row and the minimap node) rounds out the read-state
  layer — see `readState.svelte.ts`'s own header comment for the full design.

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
