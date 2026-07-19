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
  // Trust-tier framing + per-hub sync freshness (design/48 §2-3, `Hub.tier`/
  // `Hub.sync`, shipped 2026-07-18 — previously a noted "Backend gap", now
  // resolved) drive the domain cards' frame + sync line below. Both are
  // honest-nullable at the source (server-side `null` = "don't know", never
  // a fabricated calm value — src/api.rs's `hub_json`), and this component
  // keeps that contract: `tier: null` renders NEUTRAL (not "home" —
  // unclassified isn't the same as trusted), and ANY null inside `sync`
  // renders as an explicit "unknown"/warn state, never a calm all-clear
  // (redesign law #3, ui/REDESIGN.md).
  import { onDestroy, onMount } from 'svelte';
  import { getAttention } from '../api';
  import { bySeverityThenAge } from '../attention';
  import type { Attention, AttentionItem, DomainWorkItem, HubDomain, Severity } from '../attention';
  import type { HubTier } from '../types';
  import { formatAgeFromSecs, shortCode } from '../format';
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

  /** `own`/`shared` are both YOUR fleet (the mockup's "home" framing —
   * co-owned is still a hub you set up, not an outside party); `foreign` is
   * someone else's invite. `null` (never classified) is deliberately its
   * OWN bucket, not folded into either — an unclassified hub hasn't earned
   * the "home" solid frame just because it isn't explicitly foreign. */
  function tierFrame(tier: HubTier | null): 'home' | 'foreign' | 'neutral' {
    if (tier === 'own' || tier === 'shared') return 'home';
    if (tier === 'foreign') return 'foreign';
    return 'neutral';
  }

  function tierLabel(tier: HubTier | null): string {
    return tier ?? 'unclassified';
  }

  interface SyncState {
    text: string;
    /** Elevated (amber) styling — a genuinely bad or wholly-missing signal:
     * unreachable, behind, or the freshness/reachability facts needed to
     * call a hub "current" are missing outright. */
    warn: boolean;
  }

  /** Per-hub sync freshness, rendered field-by-field so one unknown piece
   * doesn't blank out the pieces that ARE known. Every null is labeled
   * explicitly ("… unknown") rather than silently dropped or defaulted —
   * law #3 — but not every null carries the same weight: missing
   * freshness/reachability/behind-count undermines the whole point of this
   * line (`warn`), while a null local-only `pending` is shown plainly
   * without the alarm styling. A fully-absent `sync` (hub never probed) is
   * the loudest case: "sync unknown", nothing else to show. */
  function syncState(sync: HubDomain['sync']): SyncState {
    if (!sync) return { text: 'sync unknown', warn: true };
    const parts: string[] = [];
    let warn = false;

    if (sync.lastFetchedSecs !== null) parts.push(`synced ${formatAgeFromSecs(sync.lastFetchedSecs)} ago`);
    else {
      parts.push('fetch time unknown');
      warn = true;
    }

    if (sync.reachable === false) {
      parts.push('unreachable');
      warn = true;
    } else if (sync.reachable === null) {
      parts.push('reachability unknown');
      warn = true;
    }

    if (sync.behind === null) {
      parts.push('behind-count unknown');
      warn = true;
    } else if (sync.behind > 0) {
      parts.push(`${sync.behind} behind`);
      warn = true;
    }

    if (sync.pending === null) parts.push('pending unknown');
    else if (sync.pending > 0) parts.push(`${sync.pending} pending`);

    return { text: parts.join(' · '), warn };
  }
</script>

