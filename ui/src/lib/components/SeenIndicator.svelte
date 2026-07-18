<script lang="ts">
  export interface SeenEntry {
    id: string;
    name: string;
    color?: string;
    ts: string | null;
    isYou?: boolean;
    unseen?: boolean;
  }

  interface Props {
    entries: SeenEntry[];
  }

  let { entries }: Props = $props();

  const allSeen = $derived(entries.length > 0 && entries.every((e) => !e.unseen));
  const seenCount = $derived(entries.filter((e) => !e.unseen && !e.isYou).length);
  const totalCount = $derived(entries.filter((e) => !e.isYou).length);
  const youEntry = $derived(entries.find((e) => e.isYou));

  const rosterTitle = $derived.by(() => {
    if (allSeen) return 'Seen · everyone addressed';
    const youPart = youEntry ? (youEntry.unseen ? '' : ' + you') : '';
    return `Seen · ${seenCount} of ${totalCount} addressed${youPart}`;
  });

  let pinned = $state(false);

  function toggle(e: MouseEvent) {
    e.stopPropagation();
    pinned = !pinned;
  }
</script>

<span class="seen" class:done={allSeen} class:pin={pinned}>
  {#if allSeen}
    <button type="button" class="seen-btn" onclick={toggle} aria-expanded={pinned} aria-label="Seen roster">
      <span class="chk">✓</span><span class="lab">all seen</span>
    </button>
  {:else}
    <button type="button" class="seen-btn" onclick={toggle} aria-expanded={pinned} aria-label="Seen roster">
      <span class="lab">seen</span>
      <span class="dots">
        {#each entries.filter((e) => !e.isYou) as entry (entry.id)}
          <span class="sd" class:un={entry.unseen} style={entry.unseen ? '' : `background:${entry.color}`}></span>
        {/each}
        {#if youEntry}
          <span class="sd you"></span>
        {/if}
      </span>
    </button>
  {/if}

  <span class="roster" role="dialog" aria-label="Seen by">
    <span class="rt">{rosterTitle}</span>
    {#each entries.filter((e) => !e.isYou) as entry (entry.id)}
      <span class="rr" class:un={entry.unseen}>
        <span class="ra" style={entry.unseen ? '' : `color:${entry.color};background:color-mix(in srgb, ${entry.color} 18%, transparent)`}
          >{entry.unseen ? '' : entry.name.slice(0, 2).toUpperCase()}</span
        >
        <span class="rn">{entry.name}</span>
        <span class="rtime">{entry.unseen ? 'unseen' : entry.ts}</span>
      </span>
    {/each}
    {#if youEntry}
      <span class="rr">
        <span class="ra eye">◉</span>
        <span class="rn">You</span>
        <span class="rtime">{youEntry.ts}</span>
      </span>
    {/if}
  </span>
</span>

<style>
  .seen {
    position: relative;
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .seen-btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    border: 0;
    background: transparent;
    padding: 0;
    color: inherit;
  }
  .seen .lab {
    font: 600 9.5px/1 var(--mono);
    letter-spacing: 0.04em;
    color: var(--faint);
  }
  .seen.done .lab {
    color: var(--done);
  }
  .seen .dots {
    display: flex;
    padding-left: 2px;
  }
  .seen .sd {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    margin-left: -2px;
    box-shadow: 0 0 0 2px var(--bg);
  }
  .seen .sd.un {
    background: transparent !important;
    box-shadow: inset 0 0 0 1.5px var(--faint), 0 0 0 2px var(--bg);
    opacity: 0.7;
  }
  .seen .sd.you {
    position: relative;
    background: transparent !important;
    box-shadow: inset 0 0 0 1.5px var(--text), 0 0 0 2px var(--bg);
  }
  .seen .sd.you::after {
    content: '';
    position: absolute;
    inset: 3px;
    border-radius: 50%;
    background: var(--text);
  }
  .seen .chk {
    color: var(--done);
    font-size: 11px;
    font-weight: 800;
  }

  .seen .roster {
    position: absolute;
    top: calc(100% + 7px);
    right: 0;
    z-index: 30;
    display: none;
    width: 210px;
    background: var(--panel-2);
    border: 1px solid var(--border-2);
    border-radius: 10px;
    padding: 10px 11px;
    box-shadow: var(--shadow);
    cursor: default;
    text-align: left;
  }
  .seen:hover .roster,
  .seen.pin .roster {
    display: block;
  }
  .roster .rt {
    font: 700 8.5px/1 var(--mono);
    letter-spacing: 0.11em;
    text-transform: uppercase;
    color: var(--faint);
    margin-bottom: 9px;
  }
  .roster .rr {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
  }
  .roster .ra {
    width: 19px;
    height: 19px;
    border-radius: 5px;
    display: grid;
    place-items: center;
    font: 700 8.5px/1 var(--mono);
    flex: 0 0 auto;
  }
  .roster .ra.eye {
    color: var(--text);
    box-shadow: inset 0 0 0 1.5px var(--text);
    font-size: 9px;
  }
  .roster .rn {
    flex: 1;
    font-size: 12px;
    color: var(--text);
  }
  .roster .rtime {
    font: 500 10px/1 var(--mono);
    color: var(--faint);
  }
  .roster .rr.un .rn {
    color: var(--muted);
  }
  .roster .rr.un .rtime {
    color: var(--blocked);
  }
</style>
