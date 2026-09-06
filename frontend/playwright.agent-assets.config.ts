import { defineConfig, devices } from '@playwright/test';

// Config for recording VI-D2's real hero GIF and Agents-page screenshots
// (docs/screenshots/hero.gif, agents.png, attempt.png, two-machines.png).
// Deliberately has NO webServer block, the same reasoning
// playwright.recovery-demo.config.ts documents: the target here is an
// already-running `tack serve --with-runner` RELEASE build (with the
// `embed-spa` feature, real installed `claude`/`codex` binaries on PATH,
// TACK_PORT=3311, a database/state dir inside this worktree) — never the
// dev `cargo run`/`npm run dev` pair the default config starts, and never
// the fake harness-shims PATH override `playwright.capture.config.ts`
// inherits from it. Every attempt this suite drives is a real, live, billed
// model call — that is the whole point of the card these assets are for.
export default defineConfig({
  testDir: './e2e',
  testMatch: ['**/agent-assets.spec.ts'],
  fullyParallel: false,
  retries: 0,
  workers: 1,
  reporter: 'list',
  timeout: 300_000,
  expect: { timeout: 15_000 },
  use: {
    trace: 'off',
    screenshot: 'only-on-failure',
    actionTimeout: 15_000,
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
