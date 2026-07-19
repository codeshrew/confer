<script lang="ts">
  // Repos — piece 7 (ui/REDESIGN.md, `redesign-mockups/07-repos-integrity-
  // gravity.html`): not a flat inventory list, a map of two real questions
  // a list never answered. INTEGRITY — is every repo `--ref` points into
  // actually registered AND cloned, so refs resolve? GRAVITY — which repos
  // does the fleet's work actually center on? The tiered grouping (tracked /
  // registered-not-local / shadow) + per-repo reference density both fold
  // from real data already served (`getRepos` + `getCodeFiles`) — see
  // repoIndex.ts's own header note for why SHADOW detection needs no new
  // fetch, just a diff of two lists already on hand.
  import type { Repo, CodeFile } from '../types';
  import { api } from '../api';
  import { buildRepoIndex, repoHealth, type RepoIndexEntry } from '../repoIndex';
  import EmptyState from './EmptyState.svelte';
  import Skeleton from './Skeleton.svelte';
  import RepoDetailPopover from './RepoDetailPopover.svelte';

  interface Props {
    hub: string;
    /** Drill-in's "open in code view" — jumps to the Code view, either a
     * specific hot file or the whole repo's rollup. */
    onOpenCode?: (repo: string, path?: string) => void;
  }

  let { hub, onOpenCode }: Props = $props();

  let loading = $state(true);
  let repos = $state<Repo[]>([]);
  let codeFiles = $state<CodeFile[]>([]);
  let loadedForHub = $state<string | null>(null);

  async function load(hubId: string) {
    loading = true;
    try {
      const [repoList, fileList] = await Promise.all([api.getRepos(hubId), api.getCodeFiles(hubId)]);
      repos = repoList;
      codeFiles = fileList;
      loadedForHub = hubId;
    } catch (err) {
      console.error('confer serve: failed to load the repo index', hubId, err);
      repos = [];
      codeFiles = [];
      loadedForHub = hubId;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (hub && hub !== loadedForHub) void load(hub);
  });

  const index = $derived(buildRepoIndex(repos, codeFiles));
  const health = $derived(repoHealth(index));
  const tracked = $derived(index.filter((e) => e.tier === 'tracked'));
  const notLocal = $derived(index.filter((e) => e.tier === 'notlocal'));
  const shadow = $derived(index.filter((e) => e.tier === 'shadow'));

  const healthLine = $derived.by((): string | null => {
    const parts: string[] = [];
    if (health.shadowCount > 0) parts.push(`${health.shadowCount} not registered`);
    if (health.notLocalCount > 0) parts.push(`${health.notLocalCount} not cloned`);
    return parts.length ? parts.join(' · ') : null;
  });

  const CLONE_LABEL: Record<RepoIndexEntry['tier'], (e: RepoIndexEntry) => string> = {
    tracked: () => '✓ cloned',
    notlocal: () => '◑ not cloned',
    shadow: () => '◇ shadow',
  };

  let selected = $state<RepoIndexEntry | null>(null);
  let detailOpen = $state(false);

  function openDetail(entry: RepoIndexEntry) {
    selected = entry;
    detailOpen = true;
  }

  function maxDensity(entries: RepoIndexEntry[]): number {
    return Math.max(1, ...entries.flatMap((e) => e.topFileCounts));
  }
</script>

