import { defineConfig, devices } from '@playwright/test';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));

// End-to-end tests drive the real app: the tack-api server + the Vite-served
// SPA, in a real browser. Playwright owns the lifecycle of both servers via the
// `webServer` block below, so `npm run test:e2e` is the only command needed.
//
// The API runs against a throwaway SQLite file (e2e.db) and storage dir so a
// run never touches your working database.

// Fake `claude`/`codex` binaries (`e2e/fixtures/harness-shims/`), prepended
// ahead of the real PATH so the embedded runner's own probe (a real
// subprocess exec — see `agents-page.spec.ts`) always finds these instead
// of whatever is or isn't really installed on the machine running the
// suite. Prepending rather than replacing keeps `cargo`/`node` resolvable
// via whatever the rest of PATH already is, on any machine or CI runner.
const HARNESS_SHIMS_DIR = resolve(__dirname, 'e2e/fixtures/harness-shims');

// Dedicated e2e ports so a dev server already running on the standard ports
// (3210 API / 5173 SPA) is never reused in place of an isolated test instance.
// The API runs against a throwaway e2e.db; the SPA proxies to it via
// VITE_PROXY_TARGET. This makes local and CI runs identical and hermetic.
const API_PORT = 3399;
const WEB_PORT = 5199;
const isCI = !!process.env.CI;

// The test runner (and helpers/api.spec) talk to the API directly on this origin.
process.env.E2E_API_ORIGIN = process.env.E2E_API_ORIGIN || `http://127.0.0.1:${API_PORT}`;

export default defineConfig({
  testDir: './e2e',
  // Capture specs (screenshots / hero GIF) are local-only tools — they require
  // ffmpeg and a running dev environment. Exclude them from the default CI suite;
  // run them explicitly with `make screenshots` or `make gif`. recovery-demo.spec.ts
  // is excluded for a different reason: it targets a release artifact in Docker,
  // not this config's dev `cargo run`/`npm run dev` webServer — see
  // playwright.recovery-demo.config.ts and scripts/record-recovery-demo.sh.
  // agent-assets.spec.ts is the same shape as the first two: it drives a
  // release build of `tack serve --with-runner` with real harness binaries on
  // PATH, not this config's dev pair, and it flips the one server-wide agent
  // execution switch without holding helpers.ts's `executionToggleLock` — left
  // in the default suite it fails on its own missing environment and makes
  // every spec that reads a capability fail alongside it.
  testIgnore: [
    '**/screenshots.spec.ts',
    '**/hero-gif.spec.ts',
    '**/recovery-demo.spec.ts',
    '**/agent-assets.spec.ts',
  ],
  // One test file shouldn't leak state into another; each creates what it needs.
  fullyParallel: true,
  forbidOnly: isCI,
  retries: isCI ? 2 : 0,
  // SQLite is single-writer; serialize in CI to avoid write-contention flakes.
  workers: isCI ? 1 : undefined,
  reporter: isCI ? [['list'], ['html', { open: 'never' }]] : 'list',
  timeout: 30_000,
  expect: { timeout: 10_000 },

  use: {
    baseURL: `http://localhost:${WEB_PORT}`,
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: isCI ? 'on-first-retry' : 'off',
  },

  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
  ],

  webServer: [
    {
      // Run from frontend/ (the config dir) — NOT the repo root — so the e2e
      // API does not pick up the repo-root tack.toml. Config::load() returns
      // early when tack.toml exists and ignores all TACK_* env vars, so the
      // toml's absence here is what lets TACK_PORT / DATABASE_URL take effect.
      // cargo still resolves the workspace by searching upward.
      command: 'cargo run -p tack-cli -- serve',
      url: `http://127.0.0.1:${API_PORT}/api/health`,
      timeout: 180_000, // first compile can be slow
      reuseExistingServer: !isCI,
      stdout: 'ignore',
      stderr: 'pipe',
      env: {
        TACK_PORT: String(API_PORT),
        TACK_DATABASE_URL: 'sqlite:e2e.db?mode=rwc',
        TACK_STORAGE_DIR: './storage-e2e',
        TACK_LOG_LEVEL: 'warn',
        // III-F4: a fixed, non-secret token so `execution-attempt-detail.spec.ts`
        // can prove the real "happy path" decision-resolve flow through the
        // production router. Additive — every other spec is unaffected, since
        // nothing but a resolve call ever reads this header (see
        // `crates/tack-api/src/handlers/decisions.rs`'s `require_decision_token`).
        TACK_EXECUTION_DECISION_TOKEN: 'e2e-decision-token',
        PATH: `${HARNESS_SHIMS_DIR}:${process.env.PATH}`,
      },
    },
    {
      command: `npm run dev -- --port ${WEB_PORT} --strictPort`,
      url: `http://localhost:${WEB_PORT}`,
      timeout: 60_000,
      reuseExistingServer: !isCI,
      env: {
        VITE_API_URL: '/api', // force same-origin proxy, ignore any .env override
        VITE_PROXY_TARGET: `http://127.0.0.1:${API_PORT}`, // proxy to the isolated e2e API
      },
    },
  ],
});
