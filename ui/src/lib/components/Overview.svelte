<script lang="ts">
  // design/47 — the cross-hub Overview/Health triage view, and the
  // dashboard's default landing. Redesigned 2026-07-18 (ui/REDESIGN.md piece
  // 1) around the fleet map: "the fleet is a place, not a list" — hubs are
  // domain cards you navigate by memory (position=identity), agents inside
  // them carry their live/down/trust/WIP state in the appearance channel
  // (appearance=state), and the ranked "needs you" overlay rides on top,
  // anchored back to the map instead of replacing it. Reference:
  // ui/redesign-mockups/01-overview.html.
  //
  // Still Phase 1 (design/47 §5): fans out client-side (getAttention() in
  // api.ts does getHubs() + getOverview() per hub, folded by attention.ts's
  // aggregateAttention) — no new backend endpoint yet. Phase 2 repoints
  // getAttention() at a real `/api/attention`; this component only depends
  // on the Attention shape (now including the `domains` projection the map
  // renders from), so that swap needs no changes here.
  //
  // What this view does NOT render, and why (ui/REDESIGN.md law #3 — never
  // fabricate state): the mockup's home/foreign trust-tier framing and
  // per-hub "synced Ns ago" freshness both need signals the web API doesn't
  // project yet (`confer trust` is LOCAL-only, see src/tiers.rs; there's no
  // per-hub sync timestamp at all). Domain cards render with one neutral
  // frame, and the masthead's age line is honestly labeled as the
  // DASHBOARD's own poll age, not a hub sync fact. See "Backend gaps" in
  // ui/REDESIGN.md.
  import { onDestroy, onMount } from 'svelte';
  import { getAttention } from '../api';
  import { bySeverityThenAge } from '../attention';
  import type { Attention, AttentionItem, DomainWorkItem, Severity } from '../attention';
  import { formatAgeFromSecs } from '../format';
  import AgentNode from './AgentNode.svelte';
  import CopyIdButton from './CopyIdButton.svelte';
  import EmptyState from './EmptyState.svelte';
  import Skeleton from './Skeleton.svelte';

  interface Props {
    /** A domain card's "work in flight" chip or an overlay row's "open
     * thread ›" — drills into that hub's Board, focused on the request
     * (design/47 §2.6). */
    onDrillRequest?: (hub: string, reqId: string) => void;
    /** An agent node's click — switches to that hub's Fleet view (per-agent
     * anchor is a later phase, design/41 Ph.3). */
    onDrillFleet?: (hub: string, agentId: string) => void;
    /** A domain card's name — drills into that hub's Board. */
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

  // The mockup's single anchored "needs you" overlay — Lane 1 (agent
  // integrity + liveness) and Lane 2 (request lifecycle) merged into one
  // ranked list, each item still carrying its own hub/verb/target so the
  // "who do I talk to" rule (design/47 §2.3) survives the merge.
  const overlay = $derived([...(attention?.needsYou ?? []), ...(attention?.coordination ?? [])].sort(bySeverityThenAge));
  const criticalCount = $derived(overlay.filter((i) => i.severity === 'critical').length);
  const allClear = $derived(attention !== null && overlay.length === 0);

  // Real per-hub-occurrence down count, straight off the domain map — an
  // agent appearing on two hubs and down on one still counts once per
  // occurrence here (matches HubRollup's own per-occurrence counting).
  const agentsDownCount = $derived(attention?.domains.flatMap((d) => d.agents).filter((a) => a.liveness === 'down').length ?? 0);
  // Deduped identity count (Lane 3's existing fold) — "N agents" in the
  // masthead means N distinct signing identities, not N hub-occurrences.
  const totalAgents = $derived(attention?.fleet.length ?? 0);
  const totalHubs = $derived(attention?.domains.length ?? 0);
  const openCount = $derived(attention?.domains.flatMap((d) => d.workInFlight).filter((w) => w.status === 'OPEN').length ?? 0);
  const claimedCount = $derived(attention?.domains.flatMap((d) => d.workInFlight).filter((w) => w.status === 'CLAIMED').length ?? 0);

  const updatedAgo = $derived.by(() => {
    if (!lastUpdatedMs) return null;
    return formatAgeFromSecs(Math.max(0, (nowMs - lastUpdatedMs) / 1000));
  });

  function ageLabel(item: AttentionItem): string {
    return item.ageSecs === null ? '' : formatAgeFromSecs(item.ageSecs);
  }

  /** A work-in-flight chip's visual state: claimed reads as in-flight (cyan),
   * stale/blocked reads as stuck (amber), otherwise plain open. */
  function workClass(w: DomainWorkItem): string {
    if (w.stale || w.status === 'BLOCKED') return 'stuck';
    if (w.status === 'CLAIMED') return 'wip';
    return 'plain';
  }

  function claimantLabel(w: DomainWorkItem): string {
    if (w.claimants.length > 0) return w.claimants.join(', ');
    return `unclaimed ${formatAgeFromSecs(w.ageSecs)}`;
  }
</script>

<div class="ov-wrap" data-testid="overview-view">
  <div class="ov-band">
    <div class="ov-band-left">
      <div class="ov-brand">
        <span class="ov-mark">c</span>
        <h2>confer · fleet</h2>
      </div>
      <!-- Honest label: this is the DASHBOARD's own poll age, not a
           per-hub git-sync fact (that signal doesn't exist yet — see the
           file-top comment / ui/REDESIGN.md Backend gaps). -->
      <span class="ov-refresh" data-testid="ov-updated">
        {#if loading && !attention}
          loading…
        {:else if updatedAgo}
          dashboard refreshed {updatedAgo} ago
        {/if}
      </span>
    </div>
    {#if attention}
      <div class="ov-verdict" data-testid="ov-verdict">
        {#if allClear}
          <div class="ov-vhead ov-ok">✓ Steady</div>
        {:else}
          <div class="ov-vhead ov-warn">{criticalCount > 0 ? '‼' : '▲'} {overlay.length} thing{overlay.length === 1 ? '' : 's'} need{overlay.length === 1 ? 's' : ''} you</div>
        {/if}
        {#if agentsDownCount > 0}
          <div class="ov-vsub"><b class="ov-vcrit">{agentsDownCount} agent{agentsDownCount === 1 ? '' : 's'} down</b></div>
        {/if}
        <div class="ov-vmeta mono">
          {totalAgents} agent{totalAgents === 1 ? '' : 's'} · {totalHubs} hub{totalHubs === 1 ? '' : 's'} · {openCount} open · {claimedCount} in progress
        </div>
      </div>
    {/if}
  </div>

  {#if loading && !attention}
    <Skeleton rows={5} />
  {:else if error && !attention}
    <EmptyState glyph="⚠" title="Overview unavailable" body={error} actionLabel="Retry" onAction={() => void load()} />
  {:else if attention}
    <div class="ov-map" data-testid="ov-map">
      {#each attention.domains as domain (domain.hub)}
        <section class="ov-domain" aria-label={`hub ${domain.label}`} data-testid="ov-domain">
          <div class="ov-dhead">
            <button type="button" class="ov-dname" onclick={() => onDrillHub?.(domain.hub)}>{domain.label}</button>
            <span class="ov-dmeta">{domain.agents.length} agent{domain.agents.length === 1 ? '' : 's'}</span>
          </div>
          {#if domain.agents.length === 0}
            <div class="ov-dclear">no agents seen yet</div>
          {:else}
            <div class="ov-agents">
              {#each domain.agents as agent (agent.id)}
                <AgentNode {agent} onOpen={(id) => onDrillFleet?.(domain.hub, id)} />
              {/each}
            </div>
          {/if}
          <div class="ov-flight">
            <span class="ov-flab">work in flight</span>
            {#if domain.workInFlight.length === 0}
              <span class="ov-quiet">quiet — no open requests</span>
            {:else}
              {#each domain.workInFlight as w (w.id)}
                <button type="button" class="ov-work ov-work-{workClass(w)}" onclick={() => onDrillRequest?.(domain.hub, w.id)}>
                  <span class="ov-wid">{w.id.replace(/^req_/, '').slice(0, 6)}</span>
                  <span class="ov-wsummary">{w.summary}</span>
                  <span class="ov-wclaim">· {claimantLabel(w)}</span>
                </button>
              {/each}
            {/if}
          </div>
        </section>
      {/each}
    </div>

    <section class="ov-attn" data-testid="ov-attention">
      <div class="ov-ahead">
        <span class="ov-alab">needs you</span>
        <span class="ov-an">{overlay.length}</span>
        <span class="ov-ahint">↑ each item points back to where it lives on the map</span>
      </div>
      {#if overlay.length === 0}
        <div class="ov-clear" data-testid="attention-clear">✓ nothing needs you</div>
      {:else}
        <div class="ov-arows">
          {#each overlay as item (item.id)}
            {@render attentionRow(item)}
          {/each}
        </div>
      {/if}
    </section>

    <section class="ov-strip" data-testid="ov-context-strip">
      <span class="ov-strip-item"><b>{openCount + claimedCount}</b> open request{openCount + claimedCount === 1 ? '' : 's'}</span>
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

{#snippet attentionRow(item: AttentionItem)}
  <div class="ov-arow ov-sev-{item.severity}" data-testid="ov-attention-row">
    <span class="ov-asev">{SEVERITY_GLYPH[item.severity]}</span>
    <div class="ov-abody">
      <div class="ov-averb">{item.verb}</div>
      <div class="ov-actx">{item.summary}</div>
    </div>
    {#if item.ageSecs !== null}
      <span class="ov-aage">{ageLabel(item)}</span>
    {/if}
    <div class="ov-aact">
      {#if item.target}
        <CopyIdButton id={item.target} class="ov-acopy" />
      {/if}
      {#if item.reqId}
        <button type="button" class="ov-aopen" onclick={() => onDrillRequest?.(item.hub, item.reqId!)}>open thread ›</button>
      {/if}
    </div>
  </div>
{/snippet}

<style>
  .ov-wrap {
    overflow: auto;
    flex: 1;
    padding: var(--phi2) var(--phi2) var(--phi4);
  }

  /* ── honesty band — verdict + (honestly-labeled) poll age ── */
  .ov-band {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--phi2);
    padding-bottom: var(--phi1);
    border-bottom: 1px dashed var(--border);
    margin-bottom: var(--phi2);
    flex-wrap: wrap;
  }
  .ov-brand {
    display: flex;
    align-items: center;
    gap: 9px;
  }
  .ov-mark {
    width: 26px;
    height: 26px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    background: linear-gradient(150deg, var(--accent), var(--claimed));
    color: #0b0e17;
    font-weight: 800;
    font-family: var(--mono);
    font-size: 14px;
  }
  .ov-brand h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 650;
  }
  .ov-refresh {
    display: block;
    margin-top: 6px;
    font: 500 11px/1 var(--mono);
    color: var(--faint);
  }
  .ov-verdict {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 3px;
    text-align: right;
  }
  .ov-vhead {
    font-size: 13.5px;
    font-weight: 650;
  }
  .ov-vhead.ov-ok {
    color: var(--done);
  }
  .ov-vhead.ov-warn {
    color: var(--blocked);
  }
  .ov-vsub {
    font-size: 11.5px;
    color: var(--muted);
  }
  .ov-vcrit {
    color: var(--error);
    font-weight: 650;
  }
  .ov-vmeta {
    font-size: 10.5px;
    color: var(--faint);
  }

  /* ── the fleet map ── */
  .ov-map {
    display: flex;
    flex-direction: column;
    gap: var(--phi2);
  }
  .ov-domain {
    border-radius: var(--radius);
    padding: var(--phi1) var(--phi1) 14px;
    background: var(--panel);
    border: 1.5px solid var(--border-2);
  }
  .ov-dhead {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }
  .ov-dname {
    font-family: var(--mono);
    font-size: 13px;
    color: var(--text);
    font-weight: 650;
    background: none;
    border: 0;
    padding: 0;
  }
  .ov-dname:hover {
    color: var(--accent);
    text-decoration: underline;
  }
  .ov-dname:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .ov-dmeta {
    font-size: 11px;
    color: var(--faint);
  }
  .ov-dclear {
    font-size: 12px;
    color: var(--faint);
    font-style: italic;
  }
  .ov-agents {
    display: flex;
    flex-wrap: wrap;
    gap: var(--phi1);
  }
  .ov-flight {
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    align-items: center;
  }
  .ov-flab {
    font: 700 10px/1 var(--mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--faint);
    margin-right: 3px;
  }
  .ov-work {
    font: 500 11.5px/1 var(--mono);
    padding: 5px 9px;
    border-radius: 6px;
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--muted);
    display: inline-flex;
    gap: 6px;
    align-items: center;
    max-width: 100%;
  }
  .ov-work:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .ov-wid {
    color: var(--faint);
  }
  .ov-wsummary {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ov-work-wip {
    border-color: color-mix(in srgb, var(--claimed) 45%, transparent);
    color: var(--claimed);
  }
  .ov-work-stuck {
    border-color: color-mix(in srgb, var(--blocked) 50%, transparent);
    color: var(--blocked);
    background: color-mix(in srgb, var(--blocked) 9%, transparent);
  }
  .ov-work-stuck::before {
    content: '▲';
  }
  .ov-quiet {
    font-size: 11.5px;
    color: var(--faint);
    font-style: italic;
  }

  /* ── attention overlay — anchored to the map above ── */
  .ov-attn {
    margin-top: var(--phi2);
    border-top: 1px dashed var(--border);
    padding-top: var(--phi2);
  }
  .ov-ahead {
    display: flex;
    align-items: baseline;
    gap: 9px;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }
  .ov-alab {
    font: 700 11px/1 var(--mono);
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .ov-an {
    font: 700 11px/1 var(--mono);
    color: var(--error);
  }
  .ov-ahint {
    font-size: 11px;
    color: var(--faint);
    margin-left: auto;
  }
  .ov-clear {
    font-size: 12.5px;
    color: var(--faint);
    font-style: italic;
    padding: 4px 2px;
  }
  .ov-arows {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .ov-arow {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--panel);
    border: 1px solid var(--border);
    border-left: 3px solid var(--border);
  }
  .ov-sev-critical {
    border-left-color: var(--error);
  }
  .ov-sev-critical .ov-asev {
    color: var(--error);
  }
  .ov-sev-attention {
    border-left-color: var(--blocked);
  }
  .ov-sev-attention .ov-asev {
    color: var(--blocked);
  }
  .ov-sev-info {
    border-left-color: var(--deferred);
  }
  .ov-sev-info .ov-asev {
    color: var(--deferred);
  }
  .ov-asev {
    font-family: var(--mono);
    font-weight: 700;
    flex: 0 0 auto;
    width: 16px;
    text-align: center;
  }
  .ov-abody {
    min-width: 0;
    flex: 1;
  }
  .ov-averb {
    font-size: 12.5px;
    font-weight: 650;
    color: var(--text);
  }
  .ov-actx {
    font-size: 11.5px;
    color: var(--muted);
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ov-aage {
    flex: 0 0 auto;
    font: 600 10.5px/1 var(--mono);
    color: var(--faint);
  }
  .ov-aact {
    display: flex;
    gap: 6px;
    flex: 0 0 auto;
    align-items: center;
  }
  .ov-aact :global(.ov-acopy) {
    opacity: 1;
  }
  .ov-aopen {
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--muted);
    font: 600 11px/1 var(--sans);
    padding: 5px 9px;
    border-radius: 6px;
  }
  .ov-aopen:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .ov-aopen:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  /* ── ambient per-hub strip ── */
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
  .ov-strip-hub:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .ov-strip-live {
    color: var(--done);
  }
  .ov-strip-attn {
    color: var(--blocked);
  }

  @media (max-width: 767.98px) {
    .ov-band {
      flex-direction: column;
    }
    .ov-verdict {
      align-items: flex-start;
      text-align: left;
    }
    .ov-arow {
      flex-wrap: wrap;
    }
  }
</style>
