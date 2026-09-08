//! Operator execution handlers, mounted into the global router.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use tack_db::{
    Repository,
    repo::execution::{EnqueueResult, ExecutionClock, NewExecutionRequest, OperatorRequeueResult},
};
use tack_orch::execution::{ExecutionRequestSnapshot, ProtocolErrorEnvelope, StableErrorCode};
use tack_orch::model_policy::wiring::resolve_request_model_policy;
use tack_orch::scheduler::types::ModelSelector;
use tack_orch::usage_provenance::derive_attempt_facts;
use utoipa::ToSchema;
use uuid::Uuid;

/// Documents `tack_orch::execution::ProtocolErrorEnvelope`'s real wire shape
/// (`docs/contracts/runner-v1/errors/*.json`) for every operator execution/
/// fleet/runner/profile route, which returns that envelope — not
/// `crate::openapi::ErrorEnvelope`, a different, incompatible shape
/// (`{status,message,code?}` vs `{code,message,request_id,retryable,
/// details}`). This is a doc-only mirror, not a second runtime authority:
/// `tack-orch` must stay free of an OpenAPI-generation dependency (see that
/// crate's own architecture boundary), so the real type cannot derive
/// `ToSchema` itself. Defined here (not in `crate::openapi`) so this file
/// keeps compiling standalone when its regression tests load it via
/// `#[path = "../src/handlers/executions.rs"]` from a separate test-binary
/// crate root, where a `crate::openapi` import would not resolve. `code` is
/// documented as a free
/// string rather than an enum because `StableErrorCode` lives in
/// `tack-orch` for the same reason; its fifteen frozen values are
/// enumerated in `docs/contracts/runner-v1/README.md`.
///
/// `allow(dead_code)`: this type is a pure OpenAPI schema marker — utoipa's
/// `#[utoipa::path(responses(... body = RunnerV1ErrorEnvelope ...))]`
/// annotations reference it by *type* (calling `ToSchema`'s associated
/// functions) but no code anywhere ever constructs a *value* of it, since
/// every real error response is built from the actual runtime type,
/// `tack_orch::execution::ProtocolErrorEnvelope`. In the real `tack-api`
/// library crate this is invisible to `dead_code` (a `pub` item in a
/// library is assumed reachable by external callers); it only surfaces
/// when this file is compiled standalone into a test *binary* via
/// `#[path]`, which has no external callers at all.
#[derive(Debug, Serialize, ToSchema)]
#[allow(dead_code)]
pub struct RunnerV1ErrorEnvelope {
    pub error: RunnerV1Error,
}

#[derive(Debug, Serialize, ToSchema)]
#[allow(dead_code)]
pub struct RunnerV1Error {
    /// One of the fifteen frozen `docs/contracts/runner-v1/errors/*.json`
    /// codes (e.g. `not_found`, `conflict`, `idempotency_conflict`,
    /// `invalid_transition`, `stale_lease`, `runner_revoked`).
    #[schema(example = "not_found")]
    pub code: String,
    pub message: String,
    pub request_id: String,
    /// Whether a conformant client may safely retry — derived from `code`
    /// alone (`StableErrorCode::retryable`), never set independently.
    pub retryable: bool,
    /// Per-code structured detail — shape documented per operation in
    /// `docs/contracts/runner-v1/README.md` (e.g. `invalid_transition`'s
    /// `{from, to}`, `stale_lease`'s `{attempt_id, current_fencing_token}`).
    pub details: Value,
}

/// Documents `tack_orch::execution::MeasurementSource`'s wire shape —
/// used by `AttemptSummary.usage_economics`. Defined here,
/// not in `crate::openapi`, for the exact reason `RunnerV1ErrorEnvelope`
/// above is: this file must keep compiling standalone when its regression
/// tests load it via `#[path]` from a separate test-binary crate root,
/// where a `crate::openapi` (or any other module's) reference would not
/// resolve. `tack-orch` has no `ToSchema`
/// (see `usage_provenance.rs`'s own module doc), so this is a
/// hand-verified mirror, never constructed or serialized by real code —
/// `#[allow(dead_code)]` for the identical reason `RunnerV1ErrorEnvelope`
/// carries it. `not_measured` is the honest value whenever a figure
/// genuinely is not known — never a fabricated zero.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum MeasurementSourceSchema {
    Measured,
    Estimated,
    NotMeasured,
}

/// Documents `tack_orch::execution::Measurement<f64>` — every dollar figure
/// in this API (`*_usd_estimated`) uses this shape. `value` is `null`
/// whenever `source` is `not_measured`; `tack-orch`'s own
/// `absent_usage_never_serializes_as_zero` test asserts the literal JSON,
/// not just the Rust type: unmeasured is documented as genuinely nullable,
/// never a number defaulting to `0`.
#[derive(Debug, Serialize, ToSchema)]
#[allow(dead_code)]
pub struct UsdMeasurementSchema {
    pub value: Option<f64>,
    pub source: MeasurementSourceSchema,
}

/// Documents `tack_orch::usage_provenance::RunnerTimeCost`.
/// `cost_usd_estimated` is `{"value": null, "source": "not_measured"}` in
/// every real response today — no runner infra cost-rate is stored anywhere
/// in this schema (see `CLAUDE.md`'s `TACK_EXECUTION_*` config table);
/// `runner_rate_usd_per_hour` must be supplied by the caller for a real
/// dollar figure to appear. `wall_clock_ms` is a plain derivable fact, not
/// itself a `Measurement` — `null` only until both the attempt's
/// `started_at`/`ended_at` are known.
#[derive(Debug, Serialize, ToSchema)]
#[allow(dead_code)]
pub struct RunnerTimeCostSchema {
    pub wall_clock_ms: Option<u64>,
    pub cost_usd_estimated: UsdMeasurementSchema,
}

