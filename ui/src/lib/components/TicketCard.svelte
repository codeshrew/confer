<script lang="ts">
  import type { RequestRow, RequestStatus } from '../types';
  import { formatAgeFromSecs } from '../format';

  interface Props {
    request: RequestRow;
    selected?: boolean;
    onSelect?: (id: string) => void;
  }

  let { request, selected = false, onSelect }: Props = $props();

  type StatusKey = 'open' | 'claimed' | 'blocked' | 'done' | 'error';

  function statusKey(status: RequestStatus): StatusKey {
    switch (status) {
      case 'CLAIMED':
        return 'claimed';
      case 'BLOCKED':
        return 'blocked';
      case 'DONE':
        return 'done';
      case 'ERROR':
        return 'error';
      default:
        return 'open';
    }
  }

  const key = $derived(statusKey(request.status));
  const isBacklog = $derived(request.deferred && request.status === 'OPEN');
  const stamp = $derived(isBacklog ? 'backlog' : key);
  const serial = $derived(request.id.slice(-3).toUpperCase());

  function cap(s: string): string {
    return s.length ? s[0]!.toUpperCase() + s.slice(1) : s;
  }

  const addressee = $derived(request.to.length ? request.to.map((t) => `@${cap(t)}`).join(', ') : '@all');
  const ageLabel = $derived(formatAgeFromSecs(request.ageSecs));

  // Lifecycle track: filed is always reached; later stages are cumulative
  // from current status. RequestRow only carries the *current* status (no
  // history), so a stage earlier in the pipeline than "blocked" that has
  // since resolved won't re-light "blocked" — this is a known projection
  // limit, not a bug.
  type StageState = 'pending' | 'done' | 'cur';
  const stages = $derived.by((): { label: string; state: StageState }[] => {
    const s = request.status;
    const claimed = request.claimants.length > 0 || s === 'CLAIMED' || s === 'BLOCKED' || s === 'DONE';
    const filed: StageState = 'done';
    const claim: StageState = claimed ? (s === 'CLAIMED' ? 'cur' : 'done') : s === 'OPEN' ? 'pending' : 'done';
    const blocked: StageState = s === 'BLOCKED' ? 'cur' : 'pending';
    const done: StageState = s === 'DONE' ? 'done' : s === 'ERROR' ? 'cur' : 'pending';
    return [
      { label: 'filed', state: filed },
      { label: 'claim', state: claim },
      { label: 'blocked', state: blocked },
      { label: 'done', state: done },
    ];
  });
</script>

<div
  class="ticket s-{key}"
  class:sel={selected}
  role="button"
  tabindex="0"
  onclick={() => onSelect?.(request.id)}
  onkeydown={(e) => {
    if (e.key === 'Enter' || e.key === ' ') onSelect?.(request.id);
  }}
