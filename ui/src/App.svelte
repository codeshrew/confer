<script lang="ts">
  import { onMount } from 'svelte';
  import TopBar from './lib/components/TopBar.svelte';
  import { api } from './lib/api';
  import { appState } from './lib/stores.svelte';
  import type { Hub } from './lib/types';

  let hubs = $state<Hub[]>([]);
  let live = $state(true);

  onMount(() => {
    document.documentElement.setAttribute('data-theme', appState.theme);

    api.getHubs().then((result) => {
      hubs = result;
    });

    const unsubscribe = api.subscribeEvents(() => {
      live = true;
    });
    return unsubscribe;
  });
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

  <!-- Later agents add the chat/board/fleet/code panes here. -->
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
</style>