/// Documents `tack_orch::usage_provenance::UsageEconomics` —
/// `AttemptSummary.usage_economics`'s real shape. Always present (never
/// `null`), unlike `ModelProvenanceSchema` below: two
/// independently-provenanced dollar dimensions, deliberately never summed
/// into one figure.
#[derive(Debug, Serialize, ToSchema)]
#[allow(dead_code)]
pub struct UsageEconomicsSchema {
    pub model_token_cost_usd_estimated: UsdMeasurementSchema,
    pub runner_time_cost: RunnerTimeCostSchema,
}

/// Documents `tack_orch::usage_provenance::ModelProvenance` —
/// `AttemptSummary.model_provenance`'s real shape (`null` while the attempt
/// has not yet reported `actual_execution`). A tagged union carrying every
/// observed fact, never coalesced into a bare boolean "matched" flag, so a
/// caller (the frontend's rendering) can show both sides of a mismatch.
#[derive(Debug, Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ModelProvenanceSchema {
    /// The attempt ran on exactly the requested provider/model.
    Matched { provider: String, model_id: String },
    /// The request allowed auto-selection (no explicit provider/model was
    /// ever resolved for it) and the attempt observed a concrete choice.
    AutoSelectObserved {
        actual_provider: String,
        actual_model_id: String,
    },
    /// The attempt ran on a provider and/or model different from what was
    /// explicitly requested. Both sides are always carried in full, never
    /// silently reconciled.
    Mismatched {
        requested_provider: String,
        requested_model_id: String,
        actual_provider: String,
        actual_model_id: String,
    },
}

/// State for the operator execution router — constructed from the shared
/// API state's repository, clock and orchestration-enabled resolver when
/// mounted in `router.rs`.
///
/// `orchestration_enabled` is a callback, not a direct call into
/// `handlers::settings::effective_orch_enabled_for`, because this file must
/// keep compiling standalone when its regression tests load it via
/// `#[path]` from a separate test-binary crate root (see
/// `RunnerV1ErrorEnvelope`'s doc comment above), where a
/// `crate::handlers::settings` reference would not resolve. `router.rs`
/// closes over `AppState::config.orch_enable` and wires this to that one
/// real implementation — nothing in this file re-implements the `app_meta`
/// read. Same shape as `clock` above: a trait-object seam for a dependency
/// this file cannot name directly.
#[derive(Clone)]
pub struct OperatorExecutionState {
    pub repo: Repository,
    pub clock: Arc<dyn ExecutionClock>,
    pub orchestration_enabled:
        Arc<dyn Fn(sqlx::SqlitePool) -> BoxFuture<'static, bool> + Send + Sync>,
}

impl OperatorExecutionState {
    pub fn with_clock(
        repo: Repository,
        clock: Arc<dyn ExecutionClock>,
        orchestration_enabled: Arc<
            dyn Fn(sqlx::SqlitePool) -> BoxFuture<'static, bool> + Send + Sync,
        >,
    ) -> Self {
        Self {
            repo,
            clock,
            orchestration_enabled,
        }
    }
}

/// Static request-correlation id placeholder. No per-request correlation
/// id is wired into these error envelopes yet.
const OPERATOR_REQUEST_ID: &str = "req_operator";

/// Builds the stable v1 error envelope via `ProtocolErrorEnvelope::new`,
/// which derives `retryable` from `code` (`StableErrorCode::retryable`) so it
/// can never drift from `docs/contracts/runner-v1/errors/*.json`. `details`
/// must follow the per-code shape documented in
/// `docs/contracts/runner-v1/README.md`.
fn error(
    status: StatusCode,
    code: StableErrorCode,
    message: &str,
    details: Value,
) -> (StatusCode, Json<Value>) {
    let envelope = ProtocolErrorEnvelope::new(code, message, OPERATOR_REQUEST_ID, details);
    (
        status,
        Json(serde_json::to_value(envelope).expect("envelope serializes")),
    )
}

#[derive(Clone)]
struct FixedExecutionClock(DateTime<Utc>);

impl ExecutionClock for FixedExecutionClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

fn principal(headers: &HeaderMap) -> Result<String, (StatusCode, Json<Value>)> {
    headers
        .get("x-tack-principal")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            error(
                StatusCode::UNAUTHORIZED,
                StableErrorCode::Unauthorized,
                "An authenticated operator principal is required",
                json!({}),
            )
        })
}

fn canonical_json(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonical_json).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, canonical_json(value)))
                .collect(),
        ),
        value => value,
    }
}

/// `field` names the request field being canonicalized, so a failure carries
/// `invalid_request`'s contract-shaped `{"field": ...}` detail rather than an
/// empty object.
fn canonical_string(value: Value, field: &str) -> Result<String, (StatusCode, Json<Value>)> {
    serde_json::to_string(&canonical_json(value)).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "Request cannot be canonicalized",
            json!({"field": field}),
        )
    })
}

fn stable_request_id(scope: &str, key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(scope.as_bytes());
    hasher.update([0]);
    hasher.update(key.as_bytes());
    format!("exec_{}", hex::encode(hasher.finalize()))
}

