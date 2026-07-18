// Small formatting helpers shared across components — relative ages, ports of
// the mockup's `agePct`/`ageCol` sparkbar math (design/serve-dashboard-v2-mockup.html).

/** Render a short relative-age label ("2m", "1h", "3d") from an ISO timestamp. */
export function formatAge(ts: string | null, nowMs: number = Date.now()): string {
  if (!ts) return '—';
  const deltaMs = Math.max(0, nowMs - new Date(ts).getTime());
  return formatAgeFromSecs(deltaMs / 1000);
}

/** Render a short relative-age label ("12m", "1h", "2d") from a duration in seconds. */
export function formatAgeFromSecs(ageSecs: number): string {
  const mins = ageSecs / 60;
  if (mins < 60) return `${Math.max(0, Math.round(mins))}m`;
  const hours = mins / 60;
  if (hours < 24) return `${Math.round(hours)}h`;
  const days = hours / 24;
  return `${Math.round(days)}d`;
}

/** Whether an agent heartbeat should render as "stale" (mockup: >~10m). */
export function isStaleAge(ts: string | null, nowMs: number = Date.now()): boolean {
  if (!ts) return true;
  const deltaSecs = Math.max(0, nowMs - new Date(ts).getTime()) / 1000;
  return deltaSecs > 600;
}

/**
 * The board's age-sparkbar fill percentage — ported verbatim from the
 * mockup's `agePct(t) = max(6, round(sqrt(agev/7200)*100))`, where `agev` was
 * minutes there; here we take seconds directly (ageSecs / 60 = agev).
 */
export function agePct(ageSecs: number): number {
  const agev = ageSecs / 60;
  return Math.max(6, Math.round(Math.sqrt(agev / 7200) * 100));
}

/** Render an ISO timestamp as a local "HH:MM" clock label, as the mockup does. */
export function formatClock(ts: string): string {
  const d = new Date(ts);
  const hh = String(d.getUTCHours()).padStart(2, '0');
  const mm = String(d.getUTCMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

/**
 * The board's age-sparkbar color — ported from the mockup's `ageCol`:
 * stale or >=24h -> blocked (warm/red), >=2h -> claimed (blue), else accent.
 */
export function ageColorVar(ageSecs: number, stale: boolean): string {
  const agev = ageSecs / 60;
  if (stale || agev >= 1440) return 'var(--blocked)';
  if (agev >= 120) return 'var(--claimed)';
  return 'var(--accent)';
}
