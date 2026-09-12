//! `dispatcher::apply_mapped_status` reads a WIP-limited column's item count
//! and writes the new status as two separate, unlocked steps; concurrent
//! sprint dispatch (`max_in_flight` items at once) can let two writers both
//! observe "under the limit" and both commit, over-filling the column.
//!
//! Drives `N` genuinely concurrent dispatches of *different* items into one
//! WIP-limited column through the real dispatch HTTP path (a `wiremock`
//! delay bunches every request's arrival at the count-then-write step).
//! Per-item `DispatchLocks` does not protect against this: it only
//! serializes two requests for the *same* item.

use std::time::Duration;

use crate::common;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::router::{AppState, build_router};
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ─── Helpers (mirrors orchestration/dispatch/item.rs /
// orchestration/auto_dispatch/sprint.rs) ────────────────────────────────────

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

async fn create_item(app: &Router, project_id: Uuid, title: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/items"),
        Some(json!({"title": title})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn create_control_plane(app: &Router, base_url: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        "/api/control-planes",
        Some(json!({"name": "docket-1", "base_url": base_url})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn link_project(app: &Router, project_id: Uuid, control_plane_id: Uuid, status_map: Value) {
    let res = req(
        app,
        Method::PUT,
        &format!("/api/projects/{project_id}/orch-link"),
        Some(json!({
            "control_plane_id": control_plane_id,
            "remote_project": "demo",
            "status_map": status_map,
        })),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
}

/// `POST /tasks/demo` — every request returns the same task id after a
/// shared delay. The delay is the point: it holds every concurrent
/// dispatch's HTTP round trip open at once, so all `N` requests reach
/// `apply_mapped_status`'s count-then-write step at roughly the same moment
/// instead of being naturally serialized by how fast an in-memory SQLite
/// call returns. Distinct items sharing one remote task id is fine —
/// `orch_tasks`' PK is `(item_id, remote_task_id)`.
async fn mock_enqueue_allow_delayed(server: &MockServer, task_id: &str, delay: Duration) {
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "ok": true, "task": task_id, "project": "demo", "status": "pending"
                }))
                .set_delay(delay),
        )
        .mount(server)
        .await;
}

async fn mock_list_tasks(server: &MockServer, task_id: &str) {
    Mock::given(method("GET"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [{
            "id": task_id, "description": "x", "priority": "normal", "status": "pending",
            "created": "2026-08-05T00:00:00Z", "source": "operator",
        }]})))
        .mount(server)
        .await;
}

// ─── The race ───────────────────────────────────────────────────────────

/// `N` distinct items, all eligible to dispatch, all mapped by `on_running`
/// into the same WIP-limited column (scrum's "In Progress", limit 5).
/// Dispatched genuinely concurrently. However many of the `N` requests win
/// the race, the column must never end up holding more than its configured
/// limit. "software" -> scrum_workflow(): "In Progress" has wip_limit =
/// Some(5) and `transitions: None` (no explicit-transition restriction), so
/// every item created here can go straight from the initial status
/// ("Backlog") to "In Progress" without construction-workflow-style gating
/// getting in the way of the race this test is trying to trigger.
#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
async fn concurrent_dispatch_into_one_wip_column_stays_under_limit() {
    const N: usize = 12;
    const WIP_LIMIT: i64 = 5;

    let server = MockServer::start().await;
    mock_enqueue_allow_delayed(&server, "task-shared", Duration::from_millis(120)).await;
    mock_list_tasks(&server, "task-shared").await;

    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "WIP Race Test Project", "software").await;

    let mut item_ids = Vec::with_capacity(N);
    for i in 0..N {
        item_ids.push(create_item(&app, project_id, &format!("Item {i}")).await);
    }

    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(
        &app,
        project_id,
        cp,
        json!({"dispatch_from": ["Backlog"], "on_running": "In Progress"}),
    )
    .await;

    // Fire all N dispatches at once — real concurrency (multi-thread runtime
    // + tokio::spawn), not just interleaved polling on one task.
    let handles: Vec<_> = item_ids
        .iter()
        .copied()
        .map(|item_id| {
            let app = app.clone();
            tokio::spawn(async move {
                req(
                    &app,
                    Method::POST,
                    &format!("/api/items/{item_id}/dispatch"),
                    None,
                )
                .await
                .status()
            })
        })
        .collect();

    let mut statuses = Vec::with_capacity(N);
    for h in handles {
        statuses.push(h.await.expect("dispatch task panicked"));
    }
    // Every dispatch should reach the handler successfully — docket enqueues
    // unconditionally in this test (no policy configured); a rejected
    // status_map transition is still a 200 (DispatchOutcome::Success with a
    // rejected status_application), not an HTTP error.
    for s in &statuses {
        assert_eq!(*s, StatusCode::OK);
    }

    let final_count = state
        .repo
        .count_items_by_status(project_id, "In Progress")
        .await
        .expect("count_items_by_status");

    assert!(
        final_count <= WIP_LIMIT,
        "WIP limit for 'In Progress' is {WIP_LIMIT}, but {final_count} of {N} concurrently \
         dispatched items ended up there — apply_mapped_status's WIP-limit check and its \
         status write are not atomic against concurrent writers into the same column"
    );

    // Sanity: nothing vanished, and every item that didn't make it into
    // "In Progress" is still sitting in "Backlog" (a rejected status_map
    // transition leaves the item untouched, per dispatcher.rs's contract —
    // it must not be lost or land somewhere else).
    let backlog_count = state
        .repo
        .count_items_by_status(project_id, "Backlog")
        .await
        .expect("count_items_by_status");
    assert_eq!(
        final_count + backlog_count,
        N as i64,
        "every item must be in exactly one of Backlog/In Progress — none lost, none duplicated"
    );
}