fn fingerprint(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

/// Parses a stored `TEXT` timestamp column (always written via
/// `DateTime::to_rfc3339`, e.g. `execution_attempts.started_at`/`ended_at`)
/// back into a `DateTime<Utc>`. `None` on a missing or malformed value —
/// never a panic, matching this file's existing
/// `DateTime::parse_from_rfc3339` usage above for `request_snapshot`'s
/// `created_at`.
fn parse_rfc3339(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct CreateExecution {
    pub item_id: Uuid,
    pub idempotency_key: String,
    pub selector_kind: String,
    pub selector_id: String,
    pub agent_profile_id: String,
    pub requested_harness_kind: String,
    #[serde(default)]
    pub requested_model_provider: Option<String>,
    #[serde(default)]
    pub requested_model_id: Option<String>,
    /// `tack_orch::execution::AgentProfileSnapshot` (`{name, instructions,
    /// tool_policy, timeout_seconds, budgets}`) — untyped here because that
    /// type lives in `tack-orch`, which does not derive `ToSchema` (see
    /// `RunnerV1ErrorEnvelope`'s doc comment for the same architectural
    /// reason). Validated against the real nested struct server-side at
    /// enqueue time.
    pub agent_profile_snapshot: Value,
    /// `tack_orch::execution::RepositorySnapshot` (`{kind, remote,
    /// base_revision, subdirectory}`).
    pub repository_snapshot: Value,
    /// `tack_orch::execution::PermissionPolicy` (`{tools, network}`).
    pub permission_policy: Value,
    pub budgets: Value,
    pub environment: Value,
    pub metadata: Value,
    pub timeout_seconds: u64,
    #[serde(default)]
    pub status_map_policy_id: Option<String>,
}

/// Response body for `POST /api/executions` — a newly created request or an
/// idempotent replay of an existing one (`replayed` distinguishes the two).
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateExecutionResponse {
    pub protocol_version: u32,
    pub request_id: String,
    /// Always `"queued"` on success — a request enters the runner-v1
    /// lifecycle (`docs/contracts/runner-v1/lifecycle-transitions.json`)
    /// only once created.
    pub state: String,
    pub replayed: bool,
}

/// One row of `GET /api/executions` / the `GET /api/executions/{id}`
/// detail. Deliberately five scalar columns today — attempt/event detail
/// is not carried here; it is available separately via `GET
/// /api/executions/{id}/attempts`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ExecutionSummary {
    pub request_id: String,
    pub item_id: String,
    pub state: String,
    pub cancellation_requested_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExecutionListResponse {
    pub protocol_version: u32,
    pub data: Vec<ExecutionSummary>,
}

/// Query parameters for `GET /api/executions`. Three read shapes, in
/// precedence order: `item_ids` (comma-separated) asks for the single
/// latest execution per item, for a batch of items — the shape a
/// Board/Sprint screen with many cards needs, one request regardless of
/// how many cards are on screen, and wins over `item_id`/`limit` when
/// present; else `item_id` scopes every request for one item; else the
/// unscoped, install-wide list, bounded by `limit`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListExecutionsQuery {
    pub item_id: Option<Uuid>,
    /// Comma-separated item ids — returns exactly one row per id, its own
    /// most recent execution. Takes precedence over `item_id`/`limit` when
    /// present.
    //
    // A plain string field, not `Vec<Uuid>`: axum's `Query` extractor
    // deserializes query parameters via `serde_urlencoded`, which cannot
    // parse a repeated same-named key into a sequence field (confirmed
    // against this exact version — `serde_urlencoded::from_str` on a
    // repeated key errors "invalid type: string ..., expected a
    // sequence"). Parsed by `ListExecutionsQuery::parsed_item_ids`, not at
    // deserialize time, so a malformed id surfaces as this route's own
    // contract-shaped `invalid_request` error instead of axum's generic
    // query-rejection response.
    #[param(
        value_type = Option<String>,
        example = "11111111-1111-1111-1111-111111111111,22222222-2222-2222-2222-222222222222"
    )]
    #[serde(default)]
    pub item_ids: Option<String>,
    pub limit: Option<u32>,
}

impl ListExecutionsQuery {
    /// Applied when `limit` is omitted.
    pub const DEFAULT_LIMIT: u32 = 200;
    /// Hard cap even when a caller asks for more — also the ceiling on how
    /// many ids `item_ids` may carry, so a pathologically long batch can't
    /// force an unbounded `IN (...)`.
    pub const MAX_LIMIT: u32 = 2000;

    fn effective_limit(&self) -> i64 {
        i64::from(
            self.limit
                .unwrap_or(Self::DEFAULT_LIMIT)
                .min(Self::MAX_LIMIT),
        )
    }

