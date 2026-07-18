import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
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

  it('the customize-identity editor live-updates the avatar (display name + abbreviation)', async () => {
    const user = userEvent.setup();
    const { container } = render(Fleet, { agents: [reader], hubName: 'agent-coord' });

    const editButtons = screen.getAllByText('Customize identity');
    await user.click(editButtons[1]!); // [0] is "You"'s editor button

    const editor = container.querySelector('[data-testid="editor-reader"]') as HTMLElement;
    expect(editor).toBeInTheDocument();

    const nameInput = editor.querySelector('input[id="reader-nm"]') as HTMLInputElement;
    await user.clear(nameInput);
    await user.type(nameInput, 'Bookworm');

    const card = container.querySelector('.agentcard:not(.you)') as HTMLElement;
    expect(card.querySelector('.ac-nm')?.textContent).toBe('Bookworm');

    const abbrInput = editor.querySelector('input[id="reader-abbr"]') as HTMLInputElement;
    await user.clear(abbrInput);
    await user.type(abbrInput, 'bw');

    expect(card.querySelector('.ac-av')?.textContent).toBe('BW');
  });
});
