use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tack_api::handlers::websocket::BoardEvent;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::{AppState, LocalRunnerControl, config::AppConfig, router::build_router};
use tack_db::repo::execution::{ExecutionClock, RequestSelection};
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;

/// Build a fully wired Axum router backed by an in-memory SQLite database.
/// Returns the router and the test workspace ID.
#[allow(dead_code)] // not every test binary calls the no-config constructor directly
pub async fn test_app() -> (Router, Uuid) {
    test_app_with_config(AppConfig::default()).await
}

/// Same as `test_app` but with a custom config (e.g. to set an api_token).
pub async fn test_app_with_config(config: AppConfig) -> (Router, Uuid) {
    test_app_with_local_runner(config, None).await
}

/// Same as `test_app_with_config`, additionally wiring `local_runner` into
/// `AppState` — the seam a test of `handlers::local_runner`'s routes needs
/// to supply a fake `LocalRunnerControl` (or `None`, to prove those routes
/// stay absent even on a loopback bind when nothing was ever wired in).
#[allow(dead_code)] // not every test binary needs a non-default local_runner
pub async fn test_app_with_local_runner(
    config: AppConfig,
    local_runner: Option<Arc<dyn LocalRunnerControl>>,
) -> (Router, Uuid) {
    let repo = tack_test_support::setup_test_db().await;

    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'CI Workspace', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(repo.pool())
    .await
    .expect("insert workspace");

    let (tx, _rx) = broadcast::channel::<BoardEvent>(16);
    // Keep config.database_url in sync with the actual pool so backup/restore
    // handlers see the right path (or correctly detect in-memory).
    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        ..config
    };
    let state = AppState {
        repo,
        config,
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        orch_runtime: OrchRuntime::new(),
        local_runner,
    };

    (build_router(state), workspace_id)
}

/// Build a test app backed by a file-based SQLite database.
/// The caller supplies the full SQLite URL (e.g. `"sqlite:/tmp/test.db?mode=rwc"`).
/// Used for tests that require file-level operations (backup/restore).
#[allow(dead_code)] // not every test binary exercises file-backed databases
pub async fn test_app_with_file_db(db_url: &str) -> (Router, Uuid) {
    let pool = init_pool(db_url).await.expect("file-based pool");
    migrations::run_all(&pool).await.expect("migrations");

    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'File Workspace', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert workspace");

    let config = AppConfig {
        database_url: db_url.to_string(),
        ..AppConfig::default()
    };

    let (tx, _rx) = broadcast::channel::<BoardEvent>(16);
    let state = AppState {
        repo: Repository::new(pool),
        config,
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        orch_runtime: OrchRuntime::new(),
        local_runner: None,
    };

    (build_router(state), workspace_id)
}

/// Create a project via `POST /api/projects`; returns its id. Every duplicate this
/// replaces built this exact request (JSON `name`/`project_type` body,
/// `Content-Type: application/json`) by hand.
#[allow(dead_code)] // not every test binary creates projects through this route
pub async fn create_project(app: &Router, name: &str, project_type: &str) -> Uuid {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/projects")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({"name": name, "project_type": project_type}))
                        .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 4 * 1024 * 1024).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

/// Send a request, returning `(status, parsed JSON body)`. An empty or non-JSON body
/// parses to `Value::Null` rather than panicking. 8MB response cap.
#[allow(dead_code)] // not every test binary sends generic JSON requests
pub async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let (status, bytes) = send_bytes(
        app,
        method,
        uri,
        Body::from(body.to_string()),
        headers,
        8 * 1024 * 1024,
    )
    .await;
    (status, parse_or_null(&bytes))
}

/// Same as [`send`] but with a 64MB response cap, for routes returning large artifact
/// payloads.
#[allow(dead_code)] // only the artifact/chaos-recovery suites need the larger cap
pub async fn send_large(
    app: &Router,
    method: &str,
    uri: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let (status, bytes) = send_bytes(
        app,
        method,
        uri,
        Body::from(body.to_string()),
        headers,
        64 * 1_048_576,
    )
    .await;
    (status, parse_or_null(&bytes))
}

