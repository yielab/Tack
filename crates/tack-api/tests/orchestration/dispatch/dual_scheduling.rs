//! Collision tests across the two scheduling planes: the legacy Docket bridge
//! (`orch_tasks`) and the neutral runner-v1 domain (`execution_requests`). See
//! `crates/tack-orch/src/adapters/legacy_bridge.rs`'s module doc ("One
//! scheduling owner") for the policy proved here in both directions.
//!
//! Drives the real, mounted dispatch and `POST /api/executions` routes through
//! `build_router`, not a test-local scaffold: every "writes nothing" claim is
//! backed by a direct row-count assertion, not just a status code.

use crate::common;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use chrono::Utc;
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

// ─── Harness (mirrors orchestration/dispatch/item.rs's own `app_with_state`/
// `req`/`body_json` helpers, deliberately not shared — a private copy avoids
// coupling this file's tests to that file's helper signatures changing
// later; project creation goes through `common::create_project` instead) ──

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

async fn create_item(app: &Router, project_id: Uuid) -> Uuid {
    let res = req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/items"),
        Some(json!({"title": "Collision candidate"})),
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

async fn link_project(app: &Router, project_id: Uuid, control_plane_id: Uuid) {
    let res = req(
        app,
        Method::PUT,
        &format!("/api/projects/{project_id}/orch-link"),
        Some(json!({
            "control_plane_id": control_plane_id,
            "remote_project": "demo",
            "status_map": {"dispatch_from": ["Backlog"], "on_running": "In Progress"},
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

/// Inserts a minimal, valid `execution_requests` row directly rather than
/// going through the runner-v1 creation path
/// (`tack-api::handlers::executions`), which would require a registered
/// `agent_runners`/fleet fixture this file has no reason to build. A direct
/// row insert is the standard way this table is seeded elsewhere too, and
/// exercises exactly the column the guard below reads (`state`), nothing
/// more.
async fn insert_execution_request(state: &AppState, item_id: Uuid, request_state: &str) {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO execution_requests (
            id, item_id, idempotency_scope, idempotency_key, request_fingerprint,
            state, selector_kind, selector_id, agent_profile_snapshot,
            repository_snapshot, permission_policy, created_at, updated_at
         ) VALUES (?, ?, 'test-scope', ?, 'fp', ?, 'exact_runner', ?, '{}', '{}', '{}', ?, ?)",
    )
    .bind(&id)
    .bind(item_id.to_string())
    .bind(&id) // idempotency_key: unique per row, reuse the request id
    .bind(request_state)
    .bind(Uuid::new_v4().to_string()) // selector_id: no real runner needed for this test
    .bind(&now)
    .bind(&now)
    .execute(state.repo.pool())
    .await
    .expect("insert execution_requests fixture row");
}

async fn count_orch_tasks(state: &AppState) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM orch_tasks")
        .fetch_one(state.repo.pool())
        .await
        .unwrap();
    row.0
}

/// Inserts an `orch_tasks` row directly (bypassing the docket HTTP call, which
/// these tests have no need to mock) with the given `remote_status` — the exact
/// column the mirror-direction guard reads.
async fn insert_orch_task(
    state: &AppState,
    item_id: Uuid,
    remote_task_id: &str,
    remote_status: &str,
) {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO orch_tasks (
            item_id, remote_task_id, remote_status, attempt, dispatched_at,
            trusted, created_at, updated_at
         ) VALUES (?, ?, ?, 1, ?, 1, ?, ?)",
    )
    .bind(item_id.to_string())
    .bind(remote_task_id)
    .bind(remote_status)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(state.repo.pool())
    .await
    .expect("insert orch_tasks fixture row");
}

async fn count_execution_requests_for_item(state: &AppState, item_id: Uuid) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM execution_requests WHERE item_id = ?")
        .bind(item_id.to_string())
        .fetch_one(state.repo.pool())
        .await
        .unwrap();
    row.0
}

/// Full production enrollment flow (mirrors `wave2_gate.rs`'s own
/// `enroll_runner`, duplicated rather than imported to avoid coupling this file
/// to that one's helper signatures changing later) so `selector_kind:
/// "exact_runner"` resolves against a real, active runner, then the actual `POST
/// /api/executions` call this file's guard tests are driving at.
async fn attempt_create_execution(
    app: &Router,
    item_id: Uuid,
    idempotency_key: &str,
) -> axum::response::Response {
    let profile_res = req(
        app,
        Method::POST,
        "/api/agent-profiles",
        Some(json!({"name": "g1 profile", "instructions": "work safely"})),
    )
    .await;
    assert_eq!(profile_res.status(), StatusCode::OK);
    let agent_profile_id = body_json(profile_res).await["agent_profile_id"]
        .as_str()
        .unwrap()
        .to_string();

    let pending_res = req(
        app,
        Method::POST,
        "/api/runners/enrollment",
        Some(json!({"name": "g1-runner", "total_capacity": 1, "available_capacity": 1})),
    )
    .await;
    assert_eq!(pending_res.status(), StatusCode::OK);
    let pending = body_json(pending_res).await;
    let runner_id = pending["runner_id"].as_str().unwrap().to_string();
    let enrollment_token = pending["enrollment_token"].as_str().unwrap().to_string();

    let enroll_now = Utc::now().to_rfc3339();
    let enroll_res = req(
        app,
        Method::POST,
        "/api/runner/v1/enroll",
        Some(json!({
            "protocol_version": 1,
            "enrollment_token": enrollment_token,
            "runner_name": "g1-runner",
            "runner_version": "0.1.0",
            "capabilities": {
                "reported_at": enroll_now,
                "labels": {"os": "linux"},
                "concurrency": {"total": 1, "available": 1},
                "harnesses": [{
                    "harness_kind": "codex",
                    "installed_version": "1.2.3",
                    "probe_error": null,
                    "probed_at": enroll_now,
                    "model_combinations": [{
                        "model_provider": "openai",
                        "model_ids": ["opaque/model-g1"],
                        "discovery": "reported"
                    }],
                }],
                "features": {},
                "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
            },
        })),
    )
    .await;
    assert_eq!(
        enroll_res.status(),
        StatusCode::OK,
        "{:?}",
        body_json(enroll_res).await
    );

    req(
        app,
        Method::POST,
        "/api/executions",
        Some(json!({
            "item_id": item_id,
            "idempotency_key": idempotency_key,
            "selector_kind": "exact_runner",
            "selector_id": runner_id,
            "agent_profile_id": agent_profile_id,
            "requested_harness_kind": "codex",
            "requested_model_provider": "openai",
            "requested_model_id": "opaque/model-g1",
            "agent_profile_snapshot": {"name": "profile", "instructions": "work safely", "tool_policy": {}, "timeout_seconds": 60, "budgets": {}},
            "repository_snapshot": {"kind": "git", "remote": "https://example.test/g1.git", "base_revision": "deadbeef", "subdirectory": null},
            "permission_policy": {"tools": ["shell"], "network": false},
            "timeout_seconds": 60,
            "budgets": {},
            "environment": {},
            "metadata": {},
        })),
    )
    .await
}

// ─── The fix: runner-v1 active blocks legacy Docket dispatch ──────────────────

/// Whether a legacy Docket redispatch is blocked depends only on the
/// runner-v1 request's `state` column, not on whether a row exists at all:
/// an active (`running`) request blocks it and writes nothing to the legacy
/// table even though the mocked docket server would happily accept the task
/// (proven by temporarily reverting the guard during review); a terminal
/// (`succeeded`) one does not, and a real `orch_tasks` row lands.
#[tokio::test]
async fn dispatch_blocks_only_on_active_runner_v1_request() {
    struct Case {
        request_state: &'static str,
        expect_status: StatusCode,
        expect_orch_tasks: i64,
    }
    let cases = [
        Case {
            request_state: "running",
            expect_status: StatusCode::CONFLICT,
            expect_orch_tasks: 0,
        },
        Case {
            request_state: "succeeded",
            expect_status: StatusCode::OK,
            expect_orch_tasks: 1,
        },
    ];

    for case in cases {
        let (app, state) = app_with_state(orch_config()).await;
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tasks/demo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "task": "remote-task", "project": "demo", "status": "pending"
            })))
            .mount(&mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/tasks/demo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tasks": [{
                    "id": "remote-task", "description": "x", "priority": "normal",
                    "status": "pending", "created": "2026-08-05T00:00:00Z", "source": "operator",
                }]
            })))
            .mount(&mock)
            .await;

        let project_id = common::create_project(&app, "Dual Dispatch Test", "software").await;
        let item_id = create_item(&app, project_id).await;
        let cp = create_control_plane(&app, &mock.uri()).await;
        link_project(&app, project_id, cp).await;

        insert_execution_request(&state, item_id, case.request_state).await;

        let res = dispatch(&app, item_id).await;
        let status = res.status();
        assert_eq!(
            status,
            case.expect_status,
            "state {}: {:?}",
            case.request_state,
            body_json(res).await
        );
        assert_eq!(
            count_orch_tasks(&state).await,
            case.expect_orch_tasks,
            "state {}: orch_tasks row count",
            case.request_state
        );
    }
}

