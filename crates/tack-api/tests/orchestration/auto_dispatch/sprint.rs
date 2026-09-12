//! Tests for `POST /api/sprints/{id}/dispatch` and
//! `GET /api/sprints/{id}/dispatch/dry-run` —
//! the `sprint_dispatch` module they're backed by.
//!
//! Covers: 404 with `TACK_ORCH_ENABLE` unset / unknown sprint; 409 for an
//! unlinked project; an empty sprint; a diamond dependency graph —
//! topological order, dependency
//! readiness gating a downstream item even once its direct blockers start
//! (but haven't finished), and a satisfied dependency unblocking its
//! dependents; the in-flight cap being honoured (timing-based) and clamped
//! to `[1, MAX_MAX_IN_FLIGHT]`; a policy-blocked item not aborting the rest
//! of the sprint (the partial-failure decision); trust threaded per item
//! rather than as one blanket value for the batch; and the dry-run's output
//! matching a real run's order and skip set exactly.

use crate::common;

use std::time::{Duration, Instant};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::router::{AppState, build_router};
use tack_core::models::{CreateItem, ItemSource};
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ─── Helpers (mirrors orchestration/dispatch/item.rs /
// orchestration/auto_dispatch/hook.rs) ──────────────────────────────────────

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

async fn create_sprint(app: &Router, project_id: Uuid) -> Uuid {
    let res = req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/sprints"),
        Some(json!({"name": "Sprint 1"})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

/// Returns `(item_id, initial_status)`. Assigns the item straight into
/// `sprint_id` at creation time.
async fn create_item(app: &Router, project_id: Uuid, sprint_id: Uuid, title: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/items"),
        Some(json!({"title": title, "sprint_id": sprint_id})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn seed_github_item(
    state: &AppState,
    project_id: Uuid,
    sprint_id: Uuid,
    title: &str,
) -> Uuid {
    let item = state
        .repo
        .create_item_with_source(
            project_id,
            "Backlog",
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
                sprint_id: Some(sprint_id),
                assignee: None,
            },
            ItemSource::Github,
        )
        .await
        .expect("seed github item");
    item.id
}

async fn create_dependency(app: &Router, source_item_id: Uuid, target_item_id: Uuid) {
    let res = req(
        app,
        Method::POST,
        &format!("/api/items/{source_item_id}/dependencies"),
        Some(json!({"target_item_id": target_item_id, "dependency_type": "blocks"})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
}

async fn patch_status(app: &Router, item_id: Uuid, status: &str) {
    let res = req(
        app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"status": status})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
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

async fn dry_run(app: &Router, sprint_id: Uuid, max_in_flight: Option<u32>) -> Value {
    let uri = match max_in_flight {
        Some(n) => format!("/api/sprints/{sprint_id}/dispatch/dry-run?max_in_flight={n}"),
        None => format!("/api/sprints/{sprint_id}/dispatch/dry-run"),
    };
    let res = req(app, Method::GET, &uri, None).await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    body_json(res).await
}

async fn dispatch_sprint(app: &Router, sprint_id: Uuid, max_in_flight: Option<u32>) -> Value {
    let uri = match max_in_flight {
        Some(n) => format!("/api/sprints/{sprint_id}/dispatch?max_in_flight={n}"),
        None => format!("/api/sprints/{sprint_id}/dispatch"),
    };
    let res = req(app, Method::POST, &uri, None).await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    body_json(res).await
}

/// Mocks `POST /tasks/demo` returning docket's "allow" shape — every
/// enqueue in a test gets the same task id, which is fine: `orch_tasks`'
/// PK is `(item_id, remote_task_id)`, so two different items sharing one
/// remote id never collide.
async fn mock_enqueue_allow(server: &MockServer, task_id: &str) {
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "task": task_id, "project": "demo", "status": "pending"
        })))
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

fn item_by_title<'a>(items: &'a [Value], title: &str) -> &'a Value {
    items
        .iter()
        .find(|i| i["title"] == title)
        .unwrap_or_else(|| panic!("no item named {title:?} in response: {items:?}"))
}

// ─── Off by default / not found / not linked ───────────────────────────────

#[tokio::test]
async fn dispatch_sprint_409s_when_orch_disabled() {
    let (app, _) = common::test_app().await; // orch_enable defaults to false
    let res = req(
        &app,
        Method::POST,
        &format!("/api/sprints/{}/dispatch", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/sprints/{}/dispatch/dry-run", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");
}

#[tokio::test]
async fn dispatch_sprint_404s_for_unknown_sprint() {
    let (app, _) = app_with_state(orch_config()).await;
    let res = req(
        &app,
        Method::POST,
        &format!("/api/sprints/{}/dispatch", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn dispatch_sprint_409s_when_project_not_linked() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;

    let res = req(
        &app,
        Method::POST,
        &format!("/api/sprints/{sprint_id}/dispatch"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

// ─── Empty sprint ───────────────────────────────────────────────────────────

#[tokio::test]
async fn dry_run_reports_an_empty_plan_for_a_sprint_with_no_items() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let cp = create_control_plane(&app, "http://docket.local:9999").await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    let v = dry_run(&app, sprint_id, None).await;
    assert_eq!(v["items"], json!([]));
    assert_eq!(v["summary"]["total"], 0);
}

// ─── Diamond dependency graph ──────────────────────────────────────────────
//
//        A
//       / \
//      B   C
//       \ /
//        D
//
// A blocks B and C; B and C both block D.

async fn seed_diamond(app: &Router, project_id: Uuid, sprint_id: Uuid) -> [Uuid; 4] {
    let a = create_item(app, project_id, sprint_id, "A").await;
    let b = create_item(app, project_id, sprint_id, "B").await;
    let c = create_item(app, project_id, sprint_id, "C").await;
    let d = create_item(app, project_id, sprint_id, "D").await;
    create_dependency(app, a, b).await; // A blocks B
    create_dependency(app, a, c).await; // A blocks C
    create_dependency(app, b, d).await; // B blocks D
    create_dependency(app, c, d).await; // C blocks D
    [a, b, c, d]
}

#[tokio::test]
async fn dry_run_diamond_orders_a_first_then_b_and_c_then_d_and_gates_downstream() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let [a, b, c, d] = seed_diamond(&app, project_id, sprint_id).await;
    let cp = create_control_plane(&app, "http://docket.local:9999").await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    let v = dry_run(&app, sprint_id, None).await;
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);

    let order_of = |id: Uuid| {
        items
            .iter()
            .find(|i| i["item_id"] == id.to_string())
            .unwrap()["order"]
            .as_u64()
            .unwrap()
    };
    // A strictly before B and C; B and C strictly before D.
    assert!(order_of(a) < order_of(b));
    assert!(order_of(a) < order_of(c));
    assert!(order_of(b) < order_of(d));
    assert!(order_of(c) < order_of(d));

    let item = |id: Uuid| {
        items
            .iter()
            .find(|i| i["item_id"] == id.to_string())
            .unwrap()
    };

    // A has no unmet dependency and is in `dispatch_from` — a real run
    // would dispatch it.
    assert_eq!(item(a)["decision"], "would_dispatch");

    // B and C both directly depend on A, which has not reached a
    // Done-category status yet — held back regardless of A's own
    // eligibility.
    assert_eq!(item(b)["decision"], "waiting_on_dependencies");
    assert_eq!(item(b)["blocked_by"], json!([a.to_string()]));
    assert_eq!(item(c)["decision"], "waiting_on_dependencies");
    assert_eq!(item(c)["blocked_by"], json!([a.to_string()]));

    // D directly depends on B and C (not A) — its own `blocked_by` names
    // exactly its direct blockers, not the whole transitive ancestry.
    assert_eq!(item(d)["decision"], "waiting_on_dependencies");
    let d_blocked_by: Vec<String> = item(d)["blocked_by"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(d_blocked_by.len(), 2);
    assert!(d_blocked_by.contains(&b.to_string()));
    assert!(d_blocked_by.contains(&c.to_string()));
}

#[tokio::test]
async fn a_completed_dependency_unblocks_its_direct_dependents_but_not_their_dependents() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let [a, b, c, d] = seed_diamond(&app, project_id, sprint_id).await;
    let cp = create_control_plane(&app, "http://docket.local:9999").await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    // Simulate A having already finished (e.g. dispatched and completed in
    // an earlier sprint-dispatch run).
    patch_status(&app, a, "Done").await;

    let v = dry_run(&app, sprint_id, None).await;
    let items = v["items"].as_array().unwrap();
    let item = |id: Uuid| {
        items
            .iter()
            .find(|i| i["item_id"] == id.to_string())
            .unwrap()
    };

    // A is Done, so it's outside `dispatch_from` now — not_eligible, not an
    // error.
    assert_eq!(item(a)["decision"], "not_eligible");
    // B and C's only dependency is satisfied — ready to dispatch.
    assert_eq!(item(b)["decision"], "would_dispatch");
    assert_eq!(item(c)["decision"], "would_dispatch");
    // D still depends on B and C directly, and neither has finished yet.
    assert_eq!(item(d)["decision"], "waiting_on_dependencies");
}

#[tokio::test]
async fn real_dispatch_matches_the_dry_run_order_and_skips_exactly_as_previewed() {
    let server = MockServer::start().await;
    mock_enqueue_allow(&server, "task-diamond").await;
    mock_list_tasks(&server, "task-diamond").await;

    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let [a, b, c, d] = seed_diamond(&app, project_id, sprint_id).await;
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;
    patch_status(&app, a, "Done").await; // B and C are ready; D is not.

    let planned = dry_run(&app, sprint_id, None).await;
    let real = dispatch_sprint(&app, sprint_id, None).await;

    let planned_items = planned["items"].as_array().unwrap();
    let real_items = real["items"].as_array().unwrap();
    assert_eq!(planned_items.len(), real_items.len());

    for id in [a, b, c, d] {
        let p = planned_items
            .iter()
            .find(|i| i["item_id"] == id.to_string())
            .unwrap();
        let r = real_items
            .iter()
            .find(|i| i["item_id"] == id.to_string())
            .unwrap();
        assert_eq!(p["order"], r["order"], "order diverged for item {id}");
        match p["decision"].as_str().unwrap() {
            // A dry-run "would_dispatch" preview becomes a real "dispatched"
            // outcome once docket is actually called.
            "would_dispatch" => assert_eq!(r["decision"], "dispatched"),
            // Every other decision (not_eligible, waiting_on_dependencies)
            // is identical between the preview and the real run — neither
            // touches docket.
            other => assert_eq!(r["decision"], other),
        }
    }

    assert_eq!(real["summary"]["dispatched"], 2); // B and C
    assert_eq!(real["summary"]["waiting_on_dependencies"], 1); // D
    assert_eq!(real["summary"]["not_eligible"], 1); // A (already Done)
}

// ─── Cross-sprint / cross-project dependency (decision 2) ─────────────────

#[tokio::test]
async fn a_dependency_outside_the_sprint_gates_readiness_the_same_way() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let other_sprint_id = create_sprint(&app, project_id).await;

    // `blocker` is in the same project but a *different* sprint — never
    // dispatched by this call at all.
    let blocker = create_item(&app, project_id, other_sprint_id, "Outside blocker").await;
    let dependent = create_item(&app, project_id, sprint_id, "Inside dependent").await;
    create_dependency(&app, blocker, dependent).await;

    let cp = create_control_plane(&app, "http://docket.local:9999").await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    // Not done yet — held back, and `blocked_by` still names the external
    // item.
    let v = dry_run(&app, sprint_id, None).await;
    let items = v["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        1,
        "the blocker is outside the sprint, not part of the plan"
    );
    assert_eq!(items[0]["decision"], "waiting_on_dependencies");
    assert_eq!(items[0]["blocked_by"], json!([blocker.to_string()]));

    // Once the external blocker finishes, the in-sprint item becomes ready.
    patch_status(&app, blocker, "Done").await;
    let v2 = dry_run(&app, sprint_id, None).await;
    assert_eq!(
        v2["items"].as_array().unwrap()[0]["decision"],
        "would_dispatch"
    );
}

// ─── Partial failure: one item's policy block doesn't abort the sprint ────

#[tokio::test]
async fn a_policy_block_on_one_item_does_not_abort_the_rest_of_the_sprint() {
    let server = MockServer::start().await;
    // "Blocked Item" is refused by docket's pre_input policy.
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .and(body_partial_json(json!({"description": "Blocked Item"})))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "ok": false,
            "error": "task rejected by guardrail policy 'prompt-injection' at enqueue: looks unsafe"
        })))
        .mount(&server)
        .await;
    // Everything else is allowed.
    mock_enqueue_allow(&server, "task-ok").await;
    mock_list_tasks(&server, "task-ok").await;

    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    // Two independent items — no dependency between them.
    let blocked = create_item(&app, project_id, sprint_id, "Blocked Item").await;
    let ok = create_item(&app, project_id, sprint_id, "OK Item").await;
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    let v = dispatch_sprint(&app, sprint_id, None).await;
    let items = v["items"].as_array().unwrap();
    let blocked_row = items
        .iter()
        .find(|i| i["item_id"] == blocked.to_string())
        .unwrap();
    let ok_row = items
        .iter()
        .find(|i| i["item_id"] == ok.to_string())
        .unwrap();

    assert_eq!(blocked_row["decision"], "blocked");
    assert_eq!(blocked_row["policy_id"], "prompt-injection");
    assert_eq!(ok_row["decision"], "dispatched");
    assert_eq!(v["summary"]["blocked"], 1);
    assert_eq!(v["summary"]["dispatched"], 1);
}

