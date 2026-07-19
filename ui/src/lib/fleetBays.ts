// Piece 8a (ui/REDESIGN.md, `08-fleet-crew-deck.html`) — "agents are
// processes that live on machines... grouping by machine is the honest
// topology." Pure fold, same convention as repoIndex.ts/boardStats.ts.
import type { Agent } from './types';
import { deriveLiveness, deriveTrust } from './attention';

export interface FleetBay {
  host: string;
  agents: Agent[];
  /** A bay whose agents are ALL down reads dimmed, with a "dark" power
   * indicator — a real fact (every occupant's real liveness), not a
   * separate flag to keep in sync. */
  dark: boolean;
}

/** Groups by `expectedHost` (the declared home — stable even if the agent
 * last posted from somewhere unusual) falling back to `lastHost`, then
 * `'unknown'` if neither is known. Healthy bays first, dark bays last (a
 * dark bay still needs to be findable, just not competing for attention
 * with the live fleet). */
export function buildBays(agents: Agent[]): FleetBay[] {
  const byHost = new Map<string, Agent[]>();
  for (const agent of agents) {
    const host = agent.expectedHost ?? agent.lastHost ?? 'unknown';
    const arr = byHost.get(host) ?? [];
    arr.push(agent);
    byHost.set(host, arr);
  }
  const bays: FleetBay[] = [...byHost.entries()].map(([host, hostAgents]) => ({
    host,
    agents: [...hostAgents].sort((a, b) => a.display.localeCompare(b.display)),
    dark: hostAgents.every((a) => deriveLiveness(a) === 'down'),
  }));
  bays.sort((a, b) => {
    if (a.dark !== b.dark) return a.dark ? 1 : -1;
    return a.host.localeCompare(b.host);
  });
  return bays;
}

export interface FleetVitals {
  agentCount: number;
  liveCount: number;
  downCount: number;
  machineCount: number;
  unsignedCount: number;
}

export function fleetVitals(agents: Agent[], bays: FleetBay[]): FleetVitals {
  let liveCount = 0;
  let downCount = 0;
  let unsignedCount = 0;
  for (const agent of agents) {
    const liveness = deriveLiveness(agent);
    if (liveness === 'live') liveCount++;
    if (liveness === 'down') downCount++;
    if (deriveTrust(agent) !== 'signed') unsignedCount++;
  }
  return { agentCount: agents.length, liveCount, downCount, machineCount: bays.length, unsignedCount };
}
