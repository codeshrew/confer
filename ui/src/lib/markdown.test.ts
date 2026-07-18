import { describe, expect, it } from 'vitest';
import DOMPurify from 'dompurify';
import { renderMarkdown } from './markdown';

describe('renderMarkdown', () => {
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
});
