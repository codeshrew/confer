// Realistic fixtures matching the API contract in ./types.ts, derived from
// design/serve-dashboard-v2-mockup.html's content: the fleet (Herald/gitconv,
// Reader, Pipeline/studio, Compositor/studio-markup, Jarvis, Orbit), the
// hubs (agent-coord, confer-lab, jarvis-orbit), the #reader plate-bundle
// ticket story, the blocked cross-topic meta-thread, and the code refs to
// wealdlore:Sources/Reader/PlateBundle.swift#L44-49.
//
// `mockApi` implements the same interface as `./api.ts`'s ConferApi so dev
// mode (or `?mock`) can run the whole dashboard with no backend.

import type { MessagesOpts } from './api';
import type {
  Agent,
  CodeFile,
  CodeRef,
  Hub,
  Message,
  Overview,
  RefHit,
  Repo,
  RequestRow,
  ServerEvent,
  Snippet,
  ThreadNode,
  Topic,
} from './types';

// design/48 §2-3 — real per-hub tier + sync fixtures, one of each shape so
// the dashboard's honest-degradation paths actually get exercised in dev
// mode (not just the all-fields-present happy path): agent-coord is a
// healthy `own` hub; confer-lab is `shared` but genuinely behind (the
// warn-styling case); jarvis-orbit is `foreign` with an unprobed `pending`
// (the field-level "unknown", not the whole-`sync`-null, case).
export const mockHubs: Hub[] = [
  {
    id: 'agent-coord',
    label: 'agent-coord',
    name: 'agent-coord',
    current: true,
    agentCount: 6,
    tier: 'own',
    sync: { lastFetchedSecs: 12, behind: 0, pending: 0, reachable: true },
  },
  {
    id: 'confer-lab',
    label: 'confer-lab',
    name: 'confer-lab',
    current: false,
    agentCount: 2,
    tier: 'shared',
    sync: { lastFetchedSecs: 940, behind: 2, pending: 0, reachable: true },
  },
  {
    id: 'jarvis-orbit',
    label: 'jarvis-orbit',
    name: 'jarvis-orbit',
    current: false,
    agentCount: 1,
    tier: 'foreign',
    sync: { lastFetchedSecs: 65, behind: 0, pending: null, reachable: true },
  },
];

// Mirrors the real fleet's hub `repos/*.md` inventory: three repos cloned
// on this machine (git-conversations/book-business/agent-coord), one not
// (wealdlore — access-restricted to reader/gitconv, no local clone mapped).
export const mockRepos: Repo[] = [
  {
    slug: 'agent-coord',
    role: 'hub',
    url: 'git@github.com:codeshrew/agent-coord.git',
    access: [],
    docs: 'shared/',
    owner: 'sk',
    cloned: true,
    clonePath: '/Users/sk/git/agent-coord-studio-markup',
    rootSha: null,
  },
  {
    slug: 'book-business',
    role: 'code',
    url: 'https://github.com/codeshrew/book-business.git',
    access: [],
    docs: 'docs/',
    owner: 'sk',
    cloned: true,
    clonePath: '/Users/sk/git/book-business',
    rootSha: null,
  },
  {
    slug: 'git-conversations',
    role: 'tooling',
    url: 'git@github.com:codeshrew/git-conversations.git',
    access: [],
    docs: 'design/',
    owner: 'sk',
    cloned: true,
    clonePath: '/Users/sk/git/git-conversations',
    rootSha: null,
  },
  {
    slug: 'wealdlore',
    role: 'code',
    url: 'git@github.com:codeshrew/wealdlore.git',
    access: ['reader', 'gitconv'],
    docs: 'docs/',
    owner: 'sk',
    cloned: false,
    clonePath: null,
    rootSha: null,
  },
];

export const mockTopics: Topic[] = [
  { slug: 'general', messages: 3, open: 0, requests: 0, status: 'discussion', stale: false, lastTs: '2026-07-17T11:40:00Z' },
  { slug: 'reader', messages: 7, open: 1, requests: 1, status: 'open', stale: false, lastTs: '2026-07-17T14:55:00Z' },
  { slug: 'studio', messages: 4, open: 2, requests: 2, status: 'open', stale: false, lastTs: '2026-07-17T14:52:00Z' },
  { slug: 'studio-markup', messages: 5, open: 0, requests: 1, status: 'discussion', stale: false, lastTs: '2026-07-17T14:31:00Z' },
  { slug: 'plate-pipeline', messages: 2, open: 1, requests: 1, status: 'open', stale: true, lastTs: '2026-07-15T09:00:00Z' },
  { slug: 'scratch', messages: 0, open: 0, requests: 0, status: 'open', stale: false, lastTs: null },
];