    /// Parses `item_ids` into a list of `Uuid`s, `None` when the parameter
    /// was absent or empty. `Err` names the malformed fragment verbatim
    /// (never silently dropped) or, when the list is longer than
    /// `MAX_LIMIT`, a message naming the count — both are surfaced by the
    /// caller as `invalid_request`.
    fn parsed_item_ids(&self) -> Result<Option<Vec<Uuid>>, String> {
        let Some(raw) = self.item_ids.as_deref().map(str::trim) else {
            return Ok(None);
        };
        if raw.is_empty() {
            return Ok(None);
        }
        let ids = raw
            .split(',')
            .map(|part| {
                let part = part.trim();
                Uuid::parse_str(part).map_err(|_| part.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if ids.len() > Self::MAX_LIMIT as usize {
            return Err(format!(
                "item_ids carries {} ids, more than the {} allowed",
                ids.len(),
                Self::MAX_LIMIT
            ));
        }
        Ok(Some(ids))
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExecutionDetailResponse {
    pub protocol_version: u32,
    pub request_id: String,
    pub item_id: String,
    pub state: String,
    pub cancellation_requested_at: Option<String>,
    pub created_at: String,
}

/// One attempt as reported by `GET /api/executions/{id}/attempts` — every
/// column `execution_attempts` carries (migration 045).
#[derive(Debug, Serialize, ToSchema)]
pub struct AttemptSummary {
    pub attempt_id: String,
    pub request_id: String,
    pub attempt_number: i64,
    pub runner_id: String,
    pub fencing_token: i64,
    pub state: String,
    pub lease_issued_at: String,
    pub lease_expires_at: String,
    pub last_heartbeat_at: Option<String>,
    pub event_checkpoint: Option<String>,
    pub completion_id: Option<String>,
    pub workspace_id: Option<String>,
    pub base_revision: Option<String>,
    /// `tack_orch::execution::ActualExecution` once the attempt has
    /// reported one, else `null`.
    pub actual_execution: Option<Value>,
    pub terminal_reason: Option<Value>,
    /// `tack_orch::execution::Usage` once reported, else `null` — never a
    /// fabricated zero.
    pub usage: Option<Value>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// `tack_orch::usage_provenance::ModelProvenance` (`matched` /
    /// `auto_select_observed` / `mismatched`), or `null` when the attempt
    /// has not yet reported `actual_execution` — populated by
    /// `derive_attempt_facts`. Runtime type stays untyped `Value` (the real
    /// type lives in `tack-orch`, which does not depend on `utoipa`);
    /// `#[schema(...)]` below points the *generated OpenAPI document* at
    /// `ModelProvenanceSchema`, this file's hand-verified mirror, instead of
    /// the untyped-`Value` default — an untyped default would generate an
    /// empty `{}` schema, the same spec-drift problem this API avoids
    /// elsewhere.
    #[schema(value_type = Option<ModelProvenanceSchema>, nullable)]
    pub model_provenance: Option<Value>,
    /// `tack_orch::usage_provenance::UsageEconomics` — two
    /// independently-provenanced dollar dimensions, never summed. Absent
    /// usage/timestamps serialize as `{"value": null, "source":
    /// "not_measured"}`, never a structural zero; no runner infra cost-rate
    /// is stored anywhere in this schema today, so
    /// `runner_time_cost.cost_usd_estimated` is always `not_measured` in
    /// every real response from this endpoint. `#[schema(...)]` below
    /// points the generated document at `UsageEconomicsSchema`, the same
    /// fix as `model_provenance` above.
    #[schema(value_type = UsageEconomicsSchema)]
    pub usage_economics: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AttemptListResponse {
    pub protocol_version: u32,
    pub data: Vec<AttemptSummary>,
}

/// One event as reported by `GET
/// /api/executions/{id}/attempts/{attempt_number}/events`.
#[derive(Debug, Serialize, ToSchema)]
pub struct EventSummary {
    pub event_id: String,
    pub sequence: i64,
    pub source: String,
    pub kind: String,
    pub payload: Value,
    pub occurred_at: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EventListResponse {
    pub protocol_version: u32,
    pub data: Vec<EventSummary>,
}

/// Response body for `POST /api/executions/{id}/cancel`. `state` is
/// deliberately **not** a `docs/contracts/runner-v1/lifecycle-transitions.json`
/// value — cancellation is recorded as a request only
/// (`cancellation_requested_at`); the request's real lifecycle state is
/// unaffected and visible via `GET /api/executions/{id}`.
#[derive(Debug, Serialize, ToSchema)]
pub struct CancellationRequestedResponse {
    pub protocol_version: u32,
    pub request_id: String,
    #[schema(example = "cancellation_requested")]
    pub state: String,
}

/// Response body for `POST /api/executions/{id}/requeue`.
#[derive(Debug, Serialize, ToSchema)]
pub struct RequeueResponse {
    pub protocol_version: u32,
    pub request_id: String,
    #[schema(example = "queued")]
    pub state: String,
    #[schema(example = "needs_operator")]
    pub recovered_from: String,
    pub replayed: bool,
}

pub fn routes(state: OperatorExecutionState) -> Router {
    Router::new()
        .route("/executions", post(create_execution).get(list_executions))
        .route("/executions/{request_id}", get(get_execution))
        .route(
            "/executions/{request_id}/attempts",
            get(list_execution_attempts),
        )
        .route(
            "/executions/{request_id}/attempts/{attempt_number}/events",
            get(list_execution_attempt_events),
        )
        .route(
            "/executions/{request_id}/cancel",
            post(request_cancellation),
        )
        .route(
            "/executions/{request_id}/requeue",
            post(requeue_needs_operator),
        )
        .with_state(state)
}

#[utoipa::path(
    post,
    path = "/api/executions",
    tag = "execution-operator",
    request_body = CreateExecution,
    responses(
        (status = 200, description = "Execution request created or idempotently replayed", body = CreateExecutionResponse),
        (status = 400, description = "invalid_request", body = RunnerV1ErrorEnvelope),
        (status = 404, description = "not_found (item does not exist)", body = RunnerV1ErrorEnvelope),
        (status = 409, description = "conflict / idempotency_conflict / runner_revoked", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn create_execution(
    State(state): State<OperatorExecutionState>,
    headers: HeaderMap,
    Json(input): Json<CreateExecution>,
) -> Result<Json<CreateExecutionResponse>, (StatusCode, Json<Value>)> {
    let authenticated_principal = principal(&headers)?;
    let idempotency_scope = format!("operator:{authenticated_principal}");
    let Some(item) = state.repo.get_item(input.item_id).await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not verify item",
            json!({}),
        )
    })?
    else {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Item does not exist",
            json!({"resource": "item"}),
        ));
    };
    let existing_snapshot: Option<String> = sqlx::query_scalar(
        "SELECT request_snapshot FROM execution_requests WHERE idempotency_scope=? AND idempotency_key=?",
    )
    .bind(&idempotency_scope)
    .bind(&input.idempotency_key)
    .fetch_optional(state.repo.pool())
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not load idempotency replay",
            json!({}),
        )
    })?;
    // One scheduling owner, the mirror direction: `dispatcher::dispatch_item`
    // already refuses legacy Docket dispatch when the item has a live runner-v1
    // request (`tack_db::repo::orch::Repository::has_active_execution_request_for_item`);
    // this is the missing other half, closed with that query's own mirror,
    // `has_active_docket_task_for_item`. Skipped for an idempotent replay
    // (`existing_snapshot.is_some()`) — a replay creates no new row, so there is
    // nothing here to collide with — and consulted only while orchestration is
    // effectively on, so a stale `orch_tasks` row from a previously-enabled bridge
    // can never block runner-v1, which stays the plan of record either way.
    if existing_snapshot.is_none()
        && (state.orchestration_enabled)(state.repo.pool().clone()).await
        && state
            .repo
            .has_active_docket_task_for_item(input.item_id)
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Could not verify legacy Docket scheduling state",
                    json!({}),
                )
            })?
    {
        return Err(error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Item has an active legacy Docket task; refusing to create a runner-v1 \
             execution request to preserve one scheduling owner",
            json!({"item_id": input.item_id}),
        ));
    }
    // An exact retry must be allowed to reach the durable replay record even
    // if a mutable runner status changed after the original create.
    if existing_snapshot.is_none() && input.selector_kind == "exact_runner" {
        let eligible: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_runners WHERE id = ? AND state = 'active' AND revoked_at IS NULL)")
            .bind(&input.selector_id).fetch_one(state.repo.pool()).await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Could not verify runner",
                    json!({}),
                )
            })?;
        if !eligible {
            return Err(error(
                StatusCode::CONFLICT,
                StableErrorCode::RunnerRevoked,
                "The selected runner is unavailable or revoked",
                json!({"runner_id": input.selector_id}),
            ));
        }
    }
    if !matches!(input.selector_kind.as_str(), "exact_runner" | "fleet") {
        return Err(error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "selector_kind must be exact_runner or fleet",
            json!({"field": "selector_kind"}),
        ));
    }
    // `resolve_request_model_policy` resolves the model via request
    // override → agent-profile default → project default → fleet default →
    // auto-select. Only resolve when the client expressed no explicit
    // choice: an explicit `requested_model_provider`/`requested_model_id`
    // pair is itself the highest-precedence tier
    // (`ModelPolicyTier::RequestOverride`) and must never be overridden by
    // a lower tier's default.
    let (resolved_model_provider, resolved_model_id) =
        if input.requested_model_provider.is_none() && input.requested_model_id.is_none() {
            let fleet_id = (input.selector_kind == "fleet").then_some(input.selector_id.as_str());
            let project_id = item.project_id.to_string();
            let resolved = resolve_request_model_policy(
                &state.repo,
                Some(input.agent_profile_id.as_str()),
                Some(project_id.as_str()),
                fleet_id,
                None,
            )
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Could not resolve model policy",
                    json!({}),
                )
            })?;
            match resolved.selector {
                ModelSelector::AutoSelect => (None, None),
                ModelSelector::Explicit { provider, model_id } => (
                    Some(provider.as_str().to_string()),
                    Some(model_id.as_str().to_string()),
                ),
            }
        } else {
            (
                input.requested_model_provider.clone(),
                input.requested_model_id.clone(),
            )
        };
    let (request_id, created_at) = match existing_snapshot {
        Some(snapshot) => {
            // A stored replay row that fails to parse means the *persisted*
            // snapshot is corrupt, not that the caller's idempotency key
            // collided with a different payload — that distinct, genuinely
            // client-caused case is detected below via
            // `EnqueueResult::Conflict`. An unreadable row we ourselves wrote
            // is a server-side fault, so it is `internal_error`, not
            // `idempotency_conflict`.
            let value: Value = serde_json::from_str(&snapshot).map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Stored request snapshot is invalid",
                    json!({}),
                )
            })?;
            let object = value.as_object().ok_or_else(|| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Stored request snapshot is invalid",
                    json!({}),
                )
            })?;
            let request_id = object
                .get("request_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        StableErrorCode::InternalError,
                        "Stored request snapshot is invalid",
                        json!({}),
                    )
                })?;
            let created_at = object
                .get("created_at")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        StableErrorCode::InternalError,
                        "Stored request snapshot is invalid",
                        json!({}),
                    )
                })?;
            let created_at = DateTime::parse_from_rfc3339(created_at)
                .map(|time| time.with_timezone(&Utc))
                .map_err(|_| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        StableErrorCode::InternalError,
                        "Stored request snapshot is invalid",
                        json!({}),
                    )
                })?;
            (request_id.to_owned(), created_at)
        }
        None => (
            stable_request_id(&idempotency_scope, &input.idempotency_key),
            state.clock.now(),
        ),
    };
    let item_id = input.item_id.to_string();
    let selector = match input.selector_kind.as_str() {
        "exact_runner" => json!({"kind":"exact_runner","runner_id":input.selector_id}),
        "fleet" => json!({"kind":"fleet","fleet_id":input.selector_id}),
        _ => unreachable!("selector kind was validated"),
    };
    let snapshot_value = json!({
        "request_id": request_id,
        "item_id": item_id,
        "idempotency_key": input.idempotency_key,
        "created_by": {"source":"operator_api","subject_id":authenticated_principal},
        "created_at": created_at.to_rfc3339(),
        "selector": selector,
        "agent_profile_id": input.agent_profile_id,
        "resolved_agent_profile": input.agent_profile_snapshot,
        "requested_harness_kind": input.requested_harness_kind,
        "requested_model_provider": resolved_model_provider.clone(),
        "requested_model_id": resolved_model_id.clone(),
        "repository": input.repository_snapshot,
        "permission_policy": input.permission_policy,
        "timeout_seconds": input.timeout_seconds,
        "budgets": input.budgets,
        "status_map_policy_id": input.status_map_policy_id,
        "environment": input.environment,
        "metadata": input.metadata,
    });
    let typed_snapshot: ExecutionRequestSnapshot =
        serde_json::from_value(snapshot_value).map_err(|err| {
            error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "Execution request snapshot is incomplete or invalid",
                json!({"field": err.to_string()}),
            )
        })?;
    let snapshot_value = serde_json::to_value(typed_snapshot).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "Execution request snapshot is invalid",
            json!({"field": "execution_request_snapshot"}),
        )
    })?;
    let request_snapshot = canonical_string(snapshot_value.clone(), "request_snapshot")?;
    let request_fingerprint = fingerprint(&request_snapshot);
    let root = snapshot_value.as_object().ok_or_else(|| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "Execution request snapshot is invalid",
            json!({"field": "execution_request_snapshot"}),
        )
    })?;
    let serialized_field = |snapshot_field: &str, public_field: &str| {
        canonical_string(root[snapshot_field].clone(), public_field)
    };
    let agent_profile_snapshot =
        serialized_field("resolved_agent_profile", "agent_profile_snapshot")?;
    let repository_snapshot = serialized_field("repository", "repository_snapshot")?;
    let permission_policy = serialized_field("permission_policy", "permission_policy")?;
    let budgets = serialized_field("budgets", "budgets")?;
    let environment = serialized_field("environment", "environment")?;
    let metadata = serialized_field("metadata", "metadata")?;
    let timeout_seconds = i64::try_from(input.timeout_seconds).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "timeout_seconds is out of range",
            json!({"field": "timeout_seconds"}),
        )
    })?;
    let request_clock = FixedExecutionClock(created_at);
    let result = state
        .repo
        .enqueue_execution(
            NewExecutionRequest {
                id: &request_id,
                item_id: &item_id,
                idempotency_scope: &idempotency_scope,
                idempotency_key: &input.idempotency_key,
                request_fingerprint: &request_fingerprint,
                selector_kind: &input.selector_kind,
                selector_id: &input.selector_id,
                agent_profile_id: Some(&input.agent_profile_id),
                agent_profile_snapshot: &agent_profile_snapshot,
                requested_harness_kind: Some(&input.requested_harness_kind),
                requested_model_provider: resolved_model_provider.as_deref(),
                requested_model_id: resolved_model_id.as_deref(),
                repository_snapshot: &repository_snapshot,
                permission_policy: &permission_policy,
                timeout_seconds: Some(timeout_seconds),
                budgets: &budgets,
                status_map_policy_id: input.status_map_policy_id.as_deref(),
                environment: &environment,
                metadata: &metadata,
                request_snapshot: &request_snapshot,
            },
            &request_clock,
        )
        .await
        .map_err(|err| {
            // `err` is a driver-level `sqlx::Error` (a constraint name, a
            // SQLite status message, or one of `execution.rs`'s own fixed
            // `snapshot_error` strings) — never a bound value, credential or
            // request body, so it is safe to log verbatim. Without this, an
            // enqueue failure that the server itself could name (a fenced-
            // out FK reference, a contended write, a snapshot mismatch)
            // reaches the client as an undifferentiated `internal_error`
            // with no trace of which of those it was.
            tracing::warn!(
                item_id = %item_id,
                request_id = %request_id,
                error = %err,
                "enqueue_execution failed"
            );
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not enqueue execution",
                json!({}),
            )
        })?;
    match result {
        EnqueueResult::Created(id) => Ok(Json(CreateExecutionResponse {
            protocol_version: 1,
            request_id: id,
            state: "queued".into(),
            replayed: false,
        })),
        EnqueueResult::Replayed(id) => Ok(Json(CreateExecutionResponse {
            protocol_version: 1,
            request_id: id,
            state: "queued".into(),
            replayed: true,
        })),
        EnqueueResult::Conflict => Err(error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The idempotency key was used with a different request",
            json!({"idempotency_key": input.idempotency_key}),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/api/executions",
    tag = "execution-operator",
    params(ListExecutionsQuery),
    responses(
        (status = 200, description = "Execution requests, newest first. `item_ids` present: exactly one row per id that has at least one execution, the batch's own most recent one. Else scoped to one item when `item_id` is given, otherwise every request the install has recorded up to `limit`", body = ExecutionListResponse),
        (status = 400, description = "invalid_request (a malformed `item_ids` entry, or more ids than the route's cap)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn list_executions(
    State(state): State<OperatorExecutionState>,
    Query(query): Query<ListExecutionsQuery>,
) -> Result<Json<ExecutionListResponse>, (StatusCode, Json<Value>)> {
    let item_ids = query.parsed_item_ids().map_err(|bad_value| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "item_ids must be a comma-separated list of UUIDs",
            json!({"field": "item_ids", "value": bad_value}),
        )
    })?;
    let rows = if let Some(item_ids) = item_ids {
        let item_ids: Vec<String> = item_ids.iter().map(Uuid::to_string).collect();
        state.repo.list_latest_executions_for_items(&item_ids).await
    } else {
        let limit = query.effective_limit();
        let item_id = query.item_id.map(|id| id.to_string());
        state.repo.list_executions(item_id.as_deref(), limit).await
    }
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not list executions",
            json!({}),
        )
    })?;
    let data: Vec<ExecutionSummary> = rows
        .into_iter()
        .map(|row| ExecutionSummary {
            request_id: row.id,
            item_id: row.item_id,
            state: row.state,
            cancellation_requested_at: row.cancellation_requested_at,
            created_at: row.created_at,
        })
        .collect();
    Ok(Json(ExecutionListResponse {
        protocol_version: 1,
        data,
    }))
}

