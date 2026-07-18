import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import Fleet from './Fleet.svelte';
import type { Agent } from '../types';

const reader: Agent = {
  id: 'reader',
  display: 'Reader',
  desc: 'reader',
  expectedHost: 'reader',
  lastTs: '2026-07-17T14:56:00Z',
  lastHost: 'reader',
  live: true,
  verified: 'signed',
  color: 'var(--ag-reader)',
  abbr: 'RE',
  wip: [{ id: 'req_01JQ8f2', summary: 'plate-bundle endpoint', status: 'DONE' }],
};

const orbit: Agent = {
  id: 'orbit',
  display: 'Orbit',
  desc: 'orbit',
  expectedHost: 'orbit',
  lastTs: '2020-01-01T00:00:00Z',
  lastHost: 'orbit',
  live: false,
  verified: 'unverified',
  color: 'var(--ag-orbit)',
  abbr: 'OR',
  wip: [],
};

describe('Fleet', () => {
  it('renders a "You" card plus an agent-identity card per agent, with WIP', () => {
    render(Fleet, { agents: [reader, orbit], hubName: 'agent-coord' });

    expect(screen.getByText('You')).toBeInTheDocument();
    expect(screen.getByText('Reader')).toBeInTheDocument();
    expect(screen.getByText('Orbit')).toBeInTheDocument();
    expect(screen.getByText('plate-bundle endpoint')).toBeInTheDocument();
    expect(screen.getByText('no active claims')).toBeInTheDocument();
  });

  it('warns on an unverified/stale agent', () => {
    const { container } = render(Fleet, { agents: [orbit], hubName: 'agent-coord' });

    expect(container.querySelector('.agentcard.stale')).toBeInTheDocument();
    expect(screen.getByText(/unverified peer/)).toBeInTheDocument();
  });

  it('drives the stale/live heartbeat indicator from agent.live, not lastTs age', () => {
    const { container } = render(Fleet, { agents: [reader, orbit], hubName: 'agent-coord' });

    // reader: live: true (despite an old-ish lastTs) -> NOT stale, shows "live"
    const cards = container.querySelectorAll('.agentcard:not(.you)');
    const readerCard = cards[0] as HTMLElement;
    const orbitCard = cards[1] as HTMLElement;

    expect(readerCard.classList.contains('stale')).toBe(false);
    expect(readerCard.querySelector('.ac-hb')?.textContent).toMatch(/^live/);
    expect(readerCard.querySelector('.ac-hb')?.textContent).toMatch(/last posted/);

    // orbit: live: false -> stale, shows "heartbeat stale"
    expect(orbitCard.classList.contains('stale')).toBe(true);
    expect(orbitCard.querySelector('.ac-hb')?.textContent).toMatch(/^heartbeat stale/);

    // header count reflects !live agents, not lastTs-age staleness
    expect(screen.getByText('2 agents · 1 stale heartbeat')).toBeInTheDocument();
  });

  it('has no "Customize identity" editing affordance anywhere in the view (identity is read-only)', () => {
    const { container } = render(Fleet, { agents: [reader, orbit], hubName: 'agent-coord' });

    expect(screen.queryByText('Customize identity')).not.toBeInTheDocument();
    expect(container.querySelector('.ac-editor')).not.toBeInTheDocument();
    expect(container.querySelector('input[type="text"]')).not.toBeInTheDocument();
    expect(container.querySelector('.ac-swatch')).not.toBeInTheDocument();
    expect(container.querySelector('.ac-editbtn')).not.toBeInTheDocument();
  });

  it('surfaces an informational note that appearance is agent-declared and not editable here', () => {
    render(Fleet, { agents: [reader], hubName: 'agent-coord' });

    expect(
      screen.getByText(/appearance is self-declared.*set by the agent, not editable from here/i)
    ).toBeInTheDocument();
  });

  it('renders identity (avatar color/abbr, display name, host) read-only for both "You" and peer agents', () => {
    const { container } = render(Fleet, { agents: [reader], hubName: 'agent-coord' });

    const youCard = container.querySelector('.agentcard.you') as HTMLElement;
    expect(youCard.querySelector('.ac-nm')?.textContent).toBe('You');
    expect(youCard.querySelector('.ac-av')?.textContent).toBe('◉');

    const readerCard = container.querySelector('.agentcard:not(.you)') as HTMLElement;
    expect(readerCard.querySelector('.ac-nm')?.textContent).toBe('Reader');
    expect(readerCard.querySelector('.ac-av')?.textContent).toBe('RE');
    expect(readerCard.querySelector('.ac-av')?.getAttribute('style')).toContain('var(--ag-reader)');
    expect(readerCard.querySelector('.ac-host')?.textContent).toBe('reader');
  });

  it('shows the "first-sight" verify glyph (distinct from "unverified") without a stale-peer warning line', () => {
    const firstSight: Agent = { ...orbit, id: 'newbie', display: 'Newbie', verified: 'first-sight', live: true };
    const { container } = render(Fleet, { agents: [firstSight], hubName: 'agent-coord' });

    const badge = container.querySelector('.ac-verify') as HTMLElement;
    expect(badge.classList.contains('warn')).toBe(true);
    expect(badge.getAttribute('title')).toMatch(/first-sight/);
    // The "unverified peer" warning line is specific to verified === 'unverified', not first-sight.
    expect(screen.queryByText(/unverified peer/)).not.toBeInTheDocument();
  });

  it('shows the "signed" verify glyph as ok, with no warning line', () => {
    const { container } = render(Fleet, { agents: [reader], hubName: 'agent-coord' });

    const badge = container.querySelector('.ac-verify') as HTMLElement;
    expect(badge.classList.contains('ok')).toBe(true);
    expect(badge.textContent).toBe('✓');
    expect(screen.queryByText(/unverified peer/)).not.toBeInTheDocument();
  });

  it('falls back to "—" for host when neither lastHost nor expectedHost is set', () => {
    const noHost: Agent = { ...reader, id: 'ghost', lastHost: null, expectedHost: null };
    const { container } = render(Fleet, { agents: [noHost], hubName: 'agent-coord' });

    expect(container.querySelector('.agentcard:not(.you) .ac-host')?.textContent).toBe('—');
  });
});
