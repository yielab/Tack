// Wire-format boundary for dispatching work to an agent fleet. Every
// assumption about request/response shapes lives in this one file, plus
// `./format.ts` for interpreting them — consuming components
// (`ItemDetailDrawer.tsx`, `Board.tsx`'s `ItemCard` menu,
// `features/sprints/DispatchSprintModal.tsx`) only ever import types
// and functions from here, never construct a request body or read a raw
// field name themselves, so when the real endpoint lands (or changes),
// reconciling against this file is the only place that needs to happen.
//
// Two endpoints:
//
//  1. `POST /items/{id}/dispatch` — confirmed field-for-field against
//     `docs/openapi.json`'s `DispatchItemResponse`/`DispatchedTaskResponse`
//     schemas (themselves generated from the real Rust handler in
//     `crates/tack-api/src/handlers/orch.rs`), including the typed
//     `policy_id` field. This is NOT a guess — every field name
//     and nullability below is copied from the generated schema.
//
//  2. `POST /sprints/{id}/dispatch` and `GET /sprints/{id}/dispatch/dry-run`
//     — confirmed field-for-field against `docs/openapi.json`'s
//     `SprintDispatchItemResponse`/`SprintDispatchSummary`/
//     `DryRunSprintDispatchResponse`/`SprintDispatchResponse` schemas.
//     Three things worth remembering because they'd otherwise fail
//     silently: (1) `max_in_flight` is a **query parameter** on both
//     routes, not a JSON body field — a body was never read by the
//     handler, so sending it there silently did nothing; (2) every sprint
//     item is **always** present with an `order` — nothing is filtered out
//     of the plan the way a nullable "position" implied; (3) eligibility is
//     a **closed vocabulary** (`decision`), not free text, and dry-run vs. a
//     real run share the same shape with different possible `decision`
//     values (`would_dispatch` only appears in a dry run;
//     `dispatched`/`blocked`/`waiting_approval`/`error` only appear in a
//     real run).

import { request } from '../api/client';
export { isOrchDisabled } from '../agentActivity/api';

// ── Single-item dispatch (`POST /items/{id}/dispatch`) ─────────────────────

/**
 * A dispatched (or already-in-flight) `orch_tasks` row, projected for the
 * dispatch response. Matches `DispatchedTaskResponse` in
 * `crates/tack-api/src/handlers/orch.rs` / `docs/openapi.json` exactly.
 */
export interface DispatchedTaskResponse {
  remote_task_id: string;
  /** `TaskStatus`, verbatim, degrade-on-unknown (same discipline as
   *  `shared/agentActivity/api.ts`'s `ItemAgentAttempt.remote_status`). */
  remote_status: string;
  attempt: number;
  dispatched_at: string;
  /** The prompt-injection boundary flag — echoed
   *  back so a caller could show "dispatched untrusted" if it
   *  wants to, though nothing renders it yet (same "no UI story yet, don't
   *  invent one" call made for the identical field on the agent-activity
   *  endpoint). */
  trusted: boolean;
}

/**
 * `POST /api/items/{id}/dispatch` response. **Every one of the six
 * `outcome` values is a 200** — including `"blocked"`: docket gave a
 * definitive, well-formed refusal, which is a successful round-trip from
 * Tack's HTTP perspective, not a Tack-side error (`docs/openapi.json`'s own
 * description, copied verbatim into this comment so it doesn't drift).
 * **Callers MUST branch on `outcome`, never on HTTP status.**
 *
 * The six values, confirmed against the schema: `"dispatched"` (task 35.6's
 * `on_running` may also have applied — see `status_applied`),
 * `"waiting_approval"` (a real, not-yet-running task exists — see
 * `approval_token`), `"already_in_flight"` (idempotency guard, C1's
 * handoff), `"no_dispatch_policy"` (the linked project's `status_map` names
 * no `dispatch_from`), `"not_eligible"` (item's current status isn't in
 * `dispatch_from` right now — see `current_status`/`dispatch_from`), and
 * `"blocked"` (docket's `pre_input` guardrail fired — see `policy_id`/
 * `message`). Kept as a plain `string` here, not a TS union, in case a
 * future backend version adds a seventh value this build doesn't know about
 * yet — `format.ts#describeDispatchOutcome` is where an unrecognised value
 * degrades to a visible-but-generic rendering rather than being silently
 * swallowed or throwing.
 */
export interface DispatchItemResponse {
  outcome: string;
  /** Present only when `outcome` is `"dispatched"` or `"waiting_approval"`. */
  task: DispatchedTaskResponse | null;
  /** Present only when `outcome === "waiting_approval"`. */
  approval_token: string | null;
  /** Present only when `outcome === "not_eligible"`. */
  current_status: string | null;
  /** Present only when `outcome === "not_eligible"`. */
  dispatch_from: string[] | null;
  /** Present only when `outcome === "blocked"` — docket's own message,
   *  verbatim, for display (never paraphrased — the operator needs docket's
   *  actual words to act on it). */
  message: string | null;
  /** Present only when `outcome === "blocked"` — the guardrail policy id
   *  that fired (the typed `OrchError::PolicyBlocked`). Shows
   *  *which* policy blocked it. */
  policy_id: string | null;
  /** The Tack status `status_map` named for this trigger and actually
   *  applied — absent when `status_map` named no target, the item was
   *  already there, or the workflow engine rejected it (see
   *  `status_map_rejected`). */
  status_applied: string | null;
  /** Set when the workflow engine refused the `status_map`-driven
   *  transition. The item was left exactly as it was;
   *  this is the engine's own reason (e.g. an invalid transition or a WIP
   *  limit) — surfaced prominently, since it means docket is
   *  running the task but Tack couldn't reflect it on the board. */
  status_map_rejected: string | null;
}

