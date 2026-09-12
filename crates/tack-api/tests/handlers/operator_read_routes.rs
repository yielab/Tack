//! Focused tests for the two operator read routes `GET /api/runners` and
//! `GET /api/executions/{id}/attempts` (+ `/attempts/{n}/events`) —
//! proving they return real data through the actual production router
//! (`tack_api::router::build_router`, exactly what `tack serve` mounts),
//! not a test-local stand-in, including the honest "empty, not yet" and
//! 404 cases.

use crate::common;
use axum::http::StatusCode;
use chrono::Utc;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, orch_runtime::OrchRuntime, router::build_router};
use tack_db::{Repository, init_pool, migrations};
use uuid::Uuid;

const OPERATOR_TOKEN: &str = "e6-routes-operator-token";
const BASE_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

async fn setup() -> (axum::Router, Repository, String) {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'E6 routes', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("workspace");
    let repo = Repository::new(pool.clone());
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        repo: repo.clone(),
        config: AppConfig {
            api_token: Some(OPERATOR_TOKEN.into()),
            database_url: "sqlite::memory:".into(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        orch_runtime: OrchRuntime::new(),
        local_runner: None,
    };
    let app = build_router(state);
    let project = repo
        .create_project(
            workspace_id,
            tack_core::models::CreateProject {
                name: "E6 routes".into(),
                description: None,
                project_type: tack_core::models::ProjectType::Software,
                template: None,
            },
        )
        .await
        .expect("project");
    let item = repo
        .create_item(
            project.id,
            "To Do",
            tack_core::models::CreateItem {
                title: "Prove the new read routes".into(),
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
        )
        .await
        .expect("item");
    (app, repo, item.id.to_string())
}

fn operator_headers() -> Vec<(&'static str, &'static str)> {
    vec![("authorization", "Bearer e6-routes-operator-token")]
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn full_capabilities() -> Value {
    let now = Utc::now().to_rfc3339();
    json!({
        "reported_at": now,
        "labels": {"os": "linux"},
        "concurrency": {"total": 1, "available": 1},
        "harnesses": [{
            "harness_kind": "codex",
            "installed_version": "1.0.0",
            "probe_error": null,
            "probed_at": now,
            "model_combinations": [{
                "model_provider": "openai",
                "model_ids": ["opaque/model-e6"],
                "discovery": "reported"
            }],
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

/// Enrolls a runner and returns (runner_id, bearer-auth-header-pair).
async fn enroll_runner(app: &axum::Router, name: &str) -> (String, [(String, String); 1]) {
    let (status, pending) = common::send(
        app,
        "POST",
        "/api/runners/enrollment",
        json!({"name": name, "total_capacity": 1, "available_capacity": 1}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    let runner_id = pending["runner_id"].as_str().unwrap().to_owned();
    let raw_token = pending["enrollment_token"].as_str().unwrap().to_owned();

    let (status, enrolled) = common::send(
        app,
        "POST",
        "/api/runner/v1/enroll",
        json!({
            "protocol_version": 1,
            "enrollment_token": raw_token,
            "runner_name": name,
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(),
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();
    (
        runner_id,
        [("authorization".to_string(), bearer(&credential))],
    )
}

fn headers_ref(owned: &[(String, String); 1]) -> Vec<(&str, &str)> {
    owned
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect()
}

async fn create_agent_profile(app: &axum::Router) -> String {
    let (status, profile) = common::send(
        app,
        "POST",
        "/api/agent-profiles",
        json!({"name": "E6 routes profile", "instructions": "work safely"}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    profile["agent_profile_id"].as_str().unwrap().to_owned()
}

fn execution_request_body(
    item_id: &str,
    key: &str,
    runner_id: &str,
    agent_profile_id: &str,
) -> Value {
    json!({
        "item_id": item_id,
        "idempotency_key": key,
        "selector_kind": "exact_runner",
        "selector_id": runner_id,
        "agent_profile_id": agent_profile_id,
        "requested_harness_kind": "codex",
        "requested_model_provider": "openai",
        "requested_model_id": "opaque/model-e6",
        "agent_profile_snapshot": {"name": "profile", "instructions": "work safely", "tool_policy": {}, "timeout_seconds": 60, "budgets": {}},
        "repository_snapshot": {"kind": "git", "remote": "https://example.test/e6.git", "base_revision": BASE_REVISION, "subdirectory": null},
        "permission_policy": {"tools": ["shell"], "network": false},
        "timeout_seconds": 60,
        "budgets": {},
        "environment": {},
        "metadata": {},
    })
}

// =======================================================================
// GET /api/runners
// =======================================================================

#[tokio::test]
async fn list_runners_returns_real_capability_and_capacity_data() {
    let (app, _repo, _item_id) = setup().await;
    let (runner_id, _auth) = enroll_runner(&app, "Runner A").await;

    let (status, listed) = common::send(
        &app,
        "GET",
        "/api/runners",
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let data = listed["data"].as_array().expect("data array");
    assert_eq!(data.len(), 1);
    let runner = &data[0];
    assert_eq!(runner["runner_id"], runner_id);
    assert_eq!(runner["state"], "active");
    assert_eq!(runner["total_capacity"], 1);
    assert_eq!(runner["available_capacity"], 1);
    assert_eq!(
        runner["capability_snapshot"]["harnesses"][0]["harness_kind"], "codex",
        "the real declared capability snapshot must round-trip, not a placeholder"
    );
    assert_eq!(runner["fleet_ids"], json!([]));
}

#[tokio::test]
async fn list_runners_fleet_filter_only_returns_members() {
    let (app, _repo, _item_id) = setup().await;
    let (runner_in, _auth_in) = enroll_runner(&app, "In Fleet").await;
    let (runner_out, _auth_out) = enroll_runner(&app, "Not In Fleet").await;

    let (status, fleet) = common::send(
        &app,
        "POST",
        "/api/runner-fleets",
        json!({"name": "Filter Fleet"}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fleet}");
    let fleet_id = fleet["fleet_id"].as_str().unwrap().to_owned();

    // Inserts directly (rather than through
    // `POST /runner-fleets/{fleet_id}/members`) to keep this fixture
    // focused on filtering, not fleet-membership plumbing.
    sqlx::query(
        "INSERT INTO agent_fleet_members (fleet_id, runner_id, created_at) VALUES (?, ?, ?)",
    )
    .bind(&fleet_id)
    .bind(&runner_in)
    .bind(Utc::now().to_rfc3339())
    .execute(_repo.pool())
    .await
    .expect("membership");

    let (status, filtered) = common::send(
        &app,
        "GET",
        &format!("/api/runners?fleet_id={fleet_id}"),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{filtered}");
    let data = filtered["data"].as_array().expect("data array");
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["runner_id"], runner_in);
    assert_ne!(data[0]["runner_id"], runner_out);
}

#[tokio::test]
async fn list_runners_requires_operator_auth() {
    let (app, _repo, _item_id) = setup().await;
    let (status, _) = common::send(&app, "GET", "/api/runners", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// =======================================================================
// GET /api/executions/{id}/attempts and .../attempts/{n}/events
// =======================================================================

#[tokio::test]
async fn attempts_are_empty_before_a_claim_and_populated_after() {
    let (app, _repo, item_id) = setup().await;
    let (runner_id, auth_owned) = enroll_runner(&app, "Attempts Runner").await;
    let auth = headers_ref(&auth_owned);
    let agent_profile_id = create_agent_profile(&app).await;

    let (status, created) = common::send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(&item_id, "attempts-key", &runner_id, &agent_profile_id),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    // Before any claim: a real, empty list — not a placeholder.
    let (status, before) = common::send(
        &app,
        "GET",
        &format!("/api/executions/{request_id}/attempts"),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{before}");
    assert_eq!(before["data"], json!([]));

    let (status, claimed) = common::send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": "attempts-claim", "available_capacity": 1, "wait_ms": 0}),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();

    let (status, after) = common::send(
        &app,
        "GET",
        &format!("/api/executions/{request_id}/attempts"),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{after}");
    let data = after["data"].as_array().expect("data array");
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["attempt_id"], attempt_id);
    assert_eq!(data[0]["attempt_number"], 1);
    assert_eq!(data[0]["runner_id"], runner_id);
    assert_eq!(data[0]["state"], "leased");
}

#[tokio::test]
async fn attempts_for_an_unknown_request_id_is_404() {
    let (app, _repo, _item_id) = setup().await;
    let (status, body) = common::send(
        &app,
        "GET",
        "/api/executions/does-not-exist/attempts",
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn events_reflect_a_real_batch_then_unknown_attempt_is_404() {
    let (app, repo, item_id) = setup().await;
    let (runner_id, auth_owned) = enroll_runner(&app, "Events Runner").await;
    let auth = headers_ref(&auth_owned);
    let agent_profile_id = create_agent_profile(&app).await;

    let (status, created) = common::send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(&item_id, "events-key", &runner_id, &agent_profile_id),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    let (status, claimed) = common::send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": "events-claim", "available_capacity": 1, "wait_ms": 0}),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    // Attempt number 1 exists but has no events yet — an honest empty list.
    let (status, before) = common::send(
        &app,
        "GET",
        &format!("/api/executions/{request_id}/attempts/1/events"),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{before}");
    assert_eq!(before["data"], json!([]));

    let (status, batch) = common::send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/events"),
        json!({
            "protocol_version": 1,
            "runner_id": runner_id,
            "attempt_id": attempt_id,
            "fencing_token": fencing_token,
            "checkpoint": "cp-1",
            "previous_checkpoint": Value::Null,
            "events": [{
                "event_id": "evt-1",
                "sequence": 1,
                "source": "runner",
                "kind": "log",
                "payload": {"message": "hello from the real harness"},
                "occurred_at": Utc::now().to_rfc3339(),
            }],
        }),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batch}");

    let (status, after) = common::send(
        &app,
        "GET",
        &format!("/api/executions/{request_id}/attempts/1/events"),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{after}");
    let data = after["data"].as_array().expect("data array");
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["event_id"], "evt-1");
    assert_eq!(data[0]["kind"], "log");
    assert_eq!(data[0]["payload"]["message"], "hello from the real harness");

    let events_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id = ?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(
        events_count, 1,
        "the read route must reflect a real persisted row, not a mock"
    );

    // Attempt number 2 was never claimed for this request — distinct 404.
    let (status, body) = common::send(
        &app,
        "GET",
        &format!("/api/executions/{request_id}/attempts/2/events"),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["details"]["resource"], "execution_attempt");
}
