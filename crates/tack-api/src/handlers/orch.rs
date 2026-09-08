//! Control-plane link API: register docket
//! control planes, link a Tack project to one, and read the Fleet view's
//! aggregate.
//!
//! **Off by default, toggleable from the UI.** Every route in this module is
//! gated behind the *effective* orchestration setting via
//! [`require_orch_enabled`], applied once as a layer on the orch sub-router
//! in `router.rs` rather than repeated per-handler. The effective value is
//! an `app_meta`-stored flag (editable at runtime via
//! `GET`/`PUT /api/settings/orchestration`, `handlers/settings.rs`'s
//! [`effective_orch_enabled`](crate::handlers::settings::effective_orch_enabled)),
//! falling back to `TACK_ORCH_ENABLE` as a deployment default when the UI has
//! never set one — mirroring the Cloud Backup precedent exactly. With
//! orchestration disabled, every
//! route here returns `409 Conflict` with a stable `error.code:
//! "orchestration_disabled"` and a message naming where to enable it — not a
//! 404. A 404 made "disabled" indistinguishable from "route doesn't exist",
//! which hid the feature from its own operator. This is not a security
//! boundary being removed (the Bearer-token gate and
//! the separate `TACK_ORCH_APPROVAL_TOKEN` check are unchanged).
//!
//! **Token discipline** mirrors the S3 backup secret precedent
//! (`handlers/settings.rs`'s `secret_key_set`): the docket Bearer token is
//! write-only over this API. [`ControlPlaneResponse`] never carries it — only
//! `token_set: bool`. A `PATCH` with the `token` field **absent** leaves the
//! stored token untouched; an explicit `"token": null` clears it; a string sets
//! or replaces it. See [`UpdateControlPlaneRequest`] and [`deserialize_some`].
//!
//! **A control-plane failure never fails a user request here.** Every handler
//! reads Tack's own database, populated out-of-band by the reconciler
//! (`tack-orch`) — a docket outage can only leave `health`/`last_seen_at` stale,
//! never turn into a 500 on a user's request.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use tracing::instrument;
use uuid::Uuid;

use tack_core::workflow::WorkflowConfig;
use tack_db::repo::orch::{
    ControlPlane, CreateControlPlane, OrchApproval, OrchEvent, OrchLink, OrchMetricLatest, OrchRun,
    OrchTask, PendingOrchApproval, UpdateControlPlane, UpsertOrchLink,
};
use tack_orch::adapters::legacy_bridge::{
    LEGACY_DOCKET_COMPATIBILITY_LABEL, LEGACY_DOCKET_COMPATIBILITY_POLICY,
};
use tack_orch::adapters::registry::{self, RegistryError};
use tack_orch::{ControlPlane as OrchControlPlane, OrchError};

use crate::dispatcher::{self, DispatchOutcome};
use crate::error::{ApiError, ApiResult};
use crate::openapi::ErrorEnvelope;
use crate::router::AppState;
use crate::sprint_dispatch::{self, ItemResult, PreviewDecision};

// ════════════════════════════════════════════════════════════════════════════
// Gate — the effective orchestration setting
// ════════════════════════════════════════════════════════════════════════════

/// Machine-readable `error.code` on the 409 every orch route returns while
/// orchestration is disabled. Exported so tests (and, if it's ever useful,
/// other handlers) can match on it without hardcoding the string twice.
pub const ORCHESTRATION_DISABLED_CODE: &str = "orchestration_disabled";

/// Gate every route in this module behind the effective orchestration
/// setting (`handlers::settings::effective_orch_enabled` — `app_meta`
/// override, falling back to `TACK_ORCH_ENABLE`). Applied once as a layer on
/// the orch sub-router (`router.rs`) so no individual handler needs to
/// repeat the check.
///
/// With orchestration disabled, returns `409 Conflict` (not `404`): the
/// route genuinely exists, and the operator — the only audience for a
/// self-hosted tool — needs a response they can act on, not one that reads
/// as "this version of Tack doesn't have this." `error.code` is the stable
/// [`ORCHESTRATION_DISABLED_CODE`] so a caller (the settings UI, in
/// particular) can distinguish "disabled" from any other conflict
/// programmatically, without parsing `message`.
pub async fn require_orch_enabled(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    if !crate::handlers::settings::effective_orch_enabled(&state).await {
        return ApiError::FeatureDisabled {
            message: "Orchestration is disabled. Enable it from Settings → Orchestration \
                      (PUT /api/settings/orchestration), or set TACK_ORCH_ENABLE for a \
                      deployment default."
                .to_string(),
            code: ORCHESTRATION_DISABLED_CODE,
        }
        .into_response();
    }
    next.run(req).await
}

/// Standard trick for a JSON tri-state field: with `#[serde(default)]` on the
/// field, a missing key never calls this function and the field stays `None`
/// ("absent" / "leave untouched"). When the key **is** present — including
/// `null` — this runs and always wraps the result in `Some(..)`, so `null`
/// becomes `Some(None)` ("clear it") and a value becomes `Some(Some(v))` ("set
/// it"). Plain `#[derive(Deserialize)]` on `Option<Option<T>>` cannot make this
/// distinction on its own; this is the whole trick.
fn deserialize_some<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

// ════════════════════════════════════════════════════════════════════════════
// Capabilities — wire mirror of tack_orch::Capabilities, surfaced
// on GET /api/control-planes/{id} and GET /api/fleet so the UI reads what a
// plane can do instead of checking `kind`.
// ════════════════════════════════════════════════════════════════════════════
//
// Local, non-generic mirrors rather than deriving `utoipa::ToSchema`
// directly on `tack_orch`'s types: that crate depends on neither `utoipa`
// nor `tack-api` (see its own module doc — the dependency graph points
// inward only), so every wire DTO in this file already follows the same
// wrap-and-convert shape (`ControlPlaneResponse` from `tack_db::repo::orch::
// ControlPlane`, `OrchLinkResponse` from `OrchLink`, and so on) — this is
// that same convention, not a new one.

/// Wire mirror of `tack_orch::Support`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Unsupported,
    Advisory,
    Supported,
}

impl From<tack_orch::Support> for SupportLevel {
    fn from(v: tack_orch::Support) -> Self {
        match v {
            tack_orch::Support::Unsupported => Self::Unsupported,
            tack_orch::Support::Advisory => Self::Advisory,
            tack_orch::Support::Supported => Self::Supported,
        }
    }
}

/// Wire mirror of `tack_orch::EventScope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventScopeLevel {
    None,
    Run,
    Project,
    Plane,
}

impl From<tack_orch::EventScope> for EventScopeLevel {
    fn from(v: tack_orch::EventScope) -> Self {
        match v {
            tack_orch::EventScope::None => Self::None,
            tack_orch::EventScope::Run => Self::Run,
            tack_orch::EventScope::Project => Self::Project,
            tack_orch::EventScope::Plane => Self::Plane,
        }
    }
}

/// Wire mirror of `tack_orch::DecisionSupport`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSupportLevel {
    None,
    Poll,
    Push,
}

impl From<tack_orch::DecisionSupport> for DecisionSupportLevel {
    fn from(v: tack_orch::DecisionSupport) -> Self {
        match v {
            tack_orch::DecisionSupport::None => Self::None,
            tack_orch::DecisionSupport::Poll => Self::Poll,
            tack_orch::DecisionSupport::Push => Self::Push,
        }
    }
}

/// Wire mirror of `tack_orch::UsageSupport`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UsageSupportLevel {
    NotMeasured,
    FromProvider,
    FromGateway,
}

impl From<tack_orch::UsageSupport> for UsageSupportLevel {
    fn from(v: tack_orch::UsageSupport) -> Self {
        match v {
            tack_orch::UsageSupport::NotMeasured => Self::NotMeasured,
            tack_orch::UsageSupport::FromProvider => Self::FromProvider,
            tack_orch::UsageSupport::FromGateway => Self::FromGateway,
        }
    }
}

/// Wire mirror of `tack_orch::ModelSelection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelSelectionLevel {
    Unsupported,
    Advisory,
    Honoured,
}

impl From<tack_orch::ModelSelection> for ModelSelectionLevel {
    fn from(v: tack_orch::ModelSelection) -> Self {
        match v {
            tack_orch::ModelSelection::Unsupported => Self::Unsupported,
            tack_orch::ModelSelection::Advisory => Self::Advisory,
            tack_orch::ModelSelection::Honoured => Self::Honoured,
        }
    }
}

/// `pause`/`resume`'s wire shape — level plus why, not a bare enum. See
/// `tack_orch::Capabilities`'s own doc comment: the reason is
/// adapter-authored data, never a string this API layer invents.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct SupportCapability {
    pub level: SupportLevel,
    pub reason: String,
}

