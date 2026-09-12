//! Tests for `POST /api/items/{id}/dispatch` and the `dispatcher` module
//! it's backed by.
//!
//! Covers: the off/unknown/unlinked guards, the no-op outcomes
//! (`no_dispatch_policy`/`not_eligible`), the happy path applying
//! `on_running`, `waiting_approval` (never reported as a plain success), a
//! `pre_input` policy block, double-dispatch idempotency, the `trusted`
//! flag by item provenance, and a construction project's linear workflow
//! rejecting an illegal `on_running` target.

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
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ─── Helpers (mirrors orchestration/control_plane/resource.rs /
// orchestration/reporting/agent_activity.rs) ────────────────────────────────

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

/// Returns `(item_id, initial_status)`.
async fn create_item(app: &Router, project_id: Uuid, title: &str) -> (Uuid, String) {
    let res = req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/items"),
        Some(json!({"title": title})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let v = body_json(res).await;
    (
        Uuid::parse_str(v["id"].as_str().unwrap()).unwrap(),
        v["status"].as_str().unwrap().to_string(),
    )
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

async fn dispatch(app: &Router, item_id: Uuid) -> axum::response::Response {
    req(
        app,
        Method::POST,
        &format!("/api/items/{item_id}/dispatch"),
        None,
    )
    .await
}

/// Mocks `POST /tasks/demo` returning docket's real "allow" shape.
async fn mock_enqueue_allow(server: &MockServer, task_id: &str) {
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "task": task_id, "project": "demo", "status": "pending"
        })))
        .mount(server)
        .await;
}

/// Mocks the follow-up `GET /tasks/demo` the dispatcher reads to learn the
/// real status/approval token.
async fn mock_list_tasks(server: &MockServer, task_id: &str, status: &str, token: Option<&str>) {
    let mut task = json!({
        "id": task_id, "description": "x", "priority": "normal", "status": status,
        "created": "2026-08-05T00:00:00Z", "source": "operator",
    });
    if let Some(t) = token {
        task["approvalToken"] = json!(t);
    }
    Mock::given(method("GET"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [task] })))
        .mount(server)
        .await;
}

// ─── Off by default / actionable refusal ───────────────────────────────────

#[tokio::test]
async fn dispatch_409s_when_orch_disabled() {
    let (app, _) = common::test_app().await; // orch_enable defaults to false
    let res = dispatch(&app, Uuid::new_v4()).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");
}

#[tokio::test]
async fn dispatch_404s_for_unknown_item() {
    let (app, _) = app_with_state(orch_config()).await;
    let res = dispatch(&app, Uuid::new_v4()).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn dispatch_409s_when_project_not_linked() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Dispatch Test Project", "software").await;
    let (item_id, _) = create_item(&app, project_id, "Unlinked").await;

    let res = dispatch(&app, item_id).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

// ─── dispatch_from gating: no-op, not an error ─────────────────────────────

#[tokio::test]
async fn dispatch_reports_no_dispatch_policy_when_empty() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Dispatch Test Project", "software").await;
    let (item_id, _) = create_item(&app, project_id, "No policy yet").await;
    let cp = create_control_plane(&app, "http://docket.local:9999").await;
    link_project(&app, project_id, cp, json!({})).await; // dispatch_from defaults to []

    let res = dispatch(&app, item_id).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["outcome"], "no_dispatch_policy");
    assert_eq!(v["task"], Value::Null);
}

#[tokio::test]
async fn dispatch_not_eligible_when_status_outside_dispatch_from() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Dispatch Test Project", "software").await;
    let (item_id, status) = create_item(&app, project_id, "Wrong column").await;
    assert_eq!(status, "Backlog");
    let cp = create_control_plane(&app, "http://docket.local:9999").await;
    link_project(
        &app,
        project_id,
        cp,
        json!({"dispatch_from": ["In Review"]}),
    )
    .await;

    let res = dispatch(&app, item_id).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["outcome"], "not_eligible");
    assert_eq!(v["current_status"], "Backlog");
    assert_eq!(v["dispatch_from"], json!(["In Review"]));
}

