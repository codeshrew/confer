import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { viteSingleFile } from 'vite-plugin-singlefile'

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte(), viteSingleFile()],
  resolve: {
    // Vitest runs in Node, so it picks Svelte's server-side condition by
    // default; force the browser build so component tests can `mount`.
    conditions: process.env.VITEST ? ['browser'] : undefined,
  },
  build: {
    // vite-plugin-singlefile inlines all assets into one dist/index.html,
    // which is embedded in the Rust binary via include_str!.
    cssCodeSplit: false,
    assetsInlineLimit: Number.MAX_SAFE_INTEGER,
  },
  server: {
    // Lets `npm run dev` talk to a real, locally-running `confer serve`
    // (default port 8422) instead of only mock.ts's fixtures. Combine with
    // `?live` (or `VITE_LIVE=1`) — see api.ts's useMock() — to actually
    // route fetches/SSE through this proxy instead of the mock API; the
    // proxy target being configured doesn't by itself change what api.ts
    // fetches. `/api/events` is a long-lived SSE connection, which the
    // proxy passes through transparently (no special config needed for
    // that in Vite's http-proxy-based server.proxy).
    proxy: {
      '/api': 'http://127.0.0.1:8422',
    },
  },
  test: {
    environment: 'jsdom',
    globals: false,
    setupFiles: ['./src/test-setup.ts'],
    include: ['src/**/*.{test,spec}.{js,ts}'],
    // format.ts's formatClock/formatIsoDate render the operator's LOCAL
    // time (fixed 2026-07-18 — they were silently UTC-only despite their
    // own comments). Pinning the test-runner's timezone to UTC keeps every
    // existing "UTC-looking" assertion deterministic (local === UTC under
    // this pin) without the suite depending on whatever timezone happens to
    // be running it — a real machine here is MDT, not UTC, so this isn't
    // hypothetical.
    env: {
      TZ: 'UTC',
    },
  },
})