impl From<tack_orch::Rated<tack_orch::Support>> for SupportCapability {
    fn from(r: tack_orch::Rated<tack_orch::Support>) -> Self {
        Self {
            level: r.level.into(),
            reason: r.reason.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct EventScopeCapability {
    pub level: EventScopeLevel,
    pub reason: String,
}

impl From<tack_orch::Rated<tack_orch::EventScope>> for EventScopeCapability {
    fn from(r: tack_orch::Rated<tack_orch::EventScope>) -> Self {
        Self {
            level: r.level.into(),
            reason: r.reason.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct DecisionsCapability {
    pub level: DecisionSupportLevel,
    pub reason: String,
}

impl From<tack_orch::Rated<tack_orch::DecisionSupport>> for DecisionsCapability {
    fn from(r: tack_orch::Rated<tack_orch::DecisionSupport>) -> Self {
        Self {
            level: r.level.into(),
            reason: r.reason.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct UsageCapability {
    pub level: UsageSupportLevel,
    pub reason: String,
}

impl From<tack_orch::Rated<tack_orch::UsageSupport>> for UsageCapability {
    fn from(r: tack_orch::Rated<tack_orch::UsageSupport>) -> Self {
        Self {
            level: r.level.into(),
            reason: r.reason.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ModelSelectionCapability {
    pub level: ModelSelectionLevel,
    pub reason: String,
}

impl From<tack_orch::Rated<tack_orch::ModelSelection>> for ModelSelectionCapability {
    fn from(r: tack_orch::Rated<tack_orch::ModelSelection>) -> Self {
        Self {
            level: r.level.into(),
            reason: r.reason.to_string(),
        }
    }
}

/// Wire mirror of `tack_orch::Capabilities` — what a control plane can
/// actually do, so the UI can disable a control and explain why instead of
/// checking `kind`.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct CapabilitiesResponse {
    pub dispatch: bool,
    pub cancel: bool,
    pub pause: SupportCapability,
    pub resume: SupportCapability,
    pub event_scope: EventScopeCapability,
    pub artifacts: bool,
    pub decisions: DecisionsCapability,
    pub usage: UsageCapability,
    pub model_selection: ModelSelectionCapability,
    pub runtimes: bool,
    pub plane_metrics: bool,
    pub provisioning: bool,
}

impl From<tack_orch::Capabilities> for CapabilitiesResponse {
    fn from(c: tack_orch::Capabilities) -> Self {
        Self {
            dispatch: c.dispatch,
            cancel: c.cancel,
            pause: c.pause.into(),
            resume: c.resume.into(),
            event_scope: c.event_scope.into(),
            artifacts: c.artifacts,
            decisions: c.decisions.into(),
            usage: c.usage.into(),
            model_selection: c.model_selection.into(),
            runtimes: c.runtimes,
            plane_metrics: c.plane_metrics,
            provisioning: c.provisioning,
        }
    }
}

/// Best-effort capabilities lookup for a `control_planes` row — `None` when
/// this build of Tack has no adapter for `kind` (the "unconfigured" case —
/// see `orch_store::RepoControlPlaneStore::mark_unconfigured`'s doc
/// comment). `ControlPlane::capabilities` is synchronous and does no I/O
/// (see the trait's own doc comment), so this never makes a network call
/// and never needs the plane's real credentials — a throwaway adapter with
/// no token is enough to read a value derived purely from the adapter's own
/// static configuration. Same placeholder `config`/`secrets` caveat as
/// `dispatcher::build_control_plane`: `tack_db::repo::orch::ControlPlane`
/// doesn't yet surface those columns, and `"docket"` doesn't read them.
fn capabilities_for(kind: &str, base_url: &str) -> Option<CapabilitiesResponse> {
    registry::build(kind, base_url, None, &serde_json::json!({}), None)
        .ok()
        .map(|adapter| adapter.capabilities().into())
}

// ════════════════════════════════════════════════════════════════════════════
// control_planes
// ════════════════════════════════════════════════════════════════════════════

/// Client-safe view of a `control_planes` row. Deliberately has **no** `token`
/// field — see the module doc.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ControlPlaneResponse {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub api_version: Option<String>,
    /// `"unknown"` (pre-first-poll default) | `"healthy"` | `"degraded"` |
    /// `"unreachable"` (the reconciler's health state machine,
    /// `tack-orch::reconciler`, persisted verbatim) | `"unconfigured"`
    /// (this build of Tack could not even build a live adapter for
    /// `kind`, so the reconciler's state machine never ran against this
    /// plane at all; see `orch_store::RepoControlPlaneStore::
    /// mark_unconfigured`'s doc comment).
    pub health: String,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub consecutive_failures: i64,
    /// True when a docket Bearer token is currently stored for this plane.
    /// The token itself is write-only over this API.
    pub token_set: bool,
    /// What this plane can actually do — `None` only in the same
    /// `"unconfigured"` case `health` names: this build of Tack has no
    /// adapter for `kind`. See [`capabilities_for`].
    pub capabilities: Option<CapabilitiesResponse>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<ControlPlane> for ControlPlaneResponse {
    fn from(c: ControlPlane) -> Self {
        let capabilities = capabilities_for(&c.kind, &c.base_url);
        Self {
            id: c.id,
            name: c.name,
            kind: c.kind,
            base_url: c.base_url,
            api_version: c.api_version,
            health: c.health,
            last_seen_at: c.last_seen_at,
            consecutive_failures: c.consecutive_failures,
            token_set: c.token_set,
            capabilities,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

/// `POST /api/control-planes` body.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateControlPlaneRequest {
    pub name: String,
    /// Defaults to `"docket"` when omitted — the only kind implemented today.
    #[serde(default)]
    pub kind: Option<String>,
    pub base_url: String,
    /// docket Bearer token. Write-only: never echoed back in any response.
    #[serde(default)]
    pub token: Option<String>,
}

/// `PATCH /api/control-planes/{id}` body. `token` is tri-state — see the module
/// doc and [`deserialize_some`]. `name`/`base_url` follow the ordinary
/// "absent means untouched" convention every other partial update in this API
/// uses.
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct UpdateControlPlaneRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    /// Absent = leave the stored token untouched. `null` = clear it. A string
    /// = set/replace it.
    #[serde(default, deserialize_with = "deserialize_some")]
    #[schema(value_type = Option<String>, nullable)]
    pub token: Option<Option<String>>,
}

const CONTROL_PLANE_NOT_FOUND: &str = "Control plane not found";

async fn get_control_plane_or_404(state: &AppState, id: Uuid) -> ApiResult<ControlPlane> {
    match state.repo.get_control_plane(id).await {
        Ok(cp) => Ok(cp),
        Err(sqlx::Error::RowNotFound) => Err(ApiError::NotFound(format!(
            "{CONTROL_PLANE_NOT_FOUND}: {id}"
        ))),
        Err(e) => Err(e.into()),
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Optimistic concurrency — ETag / If-Match for
// PATCH /api/control-planes/{id} and PUT /api/projects/{id}/orch-link,
// against the `version` columns migrations 035/036 added.
// ════════════════════════════════════════════════════════════════════════════
//
// `crates/tack-db/src/repo/orch.rs` doesn't yet surface `version` through
// `ControlPlane`/`OrchLink`'s typed read structs or `update_control_plane`/
// `upsert_orch_link`. Everything below reads/writes the raw
// column directly against `state.pool()` — the same escape hatch
// `handlers::backup` already uses for its own raw `PRAGMA`/`VACUUM`
// statements (see `get_backup`), not a new pattern in this codebase, just a
// new user of it. Whoever next gives `control_planes`/`orch_links` a typed
// `version` field in the repo layer should fold this block's raw queries
// into it rather than leaving two competing sources of truth for the same
// column.
//
// **Design: a single-statement compare-and-swap, not read-then-write.** The
// `If-Match`-present path runs `UPDATE ... SET version = version + 1 WHERE
// id = ? AND version = ?` — SQLite serializes writers, so of two concurrent
// requests racing with the same expected version, exactly one `UPDATE`
// matches a row and the other affects zero rows. That is what makes
// `concurrent_patch_with_the_same_if_match_yields_one_200_and_one_412`
// true rather than a race
// between two independent `SELECT`s. The follow-up content write (via the
// existing `state.repo.update_control_plane`/`upsert_orch_link`) is a
// separate statement — this codebase's multi-statement writes are not
// wrapped in an explicit transaction anywhere else either (see e.g.
// `update_control_plane`'s own three sequential `UPDATE`s), so this is
// consistent with, not a regression from, the existing risk profile.

/// `ETag` value for a row currently at `version` — always a quoted decimal,
/// never `W/"..."` (there is no meaningfully "equivalent but different"
/// representation of a database row here to make a weak tag worth the extra
/// parsing branch on the way back in).
fn etag_for(version: i64) -> String {
    format!("\"{version}\"")
}

/// Parse an `If-Match` header value back into the integer version a caller
/// read off a previous [`etag_for`]. Anything that isn't exactly that shape
/// (a stray `W/` prefix, `*`, garbage) is `None`, which every call site
/// below treats as "does not match" — safe by construction, since a caller
/// sending something that was never one of our own ETags can never
/// accidentally satisfy a precondition it shouldn't.
fn parse_if_match(raw: &str) -> Option<i64> {
    raw.trim().trim_matches('"').parse::<i64>().ok()
}

/// `412 Precondition Failed`, matching [`ApiError`]'s own `{"error":
/// {status, message}}` envelope exactly (see `error.rs`'s `IntoResponse`
/// impl) — this module doesn't add a variant to `ApiError` itself for this
/// so both `If-Match`-gated handlers below build this response by
/// hand instead.
fn precondition_failed(message: impl Into<String>) -> Response {
    let body = serde_json::json!({
        "error": {
            "status": StatusCode::PRECONDITION_FAILED.as_u16(),
            "message": message.into(),
        }
    });
    (StatusCode::PRECONDITION_FAILED, Json(body)).into_response()
}

async fn control_plane_version(
    pool: &sqlx::SqlitePool,
    id: Uuid,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT version FROM control_planes WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
}

/// Atomic compare-and-swap on `control_planes.version` — see this section's
/// own doc comment for why this is one statement, not a read then a write.
/// Returns `true` iff a row matched (existed **and** was at
/// `expected_version`); the caller can't yet tell those two failure modes
/// apart from this alone, and [`update_control_plane`] does one more read
/// only in the `false` branch, to preserve its existing 404-vs-412 split.
async fn bump_control_plane_version_if_match(
    pool: &sqlx::SqlitePool,
    id: Uuid,
    expected_version: i64,
) -> Result<bool, sqlx::Error> {
    let result =
        sqlx::query("UPDATE control_planes SET version = version + 1 WHERE id = ? AND version = ?")
            .bind(id.to_string())
            .bind(expected_version)
            .execute(pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

/// Unconditional bump — used when no `If-Match` was supplied, so the
/// version still moves forward for a *later* conditional request to detect
/// this write against (see `update_control_plane`'s own doc note: an absent
/// `If-Match` skips the *check*, never the bump).
async fn bump_control_plane_version(pool: &sqlx::SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE control_planes SET version = version + 1 WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

async fn orch_link_version(
    pool: &sqlx::SqlitePool,
    project_id: Uuid,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT version FROM orch_links WHERE project_id = ?")
        .bind(project_id.to_string())
        .fetch_optional(pool)
        .await
}

/// `orch_links`' twin of [`bump_control_plane_version_if_match`]. Unlike
/// the control-planes case, [`put_orch_link`] treats "no row yet" and "row
/// exists but version doesn't match" identically (both `412`) rather than
/// disambiguating with a follow-up read — `PUT .../orch-link` is a
/// create-or-replace endpoint, so a caller that sends `If-Match` for a link
/// it never actually read an `ETag` for (there is nothing to match — the
/// project has no link yet) is asserting a precondition that cannot be
/// satisfied either way.
async fn bump_orch_link_version_if_match(
    pool: &sqlx::SqlitePool,
    project_id: Uuid,
    expected_version: i64,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE orch_links SET version = version + 1 WHERE project_id = ? AND version = ?",
    )
    .bind(project_id.to_string())
    .bind(expected_version)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

async fn bump_orch_link_version(
    pool: &sqlx::SqlitePool,
    project_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE orch_links SET version = version + 1 WHERE project_id = ?")
        .bind(project_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// `POST /api/control-planes` — register a control plane.
#[utoipa::path(
    post,
    path = "/api/control-planes",
    tag = "orchestration",
    request_body = CreateControlPlaneRequest,
    responses(
        (status = 200, description = "Control plane registered (token never returned)", body = ControlPlaneResponse),
        (status = 400, description = "Validation error", body = ErrorEnvelope),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state, input))]
pub async fn create_control_plane(
    State(state): State<AppState>,
    Json(input): Json<CreateControlPlaneRequest>,
) -> ApiResult<Json<ControlPlaneResponse>> {
    if input.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".into()));
    }
    if input.base_url.trim().is_empty() {
        return Err(ApiError::BadRequest("base_url must not be empty".into()));
    }

    let cp = state
        .repo
        .create_control_plane(CreateControlPlane {
            name: input.name,
            kind: input.kind,
            base_url: input.base_url,
            token: input.token,
        })
        .await?;

    Ok(Json(cp.into()))
}

/// `GET /api/control-planes` — every registered control plane (tokens never
/// included).
#[utoipa::path(
    get,
    path = "/api/control-planes",
    tag = "orchestration",
    responses(
        (status = 200, description = "All registered control planes", body = Vec<ControlPlaneResponse>),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn list_control_planes(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<ControlPlaneResponse>>> {
    let planes = state.repo.list_control_planes().await?;
    Ok(Json(planes.into_iter().map(Into::into).collect()))
}

/// `GET /api/control-planes/{id}`.
#[utoipa::path(
    get,
    path = "/api/control-planes/{id}",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Control plane ID")),
    responses(
        (status = 200, description = "The control plane", body = ControlPlaneResponse),
        (status = 404, description = "Not found, or orchestration disabled", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn get_control_plane(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ControlPlaneResponse>> {
    let cp = get_control_plane_or_404(&state, id).await?;
    Ok(Json(cp.into()))
}

/// `PATCH /api/control-planes/{id}`. Supports optimistic concurrency: an
/// `If-Match: "<version>"` header, taken from a
/// previous response's `ETag`, must match the row's current version or the
/// request is rejected with `412` and nothing is written. **Omitting
/// `If-Match` is not an error** — the precondition
/// check is skipped entirely; the version still moves forward on every
/// successful write either way, so a later conditional request from a
/// different client can always detect this one.
#[utoipa::path(
    patch,
    path = "/api/control-planes/{id}",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Control plane ID")),
    request_body = UpdateControlPlaneRequest,
    responses(
        (status = 200, description = "Updated control plane (token never returned); carries an ETag header naming the new version", body = ControlPlaneResponse),
        (status = 404, description = "Not found, or orchestration disabled", body = ErrorEnvelope),
        (status = 412, description = "If-Match did not match the control plane's current version — nothing was written", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state, input, headers))]
pub async fn update_control_plane(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<UpdateControlPlaneRequest>,
) -> ApiResult<Response> {
    let if_match = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok());

    if let Some(raw) = if_match {
        let matched = match parse_if_match(raw) {
            Some(expected) => {
                bump_control_plane_version_if_match(state.pool(), id, expected).await?
            }
            None => false,
        };
        if !matched {
            return Ok(match control_plane_version(state.pool(), id).await? {
                Some(_) => precondition_failed(
                    "If-Match did not match the control plane's current version",
                ),
                None => {
                    ApiError::NotFound(format!("{CONTROL_PLANE_NOT_FOUND}: {id}")).into_response()
                }
            });
        }
    }

    let cp = match state
        .repo
        .update_control_plane(
            id,
            UpdateControlPlane {
                name: input.name,
                base_url: input.base_url,
                token: input.token,
            },
        )
        .await
    {
        Ok(cp) => cp,
        Err(sqlx::Error::RowNotFound) => {
            return Err(ApiError::NotFound(format!(
                "{CONTROL_PLANE_NOT_FOUND}: {id}"
            )));
        }
        Err(e) => return Err(e.into()),
    };

    if if_match.is_none() {
        // The precondition branch above already bumped the version when it
        // ran — this only covers the unconditional path, so every
        // successful write moves the version forward exactly once.
        bump_control_plane_version(state.pool(), id).await?;
    }

    let version = control_plane_version(state.pool(), id).await?.unwrap_or(1);

    Ok((
        [(header::ETAG, etag_for(version))],
        Json(ControlPlaneResponse::from(cp)),
    )
        .into_response())
}

/// `DELETE /api/control-planes/{id}`.
#[utoipa::path(
    delete,
    path = "/api/control-planes/{id}",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Control plane ID")),
    responses(
        (status = 200, description = "Deleted", body = serde_json::Value),
        (status = 404, description = "Not found, or orchestration disabled", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn delete_control_plane(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let deleted = state.repo.delete_control_plane(id).await?;
    if !deleted {
        return Err(ApiError::NotFound(format!(
            "{CONTROL_PLANE_NOT_FOUND}: {id}"
        )));
    }
    Ok(Json(serde_json::json!({"deleted": true})))
}

// ════════════════════════════════════════════════════════════════════════════
// orch_links (project_id is the PK — one link per project)
// ════════════════════════════════════════════════════════════════════════════

/// `status_map`. All keys optional except `dispatch_from`, which may be an
/// empty list before a dispatch policy is configured — dispatch needs it
/// non-empty, but registering a link ahead of that is a normal, valid state.
/// Every named status is validated against
/// the project's `WorkflowConfig` at save time — see [`validate_status_map`].
/// An absent key means "do not touch the item's status on that transition."
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StatusMap {
    #[serde(default)]
    pub dispatch_from: Vec<String>,
    #[serde(default)]
    pub on_running: Option<String>,
    #[serde(default)]
    pub on_waiting_approval: Option<String>,
    #[serde(default)]
    pub on_succeeded: Option<String>,
    #[serde(default)]
    pub on_failed: Option<String>,
    #[serde(default)]
    pub on_cancelled: Option<String>,
}

/// Client-facing view of a project's control-plane link.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrchLinkResponse {
    pub project_id: Uuid,
    pub control_plane_id: Uuid,
    pub remote_project: String,
    pub pipeline_file: Option<String>,
    pub blueprint: Option<String>,
    pub auto_dispatch: bool,
    /// User-set cap, not a derived spend figure — deliberately unsuffixed
    /// Matches `orch_links.budget_usd`.
    pub budget_usd: Option<f64>,
    pub status_map: StatusMap,
    /// `tack_orch::adapters::legacy_bridge::LEGACY_DOCKET_COMPATIBILITY_LABEL`,
    /// verbatim. Every control plane this API can register or link to is a
    /// legacy Docket bridge (the only adapter `registry::build` accepts), so
    /// this is unconditional rather than derived per-plane.
    pub compatibility_label: String,
    /// `tack_orch::adapters::legacy_bridge::LEGACY_DOCKET_COMPATIBILITY_POLICY`,
    /// verbatim, so a caller can render or quote the justification without a
    /// second round trip to the source.
    pub compatibility_policy: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<OrchLink> for OrchLinkResponse {
    fn from(l: OrchLink) -> Self {
        Self {
            project_id: l.project_id,
            control_plane_id: l.control_plane_id,
            remote_project: l.remote_project,
            pipeline_file: l.pipeline_file,
            blueprint: l.blueprint,
            auto_dispatch: l.auto_dispatch,
            budget_usd: l.budget_usd,
            status_map: serde_json::from_value(l.status_map).unwrap_or_default(),
            compatibility_label: LEGACY_DOCKET_COMPATIBILITY_LABEL.to_string(),
            compatibility_policy: LEGACY_DOCKET_COMPATIBILITY_POLICY.to_string(),
            created_at: l.created_at,
            updated_at: l.updated_at,
        }
    }
}

/// `GET /api/projects/{id}/orch-link` response. `linked: false` (with
/// `link: null`) is the ordinary state for a project that has never registered
/// a control plane — not an error, matching the `settings.rs` precedent for
/// optional per-scope config (no 404 for "not configured yet").
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrchLinkView {
    pub linked: bool,
    pub link: Option<OrchLinkResponse>,
}

/// `PUT /api/projects/{id}/orch-link` body.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpsertOrchLinkRequest {
    pub control_plane_id: Uuid,
    pub remote_project: String,
    #[serde(default)]
    pub pipeline_file: Option<String>,
    #[serde(default)]
    pub blueprint: Option<String>,
    #[serde(default)]
    pub auto_dispatch: bool,
    #[serde(default)]
    pub budget_usd: Option<f64>,
    #[serde(default)]
    pub status_map: StatusMap,
}

/// Every named status in `status_map` must exist in the project's workflow —
/// time." Validation, not a raw-SQL status write: this never bypasses the
/// workflow engine, it only checks the *names* used to configure future
/// auto-transitions are real. The transition itself is applied later by the
/// dispatcher/reconciler; this is the save-time guard that keeps a typo from
/// silently becoming a no-op transition.
///
/// `pub(crate)`, not private: `handlers::templates` reuses this verbatim for
/// a template's `orchestration.status_map` rather than writing a second
/// validator against the template's own workflow — see that module's
/// `validate_template_orchestration`.
pub(crate) fn validate_status_map(
    status_map: &StatusMap,
    workflow: &WorkflowConfig,
) -> ApiResult<()> {
    let exists = |name: &str| workflow.statuses.iter().any(|s| s.name == name);

    for name in &status_map.dispatch_from {
        if !exists(name) {
            return Err(ApiError::BadRequest(format!(
                "status_map.dispatch_from: unknown status {name:?} for this project's workflow"
            )));
        }
    }

    let named = [
        ("on_running", &status_map.on_running),
        ("on_waiting_approval", &status_map.on_waiting_approval),
        ("on_succeeded", &status_map.on_succeeded),
        ("on_failed", &status_map.on_failed),
        ("on_cancelled", &status_map.on_cancelled),
    ];
    for (key, value) in named {
        if let Some(name) = value
            && !exists(name)
        {
            return Err(ApiError::BadRequest(format!(
                "status_map.{key}: unknown status {name:?} for this project's workflow"
            )));
        }
    }

    Ok(())
}

/// `GET /api/projects/{id}/orch-link`.
#[utoipa::path(
    get,
    path = "/api/projects/{id}/orch-link",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "The project's control-plane link, if any", body = OrchLinkView),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn get_orch_link(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<OrchLinkView>> {
    let link = state.repo.get_orch_link(project_id).await?;
    Ok(Json(match link {
        Some(l) => OrchLinkView {
            linked: true,
            link: Some(l.into()),
        },
        None => OrchLinkView {
            linked: false,
            link: None,
        },
    }))
}

/// `PUT /api/projects/{id}/orch-link` — create or replace the project's
/// link. Supports optimistic concurrency the same
/// way `PATCH /api/control-planes/{id}` does — see that handler's doc
/// comment for the `If-Match`/`ETag` contract. One difference: this is a
/// create-or-replace endpoint, so a nonexistent link and a version mismatch
/// both surface as `412` here (see `bump_orch_link_version_if_match`'s doc
/// comment for why that collapse is correct, not a shortcut).
#[utoipa::path(
    put,
    path = "/api/projects/{id}/orch-link",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = UpsertOrchLinkRequest,
    responses(
        (status = 200, description = "Saved link; carries an ETag header naming the new version", body = OrchLinkResponse),
        (status = 400, description = "Validation error (e.g. an unknown status name in status_map)", body = ErrorEnvelope),
        (status = 404, description = "Project or control plane not found, or orchestration disabled", body = ErrorEnvelope),
        (status = 412, description = "If-Match did not match (or no link exists yet to match) — nothing was written", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state, input, headers))]
pub async fn put_orch_link(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    headers: HeaderMap,
    Json(input): Json<UpsertOrchLinkRequest>,
) -> ApiResult<Response> {
    let project = state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;

    // The referenced control plane must exist.
    get_control_plane_or_404(&state, input.control_plane_id).await?;

    if input.remote_project.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "remote_project must not be empty".into(),
        ));
    }

    validate_status_map(&input.status_map, &project.workflow)?;

    let if_match = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok());

    if let Some(raw) = if_match {
        let matched = match parse_if_match(raw) {
            Some(expected) => {
                bump_orch_link_version_if_match(state.pool(), project_id, expected).await?
            }
            None => false,
        };
        if !matched {
            return Ok(precondition_failed(
                "If-Match did not match the project's current orch-link version",
            ));
        }
    }

    let status_map_json =
        serde_json::to_value(&input.status_map).unwrap_or_else(|_| serde_json::json!({}));

    let link = state
        .repo
        .upsert_orch_link(
            project_id,
            UpsertOrchLink {
                control_plane_id: input.control_plane_id,
                remote_project: input.remote_project,
                pipeline_file: input.pipeline_file,
                blueprint: input.blueprint,
                auto_dispatch: input.auto_dispatch,
                budget_usd: input.budget_usd,
                status_map: status_map_json,
            },
        )
        .await?;

    if if_match.is_none() {
        bump_orch_link_version(state.pool(), project_id).await?;
    }

    let version = orch_link_version(state.pool(), project_id)
        .await?
        .unwrap_or(1);

    Ok((
        [(header::ETAG, etag_for(version))],
        Json(OrchLinkResponse::from(link)),
    )
        .into_response())
}

// ════════════════════════════════════════════════════════════════════════════
// GET /api/fleet — the Fleet view's aggregate
// ════════════════════════════════════════════════════════════════════════════
//
// This shape is reconciled against `frontend/src/features/fleet/api.ts` field
// for field, so the frontend swap is mechanical. Where there is genuinely no
// data source yet (`gateway`, `roster`, `pricing_snapshot_at`), the field
// is still present on the wire with an honest placeholder, so the existing
// TypeScript needs no shape changes later — only real values flowing in.

/// One roster member — projected from a future live `FleetAgent` snapshot.
/// **Always an empty list today**: no agent-roster table exists (migrations
/// 019–024 mirror control planes/links/tasks/runs/events/approvals only).
/// Whatever adds roster mirroring populates this; until then the field stays
/// on the wire as `[]` rather than being removed,
/// so `frontend/src/features/fleet/api.ts`'s `FleetRow.roster` never needs a
/// shape change.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FleetRosterMember {
    pub id: String,
    pub name: String,
    pub role: String,
    pub model: String,
}

/// One row per Tack project that has an `orch_links` row, joining: the link,
/// its control plane's reconciler-observed health, and mirrored cost/token/
/// approval data summed from `orch_tasks`/`orch_approvals`.
///
/// **Staleness must be representable.** `cost_usd_estimated` is `None`
/// whenever the plane is `unreachable` — never coerced to zero — so the UI can
/// grey the row and say "last seen Nm ago" instead of rendering a confident
/// zero. `Some(0.0)` means the
/// plane is reachable and genuinely has no mirrored cost yet. `tokens_in`/
/// `tokens_out` are always a plain (never-null) sum — the row component
/// gates on `health`/`isStale()`, not per-field nullability, to decide
/// whether a number is trustworthy to render.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FleetEntry {
    pub project_id: Uuid,
    pub project_name: String,
    pub control_plane_id: Uuid,
    pub control_plane_name: String,
    pub control_plane_kind: String,
    pub remote_project: String,
    /// `"unknown"` | `"healthy"` | `"degraded"` | `"unreachable"` |
    /// `"unconfigured"` — see [`ControlPlaneResponse::health`]'s doc
    /// comment for what each means.
    pub health: String,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub consecutive_failures: i64,
    pub api_version: Option<String>,
    /// What this row's control plane can actually do. `None` only in the
    /// `"unconfigured"` health case — see [`capabilities_for`].
    pub capabilities: Option<CapabilitiesResponse>,
    /// `"active"` | `"inactive"` | `"unknown"`. **Always `"unknown"` today** —
    /// `control_planes` has no persisted gateway column (see migration 019) and
    /// the reconciler only polls `/health` + `/status.json` for the health state
    /// machine, not a stored gateway snapshot. Mirroring `FleetStatus.gateway`
    /// would populate this for real.
    pub gateway: String,
    /// Always `[]` — see [`FleetRosterMember`].
    pub roster: Vec<FleetRosterMember>,
    /// Most recent `orch_tasks.dispatched_at` for this project's items, or
    /// `None` if nothing has ever been dispatched. Real data (not a
    /// placeholder) — computed from the same join as the cost/token sums.
    pub last_activity_at: Option<DateTime<Utc>>,
    pub auto_dispatch: bool,
    pub blueprint: Option<String>,
    /// User-set cap, not a derived figure — deliberately unsuffixed.
    pub budget_usd: Option<f64>,
    /// Summed from `orch_tasks.tokens_in`/`tokens_out` for this project's
    /// items. Real data, not a placeholder — an honest current total.
    pub tokens_in: i64,
    pub tokens_out: i64,
    /// Estimated cumulative spend, summed from `orch_tasks` for this
    /// project's items. `None` = plane unreachable, figure is stale/unknown.
    /// `Some(0.0)` = plane reachable, nothing dispatched yet.
    pub cost_usd_estimated: Option<f64>,
    /// Pricing-table snapshot date backing `cost_usd_estimated`. **Always
    /// `None` today** — no pricing-snapshot mechanism exists. Whatever adds one
    /// should populate this alongside real cost figures.
    pub pricing_snapshot_at: Option<String>,
    /// Pending docket approvals correlated to an item in this project (via
    /// `orch_approvals.item_id`). Approvals with no item correlation surface
    /// in the fleet-wide approvals inbox instead of here.
    pub pending_approval_count: i64,
}

/// `GET /api/fleet` response envelope — matches
/// `frontend/src/features/fleet/api.ts`'s `FleetResponse` exactly (`{ rows:
/// [...] }`, not a bare array).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FleetListResponse {
    pub rows: Vec<FleetEntry>,
}

/// `GET /api/fleet`.
#[utoipa::path(
    get,
    path = "/api/fleet",
    tag = "orchestration",
    responses(
        (status = 200, description = "One row per project linked to a control plane", body = FleetListResponse),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn get_fleet(State(state): State<AppState>) -> ApiResult<Json<FleetListResponse>> {
    let planes = state.repo.list_control_planes().await?;
    let mut rows = Vec::new();

    for plane in planes {
        let links = state.repo.list_orch_links_for_plane(plane.id).await?;
        for link in links {
            let project_name = state
                .repo
                .get_project(link.project_id)
                .await?
                .map(|p| p.name)
                .unwrap_or_else(|| "(deleted project)".to_string());

            let usage = project_task_usage(state.pool(), link.project_id).await?;

            let cost_usd_estimated = if plane.health == "unreachable" {
                None
            } else {
                Some(usage.cost_usd_estimated)
            };

            let pending_approval_count =
                count_pending_approvals(state.pool(), plane.id, link.project_id).await?;

            rows.push(FleetEntry {
                project_id: link.project_id,
                project_name,
                control_plane_id: plane.id,
                control_plane_name: plane.name.clone(),
                control_plane_kind: plane.kind.clone(),
                remote_project: link.remote_project,
                health: plane.health.clone(),
                last_seen_at: plane.last_seen_at,
                consecutive_failures: plane.consecutive_failures,
                api_version: plane.api_version.clone(),
                capabilities: capabilities_for(&plane.kind, &plane.base_url),
                gateway: "unknown".to_string(),
                roster: Vec::new(),
                last_activity_at: usage.last_activity_at,
                auto_dispatch: link.auto_dispatch,
                blueprint: link.blueprint,
                budget_usd: link.budget_usd,
                tokens_in: usage.tokens_in,
                tokens_out: usage.tokens_out,
                cost_usd_estimated,
                pricing_snapshot_at: None,
                pending_approval_count,
            });
        }
    }

    Ok(Json(FleetListResponse { rows }))
}

/// Aggregate of `orch_tasks` for one project's items — one query backs
/// `tokens_in`/`tokens_out`/`cost_usd_estimated`/`last_activity_at` together
/// rather than four round trips.
struct ProjectTaskUsage {
    tokens_in: i64,
    tokens_out: i64,
    cost_usd_estimated: f64,
    last_activity_at: Option<DateTime<Utc>>,
}

async fn project_task_usage(
    pool: &sqlx::SqlitePool,
    project_id: Uuid,
) -> Result<ProjectTaskUsage, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        tokens_in: Option<i64>,
        tokens_out: Option<i64>,
        cost_usd_estimated: Option<f64>,
        last_activity_at: Option<String>,
    }

    let row: Row = sqlx::query_as(
        "SELECT SUM(t.tokens_in) AS tokens_in, SUM(t.tokens_out) AS tokens_out, \
                SUM(t.cost_usd_estimated) AS cost_usd_estimated, \
                MAX(t.dispatched_at) AS last_activity_at \
         FROM orch_tasks t \
         JOIN items i ON i.id = t.item_id \
         WHERE i.project_id = ?",
    )
    .bind(project_id.to_string())
    .fetch_one(pool)
    .await?;

    Ok(ProjectTaskUsage {
        tokens_in: row.tokens_in.unwrap_or(0),
        tokens_out: row.tokens_out.unwrap_or(0),
        cost_usd_estimated: row.cost_usd_estimated.unwrap_or(0.0),
        last_activity_at: row.last_activity_at.as_deref().map(|s| {
            DateTime::parse_from_rfc3339(s)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now())
        }),
    })
}

/// Pending docket approvals correlated to an item in `project_id`, for one
/// control plane.
async fn count_pending_approvals(
    pool: &sqlx::SqlitePool,
    control_plane_id: Uuid,
    project_id: Uuid,
) -> Result<i64, sqlx::Error> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM orch_approvals a \
         JOIN items i ON i.id = a.item_id \
         WHERE a.control_plane_id = ? AND a.state = 'pending' AND i.project_id = ?",
    )
    .bind(control_plane_id.to_string())
    .bind(project_id.to_string())
    .fetch_one(pool)
    .await?;
    Ok(count)
}

// ════════════════════════════════════════════════════════════════════════════
// GET /api/projects/{id}/orch-budget — budget cap vs. mirrored spend
// ════════════════════════════════════════════════════════════════════════════
//
// Reuses `project_task_usage` and the exact unreachable-vs-zero staleness rule
// `GET /api/fleet` already established — this endpoint
// exists because a project detail panel wants this data keyed by
// `project_id` alone, without pulling every other linked project's row the way
// `/fleet` does.
//
// **What this endpoint deliberately does NOT report: whether the pod is
// budget-paused.** docket auto-pauses a pod's Lead once its own budget cap is
// reached (`core/dispatch.py::_pause_lead_for_budget`, confirmed by reading
// the source directly) and refuses every further task claim until an operator
// runs `docket profile <lead-id> --resume` — there is no HTTP route to clear
// this (`serve.py`'s full `do_GET`/`do_POST` route table has no `/profile`,
// no pause/resume endpoint at all). Reading that state isn't reachable
// either: `GET /status.json`'s per-agent record (`serve.py::_agent_record`)
// and `GET /metrics` (`render_metrics`) both omit the `paused`/`pausedReason`
// fields entirely — verified by reading both functions' exact output line by
// line, not inferred — even though `core/models.py::AgentMeta` tracks them
// internally. The one proxy that exists, a `paused_refused` trace event
// (`core/dispatch.py::_claim_next_task`), already flows into Tack via
// trace-event ingestion (`orch_events`) — but not attributably: that
// event's `session_id` is the generic `"agent:<project>:dispatch"` form,
// which the reconciler's own per-item correlation
// (`reconciler::session_id_task_id`) never matches (by design — it isn't a
// task id), so it lands with
// `item_id = NULL`; and `orch_events` has no `remote_project` column at all,
// only `control_plane_id`, which is many-to-one against Tack projects
// (`orch_links`). So a `paused_refused` row cannot be resolved back to
// *which* linked project's pod paused — showing a "paused" indicator here
// would mean either guessing (wrong the moment a control plane has more than
// one linked project) or a real ingestion change (persisting
// `RemoteEvent.project`/a `remote_project` column and correlating
// `paused_refused` at the project level, not the item level.
// This endpoint stays honest by omission rather than guessing.

/// `GET /api/projects/{id}/orch-budget` response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrchBudgetResponse {
    pub linked: bool,
    pub control_plane_id: Option<Uuid>,
    pub control_plane_name: Option<String>,
    /// `"unknown"` | `"healthy"` | `"degraded"` | `"unreachable"`. `None` only
    /// when `linked` is `false`.
    pub health: Option<String>,
    /// User-set cap (`orch_links.budget_usd`) — `None` if unlinked or unset.
    /// Deliberately unsuffixed, same convention as `FleetEntry::budget_usd`.
    pub budget_usd: Option<f64>,
    /// Summed from `orch_tasks` for this project's items. Real, historical
    /// data — always present (never null) regardless of `linked`, since a
    /// project can accumulate mirrored dispatch history and later be
    /// unlinked without that history becoming false.
    pub tokens_in: i64,
    pub tokens_out: i64,
    /// `None` when unlinked, when the linked plane is `unreachable`, or when
    /// its health can't be resolved — never coerced to a confident-looking
    /// zero. `Some(0.0)` means the plane is reachable and genuinely has no
    /// mirrored cost yet.
    pub cost_usd_estimated: Option<f64>,
    /// Always `None` today — no pricing-snapshot mechanism exists yet.
    pub pricing_snapshot_at: Option<String>,
}

/// `GET /api/projects/{id}/orch-budget`.
#[utoipa::path(
    get,
    path = "/api/projects/{id}/orch-budget",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "This project's budget cap vs. estimated spend to date", body = OrchBudgetResponse),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn get_orch_budget(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<OrchBudgetResponse>> {
    let link = state.repo.get_orch_link(project_id).await?;
    let usage = project_task_usage(state.pool(), project_id).await?;

    let Some(link) = link else {
        return Ok(Json(OrchBudgetResponse {
            linked: false,
            control_plane_id: None,
            control_plane_name: None,
            health: None,
            budget_usd: None,
            tokens_in: usage.tokens_in,
            tokens_out: usage.tokens_out,
            cost_usd_estimated: None,
            pricing_snapshot_at: None,
        }));
    };

    let plane = state
        .repo
        .get_control_plane(link.control_plane_id)
        .await
        .ok();
    let health = plane.as_ref().map(|p| p.health.clone());
    let cost_usd_estimated = match health.as_deref() {
        Some("unreachable") | None => None,
        Some(_) => Some(usage.cost_usd_estimated),
    };

    Ok(Json(OrchBudgetResponse {
        linked: true,
        control_plane_id: Some(link.control_plane_id),
        control_plane_name: plane.map(|p| p.name),
        health,
        budget_usd: link.budget_usd,
        tokens_in: usage.tokens_in,
        tokens_out: usage.tokens_out,
        cost_usd_estimated,
        pricing_snapshot_at: None,
    }))
}

// ════════════════════════════════════════════════════════════════════════════
// GET /api/metrics — Tack's own work-tracking metrics merged with the latest
// mirrored docket sample per metric/label set
// ════════════════════════════════════════════════════════════════════════════
//
// Prometheus text exposition format — not JSON, unlike every other handler in
// this module. Round-trips cleanly through `tack_orch::adapters::prometheus::
// parse`, the same parser that decodes docket's own `/metrics` responses.
//
// Sits behind the same gates as every other route in this sub-router
// (`require_orch_enabled`, plus the ordinary Bearer-token gate applied to the
// whole `/api` router in `router.rs`) rather than being fully unauthenticated
// like docket's own `/metrics` — this endpoint lives in the same
// TACK_ORCH_ENABLE-gated sub-router as `/fleet`/`/control-planes`, so it
// inherits that sub-router's gates rather than getting a bespoke exemption —
// a known deviation from the roadmap's "unauthenticated like docket's" wording.

fn escape_prometheus_label_value(v: &str) -> String {
    v.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn format_prometheus_labels(labels: &BTreeMap<String, String>) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = labels
        .iter()
        .map(|(k, v)| format!("{k}=\"{}\"", escape_prometheus_label_value(v)))
        .collect();
    format!("{{{}}}", parts.join(","))
}

#[derive(sqlx::FromRow)]
struct ItemStatusCount {
    project_id: String,
    project_name: String,
    status: String,
    count: i64,
}

#[derive(sqlx::FromRow)]
struct ItemCycleTime {
    project_id: String,
    project_name: String,
    avg_seconds: Option<f64>,
}

#[derive(sqlx::FromRow)]
struct ItemThroughput {
    project_id: String,
    project_name: String,
    count: i64,
}

async fn tack_item_status_counts(
    pool: &sqlx::SqlitePool,
) -> Result<Vec<ItemStatusCount>, sqlx::Error> {
    sqlx::query_as(
        "SELECT i.project_id AS project_id, p.name AS project_name, i.status AS status, \
                COUNT(*) AS count \
         FROM items i JOIN projects p ON p.id = i.project_id \
         GROUP BY i.project_id, i.status",
    )
    .fetch_all(pool)
    .await
}

async fn tack_item_cycle_time(pool: &sqlx::SqlitePool) -> Result<Vec<ItemCycleTime>, sqlx::Error> {
    sqlx::query_as(
        "SELECT i.project_id AS project_id, p.name AS project_name, \
                AVG((julianday(i.completed_at) - julianday(i.started_at)) * 86400.0) AS avg_seconds \
         FROM items i JOIN projects p ON p.id = i.project_id \
         WHERE i.completed_at IS NOT NULL AND i.started_at IS NOT NULL \
         GROUP BY i.project_id",
    )
    .fetch_all(pool)
    .await
}

async fn tack_item_throughput_7d(
    pool: &sqlx::SqlitePool,
) -> Result<Vec<ItemThroughput>, sqlx::Error> {
    let since = (Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    sqlx::query_as(
        "SELECT i.project_id AS project_id, p.name AS project_name, COUNT(*) AS count \
         FROM items i JOIN projects p ON p.id = i.project_id \
         WHERE i.completed_at IS NOT NULL AND i.completed_at >= ? \
         GROUP BY i.project_id",
    )
    .bind(since)
    .fetch_all(pool)
    .await
}

fn render_metrics_text(
    item_status: &[ItemStatusCount],
    cycle_time: &[ItemCycleTime],
    throughput: &[ItemThroughput],
    docket: &[OrchMetricLatest],
) -> String {
    let mut out = String::new();

    out.push_str("# HELP tack_items_total Number of items by project and status.\n");
    out.push_str("# TYPE tack_items_total gauge\n");
    for row in item_status {
        let mut labels = BTreeMap::new();
        labels.insert("project_id".to_string(), row.project_id.clone());
        labels.insert("project_name".to_string(), row.project_name.clone());
        labels.insert("status".to_string(), row.status.clone());
        out.push_str(&format!(
            "tack_items_total{} {}\n",
            format_prometheus_labels(&labels),
            row.count
        ));
    }

    out.push_str(
        "# HELP tack_item_cycle_time_seconds_avg Average seconds from an item's started_at to completed_at.\n",
    );
    out.push_str("# TYPE tack_item_cycle_time_seconds_avg gauge\n");
    for row in cycle_time {
        if let Some(avg) = row.avg_seconds {
            let mut labels = BTreeMap::new();
            labels.insert("project_id".to_string(), row.project_id.clone());
            labels.insert("project_name".to_string(), row.project_name.clone());
            out.push_str(&format!(
                "tack_item_cycle_time_seconds_avg{} {}\n",
                format_prometheus_labels(&labels),
                avg
            ));
        }
    }

    out.push_str("# HELP tack_items_completed_7d Items completed in the trailing 7 days.\n");
    out.push_str("# TYPE tack_items_completed_7d gauge\n");
    for row in throughput {
        let mut labels = BTreeMap::new();
        labels.insert("project_id".to_string(), row.project_id.clone());
        labels.insert("project_name".to_string(), row.project_name.clone());
        out.push_str(&format!(
            "tack_items_completed_7d{} {}\n",
            format_prometheus_labels(&labels),
            row.count
        ));
    }

    if !docket.is_empty() {
        out.push_str(
            "# Mirrored docket metrics: latest known sample per metric/label set, per linked \
             control plane. A control_plane label is added to disambiguate multiple planes; \
             every other label is docket's own, verbatim.\n",
        );
    }
    for row in docket {
        let mut labels = row.labels.clone();
        labels.insert("control_plane".to_string(), row.control_plane_name.clone());
        out.push_str(&format!(
            "{}{} {}\n",
            row.name,
            format_prometheus_labels(&labels),
            row.value
        ));
    }

    out
}

/// `GET /api/metrics` — Prometheus text exposition merging Tack's own
/// work-tracking metrics (items by status, average cycle time, 7-day
/// throughput) with the latest mirrored sample of every metric docket has
/// reported for each linked control plane (task 34.7). One Grafana/Prometheus
/// scrape of this endpoint covers the whole factory.
#[utoipa::path(
    get,
    path = "/api/metrics",
    tag = "orchestration",
    responses(
        (status = 200, description = "Prometheus text exposition: Tack's own work-tracking metrics plus the latest mirrored docket sample per metric/label set", content_type = "text/plain"),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn get_metrics(State(state): State<AppState>) -> ApiResult<Response> {
    let pool = state.pool();
    let item_status = tack_item_status_counts(pool).await?;
    let cycle_time = tack_item_cycle_time(pool).await?;
    let throughput = tack_item_throughput_7d(pool).await?;
    let docket = state.repo.list_latest_orch_metrics().await?;

    let body = render_metrics_text(&item_status, &cycle_time, &throughput, &docket);

    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
        .into_response())
}

// ════════════════════════════════════════════════════════════════════════════
// GET /api/projects/{id}/orch-policy — guardrail/tool-call/approval metrics
// for this project's linked control plane
// ════════════════════════════════════════════════════════════════════════════
//
// **Control-plane-wide, not project-scoped — read this before trusting a
// number here as "this project's" anything.** docket's own `/metrics`
// (`serve.py::render_metrics`/`_collect_trace_loop_metrics`, confirmed by
// reading the source) folds every linked project's trace files together
// fleet-wide; there is no per-project breakdown anywhere in docket's
// guardrail/approval/tool-call counters. Ingestion mirrors that same
// fleet-wide shape into `orch_metrics`, keyed by `(control_plane_id, name,
// labels)` — no `remote_project` label exists to filter on, because docket
// never emits one. So every figure this endpoint returns describes *the whole
// control plane this project is linked to*, not just this project's own
// agents — on a control plane with more than one linked project, these
// numbers are shared identically across every one of them. This is disclosed
// on the wire (`scoped_to_control_plane_only: true`, always present, never
// false) rather than left for a caller to assume, and the frontend panel
// repeats the caveat in its own copy.
//
// Chain-verification of the audit log backing these counters (tamper
// detection) is deliberately not reimplemented here — `docket audit verify`
// (CLI-only) already does this against the pod's own `DOCKET_HOME`; the
// frontend panel links out to it as a command to run rather than this
// building a second verifier in Rust.

/// One `docket_tool_calls_total{decision=...}` sample.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ToolCallEntry {
    /// `"allow"` | `"ask"` | `"deny"` — docket's own closed vocabulary
    /// (`core/tools.py`'s tool-gate decisions), shown verbatim so an
    /// unrecognised future value still renders rather than being dropped.
    pub decision: String,
    pub count: f64,
}

/// One `docket_policy_hits_total{policy_id=...,hook=...,action=...}` sample.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PolicyHitEntry {
    pub policy_id: String,
    /// `"pre_input"` | `"pre_output"` | `"pre_tool_call"`.
    pub hook: String,
    /// `"block"` | `"require_approval"` | `"ask"` | `"warn"` | `"redact"` — docket's
    /// own vocabulary, shown verbatim.
    pub action: String,
    pub count: f64,
}

/// One `docket_approvals_total{channel=...,outcome=...}` sample.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ApprovalChannelEntry {
    /// `"cli"` | `"http"` | `"mcp"` | `"telegram"` | `"tack"` | `"timeout"`.
    pub channel: String,
    /// `"granted"` | `"denied"` — a `"timeout"` channel always resolves
    /// `"denied"` (a fail-closed expiry, never a human decision; see
    /// `core/approval.py`).
    pub outcome: String,
    pub count: f64,
}

/// `GET /api/projects/{id}/orch-policy` response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OrchPolicyResponse {
    pub linked: bool,
    pub control_plane_id: Option<Uuid>,
    pub control_plane_name: Option<String>,
    /// `"unknown"` | `"healthy"` | `"degraded"` | `"unreachable"`. `None` only
    /// when `linked` is `false`.
    pub health: Option<String>,
    /// Always `true` — see the module doc above. Present on the wire so a
    /// caller can't mistake this response for per-project data just because a
    /// `project_id` is in the URL.
    pub scoped_to_control_plane_only: bool,
    /// Latest scrape time across every sample folded into this response, or
    /// `None` if the plane has never reported guardrail metrics.
    pub scraped_at: Option<DateTime<Utc>>,
    pub tool_calls: Vec<ToolCallEntry>,
    /// `deny / (allow + ask + deny)`. `None` when no tool-gate decisions have
    /// been observed at all — deliberately not `0.0`, since zero would claim
    /// a clean, evaluated history rather than "no data yet".
    pub denial_rate: Option<f64>,
    pub policy_hits: Vec<PolicyHitEntry>,
    pub approvals_by_channel: Vec<ApprovalChannelEntry>,
}

/// `GET /api/projects/{id}/orch-policy`.
#[utoipa::path(
    get,
    path = "/api/projects/{id}/orch-policy",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Guardrail/tool-call/approval metrics for this project's linked control plane (control-plane-wide — see scoped_to_control_plane_only)", body = OrchPolicyResponse),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn get_orch_policy(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<OrchPolicyResponse>> {
    let link = state.repo.get_orch_link(project_id).await?;
    let Some(link) = link else {
        return Ok(Json(OrchPolicyResponse {
            linked: false,
            control_plane_id: None,
            control_plane_name: None,
            health: None,
            scoped_to_control_plane_only: true,
            scraped_at: None,
            tool_calls: Vec::new(),
            denial_rate: None,
            policy_hits: Vec::new(),
            approvals_by_channel: Vec::new(),
        }));
    };

    let plane = state
        .repo
        .get_control_plane(link.control_plane_id)
        .await
        .ok();
    let health = plane.as_ref().map(|p| p.health.clone());
    let control_plane_name = plane.map(|p| p.name);

    let samples = state.repo.list_latest_orch_metrics().await?;
    let mine: Vec<&OrchMetricLatest> = samples
        .iter()
        .filter(|s| s.control_plane_id == link.control_plane_id)
        .collect();

    let scraped_at = mine.iter().map(|s| s.scraped_at).max();

    let mut tool_calls = Vec::new();
    let mut allow_total = 0.0;
    let mut ask_total = 0.0;
    let mut deny_total = 0.0;
    let mut policy_hits = Vec::new();
    let mut approvals_by_channel = Vec::new();

    for s in &mine {
        match s.name.as_str() {
            "docket_tool_calls_total" => {
                let decision = s.labels.get("decision").cloned().unwrap_or_default();
                match decision.as_str() {
                    "allow" => allow_total += s.value,
                    "ask" => ask_total += s.value,
                    "deny" => deny_total += s.value,
                    _ => {}
                }
                tool_calls.push(ToolCallEntry {
                    decision,
                    count: s.value,
                });
            }
            "docket_policy_hits_total" => {
                policy_hits.push(PolicyHitEntry {
                    policy_id: s.labels.get("policy_id").cloned().unwrap_or_default(),
                    hook: s.labels.get("hook").cloned().unwrap_or_default(),
                    action: s.labels.get("action").cloned().unwrap_or_default(),
                    count: s.value,
                });
            }
            "docket_approvals_total" => {
                approvals_by_channel.push(ApprovalChannelEntry {
                    channel: s.labels.get("channel").cloned().unwrap_or_default(),
                    outcome: s.labels.get("outcome").cloned().unwrap_or_default(),
                    count: s.value,
                });
            }
            _ => {}
        }
    }

    let total_tool_calls = allow_total + ask_total + deny_total;
    let denial_rate = (total_tool_calls > 0.0).then_some(deny_total / total_tool_calls);

    Ok(Json(OrchPolicyResponse {
        linked: true,
        control_plane_id: Some(link.control_plane_id),
        control_plane_name,
        health,
        scoped_to_control_plane_only: true,
        scraped_at,
        tool_calls,
        denial_rate,
        policy_hits,
        approvals_by_channel,
    }))
}

// ════════════════════════════════════════════════════════════════════════════
// GET /api/items/{id}/agent-activity, GET /api/projects/{id}/agent-activity
// ════════════════════════════════════════════════════════════════════════════
//
// Reconciled field-for-field against the frontend boundary file
// `frontend/src/shared/agentActivity/api.ts` — see that file's header comment
// for the full field-provenance table (which migration/column backs each
// field, and which `tack-orch` enum backs each raw status string). Two
// decisions resolved here:
//
// 1. **"Latest attempt" tie-break** — highest `attempt` number wins; ties
//    broken by `dispatched_at` desc. See
//    `Repository::list_latest_orch_task_status_for_project`'s doc comment for
//    the (purely mechanical) final tie-break that makes the SQL deterministic.
// 2. **Inner join vs. left join for the bulk badge endpoint** — uses an
//    inner join. An item with no `orch_tasks` row simply has no row in
//    `AgentBadgeResponse`, which is exactly the "no chip" signal
//    `useAgentActivityMap` already implements on the frontend. A left join
//    would need a nullable-status contract to express "never dispatched" vs.
//    "dispatched, but the reconciler hasn't polled since" — no UI reads that
//    distinction today, so it isn't worth the wire-shape complexity yet.
//
// One field each endpoint adds:
// `ItemAgentActivityResponse.events_truncated` / `.events_retention_days`.
// `orch_events_daily` (the retention rollup) aggregates by
// day/control_plane_id/event_type only — it drops `item_id` entirely, so
// there is no way to ask "were *this item's* events rolled up." The honest
// signal this endpoint can give instead: whether the item has any attempt
// dispatched before the current retention cutoff, meaning some of its
// history may already be gone from the raw `orch_events` table this
// endpoint reads. `events` itself is always exactly what's left in that
// table — never silently presented as the complete history when it might
// not be: an estimate presented as certainty is the same failure whether the
// number is money or a timeline. Additive fields an old client ignores
// safely; the UI does not yet render them.

/// One `orch_events` row. See `ItemAgentEventResponse` in
/// `frontend/src/shared/agentActivity/api.ts`.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ItemAgentEventResponse {
    pub id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

impl From<OrchEvent> for ItemAgentEventResponse {
    fn from(e: OrchEvent) -> Self {
        Self {
            id: e.id,
            event_type: e.event_type,
            payload: e.payload,
            occurred_at: e.occurred_at,
        }
    }
}

/// The `orch_runs` row correlated to an attempt via `remote_run_id`, if any
/// has been mirrored yet.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ItemAgentRunResponse {
    pub run_id: String,
    pub source: String,
    pub state: String,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    /// Non-empty only when `state == "failed"`. Projected as an empty string,
    /// never `null` — matches `ItemAgentRun.error: string` on the frontend
    /// (deliberately not `string | null`, unlike `OrchRun.error` in the Rust
    /// repo layer, which is `Option<String>`).
    pub error: String,
}

impl From<OrchRun> for ItemAgentRunResponse {
    fn from(r: OrchRun) -> Self {
        Self {
            run_id: r.run_id,
            source: r.source,
            state: r.state,
            started_at: r.started_at,
            ended_at: r.ended_at,
            error: r.error.unwrap_or_default(),
        }
    }
}

/// One `orch_tasks` row (one dispatch attempt) for the item.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ItemAgentAttemptResponse {
    pub remote_task_id: String,
    pub remote_run_id: Option<String>,
    pub remote_status: String,
    pub attempt: i64,
    pub dispatched_at: DateTime<Utc>,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cost_usd_estimated: Option<f64>,
    /// Always `null` — no pricing-snapshot mechanism exists anywhere in the
    /// system yet (the Fleet view has the identical gap). Left `null` rather
    /// than invented.
    pub pricing_snapshot_at: Option<String>,
    pub run: Option<ItemAgentRunResponse>,
    pub events: Vec<ItemAgentEventResponse>,
}

/// One `orch_approvals` row for the item (pending or already decided).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ItemAgentApprovalResponse {
    pub token: String,
    pub remote_task_id: Option<String>,
    pub agent: Option<String>,
    pub action: Option<String>,
    pub state: String,
    pub requested_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
}

