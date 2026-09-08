//! App-level settings persisted in the `app_meta` key/value table.
//!
//! Holds two independent settings groups, both following the same shape —
//! a value stored in `app_meta` overrides an `TACK_*` environment default:
//!
//! - **Cloud backup** (S3-compatible configuration) — see
//!   [`effective_backup_config`].
//! - **Orchestration enable** — see
//!   [`effective_orch_enabled`]. This one additionally starts/stops the
//!   reconciler at runtime (`AppState::orch_runtime`) so the toggle takes
//!   effect without a server restart; the Cloud Backup precedent has no
//!   runtime side effect to manage, which is the one place these two groups'
//!   handlers genuinely differ.

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::instrument;
use validator::Validate;

use tack_orch::reconciler::ReconcilerConfig;

use crate::config::AppConfig;
use crate::error::{ApiError, ApiResult};
use crate::orch_store::build_control_plane_store;
use crate::router::AppState;

const BACKUP_KEY: &str = "backup_config";

/// Cloud-backup settings as stored in the DB. Every field is optional so a
/// partially-configured destination round-trips cleanly. The secret key is
/// stored here but never returned to clients.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BackupSettings {
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub access_key: Option<String>,
    #[serde(default)]
    pub secret_key: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub retention: Option<usize>,
}