#[utoipa::path(
    get,
    path = "/api/executions/{request_id}",
    tag = "execution-operator",
    params(("request_id" = String, Path, description = "Execution request ID (opaque)")),
    responses(
        (status = 200, description = "Execution request detail", body = ExecutionDetailResponse),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn get_execution(
    State(state): State<OperatorExecutionState>,
    Path(request_id): Path<String>,
) -> Result<Json<ExecutionDetailResponse>, (StatusCode, Json<Value>)> {
    let row = sqlx::query("SELECT id, item_id, state, cancellation_requested_at, created_at FROM execution_requests WHERE id = ?")
        .bind(&request_id).fetch_optional(state.repo.pool()).await.map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not load execution",
                json!({}),
            )
        })?;
    let Some(row) = row else {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Execution request does not exist",
            json!({"resource": "execution_request"}),
        ));
    };
    Ok(Json(ExecutionDetailResponse {
        protocol_version: 1,
        request_id: row.get("id"),
        item_id: row.get("item_id"),
        state: row.get("state"),
        cancellation_requested_at: row.get("cancellation_requested_at"),
        created_at: row.get("created_at"),
    }))
}

/// `GET /api/executions/{request_id}/attempts` — the operator read path
/// for attempt data. `execution_attempts` (migration 045) is written by
/// the runner-v1 protocol, but `GET /executions/{id}` returns only 5
/// scalar columns (`request_id, item_id, state, cancellation_requested_at,
/// created_at`), never attempt data; this endpoint closes that gap. An
/// empty list here is a real, honest "no attempt yet" (the request is
/// still `queued`), not a placeholder — distinct from the 404 an unknown
/// `request_id` gets.
#[utoipa::path(
    get,
    path = "/api/executions/{request_id}/attempts",
    tag = "execution-operator",
    params(("request_id" = String, Path, description = "Execution request ID (opaque)")),
    responses(
        (status = 200, description = "Every attempt made against this request, oldest first (may be empty)", body = AttemptListResponse),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn list_execution_attempts(
    State(state): State<OperatorExecutionState>,
    Path(request_id): Path<String>,
) -> Result<Json<AttemptListResponse>, (StatusCode, Json<Value>)> {
    // Also carries `requested_model_provider`/`requested_model_id` —
    // `derive_attempt_facts` needs the *requested* side of provenance,
    // which lives on `execution_requests`, not on any one attempt row.
    // Same not-found semantics as a plain `EXISTS(...)` check, one query
    // instead of two.
    let request_row = sqlx::query(
        "SELECT requested_model_provider, requested_model_id FROM execution_requests WHERE id = ?",
    )
    .bind(&request_id)
    .fetch_optional(state.repo.pool())
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not verify execution",
            json!({}),
        )
    })?;
    let Some(request_row) = request_row else {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Execution request does not exist",
            json!({"resource": "execution_request"}),
        ));
    };
    let requested_model_provider: Option<String> = request_row.get("requested_model_provider");
    let requested_model_id: Option<String> = request_row.get("requested_model_id");
    let attempts = state
        .repo
        .list_attempts_for_request(&request_id)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not list attempts",
                json!({}),
            )
        })?;
    let data: Vec<AttemptSummary> = attempts
        .into_iter()
        .map(|attempt| {
            // `derive_attempt_facts` is a convenience "service handler" —
            // takes the exact raw column shapes `AttemptListingRow` already
            // carries, so nothing here re-derives provenance/economics
            // logic. No runner infra cost-rate is stored anywhere in this
            // schema today, so `runner_rate_usd_per_hour` is always `None`
            // here — `runner_time_cost.cost_usd_estimated` stays honestly
            // `not_measured`, never a fabricated rate.
            let facts = derive_attempt_facts(
                requested_model_provider.as_deref(),
                requested_model_id.as_deref(),
                attempt.actual_execution.as_deref(),
                attempt.usage.as_deref(),
                attempt.started_at.as_deref().and_then(parse_rfc3339),
                attempt.ended_at.as_deref().and_then(parse_rfc3339),
                None,
            );
            AttemptSummary {
                attempt_id: attempt.id,
                request_id: attempt.request_id,
                attempt_number: attempt.attempt_number,
                runner_id: attempt.runner_id,
                fencing_token: attempt.fencing_token,
                state: attempt.state,
                lease_issued_at: attempt.lease_issued_at,
                lease_expires_at: attempt.lease_expires_at,
                last_heartbeat_at: attempt.last_heartbeat_at,
                event_checkpoint: attempt.event_checkpoint,
                completion_id: attempt.completion_id,
                workspace_id: attempt.workspace_id,
                base_revision: attempt.base_revision,
                actual_execution: attempt
                    .actual_execution
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok()),
                terminal_reason: attempt
                    .terminal_reason
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok()),
                usage: attempt
                    .usage
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok()),
                started_at: attempt.started_at,
                ended_at: attempt.ended_at,
                created_at: attempt.created_at,
                updated_at: attempt.updated_at,
                model_provenance: facts.model_provenance.map(|provenance| {
                    serde_json::to_value(provenance).expect("ModelProvenance serializes")
                }),
                usage_economics: serde_json::to_value(facts.usage_economics)
                    .expect("UsageEconomics serializes"),
            }
        })
        .collect();
    Ok(Json(AttemptListResponse {
        protocol_version: 1,
        data,
    }))
}

