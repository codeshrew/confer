<script lang="ts">
  // The Board — piece 5b (ui/REDESIGN.md, `redesign-mockups/05-board-cockpit.html`):
  // a triage COCKPIT, not a flat ticket list. Arriving here, a human asks
  // four things — is anything stuck, what needs an owner, who's carrying
  // what, are we closing faster than we open — and this page answers them
  // BEFORE showing a single ticket: header verdict → stat strip (click to
  // filter) → a thin composition bar → two real visuals (carrying/asking
  // load, closure throughput) → the actionable work grouped by state, with
  // DONE collapsed to one line instead of dominating the page the way the
  // old flat groupBy('status'|'topic'|'claimant') swimlanes did.
  //
  // All the math lives in boardStats.ts (pure, unit-tested) — this file is
  // template + the local state-filter/done-fold UI state piece 5b itself
  // owns. Piece 5c adds the Fleet-as-filter rail replacing the left column,
  // wires the workload bars as a second (agent) filter dimension, and
  // layers the combined active-filter chip row + clear-all on top of the
  // single-state filter this piece already ships.
  import type { Agent, HubTier, Message, RequestRow } from '../types';
  import { paneFocus } from '../paneFocus.svelte';
  import { computeAsking, computeBoardStats, computeCarrying, computeFlowBar, computeThroughput, summarizeThroughput, verdictParts } from '../boardStats';
  import { ticketStateOf, type TicketState } from '../ticketState';
  import TicketRow from './TicketRow.svelte';
  import EmptyState from './EmptyState.svelte';

  interface Props {
    requests: RequestRow[];
    agents: Agent[];
    /** Full, unpaginated per-hub messages — closure throughput buckets
     * real `request`/`done` message timestamps out of this (see
     * boardStats.ts's own header note: no backend gap, the close
     * timestamp already exists on the `done` message itself). */
    messages: Message[];
    hubName: string;
    hubTier?: HubTier | null;
    selectedRequestId?: string | null;
    onSelectRequest?: (id: string) => void;
  }

  let { requests, agents, messages, hubName, hubTier = null, selectedRequestId = null, onSelectRequest }: Props = $props();

  const agentsById = $derived(new Map(agents.map((a) => [a.id, a])));
  function display(agentId: string): string {
    const a = agentsById.get(agentId);
    if (a) return a.display;
    return agentId.length ? agentId[0]!.toUpperCase() + agentId.slice(1) : agentId;
  }
  function avatar(agentId: string): string {
    return agentsById.get(agentId)?.abbr ?? agentId.slice(0, 2).toUpperCase();
  }
  function avatarColor(agentId: string): string {
    return agentsById.get(agentId)?.color ?? 'var(--muted)';
  }

  const THROUGHPUT_DAYS = 10;
  const stats = $derived(computeBoardStats(requests));
  const flow = $derived(computeFlowBar(requests));
  const carrying = $derived(computeCarrying(agents));
  const asking = $derived(computeAsking(requests));
  const throughputDays = $derived(computeThroughput(messages, THROUGHPUT_DAYS));
  const throughputSummary = $derived(summarizeThroughput(throughputDays));
  const verdict = $derived(verdictParts(stats, throughputSummary));
  const verdictSegments = $derived([verdict.needsOwner, verdict.stuck, verdict.trend].filter((s): s is string => s !== null));

  const maxCarrying = $derived(Math.max(1, ...carrying.map((r) => r.count)));
  const maxAsking = $derived(Math.max(1, ...asking.map((r) => r.count)));
  const maxDayCount = $derived(Math.max(1, ...throughputDays.map((d) => Math.max(d.opened, d.closed))));

  let stateFilter = $state<TicketState | null>(null);
  let doneOpen = $state(false);

  function ticketsIn(state: TicketState): RequestRow[] {
    return requests.filter((r) => ticketStateOf(r) === state).sort((a, b) => b.ageSecs - a.ageSecs);
  }
  const needsOwnerList = $derived(ticketsIn('unowned'));
  const inFlightList = $derived(ticketsIn('flight'));
  const stuckList = $derived(ticketsIn('stuck'));
  const doneList = $derived(ticketsIn('done'));

  interface Group {
    key: TicketState;
    label: string;
    cls: string;
    glyph: string;
    items: RequestRow[];
  }
  const allGroups = $derived.by(
    (): Group[] => [
      { key: 'unowned', label: 'needs an owner', cls: 'attn', glyph: '▲', items: needsOwnerList },
      { key: 'flight', label: 'in flight', cls: 'flight', glyph: '●', items: inFlightList },
      { key: 'stuck', label: 'blocked / stale', cls: 'stuck', glyph: '▲', items: stuckList },
    ]
  );
  const groups = $derived(stateFilter ? allGroups.filter((g) => g.key === stateFilter) : allGroups);

  /** Clicking "Open" (the total-live stat, not a disjoint bucket) clears
   * whatever filter is active — matches its own "active work" meaning:
   * there's no narrower group it could drill to. */
  function toggleStat(state: TicketState | 'activeWork') {
    stateFilter = state === 'activeWork' ? null : stateFilter === state ? null : state;
  }

  function pct(count: number, max: number): number {
    return Math.min(100, (count / max) * 100);
  }

  const TIER_LABEL: Record<HubTier, string> = { own: 'home', shared: 'shared', foreign: 'foreign' };

  // keyboard-architecture pass — "board", one of the 7 named Layer-1
  // panes. Rows (TicketRow) and the stat/done-fold buttons are already
  // individually click/Tab-reachable — this registers the board root as
  // the Ctrl+hjkl landing spot only (unchanged from pre-cockpit Board).
  let boardEl: HTMLDivElement;
  $effect(() => {
    if (!boardEl) return;
    return paneFocus.register({
      id: 'board',
      label: 'Board',
      el: boardEl,
      getRect: () => boardEl.getBoundingClientRect(),
    });
  });