>
  <div class="stub">
    <div class="kick">Req</div>
    <div class="serial">{serial}</div>
    <div class="stamp">{stamp}</div>
  </div>
  <div class="tmain">
    <div class="ttitle">{request.summary}</div>
    <div class="troute">
      <span class="mono">→ {addressee}</span> · <span class="mono">{request.id}</span> · filed {ageLabel} ago{#if request.resolution}
        · {request.resolution}{/if}
    </div>
    <div class="track">
      {#each stages as stage, i (stage.label)}
        <div class="node">
          <span class="n" class:done={stage.state === 'done'} class:cur={stage.state === 'cur'}></span>
          <span class="lbl" class:on={stage.state !== 'pending'}>{stage.label}</span>
        </div>
        {#if i < stages.length - 1}
          <span class="link" class:done={stage.state === 'done'}></span>
        {/if}
      {/each}
    </div>
  </div>
</div>

<style>
  .ticket {
    display: flex;
    align-items: stretch;
    position: relative;
    margin: 4px 0 3px;
    cursor: pointer;
    border-radius: 11px;
    overflow: hidden;
    background: var(--panel);
    box-shadow: 0 1px 1px rgba(0, 0, 0, 0.22), 0 12px 26px -14px rgba(0, 0, 0, 0.55);
    --stubw: 84px;
    --tk: var(--done);
    transition: box-shadow 0.12s ease;
  }
  .ticket:hover {
    box-shadow: 0 1px 1px rgba(0, 0, 0, 0.22), 0 16px 32px -14px rgba(0, 0, 0, 0.65);
  }
  .ticket.sel {
    box-shadow: 0 0 0 1.5px var(--tk), 0 14px 30px -14px rgba(0, 0, 0, 0.6);
  }
  .ticket.s-open {
    --tk: var(--open);
  }
  .ticket.s-claimed {
    --tk: var(--claimed);
  }
  .ticket.s-blocked {
    --tk: var(--blocked);
  }
  .ticket.s-done {
    --tk: var(--done);
  }
  .ticket.s-error {
    --tk: var(--error);
  }
  .ticket::before,
  .ticket::after {
    content: '';
    position: absolute;
    left: var(--stubw);
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--bg);
    transform: translate(-50%, -50%);
    z-index: 3;
  }
  .ticket::before {
    top: 0;
  }
  .ticket::after {
    top: 100%;
  }
  .ticket .stub {
    width: var(--stubw);
    flex: 0 0 auto;
    padding: 11px 11px 10px;
    display: flex;
    flex-direction: column;
    background: color-mix(in srgb, var(--tk) 15%, var(--panel));
    border-right: 2px dashed color-mix(in srgb, var(--tk) 50%, var(--border-2));
  }
  .ticket .stub .kick {
    font: 800 9px/1 var(--mono);
    letter-spacing: 0.15em;
    color: var(--tk);
    text-transform: uppercase;
  }
  .ticket .stub .serial {
    font: 700 17px/1 var(--mono);
    color: var(--text);
    letter-spacing: 0.02em;
    margin-top: 6px;
  }
  .ticket .stub .stamp {
    margin-top: auto;
    align-self: flex-start;
    font: 800 8.5px/1 var(--mono);
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--tk);
    border: 1.5px solid var(--tk);
    border-radius: 4px;
    padding: 3px 5px;
    transform: rotate(-4.5deg);
    opacity: 0.92;
  }
  .ticket .tmain {
    flex: 1;
    min-width: 0;
    padding: 12px 15px 13px;
    display: flex;
    flex-direction: column;
    gap: 9px;
  }
  .ticket .ttitle {
    font-weight: 600;
    font-size: 13.5px;
    color: var(--text);
  }
  .ticket .troute {
    display: flex;
    align-items: center;
    gap: 7px;
    flex-wrap: wrap;
    color: var(--muted);
    font-size: 12px;
  }
  .ticket .troute .mono {
    font-family: var(--mono);
    color: var(--faint);
    font-size: 11.5px;
  }
  .track {
    display: flex;
    align-items: center;
    gap: 0;
    margin-top: 3px;
  }
  .track .node {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .track .n {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--border-2);
    flex: 0 0 auto;
  }
  .track .n.done {
    background: var(--done);
  }
  .track .n.cur {
    background: var(--claimed);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--claimed) 25%, transparent);
  }
  .track .lbl {
    font: 600 10px/1 var(--mono);
    color: var(--faint);
  }
  .track .lbl.on {
    color: var(--text);
  }
  .track .link {
    height: 1px;
    width: 26px;
    background: var(--border-2);
    margin: 0 8px;
  }
  .track .link.done {
    background: var(--done);
  }

  /* ── Phone (<768px): the stub (torn ticket stub with serial/stamp) is a
     tall 84px-wide left column at desktop widths — on a ~360-390px viewport
     that plus the unwrapped 4-stage lifecycle track forces the card wider
     than the screen. Below 768px the stub becomes a horizontal top banner
     and the track wraps, so nothing here ever needs more width than the
     viewport gives it. ── */
  @media (max-width: 767.98px) {
    .ticket {
      flex-direction: column;
    }
    .ticket::before,
    .ticket::after {
      display: none;
    }
    .ticket .stub {
      width: 100%;
      flex-direction: row;
      align-items: center;
      gap: 9px;
      padding: 8px 12px;
      border-right: 0;
      border-bottom: 2px dashed color-mix(in srgb, var(--tk) 50%, var(--border-2));
    }
    .ticket .stub .serial {
      margin-top: 0;
    }
    .ticket .stub .stamp {
      margin-top: 0;
      margin-left: auto;
      transform: none;
    }
    .ticket .tmain {
      padding: 11px 13px 12px;
    }
    .track {
      flex-wrap: wrap;
      row-gap: 8px;
    }
    .track .link {
      width: 14px;
      margin: 0 5px;
    }
  }
</style>
