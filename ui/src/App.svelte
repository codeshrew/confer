<script lang="ts">
  import { onMount } from 'svelte';
  import TopBar from './lib/components/TopBar.svelte';
  import LeftRail from './lib/components/LeftRail.svelte';
  import FilterBar, { type StatusFilter } from './lib/components/FilterBar.svelte';
  import ChatStream from './lib/components/ChatStream.svelte';
  import Board from './lib/components/Board.svelte';
  import MetaThread from './lib/components/MetaThread.svelte';
  import { api } from './lib/api';
  import { appState } from './lib/stores.svelte';
  import type { Hub, Message, Overview, ThreadNode } from './lib/types';

  let hubs = $state<Hub[]>([]);
  let overview = $state<Overview | null>(null);
  let messages = $state<Message[]>([]);
  let thread = $state<ThreadNode[]>([]);
  let live = $state(true);

  let statusFilter = $state<StatusFilter>('all');
  let notesOn = $state(true);
  let reqsOn = $state(true);
  let selectedRequestId = $state<string | null>(null);

  async function loadHub(hubId: string) {
    const [ov, msgs, th] = await Promise.all([
      api.getOverview(hubId),
      api.getMessages(hubId),
      api.getThread(hubId, ''),
    ]);
    overview = ov;
    messages = msgs;
    thread = th;
  }

  onMount(() => {
    document.documentElement.setAttribute('data-theme', appState.theme);

    api.getHubs().then((result) => {
      hubs = result;
    });
    void loadHub(appState.hub);

    const unsubscribe = api.subscribeEvents(() => {
      live = true;
    });
    return unsubscribe;
  });

  $effect(() => {
    // Reload the hub's overview/messages/thread whenever the current hub
    // changes (TopBar hub-pill click).
    void loadHub(appState.hub);
  });

  const currentTopic = $derived(overview?.topics.find((t) => t.slug === appState.topic) ?? null);

  function selectMessage(id: string) {
    const found = messages.find((m) => m.id === id);
    appState.selectedMessage = found ?? null;
  }

  function selectTicket(id: string) {
    selectedRequestId = id;
    // A ticket's originating message shares the `msg_`/`req_` id suffix
    // convention used across the mock fixtures (see ChatStream.findRequest).
    const asMsgId = id.replace(/^req_/, 'msg_');
    const found = messages.find((m) => m.id === asMsgId);
    appState.selectedMessage = found ?? null;
  }

  function selectBoardRow(id: string) {
    selectedRequestId = id;
  }
</script>

<div class="app">
  <TopBar
    {hubs}
    currentHub={appState.hub}
    currentView={appState.view}
    {live}
    theme={appState.theme}
    onHubChange={(hubId) => (appState.hub = hubId)}
    onViewChange={(view) => (appState.view = view)}
    onThemeToggle={() => appState.toggleTheme()}
  />

  {#if appState.view === 'chat' || appState.view === 'board'}
    <FilterBar
      {statusFilter}
      {notesOn}
      {reqsOn}
      agents={overview?.fleet ?? []}
      onStatusFilterChange={(f) => (statusFilter = f)}
      onToggleNotes={() => (notesOn = !notesOn)}
      onToggleReqs={() => (reqsOn = !reqsOn)}
    />
  {/if}

  <div class="main">
    <LeftRail
      hubName={appState.hub}
      topics={overview?.topics ?? []}
      currentTopic={appState.topic}
      agents={overview?.fleet ?? []}
      onTopicSelect={(slug) => (appState.topic = slug)}
    />

    <div class="center">
      <div class="crumb">
        <span class="c">{appState.hub}</span>
        <span class="sep">›</span>
        {#if appState.view === 'chat'}
          <span class="c strong hash">#{appState.topic}</span>
          {#if currentTopic}
            <span class="meta">{currentTopic.messages} messages · {currentTopic.requests} requests</span>
          {/if}
        {:else if appState.view === 'board'}
          <span class="c strong">Board</span>
        {:else}
          <span class="c strong">{appState.view}</span>
        {/if}
      </div>

      {#if appState.view === 'chat'}
        <ChatStream
          {messages}
          requests={overview?.board.requests ?? []}
          agents={overview?.fleet ?? []}
          topic={appState.topic}
          {notesOn}
          {reqsOn}
          selectedMessageId={appState.selectedMessage?.id ?? null}
          onSelectMessage={selectMessage}
          onSelectTicket={selectTicket}
        />
      {:else if appState.view === 'board'}
        <Board
          requests={overview?.board.requests ?? []}
          agents={overview?.fleet ?? []}
          hubName={appState.hub}
          {selectedRequestId}
          onSelectRequest={selectBoardRow}
        />
      {:else if appState.view === 'fleet'}
        <!-- Fleet view: a later agent's slot (agent-identity cards). -->
      {:else if appState.view === 'code'}
        <!-- Code view: a later agent's slot (conversation-density file browser). -->
      {/if}
    </div>

    <div class="rail-r">
      <div class="ctx-head">
        <div class="k">Reference graph</div>
        <h2>Meta-thread</h2>
      </div>
      <div class="ctx-body">
        <MetaThread {thread} agents={overview?.fleet ?? []} {messages} />
      </div>
      <div class="foothint">↩ discovered via reply-hashes — no extra state, pure projection</div>
    </div>
  </div>
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  .main {
    flex: 1;
    display: grid;
    grid-template-columns: 248px 1fr 320px;
    min-height: 0;
  }
  .center {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg);
  }
  .crumb {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
    flex: 0 0 auto;
    background: var(--panel);
  }
  .crumb .c {
    color: var(--muted);
    font-size: 13px;
  }
  .crumb .c.strong {
    color: var(--text);
    font-weight: 600;
  }
  .crumb .sep {
    color: var(--faint);
  }
  .crumb .hash {
    font-family: var(--mono);
    color: var(--accent);
  }
  .crumb .meta {
    margin-left: auto;
    color: var(--faint);
    font: 500 11.5px/1 var(--mono);
  }
  .rail-r {
    background: var(--panel);
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .ctx-head {
    padding: 14px 16px 12px;
    border-bottom: 1px solid var(--border);
    flex: 0 0 auto;
  }
  .ctx-head .k {
    font: 700 10px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--faint);
  }
  .ctx-head h2 {
    margin: 8px 0 0;
    font-size: 14.5px;
    font-weight: 650;
  }
  .ctx-body {
    overflow-y: auto;
    flex: 1;
    padding: 14px 16px;
  }
  .foothint {
    padding: 10px 16px;
    border-top: 1px solid var(--border);
    color: var(--faint);
    font-size: 11px;
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 0 0 auto;
  }
</style>
