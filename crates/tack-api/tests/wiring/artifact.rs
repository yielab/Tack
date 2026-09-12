//! Proof that two artifact-download wiring points are actually load-bearing
//! in *production*, not merely present as source text in `router.rs`.
//!
//! Unlike `artifact_events.rs` (a module of the `runner_protocol` binary,
//! which loads `runner_protocol.rs` via `#[path]` and constructs
//! `artifact_download::routes(...)` as its own, separately-mounted local
//! router), this file builds its own app/router setup — never
//! `common::test_app` or another file's harness — and drives only the public
//! `tack_api::router::build_router`/`tack_api::AppState` — the exact
//! function `tack serve` calls, over pure HTTP end to end (mirroring
//! `wave2_gate.rs`'s own established convention for proving claims against
//! the real, unmodified production router — including its "no direct
//! repository writes, no injected fake clock" discipline, since the
//! production router hard-codes `SystemExecutionClock` and a hand-picked
//! fixture timestamp would drift out of its expiry/liveness windows against
//! real wall-clock comparisons).
//!
//! Two claims are proved by
//! `artifact_content_is_stored_under_configured_storage_dir_and_downloadable_through_the_real_router`:
//!
//! 1. **The operator artifact-download route is mounted on the real
//!    production router** at
//!    `GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content`,
//!    reachable through the real operator auth layer, and serves the exact
//!    bytes a runner uploaded through the real `/api/runner/v1` surface.
//! 2. **Artifact storage actually follows `AppConfig::storage_dir`**
//!    (`TACK_STORAGE_DIR` in production), not the hardcoded
//!    `./storage/execution-artifacts` fallback `RunnerProtocolState::new`
//!    alone would produce. A distinctive, per-test temp directory is
//!    configured; the uploaded artifact's exact bytes are asserted to be
//!    present *somewhere under that configured directory* and confirmed
//!    absent from the hardcoded default — the strongest form of this claim,
//!    not just "the download round-trips" (which a wrong-but-*consistent*
//!    storage root could also satisfy by accident).
//!
//! A second test,
//! `unauthenticated_operator_download_request_is_still_gated_by_a_real_lookup`,
//! proves the mounted route runs the real handler (a genuine repository
//! lookup returning a named 404) rather than some unrelated route silently
//! matching.

use crate::common;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, orch_runtime::OrchRuntime, router::build_router};
use tack_db::{Repository, init_pool, migrations};
use tower::ServiceExt;
use uuid::Uuid;

const BASE_REVISION: &str = "abc123def456abc123def456abc123def456abc";

/// A guard directory to hang a storage path off — never `./storage`, so any
/// bytes found under `./storage/execution-artifacts` after this test runs
/// would prove the wiring did *not* take effect. The storage path itself is
/// `path().join("storage")`, which does not exist until the router creates
/// it: the tests assert its absence first and its presence afterwards.
fn distinctive_temp_root(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

/// Builds the real production router (`tack_api::router::build_router`)
/// around a fresh in-memory database, with `storage_dir` pointed at the
/// given directory — exactly what an operator setting `TACK_STORAGE_DIR`
/// would produce. No `api_token` is configured (pure-local mode), so every
/// `/api/*` request below needs no `Authorization` header — this test is
/// about the artifact-storage wiring, not the separate operator-auth gate
/// `wave2_gate.rs` already covers.
async fn real_app(storage_dir: &std::path::Path) -> (axum::Router, sqlx::SqlitePool) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'F6aWiring', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert workspace");

    let repo = Repository::new(pool.clone());
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        repo,
        config: AppConfig {
            storage_dir: storage_dir.to_string_lossy().into_owned(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        orch_runtime: OrchRuntime::new(),
        local_runner: None,
    };
    (build_router(state), pool)
}

async fn put_content(
    app: &axum::Router,
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

/// Real current wall-clock time, not a frozen fixture date — the production
/// router hard-codes `SystemExecutionClock`, so a hand-picked past/future
/// timestamp would drift out of the enrollment token's expiry window and
/// the scheduler's liveness fallback (see `wave2_gate.rs::full_capabilities`'s
/// own doc comment for the identical rationale).
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
                "model_ids": ["opaque/model-f6a"],
                "discovery": "reported"
            }],
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

