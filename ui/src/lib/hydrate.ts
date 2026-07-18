// Pure selection helpers for hydrating app state from the real API's shape.
//
// The frontend used to hardcode mock defaults (hub 'agent-coord', topic
// 'reader') as the *initial* app state. Those ids don't exist on a real
// confer serve backend (real hub ids look like 'codeshrew/agent-coord'), so
// the dashboard would fetch /api/overview?hub=agent-coord, miss, and render
// empty. These helpers compute the same kind of "sensible default" from
// whatever the API actually returned, so there is nothing to hardcode.

import type { Hub, Overview } from './types';

/** The hub marked `current: true`, else the first hub, else null if there are none. */
export function selectDefaultHub(hubs: Hub[]): Hub | null {
  if (hubs.length === 0) return null;
  return hubs.find((h) => h.current) ?? hubs[0]!;
}

/**
 * The most active topic in the overview — the one with the most messages
 * (ties keep the first one in `topics` order). Falls back to the first
 * topic if none has any messages yet, or null if the hub has no topics.
 *
 * This happens to pick 'general' on the real backend (it's the busiest
 * topic there) and 'reader' against mock.ts's fixtures (also the busiest
 * there) — "most active" rather than a hardcoded slug, so it degrades
 * sensibly no matter what topics a hub actually has.
 */
export function selectDefaultTopic(overview: Overview | null): string | null {
  if (!overview || overview.topics.length === 0) return null;
  let best = overview.topics[0]!;
  for (const t of overview.topics) {
    if (t.messages > best.messages) best = t;
  }
  return best.slug;
}
