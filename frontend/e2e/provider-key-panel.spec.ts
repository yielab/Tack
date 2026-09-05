import { test, expect } from '@playwright/test';
import { waitForApp } from './helpers';

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

test.beforeEach(async ({ page }) => {
  page.on('pageerror', (err) => {
    throw new Error(`Uncaught page error: ${err.message}`);
  });
});

test('saving a key re-probes the catalog and the value never reaches the DOM', async ({ page }) => {
  await page.goto('/agents');
  await waitForApp(page);

  const heading = page.getByRole('heading', { name: 'Vercel AI Gateway key' });
  await expect(heading).toBeVisible();

  const removeButton = page.getByRole('button', { name: 'Remove' });
  const apiKeyField = page.getByLabel('API key');
  // The panel renders one of these two mutually-exclusive states once its
  // two resources (status, secrets) resolve — wait for either rather than
  // racing an `isVisible()` check against the still-loading skeleton.
  await Promise.race([
    removeButton.waitFor({ state: 'visible' }),
    apiKeyField.waitFor({ state: 'visible' }),
  ]);

  // Remove any key a previous run left behind, so this run starts from a
  // known "not configured" state.
  if (await removeButton.isVisible()) {
    await removeButton.click();
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
