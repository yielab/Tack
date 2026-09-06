import { type Page, type APIRequestContext, expect } from '@playwright/test';

// Shared helpers for E2E specs. The single source of truth for the app's API
// response shapes lives here so a backend contract change is fixed in one place.
//
// Response-shape notes (verified against crates/tack-api/src/handlers):
//   GET  /api/projects        -> Project[]            (plain array, no envelope)
//   POST /api/projects        -> Project              (the created object)
//   GET  /api/projects/:id/items -> { data: Item[], total, page, per_page }  (paginated envelope)
//   POST /api/projects/:id/items -> { id } | Item     (id is always present)
//   GET  /api/items/:id       -> { item, roles, dependencies }  (detail envelope)

// Test *setup* talks to the API server directly (deterministic), rather than
// through the Vite dev proxy. The browser `page` still uses the relative /api
// path so the proxy/same-origin behaviour is exercised by the real app.
export const API_ORIGIN = process.env.E2E_API_ORIGIN || 'http://127.0.0.1:3210';
export const API = `${API_ORIGIN}/api`;

/**
 * Wait for the SolidJS SPA to be ready after a navigation. We deliberately do
 * NOT use 'networkidle' — the app holds a persistent board WebSocket (with a
 * keepalive Ping), so the network never goes idle. 'domcontentloaded' plus the
 * web-first auto-waiting assertions in each test is the robust primitive.
 */
export async function waitForApp(page: Page): Promise<void> {
  await page.waitForLoadState('domcontentloaded');
}

/**
 * Ensure at least one project exists and return its id. Shape-agnostic: it
 * creates via POST when empty, then re-reads the list so it never depends on the
 * create response body.
 */
export async function getOrCreateProject(request: APIRequestContext): Promise<string> {
  const existing = await request.get(`${API}/projects`).then((r) => r.json());
  if (Array.isArray(existing) && existing.length) return existing[0].id;

  const res = await request.post(`${API}/projects`, {
    data: { name: 'E2E Project', project_type: 'software', description: 'created by e2e' },
  });
  expect(res.ok(), `create project failed: ${res.status()}`).toBeTruthy();

  const list = await request.get(`${API}/projects`).then((r) => r.json());
  expect(Array.isArray(list) && list.length, 'project list empty after create').toBeTruthy();
  return list[0].id;
}

/**
 * Create a brand-new project and return its id — for a test whose own
 * setup writes something to the project itself that has no way back
 * (e.g. `default_model`, which the API has no route to clear once set).
 * Reusing `getOrCreateProject`'s shared project for that kind of write
 * would permanently change what every other spec in this suite sees on a
 * reused `e2e.db`.
 */
export async function createFreshProject(request: APIRequestContext, name: string): Promise<string> {
  const res = await request.post(`${API}/projects`, {
    data: { name, project_type: 'software', description: 'created by e2e' },
  });
  expect(res.ok(), `create project failed: ${res.status()}`).toBeTruthy();
  const body = await res.json();
  if (body?.id) return body.id;
  const list = await request.get(`${API}/projects`).then((r) => r.json());
  return list.at(-1).id;
}

/**
 * Sets a project's default-model tier directly, via the same
 * `PATCH /api/projects/:id` route `features/settings/panels/AgentsPanel.tsx`'s
 * "Project default model" field writes. Only usable on a project from
 * {@link createFreshProject}, never the shared one `getOrCreateProject`
 * returns — see that function's own doc comment on why.
 */
export async function setProjectDefaultModel(
  request: APIRequestContext,
  projectId: string,
  provider: string,
  modelId: string,
): Promise<void> {
  const res = await request.patch(`${API}/projects/${projectId}`, {
    data: { default_model: { kind: 'explicit', provider, model_id: modelId } },
  });
  expect(res.ok(), `set project default model failed: ${res.status()}`).toBeTruthy();
}

/** Ensure the given project has at least one item and return its id. */
export async function getOrCreateItem(
  request: APIRequestContext,
  projectId: string,
  title = 'E2E Item',
): Promise<string> {
  const existing = await request
    .get(`${API}/projects/${projectId}/items`)
    .then((r) => r.json())
    .then((p) => p.data ?? []);
  if (existing.length) return existing[0].id;

  const res = await request.post(`${API}/projects/${projectId}/items`, {
    data: { title, item_type: 'task' },
  });
  expect(res.ok(), `create item failed: ${res.status()}`).toBeTruthy();
  const body = await res.json();
  if (body?.id) return body.id;

  const list = await request
    .get(`${API}/projects/${projectId}/items`)
    .then((r) => r.json())
    .then((p) => p.data ?? []);
  return list[0].id;
}