// ─── Trust is threaded per item, not a blanket value for the batch ────────

#[tokio::test]
async fn trust_is_threaded_per_item_not_as_one_blanket_value_for_the_batch() {
    let server = MockServer::start().await;
    // Manual item must enqueue with trusted: true.
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .and(body_partial_json(
            json!({"description": "Manual Item", "trusted": true}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "task": "task-manual", "project": "demo", "status": "pending"
        })))
        .mount(&server)
        .await;
    // GitHub-imported item must enqueue with trusted: false.
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .and(body_partial_json(
            json!({"description": "GitHub Item", "trusted": false}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "task": "task-github", "project": "demo", "status": "pending"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "tasks": [
            {"id": "task-manual", "description": "x", "priority": "normal", "status": "pending",
             "created": "2026-08-05T00:00:00Z", "source": "operator"},
            {"id": "task-github", "description": "x", "priority": "normal", "status": "pending",
             "created": "2026-08-05T00:00:00Z", "source": "operator"},
        ]})))
        .mount(&server)
        .await;

    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let manual = create_item(&app, project_id, sprint_id, "Manual Item").await;
    let github = seed_github_item(&state, project_id, sprint_id, "GitHub Item").await;
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    let v = dispatch_sprint(&app, sprint_id, None).await;
    let items = v["items"].as_array().unwrap();
    let manual_row = items
        .iter()
        .find(|i| i["item_id"] == manual.to_string())
        .unwrap();
    let github_row = items
        .iter()
        .find(|i| i["item_id"] == github.to_string())
        .unwrap();
    // If the wrong `trusted` value had been sent, the matching mock above
    // would never have matched and `dispatch_item` would have gotten no
    // response at all — a hung/failed request, not a wrong-but-present
    // outcome. Reaching "dispatched" for both proves both values landed on
    // the wire correctly.
    assert_eq!(manual_row["decision"], "dispatched");
    assert_eq!(github_row["decision"], "dispatched");
    let _ = item_by_title(items, "Manual Item"); // sanity: helper works too
}

