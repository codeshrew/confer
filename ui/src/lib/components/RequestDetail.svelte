<script lang="ts">
  // Right-rail request detail — the connective tissue between chat and
  // board. Ports `.lctrail`/`.lcev`/`.kv` from
  // design/serve-dashboard-v2-mockup.html: a lifecycle trail folded from
  // signed commits (no mutable status field), addressee/claimant/topic/age,
  // any pinned `--ref`s, and the reverse-index hook.
  //
  // CONTRACT GAP: RequestRow carries only the *current* status, not history.
  // The trail below is reconstructed from the message stream by walking
  // `of`/`replyTo` back to the request's originating message (same
  // `req_`/`msg_` id convention ChatStream/App already rely on) — this is a
  // projection, same seam as MetaThread's.
  import type { Agent, CodeRef, Message as MessageT, MsgType, RefHit, RequestRow } from '../types';
  import { formatAgeFromSecs, formatClock } from '../format';
  import { renderMarkdown, highlightRenderedCodeBlocks } from '../markdown';
  import CodeRefCard from './CodeRefCard.svelte';

  interface Props {
    request: RequestRow;
    messages: MessageT[];
    agents: Agent[];
    hub: string;
    onOpenRefs?: (ref: CodeRef, hits: RefHit[]) => void;
    /** A lifecycle-trail row was clicked — jump to the underlying message in
     * Chat (design/41 Phase 0, §4 "clickable lifecycle-trail rows"). `topic`
     * is the message's own topic, so the caller can switch if it differs
     * from whatever's currently showing. */
    onSelectMessage?: (msgId: string, topic: string | null) => void;
  }

  let { request, messages, agents, hub, onOpenRefs, onSelectMessage }: Props = $props();

  const agentsById = $derived(new Map(agents.map((a) => [a.id, a])));

  function cap(s: string): string {
    return s.length ? s[0]!.toUpperCase() + s.slice(1) : s;
  }

  const STATE_VAR: Record<MsgType, string> = {
    request: 'var(--open)',
    claim: 'var(--claimed)',
    blocked: 'var(--blocked)',
    done: 'var(--done)',
    error: 'var(--error)',
    note: 'var(--muted)',
    defer: 'var(--deferred)',
    supersede: 'var(--deferred)',
  };

  const LABEL: Record<MsgType, string> = {
    request: 'filed',
    claim: 'claim',
    blocked: 'blocked',
    done: 'done',
    error: 'error',
    note: 'note',
    defer: 'deferred',
    supersede: 'superseded',
  };

  const originMsgId = $derived(request.id.replace(/^req_/, 'msg_'));
  const originMsg = $derived(messages.find((m) => m.id === originMsgId) ?? null);

  const trailMsgs = $derived.by((): MessageT[] => {
    const related = messages
      .filter((m) => m.id !== originMsgId && (m.of === originMsgId || m.replyTo === originMsgId))
      .sort((a, b) => new Date(a.ts).getTime() - new Date(b.ts).getTime());
    return originMsg ? [originMsg, ...related] : related;
  });

  interface TrailEvent {
    msgId: string;
    topic: string | null;
    stateVar: string;
    label: string;
    who: string;
    ts: string;
    note: string | null;
  }

  const trail = $derived.by((): TrailEvent[] =>
    trailMsgs.map((m, i) => ({
      msgId: m.id,
      topic: m.topic,
      stateVar: STATE_VAR[m.type],
      label: i === 0 ? 'filed' : LABEL[m.type],
      who: agentsById.get(m.from)?.display ?? cap(m.from),
      ts: formatClock(m.ts),
      note: i === 0 ? null : m.summary,
    }))
  );

  const refs = $derived.by((): CodeRef[] => {
    const seen = new Set<string>();
    const out: CodeRef[] = [];
    for (const m of trailMsgs) {
      for (const r of m.refs) {
        const key = `${r.repo}:${r.path}:${r.sha}`;
        if (!seen.has(key)) {
          seen.add(key);
          out.push(r);
        }
      }
    }
    return out;
  });

  const claimant = $derived(request.claimants[0] ?? null);
  const addressee = $derived(request.to.length ? request.to.map((t) => `@${cap(t)}`).join(', ') : '@all');
  const ageLabel = $derived(formatAgeFromSecs(request.ageSecs));
  const statusKey = $derived(request.status.toLowerCase());

  // The full message/detail view — unlike the clamped stream (Message.svelte),
  // this renders the ORIGINATING message's body in full, no truncation:
  // this drawer is the "one tap away" full-content destination the clamp in
  // the stream promises. Still goes through the same sanitized renderer —
  // the body is equally untrusted, peer-authored content here.
  const descriptionHtml = $derived(originMsg ? renderMarkdown(originMsg.body) : null);

  let descEl: HTMLDivElement | undefined = $state();
  $effect(() => {
    void descriptionHtml;
    if (descEl) void highlightRenderedCodeBlocks(descEl);
  });
</script>

