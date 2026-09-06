//! Turns the embedded runner on/off from the UI and lets a UI-only user hand
//! it a provider key — ADR 0061 decisions 2 and 6.
//!
//! Before this, the embedded runner started only from a boot-time flag
//! (`tack serve --with-runner`/`TACK_LOCAL_RUNNER_ENABLE`) and a provider
//! key could only be set on the runner's own machine via the console
//! (`tack runner secret set`). Both remain true — this module adds a second
//! way to reach the exact same on/off gate and the exact same secret
//! store, from the UI, when the operator and the runner share a machine.
//!
//! **This crate never depends on `tack-runner`** (`CLAUDE.md`'s crate map).
//! [`LocalRunnerControl`] is the seam: a trait this crate defines and calls,
//! implemented by whichever binary actually wires an embedded runner in
//! (`tack-cli`'s `local_runner` module). `AppState::local_runner` is `None`
//! for any caller that never wired one in (a bare `tack_api::serve()`, or a
//! test) — every route in this module treats that exactly like a
//! non-loopback bind: absent, not refusing (see `router.rs`'s
//! `local_runner_routes`).
//!
//! Follows the Cloud Backup / Orchestration precedent in `handlers/
//! settings.rs` for the persisted on/off flag: an `app_meta`-stored value
//! overrides the env/CLI-flag default, computed fresh on every read, never
//! cached. Unlike those two, the *secret* half of this module never touches
//! `app_meta` at all — a value handed to [`LocalRunnerControl::set_secret`]
//! goes straight to wherever the concrete implementation's own store lives
//! (the runner's OS keychain or its owner-only file) and this crate never
//! learns which.

use std::sync::Arc;

use async_trait::async_trait;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::instrument;

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

const LOCAL_RUNNER_KEY: &str = "local_runner_config";

#[derive(Debug, Default, Serialize, Deserialize)]
struct LocalRunnerSettings {
    #[serde(default)]
    enabled: Option<bool>,
}

