// Keyboard-first cross-cutting infrastructure (ui/REDESIGN.md — the
// three-layer model, adopted 2026-07-19 from Compositor's studio proofread-UI
// spec, relayed by Herald; supersedes the earlier `g`-leader). Pure,
// DOM-adjacent predicates + the pane-focus geometry scorer — kept out of any
// component so the "never fire shortcuts while typing" rule and the
// nearest-neighbour math are unit-testable without mounting Svelte or a real
// layout. The STATEFUL pane registry that uses this geometry lives in
// paneFocus.svelte.ts; this file stays pure functions only.
import type { View } from './stores.svelte';

/** True when `el` is somewhere the browser's own typing behavior owns the
 * keystroke — an input/textarea/select, or any contenteditable region. Every
 * global shortcut below is gated on this being false FIRST (REDESIGN.md:
 * "never fire shortcuts while typing in a field"). `⌘K` is the one
 * deliberate exception (callers check it before this gate) since opening the
 * palette from inside another text field is the whole point of a command
 * palette. */
export function isTypingTarget(el: EventTarget | null): boolean {
  if (!(el instanceof HTMLElement)) return false;
  const tag = el.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  // `.isContentEditable` (the computed getter) is unimplemented in jsdom, so
  // it comes back `undefined` there instead of `false` — fall back to the
  // plain IDL attribute, which IS implemented, rather than trust the getter
  // alone.
  return el.isContentEditable === true || el.contentEditable === 'true';
}

/** True for the `⌘K`/`Ctrl+K` chord — Meta on macOS, Ctrl elsewhere/most
 * browsers-on-Linux, matching the operator's stated macOS-native intent
 * without hardcoding a single platform. Note: on Chrome, `Ctrl+K` is ALSO the
 * reserved omnibox-search chord and its keydown may never reach page JS at
 * all — the Meta/Cmd path is the one the operator (macOS) actually uses;
 * Ctrl is kept as a best-effort fallback, not a guarantee. */
export function isCommandK(e: KeyboardEvent): boolean {
  return (e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'k';
}

// design/47's view order, now the Layer-3 `Cmd`+number mapping (REDESIGN.md,
// keyboard architecture pass 2026-07-19 — the `g`-leader that used to carry
// this is RETIRED: a global `g`-prefix collided with per-pane bare `gg`).
// `repos` is deliberately NOT numbered — the settled keyboard model only
// enumerates Overview/Chat/Board/Fleet/Code; Repos stays mouse/TopBar-
// reachable rather than inventing a 6th binding nobody asked for. No letter
// aliases at this layer (the old `g o`/`g b`/`g f` aliases retired with the
// leader) — Cmd+number is the one canonical view-switch chord now.
const VIEW_BY_NUMBER: Record<string, View> = {
  '1': 'overview',
  '2': 'chat',
  '3': 'board',
  '4': 'fleet',
  '5': 'code',
};

/** Resolves a `Cmd`+number keypress to a view, or `null` if the number isn't
 * bound (6+, or a non-digit). Callers gate this on `e.metaKey` themselves —
 * deliberately Cmd-only, not Cmd-or-Ctrl: `Ctrl`+1-9 is the reserved
 * "switch to browser tab N" chord in Chrome/Firefox and is not interceptable
 * by page JS at all, so aliasing it here would silently do nothing on the
 * operator's actual browser. */
export function viewForCmdNumber(key: string): View | null {
  return VIEW_BY_NUMBER[key] ?? null;
}

/** How long a two-key chord (a pane's own `g g` "jump to top", etc.) stays
 * armed waiting for its second keypress — generous enough for a deliberate
 * chord, short enough that an unrelated `g` typed a moment later elsewhere
 * can't accidentally combine with it. (Historically this also timed the
 * app-wide `g`-leader; that layer is retired, but the SAME constant is still
 * the right one for any pane's own double-press chords — see HubRail's
 * `g g`/`G`.) */
export const LEADER_TIMEOUT_MS = 900;

// ── Layer 1 — Ctrl+h/j/k/l pane-focus geometry ──────────────────────────────
// REDESIGN.md: "move pane focus by BOUNDING-BOX GEOMETRY (nearest region on
// the pressed side, scored by primary-axis distance + 2x cross-axis
// misalignment) so it survives any responsive layout with no keymap edits."
// Gotcha #4: neighbour from geometry, NEVER a hardcoded pane order — this is
// the one function that decides "which pane is up/down/left/right", and it
// only ever looks at real `DOMRect`s, never a fixed list position.

export type Direction = 'h' | 'j' | 'k' | 'l';

/** The subset of `DOMRect` the scorer needs — kept as a plain interface (not
 * `DOMRect` itself) so the geometry math is testable with plain numbers, no
 * jsdom layout engine required. */
export interface PaneRect {
  top: number;
  left: number;
  width: number;
  height: number;
}

export interface GeometryPane {
  id: string;
  rect: PaneRect;
}

/**
 * The nearest pane to `fromId` in `direction`, by bounding-box center,
 * scored `primaryAxisDistance + 2 * crossAxisMisalignment` (Compositor's
 * spec) — a candidate whose center isn't actually past `fromId`'s center on
 * the pressed side is excluded outright (pressing `l` never jumps to
 * something to your left). Returns `null` when nothing qualifies (already at
 * the edge) or `fromId` isn't a known pane.
 */
export function nearestPaneId(panes: GeometryPane[], fromId: string, direction: Direction): string | null {
  const from = panes.find((p) => p.id === fromId);
  if (!from) return null;
  const fromCx = from.rect.left + from.rect.width / 2;
  const fromCy = from.rect.top + from.rect.height / 2;

  let best: { id: string; score: number } | null = null;
  for (const p of panes) {
    if (p.id === fromId) continue;
    const cx = p.rect.left + p.rect.width / 2;
    const cy = p.rect.top + p.rect.height / 2;
    let primary: number;
    let cross: number;
    switch (direction) {
      case 'h': // left
        primary = fromCx - cx;
        cross = Math.abs(cy - fromCy);
        break;
      case 'l': // right
        primary = cx - fromCx;
        cross = Math.abs(cy - fromCy);
        break;
      case 'k': // up
        primary = fromCy - cy;
        cross = Math.abs(cx - fromCx);
        break;
      case 'j': // down
        primary = cy - fromCy;
        cross = Math.abs(cx - fromCx);
        break;
    }
    if (primary <= 0) continue; // not actually on the pressed side
    const score = primary + cross * 2;
    if (!best || score < best.score) best = { id: p.id, score };
  }
  return best?.id ?? null;
}
