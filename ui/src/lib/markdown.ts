// Markdown rendering for message bodies — UNTRUSTED, peer-authored content
// (confer's security model treats every message body as hostile input; any
// agent on the hub can author one). The pipeline is therefore strictly:
//
//   markdown-it (html: false, so raw HTML in the source is escaped)
//     -> DOMPurify.sanitize (allow-list of tags/attrs; strip/neuter unsafe
//        links) -> the only string this module hands back.
//
// Fenced code blocks are NOT syntax-highlighted here — that needs the async
// Shiki highlighter (see highlight.ts) and this function must stay a plain,
// synchronous string transform. Callers upgrade code blocks in place after
// mount via `highlightRenderedCodeBlocks` (see below); until that resolves,
// the block just shows as plain (but still safe, still escaped) text — the
// "fall back to plain <pre> if Shiki isn't ready" case from the task brief.
import MarkdownIt from 'markdown-it';
import DOMPurify from 'dompurify';
import { getHighlighter, resolveLang } from './highlight';

// --- @mention inline rule ------------------------------------------------
// Ports the highlighting Message.svelte used to do with a hand-rolled regex
// (segmentBody) into the markdown-it pipeline itself, so mentions still get
// their pill styling when they appear alongside real Markdown constructs
// (inside a list item, after a heading, etc).
function mentionPlugin(md: MarkdownIt): void {
  const MENTION_RE = /^@[A-Za-z][\w-]*/;

  md.inline.ruler.before('emphasis', 'mention', (state, silent) => {
    const start = state.pos;
    if (state.src.charCodeAt(start) !== 0x40 /* '@' */) return false;
    if (start > 0 && /[A-Za-z0-9_]/.test(state.src[start - 1] ?? '')) return false; // e.g. "foo@bar" isn't a mention

    const match = MENTION_RE.exec(state.src.slice(start));
    if (!match) return false;

    if (!silent) {
      const token = state.push('mention', '', 0);
      token.content = match[0];
    }
    state.pos += match[0].length;
    return true;
  });

  md.renderer.rules.mention = (tokens, idx) =>
    `<span class="mention">${md.utils.escapeHtml(tokens[idx]!.content)}</span>`;
}

const md: MarkdownIt = new MarkdownIt({
  html: false, // raw HTML in message source is escaped, never parsed as markup
  linkify: true,
  breaks: false,
});
md.use(mentionPlugin);

// Match the pre-existing look-and-feel: inline code kept its `.mono` class
// (see Message.test.ts), fenced code gets a `language-x` class the async
// Shiki pass below can key off.
md.renderer.rules.code_inline = (tokens, idx) => {
  const token = tokens[idx]!;
  return `<code class="mono">${md.utils.escapeHtml(token.content)}</code>`;
};

const DEFAULT_FENCE = md.renderer.rules.fence!.bind(md.renderer.rules);
md.renderer.rules.fence = (tokens, idx, options, env, self) => {
  const html = DEFAULT_FENCE(tokens, idx, options, env, self);
  // The default fence renderer already emits `<pre><code class="language-x">`
  // (escaped). Tag the wrapping <pre> itself so post-render highlighting can
  // find it without re-parsing class names off <code>.
  const token = tokens[idx]!;
  const lang = (token.info || '').trim().split(/\s+/)[0] || '';
  return html.replace('<pre>', `<pre class="md-fence" data-lang="${md.utils.escapeHtml(lang)}">`);
};

// --- sanitize --------------------------------------------------------------
// Allow-list only what the task calls for: paragraphs/headings/lists/inline
// emphasis/code/blockquote/links/hr/basic tables, plus the `span.mention`
// this module itself produces. No images, no raw HTML passthrough.
const ALLOWED_TAGS = [
  'p', 'br',
  'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
  'ul', 'ol', 'li',
  'strong', 'em', 'del', 's',
  'code', 'pre',
  'blockquote',
  'a', 'hr',
  'span',
  'table', 'thead', 'tbody', 'tr', 'th', 'td',
];
const ALLOWED_ATTR = ['href', 'target', 'rel', 'class', 'data-lang'];

DOMPurify.addHook('afterSanitizeAttributes', (node) => {
  if (node.tagName === 'A') {
    const href = node.getAttribute('href') ?? '';
    // Belt-and-suspenders: DOMPurify already strips javascript:/data: URIs
    // from href by default, but a message body is hostile input, so refuse
    // to trust that alone — drop the link entirely if anything unsafe slips
    // through.
    if (/^\s*(javascript|data|vbscript):/i.test(href)) {
      node.removeAttribute('href');
    }
    node.setAttribute('target', '_blank');
    node.setAttribute('rel', 'noopener noreferrer nofollow');
  }
});

