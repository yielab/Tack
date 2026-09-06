//! Operator fleet, runner, and profile handlers, mounted into the global router.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use chrono::Duration;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use tack_db::Repository;
use tack_db::repo::execution::{
    AddFleetMemberOutcome, EnrollmentToken, ExecutionClock, NewAgentProfile, NewRunner,
    SystemExecutionClock,
};
use tack_orch::execution::{ProtocolErrorEnvelope, StableErrorCode};
use utoipa::ToSchema;
use uuid::Uuid;

use super::executions::{OperatorExecutionState, RunnerV1ErrorEnvelope};

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

/// Whether a `sqlx::Error` from an INSERT is a unique-constraint violation on
/// a caller-chosen name, as opposed to any other database-level fault. Used
/// so name collisions map to `conflict` and everything else maps to
/// `internal_error`, instead of collapsing every insert failure into one code.
fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|db_error| db_error.is_unique_violation())
}

pub fn routes(state: OperatorExecutionState) -> Router {
    Router::new()
        .route("/runner-fleets", post(create_fleet).get(list_fleets))
        .route("/runner-fleets/{fleet_id}/members", post(add_fleet_member))
        .route(
            "/runner-fleets/{fleet_id}/members/{runner_id}",
            delete(remove_fleet_member),
        )
        .route("/runners", get(list_runners))
        .route("/runners/enrollment", post(create_pending_runner))
        .route(
            "/runners/{runner_id}/enrollment-tokens/{token_id}/revoke",
            post(revoke_enrollment_token),
        )
        .route("/runners/{runner_id}/revoke", post(revoke_runner))
        .route("/agent-profiles", post(create_profile).get(list_profiles))
        .route(
            "/model-profiles",
            post(create_model_profile).get(list_model_profiles),
        )
        .with_state(state)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateFleet {
    pub name: String,
    #[serde(default)]
    pub concurrency_limit: Option<i64>,
    #[serde(default = "empty")]
    pub default_policy: Value,
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProfile {
    pub name: String,
    pub instructions: String,
    #[serde(default = "empty")]
    pub tool_policy: Value,
    #[serde(default = "empty")]
    pub limits: Value,
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateModelProfile {
    pub name: String,
    pub model_provider: String,
    pub model_id: String,
    #[serde(default)]
    pub config_reference: Option<String>,
}
fn empty() -> Value {
    json!({})
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePendingRunner {
    pub name: String,
    #[serde(default = "empty")]
    pub labels: Value,
    pub total_capacity: i64,
    pub available_capacity: i64,
    #[serde(default = "empty")]
    pub capability_snapshot: Value,
    #[serde(default = "protocol_v1")]
    pub protocol_version: i64,
    #[serde(default = "default_enrollment_lifetime_seconds")]
    pub enrollment_lifetime_seconds: i64,
}

fn protocol_v1() -> i64 {
    1
}

fn default_enrollment_lifetime_seconds() -> i64 {
    60 * 60
}

fn token_hash(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateFleetResponse {
    pub protocol_version: u32,
    pub fleet_id: String,
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FleetSummary {
    pub fleet_id: String,
    pub name: String,
    pub concurrency_limit: Option<i64>,
    pub default_policy: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FleetListResponse {
    pub protocol_version: u32,
    pub data: Vec<FleetSummary>,
}

/// One row of `GET /api/runners` — every column `agent_runners` carries
/// (minus credential material — see `RunnerListingRow`'s own doc comment in
/// `tack-db`), plus its current fleet roster.
#[derive(Debug, Serialize, ToSchema)]
pub struct RunnerSummary {
    pub runner_id: String,
    pub name: String,
    #[schema(example = "active")]
    pub state: String,
    /// Parsed `labels` — `null` only if the stored value is somehow not
    /// valid JSON (`labels_raw` always carries the raw stored string, so no
    /// information is lost even then).
    pub labels: Option<Value>,
    pub labels_raw: String,
    pub total_capacity: i64,
    pub available_capacity: i64,
    /// Parsed `capability_snapshot` — see `labels`' doc comment for the
    /// same "raw string always present" guarantee.
    pub capability_snapshot: Option<Value>,
    pub capability_snapshot_raw: String,
    pub protocol_version: i64,
    pub runner_version: Option<String>,
    pub last_heartbeat_at: Option<String>,
    pub revoked_at: Option<String>,
    pub fleet_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunnerListResponse {
    pub protocol_version: u32,
    pub data: Vec<RunnerSummary>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RevokeRunnerResponse {
    pub protocol_version: u32,
    pub runner_id: String,
    #[schema(example = "revoked")]
    pub state: String,
}

/// Response body for `POST /api/runners/enrollment`. `enrollment_token` is
/// the raw, one-time secret — never persisted, never returned again by any
/// later response.
#[derive(Debug, Serialize, ToSchema)]
pub struct CreatePendingRunnerResponse {
    pub protocol_version: u32,
    pub runner_id: String,
    pub token_id: String,
    pub enrollment_token: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RevokeEnrollmentTokenResponse {
    pub protocol_version: u32,
    pub runner_id: String,
    pub token_id: String,
    #[schema(example = "revoked")]
    pub state: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateProfileResponse {
    pub protocol_version: u32,
    pub agent_profile_id: String,
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgentProfileSummary {
    pub agent_profile_id: String,
    pub name: String,
    pub instructions: String,
    pub tool_policy: Value,
    pub limits: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgentProfileListResponse {
    pub protocol_version: u32,
    pub data: Vec<AgentProfileSummary>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateModelProfileResponse {
    pub protocol_version: u32,
    pub model_profile_id: String,
    pub name: String,
    pub model_provider: String,
    pub model_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModelProfileSummary {
    pub model_profile_id: String,
    pub name: String,
    pub model_provider: String,
    pub model_id: String,
    pub config_reference: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModelProfileListResponse {
    pub protocol_version: u32,
    pub data: Vec<ModelProfileSummary>,
}

#[utoipa::path(
    post,
    path = "/api/runner-fleets",
    tag = "execution-operator",
    request_body = CreateFleet,
    responses(
        (status = 200, description = "Fleet created", body = CreateFleetResponse),
        (status = 409, description = "conflict (name already exists)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn create_fleet(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreateFleet>,
) -> Result<Json<CreateFleetResponse>, (StatusCode, Json<Value>)> {
    let id = format!("fleet_{}", Uuid::new_v4());
    let now = state.clock.now().to_rfc3339();
    let inserted = sqlx::query("INSERT INTO agent_fleets (id,name,concurrency_limit,default_policy,created_at,updated_at) VALUES (?,?,?,?,?,?)")
        .bind(&id).bind(&input.name).bind(input.concurrency_limit).bind(input.default_policy.to_string()).bind(&now).bind(&now).execute(state.repo.pool()).await;
    match inserted {
        Ok(_) => Ok(Json(CreateFleetResponse {
            protocol_version: 1,
            fleet_id: id,
            name: input.name,
        })),
        Err(err) if is_unique_violation(&err) => Err(error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Fleet name already exists",
            json!({}),
        )),
        Err(_) => Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not create fleet",
            json!({}),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/api/runner-fleets",
    tag = "execution-operator",
    responses(
        (status = 200, description = "Every runner fleet, by name", body = FleetListResponse),
    ),
)]
pub async fn list_fleets(
    State(state): State<OperatorExecutionState>,
) -> Result<Json<FleetListResponse>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query(
        "SELECT id,name,concurrency_limit,default_policy FROM agent_fleets ORDER BY name",
    )
    .fetch_all(state.repo.pool())
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not list fleets",
            json!({}),
        )
    })?;
    let data: Vec<FleetSummary> = rows
        .into_iter()
        .map(|r| -> Result<FleetSummary, (StatusCode, Json<Value>)> {
            let policy = serde_json::from_str::<Value>(&r.get::<String, _>("default_policy"))
                .map_err(|_| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        StableErrorCode::InternalError,
                        "Fleet policy is corrupt",
                        json!({}),
                    )
                })?;
            Ok(FleetSummary {
                fleet_id: r.get("id"),
                name: r.get("name"),
                concurrency_limit: r.get("concurrency_limit"),
                default_policy: policy,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(Json(FleetListResponse {
        protocol_version: 1,
        data,
    }))
}

/// Request body for `POST /api/runner-fleets/{fleet_id}/members`.
/// `agent_fleet_members` (migration 041) is a live scheduling *read* input
/// (`fetch_runner_scheduling_snapshot`, `fetch_fleet_concurrency`, the
/// fleet-selector claim query); this endpoint is the write path that
/// populates a fleet's roster, since an execution request can select
/// either an exact runner or a fleet.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddFleetMember {
    pub runner_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FleetMemberResponse {
    pub protocol_version: u32,
    pub fleet_id: String,
    pub runner_id: String,
    /// One of `"added"`, `"already_member"` (idempotent re-add) or
    /// `"removed"`.
    #[schema(example = "added")]
    pub state: String,
}

#[utoipa::path(
    post,
    path = "/api/runner-fleets/{fleet_id}/members",
    tag = "execution-operator",
    params(("fleet_id" = String, Path, description = "Fleet ID (opaque)")),
    request_body = AddFleetMember,
    responses(
        (status = 200, description = "Runner is now (or already was) a member of the fleet", body = FleetMemberResponse),
        (status = 404, description = "not_found (fleet or runner does not exist)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn add_fleet_member(
    State(state): State<OperatorExecutionState>,
    Path(fleet_id): Path<String>,
    Json(input): Json<AddFleetMember>,
) -> Result<Json<FleetMemberResponse>, (StatusCode, Json<Value>)> {
    let outcome = state
        .repo
        .add_fleet_member(&fleet_id, &input.runner_id, state.clock.as_ref())
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not add fleet member",
                json!({}),
            )
        })?;
    match outcome {
        AddFleetMemberOutcome::Added => Ok(Json(FleetMemberResponse {
            protocol_version: 1,
            fleet_id,
            runner_id: input.runner_id,
            state: "added".into(),
        })),
        AddFleetMemberOutcome::AlreadyMember => Ok(Json(FleetMemberResponse {
            protocol_version: 1,
            fleet_id,
            runner_id: input.runner_id,
            state: "already_member".into(),
        })),
        AddFleetMemberOutcome::FleetNotFound => Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Fleet does not exist",
            json!({"resource": "fleet"}),
        )),
        AddFleetMemberOutcome::RunnerNotFound => Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Runner does not exist",
            json!({"resource": "runner"}),
        )),
    }
}

#[utoipa::path(
    delete,
    path = "/api/runner-fleets/{fleet_id}/members/{runner_id}",
    tag = "execution-operator",
    params(
        ("fleet_id" = String, Path, description = "Fleet ID (opaque)"),
        ("runner_id" = String, Path, description = "Runner ID (opaque)"),
    ),
    responses(
        (status = 200, description = "Runner removed from the fleet", body = FleetMemberResponse),
        (status = 404, description = "not_found (runner was not a member of this fleet)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn remove_fleet_member(
    State(state): State<OperatorExecutionState>,
    Path((fleet_id, runner_id)): Path<(String, String)>,
) -> Result<Json<FleetMemberResponse>, (StatusCode, Json<Value>)> {
    let removed = state
        .repo
        .remove_fleet_member(&fleet_id, &runner_id)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not remove fleet member",
                json!({}),
            )
        })?;
    if !removed {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Runner is not a member of this fleet",
            json!({"resource": "fleet_member"}),
        ));
    }
    Ok(Json(FleetMemberResponse {
        protocol_version: 1,
        fleet_id,
        runner_id,
        state: "removed".into(),
    }))
}

#[derive(Debug, Deserialize, Default, utoipa::IntoParams)]
pub struct ListRunnersQuery {
    /// Optional roster filter — a runner is included only if it is a
    /// current member of this fleet (`agent_fleet_members`).
    #[serde(default)]
    pub fleet_id: Option<String>,
}

/// `GET /api/runners[?fleet_id=]` — the read path for `agent_runners`
/// (migration 040) capacity/health/capability data. `capability_snapshot`/
/// `labels` are parsed JSON (matching every other list handler in this
/// file); a corrupt/unparseable value is reported per-runner as `null`
/// with the raw string preserved separately, rather than failing the whole
/// list for one bad row — an operator inspecting runner health must not
/// lose visibility into every *other* runner because one has a malformed
/// blob.
#[utoipa::path(
    get,
    path = "/api/runners",
    tag = "execution-operator",
    params(ListRunnersQuery),
    responses(
        (status = 200, description = "Every enrolled runner (optionally filtered to one fleet's roster)", body = RunnerListResponse),
    ),
)]
pub async fn list_runners(
    State(state): State<OperatorExecutionState>,
    Query(query): Query<ListRunnersQuery>,
) -> Result<Json<RunnerListResponse>, (StatusCode, Json<Value>)> {
    let runners = state.repo.list_runners().await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not list runners",
            json!({}),
        )
    })?;
    let data: Vec<RunnerSummary> = runners
        .into_iter()
        .filter(|runner| match &query.fleet_id {
            Some(fleet_id) => runner.fleet_ids.contains(fleet_id),
            None => true,
        })
        .map(|runner| {
            let labels = serde_json::from_str::<Value>(&runner.labels).ok();
            let capability_snapshot =
                serde_json::from_str::<Value>(&runner.capability_snapshot).ok();
            RunnerSummary {
                runner_id: runner.id,
                name: runner.name,
                state: runner.state,
                labels,
                labels_raw: runner.labels,
                total_capacity: runner.total_capacity,
                available_capacity: runner.available_capacity,
                capability_snapshot,
                capability_snapshot_raw: runner.capability_snapshot,
                protocol_version: runner.protocol_version,
                runner_version: runner.runner_version,
                last_heartbeat_at: runner.last_heartbeat_at,
                revoked_at: runner.revoked_at,
                fleet_ids: runner.fleet_ids,
                created_at: runner.created_at,
                updated_at: runner.updated_at,
            }
        })
        .collect();
    Ok(Json(RunnerListResponse {
        protocol_version: 1,
        data,
    }))
}