/**
 * Always create a fresh item carrying an `assignee`, so callers get a
 * populated `<Avatar>` (Board.tsx only renders one when `item.assignee` is
 * set) — unlike `getOrCreateItem`, this doesn't reuse an existing assignee-less
 * item. Returns the item id.
 */
export async function createItemWithAssignee(
  request: APIRequestContext,
  projectId: string,
  assignee: string,
  title = 'E2E Item (assigned)',
): Promise<string> {
  const res = await request.post(`${API}/projects/${projectId}/items`, {
    data: { title, item_type: 'task', assignee },
  });
  expect(res.ok(), `create item failed: ${res.status()}`).toBeTruthy();
  const body = await res.json();
  if (body?.id) return body.id;

  const list = await request
    .get(`${API}/projects/${projectId}/items`)
    .then((r) => r.json())
    .then((p) => p.data ?? []);
  const match = list.find((it: { assignee?: string }) => it.assignee === assignee);
  return (match ?? list[list.length - 1]).id;
}

/**
 * Create a fresh sprint with one item assigned to it — the minimum the
 * Sprints view's "Run sprint" dispatch control needs to render at all
 * (`Sprints.tsx` only shows the button for a sprint with `itemsForSprint(id).
 * length > 0`, TODO.md Wave 3, card C4). Returns both ids since the caller
 * typically needs the sprint id (to mock its dry-run route) and doesn't
 * otherwise have one.
 */
export async function createSprintWithItem(
  request: APIRequestContext,
  projectId: string,
  sprintName = 'E2E Sprint',
): Promise<{ sprintId: string; itemId: string; sprintName: string }> {
  // The caller's project is shared (`getOrCreateProject` reuses one per spec
  // file) and `e2e.db` survives between runs, so a fixed name accumulates a
  // fresh identically-named sprint on every run. Card F1 gave each button an
  // accessible name including its sprint's name, which disambiguates two
  // *differently* named sprints — but not six sprints that all share one name,
  // which is what repeated runs actually produce. Suffixing here makes the name
  // unique per invocation, so the accessible name is unique too and a
  // `getByRole` locator resolves to exactly one button.
  const uniqueName = `${sprintName} ${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2, 7)}`;
  const sprintRes = await request.post(`${API}/projects/${projectId}/sprints`, {
    data: { name: uniqueName },
  });
  expect(sprintRes.ok(), `create sprint failed: ${sprintRes.status()}`).toBeTruthy();
  const sprint = await sprintRes.json();

  const itemRes = await request.post(`${API}/projects/${projectId}/items`, {
    data: { title: 'E2E Sprint Item', item_type: 'task' },
  });
  expect(itemRes.ok(), `create item failed: ${itemRes.status()}`).toBeTruthy();
  const itemBody = await itemRes.json();
  const itemId: string =
    itemBody?.id ??
    (await request
      .get(`${API}/projects/${projectId}/items`)
      .then((r) => r.json())
      .then((p) => p.data.at(-1).id));

  const patchRes = await request.patch(`${API}/items/${itemId}`, {
    data: { sprint_id: sprint.id },
  });
  expect(patchRes.ok(), `assign item to sprint failed: ${patchRes.status()}`).toBeTruthy();

  return { sprintId: sprint.id, itemId, sprintName: uniqueName };
}

/**
 * Always create a brand-new item (unlike `getOrCreateItem`, which reuses
 * whatever item already exists in the project) — needed by any test that
 * asserts something about an item's OWN accumulated state (e.g. "exactly
 * one execution request exists for this item"), where reusing a
 * project-shared item across repeated runs against the same persistent
 * `e2e.db` would silently accumulate state from earlier runs and make the
 * assertion flaky. Returns the new item's id.
 */
export async function createFreshItem(
  request: APIRequestContext,
  projectId: string,
  title: string,
): Promise<string> {
  const res = await request.post(`${API}/projects/${projectId}/items`, {
    data: { title, item_type: 'task' },
  });
  expect(res.ok(), `create item failed: ${res.status()}`).toBeTruthy();
  const body = await res.json();
  if (body?.id) return body.id;
  const list = await request
    .get(`${API}/projects/${projectId}/items`)
    .then((r) => r.json())
    .then((p) => p.data ?? []);
  return list.at(-1).id;
}

