// Wire-format boundary for the operator execution/fleet/runner/profile
// surface. See `types.ts`'s header comment for why these shapes are
// hand-typed rather than imported from `../api/schema.gen`. Every shape in
// *this* file is copied directly from the live Rust handler that produces
// it — cited per section below.
//
// `crates/tack-api/src/handlers/executions.rs`/`runner_admin.rs` carry real
// `#[derive(ToSchema)]` DTOs and `docs/openapi.json`/
// `frontend/src/shared/api/schema.gen.ts` are regenerated and drift-checked.
// This file's hand-typed shapes were deliberately *not* migrated to import
// the generated types — matching each field to its generated equivalent
// across every consumer in this domain is a larger refactor, not done here
// — but every shape below was written to match the real handler
// field-for-field already, so that swap remains mechanical whenever
// whoever takes it on does.
//
// `GET /runners` exists (`runnersApi.list()` below) and is wired into
// `RunWithAgentModal.tsx`'s live capability fetch (see that file's own
// comment on why: with no live data, a *specific* model choice was
// unconditionally hard-blocked at every real call site — the only request
// shape a real operator could ever submit through the UI was `Auto`, which
// the scheduler (`tack_orch::scheduler`) always rejects too). Two gaps
// remain open, requested from a future owner:
//   - `GET /runner-fleets/{id}` (single fleet + roster) — only
//     `POST`/`GET` (list) exist; `RunnerSummary.fleet_ids` (via `GET
//     /runners`) is the only membership read path today.
//   - `GET /executions/{id}/attempts` (+ `.../attempts/{n}/events`) exist
//     server-side and are covered by
//     `crates/tack-api/tests/handlers/operator_read_routes.rs`, but are
//     deliberately not wrapped here yet — wiring them into
//     `store.ts#attemptsFor`'s `AttemptAvailability` union and every
//     consumer (`ExecutionTimeline.tsx` and their existing tests) is a
//     larger, ripple-through-multiple-test-suites refactor, not done here.
//     `ExecutionTimeline.tsx` still shows the typed "not available yet"
//     placeholder.
//
// **A wire inconsistency worth flagging alongside that gap:** `POST
// /executions/{id}/cancel`'s success response
// (`crates/tack-api/src/handlers/executions.rs`, `request_cancellation`,
// line ~515) hardcodes `"state":"cancellation_requested"` — a string that is
// NOT a member of `ExecutionState` (the frozen ten-state lifecycle has no
// such state; cancellation intent is instead the separate
// `cancellation_requested_at` timestamp column `list_executions`/
// `get_execution` already expose). A client that calls `cancel()` and then
// re-fetches via `get()` will see the row's REAL current lifecycle state
// (e.g. still `"running"`) with `cancellation_requested_at` populated, not a
// `"cancellation_requested"` state. `CancelExecutionResult.state` is typed
// as a bare `string` below (never merged into an `ExecutionState`) and
// `store.ts`'s optimistic update only ever touches
// `cancellation_requested_at`, never `state`, for exactly this reason.

import { request, requestWithHeaders } from '../api/client';

// ── Executions (`crates/tack-api/src/handlers/executions.rs`) ─────────────

/**
 * The row shape returned by both `GET /executions` (`list_executions`) and
 * `GET /executions/{id}` (`get_execution`) — five columns, verbatim from
 * each handler's own `SELECT` (executions.rs lines ~426-436, ~443-462).
 * This is deliberately much thinner than III.1.2's full execution-request
 * snapshot (no selector, no agent profile, no requested harness/model, no
 * repository/permission/budget/timeout data) — the handlers do not select
 * those columns even though `execution_requests` stores them. Do not assume
 * a field exists here just because it's in `ExecutionRequestSnapshot`.
 */
export interface ExecutionSummary {
  request_id: string;
  item_id: string;
  /** The real current lifecycle state (`ExecutionState` from `types.ts`),
   *  kept as `string` here (not narrowed) so an operator-surface value this
   *  build doesn't recognise still renders instead of failing to parse —
   *  the frozen contract has no `Unknown` escape hatch, but a defensive
   *  client is cheap insurance against a server/client version skew. */
  state: string;
  cancellation_requested_at: string | null;
  created_at: string;
}

