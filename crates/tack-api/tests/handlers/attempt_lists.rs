//! Handler tests for `GET /api/executions/{id}/attempts/{n}/artifacts` and
//! `.../decisions` (`crate::handlers::attempt_lists`): an unauthenticated
//! caller is rejected, and the two routes' ordering matches the handler's
//! own query guarantee. Each test runs once per [`ResourceKind`] since the
//! two routes are structurally identical. Rows are seeded with a direct
//! `sqlx::query` insert, not the runner-protocol write routes — the write
//! side is exercised in `runner_protocol/artifact_events.rs` and
//! `runner_protocol/decisions.rs`; this file only covers what the two
//! *read* routes return, so a claimed (`leased`) attempt is enough.

use crate::common;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, orch_runtime::OrchRuntime, router::build_router};
use tack_db::{Repository, init_pool, migrations};
use uuid::Uuid;

const OPERATOR_TOKEN: &str = "c5-attempt-lists-operator-token";
const BASE_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

async fn setup() -> (axum::Router, Repository, String) {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'C5 attempt lists', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("workspace");
    let repo = Repository::new(pool);
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
                name: "C5 attempt lists".into(),
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
                title: "Prove the attempt-list routes".into(),
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
    vec![("authorization", "Bearer c5-attempt-lists-operator-token")]
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn full_capabilities() -> Value {
    let now = chrono::Utc::now().to_rfc3339();
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
                "model_ids": ["opaque/model-c5"],
                "discovery": "reported"
            }],
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