/// Operator creates a real project and item over HTTP — mirrors
/// `wave2_gate.rs::create_project_and_item`.
async fn create_project_and_item(app: &axum::Router) -> String {
    let (status, project) = common::send_large(
        app,
        "POST",
        "/api/projects",
        json!({"name": "F6a wiring", "project_type": "software"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{project}");
    let project_id = project["id"].as_str().unwrap().to_owned();

    let (status, item) = common::send_large(
        app,
        "POST",
        &format!("/api/projects/{project_id}/items"),
        json!({"title": "Prove the artifact storage/download wiring end to end"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{item}");
    item["id"].as_str().unwrap().to_owned()
}

struct RunningAttempt {
    runner_id: String,
    credential: String,
    attempt_id: String,
    fencing_token: i64,
}

/// Operator creates an agent profile + pending runner/enrollment token, a
/// mock runner redeems it, an operator creates an execution request bound
/// to `item_id`, and the runner claims/accepts/starts it — entirely through
/// the real production router's HTTP surface (mirroring `wave2_gate.rs`'s
/// own `enroll_runner` + inline claim/accept/start sequence), leaving the
/// attempt `running`.
async fn ready_running_attempt(app: &axum::Router, item_id: &str, label: &str) -> RunningAttempt {
    let (status, profile) = common::send_large(
        app,
        "POST",
        "/api/agent-profiles",
        json!({"name": format!("{label} profile"), "instructions": "work safely"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    let agent_profile_id = profile["agent_profile_id"].as_str().unwrap().to_owned();

    let (status, pending) = common::send_large(
        app,
        "POST",
        "/api/runners/enrollment",
        json!({"name": format!("F6a runner {label}"), "total_capacity": 1, "available_capacity": 1}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    let runner_id = pending["runner_id"].as_str().unwrap().to_owned();
    let raw_enrollment_token = pending["enrollment_token"].as_str().unwrap().to_owned();

    let (status, enrolled) = common::send_large(
        app,
        "POST",
        "/api/runner/v1/enroll",
        json!({
            "protocol_version": 1,
            "enrollment_token": raw_enrollment_token,
            "runner_name": format!("F6a runner {label}"),
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(),
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();
    let auth = format!("Bearer {credential}");

    let (status, created) = common::send_large(
        app,
        "POST",
        "/api/executions",
        json!({
            "item_id": item_id,
            "idempotency_key": format!("key-{label}"),
            "selector_kind": "exact_runner",
            "selector_id": runner_id,
            "agent_profile_id": agent_profile_id,
            "requested_harness_kind": "codex",
            "requested_model_provider": "openai",
            "requested_model_id": "opaque/model-f6a",
            "agent_profile_snapshot": {"name": "profile", "instructions": "work safely", "tool_policy": {}, "timeout_seconds": 60, "budgets": {}},
            "repository_snapshot": {"kind": "git", "remote": "https://example.test/f6a.git", "base_revision": BASE_REVISION, "subdirectory": null},
            "permission_policy": {"tools": ["shell"], "network": false},
            "timeout_seconds": 60,
            "budgets": {},
            "environment": {},
            "metadata": {},
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");

    let (status, claimed) = common::send_large(
        app,
        "POST",
        "/api/runner/v1/claim",
        json!({
            "protocol_version": 1, "runner_id": runner_id, "claim_request_id": format!("claim-{label}"),
            "available_capacity": 1, "wait_ms": 0,
        }),
        &[("authorization", &auth)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    let (status, accepted) = common::send_large(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/accept"),
        json!({
            "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "workspace_id": "ws-1", "base_revision": BASE_REVISION,
        }),
        &[("authorization", &auth)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");

    let (status, started) = common::send_large(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/start"),
        json!({
            "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "workspace_id": "ws-1", "base_revision": BASE_REVISION, "process_id": "pid-1",
        }),
        &[("authorization", &auth)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");

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

async fn walk_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(mut entries) = tokio::fs::read_dir(&current).await else {
            continue;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if let Ok(meta) = entry.metadata().await
                && meta.is_dir()
            {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------
// 1 & 2 combined: the operator download route is mounted on the real
// `build_router` output, and the bytes it serves came from a file that
// physically lives under the operator-configured `storage_dir`, never the
// hardcoded `./storage` default.
// ---------------------------------------------------------------------
#[tokio::test]
async fn artifact_content_is_stored_under_configured_storage_dir_and_downloadable_through_the_real_router()
 {
    let storage_root = distinctive_temp_root("roundtrip");
    let storage_dir = storage_root.path().join("storage");
    // The directory must not exist yet — proves nothing pre-seeded it.
    assert!(!storage_dir.exists());

    let (app, pool) = real_app(&storage_dir).await;
    let item_id = create_project_and_item(&app).await;
    let attempt = ready_running_attempt(&app, &item_id, "roundtrip").await;

    let content = b"diff --git a/x b/x\n+hello from f6a\n".to_vec();
    let auth = format!("Bearer {}", attempt.credential);
    let (status, manifest) = common::send_large(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/artifacts", attempt.attempt_id),
        json!({
            "protocol_version": 1,
            "runner_id": attempt.runner_id,
            "attempt_id": attempt.attempt_id,
            "fencing_token": attempt.fencing_token,
            "artifacts": [{
                "artifact_id": "art-1",
                "kind": "patch",
                "name": "changes.patch",
                "media_type": "text/x-diff",
                "size_bytes": content.len(),
                "sha256": sha256_hex(&content),
                "content_disposition": "inline_upload",
                "metadata": {},
            }],
        }),
        &[("authorization", auth.as_str())],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{manifest}");
    let (status, uploaded) = put_content(
        &app,
        &format!(
            "/api/runner/v1/attempts/{}/artifacts/art-1/content",
            attempt.attempt_id
        ),
        content.clone(),
        &[
            ("authorization", auth.as_str()),
            (
                "x-tack-fencing-token",
                attempt.fencing_token.to_string().as_str(),
            ),
            ("content-type", "text/x-diff"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{uploaded}");
    assert_eq!(uploaded["state"], "content_verified");

    // ---- Claim 2: the storage root actually followed configuration ----
    // The configured directory must now contain the uploaded content
    // somewhere under it (recursively — `ArtifactStorage` nests by hex-
    // encoded attempt id).
    assert!(
        storage_dir.exists(),
        "expected {storage_dir:?} (the configured TACK_STORAGE_DIR-equivalent) to now exist"
    );
    let written = walk_files(&storage_dir).await;
    assert!(
        !written.is_empty(),
        "expected at least one file under the configured storage_dir, found none"
    );
    let mut found_matching_bytes = false;
    for path in &written {
        if let Ok(bytes) = tokio::fs::read(path).await
            && bytes == content
        {
            found_matching_bytes = true;
        }
    }
    assert!(
        found_matching_bytes,
        "expected the uploaded artifact's exact bytes to be present under the configured \
         storage_dir ({storage_dir:?}); found files: {written:?}"
    );
    // Every written path must be a descendant of the configured directory —
    // trivially true given how `written` was collected via `walk_files`,
    // but restated here as an explicit assertion of the absence claim: no
    // file escaped to `./storage` or anywhere else.
    for path in &written {
        assert!(
            path.starts_with(&storage_dir),
            "artifact file {path:?} is not under the configured storage_dir {storage_dir:?}"
        );
    }
    let default_storage_dir = std::path::Path::new("./storage/execution-artifacts");
    if default_storage_dir.exists() {
        let stray = walk_files(default_storage_dir).await;
        for path in &stray {
            let bytes = tokio::fs::read(path).await.ok();
            assert_ne!(
                bytes.as_deref(),
                Some(content.as_slice()),
                "artifact bytes leaked into the hardcoded default storage dir instead of the \
                 configured one"
            );
        }
    }

    // ---- Claim 1: the operator download route is mounted on the real
    // production router and serves those exact bytes end to end. ----
    let attempt_number: i64 =
        sqlx::query_scalar("SELECT attempt_number FROM execution_attempts WHERE id=?")
            .bind(&attempt.attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let request_id: String =
        sqlx::query_scalar("SELECT request_id FROM execution_attempts WHERE id=?")
            .bind(&attempt.attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    // No `Authorization` header at all — this is the *operator* surface
    // (`/api/...`), gated by `require_token`/`inject_operator_principal`,
    // never the runner bearer credential. With no `TACK_API_TOKEN`
    // configured (pure-local mode, matching this test's `AppConfig`), the
    // request must succeed without one.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/executions/{request_id}/attempts/{attempt_number}/artifacts/art-1/content"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the operator artifact-download route must be reachable on the real production router \
         built by tack_api::router::build_router — a 404 here means it is not actually mounted"
    );
    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(content_type, "text/x-diff");
    let downloaded = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    assert_eq!(downloaded.as_ref(), content.as_slice());

    let _ = tokio::fs::remove_dir_all(&storage_dir).await;
}

// ---------------------------------------------------------------------
// 2. The mounted route runs the real handler (a genuine repository lookup
//    returning a named 404), not some unrelated route silently matching.
// ---------------------------------------------------------------------
#[tokio::test]
async fn unauthenticated_operator_download_request_is_still_gated_by_a_real_lookup() {
    let storage_root = distinctive_temp_root("unknown");
    let storage_dir = storage_root.path().join("storage");
    let (app, _pool) = real_app(&storage_dir).await;

    // No such execution/attempt/artifact exists at all. This proves the
    // mounted route runs the real handler (which does a real repository
    // lookup and returns a named 404) rather than, say, silently matching
    // some unrelated wildcard route and returning 200 with an empty body.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/executions/exec_does_not_exist/attempts/1/artifacts/art-x/content")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"]["details"]["artifact_id"], "art-x");

    let _ = tokio::fs::remove_dir_all(&storage_dir).await;
}