impl From<OrchApproval> for ItemAgentApprovalResponse {
    fn from(a: OrchApproval) -> Self {
        Self {
            token: a.token,
            remote_task_id: a.remote_task_id,
            agent: a.agent,
            action: a.action,
            state: a.state,
            requested_at: a.requested_at,
            decided_at: a.decided_at,
        }
    }
}

/// `GET /api/items/{id}/agent-activity` response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ItemAgentActivityResponse {
    /// Newest attempt first (`orch_tasks.attempt DESC` — the repo layer's
    /// `list_orch_tasks_for_item` already returns this order).
    pub attempts: Vec<ItemAgentAttemptResponse>,
    /// Newest-requested first; pending and decided both included.
    pub approvals: Vec<ItemAgentApprovalResponse>,
    /// See the module-level doc comment above `ItemAgentEventResponse` for
    /// why this can't be a precise per-item fact and what it means instead.
    pub events_truncated: bool,
    pub events_retention_days: u32,
}

/// `GET /api/items/{id}/agent-activity` — every mirrored dispatch attempt and
/// approval for one item, newest first.
#[utoipa::path(
    get,
    path = "/api/items/{id}/agent-activity",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Item ID")),
    responses(
        (status = 200, description = "Dispatch attempts and approvals mirrored for this item", body = ItemAgentActivityResponse),
        (status = 404, description = "Item not found, or orchestration disabled", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn get_item_agent_activity(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> ApiResult<Json<ItemAgentActivityResponse>> {
    state
        .repo
        .get_item(item_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {item_id} not found")))?;

    let tasks = state.repo.list_orch_tasks_for_item(item_id).await?;
    let runs = state.repo.list_orch_runs_for_item(item_id).await?;
    let approvals = state.repo.list_orch_approvals_for_item(item_id).await?;
    let events = state.repo.list_orch_events_for_item(item_id, None).await?;

    let runs_by_id: BTreeMap<String, OrchRun> =
        runs.into_iter().map(|r| (r.run_id.clone(), r)).collect();

    let mut events_by_run: BTreeMap<String, Vec<ItemAgentEventResponse>> = BTreeMap::new();
    for e in events {
        if let Some(run_id) = e.run_id.clone() {
            events_by_run.entry(run_id).or_default().push(e.into());
        }
    }

    let retention_days = state.config.orch_event_retention_days;
    let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);
    let events_truncated = tasks.iter().any(|t| t.dispatched_at < cutoff);

    let attempts = tasks
        .into_iter()
        .map(|t| {
            let run = t
                .remote_run_id
                .as_ref()
                .and_then(|rid| runs_by_id.get(rid))
                .cloned()
                .map(ItemAgentRunResponse::from);
            let events = t
                .remote_run_id
                .as_ref()
                .and_then(|rid| events_by_run.get(rid))
                .cloned()
                .unwrap_or_default();

            ItemAgentAttemptResponse {
                remote_task_id: t.remote_task_id,
                remote_run_id: t.remote_run_id,
                remote_status: t.remote_status,
                attempt: t.attempt,
                dispatched_at: t.dispatched_at,
                tokens_in: t.tokens_in,
                tokens_out: t.tokens_out,
                cost_usd_estimated: t.cost_usd_estimated,
                pricing_snapshot_at: None,
                run,
                events,
            }
        })
        .collect();

    Ok(Json(ItemAgentActivityResponse {
        attempts,
        approvals: approvals.into_iter().map(Into::into).collect(),
        events_truncated,
        events_retention_days: retention_days,
    }))
}

/// One row of `GET /api/projects/{id}/agent-activity` — the minimum a
/// Board/List/Table badge needs: an item's latest dispatch attempt's raw
/// status. See the module-level doc comment for the inner-join / tie-break
/// decisions.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AgentBadgeRowResponse {
    pub item_id: Uuid,
    pub remote_status: String,
    pub attempt: i64,
    pub updated_at: DateTime<Utc>,
}

/// `GET /api/projects/{id}/agent-activity` response envelope — matches
/// `frontend/src/shared/agentActivity/api.ts`'s `AgentBadgeResponse` exactly
/// (`{ rows: [...] }`, not a bare array).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AgentBadgeResponse {
    pub rows: Vec<AgentBadgeRowResponse>,
}

/// `GET /api/projects/{id}/agent-activity` — one row per item in this project
/// that has at least one mirrored dispatch attempt, each carrying only its
/// latest attempt's raw status (the Board/List/Table badge's data source).
/// Does not 404 for an unknown `project_id` — mirrors `list_items`'s
/// precedent (`handlers/items.rs`) of just returning an empty result for a
/// project-scoped list rather than a bulk-fetch racing a project's own
/// lifecycle (e.g. deleted between the page loading and this poll).
#[utoipa::path(
    get,
    path = "/api/projects/{id}/agent-activity",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Latest dispatch-attempt status per item with agent activity", body = AgentBadgeResponse),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn get_project_agent_activity(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> ApiResult<Json<AgentBadgeResponse>> {
    let rows = state
        .repo
        .list_latest_orch_task_status_for_project(project_id)
        .await?
        .into_iter()
        .map(|r| AgentBadgeRowResponse {
            item_id: r.item_id,
            remote_status: r.remote_status,
            attempt: r.attempt,
            updated_at: r.updated_at,
        })
        .collect();

    Ok(Json(AgentBadgeResponse { rows }))
}

