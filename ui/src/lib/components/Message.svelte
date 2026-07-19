<script lang="ts">
  import type { Agent, CodeRef, Message as MessageT, RefHit, RequestRow } from '../types';
  import { formatClock, formatIso8601 } from '../format';
  import { renderMarkdown, highlightRenderedCodeBlocks } from '../markdown';
  import { readState } from '../readState.svelte';
  import { api } from '../api';
  import SeenIndicator, { type SeenEntry } from './SeenIndicator.svelte';
  import TicketMiniCard from './TicketMiniCard.svelte';
  import CodeRefCard from './CodeRefCard.svelte';
  import CopyIdButton from './CopyIdButton.svelte';
  import Icon from './Icon.svelte';
  import Kbd from './Kbd.svelte';

  interface Props {
    message: MessageT;
    fromAgent?: Agent;
    /** Only needed for the ticket mini-card's assignee avatar (piece 5) —
     * every other agent-color lookup in this component goes through
     * `fromAgent` already. */
    agents?: Agent[];
    request?: RequestRow | null;
    hub?: string;
    selected?: boolean;
    unseen?: boolean;
    seenEntries: SeenEntry[];
    /** `'summary'` shows just `message.summary`, collapsed, until the reader
     * expands it; `'full'` is the pre-existing behavior (full body, clamped
     * past the long-body threshold). Governs note/message BODY visibility
     * only — ticket cards and syslines are unaffected. */
    density?: 'summary' | 'full';
    /** True for ~2s right after a scroll-to-message navigation (meta-thread
     * node click, lifecycle-trail row click) lands here — plays a brief
     * highlight pulse so the reader can find the message that was jumped
     * to. See ChatStream's `scrollToMessageId` handling. */
    highlight?: boolean;
    onSelect?: (id: string) => void;
    onSelectTicket?: (id: string) => void;
    onOpenRefs?: (ref: CodeRef, hits: RefHit[]) => void;
    /** keyboard-architecture pass — the mouse path for the `f` shortcut
     * ("focus reader" — reads the currently-focused message). Before this,
     * opening the focus reader was keyboard-only; this is that gap's fix. */
    onOpenFocus?: (id: string) => void;
    /** Piece 6 — opens the enriched note popover (body + related tickets/
     * code/thread). Plain notes only: a ticket already has its own richer
     * destination (the Full popover, via its Mini card) — offering both
     * here would just be two "expand" buttons pointed at different places
     * for the same row. */
    onOpenNote?: (id: string) => void;
    /** Piece 8b — opens the agent dossier from the seen-by roster. */
    onOpenAgent?: (id: string) => void;
  }

  let {
    message,
    fromAgent,
    agents = [],
    request = null,
    hub = '',
    selected = false,
    unseen = false,
    seenEntries,
    density = 'full',
    highlight = false,
    onSelect,
    onSelectTicket,
    onOpenRefs,
    onOpenFocus,
    onOpenNote,
    onOpenAgent,
  }: Props = $props();

  const SYSLINE_TYPES = new Set(['claim', 'done', 'error', 'defer', 'supersede']);
  const isSysline = $derived(SYSLINE_TYPES.has(message.type));
  const isTicket = $derived(message.type === 'request');

  const fromColor = $derived(fromAgent?.color ?? 'var(--muted)');
  const fromDisplay = $derived(fromAgent?.display ?? message.from);
  const fromAbbr = $derived(fromAgent?.abbr ?? message.from.slice(0, 2).toUpperCase());

  // Long design-review / status posts (Herald/Jarvis-style) must not
  // dominate the stream — clamp anything past a modest line/char budget and
  // let the reader opt into the full rendered post. Thresholds are chars-
  // first (cheap, and catches long single-paragraph posts a line-count
  // wouldn't) with a line-count backstop for lists of many short lines.
  const CLAMP_LINES = 10;
  const CLAMP_CHARS = 600;
  function isLongBody(body: string): boolean {
    return body.length > CLAMP_CHARS || body.split('\n').length > CLAMP_LINES;
  }
  const isLong = $derived(isLongBody(message.body));
  // piece 4, item 3 — Summary means summary now: it's ALWAYS exactly one
  // line + chips, with no per-message expand into the full body anymore
  // (that job belongs to switching density to Full, or the focus reader —
  // see the mockup's own "Summary is for scanning; Full is for reading in
  // place; f is for reading one deeply"). `expanded` is therefore now
  // PURELY the full-mode long-body clamp toggle it always conceptually
  // was — summary density no longer touches it at all.
  let expanded = $state(false);
  const showBody = $derived(density === 'full');
  const showSummaryLine = $derived(density === 'summary' || isLong);

  // renderMarkdown sanitizes with DOMPurify — message bodies are untrusted,
  // peer-authored content (see markdown.ts's own header note) — so this is
  // the only string ever handed to {@html} for a message body.
  const renderedBody = $derived(renderMarkdown(message.body));
  // Summary's `⟨⟩ N` chip — counts fenced blocks straight off the rendered
  // HTML (each becomes exactly one `pre.md-fence`), so it can never drift
  // from what Full mode would actually show.
  const codeBlockCount = $derived((renderedBody.match(/<pre class="md-fence"/g) ?? []).length);

  let bodyEl: HTMLDivElement | undefined = $state();

  // piece 4, item 3 — "inline refs anchored to prose": a ref the author
  // mentioned inline (authored as `` `--ref repo:path[@sha][#range]` ``,
  // rendered by markdown.ts as a plain `code.mono` span) gets replaced
  // with a small clickable chip RIGHT THERE, instead of every ref trailing
  // in a separate list after the whole message regardless of where it was
  // actually referenced. Only refs with a REAL match in `message.refs` are
  // ever anchored (Law #3 — never invent what a bare `--ref`-looking
  // mention points to); anything left unmatched keeps the old trailing-
  // card fallback, so a real ref is never silently dropped for want of a
  // textual anchor.
  function refKey(ref: CodeRef): string {
    return `${ref.repo}:${ref.path}@${ref.sha}#${ref.range ? ref.range.join('-') : 'all'}`;
  }

  function refMatchesInlineText(ref: CodeRef, text: string): boolean {
    const m = /^--ref\s+([^:\s]+):(\S+?)(?:@(\S+?))?(?:#\S+)?$/.exec(text.trim());
    if (!m) return false;
    const [, repo, path, sha] = m;
    if (repo !== ref.repo || path !== ref.path) return false;
    if (sha && !ref.sha.startsWith(sha)) return false;
    return true;
  }

  async function openAnchoredRef(ref: CodeRef) {
    const target = `${ref.repo}:${ref.path}${ref.range ? `@${ref.range[0]}-${ref.range[1]}` : ''}`;
    try {
      const hits = await api.getRefs(hub, target, true);
      onOpenRefs?.(ref, hits);
    } catch (err) {
      console.error('confer serve: failed to load reverse-index hits', target, err);
    }
  }

  let anchoredRefKeys = $state<Set<string>>(new Set());

  function anchorInlineRefs(root: HTMLElement) {
    if (density !== 'full' || !message.refs.length) {
      if (anchoredRefKeys.size) anchoredRefKeys = new Set();
      return;
    }
    const matched = new Set<string>();
    for (const codeEl of Array.from(root.querySelectorAll<HTMLElement>('code.mono'))) {
      const text = codeEl.textContent ?? '';
      const ref = message.refs.find((r) => refMatchesInlineText(r, text));
      if (!ref) continue;
      matched.add(refKey(ref));
      const chip = document.createElement('button');
      chip.type = 'button';
      chip.className = 'inline-ref-chip mono';
      chip.textContent = `◆ ${ref.path}@${ref.sha.slice(0, 7)}`;
      chip.title = `${ref.repo}/${ref.path}${ref.range ? ` L${ref.range[0]}-${ref.range[1]}` : ''} — open reverse-index`;
      chip.addEventListener('click', (e) => {
        e.stopPropagation();
        void openAnchoredRef(ref);
      });
      codeEl.replaceWith(chip);
    }
    anchoredRefKeys = matched;
  }

  $effect(() => {
    // Re-run whenever the rendered body changes; upgrades fenced code
    // blocks to Shiki's dual-theme tokens once the (async) highlighter is
    // ready (falls back to already-safe plain text until then), and
    // anchors any inline `--ref` mentions that match a real message ref.
    void renderedBody;
    void density;
    void message.refs;
    if (bodyEl) {
      void highlightRenderedCodeBlocks(bodyEl);
      anchorInlineRefs(bodyEl);
    }
  });

  function selectMessage() {
    onSelect?.(message.id);
  }
</script>

{#if isSysline}
  <div class="sysline" class:pulse={highlight} data-type={message.type} data-msg-id={message.id}>
    <span class="tick">↳</span>
    <span><b style="color:{fromColor}">{fromDisplay}</b> {message.summary}</span>
    <span class="ts" title={formatIso8601(message.ts)}>{formatClock(message.ts)} · {message.type.toUpperCase()}</span>
  </div>
{:else}
  <div
    class="msg"
    class:sel={selected}
    class:unseen
    class:has-ticket={isTicket}
    class:pulse={highlight}
    data-type={isTicket ? 'request' : 'note'}
    data-msg-id={message.id}
    role="button"
    tabindex="0"
    onclick={selectMessage}
    onkeydown={(e) => {
      if (e.key === 'Enter' || e.key === ' ') selectMessage();
    }}
  >
    <span class="av" style="color:{fromColor};background:color-mix(in srgb, {fromColor} 18%, transparent)">{fromAbbr}</span>
    <div class="body">
      <div class="head">
        <span class="who" style="color:{fromColor}">{fromDisplay}</span>
        {#if message.host}<span class="role">{message.host}</span>{/if}
        <span class="ts" title={formatIso8601(message.ts)}>{formatClock(message.ts)}</span>
        {#if readState.isDetailViewed(message.id)}
          <!-- Completionist-safe (piece 4, item 2): marks what you HAVE
               deep-read (opened in the focus reader past the dwell/
               scroll threshold) — its ABSENCE is neutral everywhere,
               never an unread flag. No counter, no badge for the
               opposite state. -->
          <span class="detail-viewed" title="Opened in the focus reader" aria-label="Opened in the focus reader">✓</span>
        {/if}
        <CopyIdButton id={message.id} class="msg-copy-id" />
        {#if onOpenFocus}
          <button
            type="button"
            class="msg-focus-btn"
            title="Open in focus reader"
            aria-label="Open in focus reader"
            onclick={(e) => {
              e.stopPropagation();
              onOpenFocus?.(message.id);
            }}
          >
            <Icon name="arrow-up-right" size={12} />
            <Kbd keys="f" />
          </button>
        {/if}
        {#if onOpenNote && !isTicket}
          <button
            type="button"
            class="msg-inspect-btn"
            title="Open note (body + related tickets/code/thread)"
            aria-label="Open note detail"
            onclick={(e) => {
              e.stopPropagation();
              onOpenNote?.(message.id);
            }}
          >
            <Icon name="maximize" size={12} />
          </button>
        {/if}
        <SeenIndicator entries={seenEntries} {onOpenAgent} />
      </div>

      {#if isTicket && request}
        <TicketMiniCard {request} {agents} onSelect={onSelectTicket} />
      {:else}
        {#if showSummaryLine}
          <div class="summary-line">
            <span class="lead">{message.summary}</span>
            {#if density === 'summary'}
              <!-- piece 4, item 3 — Summary means summary: a one-line lead
                   plus compact chips carrying the density (◆ refs, ⟨⟩
                   code) — never the rendered blocks/cards themselves.
                   Click a chip (or switch to Full / open the focus
                   reader) to actually see them. -->
              <span class="chips">
                {#if message.refs.length}
                  <button
                    type="button"
                    class="chip ref"
                    title="{message.refs.length} code reference{message.refs.length === 1 ? '' : 's'} — open in focus reader"
                    onclick={(e) => {
                      e.stopPropagation();
                      onOpenFocus?.(message.id);
                    }}
                  >
                    ◆ {message.refs.length}
                  </button>
                {/if}
                {#if codeBlockCount}
                  <button
                    type="button"
                    class="chip code"
                    title="{codeBlockCount} code block{codeBlockCount === 1 ? '' : 's'} — open in focus reader"
                    onclick={(e) => {
                      e.stopPropagation();
                      onOpenFocus?.(message.id);
                    }}
                  >
                    ⟨⟩ {codeBlockCount}
                  </button>
                {/if}
              </span>
            {/if}
          </div>
        {/if}
        {#if showBody}
          <div class="text-wrap" class:clamped={isLong && !expanded}>
            <div class="text prose md" bind:this={bodyEl}>
              {@html renderedBody}
            </div>
            {#if isLong && !expanded}<div class="fade"></div>{/if}
          </div>
          {#if isLong}
            <button
              type="button"
              class="expand-toggle block"
              class:open={expanded}
              aria-expanded={expanded}
              aria-label={expanded ? 'Collapse message' : 'Expand message'}
              onclick={(e) => {
                e.stopPropagation();
                expanded = !expanded;
              }}
            >
              <span>{expanded ? 'Show less' : 'Show more'}</span>
              <svg class="chev" viewBox="0 0 16 16" width="11" height="11" aria-hidden="true">
                <polyline points="4 6 8 10 12 6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </button>
          {/if}
          {#if message.refs.length}
            <!-- Only refs that DIDN'T get anchored inline (see
                 anchorInlineRefs above) fall back to the trailing card
                 list — a real ref is never silently dropped for want of a
                 textual `--ref` mention to anchor it to. -->
            {#each message.refs.filter((r) => !anchoredRefKeys.has(refKey(r))) as ref, i (refKey(ref) + '#' + i)}
              <CodeRefCard {ref} {hub} onRevHook={onOpenRefs} />
            {/each}
          {/if}
        {/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .sysline {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 3px 10px;
    margin: 2px -10px;
    color: var(--muted);
    font-size: 12.5px;
  }
  .sysline .tick {
    width: 24px;
    display: grid;
    place-items: center;
    color: var(--faint);
  }
  .sysline b {
    color: var(--text);
    font-weight: 600;
  }
  .sysline .ts {
    margin-left: auto;
    font: 500 10.5px/1 var(--mono);
    color: var(--faint);
  }

  .msg {
    position: relative;
    display: flex;
    gap: 12px;
    padding: 9px 10px;
    border-radius: 10px;
    margin: 1px -10px;
    cursor: pointer;
    text-align: left;
    border: 0;
    background: transparent;
    width: 100%;
    font: inherit;
    color: inherit;
  }
  .msg:hover {
    background: var(--panel);
  }
  .msg.sel {
    background: var(--panel);
    box-shadow: inset 0 0 0 1px var(--border-2);
  }
  .msg.has-ticket:hover {
    background: transparent;
  }
  .msg.unseen::before {
    content: '';
    position: absolute;
    left: -4px;
    top: 9px;
    bottom: 9px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
    box-shadow: 0 0 9px -1px var(--accent);
  }
  /* Copy-id affordance (design/41 Phase 0): hidden until the row itself is
     hovered/focused — CopyIdButton defaults to opacity:0 and only reveals
     itself, always-visible on touch. `:global(...)` is required since the
     button lives inside a CHILD component, out of reach of this file's own
     scoped selector otherwise. */
  .msg:hover :global(.msg-copy-id),
  .msg:focus-within :global(.msg-copy-id) {
    opacity: 1;
  }
  /* "Open in focus reader" — same reveal-on-hover treatment as CopyIdButton
     above (mouse path for the `f` shortcut; the Kbd chip inside it is the
     inline-shortcut affordance the keyboard-architecture pass asks for on
     every actionable control). */
  .msg-focus-btn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    opacity: 0;
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--muted);
    border-radius: 6px;
    padding: 2px 6px;
    transition: opacity 0.12s ease;
  }
  .msg-focus-btn:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .msg:hover .msg-focus-btn,
  .msg:focus-within .msg-focus-btn,
  .msg-focus-btn:focus-visible {
    opacity: 1;
  }
  /* Piece 6's "open note" trigger — same hover/focus-reveal treatment as
     the focus-reader button right next to it. */
  .msg-inspect-btn {
    display: inline-flex;
    align-items: center;
    opacity: 0;
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--muted);
    border-radius: 6px;
    padding: 2px 6px;
    transition: opacity 0.12s ease;
  }
  .msg-inspect-btn:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .msg:hover .msg-inspect-btn,
  .msg:focus-within .msg-inspect-btn,
  .msg-inspect-btn:focus-visible {
    opacity: 1;
  }
  /* Scroll-to + highlight-pulse target (design/41 Phase 0 item 4) — a brief
     ~2s fade so a meta-thread-node / lifecycle-trail navigation is easy to
     spot once the stream scrolls to it. `prefers-reduced-motion` is
     respected upstream in ChatStream (it simply never sets `highlight`
     true), so no separate override is needed here. */
  @keyframes msg-pulse {
    0% {
      box-shadow: inset 0 0 0 0px var(--accent);
      background: color-mix(in srgb, var(--accent) 20%, var(--panel));
    }
    100% {
      box-shadow: inset 0 0 0 0px var(--accent);
      background: transparent;
    }
  }
  .msg.pulse {
    animation: msg-pulse 2s ease-out;
  }
  .sysline.pulse {
    animation: msg-pulse 2s ease-out;
    border-radius: 6px;
  }
  .av {
    width: 26px;
    height: 26px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    font: 700 10.5px/1 var(--mono);
  }
  .body {
    min-width: 0;
    flex: 1;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin-bottom: 2px;
  }
  .who {
    font-weight: 650;
    font-size: 13px;
  }
  .role {
    font: 500 10.5px/1 var(--mono);
    color: var(--faint);
    border: 1px solid var(--border-2);
    padding: 2px 5px;
    border-radius: 5px;
  }
  .ts {
    font: 500 11px/1 var(--mono);
    color: var(--faint);
  }
  /* Subtle, positive-only — never a colored/urgent badge, since its
     absence must read as neutral, not a debt (piece 4, item 2). */
  .detail-viewed {
    font: 700 10px/1 var(--mono);
    color: var(--done);
    opacity: 0.75;
  }
  .summary-line {
    display: flex;
    /* flex-start (not center): the lead can now wrap to 2 lines, and
       center-aligning would float the expand-toggle pill oddly between
       them instead of pinning it to the first line. */
    align-items: flex-start;
    gap: 8px;
    font-size: 13px;
    color: var(--text);
    margin-bottom: 4px;
  }
  .summary-line .lead {
    /* The one-line summary is the "lead" — a distinct, semibold hook.
       Everything below it (the expanded body) reads as normal-weight prose;
       see `.prose.md`'s own font-weight rule. Keeping the weight jump only
       on this one line is what makes summary vs. body read as two tiers
       instead of one undifferentiated bold wall. */
    font-weight: 650;
    min-width: 0;
    line-height: 1.42;
    /* Word-aware wrapping, clamped to 2 lines with an ellipsis — replaces
       the old hard single-line `nowrap` cutoff, which could chop a word
       mid-character. `overflow-wrap: anywhere` lets an unbreakable token
       (a long id, a URL) still wrap rather than overflow; `word-break`
       is the older Safari/webkit-line-clamp-adjacent property doing the
       same job as a belt-and-braces fallback. Stays compact — 2 lines
       max, not an unbounded paragraph — per Stefan's "tight, scannable
       list" preference. */
    overflow-wrap: anywhere;
    word-break: break-word;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  /* piece 4, item 3 — Summary's density chips (◆ refs, ⟨⟩ code): compact,
     glanceable evidence the message HAS refs/code without rendering them —
     click opens the focus reader, the one place that shows the full
     rendered content. */
  .chips {
    flex: 0 0 auto;
    display: flex;
    gap: 4px;
    margin-left: auto;
  }
  .chip {
    font: 600 10px/1 var(--mono);
    padding: 2px 6px;
    border-radius: 5px;
    border: 1px solid var(--border-2);
    color: var(--muted);
    background: var(--panel-2);
  }
  .chip:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .chip.ref {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .chip.code {
    color: var(--claimed);
    border-color: color-mix(in srgb, var(--claimed) 35%, transparent);
  }
  /* The expand/collapse control — a real, unmistakably-interactive pill with
     a text label ("Show more"/"Show less") plus a chevron, not a bare
     unlabeled glyph. Shared look for the inline (summary-line) and block
     (below a clamped full-density body) placements; `.block` only changes
     layout, not the pill's own visual language, so the same affordance shows
     up consistently everywhere a note can be expanded (chat, metathread). */
  .expand-toggle {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
    border: 1px solid var(--border-2);
    border-radius: 999px;
    background: var(--panel-2);
    padding: 3px 9px 3px 10px;
    color: var(--muted);
    font: 600 11px/1.2 var(--mono);
    letter-spacing: 0.01em;
    cursor: pointer;
    transition:
      background 0.12s ease,
      color 0.12s ease,
      border-color 0.12s ease;
  }
  .expand-toggle .chev {
    transform: rotate(0deg);
    transition: transform 0.15s ease;
  }
  .expand-toggle.open .chev {
    transform: rotate(180deg);
  }
  .expand-toggle:hover,
  .expand-toggle:focus-visible {
    color: var(--text);
    background: var(--panel-3);
    border-color: var(--accent);
  }
  .expand-toggle.block {
    margin: 6px 0 0;
  }
  .text {
    font-size: 13.5px;
    color: var(--text);
  }
  .text-wrap {
    position: relative;
  }
  .text-wrap.clamped {
    max-height: 168px;
    overflow: hidden;
  }
  .text-wrap .fade {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 56px;
    background: linear-gradient(to bottom, transparent, var(--bg) 88%);
    pointer-events: none;
  }
  /* Mention/inline-code look, and the rest of the `.prose`/`.md` markdown
     typography (headings, lists, pre/code, blockquotes, links, tables), are
     defined globally in app.css — {@html}-injected markup isn't visible to
     Svelte's scoped-CSS compiler, so it has to be matched by an unscoped
     stylesheet (or :global(...) here) either way; keeping it all in one
     place in app.css means CodeRefCard/Message/RequestDetail share exactly
     one definition of what "markdown in this app looks like". */
</style>
