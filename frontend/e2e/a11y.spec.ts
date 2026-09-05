import { test, expect } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import {
  getOrCreateProject,
  getOrCreateItem,
  createFreshItem,
  createItemWithAssignee,
  createSprintWithItem,
  createFleet,
  createAgentProfile,
  createExecution,
  enrollRunner,
  claimOnceWithLease,
  waitForApp,
} from './helpers';

// Accessibility scans (WCAG 2.0/2.1 A & AA) on the key surfaces. axe-core finds
// the machine-detectable ~40% of issues: contrast, missing labels, ARIA misuse,
// non-focusable controls. Run only on chromium — a11y is engine-independent and
// scanning three times adds noise without coverage.
//
// New violations fail CI. To triage existing debt without blocking, add the
// rule id to KNOWN_ISSUES with a tracking note rather than deleting the assertion.

test.skip(({ browserName }) => browserName !== 'chromium', 'a11y scan runs on chromium only');

// Suppress known, justified violations here ONLY so the gate keeps blocking
// *new* classes of regression. Add an axe rule id with a tracking note rather
// than deleting the assertion; remove it once the underlying issue is fixed.
// (Currently empty — the initial color-contrast and select-name findings are
// fixed: see index.css token darkening and the Sidebar select aria-label.)
const KNOWN_ISSUES: string[] = [];

async function scan(page: import('@playwright/test').Page) {
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'])
    .disableRules(KNOWN_ISSUES)
    .analyze();
  return results.violations;
}

