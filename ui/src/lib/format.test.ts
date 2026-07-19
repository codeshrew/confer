import { describe, expect, it } from 'vitest';
import {
  ageColorVar,
  agePct,
  formatAge,
  formatAgeFromSecs,
  formatClock,
  formatDayDivider,
  formatIso8601,
  formatIsoDate,
  formatLocalDateTime,
  groupByDay,
  isStaleAge,
} from './format';

// formatClock/formatIsoDate render the operator's LOCAL time — these
// assertions read as "UTC" only because vite.config.ts pins the test
// runner's TZ to UTC (see its own comment), making local === UTC here. A
// real machine running this suite is NOT UTC (MDT, confirmed 2026-07-18);
// without the pin these would be timezone-dependent and flaky.

describe('formatAge', () => {
  it('renders "—" for a null timestamp', () => {
    expect(formatAge(null)).toBe('—');
  });

  it('renders minutes/hours/days relative to a fixed "now"', () => {
    const now = new Date('2026-07-17T15:00:00Z').getTime();
    expect(formatAge('2026-07-17T14:58:00Z', now)).toBe('2m');
    expect(formatAge('2026-07-17T13:00:00Z', now)).toBe('2h');
    expect(formatAge('2026-07-15T15:00:00Z', now)).toBe('2d');
  });

  it('clamps a future timestamp (clock skew) to 0, not a negative age', () => {
    const now = new Date('2026-07-17T15:00:00Z').getTime();
    expect(formatAge('2026-07-17T15:05:00Z', now)).toBe('0m');
  });
});

describe('formatAgeFromSecs', () => {
  it('stays in minutes just under the 60-minute boundary', () => {
    expect(formatAgeFromSecs(59 * 60)).toBe('59m');
  });

  it('flips to hours exactly at 60 minutes', () => {
    expect(formatAgeFromSecs(60 * 60)).toBe('1h');
  });

  it('flips to days exactly at 24 hours', () => {
    expect(formatAgeFromSecs(24 * 60 * 60)).toBe('1d');
  });

  it('never renders a negative minute count', () => {
    expect(formatAgeFromSecs(-30)).toBe('0m');
  });
});

describe('isStaleAge', () => {
  it('a null timestamp is always stale (no heartbeat at all)', () => {
    expect(isStaleAge(null)).toBe(true);
  });

  it('is not stale at/under the 10-minute threshold', () => {
    const now = new Date('2026-07-17T15:00:00Z').getTime();
    expect(isStaleAge('2026-07-17T14:50:00Z', now)).toBe(false); // exactly 600s
  });

  it('is stale just past the 10-minute threshold', () => {
    const now = new Date('2026-07-17T15:00:00Z').getTime();
    expect(isStaleAge('2026-07-17T14:49:59Z', now)).toBe(true); // 601s
  });
});

describe('agePct', () => {
  it('never drops below the 6% floor, even for a brand-new (0s) request', () => {
    expect(agePct(0)).toBe(6);
  });

  it('grows with sqrt(age), not linearly', () => {
    const p1 = agePct(60 * 60); // 1 hour
    const p2 = agePct(4 * 60 * 60); // 4 hours (4x the age)
    // sqrt relationship: 4x age -> ~2x pct-above-floor, not 4x.
    expect(p2).toBeGreaterThan(p1);
    expect(p2).toBeLessThan(p1 * 3);
  });

  it('reaches 100% at the mockup\'s own normalization constant (agev === 7200 minutes, i.e. 5 days)', () => {
    expect(agePct(7200 * 60)).toBe(100);
  });

  it('keeps growing (no clamp) past the 5-day mark — pct is unbounded above 100', () => {
    expect(agePct(10 * 7200 * 60)).toBeGreaterThan(100);
  });
});

describe('formatClock', () => {
  it('renders the LOCAL HH:MM, zero-padded (== UTC under the pinned test TZ)', () => {
    expect(formatClock('2026-07-17T09:05:00Z')).toBe('09:05');
    expect(formatClock('2026-07-17T23:59:00Z')).toBe('23:59');
    expect(formatClock('2026-07-17T00:00:00Z')).toBe('00:00');
  });
});