const plateBundleRef: CodeRef = {
  repo: 'wealdlore',
  path: 'Sources/Reader/PlateBundle.swift',
  sha: 'a3f1c9',
  range: [44, 49],
  contentHash: 'sha256:9f2c...e01a',
  refName: 'main',
  refType: 'branch',
  commitDate: '2026-07-17T14:40:00Z',
  dirty: false,
  untracked: false,
  baseRef: null,
  forkPoint: null,
};

const restoreChainRef: CodeRef = {
  repo: 'wealdlore',
  path: 'pipeline/plates.py',
  sha: 'b7e2a4',
  range: [88, 102],
  contentHash: 'sha256:4b1d...77f3',
  refName: 'restore-chain',
  refType: 'branch',
  commitDate: '2026-07-16T09:12:00Z',
  dirty: true,
  untracked: false,
  baseRef: 'main',
  forkPoint: 'f00dfeed1234567890abcdef1234567890abcdef',
};

export const mockAgents: Agent[] = [
  {
    id: 'herald',
    display: 'Herald',
    desc: 'gitconv',
    expectedHost: 'gitconv',
    lastTs: '2026-07-17T14:57:00Z',
    lastHost: 'gitconv',
    live: true,
    verified: 'signed',
    // piece 8b (src/api.rs's agent_row_json, Herald commit 32ef9a4) — a
    // live/signed agent's watch is genuinely armed, and its trust-gated
    // beat carries a real version + its verified card a real fingerprint.
    version: '0.7.3 (a3f1c9d)',
    watchState: 'armed',
    keyFingerprint: 'SHA256:l064aRMg7xJ3nQvKp2wZ8fThYbNcMdEeRtUvWxYzAbC',
    // A real roles/herald.md body (Herald, src/api.rs commit 1318664) —
    // `desc` above stays the short frontmatter slug; this is the actual
    // card prose, sanitized server-side like a message body and rendered
    // through the same markdown pipeline as one.
    profileMarkdown:
      '## Herald\n\nGit integrity + PR review lane. Watches repo state across the fleet — clone status, reference density, shadow-repo detection — and signs every card it posts.\n\n**Owns:** `confer` repo housekeeping, PR gates, integrity audits.\n**Won\'t:** merge without a green build, or speak for a repo it hasn\'t cloned.',
    color: 'var(--ag-herald)',
    abbr: 'HE',
    wip: [],
  },
  {
    id: 'reader',
    display: 'Reader',
    desc: 'reader',
    expectedHost: 'reader',
    lastTs: '2026-07-17T14:56:00Z',
    lastHost: 'reader',
    live: true,
    verified: 'signed',
    version: '0.7.3 (a3f1c9d)',
    watchState: 'armed',
    keyFingerprint: 'SHA256:9fK2mNpQ4rS6tUvWxYzAbCdEfGhIjKlMnOpQrStUvWx',
    // No roles/reader.md body written yet — the dossier's About falls back
    // to the one-line `desc` above rather than showing nothing.
    profileMarkdown: null,
    color: 'var(--ag-reader)',
    abbr: 'RE',
    wip: [{ id: 'req_01JQ8f2', summary: 'plate-bundle endpoint', status: 'DONE' }],
  },
  {
    id: 'pipeline',
    display: 'Pipeline',
    desc: 'studio',
    expectedHost: 'studio',
    lastTs: '2026-07-17T14:55:00Z',
    lastHost: 'studio',
    live: true,
    verified: 'signed',
    version: '0.7.3 (a3f1c9d)',
    watchState: 'armed',
    keyFingerprint: 'SHA256:aB3cD5eF7gH9iJkLmNoPqRsTuVwXyZ1aB3cD5eF7gH9',
    profileMarkdown:
      '## Pipeline\n\nPlate-restoration pipeline agent on `studio`. Owns the `/plate-bundle` and alignment endpoints Reader depends on.\n\n**Owns:** plate ingest, alignment passes, bundle assembly.\n**Won\'t:** ship an endpoint without Reader\'s shape sign-off.',
    color: 'var(--ag-pipeline)',
    abbr: 'PI',
    wip: [{ id: 'req_01JQa91', summary: 'alignment pass', status: 'CLAIMED' }],
  },
  {
    id: 'compositor',
    display: 'Compositor',
    desc: 'studio-markup',
    expectedHost: 'studio-markup',
    lastTs: '2026-07-17T14:53:00Z',
    lastHost: 'studio-markup',
    live: true,
    verified: 'signed',
    // A trailing confer build — still live/armed/signed, just not on the
    // exact same build as the rest of the fleet (a real, honest fact the
    // dossier's version row is FOR: catching this at a glance).
    version: '0.7.2 (f00dfee)',
    watchState: 'armed',
    keyFingerprint: 'SHA256:qR7sT9uV1wX3yZ5aB7cD9eF1gH3iJ5kL7mN9oP1qR3s',
    profileMarkdown: null,
    color: 'var(--ag-compositor)',
    abbr: 'CO',
    wip: [{ id: 'req_01JQc4a', summary: 'CSL schema decision', status: 'BLOCKED' }],
  },
  {
    id: 'jarvis',
    display: 'Jarvis',
    desc: 'jarvis',
    expectedHost: 'jarvis',
    lastTs: '2026-07-17T14:58:00Z',
    lastHost: 'jarvis',
    live: true,
    verified: 'first-sight',
    // first-sight still means a real, cryptographically-valid signed beat
    // — just the first time this key's been seen — so it still carries a
    // real version/fingerprint; only unverified/mismatch/no-card nulls
    // the fingerprint (see orbit below).
    version: '0.7.3 (a3f1c9d)',
    watchState: 'armed',
    keyFingerprint: 'SHA256:zY8xW6vU4tS2rQ0pO8nM6lK4jI2hG0fE8dC6bA4zY2x',
    profileMarkdown: null,
    color: 'var(--ag-jarvis)',
    abbr: 'JA',
    wip: [],
  },
  {
    id: 'orbit',
    display: 'Orbit',
    desc: 'orbit',
    expectedHost: 'orbit',
    lastTs: '2026-07-17T14:45:00Z',
    lastHost: 'orbit',
    live: false,
    verified: 'unverified',
    // down (no trusted beat) + unverified (no confirmed card) — every new
    // field is honestly null, not guessed: no basis for a version, no
    // basis for an armed/idle watch state, no confirmed key to fingerprint.
    // profileMarkdown isn't trust-gated, but this fixture simply has no
    // card body written — the About falls back to `desc` like reader's.
    version: null,
    watchState: null,
    keyFingerprint: null,
    profileMarkdown: null,
    color: 'var(--ag-orbit)',
    abbr: 'OR',
    wip: [{ id: 'req_01JQe88', summary: 'plate-pipeline follow-up', status: 'OPEN' }],
  },
];

