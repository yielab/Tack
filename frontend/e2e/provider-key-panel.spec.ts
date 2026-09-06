import { test, expect, waitForApp } from './helpers';

// ADR 0061 decision 2 — a UI-only user hands the embedded runner a Vercel
// AI Gateway key, write-only, with the catalog re-probed in the same
// request (no restart). This drives the real `PUT /api/local-runner/
// secrets/{name}` route and the real `SecretStore` (file-backend fallback
// on a machine with no reachable OS keychain) — not a mock.
//
// No real Vercel AI Gateway credential is available in CI, so the pasted
// value below is a placeholder. `https://ai-gateway.vercel.sh/v1/models`
// answers `401` to any bearer token it doesn't recognize (confirmed live,
// 2026-09) rather than timing out, so the catalog line still changes from
// "not configured" to a typed "unreachable" reason — proving the re-probe
// fired with no restart, even without proving a real model count.
//
// The re-probe this test drives (`put_local_runner_secret`'s own `catalog()`
// call, `crates/tack-api/src/handlers/local_runner.rs`) holds the same
// server-wide `EmbeddedRunnerControl` lock `execution-toggle.spec.ts` and
// `agents-page.spec.ts`'s first test flip via `PUT /api/local-runner` —
// `executionToggleLock` (`./helpers.ts`) keeps this test's real network
// round trip from stalling either of theirs, and their on/off flips from
// landing mid-probe here. See that fixture's own doc comment before
// removing it.

test.beforeEach(async ({ page }) => {
  page.on('pageerror', (err) => {
    throw new Error(`Uncaught page error: ${err.message}`);
  });
});

test('saving a key re-probes the catalog and the value never reaches the DOM', async ({ page, executionToggleLock }) => {
  // `ProviderKeyPanel.tsx`'s own `stored()` getter (`secrets()?.data.find(...)
  // ?? null`) is falsy both while its `secrets` resource is still in flight
  // AND once it resolves to "no key stored" — the identical fallback `<form>`
  // renders either way. On a reused `e2e.db` where a previous run left a key
  // stored, racing `removeButton`/`apiKeyField` visibility against that
  // ambiguity can land on the *loading* rendering, decide "fill the form",
  // and then have the real response arrive a moment later saying a key
  // already exists — yanking the form out from under the fill/click
  // (measured: 4 of 10 solo runs of the *unmodified* file failed this way,
  // `npx playwright test -g "saving a key re-probes" --project=chromium
  // --workers=1`, run before this fix existed). Waiting for both of the
  // panel's own initial GETs to land first — attached before navigation, so
  // neither can complete before this test is listening — makes the DOM this
  // test reads next the settled state, not a transient one.
  const localRunnerLoaded = page.waitForResponse(
    (res) => res.request().method() === 'GET' && res.url().endsWith('/api/local-runner'),
  );
  const secretsLoaded = page.waitForResponse(
    (res) => res.request().method() === 'GET' && res.url().endsWith('/api/local-runner/secrets'),
  );

  await page.goto('/agents');
  await waitForApp(page);

  const heading = page.getByRole('heading', { name: 'Vercel AI Gateway key' });
  await expect(heading).toBeVisible();
  await Promise.all([localRunnerLoaded, secretsLoaded]);

  const removeButton = page.getByRole('button', { name: 'Remove' });
  const apiKeyField = page.getByLabel('API key');
  await Promise.race([
    removeButton.waitFor({ state: 'visible' }),
    apiKeyField.waitFor({ state: 'visible' }),
  ]);

  // A previous run — or, once `executionToggleLock` serializes this test
  // against its siblings, a sibling run against the same long-lived e2e
  // server — may have already saved a key. Removing it here drives the real
  // `DELETE /api/local-runner/secrets/{name}` route and proves the panel
  // reaches a clean slate: the catalog line drops back to "not configured"
  // and the fill-in form reappears, rather than staying stuck reporting an
  // unresolved secret for a provider still marked on.
  if (await removeButton.isVisible()) {
    await removeButton.click();
    await apiKeyField.waitFor({ state: 'visible' });
    await expect(page.getByText('Catalog: not configured')).toBeVisible();
  }

  const secretValue = 'e2e-placeholder-key-never-should-render';
  await apiKeyField.waitFor({ state: 'visible' });
  await apiKeyField.fill(secretValue);
  // Scoped to this panel's own `<form>` — the Agents page also has a
  // "Save" button for its unrelated default-model step below this one.
  await page.locator('form').getByRole('button', { name: 'Save' }).click();

  // The write-only contract: gone from the form, and never in the page's
  // own HTML at any point after save.
  await expect(page.getByText(/^Set /)).toBeVisible({ timeout: 10_000 });
  expect(await page.content()).not.toContain(secretValue);

  // Re-probed with no restart: the catalog line changed from what it was
  // before this test ever touched the key.
  await expect(page.getByText('Catalog: not configured')).not.toBeVisible();
  await expect(page.getByText(/^Catalog: /)).toBeVisible();
});
