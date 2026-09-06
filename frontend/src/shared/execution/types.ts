// Domain vocabulary for the runner-fleet execution surface.
//
// **Why this is hand-written and not imported from `../api/schema.gen`:**
// every operator execution/fleet/runner/profile route
// (`crates/tack-api/src/handlers/executions.rs`,
// `crates/tack-api/src/handlers/runner_admin.rs`, wired in
// `crates/tack-api/src/router.rs`) now publishes a real, generated OpenAPI
// schema for its request/response bodies — but this file's shapes were
// written before that existed and were never migrated to import the
// generated types: matching each field to its generated equivalent across
// every consumer in this domain is a larger refactor, not done here. Every
// shape below is instead copied field-for-field from the real Rust source
// (cited per type) the same way `shared/dispatch/api.ts` and
// `shared/orch/capabilities.ts` already do for their own domains. This file
// is the ONE place that mirrors those shapes; nothing downstream may
// redeclare a competing copy.
//
// A second, independent source grounds the richer domain concepts
// (`ExecutionState`, `RunnerSelector`, capability snapshots, usage
// provenance) that the current thin operator GET responses don't yet expose
// at all: `crates/tack-orch/src/execution/{types,capabilities}.rs` (the
// runner-v1 protocol's own Rust types) and the frozen fixtures under
// `docs/contracts/runner-v1/` (the language-neutral authority for that
// protocol). Those fixtures describe the runner<->API protocol, not the
// browser-facing operator API — but they are the only real, versioned
// description of this vocabulary that exists anywhere in the repo, and the
// operator surface is defined to carry the same values (e.g.
// `execution_requests.state` and `execution_attempts.state` are the same
// column values the runner protocol negotiates). Modeling them here means
// a richer UI has a stable type to code against once a read endpoint for
// it lands, instead of a second reconciliation later.

// ─── Lifecycle (docs/contracts/runner-v1/lifecycle-transitions.json;
//     crates/tack-orch/src/execution/types.rs `ExecutionState`) ─────────────

/**
 * The frozen v1 execution-request/attempt lifecycle. Byte-identical to
 * `docs/contracts/runner-v1/lifecycle-transitions.json`'s `states` array and
 * `crates/tack-orch/src/execution/types.rs`'s `ExecutionState` enum
 * (`#[serde(rename_all = "snake_case")]`, so the Rust `PascalCase` variants
 * are these exact `snake_case` strings on the wire). This is a real closed
 * union (unlike `shared/agentActivity/api.ts`'s `remote_status`, which stays
 * a plain `string` because docket's `TaskStatus` carries an
 * `Unknown(String)` escape hatch) — III.1's frozen contract has no such
 * escape hatch for this vocabulary, so an unrecognised value here is a
 * genuine contract break, not an expected forward-compatibility case.
 */
export type ExecutionState =
  | 'queued'
  | 'leased'
  | 'preparing'
  | 'running'
  | 'waiting_decision'
  | 'succeeded'
  | 'failed'
  | 'cancelled'
  | 'lost'
  | 'needs_operator';

/** `docs/contracts/runner-v1/lifecycle-transitions.json`'s `terminal_states`. */
export const TERMINAL_EXECUTION_STATES: ReadonlySet<ExecutionState> = new Set([
  'succeeded',
  'failed',
  'cancelled',
]);

export function isTerminalExecutionState(state: ExecutionState): boolean {
  return TERMINAL_EXECUTION_STATES.has(state);
}

/**
 * A tagged runner/fleet placement selector. Mirrors
 * `crates/tack-orch/src/execution/types.rs`'s `RunnerSelector`
 * (`#[serde(tag = "kind", rename_all = "snake_case")]`) exactly, including
 * the `'any'` variant that the current operator `POST /executions` handler
 * (`crates/tack-api/src/handlers/executions.rs`, `CreateExecution`) does not
 * yet accept — it validates `selector_kind` as only `exact_runner | fleet`
 * (see `api.ts`'s `CreateExecutionInput`). Kept here because it's part of
 * the frozen domain vocabulary the runner protocol already round-trips
 * (`claim.response.json`'s `request.selector`), even though the operator
 * create endpoint doesn't expose it yet — a consumer must not construct an
 * `'any'` selector against today's create endpoint.
 */
export type RunnerSelector =
  | { kind: 'exact_runner'; runner_id: string }
  | { kind: 'fleet'; fleet_id: string }
  | { kind: 'any' };

// ─── Runner capability snapshot (III.1.4; docs/contracts/runner-v1/
//     capabilities.json; crates/tack-orch/src/execution/capabilities.rs) ───

/** `crates/tack-orch/src/execution/capabilities.rs`'s `CapabilitySupport`. */
export type CapabilitySupport = 'supported' | 'unsupported' | 'advisory';

/**
 * A capability value coupled to the runner-supplied reason. `reason` is
 * `null` when the runner didn't supply one — preserved as `null`, never
 * coerced to an empty string, matching the Rust type's own doc comment
 * ("`null` is meaningful fixture data: preserve it instead of silently
 * omitting the key during a round trip").
 */
export interface CapabilityValue {
  support: CapabilitySupport;
  reason: string | null;
}

/** `crates/tack-orch/src/execution/capabilities.rs`'s `FeatureCapabilities`. */
export interface FeatureCapabilities {
  cancel: CapabilityValue;
  resume: CapabilityValue;
  decisions: CapabilityValue;
  artifacts: CapabilityValue;
  usage: CapabilityValue;
}