// ── Sprint dispatch (`POST /sprints/{id}/dispatch`, `GET
//    /sprints/{id}/dispatch/dry-run`) ───────────────────────────────────────

/**
 * One item's place in a sprint-dispatch plan or report — the SAME shape for
 * both the dry-run preview and a real run's per-item result, so the two
 * responses read the same way side by side, matching
 * `SprintDispatchItemResponse` in
 * `crates/tack-api/src/handlers/orch.rs` / `docs/openapi.json` exactly.
 * **Every item in the sprint is always present** — nothing is filtered out
 * of the plan; `decision` is what tells you whether it will actually run.
 */
export interface SprintDispatchItemResponse {
  item_id: string;
  title: string;
  status: string;
  /** 0-based position in the topological dispatch order — assigned to every
   *  item, including ones that won't run this pass (their sequencing still
   *  matters if a blocker clears later). */
  order: number;
  /**
   * The closed decision vocabulary (a plain `string` here, not a TS union,
   * for the same "degrade rather than break on an unrecognised value"
   * reason `DispatchItemResponse.outcome` is kept a string) — see
   * `format.ts#describeDispatchOutcome`, which now maps every one of these
   * too):
   *  - `"waiting_on_dependencies"` — a direct dependency hasn't reached a
   *    Done-category status yet (live check at plan time, not "dispatched"
   *    or "succeeded" — see `blocked_by`).
   *  - `"no_dispatch_policy"` / `"not_eligible"` / `"already_in_flight"` —
   *    same meaning as the single-item `DispatchItemResponse.outcome`
   *    values of the same name.
   *  - `"would_dispatch"` — **dry-run only**: every gate passed; a real run
   *    would have called docket and resolved to `blocked`/`waiting_approval`/
   *    `dispatched`/`error` instead. Never render this as "dispatched" — it
   *    is a prediction, not an action that happened.
   *  - `"blocked"` / `"waiting_approval"` / `"dispatched"` — **real run
   *    only**: same meaning as the identically-named single-item outcomes.
   *  - `"error"` — **real run only**: this item's own dispatch failed or its
   *    worker task panicked (one item's failure never aborts the rest of
   *    the sprint) — see `error`. Has no single-item-dispatch equivalent;
   *    a genuinely new case.
   */
  decision: string;
  /** Present only when `decision === "waiting_on_dependencies"` — every
   *  direct dependency (item id, not title — the API doesn't resolve names)
   *  that hasn't reached a Done-category status yet. */
  blocked_by: string[] | null;
  policy_id: string | null;
  message: string | null;
  status_applied: string | null;
  status_map_rejected: string | null;
  approval_token: string | null;
  current_status: string | null;
  dispatch_from: string[] | null;
  /** Present only when `decision === "error"` (real run only). */
  error: string | null;
  task: DispatchedTaskResponse | null;
}

/** Pre-computed counts over a real dispatch run's per-item `decision`
 *  values — the UI's headline "8 dispatched, 2 waiting on dependencies"
 *  line without re-deriving it client-side from the row list. Also returned
 *  by the dry-run endpoint (predicting what a real run
 *  would show). Every key is always present, even at 0 — never re-derive
 *  these by summing `items` yourself; read them directly. */
export interface SprintDispatchSummary {
  total: number;
  dispatched: number;
  waiting_approval: number;
  blocked: number;
  already_in_flight: number;
  waiting_on_dependencies: number;
  not_eligible: number;
  no_dispatch_policy: number;
  would_dispatch: number;
  errored: number;
}

/** `GET /api/sprints/{id}/dispatch/dry-run` response. Zero side effects. */
export interface DryRunSprintDispatchResponse {
  sprint_id: string;
  /** The resolved/clamped in-flight cap a real run with this input would
   *  use (`sprint_dispatch::{DEFAULT_MAX_IN_FLIGHT, MAX_MAX_IN_FLIGHT}` —
   *  clamped to `[1, 20]`, default 5) — echo this back into the cap control
   *  rather than inventing a client-side default. */
  max_in_flight: number;
  summary: SprintDispatchSummary;
  items: SprintDispatchItemResponse[];
}

/** `POST /api/sprints/{id}/dispatch` response — same shape as the dry-run,
 *  since both share one planning function; `items` here reflects what
 *  actually happened instead of a prediction. */
export interface SprintDispatchResponse {
  sprint_id: string;
  max_in_flight: number;
  summary: SprintDispatchSummary;
  items: SprintDispatchItemResponse[];
}

/** Appends `?max_in_flight=N` when given — the real contract is a query
 *  parameter on both routes (`axum::extract::Query<SprintDispatchQuery>` in
 *  `handlers/orch.rs`), never a JSON body. Omit to let the server apply its
 *  own default; any value is clamped server-side to `[1, 20]`. */
function withMaxInFlight(path: string, maxInFlight?: number): string {
  return maxInFlight != null ? `${path}?max_in_flight=${encodeURIComponent(maxInFlight)}` : path;
}

export const dispatchApi = {
  dispatchItem: (itemId: string) =>
    request<DispatchItemResponse>(`/items/${itemId}/dispatch`, { method: 'POST' }),
  dryRunSprintDispatch: (sprintId: string, maxInFlight?: number) =>
    request<DryRunSprintDispatchResponse>(withMaxInFlight(`/sprints/${sprintId}/dispatch/dry-run`, maxInFlight)),
  dispatchSprint: (sprintId: string, maxInFlight?: number) =>
    request<SprintDispatchResponse>(withMaxInFlight(`/sprints/${sprintId}/dispatch`, maxInFlight), {
      method: 'POST',
    }),
};
