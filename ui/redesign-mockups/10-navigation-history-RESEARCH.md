# Navigation History & Back-Stack Recommendation
**confer dashboard** — Svelte 5 SPA embedded in Rust binary

**Date:** 2026-07-19  
**Status:** Research & Architecture Recommendation  
**Symptom:** Fleet view → click agent → AgentDossier opens → click ticket → TicketFullPopover replaces dossier, no way back.

---

## Problem Statement

The dashboard's navigation model is **stateless per popover**. Five boolean flags control five overlays independently:

```svelte
let ticketPopoverOpen = $state(false);
let dossierOpen = $state(false);
let focusReaderOpen = $state(false);
let notePopoverOpen = $state(false);
let whichKeyOpen = $state(false);
```

Opening one popover does NOT close the others cleanly — they replace each other visually (CSS `.active` toggling), but context is **lost**:
- Dossier → click ticket → Dossier context evaporates
- User presses `esc` → closes current overlay, lands nowhere
- **No URL encoding** → reload loses everything
- **No deep-linking** → can't send a link to a specific popover state

This is the symptom of a missing **overlay back-stack**: a navigation history structure that lets overlays nest and unwind one at a time.

---

## Solution Evaluation Table

| Option | Back-Stack Support | Deep-Link + Reload? | Svelte 5 + No-Server Fit | Maintenance | Bundle Cost | Notes |
|--------|:--:|:--:|:--:|:--:|:--:|---------|
| **Hand-Rolled Overlay Stack** (this recommendation) | ✅ Full | ✅ Yes (URL sync) | ✅ Excellent | N/A — in-repo | ~500 bytes | Svelte 5 runes, History API, custom |
| **Browser History API only** | ⚠️ Partial | ✅ Yes | ✅ Excellent | N/A | 0 bytes | Raw `pushState/popstate`; no framework overhead, requires manual stack logic |
| `@keenmate/svelte-spa-router` v5.3.0 | ✅ Via nested routers | ✅ Yes | ✅ Good | Actively maintained | ~8–12 KB | Full SPA router; overkill for overlay stack, but viable if view-level routing is added later |
| `svelte-spa-router` (original) | ✅ Via routes | ❌ No (hash-only) | ❌ Svelte 4, no runes | Inactive | ~7 KB | Do not use — Svelte 5 incompatible |
| `XState` v5 + `@xstate/svelte` | ✅ Via actors/states | ⚠️ Manual | ⚠️ No examples | Maintained | ~15–20 KB | Powerful for complex state machines; overkill for overlay stack |
| **URL query params only** (no History API) | ❌ None | ✅ Reload via URL | ✅ Works | N/A | 0 bytes | No back button support; refresh-centric UX |

### Rationale for Recommendation: Hand-Rolled Overlay Stack

The **hand-rolled Svelte 5 runes + History API** approach wins because:

1. **Perfect fit for the constraint** — The Rust binary's serve handler falls back to the SPA for unknown paths:
   ```rust
   // serve.rs line 544–551
   // Everything else → the embedded SPA
   let mut response = tiny_http::Response::from_string(DASHBOARD)...
   ```
   This means **History API (clean URLs) is viable** — no server rewrites needed.

2. **Minimal bundle cost** — A custom overlay-stack store is ~500 bytes (Svelte 5 $state, no deps). Full routers add 8–20 KB.

3. **Exact control over UX** — Pop the stack on `esc`, customize the back affordance, choose which overlays join the stack and which don't (e.g., `whichKeyOpen` can stay a simple toggle).

4. **Incremental adoption** — Existing boolean flags stay; new overlays migrate to the stack one at a time. No rip-and-replace.

5. **No framework churn** — A focused in-repo solution won't break on Svelte 6 or router lib rewrites.

---

## Recommended Architecture

### Store Shape: `overlayStack.svelte.ts`