export interface Concurrency {
  total: number;
  available: number;
}

/**
 * Models observed for a harness/provider pair. Model IDs are opaque — never
 * parse, split, or infer meaning from their punctuation (mirrors the Rust
 * `ModelId` opaque wrapper's own doc comment).
 */
export interface ModelCombination {
  model_provider: string;
  model_ids: string[];
  discovery: string;
}

export interface HarnessCapability {
  harness_kind: string;
  installed_version: string;
  probe_error: string | null;
  probed_at: string;
  model_combinations: ModelCombination[];
  /**
   * Whether this harness forwards an operator-specified model id verbatim
   * rather than validating it against `model_combinations` — mirrors the
   * Rust `HarnessCapability::model_passthrough` field
   * (`crates/tack-orch/src/execution/capabilities.rs`), which is
   * `#[serde(default, skip_serializing_if = "Option::is_none")]`: absent
   * from the JSON entirely on an older runner or the shared fake probe, so
   * this key is optional here too, never a fabricated `null`. Only
   * `support === 'supported'` unlocks a free-text model id — `'advisory'`
   * and absent both mean "not attested" and behave identically (the
   * scheduler's own `select.rs` treats them the same).
   */
  model_passthrough?: CapabilityValue;
}

export interface CapabilityLimits {
  event_payload_bytes_max: number;
  artifact_content_bytes_max: number;
}

/**
 * A complete point-in-time runner capability report — byte-shape-compatible
 * with `docs/contracts/runner-v1/capabilities.json` (verified in
 * `crates/tack-orch/src/execution/types.rs`'s
 * `core_domain_snapshots_match_their_exact_fixture_shapes` test).
 * `protocol_version` is optional here for the same reason the Rust type
 * documents: an embedded snapshot (inside an enrollment/refresh envelope)
 * omits it, while a standalone capabilities report carries it.
 *
 * `GET /runners` returns this shape for every registered runner, but not
 * directly: `agent_runners.capability_snapshot` comes back as
 * `RunnerSummary.capability_snapshot` (`api.ts`), an untyped parse of the
 * stored JSON, and `RunWithAgentModal.tsx`'s `runnerSummaryToCapabilities`
 * adapts it into this shape (nesting `protocol_version`/`runner_version`
 * inside the report the way the wire type documents them, rather than as
 * the sibling columns the row carries them as). `capabilities.ts`'s
 * selector functions are written against this type and consume that
 * adapted value.
 */
export interface RunnerCapabilities {
  protocol_version?: number;
  runner_version: string;
  reported_at: string;
  labels: Record<string, string>;
  concurrency: Concurrency;
  harnesses: HarnessCapability[];
  features: FeatureCapabilities;
  limits: CapabilityLimits;
}

// ─── Usage provenance (III.1.3; crates/tack-orch/src/execution/types.rs
//     `MeasurementSource`/`Measurement`/`Usage`) ────────────────────────────

/** `crates/tack-orch/src/execution/types.rs`'s `MeasurementSource`. */
export type MeasurementSource = 'measured' | 'estimated' | 'not_measured';

/**
 * A nullable metric paired with its provenance — never a fabricated zero.
 * `value` is `null` whenever `source === 'not_measured'`; a `0` value with a
 * `'measured'`/`'estimated'` source is a real, distinct fact — unmeasured is
 * nullable, applied to every usage figure.
 */
export interface Measurement<T> {
  value: T | null;
  source: MeasurementSource;
}

export interface Usage {
  tokens_in: Measurement<number>;
  tokens_out: Measurement<number>;
  duration_ms: Measurement<number>;
  cost_usd: Measurement<number>;
}

// ─── Stable protocol errors (III.1.6; docs/contracts/runner-v1/errors/*.json;
//     crates/tack-orch/src/execution/types.rs `StableErrorCode`) ────────────

/**
 * Every stable v1 error code, byte-identical to the fifteen fixtures under
 * `docs/contracts/runner-v1/errors/` and `StableErrorCode`
 * (`crates/tack-orch/src/execution/types.rs`). The operator execution/fleet
 * handlers build every error through this same `StableErrorCode` (see
 * `crates/tack-api/src/handlers/executions.rs`'s and `runner_admin.rs`'s
 * `error()` helper, which calls `ProtocolErrorEnvelope::new`), so a caller
 * of this file's `api.ts` can rely on `ApiError.code` being one of these
 * fifteen values whenever the server set one at all.
 *
 * **Not exposed on `ApiError` today: `retryable` and `details`.**
 * `shared/api/client.ts` extracts only `error.message` and `error.code`
 * from the envelope (`toApiError`) — `retryable` and the per-code `details`
 * object (e.g. `invalid_transition`'s `{from, to}`, `stale_lease`'s
 * `{attempt_id, current_fencing_token}`) are silently dropped before this
 * file ever sees the error. `code` alone is sufficient for the
 * optimistic-cancel conflict/error distinction this file needs (see
 * `store.ts`); richer per-code detail rendering would need a
 * `shared/api/client.ts` enhancement this file can't make without editing
 * that module.
 */
export type StableErrorCode =
  | 'invalid_request'
  | 'unauthorized'
  | 'forbidden'
  | 'not_found'
  | 'conflict'
  | 'idempotency_conflict'
  | 'invalid_transition'
  | 'stale_lease'
  | 'runner_revoked'
  | 'decision_expired'
  | 'artifact_checksum_mismatch'
  | 'payload_too_large'
  | 'rate_limited'
  | 'unsupported_protocol'
  | 'internal_error';
