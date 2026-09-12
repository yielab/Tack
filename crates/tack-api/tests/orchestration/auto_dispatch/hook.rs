//! Tests for the auto-dispatch hook:
//! `handlers::items::maybe_auto_dispatch`, wired into `PATCH /api/items/{id}`
//! beside `maybe_sync_github`/`propagate_parent_completion`. When
//! `orch_links.auto_dispatch` is on and an item's status changes into one of
//! `status_map.dispatch_from`, the hook calls `dispatcher::dispatch_item`
//! off the request path, passing the item's own **persisted** trust value
//! (`item.source.is_trusted()` — migration 029).
//!
//! The test that matters most here: an item imported from GitHub,
//! auto-dispatched, reaches docket with
//! `trusted: false` **on the wire** — asserted with a wiremock matcher that
//! only responds 200 if the flag is genuinely present in the request body,
//! not by inspecting what a function was called with.
//!
//! Also covers the hazards this hook must guard against: it stays off unless
//! `TACK_ORCH_ENABLE` is set and unless the link's
//! `auto_dispatch` is on; and it must not dispatch on every update — an item
//! edited while it's already sitting in a `dispatch_from` status must not
//! re-dispatch.

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
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ─── Helpers (mirrors orchestration/dispatch/item.rs) ──────────────────────

fn orch_config() -> AppConfig {
    AppConfig {
        orch_enable: true,
        ..AppConfig::default()
    }
}

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

async fn patch_title(app: &Router, item_id: Uuid, title: &str) -> axum::response::Response {
    req(
        app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"title": title})),
    )
    .await
}

/// Seed a control plane + a linked project, entirely at the repo layer —
/// deliberately bypassing the `TACK_ORCH_ENABLE`-gated HTTP routes
/// (`/api/control-planes`, `/api/projects/{id}/orch-link`) so this helper
/// also works for the "orch disabled" test, where those routes 404.
async fn link_project(
    state: &AppState,
    project_id: Uuid,
    base_url: &str,
    auto_dispatch: bool,
    status_map: serde_json::Value,
) -> Uuid {
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
                auto_dispatch,
                budget_usd: None,
                status_map,
            },
        )
        .await
        .expect("link project");

    cp.id
}

async fn seed_item(
    state: &AppState,
    project_id: Uuid,
    status: &str,
    source: ItemSource,
    title: &str,
) -> Uuid {
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
            source,
        )
        .await
        .expect("seed item");
    item.id
}

fn mock_list_tasks_body(task_id: &str) -> serde_json::Value {
    json!({ "tasks": [{
        "id": task_id, "description": "x", "priority": "normal", "status": "pending",
        "created": "2026-08-05T00:00:00Z", "source": "operator",
    }]})
}

