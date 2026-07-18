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

  it('picking a color swatch in an agent\'s editor live-updates that card\'s avatar background/color, independent of other agents', async () => {
    const user = userEvent.setup();
    const { container } = render(Fleet, { agents: [reader, orbit], hubName: 'agent-coord' });

    const editButtons = screen.getAllByText('Customize identity');
    await user.click(editButtons[1]!); // reader's editor ([0] is "You")

    const editor = container.querySelector('[data-testid="editor-reader"]') as HTMLElement;
    const swatches = editor.querySelectorAll('.ac-swatch');
    expect(swatches.length).toBeGreaterThan(0);

    await user.click(swatches[2]!); // any non-default swatch

    const readerCard = container.querySelectorAll('.agentcard:not(.you)')[0] as HTMLElement;
    const orbitCard = container.querySelectorAll('.agentcard:not(.you)')[1] as HTMLElement;
    // The clicked swatch is now marked selected...
    expect(swatches[2]!.classList.contains('sel')).toBe(true);
    // ...and only reader's avatar style changed, not orbit's (each override is keyed by agent id).
    expect(readerCard.querySelector('.ac-av')?.getAttribute('style')).toContain(
      swatches[2]!.getAttribute('style')!.replace('background:', '')
    );
    expect(orbitCard.querySelector('.ac-av')?.getAttribute('style')).not.toEqual(
      readerCard.querySelector('.ac-av')?.getAttribute('style')
    );
  });

  it('picking a color swatch in the "You" editor updates the You card only', async () => {
    const user = userEvent.setup();
    const { container } = render(Fleet, { agents: [reader], hubName: 'agent-coord' });

    await user.click(screen.getAllByText('Customize identity')[0]!); // "You"'s editor
    const editor = container.querySelector('[data-testid="editor-you"]') as HTMLElement;
    const swatches = editor.querySelectorAll('.ac-swatch');

    const youCardStyleBefore = container.querySelector('.agentcard.you .ac-av')?.getAttribute('style');
    await user.click(swatches[1]!);
    const youCardStyleAfter = container.querySelector('.agentcard.you .ac-av')?.getAttribute('style');

    expect(youCardStyleAfter).not.toBe(youCardStyleBefore);
    expect(swatches[1]!.classList.contains('sel')).toBe(true);
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