/// Direct, unit-level proof of the read the guard above is built on —
/// isolates the "terminal doesn't count as active" claim from any
/// HTTP/docket-transport noise the full-router test above can't fully
/// separate out.
#[tokio::test]
async fn active_execution_request_read_ignores_terminal_states() {
    let (_app, state) = app_with_state(orch_config()).await;
    let project_id_res = req(
        &_app,
        Method::POST,
        "/api/projects",
        Some(json!({"name": "p", "project_type": "software"})),
    )
    .await;
    let project_id =
        Uuid::parse_str(body_json(project_id_res).await["id"].as_str().unwrap()).unwrap();
    let item_id = create_item(&_app, project_id).await;

    assert!(
        !state
            .repo
            .has_active_execution_request_for_item(item_id)
            .await
            .unwrap(),
        "no rows yet: must be false"
    );

    for terminal in ["succeeded", "failed", "cancelled"] {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO execution_requests (
                id, item_id, idempotency_scope, idempotency_key, request_fingerprint,
                state, selector_kind, selector_id, agent_profile_snapshot,
                repository_snapshot, permission_policy, created_at, updated_at
             ) VALUES (?, ?, ?, ?, 'fp', ?, 'exact_runner', ?, '{}', '{}', '{}', ?, ?)",
        )
        .bind(&id)
        .bind(item_id.to_string())
        .bind(format!("scope-{terminal}"))
        .bind(&id)
        .bind(terminal)
        .bind(Uuid::new_v4().to_string())
        .bind(&now)
        .bind(&now)
        .execute(state.repo.pool())
        .await
        .unwrap();
    }
    assert!(
        !state
            .repo
            .has_active_execution_request_for_item(item_id)
            .await
            .unwrap(),
        "three terminal rows must still read as inactive"
    );

    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO execution_requests (
            id, item_id, idempotency_scope, idempotency_key, request_fingerprint,
            state, selector_kind, selector_id, agent_profile_snapshot,
            repository_snapshot, permission_policy, created_at, updated_at
         ) VALUES (?, ?, 'scope-active', ?, 'fp', 'needs_operator', 'exact_runner', ?, '{}', '{}', '{}', ?, ?)",
    )
    .bind(&id)
    .bind(item_id.to_string())
    .bind(&id)
    .bind(Uuid::new_v4().to_string())
    .bind(&now)
    .bind(&now)
    .execute(state.repo.pool())
    .await
    .unwrap();

    assert!(
        state
            .repo
            .has_active_execution_request_for_item(item_id)
            .await
            .unwrap(),
        "needs_operator counts as active — it is not one of the three terminal states"
    );
}