/**
 * Create a runner fleet via the operator execution surface
 * (`POST /api/runner-fleets`, TODO.md III-E4's "Run with agent" modal target
 * picker). Unlike the Docket dispatch helpers above, this route is NOT
 * gated behind `TACK_ORCH_ENABLE` — see `crates/tack-api/src/router.rs`'s
 * own comment distinguishing `orch_routes` from card C1's always-on
 * operator execution/fleet routes. Returns the new fleet's id.
 */
export async function createFleet(request: APIRequestContext, name: string): Promise<string> {
  const res = await request.post(`${API}/runner-fleets`, { data: { name } });
  expect(res.ok(), `create fleet failed: ${res.status()}`).toBeTruthy();
  const body = await res.json();
  return body.fleet_id;
}

/** Create an agent profile (`POST /api/agent-profiles`) — same always-on
 *  operator surface as {@link createFleet}. Returns the new profile's id. */
export async function createAgentProfile(request: APIRequestContext, name: string): Promise<string> {
  const res = await request.post(`${API}/agent-profiles`, {
    data: { name, instructions: 'Review the change and leave comments.', tool_policy: { read: true } },
  });
  expect(res.ok(), `create agent profile failed: ${res.status()}`).toBeTruthy();
  const body = await res.json();
  return body.agent_profile_id;
}

/** Create a model profile (`POST /api/model-profiles`) — the picker
 *  `RunWithAgentModal.tsx`'s "Choose a model" mode lists. Returns the new
 *  profile's id. */
export async function createModelProfile(
  request: APIRequestContext,
  name: string,
  modelProvider: string,
  modelId: string,
): Promise<string> {
  const res = await request.post(`${API}/model-profiles`, {
    data: { name, model_provider: modelProvider, model_id: modelId },
  });
  expect(res.ok(), `create model profile failed: ${res.status()}`).toBeTruthy();
  const body = await res.json();
  return body.model_profile_id;
}

/**
 * A minimal, valid runner-v1 capability report declaring `codex`/`openai`/
 * `modelId` — for the direct runner-protocol HTTP calls
 * {@link enrollRunner}/{@link claimOnce} make (TODO.md III-E6). There is no
 * CLI/UI surface for the runner side of the protocol (enroll/refresh/claim
 * are `tack-runner`'s job, a different binary/actor than the operator UI
 * these specs otherwise drive) — these two helpers speak it directly, as a
 * real runner would.
 */
function capabilities(modelId: string) {
  const now = new Date().toISOString();
  return {
    reported_at: now,
    labels: {},
    concurrency: { total: 1, available: 1 },
    harnesses: [
      {
        harness_kind: 'codex',
        installed_version: '1.0.0',
        probe_error: null,
        probed_at: now,
        model_combinations: [{ model_provider: 'openai', model_ids: [modelId], discovery: 'reported' }],
      },
    ],
    features: {},
    limits: { event_payload_bytes_max: 65536, artifact_content_bytes_max: 52428800 },
  };
}

/**
 * Enrolls a runner as `tack-runner` would: `POST /api/runners/enrollment`
 * (operator side, issues the one-time token) then `POST
 * /api/runner/v1/enroll` (the runner side of the exchange). Returns the
 * runner id and its bearer credential for later {@link claimOnce} calls.
 */
export async function enrollRunner(
  request: APIRequestContext,
  name: string,
  modelId: string,
  capacity = 1,
): Promise<{ runnerId: string; credential: string }> {
  const pendingRes = await request.post(`${API}/runners/enrollment`, {
    data: { name, total_capacity: capacity, available_capacity: capacity },
  });
  expect(pendingRes.ok(), `create pending runner failed: ${pendingRes.status()}`).toBeTruthy();
  const pending = await pendingRes.json();

  const enrollRes = await request.post(`${API}/runner/v1/enroll`, {
    data: {
      protocol_version: 1,
      enrollment_token: pending.enrollment_token,
      runner_name: name,
      runner_version: '0.1.0',
      capabilities: capabilities(modelId),
    },
  });
  expect(enrollRes.ok(), `runner enroll failed: ${enrollRes.status()}`).toBeTruthy();
  const enrolled = await enrollRes.json();
  return { runnerId: pending.runner_id as string, credential: enrolled.runner_credential as string };
}

