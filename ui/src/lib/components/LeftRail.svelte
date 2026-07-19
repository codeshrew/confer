<script lang="ts">
  import type { Agent, Topic } from '../types';
  import { formatAge } from '../format';
  import { paneFocus } from '../paneFocus.svelte';

  interface Props {
    hubName: string;
    topics: Topic[];
    currentTopic: string | null;
    agents: Agent[];
    onTopicSelect?: (slug: string) => void;
    now?: number;
    /** design/43: the Fleet view drops this section — its center pane IS the
     * roster, 20px away, so duplicating it in the rail is pure cost. Every
     * other view keeps it (default true). */
    showFleet?: boolean;
  }

  let { hubName, topics, currentTopic, agents, onTopicSelect, now = Date.now(), showFleet = true }: Props = $props();

  // keyboard-architecture pass — "topic-list" is one of the 7 named Layer-1
  // panes; it had no bare-key vocab before this pass (only click), so this
  // adds the same roving-tabindex j/k/Enter pattern HubRail already uses for
  // its (structurally identical) button list, rather than inventing a
  // different convention for the same UI shape.
  let focusedIdx = $state(0);
  let listEl: HTMLDivElement;
  let railEl: HTMLDivElement;

  $effect(() => {
    // topics reload on hub switch / poll — keep the roving index in range.
    if (focusedIdx >= topics.length) focusedIdx = Math.max(0, topics.length - 1);
  });

  $effect(() => {
    if (!railEl) return;
    return paneFocus.register({
      id: 'topic-list',
      label: 'Topics',
      el: listEl ?? railEl,
      getRect: () => railEl.getBoundingClientRect(),
    });
  });

  // See HubRail's identical note: real focus parks on the roving BUTTON,
  // not this wrapping div, so a direct Ctrl+hjkl hop onto the div needs one
  // forward-hop onto the current button.
  function forwardContainerFocus(e: FocusEvent) {
    if (e.target === listEl) buttonEls[focusedIdx]?.focus();
  }
  let buttonEls = $state<(HTMLButtonElement | null)[]>([]);

  function handleListKeydown(e: KeyboardEvent) {
    if (topics.length === 0) return;
    if (e.key === 'j' || e.key === 'ArrowDown') {
      e.preventDefault();
      focusedIdx = Math.min(focusedIdx + 1, topics.length - 1);
      buttonEls[focusedIdx]?.focus();
      return;
    }
    if (e.key === 'k' || e.key === 'ArrowUp') {
      e.preventDefault();
      focusedIdx = Math.max(focusedIdx - 1, 0);
      buttonEls[focusedIdx]?.focus();
      return;
    }
    if (e.key === 'Enter' || e.key === 'l') {
      e.preventDefault();
      const topic = topics[focusedIdx];
      if (topic) onTopicSelect?.(topic.slug);
    }
  }

  function stateClass(topic: Topic): string {
    if (topic.stale) return 'st-stale';
    if (topic.status === 'discussion') return 'st-disc';
    if (topic.status === 'open') return topic.open > 0 ? 'st-open' : 'st-quiet';
    return 'st-quiet';
  }

  function stateLabel(topic: Topic): string {
    if (topic.stale) return 'stale';
    if (topic.status === 'discussion') return 'disc';
    if (topic.status === 'open') return topic.open > 0 ? (topic.open > 1 ? `${topic.open} open` : 'open') : `${topic.messages}`;
    return `${topic.messages}`;
  }

  // demo affordance ported from the mockup: a topic with unread activity gets
  // a small accent dot + bold name — there's no `unread` field on Topic yet,
  // so this approximates it as "open topic that isn't the active one".
  function hasUnread(topic: Topic): boolean {
    return topic.status === 'open' && topic.open > 0 && topic.slug !== currentTopic;
  }
</script>

