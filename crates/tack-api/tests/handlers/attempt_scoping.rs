//! Cross-execution scoping tests for the two operator-facing routes that
//! resolve an attempt by `(request_id, attempt_number)` and were not covered
//! by `attempt_lists.rs`'s own cross-execution tests:
//! `GET /api/executions/{request_id}/attempts/{attempt_number}/events` and
//! `GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content`.
//!
//! Mirrors `attempt_lists.rs` exactly: same production router
//! (`tack_api::router::build_router`), same runner-enrollment/execution/claim
//! fixture path, same technique for seeding two executions inside one test
//! (a real, claimed "owner" execution and a real, never-claimed "caller"
//! execution) so a query that resolves an attempt by `attempt_number` alone
//! — forgetting which execution it belongs to — can be caught returning the
//! owner's real row to the caller, not just a wrong status code.
//!
//! The artifact-content case is the one worth the extra setup: unlike every
//! other route here, a successful response is not a JSON envelope but the
//! artifact's actual bytes streamed back verbatim. Proving the leak for that
//! route means manifesting and uploading real content through the real
//! runner-protocol write path and showing those exact bytes come back
//! through the *other* execution's request id.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tack_api::config::AppConfig;
use tack_api::{AppState, orch_runtime::OrchRuntime, router::build_router};
use tack_db::{Repository, init_pool, migrations};
use tower::ServiceExt;
use uuid::Uuid;

const OPERATOR_TOKEN: &str = "c8-attempt-scoping-operator-token";
const BASE_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

/// Artifact storage root for one test. Held alive by the caller for the
/// whole test — dropping it early would delete a directory a still-pending
/// download read could be mid-stream from.
fn temp_storage_root(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

async fn setup(storage_root: &std::path::Path) -> (axum::Router, Repository, String) {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'C8 attempt scoping', '{}')",
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
            storage_dir: storage_root.to_string_lossy().into_owned(),
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
                name: "C8 attempt scoping".into(),
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
                title: "Prove the attempt-scoping routes stay execution-scoped".into(),
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

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Value,
    headers: &[(&str, &str)],
) -> (StatusCode, Value, String) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    let raw = String::from_utf8_lossy(&bytes).into_owned();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value, raw)
}

fn operator_headers() -> Vec<(&'static str, &'static str)> {
    vec![("authorization", "Bearer c8-attempt-scoping-operator-token")]
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
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
                "model_ids": ["opaque/model-c8"],
                "discovery": "reported"
            }],
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

