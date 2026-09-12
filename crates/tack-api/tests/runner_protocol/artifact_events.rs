//! HTTP tests: artifact content upload/download and the redaction guarantee
//! over event/artifact payloads. Repository-level atomicity/retention
//! proofs live in `tack-db`'s `repository/event_artifact_retention.rs`;
//! the storage primitive's own unit tests live beside it in
//! `artifact_storage.rs`. This file proves the HTTP wiring end to end.

// `lifecycle`'s own copy of this same `#[path]` load is a second,
// independent module tree over the identical file — allowed deliberately,
// see that module's own comment on its copy. `artifact_download::routes(...)`
// below is proven as its own, separately-constructed local router (never
// merged with the runner-only `runner_protocol::routes(...)` router),
// isolating this file's claims from the production router's own auth and
// mounting — that route is also mounted in the real production router and
// proven end to end by the `wiring` binary's `artifact.rs`.
#[allow(clippy::duplicate_mod)]
#[path = "../../src/handlers/runner_protocol.rs"]
mod runner_protocol;

use std::sync::{Arc, Mutex};

use crate::log_capture::{CaptureGuard, ensure_global_log_capture_installed};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::{Value, json};
use tack_core::models::{CreateItem, CreateProject, ProjectType};
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{
        EnrollmentToken, ExecutionClock, NewAgentProfile, NewExecutionRequest, NewRunner,
    },
};
use tower::ServiceExt;
use uuid::Uuid;

const REQUESTED_MODEL_PROVIDER: &str = "openai";
const REQUESTED_MODEL_ID: &str = "opaque/model-f2";

#[derive(Clone)]
struct FakeClock(Arc<Mutex<DateTime<Utc>>>);

impl FakeClock {
    fn new(start: DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(start)))
    }
}

impl ExecutionClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