<div class="repos-wrap" data-testid="repos-view">
  {#if loading}
    <Skeleton rows={4} />
  {:else if index.length === 0}
    <EmptyState
      glyph="◇"
      title="No repos registered"
      body="This hub hasn't registered any repos yet — add a repos/*.md card to the hub to make one show up here."
    />
  {:else}
    <div class="rhead">
      <h2>repos</h2>
      <span class="v mono"><b>{health.registeredCount}</b> registered</span>
      <span class="v mono"><b>{health.trackedCount}</b> cloned here</span>
      {#if health.shadowCount > 0}<span class="v mono"><b>{health.shadowCount}</b> shadow</span>{/if}
      {#if healthLine}
        <span class="health warn">◑ {healthLine}</span>
      {:else}
        <span class="health ok">✓ all mapped</span>
      {/if}
    </div>

    {#each [{ tier: 'tracked' as const, entries: tracked, label: '✓ tracked', desc: 'registered + cloned — refs resolve to real code' }, { tier: 'notlocal' as const, entries: notLocal, label: '◑ registered · not on this machine', desc: 'refs stay pointer-only until a clone is mapped' }, { tier: 'shadow' as const, entries: shadow, label: '◇ shadow · referenced, not registered', desc: 'a peer --refs it but the hub inventory doesn\'t know' }] as group (group.tier)}
      {#if group.entries.length}
        {@const max = maxDensity(group.entries)}
        <div class="grp">
          <div class="grp-h {group.tier}">
            <span class="h">{group.label}</span>
            <span class="c mono">{group.entries.length}</span>
            <span class="desc">{group.desc}</span>
            <span class="ln"></span>
          </div>
          <div class="repos">
            {#each group.entries as entry (entry.slug)}
              <button type="button" class="repo {entry.tier}" onclick={() => openDetail(entry)} data-testid="repo-row-{entry.slug}">
                <div class="rl">
                  <div class="slug">
                    {entry.slug}
                    {#if entry.role}<span class="role">{entry.role}</span>{/if}
                  </div>
                  <div class="meta mono">
                    {#if entry.tier === 'tracked'}{entry.url ?? 'local-only · no remote'} · {entry.clonePath}
                    {:else if entry.tier === 'notlocal'}{entry.url ?? 'no remote'} · no local clone
                    {:else}referenced by {entry.refCount} ref{entry.refCount === 1 ? '' : 's'} · no inventory card{/if}
                  </div>
                </div>
                <div class="dens">
                  <span class="clone">{CLONE_LABEL[entry.tier](entry)}</span>
                  <span class="n mono"><b>{entry.refCount}</b> refs</span>
                  {#if entry.topFileCounts.length}
                    <span class="densbar" aria-hidden="true">
                      {#each entry.topFileCounts as c, i (i)}<i style="height:{Math.max(2, (c / max) * 16)}px"></i>{/each}
                    </span>
                  {/if}
                </div>
              </button>
            {/each}
          </div>
        </div>
      {/if}
    {/each}
  {/if}
</div>

<RepoDetailPopover
  open={detailOpen}
  entry={selected}
  {hub}
  onOpenCode={(repo, path) => {
    detailOpen = false;
    onOpenCode?.(repo, path);
  }}
  onClose={() => (detailOpen = false)}
/>

<style>
  .repos-wrap {
    overflow: auto;
    flex: 1;
    padding: 16px 20px 40px;
  }

  .rhead {
    display: flex;
    align-items: baseline;
    gap: 14px;
    flex-wrap: wrap;
    margin-bottom: 8px;
  }
  .rhead h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 650;
  }
  .rhead .v {
    font-size: 11px;
    color: var(--muted);
  }
  .rhead .v b {
    color: var(--text);
  }
  .rhead .health {
    margin-left: auto;
    font: 600 11px/1 var(--mono);
  }
  .rhead .health.ok {
    color: var(--state-flight);
  }
  .rhead .health.warn {
    color: var(--state-unowned);
  }

  .grp {
    margin-top: 20px;
  }
  .grp-h {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }
  .grp-h .h {
    font: 700 10.5px/1 var(--mono);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .grp-h.tracked .h {
    color: var(--state-flight);
  }
  .grp-h.notlocal .h {
    color: var(--state-unowned);
  }
  .grp-h.shadow .h {
    color: var(--blue, var(--accent));
  }
  .grp-h .c {
    font-size: 10px;
    color: var(--faint);
  }
  .grp-h .desc {
    font-size: 11px;
    color: var(--muted);
  }
  .grp-h .ln {
    height: 1px;
    flex: 1;
    background: var(--border);
  }

  .repos {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .repo {
    position: relative;
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: 14px;
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 13px;
    text-align: left;
    font: inherit;
    color: inherit;
    cursor: pointer;
    transition:
      border-color 0.12s,
      transform 0.12s;
    width: 100%;
  }
  .repo::before {
    content: '';
    position: absolute;
    left: 0;
    top: 9px;
    bottom: 9px;
    width: 3px;
    border-radius: 2px;
    background: var(--c);
  }
  .repo:hover {
    border-color: var(--accent);
    transform: translateY(-1px);
  }
  .repo.tracked {
    --c: var(--state-flight);
  }
  .repo.notlocal {
    --c: var(--state-unowned);
  }
  .repo.shadow {
    --c: var(--blue, var(--accent));
    border-style: dashed;
  }
  .repo .rl {
    min-width: 0;
  }
  .repo .slug {
    font-family: var(--mono);
    font-size: 13.5px;
    font-weight: 640;
    color: var(--text);
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .repo .role {
    font: 700 8.5px/1 var(--mono);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--muted);
    border: 1px solid var(--border-2);
    border-radius: 4px;
    padding: 1px 5px;
  }
  .repo .meta {
    font-size: 10.5px;
    color: var(--muted);
    margin-top: 3px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .repo .dens {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
  }
  .repo .dens .n {
    font-size: 11px;
    color: var(--fg-dim, var(--muted));
  }
  .repo .dens .n b {
    color: var(--accent);
  }
  .repo .dens .clone {
    font: 500 10px/1 var(--mono);
    padding: 2px 6px;
    border-radius: 5px;
    border: 1px solid var(--border);
    color: var(--c);
    border-color: color-mix(in srgb, var(--c) 40%, transparent);
  }
  .repo .densbar {
    display: flex;
    gap: 1.5px;
    align-items: flex-end;
    height: 16px;
  }
  .repo .densbar i {
    width: 3px;
    background: color-mix(in srgb, var(--accent) 45%, transparent);
    border-radius: 1px;
    min-height: 2px;
  }

  @media (max-width: 720px) {
    .repo {
      grid-template-columns: 1fr auto;
    }
    .repo .meta {
      white-space: normal;
      overflow-wrap: anywhere;
    }
  }
</style>
