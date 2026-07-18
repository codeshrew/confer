<script lang="ts">
  // Repos view — the hub's repo inventory (each hub's `repos/*.md`:
  // role/url/access/docs/owner) plus which of those repos are actually
  // cloned/mapped on THIS machine. `--ref <slug>:…` points into this
  // inventory, but until now there was nowhere in the dashboard to see it.
  //
  // Card grid mirrors Fleet.svelte's `.fleetgrid`/`.agentcard` language
  // (same panel/border/radius tokens) rather than inventing a new visual
  // vocabulary for "yet another list of things."
  import type { Repo } from '../types';
  import { api } from '../api';
  import EmptyState from './EmptyState.svelte';
  import Skeleton from './Skeleton.svelte';

  interface Props {
    hub: string;
  }

  let { hub }: Props = $props();

  let loading = $state(true);
  let repos = $state<Repo[]>([]);
  let loadedForHub = $state<string | null>(null);

  async function load(hubId: string) {
    loading = true;
    try {
      repos = await api.getRepos(hubId);
      loadedForHub = hubId;
    } catch (err) {
      console.error('confer serve: failed to load repos', hubId, err);
      repos = [];
      loadedForHub = hubId;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    // Re-fetch whenever the selected hub changes — including the very
    // first mount, since loadedForHub starts null.
    if (hub && hub !== loadedForHub) void load(hub);
  });

  const clonedCount = $derived(repos.filter((r) => r.cloned).length);

  const ROLE_LABEL: Record<string, string> = {
    hub: 'hub',
    code: 'code',
    docs: 'docs',
    tooling: 'tooling',
  };

  function roleLabel(role: string): string {
    return ROLE_LABEL[role] ?? role;
  }

  function accessLabel(access: string[]): string {
    return access.length === 0 ? 'all' : access.join(', ');
  }
</script>

<div class="repos-wrap" data-testid="repos-view">
  <div class="board-head">
    <div class="board-topline">
      <h2>Repos · {hub}</h2>
      {#if !loading}
        <span class="flabel" style="margin-left:auto">{repos.length} repos · {clonedCount} cloned here</span>
      {/if}
    </div>
  </div>

  {#if loading}
    <Skeleton rows={4} />
  {:else if repos.length === 0}
    <EmptyState
      glyph="◇"
      title="No repos registered"
      body="This hub hasn't registered any repos yet — add a repos/*.md card to the hub to make one show up here."
    />
  {:else}
    <div class="reposgrid">
      {#each repos as repo (repo.slug)}
        <div class="repocard" class:notcloned={!repo.cloned} data-testid="repo-card-{repo.slug}">
          <div class="rc-top">
            <span class="rc-slug">{repo.slug}</span>
            <span class="rc-role">{roleLabel(repo.role)}</span>
          </div>
          {#if repo.url}
            <div class="rc-url mono">{repo.url}</div>
          {/if}
          <div class="rc-meta">
            <div class="rc-metaitem">
              <span class="rc-metalab">access</span>
              <span>{accessLabel(repo.access)}</span>
            </div>
            {#if repo.docs}
              <div class="rc-metaitem">
                <span class="rc-metalab">docs</span>
                <span class="mono">{repo.docs}</span>
              </div>
            {/if}
            {#if repo.owner}
              <div class="rc-metaitem">
                <span class="rc-metalab">owner</span>
                <span>{repo.owner}</span>
              </div>
            {/if}
          </div>
          <div class="rc-clone">
            {#if repo.cloned}
              <div class="rc-clonestatus ok" data-testid="clone-status-{repo.slug}">
                <span class="rc-dot"></span>
                <span>✓ cloned</span>
                {#if repo.rootSha}
                  <span class="rc-sha mono">{repo.rootSha.slice(0, 7)}</span>
                {/if}
              </div>
              {#if repo.clonePath}
                <div class="rc-path mono">{repo.clonePath}</div>
              {/if}
            {:else}
              <div class="rc-clonestatus muted" data-testid="clone-status-{repo.slug}">
                <span class="rc-dot off"></span>
                <span>not cloned here</span>
              </div>
              <div class="rc-hint mono">confer repos map {repo.slug} &lt;path&gt;</div>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .repos-wrap {
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
  .flabel {
    font: 600 10px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.09em;
    color: var(--faint);
  }
  .reposgrid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 14px;
  }
  .repocard {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 14px 15px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .repocard.notcloned {
    border-style: dashed;
  }
  .rc-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .rc-slug {
    font-weight: 650;
    font-size: 14px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rc-role {
    margin-left: auto;
    flex: 0 0 auto;
    font: 700 9px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-radius: 5px;
    padding: 3px 6px;
  }
  .rc-url {
    font-size: 11px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rc-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 10px 16px;
    border-top: 1px solid var(--border);
    padding-top: 9px;
  }
  .rc-metaitem {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
    color: var(--text);
    min-width: 0;
  }
  .rc-metalab {
    font: 700 9px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--faint);
  }
  .rc-clone {
    border-top: 1px solid var(--border);
    padding-top: 9px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .rc-clonestatus {
    display: flex;
    align-items: center;
    gap: 7px;
    font: 600 12px/1 var(--sans);
  }
  .rc-clonestatus.ok {
    color: var(--done);
  }
  .rc-clonestatus.muted {
    color: var(--faint);
  }
  .rc-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--done);
    flex: 0 0 auto;
  }
  .rc-dot.off {
    background: var(--faint);
  }
  .rc-sha {
    margin-left: auto;
    font-size: 10.5px;
    color: var(--faint);
  }
  .rc-path {
    font-size: 11px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .rc-hint {
    font-size: 11px;
    color: var(--faint);
    overflow-wrap: anywhere;
  }
  .mono {
    font-family: var(--mono);
  }
</style>