// ─── Happy path: enqueue succeeds, on_running applied through the engine ──

#[tokio::test]
async fn dispatch_success_enqueues_and_applies_on_running() {
    let server = MockServer::start().await;
    mock_enqueue_allow(&server, "task-happy-1").await;
    mock_list_tasks(&server, "task-happy-1", "pending", None).await;

    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Dispatch Test Project", "software").await;
    let (item_id, status) = create_item(&app, project_id, "Dispatch me").await;
    assert_eq!(status, "Backlog");
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(
        &app,
        project_id,
        cp,
        json!({"dispatch_from": ["Backlog"], "on_running": "In Progress"}),
    )
    .await;

    let res = dispatch(&app, item_id).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["outcome"], "dispatched");
    assert_eq!(v["task"]["remote_task_id"], "task-happy-1");
    assert_eq!(v["task"]["attempt"], 1);
    assert_eq!(v["status_applied"], "In Progress");
    assert_eq!(v["status_map_rejected"], Value::Null);

    let item = state.repo.get_item(item_id).await.unwrap().unwrap();
    assert_eq!(item.status, "In Progress");
}

#[tokio::test]
async fn dispatch_applies_on_waiting_approval_not_on_running() {
    let server = MockServer::start().await;
    mock_enqueue_allow(&server, "task-needs-approval").await;
    mock_list_tasks(
        &server,
        "task-needs-approval",
        "waiting_approval",
        Some("tok-abc"),
    )
    .await;

    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Dispatch Test Project", "software").await;
    let (item_id, _) = create_item(&app, project_id, "Needs a human").await;
    let cp = create_control_plane(&app, &server.uri()).await;
    // Reuse "In Review" (a real scrum status) as the stand-in "needs a
    // human" column for on_waiting_approval.
    link_project(
        &app,
        project_id,
        cp,
        json!({
            "dispatch_from": ["Backlog"],
            "on_running": "In Progress",
            "on_waiting_approval": "In Review",
        }),
    )
    .await;

    let res = dispatch(&app, item_id).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(
        v["outcome"], "waiting_approval",
        "must never be reported as a plain successful dispatch: {v}"
    );
    assert_eq!(v["approval_token"], "tok-abc");
    assert_eq!(v["status_applied"], "In Review");

    let item = state.repo.get_item(item_id).await.unwrap().unwrap();
    assert_eq!(
        item.status, "In Review",
        "on_waiting_approval must apply, not on_running"
    );
}

// ─── Block: a refusal, not a failure ───────────────────────────────────────

#[tokio::test]
async fn dispatch_blocked_surfaces_policy_creates_no_orch_task() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "ok": false,
            "error": "task rejected by guardrail policy 'prompt-injection' at enqueue: nope"
        })))
        .mount(&server)
        .await;

    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Dispatch Test Project", "software").await;
    let (item_id, _) = create_item(&app, project_id, "Blocked item").await;
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(
        &app,
        project_id,
        cp,
        json!({"dispatch_from": ["Backlog"], "on_running": "In Progress"}),
    )
    .await;

    let res = dispatch(&app, item_id).await;
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "a block is a 200, see the module doc"
    );
    let v = body_json(res).await;
    assert_eq!(v["outcome"], "blocked");
    assert_eq!(
        v["policy_id"], "prompt-injection",
        "policy id must be a typed field (OrchError::PolicyBlocked), \
         not something the caller parses out of message: {v}"
    );
    assert!(
        v["message"].as_str().unwrap().contains("prompt-injection"),
        "must name the policy id: {v}"
    );
    assert_eq!(v["task"], Value::Null);

    let tasks = state.repo.list_orch_tasks_for_item(item_id).await.unwrap();
    assert!(
        tasks.is_empty(),
        "a block must not create an orch_tasks row"
    );

    let item = state.repo.get_item(item_id).await.unwrap().unwrap();
    assert_eq!(item.status, "Backlog", "a block must leave the item alone");
}

// ─── Idempotency: double-dispatch creates one task, not two ──────────────