/// `GET /api/executions/{request_id}/attempts/{attempt_number}/events` —
/// the other half of the attempts/events read path above. Returns
/// `404` naming which resource is missing (`execution_request` vs
/// `execution_attempt`) rather than a single ambiguous not-found, since a
/// client can otherwise not distinguish "wrong request id" from "this
/// attempt number never existed."
#[utoipa::path(
    get,
    path = "/api/executions/{request_id}/attempts/{attempt_number}/events",
    tag = "execution-operator",
    params(
        ("request_id" = String, Path, description = "Execution request ID (opaque)"),
        ("attempt_number" = i64, Path, description = "1-based attempt number"),
    ),
    responses(
        (status = 200, description = "Every event this attempt has reported, oldest first (may be empty)", body = EventListResponse),
        (status = 404, description = "not_found (execution_request or execution_attempt)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn list_execution_attempt_events(
    State(state): State<OperatorExecutionState>,
    Path((request_id, attempt_number)): Path<(String, i64)>,
) -> Result<Json<EventListResponse>, (StatusCode, Json<Value>)> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_requests WHERE id = ?)")
            .bind(&request_id)
            .fetch_one(state.repo.pool())
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Could not verify execution",
                    json!({}),
                )
            })?;
    if !exists {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Execution request does not exist",
            json!({"resource": "execution_request"}),
        ));
    }
    let events = state
        .repo
        .list_events_for_attempt_number(&request_id, attempt_number)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not list events",
                json!({}),
            )
        })?;
    let Some(events) = events else {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Attempt does not exist",
            json!({"resource": "execution_attempt"}),
        ));
    };
    let data: Vec<EventSummary> = events
        .into_iter()
        .map(|event| EventSummary {
            event_id: event.event_id,
            sequence: event.sequence,
            source: event.source,
            kind: event.kind,
            payload: serde_json::from_str::<Value>(&event.payload).unwrap_or(Value::Null),
            occurred_at: event.occurred_at,
            created_at: event.created_at,
        })
        .collect();
    Ok(Json(EventListResponse {
        protocol_version: 1,
        data,
    }))
}

