// Shiki wrapper — the production syntax highlighter (see the design
// mockup's own note in design/serve-dashboard-v2-mockup.html: the mockup
// ships a hand-rolled tokenizer stand-in; production bundles real Shiki).
//
// Fine-grained bundle: only the grammars/themes this dashboard needs are
// imported (tree-shakeable `shiki/langs/*.mjs` / `shiki/themes/*.mjs`
// modules), and the highlighter is built once with the pure-JS regex engine
// (no WASM fetch) so `vite build` can still inline everything into a single
// `dist/index.html` with no async asset loading at runtime.
//
// Dual light/dark theming: `codeToTokens` is called with both themes and
// `defaultColor: false`, which makes Shiki emit per-token CSS custom
// properties (`--shiki-light` / `--shiki-dark`) instead of baking in a
// single color. We render those as inline `style` on each token span, and
// app.css flips which variable wins via `:root[data-theme="dark"]` — so the
// theme toggle re-themes code with zero re-highlighting.
import { createHighlighterCore, type HighlighterCore } from 'shiki/core';
import { createJavaScriptRegexEngine } from 'shiki/engine/javascript';

import githubDark from 'shiki/themes/github-dark.mjs';
import githubLight from 'shiki/themes/github-light.mjs';
import swift from 'shiki/langs/swift.mjs';
import python from 'shiki/langs/python.mjs';
import rust from 'shiki/langs/rust.mjs';
import typescript from 'shiki/langs/typescript.mjs';
import javascript from 'shiki/langs/javascript.mjs';
import json from 'shiki/langs/json.mjs';
import bash from 'shiki/langs/bash.mjs';

export const SUPPORTED_LANGS = new Set([
  'swift',
  'python',
  'rust',
  'typescript',
  'javascript',
  'json',
  'bash',
]);

/** Map a lang hint (possibly unsupported, possibly a file extension) onto a bundled grammar, falling back to plain text. */
export function resolveLang(lang: string | null | undefined): string {
  if (!lang) return 'text';
  const normalized = lang.toLowerCase();
  const EXT_MAP: Record<string, string> = {
    swift: 'swift',
    py: 'python',
    python: 'python',
    rs: 'rust',
    rust: 'rust',
    ts: 'typescript',
    typescript: 'typescript',
    js: 'javascript',
    javascript: 'javascript',
    mjs: 'javascript',
    json: 'json',
    sh: 'bash',
    bash: 'bash',
    shell: 'bash',
  };
  const mapped = EXT_MAP[normalized];
  return mapped && SUPPORTED_LANGS.has(mapped) ? mapped : 'text';
}

let highlighterPromise: Promise<HighlighterCore> | null = null;

/** Async init once, cached — subsequent calls reuse the same highlighter instance. */
export function getHighlighter(): Promise<HighlighterCore> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighterCore({
      themes: [githubDark, githubLight],
      langs: [swift, python, rust, typescript, javascript, json, bash],
      engine: createJavaScriptRegexEngine(),
    });
  }
  return highlighterPromise;
}

export interface HighlightToken {
  text: string;
  /** Inline CSS text, e.g. "--shiki-light:#032f62;--shiki-dark:#9ecbff" */
  style: string;
}

export interface HighlightedLine {
  n: number;
  tokens: HighlightToken[];
}

function styleToCss(style: Record<string, string> | undefined): string {
  if (!style) return '';
  return Object.entries(style)
    .map(([k, v]) => `${k}:${v}`)
    .join(';');
}

// Re-mounting CodeLens (tabbing away and back, or re-selecting the same
// file) re-tokenizes the exact same file text through Shiki unless we
// remember the answer. Keyed on (lang, joined source) — the tokenized
// result doesn't depend on which line numbers were requested, so it's safe
// to cache independent of the `n`s passed in. Bounded with simple FIFO
// eviction (Map preserves insertion order), same approach as markdown.ts's
// caches.
const SNIPPET_CACHE_MAX = 200;
const snippetTokenCache = new Map<string, HighlightToken[][]>();

/** Test-only: drop cached tokenization output (avoids cross-test bleed). */
export function __clearHighlightCacheForTest(): void {
  snippetTokenCache.clear();
}

/**
 * Highlight a set of (possibly non-contiguous) numbered lines as one blob —
 * multi-line constructs (docstrings, block comments) tokenize correctly
 * because the whole snippet is fed to Shiki in one call, then split back
 * out by line index to re-pair with the original line numbers.
 */
export async function highlightSnippetLines(
  lines: { n: number; text: string }[],
  lang: string | null | undefined
): Promise<HighlightedLine[]> {
  const effectiveLang = resolveLang(lang);
  const code = lines.map((l) => l.text).join('\n');
  const cacheKey = `${effectiveLang} ${code}`;

  let tokensByLine = snippetTokenCache.get(cacheKey);
  if (!tokensByLine) {
    const highlighter = await getHighlighter();
    const { tokens } = highlighter.codeToTokens(code, {
      lang: effectiveLang,
      themes: { light: 'github-light', dark: 'github-dark' },
      defaultColor: false,
    });
    tokensByLine = tokens.map((line) => line.map((t) => ({ text: t.content, style: styleToCss(t.htmlStyle) })));
    if (snippetTokenCache.size >= SNIPPET_CACHE_MAX) {
      const oldest = snippetTokenCache.keys().next().value;
      if (oldest !== undefined) snippetTokenCache.delete(oldest);
    }
    snippetTokenCache.set(cacheKey, tokensByLine);
  }

  return lines.map((line, i) => ({
    n: line.n,
    tokens: tokensByLine![i] ?? [],
  }));
}