#[utoipa::path(
    post,
    path = "/api/runners/{runner_id}/revoke",
    tag = "execution-operator",
    params(("runner_id" = String, Path, description = "Runner ID (opaque)")),
    responses(
        (status = 200, description = "Runner revoked", body = RevokeRunnerResponse),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn revoke_runner(
    State(state): State<OperatorExecutionState>,
    Path(runner_id): Path<String>,
) -> Result<Json<RevokeRunnerResponse>, (StatusCode, Json<Value>)> {
    let changed = state
        .repo
        .revoke_runner(&runner_id, state.clock.as_ref())
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not revoke runner",
                json!({}),
            )
        })?;
    if !changed {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Runner does not exist",
            json!({"resource": "runner"}),
        ));
    }
    Ok(Json(RevokeRunnerResponse {
        protocol_version: 1,
        runner_id,
        state: "revoked".into(),
    }))
}

/// Creates a pending runner and stores only a SHA-256 enrollment-token hash.
/// The raw token is deliberately emitted once here and is never readable from
/// metadata, list, revocation, or runner responses.
#[utoipa::path(
    post,
    path = "/api/runners/enrollment",
    tag = "execution-operator",
    request_body = CreatePendingRunner,
    responses(
        (status = 200, description = "Pending runner created; the raw enrollment token is returned exactly once", body = CreatePendingRunnerResponse),
        (status = 400, description = "invalid_request", body = RunnerV1ErrorEnvelope),
        (status = 409, description = "conflict (name already exists)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn create_pending_runner(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreatePendingRunner>,
) -> Result<Json<CreatePendingRunnerResponse>, (StatusCode, Json<Value>)> {
    provision_pending_runner(&state.repo, state.clock.as_ref(), input)
        .await
        .map(Json)
        .map_err(|err| match err {
            ProvisionError::InvalidLifetime => error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "enrollment_lifetime_seconds must be positive",
                json!({"field": "enrollment_lifetime_seconds"}),
            ),
            ProvisionError::LifetimeOutOfRange => error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "enrollment_lifetime_seconds is out of range",
                json!({"field": "enrollment_lifetime_seconds"}),
            ),
            ProvisionError::LabelsNotSerializable => error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "labels cannot be serialized",
                json!({"field": "labels"}),
            ),
            ProvisionError::CapabilitySnapshotNotSerializable => error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "capability_snapshot cannot be serialized",
                json!({"field": "capability_snapshot"}),
            ),
            ProvisionError::InvalidCapacityOrWindow => error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "Pending runner capacity or enrollment window is invalid",
                json!({"field": "total_capacity"}),
            ),
            ProvisionError::NameConflict => error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "Runner name already exists",
                json!({}),
            ),
            ProvisionError::Internal => error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not create pending runner",
                json!({}),
            ),
        })
}

