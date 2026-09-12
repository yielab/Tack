//! `handlers::items::maybe_auto_dispatch` must gate on
//! `crate::handlers::settings::effective_orch_enabled`, never on the raw
//! `state.config.orch_enable` (`TACK_ORCH_ENABLE`'s startup-only env value):
//! every HTTP orchestration route (and the Settings UI) reads
//! `effective_orch_enabled`, which prefers whatever was most recently stored
//! in `app_meta` via `PUT /api/settings/orchestration`. Gating on the raw
//! env value instead would mean an operator who started the server with
//! `TACK_ORCH_ENABLE=1` and then switched orchestration off in Settings
//! still gets auto-dispatch on every eligible status change — the raw env
//! flag never notices the UI toggle.
//!
//! This is the regression test: with `TACK_ORCH_ENABLE=1` in config but
//! orchestration toggled *off* in `app_meta`, moving an item into a
//! `dispatch_from` status must not dispatch. It fails against a gate that
//! reads `!state.config.orch_enable` directly (which never consults
//! `app_meta`) and passes once the gate reads `effective_orch_enabled`.

use crate::common;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::router::{AppState, build_router};
use tack_core::models::{CreateItem, ItemSource};
use tack_db::repo::orch::{CreateControlPlane, UpsertOrchLink};
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::MockServer;

// ─── Helpers (mirrors orchestration/auto_dispatch/hook.rs) ─────────────────

async fn app_with_state(config: AppConfig) -> (Router, AppState) {
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

    (build_router(state.clone()), state)
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

async fn patch_status(app: &Router, item_id: Uuid, status: &str) -> axum::response::Response {
    req(
        app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"status": status})),
    )
    .await
}

/// Same shape as `orchestration/auto_dispatch/hook.rs`'s helper: seeds a control plane +
/// linked project entirely at the repo layer, bypassing the
/// `TACK_ORCH_ENABLE`-gated HTTP routes — which, in this test, are 409ing
/// anyway once orchestration is toggled off, exactly as an operator would
/// see them after flipping the Settings switch.
async fn link_project(state: &AppState, project_id: Uuid, base_url: &str) -> Uuid {
    let cp = state
        .repo
        .create_control_plane(CreateControlPlane {
            name: "docket-1".into(),
            kind: None,
            base_url: base_url.to_string(),
            token: None,
        })
        .await
        .expect("create control plane");

    state
        .repo
        .upsert_orch_link(
            project_id,
            UpsertOrchLink {
                control_plane_id: cp.id,
                remote_project: "demo".into(),
                pipeline_file: None,
                blueprint: None,
                auto_dispatch: true,
                budget_usd: None,
                status_map: json!({"dispatch_from": ["To Do"], "on_running": "In Progress"}),
            },
        )
        .await
        .expect("link project");

    cp.id
}

async fn seed_item(state: &AppState, project_id: Uuid, status: &str, title: &str) -> Uuid {
    let item = state
        .repo
        .create_item_with_source(
            project_id,
            status,
            CreateItem {
                title: title.to_string(),
                description: None,
                item_type: None,
                parent_id: None,
                priority: None,
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
            ItemSource::Manual,
        )
        .await
        .expect("seed item");
    item.id
}

/// Flip the same switch the Settings UI does. `enabled: false` is written
/// to `app_meta` regardless of `TACK_ORCH_ENABLE`'s startup value — that's
/// the whole point of `effective_orch_enabled` preferring the stored value.
async fn turn_orchestration_off_via_the_ui_toggle(app: &Router) {
    let res = req(
        app,
        Method::PUT,
        "/api/settings/orchestration",
        Some(json!({"enabled": false})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
}

// ─── The regression test ───────────────────────────────────────────────────

#[tokio::test]
async fn auto_dispatch_does_not_fire_when_orch_enable_env_is_set_but_the_ui_toggle_is_off() {
    let server = MockServer::start().await;
    // Deliberately no mocks registered: if `maybe_auto_dispatch` reaches the
    // control plane at all, wiremock has nothing to answer with and the
    // dispatch attempt fails loudly — but the real assertion is the
    // received-request count below, not that failure mode.

    // TACK_ORCH_ENABLE=1 at the process level...
    let (app, state) = app_with_state(AppConfig {
        orch_enable: true,
        ..AppConfig::default()
    })
    .await;

    // ...then the operator switches orchestration off from Settings. This
    // is the exact sequence: env on, UI off.
    turn_orchestration_off_via_the_ui_toggle(&app).await;

    let project_id = common::create_project(&app, "Auto-dispatch Gate Test", "software").await;
    let item_id = seed_item(&state, project_id, "Backlog", "Should not auto-dispatch").await;
    link_project(&state, project_id, &server.uri()).await;

    let res = patch_status(&app, item_id, "To Do").await;
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "the ordinary item PATCH must still succeed even though the effective toggle is off: {:?}",
        body_json(res).await
    );

    // Give a wrongly-firing hook time to show up, then assert nothing did —
    // same polling shape `orchestration/auto_dispatch/hook.rs` uses for the
    // equivalent "off by default" case.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let hits = server.received_requests().await.unwrap_or_default();
    assert!(
        hits.is_empty(),
        "orchestration toggled off in the UI must mean no auto-dispatch, even with \
         TACK_ORCH_ENABLE=1 at the process level — the raw env flag must not override the \
         UI's explicit off: {hits:?}"
    );
    let tasks = state.repo.list_orch_tasks_for_item(item_id).await.unwrap();
    assert!(
        tasks.is_empty(),
        "no orch_tasks row should have been created either"
    );
}

/// Sanity check in the opposite direction, so this file proves the gate
/// reads the *effective* setting rather than just being permanently closed:
/// env off, UI explicitly on, must dispatch. If this test fails while the
/// one above passes, the fix over-corrected into "never dispatch."
#[tokio::test]
async fn auto_dispatch_fires_when_the_ui_toggle_is_on_even_with_the_env_flag_unset() {
    let server = MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/tasks/demo"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "task": "task-ui-enabled", "project": "demo", "status": "pending"
        })))
        .mount(&server)
        .await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/tasks/demo"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "tasks": [{
                "id": "task-ui-enabled", "description": "x", "priority": "normal",
                "status": "pending", "created": "2026-08-05T00:00:00Z", "source": "operator",
            }]
        })))
        .mount(&server)
        .await;

    // `AppConfig::default()` has `orch_enable: false` — the env flag is
    // unset — but the orchestration-settings route stays reachable
    // regardless (router.rs keeps it outside `orch_routes`'s gate), so the
    // UI can still turn the feature on.
    let (app, state) = app_with_state(AppConfig::default()).await;

    let res = req(
        &app,
        Method::PUT,
        "/api/settings/orchestration",
        Some(json!({"enabled": true})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);

    let project_id = common::create_project(&app, "Auto-dispatch Gate Test", "software").await;
    let item_id = seed_item(&state, project_id, "Backlog", "Should auto-dispatch").await;
    link_project(&state, project_id, &server.uri()).await;

    let res = patch_status(&app, item_id, "To Do").await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);

    let mut tasks = Vec::new();
    for _ in 0..40 {
        tasks = state.repo.list_orch_tasks_for_item(item_id).await.unwrap();
        if !tasks.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        tasks.len(),
        1,
        "the UI-enabled toggle must still allow auto-dispatch when the env flag is unset"
    );
}
