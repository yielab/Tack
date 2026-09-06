import { type Locator } from '@playwright/test';
import {
  test,
  expect,
  API,
  getOrCreateProject,
  getOrCreateItem,
  createFreshItem,
  createSprintWithItem,
  createFleet,
  createAgentProfile,
  enrollRunner,
  waitForApp,
} from './helpers';

// "Run with agent" — item/sprint execution UI (TODO.md III-E4, Wave 4 /
// Phase 54; reworked by VI-C2 for zero hand-typed identifiers). Distinct
// from the older Docket "Dispatch to agents"/"Run sprint" features covered
// by `journey.spec.ts`/`a11y.spec.ts`'s dispatch tests: this feature targets
// `/api/executions`, `/api/runner-fleets`, `/api/agent-profiles`,
// `/api/projects/{id}` (the model default) — an always-on operator surface,
// NOT gated behind `TACK_ORCH_ENABLE` (unlike every Docket dispatch route),
// so no orchestration-enable setup is needed for these specs. See
// `crates/tack-api/src/router.rs`'s own comment on `orch_routes` vs. card
// C1's operator execution/fleet routes.
//
// Every test enrolls at least one runner: with none, the modal shows its
// "agent execution is off" state instead of the form (VI-C2's own honest
// signal — see `shared.ts#isExecutionOff`'s doc comment for what it reads
// once VI-B3 lands a real on/off flag).
//
// `GET /api/runners` is global, not project-scoped (measured while writing
// this card — no route or handler filters it), and `e2e.db` is a persistent
// database every spec run adds enrolled runners to. So "exactly one active
// runner" (this card's "hidden when exactly one machine is active" case) is
// real and unit-tested (`RunWithAgentModal.test.tsx`), but not something an
// E2E test can assume here — the picker may legitimately show because some
// *other* spec's runner is also active. These tests select their own
// runner, by its unique name, only when the picker is showing; they never
// assert whether it is hidden or shown.
async function selectTargetIfPickerShows(modal: Locator, runnerName: string) {
  const picker = modal.getByRole('combobox', { name: 'Machine or group' });
  // The picker either settles hidden (exactly one active runner, auto-selected —
  // a one-shot `isVisible()` snapshot can catch it mid-fetch, before `GET
  // /runners` resolves, and wrongly conclude "hidden") or visible with a
  // "Loading…" placeholder before this test's own freshly-enrolled runner's
  // option exists in it — so wait for one of those two real end states rather
  // than sampling the DOM once.
  const appeared = await picker
    .waitFor({ state: 'visible', timeout: 3000 })
    .then(() => true)
    .catch(() => false);
  if (!appeared) return;
  await expect(picker.getByRole('option', { name: runnerName })).toBeAttached({ timeout: 10000 });
  await picker.selectOption({ label: runnerName });
}

test('Board: "Run with agent" opens the shared modal, and required-field reasons block submit', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const itemId = await getOrCreateItem(request, projectId, `RWA board item ${Date.now()}`);
  // A fleet AND a runner — two targets, so the picker shows instead of
  // auto-selecting (this test wants "Select where this runs." visible).
  await createFleet(request, `RWA board fleet ${Date.now()}`);
  await enrollRunner(request, `RWA board runner ${Date.now()}`, 'opaque/model-alpha');

  await page.goto(`/projects/${projectId}/board`);
  await waitForApp(page);

  const trigger = page.getByRole('button', { name: /^Run with agent:/ }).first();
  await expect(trigger).toBeVisible();
  await trigger.click();

  const dialog = page.getByRole('dialog', { name: /^Run with agent:/ });
  await expect(dialog).toBeVisible();

  // Nothing is filled in yet — the submit control must be disabled with
  // visible, specific reasons (this card's acceptance bar: "disabled +
  // reasoned, not merely rejected server-side").
  // `exact: true` — an inexact match also catches this card's new
  // "Change for this run" button, which contains "run" as a substring.
  const runButton = dialog.getByRole('button', { name: 'Run', exact: true });
  await expect(runButton).toBeDisabled();
  await expect(dialog.getByText('Select where this runs.')).toBeVisible();
  await expect(dialog.getByText('Select an agent profile.')).toBeVisible();
  await expect(dialog.getByText('Enter a repository remote.')).toBeVisible();

  // Escape closes it — the same keyboard path every other modal in this app
  // supports (`shared/ui/Modal.tsx`), exercised here for this card's own
  // acceptance bar ("keyboard/focus path passes").
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  void itemId;
});

