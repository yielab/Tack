import { test, expect, type Locator, type Page } from '@playwright/test';
import { API, createFreshProject, enrollRunner, getOrCreateProject, waitForApp } from './helpers';

// The Agents page — the one screen from an installed binary to a completed
// attempt. `playwright.config.ts` prepends
// `e2e/fixtures/harness-shims/` to the API webServer's own PATH, so the
// embedded runner's real probe (a real subprocess exec, not a mock) always
// finds these two fake `claude`/`codex` binaries instead of whatever is or
// isn't installed on the machine running the suite — see that file's own
// header comment. Their reported versions (9.9.1/9.9.2) are the load-bearing
// proof that step 1-2 assertions below observe the real probe, not
// something the frontend fabricated.

test.beforeEach(async ({ page }) => {
  page.on('pageerror', (err) => {
    throw new Error(`Uncaught page error: ${err.message}`);
  });
});

/** Each numbered step is its own `<section>` with a heading — scoping
 *  queries to one avoids ambiguity between, e.g., step 4's and step 5's
 *  identically-labeled "Provider"/"Model ID" fields. */
function stepSection(page: Page, headingName: string): Locator {
  return page.locator('section').filter({ has: page.getByRole('heading', { name: headingName, exact: true }) });
}

/** The project picker only renders once more than one project exists — a
 *  reused `e2e.db` usually already has one from another spec. Select this
 *  test's own fresh project by name whenever it's showing, so step 4/5
 *  assertions never land on a different project's own state. */
async function selectProjectIfPickerShows(page: Page, projectName: string) {
  const picker = page.getByLabel('Project', { exact: true });
  if (await picker.isVisible().catch(() => false)) {
    await picker.selectOption({ label: projectName });
  }
}

test('steps 1, 2 and 4: turning agent execution on reveals both agents installed at the shim\'s own version, and a default model saves against the real project', async ({
  page,
  request,
}) => {
  // A dedicated project, never the suite's shared one: `default_model` has
  // no route to unset once written (`ModelDefaultStep.tsx`'s own doc
  // comment), so saving one against a shared project would permanently
  // change what every other spec sees on a reused `e2e.db`. Ensuring the
  // suite's own shared project exists first keeps it (not this test's own,
  // freshly created afterward) as `existing[0]` — the row every other
  // spec's own `getOrCreateProject` reuses.
  await getOrCreateProject(request);
  const projectName = `Agents page e2e ${Date.now()}`;
  await createFreshProject(request, projectName);
  await page.goto('/agents');
  await waitForApp(page);
  await selectProjectIfPickerShows(page, projectName);

  const toggleButton = page.getByRole('button', { name: /Turn (on|off)/ });
  await expect(toggleButton).toBeVisible();

  // Leave a clean, known "off" state if a previous run left it on.
  if ((await toggleButton.textContent())?.includes('Turn off')) {
    await toggleButton.click();
    await expect(page.getByText('Stopped', { exact: true })).toBeVisible();
  }

  await expect(page.getByText("Turn on agent execution above to see what's installed here.")).toBeVisible();

  await toggleButton.click();
  await expect(page.getByText('Running', { exact: true })).toBeVisible({ timeout: 15_000 });

  // Both fake harnesses report at the shim's own version — the real
  // embedded runner's own probe reading a real subprocess, not anything
  // this page fabricated.
  await expect(page.getByText('Installed v9.9.1')).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText('Installed v9.9.2')).toBeVisible();

  // Step 3's "use the agent's own login" path renders once each is
  // installed, honestly unverified until a test run succeeds.
  await expect(page.getByText('Present, unverified').first()).toBeVisible();

  // Step 4: with an active runner attesting model-id passthrough (both
  // adapters do), the free-text option is offered; save it and prove it
  // round-trips against the real `PATCH /api/projects/{id}`.
  const modelDefault = stepSection(page, 'Default model');
  await modelDefault.getByRole('radio', { name: 'Type a model id' }).check();
  const modelId = `e2e-model-${Date.now()}`;
  await modelDefault.getByLabel('Provider', { exact: true }).fill('openai');
  await modelDefault.getByLabel('Model ID', { exact: true }).fill(modelId);
  await modelDefault.getByRole('button', { name: 'Save' }).click();
  await expect(page.getByText('Saved')).toBeVisible();

  await page.reload();
  await waitForApp(page);
  await expect(stepSection(page, 'Default model').getByLabel('Model ID', { exact: true })).toHaveValue(modelId);

  // Leave the machine as this test found it, for every other spec in this
  // suite that assumes agent execution starts off.
  await page.getByRole('button', { name: 'Turn off' }).click();
  await expect(page.getByText('Stopped', { exact: true })).toBeVisible({ timeout: 15_000 });
});