<div class="reqdetail">
  <div class="kv"><span class="k2">Status</span><span class="v2"><span class="pill p-{statusKey}">{request.status}</span></span></div>
  <div class="kv"><span class="k2">Ticket</span><span class="v2 mono">{request.id}</span></div>
  <div class="kv"><span class="k2">Filed by</span><span class="v2">{agentsById.get(request.from)?.display ?? cap(request.from)}</span></div>
  <div class="kv"><span class="k2">Addressed to</span><span class="v2">{addressee}</span></div>
  <div class="kv"><span class="k2">Claimant</span><span class="v2">{claimant ? (agentsById.get(claimant)?.display ?? cap(claimant)) : 'unclaimed'}</span></div>
  <div class="kv"><span class="k2">Topic</span><span class="v2 mono">#{request.topic ?? '—'}</span></div>
  <div class="kv"><span class="k2">Age</span><span class="v2 mono">{ageLabel}</span></div>

  {#if descriptionHtml}
    <p class="ctx-note">Description — full message, rendered in full (no clamp):</p>
    <div class="text prose md" bind:this={descEl}>
      {@html descriptionHtml}
    </div>
  {/if}

  <p class="ctx-note">Lifecycle — folded from signed commits, no mutable status field:</p>
  <div class="lctrail">
    {#each trail as ev, i (i)}
      <div class="lcev">
        <span class="lcdot" style="--st:{ev.stateVar}"></span>
        <button
          type="button"
          class="lccard"
          onclick={() => onSelectMessage?.(ev.msgId, ev.topic)}
          aria-label={`Jump to the message where this was ${ev.label}`}
        >
          <div class="lchead">
            <span class="lclabel" style="--st:{ev.stateVar}">{ev.label}</span>
            <span class="lcwho">{ev.who}</span>
            <span class="lcts">{ev.ts}</span>
          </div>
          {#if ev.note}<div class="lcnote">{ev.note}</div>{/if}
        </button>
      </div>
    {/each}
  </div>

  {#each refs as ref (ref.path + ref.sha)}
    <CodeRefCard {ref} {hub} onRevHook={onOpenRefs} />
  {/each}
</div>

<style>
  .reqdetail {
    text-align: left;
  }
  .ctx-note {
    color: var(--muted);
    font-size: 12.5px;
    margin: 14px 0 8px;
  }
  .kv {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
    font-size: 12.5px;
  }
  .kv .k2 {
    color: var(--muted);
  }
  .kv .v2 {
    color: var(--text);
    font-weight: 550;
    text-align: right;
  }
  .kv .v2.mono {
    font-family: var(--mono);
    font-weight: 500;
  }
  .pill {
    font: 700 9.5px/1 var(--mono);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    padding: 4px 7px;
    border-radius: 6px;
  }
  .p-open {
    color: var(--open);
    background: color-mix(in srgb, var(--open) 16%, transparent);
  }
  .p-claimed {
    color: var(--claimed);
    background: color-mix(in srgb, var(--claimed) 16%, transparent);
  }
  .p-blocked {
    color: var(--blocked);
    background: color-mix(in srgb, var(--blocked) 16%, transparent);
  }
  .p-done {
    color: var(--done);
    background: color-mix(in srgb, var(--done) 16%, transparent);
  }
  .p-error {
    color: var(--error);
    background: color-mix(in srgb, var(--error) 16%, transparent);
  }
  .p-superseded {
    color: var(--deferred);
    background: color-mix(in srgb, var(--deferred) 16%, transparent);
  }
  .lctrail {
    margin: 2px 0 6px;
  }
  .lcev {
    position: relative;
    padding: 0 0 17px 22px;
  }
  .lcev::before {
    content: '';
    position: absolute;
    left: 6px;
    top: 19px;
    bottom: -3px;
    width: 2px;
    background: var(--border-2);
  }
  .lcev:last-child::before {
    display: none;
  }
  .lcev:last-child {
    padding-bottom: 2px;
  }
  .lcev .lcdot {
    position: absolute;
    left: 0;
    top: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--st);
    box-shadow: 0 0 0 3px var(--panel);
  }
  .lcev .lccard {
    display: block;
    width: 100%;
    text-align: left;
    font: inherit;
    color: inherit;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 9px 11px;
    cursor: pointer;
    transition:
      background 0.12s ease,
      border-color 0.12s ease;
  }
  .lcev .lccard:hover,
  .lcev .lccard:focus-visible {
    background: var(--panel-3);
    border-color: var(--accent);
  }
  .lcev .lchead {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 2px;
  }
  .lcev .lclabel {
    font: 800 9.5px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--st);
    border: 1.5px solid var(--st);
    border-radius: 5px;
    padding: 2px 5px;
  }
  .lcev .lcwho {
    font-weight: 650;
    font-size: 12.5px;
  }
  .lcev .lcts {
    margin-left: auto;
    font: 500 10.5px/1 var(--mono);
    color: var(--faint);
  }
  .lcev .lcnote {
    font-size: 12px;
    color: var(--muted);
    margin-top: 3px;
  }
</style>
