//! `agent_fleet_members` (migration 041) is both a live scheduling *read*
//! input — the claim path joins it to resolve a `selector_kind = "fleet"`
//! request onto any of the fleet's runners — and now writable: an operator
//! route populates a fleet's roster.
//!
//! This file proves that end to end, directly against the real production
//! router (`tack_api::router::build_router`, exactly what `tack serve`
//! mounts): an operator can populate a fleet over the API, and a
//! fleet-targeted request then schedules onto a runner that is a member of
//! that fleet — not merely that the write endpoint itself returns 200.

use crate::common;
use axum::http::StatusCode;
use chrono::Utc;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, orch_runtime::OrchRuntime, router::build_router};
use tack_db::{Repository, init_pool, migrations};
use uuid::Uuid;

const OPERATOR_TOKEN: &str = "h8-fleet-membership-operator-token";
const BASE_REVISION: &str = "abc123def456abc123def456abc123def456abc";

async fn fresh_database() -> (sqlx::SqlitePool, Uuid) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'H8FleetMembership', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert workspace");
    (pool, workspace_id)
}

async fn router_for(pool: sqlx::SqlitePool, workspace_id: Uuid) -> axum::Router {
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        repo: Repository::new(pool),
        config: AppConfig {
            api_token: Some(OPERATOR_TOKEN.into()),
            database_url: "sqlite::memory:".to_string(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        orch_runtime: OrchRuntime::new(),
        local_runner: None,
    };
    build_router(state)
}

fn operator_headers() -> Vec<(&'static str, &'static str)> {
    vec![("authorization", "Bearer h8-fleet-membership-operator-token")]
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// Every fixture in this file declares this exact harness/model pairing so
/// the real scheduler treats it as eligible — matching the convention
/// `wave2_gate.rs` uses for the same reason.
fn full_capabilities() -> Value {
    let now = Utc::now().to_rfc3339();
    json!({
        "reported_at": now,
        "labels": {"os": "linux"},
        "concurrency": {"total": 1, "available": 1},
        "harnesses": [{
            "harness_kind": "codex",
            "installed_version": "1.2.3",
            "probe_error": null,
            "probed_at": now,
            "model_combinations": [{
                "model_provider": "openai",
                "model_ids": ["opaque/model-h8"],
                "discovery": "reported"
            }],
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

async fn create_project_and_item(app: &axum::Router) -> String {
    let (status, project) = common::send(
        app,
        "POST",
        "/api/projects",
        json!({"name": "H8 fleet membership", "project_type": "software"}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{project}");
    let project_id = project["id"].as_str().unwrap().to_owned();

    let (status, item) = common::send(
        app,
        "POST",
        &format!("/api/projects/{project_id}/items"),
        json!({"title": "Prove fleet-targeted scheduling lands on a member"}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{item}");
    item["id"].as_str().unwrap().to_owned()
}

/// Operator creates a pending runner and its one-time enrollment token, then
/// a mock runner redeems it — returning the runner id and a ready-to-use
/// `Authorization` header pair carrying the raw runner credential.
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
    let raw_enrollment_token = pending["enrollment_token"].as_str().unwrap().to_owned();

    let (status, enrolled) = common::send(
        app,
        "POST",
        "/api/runner/v1/enroll",
        json!({
            "protocol_version": 1,
            "enrollment_token": raw_enrollment_token,
            "runner_name": name,
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(),
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    assert_eq!(enrolled["runner_id"], runner_id);
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

fn execution_request_body(
    item_id: &str,
    key: &str,
    fleet_id: &str,
    agent_profile_id: &str,
) -> Value {
    json!({
        "item_id": item_id,
        "idempotency_key": key,
        "selector_kind": "fleet",
        "selector_id": fleet_id,
        "agent_profile_id": agent_profile_id,
        "requested_harness_kind": "codex",
        "requested_model_provider": "openai",
        "requested_model_id": "opaque/model-h8",
        "agent_profile_snapshot": {"name": "profile", "instructions": "work safely", "tool_policy": {}, "timeout_seconds": 60, "budgets": {}},
        "repository_snapshot": {"kind": "git", "remote": "https://example.test/h8.git", "base_revision": BASE_REVISION, "subdirectory": null},
        "permission_policy": {"tools": ["shell"], "network": false},
        "timeout_seconds": 60,
        "budgets": {},
        "environment": {},
        "metadata": {},
    })
}

// =======================================================================
// 1. The acceptance claim itself: an operator populates a fleet over the
//    API, and a fleet-targeted request schedules onto a member — a runner
//    that was never added to the fleet gets no work, and the member does.
// =======================================================================

#[tokio::test]
async fn fleet_targeted_request_schedules_onto_member_not_outsider() {
    let (pool, workspace_id) = fresh_database().await;
    let app = router_for(pool.clone(), workspace_id).await;
    let op = operator_headers();

    let item_id = create_project_and_item(&app).await;
    let (status, profile) = common::send(
        &app,
        "POST",
        "/api/agent-profiles",
        json!({"name": "H8 profile", "instructions": "work safely"}),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    let agent_profile_id = profile["agent_profile_id"].as_str().unwrap().to_owned();

    // Two enrolled runners: `member` will be added to the fleet, `outsider`
    // never is.
    let (member_id, member_auth_owned) = enroll_runner(&app, "H8 fleet member").await;
    let member_auth = headers_ref(&member_auth_owned);
    let (outsider_id, outsider_auth_owned) = enroll_runner(&app, "H8 fleet outsider").await;
    let outsider_auth = headers_ref(&outsider_auth_owned);

    // --- Operator creates the fleet over the API. ---
    let (status, fleet) = common::send(
        &app,
        "POST",
        "/api/runner-fleets",
        json!({"name": "H8 fleet"}),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fleet}");
    let fleet_id = fleet["fleet_id"].as_str().unwrap().to_owned();

    // --- Populate the fleet: add `member`, leave `outsider` out. ---
    let (status, added) = common::send(
        &app,
        "POST",
        &format!("/api/runner-fleets/{fleet_id}/members"),
        json!({"runner_id": member_id}),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    assert_eq!(added["state"], "added");
    assert_eq!(added["fleet_id"], fleet_id);
    assert_eq!(added["runner_id"], member_id);

    // Directly verify the write landed in `agent_fleet_members` (not just
    // that the endpoint returned 200).
    let member_row_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM agent_fleet_members WHERE fleet_id = ? AND runner_id = ?)",
    )
    .bind(&fleet_id)
    .bind(&member_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(member_row_exists, "membership row was not persisted");

    // `GET /api/runners?fleet_id=` reflects the new membership.
    let (status, roster) = common::send(
        &app,
        "GET",
        &format!("/api/runners?fleet_id={fleet_id}"),
        Value::Null,
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{roster}");
    let roster_ids: Vec<&str> = roster["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["runner_id"].as_str().unwrap())
        .collect();
    assert_eq!(roster_ids, vec![member_id.as_str()]);

    // Adding the same runner again is an idempotent no-op, not a conflict.
    let (status, added_again) = common::send(
        &app,
        "POST",
        &format!("/api/runner-fleets/{fleet_id}/members"),
        json!({"runner_id": member_id}),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added_again}");
    assert_eq!(added_again["state"], "already_member");

    // --- Operator creates a fleet-targeted execution request. ---
    let (status, created) = common::send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(&item_id, "h8-fleet-claim", &fleet_id, &agent_profile_id),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    // --- The outsider (never added to the fleet) claims first and must get
    //     no work: the request is fleet-scoped and it is not a member. ---
    let (status, refreshed) = common::send(
        &app,
        "POST",
        "/api/runner/v1/refresh",
        json!({
            "protocol_version": 1,
            "runner_id": outsider_id,
            "runner_name": "H8 fleet outsider",
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(),
        }),
        &outsider_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{refreshed}");
    let (status, outsider_claim) = common::send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": outsider_id, "claim_request_id": "h8-outsider-claim", "available_capacity": 1, "wait_ms": 0}),
        &outsider_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{outsider_claim}");
    assert_eq!(
        outsider_claim["lease"],
        Value::Null,
        "a runner outside the fleet must not be handed a fleet-scoped request: {outsider_claim}"
    );

    // Prove the request is still queued, untouched by the outsider's claim
    // attempt — not merely that its response looked empty.
    let (state, attempt_count): (String, i64) = sqlx::query_as(
        "SELECT er.state, (SELECT COUNT(*) FROM execution_attempts WHERE request_id = er.id) \
         FROM execution_requests er WHERE er.id = ?",
    )
    .bind(&request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "queued");
    assert_eq!(attempt_count, 0);

    // --- The member claims and gets exactly this request. ---
    let (status, refreshed) = common::send(
        &app,
        "POST",
        "/api/runner/v1/refresh",
        json!({
            "protocol_version": 1,
            "runner_id": member_id,
            "runner_name": "H8 fleet member",
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(),
        }),
        &member_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{refreshed}");
    let (status, member_claim) = common::send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": member_id, "claim_request_id": "h8-member-claim", "available_capacity": 1, "wait_ms": 0}),
        &member_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{member_claim}");
    assert_eq!(
        member_claim["request"]["request_id"], request_id,
        "the fleet's member must be handed the fleet-scoped request: {member_claim}"
    );
    let attempt_id = member_claim["lease"]["attempt_id"].as_str().unwrap();
    let (db_state, db_runner_id): (String, String) =
        sqlx::query_as("SELECT state, runner_id FROM execution_attempts WHERE id = ?")
            .bind(attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(db_state, "leased");
    assert_eq!(db_runner_id, member_id);

    // --- Removing the member from the fleet is also a real write. ---
    let (status, removed) = common::send(
        &app,
        "DELETE",
        &format!("/api/runner-fleets/{fleet_id}/members/{member_id}"),
        Value::Null,
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(removed["state"], "removed");
    let member_row_exists_after_removal: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM agent_fleet_members WHERE fleet_id = ? AND runner_id = ?)",
    )
    .bind(&fleet_id)
    .bind(&member_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !member_row_exists_after_removal,
        "membership row must be gone after removal"
    );
}

// =======================================================================
// 2. Adversarial cases: adding a member to a fleet or runner that does not
//    exist is rejected as `not_found`, and nothing is written.
// =======================================================================

#[tokio::test]
async fn adding_member_to_nonexistent_fleet_or_runner_writes_nothing() {
    let (pool, workspace_id) = fresh_database().await;
    let app = router_for(pool.clone(), workspace_id).await;
    let op = operator_headers();

    let (real_runner_id, _auth) = enroll_runner(&app, "H8 real runner").await;

    let (status, fleet) = common::send(
        &app,
        "POST",
        "/api/runner-fleets",
        json!({"name": "H8 adversarial fleet"}),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fleet}");
    let fleet_id = fleet["fleet_id"].as_str().unwrap().to_owned();

    // Nonexistent fleet.
    let (status, body) = common::send(
        &app,
        "POST",
        "/api/runner-fleets/fleet_does_not_exist/members",
        json!({"runner_id": real_runner_id}),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "not_found");

    // Nonexistent runner.
    let (status, body) = common::send(
        &app,
        "POST",
        &format!("/api/runner-fleets/{fleet_id}/members"),
        json!({"runner_id": "runr_does_not_exist"}),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "not_found");

    // Neither adversarial call left a row behind.
    let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_fleet_members")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        row_count, 0,
        "a rejected add must not leave a membership row behind"
    );

    // Removing a membership that never existed is also `not_found`, not a
    // silent success.
    let (status, body) = common::send(
        &app,
        "DELETE",
        &format!("/api/runner-fleets/{fleet_id}/members/{real_runner_id}"),
        Value::Null,
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "not_found");
}
