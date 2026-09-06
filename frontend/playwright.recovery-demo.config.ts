import { defineConfig, devices } from '@playwright/test';

// Config for recording the durable-recovery demo, used only by
// scripts/record-recovery-demo.sh. Deliberately has NO webServer block:
// playwright.config.ts and playwright.capture.config.ts both start (or reuse)
// a dev build via `cargo run` + `npm run dev`, but this demo's server is a
// real GitHub Release artifact running in Docker — starting a dev build here
// would contradict the point of the recording. baseURL is not set either;
// the spec navigates with absolute URLs built from E2E_API_ORIGIN, which the
// wrapper script points at the release container.
export default defineConfig({
  testDir: './e2e',
  testMatch: ['**/recovery-demo.spec.ts'],
  fullyParallel: false,
  retries: 0,
  workers: 1,
  reporter: 'list',
  timeout: 180_000,
  expect: { timeout: 15_000 },
  use: {
    trace: 'off',
    screenshot: 'only-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
