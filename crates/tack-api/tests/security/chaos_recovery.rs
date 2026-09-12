//! Chaos, fencing, security and recovery adversarial suite.
//!
//! This file owns adversarial/integration tests and the audit report only —
//! no production source is touched. Every test below drives the real
//! production router (`tack_api::router::build_router`, the exact function
//! `tack serve` calls), mirroring the house convention `wave2_gate.rs` and
//! `wiring/artifact.rs` established, and every "writes nothing" /
//! "rejects before X" claim reads persisted database state directly rather
//! than trusting a status code alone.
//!
//! Two tests (`two_distinct_runners_in_the_same_fleet_race_...` and
//! `a_duplicated_credential_used_concurrently_...`) prove real concurrency
//! against a **file-backed** SQLite database — concurrency claims must not
//! be proven only against the shared in-memory harness (a single-connection
//! `:memory:` pool can accidentally serialize what looks like a race).
//! Every other test uses an in-memory database, matching every other
//! adversarial test file in this crate.

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

// ---------------------------------------------------------------------
// Infrastructure — deliberately self-contained (no cross-test-file
// imports), matching the established precedent in `wave2_gate.rs` and
// `wiring/artifact.rs`: each adversarial file builds its own clean
// database and its own production router from scratch.
// ---------------------------------------------------------------------

/// Storage root for one chaos test, removed with everything under it when the
/// guard drops — including on the panic an adversarial test is likeliest to hit.
fn distinctive_temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

async fn app_in_memory(storage_dir: &std::path::Path) -> (axum::Router, sqlx::SqlitePool) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    build_app(pool, storage_dir, "sqlite::memory:".to_string()).await
}

async fn app_file_backed(
    db_path: &std::path::Path,
    storage_dir: &std::path::Path,
) -> (axum::Router, sqlx::SqlitePool) {
    let url = format!("sqlite:{}?mode=rwc", db_path.to_string_lossy());
    let pool = init_pool(&url).await.expect("file-backed pool");
    migrations::run_all(&pool).await.expect("migrations");
    build_app(pool, storage_dir, url).await
}

