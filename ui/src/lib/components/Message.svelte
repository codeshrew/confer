<script lang="ts">
  import type { Agent, CodeRef, Message as MessageT, RefHit, RequestRow } from '../types';
  import { formatClock, formatIso8601 } from '../format';
  import { renderMarkdown, highlightRenderedCodeBlocks } from '../markdown';
  import SeenIndicator, { type SeenEntry } from './SeenIndicator.svelte';
  import TicketCard from './TicketCard.svelte';
  import CodeRefCard from './CodeRefCard.svelte';

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
  <div class="sysline" data-type={message.type}>
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
    data-type={isTicket ? 'request' : 'note'}
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
            {#if density === 'summary'}
              <button
                type="button"
                class="expand-chevron"
                class:open={showBody}
                aria-expanded={showBody}
                aria-label={showBody ? 'Collapse message' : 'Expand message'}
                onclick={(e) => {
                  e.stopPropagation();
                  expanded = !expanded;
                }}
              >
                ▸
              </button>
            {/if}
            <span>{message.summary}</span>
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
              class="show-more"
              onclick={(e) => {
                e.stopPropagation();
                expanded = !expanded;
              }}
            >
              {expanded ? '▴ Show less' : '▾ Show more'}
            </button>
          {/if}
        {/if}
        {#if message.refs.length}
          {#each message.refs as ref (ref.path + ref.sha)}
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
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 650;
    color: var(--text);
    margin-bottom: 3px;
  }
  .summary-line.clickable {
    cursor: pointer;
  }
  /* A real, obvious tap target — not just a bare 10px glyph. ≥24×24px
     (accessibility + phone) via padding, with a hover/focus background so it
     reads as interactive at a glance, not just on close inspection. */
  .expand-chevron {
    flex: 0 0 auto;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    margin: -3px 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    padding: 0;
    color: var(--muted);
    font-size: 14px;
    line-height: 1;
    cursor: pointer;
    transform: rotate(0deg);
    transition:
      transform 0.12s ease,
      background 0.12s ease,
      color 0.12s ease;
  }
  .expand-chevron.open {
    transform: rotate(90deg);
  }
  .expand-chevron:hover,
  .expand-chevron:focus-visible {
    color: var(--text);
    background: var(--panel-2);
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
  .show-more {
    margin-top: 4px;
    margin-left: -6px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--accent);
    font: 600 11.5px/1 var(--mono);
    padding: 6px;
    cursor: pointer;
  }
  .show-more:hover,
  .show-more:focus-visible {
    background: var(--panel-2);
    text-decoration: underline;
  }
  /* Mention/inline-code look, and the rest of the `.prose`/`.md` markdown
     typography (headings, lists, pre/code, blockquotes, links, tables), are
     defined globally in app.css — {@html}-injected markup isn't visible to
     Svelte's scoped-CSS compiler, so it has to be matched by an unscoped
     stylesheet (or :global(...) here) either way; keeping it all in one
     place in app.css means CodeRefCard/Message/RequestDetail share exactly
     one definition of what "markdown in this app looks like". */
</style>