export const mockMessages: Message[] = [
  {
    id: 'msg_01JQ001',
    from: 'herald',
    type: 'note',
    ts: '2026-07-17T14:02:00Z',
    host: 'gitconv',
    to: ['all'],
    cc: [],
    topic: 'reader',
    summary: 'Shipping confer 0.7.3',
    body: 'Shipping confer 0.7.3 — @all the `serve --all-hubs` broken-tab fix is in. Restart your watch to adopt.',
    of: null,
    replyTo: null,
    supersedes: null,
    refs: [],
    seenBy: [
      { role: 'reader', ts: '2026-07-17T14:03:10Z' },
      { role: 'pipeline', ts: '2026-07-17T14:04:00Z' },
      { role: 'compositor', ts: '2026-07-17T14:10:00Z' },
    ],
  },
  {
    id: 'msg_01JQ8f2',
    from: 'pipeline',
    type: 'request',
    ts: '2026-07-17T14:08:00Z',
    host: 'studio',
    to: ['reader'],
    cc: [],
    topic: 'reader',
    summary: 'Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader',
    body: 'Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader.',
    of: null,
    replyTo: null,
    supersedes: null,
    refs: [],
    seenBy: [{ role: 'reader', ts: '2026-07-17T14:09:00Z' }],
  },
  {
    id: 'msg_01JQa10',
    from: 'reader',
    type: 'claim',
    ts: '2026-07-17T14:11:00Z',
    host: 'reader',
    to: ['pipeline'],
    cc: [],
    topic: 'reader',
    summary: 'claimed req_01JQ8f2',
    body: 'Reader claimed this request.',
    of: 'msg_01JQ8f2',
    replyTo: 'msg_01JQ8f2',
    supersedes: null,
    refs: [],
    seenBy: [{ role: 'pipeline', ts: '2026-07-17T14:12:00Z' }],
  },
  {
    id: 'msg_01JQc4a',
    from: 'reader',
    type: 'blocked',
    ts: '2026-07-17T14:20:00Z',
    host: 'reader',
    to: ['pipeline'],
    cc: [],
    topic: 'reader',
    summary: 'blocked on uid-spine contract',
    body: '@Pipeline blocked — I need the uid-spine contract frozen before I can shape the response. Referencing Compositor’s note in #studio-markup.',
    of: 'msg_01JQ8f2',
    replyTo: 'msg_01JQ8f2',
    supersedes: null,
    refs: [],
    seenBy: [{ role: 'pipeline', ts: '2026-07-17T14:21:00Z' }],
  },
  {
    id: 'msg_01JQd55',
    from: 'compositor',
    type: 'note',
    ts: '2026-07-17T14:31:00Z',
    host: 'studio-markup',
    to: ['reader'],
    cc: [],
    topic: 'studio-markup',
    summary: 'uid spine v2 frozen',
    body: '@Reader uid spine v2 is frozen — anchors stable. Doc: `--ref studio:docs/uid-spine.md@a3f1`. You’re unblocked.',
    of: null,
    replyTo: 'msg_01JQc4a',
    supersedes: null,
    refs: [],
    seenBy: [{ role: 'reader', ts: '2026-07-17T14:32:00Z' }],
  },
  {
    id: 'msg_01JQd60',
    from: 'reader',
    type: 'claim',
    ts: '2026-07-17T14:31:30Z',
    host: 'reader',
    to: ['pipeline'],
    cc: [],
    topic: 'reader',
    summary: 're-claimed req_01JQ8f2 — unblocked by Compositor',
    body: 'Re-claiming — unblocked by Compositor’s frozen uid spine.',
    of: 'msg_01JQ8f2',
    replyTo: 'msg_01JQd55',
    supersedes: null,
    refs: [],
    seenBy: [],
  },
  {
    id: 'msg_01JQe73',
    from: 'reader',
    type: 'note',
    ts: '2026-07-17T14:46:00Z',
    host: 'reader',
    to: ['pipeline', 'compositor'],
    cc: [],
    topic: 'reader',
    summary: 'bundle assembly wired, pinned to a3f1c9',
    body: 'Wired it — bundle assembly is here, pinned so we’re all looking at the exact same code: `--ref wealdlore:Sources/Reader/PlateBundle.swift@a3f1c9`',
    of: null,
    replyTo: 'msg_01JQd55',
    supersedes: null,
    refs: [plateBundleRef],
    seenBy: [],
  },
  {
    id: 'msg_01JQf01',
    from: 'pipeline',
    type: 'note',
    ts: '2026-07-17T14:52:00Z',
    host: 'studio',
    to: [],
    cc: [],
    topic: 'reader',
    summary: 'restore chain context',
    body: 'For context, the restore chain these regions feed into — bigger, so it comes in collapsed:',
    of: null,
    replyTo: 'msg_01JQe73',
    supersedes: null,
    refs: [restoreChainRef],
    seenBy: [],
  },
  {
    id: 'msg_01JQe80',
    from: 'reader',
    type: 'done',
    ts: '2026-07-17T14:48:00Z',
    host: 'reader',
    to: ['pipeline'],
    cc: [],
    topic: 'reader',
    summary: 'endpoint live, tests green',
    body: 'Marking req_01JQ8f2 done — endpoint live, tests green.',
    of: 'msg_01JQ8f2',
    replyTo: 'msg_01JQe73',
    supersedes: null,
    refs: [],
    seenBy: [],
  },
  {
    id: 'msg_01JQg44',
    from: 'jarvis',
    type: 'note',
    ts: '2026-07-17T14:55:00Z',
    host: 'jarvis',
    to: ['all'],
    cc: [],
    topic: 'reader',
    summary: 'canaried 0.7.3',
    body: '@all nice. Canaried 0.7.3 on pop-os — clean. Bundle regions render correctly in the reader.',
    of: null,
    replyTo: null,
    supersedes: null,
    refs: [],
    seenBy: [],
  },
];

