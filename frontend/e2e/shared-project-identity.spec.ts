import { test, expect } from '@playwright/test';
import { API, getOrCreateProject, SHARED_PROJECT_NAME } from './helpers';

test.skip(({ browserName }) => browserName !== 'chromium', 'identity is browser-independent');

// Regression guard for the leak `getOrCreateProject`'s own doc comment
// describes: a position-based ("first back") lookup over `GET /api/projects`
// (ordered by `updated_at DESC`) would return whichever project any other
// concurrently-running spec file most recently created or patched, not a
// stable identity. This test's own assertion is independent of
// `getOrCreateProject`'s internals — it re-derives the expected id by
// querying the API directly and matching on the fixed name, so it would
// catch a regression even if `global-setup.ts` stopped setting
// `E2E_SHARED_PROJECT_ID` and the helper fell back to a broken lookup.
test('the shared project keeps one stable identity while other specs run concurrently', async ({ request }) => {
  const id = await getOrCreateProject(request);

  const list: Array<{ id: string; name: string }> = await request.get(`${API}/projects`).then((r) => r.json());
  const matches = list.filter((p) => p.name === SHARED_PROJECT_NAME);

  // Exactly one project carries the shared name — if two workers had ever
  // raced past a "does it exist yet" check without global setup's
  // single-threaded guarantee, this would catch the duplicate directly,
  // not just its symptom.
  expect(matches).toHaveLength(1);
  expect(matches[0].id).toBe(id);
});
