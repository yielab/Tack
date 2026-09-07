use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use serde_json::json;
use tracing::{info, instrument};

use crate::router::AppState;

/// GET /api/health — Liveness + readiness check
#[utoipa::path(
    get,
    path = "/api/health",
    tag = "system",
    responses((status = 200, description = "Service is live; reports version and applied migration count", body = serde_json::Value)),
)]
#[instrument(skip(state))]
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let migrations_applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations")
        .fetch_one(state.repo.pool())
        .await
        .unwrap_or(0);

    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "migrations_applied": migrations_applied,
    }))
}

/// GET /api/debug/info — System info (only in debug builds)
#[utoipa::path(
    get,
    path = "/api/debug/info",
    tag = "system",
    responses((status = 200, description = "Build, version, database size, and non-sensitive config", body = serde_json::Value)),
)]
#[instrument(skip(state))]
pub async fn debug_info(State(state): State<AppState>) -> impl IntoResponse {
    info!("Debug info requested");

    let db_size = get_db_size(&state).await;

    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "build": if cfg!(debug_assertions) { "debug" } else { "release" },
        "database": {
            "size_bytes": db_size,
            // `url` deliberately omitted — it can embed a filesystem path or
            // credentials and must not leak from an unauthenticated debug route.
        },
        "config": {
            "host": state.config.host,
            "port": state.config.port,
            "log_level": state.config.log_level,
        }
    }))
}

/// GET /api/debug/db-stats — Database statistics
#[utoipa::path(
    get,
    path = "/api/debug/db-stats",
    tag = "system",
    responses((status = 200, description = "Per-table row counts", body = serde_json::Value)),
)]
#[instrument(skip(state))]
pub async fn db_stats(State(state): State<AppState>) -> impl IntoResponse {
    let counts = get_table_counts(&state).await;

    Json(json!({
        "tables": counts,
    }))
}

async fn get_db_size(state: &AppState) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
    )
    .fetch_one(state.repo.pool())
    .await
    .unwrap_or(0)
}

async fn get_table_counts(state: &AppState) -> serde_json::Value {
    let tables = [
        "projects",
        "items",
        "sprints",
        "roles",
        "comments",
        "dependencies",
        "attachments",
    ];
    let mut counts = serde_json::Map::new();

    for table in tables {
        let query = format!("SELECT COUNT(*) FROM {table}");
        let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(query))
            .fetch_one(state.repo.pool())
            .await
            .unwrap_or(0);
        counts.insert(table.to_string(), json!(count));
    }

    serde_json::Value::Object(counts)
}