/**
 * Polls `POST /api/runner/v1/claim` once, as `tack-runner` would each
 * cycle. Returns the claimed `request_id`, or `null` for a `no work`
 * response.
 */
export async function claimOnce(
  request: APIRequestContext,
  runnerId: string,
  credential: string,
  claimRequestId: string,
): Promise<string | null> {
  const res = await request.post(`${API}/runner/v1/claim`, {
    headers: { authorization: `Bearer ${credential}` },
    data: {
      protocol_version: 1,
      runner_id: runnerId,
      claim_request_id: claimRequestId,
      available_capacity: 1,
      wait_ms: 0,
    },
  });
  expect(res.ok(), `claim failed: ${res.status()}`).toBeTruthy();
  const body = await res.json();
  return body?.request?.request_id ?? null;
}

// ─── III-F4: attempt-detail additions (decisions/artifacts/events) ─────────
//
// The four helpers below extend the runner-protocol simulation `claimOnce`
// already established, far enough to get a real attempt into `running`
// state and raise a real decision/artifact — the only way to prove
// `DecisionInbox`/`ArtifactDownloadPanel`'s "happy path" against the real
// production router, since no CLI/UI surface exists for the runner side of
// this protocol (the same reasoning `enrollRunner`/`claimOnce` themselves
// document). See `execution-attempt-detail.spec.ts`.

/** Full lease detail from a claim response (`claimOnce` above only extracts
 *  `request_id`, which is all the pre-existing scheduler specs needed) —
 *  `attempt_id`/`fencing_token` are required to drive every subsequent
 *  runner-protocol call in this file. Returns `null` for a `no work`
 *  response, matching `claimOnce`'s own contract. */
export async function claimOnceWithLease(
  request: APIRequestContext,
  runnerId: string,
  credential: string,
  claimRequestId: string,
): Promise<{ requestId: string; attemptId: string; fencingToken: number } | null> {
  const res = await request.post(`${API}/runner/v1/claim`, {
    headers: { authorization: `Bearer ${credential}` },
    data: {
      protocol_version: 1,
      runner_id: runnerId,
      claim_request_id: claimRequestId,
      available_capacity: 1,
      wait_ms: 0,
    },
  });
  expect(res.ok(), `claim failed: ${res.status()}`).toBeTruthy();
  const body = await res.json();
  if (!body?.request?.request_id) return null;
  return {
    requestId: body.request.request_id as string,
    attemptId: body.lease.attempt_id as string,
    fencingToken: body.lease.fencing_token as number,
  };
}

/** Drives a claimed attempt through `accept` then `start` — as `tack-runner`
 *  would before doing any real work — so its `execution_attempts.state`
 *  reaches `running`, the precondition `create_decision`/`submit_artifacts`
 *  both require. */
export async function acceptAndStartAttempt(
  request: APIRequestContext,
  runnerId: string,
  credential: string,
  attemptId: string,
  fencingToken: number,
): Promise<void> {
  const base = {
    protocol_version: 1,
    runner_id: runnerId,
    attempt_id: attemptId,
    fencing_token: fencingToken,
    workspace_id: `ws-${attemptId}`,
    base_revision: '0'.repeat(40),
  };
  const acceptRes = await request.post(`${API}/runner/v1/attempts/${attemptId}/accept`, {
    headers: { authorization: `Bearer ${credential}` },
    data: base,
  });
  expect(acceptRes.ok(), `accept failed: ${acceptRes.status()}`).toBeTruthy();
  const startRes = await request.post(`${API}/runner/v1/attempts/${attemptId}/start`, {
    headers: { authorization: `Bearer ${credential}` },
    data: { ...base, process_id: 'e2e-fake-process' },
  });
  expect(startRes.ok(), `start failed: ${startRes.status()}`).toBeTruthy();
}

/** Raises a real, pending decision on a running attempt — as a harness
 *  would via `create_decision` (`crates/tack-api/src/handlers/
 *  runner_protocol.rs`). Returns the `decision_id` the test supplied, for
 *  convenience at call sites. */