test('item-detail: submitting a run creates the request and it appears in the Execution tab without navigation', async ({
  page,
  request,
  executionToggleLock,
}) => {
  void executionToggleLock;
  const projectId = await getOrCreateProject(request);
  // A guaranteed-fresh item (not `getOrCreateItem`, which would reuse
  // whatever item this project already has and accumulate a "Queued" badge
  // per past test run against the same persistent e2e.db).
  const itemId = await createFreshItem(request, projectId, `RWA detail item ${Date.now()}`);
  const profileId = await createAgentProfile(request, `E2E Profile ${Date.now()}`);
  const runnerName = `RWA detail runner ${Date.now()}`;
  await enrollRunner(request, runnerName, 'opaque/model-alpha');

  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);

  const drawer = page.getByRole('dialog');
  await expect(drawer).toBeVisible();

  await drawer.getByRole('button', { name: 'Run with agent' }).click();
  const modal = page.getByRole('dialog', { name: /^Run with agent:/ });
  await expect(modal).toBeVisible();
  await selectTargetIfPickerShows(modal, runnerName);

  await modal.getByRole('combobox', { name: 'Agent profile' }).selectOption(profileId);
  // Repository is a read-only summary by default — no free-text field
  // visible until "Change for this run" (this card's acceptance bar).
  await modal.getByRole('button', { name: 'Change for this run' }).click();
  await modal.getByLabel('Remote').fill('git@example.com:org/repo.git');

  const runButton = modal.getByRole('button', { name: 'Run', exact: true });
  await expect(runButton).toBeEnabled();
  await runButton.click();

  // The modal closes on success, with no page navigation — the request
  // shows up via the shared store, not a reload.
  await expect(modal).toBeHidden();
  await expect(page).toHaveURL(new RegExp(`item=${itemId}`));

  const executionTab = drawer.getByRole('tab', { name: 'Execution' });
  await executionTab.click();
  await expect(drawer.getByText('Queued')).toBeVisible();
  // III-F4 wired the real attempts endpoint (card III-E6 added it; this was
  // a typed "not available yet" placeholder before) — a freshly-created,
  // unclaimed request honestly shows zero attempts, not the old placeholder.
  await expect(drawer.getByText('No attempts yet.')).toBeVisible();

  // Confirm the request is real, not just an optimistic client-side fake —
  // it round-trips through the actual API.
  const list = await request.get(`${API}/executions`).then((r) => r.json());
  expect((list.data as Array<{ item_id: string }>).some((r) => r.item_id === itemId)).toBe(true);
});

test('the run form submits the project\'s configured model default, byte for byte, with zero hand-typed identifiers', async ({
  page,
  request,
  executionToggleLock,
}) => {
  void executionToggleLock;
  const projectId = await getOrCreateProject(request);
  const itemId = await createFreshItem(request, projectId, `RWA project-default item ${Date.now()}`);
  const profileId = await createAgentProfile(request, `RWA project-default profile ${Date.now()}`);
  // The one runner reports the exact combination the project will default
  // to — required for the submit gate (`shared.ts#gateHarnessModelSelection`,
  // unchanged by this card) to allow it through.
  const runnerName = `RWA project-default runner ${Date.now()}`;
  const { runnerId } = await enrollRunner(request, runnerName, 'opaque/model-alpha');

  const patchRes = await request.patch(`${API}/projects/${projectId}`, {
    data: { default_model: { kind: 'explicit', provider: 'openai', model_id: 'opaque/model-alpha' } },
  });
  expect(patchRes.ok(), `project default_model patch failed: ${patchRes.status()}`).toBeTruthy();

  let capturedBody: Record<string, unknown> | undefined;
  await page.route('**/api/executions', async (route) => {
    if (route.request().method() === 'POST') capturedBody = route.request().postDataJSON();
    await route.continue();
  });

  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);

  const drawer = page.getByRole('dialog');
  await drawer.getByRole('button', { name: 'Run with agent' }).click();
  const modal = page.getByRole('dialog', { name: /^Run with agent:/ });
  await expect(modal).toBeVisible();

  // The project's real model default is shown and selected without
  // touching anything — the one hand-typed identifier possibly still
  // needed is the target, only if this shared e2e database's accumulated
  // runners mean the picker shows (`selectTargetIfPickerShows`'s own
  // comment); either way `selector_kind`/`selector_id` below prove it
  // resolved to this test's own runner, by name, never a free-typed id.
  await expect(modal.getByText('Project default — openai / opaque/model-alpha')).toBeVisible();
  await selectTargetIfPickerShows(modal, runnerName);

  await modal.getByRole('combobox', { name: 'Agent profile' }).selectOption(profileId);
  // Repository has no project-level default in this branch's base (VI-C3
  // landed only the model tier — see this card's handoff) — the remote is
  // the one field that is genuinely still typed by hand here.
  await modal.getByRole('button', { name: 'Change for this run' }).click();
  await modal.getByLabel('Remote').fill('git@example.com:org/repo.git');

  await modal.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(modal).toBeHidden();

  expect(capturedBody, 'no POST /api/executions was captured').toBeDefined();
  expect(capturedBody?.requested_model_provider).toBe('openai');
  expect(capturedBody?.requested_model_id).toBe('opaque/model-alpha');
  expect(capturedBody?.selector_kind).toBe('exact_runner');
  expect(capturedBody?.selector_id).toBe(runnerId);
  expect(capturedBody?.agent_profile_id).toBe(profileId);
});

test('Sprint: the per-item "Run with agent" trigger is present and opens the same shared modal', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const { sprintId, itemId } = await createSprintWithItem(request, projectId, `RWA sprint ${Date.now()}`);

  await page.goto(`/projects/${projectId}/sprint`);
  await waitForApp(page);

  const trigger = page.getByRole('button', { name: /^Run with agent:/ }).first();
  await expect(trigger).toBeVisible();
  await trigger.click();

  const dialog = page.getByRole('dialog', { name: /^Run with agent:/ });
  await expect(dialog).toBeVisible();
  // Opening the per-item trigger must not also open the item-detail drawer
  // (the card's own click handler stops propagation) — the sprint board
  // itself, not a drawer, should still be what's behind the modal.
  await expect(page.getByRole('dialog', { name: 'Item details' })).toBeHidden();

  void sprintId;
  void itemId;
});
