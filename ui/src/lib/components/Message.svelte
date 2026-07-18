<script lang="ts">
  import type { Agent, CodeRef, Message as MessageT, RefHit, RequestRow } from '../types';
  import { formatClock, formatIso8601 } from '../format';
  import { renderMarkdown, highlightRenderedCodeBlocks } from '../markdown';
  import SeenIndicator, { type SeenEntry } from './SeenIndicator.svelte';
  import TicketCard from './TicketCard.svelte';
  import CodeRefCard from './CodeRefCard.svelte';
  import CopyIdButton from './CopyIdButton.svelte';

  interface Props {
    message: MessageT;
    fromAgent?: Agent;
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
  }

  let {
    message,
    fromAgent,
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
  // Per-message expand state, independent of every other message and of the
  // global `density` toggle: in 'summary' density it governs whether the
  // full body is revealed at all; in 'full' density it's the pre-existing
  // long-body show-more/show-less flag. An individually-expanded message
  // stays expanded until collapsed, even if the global toggle flips.
  let expanded = $state(false);
  const showBody = $derived(density === 'full' || expanded);
  const showSummaryLine = $derived(density === 'summary' || isLong);

  // renderMarkdown sanitizes with DOMPurify — message bodies are untrusted,
  // peer-authored content (see markdown.ts's own header note) — so this is
  // the only string ever handed to {@html} for a message body.
  const renderedBody = $derived(renderMarkdown(message.body));

  let bodyEl: HTMLDivElement | undefined = $state();
  $effect(() => {
    // Re-run whenever the rendered body changes; upgrades fenced code
    // blocks to Shiki's dual-theme tokens once the (async) highlighter is
    // ready. Falls back to (already-safe) plain text until then.
    void renderedBody;
    if (bodyEl) void highlightRenderedCodeBlocks(bodyEl);
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
        <CopyIdButton id={message.id} class="msg-copy-id" />
        <SeenIndicator entries={seenEntries} />
      </div>

      {#if isTicket && request}
        <TicketCard {request} onSelect={onSelectTicket} />
      {:else}
        {#if showSummaryLine}
          <!-- Intentionally not its own tab stop: the outer `.msg` row (role="button",
               tabindex, Enter/Space -> selectMessage) already makes this fully keyboard-
               reachable. This div's onclick is a mouse-only convenience — clicking the
               summary text also expands the body, matching what `.clickable` promises —
               layered on TOP of that existing keyboard path, not a replacement for it. -->
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="summary-line"
            class:clickable={density === 'summary'}
            onclick={
              density === 'summary'
                ? () => {
                    // No stopPropagation here — this is a DIFFERENT action
                    // from the chevron's own toggle (which does stop
                    // propagation, to avoid a double-toggle when the click
                    // lands on the chevron itself). Letting this bubble to
                    // the outer `.msg` row means clicking the summary text
                    // both expands AND selects the message — the row stays
                    // clickable, exactly as `.clickable` promises.
                    expanded = !expanded;
                  }
                : undefined
            }
          >
            <span class="lead">{message.summary}</span>
            {#if density === 'summary'}
              <button
                type="button"
                class="expand-toggle"
                class:open={showBody}
                aria-expanded={showBody}
                aria-label={showBody ? 'Collapse message' : 'Expand message'}
                onclick={(e) => {
                  e.stopPropagation();
                  expanded = !expanded;
                }}
              >
                <span>{showBody ? 'Show less' : 'Show more'}</span>
                <svg class="chev" viewBox="0 0 16 16" width="11" height="11" aria-hidden="true">
                  <polyline points="4 6 8 10 12 6" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
                </svg>
              </button>
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
        {/if}
        {#if message.refs.length}
          {#each message.refs as ref, i (ref.repo + ':' + ref.path + '@' + ref.sha + '#' + (ref.range ? ref.range.join('-') : 'all') + '#' + i)}
            <CodeRefCard {ref} {hub} onRevHook={onOpenRefs} />
          {/each}
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
  .summary-line.clickable {
    cursor: pointer;
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