// ════════════════════════════════════════════════════════════════════════════
// POST /api/items/{id}/dispatch
// ════════════════════════════════════════════════════════════════════════════
//
// The actual dispatch flow lives in `crate::dispatcher` — this section is
// just the HTTP↔`DispatchOutcome` translation. See that module's doc
// comment for the idempotency guarantee and the `trusted` boundary.

/// A dispatched (or already-in-flight) `orch_tasks` row, projected for the
/// dispatch response. Deliberately smaller than `ItemAgentAttemptResponse`
/// (`GET /items/{id}/agent-activity`) — no `run`/`events`/token-cost
/// fields, since a task this fresh has none of that mirrored yet.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DispatchedTaskResponse {
    pub remote_task_id: String,
    pub remote_status: String,
    pub attempt: i64,
    pub dispatched_at: DateTime<Utc>,
    pub trusted: bool,
}

impl From<OrchTask> for DispatchedTaskResponse {
    fn from(t: OrchTask) -> Self {
        Self {
            remote_task_id: t.remote_task_id,
            remote_status: t.remote_status,
            attempt: t.attempt,
            dispatched_at: t.dispatched_at,
            trusted: t.trusted,
        }
    }
}

/// `POST /api/items/{id}/dispatch` response. `outcome` is one of
/// `"dispatched"`, `"waiting_approval"`, `"already_in_flight"`,
/// `"no_dispatch_policy"`, `"not_eligible"`, `"blocked"` — every one of
/// these is a `200`, including `"blocked"`: docket gave a definitive,
/// well-formed refusal, which is a successful round-trip from Tack's HTTP
/// perspective, not a Tack-side error. Callers must branch on `outcome`,
/// not on HTTP status, to tell these apart. See `dispatcher::DispatchOutcome`
/// for the same taxonomy on the Rust side.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DispatchItemResponse {
    pub outcome: String,
    pub task: Option<DispatchedTaskResponse>,
    /// Present only when `outcome == "waiting_approval"`.
    pub approval_token: Option<String>,
    /// Present only when `outcome == "not_eligible"`.
    pub current_status: Option<String>,
    /// Present only when `outcome == "not_eligible"`.
    pub dispatch_from: Option<Vec<String>>,
    /// Present only when `outcome == "blocked"` — the id of the guardrail
    /// policy that fired (`OrchError::PolicyBlocked::policy_id`),
    /// as a typed field rather than something a caller has to parse back out
    /// of `message`.
    pub policy_id: Option<String>,
    /// Present only when `outcome == "blocked"` — docket's own message,
    /// verbatim, for display.
    pub message: Option<String>,
    /// The Tack status `status_map` named for this trigger and actually
    /// applied. Absent when `status_map` named no target for this trigger,
    /// when the item was already there, or when the workflow engine
    /// rejected it (see `status_map_rejected` below).
    pub status_applied: Option<String>,
    /// Set when the workflow engine refused the `status_map`-driven
    /// transition (`status_map_rejected`
    /// outcome). The item was left exactly as it was; this is the engine's
    /// own reason (e.g. an invalid transition or a WIP limit).
    pub status_map_rejected: Option<String>,
}

