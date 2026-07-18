<script lang="ts">
  import type { Agent, Topic } from '../types';
  import { formatAge, isStaleAge } from '../format';

  interface Props {
    hubName: string;
    topics: Topic[];
    currentTopic: string | null;
    agents: Agent[];
    onTopicSelect?: (slug: string) => void;
    now?: number;
  }

  let { hubName, topics, currentTopic, agents, onTopicSelect, now = Date.now() }: Props = $props();

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

<div class="rail-l">
  <div class="rail-scroll">
    <div class="rail-head">
      <h3>{hubName}</h3>
      <span class="add">+</span>
    </div>

    {#each topics as topic (topic.slug)}
      <button
        type="button"
        class="topic"
        class:active={topic.slug === currentTopic}
        class:hasnew={hasUnread(topic)}
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

    <div class="rail-sep"></div>
    <div class="rail-head"><h3>Fleet · you + {agents.length}</h3></div>
    <div class="fleet">
      <div class="agent viewer">
        <span class="av veye">◉</span>
        <span class="nm">You</span>
        <span class="hb vw">viewing</span>
      </div>
      {#each agents as agent (agent.id)}
        <div class="agent" class:stale={isStaleAge(agent.lastTs, now)}>
          <span class="av" style="color:{agent.color};background:color-mix(in srgb, {agent.color} 18%, transparent)">{agent.abbr}</span>
          <span class="nm">{agent.display}</span>
          <span class="hb">{formatAge(agent.lastTs, now)}</span>
        </div>
      {/each}
    </div>
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
  .rail-head .add {
    color: var(--faint);
    font-size: 15px;
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
</style>