</script>

<div class="board-wrap" tabindex="-1" bind:this={boardEl} data-testid="board-view">
  {#if requests.length === 0}
    <EmptyState glyph="◇" title="Nothing on the board yet" body="Requests filed on this hub will show up here, triaged by state." />
  {:else}
    <div class="bhead">
      {#if hubTier}
        <span class="tier tier-{hubTier}">{TIER_LABEL[hubTier]}</span>
      {/if}
      <span class="hub mono">{hubName}</span>
      {#if verdictSegments.length}
        <span class="verdict">
          {#each verdictSegments as seg, i (i)}{#if i === 0}<b>{seg}</b>{:else}{seg}{/if}{#if i < verdictSegments.length - 1}<span class="vsep"> · </span>{/if}{/each}
        </span>
      {/if}
    </div>

    <div class="stats">
      <button type="button" class="stat s-open" class:on={stateFilter === null} onclick={() => toggleStat('activeWork')} data-testid="board-stat-open">
        <div class="n">{stats.activeWork}</div>
        <div class="k">Open</div>
        <div class="sub">active work</div>
      </button>
      <button type="button" class="stat s-flight" class:on={stateFilter === 'flight'} onclick={() => toggleStat('flight')} data-testid="board-stat-flight">
        <div class="n">{stats.inFlight}</div>
        <div class="k">In flight</div>
        <div class="sub">claimed · being worked</div>
      </button>
      <button type="button" class="stat s-stuck" class:on={stateFilter === 'stuck'} onclick={() => toggleStat('stuck')} data-testid="board-stat-stuck">
        <div class="n">{stats.stuck}</div>
        <div class="k">Stuck</div>
        <div class="sub">blocked or stale</div>
      </button>
      <button type="button" class="stat s-unowned" class:on={stateFilter === 'unowned'} onclick={() => toggleStat('unowned')} data-testid="board-stat-unowned">
        <div class="n">{stats.needsOwner}</div>
        <div class="k">Need an owner</div>
        <div class="sub">open · no claimant</div>
      </button>
    </div>

    {#if flow.length}
      <div class="flowbar" role="img" aria-label="status composition — {flow.map((s) => `${s.count} ${s.state}`).join(', ')}">
        {#each flow as seg (seg.state)}
          <span class="fseg" style="width:{seg.pct}%;background:var(--state-{seg.state})"></span>
        {/each}
      </div>
      <div class="flowlegend">
        {#each flow as seg (seg.state)}
          <span class="lg"><span class="sw" style="background:var(--state-{seg.state})"></span>{seg.state} {seg.count}</span>
        {/each}
        {#if stats.done > 0}<span class="hidden-count">{stats.done} closed (hidden)</span>{/if}
      </div>
    {/if}

    <div class="viz">
      <div class="card">
        <span class="lab">carrying <span class="dim">· who's doing the work</span></span>
        <div class="load">
          {#each carrying as row (row.agentId)}
            <div class="lrow">
              <span class="who"><span class="av" style="background:{avatarColor(row.agentId)}">{avatar(row.agentId)}</span>{display(row.agentId)}</span>
              <div class="track"><div class="fill" style="width:{pct(row.count, maxCarrying)}%;background:var(--state-flight)"></div></div>
              <span class="cnt mono">{row.count}</span>
            </div>
          {:else}
            <p class="empty-note">nobody's carrying anything right now</p>
          {/each}
        </div>
        <span class="lab lab-2">asking <span class="dim">· who's waiting on work · <span class="unowned-key">■</span> unowned</span></span>
        <div class="load">
          {#each asking as row (row.agentId)}
            {@const unownedFrac = row.count ? (row.unownedCount ?? 0) / row.count : 0}
            <div class="lrow">
              <span class="who"><span class="av" style="background:{avatarColor(row.agentId)}">{avatar(row.agentId)}</span>{display(row.agentId)}</span>
              <div class="track">
                <div class="fill fill-ask" style="width:{pct(row.count, maxAsking)}%">
                  {#if unownedFrac > 0}<span class="unowned-portion" style="width:{unownedFrac * 100}%"></span>{/if}
                </div>
              </div>
              <span class="cnt mono">{row.count}</span>
            </div>
          {:else}
            <p class="empty-note">nobody's waiting on anything right now</p>
          {/each}
        </div>
      </div>

      <div class="card chart">
        <span class="lab">closure throughput <span class="dim">· last {THROUGHPUT_DAYS} days</span></span>
        <div class="bars">
          {#each throughputDays as d, i (d.day)}
            <div
              class="bar"
              class:today={i === throughputDays.length - 1}
              style="height:{Math.max(2, Math.round((d.closed / maxDayCount) * 100))}%"
              title="{d.day}: {d.closed} closed, {d.opened} opened"
            ></div>
          {/each}
        </div>
        <div class="axis mono">
          <span>{throughputDays[0]?.day ?? '—'}</span>
          <span>today</span>
        </div>
        <div class="chart-meta mono">
          <b>{throughputSummary.closed}</b> closed this window · opened {throughputSummary.opened}
          — net {throughputSummary.net > 0 ? '+' : ''}{throughputSummary.net}{#if throughputSummary.net !== 0}, {throughputSummary.net > 0
              ? 'backlog shrinking'
              : 'backlog growing'}{/if}
        </div>
      </div>
    </div>

    {#if stateFilter}
      <div class="filter-note" data-testid="board-filter-note">
        <span>showing only: {groups[0]?.label}</span>
        <button type="button" onclick={() => (stateFilter = null)}>show all ↺</button>
      </div>
    {/if}

    {#each groups as group (group.key)}
      {#if group.items.length}
        <div class="worklab">
          <span class="h h-{group.cls}">{group.glyph} {group.label}</span>
          <span class="c mono">{group.items.length}</span>
          <span class="ln"></span>
        </div>
        <div class="wlist">
          {#each group.items as request (request.id)}
            <TicketRow {request} {agents} selected={selectedRequestId === request.id} onSelect={onSelectRequest} />
          {/each}
        </div>
      {/if}
    {/each}

    {#if !stateFilter && stats.done > 0}
      <button type="button" class="done-fold" onclick={() => (doneOpen = !doneOpen)} aria-expanded={doneOpen} data-testid="board-done-fold">
        <span>✓</span> <span class="n">{stats.done} closed</span> — the arc that's already resolved
        <span class="chev">{doneOpen ? 'hide ︿' : 'show ⌄'}</span>
      </button>
      {#if doneOpen}
        <div class="wlist">
          {#each doneList as request (request.id)}
            <TicketRow {request} {agents} selected={selectedRequestId === request.id} onSelect={onSelectRequest} />
          {/each}
        </div>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .board-wrap {
    overflow: auto;
    flex: 1;
    padding: 16px 20px 40px;
  }

  .bhead {
    display: flex;
    align-items: baseline;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 14px;
  }
  .tier {
    font: 700 9.5px/1 var(--mono);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    border-radius: 5px;
    padding: 2px 6px;
  }
  .tier-own {
    color: var(--home-frame);
    background: var(--home-glow);
  }
  .tier-shared {
    color: var(--shared-frame);
    background: var(--shared-glow);
  }
  .tier-foreign {
    color: var(--foreign-frame);
    background: var(--foreign-glow);
  }
  .hub {
    font-weight: 640;
    font-size: 14px;
  }
  .verdict {
    margin-left: auto;
    font-size: 12.5px;
    color: var(--muted);
  }
  .verdict b {
    color: var(--state-unowned);
  }
  .vsep {
    color: var(--faint);
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
    margin-bottom: 12px;
  }
  .stat {
    position: relative;
    text-align: left;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 11px 13px;
    cursor: pointer;
    font: inherit;
    color: inherit;
  }
  .stat::before {
    content: '';
    position: absolute;
    left: 0;
    top: 10px;
    bottom: 10px;
    width: 3px;
    border-radius: 2px;
    background: var(--c);
  }
  .stat:hover,
  .stat.on {
    border-color: var(--c);
  }
  .stat.s-open {
    --c: var(--state-open);
  }
  .stat.s-flight {
    --c: var(--state-flight);
  }
  .stat.s-stuck {
    --c: var(--state-stuck);
  }
  .stat.s-unowned {
    --c: var(--state-unowned);
  }
  .stat .n {
    font: 700 22px/1 var(--mono);
    color: var(--c);
    font-variant-numeric: tabular-nums;
  }
  .stat .k {
    font-size: 12px;
    color: var(--text);
    margin-top: 4px;
  }
  .stat .sub {
    font: 500 10px/1 var(--mono);
    color: var(--faint);
    margin-top: 2px;
  }

  .flowbar {
    display: flex;
    height: 7px;
    border-radius: 4px;
    overflow: hidden;
    background: var(--panel-2);
    margin-bottom: 6px;
    gap: 1px;
  }
  .flowbar .fseg {
    height: 100%;
  }
  .flowlegend {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    font: 600 10.5px/1 var(--mono);
    color: var(--muted);
    margin-bottom: 16px;
  }
  .flowlegend .lg {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .flowlegend .sw {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  .flowlegend .hidden-count {
    margin-left: auto;
  }

  .viz {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    margin-bottom: 18px;
  }
  .card {
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 14px 15px;
  }
  .card .lab {
    display: block;
    font: 700 10px/1 var(--mono);
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--muted);
    margin-bottom: 9px;
  }
  .card .lab.lab-2 {
    margin: 14px 0 9px;
  }
  .card .lab .dim {
    color: var(--faint);
    text-transform: none;
    letter-spacing: normal;
    font-weight: 500;
  }
  .unowned-key {
    color: var(--state-unowned);
  }
  .load {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .load .empty-note {
    margin: 0;
    font-size: 11.5px;
    color: var(--faint);
    font-style: italic;
  }
  .lrow {
    display: grid;
    grid-template-columns: 5.5rem 1fr auto;
    align-items: center;
    gap: 9px;
  }
  .lrow .who {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .lrow .av {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    display: grid;
    place-items: center;
    font: 700 8px/1 var(--mono);
    color: #0b0e17;
    flex: 0 0 auto;
  }
  .lrow .track {
    height: 8px;
    border-radius: 4px;
    background: var(--panel-3, var(--panel));
    overflow: hidden;
    position: relative;
  }
  .lrow .fill {
    height: 100%;
    border-radius: 4px;
    position: relative;
  }
  .lrow .fill-ask {
    background: color-mix(in srgb, var(--muted) 40%, transparent);
  }
  .lrow .unowned-portion {
    position: absolute;
    right: 0;
    top: 0;
    bottom: 0;
    background: var(--state-unowned);
  }
  .lrow .cnt {
    font-size: 11.5px;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .chart {
    display: flex;
    flex-direction: column;
  }
  .chart .bars {
    display: flex;
    align-items: flex-end;
    gap: 4px;
    height: 74px;
    margin-top: 2px;
  }
  .chart .bar {
    flex: 1;
    background: color-mix(in srgb, var(--state-metric) 50%, transparent);
    border-radius: 3px 3px 0 0;
    min-height: 2px;
  }
  .chart .bar.today {
    background: var(--state-metric);
  }
  .chart .axis {
    display: flex;
    justify-content: space-between;
    font-size: 10px;
    color: var(--faint);
    margin-top: 5px;
  }
  .chart-meta {
    font-size: 11px;
    color: var(--muted);
    margin-top: 8px;
  }
  .chart-meta b {
    color: var(--state-metric);
    font-size: 13px;
  }

  .filter-note {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 8px;
  }
  .filter-note button {
    font: 600 11px/1 var(--mono);
    color: var(--accent);
    background: transparent;
    border: 1px solid var(--border-2);
    border-radius: 6px;
    padding: 3px 8px;
    cursor: pointer;
  }

  .worklab {
    display: flex;
    align-items: center;
    gap: 9px;
    margin: 18px 0 6px;
  }
  .worklab .h {
    font: 700 11px/1 var(--mono);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .worklab .h-attn {
    color: var(--state-unowned);
  }
  .worklab .h-flight {
    color: var(--state-flight);
  }
  .worklab .h-stuck {
    color: var(--state-stuck);
  }
  .worklab .c {
    font-size: 10.5px;
    color: var(--faint);
  }
  .worklab .ln {
    height: 1px;
    flex: 1;
    background: var(--border);
  }
  .wlist {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .done-fold {
    margin-top: 18px;
    border: 1px dashed var(--border-2);
    border-radius: var(--radius-sm);
    padding: 9px 13px;
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: 12px;
    background: transparent;
    width: 100%;
    text-align: left;
    cursor: pointer;
  }
  .done-fold:hover {
    border-color: var(--faint);
  }
  .done-fold .n {
    font-weight: 640;
    color: var(--text);
  }
  .done-fold .chev {
    margin-left: auto;
    font: 500 10.5px/1 var(--mono);
  }

  @media (max-width: 900px) {
    .stats {
      grid-template-columns: repeat(2, 1fr);
    }
    .viz {
      grid-template-columns: 1fr;
    }
  }
</style>