/// Enrolls a runner and returns (runner_id, bearer-auth-header-pair).
async fn enroll_runner(app: &axum::Router, name: &str) -> (String, [(String, String); 1]) {
    let (status, pending, _) = send(
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

    let (status, enrolled, _) = send(
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

/// `label` keeps the profile name unique per call, matching
/// `attempt_lists.rs`'s own helper — a test that stands up two separate
/// executions calls this twice.
async fn create_agent_profile(app: &axum::Router, label: &str) -> String {
    let (status, profile, _) = send(
        app,
        "POST",
        "/api/agent-profiles",
        json!({"name": format!("C8 {label} profile"), "instructions": "work safely"}),
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
        "requested_model_id": "opaque/model-c8",
        "agent_profile_snapshot": {"name": "profile", "instructions": "work safely", "tool_policy": {}, "timeout_seconds": 60, "budgets": {}},
        "repository_snapshot": {"kind": "git", "remote": "https://example.test/c8.git", "base_revision": BASE_REVISION, "subdirectory": null},
        "permission_policy": {"tools": ["shell"], "network": false},
        "timeout_seconds": 60,
        "budgets": {},
        "environment": {},
        "metadata": {},
    })
}

/// One real, claimed attempt with everything a caller needs to act as its
/// owning runner afterward (report events, manifest/upload an artifact).
struct ClaimedAttempt {
    request_id: String,
    attempt_id: String,
    runner_id: String,
    fencing_token: i64,
    auth: [(String, String); 1],
}

/// Creates an execution request and claims it — enough state to report
/// events directly; artifacts additionally need [`accept_and_start`] before
/// they can be manifested. Returns everything a caller needs to act as the
/// owning runner afterward.
async fn request_and_claim(app: &axum::Router, item_id: &str, label: &str) -> ClaimedAttempt {
    let (runner_id, auth_owned) = enroll_runner(app, &format!("{label} runner")).await;
    let auth = headers_ref(&auth_owned);
    let agent_profile_id = create_agent_profile(app, label).await;

    let (status, created, _) = send(
        app,
        "POST",
        "/api/executions",
        execution_request_body(item_id, label, &runner_id, &agent_profile_id),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    let (status, claimed, _) = send(
        app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": format!("{label}-claim"), "available_capacity": 1, "wait_ms": 0}),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    ClaimedAttempt {
        request_id,
        attempt_id,
        runner_id,
        fencing_token,
        auth: auth_owned,
    }
}

/// Creates an execution request but never claims it, so its
/// `execution_attempts` table stays empty for it — the "another execution
/// exists, but this one never claimed an attempt" half of every
/// cross-execution test below.
async fn request_without_claiming(app: &axum::Router, item_id: &str, label: &str) -> String {
    let (runner_id, _auth_owned) = enroll_runner(app, &format!("{label} runner")).await;
    let agent_profile_id = create_agent_profile(app, label).await;
    let (status, created, _) = send(
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

/// Transitions a claimed attempt through accept and start — required before
/// the artifact manifest route accepts anything (it rejects a merely
/// `leased` attempt with a named `conflict`, stricter than the repository's
/// own eligibility check).
async fn accept_and_start(app: &axum::Router, attempt: &ClaimedAttempt) {
    let auth = headers_ref(&attempt.auth);
    let (status, accepted, _) = send(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/accept", attempt.attempt_id),
        json!({
            "protocol_version": 1,
            "runner_id": attempt.runner_id,
            "attempt_id": attempt.attempt_id,
            "fencing_token": attempt.fencing_token,
            "workspace_id": "ws-1",
            "base_revision": BASE_REVISION,
        }),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");

    let (status, started, _) = send(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/start", attempt.attempt_id),
        json!({
            "protocol_version": 1,
            "runner_id": attempt.runner_id,
            "attempt_id": attempt.attempt_id,
            "fencing_token": attempt.fencing_token,
            "workspace_id": "ws-1",
            "base_revision": BASE_REVISION,
            "process_id": "pid-1",
        }),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
}

/// Manifests one artifact against a real, claimed attempt through the real
/// runner-protocol write route.
async fn manifest_artifact(
    app: &axum::Router,
    attempt: &ClaimedAttempt,
    artifact_id: &str,
    content: &[u8],
) {
    let auth = headers_ref(&attempt.auth);
    let (status, manifest, _) = send(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/artifacts", attempt.attempt_id),
        json!({
            "protocol_version": 1,
            "runner_id": attempt.runner_id,
            "attempt_id": attempt.attempt_id,
            "fencing_token": attempt.fencing_token,
            "artifacts": [{
                "artifact_id": artifact_id,
                "kind": "patch",
                "name": "changes.patch",
                "media_type": "text/plain",
                "size_bytes": content.len(),
                "sha256": sha256_hex(content),
                "content_disposition": "inline_upload",
                "metadata": {},
            }],
        }),
        &auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{manifest}");
}

/// Uploads content for an already-manifested artifact through the real
/// runner-protocol write route, verifying the server committed it.
async fn put_artifact_content(
    app: &axum::Router,
    attempt: &ClaimedAttempt,
    artifact_id: &str,
    content: Vec<u8>,
) {
    let auth = attempt.auth[0].1.clone();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!(
                    "/api/runner/v1/attempts/{}/artifacts/{artifact_id}/content",
                    attempt.attempt_id
                ))
                .header("authorization", auth)
                .header("x-tack-fencing-token", attempt.fencing_token.to_string())
                .header("content-type", "text/plain")
                .body(Body::from(content))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    let raw = String::from_utf8_lossy(&bytes).into_owned();
    assert_eq!(status, StatusCode::OK, "{raw}");
}

// =======================================================================
// GET /api/executions/{request_id}/attempts/{attempt_number}/events
// =======================================================================

#[tokio::test]
async fn attempt_events_from_a_different_execution_is_404() {
    let storage_root = temp_storage_root("c8-events-cross");
    let (app, _repo, item_id) = setup(storage_root.path()).await;

    // Execution X claims attempt 1 and reports a real event batch against
    // it, carrying a payload distinctive enough to prove it never reached an
    // unrelated response.
    let owner = request_and_claim(&app, &item_id, "events-cross-owner").await;
    let owner_auth = headers_ref(&owner.auth);
    let (status, batch, _) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/events", owner.attempt_id),
        json!({
            "protocol_version": 1,
            "runner_id": owner.runner_id,
            "attempt_id": owner.attempt_id,
            "fencing_token": owner.fencing_token,
            "checkpoint": "cp-1",
            "previous_checkpoint": Value::Null,
            "events": [{
                "event_id": "cross-execution-event",
                "sequence": 1,
                "source": "runner",
                "kind": "log",
                "payload": {"message": "cross-execution-event-payload"},
                "occurred_at": chrono::Utc::now().to_rfc3339(),
            }],
        }),
        &owner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batch}");

    // Execution Y is real but never claimed anything — it has no attempt 1
    // of its own.
    let request_id = request_without_claiming(&app, &item_id, "events-cross-caller").await;

    let (status, body, raw) = send(
        &app,
        "GET",
        &format!("/api/executions/{request_id}/attempts/1/events"),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["details"]["resource"], "execution_attempt");
    // A status code alone would not catch a query that scopes only by
    // attempt_number and forgets request_id: assert X's real event never
    // reached this response.
    assert!(
        !raw.contains("cross-execution-event-payload") && !raw.contains("cross-execution-event"),
        "must not leak another execution's event: {raw}"
    );
}

#[tokio::test]
async fn attempt_events_unknown_attempt_number_is_404() {
    let storage_root = temp_storage_root("c8-events-unknown");
    let (app, _repo, item_id) = setup(storage_root.path()).await;
    let owner = request_and_claim(&app, &item_id, "events-unknown-n").await;

    // Only attempt 1 was ever claimed for this request; attempt 99 never
    // existed for anyone.
    let (status, body, _) = send(
        &app,
        "GET",
        &format!("/api/executions/{}/attempts/99/events", owner.request_id),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["details"]["resource"], "execution_attempt");
}

// =======================================================================
// GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content
// =======================================================================

#[tokio::test]
async fn attempt_artifact_download_from_a_different_execution_is_404() {
    let storage_root = temp_storage_root("c8-download-cross");
    let (app, _repo, item_id) = setup(storage_root.path()).await;

    // Execution X claims attempt 1 and manifests + uploads a real artifact
    // with distinctive content against it.
    let owner = request_and_claim(&app, &item_id, "download-cross-owner").await;
    accept_and_start(&app, &owner).await;
    let content = b"cross-execution-artifact-bytes".to_vec();
    manifest_artifact(&app, &owner, "shared-artifact", &content).await;
    put_artifact_content(&app, &owner, "shared-artifact", content.clone()).await;

    // Execution Y is real but never claimed anything — it has no attempt 1
    // of its own, and therefore no artifact "shared-artifact" either.
    let request_id = request_without_claiming(&app, &item_id, "download-cross-caller").await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/executions/{request_id}/attempts/1/artifacts/shared-artifact/content"
                ))
                .header("authorization", "Bearer c8-attempt-scoping-operator-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    let raw = String::from_utf8_lossy(&bytes).into_owned();
    assert_eq!(status, StatusCode::NOT_FOUND, "{raw}");
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    assert_eq!(body["error"]["details"]["artifact_id"], "shared-artifact");
    // A status code alone would not catch a query that scopes only by
    // attempt_number and forgets request_id: assert X's real artifact bytes
    // never reached this response. A download route leaking here is not
    // metadata exposure — it is handing over another execution's file.
    assert!(
        !raw.contains("cross-execution-artifact-bytes"),
        "must not hand over another execution's artifact content: {raw}"
    );
}

#[tokio::test]
async fn attempt_artifact_download_unknown_attempt_number_is_404() {
    let storage_root = temp_storage_root("c8-download-unknown");
    let (app, _repo, item_id) = setup(storage_root.path()).await;
    let owner = request_and_claim(&app, &item_id, "download-unknown-n").await;
    accept_and_start(&app, &owner).await;
    let content = b"irrelevant-content".to_vec();
    manifest_artifact(&app, &owner, "art-1", &content).await;
    put_artifact_content(&app, &owner, "art-1", content).await;

    // Only attempt 1 was ever claimed for this request; attempt 99 never
    // existed for anyone.
    let (status, body, _) = send(
        &app,
        "GET",
        &format!(
            "/api/executions/{}/attempts/99/artifacts/art-1/content",
            owner.request_id
        ),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["details"]["artifact_id"], "art-1");
}