#[utoipa::path(
    post,
    path = "/api/executions/{request_id}/cancel",
    tag = "execution-operator",
    params(("request_id" = String, Path, description = "Execution request ID (opaque)")),
    responses(
        (status = 200, description = "Cancellation requested — not yet terminal", body = CancellationRequestedResponse),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
        (status = 409, description = "conflict (already terminal)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn request_cancellation(
    State(state): State<OperatorExecutionState>,
    Path(request_id): Path<String>,
) -> Result<Json<CancellationRequestedResponse>, (StatusCode, Json<Value>)> {
    let changed = state
        .repo
        .request_execution_cancellation(&request_id, state.clock.as_ref())
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not request cancellation",
                json!({}),
            )
        })?;
    if !changed {
        // `request_execution_cancellation` returns false both when the
        // request id is unknown and when it exists but already reached a
        // terminal state; disambiguate so the client gets `not_found` only
        // for the former and a genuine, contract-shaped `conflict` for the
        // latter instead of a misleading `not_found`.
        let existing_state: Option<String> =
            sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = ?")
                .bind(&request_id)
                .fetch_optional(state.repo.pool())
                .await
                .map_err(|_| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        StableErrorCode::InternalError,
                        "Could not verify execution state",
                        json!({}),
                    )
                })?;
        return Err(match existing_state {
            None => error(
                StatusCode::NOT_FOUND,
                StableErrorCode::NotFound,
                "Execution request does not exist",
                json!({"resource": "execution_request"}),
            ),
            Some(_) => error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "Execution already reached a terminal state before cancellation could apply",
                json!({}),
            ),
        });
    }
    Ok(Json(CancellationRequestedResponse {
        protocol_version: 1,
        request_id,
        state: "cancellation_requested".into(),
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RecoveryConfirmation {
    pub recovery_key: String,
    pub reason: String,
}

#[utoipa::path(
    post,
    path = "/api/executions/{request_id}/requeue",
    tag = "execution-operator",
    params(("request_id" = String, Path, description = "Execution request ID (opaque)")),
    request_body = RecoveryConfirmation,
    responses(
        (status = 200, description = "Requeued (or replayed) after an audited recovery decision", body = RequeueResponse),
        (status = 409, description = "conflict / idempotency_conflict / invalid_transition", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn requeue_needs_operator(
    State(state): State<OperatorExecutionState>,
    headers: HeaderMap,
    Path(request_id): Path<String>,
    Json(input): Json<RecoveryConfirmation>,
) -> Result<Json<RequeueResponse>, (StatusCode, Json<Value>)> {
    let actor = principal(&headers)?;
    let reason_fingerprint = fingerprint(&input.reason);
    let result = state
        .repo
        .operator_requeue_needs_operator(
            &request_id,
            &input.recovery_key,
            &actor,
            &reason_fingerprint,
            state.clock.as_ref(),
        )
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not requeue execution",
                json!({}),
            )
        })?;
    match result {
        OperatorRequeueResult::Requeued | OperatorRequeueResult::Replayed => {
            Ok(Json(RequeueResponse {
                protocol_version: 1,
                request_id,
                state: "queued".into(),
                recovered_from: "needs_operator".into(),
                replayed: matches!(result, OperatorRequeueResult::Replayed),
            }))
        }
        OperatorRequeueResult::Conflict => Err(error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The recovery key was used with a different confirmation",
            json!({"idempotency_key": input.recovery_key}),
        )),
        OperatorRequeueResult::InvalidTransition | OperatorRequeueResult::NotFound => {
            // The repo layer reports only that the requeue is disallowed, not
            // which attempt state blocked it. Look up the latest attempt's
            // state so `details` carries `invalid_transition`'s
            // contract-shaped `{"from":..., "to":...}` pair instead of `{}`.
            let from_state: String = sqlx::query_scalar(
                "SELECT state FROM execution_attempts WHERE request_id = ? ORDER BY attempt_number DESC LIMIT 1",
            )
            .bind(&request_id)
            .fetch_optional(state.repo.pool())
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "unknown".to_string());
            Err(error(
                StatusCode::CONFLICT,
                StableErrorCode::InvalidTransition,
                "Only authoritatively recovered needs_operator attempts may be requeued",
                json!({"from": from_state, "to": "queued"}),
            ))
        }
    }
}
