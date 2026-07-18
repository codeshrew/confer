import { describe, expect, it } from 'vitest';
import { selectDefaultHub, selectDefaultTopic } from './hydrate';
import type { Hub, Overview, Topic } from './types';

function hub(id: string, current: boolean): Hub {
  return { id, label: id, name: id, current, agentCount: 1 };
}

function topic(slug: string, messages: number): Topic {
  return { slug, messages, open: 0, requests: 0, status: 'open', stale: false, lastTs: null };
}

function overviewOf(topics: Topic[]): Overview {
  return {
    hub: hub('x', true),
    topics,
    board: { requests: [], open: 0, claimed: 0, blocked: 0, backlog: 0, closed: 0 },
    fleet: [],
  };
}

describe('selectDefaultHub', () => {
  it('picks the hub marked current, regardless of position', () => {
    const hubs = [hub('a/one', false), hub('b/two', true), hub('c/three', false)];
    expect(selectDefaultHub(hubs)?.id).toBe('b/two');
  });

  it('falls back to the first hub when none is marked current', () => {
    const hubs = [hub('a/one', false), hub('b/two', false)];
    expect(selectDefaultHub(hubs)?.id).toBe('a/one');
  });

  it('returns null for an empty hub list', () => {
    expect(selectDefaultHub([])).toBeNull();
  });
});

describe('selectDefaultTopic', () => {
  it('picks the topic with the most messages (real-API-shaped overview)', () => {
    const ov = overviewOf([
      topic('about', 1),
      topic('general', 157),
      topic('markup-print', 6),
    ]);
    expect(selectDefaultTopic(ov)).toBe('general');
  });

  it('picks the busiest topic even when it is not literally "general" (mock-shaped overview)', () => {
    const ov = overviewOf([
      topic('general', 3),
      topic('reader', 7),
      topic('studio', 4),
      topic('studio-markup', 5),
      topic('plate-pipeline', 2),
      topic('scratch', 0),
    ]);
    expect(selectDefaultTopic(ov)).toBe('reader');
  });

  it('falls back to the first topic when none has messages yet', () => {
    const ov = overviewOf([topic('scratch', 0), topic('empty-too', 0)]);
    expect(selectDefaultTopic(ov)).toBe('scratch');
  });

  it('returns null when the hub has no topics, or overview is null', () => {
    expect(selectDefaultTopic(overviewOf([]))).toBeNull();
    expect(selectDefaultTopic(null)).toBeNull();
  });
});
