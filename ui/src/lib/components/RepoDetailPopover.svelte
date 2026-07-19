<script lang="ts">
  // Piece 7's repo drill-in (ui/REDESIGN.md, `07-repos-integrity-gravity.html`)
  // — "hot files ranked by conversation density (mini code-ref cards) → the
  // big code view, plus its identity and the actions to keep it mapped."
  // Same overlay convention as TicketFullPopover/NotePopover. Fetches the
  // repo's real reverse-index hits ONCE, on open (reuses the SAME
  // `getRefs(hub, repo, true)` + `groupRefHitsByFile` CodeLens's own repo-
  // rollup already established — design/44 §6 item 2.4 — not a new query
  // shape) — every number here (ref count, distinct messages, topics, hot
  // files) is real, nothing estimated.
  import type { RepoIndexEntry } from '../repoIndex';
  import { api } from '../api';
  import { groupRefHitsByFile, type FileRollupGroup } from '../codeTree';
  import { isTypingTarget } from '../keys';

  interface Props {
    open: boolean;
    entry: RepoIndexEntry | null;
    hub: string;
    /** Jumps to the Code view — a specific file if given, else the whole
     * repo's rollup (same distinction CodeTree's own file-vs-repo-node
     * click already makes). */
    onOpenCode?: (repo: string, path?: string) => void;
    onClose?: () => void;
  }

  let { open, entry, hub, onOpenCode, onClose }: Props = $props();

  const showing = $derived(open && entry !== null);

  const HOT_FILES_VISIBLE = 4;

  let loading = $state(false);
  let hits = $state<import('../types').RefHit[]>([]);
  let loadedFor = $state<string | null>(null);

  async function load(slug: string, hubId: string) {
    loading = true;
    try {
      hits = await api.getRefs(hubId, slug, true);
      loadedFor = slug;
    } catch (err) {
      console.error('confer serve: failed to load repo reverse-index', slug, err);
      hits = [];
      loadedFor = slug;
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (!showing || !entry) return;
    if (loadedFor !== entry.slug) void load(entry.slug, hub);
  });

  const hotFiles = $derived<FileRollupGroup[]>(entry && loadedFor === entry.slug ? groupRefHitsByFile(hits) : []);
  const visibleFiles = $derived(hotFiles.slice(0, HOT_FILES_VISIBLE));
  const moreCount = $derived(Math.max(0, hotFiles.length - HOT_FILES_VISIBLE));

  const messageCount = $derived(new Set(hits.map((h) => h.msgId)).size);
  const topicCount = $derived(new Set(hits.map((h) => h.topic).filter((t): t is string => t !== null)).size);

  const maxFileCount = $derived(Math.max(1, ...hotFiles.map((f) => f.count)));

  function handleKeydown(e: KeyboardEvent) {
    if (!showing) return;
    if (isTypingTarget(e.target)) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose?.();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if showing && entry}
  <div class="rd-overlay">
    <div class="rd-backdrop" onclick={onClose} aria-hidden="true" data-testid="repo-detail-backdrop"></div>
    <div class="rd-panel" role="dialog" aria-modal="true" aria-label="Repo detail" tabindex="-1" data-testid="repo-detail-popover">
      <div class="rd-head">
        <span class="slug mono">{entry.slug}</span>
        {#if entry.role}<span class="role">{entry.role}</span>{/if}
        {#if entry.cloned}
          <span class="clone ok mono">✓ cloned · {entry.clonePath}</span>
        {:else}
          <span class="clone mono">{entry.tier === 'shadow' ? '◇ shadow — not registered' : '◑ not cloned here'}</span>
        {/if}
        <button type="button" class="rd-close" aria-label="Close repo detail" onclick={onClose}>esc ✕</button>
      </div>

      <div class="rd-grid">
        <div class="rd-main">
          <span class="lab">hot files <span class="dim">· most referenced in conversation</span></span>
          {#if loading}
            <p class="rd-note">loading…</p>
          {:else if hotFiles.length === 0}
            <p class="rd-note">nothing referenced yet</p>
          {:else}
            {#each visibleFiles as f (f.path)}
              <button type="button" class="filerow" onclick={() => onOpenCode?.(entry!.slug, f.path)} data-testid="repo-hot-file">
                <span class="gutter"><i style="height:{Math.max(3, (f.count / maxFileCount) * 15)}px"></i></span>
                <span class="path mono">{f.path}</span>
                <span class="conv mono"><b>{f.count}</b> ref{f.count === 1 ? '' : 's'}</span>
              </button>
            {/each}
            {#if moreCount > 0}
              <div class="filerow more">
                <span class="gutter"></span>
                <span class="path">+ {moreCount} more referenced file{moreCount === 1 ? '' : 's'}</span>
                <span class="conv"></span>
              </div>
            {/if}
          {/if}
        </div>

        <aside class="rd-side">
          {#if entry.url}
            <div class="kv"><span class="k">remote</span><span class="v mono">{entry.url}</span></div>
          {/if}
          <div class="kv">
            <span class="k">local clone</span>
            <span class="v mono" class:ok={entry.cloned}>{entry.cloned ? `${entry.clonePath} ✓ mapped` : 'none'}</span>
          </div>
          {#if entry.role}<div class="kv"><span class="k">role</span><span class="v mono">{entry.role}</span></div>{/if}
          {#if entry.access.length}
            <div class="kv"><span class="k">access</span><span class="v">{entry.access.join(', ')}</span></div>
          {/if}
          <div class="kv">
            <span class="k">referenced</span>
            <span class="v"
              >{entry.refCount} refs{!loading
                ? ` · across ${messageCount} message${messageCount === 1 ? '' : 's'} · ${topicCount} topic${topicCount === 1 ? '' : 's'}`
                : ''}</span
            >
          </div>
          <div class="rd-actions">
            <button type="button" class="btn" onclick={() => onOpenCode?.(entry!.slug)}>open in code view ›</button>
          </div>
          <!-- DEFERRED: state-mutating actions (map/register/pull) wait on
               a future human-action layer — confer serve is read-only
               today. Kept as intent, not a fake button: dashed, inert,
               titled so it reads as "coming", never a silent no-op. -->
          <div class="rd-deferred">
            <span class="lab" style="display:block;margin-bottom:5px;font-size:9px">later · needs the human-action layer</span>
            {#if entry.tier === 'shadow'}
              <span class="dbtn" title="serve is read-only today — this will register the repo once the human-action layer lands">register</span>
            {:else if entry.tier === 'notlocal'}
              <span class="dbtn" title="serve is read-only today — this will map a local clone once the human-action layer lands">map / re-discover clone</span>
            {:else}
              <span class="dbtn" title="serve is read-only today — this will pull latest once the human-action layer lands">pull latest · human-gated</span>
            {/if}
          </div>
        </aside>
      </div>
    </div>
  </div>
{/if}

<style>
  .rd-overlay {
    position: fixed;
    inset: 0;
    z-index: 61;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--phi2, 24px);
  }
  .rd-backdrop {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--bg) 72%, transparent);
    backdrop-filter: blur(2px);
  }
  .rd-panel {
    position: relative;
    width: min(760px, 100%);
    max-height: 88vh;
    overflow: hidden;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
  }
  .rd-head {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 12px 15px;
    border-bottom: 1px solid var(--border);
    background: var(--panel-2);
  }
  .rd-head .slug {
    font-weight: 640;
    font-size: 14px;
  }
  .rd-head .role {
    font: 700 9px/1 var(--mono);
    text-transform: uppercase;
    color: var(--muted);
    border: 1px solid var(--border-2);
    border-radius: 4px;
    padding: 2px 5px;
  }
  .rd-head .clone {
    margin-left: auto;
    font-size: 11px;
    color: var(--muted);
  }
  .rd-head .clone.ok {
    color: var(--state-flight);
  }
  .rd-close {
    font: 500 11px/1 var(--mono);
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 7px;
    background: transparent;
    cursor: pointer;
  }
  .rd-close:hover {
    color: var(--text);
    border-color: var(--faint);
  }

  .rd-grid {
    display: grid;
    grid-template-columns: 1fr 260px;
    min-height: 0;
    overflow: hidden;
  }
  .rd-main {
    padding: 16px 18px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
  }
  .rd-main .lab {
    display: block;
    font: 700 10px/1 var(--mono);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    margin-bottom: 10px;
  }
  .rd-main .lab .dim {
    color: var(--faint);
    text-transform: none;
    letter-spacing: normal;
    font-weight: 500;
  }
  .rd-note {
    font-size: 12px;
    color: var(--faint);
    font-style: italic;
    margin: 0;
  }
  .filerow {
    display: grid;
    grid-template-columns: 2rem 1fr auto;
    align-items: center;
    gap: 10px;
    padding: 6px 7px;
    border-radius: 7px;
    width: 100%;
    background: transparent;
    border: 0;
    text-align: left;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .filerow:hover {
    background: var(--panel-2);
  }
  .filerow.more {
    color: var(--faint);
    cursor: default;
  }
  .filerow.more:hover {
    background: transparent;
  }
  .filerow .gutter {
    display: flex;
    align-items: flex-end;
    justify-content: flex-end;
    height: 15px;
  }
  .filerow .gutter i {
    width: 4px;
    background: var(--accent);
    opacity: 0.75;
    border-radius: 1px;
    min-height: 2px;
  }
  .filerow .path {
    font-size: 11.5px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .filerow .conv {
    font-size: 10.5px;
    color: var(--muted);
  }
  .filerow .conv b {
    color: var(--accent);
  }

  .rd-side {
    padding: 16px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    background: color-mix(in srgb, var(--panel-2) 55%, var(--panel));
  }
  .kv .k {
    display: block;
    font: 700 9px/1 var(--mono);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--muted);
    margin-bottom: 3px;
  }
  .kv .v {
    color: var(--text);
    font-size: 12px;
  }
  .kv .v.mono {
    font-family: var(--mono);
    font-size: 11px;
    word-break: break-all;
  }
  .kv .v.ok {
    color: var(--state-flight);
  }
  .rd-actions {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 2px;
  }
  .rd-actions .btn {
    font: 500 11px/1 var(--mono);
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 8px;
    text-align: center;
    background: transparent;
    cursor: pointer;
  }
  .rd-actions .btn:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
  .rd-deferred {
    margin-top: 8px;
    padding-top: 8px;
    border-top: 1px dashed var(--border-2);
    display: flex;
    flex-direction: column;
    gap: 5px;
    opacity: 0.6;
  }
  .rd-deferred .dbtn {
    font: 500 10.5px/1 var(--mono);
    color: var(--muted);
    border: 1px dashed var(--border-2);
    border-radius: 6px;
    padding: 5px 8px;
    text-align: center;
    cursor: not-allowed;
  }

  @media (max-width: 720px) {
    .rd-grid {
      grid-template-columns: 1fr;
    }
    .rd-main {
      border-right: none;
      border-bottom: 1px solid var(--border);
    }
  }
</style>