/// The hook runs on a background `tokio::spawn` — poll wiremock's received
/// request log for up to ~2s, the same pattern `handlers/crud.rs`'s GitHub
/// push-back test already uses for exactly the same "fire and forget"
/// reason. Returns the number of matching hits observed.
async fn wait_for_hits(server: &MockServer, path_suffix: &str, at_least: usize) -> usize {
    for _ in 0..40 {
        let reqs = server.received_requests().await.unwrap_or_default();
        let count = reqs
            .iter()
            .filter(|r| r.url.path().ends_with(path_suffix))
            .count();
        if count >= at_least {
            return count;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.url.path().ends_with(path_suffix))
        .count()
}

/// Poll `list_orch_tasks_for_item` for up to ~2s — used instead of
/// `wait_for_hits` when a test needs to assert on the persisted task (e.g.
/// its `trusted` column), not just that a request landed.
async fn wait_for_orch_task(state: &AppState, item_id: Uuid) -> Vec<tack_db::repo::orch::OrchTask> {
    for _ in 0..40 {
        let tasks = state.repo.list_orch_tasks_for_item(item_id).await.unwrap();
        if !tasks.is_empty() {
            return tasks;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    state.repo.list_orch_tasks_for_item(item_id).await.unwrap()
}

// ─── The headline test: trusted:false on the wire for a GitHub-imported item ─

#[tokio::test]
async fn auto_dispatch_sends_trusted_false_on_the_wire_for_a_github_imported_item() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .and(body_partial_json(json!({"trusted": false})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "task": "task-auto-untrusted", "project": "demo", "status": "pending"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tasks/demo"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(mock_list_tasks_body("task-auto-untrusted")),
        )
        .mount(&server)
        .await;

    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Auto-dispatch Test Project", "software").await;
    let item_id = seed_item(
        &state,
        project_id,
        "Backlog",
        ItemSource::Github,
        "Imported from GitHub",
    )
    .await;
    link_project(
        &state,
        project_id,
        &server.uri(),
        true, // auto_dispatch
        json!({"dispatch_from": ["To Do"], "on_running": "In Progress"}),
    )
    .await;

    let res = patch_status(&app, item_id, "To Do").await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);

    // wiremock's POST matcher only responds 200 (letting the dispatcher
    // proceed to persist an orch_tasks row) if `trusted: false` was
    // genuinely in the request body — this is the real check, not the
    // assertions below, which just confirm the outcome wasn't a fluke.
    let tasks = wait_for_orch_task(&state, item_id).await;
    assert_eq!(tasks.len(), 1, "expected exactly one auto-dispatched task");
    assert_eq!(tasks[0].remote_task_id, "task-auto-untrusted");
    assert!(
        !tasks[0].trusted,
        "a GitHub-imported item's persisted trust must reach docket as trusted:false"
    );

    // And the mapped status (on_running) really applied through the engine.
    let item = state.repo.get_item(item_id).await.unwrap().unwrap();
    assert_eq!(item.status, "In Progress");
}

#[tokio::test]
async fn auto_dispatch_sends_trusted_true_for_a_manually_created_item() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .and(body_partial_json(json!({"trusted": true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "task": "task-auto-trusted", "project": "demo", "status": "pending"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tasks/demo"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(mock_list_tasks_body("task-auto-trusted")),
        )
        .mount(&server)
        .await;

    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Auto-dispatch Test Project", "software").await;
    // ItemSource::Manual — the default for the ordinary create-item path.
    let item_id = seed_item(
        &state,
        project_id,
        "Backlog",
        ItemSource::Manual,
        "Typed directly in Tack",
    )
    .await;
    link_project(
        &state,
        project_id,
        &server.uri(),
        true,
        json!({"dispatch_from": ["To Do"], "on_running": "In Progress"}),
    )
    .await;

    let res = patch_status(&app, item_id, "To Do").await;
    assert_eq!(res.status(), StatusCode::OK);

    let tasks = wait_for_orch_task(&state, item_id).await;
    assert_eq!(tasks.len(), 1);
    assert!(
        tasks[0].trusted,
        "an operator-authored item must dispatch as trusted"
    );
}

// ─── Off by default ─────────────────────────────────────────────────────────

#[tokio::test]
async fn auto_dispatch_does_not_fire_when_orch_disabled() {
    let server = MockServer::start().await;
    // No mocks at all — if anything hit this server, wiremock has nothing
    // to respond with and the request would fail loudly enough to notice,
    // but the real assertion is the received-request count below.

    // `AppConfig::default()` has `orch_enable: false`.
    let (app, state) = app_with_state(AppConfig::default()).await;
    let project_id = common::create_project(&app, "Auto-dispatch Test Project", "software").await;
    let item_id = seed_item(
        &state,
        project_id,
        "Backlog",
        ItemSource::Github,
        "Imported from GitHub",
    )
    .await;
    link_project(
        &state,
        project_id,
        &server.uri(),
        true,
        json!({"dispatch_from": ["To Do"], "on_running": "In Progress"}),
    )
    .await;

    let res = patch_status(&app, item_id, "To Do").await;
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "the ordinary item PATCH must still succeed even though orch is disabled"
    );

    // Give a wrongly-firing hook time to show up, then assert nothing did.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let hits = server.received_requests().await.unwrap_or_default();
    assert!(
        hits.is_empty(),
        "TACK_ORCH_ENABLE unset must mean no dispatch, no exceptions: {hits:?}"
    );
    let tasks = state.repo.list_orch_tasks_for_item(item_id).await.unwrap();
    assert!(tasks.is_empty());
}

#[tokio::test]
async fn auto_dispatch_does_not_fire_when_link_auto_dispatch_is_off() {
    let server = MockServer::start().await;

    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Auto-dispatch Test Project", "software").await;
    let item_id = seed_item(
        &state,
        project_id,
        "Backlog",
        ItemSource::Github,
        "Imported from GitHub",
    )
    .await;
    link_project(
        &state,
        project_id,
        &server.uri(),
        false, // auto_dispatch OFF
        json!({"dispatch_from": ["To Do"], "on_running": "In Progress"}),
    )
    .await;

    let res = patch_status(&app, item_id, "To Do").await;
    assert_eq!(res.status(), StatusCode::OK);

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let hits = server.received_requests().await.unwrap_or_default();
    assert!(
        hits.is_empty(),
        "auto_dispatch: false on the link must mean no auto-dispatch: {hits:?}"
    );
}

// ─── Don't dispatch on every update ─────────────────────────────────────────

#[tokio::test]
async fn auto_dispatch_does_not_refire_on_an_edit_that_does_not_change_status() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "task": "task-once", "project": "demo", "status": "pending"
        })))
        .expect(1) // the whole point of this test
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_list_tasks_body("task-once")))
        .mount(&server)
        .await;

    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Auto-dispatch Test Project", "software").await;
    let item_id = seed_item(
        &state,
        project_id,
        "Backlog",
        ItemSource::Manual,
        "Edited repeatedly",
    )
    .await;
    link_project(
        &state,
        project_id,
        &server.uri(),
        true,
        json!({"dispatch_from": ["To Do"], "on_running": "In Progress"}),
    )
    .await;

    // Enter the dispatch_from status — this fires the hook once.
    let res = patch_status(&app, item_id, "To Do").await;
    assert_eq!(res.status(), StatusCode::OK);
    wait_for_hits(&server, "/tasks/demo", 1).await;

    // The mapped on_running status moves the item to "In Progress"; edit an
    // unrelated field a few times while it sits there — none of these
    // changed `status`, so the hook must short-circuit before ever calling
    // dispatch_item again.
    for i in 0..3 {
        let res = patch_title(&app, item_id, &format!("Edited title #{i}")).await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    // Give a wrongly-refiring hook time to show up.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let tasks = state.repo.list_orch_tasks_for_item(item_id).await.unwrap();
    assert_eq!(
        tasks.len(),
        1,
        "editing an item already past its dispatch_from status must not create a second task"
    );

    server.verify().await; // asserts the mock's .expect(1) held
}