export const mockRequests: RequestRow[] = [
  {
    id: 'req_01JQ8f2',
    from: 'pipeline',
    to: ['reader'],
    summary: 'Wire up /plate-bundle/:uid — restored plate + regions JSON for the reader',
    status: 'DONE',
    resolution: 'endpoint live, tests green',
    deferred: false,
    claimants: ['reader'],
    ageSecs: 2400,
    stale: false,
    topic: 'reader',
  },
  {
    id: 'req_01JQa91',
    from: 'pipeline',
    to: ['pipeline'],
    summary: 'Run the alignment pass over the restored plate set',
    status: 'CLAIMED',
    resolution: null,
    deferred: false,
    claimants: ['pipeline'],
    ageSecs: 3600,
    stale: false,
    topic: 'studio',
  },
  {
    id: 'req_01JQc4a',
    from: 'reader',
    to: ['compositor'],
    summary: 'Freeze the CSL schema — needs a decision from Herald',
    status: 'BLOCKED',
    resolution: null,
    deferred: false,
    claimants: ['compositor'],
    ageSecs: 7200,
    stale: false,
    topic: 'studio-markup',
  },
  {
    id: 'req_01JQd21',
    from: 'compositor',
    to: ['all'],
    summary: 'Should we stream regions instead of assembling the full bundle?',
    status: 'OPEN',
    resolution: null,
    deferred: true,
    claimants: [],
    ageSecs: 10800,
    stale: false,
    topic: 'studio-markup',
  },
  {
    id: 'req_01JQe88',
    from: 'pipeline',
    to: ['pipeline'],
    summary: 'Revisit eager per-region loop once plate counts exceed 200',
    status: 'OPEN',
    resolution: null,
    deferred: false,
    claimants: [],
    ageSecs: 172800,
    stale: true,
    topic: 'plate-pipeline',
  },
];