// ─── The mirror direction: runner-v1 request creation defers to an active
// legacy Docket task ─────────────────────────────────────────────────────────

/// Direct, unit-level proof of the read the mirror guard is built on —
/// isolates the "which `remote_status` values count as active" claim from any
/// HTTP/enrollment noise the full-router tests below can't fully separate out.
/// Asserts on the full `Option` (not just its `.is_some()`), so this also pins
/// which task and status a caller actually gets back once one is active.
#[tokio::test]
async fn active_docket_task_for_item_ignores_terminal_statuses() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Dual Dispatch Test", "software").await;
    let item_id = create_item(&app, project_id).await;

    assert_eq!(
        state
            .repo
            .active_docket_task_for_item(item_id)
            .await
            .unwrap(),
        None,
        "no rows yet: must be None"
    );

    for (task_id, status) in [
        ("t-completed", "completed"),
        ("t-failed", "failed"),
        ("t-stale", "stale"),
        ("t-unknown", "some_future_docket_status"),
    ] {
        insert_orch_task(&state, item_id, task_id, status).await;
    }
    assert_eq!(
        state
            .repo
            .active_docket_task_for_item(item_id)
            .await
            .unwrap(),
        None,
        "terminal, stale, and unrecognised statuses must all read as inactive"
    );

    insert_orch_task(&state, item_id, "t-running", "running").await;
    assert_eq!(
        state
            .repo
            .active_docket_task_for_item(item_id)
            .await
            .unwrap(),
        Some(("t-running".to_string(), "running".to_string())),
        "running counts as active, and is the task named"
    );
}

