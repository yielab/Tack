import { defineConfig } from '@playwright/test';
import base from './playwright.config';

// Config for the local-only README screenshot spec. The main
// playwright.config.ts testIgnores it so it never runs in CI; this config
// re-includes it while reusing the same webServer, projects, and settings.
// Used by `make screenshots`.
export default defineConfig({
  ...base,
  testIgnore: undefined,
  testMatch: ['**/screenshots.spec.ts'],
});