impl DispatchItemResponse {
    fn empty(outcome: &str) -> Self {
        Self {
            outcome: outcome.to_string(),
            task: None,
            approval_token: None,
            current_status: None,
            dispatch_from: None,
            policy_id: None,
            message: None,
            status_applied: None,
            status_map_rejected: None,
        }
    }
}

impl From<DispatchOutcome> for DispatchItemResponse {
    fn from(outcome: DispatchOutcome) -> Self {
        match outcome {
            DispatchOutcome::NoDispatchPolicy => Self::empty("no_dispatch_policy"),
            DispatchOutcome::NotEligible {
                current_status,
                dispatch_from,
            } => Self {
                current_status: Some(current_status),
                dispatch_from: Some(dispatch_from),
                ..Self::empty("not_eligible")
            },
            DispatchOutcome::AlreadyInFlight { task } => Self {
                task: Some(task.into()),
                ..Self::empty("already_in_flight")
            },
            DispatchOutcome::Blocked { policy_id, message } => Self {
                policy_id: Some(policy_id),
                message: Some(message),
                ..Self::empty("blocked")
            },
            DispatchOutcome::Success(s) => Self {
                task: Some(s.task.into()),
                approval_token: s.approval_token,
                status_applied: s
                    .status_application
                    .as_ref()
                    .filter(|a| a.applied)
                    .map(|a| a.target_status.clone()),
                status_map_rejected: s
                    .status_application
                    .as_ref()
                    .and_then(|a| a.rejected_reason.clone()),
                ..Self::empty(if s.waiting_approval {
                    "waiting_approval"
                } else {
                    "dispatched"
                })
            },
        }
    }
}