describe('formatIsoDate', () => {
  it('renders the LOCAL YYYY-MM-DD, zero-padded (== UTC under the pinned test TZ)', () => {
    expect(formatIsoDate('2026-07-17T09:05:00Z')).toBe('2026-07-17');
    expect(formatIsoDate('2026-01-05T23:59:00Z')).toBe('2026-01-05');
  });
});

describe('formatLocalDateTime', () => {
  it('renders a readable date + time + zone label', () => {
    // en-US Intl output under UTC: "Jul 17, 2026, 9:05 AM UTC" — assert on
    // the stable substrings rather than the whole string, since exact
    // Intl punctuation/spacing can vary by ICU version.
    const label = formatLocalDateTime('2026-07-17T09:05:00Z');
    expect(label).toContain('Jul 17, 2026');
    expect(label).toContain('9:05');
    expect(label).toContain('UTC');
  });
});

describe('formatIso8601', () => {
  it('renders the full instant, seconds precision, Z-suffixed', () => {
    expect(formatIso8601('2026-07-17T09:05:00Z')).toBe('2026-07-17T09:05:00Z');
  });

  it('drops sub-second precision if the source timestamp carried milliseconds', () => {
    expect(formatIso8601('2026-07-17T09:05:00.123Z')).toBe('2026-07-17T09:05:00Z');
  });
});

describe('formatDayDivider', () => {
  const now = new Date('2026-07-17T15:00:00Z').getTime();

  it('labels the current LOCAL day "Today", alongside its ISO date', () => {
    expect(formatDayDivider('2026-07-17', now)).toBe('Today · 2026-07-17');
  });

  it('labels the previous LOCAL day "Yesterday", alongside its ISO date', () => {
    expect(formatDayDivider('2026-07-16', now)).toBe('Yesterday · 2026-07-16');
  });

  it('renders just the ISO date for anything older than yesterday', () => {
    expect(formatDayDivider('2026-07-15', now)).toBe('2026-07-15');
    expect(formatDayDivider('2026-06-01', now)).toBe('2026-06-01');
  });
});

describe('groupByDay', () => {
  it('buckets messages by LOCAL calendar day, preserving order within a day', () => {
    const messages = [
      { id: 'a', ts: '2026-07-16T10:00:00Z' },
      { id: 'b', ts: '2026-07-16T11:00:00Z' },
      { id: 'c', ts: '2026-07-17T09:00:00Z' },
    ];
    const groups = groupByDay(messages);
    expect(groups).toHaveLength(2);
    expect(groups[0]).toEqual({ day: '2026-07-16', messages: [messages[0], messages[1]] });
    expect(groups[1]).toEqual({ day: '2026-07-17', messages: [messages[2]] });
  });

  it('starts a new bucket every time the day changes, even if it repeats later', () => {
    // Same day appearing twice, non-contiguously, still yields two buckets —
    // grouping walks the stream in order, it does not sort/merge by day.
    const messages = [
      { id: 'a', ts: '2026-07-16T10:00:00Z' },
      { id: 'b', ts: '2026-07-17T09:00:00Z' },
      { id: 'c', ts: '2026-07-16T23:00:00Z' },
    ];
    const groups = groupByDay(messages);
    expect(groups.map((g) => g.day)).toEqual(['2026-07-16', '2026-07-17', '2026-07-16']);
  });

  it('returns an empty array for no messages', () => {
    expect(groupByDay([])).toEqual([]);
  });
});

describe('ageColorVar', () => {
  it('is always "blocked" (red) when stale, regardless of age', () => {
    expect(ageColorVar(0, true)).toBe('var(--blocked)');
    expect(ageColorVar(10 * 60, true)).toBe('var(--blocked)');
  });

  it('is "blocked" once age passes 24h, even if not flagged stale', () => {
    expect(ageColorVar(25 * 60 * 60, false)).toBe('var(--blocked)');
  });

  it('is "claimed" (blue) in the 2h-24h band', () => {
    expect(ageColorVar(3 * 60 * 60, false)).toBe('var(--claimed)');
  });

  it('is "accent" for anything fresh (< 2h) and not stale', () => {
    expect(ageColorVar(5 * 60, false)).toBe('var(--accent)');
  });

  it('boundary: exactly 120 minutes is "claimed", exactly 1440 minutes is "blocked"', () => {
    expect(ageColorVar(120 * 60, false)).toBe('var(--claimed)');
    expect(ageColorVar(1440 * 60, false)).toBe('var(--blocked)');
  });
});
