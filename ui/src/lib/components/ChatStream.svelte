<script lang="ts">
  // The message feed for the current topic. Ports `.stream` from
  // design/serve-dashboard-v2-mockup.html: a daybreak divider, notes/tickets/
  // syslines in order, a "NEW · since you last looked" divider, and an
  // empty-state for topics with nothing filed yet.
  //
  // CONTRACT GAP: `Message` carries no read-receipt / seen-by data, so the
  // per-message "seen" roster and the unseen/NEW cutoff are synthesized here
  // deterministically (see buildSeenEntries/NEW_CUTOFF) rather than sourced
  // from real state. If confer serve's backend grows a real seen-by
  // projection, this is the seam to wire it in.
  import type { Agent, Message as MessageT, RequestRow } from '../types';
  import MessageComponent from './Message.svelte';
  import type { SeenEntry } from './SeenIndicator.svelte';
  import { formatClock } from '../format';

  interface Props {
    messages: MessageT[];
    requests: RequestRow[];
    agents: Agent[];
    topic: string | null;
    notesOn: boolean;
    reqsOn: boolean;
    selectedMessageId?: string | null;
    onSelectMessage?: (id: string) => void;
    onSelectTicket?: (id: string) => void;
  }

  let {
    messages,
    requests,
    agents,
    topic,
    notesOn,
    reqsOn,
    selectedMessageId = null,
    onSelectMessage,
    onSelectTicket,
  }: Props = $props();

  const agentsById = $derived(new Map(agents.map((a) => [a.id, a])));

  function findRequest(message: MessageT): RequestRow | null {
    const guess = message.id.replace(/^msg_/, 'req_');
    return requests.find((r) => r.id === guess) ?? requests.find((r) => r.summary === message.summary) ?? null;
  }

  function bucket(message: MessageT): 'note' | 'request' {
    return message.type === 'note' ? 'note' : 'request';
  }

  const topicMessages = $derived(
    messages
      .filter((m) => m.topic === topic)
      .filter((m) => (bucket(m) === 'note' ? notesOn : reqsOn))
      .sort((a, b) => new Date(a.ts).getTime() - new Date(b.ts).getTime())
  );

  // Demo "since you last looked" cutoff — see the CONTRACT GAP note above.
  const NEW_CUTOFF = new Date('2026-07-17T14:53:00Z').getTime();

  function isUnseenByYou(message: MessageT): boolean {
    return new Date(message.ts).getTime() > NEW_CUTOFF;
  }

  function buildSeenEntries(message: MessageT): SeenEntry[] {
    const others = agents.filter((a) => a.id !== message.from);
    const baseMs = new Date(message.ts).getTime();
    if (!isUnseenByYou(message)) {
      return [
        ...others.map((a, i) => ({
          id: a.id,
          name: a.display,
          color: a.color,
          ts: formatClock(new Date(baseMs + (i + 1) * 90_000).toISOString()),
        })),
        {
          id: 'you',
          name: 'You',
          ts: formatClock(new Date(baseMs + (others.length + 1) * 90_000).toISOString()),
          isYou: true,
        },
      ];
    }
    return others.map((a, i) =>
      i === 0
        ? { id: a.id, name: a.display, color: a.color, ts: formatClock(message.ts) }
        : { id: a.id, name: a.display, color: a.color, ts: null, unseen: true }
    );
  }

  const firstUnseenId = $derived(topicMessages.find((m) => isUnseenByYou(m))?.id ?? null);
</script>

<div class="stream">
  {#if topicMessages.length === 0}
    <div class="emptystate">
      <div class="es-glyph">#</div>
      <div class="es-title">No messages yet</div>
      <div class="es-body">
        Nothing has been posted in <b class="mono">#{topic}</b>. Requests and notes filed here will show up in this
        stream — for anyone watching the hub.
      </div>
    </div>
  {:else}
    <div class="daybreak">Today</div>
    {#each topicMessages as message (message.id)}
      {#if message.id === firstUnseenId}
        <div class="newmark"><span class="t">NEW · SINCE YOU LAST LOOKED</span></div>
      {/if}
      <MessageComponent
        {message}
        fromAgent={agentsById.get(message.from)}
        request={message.type === 'request' ? findRequest(message) : null}
        selected={selectedMessageId === message.id}
        unseen={isUnseenByYou(message)}
        seenEntries={buildSeenEntries(message)}
        onSelect={onSelectMessage}
        onSelectTicket={onSelectTicket}
      />
    {/each}
  {/if}
</div>

<style>
  .stream {
    overflow-y: auto;
    flex: 1;
    padding: 18px 20px 40px;
  }
  .daybreak {
    display: flex;
    align-items: center;
    gap: 12px;
    margin: 6px 0 16px;
    color: var(--faint);
    font: 600 10.5px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .daybreak::before,
  .daybreak::after {
    content: '';
    height: 1px;
    background: var(--border);
    flex: 1;
  }
  .newmark {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 18px 0 8px;
  }
  .newmark .t {
    font: 800 9px/1 var(--mono);
    letter-spacing: 0.15em;
    color: var(--accent);
    white-space: nowrap;
  }
  .newmark::before {
    content: '';
    height: 1px;
    width: 22px;
    background: var(--accent);
    flex: 0 0 auto;
  }
  .newmark::after {
    content: '';
    height: 1px;
    flex: 1;
    background: linear-gradient(90deg, var(--accent), transparent);
  }
  .emptystate {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 9px;
    max-width: 420px;
    margin: 40px auto 0;
    padding: 0 20px;
    text-align: left;
  }
  .es-glyph {
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--panel-2);
    border: 1px solid var(--border-2);
    display: grid;
    place-items: center;
    font: 700 16px/1 var(--mono);
    color: var(--faint);
    margin-bottom: 4px;
  }
  .es-title {
    font-weight: 650;
    font-size: 14px;
    color: var(--text);
  }
  .es-body {
    font-size: 12.5px;
    color: var(--muted);
    line-height: 1.55;
  }
</style>