#[tokio::test]
async fn double_dispatch_hits_docket_once_second_call_in_flight() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "task": "task-once", "project": "demo", "status": "pending"
        })))
        .expect(1) // the whole point of this test
        .mount(&server)
        .await;
    mock_list_tasks(&server, "task-once", "pending", None).await;

    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Dispatch Test Project", "software").await;
    let (item_id, _) = create_item(&app, project_id, "Double-clicked").await;
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(
        &app,
        project_id,
        cp,
        json!({"dispatch_from": ["Backlog", "In Progress"], "on_running": "In Progress"}),
    )
    .await;

    let first = dispatch(&app, item_id).await;
    assert_eq!(first.status(), StatusCode::OK);
    let first_body = body_json(first).await;
    assert_eq!(first_body["outcome"], "dispatched");

    let second = dispatch(&app, item_id).await;
    assert_eq!(second.status(), StatusCode::OK);
    let second_body = body_json(second).await;
    assert_eq!(second_body["outcome"], "already_in_flight");
    assert_eq!(second_body["task"]["remote_task_id"], "task-once");

    let tasks = state.repo.list_orch_tasks_for_item(item_id).await.unwrap();
    assert_eq!(tasks.len(), 1, "must be exactly one orch_tasks row");
    assert_eq!(tasks[0].attempt, 1);

    // wiremock's `.expect(1)` on the mount is verified when `server` drops
    // at the end of the test; an explicit received-request check makes the
    // failure message clearer if it ever regresses.
    let received = server.received_requests().await.unwrap();
    let post_count = received
        .iter()
        .filter(|r| r.method == wiremock::http::Method::POST)
        .count();
    assert_eq!(
        post_count, 1,
        "docket's POST /tasks/demo must be hit exactly once"
    );
}

#[tokio::test]
async fn concurrent_double_dispatch_creates_exactly_one_task() {
    // A slow mock response widens the race window so two concurrent
    // requests for the same item genuinely overlap.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "ok": true, "task": "task-race", "project": "demo", "status": "pending"
                }))
                .set_delay(std::time::Duration::from_millis(150)),
        )
        .expect(1)
        .mount(&server)
        .await;
    mock_list_tasks(&server, "task-race", "pending", None).await;

    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Dispatch Test Project", "software").await;
    let (item_id, _) = create_item(&app, project_id, "Raced").await;
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(
        &app,
        project_id,
        cp,
        json!({"dispatch_from": ["Backlog"], "on_running": "In Progress"}),
    )
    .await;

    let app1 = app.clone();
    let app2 = app.clone();
    let (r1, r2) = tokio::join!(dispatch(&app1, item_id), dispatch(&app2, item_id));

    let statuses: Vec<StatusCode> = vec![r1.status(), r2.status()];
    let bodies = vec![body_json(r1).await, body_json(r2).await];

    // Exactly one request must have raced past the lock and reached docket;
    // the other must have been rejected by the in-process dispatch lock
    // (409) before ever calling out.
    let conflicts = statuses
        .iter()
        .filter(|s| **s == StatusCode::CONFLICT)
        .count();
    let successes = bodies
        .iter()
        .filter(|b| b["outcome"] == "dispatched")
        .count();
    assert_eq!(
        (conflicts, successes),
        (1, 1),
        "exactly one request must win the lock and dispatch, the other must \
         be rejected as already in flight: {statuses:?} {bodies:?}"
    );

    let tasks = state.repo.list_orch_tasks_for_item(item_id).await.unwrap();
    assert_eq!(tasks.len(), 1, "must be exactly one orch_tasks row");
}

// ─── trusted flag reaches the wire ─────────────────────────────────────────

