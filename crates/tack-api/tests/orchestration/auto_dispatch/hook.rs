//! Tests for the auto-dispatch hook:
//! `handlers::items::maybe_auto_dispatch`, wired into `PATCH /api/items/{id}`
//! beside `maybe_sync_github`/`propagate_parent_completion`. When
//! `orch_links.auto_dispatch` is on and an item's status changes into one of
//! `status_map.dispatch_from`, the hook calls `dispatcher::dispatch_item`
//! off the request path, passing the item's own **persisted** trust value
//! (`item.source.is_trusted()` — migration 029; see the trust test below for
//! why that matters).
//!
//! Also covers: the off/auto_dispatch-off guards, and that it must not
//! dispatch on every update — an item edited while already sitting in a
//! `dispatch_from` status must not re-dispatch.

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

/// Mounts the dispatch POST/GET pair, matching on `trusted` in the POST
/// body so the mock only lets the dispatcher through if the flag genuinely
/// matches `expect_trusted`.
async fn mount_dispatch_mocks(server: &MockServer, task_id: &str, expect_trusted: bool) {
    Mock::given(method("POST"))
        .and(path("/tasks/demo"))
        .and(body_partial_json(json!({"trusted": expect_trusted})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "task": task_id, "project": "demo", "status": "pending"
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tasks/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mock_list_tasks_body(task_id)))
        .mount(server)
        .await;
}

/// A project + auto-dispatch-linked item, ready to be moved into `dispatch_from`
/// — the setup every trust-flag case shares, differing only in the seeded
/// item's source and the mocked task id/trust value.
async fn setup_auto_dispatch_case(
    source: ItemSource,
    title: &str,
    task_id: &str,
    expect_trusted: bool,
) -> (Router, AppState, Uuid, MockServer) {
    let server = MockServer::start().await;
    mount_dispatch_mocks(&server, task_id, expect_trusted).await;
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Auto-dispatch Test Project", "software").await;
    let item_id = seed_item(&state, project_id, "Backlog", source, title).await;
    link_project(
        &state,
        project_id,
        &server.uri(),
        true, // auto_dispatch
        json!({"dispatch_from": ["To Do"], "on_running": "In Progress"}),
    )
    .await;
    (app, state, item_id, server)
}

/// The hook runs on a background `tokio::spawn`, doing real (loopback) HTTP
/// I/O against `server`. A plain `tokio::task::yield_now()` retry loop is
/// not enough here: it only re-queues this task, it never forces the
/// current-thread runtime to park and let its I/O driver poll for the
/// spawn's actual socket readiness, so the spawn can starve indefinitely.
/// `Interval::tick` does force that park on every iteration, which is the
/// bounded-poll mechanism this waits on. Returns the number of matching
/// hits observed.
async fn wait_for_hits(server: &MockServer, path_suffix: &str, at_least: usize) -> usize {
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(25));
    for _ in 0..80 {
        ticker.tick().await;
        let reqs = server.received_requests().await.unwrap_or_default();
        let count = reqs
            .iter()
            .filter(|r| r.url.path().ends_with(path_suffix))
            .count();
        if count >= at_least {
            return count;
        }
    }
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.url.path().ends_with(path_suffix))
        .count()
}

/// Poll `list_orch_tasks_for_item`, bounded the same way and for the same
/// reason as `wait_for_hits` — used instead of it when a test needs to
/// assert on the persisted task (e.g. its `trusted` column), not just that
/// a request landed.
async fn wait_for_orch_task(state: &AppState, item_id: Uuid) -> Vec<tack_db::repo::orch::OrchTask> {
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(25));
    for _ in 0..80 {
        ticker.tick().await;
        let tasks = state.repo.list_orch_tasks_for_item(item_id).await.unwrap();
        if !tasks.is_empty() {
            return tasks;
        }
    }
    state.repo.list_orch_tasks_for_item(item_id).await.unwrap()
}

/// Waits out a bounded, real settle window so a wrongly-firing or
/// wrongly-refiring background spawn has a genuine chance to land before an
/// absence/steady-state assertion is trusted — always the full window, not
/// an early-exit poll, since there is no single condition to poll for here
/// (some callers already have one prior hit on record and are watching for
/// a second that must never come). Uses `Interval::tick`, not a bare
/// `yield_now` loop, for the same reason `wait_for_hits` does: only a real
/// timer forces this current-thread runtime to park and let its I/O driver
/// service the spawn's actual socket I/O.
async fn drain_background_spawns() {
    let mut ticker = tokio::time::interval(std::time::Duration::from_millis(25));
    for _ in 0..8 {
        ticker.tick().await;
    }
}

// ─── The headline claim: persisted trust reaches the wire, per item source ──

/// A GitHub-imported item's persisted trust (migration 029) reaches docket
/// as `trusted: false`; a manually created item's reaches it as `true` —
/// asserted with a wiremock matcher that only responds 200 (letting the
/// dispatcher proceed to persist an `orch_tasks` row) if the flag is
/// genuinely present in the request body, not by inspecting what a
/// function was called with. The false case is the one that matters most:
/// it's why this hook reads persisted trust at all, not a per-call flag.
#[tokio::test]
async fn auto_dispatch_sends_persisted_trust_flag_on_the_wire() {
    struct Case {
        source: ItemSource,
        expect_trusted: bool,
        task_id: &'static str,
        title: &'static str,
    }
    let cases = [
        Case {
            source: ItemSource::Github,
            expect_trusted: false,
            task_id: "task-auto-untrusted",
            title: "Imported from GitHub",
        },
        Case {
            source: ItemSource::Manual,
            expect_trusted: true,
            task_id: "task-auto-trusted",
            title: "Typed directly in Tack",
        },
    ];

    for case in cases {
        let (app, state, item_id, _server) = setup_auto_dispatch_case(
            case.source.clone(),
            case.title,
            case.task_id,
            case.expect_trusted,
        )
        .await;

        let res = patch_status(&app, item_id, "To Do").await;
        assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);

        let tasks = wait_for_orch_task(&state, item_id).await;
        assert_eq!(tasks.len(), 1, "expected exactly one auto-dispatched task");
        assert_eq!(tasks[0].remote_task_id, case.task_id);
        assert_eq!(
            tasks[0].trusted, case.expect_trusted,
            "{:?} item must dispatch as trusted={}",
            case.source, case.expect_trusted
        );

        // The mapped status (on_running) applies through the engine either way.
        let item = state.repo.get_item(item_id).await.unwrap().unwrap();
        assert_eq!(item.status, "In Progress");
    }
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

    // Give a wrongly-firing hook a chance to show up, then assert nothing did.
    drain_background_spawns().await;
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

    drain_background_spawns().await;
    let hits = server.received_requests().await.unwrap_or_default();
    assert!(
        hits.is_empty(),
        "auto_dispatch: false on the link must mean no auto-dispatch: {hits:?}"
    );
}

// ─── Don't dispatch on every update ─────────────────────────────────────────

#[tokio::test]
async fn auto_dispatch_does_not_refire_on_edit_with_no_status_change() {
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

    // Give a wrongly-refiring hook a chance to show up.
    drain_background_spawns().await;

    let tasks = state.repo.list_orch_tasks_for_item(item_id).await.unwrap();
    assert_eq!(
        tasks.len(),
        1,
        "editing an item already past its dispatch_from status must not create a second task"
    );

    server.verify().await; // asserts the mock's .expect(1) held
}