<div class="ov-wrap" data-testid="overview-view">
  <div class="ov-band">
    <div class="ov-band-left">
      <div class="ov-brand">
        <span class="ov-mark">c</span>
        <h2>confer · fleet</h2>
      </div>
      <!-- The DASHBOARD's own poll age — a different fact from each domain
           card's per-hub git-sync freshness below (that's the real signal
           for "is this hub's picture current"; this is just "did the
           browser last ask recently"). Both are real, neither substitutes
           for the other. -->
      <span class="ov-refresh" data-testid="ov-updated">
        {#if loading && !attention}
          loading…
        {:else if updatedAgo}
          dashboard polled {updatedAgo} ago
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
        <section
          class="ov-domain ov-domain-{tierFrame(domain.tier)}"
          aria-label={`hub ${domain.label} — ${tierLabel(domain.tier)}`}
          data-testid="ov-domain"
        >
          <div class="ov-dhead">
            <div class="ov-dtitle">
              <span class="ov-tier ov-tier-{tierFrame(domain.tier)}" data-testid="ov-tier">{tierLabel(domain.tier)}</span>
              <button type="button" class="ov-dname" onclick={() => onDrillHub?.(domain.hub)}>{domain.label}</button>
              <span class="ov-dmeta">{domain.agents.length} agent{domain.agents.length === 1 ? '' : 's'}</span>
            </div>
            <span class="ov-dsync" class:ov-dsync-warn={syncState(domain.sync).warn} data-testid="ov-dsync">
              <span class="ov-dsync-pip"></span>{syncState(domain.sync).text}
            </span>
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
              <div class="ov-worklist">
                {#each domain.workInFlight as w (w.id)}
                  <button type="button" class="ov-work ov-work-{workClass(w)}" onclick={() => onDrillRequest?.(domain.hub, w.id)}>
                    <span class="ov-wdot" aria-hidden="true"></span>
                    <span class="ov-wid">{shortCode(w.id)}</span>
                    <span class="ov-wsummary">{w.summary}</span>
                    <span class="ov-wmeta">{claimantLabel(w)}</span>
                  </button>
                {/each}
              </div>
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
  /* Trust-domain framing (design/48, real `Hub.tier`) — own/shared reads as
     YOUR fleet (solid home frame + faint glow); foreign reads as someone
     else's hub (dashed frame); unclassified stays visually neutral — see
     tierFrame()'s doc comment for why "unknown" isn't folded into "home". */
  /* A soft glow that radiates from the top-left (behind the hub's identity in
     the header) and fades to nothing well before the work-in-flight footer —
     so no content ever sits on a hard color band. The trust signal is carried
     by the solid/dashed frame + tier chip; the glow is just gentle ambiance. */
  .ov-domain-home {
    border: 1.5px solid var(--home-frame);
    background:
      radial-gradient(135% 115% at 6% -14%, var(--home-glow), transparent 56%),
      var(--panel);
  }
  .ov-domain-foreign {
    border: 1.5px dashed var(--foreign-frame);
    background:
      radial-gradient(135% 115% at 6% -14%, var(--foreign-glow), transparent 56%),
      var(--panel);
  }
  .ov-dhead {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 12px;
    flex-wrap: wrap;
  }
  .ov-dtitle {
    display: flex;
    align-items: center;
    gap: 9px;
    flex-wrap: wrap;
  }
  .ov-tier {
    font: 700 9.5px/1 var(--mono);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding: 3px 7px;
    border-radius: 5px;
    border: 1px solid var(--border-2);
    color: var(--faint);
  }
  .ov-tier-home {
    color: var(--home-frame);
    background: color-mix(in srgb, var(--home-frame) 14%, transparent);
    border-color: color-mix(in srgb, var(--home-frame) 40%, transparent);
  }
  .ov-tier-foreign {
    color: var(--foreign-frame);
    background: color-mix(in srgb, var(--foreign-frame) 14%, transparent);
    border-color: color-mix(in srgb, var(--foreign-frame) 40%, transparent);
  }
  .ov-dsync {
    font-size: 10.5px;
    color: var(--faint);
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .ov-dsync-pip {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--done);
    flex: 0 0 auto;
  }
  .ov-dsync-warn {
    color: var(--blocked);
  }
  .ov-dsync-warn .ov-dsync-pip {
    background: var(--blocked);
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
    margin-top: 13px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }
  .ov-flab {
    display: block;
    font: 700 9.5px/1 var(--mono);
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--faint);
    margin-bottom: 6px;
  }
  /* Work-in-flight as a compact work-item list: one grid row each, so the
     right-hand meta (claimant / age) lines up in a column across all rows;
     status reads from a color dot (open=grey, claimed=cyan, stuck=amber),
     not a glyph. Rows are borderless until hover — quiet at rest. */
  .ov-worklist {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .ov-work {
    display: grid;
    grid-template-columns: auto auto 1fr auto;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 3px 8px;
    border-radius: 5px;
    border: 1px solid transparent;
    background: transparent;
    font: 500 11px/1.4 var(--mono);
    color: var(--fg-dim);
    text-align: left;
    cursor: pointer;
    transition:
      background 0.12s ease,
      border-color 0.12s ease;
  }
  .ov-work:hover {
    background: var(--panel-2);
    border-color: var(--border-2);
  }
  .ov-work:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .ov-wdot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--muted-2);
  }
  .ov-wid {
    color: var(--faint);
    font-size: 10px;
    letter-spacing: 0.02em;
  }
  .ov-wsummary {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--fg-dim);
    min-width: 0;
  }
  .ov-wmeta {
    justify-self: end;
    white-space: nowrap;
    font-size: 10px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .ov-work-wip .ov-wdot {
    background: var(--claimed);
  }
  .ov-work-wip .ov-wmeta {
    color: var(--claimed);
  }
  .ov-work-stuck .ov-wdot {
    background: var(--blocked);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--blocked) 16%, transparent);
  }
  .ov-work-stuck .ov-wmeta {
    color: var(--blocked);
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
