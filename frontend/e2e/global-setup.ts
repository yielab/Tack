import { request as pwRequest } from '@playwright/test';
import { API, SHARED_PROJECT_NAME } from './helpers';

/**
 * Ensures the suite's one shared project exists before any worker process
 * is forked, and publishes its id on `process.env.E2E_SHARED_PROJECT_ID` —
 * `getOrCreateProject` (`./helpers.ts`) reads it first, before falling back
 * to a name-scoped lookup of its own. Running this once, single-threaded,
 * ahead of every worker is what removes the race a per-call "does it exist
 * yet" check would otherwise have under `fullyParallel: true`: two workers
 * finding no shared project at the same moment could each create one.
 *
 * Playwright runs its configured `webServer` health checks before any
 * configured `globalSetup` (verified against the installed `playwright`
 * package's own task ordering), so the API is already reachable here.
 */
export default async function globalSetup(): Promise<void> {
  const context = await pwRequest.newContext();
  try {
    const existing = await context.get(`${API}/projects`).then((r) => r.json());
    const found = Array.isArray(existing)
      ? existing.find((p: { name?: string }) => p.name === SHARED_PROJECT_NAME)
      : undefined;
    if (found) {
      process.env.E2E_SHARED_PROJECT_ID = found.id;
      return;
    }

    const res = await context.post(`${API}/projects`, {
      data: { name: SHARED_PROJECT_NAME, project_type: 'software', description: 'created by e2e' },
    });
    if (!res.ok()) {
      throw new Error(`global setup: create shared project failed: ${res.status()}`);
    }
    const body = await res.json();
    if (body?.id) {
      process.env.E2E_SHARED_PROJECT_ID = body.id;
      return;
    }

    // Shape-agnostic fallback, matching `getOrCreateProject`'s own: re-read
    // the list rather than trust the create response body.
    const list = await context.get(`${API}/projects`).then((r) => r.json());
    const created = Array.isArray(list)
      ? list.find((p: { name?: string }) => p.name === SHARED_PROJECT_NAME)
      : undefined;
    if (!created) throw new Error('global setup: shared project not found by name after create');
    process.env.E2E_SHARED_PROJECT_ID = created.id;
  } finally {
    await context.dispose();
  }
}
