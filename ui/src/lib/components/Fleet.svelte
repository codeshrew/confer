<script lang="ts">
  // Fleet / agent-identity view. Ports `.fleetgrid`/`.agentcard`/`.ac-*` from
  // design/serve-dashboard-v2-mockup.html: identity cards (color/host/
  // heartbeat/verification), a "You · viewing" card, a public-hub warn card,
  // and a "customize identity" inline editor that live-updates the avatar.
  //
  // Appearance edits here are local-preview only (no backend call exists
  // yet to persist a role card's self-declared display/color/abbr) — same
  // seam as the mockup's `liveEdit`/`pickColor`, which only mutate the DOM.
  import type { Agent } from '../types';
  import { formatAge, isStaleAge } from '../format';

  interface Props {
    agents: Agent[];
    hubName: string;
  }

  let { agents, hubName }: Props = $props();

  const SWATCHES = ['--ag-herald', '--ag-reader', '--ag-pipeline', '--ag-compositor', '--ag-jarvis', '--ag-orbit', '--accent'];

  interface Override {
    display: string;
    abbr: string;
    color: string;
  }

  // Keyed by agent id (or 'you'); undefined until the editor's first touched.
  let overrides = $state<Record<string, Override>>({});
  let openEditors = $state<Record<string, boolean>>({});

  function overrideFor(id: string, base: Override): Override {
    return overrides[id] ?? base;
  }

  function setOverride(id: string, patch: Partial<Override>, base: Override) {
    const current = overrideFor(id, base);
    overrides = { ...overrides, [id]: { ...current, ...patch } };
  }

  function toggleEditor(id: string) {
    openEditors = { ...openEditors, [id]: !openEditors[id] };
  }

  const staleCount = $derived(agents.filter((a) => isStaleAge(a.lastTs)).length);

  const youBase: Override = { display: 'You', abbr: '◉', color: 'var(--accent)' };
  const you = $derived(overrideFor('you', youBase));

  const VERIFY_GLYPH: Record<Agent['verified'], { g: string; cls: string; title: string }> = {
    signed: { g: '✓', cls: 'ok', title: 'verified — key matches the role card' },
    'first-sight': { g: '⚠', cls: 'warn', title: 'first-sight — no history to verify against yet' },
    unverified: { g: '⚠', cls: 'warn', title: 'unverified — heads up before trusting claims' },
  };
</script>