export const mockOverview: Overview = {
  hub: mockHubs[0]!,
  topics: mockTopics,
  board: {
    requests: mockRequests,
    open: mockRequests.filter((r) => r.status === 'OPEN').length,
    claimed: mockRequests.filter((r) => r.status === 'CLAIMED').length,
    blocked: mockRequests.filter((r) => r.status === 'BLOCKED').length,
    backlog: mockRequests.filter((r) => r.deferred).length,
    closed: mockRequests.filter((r) => r.status === 'DONE').length,
  },
  fleet: mockAgents,
};

// ── design/47 — per-hub Overview fixtures for the cross-hub Overview view ──
// Every OTHER view only ever asks for the CURRENT hub's overview, so one
// shared fixture (mockOverview above) was enough. Overview.svelte's
// `getAttention()` fans out across ALL of mockHubs, so it needs each hub to
// actually differ — otherwise every lane would just triple the same agent-
// coord story. `confer-lab` exercises the loudest alarm (a key mismatch —
// §2.2 Lane 1's "single loudest thing on the screen") plus a stale-claimed
// request; `jarvis-orbit` is deliberately calm (all signed, all live, no
// stuck requests) so the aggregate view — and that hub's own mini-rollup —
// has a genuine "all clear" case to render, not just a synthetic one.

function overviewOf(hubId: string, fleet: Overview['fleet'], requests: RequestRow[]): Overview {
  const found = mockHubs.find((h) => h.id === hubId) ?? mockHubs[0]!;
  return {
    hub: found,
    topics: [],
    board: {
      requests,
      open: requests.filter((r) => r.status === 'OPEN').length,
      claimed: requests.filter((r) => r.status === 'CLAIMED').length,
      blocked: requests.filter((r) => r.status === 'BLOCKED').length,
      backlog: requests.filter((r) => r.deferred).length,
      closed: requests.filter((r) => r.status === 'DONE').length,
    },
    fleet,
  };
}

const mockConferLabOverview: Overview = overviewOf(
  'confer-lab',
  [
    {
      id: 'sentinel',
      display: 'Sentinel',
      desc: 'confer-lab',
      expectedHost: 'lab-01',
      lastTs: '2026-07-18T09:40:00Z',
      lastHost: 'lab-02',
      live: true,
      verified: 'unverified',
      trust: 'mismatch',
      // A key MISMATCH is exactly the untrusted case the honest-nullable
      // rule exists for — no version/watch/fingerprint claimed for an
      // identity we can't vouch for, regardless of how fresh its beat is.
      version: null,
      watchState: null,
      keyFingerprint: null,
      profileMarkdown: null,
      color: 'var(--ag-jarvis)',
      abbr: 'SE',
      wip: [],
    },
    {
      id: 'herald',
      display: 'Herald',
      desc: 'gitconv',
      expectedHost: 'gitconv',
      lastTs: '2026-07-18T09:58:00Z',
      lastHost: 'gitconv',
      live: true,
      verified: 'signed',
      version: '0.7.3 (a3f1c9d)',
      watchState: 'armed',
      keyFingerprint: 'SHA256:l064aRMg7xJ3nQvKp2wZ8fThYbNcMdEeRtUvWxYzAbC',
      profileMarkdown: null,
      color: 'var(--ag-herald)',
      abbr: 'HE',
      wip: [{ id: 'req_02LAB1', summary: 'review the patch', status: 'CLAIMED' }],
    },
  ],
  [
    {
      id: 'req_02LAB1',
      from: 'sentinel',
      to: ['herald'],
      summary: 'review the patch',
      status: 'CLAIMED',
      resolution: null,
      deferred: false,
      claimants: ['herald'],
      ageSecs: 3 * 24 * 3600,
      stale: true,
      topic: 'code-review',
    },
  ]
);

