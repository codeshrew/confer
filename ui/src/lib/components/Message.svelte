<script lang="ts">
  import type { Agent, CodeRef, Message as MessageT, RefHit, RequestRow } from '../types';
  import { formatClock } from '../format';
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

  type Seg = { type: 'text' | 'mention' | 'code'; value: string };
  function segmentBody(body: string): Seg[] {
    const segs: Seg[] = [];
    const re = /(@[A-Za-z][\w-]*)|(`[^`]+`)/g;
    let last = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(body))) {
      if (m.index > last) segs.push({ type: 'text', value: body.slice(last, m.index) });
      if (m[1]) segs.push({ type: 'mention', value: m[1] });
      else if (m[2]) segs.push({ type: 'code', value: m[2].slice(1, -1) });
      last = re.lastIndex;
    }
    if (last < body.length) segs.push({ type: 'text', value: body.slice(last) });
    return segs;
  }
  const segs = $derived(segmentBody(message.body));

  function selectMessage() {
    onSelect?.(message.id);
  }
</script>

{#if isSysline}
  <div class="sysline" data-type={message.type}>
    <span class="tick">↳</span>
    <span><b style="color:{fromColor}">{fromDisplay}</b> {message.summary}</span>
    <span class="ts">{formatClock(message.ts)} · {message.type.toUpperCase()}</span>
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
        <span class="ts">{formatClock(message.ts)}</span>
        <SeenIndicator entries={seenEntries} />
      </div>

      {#if isTicket && request}
        <TicketCard {request} onSelect={onSelectTicket} />
      {:else}
        <div class="text">
          {#each segs as seg, i (i)}
            {#if seg.type === 'mention'}<span class="mention">{seg.value}</span
              >{:else if seg.type === 'code'}<code class="mono">{seg.value}</code
              >{:else}{seg.value}{/if}
          {/each}
        </div>
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
  .text {
    font-size: 13.5px;
    color: var(--text);
  }
  .text :global(.mention) {
    color: var(--accent);
    font-weight: 600;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    padding: 0 4px;
    border-radius: 5px;
  }
  .text :global(code.mono) {
    font-family: var(--mono);
  }
</style>
