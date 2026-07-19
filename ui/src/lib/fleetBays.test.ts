import { describe, expect, it } from 'vitest';
import { buildBays, fleetVitals } from './fleetBays';
import type { Agent } from './types';

function agent(overrides: Partial<Agent> = {}): Agent {
  return {
    id: 'jarvis',
    display: 'Jarvis',
    desc: null,
    expectedHost: 'pop-os',
    lastTs: null,
    lastHost: 'pop-os',
    live: true,
    verified: 'signed',
    liveness: 'live',
    version: null,
    watchState: null,
    keyFingerprint: null,
    profileMarkdown: null,
    color: '#7dcfff',
    abbr: 'JA',
    wip: [],
    ...overrides,
  };
}

describe('buildBays', () => {
  it('groups agents by their real expectedHost', () => {
    const bays = buildBays([agent({ id: 'jarvis', expectedHost: 'pop-os' }), agent({ id: 'herald', expectedHost: 'Batman.local', display: 'Herald' })]);
    expect(bays.map((b) => b.host).sort()).toEqual(['Batman.local', 'pop-os']);
  });

  it('falls back to lastHost when expectedHost is unknown, then "unknown"', () => {
    const bays = buildBays([agent({ expectedHost: null, lastHost: 'Athena.local' })]);
    expect(bays[0]!.host).toBe('Athena.local');
    const noHost = buildBays([agent({ expectedHost: null, lastHost: null })]);
    expect(noHost[0]!.host).toBe('unknown');
  });

  it('a bay is dark ONLY when every occupant is really down — a real fact, not a separate flag', () => {
    const mixedBay = buildBays([
      agent({ id: 'a', expectedHost: 'shared-host', liveness: 'live' }),
      agent({ id: 'b', expectedHost: 'shared-host', liveness: 'down' }),
    ]);
    expect(mixedBay[0]!.dark).toBe(false);

    const allDown = buildBays([agent({ expectedHost: 'dead-host', liveness: 'down' })]);
    expect(allDown[0]!.dark).toBe(true);
  });

  it('dark bays sort last, healthy bays sort by host name', () => {
    const bays = buildBays([
      agent({ id: 'a', expectedHost: 'zzz-host', liveness: 'live' }),
      agent({ id: 'b', expectedHost: 'aaa-dead', liveness: 'down' }),
      agent({ id: 'c', expectedHost: 'aaa-host', liveness: 'live' }),
    ]);
    expect(bays.map((b) => b.host)).toEqual(['aaa-host', 'zzz-host', 'aaa-dead']);
  });
});

describe('fleetVitals', () => {
  it('counts real liveness/trust across the whole fleet, and real machine count from the bays', () => {
    const agents = [
      agent({ id: 'a', expectedHost: 'h1', liveness: 'live', trust: 'signed' }),
      agent({ id: 'b', expectedHost: 'h2', liveness: 'down', trust: 'signed' }),
      agent({ id: 'c', expectedHost: 'h2', liveness: 'stale', trust: 'mismatch' }),
    ];
    const bays = buildBays(agents);
    expect(fleetVitals(agents, bays)).toEqual({ agentCount: 3, liveCount: 1, downCount: 1, machineCount: 2, unsignedCount: 1 });
  });

  it('an empty fleet is honestly all zeros', () => {
    expect(fleetVitals([], [])).toEqual({ agentCount: 0, liveCount: 0, downCount: 0, machineCount: 0, unsignedCount: 0 });
  });
});