export interface ExecutionListResult {
  protocol_version: number;
  data: ExecutionSummary[];
}

/**
 * `POST /executions` request body — every field from `CreateExecution`
 * (executions.rs lines 126-147). `selector_kind` is constrained to exactly
 * `"exact_runner" | "fleet"` server-side (line 229); `"any"` (a real
 * `RunnerSelector` variant per `types.ts`) is rejected today.
 */
export interface CreateExecutionInput {
  item_id: string;
  idempotency_key: string;
  selector_kind: 'exact_runner' | 'fleet';
  selector_id: string;
  agent_profile_id: string;
  requested_harness_kind: string;
  requested_model_provider?: string | null;
  requested_model_id?: string | null;
  agent_profile_snapshot: unknown;
  repository_snapshot: unknown;
  permission_policy: unknown;
  budgets: unknown;
  environment: unknown;
  metadata: unknown;
  timeout_seconds: number;
  status_map_policy_id?: string | null;
}

/** `POST /executions` success body (executions.rs lines 409-415). `state` is
 *  always the literal `"queued"` on both a fresh create and a replay — this
 *  endpoint's one and only honest lifecycle claim, unlike cancel's (see
 *  header note). `replayed` distinguishes a fresh create from an
 *  idempotency-key hit that reused the original request. */
export interface CreateExecutionResult {
  protocol_version: number;
  request_id: string;
  state: 'queued';
  replayed: boolean;
}

/** `POST /executions/{id}/cancel` success body (executions.rs lines
 *  515-517). **`state` is a fixed acknowledgement string, not a member of
 *  `ExecutionState`** — see this file's header note. Treat it as opaque
 *  display text, never feed it into `ExecutionState`-typed logic. */
export interface CancelExecutionResult {
  protocol_version: number;
  request_id: string;
  state: string;
}

/** `POST /executions/{id}/requeue` request body — `RecoveryConfirmation`
 *  (executions.rs lines 520-524). Requires the operator's out-of-band
 *  `recovery_key` and a human `reason`; only valid from `needs_operator`. */
export interface RequeueExecutionInput {
  recovery_key: string;
  reason: string;
}

/** `POST /executions/{id}/requeue` success body (executions.rs lines
 *  552-554). `state` is always the literal `"queued"` here — a genuinely
 *  real lifecycle claim, since a successful requeue really does transition
 *  the request back to `queued` (III.1.1: `needs_operator -> queued`,
 *  operator-only). */
export interface RequeueExecutionResult {
  protocol_version: number;
  request_id: string;
  state: 'queued';
  recovered_from: 'needs_operator';
  replayed: boolean;
}