/// The fix this file exists for, and the trap it must not fall into: a new
/// runner-v1 request is blocked only when both an active (`running`) legacy
/// Docket task exists AND orchestration is on — a terminal (`completed`) task
/// never blocks, and with orchestration off (`AppConfig::default()` —
/// `TACK_ORCH_ENABLE` unset) even a row stranded `running` by a previously
/// enabled bridge must never block (blocking it would invert "runner-v1 is
/// the plan of record"). Drives the real, mounted `POST /api/executions`
/// handler through a full enrollment flow — no shortcuts — and proves each
/// case by row count, not just a status code: reverting the guard makes the
/// first case's final assertion fail (`execution_requests` gains a row it
/// must not).
#[tokio::test]
async fn create_execution_blocks_only_active_docket_task_with_orch_on() {
    struct Case {
        orch_enabled: bool,
        docket_status: &'static str,
        expect_status: StatusCode,
        expect_count: i64,
    }
    let cases = [
        Case {
            orch_enabled: true,
            docket_status: "running",
            expect_status: StatusCode::CONFLICT,
            expect_count: 0,
        },
        Case {
            orch_enabled: true,
            docket_status: "completed",
            expect_status: StatusCode::OK,
            expect_count: 1,
        },
        Case {
            orch_enabled: false,
            docket_status: "running",
            expect_status: StatusCode::OK,
            expect_count: 1,
        },
    ];

    for case in cases {
        let config = if case.orch_enabled {
            orch_config()
        } else {
            AppConfig::default()
        };
        let (app, state) = app_with_state(config).await;
        let project_id = common::create_project(&app, "Dual Dispatch Test", "software").await;
        let item_id = create_item(&app, project_id).await;

        insert_orch_task(&state, item_id, "remote-task", case.docket_status).await;

        let res = attempt_create_execution(&app, item_id, "mirror-guard").await;
        let status = res.status();
        assert_eq!(
            status,
            case.expect_status,
            "orch_enabled={} status={}: {:?}",
            case.orch_enabled,
            case.docket_status,
            body_json(res).await
        );
        assert_eq!(
            count_execution_requests_for_item(&state, item_id).await,
            case.expect_count,
            "orch_enabled={} status={}: execution_requests row count",
            case.orch_enabled,
            case.docket_status
        );
    }
}

// ─── The idempotent-replay case: the mirror guard must never turn a client's
// retry of its own prior create into a new conflict ─────────────────────────

/// The enrollment half of `attempt_create_execution`, split out so a replay
/// test can submit the exact same `agent_profile_id`/`runner_id` twice — an
/// idempotent replay only reaches the durable replay record when the stored
/// request snapshot matches byte-for-byte, so two independently enrolled
/// runners (what calling `attempt_create_execution` twice would do) would
/// hit `idempotency_conflict`, not a replay.
async fn enroll_g1_runner(app: &Router) -> (String, String) {
    let profile_res = req(
        app,
        Method::POST,
        "/api/agent-profiles",
        Some(json!({"name": "g1 profile", "instructions": "work safely"})),
    )
    .await;
    assert_eq!(profile_res.status(), StatusCode::OK);
    let agent_profile_id = body_json(profile_res).await["agent_profile_id"]
        .as_str()
        .unwrap()
        .to_string();

    let pending_res = req(
        app,
        Method::POST,
        "/api/runners/enrollment",
        Some(json!({"name": "g1-runner", "total_capacity": 1, "available_capacity": 1})),
    )
    .await;
    assert_eq!(pending_res.status(), StatusCode::OK);
    let pending = body_json(pending_res).await;
    let runner_id = pending["runner_id"].as_str().unwrap().to_string();
    let enrollment_token = pending["enrollment_token"].as_str().unwrap().to_string();

    let enroll_now = Utc::now().to_rfc3339();
    let enroll_res = req(
        app,
        Method::POST,
        "/api/runner/v1/enroll",
        Some(json!({
            "protocol_version": 1,
            "enrollment_token": enrollment_token,
            "runner_name": "g1-runner",
            "runner_version": "0.1.0",
            "capabilities": {
                "reported_at": enroll_now,
                "labels": {"os": "linux"},
                "concurrency": {"total": 1, "available": 1},
                "harnesses": [{
                    "harness_kind": "codex",
                    "installed_version": "1.2.3",
                    "probe_error": null,
                    "probed_at": enroll_now,
                    "model_combinations": [{
                        "model_provider": "openai",
                        "model_ids": ["opaque/model-g1"],
                        "discovery": "reported"
                    }],
                }],
                "features": {},
                "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
            },
        })),
    )
    .await;
    assert_eq!(
        enroll_res.status(),
        StatusCode::OK,
        "{:?}",
        body_json(enroll_res).await
    );

    (agent_profile_id, runner_id)
}

