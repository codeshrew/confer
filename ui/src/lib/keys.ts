// Keyboard-first cross-cutting infrastructure (ui/REDESIGN.md, "keyboard-first
// as a cross-cutting design principle", piece 2's first slice). Pure,
// DOM-adjacent predicates + the g-leader chord state machine — kept out of
// any component so the "never fire shortcuts while typing" rule and the
// leader-chord timing are unit-testable without mounting Svelte.
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
 * without hardcoding a single platform. */
export function isCommandK(e: KeyboardEvent): boolean {
  return (e.metaKey || e.ctrlKey) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'k';
}

// design/47's view order, reused here as the g+number mapping (REDESIGN.md:
// "g then a number 1–5 ... mirrors tmux window-number muscle memory").
// `repos` is deliberately NOT numbered — REDESIGN.md's settled keyboard
// model only enumerates Overview/Chat/Board/Fleet/Code; Repos stays
// mouse/TopBar-reachable rather than inventing a 6th binding the operator
// never asked for.
const VIEW_BY_NUMBER: Record<string, View> = {
  '1': 'overview',
  '2': 'chat',
  '3': 'board',
  '4': 'fleet',
  '5': 'code',
};

// First-letter aliases "where unambiguous" (REDESIGN.md) — Chat and Code
// both start with C, so neither gets a letter alias; only the three
// single-owner letters do.
const VIEW_BY_LETTER: Record<string, View> = {
  o: 'overview',
  b: 'board',
  f: 'fleet',
};

/** Resolves a g-leader's second keypress to a view, or `null` if the key
 * isn't bound (leader should stay armed only for a beat — the caller decides
 * whether an unbound key disarms it or falls through). */
export function viewForLeaderKey(key: string): View | null {
  return VIEW_BY_NUMBER[key] ?? VIEW_BY_LETTER[key.toLowerCase()] ?? null;
}

/** How long the `g` leader stays armed waiting for its second key (ms) —
 * generous enough for a deliberate two-key chord, short enough that a lone
 * "g" typed elsewhere a moment later can't accidentally combine with it. */
export const LEADER_TIMEOUT_MS = 900;