async fn load(pool: &sqlx::SqlitePool) -> BackupSettings {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM app_meta WHERE key = ?")
        .bind(BACKUP_KEY)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

async fn save(pool: &sqlx::SqlitePool, settings: &BackupSettings) -> Result<(), sqlx::Error> {
    let value = serde_json::to_string(settings).unwrap_or_else(|_| "{}".into());
    sqlx::query(
        "INSERT INTO app_meta (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(BACKUP_KEY)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// The backup config the server should actually use: the env/startup [`AppConfig`]
/// with any UI-saved DB settings overlaid on top.
pub async fn effective_backup_config(state: &AppState) -> AppConfig {
    let mut cfg = state.config.clone();
    let s = load(state.pool()).await;

    let nonempty = |o: Option<String>| o.filter(|v| !v.trim().is_empty());

    if let Some(v) = nonempty(s.endpoint) {
        cfg.backup_endpoint = Some(v);
    }
    if let Some(v) = nonempty(s.bucket) {
        cfg.backup_bucket = Some(v);
    }
    if let Some(v) = nonempty(s.region) {
        cfg.backup_region = v;
    }
    if let Some(v) = nonempty(s.access_key) {
        cfg.backup_access_key = Some(v);
    }
    if let Some(v) = nonempty(s.secret_key) {
        cfg.backup_secret_key = Some(v);
    }
    if let Some(v) = nonempty(s.prefix) {
        cfg.backup_prefix = v;
    }
    if let Some(rt) = s.retention {
        cfg.backup_retention = rt;
    }
    cfg
}

/// Client-safe view of the effective config — the secret key is replaced by a
/// boolean flag and never sent to the browser.
fn public_view(cfg: &AppConfig) -> Value {
    json!({
        "configured": cfg.remote_backup_enabled(),
        "endpoint": cfg.backup_endpoint,
        "bucket": cfg.backup_bucket,
        "region": cfg.backup_region,
        "access_key": cfg.backup_access_key,
        "secret_key_set": cfg.backup_secret_key.is_some(),
        "prefix": cfg.backup_prefix,
        "retention": cfg.backup_retention,
    })
}

/// GET /api/settings/backup — current cloud-backup configuration (secret masked).
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/settings/backup",
    tag = "settings",
    responses(
        (status = 200, description = "Cloud-backup config (secret masked as `secret_key_set`)", body = serde_json::Value),
    ),
)]
pub async fn get_backup_settings(State(state): State<AppState>) -> Json<Value> {
    let cfg = effective_backup_config(&state).await;
    Json(public_view(&cfg))
}

/// Incoming update. Any omitted/blank string field clears that override and
/// falls back to the environment default. A blank `secret_key` keeps the
/// existing stored secret (so the masked UI field can be left untouched).
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct UpdateBackupSettings {
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub access_key: Option<String>,
    #[serde(default)]
    pub secret_key: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    /// Number of backups to keep. Must be at least 1 — `retention: 0` would
    /// prune the backup that was just created (and, when it is the only one,
    /// leave zero backups).
    #[serde(default)]
    #[validate(range(min = 1, message = "retention must be at least 1"))]
    pub retention: Option<usize>,
}

fn clean(o: Option<String>) -> Option<String> {
    o.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// PUT /api/settings/backup — save cloud-backup configuration.
#[instrument(skip(state, input))]
#[utoipa::path(
    put,
    path = "/api/settings/backup",
    tag = "settings",
    request_body = UpdateBackupSettings,
    responses(
        (status = 200, description = "Updated config (secret masked)", body = serde_json::Value),
        (status = 422, description = "Validation error", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn put_backup_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateBackupSettings>,
) -> ApiResult<Json<Value>> {
    input
        .validate()
        .map_err(|e| ApiError::Unprocessable(e.to_string()))?;

    let mut current = load(state.pool()).await;

    current.endpoint = clean(input.endpoint);
    current.bucket = clean(input.bucket);
    current.region = clean(input.region);
    current.access_key = clean(input.access_key);
    // Only replace the secret when a non-blank value is supplied; otherwise keep
    // whatever is stored (the UI sends the field blank when it is unchanged).
    if let Some(secret) = clean(input.secret_key) {
        current.secret_key = Some(secret);
    }
    current.prefix = clean(input.prefix);
    current.retention = input.retention;

    save(state.pool(), &current).await?;

    let cfg = effective_backup_config(&state).await;
    Ok(Json(public_view(&cfg)))
}

// ════════════════════════════════════════════════════════════════════════
// Orchestration enable
// ════════════════════════════════════════════════════════════════════════
//
// Replaces the old design where `TACK_ORCH_ENABLE` was the *only* switch —
// invisible from the UI, and requiring a restart to change. Follows the
// Cloud Backup precedent above exactly: an `app_meta`-stored value
// overrides the env default. The one addition specific to this setting is
// that flipping it also starts or stops the reconciler
// (`AppState::orch_runtime`, see `orch_runtime.rs`'s module doc for the
// start/stop design) — Cloud Backup has no equivalent background task to
// manage.
//
// `enabled` is `Option<bool>`, not `bool`, so the stored row can distinguish
// "never touched by the UI" (`None` ⇒ env default wins, `source:
// "env_default"`) from "explicitly set, even if set back to `false`"
// (`Some(_)` ⇒ `source: "database"`). A plain `bool` with `#[serde(default)]`
// can't make that distinction — every missing/absent field would silently
// mean `false`, indistinguishable from an explicit off.

const ORCH_KEY: &str = "orch_config";

#[derive(Debug, Default, Serialize, Deserialize)]
struct OrchSettings {
    #[serde(default)]
    enabled: Option<bool>,
}

async fn load_orch(pool: &sqlx::SqlitePool) -> OrchSettings {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM app_meta WHERE key = ?")
        .bind(ORCH_KEY)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

async fn save_orch(pool: &sqlx::SqlitePool, settings: &OrchSettings) -> Result<(), sqlx::Error> {
    let value = serde_json::to_string(settings).unwrap_or_else(|_| "{}".into());
    sqlx::query(
        "INSERT INTO app_meta (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(ORCH_KEY)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// The orchestration enable flag Tack should actually use right now: the
/// `app_meta`-stored value if the UI has ever set one, else
/// `TACK_ORCH_ENABLE`'s startup value. This is the single source of truth
/// both [`orch::require_orch_enabled`](crate::handlers::orch::require_orch_enabled)
/// (the route gate) and `server.rs`'s boot path consult — a request-time DB
/// read, same cost class as the existing Bearer-token check, so the gate
/// stays correct even if a toggle happened without a restart.
pub async fn effective_orch_enabled(state: &AppState) -> bool {
    effective_orch_enabled_for(state.pool(), state.config.orch_enable).await
}

/// Same result as [`effective_orch_enabled`], for a caller that has a pool and
/// its own env-default but not a full [`AppState`] — `router.rs` closes over
/// `AppState::config.orch_enable` and wires this function into
/// `handlers::executions::OperatorExecutionState::orchestration_enabled`, the
/// callback `create_execution`'s dual-scheduling guard resolves against per
/// request. Both `effective_orch_enabled` and this function read the one
/// `app_meta` row through [`load_orch`]; there is no second parser of that
/// JSON anywhere.
pub(crate) async fn effective_orch_enabled_for(pool: &sqlx::SqlitePool, env_default: bool) -> bool {
    load_orch(pool).await.enabled.unwrap_or(env_default)
}

/// Full effective view shared by `GET` and `PUT` — both return exactly the
/// same shape, computed fresh each time (never cached), so a client that
/// polls right after a `PUT` always sees the settings it just changed.
async fn orch_settings_view(state: &AppState) -> Value {
    let stored = load_orch(state.pool()).await;
    let env_default = state.config.orch_enable;
    let enabled = stored.enabled.unwrap_or(env_default);
    let source = if stored.enabled.is_some() {
        "database"
    } else {
        "env_default"
    };

    let reconciler_running = state.orch_runtime.live_task_count().await > 0;
    let control_plane_count = state
        .repo
        .list_control_planes()
        .await
        .map(|planes| planes.len() as i64)
        .unwrap_or(0);
    let linked_project_count = state.repo.count_orch_links().await.unwrap_or(0);

    json!({
        "enabled": enabled,
        "source": source,
        "reconciler_running": reconciler_running,
        "control_plane_count": control_plane_count,
        "linked_project_count": linked_project_count,
        "poll_secs": state.config.orch_poll_secs,
        // Never the token value — see `AppConfig::orch_approval_token`'s own
        // doc comment on why it's never logged either.
        "approval_token_set": state.config.orch_approval_token.is_some(),
        "env_default": env_default,
    })
}

/// GET /api/settings/orchestration — current orchestration settings.
///
/// Deliberately reachable regardless of whether orchestration is enabled
/// (registered outside `orch_routes`' gate in `router.rs`) — a UI on a
/// server where orchestration has never been turned on must still be able
/// to read this and offer to turn it on. See `router.rs`'s route comment.
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/settings/orchestration",
    tag = "settings",
    responses(
        (status = 200, description = "Orchestration settings: effective enabled flag, where it came from, and reconciler/link counts", body = serde_json::Value),
    ),
)]
pub async fn get_orch_settings(State(state): State<AppState>) -> Json<Value> {
    Json(orch_settings_view(&state).await)
}

/// Incoming update — just the one field the contract defines. No tri-state
/// "unset back to env default" path exists yet; once stored, `source` stays
/// `"database"` permanently, mirroring how Cloud Backup's string fields
/// already behave for their own overrides.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateOrchSettings {
    pub enabled: bool,
}

/// PUT /api/settings/orchestration — save the orchestration enable flag and
/// start/stop the reconciler to match, immediately, without a restart.
///
/// Order matters: persist first, then reconcile the runtime. If the process
/// restarts between the two (crash, deploy), the stored value is already
/// correct and the next boot's `server.rs` picks it up — the only thing lost
/// is this one instant's runtime state, not the setting itself.
#[instrument(skip(state))]
#[utoipa::path(
    put,
    path = "/api/settings/orchestration",
    tag = "settings",
    request_body = UpdateOrchSettings,
    responses(
        (status = 200, description = "Updated orchestration settings (same shape as GET)", body = serde_json::Value),
    ),
)]
pub async fn put_orch_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateOrchSettings>,
) -> ApiResult<Json<Value>> {
    save_orch(
        state.pool(),
        &OrchSettings {
            enabled: Some(input.enabled),
        },
    )
    .await?;

    if input.enabled {
        let store = build_control_plane_store(&state);
        state
            .orch_runtime
            .start(
                store,
                ReconcilerConfig {
                    poll_secs: state.config.orch_poll_secs,
                    event_retention_days: state.config.orch_event_retention_days,
                    ..Default::default()
                },
            )
            .await;
    } else {
        state.orch_runtime.stop().await;
    }

    Ok(Json(orch_settings_view(&state).await))
}