/// Seeds an item the same way a real GitHub import
/// (`handlers::import_github`) does: `ItemSource::Github` is the persisted
/// provenance marker, not the `github_links` row, that `resolve_default_trust`
/// reads. Uses `create_item_with_source` directly rather than the plain
/// `create_item` HTTP helper, which always creates `ItemSource::Manual`
/// (trusted) items.
async fn seed_github_imported_item(state: &AppState, project_id: Uuid) -> Uuid {
    let project = state
        .repo
        .get_project(project_id)
        .await
        .expect("get project")
        .expect("project exists");
    let initial_status = project
        .workflow
        .initial_status()
        .expect("initial status")
        .to_string();
    let item = state
        .repo
        .create_item_with_source(
            project_id,
            &initial_status,
            tack_core::models::CreateItem {
                title: "Imported from GitHub".to_string(),
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
            tack_core::models::ItemSource::Github,
        )
        .await
        .expect("seed github-imported item");
    state
        .repo
        .set_github_link(item.id, "acme/repo", 42)
        .await
        .expect("seed github link");
    item.id
}

/// `trusted` reaches the wire by the item's own provenance
/// (`ItemSource`), not by how it's dispatched: a GitHub import gets
/// `false`, an item typed directly in Tack gets `true`. wiremock only
/// matches (200 instead of 404) if the expected value is really on the
/// wire, so each case's status assertion is the real check.
#[tokio::test]
async fn dispatch_sends_trusted_flag_by_item_provenance() {
    struct Case {
        from_github: bool,
        expect_trusted: bool,
        task_id: &'static str,
    }
    let cases = [
        Case {
            from_github: true,
            expect_trusted: false,
            task_id: "task-untrusted",
        },
        Case {
            from_github: false,
            expect_trusted: true,
            task_id: "task-trusted",
        },
    ];

    for case in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tasks/demo"))
            .and(body_partial_json(json!({"trusted": case.expect_trusted})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "task": case.task_id, "project": "demo", "status": "pending"
            })))
            .mount(&server)
            .await;
        mock_list_tasks(&server, case.task_id, "pending", None).await;

        let (app, state) = app_with_state(orch_config()).await;
        let project_id = common::create_project(&app, "Dispatch Test Project", "software").await;

        let item_id = if case.from_github {
            seed_github_imported_item(&state, project_id).await
        } else {
            create_item(&app, project_id, "Typed directly in Tack")
                .await
                .0
        };

        let cp = create_control_plane(&app, &server.uri()).await;
        link_project(
            &app,
            project_id,
            cp,
            json!({"dispatch_from": ["Backlog"], "on_running": "In Progress"}),
        )
        .await;

        let res = dispatch(&app, item_id).await;
        assert_eq!(
            res.status(),
            StatusCode::OK,
            "from_github={}: {:?}",
            case.from_github,
            body_json(res).await
        );
    }
}

// ─── status_map_rejected: the workflow engine still governs the move ─────

#[tokio::test]
async fn construction_rejects_illegal_on_running_item_untouched() {
    let server = MockServer::start().await;
    mock_enqueue_allow(&server, "task-construction-1").await;
    mock_list_tasks(&server, "task-construction-1", "pending", None).await;

    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Dispatch Test Project", "construction").await;
    let (item_id, status) = create_item(&app, project_id, "Frame the walls").await;
    assert_eq!(status, "Permit");
    let cp = create_control_plane(&app, &server.uri()).await;
    // "Handover" is a real status in the construction preset, but jumping
    // straight there from "Permit" isn't a legal transition (Permit's only
    // allowed next step is "Procurement").
    link_project(
        &app,
        project_id,
        cp,
        json!({"dispatch_from": ["Permit"], "on_running": "Handover"}),
    )
    .await;

    let res = dispatch(&app, item_id).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(
        v["outcome"], "dispatched",
        "the dispatch itself still succeeded"
    );
    assert_eq!(v["status_applied"], Value::Null);
    assert!(
        v["status_map_rejected"]
            .as_str()
            .unwrap()
            .contains("Handover"),
        "must surface the workflow engine's own reason: {v}"
    );

    let item = state.repo.get_item(item_id).await.unwrap().unwrap();
    assert_eq!(
        item.status, "Permit",
        "a rejected status_map transition must leave the item untouched"
    );

    let events = state
        .repo
        .list_orch_events_for_item(item_id, None)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "status_map_rejected");
    assert_eq!(events[0].payload["target_status"], "Handover");
    assert_eq!(events[0].payload["from_status"], "Permit");
}