const mockJarvisOrbitOverview: Overview = overviewOf(
  'jarvis-orbit',
  [
    {
      id: 'orbit',
      display: 'Orbit',
      desc: 'orbit',
      expectedHost: 'orbit',
      lastTs: '2026-07-18T09:59:30Z',
      lastHost: 'orbit',
      live: true,
      verified: 'signed',
      version: '0.7.3 (a3f1c9d)',
      watchState: 'armed',
      keyFingerprint: 'SHA256:mN4oP6qR8sT0uV2wX4yZ6aB8cD0eF2gH4iJ6kL8mN0o',
      profileMarkdown: null,
      color: 'var(--ag-orbit)',
      abbr: 'OR',
      wip: [],
    },
  ],
  []
);

const MOCK_OVERVIEW_BY_HUB: Record<string, Overview> = {
  'agent-coord': mockOverview,
  'confer-lab': mockConferLabOverview,
  'jarvis-orbit': mockJarvisOrbitOverview,
};

// `?mock&clear` (or, since dev mode defaults to mock, just `?clear` while
// developing) swaps in a single fully-nominal hub — the Overview view's
// "everything's fine" state, exercised by e2e/overview.spec.ts without
// needing a scenario the default fixture (which deliberately has plenty
// wrong) can't produce.
function isClearScenario(): boolean {
  if (typeof window === 'undefined') return false;
  return new URLSearchParams(window.location.search).has('clear');
}

const mockClearHubs: Hub[] = [
  {
    id: 'jarvis-orbit',
    label: 'jarvis-orbit',
    name: 'jarvis-orbit',
    current: true,
    agentCount: 1,
    tier: 'own',
    sync: { lastFetchedSecs: 8, behind: 0, pending: 0, reachable: true },
  },
];

const mockThread: ThreadNode[] = [
  { msgId: 'msg_01JQ8f2', from: 'pipeline', type: 'request', topic: 'reader', summary: 'Filed the plate-bundle ticket', refs: [] },
  { msgId: 'msg_01JQa10', from: 'reader', type: 'claim', topic: 'reader', summary: 'Claimed it — taking the endpoint', refs: [] },
  { msgId: 'msg_01JQc4a', from: 'reader', type: 'blocked', topic: 'reader', summary: '“need the uid spine frozen” — references Compositor’s note', refs: [] },
  { msgId: 'msg_01JQd55', from: 'compositor', type: 'note', topic: 'studio-markup', summary: 'uid spine v2 frozen — anchors stable, unblocks Reader', refs: [] },
  { msgId: 'msg_01JQe73', from: 'reader', type: 'done', topic: 'reader', summary: 'Endpoint live, tests green', refs: [plateBundleRef] },
];