```typescript
export interface OverlayFrame {
  id: string; // Unique identifier: 'agent-dossier', 'ticket', 'note', etc.
  type: 'popover' | 'drawer'; // Visual category (popover vs drawer behavior)
  data?: Record<string, string | number | null>; // Context: agentId, ticketId, etc.
}

function createOverlayStack() {
  let stack = $state<OverlayFrame[]>([]);
  let isInitializing = $state(true); // Suppress redundant pushState during hydration
  
  return {
    get stack(): OverlayFrame[] {
      return stack;
    },
    get top(): OverlayFrame | null {
      return stack[stack.length - 1] ?? null;
    },
    push(frame: OverlayFrame): void {
      stack = [...stack, frame];
      this.syncToUrl();
    },
    replace(frame: OverlayFrame): void {
      if (stack.length === 0) {
        this.push(frame);
      } else {
        stack[stack.length - 1] = frame;
        this.syncToUrl();
      }
    },
    pop(): void {
      if (stack.length > 0) {
        stack = stack.slice(0, -1);
        this.syncToUrl();
      }
    },
    clear(): void {
      stack = [];
      this.syncToUrl();
    },
    syncToUrl(): void {
      if (isInitializing) return;
      const encoded = encodeStackToUrl(stack);
      const url = new URL(window.location.href);
      url.hash = encoded; // or: url.pathname = `/overlays/${encoded}` for clean URLs
      window.history.pushState({ stack }, '', url.toString());
    },
    hydrateFromUrl(): void {
      const decoded = decodeStackFromUrl(window.location.hash);
      stack = decoded;
      isInitializing = false;
    },
  };
}

export const overlayStack = createOverlayStack();

// URL codecs (example: hash-based; can switch to clean URLs/pathname later)
function encodeStackToUrl(stack: OverlayFrame[]): string {
  if (stack.length === 0) return '';
  return stack
    .map(f => `${f.id}:${Object.entries(f.data ?? {}).map(([k, v]) => `${k}=${v}`).join(',')}`)
    .join('/');
}

function decodeStackFromUrl(hash: string): OverlayFrame[] {
  if (!hash || hash === '#') return [];
  const path = hash.slice(1); // Remove leading '#'
  return path.split('/').map(segment => {
    const [id, dataStr] = segment.split(':');
    const data: Record<string, string | number | null> = {};
    if (dataStr) {
      dataStr.split(',').forEach(pair => {
        const [k, v] = pair.split('=');
        data[k] = isNaN(+v) ? v : +v;
      });
    }
    return { id, type: 'popover', data };
  });
}
```

### URL Examples

**Hash-based** (simplest, works immediately):
- `http://localhost:8422/#` — no overlays
- `http://localhost:8422/#agent-dossier:agentId=alice` — dossier open
- `http://localhost:8422/#agent-dossier:agentId=alice/ticket:ticketId=req_123` — dossier + nested ticket

**Clean URLs** (requires no server changes — already supported by serve.rs):
- `http://localhost:8422/` — no overlays
- `http://localhost:8422/overlays/agent-dossier/agentId/alice` — dossier
- `http://localhost:8422/overlays/agent-dossier/agentId/alice/ticket/ticketId/req_123` — nested

---

## Migration Sketch: From Booleans to Stack

### Phase 1: Add the Store (No Changes to App.svelte)

1. Create `ui/src/lib/overlayStack.svelte.ts` with the structure above
2. Initialize in `App.svelte`'s `onMount`:
   ```typescript
   onMount(() => {
     overlayStack.hydrateFromUrl();
   });
   ```

### Phase 2: Wire Existing Overlays to the Stack

For `AgentDossier` → `TicketFullPopover` sequence:

**Before** (lines 502–507 in App.svelte):
```typescript
let dossierOpen = $state(false);
let dossierAgentId = $state<string | null>(null);
function openAgentDossier(agentId: string) {
  dossierAgentId = agentId;
  dossierOpen = true; // ← Replaces old context
}
```

**After**:
```typescript
function openAgentDossier(agentId: string) {
  overlayStack.push({ id: 'agent-dossier', type: 'popover', data: { agentId } });
}

// In the markup, replace the boolean check:
{#if overlayStack.top?.id === 'agent-dossier'}
  <AgentDossier
    agentId={overlayStack.top.data?.agentId}
    onClose={() => overlayStack.pop()}
    onSelectTicket={(ticketId) => overlayStack.push({ id: 'ticket', type: 'popover', data: { ticketId } })}
  />
{/if}
```

### Phase 3: Keyboard Navigation

Add to App.svelte's keyboard handler:

```typescript
$effect(() => {
  const handleEsc = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && overlayStack.top) {
      overlayStack.pop();
      e.preventDefault();
    }
  };
  document.addEventListener('keydown', handleEsc);
  return () => document.removeEventListener('keydown', handleEsc);
});
```

### Phase 4: Selective Migration

**Don't** migrate everything at once. Keep these as simple toggles (they're not nested overlays):
- `whichKeyOpen` — help overlay (top-level, no nesting)
- `focusReaderOpen` — peek reader (side pane, independent)

