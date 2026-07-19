# Code view — build brief (piece 11)

**Mocks:** `11-code-anchored-conversations.html` (the screen), `12-code-gutter-and-time.html` (gutter +
timeline study), `13-code-range-biography.html` (DEFERRED future view — do NOT build). **Research:**
`11-code-collapse-RESEARCH.md` (roll-our-own collapse). Build the file view **end to end, in phases** —
verify each phase live before the next, same cadence as the nav overlay-stack work.

## The guiding principle — composability all the way up
Small components compose into larger ones compose into **views**; the **data stays the same** — a view is
just the same conversations-pinned-to-code, **reordered / restacked / perceived over time**. Reuse the
existing composable pieces (CodeRefCard/CodeRefMini, TicketMiniCard, the message cards) — don't invent
parallel ones. Three views over one substrate:
- **File view (code-first)** — browse a file, conversations pinned to ranges. **← BUILD NOW.**
- **Range biography (subject-first)** — one range's whole cross-thread discussion + code evolution (mock 13). **← DEFERRED**; its MVP is Phase 5's sidebar timeline ("expand this range →" is the future entry point).
- **Thread view (topic-first)** — already exists.

## What already exists (reuse, don't rebuild)
- `CodeLens.svelte` — renders the file line-by-line (Shiki tokens) + per-line density gutter; `hitsByLine`,
  `fileRefs`, `pickSha` (already renders at the ref's **pinned sha**, not HEAD).
- `CodeTree.svelte` (navigator), `ReverseIndexPanel.svelte` (the file/repo ref list — becomes the anchored reader), `CodeRefCard`/`CodeRefMini`.
- **Backend already serves point-in-time correctly:** `refcode.rs::snippet()` reads `git cat-file -p <sha>:<path>` (exact blob, blobless clone, no worktree); `identity_paren()` gives `(main · 2026-07-12)` + staleness/`[dirty]` badges. Phase 4 is **surfacing** this, not new backend.
- Law #3 throughout: dangling refs → honest omission, never a fake chip/timeline; drift markers must reflect real sha mismatch.

---

## Phase 1 — Anchored reader (the #1 fix: stop jumping to Chat)
Today `App.svelte::openHitInChat()` (line ~607) does `appState.view = 'chat'; selectMessage(...)` — it
abandons the code. **Kill that.** Clicking a code conversation opens it in a **persistent in-Code reader
pane** (evolve `ReverseIndexPanel` into it): code stays put, the clicked range highlights.
- **Scope header** locked to the selection: `▤ whole file` vs `▐ 267–285`, with `↩ whole file` to widen (the `backToWholeFile` handler already exists — wire it to the header).
- Conversations render as **scannable rows** (reuse the card row form); focus one → it expands, the rest shrink to a `‹ N more` strip. `j/k` steps between them.
- **"Open full thread ›" stays** as an opt-in per card (the ONLY path back to Chat) — never the default click.
- **Accept:** click a hot range → its conversations read in the pane, code visible, range highlighted; `esc`/widen returns to whole-file scope; "open full thread" is the only thing that leaves Code. Both themes, tests + e2e.

## Phase 2 — The powered gutter (conversation map)
Replace the bare per-line density count with the three-channel map (mock 12):
- **Shape = scope:** whole-file convos in a **top file-lane** (above line 1); range convos = a **bracket** spanning their lines; single-line = a **tick**.
- **Color = meaning:** the settled palette — open=cyan · note=teal · needs-owner=amber · blocked=red · resolved=grey. One hue, one meaning.
- **Column = overlap:** two threads on overlapping lines get **side-by-side bracket columns**; intensity/thickness = count.
- The **range tab** (`2 · JV HE`) is the click target → Phase 1 reader. Hover = peek (tooltip: who/count/latest). `j/k` flows anchor-to-anchor.
- **Drift marker** (dashed bracket edge) when a conversation's pinned sha ≠ the viewed sha and the lines moved — law #3, only when real.
- **Accept:** brackets/ticks/file-lane render by real scope + type + palette; overlap shows distinct columns; clicking a tab opens the reader; drift only on real mismatch. Both themes, tests.

## Phase 2b — Conversation minimap
Far-edge strip: the **whole file** compressed, every conversation cluster shown (colored by state) **including folded regions**, with a viewport indicator; click-to-scroll. Pairs with collapse so folded discussion is never invisible.

## Phase 3 — PR-style collapse (roll-our-own, per research)
Collapse unreferenced spans to `⋯ expand N (↑8 ↓8 ⤢all)`; referenced ranges + context stay open. A
**referenced / show-all** toggle in the rev bar. Algorithm + gotchas (scroll anchoring, gutter line-number
continuity, large files) in `11-code-collapse-RESEARCH.md`. **Accept:** default lands on "referenced"
(folded); expanders reveal gaps without losing scroll position; show-all renders the whole file; gutter line
numbers stay correct across folds. Tests for the span math + expand/show-all.

## Phase 4 — Revision orientation (surface what the backend already knows)
Rev bar always shows **sha · ref · date** and whether you're on **HEAD (green)** or a **pinned past commit
(amber)** — so you know for sure you're seeing exactly what the agent referenced. Data is already served
(`snippet` renders at pinned sha; `identity_paren` for the ref/date). **`⇄ compare to HEAD`** = a **stub**
this phase (button present, wired to a "coming soon"/disabled state) — real diff view is future. **Accept:**
pinned vs HEAD reads unambiguously; ref+date shown when present; degrades honestly when unknown.

## Phase 5 — Sidebar conversation timeline (the range biography MVP)
The scope's conversations laid **oldest→newest** as a timeline spine, each node pinned to a version (date +
sha). Node on the viewed version = **green (● this)**; older = **amber (◷ older)** with **"↳ align code to
this version"** → re-pins the code view to that exact sha (opt-in, never automatic). **Accept:** timeline
orders by real ts across versions; align-to-revision snaps the code (Phase 4 rev bar updates to match); law
#3 — no fabricated nodes. This is the entry point the deferred range-biography (mock 13) later expands from.

---

## Sequence & DoD
Build **Phase 1 first**, verify live (I screenshot-verify each against the mocks), then 2 → 2b → 3 → 4 → 5.
Each phase: both themes, semantic palette, law #3, unit + e2e, build green, commit/push per phase. Reuse the
composable card components throughout. Deferred out of scope: **mock 13 (range biography)** and the real
**compare-to-HEAD** diff (Phase 4 stubs it). The nav **Phase B** (URL sync/deep-link/popstate) stays
separately queued — Code view takes priority per Stefan's focus.
