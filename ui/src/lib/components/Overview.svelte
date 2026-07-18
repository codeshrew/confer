<script lang="ts">
  // design/47 — the cross-hub Overview/Health triage view, and (per §3) the
  // dashboard's new default landing. Answers exactly one question: "what
  // does the human need to do to intervene?" — masthead health headline,
  // then three ranked lanes (Needs-you / Coordination / Fleet-health), then
  // an ambient context strip. Every attention item names a target agent +
  // verb (the "who do I talk to" rule, §2.3) and carries a copy-agent-name
  // chip (CopyIdButton) so the human can paste the name straight into that
  // agent's own chat — this view is read-only by design; it never writes
  // back into confer (§0's interaction model).
  //
  // Phase 1 (design/47 §5): fans out client-side (getAttention() in api.ts
  // does getHubs() + getOverview() per hub, folded by attention.ts's
  // aggregateAttention) — no new backend endpoint yet. Phase 2 repoints
  // getAttention() at a real `/api/attention`; this component only depends
  // on the Attention shape, so that swap needs no changes here.
  import { onDestroy, onMount } from 'svelte';
  import { getAttention } from '../api';
  import type { Attention, AttentionItem, FleetCard, Severity } from '../attention';
  import type { Liveness, Trust } from '../types';
  import { formatAgeFromSecs } from '../format';
  import CopyIdButton from './CopyIdButton.svelte';
  import EmptyState from './EmptyState.svelte';
  import Skeleton from './Skeleton.svelte';

  interface Props {
    /** A Lane-2 card's "open thread ›" — drills into that hub's Board,
     * focused on the request (design/47 §2.6). */
    onDrillRequest?: (hub: string, reqId: string) => void;
    /** A Lane-3 fleet card's fix-hint / avatar click — switches to that
     * hub's Fleet view (per-agent anchor is a later phase, design/41 Ph.3). */
    onDrillFleet?: (hub: string, agentId: string) => void;
    /** The context strip's per-hub rollup — drills into that hub's Board. */
    onDrillHub?: (hub: string) => void;
  }

  let { onDrillRequest, onDrillFleet, onDrillHub }: Props = $props();

  let loading = $state(true);
  let error = $state<string | null>(null);
  let attention = $state<Attention | null>(null);
  let lastUpdatedMs = $state<number | null>(null);
  let nowMs = $state(Date.now());

  // Triage doesn't need sub-second latency (design/47 §4.2's "poll, don't
  // multiplex SSE" call, brought forward to the Phase-1 client fan-out too).
  const REFRESH_MS = 15000;
  let refreshTimer: ReturnType<typeof setInterval> | undefined;
  let clockTimer: ReturnType<typeof setInterval> | undefined;

  async function load() {
    try {
      const result = await getAttention();
      attention = result;
      lastUpdatedMs = Date.now();
      error = null;
    } catch (err) {
      console.error('confer serve: failed to load the cross-hub overview', err);
      error = 'Could not load the cross-hub overview — check that confer serve is reachable.';
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
    refreshTimer = setInterval(() => void load(), REFRESH_MS);
    clockTimer = setInterval(() => {
      nowMs = Date.now();
    }, 1000);
  });
  onDestroy(() => {
    clearInterval(refreshTimer);
    clearInterval(clockTimer);
  });

  const SEVERITY_GLYPH: Record<Severity, string> = { critical: '‼', attention: '▲', info: '◇', nominal: '✓' };
  const LIVENESS_LABEL: Record<Liveness, string> = { live: '⬤ live', stale: '◐ stale', down: '○ down' };
  const TRUST_LABEL: Record<Trust, string> = {
    signed: '✓ signed',
    mismatch: '‼ mismatch',
    'first-sight': '⚠ first-sight',
    unsigned: '⚠ unsigned',
  };

  const needsYouCount = $derived(attention?.needsYou.length ?? 0);
  const coordinationCount = $derived(attention?.coordination.length ?? 0);
  const criticalCount = $derived(
    (attention?.needsYou.filter((i) => i.severity === 'critical').length ?? 0) +
      (attention?.fleet.filter((c) => c.severity === 'critical').length ?? 0)
  );
  const attentionOnlyCount = $derived(attention?.coordination.filter((i) => i.severity === 'attention').length ?? 0);
  const allClear = $derived(attention !== null && needsYouCount === 0 && coordinationCount === 0);

  const updatedAgo = $derived.by(() => {
    if (!lastUpdatedMs) return null;
    return formatAgeFromSecs(Math.max(0, (nowMs - lastUpdatedMs) / 1000));
  });

  function ageLabel(item: AttentionItem): string {
    return item.ageSecs === null ? '' : formatAgeFromSecs(item.ageSecs);
  }
