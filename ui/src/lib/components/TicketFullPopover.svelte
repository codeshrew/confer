<script lang="ts">
  // The "Full" tier of piece 5's ticket card trio (ui/REDESIGN.md, "the
  // composable card system") — `redesign-mockups/05-board-cockpit.html`'s
  // ticket detail popover: the lifecycle track (Requested→Claimed→Done,
  // adapting per state) as the HERO, a meta grid, and a 2-line teaser only
  // — a LAUNCHPAD, not a container (no thread list, no full body; those
  // live one jump away). Replaces RequestDetail.svelte's old right-rail
  // role: rendered as a centered OVERLAY instead (same `.fr-overlay`/
  // `.fr-backdrop`/`.fr-panel` convention FocusReader.svelte established),
  // reachable from BOTH Chat (a TicketMiniCard's onSelect) and Board (a
  // TicketRow's onSelect) — "never a dead end" for either.
  //
  // Only REQUESTS get this card (they have a lifecycle) — callers only ever
  // pass a `requestId` that resolves against `requests`, never a plain
  // message id.
  import type { Agent, CodeRef, Message, RefHit, RequestRow } from '../types';
  import { formatAgeFromSecs } from '../format';
  import { buildLifecycleTrack, ticketOriginMessage, ticketRefs, ticketStateLabel, ticketStateOf, ticketStateVar, type LifecycleStage } from '../ticketState';
  import { isTypingTarget } from '../keys';
  import { api } from '../api';
  import { copyToClipboard } from '../clipboard';
  import CopyIdButton from './CopyIdButton.svelte';
  import CopiedToast from './CopiedToast.svelte';

  interface Props {
    /** Separate from `requestId` (mirrors FocusReader's `open`+`msgId`
     * pair): closing the popover (Esc, the ✕) must NOT clear which ticket
     * is selected — "esc → back to the board where you were" means the
     * row highlight and the right rail's meta-thread stay put, only the
     * overlay hides. */
    open: boolean;
    requestId: string | null;
    /** The navigable list `j`/`k` walks — Board passes its own FILTERED
     * list (so prev/next respects whatever's on screen); Chat passes the
     * hub's full request set. */
    requests: RequestRow[];
    /** Full, unpaginated per-hub messages — needed to reconstruct the
     * lifecycle track and the origin message (see ticketState.ts). */
    messages: Message[];
    agents: Agent[];
    hub: string;
    /** True while the focus reader is ALSO open — this popover auto-closes
     * rather than stack behind it (launchpad, not a container: "focus
     * read" hands off, it doesn't layer). */
    focusReaderOpen?: boolean;
    /** Piece 10 Phase A (overlayStack.svelte.ts) — true when this popover
     * is NESTED on top of another overlay (opened from within the agent
     * dossier or a note, not top-level from Chat/Board). Shows a "‹ back"
     * affordance alongside the close button — same action as `onClose`
     * (both pop exactly one stack layer), just framed as "return to where
     * I came from" rather than "I'm done." */
    hasParent?: boolean;
    onOpenThread?: (msgId: string, topic: string | null) => void;
    onFocusRead?: (msgId: string) => void;
    onOpenRefs?: (ref: CodeRef, hits: RefHit[]) => void;
    onNavigate?: (id: string) => void;
    onClose?: () => void;
  }

  let { open, requestId, requests, messages, agents, hub, focusReaderOpen = false, hasParent = false, onOpenThread, onFocusRead, onOpenRefs, onNavigate, onClose }: Props = $props();

  const agentsById = $derived(new Map(agents.map((a) => [a.id, a])));
  const request = $derived(requestId ? (requests.find((r) => r.id === requestId) ?? null) : null);
  const showing = $derived(open && request !== null);

  function display(agentId: string): string {
    const a = agentsById.get(agentId);
    if (a) return a.display;
    return agentId.length ? agentId[0]!.toUpperCase() + agentId.slice(1) : agentId;
  }

  // Named `workState`, not `state` — a local named `state` collides with
  // Svelte's `$state` RUNE (the compiler reads `$state` as "auto-subscribe
  // to the store named `state`" when one's in scope, the same mechanism
  // `$appState` relies on elsewhere in this codebase) and svelte-check
  // fails the file outright once `toastText = $state(...)` appears below.
  const workState = $derived(request ? ticketStateOf(request) : 'open');
  const stateVar = $derived(ticketStateVar(workState));
  const track = $derived(request ? buildLifecycleTrack(request, messages, agents) : null);

  // The connector's colored progress-fill — how far into the 3-node track
  // (Requested/Claimed/Done) real progress reaches, matching the mockup's
  // own `.track3 .prog{left:12%;width:38%}` for the in-flight (Claimed
  // current) case exactly: 12%→50% (a node's center, for a 3-column even
  // grid) is 38% wide. A stuck ticket's fill stops at the branch point
  // instead of wherever `stages` alone would say, so the fill visually
  // agrees with the red branch below it.
  const progressWidth = $derived.by((): number => {
    if (!track) return 0;
    if (track.branch) return track.branch.off === 'Claimed' ? 38 : 0;
    let reachedIndex = 0;
    track.stages.forEach((stage, i) => {
      if (stage.state === 'done' || stage.state === 'current') reachedIndex = i;
    });
    return reachedIndex >= 2 ? 76 : reachedIndex === 1 ? 38 : 0;
  });

  const refs = $derived(request ? ticketRefs(request, messages) : []);
  const originMsg = $derived(request ? ticketOriginMessage(request, messages) : null);
  // A 2-line TEASER only — plain text, not rendered markdown, and never the
  // full body: this popover is explicitly a launchpad, not a reading
  // surface (that's the focus reader's job, one jump away).
  const teaser = $derived(originMsg?.body.replace(/\s+/g, ' ').trim() ?? null);

  const index = $derived(requestId ? requests.findIndex((r) => r.id === requestId) : -1);
  const prevId = $derived(index > 0 ? (requests[index - 1]?.id ?? null) : null);
  const nextId = $derived(index >= 0 && index < requests.length - 1 ? (requests[index + 1]?.id ?? null) : null);

  function shortId(id: string): string {
    return id.length > 10 ? `${id.slice(0, 8)}…` : id;
  }

  function stageClass(stage: LifecycleStage, label: LifecycleStage['label']): string {
    if (track?.branch && track.branch.off === label) return 'stuck';
    return stage.state;
  }

  async function openRefHit(ref: CodeRef) {
    const target = `${ref.repo}:${ref.path}${ref.range ? `@${ref.range[0]}-${ref.range[1]}` : ''}`;
    try {
      const hits = await api.getRefs(hub, target, true);
      onOpenRefs?.(ref, hits);
    } catch (err) {
      console.error('confer serve: failed to load reverse-index hits', target, err);
    }
  }

  let toastText = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  async function copyTicketId() {
    if (!request) return;
    const ok = await copyToClipboard(request.id);
    if (!ok) return;
    toastText = `copied ${shortId(request.id)}`;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toastText = null;
    }, 1500);
  }

  // Launchpad, not a container: opening the thread or the focus reader
  // hands off entirely rather than stacking another overlay on top.
  function handleOpenThread() {
    if (!originMsg) return;
    onOpenThread?.(originMsg.id, originMsg.topic);
  }
  function handleFocusRead() {
    if (!originMsg) return;
    onFocusRead?.(originMsg.id);
  }

  $effect(() => {
    if (focusReaderOpen) onClose?.();
  });

  function handleKeydown(e: KeyboardEvent) {
    if (!showing) return;
    if (isTypingTarget(e.target)) return;
    switch (e.key) {
      case 'j':
        if (nextId) {
          e.preventDefault();
          onNavigate?.(nextId);
        }
        break;
      case 'k':
        if (prevId) {
          e.preventDefault();
          onNavigate?.(prevId);
        }
        break;
      case 'y':
        e.preventDefault();
        void copyTicketId();
        break;
      case 'Escape':
        e.preventDefault();
        // Piece 10 Phase A — `onClose` pops the overlay stack, which can
        // SYNCHRONOUSLY reveal a parent frame underneath (e.g. the agent
        // dossier this ticket was pushed over). That parent's own
        // `<svelte:window onkeydown>` is always attached (each popover
        // gates itself on its own `showing`, not on being mounted at all),
        // so without this, the SAME keydown event would cascade straight
        // into ITS Escape case too — a single Esc popping two layers at
        // once. `stopImmediatePropagation` (not just `stopPropagation` —
        // these are sibling listeners on the SAME `window` target, not an
        // ancestor) stops any not-yet-invoked listener for this dispatch.
        e.stopImmediatePropagation();
        onClose?.();
        break;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if showing && request && track}
  <div class="tk-overlay">
    <div class="tk-backdrop" onclick={onClose} aria-hidden="true" data-testid="ticket-popover-backdrop"></div>
    <div class="tk-panel" style="--c:{stateVar}" role="dialog" aria-modal="true" aria-label="Ticket detail" tabindex="-1" data-testid="ticket-popover">
      <div class="tk-head">
        <span class="tk-kind">request</span>
        <span class="tk-id mono">{request.id}</span>
        {#if request.topic}<span class="tk-topic mono">#{request.topic}</span>{/if}
        <CopiedToast text={toastText} />
        {#if hasParent}
          <button type="button" class="tk-back" aria-label="Back" title="Back to where you opened this from" onclick={onClose}>‹ back</button>
        {/if}
        <button type="button" class="tk-close" aria-label="Close ticket" onclick={onClose}>esc ✕</button>
      </div>

      <div class="tk-body">
        <h3 class="tk-title">{request.summary}</h3>

        <div class="track3" class:has-branch={!!track.branch}>
          <span class="prog" aria-hidden="true" style="width:{progressWidth}%"></span>
          {#each track.stages as stage (stage.label)}
            {@const cls = stageClass(stage, stage.label)}
            <div class="st st-{cls}">
              <span class="knob">{cls === 'done' ? '✓' : cls === 'stuck' ? '⊘' : cls === 'current' ? '●' : '○'}</span>
              <span class="sl">{stage.label}</span>
              <span class="sm mono">
                {#if stage.who}{stage.who}<br />{stage.ts}{:else if stage.state === 'pending' && cls !== 'stuck'}—<br
                  />pending{:else}—{/if}
              </span>
            </div>
          {/each}
        </div>
        {#if track.branch}
          <p class="tk-branch">⊘ stuck at <b>{track.branch.off}</b> — {track.branch.who}, {track.branch.ts}: {track.branch.reason}</p>
        {/if}
        {#if track.resolution}
          <p class="tk-resolution">✓ resolved: {track.resolution}</p>
        {/if}

        <div class="tk-meta">
          <div class="mrow"><span class="mk">Requester</span><span class="mv">{display(request.from)}</span></div>
          <div class="mrow">
            <span class="mk">Assignee</span><span class="mv">{request.claimants[0] ? display(request.claimants[0]) : 'unclaimed'}</span>
          </div>
          <div class="mrow"><span class="mk">Age</span><span class="mv mono">{formatAgeFromSecs(request.ageSecs)} · {ticketStateLabel(workState)}</span></div>
          <div class="mrow">
            <span class="mk">Refs</span>
            <span class="mv refs">
              {#each refs as ref (ref.repo + ':' + ref.path + '@' + ref.sha)}
                <button type="button" class="refchip mono" onclick={() => void openRefHit(ref)}>{ref.path}</button>
              {:else}
                <span class="mv-none">none</span>
              {/each}
            </span>
          </div>
        </div>

        {#if teaser}
          <p class="tk-excerpt">{teaser}</p>
        {/if}
      </div>

      <div class="tk-foot">
        <button type="button" class="fbtn" disabled={!originMsg} onclick={handleOpenThread}>open thread ›</button>
        <button type="button" class="fbtn" disabled={!originMsg} onclick={handleFocusRead}>focus read · <span class="kk">f</span></button>
        <CopyIdButton id={request.id} class="tk-copy-id" />
        <span class="tk-nav">
          <button type="button" class="tk-navbtn" disabled={!prevId} onclick={() => prevId && onNavigate?.(prevId)} aria-label="Previous ticket">‹</button>
          <span class="kk">j</span><span class="kk">k</span> prev · next ticket
          <button type="button" class="tk-navbtn" disabled={!nextId} onclick={() => nextId && onNavigate?.(nextId)} aria-label="Next ticket">›</button>
        </span>
      </div>
    </div>
  </div>
{/if}

<style>
  .tk-overlay {
    position: fixed;
    inset: 0;
    z-index: 62;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--phi2, 24px);
  }
  .tk-backdrop {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--bg) 72%, transparent);
    backdrop-filter: blur(2px);
  }
  .tk-panel {
    position: relative;
    width: min(560px, 100%);
    max-height: 88vh;
    overflow: auto;
    background: var(--bg-deep, var(--panel));
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
  }
  .tk-head {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 13px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--panel-2);
  }
  .tk-kind {
    font: 700 9.5px/1 var(--mono);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--c);
    background: color-mix(in srgb, var(--c) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--c) 40%, transparent);
    border-radius: 4px;
    padding: 2px 6px;
  }
  .tk-id {
    font-size: 12.5px;
    color: var(--text);
  }
  .tk-topic {
    font-size: 11px;
    color: var(--muted);
  }
  .tk-close {
    margin-left: auto;
    font: 500 11px/1 var(--mono);
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 7px;
    background: transparent;
    cursor: pointer;
  }
  .tk-close:hover {
    color: var(--text);
    border-color: var(--faint);
  }
  /* Piece 10 Phase A — shown only when this popover is nested (pushed on
     top of the agent dossier or a note). `margin-left: auto` moves to THIS
     button when present, since it's now the first of the two right-aligned
     controls; `.tk-close` keeps its own `margin-left: auto` for the
     no-parent case where it's alone. */
  .tk-back {
    margin-left: auto;
    font: 500 11px/1 var(--mono);
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 8px;
    background: transparent;
    cursor: pointer;
  }
  .tk-back:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .tk-back + .tk-close {
    margin-left: 0;
  }
  .tk-body {
    padding: 17px 19px;
  }
  .tk-title {
    font-size: 16px;
    font-weight: 640;
    line-height: 1.35;
    margin: 0 0 18px;
    letter-spacing: -0.01em;
    color: var(--text);
  }

  .track3 {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    position: relative;
    margin: 0 0 10px;
  }
  .track3::before {
    content: '';
    position: absolute;
    top: 12px;
    left: 12%;
    right: 12%;
    height: 2px;
    background: var(--border-2);
  }
  /* MUST be `position: absolute` — `.track3` is a 3-column grid (one
     column per lifecycle stage) and this was previously a plain, unstyled
     grid CHILD alongside the 3 `.st` stage divs. A 4th item in a 3-column
     grid wraps: Requested + Claimed filled row 1, and Done dropped to a
     misaligned row 2 below. Taking `.prog` out of grid flow (absolute,
     layered via `.track3`'s own `position: relative`) fixes the wrap AND
     lets it double as the real colored progress-fill (see
     `progressWidth` in the script) instead of dead, layout-breaking
     markup. */
  .track3 .prog {
    position: absolute;
    top: 12px;
    left: 12%;
    height: 2px;
    background: var(--c);
    transition: width 0.2s ease;
  }
  .st {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 5px;
    position: relative;
    z-index: 1;
    /* A grid item's default min-width is `auto` (its content's own
       min-content size), which can win over the `1fr` track and force a
       wrap when a stage's actor name is long — 0 lets it actually shrink
       to the column it's given, matching every other 3-across grid cell
       in this app (App.svelte's own `.center` has the identical note). */
    min-width: 0;
  }
  .st .knob {
    width: 25px;
    height: 25px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    font: 700 11px/1 var(--mono);
    border: 2px solid var(--border-2);
    background: var(--bg-deep, var(--panel));
    color: var(--muted);
  }
  .st-done .knob {
    background: var(--c);
    border-color: var(--c);
    color: #0b0e17;
  }
  .st-current .knob {
    border-color: var(--c);
    color: var(--c);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--c) 20%, transparent);
    animation: tk-pulse 2.6s ease-in-out infinite;
  }
  .st-stuck .knob {
    border-color: var(--state-stuck);
    color: var(--state-stuck);
    background: color-mix(in srgb, var(--state-stuck) 15%, transparent);
  }
  @media (prefers-reduced-motion: reduce) {
    .st-current .knob {
      animation: none;
    }
  }
  @keyframes tk-pulse {
    0%,
    100% {
      box-shadow: 0 0 0 4px color-mix(in srgb, var(--c) 18%, transparent);
    }
    50% {
      box-shadow: 0 0 0 6px color-mix(in srgb, var(--c) 26%, transparent);
    }
  }
  .st .sl {
    font-size: 12px;
    font-weight: 640;
    color: var(--faint);
  }
  .st-done .sl,
  .st-current .sl,
  .st-stuck .sl {
    color: var(--text);
  }
  .st .sm {
    font-size: 10px;
    color: var(--muted);
    text-align: center;
    line-height: 1.4;
  }

  .tk-branch {
    font-size: 12px;
    color: var(--state-stuck);
    margin: 0 0 14px;
  }
  .tk-branch b {
    text-transform: lowercase;
  }
  .tk-resolution {
    font-size: 12px;
    color: var(--c);
    margin: 0 0 14px;
  }

  .tk-meta {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 9px 15px;
    padding: 13px 0;
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
    margin-bottom: 14px;
  }
  .tk-meta .mrow {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 12px;
  }
  .tk-meta .mk {
    font: 600 9.5px/1 var(--mono);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--muted);
    width: 68px;
    flex: 0 0 auto;
  }
  .tk-meta .mv {
    color: var(--fg-dim, var(--muted));
    min-width: 0;
  }
  .tk-meta .refs {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .tk-meta .refchip {
    font-size: 10px;
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    border-radius: 4px;
    padding: 1px 5px;
    background: transparent;
    cursor: pointer;
  }
  .tk-meta .refchip:hover {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .tk-meta .mv-none {
    color: var(--faint);
  }

  .tk-excerpt {
    font-size: 13px;
    color: var(--muted);
    line-height: 1.6;
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  .tk-foot {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 11px 16px;
    border-top: 1px solid var(--border);
    background: var(--panel-2);
    font: 500 11px/1 var(--mono);
    color: var(--muted);
    flex-wrap: wrap;
  }
  .tk-foot .fbtn {
    color: var(--fg-dim, var(--muted));
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 4px 9px;
    background: transparent;
    cursor: pointer;
    font: inherit;
  }
  .tk-foot .fbtn:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .tk-foot .fbtn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .tk-foot .kk {
    color: var(--fg-dim, var(--muted));
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 4px;
  }
  .tk-foot .tk-nav {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .tk-foot .tk-navbtn {
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 1px 6px;
    background: transparent;
    cursor: pointer;
    font: inherit;
  }
  .tk-foot .tk-navbtn:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .tk-foot .tk-navbtn:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .tk-head :global(.tk-copy-id),
  .tk-foot :global(.tk-copy-id) {
    opacity: 1;
  }
</style>