export async function createRunnerDecision(
  request: APIRequestContext,
  runnerId: string,
  credential: string,
  attemptId: string,
  fencingToken: number,
  decisionId: string,
  options: Array<{ option_id: string; label: string }>,
): Promise<string> {
  const res = await request.post(`${API}/runner/v1/attempts/${attemptId}/decisions`, {
    headers: { authorization: `Bearer ${credential}` },
    data: {
      protocol_version: 1,
      runner_id: runnerId,
      attempt_id: attemptId,
      fencing_token: fencingToken,
      decision_id: decisionId,
      kind: 'permission',
      prompt: 'e2e: allow this action?',
      options,
    },
  });
  expect(res.ok(), `create_decision failed: ${res.status()}`).toBeTruthy();
  return decisionId;
}

/** Manifests + uploads a real, verified artifact on a running attempt —
 *  `submit_artifacts` (manifest) then `PUT .../content` (III-F2), computing
 *  the real sha256 the streaming verifier checks against. Returns the
 *  `artifact_id` the test supplied. */
export async function submitRunnerArtifact(
  request: APIRequestContext,
  runnerId: string,
  credential: string,
  attemptId: string,
  fencingToken: number,
  artifactId: string,
  content: string,
): Promise<string> {
  const crypto = await import('node:crypto');
  const bytes = Buffer.from(content, 'utf-8');
  const sha256 = crypto.createHash('sha256').update(bytes).digest('hex');

  const manifestRes = await request.post(`${API}/runner/v1/attempts/${attemptId}/artifacts`, {
    headers: { authorization: `Bearer ${credential}` },
    data: {
      protocol_version: 1,
      runner_id: runnerId,
      attempt_id: attemptId,
      fencing_token: fencingToken,
      artifacts: [
        {
          artifact_id: artifactId,
          kind: 'diff',
          name: `${artifactId}.txt`,
          media_type: 'text/plain',
          size_bytes: bytes.length,
          sha256,
          content_disposition: 'inline_upload',
        },
      ],
    },
  });
  expect(manifestRes.ok(), `submit_artifacts failed: ${manifestRes.status()}`).toBeTruthy();

  const uploadRes = await request.put(`${API}/runner/v1/attempts/${attemptId}/artifacts/${artifactId}/content`, {
    headers: {
      authorization: `Bearer ${credential}`,
      'x-tack-fencing-token': String(fencingToken),
      'content-type': 'text/plain',
    },
    data: bytes,
  });
  expect(uploadRes.ok(), `artifact content upload failed: ${uploadRes.status()}`).toBeTruthy();
  return artifactId;
}

/** Creates an execution request directly (`POST /executions`) — faster and
 *  more deterministic than driving `RunWithAgentModal` through the UI when
 *  a test's real subject is what happens *after* the request exists.
 *  Field-for-field the same body `shared/runWithAgent/shared.ts#buildCreateExecutionInput`
 *  sends.
 *
 *  Uses an `exact_runner` selector, not `fleet` — `agent_fleet_members`
 *  still has no write route on any API surface (E6's own flagged gap,
 *  still open as of this card), so a `fleet`-selector request can never be
 *  claimed in an E2E environment; `scheduler-e2e.spec.ts` hit the same
 *  constraint and made the identical choice. Callers must enroll the
 *  target runner (`enrollRunner`) BEFORE calling this, so its id is known. */
export async function createExecution(
  request: APIRequestContext,
  itemId: string,
  runnerId: string,
  profileId: string,
  modelId: string,
): Promise<string> {
  const res = await request.post(`${API}/executions`, {
    data: {
      item_id: itemId,
      idempotency_key: `e2e-${Date.now()}-${Math.random()}`,
      selector_kind: 'exact_runner',
      selector_id: runnerId,
      agent_profile_id: profileId,
      requested_harness_kind: 'codex',
      requested_model_provider: 'openai',
      requested_model_id: modelId,
      agent_profile_snapshot: { name: 'e2e', instructions: 'e2e', tool_policy: {}, timeout_seconds: 3600, budgets: {} },
      repository_snapshot: { kind: 'git', remote: 'git@example.com:org/repo.git', base_revision: 'main', subdirectory: null },
      permission_policy: { tools: [], network: false },
      budgets: {},
      environment: {},
      metadata: {},
      timeout_seconds: 3600,
      status_map_policy_id: null,
    },
  });
  expect(res.ok(), `create execution failed: ${res.status()}`).toBeTruthy();
  const body = await res.json();
  return body.request_id as string;
}