/// `POST /api/items/{id}/dispatch` — enqueue a governed task on the item's
/// project's linked control plane. See the `dispatcher` module for the full
/// flow; this handler is just the HTTP boundary.
///
/// **Trust default.** Unlike `dispatcher::dispatch_item`'s own required
/// `trusted: bool` parameter (no default, by design — see that module's
/// doc), this direct/manual entry point has no request body asking the
/// caller to state trust explicitly, so it resolves one via
/// `dispatcher::resolve_default_trust` — a conservative stopgap pending
/// item-provenance-driven trust resolution. See that function's own
/// doc comment for exactly what it checks today and what it doesn't yet.
#[utoipa::path(
    post,
    path = "/api/items/{id}/dispatch",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Item ID")),
    responses(
        (status = 200, description = "Dispatch outcome — branch on the `outcome` field, not HTTP status", body = DispatchItemResponse),
        (status = 404, description = "Item not found, or orchestration disabled", body = ErrorEnvelope),
        (status = 409, description = "Project not linked to a control plane, a dispatch for this item is already in flight, or the control plane could not be reached", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn dispatch_item(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> ApiResult<Json<DispatchItemResponse>> {
    let trusted = dispatcher::resolve_default_trust(&state, item_id).await?;
    let outcome = dispatcher::dispatch_item(&state, item_id, trusted).await?;
    Ok(Json(outcome.into()))
}

// ════════════════════════════════════════════════════════════════════════════
// POST /api/sprints/{id}/dispatch, GET /api/sprints/{id}/dispatch/dry-run
// DAG-ordered sprint dispatch.
// ════════════════════════════════════════════════════════════════════════════
//
// The planning (dependency order + readiness gating) and execution
// (bounded-concurrency dispatch) logic lives in `crate::sprint_dispatch` —
// see that module's doc comment for the five design decisions
// (partial failure, dependency readiness, concurrency, the
// no-write-txn-across-HTTP rule, and dry-run honesty). This section is just
// the HTTP↔domain-type translation, the same split used for the
// single-item dispatch endpoint above.

/// Query params shared by both sprint-dispatch routes. A bare query param
/// rather than a request body — `POST /dispatch` has nothing else to say,
/// and using the same shape for both routes keeps the dry-run preview and
/// the real run trivially comparable (same input, same knob).
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct SprintDispatchQuery {
    /// Bound on concurrent HTTP calls to the control plane for this run.
    /// Omit to use `sprint_dispatch::DEFAULT_MAX_IN_FLIGHT`; any value is
    /// clamped to `[1, sprint_dispatch::MAX_MAX_IN_FLIGHT]`. The dry-run
    /// response's own `max_in_flight` field reports the clamped value, so
    /// the UI can show exactly what a real run with this input would use.
    #[serde(default)]
    pub max_in_flight: Option<u32>,
}

/// One item's place in a sprint-dispatch plan or report — shared shape for
/// both the dry-run preview and the real run's per-item result, so the two
/// responses read the same way side by side. `decision` is one of:
/// `"waiting_on_dependencies"`, `"no_dispatch_policy"`, `"not_eligible"`,
/// `"already_in_flight"`, `"blocked"`, `"waiting_approval"`, `"dispatched"`,
/// `"would_dispatch"` (dry-run only — a real run would have called docket
/// and resolved to one of the outcomes above instead), or `"error"`
/// (real run only — this item's own dispatch failed or its worker task
/// panicked; every other item in the sprint still ran).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SprintDispatchItemResponse {
    pub item_id: Uuid,
    pub title: String,
    pub status: String,
    pub order: usize,
    pub decision: String,
    /// Present only when `decision == "waiting_on_dependencies"` — every
    /// direct dependency that hasn't reached a Done-category status yet.
    pub blocked_by: Option<Vec<Uuid>>,
    /// Present only when `decision == "error"` (real run only).
    pub error: Option<String>,
    pub task: Option<DispatchedTaskResponse>,
    pub approval_token: Option<String>,
    pub current_status: Option<String>,
    pub dispatch_from: Option<Vec<String>>,
    pub policy_id: Option<String>,
    pub message: Option<String>,
    pub status_applied: Option<String>,
    pub status_map_rejected: Option<String>,
}

impl SprintDispatchItemResponse {
    fn empty(item_id: Uuid, title: String, status: String, order: usize, decision: &str) -> Self {
        Self {
            item_id,
            title,
            status,
            order,
            decision: decision.to_string(),
            blocked_by: None,
            error: None,
            task: None,
            approval_token: None,
            current_status: None,
            dispatch_from: None,
            policy_id: None,
            message: None,
            status_applied: None,
            status_map_rejected: None,
        }
    }

    /// Overlay `DispatchItemResponse`'s fields (produced from a real
    /// `DispatchOutcome` via its own `From` impl — reused verbatim, not
    /// reimplemented) onto this row.
    fn with_dispatch_outcome(mut self, mapped: DispatchItemResponse) -> Self {
        self.decision = mapped.outcome;
        self.task = mapped.task;
        self.approval_token = mapped.approval_token;
        self.current_status = mapped.current_status;
        self.dispatch_from = mapped.dispatch_from;
        self.policy_id = mapped.policy_id;
        self.message = mapped.message;
        self.status_applied = mapped.status_applied;
        self.status_map_rejected = mapped.status_map_rejected;
        self
    }
}

impl From<sprint_dispatch::DryRunItem> for SprintDispatchItemResponse {
    fn from(i: sprint_dispatch::DryRunItem) -> Self {
        let base = |decision: &str| {
            Self::empty(
                i.item_id,
                i.title.clone(),
                i.status.clone(),
                i.order,
                decision,
            )
        };
        match i.decision {
            PreviewDecision::WaitingOnDependencies { blocked_by } => Self {
                blocked_by: Some(blocked_by),
                ..base("waiting_on_dependencies")
            },
            PreviewDecision::NoDispatchPolicy => base("no_dispatch_policy"),
            PreviewDecision::NotEligible {
                current_status,
                dispatch_from,
            } => Self {
                current_status: Some(current_status),
                dispatch_from: Some(dispatch_from),
                ..base("not_eligible")
            },
            PreviewDecision::AlreadyInFlight { task } => Self {
                task: Some(task.into()),
                ..base("already_in_flight")
            },
            PreviewDecision::WouldDispatch => base("would_dispatch"),
        }
    }
}

impl From<sprint_dispatch::SprintDispatchItem> for SprintDispatchItemResponse {
    fn from(i: sprint_dispatch::SprintDispatchItem) -> Self {
        let (title, status, order, item_id) =
            (i.title.clone(), i.status.clone(), i.order, i.item_id);
        match i.result {
            ItemResult::WaitingOnDependencies { blocked_by } => Self {
                blocked_by: Some(blocked_by),
                ..Self::empty(item_id, title, status, order, "waiting_on_dependencies")
            },
            ItemResult::Error(error) => Self {
                error: Some(error),
                ..Self::empty(item_id, title, status, order, "error")
            },
            ItemResult::Outcome(outcome) => {
                let mapped: DispatchItemResponse = (*outcome).into();
                Self::empty(item_id, title, status, order, "").with_dispatch_outcome(mapped)
            }
        }
    }
}

/// Summary counts over a real dispatch run's per-item `decision` values —
/// the UI's headline "8 dispatched, 2 waiting on dependencies" line without
/// re-deriving it client-side from the row list.
#[derive(Debug, Default, Serialize, utoipa::ToSchema)]
pub struct SprintDispatchSummary {
    pub total: usize,
    pub dispatched: usize,
    pub waiting_approval: usize,
    pub blocked: usize,
    pub already_in_flight: usize,
    pub waiting_on_dependencies: usize,
    pub not_eligible: usize,
    pub no_dispatch_policy: usize,
    pub would_dispatch: usize,
    pub errored: usize,
}

impl SprintDispatchSummary {
    fn tally(items: &[SprintDispatchItemResponse]) -> Self {
        let mut s = Self {
            total: items.len(),
            ..Self::default()
        };
        for item in items {
            match item.decision.as_str() {
                "dispatched" => s.dispatched += 1,
                "waiting_approval" => s.waiting_approval += 1,
                "blocked" => s.blocked += 1,
                "already_in_flight" => s.already_in_flight += 1,
                "waiting_on_dependencies" => s.waiting_on_dependencies += 1,
                "not_eligible" => s.not_eligible += 1,
                "no_dispatch_policy" => s.no_dispatch_policy += 1,
                "would_dispatch" => s.would_dispatch += 1,
                "error" => s.errored += 1,
                _ => {}
            }
        }
        s
    }
}

/// `GET /api/sprints/{id}/dispatch/dry-run` response. Zero side effects —
/// see `sprint_dispatch`'s module doc, decision 5.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DryRunSprintDispatchResponse {
    pub sprint_id: Uuid,
    pub max_in_flight: u32,
    pub summary: SprintDispatchSummary,
    pub items: Vec<SprintDispatchItemResponse>,
}

impl From<sprint_dispatch::DryRunPlan> for DryRunSprintDispatchResponse {
    fn from(plan: sprint_dispatch::DryRunPlan) -> Self {
        let items: Vec<SprintDispatchItemResponse> =
            plan.items.into_iter().map(Into::into).collect();
        let summary = SprintDispatchSummary::tally(&items);
        Self {
            sprint_id: plan.sprint_id,
            max_in_flight: plan.max_in_flight,
            summary,
            items,
        }
    }
}

/// `POST /api/sprints/{id}/dispatch` response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SprintDispatchResponse {
    pub sprint_id: Uuid,
    pub max_in_flight: u32,
    pub summary: SprintDispatchSummary,
    pub items: Vec<SprintDispatchItemResponse>,
}