test('home page has no accessibility violations', async ({ page }) => {
  await page.goto('/');
  await waitForApp(page);
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('board view has no accessibility violations', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  await page.goto(`/projects/${projectId}/board`);
  await waitForApp(page);
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// TODO.md §6 "A12": no fixture here ever set an `assignee`, so `<Avatar>`
// (shared/ui/Avatar.tsx) — white initials over a per-name `hsl()` chip — never
// actually rendered during a scan. ~56% of the generated hues failed AA
// against fixed white text; this went undetected for months. "Avery Green"
// (hue 58) sits right in that failing band (old white-on-bg contrast ~2.1:1),
// so this fixture actually exercises the fix rather than getting lucky with a
// hue that happened to pass even under the bug.
test('board view with an assigned item (populated avatar) has no accessibility violations', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  await createItemWithAssignee(request, projectId, 'Avery Green');
  await page.goto(`/projects/${projectId}/board`);
  await waitForApp(page);
  await expect(page.getByTitle('Avery Green').first()).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('global settings has no accessibility violations', async ({ page }) => {
  await page.goto('/settings');
  await waitForApp(page);
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// Settings → Orchestration (frontend/src/features/settings/orchestrationSettings/**,
// TODO.md §6 "E2", Phase 39: "make the agent-factory control center
// discoverable"). `GET /api/settings/orchestration` is reachable even when
// orchestration is off (by contract — this route sits outside the
// `TACK_ORCH_ENABLE` gate every other orchestration route uses), so the
// disabled-state scan below hits the real, unmodified dev server exactly
// like the "global settings" scan above — no route interception needed for
// that one. The populated scan intercepts `GET /api/control-planes` and
// `GET /api/projects` the same way the Fleet "populated" scan above
// intercepts `GET /api/fleet`, so axe scans the real guided-setup UI
// (control-plane list with every health state, the project picker) rather
// than just its absence.

test('settings orchestration section (disabled, env default) has no accessibility violations', async ({
  page,
}) => {
  await page.goto('/settings');
  await waitForApp(page);
  await expect(page.getByText('TACK_ORCH_ENABLE')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('settings orchestration section (enabled, populated) has no accessibility violations', async ({
  page,
}) => {
  await page.route('**/api/settings/orchestration', (route) => {
    if (route.request().method() !== 'GET') return route.fallback();
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        enabled: true,
        source: 'database',
        reconciler_running: true,
        control_plane_count: 2,
        linked_project_count: 1,
        poll_secs: 10,
        approval_token_set: true,
        env_default: false,
      }),
    });
  });
  await page.route('**/api/control-planes', (route) => {
    if (route.request().method() !== 'GET') return route.fallback();
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        {
          id: 'cp-1',
          name: 'docket-prod',
          kind: 'docket',
          base_url: 'https://docket.internal.example.com',
          api_version: '1',
          health: 'healthy',
          last_seen_at: new Date().toISOString(),
          consecutive_failures: 0,
          token_set: true,
          created_at: '2026-01-01T00:00:00Z',
          updated_at: '2026-08-05T00:00:00Z',
        },
        {
          id: 'cp-2',
          name: 'docket-fresh',
          kind: 'docket',
          base_url: 'https://docket-fresh.internal.example.com',
          api_version: null,
          health: 'unknown',
          last_seen_at: null,
          consecutive_failures: 0,
          token_set: false,
          created_at: '2026-08-05T00:00:00Z',
          updated_at: '2026-08-05T00:00:00Z',
        },
      ]),
    });
  });
  await page.route('**/api/projects', (route) => {
    if (route.request().method() !== 'GET') return route.fallback();
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([{ id: 'proj-1', name: 'Website Revamp' }]),
    });
  });

  await page.goto('/settings');
  await waitForApp(page);
  await expect(page.getByText('docket-prod', { exact: true })).toBeVisible();
  await expect(page.getByText('docket-fresh', { exact: true })).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// Every work lens is a distinct surface with its own controls (drag handles,
// grids, legends). Scan each so a regression in one view can't hide behind a
// clean board scan.
for (const lens of ['table', 'timeline', 'calendar', 'sprint'] as const) {
  test(`${lens} view has no accessibility violations`, async ({ page, request }) => {
    const projectId = await getOrCreateProject(request);
    await getOrCreateItem(request, projectId);
    await page.goto(`/projects/${projectId}/${lens}`);
    await waitForApp(page);
    const violations = await scan(page);
    expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
  });
}

test('item detail drawer has no accessibility violations', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  const itemId = await getOrCreateItem(request, projectId);
  // The drawer is driven by the `item` search param on any project route.
  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);
  // Wait for the drawer to mount (lazy-loaded) before scanning.
  await expect(page.getByRole('dialog')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// Fleet view (frontend/src/features/fleet/**, TODO.md §6 "A5"). `GET
// /api/fleet` 404s unless the server is started with TACK_ORCH_ENABLE=true —
// this e2e harness's webServer (playwright.config.ts) does not set it, and
// that file isn't owned by this spec, so the disabled state below is the
// real, unmodified default a fresh install sees. There is also no
// control-plane-registration UI yet (A5's handoff, TODO.md §6) to seed a
// populated row through the API even if orchestration were on. So the
// populated scan below intercepts the browser's `GET /api/fleet` request
// directly (`page.route`) and fulfills it with a payload shaped exactly like
// `frontend/src/features/fleet/api.ts`'s `FleetResponse`/`FleetRow` — the
// file A5's card designates as the single source of truth for the wire
// shape. This still renders and scans the real `FleetPage`/`FleetRow`/
// `HealthChip` components against real (mocked-network) data, covering all
// four `ControlPlaneHealth` states — including the stale/dashed-field
// treatment for `unreachable`/`unknown` rows, which is the card's central
// accessibility-relevant decision (no opacity-dimming, no confident-looking
// zero). It does not exercise the real `tack-orch` reconciler or `GET
// /api/fleet` handler themselves — only the frontend's rendering of their
// documented contract.

test('fleet page (orchestration disabled) has no accessibility violations', async ({ page }) => {
  await page.goto('/fleet');
  await waitForApp(page);
  // Assert the real disabled-state copy rendered — a blank or broken page
  // must not slip through as a "clean" scan just because nothing was there
  // to flag.
  await expect(page.getByText('Agent-fleet orchestration is disabled')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('fleet page (populated) has no accessibility violations', async ({ page }) => {
  const now = new Date();
  const isoAgo = (ms: number) => new Date(now.getTime() - ms).toISOString();

  const mockResponse = {
    rows: [
      {
        project_id: 'e2e-fleet-healthy',
        project_name: 'Fleet Row Healthy',
        control_plane_id: 'cp-healthy',
        control_plane_name: 'docket-prod',
        control_plane_kind: 'docket',
        health: 'healthy',
        last_seen_at: isoAgo(30_000),
        consecutive_failures: 0,
        gateway: 'active',
        roster: [
          { id: 'agent-1', name: 'Backend Dev', role: 'backend-dev', model: 'claude-sonnet-5' },
          { id: 'agent-2', name: 'Reviewer', role: 'reviewer', model: 'claude-opus-5' },
        ],
        last_activity_at: isoAgo(30_000),
        tokens_in: 128_400,
        tokens_out: 45_200,
        cost_usd_estimated: 12.34,
        pricing_snapshot_at: isoAgo(86_400_000),
        budget_usd: 100,
        pending_approval_count: 2,
      },
      {
        project_id: 'e2e-fleet-degraded',
        project_name: 'Fleet Row Degraded',
        control_plane_id: 'cp-degraded',
        control_plane_name: 'docket-staging',
        control_plane_kind: 'docket',
        health: 'degraded',
        last_seen_at: isoAgo(5 * 60_000),
        consecutive_failures: 3,
        gateway: 'active',
        roster: [{ id: 'agent-3', name: 'Frontend Dev', role: 'frontend-dev', model: 'claude-sonnet-5' }],
        last_activity_at: isoAgo(5 * 60_000),
        tokens_in: 4200,
        tokens_out: 900,
        cost_usd_estimated: null,
        pricing_snapshot_at: null,
        budget_usd: null,
        pending_approval_count: 0,
      },
      {
        project_id: 'e2e-fleet-unreachable',
        project_name: 'Fleet Row Unreachable',
        control_plane_id: 'cp-unreachable',
        control_plane_name: 'docket-remote',
        control_plane_kind: 'docket',
        health: 'unreachable',
        last_seen_at: isoAgo(3_600_000),
        consecutive_failures: 12,
        gateway: 'inactive',
        roster: [{ id: 'agent-4', name: 'QA', role: 'qa', model: 'claude-haiku-5' }],
        last_activity_at: isoAgo(3_600_000),
        tokens_in: 9000,
        tokens_out: 2000,
        cost_usd_estimated: 3.5,
        pricing_snapshot_at: isoAgo(86_400_000),
        budget_usd: 50,
        pending_approval_count: 1,
      },
      {
        project_id: 'e2e-fleet-unknown',
        project_name: 'Fleet Row Unknown',
        control_plane_id: 'cp-unknown',
        control_plane_name: 'docket-new',
        control_plane_kind: 'docket',
        health: 'unknown',
        last_seen_at: null,
        consecutive_failures: 0,
        gateway: 'unknown',
        roster: [],
        last_activity_at: null,
        tokens_in: 0,
        tokens_out: 0,
        cost_usd_estimated: null,
        pricing_snapshot_at: null,
        budget_usd: null,
        pending_approval_count: 0,
      },
    ],
  };

  await page.route('**/api/fleet', (route) =>
    route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(mockResponse) }),
  );
  await page.goto('/fleet');
  await waitForApp(page);
  await expect(page.getByRole('table')).toBeVisible();
  await expect(page.getByText('Fleet Row Unreachable')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// Agents page — Advanced section (frontend/src/features/agents/runnerFleet/**
// — a distinct, harness-agnostic execution runner from the legacy Docket
// scans above, which happen to share the word "fleet"). The operator
// execution/fleet/runner/profile routes this section calls
// (`/api/executions`, `/api/runner-fleets`, `/api/runners/*`,
// `/api/agent-profiles`, `/api/model-profiles`) are NOT gated behind
// `TACK_ORCH_ENABLE` (`crates/tack-api/src/router.rs`'s
// `operator_execution_routes` is merged into `/api` independently of
// `orch_routes`) — so these scans hit the real, unmodified webServer with no
// `page.route` interception at all, including a genuine enroll round-trip
// against `POST /api/runners/enrollment`.
// Collapsed by default (its vocabulary lives only here), so every
// scan below opens it first.

async function openAdvanced(page: import('@playwright/test').Page) {
  await page.getByRole('button', { name: /Advanced/ }).click();
}

test('agents page — advanced section (default Runners tab, empty) has no accessibility violations', async ({
  page,
}) => {
  await page.goto('/agents');
  await waitForApp(page);
  await openAdvanced(page);
  await expect(page.getByRole('heading', { name: 'Enroll a runner' })).toBeVisible();
  await expect(page.getByRole('tablist')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('agents page — enrolling a runner and viewing the one-time token modal has no accessibility violations', async ({
  page,
}) => {
  await page.goto('/agents');
  await waitForApp(page);
  await openAdvanced(page);

  const runnerName = `e2e-runner-${Date.now()}`;
  await page.getByLabel('Name').fill(runnerName);
  await page.getByRole('button', { name: 'Enroll', exact: true }).click();

  await expect(page.getByRole('dialog', { name: 'Runner enrollment token' })).toBeVisible();
  await expect(page.getByText('Shown once')).toBeVisible();
  // The real enrollment token is a real secret round-tripped from the live
  // API — assert SOME token text rendered (not empty), without hard-coding
  // its value.
  await expect(page.getByText(/^enr_/)).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);

  // Close and confirm the roster now shows the runner as unconfirmed —
  // never a fabricated "Healthy" reading.
  await page.getByRole('button', { name: "I've copied it — close" }).click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.getByText(runnerName, { exact: true })).toBeVisible();
  await expect(page.getByText('Connection unconfirmed')).toBeVisible();

  const violationsAfterClose = await scan(page);
  expect(violationsAfterClose, JSON.stringify(violationsAfterClose.map((v) => v.id), null, 2)).toEqual([]);
});

test('agents page — Fleets/Agent profiles/Model profiles tabs have no accessibility violations', async ({ page }) => {
  await page.goto('/agents');
  await waitForApp(page);
  await openAdvanced(page);

  await page.getByRole('tab', { name: 'Fleets' }).click();
  await expect(page.getByRole('tabpanel')).toBeVisible();
  let violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);

  await page.getByRole('tab', { name: 'Agent profiles' }).click();
  violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);

  await page.getByRole('tab', { name: 'Model profiles' }).click();
  violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('agents page — creating a fleet via the form has no accessibility violations', async ({ page }) => {
  await page.goto('/agents');
  await waitForApp(page);
  await openAdvanced(page);

  await page.getByRole('tab', { name: 'Fleets' }).click();
  await page.getByRole('button', { name: '+ Create fleet' }).click();
  const fleetName = `e2e-fleet-${Date.now()}`;
  await page.getByLabel('Name').fill(fleetName);
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page.getByText(fleetName, { exact: true })).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// Dispatch UI (frontend/src/shared/dispatch/**, TODO.md §6 "C4", tasks
// 35.8/35.9). Same technique as the Fleet scans above: `TACK_ORCH_ENABLE`
// isn't set for this harness's webServer, so every dispatch route 404s by
// default — exactly the "no dispatch controls" state every other scan in
// this file already covers incidentally (none of them ever see a dispatch
// button, since it only renders once its own orchestration probe succeeds).
// These tests intercept the specific orch routes each surface depends on to
// render its *enabled* state, so axe actually scans the dispatch button, the
// per-outcome note, and the sprint dry-run/results modal — not just their
// absence.

test('item detail drawer with the dispatch control visible has no accessibility violations', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const itemId = await getOrCreateItem(request, projectId);

  await page.route(`**/api/items/${itemId}/agent-activity`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ attempts: [], approvals: [], events_truncated: false, events_retention_days: 90 }),
    }),
  );

  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Dispatch to agents' })).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('item detail drawer after a blocked dispatch outcome has no accessibility violations', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const itemId = await getOrCreateItem(request, projectId);

  await page.route(`**/api/items/${itemId}/agent-activity`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ attempts: [], approvals: [], events_truncated: false, events_retention_days: 90 }),
    }),
  );
  await page.route(`**/api/items/${itemId}/dispatch`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        outcome: 'blocked',
        task: null,
        policy_id: 'prompt-injection',
        message: 'destructive shell command in task description',
      }),
    }),
  );

  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);
  await page.getByRole('button', { name: 'Dispatch to agents' }).click();
  // The blocked outcome names the policy — the card's own correctness bar
  // ("show WHICH policy blocked it"), and a visible marker the note actually
  // rendered in the item-details surface before scanning. The same policy is
  // also repeated in a transient notification, so keep this locator scoped to
  // the dialog rather than relying on a globally unique text match.
  await expect(
    page.getByRole('dialog', { name: 'Item details' }).getByText('prompt-injection'),
  ).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('sprint "Run sprint" dry-run preview has no accessibility violations', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  // `getOrCreateProject` reuses one shared project across this whole spec
  // file (and across the two sprint-dispatch tests specifically), and
  // `createSprintWithItem` always creates a fresh sprint rather than
  // reusing one — so a sprint left over from another test can still be
  // "active" (non-closed) with items assigned, and would render its own,
  // equally legitimate "Run sprint" button (Sprints.tsx renders one button
  // per eligible sprint, by design — see TODO.md §6 "F1"). A unique sprint
  // name plus an accessible name that includes it (`Run sprint: <name>`,
  // also added by F1) is what disambiguates the two real buttons instead of
  // relying on there being exactly one sprint in the project.
  const sprintName = 'E2E Sprint (dry-run preview)';
  const { sprintId, sprintName: uniqueSprintName } = await createSprintWithItem(
    request,
    projectId,
    sprintName,
  );

  // `useAgentActivityMap`'s bulk fetch is Sprints.tsx's own "is orchestration
  // enabled" gate for the "Run sprint" button (reusing the same probe Board.tsx
  // uses — see `TODO.md` §6 "C4").
  await page.route(`**/api/projects/${projectId}/agent-activity`, (route) =>
    route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ rows: [] }) }),
  );
  await page.route(`**/api/sprints/${sprintId}/dispatch/dry-run`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        sprint_id: sprintId,
        max_in_flight: 5,
        summary: {
          total: 3,
          dispatched: 0,
          waiting_approval: 0,
          blocked: 0,
          already_in_flight: 0,
          waiting_on_dependencies: 1,
          not_eligible: 0,
          no_dispatch_policy: 0,
          would_dispatch: 2,
          errored: 0,
        },
        items: [
          { item_id: 'a', title: 'Design schema', status: 'Ready', order: 0, decision: 'would_dispatch', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
          { item_id: 'b', title: 'Build API', status: 'Ready', order: 1, decision: 'would_dispatch', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
          { item_id: 'c', title: 'Write docs', status: 'Ready', order: 2, decision: 'waiting_on_dependencies', blocked_by: ['a'], policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
        ],
      }),
    }),
  );

  await page.goto(`/projects/${projectId}/sprint`);
  await waitForApp(page);
  await page.getByRole('button', { name: `Run sprint: ${uniqueSprintName}` }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.getByText('Design schema')).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('sprint dispatch results (mixed outcomes) has no accessibility violations', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  // See the sibling "dry-run preview" test above for why this needs its own
  // unique sprint name: the project is shared across this spec file, so a
  // sprint from another test can still be eligible for its own, distinct
  // "Run sprint" button.
  const sprintName = 'E2E Sprint (dispatch results)';
  const { sprintId, sprintName: uniqueSprintName } = await createSprintWithItem(
    request,
    projectId,
    sprintName,
  );

  await page.route(`**/api/projects/${projectId}/agent-activity`, (route) =>
    route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ rows: [] }) }),
  );
  await page.route(`**/api/sprints/${sprintId}/dispatch/dry-run`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        sprint_id: sprintId,
        max_in_flight: 2,
        summary: {
          total: 2,
          dispatched: 0,
          waiting_approval: 0,
          blocked: 0,
          already_in_flight: 0,
          waiting_on_dependencies: 0,
          not_eligible: 0,
          no_dispatch_policy: 0,
          would_dispatch: 2,
          errored: 0,
        },
        items: [
          { item_id: 'a', title: 'Item A', status: 'Ready', order: 0, decision: 'would_dispatch', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
          { item_id: 'b', title: 'Item B', status: 'Ready', order: 1, decision: 'would_dispatch', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
        ],
      }),
    }),
  );
  // Trailing `*` (not `**`) is required: `DispatchSprintModal`'s "Confirm
  // dispatch" pre-fills the in-flight cap from the dry-run response and
  // always sends it as a query param (card C4's fix #1 — `?max_in_flight=N`,
  // never a JSON body), so the real POST URL is
  // `.../dispatch?max_in_flight=2`, not the bare path. Playwright glob
  // routes are anchored (`^...$`), so an unqualified `.../dispatch` pattern
  // never matches a URL with a query string and this route silently never
  // fires, falling through to the real (orchestration-disabled) backend. A
  // single `*` is enough since query strings never contain `/`, so this
  // still can't accidentally swallow the sibling `.../dispatch/dry-run`
  // route registered above.
  await page.route(`**/api/sprints/${sprintId}/dispatch*`, (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        sprint_id: sprintId,
        max_in_flight: 2,
        summary: {
          total: 2,
          dispatched: 1,
          waiting_approval: 1,
          blocked: 0,
          already_in_flight: 0,
          waiting_on_dependencies: 0,
          not_eligible: 0,
          no_dispatch_policy: 0,
          would_dispatch: 0,
          errored: 0,
        },
        items: [
          { item_id: 'a', title: 'Item A', status: 'Ready', order: 0, decision: 'dispatched', blocked_by: null, policy_id: null, message: null, status_applied: 'In Progress', status_map_rejected: null, approval_token: null, current_status: null, dispatch_from: null, error: null, task: null },
          { item_id: 'b', title: 'Item B', status: 'Ready', order: 1, decision: 'waiting_approval', blocked_by: null, policy_id: null, message: null, status_applied: null, status_map_rejected: null, approval_token: 'tok-1', current_status: null, dispatch_from: null, error: null, task: null },
        ],
      }),
    });
  });

  await page.goto(`/projects/${projectId}/sprint`);
  await waitForApp(page);
  await page.getByRole('button', { name: `Run sprint: ${uniqueSprintName}` }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await page.getByRole('button', { name: /Confirm dispatch/ }).click();
  // Never a merged "2 dispatched" — the two outcomes stay in separate, named counts.
  await expect(page.getByText('1 waiting on approval')).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// Approvals inbox (frontend/src/features/approvals/**, TODO.md §6 "D1",
// tasks 36.1/36.2). Same `page.route()` interception technique as the Fleet
// and dispatch scans above — this harness's webServer doesn't set
// `TACK_ORCH_ENABLE`, so the disabled state below is the real default, and
// there's no seeding UI for `orch_approvals` rows to populate the page
// through the real API either.

test('approvals inbox (orchestration disabled) has no accessibility violations', async ({ page }) => {
  await page.goto('/approvals');
  await waitForApp(page);
  await expect(page.getByText('Agent-fleet orchestration is disabled')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

const mockApprovalRows = [
  {
    token: 'apr-uncorrelated',
    control_plane_id: 'cp-1',
    control_plane_name: 'docket-prod',
    item_id: null,
    item_title: null,
    item_status: null,
    project_id: null,
    project_name: null,
    remote_task_id: null,
    agent: 'cli-agent',
    action: 'rm -rf /tmp/build',
    requested_at: new Date(Date.now() - 3_600_000).toISOString(),
  },
  {
    token: 'apr-correlated',
    control_plane_id: 'cp-1',
    control_plane_name: 'docket-prod',
    item_id: 'e2e-approvals-item',
    item_title: 'Deploy service',
    item_status: 'In Progress',
    project_id: 'e2e-approvals-project',
    project_name: 'Backend',
    remote_task_id: 'task-1',
    agent: 'builder',
    action: 'git push origin main',
    requested_at: new Date().toISOString(),
  },
];

test('approvals inbox (populated, decisions enabled) has no accessibility violations', async ({
  page,
}) => {
  await page.route('**/api/approvals', (route) => {
    if (route.request().method() !== 'GET') return route.fallback();
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ rows: mockApprovalRows, grant_available: true }),
    });
  });

  await page.goto('/approvals');
  await waitForApp(page);
  await expect(page.getByText('Deploy service')).toBeVisible();
  // The uncorrelated approval — the one this whole inbox exists to surface —
  // must actually render, not be silently dropped.
  await expect(page.getByText(/Uncorrelated/)).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('approvals inbox confirmation modal has no accessibility violations', async ({ page }) => {
  await page.route('**/api/approvals', (route) => {
    if (route.request().method() !== 'GET') return route.fallback();
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ rows: mockApprovalRows, grant_available: true }),
    });
  });

  await page.goto('/approvals');
  await waitForApp(page);
  await page.getByRole('button', { name: 'Grant' }).first().click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.getByText('cannot be undone')).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('approvals inbox without a saved browser decision credential has no accessibility violations', async ({
  page,
}) => {
  await page.route('**/api/approvals', (route) => {
    if (route.request().method() !== 'GET') return route.fallback();
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      // The server no longer guesses decision availability in a list response.
      // Authorization is decided by the separately supplied browser credential
      // on the real POST, so the action controls remain visible here.
      body: JSON.stringify({ rows: mockApprovalRows }),
    });
  });

  await page.goto('/approvals');
  await waitForApp(page);
  await expect(page.getByText('Deploy service')).toBeVisible();
  await expect(page.getByRole('textbox', { name: 'Your approval token' })).toHaveValue('');
  await expect(page.getByRole('button', { name: 'Grant' })).toHaveCount(mockApprovalRows.length);

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// Project Settings → Orchestration tab (frontend/src/features/settings/
// orchestration/**, TODO.md §6 "D2", tasks 36.3/36.4: budget + policy
// panels). Same `page.route()` interception technique as the Fleet/
// Approvals scans above — `GET /api/projects/{id}/orch-link`,
// `GET /api/control-planes`, `GET /api/projects/{id}/orch-budget`, and
// `GET /api/projects/{id}/orch-policy` are all mocked; the real project (via
// `getOrCreateProject`) and the real `ProjectSettings`/`OrchestrationPanel`/
// `LinkForm`/`BudgetPanel`/`PolicyPanel` components render and are scanned
// against that mocked data. The disabled-state scan needs no interception —
// `TACK_ORCH_ENABLE` is unset in this harness, same real default the Fleet
// scan above relies on.

