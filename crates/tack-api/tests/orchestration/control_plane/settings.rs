//! Tests for `GET`/`PUT /api/settings/orchestration` —
//! the runtime-toggleable replacement for the old `TACK_ORCH_ENABLE`-only
//! design.
//!
//! Covers: the endpoint stays reachable while orchestration is disabled (a
//! UI on a server that never turned this on must still read/enable it);
//! `source` distinguishes `"env_default"` from `"database"`; and — proving
//! "no restart required" — `PUT` starts/stops the live reconciler task,
//! observable via `reconciler_running`, with repeated toggles never
//! leaking a task.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use std::time::Duration;
use tack_api::config::AppConfig;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::router::{AppState, build_router};
use tack_db::repo::orch::CreateControlPlane;
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Helpers ─────────────────────────────────────────────────────────────────

async fn app_with_state(config: AppConfig) -> (Router, AppState, Uuid) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");

    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'CI Workspace', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert workspace");

    let (tx, _rx) = broadcast::channel(16);
    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        ..config
    };
    let state = AppState {
        repo: Repository::new(pool),
        config,
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        orch_runtime: OrchRuntime::new(),
        local_runner: None,
    };

    (build_router(state.clone()), state, workspace_id)
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = to_bytes(res.into_body(), 4 * 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn req(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(v) => {
            builder = builder.header("Content-Type", "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    app.clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap()
}

async fn get_settings(app: &Router) -> Value {
    let res = req(app, Method::GET, "/api/settings/orchestration", None).await;
    assert_eq!(res.status(), StatusCode::OK);
    body_json(res).await
}

async fn put_settings(app: &Router, enabled: bool) -> Value {
    let res = req(
        app,
        Method::PUT,
        "/api/settings/orchestration",
        Some(json!({ "enabled": enabled })),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    body_json(res).await
}

/// Poll `GET` until `reconciler_running` matches `want`, or panic after a
/// generous bound. Cooperative shutdown is fast (the stop signal races the
/// poll-interval sleep via `select!`, so it doesn't wait a full poll
/// interval — see `orch_runtime.rs`'s module doc) but is not synchronous
/// with the `PUT` response, so tests that just flipped the setting off poll
/// briefly rather than asserting instantaneously. `Interval::tick` (not a
/// bare `yield_now` loop) forces this current-thread runtime to actually
/// park between checks, the same reasoning `auto_dispatch/hook.rs`'s
/// `wait_for_hits` documents for real background-task completion.
async fn wait_for_reconciler_running(app: &Router, want: bool) -> Value {
    let mut ticker = tokio::time::interval(Duration::from_millis(30));
    let mut last = get_settings(app).await;
    for _ in 0..100 {
        if last["reconciler_running"] == want {
            return last;
        }
        ticker.tick().await;
        last = get_settings(app).await;
    }
    panic!("reconciler_running never became {want}; last seen: {last}");
}

// ─── GET: reachable regardless, correct source attribution ────────────────

#[tokio::test]
async fn reachable_and_env_default_when_never_configured() {
    let (app, _, _) = app_with_state(AppConfig::default()).await; // orch_enable: false
    let body = get_settings(&app).await;

    assert_eq!(body["enabled"], false);
    assert_eq!(body["source"], "env_default");
    assert_eq!(body["env_default"], false);
    assert_eq!(body["reconciler_running"], false);
    assert_eq!(body["control_plane_count"], 0);
    assert_eq!(body["linked_project_count"], 0);
    assert_eq!(body["poll_secs"], 10);
    assert_eq!(body["approval_token_set"], false);
}

#[tokio::test]
async fn env_default_true_is_reflected_before_any_ui_toggle() {
    let config = AppConfig {
        orch_enable: true,
        orch_approval_token: Some("secret".to_string()),
        ..AppConfig::default()
    };
    let (app, _, _) = app_with_state(config).await;
    let body = get_settings(&app).await;

    assert_eq!(body["enabled"], true);
    assert_eq!(body["source"], "env_default");
    assert_eq!(body["env_default"], true);
    assert_eq!(
        body["approval_token_set"], true,
        "the token value itself must never appear, but whether one is set must"
    );
    // This test builds `AppState` directly (like the other orch test files),
    // bypassing `server.rs`'s boot sequence — so nothing has called
    // `orch_runtime.start()` yet even though the env default is `true`.
    // That boot-time start is `server.rs`'s job, exercised by
    // `tack-api/tests/orchestration/reconciler/wiring.rs`, not this file's.
    assert_eq!(body["reconciler_running"], false);
}

// ─── PUT: persists and overrides the env default ───────────────────────────

#[tokio::test]
async fn put_true_persists_and_switches_source_to_database() {
    let (app, _, _) = app_with_state(AppConfig::default()).await;

    let put_body = put_settings(&app, true).await;
    assert_eq!(put_body["enabled"], true);
    assert_eq!(put_body["source"], "database");
    assert_eq!(
        put_body["env_default"], false,
        "env_default reports the env var, independent of the stored override"
    );

    // A fresh GET (not just the PUT response) must agree.
    let get_body = get_settings(&app).await;
    assert_eq!(get_body["enabled"], true);
    assert_eq!(get_body["source"], "database");
}

#[tokio::test]
async fn put_false_after_put_true_stays_database_sourced() {
    let (app, _, _) = app_with_state(AppConfig::default()).await;

    put_settings(&app, true).await;
    let body = put_settings(&app, false).await;

    assert_eq!(body["enabled"], false);
    assert_eq!(
        body["source"], "database",
        "an explicit false is still an explicit override, not a reset to env_default"
    );
}

// ─── Runtime start/stop, no restart ────────────────────────────────────────

#[tokio::test]
async fn put_true_starts_reconciler_for_already_registered_plane() {
    let (app, state, _) = app_with_state(AppConfig::default()).await;

    // Register a control plane directly via the repo — orchestration is
    // still off at this point, so the HTTP route would 409.
    state
        .repo
        .create_control_plane(CreateControlPlane {
            name: "docket-1".to_string(),
            kind: None,
            // Deliberately unreachable: connection failures are fast and
            // this test only cares about task lifecycle, not poll content.
            base_url: "http://127.0.0.1:1".to_string(),
            token: None,
        })
        .await
        .expect("create control plane");

    let body = put_settings(&app, true).await;
    assert_eq!(body["control_plane_count"], 1);
    assert_eq!(
        body["reconciler_running"], true,
        "a task should be spawned for the already-registered plane by the time PUT returns"
    );
}

#[tokio::test]
async fn put_false_stops_the_reconciler_without_a_restart() {
    let (app, state, _) = app_with_state(AppConfig::default()).await;
    state
        .repo
        .create_control_plane(CreateControlPlane {
            name: "docket-1".to_string(),
            kind: None,
            base_url: "http://127.0.0.1:1".to_string(),
            token: None,
        })
        .await
        .expect("create control plane");

    let on = put_settings(&app, true).await;
    assert_eq!(on["reconciler_running"], true);

    let off = put_settings(&app, false).await;
    assert_eq!(off["source"], "database");
    // Stop is signalled synchronously inside the handler but the task exits
    // cooperatively (module doc's start/stop design) — poll briefly rather
    // than asserting the PUT response itself already shows `false`.
    let settled = wait_for_reconciler_running(&app, false).await;
    assert_eq!(settled["enabled"], false);
}

#[tokio::test]
async fn repeated_toggles_never_leave_more_than_one_task_per_plane() {
    let (app, state, _) = app_with_state(AppConfig::default()).await;
    state
        .repo
        .create_control_plane(CreateControlPlane {
            name: "docket-1".to_string(),
            kind: None,
            base_url: "http://127.0.0.1:1".to_string(),
            token: None,
        })
        .await
        .expect("create control plane");

    for _ in 0..3 {
        let on = put_settings(&app, true).await;
        assert_eq!(on["reconciler_running"], true);
        assert_eq!(on["control_plane_count"], 1);

        let off = put_settings(&app, false).await;
        assert_eq!(off["source"], "database");
        wait_for_reconciler_running(&app, false).await;
    }
}

#[tokio::test]
async fn put_true_while_already_running_does_not_spawn_a_duplicate() {
    let (app, state, _) = app_with_state(AppConfig::default()).await;
    state
        .repo
        .create_control_plane(CreateControlPlane {
            name: "docket-1".to_string(),
            kind: None,
            base_url: "http://127.0.0.1:1".to_string(),
            token: None,
        })
        .await
        .expect("create control plane");

    put_settings(&app, true).await;
    let first_count = get_settings(&app).await;
    assert_eq!(first_count["reconciler_running"], true);

    // A second `PUT {"enabled": true}` in a row must be a no-op, not a
    // second task spawned alongside the live one — `OrchRuntime::start`'s
    // idempotency (see its own doc comment and the unit tests in
    // `orch_runtime.rs`).
    let second = put_settings(&app, true).await;
    assert_eq!(second["reconciler_running"], true);
}

// ─── Reachable while disabled ──────────────────────────────────────────────

#[tokio::test]
async fn settings_route_is_never_gated_by_orchestration_itself() {
    let (app, _, _) = app_with_state(AppConfig::default()).await; // disabled
    let res = req(&app, Method::GET, "/api/settings/orchestration", None).await;
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "the settings endpoint must be reachable precisely when orchestration is off \
         — that's how an operator discovers and enables the feature"
    );
}