impl From<sprint_dispatch::SprintDispatchReport> for SprintDispatchResponse {
    fn from(report: sprint_dispatch::SprintDispatchReport) -> Self {
        let items: Vec<SprintDispatchItemResponse> =
            report.items.into_iter().map(Into::into).collect();
        let summary = SprintDispatchSummary::tally(&items);
        Self {
            sprint_id: report.sprint_id,
            max_in_flight: report.max_in_flight,
            summary,
            items,
        }
    }
}

/// `GET /api/sprints/{id}/dispatch/dry-run` — the exact plan a real
/// `POST .../dispatch` call would execute (same order, same
/// dependency-readiness skips), with zero database writes and zero HTTP
/// calls to the control plane. See `sprint_dispatch::dry_run_sprint_dispatch`.
#[utoipa::path(
    get,
    path = "/api/sprints/{id}/dispatch/dry-run",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Sprint ID"), SprintDispatchQuery),
    responses(
        (status = 200, description = "Dependency-ordered dispatch plan — zero side effects", body = DryRunSprintDispatchResponse),
        (status = 404, description = "Sprint not found, or orchestration disabled", body = ErrorEnvelope),
        (status = 409, description = "Project not linked to a control plane", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn dry_run_sprint_dispatch(
    State(state): State<AppState>,
    Path(sprint_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<SprintDispatchQuery>,
) -> ApiResult<Json<DryRunSprintDispatchResponse>> {
    let plan =
        sprint_dispatch::dry_run_sprint_dispatch(&state, sprint_id, query.max_in_flight).await?;
    Ok(Json(plan.into()))
}

/// `POST /api/sprints/{id}/dispatch` — dispatch every dependency-ready item
/// in the sprint, in topological order, bounded by `max_in_flight`
/// concurrent control-plane calls. Each item's own `ItemSource` decides its
/// `trusted` flag (never a blanket value for the batch). A failure
/// dispatching one item never aborts the rest;
/// see `sprint_dispatch`'s module doc, decision 1.
#[utoipa::path(
    post,
    path = "/api/sprints/{id}/dispatch",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Sprint ID"), SprintDispatchQuery),
    responses(
        (status = 200, description = "Sprint dispatch report — one row per item, in dependency order", body = SprintDispatchResponse),
        (status = 404, description = "Sprint not found, or orchestration disabled", body = ErrorEnvelope),
        (status = 409, description = "Project not linked to a control plane", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn dispatch_sprint(
    State(state): State<AppState>,
    Path(sprint_id): Path<Uuid>,
    axum::extract::Query(query): axum::extract::Query<SprintDispatchQuery>,
) -> ApiResult<Json<SprintDispatchResponse>> {
    let report = sprint_dispatch::dispatch_sprint(&state, sprint_id, query.max_in_flight).await?;
    Ok(Json(report.into()))
}

// ════════════════════════════════════════════════════════════════════════════
// GET /api/approvals, POST /api/approvals/{token}
// Approvals inbox + decision proxy.
// ════════════════════════════════════════════════════════════════════════════
//
// The fleet-wide inbox of approvals **currently blocking an agent fleet**,
// plus the proxy that actually grants or denies one via docket's own
// `POST /approvals/{token}`. This is a two-privilege-level surface —
// reading the inbox needs only the ordinary orchestration gate (this
// module's `require_orch_enabled` layer + the outer `TACK_API_TOKEN` Bearer
// gate `router.rs` applies to the whole API), but *deciding* one needs the
// separate `TACK_ORCH_APPROVAL_TOKEN` on top (see
// `require_approval_token`'s own doc comment for the full "why", and
// granting an approval is a higher-privilege action than editing a card).
//
// Uncorrelated approvals (`item_id: null` — the gate could not be attributed
// to a Tack item, e.g. a CLI-dispatched run) are included here, not
// filtered out: the per-project Fleet view (`GET /fleet`) deliberately
// excludes them (an inner join on `items`/`projects`)
// specifically because this inbox is where they're meant to surface — an
// approval Tack can't attribute to a project is the one most likely to be
// silently blocking a fleet.

/// Header carrying the operator's `TACK_ORCH_APPROVAL_TOKEN` on a decision
/// request. Deliberately not `Authorization` (already spoken for by the
/// ordinary `TACK_API_TOKEN` Bearer gate — this is a second, independent
/// credential, not a replacement for the first) and deliberately a header,
/// not a request-body field (so it never ends up echoed into a JSON log
/// line the way a body field might).
pub const APPROVAL_TOKEN_HEADER: &str = "x-tack-approval-token";

/// One row of the fleet-wide approvals inbox — a pending `orch_approvals`
/// record enriched with the correlated control plane / item / project, when
/// known. See the module-doc section above on why uncorrelated rows
/// (`item_id: null`) are never filtered out.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PendingApprovalResponse {
    pub token: String,
    pub control_plane_id: Uuid,
    pub control_plane_name: String,
    pub item_id: Option<Uuid>,
    pub item_title: Option<String>,
    pub item_status: Option<String>,
    pub project_id: Option<Uuid>,
    pub project_name: Option<String>,
    pub remote_task_id: Option<String>,
    /// `orch_approvals.agent` — populated from docket's `role` field on
    /// ingestion. Role is the closest field docket's wire shape offers.
    pub agent: Option<String>,
    /// The gated action's description, already redacted by docket before it
    /// reached Tack's mirror.
    pub action: Option<String>,
    pub requested_at: DateTime<Utc>,
}

impl From<PendingOrchApproval> for PendingApprovalResponse {
    fn from(a: PendingOrchApproval) -> Self {
        Self {
            token: a.token,
            control_plane_id: a.control_plane_id,
            control_plane_name: a.control_plane_name,
            item_id: a.item_id,
            item_title: a.item_title,
            item_status: a.item_status,
            project_id: a.project_id,
            project_name: a.project_name,
            remote_task_id: a.remote_task_id,
            agent: a.agent,
            action: a.action,
            requested_at: a.requested_at,
        }
    }
}

/// `GET /api/approvals` response envelope.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PendingApprovalListResponse {
    /// Oldest-requested first — docket approvals fail closed on timeout, so
    /// surfacing the longest-waiting one first has a real cost (this
    /// card's rationale).
    pub rows: Vec<PendingApprovalResponse>,
    /// Whether `TACK_ORCH_APPROVAL_TOKEN` is configured on this server at
    /// all, **without ever exposing its value** — the same write-only-secret
    /// discipline as `ControlPlaneResponse.token_set` /
    /// `handlers::settings`'s `secret_key_set`. The frontend uses this to
    /// decide whether to render Grant/Deny controls at all (a missing
    /// server-side secret means nobody can act on this inbox today); the
    /// server still enforces the real check independently on every
    /// `POST /api/approvals/{token}` call regardless of what this flag says,
    /// so a stale/cached `true` can never grant a privilege the header check
    /// wouldn't also grant.
    pub grant_available: bool,
}

/// `GET /api/approvals` — the fleet-wide pending-approval inbox, oldest
/// first. Read-only; no `TACK_ORCH_APPROVAL_TOKEN` needed (see the
/// module-doc section above on why reading and deciding are different
/// privilege levels).
#[utoipa::path(
    get,
    path = "/api/approvals",
    tag = "orchestration",
    responses(
        (status = 200, description = "Fleet-wide pending-approval inbox, oldest first — includes uncorrelated approvals", body = PendingApprovalListResponse),
        (status = 404, description = "Orchestration disabled", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn list_pending_approvals(
    State(state): State<AppState>,
) -> ApiResult<Json<PendingApprovalListResponse>> {
    let rows = state
        .repo
        .list_pending_orch_approvals_with_context()
        .await?;
    Ok(Json(PendingApprovalListResponse {
        rows: rows.into_iter().map(Into::into).collect(),
        grant_available: state.config.orch_approval_token.is_some(),
    }))
}

/// Granting or denying a docket approval releases whatever an autonomous
/// agent was paused for — a materially higher-privilege action than the
/// ordinary `TACK_API_TOKEN`
/// Bearer gate already covers (which lets a caller move any card on the
/// board). So it requires the **separate** `TACK_ORCH_APPROVAL_TOKEN` on
/// top, checked here rather than as a blanket middleware layer (unlike
/// `require_orch_enabled`) because it applies to exactly one route, not the
/// whole `orch_routes()` sub-router — reading the inbox (`GET /approvals`)
/// deliberately does not go through this function.
///
/// **The safe default when `TACK_ORCH_APPROVAL_TOKEN` is unset: always
/// reject.** There is deliberately no "no secret configured, so skip the
/// check" branch the way `middleware::require_token`'s ordinary Bearer gate
/// has for an unset `TACK_API_TOKEN` ("pure-local mode, allow everything").
/// The two gates look similar but their safe defaults point opposite ways
/// on purpose: an unconfigured `TACK_API_TOKEN` means "no auth configured
/// for this whole install, trust the network boundary instead" — a
/// deliberate, instantly-reversible operator choice. An unconfigured
/// `TACK_ORCH_APPROVAL_TOKEN` must mean "nothing on this server is
/// configured to release a gated agent action" — not "anyone holding the
/// ordinary API token can." [`PendingApprovalListResponse::grant_available`]
/// is how the frontend learns this without ever seeing the secret itself.
fn require_approval_token(state: &AppState, headers: &HeaderMap) -> ApiResult<()> {
    let Some(expected) = &state.config.orch_approval_token else {
        return Err(ApiError::Forbidden(
            "granting or denying approvals requires TACK_ORCH_APPROVAL_TOKEN to be configured \
             on this server"
                .to_string(),
        ));
    };
    let provided = headers
        .get(APPROVAL_TOKEN_HEADER)
        .and_then(|v| v.to_str().ok());
    match provided {
        Some(tok) if crate::middleware::constant_time_eq(tok.as_bytes(), expected.as_bytes()) => {
            Ok(())
        }
        _ => Err(ApiError::Forbidden(format!(
            "missing or invalid {APPROVAL_TOKEN_HEADER} header"
        ))),
    }
}

/// Resolve `control_plane_id` into a live control-plane client. Built on
/// `adapters::registry::build`, which
/// `dispatcher::build_control_plane` and `handlers::provisioning::
/// resolve_control_plane` are built on too — three near-identical
/// callers sharing one implementation. Each caller keeps
/// its own request-scoped error mapping (this one, unlike the reconciler's
/// batch-loop use in `orch_store.rs`, must error rather than skip — see
/// `dispatcher::build_control_plane`'s doc comment for that distinction).
async fn build_control_plane_for_decision(
    state: &AppState,
    control_plane_id: Uuid,
) -> ApiResult<Arc<dyn OrchControlPlane>> {
    let row = match state.repo.get_control_plane(control_plane_id).await {
        Ok(row) => row,
        Err(sqlx::Error::RowNotFound) => {
            return Err(ApiError::NotFound(format!(
                "control plane {control_plane_id} not found"
            )));
        }
        Err(e) => return Err(ApiError::Database(e)),
    };
    let token = state.repo.get_control_plane_token(control_plane_id).await?;

    match registry::build(
        &row.kind,
        &row.base_url,
        token,
        &serde_json::json!({}),
        None,
    ) {
        Ok(adapter) => Ok(adapter),
        Err(RegistryError::Construction(e)) => Err(ApiError::Internal(anyhow::anyhow!(
            "failed to construct control-plane adapter: {e}"
        ))),
        Err(RegistryError::UnknownKind(kind)) => Err(ApiError::Internal(anyhow::anyhow!(
            "unsupported control-plane kind {kind:?}"
        ))),
    }
}

/// `POST /api/approvals/{token}` request body.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecisionAction {
    Grant,
    Deny,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DecideApprovalRequest {
    pub action: ApprovalDecisionAction,
}

/// `POST /api/approvals/{token}` response — docket's own resulting state
/// (`"granted"`/`"denied"`, or an unrecognised value shown as-is — this API
/// never fails on an unknown remote value).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DecideApprovalResponse {
    pub token: String,
    pub state: String,
}

/// `POST /api/approvals/{token}` — grant or deny a pending approval,
/// proxying to docket's own `POST /approvals/{token}` with `channel: "tack"`
/// so the decision is honestly attributed in docket's hash-chained audit
/// log rather than reading as an anonymous/CLI decision.
///
/// **Not idempotent, not reversible** — see [`OrchError::AlreadyDecided`]'s
/// doc comment for what happens when the token was already resolved
/// elsewhere (a normal race for an inbox like this, not a bug): this
/// handler reports it as `409`, not `500`, and the frontend treats it as
/// "remove this stale row," not as an error toast.
#[utoipa::path(
    post,
    path = "/api/approvals/{token}",
    tag = "orchestration",
    params(("token" = String, Path, description = "docket approval token")),
    request_body = DecideApprovalRequest,
    responses(
        (status = 200, description = "Decision applied — docket's own resulting state", body = DecideApprovalResponse),
        (status = 403, description = "Missing/invalid X-Tack-Approval-Token header, or TACK_ORCH_APPROVAL_TOKEN not configured on this server", body = ErrorEnvelope),
        (status = 404, description = "Unknown token, orchestration disabled, or the control plane that issued it was deleted", body = ErrorEnvelope),
        (status = 409, description = "The approval was already decided (granted/denied/expired) elsewhere", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state, headers))]
pub async fn decide_approval(
    State(state): State<AppState>,
    Path(token): Path<String>,
    headers: HeaderMap,
    Json(body): Json<DecideApprovalRequest>,
) -> ApiResult<Json<DecideApprovalResponse>> {
    require_approval_token(&state, &headers)?;

    let approval = state
        .repo
        .get_orch_approval(&token)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("approval {token} not found")))?;

    let control_plane = build_control_plane_for_decision(&state, approval.control_plane_id).await?;
    let grant = matches!(body.action, ApprovalDecisionAction::Grant);

    let result_state = match control_plane.decide_approval(&token, grant).await {
        Ok(s) => s,
        Err(OrchError::AlreadyDecided(message)) => return Err(ApiError::Conflict(message)),
        Err(OrchError::NotFound(message)) => return Err(ApiError::NotFound(message)),
        Err(OrchError::Auth) => {
            return Err(ApiError::Internal(anyhow::anyhow!(
                "control plane rejected Tack's own credentials while deciding an approval"
            )));
        }
        Err(e) => {
            return Err(ApiError::Conflict(format!(
                "failed to reach control plane to decide approval: {e}"
            )));
        }
    };

    // Local mirror only — docket already made the real decision above; this
    // just keeps the row out of the next `GET /api/approvals` fetch. See
    // `mark_orch_approval_decided`'s own doc comment.
    state
        .repo
        .mark_orch_approval_decided(&token, result_state.as_str(), Utc::now())
        .await?;

    Ok(Json(DecideApprovalResponse {
        token,
        state: result_state.as_str().to_string(),
    }))
}

// ════════════════════════════════════════════════════════════════════════════
// POST /api/projects/{id}/orch-dispatch — trigger a docket pipeline run
// ════════════════════════════════════════════════════════════════════════════
//
// ADR 0065. Distinct from every dispatch route above: this one claims no
// Tack item. It resolves the docket project from the caller's *existing*
// `orch_links` row (no second way to name one is invented), writes no
// `orch_tasks`/`execution_requests` row of its own, and never consults
// `decide_scheduling_owner` — a project-level pipeline run cannot collide
// with the one-item invariant those rows exist to enforce.

/// Header carrying the operator's `TACK_ORCH_DISPATCH_TOKEN` on
/// `POST /api/projects/{id}/orch-dispatch`. Distinct from
/// [`APPROVAL_TOKEN_HEADER`] — a different privileged action gets a
/// different token and a different header, never a shared one two unrelated
/// checks could be satisfied with by accident.
pub const DISPATCH_TOKEN_HEADER: &str = "x-tack-dispatch-token";

/// Triggering a docket pipeline run spends money on a remote fleet and
/// cannot be called back afterward (`DocketAdapter::capabilities().cancel`
/// is `false`) — a materially higher-privilege action than the ordinary
/// `TACK_API_TOKEN` Bearer gate covers. This mirrors
/// [`require_approval_token`] exactly, including its safe default: **an
/// unconfigured `TACK_ORCH_DISPATCH_TOKEN` means "nothing on this server is
/// configured to trigger a pipeline run," not "anyone holding the ordinary
/// API token can."** See that function's own doc comment for the full
/// "why" this differs from `TACK_API_TOKEN`'s own unset-means-trust-the-
/// network-boundary default.
fn require_dispatch_token(state: &AppState, headers: &HeaderMap) -> ApiResult<()> {
    let Some(expected) = &state.config.orch_dispatch_token else {
        return Err(ApiError::Forbidden(
            "dispatching a docket pipeline run requires TACK_ORCH_DISPATCH_TOKEN to be \
             configured on this server"
                .to_string(),
        ));
    };
    let provided = headers
        .get(DISPATCH_TOKEN_HEADER)
        .and_then(|v| v.to_str().ok());
    match provided {
        Some(tok) if crate::middleware::constant_time_eq(tok.as_bytes(), expected.as_bytes()) => {
            Ok(())
        }
        _ => Err(ApiError::Forbidden(format!(
            "missing or invalid {DISPATCH_TOKEN_HEADER} header"
        ))),
    }
}

/// `POST /api/projects/{id}/orch-dispatch` response.
///
/// **`run_id` is not a promise the run was permitted.** docket's
/// `POST /dispatch/{project}` creates the run record and answers before the
/// pipeline itself executes — guardrail evaluation included — on a thread
/// this response never waits on (ADR 0065, "A block is not synchronously
/// observable on this route"). This struct carries no `status`/`outcome`
/// field for that reason: the only fact this route can honestly report is
/// that docket accepted the request and started a run. What that run goes
/// on to do — success, failure, or a guardrail block — is not visible here;
/// it only becomes visible once the reconciler's own periodic poll of
/// docket's `/runs` endpoint mirrors this run's outcome into Tack.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DispatchProjectPipelineResponse {
    /// The docket project (`orch_links.remote_project`) the pipeline was
    /// started against.
    pub remote_project: String,
    /// docket's own pipeline-run id — a different kind of id from an
    /// `orch_tasks.remote_task_id`. Hand this to an operator who wants to
    /// correlate what they see later once the reconciler mirrors it.
    pub run_id: String,
}