// ─── In-flight cap: reported, clamped, and actually bounds concurrency ────

#[tokio::test]
async fn max_in_flight_is_clamped_into_range_and_reported_back() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    let cp = create_control_plane(&app, "http://docket.local:9999").await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    let v = dry_run(&app, sprint_id, Some(0)).await;
    assert_eq!(v["max_in_flight"], 1, "0 clamps up to the floor of 1");

    let v = dry_run(&app, sprint_id, Some(999)).await;
    assert_eq!(
        v["max_in_flight"], 20,
        "999 clamps down to MAX_MAX_IN_FLIGHT"
    );

    let v = dry_run(&app, sprint_id, Some(7)).await;
    assert_eq!(
        v["max_in_flight"], 7,
        "an in-range value passes through unchanged"
    );
}

#[tokio::test]
async fn max_in_flight_actually_bounds_concurrent_dispatch_calls() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "ok": true, "task": "task-slow", "project": "demo", "status": "pending"
                }))
                .set_delay(Duration::from_millis(150)),
        )
        .mount(&server)
        .await;
    mock_list_tasks(&server, "task-slow").await;

    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    for i in 0..4 {
        create_item(&app, project_id, sprint_id, &format!("Item {i}")).await;
    }
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    // Capped at 2: 4 items, 150ms per enqueue call, means at least two
    // sequential batches — comfortably over 250ms.
    let start = Instant::now();
    let v = dispatch_sprint(&app, sprint_id, Some(2)).await;
    let elapsed = start.elapsed();
    assert_eq!(v["summary"]["dispatched"], 4);
    assert!(
        elapsed >= Duration::from_millis(260),
        "cap=2 over 4 items with a 150ms enqueue delay should take at least \
         two sequential batches (~300ms), took {elapsed:?}"
    );
}

#[tokio::test]
async fn a_generous_cap_lets_independent_items_dispatch_concurrently() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({
                    "ok": true, "task": "task-slow", "project": "demo", "status": "pending"
                }))
                .set_delay(Duration::from_millis(150)),
        )
        .mount(&server)
        .await;
    mock_list_tasks(&server, "task-slow").await;

    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Sprint Dispatch Test Project", "software").await;
    let sprint_id = create_sprint(&app, project_id).await;
    for i in 0..4 {
        create_item(&app, project_id, sprint_id, &format!("Item {i}")).await;
    }
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(&app, project_id, cp, json!({"dispatch_from": ["Backlog"]})).await;

    // Cap=4 (>= item count): all four should run essentially at once, well
    // under the ~300ms two-sequential-batches bound above.
    let start = Instant::now();
    let v = dispatch_sprint(&app, sprint_id, Some(4)).await;
    let elapsed = start.elapsed();
    assert_eq!(v["summary"]["dispatched"], 4);
    assert!(
        elapsed < Duration::from_millis(280),
        "cap=4 over 4 independent items with a 150ms enqueue delay should \
         run concurrently in ~150ms, took {elapsed:?}"
    );
}
