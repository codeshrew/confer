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
  test: {
    environment: 'jsdom',
    globals: false,
    setupFiles: ['./src/test-setup.ts'],
    include: ['src/**/*.{test,spec}.{js,ts}'],
  },
})