/// `POST /api/projects/{id}/orch-dispatch` — trigger a full docket pipeline
/// run for the project's linked docket project. `variables` is forwarded to
/// docket exactly as received — an opaque `{name: value}` JSON object this
/// route never inspects, validates, or logs (ADR 0065 decision 6); omit the
/// field, or send `{}`, for a pipeline with no variables to resolve.
#[utoipa::path(
    post,
    path = "/api/projects/{id}/orch-dispatch",
    tag = "orchestration",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = DispatchProjectPipelineRequest,
    responses(
        (status = 200, description = "docket accepted the request and started a pipeline run. This reports that the run started, never that it was permitted — poll the reconciler's mirrored run state for the eventual outcome", body = DispatchProjectPipelineResponse),
        (status = 403, description = "Missing/invalid X-Tack-Dispatch-Token header, or TACK_ORCH_DISPATCH_TOKEN not configured on this server", body = ErrorEnvelope),
        (status = 404, description = "Project not found, project not linked to a control plane, or orchestration disabled", body = ErrorEnvelope),
        (status = 409, description = "docket refused the request itself (malformed variables, the control plane unreachable, or — defensively, though not reachable on this route today — a synchronous guardrail refusal)", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state, headers, body))]
pub async fn dispatch_project_pipeline(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<DispatchProjectPipelineRequest>,
) -> ApiResult<Json<DispatchProjectPipelineResponse>> {
    require_dispatch_token(&state, &headers)?;

    state
        .repo
        .get_project(project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {project_id} not found")))?;

    let Some(link) = state.repo.get_orch_link(project_id).await? else {
        return Err(ApiError::NotFound(format!(
            "project {project_id} is not linked to a control plane"
        )));
    };

    let control_plane = build_control_plane_for_decision(&state, link.control_plane_id).await?;

    let run_id = match control_plane
        .dispatch(&link.remote_project, body.variables)
        .await
    {
        Ok(id) => id,
        Err(OrchError::PolicyBlocked { policy_id, message }) => {
            return Err(ApiError::Conflict(format!(
                "docket refused the dispatch: guardrail policy {policy_id} — {message}"
            )));
        }
        Err(e) => {
            return Err(ApiError::Conflict(format!(
                "failed to dispatch pipeline on control plane: {e}"
            )));
        }
    };

    Ok(Json(DispatchProjectPipelineResponse {
        remote_project: link.remote_project,
        run_id,
    }))
}

/// `POST /api/projects/{id}/orch-dispatch` request body.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct DispatchProjectPipelineRequest {
    /// Opaque `{name: value}` object, forwarded to docket verbatim as the
    /// pipeline's `variables` — this route has no opinion on its shape
    /// beyond "a JSON object" and never logs it. Omit for `{}`.
    #[serde(default = "default_dispatch_variables")]
    pub variables: serde_json::Value,
}

fn default_dispatch_variables() -> serde_json::Value {
    serde_json::json!({})
}
