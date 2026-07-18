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

/** Render an ISO timestamp's calendar date (UTC) as `YYYY-MM-DD`. */
export function formatIsoDate(ts: string): string {
  const d = new Date(ts);
  const yyyy = String(d.getUTCFullYear()).padStart(4, '0');
  const mm = String(d.getUTCMonth() + 1).padStart(2, '0');
  const dd = String(d.getUTCDate()).padStart(2, '0');
  return `${yyyy}-${mm}-${dd}`;
}

/**
 * Render the full ISO8601 instant (UTC, seconds precision, `Z`-suffixed) for
 * a message timestamp — the hover/tap-away detail behind the compact
 * `formatClock` label and the day divider.
 */
export function formatIso8601(ts: string): string {
  const d = new Date(ts);
  return d.toISOString().replace(/\.\d{3}Z$/, 'Z');
}

/**
 * The day-divider label for a calendar day: "Today"/"Yesterday" (plus the
 * ISO date alongside, so it's never ambiguous) for the two most recent days,
 * else just the ISO date. Both the divider's day and "now" are compared as
 * UTC calendar days (matching `formatIsoDate`'s UTC convention) so the
 * divider lines up with the date it's grouping, regardless of the viewer's
 * own timezone.
 */
export function formatDayDivider(dayIso: string, nowMs: number = Date.now()): string {
  const today = formatIsoDate(new Date(nowMs).toISOString());
  if (dayIso === today) return `Today · ${dayIso}`;
  const yesterday = formatIsoDate(new Date(nowMs - 24 * 60 * 60 * 1000).toISOString());
  if (dayIso === yesterday) return `Yesterday · ${dayIso}`;
  return dayIso;
}

/**
 * Groups a chronologically-sorted (ascending) message stream into per-day
 * buckets, keyed by each message's UTC calendar day (`formatIsoDate`). A new
 * bucket starts whenever the day changes going down the stream — the
 * ChatStream divider renders one per bucket, in order.
 */
export function groupByDay<T extends { ts: string }>(messages: T[]): { day: string; messages: T[] }[] {
  const groups: { day: string; messages: T[] }[] = [];
  for (const message of messages) {
    const day = formatIsoDate(message.ts);
    const last = groups[groups.length - 1];
    if (last && last.day === day) {
      last.messages.push(message);
    } else {
      groups.push({ day, messages: [message] });
    }
  }
  return groups;
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