/// Failure from [`provision_pending_runner`], independent of any transport.
/// [`create_pending_runner`] maps each variant to the exact status code and
/// error envelope the HTTP route has always returned; [`provision_local_runner`]
/// surfaces the same variants to its own in-process caller instead.
#[derive(Debug, thiserror::Error)]
pub enum ProvisionError {
    #[error("enrollment_lifetime_seconds must be positive")]
    InvalidLifetime,
    #[error("enrollment_lifetime_seconds is out of range")]
    LifetimeOutOfRange,
    #[error("labels cannot be serialized")]
    LabelsNotSerializable,
    #[error("capability_snapshot cannot be serialized")]
    CapabilitySnapshotNotSerializable,
    #[error("pending runner capacity or enrollment window is invalid")]
    InvalidCapacityOrWindow,
    #[error("runner name already exists")]
    NameConflict,
    #[error("could not create pending runner")]
    Internal,
}

/// Validates `input`, generates the runner/token ids and the raw one-time
/// token, hashes it, and persists the pending runner via
/// `create_pending_runner_and_issue_token`. The raw token is returned only in
/// [`CreatePendingRunnerResponse::enrollment_token`] and is never stored —
/// only its hash is. Shared by the HTTP route (`create_pending_runner`) and
/// by in-process local provisioning (`provision_local_runner`); neither the
/// validation nor the storage differs between the two callers, only how the
/// result is reported back.
pub async fn provision_pending_runner(
    repo: &Repository,
    clock: &dyn ExecutionClock,
    input: CreatePendingRunner,
) -> Result<CreatePendingRunnerResponse, ProvisionError> {
    if input.enrollment_lifetime_seconds <= 0 {
        return Err(ProvisionError::InvalidLifetime);
    }
    let runner_id = format!("runr_{}", Uuid::new_v4());
    let token_id = format!("ent_{}", Uuid::new_v4());
    let raw_token = format!("enr_{}", Uuid::new_v4());
    let now = clock.now();
    let enrollment_lifetime = Duration::try_seconds(input.enrollment_lifetime_seconds)
        .ok_or(ProvisionError::LifetimeOutOfRange)?;
    let expires_at = now
        .checked_add_signed(enrollment_lifetime)
        .ok_or(ProvisionError::LifetimeOutOfRange)?;
    let labels =
        serde_json::to_string(&input.labels).map_err(|_| ProvisionError::LabelsNotSerializable)?;
    let capabilities = serde_json::to_string(&input.capability_snapshot)
        .map_err(|_| ProvisionError::CapabilitySnapshotNotSerializable)?;
    let enrollment_token_hash = token_hash(&raw_token);
    repo.create_pending_runner_and_issue_token(
        NewRunner {
            id: &runner_id,
            name: &input.name,
            credential_hash: "pending:no-credential",
            labels: &labels,
            total_capacity: input.total_capacity,
            available_capacity: input.available_capacity,
            capability_snapshot: &capabilities,
            protocol_version: input.protocol_version,
        },
        EnrollmentToken {
            id: &token_id,
            runner_id: &runner_id,
            token_hash: &enrollment_token_hash,
            expires_at,
        },
        clock,
    )
    .await
    .map_err(|err| match err {
        // The repo layer reports out-of-bounds capacity/expiry via
        // `sqlx::Error::Protocol` — a genuine client input problem.
        sqlx::Error::Protocol(_) => ProvisionError::InvalidCapacityOrWindow,
        _ if is_unique_violation(&err) => ProvisionError::NameConflict,
        _ => ProvisionError::Internal,
    })?;
    Ok(CreatePendingRunnerResponse {
        protocol_version: 1,
        runner_id,
        token_id,
        enrollment_token: raw_token,
        expires_at: expires_at.to_rfc3339(),
    })
}