/// Same as [`send`], additionally returning the raw response body text.
#[allow(dead_code)] // only the attempt-list/attempt-scoping suites need the raw text
pub async fn send_with_raw(
    app: &Router,
    method: &str,
    uri: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> (StatusCode, Value, String) {
    let (status, bytes) = send_bytes(
        app,
        method,
        uri,
        Body::from(body.to_string()),
        headers,
        8 * 1024 * 1024,
    )
    .await;
    let raw = String::from_utf8_lossy(&bytes).into_owned();
    (status, parse_or_null(&bytes), raw)
}

/// POST-only `send`, panicking (instead of defaulting to `Value::Null`) when the response
/// body isn't valid JSON.
#[allow(dead_code)] // only the runner-protocol decisions suite wants strict parsing
pub async fn send_post_strict(
    app: &Router,
    uri: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let (status, bytes) = send_bytes(
        app,
        "POST",
        uri,
        Body::from(body.to_string()),
        headers,
        8 * 1024 * 1024,
    )
    .await;
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

/// `send` with a raw string body, panicking (instead of defaulting to `Value::Null`) when
/// the response body isn't valid JSON.
#[allow(dead_code)] // only the runner-protocol lifecycle suite wants strict parsing
pub async fn send_str_strict(
    app: &Router,
    method: &str,
    uri: &str,
    body: String,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let (status, bytes) =
        send_bytes(app, method, uri, Body::from(body), headers, 8 * 1024 * 1024).await;
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

/// Send a request as the given `x-tack-principal`, panicking if the response isn't valid
/// JSON. 128KB response cap — this route family's bodies are small.
#[allow(dead_code)] // only the executions/runner-admin suite impersonates a principal
pub async fn send_as(
    app: &Router,
    method: &str,
    uri: &str,
    body: String,
    principal: &str,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-tack-principal", principal)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let value =
        serde_json::from_slice(&to_bytes(response.into_body(), 131_072).await.unwrap()).unwrap();
    (status, value)
}

/// [`send_as`] with the default `"operator-1"` principal.
#[allow(dead_code)] // only the executions/runner-admin suite impersonates a principal
pub async fn send_as_operator(
    app: &Router,
    method: &str,
    uri: &str,
    body: String,
) -> (StatusCode, Value) {
    send_as(app, method, uri, body, "operator-1").await
}

#[allow(dead_code)] // exercised transitively by whichever `send*` variants a binary uses
async fn send_bytes(
    app: &Router,
    method: &str,
    uri: &str,
    body: Body,
    headers: &[(&str, &str)],
    limit: usize,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), limit).await.unwrap();
    (status, bytes.to_vec())
}

#[allow(dead_code)] // exercised transitively by whichever `send*` variants a binary uses
fn parse_or_null(bytes: &[u8]) -> Value {
    if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(bytes).unwrap_or(Value::Null)
    }
}

/// Runner-v1 `POST /api/runner/v1/claim` as `credential`, returning `(status, parsed JSON
/// body)`.
#[allow(dead_code)] // only the chaos-recovery suite drives the claim route directly
pub async fn claim_runner(
    app: &Router,
    runner_id: &str,
    credential: &str,
    claim_request_id: &str,
) -> (StatusCode, Value) {
    send_large(
        app,
        "POST",
        "/api/runner/v1/claim",
        json!({
            "protocol_version": 1, "runner_id": runner_id, "claim_request_id": claim_request_id,
            "available_capacity": 1, "wait_ms": 0,
        }),
        &[("authorization", &format!("Bearer {credential}"))],
    )
    .await
}

/// Claim the fixed `runner-crash`/`claim-crash`/`attempt-crash` execution directly at the
/// repository layer, bypassing HTTP — the runner-vertical-slice crash matrix's own subject
/// under test is the repository method, not a route.
#[allow(dead_code)] // only the runner-vertical-slice crash matrix claims at the repo layer
pub async fn claim_execution_for_test(repo: &Repository, clock: &dyn ExecutionClock) {
    repo.claim_execution_idempotent_with_snapshot(
        "runner-crash",
        "claim-crash",
        "attempt-crash",
        chrono::Duration::seconds(60),
        clock,
        RequestSelection::Naive,
    )
    .await
    .expect("claim query")
    .expect("lease");
}
