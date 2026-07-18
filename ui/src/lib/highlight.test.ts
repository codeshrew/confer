import { describe, expect, it } from 'vitest';
import {
  __clearHighlightCacheForTest,
  getHighlighter,
  highlightSnippetLines,
  resolveLang,
  SUPPORTED_LANGS,
} from './highlight';

describe('resolveLang', () => {
  it('maps a bare language name to itself when supported', () => {
    expect(resolveLang('rust')).toBe('rust');
    expect(resolveLang('python')).toBe('python');
  });

  it('maps common file extensions onto their grammar', () => {
    expect(resolveLang('py')).toBe('python');
    expect(resolveLang('rs')).toBe('rust');
    expect(resolveLang('ts')).toBe('typescript');
    expect(resolveLang('js')).toBe('javascript');
    expect(resolveLang('mjs')).toBe('javascript');
    expect(resolveLang('sh')).toBe('bash');
    expect(resolveLang('shell')).toBe('bash');
  });

  it('is case-insensitive', () => {
    expect(resolveLang('Rust')).toBe('rust');
    expect(resolveLang('PY')).toBe('python');
  });

  it('falls back to "text" for an unsupported/unknown language hint', () => {
    expect(resolveLang('cobol')).toBe('text');
    expect(resolveLang('haskell')).toBe('text');
  });

  it('falls back to "text" for null/undefined/empty', () => {
    expect(resolveLang(null)).toBe('text');
    expect(resolveLang(undefined)).toBe('text');
    expect(resolveLang('')).toBe('text');
  });

  it('every mapped-to language is actually present in SUPPORTED_LANGS (mapping table stays consistent with the bundle)', () => {
    const candidates = ['swift', 'py', 'python', 'rs', 'rust', 'ts', 'typescript', 'js', 'javascript', 'mjs', 'json', 'sh', 'bash', 'shell'];
    for (const c of candidates) {
      expect(SUPPORTED_LANGS.has(resolveLang(c))).toBe(true);
    }
  });
});

describe('getHighlighter', () => {
  it('returns the same cached highlighter instance on repeated calls (async init happens once)', async () => {
    const first = getHighlighter();
    const second = getHighlighter();
    // Same in-flight/settled promise object — init isn't kicked off twice.
    expect(second).toBe(first);
    await first;
  });
});

describe('highlightSnippetLines', () => {
  it('tokenizes multi-line code and preserves the original (possibly non-contiguous) line numbers', async () => {
    __clearHighlightCacheForTest();
    const lines = [
      { n: 44, text: 'fn main() {' },
      { n: 45, text: '  println!("hi");' },
      { n: 46, text: '}' },
    ];
    const result = await highlightSnippetLines(lines, 'rust');

    expect(result.map((r) => r.n)).toEqual([44, 45, 46]);
    // Every line got at least one token, and the joined text reconstructs the source line.
    for (const [i, line] of result.entries()) {
      expect(line.tokens.length).toBeGreaterThan(0);
      expect(line.tokens.map((t) => t.text).join('')).toBe(lines[i]!.text);
    }
  });

  it('memoizes on (lang, joined source): re-highlighting the identical snippet skips re-tokenization', async () => {
    __clearHighlightCacheForTest();
    const lines = [{ n: 1, text: 'const x = 1;' }];

    const highlighter = await getHighlighter();
    let calls = 0;
    const original = highlighter.codeToTokens.bind(highlighter);
    highlighter.codeToTokens = ((...args: Parameters<typeof original>) => {
      calls++;
      return original(...args);
    }) as typeof original;

    await highlightSnippetLines(lines, 'typescript');
    expect(calls).toBe(1);

    await highlightSnippetLines(lines, 'typescript');
    expect(calls).toBe(1); // served from cache, no second tokenize call

    // A different lang for the same text is a different cache key -> re-tokenizes.
    await highlightSnippetLines(lines, 'javascript');
    expect(calls).toBe(2);

    highlighter.codeToTokens = original;
  });

  it('an unresolved/unsupported lang hint falls back to plain "text" tokenization rather than throwing', async () => {
    __clearHighlightCacheForTest();
    const lines = [{ n: 1, text: 'some unrecognized-language content' }];
    await expect(highlightSnippetLines(lines, 'some-made-up-language')).resolves.toBeDefined();
  });

  it('__clearHighlightCacheForTest actually empties the cache — a cleared call re-tokenizes', async () => {
    __clearHighlightCacheForTest();
    const lines = [{ n: 1, text: 'let y = 2;' }];

    const highlighter = await getHighlighter();
    let calls = 0;
    const original = highlighter.codeToTokens.bind(highlighter);
    highlighter.codeToTokens = ((...args: Parameters<typeof original>) => {
      calls++;
      return original(...args);
    }) as typeof original;

    await highlightSnippetLines(lines, 'javascript');
    expect(calls).toBe(1);

    __clearHighlightCacheForTest();
    await highlightSnippetLines(lines, 'javascript');
    expect(calls).toBe(2);

    highlighter.codeToTokens = original;
  });
});