/// Failure from [`provision_local_runner`]: either the short-lived pool it
/// opens against `database_url` could not be established, or the
/// provisioning call itself failed.
#[derive(Debug, thiserror::Error)]
pub enum ProvisionLocalRunnerError {
    #[error("could not open the database: {0}")]
    Pool(#[from] sqlx::Error),
    #[error(transparent)]
    Provision(#[from] ProvisionError),
}

/// Self-provisions a single local runner by calling
/// [`provision_pending_runner`] in-process against `database_url` — the same
/// URL `tack_api::server`'s own boot sequence opens — instead of over HTTP.
/// Exists for `tack serve --with-runner`'s zero-touch local case: the
/// operator and the runner are the same person on the same machine, so
/// creating the pending runner and minting its one-time token is an
/// admin/bootstrap concern, not the runner protocol. The returned token
/// still has to be *redeemed* over real runner-v1 HTTP by the runner role —
/// nothing about that path changes or is bypassed here. The runner name is
/// suffixed with a fresh UUID so repeated self-provisioning (e.g. a retry
/// after a crash between provisioning and storing the resulting session)
/// cannot collide with a still-pending runner from an earlier attempt.
///
/// Opens and drops its own connection pool rather than reusing the server's:
/// `tack_api::serve_with_ready` does not hand its `Repository` back to the
/// caller, and SQLite's WAL mode (which `tack_db::init_pool` enables) makes a
/// second, short-lived pool against the same file safe — it is one more
/// connection to the database, not a competing writer racing the server's
/// own pool.
pub async fn provision_local_runner(
    database_url: &str,
) -> Result<CreatePendingRunnerResponse, ProvisionLocalRunnerError> {
    let pool = tack_db::init_pool(database_url).await?;
    let repo = Repository::new(pool);
    let input = CreatePendingRunner {
        name: format!("local-{}", Uuid::new_v4()),
        labels: empty(),
        total_capacity: 1,
        available_capacity: 1,
        capability_snapshot: empty(),
        protocol_version: protocol_v1(),
        enrollment_lifetime_seconds: default_enrollment_lifetime_seconds(),
    };
    let response = provision_pending_runner(&repo, &SystemExecutionClock, input).await?;
    Ok(response)
}

/// Whether `runner_id` still exists at `database_url` — opens and drops its
/// own short-lived pool exactly like [`provision_local_runner`] itself does,
/// against the same URL the caller's own server already opened successfully.
/// That is what makes this check authoritative rather than a guess: a `false`
/// here means this specific database has no such row, not merely that it was
/// unreachable just now.
pub async fn local_runner_id_exists(
    database_url: &str,
    runner_id: &str,
) -> Result<bool, ProvisionLocalRunnerError> {
    let pool = tack_db::init_pool(database_url).await?;
    let repo = Repository::new(pool);
    Ok(repo
        .fetch_runner_scheduling_snapshot(runner_id)
        .await?
        .is_some())
}

#[utoipa::path(
    post,
    path = "/api/runners/{runner_id}/enrollment-tokens/{token_id}/revoke",
    tag = "execution-operator",
    params(
        ("runner_id" = String, Path, description = "Runner ID (opaque)"),
        ("token_id" = String, Path, description = "Enrollment token ID (opaque)"),
    ),
    responses(
        (status = 200, description = "Token revoked", body = RevokeEnrollmentTokenResponse),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
        (status = 409, description = "conflict (already consumed)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn revoke_enrollment_token(
    State(state): State<OperatorExecutionState>,
    Path((runner_id, token_id)): Path<(String, String)>,
) -> Result<Json<RevokeEnrollmentTokenResponse>, (StatusCode, Json<Value>)> {
    let changed = state
        .repo
        .revoke_enrollment_token_by_id(&runner_id, &token_id, state.clock.as_ref())
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not revoke enrollment token",
                json!({}),
            )
        })?;
    if !changed {
        // `revoke_enrollment_token_by_id` also returns false for a token
        // that exists but was already consumed; disambiguate that genuine
        // conflict from a token id that never existed instead of reporting
        // `not_found` for both.
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM agent_enrollment_tokens WHERE runner_id = ? AND id = ?)",
        )
        .bind(&runner_id)
        .bind(&token_id)
        .fetch_one(state.repo.pool())
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not verify enrollment token",
                json!({}),
            )
        })?;
        return Err(if exists {
            error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "Enrollment token was already consumed",
                json!({}),
            )
        } else {
            error(
                StatusCode::NOT_FOUND,
                StableErrorCode::NotFound,
                "Enrollment token does not exist",
                json!({"resource": "enrollment_token"}),
            )
        });
    }
    Ok(Json(RevokeEnrollmentTokenResponse {
        protocol_version: 1,
        runner_id,
        token_id,
        state: "revoked".into(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/agent-profiles",
    tag = "execution-operator",
    request_body = CreateProfile,
    responses(
        (status = 200, description = "Agent profile created", body = CreateProfileResponse),
        (status = 409, description = "conflict (name already exists)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn create_profile(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreateProfile>,
) -> Result<Json<CreateProfileResponse>, (StatusCode, Json<Value>)> {
    let id = format!("ap_{}", Uuid::new_v4());
    let tool_policy = serde_json::to_string(&input.tool_policy).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "tool_policy cannot be serialized",
            json!({"field": "tool_policy"}),
        )
    })?;
    let limits = serde_json::to_string(&input.limits).map_err(|_| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "limits cannot be serialized",
            json!({"field": "limits"}),
        )
    })?;
    let result = state
        .repo
        .create_agent_profile(
            NewAgentProfile {
                id: &id,
                name: &input.name,
                instructions: &input.instructions,
                tool_policy: &tool_policy,
                limits: &limits,
            },
            state.clock.as_ref(),
        )
        .await;
    match result {
        Ok(_) => Ok(Json(CreateProfileResponse {
            protocol_version: 1,
            agent_profile_id: id,
            name: input.name,
        })),
        Err(err) if is_unique_violation(&err) => Err(error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Agent profile name already exists",
            json!({}),
        )),
        Err(_) => Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not create agent profile",
            json!({}),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/api/agent-profiles",
    tag = "execution-operator",
    responses(
        (status = 200, description = "Every agent profile, by name", body = AgentProfileListResponse),
    ),
)]
pub async fn list_profiles(
    State(state): State<OperatorExecutionState>,
) -> Result<Json<AgentProfileListResponse>, (StatusCode, Json<Value>)> {
    let rows = sqlx::query(
        "SELECT id,name,instructions,tool_policy,limits FROM agent_profiles ORDER BY name",
    )
    .fetch_all(state.repo.pool())
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not list agent profiles",
            json!({}),
        )
    })?;
    let data: Vec<AgentProfileSummary> = rows
        .into_iter()
        .map(
            |r| -> Result<AgentProfileSummary, (StatusCode, Json<Value>)> {
                let tool_policy = serde_json::from_str::<Value>(&r.get::<String, _>("tool_policy"))
                    .map_err(|_| {
                        error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            StableErrorCode::InternalError,
                            "Agent profile policy is corrupt",
                            json!({}),
                        )
                    })?;
                let limits =
                    serde_json::from_str::<Value>(&r.get::<String, _>("limits")).map_err(|_| {
                        error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            StableErrorCode::InternalError,
                            "Agent profile limits are corrupt",
                            json!({}),
                        )
                    })?;
                Ok(AgentProfileSummary {
                    agent_profile_id: r.get("id"),
                    name: r.get("name"),
                    instructions: r.get("instructions"),
                    tool_policy,
                    limits,
                })
            },
        )
        .collect::<Result<_, _>>()?;
    Ok(Json(AgentProfileListResponse {
        protocol_version: 1,
        data,
    }))
}

