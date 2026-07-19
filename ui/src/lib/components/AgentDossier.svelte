<script lang="ts">
  // Piece 8b (ui/REDESIGN.md, `08-fleet-crew-deck.html` + `08-fleet-BRIEF.md`)
  // — the reusable agent dossier: "an agent is a composable type (like
  // ticket/note/code-ref)... the dossier is a reusable popover reachable
  // from anywhere an agent appears." Same overlay convention as the piece
  // 5/6/7 popovers. Wired from the Fleet deck, an Overview AgentNode, and a
  // message's seen-by roster (App.svelte).
  //
  // Law #3: confer version, watch/armed state, a signing-key fingerprint,
  // and the full roles/<id>.md profile were ALL flagged as backend asks at
  // launch and honestly omitted rather than faked — Herald shipped all four
  // same-night (src/api.rs commits 32ef9a4, 1318664), and every row below is
  // now real and wired, still honestly omitted per-agent when the
  // underlying signal is genuinely null. Trust state, host-match, WIP,
  // carrying/asking, real cross-hub presence, real activity buckets were
  // already a fold of data served from day one.
  import type { Agent, Message, RequestRow } from '../types';
  import { fetchHubOverviews } from '../api';
  import { agentPresence, deriveLiveness, deriveTrust, type AgentHubPresence } from '../attention';
  import { activityBuckets, askingFor, carryingFor, closedRecentCount } from '../fleetDossier';
  import { formatAge, formatAgeFromSecs } from '../format';
  import { renderMarkdown, highlightRenderedCodeBlocks } from '../markdown';
  import { isTypingTarget } from '../keys';
  import TicketMiniCard from './TicketMiniCard.svelte';

  interface Props {
    open: boolean;
    agentId: string | null;
    /** The current hub's fleet — the navigable `j`/`k` list, and where
     * `agentId` itself is looked up. */
    agents: Agent[];
    requests: RequestRow[];
    messages: Message[];
    onOpenTicket?: (id: string) => void;
    onNavigate?: (id: string) => void;
    onClose?: () => void;
  }

  let { open, agentId, agents, requests, messages, onOpenTicket, onNavigate, onClose }: Props = $props();

  const agent = $derived(agentId ? (agents.find((a) => a.id === agentId) ?? null) : null);
  const showing = $derived(open && agent !== null);

  const liveness = $derived(agent ? deriveLiveness(agent) : 'down');
  const trust = $derived(agent ? deriveTrust(agent) : 'unsigned');

  const carrying = $derived(agent ? carryingFor(agent.id, requests) : []);
  const asking = $derived(agent ? askingFor(agent.id, requests) : []);
  const unownedAskingCount = $derived(asking.filter((r) => r.claimants.length === 0).length);
  const closed7d = $derived(agent ? closedRecentCount(agent.id, messages, 7) : 0);

  const ACTIVITY_HOURS = 12;
  const activity = $derived(agent ? activityBuckets(agent.id, messages, ACTIVITY_HOURS) : []);
  const maxActivity = $derived(Math.max(1, ...activity.map((a) => a.count)));

  // Cross-hub presence — a lazy fetch on open (same pattern
  // RepoDetailPopover's own drill-in fetch uses), not eager for every
  // agent in a roster that's never opened.
  let presenceLoading = $state(false);
  let presence = $state<AgentHubPresence[]>([]);
  let presenceLoadedFor = $state<string | null>(null);

  async function loadPresence(id: string) {
    presenceLoading = true;
    try {
      const hubOverviews = await fetchHubOverviews();
      presence = agentPresence(hubOverviews, id);
      presenceLoadedFor = id;
    } catch (err) {
      console.error('confer serve: failed to load agent presence', id, err);
      presence = [];
      presenceLoadedFor = id;
    } finally {
      presenceLoading = false;
    }
  }

  $effect(() => {
    if (!showing || !agent) return;
    if (presenceLoadedFor !== agent.id) void loadPresence(agent.id);
  });

  // profileMarkdown (Herald, src/api.rs's `agent_row_json`, commit 1318664)
  // is the REAL `roles/<id>.md` body — `desc` stays the one-line frontmatter
  // fallback for a card that has no prose below it. Three-way honest chain:
  // real profile -> one-line desc -> nothing rendered at all. Both are
  // peer-authored, untrusted prose (server-sanitized the same way a message
  // body is), so both go through the SAME markdown pipeline a message body
  // does — no separate rendering path for "about" text.
  const aboutSource = $derived(agent?.profileMarkdown ?? agent?.desc ?? null);
  const aboutIsProfile = $derived(!!agent?.profileMarkdown);
  const aboutHtml = $derived(aboutSource ? renderMarkdown(aboutSource) : null);
  let aboutEl: HTMLElement | undefined = $state();
  $effect(() => {
    void aboutHtml;
    if (aboutEl) void highlightRenderedCodeBlocks(aboutEl);
  });

  const index = $derived(agentId ? agents.findIndex((a) => a.id === agentId) : -1);
  const prevId = $derived(index > 0 ? (agents[index - 1]?.id ?? null) : null);
  const nextId = $derived(index >= 0 && index < agents.length - 1 ? (agents[index + 1]?.id ?? null) : null);

  const LIVE_LABEL: Record<string, string> = { live: '⬤ live', stale: '◐ stale', down: '○ down' };
  const TRUST_LABEL: Record<string, string> = { signed: '✓ confirmed', mismatch: '‼ mismatch', 'first-sight': '⚠ first-sight', unsigned: '⚠ unsigned' };

  const hostMatches = $derived(agent && agent.expectedHost ? agent.lastHost === agent.expectedHost : null);

  /** `SHA256:l064aRMg7xJ3…zAbC` — the mockup's own shortened form: the
   * `SHA256:` prefix (meaningful, keep it) + a few chars of the hash on
   * each end, elided in the middle. The full fingerprint is still in the
   * `title` attribute for anyone who needs to actually compare it. */
  function shortenFingerprint(fp: string): string {
    const prefix = 'SHA256:';
    const hash = fp.startsWith(prefix) ? fp.slice(prefix.length) : fp;
    if (hash.length <= 16) return fp;
    return `${prefix}${hash.slice(0, 8)}…${hash.slice(-6)}`;
  }

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
      case 'Escape':
        e.preventDefault();
        onClose?.();
        break;
    }
  }

  // "On their plate" as ONE flat, roving-tabindex list (carrying, then
  // asking — the mockup's own reading order) — j/k select within the
  // popover already move AGENTS (prev/next), so this pane uses its own
  // arrow-key convention (↑↓), matching the composable-cards Related
  // column's precedent in NotePopover.
  const plateItems = $derived([...carrying, ...asking]);
  let plateIdx = $state(0);
  $effect(() => {
    if (plateIdx >= plateItems.length) plateIdx = Math.max(0, plateItems.length - 1);
  });
  function handlePlateKeydown(e: KeyboardEvent) {
    if (plateItems.length === 0) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      plateIdx = Math.min(plateIdx + 1, plateItems.length - 1);
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      plateIdx = Math.max(plateIdx - 1, 0);
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const item = plateItems[plateIdx];
      if (item) onOpenTicket?.(item.id);
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if showing && agent}
  <div class="ad-overlay">
    <div class="ad-backdrop" onclick={onClose} aria-hidden="true" data-testid="agent-dossier-backdrop"></div>
    <div class="ad-panel" role="dialog" aria-modal="true" aria-label="Agent dossier" tabindex="-1" data-testid="agent-dossier">
      <div class="ad-head">
        <span class="avatar {liveness}" style={liveness === 'down' ? undefined : `background:${agent.color}`}>{agent.abbr}</span>
        <span class="who">
          <span class="nm">{agent.display}</span>
          <span class="role mono">{agent.id}{#if agent.lastHost} · {agent.lastHost}{/if}</span>
        </span>
        <span class="live-badge {liveness}">
          {LIVE_LABEL[liveness]}{#if agent.hbAgeSecs != null} · hb {formatAgeFromSecs(agent.hbAgeSecs)}{/if}
        </span>
        <button type="button" class="ad-close" aria-label="Close dossier" onclick={onClose}>esc ✕</button>
      </div>

      <div class="ad-grid">
        <div class="ad-main">
          {#if aboutSource}
            <div class="ad-block">
              <span class="lab">{aboutIsProfile ? `roles/${agent.id}.md` : 'about'}</span>
              <div class="about prose md" bind:this={aboutEl}>
                {#if aboutHtml}{@html aboutHtml}{/if}
              </div>
            </div>
          {/if}

          <div class="ad-block">
            <span class="lab">at a glance</span>
            <div class="facts">
              <div class="fact"><div class="fn">{carrying.length}</div><div class="fk">carrying now</div></div>
              <div class="fact"><div class="fn">{closed7d}</div><div class="fk">closed · 7d</div></div>
              <div class="fact"><div class="fn">{presenceLoading ? '—' : presence.length}</div><div class="fk">hubs</div></div>
            </div>
          </div>

          {#if plateItems.length}
            <div class="ad-block">
              <span class="lab">on their plate</span>
              <div class="plate" role="listbox" aria-label="carrying and asking" tabindex="-1" onkeydown={handlePlateKeydown} data-testid="agent-plate">
                {#if carrying.length}
                  <span class="sublab">carrying <span class="dim">{carrying.length}</span></span>
                  {#each carrying as row (row.id)}
                    {@const idx = plateItems.findIndex((r) => r.id === row.id)}
                    <TicketMiniCard request={row} {agents} selected={idx === plateIdx} onSelect={() => onOpenTicket?.(row.id)} />
                  {/each}
                {/if}
                {#if asking.length}
                  <span class="sublab"
                    >asking <span class="dim">{asking.length}{#if unownedAskingCount}
                        · {unownedAskingCount === asking.length ? 'all' : unownedAskingCount} unowned{/if}</span
                    ></span
                  >
                  {#each asking as row (row.id)}
                    {@const idx = plateItems.findIndex((r) => r.id === row.id)}
                    <TicketMiniCard request={row} {agents} selected={idx === plateIdx} onSelect={() => onOpenTicket?.(row.id)} />
                  {/each}
                {/if}
              </div>
            </div>
          {:else}
            <div class="ad-block">
              <span class="lab">on their plate</span>
              <p class="ad-empty">nothing carried or asked right now</p>
            </div>
          {/if}

          <div class="ad-block">
            <span class="lab">activity <span class="dim">· last {ACTIVITY_HOURS}h</span></span>
            <div class="actchart">
              {#each activity as bucket, i (bucket.hour)}
                <i class:hot={i === activity.length - 1 && bucket.count > 0} style="height:{Math.max(2, (bucket.count / maxActivity) * 100)}%" title="{bucket.hour}: {bucket.count}"></i>
              {/each}
            </div>
            <div class="actaxis mono">
              <span>{ACTIVITY_HOURS}h ago</span>
              <span>now</span>
            </div>
          </div>
        </div>

        <aside class="ad-side">
          <div class="kv"><span class="k">role</span><span class="v mono">{agent.id}</span></div>
          {#if agent.version}
            <div class="kv"><span class="k">confer version</span><span class="v mono">{agent.version}</span></div>
          {/if}
          <div class="kv">
            <span class="k">signing key</span>
            {#if agent.keyFingerprint}
              <span class="v mono fp" title={agent.keyFingerprint}>{shortenFingerprint(agent.keyFingerprint)}</span>
            {/if}
            <span class="v mono trust-{trust}">{TRUST_LABEL[trust]}</span>
          </div>
          {#if agent.watchState}
            <div class="kv">
              <span class="k">watch</span>
              <span class="v mono watch-{agent.watchState}">{agent.watchState === 'armed' ? '● armed · reactive' : '◐ idle'}</span>
            </div>
          {/if}
          <div class="kv">
            <span class="k">host</span>
            <span class="v mono">
              {agent.lastHost ?? '—'}
              {#if hostMatches !== null}
                {hostMatches ? '✓ matches expected' : `≠ expected ${agent.expectedHost}`}
              {/if}
            </span>
          </div>
          <div class="kv">
            <span class="k">present in</span>
            {#if presenceLoading}
              <span class="v dim">loading…</span>
            {:else if presence.length === 0}
              <span class="v dim">not seen on any hub yet</span>
            {:else}
              <div class="presence">
                {#each presence as p (p.hub)}
                  <div class="p">
                    <span class="hd tier-{p.tier ?? 'unclassified'}"></span>
                    <span class="nm">{p.hub}</span>
                    <span class="st">{p.lastTs ? `last ${formatAge(p.lastTs)}` : 'unknown'}</span>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
          <div class="kv"><span class="k">last posted</span><span class="v mono">{agent.lastTs ? formatAge(agent.lastTs) + ' ago' : 'never'}</span></div>
        </aside>
      </div>

      <div class="ad-foot mono">
        <span class="kk">↑</span><span class="kk">↓</span> select · <span class="kk">↵</span> open ticket
        <span class="ad-nav">
          <button type="button" class="ad-navbtn" disabled={!prevId} onclick={() => prevId && onNavigate?.(prevId)} aria-label="Previous agent">‹</button>
          <span class="kk">j</span><span class="kk">k</span> prev · next agent
          <button type="button" class="ad-navbtn" disabled={!nextId} onclick={() => nextId && onNavigate?.(nextId)} aria-label="Next agent">›</button>
        </span>
      </div>
    </div>
  </div>
{/if}

<style>
  .ad-overlay {
    position: fixed;
    inset: 0;
    z-index: 61;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--phi2, 24px);
  }
  .ad-backdrop {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--bg) 72%, transparent);
    backdrop-filter: blur(2px);
  }
  .ad-panel {
    position: relative;
    width: min(780px, 100%);
    max-height: 88vh;
    overflow: hidden;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
  }
  .ad-head {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 13px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--panel-2);
  }
  .ad-head .avatar {
    width: 40px;
    height: 40px;
    border-radius: 9px;
    display: grid;
    place-items: center;
    font: 700 13px/1 var(--mono);
    color: #0b0e17;
    flex: 0 0 auto;
    position: relative;
  }
  .ad-head .avatar.live {
    box-shadow:
      0 0 0 2px var(--panel-2),
      0 0 12px -1px color-mix(in srgb, var(--state-flight) 55%, transparent);
    animation: ad-breathe 3.4s ease-in-out infinite;
  }
  .ad-head .avatar.down {
    background: transparent !important;
    color: var(--faint) !important;
    border: 1.5px dashed var(--border-2);
  }
  @media (prefers-reduced-motion: reduce) {
    .ad-head .avatar.live {
      animation: none;
    }
  }
  @keyframes ad-breathe {
    0%,
    100% {
      box-shadow:
        0 0 0 2px var(--panel-2),
        0 0 8px -2px color-mix(in srgb, var(--state-flight) 55%, transparent);
    }
    50% {
      box-shadow:
        0 0 0 2px var(--panel-2),
        0 0 16px 1px color-mix(in srgb, var(--state-flight) 55%, transparent);
    }
  }
  .ad-head .who {
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .ad-head .nm {
    font-weight: 650;
    font-size: 15px;
  }
  .ad-head .role {
    font-size: 10.5px;
    color: var(--faint);
  }
  .ad-head .live-badge {
    margin-left: auto;
    font-size: 11px;
  }
  .ad-head .live-badge.live {
    color: var(--state-flight);
  }
  .ad-head .live-badge.stale {
    color: var(--state-unowned);
  }
  .ad-head .live-badge.down {
    color: var(--state-stuck);
  }
  .ad-close {
    font: 500 11px/1 var(--mono);
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 3px 7px;
    background: transparent;
    cursor: pointer;
  }
  .ad-close:hover {
    color: var(--text);
    border-color: var(--faint);
  }

  .ad-grid {
    display: grid;
    grid-template-columns: 1fr 280px;
    min-height: 0;
    overflow: hidden;
  }
  .ad-main {
    padding: 16px 18px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
  }
  .ad-block {
    margin-bottom: 18px;
  }
  .ad-block:last-child {
    margin-bottom: 0;
  }
  .ad-block .lab {
    display: block;
    font: 700 10px/1 var(--mono);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
    margin-bottom: 8px;
  }
  .ad-block .lab .dim {
    color: var(--faint);
    text-transform: none;
    letter-spacing: normal;
    font-weight: 500;
  }
  .about :global(p) {
    font-size: 12.5px;
    color: var(--fg-dim, var(--muted));
    line-height: 1.6;
    margin: 0 0 8px;
  }
  .about :global(p:last-child) {
    margin-bottom: 0;
  }

  .facts {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .fact {
    background: var(--panel-2);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 8px 9px;
  }
  .fact .fn {
    font: 700 16px/1 var(--mono);
    color: var(--text);
  }
  .fact .fk {
    font-size: 9.5px;
    color: var(--muted);
    margin-top: 2px;
  }

  .plate {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .sublab {
    font: 600 9.5px/1 var(--mono);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--muted);
    margin: 4px 0 2px;
  }
  .sublab:first-child {
    margin-top: 0;
  }
  .sublab .dim {
    color: var(--faint);
    text-transform: none;
    letter-spacing: normal;
  }
  .ad-empty {
    margin: 0;
    font-size: 11.5px;
    color: var(--faint);
    font-style: italic;
  }

  .actchart {
    display: flex;
    align-items: flex-end;
    gap: 3px;
    height: 52px;
  }
  .actchart i {
    flex: 1;
    background: color-mix(in srgb, var(--accent) 40%, transparent);
    border-radius: 2px 2px 0 0;
    min-height: 2px;
  }
  .actchart i.hot {
    background: var(--accent);
  }
  .actaxis {
    display: flex;
    justify-content: space-between;
    font-size: 9.5px;
    color: var(--faint);
    margin-top: 5px;
  }

  .ad-side {
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
  .kv .v.dim {
    color: var(--faint);
    font-style: italic;
  }
  .kv .v.trust-signed {
    color: var(--state-flight);
  }
  .kv .v.trust-mismatch,
  .kv .v.trust-unsigned {
    color: var(--state-stuck);
  }
  .kv .v.trust-first-sight {
    color: var(--state-unowned);
  }
  .kv .v.fp {
    color: var(--faint);
    font-size: 10.5px;
    display: block;
    margin-bottom: 3px;
  }
  .kv .v.watch-armed {
    color: var(--state-flight);
  }
  .kv .v.watch-idle {
    color: var(--muted);
  }
  .presence {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .presence .p {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 11px;
  }
  .presence .p .hd {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    flex: 0 0 auto;
  }
  .presence .p .hd.tier-own {
    background: var(--home-frame, var(--accent));
  }
  .presence .p .hd.tier-shared {
    background: var(--shared-frame, var(--accent));
  }
  .presence .p .hd.tier-foreign {
    background: var(--foreign-frame, var(--muted));
  }
  .presence .p .hd.tier-unclassified {
    background: var(--neutral-frame, var(--faint));
  }
  .presence .p .nm {
    color: var(--fg-dim, var(--muted));
  }
  .presence .p .st {
    margin-left: auto;
    color: var(--muted);
    font-family: var(--mono);
    font-size: 10px;
  }

  .ad-foot {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 16px;
    border-top: 1px solid var(--border);
    background: var(--panel-2);
    font-size: 10.5px;
    color: var(--muted);
    flex-wrap: wrap;
  }
  .ad-foot .kk {
    color: var(--fg-dim, var(--muted));
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0 4px;
  }
  .ad-nav {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .ad-navbtn {
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 1px 6px;
    background: transparent;
    cursor: pointer;
    font: inherit;
  }
  .ad-navbtn:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }
  .ad-navbtn:disabled {
    opacity: 0.35;
    cursor: default;
  }

  @media (max-width: 720px) {
    .ad-grid {
      grid-template-columns: 1fr;
    }
    .ad-main {
      border-right: none;
      border-bottom: 1px solid var(--border);
    }
  }
</style>