/// Enrolls a runner and returns (runner_id, bearer-auth-header-pair).
async fn enroll_runner(app: &axum::Router, name: &str) -> (String, [(String, String); 1]) {
    let (status, pending, _) = common::send_with_raw(
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

    let (status, enrolled, _) = common::send_with_raw(
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

/// `label` keeps the profile name unique per call — a test that stands up
/// two separate executions (the cross-execution not-found tests) calls this
/// twice, and profile names are unique.
async fn create_agent_profile(app: &axum::Router, label: &str) -> String {
    let (status, profile, _) = common::send_with_raw(
        app,
        "POST",
        "/api/agent-profiles",
        json!({"name": format!("C6 {label} profile"), "instructions": "work safely"}),
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
        "requested_model_id": "opaque/model-c5",
        "agent_profile_snapshot": {"name": "profile", "instructions": "work safely", "tool_policy": {}, "timeout_seconds": 60, "budgets": {}},
        "repository_snapshot": {"kind": "git", "remote": "https://example.test/c5.git", "base_revision": BASE_REVISION, "subdirectory": null},
        "permission_policy": {"tools": ["shell"], "network": false},
        "timeout_seconds": 60,
        "budgets": {},
        "environment": {},
        "metadata": {},
    })
}

/// Creates an execution request and claims it, returning the operator-facing
/// `request_id` and the internal `attempt_id` a direct fixture insert needs.
async fn request_and_claim(app: &axum::Router, item_id: &str, label: &str) -> (String, String) {
    let (runner_id, auth_owned) = enroll_runner(app, &format!("{label} runner")).await;
    let auth = headers_ref(&auth_owned);
    let agent_profile_id = create_agent_profile(app, label).await;

    let (status, created, _) = common::send_with_raw(
        app,
        "POST",
        "/api/executions",
        execution_request_body(item_id, label, &runner_id, &agent_profile_id),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    let (status, claimed, _) = common::send_with_raw(
        app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": format!("{label}-claim"), "available_capacity": 1, "wait_ms": 0}),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    (request_id, attempt_id)
}

/// Seeds one `execution_artifacts` manifest row directly — the write path's
/// own fencing/attempt-state gate is proven elsewhere
/// (`runner_protocol/artifact_events.rs`); this file only needs a real row
/// with a chosen `created_at` to test the read route.
async fn insert_artifact(repo: &Repository, attempt_id: &str, artifact_id: &str, created_at: &str) {
    sqlx::query(
        "INSERT INTO execution_artifacts \
         (id, attempt_id, artifact_id, kind, name, size_bytes, sha256, created_at) \
         VALUES (?, ?, ?, 'patch', 'changes.patch', 42, ?, ?)",
    )
    .bind(format!("art_row_{}", Uuid::new_v4()))
    .bind(attempt_id)
    .bind(artifact_id)
    .bind("a".repeat(64))
    .bind(created_at)
    .execute(repo.pool())
    .await
    .expect("insert artifact fixture");
}

/// Seeds one `execution_decisions` row directly — same rationale as
/// [`insert_artifact`], mirroring `runner_protocol/decisions.rs` for the
/// write side.
async fn insert_decision(repo: &Repository, attempt_id: &str, decision_id: &str, created_at: &str) {
    sqlx::query(
        "INSERT INTO execution_decisions \
         (id, attempt_id, decision_id, kind, prompt, created_at, updated_at) \
         VALUES (?, ?, ?, 'approval', 'Proceed?', ?, ?)",
    )
    .bind(format!("dec_row_{}", Uuid::new_v4()))
    .bind(attempt_id)
    .bind(decision_id)
    .bind(created_at)
    .bind(created_at)
    .execute(repo.pool())
    .await
    .expect("insert decision fixture");
}

/// The two list routes below (`.../artifacts` and `.../decisions`) are
/// structurally identical, so every test in this file runs once per kind
/// rather than existing as two near-duplicate functions.
#[derive(Clone, Copy)]
enum ResourceKind {
    Artifact,
    Decision,
}

impl ResourceKind {
    fn route_segment(self) -> &'static str {
        match self {
            ResourceKind::Artifact => "artifacts",
            ResourceKind::Decision => "decisions",
        }
    }

    fn noun(self) -> &'static str {
        match self {
            ResourceKind::Artifact => "artifact",
            ResourceKind::Decision => "decision",
        }
    }

    fn id_field(self) -> &'static str {
        match self {
            ResourceKind::Artifact => "artifact_id",
            ResourceKind::Decision => "decision_id",
        }
    }

    async fn insert(self, repo: &Repository, attempt_id: &str, id: &str, created_at: &str) {
        match self {
            ResourceKind::Artifact => insert_artifact(repo, attempt_id, id, created_at).await,
            ResourceKind::Decision => insert_decision(repo, attempt_id, id, created_at).await,
        }
    }
}

/// Creates an execution request but never claims it, so its
/// `execution_attempts` table stays empty — the "another execution exists,
/// but this one never claimed an attempt" half of the cross-execution
/// not-found tests below.
async fn request_without_claiming(app: &axum::Router, item_id: &str, label: &str) -> String {
    let (runner_id, _auth_owned) = enroll_runner(app, &format!("{label} runner")).await;
    let agent_profile_id = create_agent_profile(app, label).await;
    let (status, created, _) = common::send_with_raw(
        app,
        "POST",
        "/api/executions",
        execution_request_body(item_id, label, &runner_id, &agent_profile_id),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    created["request_id"].as_str().unwrap().to_owned()
}

// =======================================================================
// GET /api/executions/{request_id}/attempts/{attempt_number}/{artifacts,decisions}
// =======================================================================

#[tokio::test]
async fn attempt_list_requires_auth_and_leaks_nothing_without_it() {
    for kind in [ResourceKind::Artifact, ResourceKind::Decision] {
        let (app, repo, item_id) = setup().await;
        let label = format!("{}-auth", kind.route_segment());
        let (request_id, attempt_id) = request_and_claim(&app, &item_id, &label).await;
        let seeded_id = format!("auth-check-{}", kind.noun());
        kind.insert(&repo, &attempt_id, &seeded_id, "2026-01-01T00:00:00Z")
            .await;

        let (status, body, raw) = common::send_with_raw(
            &app,
            "GET",
            &format!(
                "/api/executions/{request_id}/attempts/1/{}",
                kind.route_segment()
            ),
            Value::Null,
            &[],
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{}: {body}",
            kind.route_segment()
        );
        // A status code alone would not catch a gate that rejects but still
        // lets the handler run: assert the real seeded row never reached the
        // response body at all.
        assert!(
            body.get("data").is_none(),
            "{}: unauthenticated response must carry no data field: {body}",
            kind.route_segment()
        );
        assert!(
            !raw.contains(&seeded_id),
            "{}: unauthenticated response must not leak the seeded id: {raw}",
            kind.route_segment()
        );
    }
}

#[tokio::test]
async fn attempt_list_is_empty_before_any_row() {
    for kind in [ResourceKind::Artifact, ResourceKind::Decision] {
        let (app, _repo, item_id) = setup().await;
        let label = format!("{}-empty", kind.route_segment());
        let (request_id, _attempt_id) = request_and_claim(&app, &item_id, &label).await;

        let (status, body, _) = common::send_with_raw(
            &app,
            "GET",
            &format!(
                "/api/executions/{request_id}/attempts/1/{}",
                kind.route_segment()
            ),
            Value::Null,
            &operator_headers(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{}: {body}", kind.route_segment());
        assert_eq!(body["data"], json!([]), "{}", kind.route_segment());
    }
}

#[tokio::test]
async fn attempt_list_is_returned_oldest_first() {
    for kind in [ResourceKind::Artifact, ResourceKind::Decision] {
        let (app, repo, item_id) = setup().await;
        let label = format!("{}-order", kind.route_segment());
        let (request_id, attempt_id) = request_and_claim(&app, &item_id, &label).await;

        // Inserted out of chronological order on purpose: "newer" lands in
        // the table first but is stamped with the later `created_at`. If the
        // route ever returned rows in insertion/rowid order instead of the
        // handler's actual `ORDER BY created_at`, this would come back
        // [newer, older] and the assertion below would catch it.
        let short = &kind.noun()[..3];
        let newer = format!("{short}-newer");
        let older = format!("{short}-older");
        kind.insert(&repo, &attempt_id, &newer, "2026-01-02T00:00:00Z")
            .await;
        kind.insert(&repo, &attempt_id, &older, "2026-01-01T00:00:00Z")
            .await;

        let (status, body, _) = common::send_with_raw(
            &app,
            "GET",
            &format!(
                "/api/executions/{request_id}/attempts/1/{}",
                kind.route_segment()
            ),
            Value::Null,
            &operator_headers(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{}: {body}", kind.route_segment());
        let data = body["data"].as_array().expect("data array");
        assert_eq!(data.len(), 2, "{}", kind.route_segment());
        assert_eq!(data[0][kind.id_field()], older, "{}", kind.route_segment());
        assert_eq!(data[1][kind.id_field()], newer, "{}", kind.route_segment());
    }
}

#[tokio::test]
async fn attempt_list_unknown_attempt_number_is_404() {
    for kind in [ResourceKind::Artifact, ResourceKind::Decision] {
        let (app, _repo, item_id) = setup().await;
        let label = format!("{}-unknown-n", kind.route_segment());
        let (request_id, _attempt_id) = request_and_claim(&app, &item_id, &label).await;

        // Only attempt 1 was ever claimed for this request; attempt 99 never
        // existed for anyone.
        let (status, body, _) = common::send_with_raw(
            &app,
            "GET",
            &format!(
                "/api/executions/{request_id}/attempts/99/{}",
                kind.route_segment()
            ),
            Value::Null,
            &operator_headers(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{}: {body}",
            kind.route_segment()
        );
        assert_eq!(
            body["error"]["details"]["resource"],
            "execution_attempt",
            "{}",
            kind.route_segment()
        );
    }
}

#[tokio::test]
async fn attempt_list_from_a_different_execution_is_404() {
    for kind in [ResourceKind::Artifact, ResourceKind::Decision] {
        let (app, repo, item_id) = setup().await;
        // Execution X claims attempt 1 and writes a real row against it.
        let owner_label = format!("{}-cross-owner", kind.route_segment());
        let (_other_request_id, other_attempt_id) =
            request_and_claim(&app, &item_id, &owner_label).await;
        let seeded_id = format!("cross-execution-{}", kind.noun());
        kind.insert(&repo, &other_attempt_id, &seeded_id, "2026-01-01T00:00:00Z")
            .await;
        // Execution Y is real but never claimed anything — it has no
        // attempt 1 of its own.
        let caller_label = format!("{}-cross-caller", kind.route_segment());
        let request_id = request_without_claiming(&app, &item_id, &caller_label).await;

        let (status, body, raw) = common::send_with_raw(
            &app,
            "GET",
            &format!(
                "/api/executions/{request_id}/attempts/1/{}",
                kind.route_segment()
            ),
            Value::Null,
            &operator_headers(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "{}: {body}",
            kind.route_segment()
        );
        assert_eq!(
            body["error"]["details"]["resource"],
            "execution_attempt",
            "{}",
            kind.route_segment()
        );
        // A status code alone would not catch a query that scopes only by
        // attempt_number and forgets request_id: assert X's real row never
        // reached this response.
        assert!(
            !raw.contains(&seeded_id),
            "{}: must not leak another execution's {}: {raw}",
            kind.route_segment(),
            kind.noun()
        );
    }
}