async fn load(pool: &sqlx::SqlitePool) -> LocalRunnerSettings {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM app_meta WHERE key = ?")
        .bind(LOCAL_RUNNER_KEY)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

async fn save(pool: &sqlx::SqlitePool, settings: &LocalRunnerSettings) -> Result<(), sqlx::Error> {
    let value = serde_json::to_string(settings).unwrap_or_else(|_| "{}".into());
    sqlx::query(
        "INSERT INTO app_meta (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(LOCAL_RUNNER_KEY)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// The on/off preference Tack should actually use right now: the
/// `app_meta`-stored value if the UI has ever set one, else the env/CLI-flag
/// default (`--with-runner`/`TACK_LOCAL_RUNNER_ENABLE`, folded into
/// `AppConfig::local_runner_enable` at load time) — the same precedence
/// `effective_orch_enabled` already established. Read fresh on every call,
/// never cached, so a toggle a moment ago is always reflected.
pub async fn effective_local_runner_enabled(state: &AppState) -> bool {
    load(state.pool())
        .await
        .enabled
        .unwrap_or(state.config.local_runner_enable)
}

/// The embedded runner's actual runtime state — never the persisted
/// preference above. A preference can read "on" for one instant after boot,
/// before the auto-start task (`server.rs`) has actually run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Stopped,
    Running,
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeStatus {
    pub state: RuntimeState,
    pub since: Option<DateTime<Utc>>,
}

/// What asking the runner's own configured provider for its model catalog
/// produced. Deliberately not the real provider crate's own catalog-status
/// type — this crate never depends on `tack-runner`; the concrete
/// [`LocalRunnerControl`] translates its real type into this one at the
/// boundary, the same way `AppState::local_runner` itself crosses it.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CatalogSnapshot {
    NotConfigured,
    SecretUnresolved,
    Unreachable {
        http_status: Option<u16>,
    },
    Configured {
        model_count: usize,
        checked_at: DateTime<Utc>,
    },
}

/// One stored secret's name and when it was last set — never its value.
#[derive(Debug, Clone, Serialize)]
pub struct SecretMeta {
    pub name: String,
    /// `None` when the name is present in the store but this process has no
    /// record of when it was set (e.g. it was written by `tack runner
    /// secret set` before this UI ever ran) — never a fabricated timestamp.
    pub set_at: Option<DateTime<Utc>>,
}

/// Every variant names an operation and, where the backend gave one, its own
/// reason — never a secret value. Safe to log or fold into an [`ApiError`].
#[derive(Debug, thiserror::Error)]
pub enum LocalRunnerControlError {
    #[error("embedded runner failed to start: {0}")]
    StartFailed(String),
    #[error("secret store operation failed: {0}")]
    SecretStore(String),
}

impl From<LocalRunnerControlError> for ApiError {
    fn from(error: LocalRunnerControlError) -> Self {
        ApiError::Internal(anyhow::anyhow!(error.to_string()))
    }
}

/// The seam this crate calls to control an embedded runner without ever
/// depending on `tack-runner` or learning where its secret store lives.
/// `crates/tack-cli/src/local_runner.rs` is the only implementation today —
/// see that module's doc comment for how it composes this with
/// `tack_runner::bootstrap`.
#[async_trait]
pub trait LocalRunnerControl: Send + Sync {
    /// Whether the embedded runner is actually alive right now, and since
    /// when. Never the persisted preference — see [`RuntimeStatus`]'s doc.
    async fn status(&self) -> RuntimeStatus;

    /// Starts the embedded runner if it is not already running. A no-op,
    /// not an error, if it is (mirrors `OrchRuntime::start`'s idempotency —
    /// `orch_runtime.rs`).
    async fn start(&self) -> Result<(), LocalRunnerControlError>;

    /// Stops the embedded runner if running. A no-op otherwise.
    async fn stop(&self);

    /// Names and set-at timestamps of every stored secret. Never values.
    async fn list_secrets(&self) -> Vec<SecretMeta>;

    /// Stores `value` under `name`, overwriting any existing entry. A
    /// freshly configured provider's catalog is visible on the very next
    /// [`LocalRunnerControl::catalog`] call. If the runner is running and
    /// `name` is the entry a configured provider resolves its credential
    /// from, the runner is restarted before this returns: it received its
    /// configuration by value when it was spawned, so the only way the
    /// next dispatch can use this key is a runner that booted with it. A
    /// restart that fails leaves the runner stopped and returns the error,
    /// never a runner that still serves the old configuration.
    async fn set_secret(&self, name: &str, value: &str) -> Result<(), LocalRunnerControlError>;

    /// Removes the secret named `name`. Not an error if it was already
    /// absent (matches `rm -f`). Restarts a running runner under the same
    /// rule as [`LocalRunnerControl::set_secret`], in the other direction.
    async fn remove_secret(&self, name: &str) -> Result<(), LocalRunnerControlError>;

    /// What the configured provider's catalog looks like right now.
    /// Computed fresh on every call — never cached — the same "never
    /// cached" rule `orch_settings_view` documents for its own status view,
    /// so this is always the re-probe the card asks for; there is nothing
    /// else to invalidate.
    async fn catalog(&self) -> CatalogSnapshot;
}

fn require_control(state: &AppState) -> ApiResult<Arc<dyn LocalRunnerControl>> {
    state
        .local_runner
        .clone()
        .ok_or_else(|| ApiError::NotFound("local runner control is not available".into()))
}

fn runtime_state_str(state: RuntimeState) -> &'static str {
    match state {
        RuntimeState::Running => "running",
        RuntimeState::Stopped => "stopped",
    }
}

fn catalog_json(catalog: &CatalogSnapshot) -> Value {
    serde_json::to_value(catalog).unwrap_or(Value::Null)
}

/// GET /api/local-runner — the persisted preference, the live runtime
/// state, and the current provider catalog. Only mounted on a loopback bind
/// with a control actually wired in — see `router.rs`'s `local_runner_routes`.
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/local-runner",
    tag = "local-runner",
    responses(
        (status = 200, description = "Embedded-runner preference, runtime state, and provider catalog", body = serde_json::Value),
    ),
)]
pub async fn get_local_runner(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let control = require_control(&state)?;
    let enabled = effective_local_runner_enabled(&state).await;
    let status = control.status().await;
    let catalog = control.catalog().await;
    Ok(Json(json!({
        "enabled": enabled,
        "state": runtime_state_str(status.state),
        "since": status.since,
        "catalog": catalog_json(&catalog),
    })))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateLocalRunner {
    pub enabled: bool,
}

/// PUT /api/local-runner — save the preference and start/stop the embedded
/// runner to match, immediately, with no restart. Persist first, then
/// reconcile the runtime — same ordering `put_orch_settings` uses and for
/// the same reason: a crash between the two still boots correctly next time.
#[instrument(skip(state))]
#[utoipa::path(
    put,
    path = "/api/local-runner",
    tag = "local-runner",
    request_body = UpdateLocalRunner,
    responses((status = 204, description = "Preference saved and the runtime reconciled to match")),
)]
pub async fn put_local_runner(
    State(state): State<AppState>,
    Json(input): Json<UpdateLocalRunner>,
) -> ApiResult<StatusCode> {
    let control = require_control(&state)?;
    save(
        state.pool(),
        &LocalRunnerSettings {
            enabled: Some(input.enabled),
        },
    )
    .await?;

    if input.enabled {
        control.start().await?;
    } else {
        control.stop().await;
    }

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/local-runner/secrets — names and set-at timestamps only.
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/local-runner/secrets",
    tag = "local-runner",
    responses((status = 200, description = "Stored secret names and set-at timestamps, never values", body = serde_json::Value)),
)]
pub async fn list_local_runner_secrets(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let control = require_control(&state)?;
    let secrets = control.list_secrets().await;
    Ok(Json(json!({
        "data": secrets
            .into_iter()
            .map(|s| json!({ "name": s.name, "set_at": s.set_at }))
            .collect::<Vec<_>>(),
    })))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetLocalRunnerSecret {
    pub value: String,
}

/// PUT /api/local-runner/secrets/{name} — store a value. Never echoes it
/// back, not even as a hash: the response carries nothing but a status code.
#[instrument(skip(state, input))]
#[utoipa::path(
    put,
    path = "/api/local-runner/secrets/{name}",
    tag = "local-runner",
    request_body = SetLocalRunnerSecret,
    responses((status = 204, description = "Stored; the value is never echoed back")),
)]
pub async fn put_local_runner_secret(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(input): Json<SetLocalRunnerSecret>,
) -> ApiResult<StatusCode> {
    let control = require_control(&state)?;
    control.set_secret(&name, &input.value).await?;
    // `catalog()` always computes fresh (see its own doc comment), so
    // calling it here — even though this route discards the result — is
    // what makes "set a key" and "the catalog reflects it" happen inside
    // one request, rather than leaving it to whichever GET happens next.
    let _ = control.catalog().await;
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/local-runner/secrets/{name} — not an error if already absent.
#[instrument(skip(state))]
#[utoipa::path(
    delete,
    path = "/api/local-runner/secrets/{name}",
    tag = "local-runner",
    responses((status = 204, description = "Removed (or already absent)")),
)]
pub async fn delete_local_runner_secret(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    let control = require_control(&state)?;
    control.remove_secret(&name).await?;
    Ok(StatusCode::NO_CONTENT)
}