/// Submits the exact create-execution body `attempt_create_execution` uses,
/// against a caller-supplied (already enrolled) profile/runner pair, so a
/// caller can submit the identical request twice for a genuine idempotent
/// replay rather than two independently enrolled ones.
async fn submit_execution_request(
    app: &Router,
    item_id: Uuid,
    idempotency_key: &str,
    agent_profile_id: &str,
    runner_id: &str,
) -> axum::response::Response {
    req(
        app,
        Method::POST,
        "/api/executions",
        Some(json!({
            "item_id": item_id,
            "idempotency_key": idempotency_key,
            "selector_kind": "exact_runner",
            "selector_id": runner_id,
            "agent_profile_id": agent_profile_id,
            "requested_harness_kind": "codex",
            "requested_model_provider": "openai",
            "requested_model_id": "opaque/model-g1",
            "agent_profile_snapshot": {"name": "profile", "instructions": "work safely", "tool_policy": {}, "timeout_seconds": 60, "budgets": {}},
            "repository_snapshot": {"kind": "git", "remote": "https://example.test/g1.git", "base_revision": "deadbeef", "subdirectory": null},
            "permission_policy": {"tools": ["shell"], "network": false},
            "timeout_seconds": 60,
            "budgets": {},
            "environment": {},
            "metadata": {},
        })),
    )
    .await
}

/// The guard's `existing_snapshot.is_none()` arm exists so a client retrying a
/// create it already made never starts getting a `409` because Docket claimed
/// the item in between. This is that path, end to end: the first call creates
/// the request while no Docket task exists; a Docket task then goes active on
/// the same item; the exact same request (same idempotency key) is replayed
/// and must still succeed, reaching the durable replay record rather than the
/// mirror guard. Row count stays `1` — the replay must not create a second
/// row, and the guard must not have refused it either.
#[tokio::test]
async fn execution_replay_succeeds_despite_active_docket_task() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Dual Dispatch Test", "software").await;
    let item_id = create_item(&app, project_id).await;
    let (agent_profile_id, runner_id) = enroll_g1_runner(&app).await;

    let first =
        submit_execution_request(&app, item_id, "replay-key", &agent_profile_id, &runner_id).await;
    assert_eq!(
        first.status(),
        StatusCode::OK,
        "{:?}",
        body_json(first).await
    );

    insert_orch_task(&state, item_id, "remote-task-after-create", "running").await;

    let replay =
        submit_execution_request(&app, item_id, "replay-key", &agent_profile_id, &runner_id).await;
    let replay_status = replay.status();
    let replay_body = body_json(replay).await;
    assert_eq!(
        replay_status,
        StatusCode::OK,
        "a replay of an existing request must never be blocked by a Docket task that \
         went active after the original create: {replay_body:?}"
    );
    assert_eq!(
        replay_body["replayed"],
        Value::Bool(true),
        "{replay_body:?}"
    );
    assert_eq!(
        count_execution_requests_for_item(&state, item_id).await,
        1,
        "the replay must not create a second execution_requests row"
    );
}

/// The `409` a fresh (non-replay) create gets while the item has an active
/// Docket task names *which* task collided and its status, not only the
/// `item_id` — before this, diagnosing a collision meant reading `orch_tasks`
/// by hand.
#[tokio::test]
async fn create_execution_conflict_names_the_colliding_docket_task() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Dual Dispatch Test", "software").await;
    let item_id = create_item(&app, project_id).await;

    insert_orch_task(&state, item_id, "remote-task-named", "waiting_approval").await;

    let res = attempt_create_execution(&app, item_id, "mirror-guard-named").await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(
        body["error"]["details"]["docket_task_id"],
        Value::String("remote-task-named".to_string()),
        "{body:?}"
    );
    assert_eq!(
        body["error"]["details"]["docket_task_status"],
        Value::String("waiting_approval".to_string()),
        "{body:?}"
    );
}