const mockRefHits: RefHit[] = [
  {
    repo: 'wealdlore',
    path: 'Sources/Reader/PlateBundle.swift',
    sha: 'a3f1c9',
    range: [44, 49],
    contentHash: plateBundleRef.contentHash,
    staleness: 'current',
    msgId: 'msg_01JQe73',
    from: 'reader',
    msgType: 'note',
    ts: '2026-07-17T14:46:00Z',
    topic: 'reader',
    summary: 'plate-bundle endpoint — the request these lines shipped for',
    threadRoot: 'msg_01JQ8f2',
    requestStatus: 'DONE',
    hub: 'agent-coord',
    hubPrivate: false,
    refName: 'main',
    refType: 'branch',
    commitDate: '2026-07-17T14:40:00Z',
    dirty: false,
    untracked: false,
    baseRef: null,
    forkPoint: null,
  },
  {
    repo: 'wealdlore',
    path: 'Sources/Reader/PlateBundle.swift',
    sha: 'a3f1c9',
    range: [44, 49],
    contentHash: plateBundleRef.contentHash,
    staleness: 'current',
    msgId: 'msg_01JQf01',
    from: 'pipeline',
    msgType: 'note',
    ts: '2026-07-17T14:52:00Z',
    topic: 'studio',
    summary: 'bundle assembly perf — eager assembly is fine at current plate counts',
    threadRoot: 'msg_01JQf01',
    requestStatus: null,
    hub: 'agent-coord',
    hubPrivate: false,
    refName: 'main',
    refType: 'branch',
    commitDate: '2026-07-17T14:40:00Z',
    dirty: false,
    untracked: false,
    baseRef: null,
    forkPoint: null,
  },
  {
    repo: 'wealdlore',
    path: 'Sources/Reader/PlateBundle.swift',
    sha: 'a3f1c9',
    range: [44, 49],
    contentHash: plateBundleRef.contentHash,
    staleness: 'current',
    msgId: 'msg_01JQh12',
    from: 'compositor',
    msgType: 'note',
    ts: '2026-07-10T09:00:00Z',
    topic: 'design',
    summary: 'why not stream the regions? — private design rationale, deliberately kept out of the code comments',
    threadRoot: 'msg_01JQh12',
    requestStatus: null,
    hub: 'wealdlore-internal',
    hubPrivate: true,
    refName: 'main',
    refType: 'branch',
    commitDate: '2026-07-17T14:40:00Z',
    dirty: false,
    untracked: false,
    baseRef: null,
    forkPoint: null,
  },
  // design/44 Phase 2 — exercises the working-tree-snapshot warning chip
  // (`--allow-dirty`'s captured `dirty`/`untracked` flags) on a feature
  // branch that hasn't merged yet (`refType: 'branch'`, non-null `refName`).
  {
    repo: 'wealdlore',
    path: 'pipeline/plates.py',
    sha: 'b7e2a4',
    range: [88, 102],
    contentHash: restoreChainRef.contentHash,
    staleness: 'changed',
    msgId: 'msg_01JQf02',
    from: 'pipeline',
    msgType: 'note',
    ts: '2026-07-16T09:15:00Z',
    topic: 'studio',
    summary: 'restore chain — snapshot taken with a dirty tree, see embedded fence',
    threadRoot: 'msg_01JQf02',
    requestStatus: null,
    hub: 'agent-coord',
    hubPrivate: false,
    refName: 'restore-chain',
    refType: 'branch',
    commitDate: '2026-07-16T09:12:00Z',
    dirty: true,
    untracked: false,
    baseRef: 'main',
    forkPoint: 'f00dfeed1234567890abcdef1234567890abcdef',
  },
  // A legacy pre-44 ref: `sha: HEAD` was the only pointer confer ever
  // captured before Phase 1 — no branch/date/dirty metadata exists for it,
  // so every temporal field is null and staleness reads `unpinned`.
  {
    repo: 'wealdlore',
    path: 'Sources/Reader/PlateBundle.swift',
    sha: 'HEAD',
    range: null,
    contentHash: null,
    staleness: 'unpinned',
    msgId: 'msg_01JQb00',
    from: 'reader',
    msgType: 'note',
    ts: '2026-06-02T10:00:00Z',
    topic: 'reader',
    summary: 'early note, pinned before confer captured a real sha (pre-0.6.9)',
    threadRoot: 'msg_01JQb00',
    requestStatus: null,
    hub: 'agent-coord',
    hubPrivate: false,
    refName: null,
    refType: null,
    commitDate: null,
    dirty: false,
    untracked: false,
    baseRef: null,
    forkPoint: null,
  },
  // A squashed-away feature branch (Addendum 2): the pinned sha is no longer
  // reachable from HEAD (the branch's commits were squash-merged), but
  // `baseRef`/`forkPoint` still resolve — "merged/squashed away, forked from
  // main@<forkPoint>" instead of a bare, unhelpful Off-line.
  {
    repo: 'wealdlore',
    path: 'pipeline/plates.py',
    sha: 'deadbeef00112233445566778899aabbccddeeff',
    range: [12, 20],
    contentHash: null,
    staleness: 'squashed',
    msgId: 'msg_01JQb10',
    from: 'compositor',
    msgType: 'note',
    ts: '2026-06-20T11:00:00Z',
    topic: 'studio-markup',
    summary: 'discussed before the alignment-pass branch got squash-merged into main',
    threadRoot: 'msg_01JQb10',
    requestStatus: null,
    hub: 'agent-coord',
    hubPrivate: false,
    refName: 'alignment-pass',
    refType: 'branch',
    commitDate: '2026-06-19T08:30:00Z',
    dirty: false,
    untracked: false,
    baseRef: 'main',
    forkPoint: 'cafebabe0011223344556677889900aabbccddee',
  },
  // Off-line: rebased away / abandoned side branch — the sha simply isn't
  // reachable from HEAD anymore, and there's no base_ref/fork_point to
  // recover it (the discard case Addendum 2 calls out as unrecoverable).
  {
    repo: 'wealdlore',
    path: 'Sources/Reader/PlateBundle.swift',
    sha: 'ab00cd11ef22334455667788990011aabbccddee',
    range: [10, 15],
    contentHash: null,
    staleness: 'offline',
    msgId: 'msg_01JQb20',
    from: 'jarvis',
    msgType: 'note',
    ts: '2026-06-25T13:00:00Z',
    topic: 'reader',
    summary: 'reference to a since-discarded experimental branch',
    threadRoot: 'msg_01JQb20',
    requestStatus: null,
    hub: 'agent-coord',
    hubPrivate: false,
    refName: null,
    refType: 'detached',
    commitDate: '2026-06-25T12:50:00Z',
    dirty: false,
    untracked: false,
    baseRef: null,
    forkPoint: null,
  },
];

