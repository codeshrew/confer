<script lang="ts">
  import type { Agent, RequestRow } from '../types';
  import BoardRow from './BoardRow.svelte';

  interface Props {
    requests: RequestRow[];
    agents: Agent[];
    hubName: string;
    selectedRequestId?: string | null;
    onSelectRequest?: (id: string) => void;
  }

  let { requests, agents, hubName, selectedRequestId = null, onSelectRequest }: Props = $props();

  type GroupBy = 'status' | 'topic' | 'claimant';
  let groupBy = $state<GroupBy>('status');

  type Bucket = 'open' | 'claimed' | 'blocked' | 'done' | 'backlog' | 'error';
  const BUCKET_ORDER: Bucket[] = ['open', 'claimed', 'blocked', 'done', 'backlog', 'error'];
  const BUCKET_VAR: Record<Bucket, string> = {
    open: 'var(--open)',
    claimed: 'var(--claimed)',
    blocked: 'var(--blocked)',
    done: 'var(--done)',
    backlog: 'var(--deferred)',
    error: 'var(--error)',
  };

  function bucketOf(r: RequestRow): Bucket {
    if (r.deferred && r.status === 'OPEN') return 'backlog';
    switch (r.status) {
      case 'CLAIMED':
        return 'claimed';
      case 'BLOCKED':
        return 'blocked';
      case 'DONE':
      case 'SUPERSEDED':
        return 'done';
      case 'ERROR':
        return 'error';
      default:
        return 'open';
    }
  }

  function cap(s: string): string {
    return s.length ? s[0]!.toUpperCase() + s.slice(1) : s;
  }

  const distribution = $derived.by(() => {
    const counts = new Map<Bucket, number>();
    for (const r of requests) counts.set(bucketOf(r), (counts.get(bucketOf(r)) ?? 0) + 1);
    return BUCKET_ORDER.filter((b) => counts.get(b)).map((b) => ({ bucket: b, count: counts.get(b) ?? 0 }));
  });
  const total = $derived(requests.length || 1);

  interface Lane {
    key: string;
    label: string;
    color: string;
    statusVarFor: (r: RequestRow) => string;
    items: RequestRow[];
  }

  const lanes = $derived.by((): Lane[] => {
    if (groupBy === 'status') {
      return distribution.map(({ bucket }) => ({
        key: bucket,
        label: bucket,
        color: BUCKET_VAR[bucket],
        statusVarFor: () => BUCKET_VAR[bucket],
        items: requests.filter((r) => bucketOf(r) === bucket),
      }));
    }
    if (groupBy === 'topic') {
      const topics = [...new Set(requests.map((r) => r.topic ?? '—'))];
      return topics.map((t) => ({
        key: t,
        label: `#${t}`,
        color: 'var(--muted)',
        statusVarFor: (r) => BUCKET_VAR[bucketOf(r)],
        items: requests.filter((r) => (r.topic ?? '—') === t),
      }));
    }
    const claimants = [...new Set(requests.map((r) => r.claimants[0] ?? 'unclaimed'))];
    return claimants.map((c) => ({
      key: c,
      label: c === 'unclaimed' ? 'unclaimed' : cap(c),
      color: 'var(--faint)',
      statusVarFor: (r) => BUCKET_VAR[bucketOf(r)],
      items: requests.filter((r) => (r.claimants[0] ?? 'unclaimed') === c),
    }));
  });
</script>

<div class="board-wrap">
  <div class="board-head">
    <div class="board-topline">
      <h2>Ticket board · {hubName}</h2>
      <div class="groupby" role="tablist" aria-label="Group by">
        <button type="button" class:on={groupBy === 'status'} onclick={() => (groupBy = 'status')}>Status</button>
        <button type="button" class:on={groupBy === 'topic'} onclick={() => (groupBy = 'topic')}>Topic</button>
        <button type="button" class:on={groupBy === 'claimant'} onclick={() => (groupBy = 'claimant')}>Claimant</button>
      </div>
    </div>
    <div class="distbar">
      {#each distribution as d (d.bucket)}
        <i style="width:{(d.count / total) * 100}%;background:{BUCKET_VAR[d.bucket]}"></i>
      {/each}
    </div>
    <div class="board-counts">
      {#each distribution as d (d.bucket)}
        <span class="bc"><i style="background:{BUCKET_VAR[d.bucket]}"></i><b>{d.count}</b> {d.bucket}</span>
      {/each}
    </div>
  </div>

  <div class="board-lanes">
    {#each lanes as lane (lane.key)}
      <div class="lane">
        <div class="lane-head">
          <span class="ld" style="background:{lane.color}"></span>
          <span class="ln">{lane.label}</span>
          <span class="lc">{lane.items.length}</span>
        </div>
        {#each lane.items as request (request.id)}
          <BoardRow
            {request}
            {agents}
            statusVar={lane.statusVarFor(request)}
            selected={selectedRequestId === request.id}
            onSelect={onSelectRequest}
          />
        {/each}
      </div>
    {/each}
  </div>
</div>

<style>
  .board-wrap {
    overflow: auto;
    flex: 1;
    padding: 16px 20px;
  }
  .board-head {
    margin-bottom: 6px;
  }
  .board-topline {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 12px;
  }
  .board-topline h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 650;
  }
  .groupby {
    margin-left: auto;
    display: flex;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 2px;
  }
  .groupby button {
    border: 0;
    background: transparent;
    color: var(--muted);
    font: 600 11.5px/1 var(--sans);
    padding: 6px 12px;
    border-radius: 6px;
    cursor: pointer;
  }
  .groupby button.on {
    background: var(--panel-3);
    color: var(--text);
    box-shadow: inset 0 0 0 1px var(--border-2);
  }
  .distbar {
    display: flex;
    height: 8px;
    border-radius: 5px;
    overflow: hidden;
    background: var(--panel-2);
    margin-bottom: 10px;
    gap: 2px;
  }
  .distbar i {
    height: 100%;
    border-radius: 2px;
  }
  .board-counts {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
    font: 600 11px/1 var(--mono);
    color: var(--muted);
  }
  .board-counts .bc {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .board-counts .bc i {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  .board-counts .bc b {
    color: var(--text);
  }
  .lane {
    margin-top: 18px;
  }
  .lane-head {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 6px;
    padding-bottom: 7px;
    border-bottom: 1px solid var(--border);
  }
  .lane-head .ld {
    width: 9px;
    height: 9px;
    border-radius: 3px;
    flex: 0 0 auto;
  }
  .lane-head .ln {
    font: 700 11px/1 var(--mono);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text);
  }
  .lane-head .lc {
    font: 600 10.5px/1 var(--mono);
    color: var(--faint);
  }
</style>