/// Artifact storage root for one artifact-events test.
///
/// The `TempDir` removes the directory and everything under it when it drops,
/// so a failing assertion leaves nothing behind either.
fn temp_storage_root(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

async fn setup() -> (Router, Repository, FakeClock, String, tempfile::TempDir) {
    // Must run before any handler code in this binary executes — see
    // `log_capture.rs`'s own doc comment for the `tracing` interest-cache
    // race this closes.
    ensure_global_log_capture_installed();
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES (?, 'F2', '{}')")
        .bind(workspace.to_string())
        .execute(repo.pool())
        .await
        .expect("workspace");
    let project = repo
        .create_project(
            workspace,
            CreateProject {
                name: "F2".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .expect("project");
    let item = repo
        .create_item(
            project.id,
            "To Do",
            CreateItem {
                title: "I".into(),
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

    let clock = FakeClock::new(Utc.with_ymd_and_hms(2026, 8, 12, 12, 0, 0).unwrap());
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-f2",
            name: "F2 Profile",
            instructions: "work safely",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: r#"{"tokens":1000}"#,
        },
        &clock,
    )
    .await
    .expect("profile");

    let storage_root = temp_storage_root("root");
    let state = runner_protocol::RunnerProtocolState::new(repo.clone(), Arc::new(clock.clone()))
        .with_artifact_storage_root(storage_root.path().to_path_buf());
    let app = runner_protocol::routes(state, usize::MAX);
    (app, repo, clock, item.id.to_string(), storage_root)
}

#[allow(clippy::too_many_arguments)]
async fn enqueue_request(
    repo: &Repository,
    clock: &FakeClock,
    item_id: &str,
    runner_id: &str,
    agent_profile_id: &str,
    key: &str,
) -> String {
    let request_id = format!("exec_{}", Uuid::new_v4());
    let created_at = clock.now();
    let snapshot = json!({
        "request_id": request_id,
        "item_id": item_id,
        "idempotency_key": key,
        "created_by": {"source": "test", "subject_id": "f2-test"},
        "created_at": created_at.to_rfc3339(),
        "selector": {"kind": "exact_runner", "runner_id": runner_id},
        "agent_profile_id": agent_profile_id,
        "resolved_agent_profile": {"name":"F2 Profile","instructions":"work safely","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{"tokens":1000}},
        "requested_harness_kind": "codex",
        "requested_model_provider": REQUESTED_MODEL_PROVIDER,
        "requested_model_id": REQUESTED_MODEL_ID,
        "repository": {"kind":"git","remote":"https://example.test/f2.git","base_revision":"abc123def456abc123def456abc123def456abc","subdirectory": Value::Null},
        "permission_policy": {"tools":["shell"],"network": false},
        "timeout_seconds": 60,
        "budgets": {"tokens": 1000},
        "status_map_policy_id": Value::Null,
        "environment": {},
        "metadata": {},
    });
    let snapshot_string = serde_json::to_string(&snapshot).unwrap();
    let root = snapshot.as_object().unwrap();
    let field_str = |name: &str| serde_json::to_string(&root[name]).unwrap();
    repo.enqueue_execution(
        NewExecutionRequest {
            id: &request_id,
            item_id,
            idempotency_scope: "test",
            idempotency_key: key,
            request_fingerprint: key,
            selector_kind: "exact_runner",
            selector_id: runner_id,
            agent_profile_id: Some(agent_profile_id),
            agent_profile_snapshot: &field_str("resolved_agent_profile"),
            requested_harness_kind: Some("codex"),
            requested_model_provider: Some(REQUESTED_MODEL_PROVIDER),
            requested_model_id: Some(REQUESTED_MODEL_ID),
            repository_snapshot: &field_str("repository"),
            permission_policy: &field_str("permission_policy"),
            timeout_seconds: Some(60),
            budgets: &field_str("budgets"),
            status_map_policy_id: None,
            environment: &field_str("environment"),
            metadata: &field_str("metadata"),
            request_snapshot: &snapshot_string,
        },
        clock,
    )
    .await
    .expect("enqueue");
    request_id
}

fn full_capabilities(reported_at: DateTime<Utc>, total: i64, available: i64) -> Value {
    json!({
        "reported_at": reported_at.to_rfc3339(),
        "labels": {"os": "linux"},
        "concurrency": {"total": total, "available": available},
        "harnesses": [{
            "harness_kind": "codex",
            "installed_version": "1.0.0",
            "probe_error": null,
            "probed_at": reported_at.to_rfc3339(),
            "model_combinations": [{
                "model_provider": REQUESTED_MODEL_PROVIDER,
                "model_ids": [REQUESTED_MODEL_ID],
                "discovery": "reported"
            }]
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

async fn send_json(
    app: &Router,
    method: &str,
    uri: &str,
    body: String,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 64 * 1_048_576)
        .await
        .unwrap();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

/// Raw-bytes PUT for the artifact content endpoint. `extra_headers` carries
/// the fencing-token header and, optionally, `content-type`.
async fn put_content(
    app: &Router,
    uri: &str,
    body: Vec<u8>,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method("PUT").uri(uri);
    for (name, value) in extra_headers {
        builder = builder.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 64 * 1_048_576)
        .await
        .unwrap();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

struct RunningAttempt {
    runner_id: String,
    credential: String,
    attempt_id: String,
    fencing_token: i64,
}

/// Enroll → claim → accept → start, entirely through the real HTTP handlers,
/// leaving the attempt in `running` — the state artifact/event writes
/// require.
async fn ready_running_attempt(
    app: &Router,
    repo: &Repository,
    clock: &FakeClock,
    item_id: &str,
    label: &str,
) -> RunningAttempt {
    let runner_id = format!("runner-{label}");
    let raw_token = format!("example_enrollment_token_{label}");
    let token_hash = runner_protocol::runner_auth::credential_hash(&raw_token);
    repo.create_pending_runner_and_issue_token(
        NewRunner {
            id: &runner_id,
            name: "F2 Runner",
            credential_hash: "pending:no-credential",
            labels: "{}",
            total_capacity: 2,
            available_capacity: 2,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        EnrollmentToken {
            id: &format!("tok-{label}"),
            runner_id: &runner_id,
            token_hash: &token_hash,
            expires_at: clock.now() + Duration::hours(1),
        },
        clock,
    )
    .await
    .expect("pending runner");

    let (status, enrolled) = send_json(
        app,
        "POST",
        "/enroll",
        json!({
            "protocol_version": 1,
            "enrollment_token": raw_token,
            "runner_name": "F2 Runner",
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(clock.now(), 2, 2),
        })
        .to_string(),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();
    let auth_header = format!("Bearer {credential}");

    repo.create_agent_profile(
        NewAgentProfile {
            id: &format!("profile-{label}"),
            name: "F2 Profile",
            instructions: "work safely",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: r#"{"tokens":1000}"#,
        },
        clock,
    )
    .await
    .ok();

    let request_id = enqueue_request(
        repo,
        clock,
        item_id,
        &runner_id,
        "profile-f2",
        &format!("key-{label}"),
    )
    .await;

    let (status, claimed) = send_json(
        app,
        "POST",
        "/claim",
        json!({
            "protocol_version": 1, "runner_id": runner_id, "claim_request_id": format!("claim-{label}"),
            "available_capacity": 1, "wait_ms": 1000,
        })
        .to_string(),
        &[("authorization", &auth_header)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed} for request {request_id}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    let (status, _) = send_json(
        app,
        "POST",
        &format!("/attempts/{attempt_id}/accept"),
        json!({
            "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "workspace_id": "ws-1", "base_revision": "abc123def456abc123def456abc123def456abc",
        })
        .to_string(),
        &[("authorization", &auth_header)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send_json(
        app,
        "POST",
        &format!("/attempts/{attempt_id}/start"),
        json!({
            "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "workspace_id": "ws-1", "base_revision": "abc123def456abc123def456abc123def456abc", "process_id": "pid-1",
        })
        .to_string(),
        &[("authorization", &auth_header)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    RunningAttempt {
        runner_id,
        credential,
        attempt_id,
        fencing_token,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

async fn manifest_artifact(
    app: &Router,
    attempt: &RunningAttempt,
    artifact_id: &str,
    content: &[u8],
    media_type: Option<&str>,
) -> Value {
    let (status, response) = send_json(
        app,
        "POST",
        &format!("/attempts/{}/artifacts", attempt.attempt_id),
        json!({
            "protocol_version": 1,
            "runner_id": attempt.runner_id,
            "attempt_id": attempt.attempt_id,
            "fencing_token": attempt.fencing_token,
            "artifacts": [{
                "artifact_id": artifact_id,
                "kind": "patch",
                "name": "changes.patch",
                "media_type": media_type,
                "size_bytes": content.len(),
                "sha256": sha256_hex(content),
                "content_disposition": "inline_upload",
                "metadata": {},
            }],
        })
        .to_string(),
        &[("authorization", &format!("Bearer {}", attempt.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    response
}

fn content_uri(attempt: &RunningAttempt, artifact_id: &str) -> String {
    format!(
        "/attempts/{}/artifacts/{artifact_id}/content",
        attempt.attempt_id
    )
}

fn fencing_header(attempt: &RunningAttempt) -> (String, String) {
    (
        "x-tack-fencing-token".to_string(),
        attempt.fencing_token.to_string(),
    )
}

fn auth_header(attempt: &RunningAttempt) -> (String, String) {
    (
        "authorization".to_string(),
        format!("Bearer {}", attempt.credential),
    )
}

async fn dir_is_empty_or_absent(path: &std::path::Path) -> bool {
    match tokio::fs::read_dir(path).await {
        Ok(mut entries) => entries.next_entry().await.unwrap().is_none(),
        Err(_) => true, // never created at all
    }
}

// ---------------------------------------------------------------------
// 1. Happy path: manifest, PUT content, commit, and a streamed operator
//    download of the exact same bytes.
// ---------------------------------------------------------------------

#[tokio::test]
async fn artifact_content_round_trips_through_upload_and_download() {
    let (app, repo, clock, item_id, storage_root_dir) = setup().await;
    let storage_root = storage_root_dir.path();
    let attempt = ready_running_attempt(&app, &repo, &clock, &item_id, "roundtrip").await;
    let content = b"diff --git a/x b/x\n+hello\n".to_vec();
    manifest_artifact(&app, &attempt, "art-1", &content, Some("text/x-diff")).await;

    let auth = auth_header(&attempt);
    let fence = fencing_header(&attempt);
    let (status, uploaded) = put_content(
        &app,
        &content_uri(&attempt, "art-1"),
        content.clone(),
        &[
            (auth.0.as_str(), auth.1.as_str()),
            (fence.0.as_str(), fence.1.as_str()),
            ("content-type", "text/x-diff"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{uploaded}");
    assert_eq!(uploaded["state"], "content_verified");
    assert_eq!(uploaded["size_bytes"], content.len());

    let stored_reference: Option<String> = sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts WHERE attempt_id=? AND artifact_id='art-1'",
    )
    .bind(&attempt.attempt_id)
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert!(stored_reference.is_some());

    // Download via the operator-facing router, constructed locally per
    // `artifact_download.rs`'s own doc comment (not merged into the
    // runner-only `app` router above).
    let attempt_number: i64 =
        sqlx::query_scalar("SELECT attempt_number FROM execution_attempts WHERE id=?")
            .bind(&attempt.attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    let request_id: String =
        sqlx::query_scalar("SELECT request_id FROM execution_attempts WHERE id=?")
            .bind(&attempt.attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();

    let download_state = runner_protocol::artifact_download::ArtifactDownloadState {
        repo: repo.clone(),
        artifact_storage: Arc::new(runner_protocol::artifact_storage::ArtifactStorage::new(
            storage_root,
        )),
    };
    let download_app = runner_protocol::artifact_download::routes(download_state);
    let response = download_app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/executions/{request_id}/attempts/{attempt_number}/artifacts/art-1/content"
                ))
                .header("x-tack-principal", "operator-test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(content_type, "text/x-diff");
    let disposition = response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(disposition.contains("changes.patch"));
    let downloaded = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    assert_eq!(downloaded.as_ref(), content.as_slice());

    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

// ---------------------------------------------------------------------
// 2. Acceptance: checksum mismatch stages nothing.
// ---------------------------------------------------------------------

/// Load-bearing (proven by hand, not left in the tree): temporarily made
/// `put_artifact_content` call `set_execution_artifact_content_reference`
/// unconditionally, skipping the checksum-mismatch early return — this test
/// failed (a `content_reference` was committed despite the mismatch).
/// Reverted, and it passes again. Two cases isolate the checksum check from
/// the size check: mismatched length, and same length with wrong bytes.
#[tokio::test]
async fn checksum_mismatch_stages_nothing() {
    // `same_length`: true isolates the checksum check from the size check
    // (a mismatched-length wrong body could conflict on either).
    for (label, artifact_id, declared, wrong, same_length) in [
        (
            "checksum",
            "art-mismatch",
            b"the real bytes".to_vec(),
            b"the WRONG bytes!!".to_vec(),
            false,
        ),
        (
            "checksum-samesize",
            "art-samesize",
            b"AAAAAAAAAA".to_vec(),
            b"BBBBBBBBBB".to_vec(),
            true,
        ),
    ] {
        let (app, repo, clock, item_id, storage_root_dir) = setup().await;
        let storage_root = storage_root_dir.path();
        let attempt = ready_running_attempt(&app, &repo, &clock, &item_id, label).await;
        manifest_artifact(&app, &attempt, artifact_id, &declared, None).await;

        let auth = auth_header(&attempt);
        let fence = fencing_header(&attempt);
        let (status, response) = put_content(
            &app,
            &content_uri(&attempt, artifact_id),
            wrong,
            &[
                (auth.0.as_str(), auth.1.as_str()),
                (fence.0.as_str(), fence.1.as_str()),
            ],
        )
        .await;
        assert!(
            status == StatusCode::CONFLICT || status == StatusCode::PAYLOAD_TOO_LARGE,
            "{status}: {response}"
        );
        if same_length {
            assert_eq!(status, StatusCode::CONFLICT, "{response}");
            assert_eq!(response["error"]["code"], "artifact_checksum_mismatch");
            assert_eq!(response["error"]["retryable"], true);
        }

        let stored_reference: Option<String> = sqlx::query_scalar(
            "SELECT content_reference FROM execution_artifacts WHERE attempt_id=? AND artifact_id=?",
        )
        .bind(&attempt.attempt_id)
        .bind(artifact_id)
        .fetch_one(repo.pool())
        .await
        .unwrap();
        assert_eq!(
            stored_reference, None,
            "no content_reference may be committed"
        );
        let attempt_dir = storage_root.join(hex_encode(&attempt.attempt_id));
        assert!(
            dir_is_empty_or_absent(&attempt_dir).await,
            "no blob may be left on disk after a checksum mismatch"
        );
        let _ = tokio::fs::remove_dir_all(&storage_root).await;
    }
}

// ---------------------------------------------------------------------
// 3. Acceptance: oversize / compression-bomb-style upload rejected.
// ---------------------------------------------------------------------

#[tokio::test]
async fn oversize_upload_is_rejected_and_stages_nothing() {
    let (app, repo, clock, item_id, storage_root_dir) = setup().await;
    let storage_root = storage_root_dir.path();
    let attempt = ready_running_attempt(&app, &repo, &clock, &item_id, "oversize").await;
    let declared_content = b"tiny".to_vec(); // manifest declares 4 bytes
    manifest_artifact(&app, &attempt, "art-oversize", &declared_content, None).await;

    let auth = auth_header(&attempt);
    let fence = fencing_header(&attempt);
    // Actually sends far more than declared — the "compression bomb" shape:
    // a small declared size, a much larger real body.
    let bomb = vec![0u8; 5 * 1024 * 1024]; // 5 MiB actually sent vs. 4 bytes declared
    let (status, response) = put_content(
        &app,
        &content_uri(&attempt, "art-oversize"),
        bomb,
        &[
            (auth.0.as_str(), auth.1.as_str()),
            (fence.0.as_str(), fence.1.as_str()),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{response}");
    assert_eq!(response["error"]["code"], "payload_too_large");

    let stored_reference: Option<String> = sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts WHERE attempt_id=? AND artifact_id='art-oversize'",
    )
    .bind(&attempt.attempt_id)
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(stored_reference, None);
    let attempt_dir = storage_root.join(hex_encode(&attempt.attempt_id));
    assert!(
        dir_is_empty_or_absent(&attempt_dir).await,
        "no partial bomb content may remain on disk"
    );
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

// ---------------------------------------------------------------------
// 4. Path traversal via crafted ids never escapes the storage root.
// ---------------------------------------------------------------------

#[tokio::test]
async fn crafted_traversal_artifact_id_stays_inside_storage_root() {
    let (app, repo, clock, item_id, storage_root_dir) = setup().await;
    let storage_root = storage_root_dir.path();
    let attempt = ready_running_attempt(&app, &repo, &clock, &item_id, "traversal").await;
    let content = b"safe content".to_vec();
    // The manifest step takes `artifact_id` from a JSON string field — no
    // URL decoding involved, so the literal traversal-shaped value is
    // exactly what gets stored.
    let malicious_artifact_id = "../../../etc/passwd";
    manifest_artifact(&app, &attempt, malicious_artifact_id, &content, None).await;

    let auth = auth_header(&attempt);
    let fence = fencing_header(&attempt);
    // The PUT URI, by contrast, must percent-encode the same literal value
    // into its `{artifact_id}` path segment (`axum::extract::Path` decodes
    // captured segments) so this test exercises the same request shape a
    // real client sending this artifact_id would produce — a raw literal
    // `/` in the URI would not even route to this handler at all, so
    // percent-encoding is not a test-only workaround, it is what a genuine
    // attacker attempting this traversal shape would have to send too.
    let uri = format!(
        "/attempts/{}/artifacts/..%2f..%2f..%2fetc%2fpasswd/content",
        attempt.attempt_id
    );
    let (status, response) = put_content(
        &app,
        &uri,
        content.clone(),
        &[
            (auth.0.as_str(), auth.1.as_str()),
            (fence.0.as_str(), fence.1.as_str()),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");

    // Confirm the write landed strictly inside the canonical storage root.
    let canonical_root = tokio::fs::canonicalize(&storage_root).await.unwrap();
    let content_reference: Option<String> = sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts WHERE attempt_id=? AND artifact_id=?",
    )
    .bind(&attempt.attempt_id)
    .bind(malicious_artifact_id)
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let content_reference =
        content_reference.expect("content_reference must be set after a successful upload");
    let full_path = storage_root.join(&content_reference);
    let canonical_full = tokio::fs::canonicalize(full_path.parent().unwrap())
        .await
        .unwrap();
    assert!(canonical_full.starts_with(&canonical_root));
    assert!(!content_reference.contains(".."));
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

// ---------------------------------------------------------------------
// 5. Immutability: a second upload for the same artifact_id is refused.
// ---------------------------------------------------------------------

#[tokio::test]
async fn content_is_immutable_once_verified() {
    let (app, repo, clock, item_id, storage_root_dir) = setup().await;
    let storage_root = storage_root_dir.path();
    let attempt = ready_running_attempt(&app, &repo, &clock, &item_id, "immutable").await;
    let content = b"first and only".to_vec();
    manifest_artifact(&app, &attempt, "art-immutable", &content, None).await;

    let auth = auth_header(&attempt);
    let fence = fencing_header(&attempt);
    let (status, _) = put_content(
        &app,
        &content_uri(&attempt, "art-immutable"),
        content.clone(),
        &[
            (auth.0.as_str(), auth.1.as_str()),
            (fence.0.as_str(), fence.1.as_str()),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // A byte-identical second upload is still refused — content is
    // recorded once, not "once per distinct value."
    let (status, response) = put_content(
        &app,
        &content_uri(&attempt, "art-immutable"),
        content,
        &[
            (auth.0.as_str(), auth.1.as_str()),
            (fence.0.as_str(), fence.1.as_str()),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

// ---------------------------------------------------------------------
// 6. Fencing: stale/missing fencing token, content-type mismatch.
// ---------------------------------------------------------------------

#[tokio::test]
async fn stale_fencing_token_is_rejected_before_any_write() {
    let (app, repo, clock, item_id, storage_root_dir) = setup().await;
    let storage_root = storage_root_dir.path();
    let attempt = ready_running_attempt(&app, &repo, &clock, &item_id, "stale-fence").await;
    let content = b"content".to_vec();
    manifest_artifact(&app, &attempt, "art-stale", &content, None).await;

    let auth = auth_header(&attempt);
    let wrong_fence = (attempt.fencing_token + 999).to_string();
    let (status, response) = put_content(
        &app,
        &content_uri(&attempt, "art-stale"),
        content,
        &[
            (auth.0.as_str(), auth.1.as_str()),
            ("x-tack-fencing-token", &wrong_fence),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{response}");
    assert_eq!(response["error"]["code"], "stale_lease");
    let attempt_dir = storage_root.join(hex_encode(&attempt.attempt_id));
    assert!(dir_is_empty_or_absent(&attempt_dir).await);
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

#[tokio::test]
async fn missing_fencing_header_is_invalid_request() {
    let (app, repo, clock, item_id, storage_root_dir) = setup().await;
    let storage_root = storage_root_dir.path();
    let attempt = ready_running_attempt(&app, &repo, &clock, &item_id, "missing-fence").await;
    let content = b"content".to_vec();
    manifest_artifact(&app, &attempt, "art-missing-fence", &content, None).await;

    let auth = auth_header(&attempt);
    let (status, response) = put_content(
        &app,
        &content_uri(&attempt, "art-missing-fence"),
        content,
        &[(auth.0.as_str(), auth.1.as_str())],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(response["error"]["code"], "invalid_request");
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

#[tokio::test]
async fn content_type_mismatch_with_declared_media_type_is_rejected() {
    let (app, repo, clock, item_id, storage_root_dir) = setup().await;
    let storage_root = storage_root_dir.path();
    let attempt = ready_running_attempt(&app, &repo, &clock, &item_id, "content-type").await;
    let content = b"diff content".to_vec();
    manifest_artifact(&app, &attempt, "art-ct", &content, Some("text/x-diff")).await;

    let auth = auth_header(&attempt);
    let fence = fencing_header(&attempt);
    let (status, response) = put_content(
        &app,
        &content_uri(&attempt, "art-ct"),
        content,
        &[
            (auth.0.as_str(), auth.1.as_str()),
            (fence.0.as_str(), fence.1.as_str()),
            ("content-type", "application/json"), // does not match manifest's text/x-diff
        ],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(response["error"]["code"], "invalid_request");
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

// ---------------------------------------------------------------------
// 7. Operator-download edge cases.
// ---------------------------------------------------------------------

#[tokio::test]
async fn unverified_manifest_download_is_named_conflict_not_404() {
    let (app, repo, clock, item_id, storage_root_dir) = setup().await;
    let storage_root = storage_root_dir.path();
    let attempt = ready_running_attempt(&app, &repo, &clock, &item_id, "download-unverified").await;
    manifest_artifact(&app, &attempt, "art-unverified", b"content", None).await;

    let attempt_number: i64 =
        sqlx::query_scalar("SELECT attempt_number FROM execution_attempts WHERE id=?")
            .bind(&attempt.attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    let request_id: String =
        sqlx::query_scalar("SELECT request_id FROM execution_attempts WHERE id=?")
            .bind(&attempt.attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    let download_state = runner_protocol::artifact_download::ArtifactDownloadState {
        repo: repo.clone(),
        artifact_storage: Arc::new(runner_protocol::artifact_storage::ArtifactStorage::new(
            storage_root,
        )),
    };
    let response = runner_protocol::artifact_download::routes(download_state)
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/executions/{request_id}/attempts/{attempt_number}/artifacts/art-unverified/content"
                ))
                .header("x-tack-principal", "operator-test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["code"], "conflict");
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

#[tokio::test]
async fn download_without_an_operator_principal_is_unauthorized() {
    let (_app, repo, _clock, _item_id, storage_root_dir) = setup().await;
    let storage_root = storage_root_dir.path();
    let download_state = runner_protocol::artifact_download::ArtifactDownloadState {
        repo: repo.clone(),
        artifact_storage: Arc::new(runner_protocol::artifact_storage::ArtifactStorage::new(
            storage_root,
        )),
    };
    let response = runner_protocol::artifact_download::routes(download_state)
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/executions/req-x/attempts/1/artifacts/art-x/content")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

#[tokio::test]
async fn download_of_an_unknown_artifact_is_not_found() {
    let (_app, repo, _clock, _item_id, storage_root_dir) = setup().await;
    let storage_root = storage_root_dir.path();
    let download_state = runner_protocol::artifact_download::ArtifactDownloadState {
        repo: repo.clone(),
        artifact_storage: Arc::new(runner_protocol::artifact_storage::ArtifactStorage::new(
            storage_root,
        )),
    };
    let response = runner_protocol::artifact_download::routes(download_state)
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/executions/does-not-exist/attempts/1/artifacts/art-x/content")
                .header("x-tack-principal", "operator-test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

// ---------------------------------------------------------------------
// 8. A real, moderately large upload proves the per-route body-limit
//    override actually took effect (the router-wide 4 MiB JSON ceiling
//    would otherwise reject this before the handler's own streaming logic
//    ever ran).
// ---------------------------------------------------------------------

#[tokio::test]
async fn upload_over_default_json_body_ceiling_still_succeeds() {
    let (app, repo, clock, item_id, storage_root_dir) = setup().await;
    let storage_root = storage_root_dir.path();
    let attempt = ready_running_attempt(&app, &repo, &clock, &item_id, "large").await;
    // 6 MiB: comfortably over the 4 MiB router-wide DefaultBodyLimit meant
    // for JSON control-plane bodies, comfortably under the 50 MiB protocol
    // ceiling.
    let large_content = vec![7u8; 6 * 1024 * 1024];
    manifest_artifact(&app, &attempt, "art-large", &large_content, None).await;

    let auth = auth_header(&attempt);
    let fence = fencing_header(&attempt);
    let (status, response) = put_content(
        &app,
        &content_uri(&attempt, "art-large"),
        large_content.clone(),
        &[
            (auth.0.as_str(), auth.1.as_str()),
            (fence.0.as_str(), fence.1.as_str()),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["size_bytes"], large_content.len());
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}

fn hex_encode(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

// ---------------------------------------------------------------------
// 9. Redaction: event payloads and artifact content never reach logs.
//
// Mirrors `lifecycle.rs`'s `logs_never_contain_raw_credentials_only_ids`
// rig exactly (a real, process-global `tracing_subscriber::fmt` subscriber,
// captured per-test via a thread-local so parallel tests in this binary
// cannot see each other's output) — see `log_capture.rs`'s doc comment for
// why a *global* default subscriber, not a thread-local `set_default`, is
// what actually closes the race.
// ---------------------------------------------------------------------

/// Distinctive markers unlikely to appear anywhere in genuine log
/// scaffolding (ids, status words, etc.), so their absence from captured
/// output is a meaningful, non-vacuous assertion.
const SECRET_EVENT_MARKER: &str = "SECRET_EVENT_PAYLOAD_MARKER_1f9c7";
const SECRET_ARTIFACT_MARKER: &[u8] = b"SECRET_ARTIFACT_BYTES_MARKER_9e21c";

#[tokio::test]
async fn logs_never_leak_event_payloads_or_artifact_content_only_ids() {
    let (app, repo, clock, item_id, storage_root_dir) = setup().await;
    let storage_root = storage_root_dir.path();
    let attempt = ready_running_attempt(&app, &repo, &clock, &item_id, "redaction").await;

    let (guard, captured) = CaptureGuard::start();

    // Event batch carrying a distinctive payload string.
    let (status, batch) = send_json(
        &app,
        "POST",
        &format!("/attempts/{}/events", attempt.attempt_id),
        json!({
            "protocol_version": 1, "runner_id": attempt.runner_id, "attempt_id": attempt.attempt_id,
            "fencing_token": attempt.fencing_token,
            "previous_checkpoint": Value::Null, "checkpoint": "checkpoint-redaction",
            "events": [{
                "event_id": "evt-redaction", "sequence": 1, "occurred_at": clock.now().to_rfc3339(),
                "source": "runner", "kind": "message", "payload": {"text": SECRET_EVENT_MARKER},
            }],
        })
        .to_string(),
        &[("authorization", &format!("Bearer {}", attempt.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batch}");

    // Artifact manifest + content upload carrying distinctive bytes.
    manifest_artifact(
        &app,
        &attempt,
        "art-redaction",
        SECRET_ARTIFACT_MARKER,
        None,
    )
    .await;
    let auth = auth_header(&attempt);
    let fence = fencing_header(&attempt);
    let (status, uploaded) = put_content(
        &app,
        &content_uri(&attempt, "art-redaction"),
        SECRET_ARTIFACT_MARKER.to_vec(),
        &[
            (auth.0.as_str(), auth.1.as_str()),
            (fence.0.as_str(), fence.1.as_str()),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{uploaded}");

    // Also force a rejection path (checksum mismatch) — the error path's
    // `details` (e.g. `{"artifact_id": ...}`) must stay id-only too. Same
    // length as the mismatch marker so this exercises a pure checksum
    // mismatch, not the oversize path.
    let mismatch_marker: &[u8] = b"SECRET_MISMATCH_BYTES_MARKER_44a1";
    let declared_but_never_sent = vec![0u8; mismatch_marker.len()];
    manifest_artifact(
        &app,
        &attempt,
        "art-redaction-2",
        &declared_but_never_sent,
        None,
    )
    .await;
    let (status, _) = put_content(
        &app,
        &content_uri(&attempt, "art-redaction-2"),
        mismatch_marker.to_vec(),
        &[
            (auth.0.as_str(), auth.1.as_str()),
            (fence.0.as_str(), fence.1.as_str()),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    drop(guard);
    let text = String::from_utf8_lossy(&captured.lock().unwrap()).into_owned();

    assert!(
        !text.is_empty(),
        "capture rig must have observed real log output"
    );
    assert!(
        !text.contains(SECRET_EVENT_MARKER),
        "event payload text leaked into logs:\n{text}"
    );
    assert!(
        !text.contains(std::str::from_utf8(SECRET_ARTIFACT_MARKER).unwrap()),
        "artifact content bytes leaked into logs:\n{text}"
    );
    assert!(
        !text.contains("SECRET_MISMATCH_BYTES_MARKER_44a1"),
        "mismatched artifact content leaked into logs on the error path:\n{text}"
    );
    // Non-vacuous: the runner id (an id, not a secret) is expected to appear
    // somewhere in real handler logs, confirming the capture rig is actually
    // observing genuine production log lines from this request, not just an
    // empty/unreached subscriber.
    assert!(
        text.contains(&attempt.runner_id),
        "expected the runner id to appear in captured logs as evidence the rig observed real output:\n{text}"
    );
    let _ = tokio::fs::remove_dir_all(&storage_root).await;
}
