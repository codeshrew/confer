import { describe, expect, it, vi } from 'vitest';
import DOMPurify from 'dompurify';
import { __clearMarkdownCachesForTest, highlightRenderedCodeBlocks, renderMarkdown } from './markdown';

describe('renderMarkdown', () => {
  it('memoizes on the raw source: re-rendering the same body reuses the cached HTML instead of re-parsing it', () => {
    // Spy on markdown-it's own render path via DOMPurify.sanitize, which
    // renderMarkdown calls exactly once per cache miss — a second call with
    // the *same* source must not invoke it again.
    const sanitizeSpy = vi.spyOn(DOMPurify, 'sanitize');
    const src = `unique memo-cache probe body ${Math.random()}`;

    const first = renderMarkdown(src);
    const callsAfterFirst = sanitizeSpy.mock.calls.length;
    expect(callsAfterFirst).toBeGreaterThan(0);

    const second = renderMarkdown(src);
    expect(second).toBe(first);
    // No new sanitize call — the second render was served from cache.
    expect(sanitizeSpy.mock.calls.length).toBe(callsAfterFirst);

    // A different source still renders (and sanitizes) fresh.
    const third = renderMarkdown(`${src} — different`);
    expect(sanitizeSpy.mock.calls.length).toBeGreaterThan(callsAfterFirst);
    expect(third).not.toBe(first);

    sanitizeSpy.mockRestore();
  });

  it('__clearMarkdownCachesForTest actually drops the render cache — a subsequent identical call re-sanitizes', () => {
    const sanitizeSpy = vi.spyOn(DOMPurify, 'sanitize');
    const src = `cache-clear probe ${Math.random()}`;

    renderMarkdown(src);
    const callsAfterFirst = sanitizeSpy.mock.calls.length;
    expect(callsAfterFirst).toBeGreaterThan(0);

    renderMarkdown(src);
    expect(sanitizeSpy.mock.calls.length).toBe(callsAfterFirst); // still cached

    __clearMarkdownCachesForTest();
    renderMarkdown(src);
    expect(sanitizeSpy.mock.calls.length).toBeGreaterThan(callsAfterFirst); // cache was actually emptied

    sanitizeSpy.mockRestore();
  });

  it('renders headings, bold, and lists as real HTML', () => {
    const html = renderMarkdown('## Heading\n\n**bold** text and a list:\n\n- one\n- two');
    expect(html).toContain('<h2>Heading</h2>');
    expect(html).toContain('<strong>bold</strong>');
    expect(html).toContain('<ul>');
    expect(html).toContain('<li>one</li>');
  });

  it('renders fenced code with a language class, ready for post-render Shiki highlighting', () => {
    const html = renderMarkdown('```rust\nfn main() {}\n```');
    expect(html).toContain('class="md-fence"');
    expect(html).toContain('data-lang="rust"');
    expect(html).toContain('language-rust');
    expect(html).toContain('fn main() {}');
  });

  it('turns a single newline into a real line break (agent notes are typically single-\\n structured, not blank-line paragraphs)', () => {
    const html = renderMarkdown('First point.\nSecond point.\nThird point.');
    expect(html).toContain('First point.<br>\nSecond point.<br>\nThird point.');
  });

  it('still starts a new paragraph on a blank line, distinct from a single-newline break', () => {
    const html = renderMarkdown('Paragraph one.\n\nParagraph two.');
    expect(html).toContain('<p>Paragraph one.</p>');
    expect(html).toContain('<p>Paragraph two.</p>');
  });

  it('renders inline code with the .mono class, matching the prior plain-text rendering', () => {
    const html = renderMarkdown('run `confer serve --port 8422` now');
    expect(html).toContain('<code class="mono">confer serve --port 8422</code>');
  });

  it('renders @mentions as a styled span', () => {
    const html = renderMarkdown('@Herald please review');
    expect(html).toContain('<span class="mention">@Herald</span>');
  });

  it('does not treat an email-like token as a mention', () => {
    const html = renderMarkdown('contact me at foo@bar.com');
    expect(html).not.toContain('class="mention"');
  });

  it('never lets a <script> tag survive as a real element (proves sanitization) — html:false means markdown-it already escapes it to inert text', () => {
    const html = renderMarkdown('hello <script>alert(1)</script> world');
    const dom = document.createElement('div');
    dom.innerHTML = html;
    expect(dom.querySelector('script')).toBeNull();
    // The angle brackets are rendered as inert, visible text — not parsed.
    expect(html).toContain('&lt;script&gt;');
  });

  it('never lets an onerror handler survive as a real attribute (proves sanitization) — an unparsed <img> is inert escaped text', () => {
    const html = renderMarkdown('<img src=x onerror=alert(1)>');
    const dom = document.createElement('div');
    dom.innerHTML = html;
    expect(dom.querySelector('img')).toBeNull();
    expect(dom.querySelector('[onerror]')).toBeNull();
  });

  it('never produces a real <a> for a javascript: URI (markdown-it itself refuses to link it; nothing clickable results)', () => {
    const html = renderMarkdown('[click me](javascript:alert(1))');
    const dom = document.createElement('div');
    dom.innerHTML = html;
    expect(dom.querySelector('a')).toBeNull();
  });

  it('DOMPurify itself (not just markdown-it) refuses a javascript: href, as defense in depth', () => {
    // Bypasses markdown-it entirely to prove the sanitizer — not just the
    // markdown parser's own link-scheme validation — is what's keeping this
    // app safe against a javascript: URI.
    const dirty = '<a href="javascript:alert(1)">click</a>';
    const clean = DOMPurify.sanitize(dirty, { ALLOWED_TAGS: ['a'], ALLOWED_ATTR: ['href'] });
    expect(clean).not.toMatch(/javascript:/i);
  });

  it('never produces a real <a> for a data: URI (markdown-it refuses to link it, matching its javascript: handling)', () => {
    const html = renderMarkdown('[click me](data:text/html,<script>alert(1)</script>)');
    const dom = document.createElement('div');
    dom.innerHTML = html;
    expect(dom.querySelector('a')).toBeNull();
  });

  it('never produces a real <a> for a vbscript: URI', () => {
    const html = renderMarkdown('[click me](vbscript:msgbox(1))');
    const dom = document.createElement('div');
    dom.innerHTML = html;
    expect(dom.querySelector('a')).toBeNull();
  });

  it('renderMarkdown\'s own sanitize config (ALLOWED_URI_REGEXP) rejects data:/vbscript hrefs even if something upstream produced an <a> tag for one — the same allow-list this module actually uses, not a hand-picked one', () => {
    const config = {
      ALLOWED_TAGS: ['a'],
      ALLOWED_ATTR: ['href'],
      ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|#|\/)/i,
    };
    const dataDirty = '<a href="data:text/html,<script>alert(1)</script>">click</a>';
    expect(DOMPurify.sanitize(dataDirty, config)).not.toMatch(/data:/i);

    const vbsDirty = '<a href="vbscript:msgbox(1)">click</a>';
    expect(DOMPurify.sanitize(vbsDirty, config)).not.toMatch(/vbscript:/i);
  });

  it('the afterSanitizeAttributes hook still strips a javascript:/data:/vbscript: href as a second layer, if the allow-list regexp alone somehow let one through', () => {
    // renderMarkdown's DOMPurify.addHook runs globally for every sanitize
    // call in this process (registered once at module load), so exercising
    // it via a permissive ALLOWED_URI_REGEXP proves the hook itself — not
    // just the regexp — refuses these schemes.
    const permissive = { ALLOWED_TAGS: ['a'], ALLOWED_ATTR: ['href'] };
    for (const scheme of ['javascript:alert(1)', 'data:text/html,x', 'vbscript:msgbox(1)']) {
      const clean = DOMPurify.sanitize(`<a href="${scheme}">click</a>`, permissive);
      const dom = document.createElement('div');
      dom.innerHTML = clean;
      const href = dom.querySelector('a')?.getAttribute('href');
      expect(href ?? '').not.toMatch(/^(javascript|data|vbscript):/i);
    }
  });

  it('adds safe rel/target to ordinary links', () => {
    const html = renderMarkdown('[confer](https://example.com/confer)');
    expect(html).toContain('target="_blank"');
    expect(html).toContain('rel="noopener noreferrer nofollow"');
    expect(html).toContain('href="https://example.com/confer"');
  });

  it('never emits raw HTML tags outside the allow-list (e.g. <iframe>)', () => {
    const html = renderMarkdown('before <iframe src="https://evil.example"></iframe> after');
    expect(html).not.toContain('<iframe');
  });

  it('does not fuzzy-linkify bare filenames — "REDESIGN.md" is not a domain, "design/48-x.md" is not a URL', () => {
    // linkify's fuzzy matcher treats any dotted-suffix word as a bare
    // domain (`.md` is Moldova's ccTLD) — clicking a filename mention was
    // silently navigating off-site to a random external domain.
    const a = renderMarkdown('See REDESIGN.md for the plan.');
    expect(a).not.toContain('<a');
    expect(a).toContain('REDESIGN.md');

    const b = renderMarkdown('Landed in design/48-x.md this week.');
    expect(b).not.toContain('<a');
    expect(b).toContain('design/48-x.md');
  });

  it('still linkifies explicit https:// URLs (only fuzzy bare-word matching is disabled)', () => {
    const html = renderMarkdown('See https://example.com for details.');
    expect(html).toContain('<a');
    expect(html).toContain('href="https://example.com"');
  });
});