export const executionsApi = {
  /** Preserves response headers via `requestWithHeaders` even though the
   *  handler sets none beyond the defaults today — so a future
   *  fencing/ETag-style header lands here automatically instead of being
   *  silently dropped by a `data`-only wrapper (proven in `api.test.ts`).
   *
   *  `itemId`, when given, scopes the request to that item alone
   *  (`list_executions`'s `item_id` query parameter) instead of every
   *  execution request the install has ever recorded — the one-item
   *  Execution tab (`ExecutionTimeline.tsx`) uses this so opening it never
   *  pulls the whole table over the wire. `limit` forwards to
   *  `list_executions`'s own `?limit=`, server-capped at `MAX_LIMIT`
   *  regardless of what is asked; meaningless (and ignored server-side)
   *  whenever `itemIds` is also given.
   *
   *  `itemIds`, when given, asks for the latest execution for each of
   *  those items in one request (`?item_ids=`, comma-joined) — what
   *  `store.ts#watchItem` batches Board/Sprint badge reads into, instead of
   *  a single shared "most recent N install-wide" fetch that degrades with
   *  install size. Wins over `itemId`/`limit` server-side when present. */
  list: (itemId?: string, limit?: number, itemIds?: readonly string[]) => {
    const params = new URLSearchParams();
    if (itemId) params.set('item_id', itemId);
    if (limit !== undefined) params.set('limit', String(limit));
    if (itemIds && itemIds.length > 0) params.set('item_ids', itemIds.join(','));
    const qs = params.toString();
    return requestWithHeaders<ExecutionListResult>(qs ? `/executions?${qs}` : '/executions');
  },
  get: (requestId: string) =>
    requestWithHeaders<ExecutionSummary>(`/executions/${encodeURIComponent(requestId)}`),
  create: (input: CreateExecutionInput) =>
    request<CreateExecutionResult>('/executions', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  cancel: (requestId: string) =>
    request<CancelExecutionResult>(`/executions/${encodeURIComponent(requestId)}/cancel`, {
      method: 'POST',
    }),
  requeue: (requestId: string, input: RequeueExecutionInput) =>
    request<RequeueExecutionResult>(`/executions/${encodeURIComponent(requestId)}/requeue`, {
      method: 'POST',
      body: JSON.stringify(input),
    }),
};

// ── Runner fleets (`crates/tack-api/src/handlers/runner_admin.rs`) ────────
//
// **Vocabulary note (III.0):** this `Fleet` (the `agent_fleets` table) is a
// completely different concept from `shared/orch/capabilities.ts`'s /
// `features/fleet/api.ts`'s `FleetEntry` (`GET /api/fleet`, Part II's
// per-project docket control-plane roster). Both happen to be named
// "fleet"; nothing here imports from or is compatible with that other
// module. A caller that means "docket's fleet view" wants
// `shared/orch`/`features/fleet`, not this file.

export interface FleetSummary {
  fleet_id: string;
  name: string;
  concurrency_limit: number | null;
  default_policy: unknown;
}

export interface FleetListResult {
  protocol_version: number;
  data: FleetSummary[];
}

export interface CreateFleetInput {
  name: string;
  concurrency_limit?: number | null;
  default_policy?: unknown;
}

export interface CreateFleetResult {
  protocol_version: number;
  fleet_id: string;
  name: string;
}

export const fleetsApi = {
  list: () => requestWithHeaders<FleetListResult>('/runner-fleets'),
  create: (input: CreateFleetInput) =>
    request<CreateFleetResult>('/runner-fleets', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
};

// ── Agent profiles ─────────────────────────────────────────────────────────

export interface AgentProfileSummary {
  agent_profile_id: string;
  name: string;
  instructions: string;
  tool_policy: unknown;
  limits: unknown;
}

export interface AgentProfileListResult {
  protocol_version: number;
  data: AgentProfileSummary[];
}

export interface CreateAgentProfileInput {
  name: string;
  instructions: string;
  tool_policy?: unknown;
  limits?: unknown;
}

export interface CreateAgentProfileResult {
  protocol_version: number;
  agent_profile_id: string;
  name: string;
}

export const agentProfilesApi = {
  list: () => requestWithHeaders<AgentProfileListResult>('/agent-profiles'),
  create: (input: CreateAgentProfileInput) =>
    request<CreateAgentProfileResult>('/agent-profiles', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
};

// ── Model profiles ─────────────────────────────────────────────────────────

export interface ModelProfileSummary {
  model_profile_id: string;
  name: string;
  model_provider: string;
  model_id: string;
  config_reference: string | null;
  enabled: boolean;
}

export interface ModelProfileListResult {
  protocol_version: number;
  data: ModelProfileSummary[];
}

export interface CreateModelProfileInput {
  name: string;
  model_provider: string;
  model_id: string;
  config_reference?: string | null;
}

export interface CreateModelProfileResult {
  protocol_version: number;
  model_profile_id: string;
  name: string;
  model_provider: string;
  model_id: string;
}

export const modelProfilesApi = {
  list: () => requestWithHeaders<ModelProfileListResult>('/model-profiles'),
  create: (input: CreateModelProfileInput) =>
    request<CreateModelProfileResult>('/model-profiles', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
};

// ── Runners ─────────────────────────────────────────────────────────────────

/** `POST /runners/enrollment` request body — `CreatePendingRunner`
 *  (runner_admin.rs lines 100-113). */
export interface EnrollRunnerInput {
  name: string;
  labels?: unknown;
  total_capacity: number;
  available_capacity: number;
  capability_snapshot?: unknown;
  protocol_version?: number;
  enrollment_lifetime_seconds?: number;
}

/**
 * `POST /runners/enrollment` success body (runner_admin.rs lines 306-312).
 * **`enrollment_token` is a raw, one-time secret** — the handler's own doc
 * comment: "deliberately emitted once here and is never readable from
 * metadata, list, revocation, or runner responses." Callers of this API
 * must display/copy it immediately and MUST NOT persist it into
 * `store.ts`'s cache, `sessionStorage`, `localStorage`, or any logger — the
 * same discipline `TACK_RUNNER_ENROLLMENT_TOKEN` gets on the runner side
 * (root `CLAUDE.md`: "exchanged for a durable credential and never
 * persisted").
 */
export interface EnrollRunnerResult {
  protocol_version: number;
  runner_id: string;
  token_id: string;
  enrollment_token: string;
  expires_at: string;
}

export interface RevokeRunnerResult {
  protocol_version: number;
  runner_id: string;
  state: 'revoked';
}

export interface RevokeEnrollmentTokenResult {
  protocol_version: number;
  runner_id: string;
  token_id: string;
  state: 'revoked';
}

/**
 * The row shape returned by `GET /runners[?fleet_id=]`
 * (`crates/tack-api/src/handlers/runner_admin.rs::list_runners`). `labels`/
 * `capability_snapshot` are the handler's own best-effort parse of the
 * stored JSON columns (`null` only if the stored value is somehow not
 * valid JSON — `labels_raw`/`capability_snapshot_raw` always carry the raw
 * string, so no information is lost even then).
 */
export interface RunnerSummary {
  runner_id: string;
  name: string;
  state: 'pending_enrollment' | 'active' | 'revoked' | (string & {});
  labels: Record<string, string> | null;
  labels_raw: string;
  total_capacity: number;
  available_capacity: number;
  /** Parsed `EmbeddedCapabilitySnapshot` — see `RunWithAgentModal.tsx`'s
   *  `runnerSummaryToCapabilities` for how this is adapted into the
   *  `RunnerCapabilities` shape `gateHarnessModelSelection` expects
   *  (that type nests `protocol_version`/`runner_version` *inside* the
   *  capability report; this handler reports them as sibling columns —
   *  see `RunnerListingRow`'s doc comment in `tack-db` for why). */
  capability_snapshot: Record<string, unknown> | null;
  capability_snapshot_raw: string;
  protocol_version: number;
  runner_version: string | null;
  last_heartbeat_at: string | null;
  revoked_at: string | null;
  fleet_ids: string[];
  created_at: string;
  updated_at: string;
}

export interface RunnerListResult {
  protocol_version: number;
  data: RunnerSummary[];
}

export const runnersApi = {
  list: (fleetId?: string) =>
    requestWithHeaders<RunnerListResult>(
      fleetId ? `/runners?fleet_id=${encodeURIComponent(fleetId)}` : '/runners',
    ),
  enroll: (input: EnrollRunnerInput) =>
    request<EnrollRunnerResult>('/runners/enrollment', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  revokeRunner: (runnerId: string) =>
    request<RevokeRunnerResult>(`/runners/${encodeURIComponent(runnerId)}/revoke`, {
      method: 'POST',
    }),
  revokeEnrollmentToken: (runnerId: string, tokenId: string) =>
    request<RevokeEnrollmentTokenResult>(
      `/runners/${encodeURIComponent(runnerId)}/enrollment-tokens/${encodeURIComponent(tokenId)}/revoke`,
      { method: 'POST' },
    ),
};