<div class="fleet-wrap">
  <div class="board-head">
    <div class="board-topline">
      <h2>Fleet · {hubName}</h2>
      <span class="flabel" style="margin-left:auto">{agents.length} agents · {staleCount} stale heartbeat{staleCount === 1 ? '' : 's'}</span>
    </div>
  </div>

  <div class="fleetgrid">
    <!-- "You" — the dashboard viewer, a first-class but non-claiming peer -->
    <div class="agentcard you">
      <div class="ac-top">
        <span class="ac-av" style="color:{you.color};background:color-mix(in srgb, {you.color} 18%, transparent)">{you.abbr}</span>
        <div class="ac-id">
          <div class="ac-nm">{you.display}</div>
          <div class="ac-host">viewing this dashboard</div>
        </div>
      </div>
      <div class="ac-hb">watching · not a claiming peer</div>
      <button type="button" class="ac-editbtn" onclick={() => toggleEditor('you')}>Customize identity</button>
      {#if openEditors['you']}
        <div class="ac-editor open" data-testid="editor-you">
          <div class="ac-field">
            <label for="you-nm">Display name</label>
            <input id="you-nm" type="text" value={you.display} oninput={(e) => setOverride('you', { display: e.currentTarget.value }, youBase)} />
          </div>
          <div class="ac-field">
            <label for="you-abbr">Abbreviation (2 chars)</label>
            <input
              id="you-abbr"
              type="text"
              maxlength="2"
              value={you.abbr}
              oninput={(e) => setOverride('you', { abbr: e.currentTarget.value }, youBase)}
            />
          </div>
          <div class="ac-field">
            <label for="you-color">Color</label>
            <div class="ac-swatches" id="you-color">
              {#each SWATCHES as sw (sw)}
                <button
                  type="button"
                  class="ac-swatch"
                  class:sel={you.color === `var(${sw})`}
                  style="background:var({sw})"
                  aria-label={sw}
                  onclick={() => setOverride('you', { color: `var(${sw})` }, youBase)}
                ></button>
              {/each}
            </div>
          </div>
          <div class="ac-imgslot"><span class="box">🖼</span> image icon — coming later, falls back to the abbreviation</div>
          <p class="ac-note">
            Appearance is self-declared and travels with the role card — every peer sees the same name/color/abbreviation
            you pick here.
          </p>
        </div>
      {/if}
    </div>

    {#each agents as agent (agent.id)}
      {@const base = { display: agent.display, abbr: agent.abbr, color: agent.color }}
      {@const cur = overrideFor(agent.id, base)}
      {@const stale = isStaleAge(agent.lastTs)}
      {@const vm = VERIFY_GLYPH[agent.verified]}
      <div class="agentcard" class:stale>
        <div class="ac-top">
          <span class="ac-av" style="color:{cur.color};background:color-mix(in srgb, {cur.color} 18%, transparent)">{cur.abbr}</span>
          <div class="ac-id">
            <div class="ac-nm">{cur.display}</div>
            <div class="ac-host">{agent.lastHost ?? agent.expectedHost ?? '—'}</div>
          </div>
          <span class="ac-verify {vm.cls}" title={vm.title}>{vm.g}</span>
        </div>
        <div class="ac-hb">{formatAge(agent.lastTs)} ago{stale ? ' · heartbeat stale' : ''}</div>
        {#if agent.verified === 'unverified'}
          <div class="ac-warnline">⚠ unverified peer — if this hub's remote allows anonymous read, verify the key before trusting claims</div>
        {/if}
        <div class="ac-wip">
          <div class="ac-wiplab">Current WIP</div>
          {#if agent.wip.length === 0}
            <div class="ac-idle">no active claims</div>
          {:else}
            {#each agent.wip as w (w.id)}
              <div class="ac-wipitem">
                <span class="pill p-{w.status.toLowerCase()}">{w.status}</span>
                <span>{w.summary}</span>
                <span class="mono">{w.id}</span>
              </div>
            {/each}
          {/if}
        </div>
        <button type="button" class="ac-editbtn" onclick={() => toggleEditor(agent.id)}>Customize identity</button>
        {#if openEditors[agent.id]}
          <div class="ac-editor open" data-testid="editor-{agent.id}">
            <div class="ac-field">
              <label for="{agent.id}-nm">Display name</label>
              <input
                id="{agent.id}-nm"
                type="text"
                value={cur.display}
                oninput={(e) => setOverride(agent.id, { display: e.currentTarget.value }, base)}
              />
            </div>
            <div class="ac-field">
              <label for="{agent.id}-abbr">Abbreviation (2 chars)</label>
              <input
                id="{agent.id}-abbr"
                type="text"
                maxlength="2"
                value={cur.abbr}
                oninput={(e) => setOverride(agent.id, { abbr: e.currentTarget.value.toUpperCase() }, base)}
              />
            </div>
            <div class="ac-field">
              <label for="{agent.id}-color">Color</label>
              <div class="ac-swatches" id="{agent.id}-color">
                {#each SWATCHES.slice(0, 6) as sw (sw)}
                  <button
                    type="button"
                    class="ac-swatch"
                    class:sel={cur.color === `var(${sw})`}
                    style="background:var({sw})"
                    aria-label={sw}
                    onclick={() => setOverride(agent.id, { color: `var(${sw})` }, base)}
                  ></button>
                {/each}
              </div>
            </div>
            <div class="ac-imgslot"><span class="box">🖼</span> image icon — coming later, falls back to the abbreviation</div>
            <p class="ac-note">
              Appearance is self-declared and travels with the role card — every peer sees the same
              name/color/abbreviation you pick here.
            </p>
          </div>
        {/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .fleet-wrap {
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
  .fleetgrid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 14px;
  }
  .agentcard {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 14px 15px;
    display: flex;
    flex-direction: column;
    gap: 11px;
  }
  .agentcard.you {
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
  }
  .ac-top {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .ac-av {
    width: 36px;
    height: 36px;
    border-radius: 9px;
    display: grid;
    place-items: center;
    font: 700 13px/1 var(--mono);
    flex: 0 0 auto;
  }
  .ac-id {
    min-width: 0;
    flex: 1;
  }
  .ac-nm {
    font-weight: 650;
    font-size: 13.5px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .ac-host {
    font: 500 10.5px/1 var(--mono);
    color: var(--faint);
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ac-verify {
    font-size: 13px;
    flex: 0 0 auto;
  }
  .ac-verify.ok {
    color: var(--done);
  }
  .ac-verify.warn {
    color: var(--blocked);
  }
  .ac-verify.unk {
    color: var(--faint);
  }
  .ac-hb {
    display: flex;
    align-items: center;
    gap: 6px;
    font: 500 11px/1 var(--mono);
    color: var(--muted);
  }
  .ac-hb::before {
    content: '';
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--done);
  }
  .agentcard.stale .ac-hb {
    color: var(--blocked);
  }
  .agentcard.stale .ac-hb::before {
    background: var(--blocked);
  }
  .ac-wip {
    border-top: 1px solid var(--border);
    padding-top: 9px;
  }
  .ac-wiplab {
    font: 700 9px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--faint);
    margin-bottom: 6px;
  }
  .ac-wipitem {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: var(--text);
    padding: 2px 0;
  }
  .ac-wipitem .mono {
    color: var(--faint);
    font-size: 10.5px;
  }
  .ac-wipitem .pill {
    font: 700 8.5px/1 var(--mono);
    letter-spacing: 0.05em;
    text-transform: uppercase;
    padding: 2px 5px;
    border-radius: 5px;
  }
  .p-open {
    color: var(--open);
    background: color-mix(in srgb, var(--open) 16%, transparent);
  }
  .p-claimed {
    color: var(--claimed);
    background: color-mix(in srgb, var(--claimed) 16%, transparent);
  }
  .p-blocked {
    color: var(--blocked);
    background: color-mix(in srgb, var(--blocked) 16%, transparent);
  }
  .p-done {
    color: var(--done);
    background: color-mix(in srgb, var(--done) 16%, transparent);
  }
  .p-error {
    color: var(--error);
    background: color-mix(in srgb, var(--error) 16%, transparent);
  }
  .ac-idle {
    font-size: 12px;
    color: var(--faint);
    font-style: italic;
  }
  .ac-warnline {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 2px;
    font-size: 11px;
    color: var(--blocked);
  }
  .ac-editbtn {
    margin-left: auto;
    border: 1px solid var(--border-2);
    background: var(--panel-2);
    color: var(--muted);
    font: 600 10.5px/1 var(--sans);
    padding: 4px 9px;
    border-radius: 6px;
  }
  .ac-editbtn:hover {
    color: var(--text);
    border-color: var(--faint);
  }
  .ac-editor {
    border-top: 1px solid var(--border);
    padding-top: 11px;
    display: flex;
    flex-direction: column;
    gap: 9px;
  }
  .ac-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ac-field label {
    font: 700 9px/1 var(--mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--faint);
  }
  .ac-field input[type='text'] {
    font: 600 12.5px/1 var(--sans);
    color: var(--text);
    background: var(--panel-2);
    border: 1px solid var(--border-2);
    border-radius: 6px;
    padding: 6px 8px;
  }
  .ac-swatches {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .ac-swatch {
    width: 20px;
    height: 20px;
    border-radius: 6px;
    border: 2px solid transparent;
    cursor: pointer;
    padding: 0;
  }
  .ac-swatch.sel {
    border-color: var(--text);
  }
  .ac-imgslot {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 9px;
    border: 1.5px dashed var(--border-2);
    border-radius: 8px;
    color: var(--faint);
    font-size: 11.5px;
  }
  .ac-imgslot .box {
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: var(--panel-3);
    display: grid;
    place-items: center;
    font-size: 13px;
    flex: 0 0 auto;
  }
  .ac-note {
    font-size: 11px;
    color: var(--faint);
    margin: 0;
  }
</style>