#[utoipa::path(
    post,
    path = "/api/model-profiles",
    tag = "execution-operator",
    request_body = CreateModelProfile,
    responses(
        (status = 200, description = "Model profile created", body = CreateModelProfileResponse),
        (status = 409, description = "conflict (name already exists)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn create_model_profile(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreateModelProfile>,
) -> Result<Json<CreateModelProfileResponse>, (StatusCode, Json<Value>)> {
    let id = format!("mp_{}", Uuid::new_v4());
    let now = state.clock.now().to_rfc3339();
    let result=sqlx::query("INSERT INTO model_profiles (id,name,model_provider,model_id,config_reference,created_at,updated_at) VALUES (?,?,?,?,?,?,?)").bind(&id).bind(&input.name).bind(&input.model_provider).bind(&input.model_id).bind(&input.config_reference).bind(&now).bind(&now).execute(state.repo.pool()).await;
    match result {
        Ok(_) => Ok(Json(CreateModelProfileResponse {
            protocol_version: 1,
            model_profile_id: id,
            name: input.name,
            model_provider: input.model_provider,
            model_id: input.model_id,
        })),
        Err(err) if is_unique_violation(&err) => Err(error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Model profile name already exists",
            json!({}),
        )),
        Err(_) => Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not create model profile",
            json!({}),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/api/model-profiles",
    tag = "execution-operator",
    responses(
        (status = 200, description = "Every model profile, by name", body = ModelProfileListResponse),
    ),
)]
pub async fn list_model_profiles(
    State(state): State<OperatorExecutionState>,
) -> Result<Json<ModelProfileListResponse>, (StatusCode, Json<Value>)> {
    let rows=sqlx::query("SELECT id,name,model_provider,model_id,config_reference,enabled FROM model_profiles ORDER BY name").fetch_all(state.repo.pool()).await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not list model profiles",
            json!({}),
        )
    })?;
    let data: Vec<ModelProfileSummary> = rows
        .into_iter()
        .map(|r| ModelProfileSummary {
            model_profile_id: r.get("id"),
            name: r.get("name"),
            model_provider: r.get("model_provider"),
            model_id: r.get("model_id"),
            config_reference: r.get("config_reference"),
            enabled: r.get::<i64, _>("enabled") != 0,
        })
        .collect();
    Ok(Json(ModelProfileListResponse {
        protocol_version: 1,
        data,
    }))
}