const mockSnippet: Snippet = {
  lang: 'swift',
  staleness: 'current',
  lines: [
    { n: 44, text: 'func assembleBundle(uid: UID) throws -> PlateBundle {' },
    { n: 45, text: '  let plate = try store.restoredPlate(uid)' },
    { n: 46, text: '  let regions = try store.regions(for: uid)   // uid spine v2' },
    { n: 47, text: '  let cites = citations.tieIns(regions)' },
    { n: 48, text: '  return PlateBundle(plate, regions, cites)' },
    { n: 49, text: '}' },
  ],
};

// Mirrors the two code refs already threaded through mockMessages/mockRefHits
// (plateBundleRef/restoreChainRef), plus a THIRD, deliberately unmapped file
// (citations.py — no local clone, same "access-restricted, no clone mapped"
// story wealdlore already tells in mockRepos) so dev/tests exercise both the
// mapped and unmapped dot/empty-state without a backend.
export const mockCodeFiles: CodeFile[] = [
  { repo: 'wealdlore', path: 'Sources/Reader/PlateBundle.swift', refCount: 2, mapped: true, lastTs: '2026-07-17T14:46:00Z' },
  { repo: 'wealdlore', path: 'pipeline/plates.py', refCount: 1, mapped: true, lastTs: '2026-07-17T14:52:00Z' },
  { repo: 'wealdlore', path: 'studio-markup/citations.py', refCount: 1, mapped: false, lastTs: '2026-07-10T09:00:00Z' },
  // piece 7 (ui/REDESIGN.md) — a genuine SHADOW repo: --ref'd by real
  // messages but never registered in this hub's inventory (mockRepos has
  // no 'openjarvis' entry). Closes the loop on the Board's own
  // "Shadow-repo surfacing" ticket fixture (req_01JQb22).
  { repo: 'openjarvis', path: 'internal/brain/router.go', refCount: 2, mapped: false, lastTs: '2026-07-16T10:00:00Z' },
];

function delay<T>(value: T, ms = 40): Promise<T> {
  return new Promise((resolve) => setTimeout(() => resolve(value), ms));
}

export const mockApi = {
  async getHubs(): Promise<Hub[]> {
    return delay(isClearScenario() ? mockClearHubs : mockHubs);
  },
  async getOverview(hub: string): Promise<Overview> {
    if (isClearScenario()) return delay(mockJarvisOrbitOverview);
    return delay(MOCK_OVERVIEW_BY_HUB[hub] ?? { ...mockOverview, hub: mockHubs.find((h) => h.id === hub) ?? mockHubs[0]! });
  },
  async getMessages(_hub: string, topic?: string, opts?: MessagesOpts): Promise<Message[]> {
    // Mirrors the real backend's /api/messages semantics (src/api.rs's
    // `messages()`): sort chronologically, filter by topic, then `before`
    // (strictly older than that id), then `limit` (keep the most-recent
    // `limit` of what's left). Ids are ULIDs — ascending string sort ==
    // chronological, so `<` string-compares work the same as on the server.
    let msgs = [...mockMessages].sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
    if (topic) msgs = msgs.filter((m) => m.topic === topic);
    if (opts?.before) msgs = msgs.filter((m) => m.id < opts.before!);
    if (opts?.limit !== undefined && msgs.length > opts.limit) {
      msgs = msgs.slice(msgs.length - opts.limit);
    }
    return delay(msgs);
  },
  async getThread(_hub: string, _id: string): Promise<ThreadNode[]> {
    return delay(mockThread);
  },
  async getRefs(_hub: string, _target: string, _allHubs?: boolean): Promise<RefHit[]> {
    return delay(mockRefHits);
  },
  async getCode(_hub: string, _repo: string, _path: string, _sha: string, _range?: string): Promise<Snippet> {
    return delay(mockSnippet);
  },
  async getRepos(_hub: string): Promise<Repo[]> {
    return delay(mockRepos);
  },
  async getCodeFiles(_hub: string): Promise<CodeFile[]> {
    return delay(mockCodeFiles);
  },
  subscribeEvents(
    _hub: string,
    onEvent: (event: ServerEvent) => void,
    onStatus: (status: 'live' | 'reconnecting') => void
  ): () => void {
    // Mock mode has no real transport to lose — it's "live" the instant
    // something subscribes, same as a freshly-opened EventSource.
    onStatus('live');
    const timer = setInterval(() => {
      onEvent({ event: 'ping' });
    }, 15000);
    return () => clearInterval(timer);
  },
};