<div class="rail-l" bind:this={railEl}>
  <div class="rail-scroll">
    <div class="rail-head">
      <h3>{hubName}</h3>
    </div>

    <div
      class="topic-list"
      role="toolbar"
      aria-orientation="vertical"
      aria-label="topics"
      tabindex="-1"
      bind:this={listEl}
      onkeydown={handleListKeydown}
      onfocus={forwardContainerFocus}
    >
      {#each topics as topic, i (topic.slug)}
        <button
          type="button"
          class="topic"
          class:active={topic.slug === currentTopic}
          class:hasnew={hasUnread(topic)}
          tabindex={i === focusedIdx ? 0 : -1}
          bind:this={buttonEls[i]}
          onfocus={() => (focusedIdx = i)}
          onclick={() => onTopicSelect?.(topic.slug)}
        >
          <span class="hash">#</span>
          <span class="nm">{topic.slug}</span>
          {#if hasUnread(topic)}
            <span class="unread" title="unread"></span>
          {/if}
          <span class="state {stateClass(topic)}">{stateLabel(topic)}</span>
        </button>
      {/each}
    </div>

    {#if showFleet}
      <div class="rail-sep"></div>
      <div class="rail-head"><h3>Fleet · you + {agents.length}</h3></div>
      <div class="fleet">
        <div class="agent viewer">
          <span class="av veye">◉</span>
          <span class="nm">You</span>
          <span class="hb vw">viewing</span>
        </div>
        {#each agents as agent (agent.id)}
          <!-- Liveness is the real heartbeat-derived `live` field (matching Fleet.svelte),
               NOT the last-posted age: an agent can be live (watch armed, heartbeating) yet
               not have posted in days. The age shown is last-*posted*, surfaced as context. -->
          <div class="agent" class:stale={!agent.live} title={`${agent.live ? 'live' : 'heartbeat stale'} · last posted ${formatAge(agent.lastTs, now)} ago`}>
            <span class="av" style="color:{agent.color};background:color-mix(in srgb, {agent.color} 18%, transparent)">{agent.abbr}</span>
            <span class="nm">{agent.display}</span>
            <span class="hb">{formatAge(agent.lastTs, now)}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .rail-l {
    background: var(--panel);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .rail-scroll {
    overflow-y: auto;
    flex: 1;
    padding: 12px 10px;
  }
  .rail-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 2px 8px 8px;
  }
  .rail-head h3 {
    margin: 0;
    font: 700 11px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--faint);
  }
  .topic {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 7px 8px;
    border-radius: 8px;
    color: var(--muted);
    width: 100%;
    border: 0;
    background: transparent;
    text-align: left;
    font-size: 13px;
  }
  .topic:hover {
    background: var(--panel-2);
    color: var(--text);
  }
  .topic.active {
    background: var(--panel-3);
    color: var(--text);
  }
  .topic:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
  .topic .hash {
    font-family: var(--mono);
    color: var(--faint);
    font-size: 13px;
  }
  .topic.active .hash {
    color: var(--accent);
  }
  .topic .nm {
    flex: 1;
    font-weight: 550;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .topic .state {
    font: 600 9.5px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 3px 5px;
    border-radius: 5px;
  }
  .st-open {
    color: var(--open);
    background: color-mix(in srgb, var(--open) 15%, transparent);
  }
  .st-disc {
    color: var(--claimed);
    background: color-mix(in srgb, var(--claimed) 14%, transparent);
  }
  .st-stale {
    color: var(--blocked);
    background: color-mix(in srgb, var(--blocked) 15%, transparent);
  }
  .st-quiet {
    color: var(--faint);
  }
  .rail-sep {
    height: 1px;
    background: var(--border);
    margin: 12px 8px;
  }
  .topic .unread {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 7px -1px var(--accent);
  }
  .topic.hasnew .nm {
    color: var(--text);
    font-weight: 650;
  }

  .fleet {
    padding: 2px 8px 10px;
  }
  .fleet .agent {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 5px 6px;
    border-radius: 7px;
  }
  .fleet .agent:hover {
    background: var(--panel-2);
  }
  .av {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    font: 700 10px/1 var(--mono);
    letter-spacing: 0.02em;
  }
  .fleet .av.veye {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    font-size: 11px;
  }
  .fleet .nm {
    flex: 1;
    font-size: 12.5px;
    color: var(--text);
    font-weight: 500;
  }
  .fleet .agent.viewer .nm {
    color: var(--text);
    font-weight: 600;
  }
  .fleet .hb {
    font: 500 10.5px/1 var(--mono);
    color: var(--faint);
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .fleet .hb::before {
    content: '';
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--done);
  }
  .fleet .hb.vw {
    color: var(--accent);
  }
  .fleet .agent.viewer .hb.vw::before {
    background: var(--accent);
  }
  .fleet .agent.stale .hb {
    color: var(--blocked);
  }
  .fleet .agent.stale .hb::before {
    background: var(--blocked);
  }

  @media (max-width: 767.98px) {
    .topic,
    .fleet .agent {
      min-height: 40px;
    }
  }
</style>