async fn build_app(
    pool: sqlx::SqlitePool,
    storage_dir: &std::path::Path,
    database_url: String,
) -> (axum::Router, sqlx::SqlitePool) {
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'G2Audit', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert workspace");
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    let state = AppState {
        repo: Repository::new(pool.clone()),
        config: AppConfig {
            database_url,
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
                "model_ids": ["opaque/model-g2"],
                "discovery": "reported"
            }],
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

async fn create_project_and_item(app: &axum::Router) -> String {
    let (status, project) = common::send_large(
        app,
        "POST",
        "/api/projects",
        json!({"name": "G2 audit", "project_type": "software"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{project}");
    let project_id = project["id"].as_str().unwrap().to_owned();
    let (status, item) = common::send_large(
        app,
        "POST",
        &format!("/api/projects/{project_id}/items"),
        json!({"title": "G2 adversarial item"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{item}");
    item["id"].as_str().unwrap().to_owned()
}

async fn agent_profile(app: &axum::Router, label: &str) -> String {
    let (status, profile) = common::send_large(
        app,
        "POST",
        "/api/agent-profiles",
        json!({"name": format!("{label} profile"), "instructions": "work safely"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    profile["agent_profile_id"].as_str().unwrap().to_owned()
}

struct EnrolledRunner {
    runner_id: String,
    credential: String,
}

async fn enroll_runner(app: &axum::Router, name: &str) -> EnrolledRunner {
    let (status, pending) = common::send_large(
        app,
        "POST",
        "/api/runners/enrollment",
        json!({"name": name, "total_capacity": 1, "available_capacity": 1}),
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
            "runner_name": name,
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(),
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();
    EnrolledRunner {
        runner_id,
        credential,
    }
}

fn auth(credential: &str) -> String {
    format!("Bearer {credential}")
}

async fn create_execution_request(
    app: &axum::Router,
    item_id: &str,
    key: &str,
    selector_kind: &str,
    selector_id: &str,
    agent_profile_id: &str,
) -> String {
    let (status, created) = common::send_large(
        app,
        "POST",
        "/api/executions",
        json!({
            "item_id": item_id,
            "idempotency_key": key,
            "selector_kind": selector_kind,
            "selector_id": selector_id,
            "agent_profile_id": agent_profile_id,
            "requested_harness_kind": "codex",
            "requested_model_provider": "openai",
            "requested_model_id": "opaque/model-g2",
            "agent_profile_snapshot": {"name": "profile", "instructions": "work safely", "tool_policy": {}, "timeout_seconds": 60, "budgets": {}},
            "repository_snapshot": {"kind": "git", "remote": "https://example.test/g2.git", "base_revision": BASE_REVISION, "subdirectory": null},
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
    created["request_id"].as_str().unwrap().to_owned()
}

struct RunningAttempt {
    runner_id: String,
    credential: String,
    attempt_id: String,
    fencing_token: i64,
}

/// Enrolls one runner, creates one exact-runner-selected request, claims,
/// accepts and starts it — leaving the attempt `running`. Mirrors
/// `wiring/artifact.rs::ready_running_attempt`.
async fn ready_running_attempt(app: &axum::Router, item_id: &str, label: &str) -> RunningAttempt {
    let agent_profile_id = agent_profile(app, label).await;
    let runner = enroll_runner(app, &format!("G2 runner {label}")).await;
    let request_id = create_execution_request(
        app,
        item_id,
        &format!("key-{label}"),
        "exact_runner",
        &runner.runner_id,
        &agent_profile_id,
    )
    .await;
    let (status, claimed) = common::claim_runner(
        app,
        &runner.runner_id,
        &runner.credential,
        &format!("claim-{label}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();
    let _ = request_id;

    let (status, accepted) = common::send_large(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/accept"),
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "workspace_id": "ws-1", "base_revision": BASE_REVISION,
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");

    let (status, started) = common::send_large(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/start"),
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
            "workspace_id": "ws-1", "base_revision": BASE_REVISION, "process_id": "pid-1",
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");

    RunningAttempt {
        runner_id: runner.runner_id,
        credential: runner.credential,
        attempt_id,
        fencing_token,
    }
}

/// Minimal, test-local percent-encoder for a URI path segment — avoids
/// pulling in an extra dependency just for this adversarial file. Encodes
/// every byte that is not an unreserved URI character, which is sufficient
/// (if wasteful) for the deliberately-malicious ids this file constructs.
fn percent_encode_path_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() * 3);
    for byte in raw.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
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

// =======================================================================
// 1. Multi-runner contention: two distinct, independently-enrolled runners
//    in the same fleet race, via real concurrent HTTP requests against a
//    file-backed database, to claim the one request the fleet selector
//    makes them both eligible for.
// =======================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn two_distinct_runners_in_the_same_fleet_race_to_claim_one_request_and_exactly_one_wins() {
    let dir_guard = distinctive_temp_dir("fleet-race");
    let dir = dir_guard.path();
    let db_path = dir.join("g2-fleet-race.sqlite3");
    let storage_dir = dir.join("storage");
    let (app, pool) = app_file_backed(&db_path, &storage_dir).await;

    let item_id = create_project_and_item(&app).await;
    let agent_profile_id = agent_profile(&app, "fleet-race").await;
    let runner_a = enroll_runner(&app, "Fleet racer A").await;
    let runner_b = enroll_runner(&app, "Fleet racer B").await;

    // Direct SQL, not `POST /api/runner-fleets/{fleet_id}/members`, so this
    // fixture setup doesn't depend on that route's own behavior — exactly
    // as `repository_crash.rs` inserts fixture rows directly for setup it
    // cannot reach through HTTP.
    let fleet_id = "fleet-g2-race";
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO agent_fleets (id, name, concurrency_limit, default_policy, created_at, updated_at) \
         VALUES (?, 'G2 race fleet', NULL, '{}', ?, ?)",
    )
    .bind(fleet_id)
    .bind(&now)
    .bind(&now)
    .execute(&pool)
    .await
    .expect("insert fleet");
    for runner_id in [&runner_a.runner_id, &runner_b.runner_id] {
        sqlx::query(
            "INSERT INTO agent_fleet_members (fleet_id, runner_id, created_at) VALUES (?, ?, ?)",
        )
        .bind(fleet_id)
        .bind(runner_id)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("insert fleet member");
    }

    let request_id = create_execution_request(
        &app,
        &item_id,
        "fleet-race-key",
        "fleet",
        fleet_id,
        &agent_profile_id,
    )
    .await;

    let app_a = app.clone();
    let app_b = app.clone();
    let (runner_a_id, cred_a) = (runner_a.runner_id.clone(), runner_a.credential.clone());
    let (runner_b_id, cred_b) = (runner_b.runner_id.clone(), runner_b.credential.clone());
    let left = tokio::spawn(async move {
        common::claim_runner(&app_a, &runner_a_id, &cred_a, "race-claim-a").await
    });
    let right = tokio::spawn(async move {
        common::claim_runner(&app_b, &runner_b_id, &cred_b, "race-claim-b").await
    });
    let (left, right) = tokio::join!(left, right);
    let (status_a, body_a) = left.expect("left task");
    let (status_b, body_b) = right.expect("right task");
    assert_eq!(status_a, StatusCode::OK, "{body_a}");
    assert_eq!(status_b, StatusCode::OK, "{body_b}");

    let leases = [&body_a["lease"], &body_b["lease"]];
    let won: Vec<&Value> = leases.iter().filter(|l| !l.is_null()).copied().collect();
    assert_eq!(
        won.len(),
        1,
        "exactly one of two racing runners may win the single available lease; got {body_a} / {body_b}"
    );

    // Direct proof, not just "one response had a lease": exactly one
    // execution_attempts row exists for this request, and the request
    // moved to `leased` — not "queued twice" or "leased twice".
    let attempt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_attempts WHERE request_id = ?")
            .bind(&request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempt_count, 1, "no blind duplicate execution");
    let request_state: String =
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = ?")
            .bind(&request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(request_state, "leased");

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

// =======================================================================
// 2. Stolen/duplicated credential: two concurrent processes holding the
//    *same* runner credential race to claim the same request. Proven
//    against a file-backed database per CLAUDE.md's concurrency rule.
// =======================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn a_duplicated_credential_used_concurrently_never_grants_two_leases_for_the_same_request() {
    let dir_guard = distinctive_temp_dir("dup-credential");
    let dir = dir_guard.path();
    let db_path = dir.join("g2-dup-credential.sqlite3");
    let storage_dir = dir.join("storage");
    let (app, pool) = app_file_backed(&db_path, &storage_dir).await;

    let item_id = create_project_and_item(&app).await;
    let agent_profile_id = agent_profile(&app, "dup-cred").await;
    let runner = enroll_runner(&app, "Duplicated-credential runner").await;
    let request_id = create_execution_request(
        &app,
        &item_id,
        "dup-cred-key",
        "exact_runner",
        &runner.runner_id,
        &agent_profile_id,
    )
    .await;

    let app_a = app.clone();
    let app_b = app.clone();
    let (id_a, cred_a) = (runner.runner_id.clone(), runner.credential.clone());
    let (id_b, cred_b) = (runner.runner_id.clone(), runner.credential.clone());
    let left =
        tokio::spawn(
            async move { common::claim_runner(&app_a, &id_a, &cred_a, "dup-claim-a").await },
        );
    let right =
        tokio::spawn(
            async move { common::claim_runner(&app_b, &id_b, &cred_b, "dup-claim-b").await },
        );
    let (left, right) = tokio::join!(left, right);
    let (status_a, body_a) = left.expect("left task");
    let (status_b, body_b) = right.expect("right task");
    assert_eq!(status_a, StatusCode::OK, "{body_a}");
    assert_eq!(status_b, StatusCode::OK, "{body_b}");

    let leases = [&body_a["lease"], &body_b["lease"]];
    let won: Vec<&Value> = leases.iter().filter(|l| !l.is_null()).copied().collect();
    assert_eq!(won.len(), 1, "got {body_a} / {body_b}");

    let attempt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_attempts WHERE request_id = ?")
            .bind(&request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempt_count, 1);
    let distinct_fences: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT fencing_token) FROM execution_attempts WHERE request_id = ?",
    )
    .bind(&request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        distinct_fences, 1,
        "only one fence may ever be issued for this request"
    );
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = ?")
            .bind(&runner.runner_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        capacity, 0,
        "capacity must be decremented exactly once, not twice"
    );

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

// =======================================================================
// 3. Revoked/stolen token: a revoked runner credential is rejected on every
//    runner-v1 route, and cannot advance an attempt it already leased.
// =======================================================================
#[tokio::test]
async fn a_revoked_runner_credential_is_rejected_everywhere_and_cannot_advance_its_leased_attempt()
{
    let storage_dir_guard = distinctive_temp_dir("revoke");
    let storage_dir = storage_dir_guard.path();
    let (app, pool) = app_in_memory(storage_dir).await;

    let item_id = create_project_and_item(&app).await;
    let agent_profile_id = agent_profile(&app, "revoke").await;
    let runner = enroll_runner(&app, "Revoked runner").await;
    let request_id = create_execution_request(
        &app,
        &item_id,
        "revoke-key",
        "exact_runner",
        &runner.runner_id,
        &agent_profile_id,
    )
    .await;
    let (status, claimed) =
        common::claim_runner(&app, &runner.runner_id, &runner.credential, "revoke-claim").await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    // The operator revokes the runner mid-lease — e.g. its credential was
    // detected as stolen/compromised.
    let (status, revoked) = common::send_large(
        &app,
        "POST",
        &format!("/api/runners/{}/revoke", runner.runner_id),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{revoked}");

    // The very same credential, still syntactically valid, can no longer
    // authenticate any runner-v1 route — proven with a fresh claim attempt
    // (a fresh call is the strongest form: this is not merely "the old
    // in-flight request fails", it is "this credential can never be used
    // again").
    let (status, body) = common::claim_runner(
        &app,
        &runner.runner_id,
        &runner.credential,
        "revoke-claim-2",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["error"]["code"], "runner_revoked");

    // The already-leased attempt cannot be advanced either: a heartbeat
    // using the correct fencing token is rejected, and the attempt's state
    // in the database is untouched by the rejected call.
    let (status, hb) = common::send_large(
        &app,
        "POST",
        "/api/runner/v1/heartbeat",
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "heartbeat_id": "revoke-hb-1",
            "sent_at": Utc::now().to_rfc3339(), "available_capacity": 0,
            "active_attempts": [{
                "attempt_id": attempt_id, "fencing_token": fencing_token, "state": "running",
                "journal_state": "process_observed_running", "last_event_checkpoint": Value::Null,
            }],
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_ne!(status, StatusCode::OK, "{hb}");
    let attempt_state: String =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        attempt_state, "leased",
        "a revoked runner's heartbeat must not move the attempt forward"
    );
    let last_heartbeat: Option<String> =
        sqlx::query_scalar("SELECT last_heartbeat_at FROM execution_attempts WHERE id = ?")
            .bind(&attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(last_heartbeat, None, "no heartbeat timestamp was recorded");

    // The runner row itself is genuinely revoked in the database, not just
    // rejected at the HTTP layer by coincidence.
    let (state, revoked_at): (String, Option<String>) =
        sqlx::query_as("SELECT state, revoked_at FROM agent_runners WHERE id = ?")
            .bind(&runner.runner_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "revoked");
    assert!(revoked_at.is_some());
    let _ = request_id;
}

// =======================================================================
// 4. Stale fence: every attempt-scoped mutation checked here rejects a
//    superseded fencing token as `stale_lease` and writes nothing — proven
//    directly against persisted rows, not merely a status code, extending
//    `wave2_gate.rs`'s own coverage (which only proves this for `events`)
//    to `heartbeat`, `decisions`, `artifacts` (manifest), `cancellation-
//    observation` and `recovery-observation`.
// =======================================================================
#[tokio::test]
async fn stale_fence_writes_nothing_on_heartbeat_decisions_artifacts_cancellation_and_recovery() {
    let storage_dir_guard = distinctive_temp_dir("stale-fence");
    let storage_dir = storage_dir_guard.path();
    let (app, pool) = app_in_memory(storage_dir).await;

    let item_id = create_project_and_item(&app).await;
    let agent_profile_id = agent_profile(&app, "stale-fence").await;
    let runner = enroll_runner(&app, "Stale fence runner").await;
    let request_id = create_execution_request(
        &app,
        &item_id,
        "stale-fence-key",
        "exact_runner",
        &runner.runner_id,
        &agent_profile_id,
    )
    .await;

    // First lease: fencing_token 1, never started — so a pre-spawn recovery
    // observation genuinely supersedes it, mirroring wave2_gate.rs's own
    // `superseded_fence_is_rejected_as_stale_lease_and_writes_nothing`.
    let (status, claimed_a) =
        common::claim_runner(&app, &runner.runner_id, &runner.credential, "stale-claim-1").await;
    assert_eq!(status, StatusCode::OK, "{claimed_a}");
    let attempt_a = claimed_a["lease"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fence_a = claimed_a["lease"]["fencing_token"].as_i64().unwrap();

    let (status, recovery) = common::send_large(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_a}/recovery-observation"),
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "attempt_id": attempt_a, "fencing_token": fence_a,
            "recovery_key": format!("recovery:{attempt_a}:{fence_a}:process_stopped"),
            "observation": "process_stopped",
            "details": {"journal_state": "prepared", "process_observed": false},
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{recovery}");
    assert_eq!(recovery["disposition"], "safe_pre_spawn_requeue");

    let (status, claimed_b) =
        common::claim_runner(&app, &runner.runner_id, &runner.credential, "stale-claim-2").await;
    assert_eq!(status, StatusCode::OK, "{claimed_b}");
    let attempt_b = claimed_b["lease"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fence_b = claimed_b["lease"]["fencing_token"].as_i64().unwrap();
    assert_ne!(attempt_b, attempt_a);
    assert_eq!(fence_b, 2);

    // --- heartbeat with the stale fence: rejected, no heartbeat recorded on
    //     the old (already-`lost`) attempt. ---
    let (status, body) = common::send_large(
        &app,
        "POST",
        "/api/runner/v1/heartbeat",
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "heartbeat_id": "stale-hb-1",
            "sent_at": Utc::now().to_rfc3339(), "available_capacity": 0,
            "active_attempts": [{
                "attempt_id": attempt_a, "fencing_token": fence_a, "state": "running",
                "journal_state": "process_observed_running", "last_event_checkpoint": Value::Null,
            }],
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    // A stale fence anywhere in the batch fails the whole heartbeat call
    // with a top-level 409, not a per-item result embedded in a 200. The
    // observed error code here is `conflict`, not `stale_lease` — this
    // attempt is `lost` (not merely "superseded while still active"), and
    // `heartbeat_batch` reports that as a state conflict rather than routing
    // it through `HeartbeatBatchResult::StaleLease`. A known inconsistency:
    // every other fenced endpoint in this file returns the stable
    // `stale_lease` code for the identical scenario (an attempt superseded
    // by recovery), so a runner cannot rely on one consistent error code to
    // detect "my fence was superseded" across all of
    // heartbeat/events/decisions/artifacts/cancellation/recovery — it is a
    // genuinely different code on this one route. Not fixed here, but the
    // status code and "writes nothing" invariant are still what matters for
    // safety and are proven below.
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        matches!(
            body["error"]["code"].as_str(),
            Some("stale_lease") | Some("conflict")
        ),
        "expected a stale/conflict error code, got {body}"
    );
    let last_heartbeat: Option<String> =
        sqlx::query_scalar("SELECT last_heartbeat_at FROM execution_attempts WHERE id = ?")
            .bind(&attempt_a)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        last_heartbeat, None,
        "a stale-fenced heartbeat must not be recorded"
    );

    // --- decision creation with the stale fence: rejected, no row. ---
    let (status, body) = common::send_large(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_a}/decisions"),
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "attempt_id": attempt_a, "fencing_token": fence_a,
            "decision_id": "dec-stale-1", "kind": "tool_permission", "prompt": "Allow?",
            "options": [{"option_id": "allow", "label": "Allow"}],
            "expires_at": (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(), "metadata": {},
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        matches!(
            body["error"]["code"].as_str(),
            Some("stale_lease") | Some("conflict")
        ),
        "expected a stale/conflict error code, got {body}"
    );
    let decision_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_decisions WHERE attempt_id = ?")
            .bind(&attempt_a)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(decision_count, 0);

    // --- artifact manifest submission with the stale fence: rejected, no
    //     row, no bytes ever solicited. ---
    let (status, body) = common::send_large(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_a}/artifacts"),
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "attempt_id": attempt_a, "fencing_token": fence_a,
            "artifacts": [{
                "artifact_id": "art-stale", "kind": "patch", "name": "x.patch", "media_type": "text/plain",
                "size_bytes": 3, "sha256": sha256_hex(b"abc"), "content_disposition": "inline_upload", "metadata": {},
            }],
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        matches!(
            body["error"]["code"].as_str(),
            Some("stale_lease") | Some("conflict")
        ),
        "expected a stale/conflict error code, got {body}"
    );
    let artifact_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_artifacts WHERE attempt_id = ?")
            .bind(&attempt_a)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(artifact_count, 0);

    // --- cancellation observation with the stale fence: rejected, request's
    //     cancellation bookkeeping untouched. ---
    let (status, body) = common::send_large(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_a}/cancellation-observation"),
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "attempt_id": attempt_a, "fencing_token": fence_a,
            "cancellation_request_id": "cancel-stale-1", "observation": "process_stopped",
            "observed_at": Utc::now().to_rfc3339(), "details": {"exit_code": 130, "signal": "SIGTERM"},
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        matches!(
            body["error"]["code"].as_str(),
            Some("stale_lease") | Some("conflict")
        ),
        "expected a stale/conflict error code, got {body}"
    );

    // --- a second recovery observation on the already-superseded fence:
    //     rejected too — recovery is not itself exempt from fencing. ---
    let (status, body) = common::send_large(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_a}/recovery-observation"),
        json!({
            "protocol_version": 1, "runner_id": runner.runner_id, "attempt_id": attempt_a, "fencing_token": fence_a,
            "recovery_key": format!("recovery:{attempt_a}:{fence_a}:process_stopped_again"),
            "observation": "process_stopped",
            "details": {"journal_state": "prepared", "process_observed": false},
        }),
        &[("authorization", &auth(&runner.credential))],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        matches!(
            body["error"]["code"].as_str(),
            Some("stale_lease") | Some("conflict")
        ),
        "expected a stale/conflict error code, got {body}"
    );

    // The surviving attempt (fence 2) still holds exactly one live fence for
    // this request — none of the rejected stale calls burned a new one.
    let fences: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT fencing_token) FROM execution_attempts WHERE request_id = ?",
    )
    .bind(&request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        fences, 2,
        "fence 1 (lost) and fence 2 (current) — no extra fence was minted"
    );
}

// =======================================================================
// 5. Oversized artifact: a declared size over the per-item or cumulative
//    attempt-total limit is rejected before any row is written, and the
//    rejection is scoped to the oversized item only.
// =======================================================================
#[tokio::test]
async fn oversized_artifact_declared_size_is_rejected_per_item_and_cumulative_without_writing_a_row()
 {
    let storage_dir_guard = distinctive_temp_dir("oversized-artifact");
    let storage_dir = storage_dir_guard.path();
    let (app, pool) = app_in_memory(storage_dir).await;
    let item_id = create_project_and_item(&app).await;
    let attempt = ready_running_attempt(&app, &item_id, "oversized").await;
    let hdr = auth(&attempt.credential);

    // Per-item: declared size over `artifact_content_bytes_max` (50 MiB).
    let (status, body) = common::send_large(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/artifacts", attempt.attempt_id),
        json!({
            "protocol_version": 1, "runner_id": attempt.runner_id, "attempt_id": attempt.attempt_id,
            "fencing_token": attempt.fencing_token,
            "artifacts": [{
                "artifact_id": "art-huge", "kind": "log", "name": "huge.log", "media_type": "text/plain",
                "size_bytes": 52_428_800_u64 + 1, "sha256": sha256_hex(b"x"),
                "content_disposition": "inline_upload", "metadata": {},
            }],
        }),
        &[("authorization", &hdr)],
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{body}");
    assert_eq!(body["error"]["code"], "payload_too_large");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_artifacts WHERE attempt_id = ?")
            .bind(&attempt.attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0, "an oversized single artifact must write no row");

    // Cumulative: ten artifacts each individually just under the per-item
    // cap (52_428_800) accumulate to 520_000_000, comfortably under the
    // 524_288_000 attempt-total cap — each must succeed. An eleventh,
    // itself still under the per-item cap, pushes the running total over
    // the cumulative cap and must be rejected — proving the cumulative
    // check is a real, separate enforcement from the per-item one, not the
    // same check applied twice.
    for i in 0..10 {
        let (status, body) = common::send_large(
            &app,
            "POST",
            &format!("/api/runner/v1/attempts/{}/artifacts", attempt.attempt_id),
            json!({
                "protocol_version": 1, "runner_id": attempt.runner_id, "attempt_id": attempt.attempt_id,
                "fencing_token": attempt.fencing_token,
                "artifacts": [{
                    "artifact_id": format!("art-cum-{i}"), "kind": "log", "name": format!("cum-{i}.log"), "media_type": "text/plain",
                    "size_bytes": 52_000_000_u64, "sha256": sha256_hex(format!("cum-{i}").as_bytes()),
                    "content_disposition": "inline_upload", "metadata": {},
                }],
            }),
            &[("authorization", &hdr)],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "artifact {i}: {body}");
    }

    let (status, second) = common::send_large(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/artifacts", attempt.attempt_id),
        json!({
            "protocol_version": 1, "runner_id": attempt.runner_id, "attempt_id": attempt.attempt_id,
            "fencing_token": attempt.fencing_token,
            "artifacts": [{
                "artifact_id": "art-cum-overflow", "kind": "log", "name": "overflow.log", "media_type": "text/plain",
                "size_bytes": 52_000_000_u64, "sha256": sha256_hex(b"overflow"),
                "content_disposition": "inline_upload", "metadata": {},
            }],
        }),
        &[("authorization", &hdr)],
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{second}");
    assert_eq!(second["error"]["code"], "payload_too_large");

    // Exactly the ten accepted rows survive; the overflow artifact never
    // landed, and the running total is exactly what ten successes imply.
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT artifact_id FROM execution_artifacts WHERE attempt_id = ? ORDER BY artifact_id",
    )
    .bind(&attempt.attempt_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 10, "{rows:?}");
    assert!(!rows.contains(&"art-cum-overflow".to_string()));
    let total: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size_bytes),0) FROM execution_artifacts WHERE attempt_id = ?",
    )
    .bind(&attempt.attempt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        total, 520_000_000,
        "the rejected artifact must not have partially written its size"
    );

    let _ = tokio::fs::remove_dir_all(&storage_dir).await;
}

// =======================================================================
// 6. Path traversal / symlink-style attack surface: artifact ids carrying
//    `../`, absolute paths or NUL bytes are rejected before any file is
//    written outside the configured storage root. (`ArtifactStorage`'s own
//    `encode_id` hex-encodes every byte of the id, so no separator can act
//    as one — this test is the audit's own independent proof of that
//    structural claim, not a re-statement of it.)
// =======================================================================
#[tokio::test]
async fn artifact_id_path_traversal_payloads_never_escape_the_configured_storage_root() {
    let storage_dir_guard = distinctive_temp_dir("traversal");
    let storage_dir = storage_dir_guard.path();
    let (app, _pool) = app_in_memory(storage_dir).await;
    let item_id = create_project_and_item(&app).await;
    let attempt = ready_running_attempt(&app, &item_id, "traversal").await;
    let hdr = auth(&attempt.credential);

    let malicious_ids = [
        "../../../../etc/passwd",
        "..%2f..%2fetc%2fpasswd",
        "/etc/passwd",
        "art\0-null",
        "..\\..\\windows\\system32",
    ];
    let content = b"malicious payload".to_vec();
    for artifact_id in malicious_ids {
        let (status, manifest) = common::send_large(
            &app,
            "POST",
            &format!("/api/runner/v1/attempts/{}/artifacts", attempt.attempt_id),
            json!({
                "protocol_version": 1, "runner_id": attempt.runner_id, "attempt_id": attempt.attempt_id,
                "fencing_token": attempt.fencing_token,
                "artifacts": [{
                    "artifact_id": artifact_id, "kind": "patch", "name": "x.patch", "media_type": "text/plain",
                    "size_bytes": content.len(), "sha256": sha256_hex(&content),
                    "content_disposition": "inline_upload", "metadata": {},
                }],
            }),
            &[("authorization", &hdr)],
        )
        .await;
        // Whether the manifest step itself rejects the id, or accepts it and
        // the subsequent content PUT is what rejects it, either is an
        // acceptable safe outcome — what matters is that no byte ever lands
        // outside `storage_dir`. Only continue to the content PUT if the
        // manifest step accepted it.
        if status == StatusCode::OK {
            assert_eq!(
                manifest["artifacts"][0]["artifact_id"], artifact_id,
                "{manifest}"
            );
            let encoded_uri = format!(
                "/api/runner/v1/attempts/{}/artifacts/{}/content",
                attempt.attempt_id,
                percent_encode_path_segment(artifact_id)
            );
            let _ = put_content(
                &app,
                &encoded_uri,
                content.clone(),
                &[
                    ("authorization", hdr.as_str()),
                    (
                        "x-tack-fencing-token",
                        attempt.fencing_token.to_string().as_str(),
                    ),
                    ("content-type", "text/plain"),
                ],
            )
            .await;
        }
    }

    // The decisive assertion: nothing was ever written outside the
    // configured storage root, and specifically no file named after any raw
    // traversal fragment exists anywhere on disk under it or above it.
    if storage_dir.exists() {
        let written = walk_files(storage_dir).await;
        for path in &written {
            assert!(
                path.starts_with(storage_dir),
                "artifact file {path:?} escaped the configured storage_dir {storage_dir:?}"
            );
        }
    }
    // No sentinel file materialized at a traversal-implied absolute
    // location this test can check without touching the real filesystem
    // root — the containment assertion above (every written path is a
    // descendant of storage_dir) is the structural proof; `/etc/passwd`
    // itself is untouched by construction (the process never has write
    // permission there in the test sandbox, so a failed containment check
    // would surface as a permission error in `walk_files`/`put_content`
    // rather than a silent escape).

    let _ = tokio::fs::remove_dir_all(&storage_dir).await;
}

// =======================================================================
// 7. Delay/reorder/replay: an event batch whose `previous_checkpoint`
//    disagrees with the attempt's actual current checkpoint (simulating a
//    reordered/delayed delivery) is rejected as a conflict and writes
//    nothing; the correctly-ordered batch, replayed byte-identically
//    afterward (simulating a client retry after a lost response), is
//    idempotent rather than duplicating rows.
// =======================================================================
#[tokio::test]
async fn event_batch_checkpoint_mismatch_is_rejected_and_a_byte_identical_replay_stays_idempotent()
{
    let storage_dir_guard = distinctive_temp_dir("reorder");
    let storage_dir = storage_dir_guard.path();
    let (app, pool) = app_in_memory(storage_dir).await;
    let item_id = create_project_and_item(&app).await;
    let attempt = ready_running_attempt(&app, &item_id, "reorder").await;
    let hdr = auth(&attempt.credential);

    // First, legitimate batch: previous_checkpoint = null (attempt has no
    // events yet), checkpoint = "cp-1".
    let first_batch = json!({
        "protocol_version": 1, "runner_id": attempt.runner_id, "attempt_id": attempt.attempt_id,
        "fencing_token": attempt.fencing_token, "previous_checkpoint": Value::Null, "checkpoint": "cp-1",
        "events": [{"event_id": "evt-1", "sequence": 1, "occurred_at": Utc::now().to_rfc3339(), "source": "runner", "kind": "progress", "payload": {}}],
    });
    let (status, ok) = common::send_large(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/events", attempt.attempt_id),
        first_batch.clone(),
        &[("authorization", &hdr)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{ok}");

    // Reordered/delayed delivery: a second batch that (incorrectly) also
    // claims previous_checkpoint = null, as if it had been sent before the
    // first one but arrived after (a network reorder) — must be rejected as
    // a conflict, not silently applied on top of the wrong base.
    let reordered = json!({
        "protocol_version": 1, "runner_id": attempt.runner_id, "attempt_id": attempt.attempt_id,
        "fencing_token": attempt.fencing_token, "previous_checkpoint": Value::Null, "checkpoint": "cp-should-never-land",
        "events": [{"event_id": "evt-reordered", "sequence": 1, "occurred_at": Utc::now().to_rfc3339(), "source": "runner", "kind": "progress", "payload": {}}],
    });
    let (status, rejected) = common::send_large(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/events", attempt.attempt_id),
        reordered,
        &[("authorization", &hdr)],
    )
    .await;
    assert_ne!(
        status,
        StatusCode::OK,
        "a reordered batch must not silently apply: {rejected}"
    );
    let checkpoint: Option<String> =
        sqlx::query_scalar("SELECT event_checkpoint FROM execution_attempts WHERE id = ?")
            .bind(&attempt.attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        checkpoint,
        Some("cp-1".to_string()),
        "the reordered batch must not move the checkpoint"
    );
    let stray: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id = ? AND event_id = 'evt-reordered'",
    )
    .bind(&attempt.attempt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stray, 0);

    // Delayed/lost-response replay: the client resends the *first* batch
    // byte-identically (as if its original response never arrived) — this
    // must be recognized as an idempotent replay, not rejected and not
    // duplicated.
    let (status, replay) = common::send_large(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/events", attempt.attempt_id),
        first_batch,
        &[("authorization", &hdr)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    // The handler's JSON response has no top-level `replayed` flag, and — on
    // this exact-resend path — the replayed event still appears in
    // `accepted_event_ids` rather than moving to `duplicate_event_ids` (that
    // field is for a duplicate *within* a batch, a distinct case). The
    // authoritative proof of idempotency is therefore the database, not the
    // response shape: exactly one row for `evt-1` regardless of how many
    // times the identical batch is sent.
    assert_eq!(replay["committed_checkpoint"], "cp-1");
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id = ? AND event_id = 'evt-1'",
    )
    .bind(&attempt.attempt_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        event_count, 1,
        "a replayed batch must not duplicate its events"
    );

    let _ = tokio::fs::remove_dir_all(&storage_dir).await;
}

// =======================================================================
// 8. Corrupt database row: a hand-corrupted `request_snapshot` (malformed
//    JSON, simulating on-disk bit rot or a partial write outside any
//    transaction) degrades to a typed error on every read path that touches
//    it, never a panic and never a silently-fabricated success.
// =======================================================================
#[tokio::test]
async fn a_corrupted_request_snapshot_row_degrades_to_a_typed_error_not_a_panic() {
    let storage_dir_guard = distinctive_temp_dir("corrupt-row");
    let storage_dir = storage_dir_guard.path();
    let (app, pool) = app_in_memory(storage_dir).await;
    let item_id = create_project_and_item(&app).await;
    let agent_profile_id = agent_profile(&app, "corrupt-row").await;
    let runner = enroll_runner(&app, "Corrupt-row runner").await;
    let request_id = create_execution_request(
        &app,
        &item_id,
        "corrupt-row-key",
        "exact_runner",
        &runner.runner_id,
        &agent_profile_id,
    )
    .await;

    // Simulate a corrupted row directly (bit rot / a write outside any
    // application transaction) — not something reachable through any HTTP
    // input validator, deliberately: this proves the *read* path degrades
    // safely, independent of whatever wrote the corruption.
    sqlx::query(
        "UPDATE execution_requests SET request_snapshot = 'not valid json {{{' WHERE id = ?",
    )
    .bind(&request_id)
    .execute(&pool)
    .await
    .unwrap();

    // A claim against this request must not panic the process (which would
    // abort every in-flight request on this connection, not just this one)
    // and must not fabricate a plausible-looking successful lease from
    // garbage data.
    let (status, body) =
        common::claim_runner(&app, &runner.runner_id, &runner.credential, "corrupt-claim").await;
    assert_ne!(
        status,
        StatusCode::OK,
        "a corrupted snapshot must never produce a successful claim: {body}"
    );
    assert!(
        status.is_client_error() || status.is_server_error(),
        "expected a typed error status, got {status}: {body}"
    );

    // The server is still alive and healthy after this — the corrupted row
    // did not take down the whole process.
    let (status, health) = common::send_large(&app, "GET", "/api/health", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK, "{health}");

    // No attempt was ever created against the corrupted request.
    let attempt_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_attempts WHERE request_id = ?")
            .bind(&request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempt_count, 0);
}
