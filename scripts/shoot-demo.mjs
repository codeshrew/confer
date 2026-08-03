// shoot-demo.mjs — screenshot every view of a running `confer serve` (point it at the demo hub
// from scripts/demo-hub.sh). Headless Chromium via Playwright — NO real display, no macOS capture.
// Writes retina PNGs to docs/img/. Run AFTER a demo serve is up:
//   scripts/demo-hub.sh --serve 8899   # in one shell
//   cd ui && node ../scripts/shoot-demo.mjs
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..');
// Playwright is a dev-dep of the UI package, so resolve it from ui/node_modules regardless of cwd.
const require = createRequire(resolve(REPO, 'ui') + '/');
const { chromium } = require('@playwright/test');
const BASE = process.env.BASE || 'http://127.0.0.1:8899';
const OUT = process.env.OUT || resolve(REPO, 'docs/img');
const SCHEMES = (process.env.SCHEMES || 'light,dark').split(',');
const VIEWS = ['Overview', 'Chat', 'Board', 'Fleet', 'Code'];
mkdirSync(OUT, { recursive: true });

// Wait for a view to actually finish loading: give it a beat to swap in, let any loading skeletons
// detach, then a final beat for Shiki highlighting / layout to settle. Robust to views with no skeleton.
async function settle(page) {
  await page.waitForTimeout(500);
  await page.locator('[data-testid="skeleton"]').first().waitFor({ state: 'detached', timeout: 8000 }).catch(() => {});
  await page.waitForTimeout(700);
}

const browser = await chromium.launch();
try {
  for (const scheme of SCHEMES) {
    const ctx = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      deviceScaleFactor: 2,
    });
    const page = await ctx.newPage();
    // The dashboard keeps an SSE stream open, so 'networkidle' never fires — wait on the UI instead.
    await page.goto(BASE, { waitUntil: 'domcontentloaded' });
    await page.getByRole('tablist', { name: 'View' }).waitFor({ timeout: 15000 });
    await settle(page); // let the first data load resolve before touching anything
    // The app ignores prefers-color-scheme; it defaults to dark and applies data-theme on mount.
    // For the light set, flip it via the toggle so the real theme machinery runs.
    if (scheme === 'light') {
      await page.getByRole('button', { name: 'Toggle theme' }).click();
      await page.waitForFunction(() => document.documentElement.getAttribute('data-theme') === 'light');
    }
    for (const v of VIEWS) {
      await page.getByRole('tab', { name: v, exact: true }).click();
      await settle(page);
      const suffix = scheme === 'dark' ? '-dark' : '';
      const file = resolve(OUT, `dashboard-${v.toLowerCase()}${suffix}.png`);
      await page.screenshot({ path: file });
      console.log('wrote', file);
    }
    await ctx.close();
  }
} finally {
  await browser.close();
}