test('step 5: a test run reaches the real production router and appears in the timeline as queued, then leased', async ({
  page,
  request,
}) => {
  await getOrCreateProject(request); // keeps the suite's shared project as `existing[0]`
  const projectName = `Agents page e2e test run ${Date.now()}`;
  await createFreshProject(request, projectName);
  const modelId = `e2e-test-run-${Date.now()}`;

  // The page auto-selects a target with no operator-facing picker, so an
  // already-active runner left over from an earlier local
  // run of this same spec (or from `a11y.spec.ts`'s own enrollments, on a
  // reused `e2e.db`) would otherwise race this test's own for that
  // selection. Revoking every pre-existing active runner first makes this
  // test's own the only eligible one, deterministically, on a fresh CI
  // checkout and on a repeat local run alike.
  const before = await request.get(`${API}/runners`).then((r) => r.json());
  for (const row of before.data as Array<{ runner_id: string; name: string; state: string }>) {
    // Never revoke the embedded runner's own row (`local-*`) — its
    // credential lives in the webServer's own state directory and is
    // meant to be reused across the whole suite; revoking it here would
    // strand every other spec that turns it back on later.
    if (row.state === 'active' && !row.name.startsWith('local-')) {
      await request.post(`${API}/runners/${encodeURIComponent(row.runner_id)}/revoke`);
    }
  }

  // A runner this test controls the credential for, standing in for "an
  // active agent with an installed harness" — the embedded runner (the
  // previous test's own subject) is left off here on purpose: driving a
  // real harness subprocess to a genuine "succeeded" completion needs each
  // adapter's own argv/output shape, which is out of reach from a frontend
  // test (see the handoff's "What is left" section). This proves the
  // request reaches the real scheduler/lease pipeline instead, exactly as
  // far as `execution-attempt-detail.spec.ts` already proves for the older
  // "Run with agent" modal.
  const { runnerId, credential } = await enrollRunner(request, `agents-page-test-run-${Date.now()}`, modelId);

  await page.goto('/agents');
  await waitForApp(page);
  await selectProjectIfPickerShows(page, projectName);

  const testRun = stepSection(page, 'Test run');
  await expect(testRun).toBeVisible();
  await testRun.getByLabel('Repository remote', { exact: true }).fill('https://example.com/org/sample');
  await testRun.getByLabel('Provider', { exact: true }).fill('openai');
  await testRun.getByLabel('Model ID', { exact: true }).fill(modelId);

  await testRun.getByRole('button', { name: 'Run test' }).click();
  await expect(page.getByText('Test run started')).toBeVisible();
  await expect(page.getByText('Queued')).toBeVisible({ timeout: 10_000 });

  // Drive the real request through the real runner-v1 protocol, as
  // `tack-runner` itself would. `claimOnce` only succeeds if this test's
  // own runner is the one the page's auto-selection actually chose.
  const claimRes = await request.post(`${API}/runner/v1/claim`, {
    headers: { authorization: `Bearer ${credential}` },
    data: { protocol_version: 1, runner_id: runnerId, claim_request_id: `agents-page-${Date.now()}`, available_capacity: 1, wait_ms: 0 },
  });
  expect(claimRes.ok(), `claim failed: ${claimRes.status()}`).toBeTruthy();
  const claimed = await claimRes.json();
  expect(claimed?.request?.item_id, "the claimed request is not this test's own item").toBeTruthy();

  // No reload: the mounted `ExecutionTimeline` (via `shared/state/
  // executionContext.tsx`'s bounded realtime poll) picks up the claim on
  // its own — reloading would also lose this component's own local
  // `itemId` signal, since nothing here persists it across a navigation.
  await expect(page.getByText('Leased').first()).toBeVisible({ timeout: 20_000 });
});