test('project settings — orchestration tab (disabled) has no accessibility violations', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  await page.goto(`/projects/${projectId}/settings?tab=orchestration`);
  await waitForApp(page);
  await expect(page.getByText('Agent-fleet orchestration is disabled')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('project settings — orchestration tab (unlinked, link form) has no accessibility violations', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);

  await page.route(`**/api/projects/${projectId}/orch-link`, (route) =>
    route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ linked: false, link: null }) }),
  );
  await page.route('**/api/control-planes', (route) => {
    if (route.request().method() !== 'GET') return route.fallback();
    return route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([{ id: 'cp-1', name: 'docket-prod', kind: 'docket', health: 'healthy' }]),
    });
  });

  await page.goto(`/projects/${projectId}/settings?tab=orchestration`);
  await waitForApp(page);
  await expect(page.getByText('Link this project to a control plane')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('project settings — orchestration tab (linked, budget + policy populated) has no accessibility violations', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);

  await page.route(`**/api/projects/${projectId}/orch-link`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        linked: true,
        link: {
          project_id: projectId,
          control_plane_id: 'cp-1',
          remote_project: 'e2e-remote-project',
          pipeline_file: null,
          blueprint: null,
          auto_dispatch: false,
          budget_usd: 100,
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        },
      }),
    }),
  );
  await page.route(`**/api/projects/${projectId}/orch-budget`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        linked: true,
        control_plane_id: 'cp-1',
        control_plane_name: 'docket-prod',
        health: 'healthy',
        budget_usd: 100,
        tokens_in: 128_400,
        tokens_out: 45_200,
        cost_usd_estimated: 92.5,
        pricing_snapshot_at: null,
      }),
    }),
  );
  await page.route(`**/api/projects/${projectId}/orch-policy`, (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        linked: true,
        control_plane_id: 'cp-1',
        control_plane_name: 'docket-prod',
        health: 'healthy',
        scoped_to_control_plane_only: true,
        scraped_at: new Date().toISOString(),
        tool_calls: [
          { decision: 'allow', count: 42 },
          { decision: 'ask', count: 5 },
          { decision: 'deny', count: 3 },
        ],
        denial_rate: 0.06,
        policy_hits: [
          { policy_id: 'no-prod-secrets', hook: 'pre_tool_call', action: 'deny', count: 3 },
        ],
        approvals_by_channel: [
          { channel: 'tack', outcome: 'granted', count: 4 },
          { channel: 'timeout', outcome: 'denied', count: 1 },
        ],
      }),
    }),
  );

  await page.goto(`/projects/${projectId}/settings?tab=orchestration`);
  await waitForApp(page);
  // Over-90%-of-cap state — asserts the warning-band progress bar rendered,
  // not just that some text appeared.
  await expect(page.getByText('no-prod-secrets')).toBeVisible();
  await expect(page.getByRole('progressbar')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// Unit economics dashboard (frontend/src/features/economics/**, TODO.md §6
// "D5", tasks 38.1-38.4). Same `page.route()` interception technique as the
// Fleet/Approvals/Orchestration-tab scans above — this harness's webServer
// doesn't set `TACK_ORCH_ENABLE`, so the disabled state below is the real,
// unmodified default. The populated scan mocks `GET /api/economics/summary`
// with a shape matching `frontend/src/features/economics/api.ts`'s
// `EconomicsSummaryResponse` — including a below-min-sample slice (raw hours,
// not an average) and a slice with excluded-stale rework attempts, since
// those are the two states most likely to introduce an a11y issue (extra
// badges/caveat text next to a number) that an all-populated fixture would
// never exercise.

test('economics page (orchestration disabled) has no accessibility violations', async ({ page }) => {
  await page.goto('/economics');
  await waitForApp(page);
  await expect(page.getByText('Agent-fleet orchestration is disabled')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('economics page (no completed items yet) has no accessibility violations', async ({ page }) => {
  await page.route('**/api/economics/summary', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        generated_at: new Date().toISOString(),
        min_sample_size: 5,
        events_retention_days: 90,
        overall: {
          key: 'overall',
          completed_item_count: 0,
          agent_completed_count: 0,
          human_completed_count: 0,
          tokens_in: 0,
          tokens_out: 0,
          cost_usd_estimated: null,
          pricing_snapshot_at: null,
          cost_usd_estimated_per_item: null,
          agent_lead_time: { sample_count: 0, below_min_sample: true, avg_hours: null, raw_hours: null },
          human_lead_time: { sample_count: 0, below_min_sample: true, avg_hours: null, raw_hours: null },
          lead_time_selection_bias_note: 'Items dispatched to agents are not a random sample of all work.',
          rework: {
            attempts_total: 0,
            attempts_excluded_stale: 0,
            attempts_with_rework_signal: 0,
            below_min_sample: true,
            rate: null,
            definition: 'Share of dispatched items with a qualifying rework event.',
            truncation_note: 'Rework signals age out after the configured retention window.',
          },
        },
        by_project_type: [],
        by_item_type: [],
      }),
    }),
  );
  await page.goto('/economics');
  await waitForApp(page);
  await expect(page.getByText('No completed items yet')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('economics page (populated, below-min-sample and stale-rework states) has no accessibility violations', async ({
  page,
}) => {
  const belowMinSlice = (key: string) => ({
    key,
    completed_item_count: 3,
    agent_completed_count: 2,
    human_completed_count: 1,
    tokens_in: 4_200,
    tokens_out: 1_800,
    cost_usd_estimated: 0.42,
    pricing_snapshot_at: null,
    cost_usd_estimated_per_item: null,
    agent_lead_time: { sample_count: 2, below_min_sample: true, avg_hours: null, raw_hours: [3.5, 8.1] },
    human_lead_time: { sample_count: 1, below_min_sample: true, avg_hours: null, raw_hours: [12.0] },
    lead_time_selection_bias_note:
      'Items dispatched to agents are not a random sample of all work — auto-dispatch fires only on specific statuses.',
    rework: {
      attempts_total: 2,
      attempts_excluded_stale: 1,
      attempts_with_rework_signal: 1,
      below_min_sample: true,
      rate: null,
      definition:
        'Share of dispatched items (completed, with at least one docket dispatch) that have at least one rework_started, verification_failed, or tester_verdict_failed event recorded against them.',
      truncation_note:
        'Rework signals come from mirrored docket events, which age out after the configured retention window.',
    },
  });

  await page.route('**/api/economics/summary', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({
        generated_at: new Date().toISOString(),
        min_sample_size: 5,
        events_retention_days: 90,
        overall: belowMinSlice('overall'),
        by_project_type: [belowMinSlice('software'), belowMinSlice('construction')],
        by_item_type: [belowMinSlice('task'), belowMinSlice('bug')],
      }),
    }),
  );

  await page.goto('/economics');
  await waitForApp(page);
  await expect(page.getByRole('heading', { name: 'By project type' })).toBeVisible();
  await expect(page.getByText('too few').first()).toBeVisible();
  await expect(page.getByText(/excluded/).first()).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// Provisioning wizard (frontend/src/features/provisioning/**, TODO.md §6
// "D4", tasks 37.2/37.4). Same orchestration-disabled-by-default reality as
// the Fleet/dispatch scans above: this harness's webServer does not set
// TACK_ORCH_ENABLE, so the disabled scan below is the real default. The
// populated scan intercepts `GET /api/control-planes` and `GET /api/templates`
// (the two reads the wizard's gate + step 1/2 pickers need) and walks all the
// way to the confirmation `Modal` — the highest-risk state for focus
// trapping/labelling, and the one state C4's dispatch-confirmation precedent
// and D1's approval-decision precedent both flagged as worth scanning
// explicitly rather than assuming a generic `Modal` pass elsewhere covers it.

test('provisioning wizard (orchestration disabled) has no accessibility violations', async ({ page }) => {
  await page.goto('/provision');
  await waitForApp(page);
  await expect(page.getByText('Orchestration is disabled')).toBeVisible();
  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('provisioning wizard (confirmation modal open) has no accessibility violations', async ({ page }) => {
  await page.route('**/api/control-planes', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([{ id: 'cp-1', name: 'docket-e2e', kind: 'docket', health: 'healthy' }]),
    }),
  );
  await page.route('**/api/templates', (route) =>
    route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([
        { id: 'tmpl-1', name: 'Software starter', project_type: 'software', orchestration: null },
      ]),
    }),
  );

  await page.goto('/provision');
  await waitForApp(page);
  await expect(page.getByText('1. Project')).toBeVisible();

  await page.getByLabel('Project name').fill('E2E Provisioned Project');
  await page.getByLabel('Template').selectOption('tmpl-1');
  await page.getByRole('button', { name: 'Next: Pod' }).click();

  await page.getByLabel('Control plane').selectOption('cp-1');
  await page.getByRole('button', { name: 'Next: Review' }).click();

  await page.getByRole('button', { name: 'Provision…' }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
  await expect(page.getByText('cannot be automatically undone')).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// "Run with agent" (frontend/src/shared/runWithAgent/**, TODO.md III-E4,
// Wave 4 / Phase 54). Unlike every Docket dispatch scan above, these routes
// are NOT gated behind `TACK_ORCH_ENABLE` (see `run-with-agent.spec.ts`'s
// own header comment) — no orchestration setup needed to reach this
// surface's populated state. The two states scanned are the highest-risk
// ones for focus/labelling per this file's own established precedent (a
// modal with several distinct field groups, and a tab panel rendering a
// list with inline action controls).

test('Board card "Run with agent" modal has no accessibility violations', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  await getOrCreateItem(request, projectId, `A11y RWA board ${Date.now()}`);

  await page.goto(`/projects/${projectId}/board`);
  await waitForApp(page);
  await page.getByRole('button', { name: /^Run with agent:/ }).first().click();
  await expect(page.getByRole('dialog', { name: /^Run with agent:/ })).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

test('item detail Execution tab (with a real request) has no accessibility violations', async ({ page, request }) => {
  const projectId = await getOrCreateProject(request);
  // A guaranteed-fresh item — see `run-with-agent.spec.ts`'s identical note
  // on why `getOrCreateItem` would accumulate state across repeated runs.
  const itemId = await createFreshItem(request, projectId, `A11y RWA detail ${Date.now()}`);
  const fleetId = await createFleet(request, `A11y Fleet ${Date.now()}`);
  const profileId = await createAgentProfile(request, `A11y Profile ${Date.now()}`);

  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);
  const drawer = page.getByRole('dialog');
  await drawer.getByRole('button', { name: 'Run with agent' }).click();

  const modal = page.getByRole('dialog', { name: /^Run with agent:/ });
  await modal.getByRole('combobox', { name: 'Fleet' }).selectOption(fleetId);
  await modal.getByRole('combobox', { name: 'Agent profile' }).selectOption(profileId);
  await modal.getByLabel('Remote').fill('git@example.com:org/repo.git');
  await modal.getByRole('button', { name: 'Run' }).click();
  await expect(modal).toBeHidden();

  await drawer.getByRole('tab', { name: 'Execution' }).click();
  await expect(drawer.getByText('Queued')).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});

// III-F4: the attempt-detail panel (`AttemptList.tsx` — model provenance,
// usage economics, and the expanded events/decisions/artifacts sections).
// Scans with a real claimed attempt so the expanded state (radios, text
// fields, buttons across `EventTimeline`/`DecisionInbox`/
// `ArtifactDownloadPanel`) is actually present in the DOM, not just the
// collapsed row.
test('item detail Execution tab — expanded attempt detail (events/decisions/artifacts) has no accessibility violations', async ({
  page,
  request,
}) => {
  const projectId = await getOrCreateProject(request);
  const itemId = await createFreshItem(request, projectId, `A11y attempt detail ${Date.now()}`);
  const profileId = await createAgentProfile(request, `A11y Attempt Profile ${Date.now()}`);
  const modelId = 'opaque/model-alpha';

  const { runnerId, credential } = await enrollRunner(request, `A11y Attempt Runner ${Date.now()}`, modelId);
  const requestId = await createExecution(request, itemId, runnerId, profileId, modelId);
  const lease = await claimOnceWithLease(request, runnerId, credential, `a11y-attempt-claim-${Date.now()}`);
  expect(lease?.requestId).toBe(requestId);

  await page.goto(`/projects/${projectId}/board?item=${itemId}`);
  await waitForApp(page);
  const drawer = page.getByRole('dialog');
  await drawer.getByRole('tab', { name: 'Execution' }).click();
  await expect(drawer.getByText('Attempt #1')).toBeVisible();

  await drawer.getByRole('button', { name: /Show events, decisions & artifacts/ }).click();
  await expect(drawer.getByText('No events reported yet')).toBeVisible();

  const violations = await scan(page);
  expect(violations, JSON.stringify(violations.map((v) => v.id), null, 2)).toEqual([]);
});
