use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{StatusCode, header},
    response::Response,
};
use serde::Deserialize;
use serde_json::json;
use tracing::instrument;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::remote_backup;
use crate::router::AppState;

/// GET /api/backup — VACUUM INTO snapshot streamed as application/octet-stream.
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/backup",
    tag = "backup",
    responses(
        (status = 200, description = "SQLite snapshot (secrets scrubbed)", content_type = "application/octet-stream"),
        (status = 400, description = "Not a file-based database", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_backup(State(state): State<AppState>) -> ApiResult<Response> {
    let db_path = state
        .config
        .db_file_path()
        .ok_or_else(|| ApiError::BadRequest("backup requires a file-based database".into()))?;

    // Checkpoint WAL into the main file so the snapshot is current.
    sqlx::query("PRAGMA wal_checkpoint(FULL)")
        .execute(state.pool())
        .await?;

    let temp_path = std::env::temp_dir().join(format!("tack-backup-{}.db", Uuid::new_v4()));
    let path_str = temp_path.to_string_lossy().replace('\'', "''");

    sqlx::query(sqlx::AssertSqlSafe(format!("VACUUM INTO '{path_str}'")))
        .execute(state.pool())
        .await?;

    // From here the snapshot exists and still holds every secret the live
    // database does, so no path may return without removing it. Strip the S3
    // secret and install identity so they never leave the machine inside a
    // backup file.
    let scrubbed: Result<Vec<u8>, ApiError> = async {
        remote_backup::scrub_snapshot_secrets(&temp_path)
            .await
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("{e}")))?;
        tokio::fs::read(&temp_path)
            .await
            .map_err(|e| ApiError::Internal(e.into()))
    }
    .await;
    let _ = tokio::fs::remove_file(&temp_path).await;

    let bytes = scrubbed?;

    let filename = db_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("tack-backup.db");

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

/// POST /api/restore — Validate a SQLite backup and stage it for the next restart.
///
/// The file is written as `<db-path>.restore`. On the next server startup,
/// `main.rs` applies the staged restore automatically.
#[instrument(skip(state, body))]
#[utoipa::path(
    post,
    path = "/api/restore",
    tag = "backup",
    request_body(content = String, content_type = "application/octet-stream", description = "A SQLite database file to stage for restore"),
    responses(
        (status = 200, description = "Restore staged for next restart", body = serde_json::Value),
        (status = 400, description = "Not a valid SQLite file", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "Uploaded schema is newer than this binary", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn post_restore(
    State(state): State<AppState>,
    body: Bytes,
) -> ApiResult<Json<serde_json::Value>> {
    const SQLITE_MAGIC: &[u8] = b"SQLite format 3\x00";
    if body.len() < 16 || &body[..16] != SQLITE_MAGIC {
        return Err(ApiError::BadRequest(
            "not a valid SQLite database file".into(),
        ));
    }

    let db_path = state
        .config
        .db_file_path()
        .ok_or_else(|| ApiError::BadRequest("restore requires a file-based database".into()))?;

    // Migration-version guard (parity with the remote path): a restore of a
    // newer-schema DB bricks startup, so reject it here rather than after the
    // swap. Count `_migrations` rows in the uploaded file.
    let uploaded_version = migration_count_of_bytes(&body)
        .await
        .map_err(|e| ApiError::BadRequest(format!("could not read uploaded database: {e}")))?;
    let local_version: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations")
        .fetch_one(state.pool())
        .await?;
    if uploaded_version > local_version as u32 {
        return Err(ApiError::Conflict(format!(
            "restore rejected: uploaded database schema ({uploaded_version}) is ahead of the running binary ({local_version}); upgrade Tack before restoring"
        )));
    }

    // Backup before restore: best-effort remote snapshot of the current state.
    let cfg = crate::handlers::settings::effective_backup_config(&state).await;
    if cfg.remote_backup_enabled()
        && let Ok(store) = remote_backup::store_from_config(&cfg)
        && let Err(e) =
            remote_backup::perform_backup(state.pool(), &cfg, store.as_ref(), true).await
    {
        tracing::warn!(error = %e, "backup-before-restore failed; continuing with local restore");
    }

    let restore_path = format!("{}.restore", db_path.to_string_lossy());

    tokio::fs::write(&restore_path, &body)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    Ok(Json(json!({
        "status": "staged",
        "restore_path": restore_path,
        "message": "Restore staged. Stop the server and restart to apply."
    })))
}

/// Count `_migrations` rows in a raw SQLite database byte buffer by opening it
/// read-only from a temp file. Used to guard local restores against a newer schema.
async fn migration_count_of_bytes(bytes: &[u8]) -> Result<u32, String> {
    use sqlx::ConnectOptions;
    use sqlx::Connection;
    use sqlx::sqlite::SqliteConnectOptions;

    let tmp = std::env::temp_dir().join(format!("tack-restore-check-{}.db", Uuid::new_v4()));
    tokio::fs::write(&tmp, bytes)
        .await
        .map_err(|e| e.to_string())?;

    let result = async {
        let mut conn = SqliteConnectOptions::new()
            .filename(&tmp)
            .create_if_missing(false)
            .read_only(true)
            .connect()
            .await
            .map_err(|e| e.to_string())?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations")
            .fetch_one(&mut conn)
            .await
            .map_err(|e| e.to_string())?;
        conn.close().await.ok();
        Ok::<u32, String>(count as u32)
    }
    .await;

    let _ = tokio::fs::remove_file(&tmp).await;
    result
}

// ── Remote backup endpoints ────────────────────────────────────────────────────

fn remote_not_configured() -> ApiError {
    ApiError::Conflict(
        "remote backup is not configured — set TACK_BACKUP_BUCKET, TACK_BACKUP_ACCESS_KEY, TACK_BACKUP_SECRET_KEY"
            .into(),
    )
}

fn backup_err(e: remote_backup::BackupError) -> ApiError {
    use remote_backup::BackupError as B;
    match &e {
        // Business-rule conflicts (409). The generation conflict does not
        // surface the remote head manifest inline; that structured payload is
        // dropped when unifying on the error envelope, since no client consumes it.
        B::NotConfigured
        | B::SchemaTooNew { .. }
        | B::UnsupportedFormat(_)
        | B::IntegrityMismatch
        | B::RestoreWouldLoseWork { .. }
        | B::GenerationConflict { .. } => ApiError::Conflict(e.to_string()),
        B::UnsafePath(_) | B::CorruptBundle | B::InMemoryDb => ApiError::BadRequest(e.to_string()),
        _ => {
            tracing::error!(error = %e, "remote backup error");
            ApiError::Internal(anyhow::anyhow!("{e}"))
        }
    }
}

/// Parse an optional `{"force": true}` flag from a request body that may be
/// empty or contain other fields.
fn parse_force(raw_body: &Bytes) -> bool {
    if raw_body.is_empty() {
        return false;
    }
    serde_json::from_slice::<serde_json::Value>(raw_body)
        .ok()
        .and_then(|v| v.get("force").and_then(|f| f.as_bool()))
        .unwrap_or(false)
}

/// POST /api/backup/remote — create a bundle and upload it to the configured S3
/// store. Bumps the sync generation and rejects with 409 when another device
/// has uploaded newer work, unless the request body carries `{"force": true}`.
#[instrument(skip(state, raw_body))]
#[utoipa::path(
    post,
    path = "/api/backup/remote",
    tag = "backup",
    request_body(content = serde_json::Value, description = "Optional `{ \"force\": true }` to override the newer-work guard"),
    responses(
        (status = 200, description = "Backup manifest", body = serde_json::Value),
        (status = 409, description = "Not configured, or another device has newer work", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn post_remote_backup(
    State(state): State<AppState>,
    raw_body: Bytes,
) -> ApiResult<Json<serde_json::Value>> {
    let cfg = crate::handlers::settings::effective_backup_config(&state).await;
    if !cfg.remote_backup_enabled() {
        return Err(remote_not_configured());
    }

    let force = parse_force(&raw_body);
    let store = remote_backup::store_from_config(&cfg).map_err(backup_err)?;

    let manifest = remote_backup::perform_backup(state.pool(), &cfg, store.as_ref(), force)
        .await
        .map_err(backup_err)?;

    Ok(Json(serde_json::to_value(&manifest).unwrap()))
}

/// GET /api/backup/remote — list remote backups newest-first.
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/backup/remote",
    tag = "backup",
    responses(
        (status = 200, description = "Remote backup manifests, newest first", body = serde_json::Value),
        (status = 409, description = "Remote backup not configured", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_remote_backups(
    State(state): State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    let cfg = crate::handlers::settings::effective_backup_config(&state).await;
    if !cfg.remote_backup_enabled() {
        return Err(remote_not_configured());
    }

    let store = remote_backup::store_from_config(&cfg).map_err(backup_err)?;
    let manifests = remote_backup::list(store.as_ref(), &cfg.backup_prefix)
        .await
        .map_err(backup_err)?;

    Ok(Json(serde_json::to_value(&manifests).unwrap()))
}

#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct RestoreRemoteRequest {
    /// Object key to restore. Defaults to the latest backup when omitted.
    #[serde(default)]
    pub key: Option<String>,
    /// Override the "this device has newer work" guard.
    #[serde(default)]
    pub force: bool,
}

/// POST /api/backup/remote/restore — download a bundle and stage it for next restart.
#[instrument(skip(state, raw_body))]
#[utoipa::path(
    post,
    path = "/api/backup/remote/restore",
    tag = "backup",
    request_body = RestoreRemoteRequest,
    responses(
        (status = 200, description = "Restore staged for next restart", body = serde_json::Value),
        (status = 404, description = "No remote backups found", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "Not configured, or restore would lose newer work", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn post_remote_restore(
    State(state): State<AppState>,
    raw_body: Bytes,
) -> ApiResult<Json<serde_json::Value>> {
    let cfg = crate::handlers::settings::effective_backup_config(&state).await;
    if !cfg.remote_backup_enabled() {
        return Err(remote_not_configured());
    }

    let store = remote_backup::store_from_config(&cfg).map_err(backup_err)?;

    let req: RestoreRemoteRequest = if raw_body.is_empty() {
        RestoreRemoteRequest::default()
    } else {
        serde_json::from_slice(&raw_body).unwrap_or_default()
    };
    let force = req.force;

    // Resolve key: use provided key or pick the latest.
    let key = if let Some(k) = req.key.clone() {
        k
    } else {
        let manifests = remote_backup::list(store.as_ref(), &cfg.backup_prefix)
            .await
            .map_err(backup_err)?;
        manifests
            .into_iter()
            .next()
            .ok_or_else(|| ApiError::NotFound("no remote backups found".into()))?
            .object_key
    };

    let bundle_bytes = remote_backup::download(store.as_ref(), &key)
        .await
        .map_err(backup_err)?;

    // Parse the sidecar manifest to get migration_version + generation.
    let sidecar_key = format!("{}.manifest.json", key);
    let sidecar_bytes = remote_backup::download(store.as_ref(), &sidecar_key)
        .await
        .map_err(backup_err)?;
    let manifest: remote_backup::BackupManifest =
        serde_json::from_slice(&sidecar_bytes).map_err(|e| backup_err(e.into()))?;

    // Conflict guard: refuse to clobber newer local work unless forced.
    let local_gen = remote_backup::generation(state.pool())
        .await
        .map_err(backup_err)?;
    if remote_backup::restore_conflicts(local_gen, manifest.generation, force) {
        return Err(backup_err(
            remote_backup::BackupError::RestoreWouldLoseWork {
                local_generation: local_gen,
                snapshot_generation: manifest.generation,
            },
        ));
    }

    // Get the current local migration version.
    let local_version: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations")
        .fetch_one(state.pool())
        .await
        .map_err(|e| backup_err(e.into()))?;

    let db_path = cfg
        .db_file_path()
        .ok_or_else(|| ApiError::BadRequest("restore requires a file-based database".into()))?;

    // Backup before restore: best-effort snapshot of the *current* state to the
    // remote so a mistaken restore is recoverable. Never blocks the restore.
    if let Err(e) = remote_backup::perform_backup(state.pool(), &cfg, store.as_ref(), true).await {
        tracing::warn!(error = %e, "backup-before-restore failed; continuing with restore");
    }

    remote_backup::stage_restore(
        bundle_bytes,
        &manifest,
        local_version as u32,
        &db_path,
        &cfg.storage_dir,
    )
    .await
    .map_err(backup_err)?;

    Ok(Json(json!({
        "staged": true,
        "restart_required": true,
        "object_key": key,
        "message": "Restore staged. Restart the server to apply."
    })))
}

/// POST /api/backup/remote/verify — download a bundle and validate it (sha256 +
/// format + schema version) WITHOUT staging anything. Returns the manifest and
/// an `ok`/mismatch verdict so the UI can preview a restore safely.
#[instrument(skip(state, raw_body))]
#[utoipa::path(
    post,
    path = "/api/backup/remote/verify",
    tag = "backup",
    request_body = RestoreRemoteRequest,
    responses(
        (status = 200, description = "Verification verdict plus the manifest", body = serde_json::Value),
        (status = 404, description = "No remote backups found", body = crate::openapi::ErrorEnvelope),
        (status = 409, description = "Remote backup not configured", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn post_remote_verify(
    State(state): State<AppState>,
    raw_body: Bytes,
) -> ApiResult<Json<serde_json::Value>> {
    let cfg = crate::handlers::settings::effective_backup_config(&state).await;
    if !cfg.remote_backup_enabled() {
        return Err(remote_not_configured());
    }

    let store = remote_backup::store_from_config(&cfg).map_err(backup_err)?;

    let req_key: Option<String> = if raw_body.is_empty() {
        None
    } else {
        serde_json::from_slice::<RestoreRemoteRequest>(&raw_body)
            .ok()
            .and_then(|r| r.key)
    };

    let key = if let Some(k) = req_key {
        k
    } else {
        let manifests = remote_backup::list(store.as_ref(), &cfg.backup_prefix)
            .await
            .map_err(backup_err)?;
        manifests
            .into_iter()
            .next()
            .ok_or_else(|| ApiError::NotFound("no remote backups found".into()))?
            .object_key
    };

    let sidecar_key = format!("{}.manifest.json", key);
    let sidecar_bytes = remote_backup::download(store.as_ref(), &sidecar_key)
        .await
        .map_err(backup_err)?;
    let manifest: remote_backup::BackupManifest =
        serde_json::from_slice(&sidecar_bytes).map_err(|e| backup_err(e.into()))?;

    let bundle_bytes = remote_backup::download(store.as_ref(), &key)
        .await
        .map_err(backup_err)?;

    let local_version: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations")
        .fetch_one(state.pool())
        .await
        .map_err(|e| backup_err(e.into()))?;

    match remote_backup::verify_bundle(bundle_bytes, &manifest, local_version as u32).await {
        Ok(()) => Ok(Json(json!({
            "ok": true,
            "object_key": key,
            "manifest": manifest,
        }))),
        Err(e) => Ok(Json(json!({
            "ok": false,
            "object_key": key,
            "manifest": manifest,
            "reason": e.to_string(),
        }))),
    }
}
