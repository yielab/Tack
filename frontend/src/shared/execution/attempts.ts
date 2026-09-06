// Wire-format boundary for the per-request attempt/event surface:
// `GET /executions/{id}/attempts` and `GET
// /executions/{id}/attempts/{attempt_number}/events`
// (`crates/tack-api/src/handlers/executions.rs`,
// `list_execution_attempts`/`list_execution_attempt_events`). This file is
// that wiring's wire-format layer; `store.ts#loadAttempts`/`attemptsFor` is
// the reactive layer built on top of it.
//
// Every shape below is copied field-for-field from the real handler
// (`AttemptSummary`/`AttemptListResponse`/`EventSummary`/`EventListResponse`
// in `executions.rs`), the same hand-typed-from-Rust convention `api.ts`'s
// header comment documents for this whole domain (this domain's OpenAPI
// schemas type `model_provenance`/`usage_economics` via
// `#[schema(value_type = ...)]` mirrors defined in `executions.rs` itself,
// not surfaced as reusable named schemas in `schema.gen.ts`).
//
// `model_provenance` (nullable — `null` until the attempt reports
// `actual_execution`) and `usage_economics` (always present) on
// `AttemptSummary` wire `tack_orch::usage_provenance`'s pure resolution into
// this handler. Both are typed here from
// `tack_orch::usage_provenance::{ModelProvenance, UsageEconomics,
// RunnerTimeCost}` (via `executions.rs`'s own
// `ModelProvenanceSchema`/`UsageEconomicsSchema`/`RunnerTimeCostSchema`
// OpenAPI mirrors, which this file matches byte-for-byte).

import { requestWithHeaders } from '../api/client';
import type { Measurement } from './types';

// ─── Model provenance (`usage_provenance.rs::ModelProvenance`) ─────────────

/**
 * Whether an attempt ran on what was requested. A tagged union, never
 * collapsed into a bare "matched" boolean, so both sides of a mismatch are
 * always visible — unsupported is typed, unknown is explicit, applied to
 * provenance, not just numbers — see
 * `attemptFormat.ts#describeModelProvenance` for the display layer.
 */
export type ModelProvenance =
  | { kind: 'matched'; provider: string; model_id: string }
  | { kind: 'auto_select_observed'; actual_provider: string; actual_model_id: string }
  | {
      kind: 'mismatched';
      requested_provider: string;
      requested_model_id: string;
      actual_provider: string;
      actual_model_id: string;
    };

// ─── Usage economics (`usage_provenance.rs::UsageEconomics`) ───────────────

/**
 * The runner-witnessed wall-clock dimension — a plain fact, never itself a
 * `Measurement` (there is no "estimated" wall clock; see
 * `usage_provenance.rs`'s own doc comment). `wall_clock_ms` is `null` only
 * until both `started_at`/`ended_at` are known; `cost_usd_estimated` is a
 * `Measurement` because turning that duration into a dollar figure needs an
 * infra rate no deployment configures today — every real response has this
 * at `{value: null, source: 'not_measured'}`.
 */
export interface RunnerTimeCost {
  wall_clock_ms: number | null;
  cost_usd_estimated: Measurement<number>;
}

/**
 * Two independently-provenanced dollar dimensions, deliberately never
 * summed into one figure: the harness/vendor's own self-reported token
 * cost, and a derived runner-infra-time cost. Always present on an
 * `AttemptSummary` (unlike `model_provenance`), because both sub-fields
 * degrade to an honest `not_measured` rather than needing to be absent
 * themselves.
 */
export interface UsageEconomics {
  model_token_cost_usd_estimated: Measurement<number>;
  runner_time_cost: RunnerTimeCost;
}

// ─── Attempts (`GET /executions/{id}/attempts`) ────────────────────────────

/**
 * One attempt, exactly as `executions.rs`'s `AttemptSummary` serializes it.
 * `actual_execution`/`terminal_reason`/`usage` stay `unknown` (untyped JSON)
 * here for the same reason the Rust struct types them as a bare `Value`: the
 * real types (`tack_orch::execution::{ActualExecution, Usage}`) live in a
 * crate this frontend has no generated bridge to yet — this file only needs
 * enough shape to drive `model_provenance`/`usage_economics` display, which
 * are already fully typed above.
 */
export interface AttemptSummary {
  attempt_id: string;
  request_id: string;
  attempt_number: number;
  runner_id: string;
  fencing_token: number;
  state: string;
  lease_issued_at: string;
  lease_expires_at: string;
  last_heartbeat_at: string | null;
  event_checkpoint: string | null;
  completion_id: string | null;
  workspace_id: string | null;
  base_revision: string | null;
  actual_execution: unknown | null;
  terminal_reason: unknown | null;
  usage: unknown | null;
  started_at: string | null;
  ended_at: string | null;
  created_at: string;
  updated_at: string;
  /** `null` while the attempt has not yet reported `actual_execution`. */
  model_provenance: ModelProvenance | null;
  usage_economics: UsageEconomics;
}

export interface AttemptListResult {
  protocol_version: number;
  data: AttemptSummary[];
}

// ─── Events (`GET /executions/{id}/attempts/{n}/events`) ──────────────────

/**
 * One normalized timeline event, exactly as `executions.rs`'s `EventSummary`
 * serializes it. `kind`/`source` stay plain strings (matching
 * `docs/contracts/runner-v1/event-batch.request.json`'s own free-form
 * vocabulary — `"message"`/`"progress"` in the frozen example, never a fixed
 * enum); `payload` is the harness/runner's own arbitrary JSON, rendered
 * defensively by `attemptFormat.ts#describeEventPayload`.
 */
export interface EventSummary {
  event_id: string;
  sequence: number;
  source: string;
  kind: string;
  payload: unknown;
  occurred_at: string;
  created_at: string;
}

export interface EventListResult {
  protocol_version: number;
  data: EventSummary[];
}

export const attemptsApi = {
  list: (requestId: string) =>
    requestWithHeaders<AttemptListResult>(`/executions/${encodeURIComponent(requestId)}/attempts`),
  events: (requestId: string, attemptNumber: number) =>
    requestWithHeaders<EventListResult>(
      `/executions/${encodeURIComponent(requestId)}/attempts/${encodeURIComponent(String(attemptNumber))}/events`,
    ),
};