</script>

<div class="ov-wrap" data-testid="overview-view">
  <div class="ov-mast">
    <div class="ov-mast-top">
      <h2>confer · fleet overview</h2>
      {#if attention}
        <span class="ov-sub">{attention.metrics.perHub.length} hub{attention.metrics.perHub.length === 1 ? '' : 's'} · {attention.fleet.length} agent{attention.fleet.length === 1 ? '' : 's'}</span>
      {/if}
      <span class="ov-spacer"></span>
      <span class="ov-updated" data-testid="ov-updated">
        {#if loading && !attention}
          loading…
        {:else if updatedAgo}
          updated {updatedAgo} ago
        {/if}
      </span>
    </div>
    <div class="ov-headline">
      {#if attention}
        {#if criticalCount > 0}
          <span class="hl crit">‼ {criticalCount} needs you</span>
        {/if}
        {#if attentionOnlyCount > 0}
          <span class="hl att">▲ {attentionOnlyCount} stuck</span>
        {/if}
        {#if allClear}
          <span class="hl ok">✓ all clear</span>
        {/if}
      {/if}
    </div>
  </div>

  {#if loading && !attention}
    <Skeleton rows={5} />
  {:else if error && !attention}
    <EmptyState glyph="⚠" title="Overview unavailable" body={error} actionLabel="Retry" onAction={() => void load()} />
  {:else if attention}
    <section class="ov-lane" data-testid="lane-needs-you">
      <div class="ov-lane-head sev-critical">
        <span class="ov-lane-title">‼ NEEDS YOU</span>
        <span class="ov-lane-count">{attention.needsYou.length}</span>
      </div>
      {#if attention.needsYou.length === 0}
        <div class="ov-clear" data-testid="needs-you-clear">✓ nothing needs you</div>
      {:else}
        <div class="ov-cards">
          {#each attention.needsYou as item (item.id)}
            {@render attentionCard(item)}
          {/each}
        </div>
      {/if}
    </section>

    <section class="ov-lane" data-testid="lane-coordination">
      <div class="ov-lane-head sev-attention">
        <span class="ov-lane-title">▲ COORDINATION</span>
        <span class="ov-lane-count">{attention.coordination.length}</span>
      </div>
      {#if attention.coordination.length === 0}
        <div class="ov-clear" data-testid="coordination-clear">✓ nothing stuck or unowned</div>
      {:else}
        <div class="ov-cards">
          {#each attention.coordination as item (item.id)}
            {@render attentionCard(item)}
          {/each}
        </div>
      {/if}
    </section>

    <section class="ov-lane" data-testid="lane-fleet-health">
      <div class="ov-lane-head">
        <span class="ov-lane-title">◔ FLEET HEALTH</span>
        <span class="ov-lane-count">{attention.fleet.length} agent{attention.fleet.length === 1 ? '' : 's'}</span>
      </div>
      {#if attention.fleet.length === 0}
        <div class="ov-clear">no agents seen yet</div>
      {:else}
        <div class="ov-fleetgrid">
          {#each attention.fleet as card (card.id)}
            {@render fleetCardSnippet(card)}
          {/each}
        </div>
      {/if}
    </section>

    <section class="ov-strip" data-testid="ov-context-strip">
      <span class="ov-strip-item"><b>{attention.metrics.openRequests}</b> open request{attention.metrics.openRequests === 1 ? '' : 's'}</span>
      {#each attention.metrics.perHub as row (row.hub)}
        <button type="button" class="ov-strip-hub" onclick={() => onDrillHub?.(row.hub)}>
          <span class="ov-strip-hubname">{row.label}</span>
          <span class="ov-strip-live">●{row.live}</span>
          {#if row.attention > 0}
            <span class="ov-strip-attn">▲{row.attention}</span>
          {/if}
        </button>
      {/each}
    </section>
  {/if}
</div>

{#snippet attentionCard(item: AttentionItem)}
  <div class="ov-card sev-{item.severity}">
    <div class="ov-card-top">
      <span class="ov-glyph">{SEVERITY_GLYPH[item.severity]}</span>
      <span class="ov-card-summary">{item.summary}</span>
      {#if item.ageSecs !== null}
        <span class="ov-card-age">{ageLabel(item)}</span>
      {/if}
    </div>
    {#if item.detail}
      <div class="ov-card-detail">{item.detail}</div>
    {/if}
    <div class="ov-card-foot">
      <span class="ov-verb">→ {item.verb}</span>
      {#if item.target}
        <CopyIdButton id={item.target} class="ov-copy" />
      {/if}
      {#if item.reqId}
        <button type="button" class="ov-open" onclick={() => onDrillRequest?.(item.hub, item.reqId!)}>open thread ›</button>
      {/if}
    </div>
  </div>
{/snippet}

{#snippet fleetCardSnippet(card: FleetCard)}
  <div class="ov-agentcard sev-{card.severity}" class:nominal={card.severity === 'nominal'}>
    <div class="ov-ac-top">
      <span class="ov-ac-av" style="color:{card.color};background:color-mix(in srgb, {card.color} 18%, transparent)">{card.abbr}</span>
      <div class="ov-ac-id">
        <div class="ov-ac-nm">{card.display}</div>
        <div class="ov-ac-host">{card.host ?? '—'} · {card.hubs.join(', ')}</div>
      </div>
      <CopyIdButton id={card.id} class="ov-ac-copy" />
    </div>
    <div class="ov-ac-line">{LIVENESS_LABEL[card.liveness]}{card.hbAgeSecs !== null ? ` · ${formatAgeFromSecs(card.hbAgeSecs)}` : ''}</div>
    <div class="ov-ac-line">{TRUST_LABEL[card.trust]}</div>
    <div class="ov-ac-line ov-ac-wip">{card.wip} WIP</div>
    {#if card.fixVerb}
      <button type="button" class="ov-ac-fix" onclick={() => onDrillFleet?.(card.hubs[0]!, card.id)}>→ {card.fixVerb}</button>
    {/if}
  </div>
{/snippet}

<style>
  .ov-wrap {
    overflow: auto;
    flex: 1;
    padding: 16px 20px 28px;
  }
  .ov-mast {
    margin-bottom: 14px;
  }
  .ov-mast-top {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  .ov-mast-top h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
  }
  .ov-sub {
    font: 600 11px/1 var(--mono);
    color: var(--faint);
  }
  .ov-spacer {
    flex: 1;
  }
  .ov-updated {
    font: 500 11px/1 var(--mono);
    color: var(--faint);
  }
  .ov-headline {
    display: flex;
    gap: 10px;
    margin-top: 8px;
    flex-wrap: wrap;
  }
  .hl {
    font: 700 12px/1 var(--sans);
    padding: 5px 10px;
    border-radius: 7px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .hl.crit {
    color: var(--error);
    background: color-mix(in srgb, var(--error) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--error) 35%, transparent);
  }
  .hl.att {
    color: var(--blocked);
    background: color-mix(in srgb, var(--blocked) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--blocked) 35%, transparent);
  }
  .hl.ok {
    color: var(--done);
    background: color-mix(in srgb, var(--done) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--done) 30%, transparent);
  }

  .ov-lane {
    margin-top: 18px;
  }
  .ov-lane-head {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 8px;
    padding-bottom: 7px;
    border-bottom: 1px solid var(--border);
  }
  .ov-lane-title {
    font: 700 11px/1 var(--mono);
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .ov-lane-head.sev-critical .ov-lane-title {
    color: var(--error);
  }
  .ov-lane-head.sev-attention .ov-lane-title {
    color: var(--blocked);
  }
  .ov-lane-count {
    font: 600 10.5px/1 var(--mono);
    color: var(--faint);
  }
  /* The calm state must LOOK calm — an empty lane collapses to one quiet
     line, never a big empty card taking up vertical rhythm (design/47 §2.1). */
  .ov-clear {
    font-size: 12.5px;
    color: var(--faint);
    font-style: italic;
    padding: 4px 2px;
  }

  .ov-cards {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .ov-card {
    background: var(--panel);
    border: 1px solid var(--border);
    border-left: 3px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .ov-card.sev-critical {
    border-left-color: var(--error);
  }
  .ov-card.sev-attention {
    border-left-color: var(--blocked);
  }
  .ov-card.sev-info {
    border-left-color: var(--deferred);
  }
  .ov-card-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .ov-glyph {
    flex: 0 0 auto;
    font-size: 13px;
  }
  .sev-critical .ov-glyph {
    color: var(--error);
  }
  .sev-attention .ov-glyph {
    color: var(--blocked);
  }
  .sev-info .ov-glyph {
    color: var(--deferred);
  }
  .ov-card-summary {
    font-weight: 650;
    font-size: 12.5px;
    color: var(--text);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ov-card-age {
    margin-left: auto;
    flex: 0 0 auto;
    font: 600 10.5px/1 var(--mono);
    color: var(--faint);
  }
  .ov-card-detail {
    font-size: 11.5px;
    color: var(--muted);
  }
  .ov-card-foot {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .ov-verb {
    font: 600 11.5px/1 var(--sans);
    color: var(--accent);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ov-card-foot :global(.ov-copy) {
    opacity: 1;
  }
  .ov-open {
    margin-left: auto;
    flex: 0 0 auto;
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--muted);
    font: 600 11px/1 var(--sans);
    padding: 5px 9px;
    border-radius: 6px;
  }
  .ov-open:hover {
    color: var(--text);
    border-color: var(--accent);
  }

  .ov-fleetgrid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 12px;
  }
  .ov-agentcard {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 12px 13px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  /* Nominal cards are the COMMON case — dim them so the eye lands on what's
     wrong (design/47 §2.4: "only Critical and Attention items get chrome"). */
  .ov-agentcard.nominal {
    opacity: 0.72;
  }
  .ov-agentcard.sev-critical {
    border-color: color-mix(in srgb, var(--error) 45%, var(--border));
  }
  .ov-agentcard.sev-attention {
    border-color: color-mix(in srgb, var(--blocked) 40%, var(--border));
  }
  .ov-ac-top {
    display: flex;
    align-items: center;
    gap: 9px;
  }
  .ov-ac-av {
    width: 30px;
    height: 30px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    font: 700 12px/1 var(--mono);
    flex: 0 0 auto;
  }
  .ov-ac-id {
    min-width: 0;
    flex: 1;
  }
  .ov-ac-nm {
    font-weight: 650;
    font-size: 12.5px;
  }
  .ov-ac-host {
    font: 500 10px/1 var(--mono);
    color: var(--faint);
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ov-ac-line {
    font: 500 10.5px/1 var(--mono);
    color: var(--muted);
  }
  .sev-critical .ov-ac-line:first-of-type {
    color: var(--error);
  }
  .ov-ac-wip {
    color: var(--faint);
  }
  .ov-ac-fix {
    margin-top: 2px;
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--muted);
    font: 600 10.5px/1 var(--sans);
    padding: 5px 8px;
    border-radius: 6px;
    text-align: left;
  }
  .ov-ac-fix:hover {
    color: var(--text);
    border-color: var(--accent);
  }

  .ov-strip {
    margin-top: 22px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
  }
  .ov-strip-item {
    font: 500 11.5px/1 var(--sans);
    color: var(--muted);
  }
  .ov-strip-item b {
    color: var(--text);
  }
  .ov-strip-hub {
    display: flex;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--border);
    background: var(--panel-2);
    border-radius: 7px;
    padding: 4px 9px;
    font: 600 11px/1 var(--mono);
    color: var(--muted);
  }
  .ov-strip-hub:hover {
    color: var(--text);
    border-color: var(--border-2);
  }
  .ov-strip-live {
    color: var(--done);
  }
  .ov-strip-attn {
    color: var(--blocked);
  }

  @media (max-width: 767.98px) {
    .ov-fleetgrid {
      grid-template-columns: 1fr;
    }
  }
</style>