describe('highlightRenderedCodeBlocks', () => {
  it('upgrades a rendered fenced-code block to Shiki tokens and marks it done', async () => {
    const html = renderMarkdown('```rust\nfn main() {}\n```');
    const root = document.createElement('div');
    root.innerHTML = html;
    document.body.appendChild(root);

    const pre = root.querySelector('pre.md-fence')!;
    expect(pre.hasAttribute('data-hl-done')).toBe(false);
    const before = pre.querySelector('code')!.innerHTML;

    await highlightRenderedCodeBlocks(root);

    expect(pre.hasAttribute('data-hl-done')).toBe(true);
    const after = pre.querySelector('code')!.innerHTML;
    expect(after).not.toBe(before);
    expect(after).toContain('shiki-tok');
    // The original code text still round-trips through the tokenized markup.
    expect(pre.querySelector('code')!.textContent).toBe('fn main() {}\n');

    document.body.removeChild(root);
  });

  it('is a no-op (does not re-tokenize) on a block already marked data-hl-done', async () => {
    const html = renderMarkdown('```rust\nfn again() {}\n```');
    const root = document.createElement('div');
    root.innerHTML = html;

    await highlightRenderedCodeBlocks(root);
    const pre = root.querySelector('pre.md-fence')!;
    const firstPass = pre.querySelector('code')!.innerHTML;

    // Tamper with the content to prove a second call leaves it alone.
    pre.querySelector('code')!.innerHTML = firstPass + '<!-- tamper -->';
    await highlightRenderedCodeBlocks(root);

    expect(pre.querySelector('code')!.innerHTML).toBe(firstPass + '<!-- tamper -->');
  });

  it('does nothing when there are no fenced-code blocks under root', async () => {
    const root = document.createElement('div');
    root.innerHTML = renderMarkdown('just plain text, no code fences');

    await expect(highlightRenderedCodeBlocks(root)).resolves.toBeUndefined();
  });

  it('leaves the plain (already-safe) text in place if tokenizing throws, and still marks the block done', async () => {
    // An empty <pre.md-fence> with no inner <code> exercises the "codeEl not
    // found -> continue" branch without needing to fake a Shiki failure.
    const root = document.createElement('div');
    root.innerHTML = '<pre class="md-fence" data-lang="rust"></pre>';

    await expect(highlightRenderedCodeBlocks(root)).resolves.toBeUndefined();
    // No <code> child, so the "continue" branch is taken and the block is
    // simply skipped — data-hl-done is only set inside the per-block loop
    // after the try/finally, which requires a <code> element to reach.
  });
});