// --- caches ------------------------------------------------------------
// Re-mounting Chat (tabbing Board/Fleet/Code -> Chat, or revisiting a hub)
// re-passes the exact same message bodies through markdown-it +
// DOMPurify.sanitize, and re-mounted fenced code blocks re-tokenize through
// Shiki, unless we remember the answer. Both caches are keyed on the exact
// input and bounded with simple FIFO eviction (Map preserves insertion
// order in JS, so deleting the first key evicts the oldest entry) — an
// approximate LRU that's plenty to avoid redoing ~166 messages' worth of
// work on every switch, without extra bookkeeping.
const RENDER_CACHE_MAX = 1000;
const renderCache = new Map<string, string>();
const HIGHLIGHT_CACHE_MAX = 500;
const highlightCache = new Map<string, string>();

function cachedCompute(cache: Map<string, string>, max: number, key: string, compute: () => string): string {
  const hit = cache.get(key);
  if (hit !== undefined) return hit;
  const value = compute();
  if (cache.size >= max) {
    const oldest = cache.keys().next().value;
    if (oldest !== undefined) cache.delete(oldest);
  }
  cache.set(key, value);
  return value;
}

/** Test-only: drop all cached render/highlight output (avoids cross-test bleed). */
export function __clearMarkdownCachesForTest(): void {
  renderCache.clear();
  highlightCache.clear();
}

/**
 * Render a Markdown message body to sanitized, safe-to-`{@html}` HTML.
 * Message bodies are peer-authored and therefore untrusted; this is the
 * ONLY function in the app that may produce HTML from a message body, and
 * every code path that inserts a body into the DOM must go through it.
 *
 * Memoized on the raw source: re-rendering the same body (a re-mount, a
 * revisited hub) reuses the sanitized HTML instead of re-parsing +
 * re-sanitizing it.
 */
export function renderMarkdown(src: string): string {
  return cachedCompute(renderCache, RENDER_CACHE_MAX, src, () => {
    const rendered = md.render(src);
    return DOMPurify.sanitize(rendered, {
      ALLOWED_TAGS,
      ALLOWED_ATTR,
      ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|#|\/)/i,
    });
  });
}

// --- post-render code highlighting -----------------------------------------
// Shiki is async; markdown-it/DOMPurify are not. Fenced code blocks render
// as plain (escaped, safe) text first, then this upgrades any `pre.md-fence`
// found under `root` to Shiki's dual-theme token spans — same visual
// treatment CodeRefCard uses for pinned `--ref` snippets. Safe to call
// repeatedly/redundantly (e.g. from a Svelte $effect on every render): a
// block that's already highlighted is skipped.
export async function highlightRenderedCodeBlocks(root: ParentNode): Promise<void> {
  const blocks = Array.from(root.querySelectorAll<HTMLPreElement>('pre.md-fence:not([data-hl-done])'));
  if (blocks.length === 0) return;

  const highlighter = await getHighlighter();

  for (const pre of blocks) {
    const codeEl = pre.querySelector('code');
    if (!codeEl) continue;
    const lang = resolveLang(pre.dataset.lang);
    // textContent decodes the entities markdown-it/DOMPurify escaped the
    // code as, giving back the original source text — never re-parsed as
    // HTML, just fed to Shiki's tokenizer.
    const code = codeEl.textContent ?? '';
    const cacheKey = `${lang} ${code}`;
    try {
      const html = cachedCompute(highlightCache, HIGHLIGHT_CACHE_MAX, cacheKey, () => {
        const { tokens } = highlighter.codeToTokens(code, {
          lang,
          themes: { light: 'github-light', dark: 'github-dark' },
          defaultColor: false,
        });
        return tokens
          .map((line) =>
            line
              .map((t) => {
                const style = t.htmlStyle
                  ? Object.entries(t.htmlStyle)
                      .map(([k, v]) => `${k}:${v}`)
                      .join(';')
                  : '';
                return `<span class="shiki-tok" style="${style}">${md.utils.escapeHtml(t.content)}</span>`;
              })
              .join('')
          )
          .join('\n');
      });
      codeEl.innerHTML = html;
    } catch {
      // Leave the plain, already-safe text in place.
    } finally {
      pre.setAttribute('data-hl-done', '');
    }
  }
}