Migrate these (they **are** nested overlays):
- `dossierOpen` + `ticketPopoverOpen` → single "agent-dossier + ticket" stack
- `notePopoverOpen` → part of the stack

---

## Implementation Notes

### URL Encoding Strategy

Choose **one**:

1. **Hash-based** (`#agent-dossier:agentId=alice/ticket:ticketId=req_123`):
   - ✅ Works immediately, no server changes needed
   - ✅ Simplest encoding
   - ❌ Less aesthetically clean

2. **Clean URLs via History API** (`/overlays/agent-dossier/agentId/alice/ticket/ticketId/req_123`):
   - ✅ Beautiful URLs
   - ✅ Still supported by serve.rs (falls back to index.html)
   - ❌ URL parser is more complex
   - **Recommendation:** Start with hash, migrate to clean URLs later if desired

### Back-Button Behavior

The browser's **back button** must work out-of-the-box once `syncToUrl()` calls `history.pushState()`. No extra wiring needed.

**Test**: Dossier → Ticket → click browser back → returns to Dossier.

### Deep-Linking

A shared link `http://localhost:8422/#agent-dossier:agentId=alice/ticket:ticketId=req_123` will:
1. Load the page
2. `onMount` calls `overlayStack.hydrateFromUrl()`
3. Stack is reconstructed from URL
4. Overlays render in order

The user lands exactly where the link specified.

### Reload Preservation

Refresh the page → URL is unchanged → `overlayStack.hydrateFromUrl()` reconstructs the stack → overlays reappear. No loss.

---

## Alternatives Considered (Why They Lost)

### `@keenmate/svelte-spa-router` v5.3.0

**Pros:** Maintained, Svelte 5 runes, dual-mode routing.  
**Cons:**
- Designed for view-level routing, not overlay stacks
- Requires wrapping every overlay as a route (verbose)
- Adds 8–12 KB when only overlay stack is needed now
- If view routing is added later, can migrate; for now, it's overengineering

**Verdict:** Keep in mind for future **view-level** routing (e.g., `/chat?hub=alice/topic=bugs/message=msg_123`), but not for overlay back-stacks today.

### Hand-Rolled History API (No Store)

**Pros:** Zero dependencies, minimal code.  
**Cons:**
- Couples URL logic to every overlay open/close site
- Hard to test and reason about
- Easy to desync URL and UI state

**Verdict:** The store pattern (even hand-rolled) is worth it for maintainability.

### XState v5

**Pros:** Powerful state machine patterns, actor model, event-driven.  
**Cons:**
- Bundle cost (15–20 KB)
- Learning curve for a simple stack
- Designed for complex workflows, not popover sequencing
- No Svelte 5–specific examples in the community

**Verdict:** Overkill; reserve for future autonomous agent orchestration work.

---

## Risk & Mitigation

| Risk | Mitigation |
|------|-----------|
| URL encoding breaks with special chars | Use `encodeURIComponent()` on data values; unit test codecs |
| Reload loses non-serializable state (e.g., object refs) | Encode only primitives in URL; re-fetch data on hydration if needed |
| Back button vs. app's own back affordance race | Handle `popstate` event to sync stack when user clicks browser back; test both flows |
| Deep-link to invalid agentId/ticketId | Fall back to top-level view on hydration error; log the stale link |

---

## Rollout Plan

1. **Week 1:** Implement `overlayStack.svelte.ts` + unit tests (URL codecs)
2. **Week 2:** Integrate into AgentDossier → Ticket flow; test nested open/close/esc/back
3. **Week 3:** Migrate NotePopover; extend to other overlays
4. **Week 4:** Add deep-linking docs; finalize UX affordances (back button visibility, etc.)

**Go-live:** Once AgentDossier → Ticket back-stack works flawlessly, roll out to users.

---

## References

- **Rust serve handler:** `/home/sk/git/confer/src/serve.rs` lines 544–551 (fallback to SPA)
- **Current popover state:** `/home/sk/git/confer/ui/src/App.svelte` lines 57, 147, 502–507, 745
- **Svelte 5 runes docs:** https://svelte.dev/docs/svelte-5-runes (Svelte docs)
- **Browser History API:** [MDN History API](https://developer.mozilla.org/en-US/docs/Web/API/History_API)
- **`@keenmate/svelte-spa-router`:** [GitHub](https://github.com/KeenMate/svelte-spa-router) — Svelte 5 fork if view routing is added later

