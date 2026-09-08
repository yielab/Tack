//! Runner-protocol lifecycle over HTTP: enroll, refresh, claim, heartbeat,
//! fencing, events, completion, recovery, and the operator/runner
//! auth non-substitution proof.
//!
//! Loads `handlers/runner_protocol.rs` the same way
//! `handlers/executions_runner_admin.rs`
//! loads its own handlers: via `#[path]`. `runner_protocol.rs`'s own
//! `mod runner_auth;` then resolves relative to that file's path
//! (`handlers/runner_protocol/runner_auth.rs`), which is what keeps the
//! auth module unregistered in `handlers.rs` while still compiling here.
//! `artifact_events` (the sibling module in this binary) loads the same
//! product file again under its own module path rather than sharing this
//! one — each keeps its own directly-constructed router, matching the two
//! files' own separate history before this binary merged them; see the
//! `#[allow(clippy::duplicate_mod)]` below.
//!
//! `executions.rs` is loaded read-only, the same technique its own test
//! already uses on itself, so the operator/runner auth
//! non-substitution test can exercise the real operator router rather than a
//! stub. The file is not modified. (`runner_admin.rs` is deliberately not
//! loaded here: it is a separate, independently-evolving file this
//! test does not need, and pulling it into this binary would make this
//! file's compilation depend on runner_admin.rs's own edits.)

// `artifact_events`'s own copy of this same `#[path]` load is a second,
// independent module tree over the identical file — clippy's default
// lint set forbids that within one crate. Allowed deliberately: collapsing
// the two into one shared module would also collapse `runner_protocol.rs`'s
// own colocated unit tests from two independent copies into one, changing
// this binary's test count.
#[allow(clippy::duplicate_mod)]
#[path = "../../src/handlers/runner_protocol.rs"]
mod runner_protocol;

#[path = "../../src/handlers/executions.rs"]
mod executions;

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

const RUNNER_ID: &str = "runner-c2";
const RUNNER_CREDENTIAL: &str = "raw-test-runner-credential";
/// The explicit model every `enqueue_request()` call requests, matching
/// `full_capabilities()`'s declared combination — see the scheduler
/// wiring note on both functions.
const REQUESTED_MODEL_PROVIDER: &str = "openai";
const REQUESTED_MODEL_ID: &str = "opaque/model-c2";

// ---------------------------------------------------------------------
// Fake clock: every lease/heartbeat/expiry test below injects time rather
// than sleeping (rule 9).
// ---------------------------------------------------------------------

#[derive(Clone)]
struct FakeClock(Arc<Mutex<DateTime<Utc>>>);

impl FakeClock {
    fn new(start: DateTime<Utc>) -> Self {
        Self(Arc::new(Mutex::new(start)))
    }

    fn advance(&self, delta: Duration) {
        let mut guard = self.0.lock().unwrap();
        *guard += delta;
    }
}

impl ExecutionClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

// ---------------------------------------------------------------------
// Test setup.
// ---------------------------------------------------------------------

async fn setup() -> (Router, Repository, FakeClock, String) {
    // Must run before anything else in every test (see `log_capture.rs`'s
    // doc comment): it closes the race window between this binary's tests
    // by making sure no test's HTTP request can reach production handler
    // code before the one global `tracing` subscriber this binary ever
    // installs is in place.
    ensure_global_log_capture_installed();
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES (?, 'C2', '{}')")
        .bind(workspace.to_string())
        .execute(repo.pool())
        .await
        .expect("workspace");
    let project = repo
        .create_project(
            workspace,
            CreateProject {
                name: "C2".into(),
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

    let clock = FakeClock::new(Utc.with_ymd_and_hms(2026, 8, 6, 12, 0, 0).unwrap());
    let credential_hash = runner_protocol::runner_auth::credential_hash(RUNNER_CREDENTIAL);
    // `tack_orch::scheduler` is wired into the real claim path, so
    // `RUNNER_ID` must declare a real harness/model combination — an empty
    // `"{}"` snapshot ("not enrolled
    // yet, doesn't matter") would make every claim in this file
    // eligibility-reject before ever reaching the fencing/idempotency/replay
    // behavior these tests actually exist to prove.
    let capability_snapshot = full_capabilities(clock.now(), 2, 2).to_string();
    repo.register_runner(
        NewRunner {
            id: RUNNER_ID,
            name: "C2 Runner",
            credential_hash: &credential_hash,
            labels: "{}",
            total_capacity: 2,
            available_capacity: 2,
            capability_snapshot: &capability_snapshot,
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .expect("runner");
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-c2",
            name: "C2 Profile",
            instructions: "work safely",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: r#"{"tokens":1000}"#,
        },
        &clock,
    )
    .await
    .expect("profile");

    let state = runner_protocol::RunnerProtocolState::new(repo.clone(), Arc::new(clock.clone()));
    // This router is tested in isolation from any operator
    // config, so `usize::MAX` here means "no additional global-config
    // restriction" — the effective limit still collapses to the fixed 4 MiB
    // protocol ceiling via `effective_body_limit_bytes`. The min-of-configured-and-ceiling
    // precedence itself is proven against the real production router in
    // `handlers/production_router.rs`, not here.
    let app = runner_protocol::routes(state, usize::MAX);
    (app, repo, clock, item.id.to_string())
}

/// Enqueues an execution request selecting `runner_id`, matching the frozen
/// snapshot shape exactly (mirrors what `create_execution` normalizes).
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
        "created_by": {"source": "test", "subject_id": "c2-test"},
        "created_at": created_at.to_rfc3339(),
        "selector": {"kind": "exact_runner", "runner_id": runner_id},
        "agent_profile_id": agent_profile_id,
        "resolved_agent_profile": {"name":"C2 Profile","instructions":"work safely","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{"tokens":1000}},
        "requested_harness_kind": "codex",
        // Explicit, matching `full_capabilities()`'s declared combination —
        // see `REQUESTED_MODEL_PROVIDER`/`REQUESTED_MODEL_ID`'s doc comment.
        "requested_model_provider": REQUESTED_MODEL_PROVIDER,
        "requested_model_id": REQUESTED_MODEL_ID,
        "repository": {"kind":"git","remote":"https://example.test/c2.git","base_revision":"abc123def456abc123def456abc123def456abc","subdirectory": Value::Null},
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

async fn send(
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
    let bytes = to_bytes(response.into_body(), 8 * 1_048_576).await.unwrap();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

async fn send_as_runner(
    app: &Router,
    method: &str,
    uri: &str,
    body: String,
) -> (StatusCode, Value) {
    send(
        app,
        method,
        uri,
        body,
        &[("authorization", &format!("Bearer {RUNNER_CREDENTIAL}"))],
    )
    .await
}

/// `harnesses` declares "codex"/"openai"/`REQUESTED_MODEL_ID` — every
/// `enqueue_request()` call in this file requests exactly that pair (see
/// its own doc comment) so the real scheduler finds this
/// runner eligible, the same way a real runner's declared capabilities
/// would need to match a real request for a claim to succeed.
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

fn artifact_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    artifact_id: &str,
    sha256: &str,
) -> String {
    json!({
        "protocol_version": 1,
        "runner_id": runner_id,
        "attempt_id": attempt_id,
        "fencing_token": fencing_token,
        "artifacts": [{
            "artifact_id": artifact_id,
            "kind": "patch",
            "name": "changes.patch",
            "media_type": "text/x-diff",
            "size_bytes": 12,
            "sha256": sha256,
            "content_disposition": "inline_upload",
            "metadata": {"base_revision": "abc123"},
        }],
    })
    .to_string()
}

fn completion_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    completion_id: &str,
    started_at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    final_event_checkpoint: Option<&str>,
) -> String {
    json!({
        "protocol_version": 1,
        "runner_id": runner_id,
        "attempt_id": attempt_id,
        "fencing_token": fencing_token,
        "completion_id": completion_id,
        "terminal_state": "succeeded",
        "terminal_reason": {"code": "completed", "message": "Harness exited successfully"},
        "actual_execution": {
            "harness_kind": "codex",
            "harness_version": "1.2.3",
            "model_provider": "openai",
            "model_id": "opaque/model-alpha",
            "model_observation_source": "harness_reported",
            "capability_snapshot": {
                "cancel": {"support": "supported", "reason": null},
                "resume": {"support": "unsupported", "reason": "no resumable session contract"},
                "decisions": {"support": "supported", "reason": null},
                "artifacts": {"support": "supported", "reason": null},
                "usage": {"support": "advisory", "reason": "usage may be absent"},
            },
            "workspace_id": "ws-1",
            "base_revision": "abc123def456abc123def456abc123def456abc",
            "started_at": started_at.to_rfc3339(),
            "ended_at": ended_at.to_rfc3339(),
        },
        "usage": {
            "tokens_in": {"value": 1234, "source": "measured"},
            "tokens_out": {"value": 456, "source": "measured"},
            "duration_ms": {"value": 295000, "source": "measured"},
            "cost_usd": {"value": null, "source": "not_measured"},
        },
        "final_event_checkpoint": final_event_checkpoint,
    })
    .to_string()
}

// ---------------------------------------------------------------------
// 1. Full lifecycle, exercised through a fresh enrollment.
// ---------------------------------------------------------------------

#[tokio::test]
async fn full_runner_protocol_lifecycle_enroll_through_completion() {
    let (app, repo, clock, item_id) = setup().await;

    let raw_enrollment_token = "example_lifecycle_enrollment_token";
    let token_hash = runner_protocol::runner_auth::credential_hash(raw_enrollment_token);
    repo.create_pending_runner_and_issue_token(
        NewRunner {
            id: "runner-lifecycle",
            name: "Lifecycle Runner",
            credential_hash: "pending:no-credential",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        EnrollmentToken {
            id: "tok-lifecycle",
            runner_id: "runner-lifecycle",
            token_hash: &token_hash,
            expires_at: clock.now() + Duration::hours(1),
        },
        &clock,
    )
    .await
    .expect("pending runner");

    let enroll_body = json!({
        "protocol_version": 1,
        "enrollment_token": raw_enrollment_token,
        "runner_name": "Lifecycle Runner",
        "runner_version": "0.1.0",
        "capabilities": full_capabilities(clock.now(), 1, 1),
    })
    .to_string();
    let (status, enrolled) = send(&app, "POST", "/enroll", enroll_body, &[]).await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    let runner_id = enrolled["runner_id"].as_str().unwrap().to_owned();
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();
    assert_ne!(credential, raw_enrollment_token);
    assert_eq!(enrolled["heartbeat_interval_seconds"], 15);
    assert_eq!(enrolled["lease_duration_seconds"], 60);
    let stored_hash: String =
        sqlx::query_scalar("SELECT credential_hash FROM agent_runners WHERE id=?")
            .bind(&runner_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_ne!(stored_hash, credential, "only the hash may be stored");

    let auth_header = [("authorization", format!("Bearer {credential}"))];
    let auth = |uri: &str, body: String| {
        let (name, value) = auth_header[0].clone();
        let app = app.clone();
        let uri = uri.to_string();
        async move { send(&app, "POST", &uri, body, &[(name, &value)]).await }
    };

    // Refresh (capability refresh) with rotation.
    let refresh_body = json!({
        "protocol_version": 1,
        "runner_id": runner_id,
        "runner_name": "Lifecycle Runner",
        "runner_version": "0.1.1",
        "rotate_credential": true,
        "capabilities": full_capabilities(clock.now(), 1, 1),
    })
    .to_string();
    let (status, refreshed) = auth("/refresh", refresh_body).await;
    assert_eq!(status, StatusCode::OK, "{refreshed}");
    let rotated_credential = refreshed["runner_credential"].as_str().unwrap().to_owned();
    assert_ne!(rotated_credential, credential);
    // The old credential no longer authenticates once rotated.
    let (old_status, _) = auth(
        "/refresh",
        json!({"protocol_version":1,"runner_id":runner_id,"runner_name":"x","runner_version":"x","rotate_credential":false,"capabilities":full_capabilities(clock.now(),1,1)}).to_string(),
    )
    .await;
    assert_eq!(old_status, StatusCode::UNAUTHORIZED);

    let auth_header = [("authorization", format!("Bearer {rotated_credential}"))];
    let auth = |uri: &str, body: String| {
        let (name, value) = auth_header[0].clone();
        let app = app.clone();
        let uri = uri.to_string();
        async move { send(&app, "POST", &uri, body, &[(name, &value)]).await }
    };

    let request_id = enqueue_request(
        &repo,
        &clock,
        &item_id,
        &runner_id,
        "profile-c2",
        "lifecycle-key",
    )
    .await;

    // Claim.
    let claim_body = json!({
        "protocol_version": 1, "runner_id": runner_id, "claim_request_id": "claim-lifecycle",
        "available_capacity": 1, "wait_ms": 1000,
    })
    .to_string();
    let (status, claimed) = auth("/claim", claim_body).await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    assert_eq!(claimed["request"]["request_id"], request_id);
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();
    assert_eq!(claimed["attempt"]["state"], "leased");

    // Accept (preparing), then an exact replay.
    let accept_body = json!({
        "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "workspace_id": "ws-1", "base_revision": "abc123def456abc123def456abc123def456abc",
    })
    .to_string();
    let (status, accepted) = auth(
        &format!("/attempts/{attempt_id}/accept"),
        accept_body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["state"], "preparing");
    assert_eq!(accepted["replayed"], false);
    let (status, replayed) = auth(&format!("/attempts/{attempt_id}/accept"), accept_body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed["replayed"], true);
    assert_eq!(replayed["committed_at"], accepted["committed_at"]);

    // Start (running).
    let start_body = json!({
        "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "workspace_id": "ws-1", "base_revision": "abc123def456abc123def456abc123def456abc", "process_id": "pid-123",
    })
    .to_string();
    let (status, started) = auth(&format!("/attempts/{attempt_id}/start"), start_body).await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(started["state"], "running");

    // Event batch.
    let events_body = json!({
        "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "previous_checkpoint": Value::Null, "checkpoint": "checkpoint-0001",
        "events": [{
            "event_id": "evt-1", "sequence": 1, "occurred_at": clock.now().to_rfc3339(),
            "source": "runner", "kind": "progress", "payload": {"phase": "testing"},
        }],
    })
    .to_string();
    let (status, batch) = auth(&format!("/attempts/{attempt_id}/events"), events_body).await;
    assert_eq!(status, StatusCode::OK, "{batch}");
    assert_eq!(batch["accepted_event_ids"], json!(["evt-1"]));
    assert_eq!(batch["committed_checkpoint"], "checkpoint-0001");
    let event_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id=?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(event_rows, 1);

    // Decision create then poll: pending, then simulate resolution and poll again.
    let decision_body = json!({
        "protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "decision_id": "dec-1", "kind": "tool_permission", "prompt": "Allow the harness to run tests?",
        "options": [{"option_id":"allow_once","label":"Allow once"},{"option_id":"deny","label":"Deny"}],
        "expires_at": Value::Null, "metadata": {"tool": "cargo test"},
    })
    .to_string();
    let (status, decision) =
        auth(&format!("/attempts/{attempt_id}/decisions"), decision_body).await;
    assert_eq!(status, StatusCode::OK, "{decision}");
    assert_eq!(decision["state"], "pending");
    let created_at = decision["created_at"].as_str().unwrap().to_owned();

    let poll_body = |after: &str| {
        json!({"protocol_version":1,"runner_id":runner_id,"attempt_id":attempt_id,"fencing_token":fencing_token,"after":after}).to_string()
    };
    let (status, first_poll) = auth(
        &format!("/attempts/{attempt_id}/decisions/poll"),
        poll_body(&request_created_before(&clock)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first_poll}");
    assert_eq!(first_poll["decisions"][0]["decision_id"], "dec-1");
    assert_eq!(first_poll["decisions"][0]["state"], "pending");
    let next_after = first_poll["next_after"].as_str().unwrap().to_owned();
    assert_eq!(next_after, created_at);

    clock.advance(Duration::seconds(30));
    sqlx::query("UPDATE execution_decisions SET state='resolved', answer=?, resolved_at=?, resolved_by=?, updated_at=? WHERE attempt_id=? AND decision_id='dec-1'")
        .bind(json!({"option_id":"allow_once","text":Value::Null}).to_string())
        .bind(clock.now().to_rfc3339())
        .bind(json!({"kind":"operator","subject_id":"local-admin"}).to_string())
        .bind(clock.now().to_rfc3339())
        .bind(&attempt_id)
        .execute(repo.pool())
        .await
        .unwrap();
    let (status, second_poll) = auth(
        &format!("/attempts/{attempt_id}/decisions/poll"),
        poll_body(&next_after),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second_poll}");
    assert_eq!(second_poll["decisions"][0]["state"], "resolved");
    assert_eq!(
        second_poll["decisions"][0]["answer"]["option_id"],
        "allow_once"
    );
    assert_ne!(second_poll["next_after"].as_str().unwrap(), next_after);

    // Artifact manifest.
    let sha256 = "f".repeat(64);
    let (status, artifacts) = auth(
        &format!("/attempts/{attempt_id}/artifacts"),
        artifact_body(&runner_id, &attempt_id, fencing_token, "art-1", &sha256),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{artifacts}");
    assert_eq!(artifacts["artifacts"][0]["state"], "manifest_accepted");
    assert_eq!(artifacts["artifacts"][0]["upload"]["method"], "PUT");

    // Completion, then an exact replay.
    let started_at = clock.now() - Duration::minutes(5);
    let completion_body_str = completion_body(
        &runner_id,
        &attempt_id,
        fencing_token,
        "complete-1",
        started_at,
        clock.now(),
        Some("checkpoint-0001"),
    );
    let (status, completed) = auth(
        &format!("/attempts/{attempt_id}/completion"),
        completion_body_str.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["state"], "succeeded");
    assert_eq!(completed["replayed"], false);
    let (status, replayed_completion) = auth(
        &format!("/attempts/{attempt_id}/completion"),
        completion_body_str,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed_completion["replayed"], true);
    assert_eq!(
        replayed_completion["committed_at"],
        completed["committed_at"]
    );

    let request_state: String =
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE id=?")
            .bind(&request_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(request_state, "succeeded");
    let available_capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id=?")
            .bind(&runner_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(available_capacity, 1, "capacity is restored exactly once");
}

fn request_created_before(clock: &FakeClock) -> String {
    (clock.now() - Duration::hours(1)).to_rfc3339()
}

// ---------------------------------------------------------------------
// Two default-configured runners self-report the identical
// `runner_name` (both default it from `TACK_RUNNER_ID`). Without the
// `_runner_name`/removed `name=?` special-casing in
// `crates/tack-db/src/repo/execution.rs::redeem_enrollment_token`, this
// would silently overwrite the operator-assigned, uniquely-named
// pending-runner row and crash the second enrollment on
// `agent_runners`'s `UNIQUE` `name` constraint (two curl enrollments
// differing only in token, first 200, second 500). Load-bearing:
// reverting that change makes this test fail with a 500 on the second
// enrollment.
// ---------------------------------------------------------------------

#[tokio::test]
async fn duplicate_self_reported_runner_name_enrolls_both_runners() {
    let (app, repo, clock, _item_id) = setup().await;

    // The operator assigns each pending runner a distinct, unique name —
    // exactly as `create_pending_runner` requires (its own INSERT is
    // `UNIQUE`-constrained on `name`).
    const RAW_TOKEN_A: &str = "iii-h7-token-a";
    const RAW_TOKEN_B: &str = "iii-h7-token-b";
    let token_hash_a = runner_protocol::runner_auth::credential_hash(RAW_TOKEN_A);
    let token_hash_b = runner_protocol::runner_auth::credential_hash(RAW_TOKEN_B);
    repo.create_pending_runner_and_issue_token(
        NewRunner {
            id: "runner-h7-a",
            name: "operator-assigned-a",
            credential_hash: "pending:no-credential",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        EnrollmentToken {
            id: "tok-h7-a",
            runner_id: "runner-h7-a",
            token_hash: &token_hash_a,
            expires_at: clock.now() + Duration::hours(1),
        },
        &clock,
    )
    .await
    .expect("pending runner a");
    repo.create_pending_runner_and_issue_token(
        NewRunner {
            id: "runner-h7-b",
            name: "operator-assigned-b",
            credential_hash: "pending:no-credential",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        EnrollmentToken {
            id: "tok-h7-b",
            runner_id: "runner-h7-b",
            token_hash: &token_hash_b,
            expires_at: clock.now() + Duration::hours(1),
        },
        &clock,
    )
    .await
    .expect("pending runner b");

    // Both enroll bodies differ only in `enrollment_token` and self-report
    // the identical `runner_name`, the
    // way two default-configured runners on one host would (both defaulting
    // it from `TACK_RUNNER_ID`).
    let enroll_body = |token: &str| {
        json!({
            "protocol_version": 1,
            "enrollment_token": token,
            "runner_name": "default-runner-id",
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(clock.now(), 1, 1),
        })
        .to_string()
    };

    let (status_a, enrolled_a) = send(&app, "POST", "/enroll", enroll_body(RAW_TOKEN_A), &[]).await;
    assert_eq!(status_a, StatusCode::OK, "{enrolled_a}");
    let runner_id_a = enrolled_a["runner_id"].as_str().unwrap().to_owned();

    let (status_b, enrolled_b) = send(&app, "POST", "/enroll", enroll_body(RAW_TOKEN_B), &[]).await;
    assert_eq!(
        status_b,
        StatusCode::OK,
        "second same-self-reported-name enrollment must not 500: {enrolled_b}"
    );
    let runner_id_b = enrolled_b["runner_id"].as_str().unwrap().to_owned();
    assert_ne!(runner_id_a, runner_id_b);

    // The operator-assigned names are untouched by the self-reported value —
    // it is accepted for protocol-shape validation only, never persisted
    // over the operator's assignment.
    let stored_name_a: String = sqlx::query_scalar("SELECT name FROM agent_runners WHERE id=?")
        .bind(&runner_id_a)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    let stored_name_b: String = sqlx::query_scalar("SELECT name FROM agent_runners WHERE id=?")
        .bind(&runner_id_b)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(stored_name_a, "operator-assigned-a");
    assert_eq!(stored_name_b, "operator-assigned-b");
}

// ---------------------------------------------------------------------
// 2. Operator auth cannot substitute for runner auth, and vice versa. A
//    runner therefore cannot reach any PM-mutating (item/execution/runner
//    admin) route — it can only reach its own report-only endpoints.
// ---------------------------------------------------------------------

#[tokio::test]
async fn operator_auth_cannot_substitute_for_runner_auth_and_vice_versa() {
    let (runner_app, repo, clock, item_id) = setup().await;

    // An operator-style principal header alone does not authenticate a
    // runner route: the runner-auth module reads only `Authorization`.
    let (status, body) = send(
        &runner_app,
        "POST",
        "/heartbeat",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"heartbeat_id":"hb-x","sent_at":clock.now().to_rfc3339(),"available_capacity":2,"active_attempts":[]}).to_string(),
        &[("x-tack-principal", "operator-1")],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");

    // A valid runner bearer credential alone does not authenticate any
    // operator route: `principal()` in `executions.rs` reads only
    // `x-tack-principal`. This is the structural proof that a runner cannot
    // create, cancel, or otherwise mutate a PM execution request.
    let operator_state = executions::OperatorExecutionState::with_clock(
        repo.clone(),
        Arc::new(clock.clone()),
        false,
    );
    let operator_app = executions::routes(operator_state);
    let runner_bearer_value = format!("Bearer {RUNNER_CREDENTIAL}");
    let runner_bearer: [(&str, &str); 1] = [("authorization", &runner_bearer_value)];

    let create_body = json!({
        "item_id": item_id, "idempotency_key": "attempt-by-runner", "selector_kind": "exact_runner",
        "selector_id": RUNNER_ID, "agent_profile_id": "profile-c2", "requested_harness_kind": "codex",
        "agent_profile_snapshot": {"name":"C2 Profile","instructions":"work safely","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{"tokens":1000}},
        "repository_snapshot": {"kind":"git","remote":"https://example.test/c2.git","base_revision":"abc123","subdirectory":null},
        "permission_policy": {"tools":["shell"],"network":false}, "timeout_seconds":60, "budgets":{"tokens":1000},
        "environment": {}, "metadata": {},
    })
    .to_string();
    let (status, _) = send(
        &operator_app,
        "POST",
        "/executions",
        create_body,
        &runner_bearer,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a runner credential must not create a PM execution request"
    );

    // `requeue_needs_operator` is another principal-scoped mutation (audited
    // recovery of a `needs_operator` request); same proof, different route.
    let (status, _) = send(
        &operator_app,
        "POST",
        "/executions/does-not-exist/requeue",
        json!({"recovery_key": "k", "reason": "r"}).to_string(),
        &runner_bearer,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a runner credential must not perform an audited operator requeue either"
    );

    // runner-admin's simpler mutations (e.g. `revoke_runner`) do not
    // themselves check `x-tack-principal` — that router leaves
    // top-level bearer-token gating to the global `require_token` middleware,
    // so it is not a meaningful non-substitution proof point in
    // isolation the way the two principal-scoped routes above are.
}

// ---------------------------------------------------------------------
// 3. Stale/expired fence writes nothing and returns `stale_lease`.
// ---------------------------------------------------------------------

#[tokio::test]
async fn stale_and_expired_fence_write_nothing() {
    let (app, repo, clock, item_id) = setup().await;
    enqueue_request(
        &repo,
        &clock,
        &item_id,
        RUNNER_ID,
        "profile-c2",
        "stale-key",
    )
    .await;
    let (_, claimed) = send_as_runner(
        &app,
        "POST",
        "/claim",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"claim_request_id":"claim-stale","available_capacity":2,"wait_ms":1000}).to_string(),
    )
    .await;
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    // Wrong fencing token on a real attempt.
    let (status, body) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/events"),
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"attempt_id":attempt_id,"fencing_token":fencing_token+1,"checkpoint":"cp-1","previous_checkpoint":Value::Null,"events":[]}).to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "stale_lease");
    let event_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id=?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(event_rows, 0, "a stale-fence write must write nothing");

    // Expired lease with the *correct* fencing token: never heartbeat, just
    // advance the fake clock past `lease_duration_seconds` (60s).
    clock.advance(Duration::seconds(61));
    let (status, body) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/events"),
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"attempt_id":attempt_id,"fencing_token":fencing_token,"checkpoint":"cp-1","previous_checkpoint":Value::Null,"events":[]}).to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "stale_lease");
    let event_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id=?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(event_rows, 0, "an expired lease must write nothing either");

    let checkpoint: Option<String> =
        sqlx::query_scalar("SELECT event_checkpoint FROM execution_attempts WHERE id=?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(checkpoint, None, "the attempt row itself is untouched");
}

// ---------------------------------------------------------------------
// 4. Idempotent replay returns the original success; a same-key replay with
//    different content is a stable, distinct conflict code.
// ---------------------------------------------------------------------

#[tokio::test]
async fn heartbeat_and_completion_idempotent_replay_and_conflicting_replay_are_distinguished() {
    let (app, repo, clock, item_id) = setup().await;
    enqueue_request(
        &repo,
        &clock,
        &item_id,
        RUNNER_ID,
        "profile-c2",
        "replay-key",
    )
    .await;
    let (_, claimed) = send_as_runner(
        &app,
        "POST",
        "/claim",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"claim_request_id":"claim-replay","available_capacity":2,"wait_ms":1000}).to_string(),
    )
    .await;
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    let heartbeat_body = |capacity: i64| {
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"heartbeat_id":"hb-replay","sent_at":clock.now().to_rfc3339(),"available_capacity":capacity,"active_attempts":[]}).to_string()
    };
    let (status, first) = send_as_runner(&app, "POST", "/heartbeat", heartbeat_body(1)).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let (status, replay) = send_as_runner(&app, "POST", "/heartbeat", heartbeat_body(1)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        replay["accepted_at"], first["accepted_at"],
        "exact replay returns the original success"
    );
    let (status, conflicting) = send_as_runner(&app, "POST", "/heartbeat", heartbeat_body(2)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflicting["error"]["code"], "idempotency_conflict");

    // Completion: exact replay succeeds; a same-id, different-content retry
    // is rejected and writes nothing new.
    let started_at = clock.now() - Duration::minutes(1);
    let completion = completion_body(
        RUNNER_ID,
        &attempt_id,
        fencing_token,
        "complete-replay",
        started_at,
        clock.now(),
        None,
    );
    let (status, committed) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/completion"),
        completion.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{committed}");
    let changed = completion.replace("succeeded", "failed");
    let (status, _conflict) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/completion"),
        changed,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let replay_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_completion_replays WHERE attempt_id=?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(
        replay_count, 1,
        "the conflicting retry wrote no second replay record"
    );
    let state: String = sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id=?")
        .bind(&attempt_id)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(
        state, "succeeded",
        "the original committed outcome is unchanged"
    );
}

// ---------------------------------------------------------------------
// 5. Oversized event batch writes nothing at all.
// ---------------------------------------------------------------------

#[tokio::test]
async fn oversized_event_batch_writes_nothing() {
    let (app, repo, clock, item_id) = setup().await;
    enqueue_request(
        &repo,
        &clock,
        &item_id,
        RUNNER_ID,
        "profile-c2",
        "oversized-key",
    )
    .await;
    let (_, claimed) = send_as_runner(
        &app,
        "POST",
        "/claim",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"claim_request_id":"claim-oversized","available_capacity":2,"wait_ms":1000}).to_string(),
    )
    .await;
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    // Over `event_batch_count_max` (100): 101 tiny events.
    let too_many_events: Vec<Value> = (0..101)
        .map(|i| json!({"event_id": format!("evt-{i}"), "sequence": i, "occurred_at": clock.now().to_rfc3339(), "source":"runner","kind":"progress","payload":{}}))
        .collect();
    let (status, body) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/events"),
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"attempt_id":attempt_id,"fencing_token":fencing_token,"checkpoint":"cp-1","previous_checkpoint":Value::Null,"events":too_many_events}).to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body["error"]["code"], "payload_too_large");
    assert_eq!(body["error"]["details"]["limit"], "event_batch_count_max");
    let event_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id=?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(event_rows, 0);
    let checkpoint: Option<String> =
        sqlx::query_scalar("SELECT event_checkpoint FROM execution_attempts WHERE id=?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(checkpoint, None);

    // Over the whole-body byte cap (`json_body_bytes_max` == `event_batch_bytes_max`,
    // both 1 MiB): one event with an oversized payload.
    let huge_payload = json!({"blob": "x".repeat(2 * 1_048_576)});
    let (status, body) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/events"),
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"attempt_id":attempt_id,"fencing_token":fencing_token,"checkpoint":"cp-1","previous_checkpoint":Value::Null,"events":[{"event_id":"evt-huge","sequence":1,"occurred_at":clock.now().to_rfc3339(),"source":"runner","kind":"progress","payload":huge_payload}]}).to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body["error"]["code"], "payload_too_large");
    let event_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id=?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(
        event_rows, 0,
        "an oversized batch writes nothing, not just a 413"
    );
}

// ---------------------------------------------------------------------
// 6. Reusing a decision_id/artifact_id with different content is an
//    idempotency conflict (a compensating check for the
//    `ON CONFLICT DO NOTHING` inserts).
// ---------------------------------------------------------------------

#[tokio::test]
async fn decision_and_artifact_id_reuse_with_different_content_is_idempotency_conflict() {
    let (app, repo, clock, item_id) = setup().await;
    enqueue_request(
        &repo,
        &clock,
        &item_id,
        RUNNER_ID,
        "profile-c2",
        "reuse-key",
    )
    .await;
    let (_, claimed) = send_as_runner(
        &app,
        "POST",
        "/claim",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"claim_request_id":"claim-reuse","available_capacity":2,"wait_ms":1000}).to_string(),
    )
    .await;
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();
    send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/accept"),
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"attempt_id":attempt_id,"fencing_token":fencing_token,"workspace_id":"ws-1","base_revision":"abc123def456abc123def456abc123def456abc"}).to_string(),
    ).await;
    send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/start"),
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"attempt_id":attempt_id,"fencing_token":fencing_token,"workspace_id":"ws-1","base_revision":"abc123def456abc123def456abc123def456abc","process_id":"pid-1"}).to_string(),
    ).await;

    let decision_body = |prompt: &str| {
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"attempt_id":attempt_id,"fencing_token":fencing_token,"decision_id":"dec-reuse","kind":"tool_permission","prompt":prompt,"options":[{"option_id":"allow","label":"Allow"}],"expires_at":Value::Null,"metadata":{}}).to_string()
    };
    let (status, first) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/decisions"),
        decision_body("Allow A?"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let (status, exact_replay) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/decisions"),
        decision_body("Allow A?"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "an exact replay is not a conflict");
    assert_eq!(exact_replay["created_at"], first["created_at"]);
    let (status, conflict) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/decisions"),
        decision_body("Allow B?"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"]["code"], "idempotency_conflict");
    let stored_prompt: String = sqlx::query_scalar(
        "SELECT prompt FROM execution_decisions WHERE attempt_id=? AND decision_id='dec-reuse'",
    )
    .bind(&attempt_id)
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(
        stored_prompt, "Allow A?",
        "the conflicting retry did not overwrite the original"
    );

    let sha_a = "a".repeat(64);
    let sha_b = "b".repeat(64);
    let (status, _) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/artifacts"),
        artifact_body(RUNNER_ID, &attempt_id, fencing_token, "art-reuse", &sha_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, conflict) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/artifacts"),
        artifact_body(RUNNER_ID, &attempt_id, fencing_token, "art-reuse", &sha_b),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"]["code"], "idempotency_conflict");
    let stored_sha: String = sqlx::query_scalar(
        "SELECT sha256 FROM execution_artifacts WHERE attempt_id=? AND artifact_id='art-reuse'",
    )
    .bind(&attempt_id)
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(stored_sha, sha_a);
}

// ---------------------------------------------------------------------
// 7. Recovery observation: a proven pre-spawn-stopped observation safely
//    requeues the request, and replays idempotently.
// ---------------------------------------------------------------------

#[tokio::test]
async fn recovery_observation_safe_requeue_requeues_the_request_and_replays_idempotently() {
    let (app, repo, clock, item_id) = setup().await;
    enqueue_request(
        &repo,
        &clock,
        &item_id,
        RUNNER_ID,
        "profile-c2",
        "recovery-key",
    )
    .await;
    let (_, claimed) = send_as_runner(
        &app,
        "POST",
        "/claim",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"claim_request_id":"claim-recovery","available_capacity":2,"wait_ms":1000}).to_string(),
    )
    .await;
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();
    let request_id = claimed["request"]["request_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let recovery_body = json!({
        "protocol_version": 1, "runner_id": RUNNER_ID, "attempt_id": attempt_id, "fencing_token": fencing_token,
        "recovery_key": format!("recovery:{attempt_id}:{fencing_token}:process_stopped"),
        "observation": "process_stopped",
        "details": {"journal_state": "prepared", "process_observed": false},
    })
    .to_string();
    let (status, applied) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/recovery-observation"),
        recovery_body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    assert_eq!(applied["disposition"], "safe_pre_spawn_requeue");
    assert_eq!(applied["replayed"], false);

    let attempt_state: String =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id=?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(attempt_state, "lost");
    let request_state: String =
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE id=?")
            .bind(&request_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(request_state, "queued");

    let (status, replayed) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/recovery-observation"),
        recovery_body,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replayed["replayed"], true);
    assert_eq!(replayed["committed_at"], applied["committed_at"]);
}

// ---------------------------------------------------------------------
// 8. Logs carry ids only.
//
// This is the only test in this module that captures `tracing` output.
// See `log_capture.rs` for the process-global subscriber this relies on,
// and why a global default (not a thread-local `set_default`) is what
// actually makes capture reliable when many tests share this binary.
// ---------------------------------------------------------------------

#[tokio::test]
async fn logs_never_contain_raw_credentials_only_ids() {
    let (app, repo, clock, _item_id) = setup().await;
    let raw_enrollment_token = "example_super_secret_enrollment_token_value";
    let token_hash = runner_protocol::runner_auth::credential_hash(raw_enrollment_token);
    repo.create_pending_runner_and_issue_token(
        NewRunner {
            id: "runner-log",
            name: "Log Runner",
            credential_hash: "pending:no-credential",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        EnrollmentToken {
            id: "tok-log",
            runner_id: "runner-log",
            token_hash: &token_hash,
            expires_at: clock.now() + Duration::hours(1),
        },
        &clock,
    )
    .await
    .expect("pending runner");

    let (guard, captured) = CaptureGuard::start();

    let (status, enrolled) = send(
        &app,
        "POST",
        "/enroll",
        json!({"protocol_version":1,"enrollment_token":raw_enrollment_token,"runner_name":"Log Runner","runner_version":"0.1.0","capabilities":full_capabilities(clock.now(),1,1)}).to_string(),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    let issued_runner_id = enrolled["runner_id"].as_str().unwrap().to_owned();
    let issued_credential = enrolled["runner_credential"].as_str().unwrap().to_owned();

    let bogus_credential = "bogus-bearer-credential-value-should-never-log";
    let (status, _) = send(
        &app,
        "POST",
        "/refresh",
        json!({"protocol_version":1,"runner_id":issued_runner_id,"runner_name":"x","runner_version":"x","rotate_credential":false,"capabilities":full_capabilities(clock.now(),1,1)}).to_string(),
        &[("authorization", &format!("Bearer {bogus_credential}"))],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    drop(guard);
    let log_text = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert!(
        log_text.contains(&issued_runner_id),
        "runner_id should be logged for observability: {log_text}"
    );
    assert!(
        !log_text.contains(raw_enrollment_token),
        "raw enrollment token leaked into logs: {log_text}"
    );
    assert!(
        !log_text.contains(&issued_credential),
        "raw runner credential leaked into logs: {log_text}"
    );
    assert!(
        !log_text.contains(bogus_credential),
        "raw bearer credential leaked into logs: {log_text}"
    );
}

// ---------------------------------------------------------------------
// 9. A retryable stable code (`conflict`) carries `retryable: true` in the
//    real, serialized JSON body — sourced from
//    the single retryability authority (`StableErrorCode::retryable`,
//    `crates/tack-orch/src/execution/types.rs`), not a locally re-derived
//    classification. An event batch whose `previous_checkpoint` no longer
//    matches the attempt's committed stream position hits
//    `EventApplyResult::Conflict` — the benign, retryable, out-of-order-resync
//    case split out of the old,
//    collapsed `ReplayConflict`.
//    This test drives *only* that benign path and asserts `retryable: true`;
//    its sibling
//    `event_batch_replay_changed_content_is_idempotency_conflict_and_writes_nothing`
//    below drives the other split cause — the same `(attempt_id,
//    checkpoint)` key reused with genuinely different content — and asserts
//    the opposite, non-retryable `idempotency_conflict`, making the
//    contrast between the two explicit rather than leaving it untested (the
//    gap that let the original collapse of the two causes survive
//    undetected at the HTTP layer).
// ---------------------------------------------------------------------

#[tokio::test]
async fn event_checkpoint_conflict_response_carries_contract_correct_retryable_true() {
    let (app, repo, clock, item_id) = setup().await;
    enqueue_request(
        &repo,
        &clock,
        &item_id,
        RUNNER_ID,
        "profile-c2",
        "conflict-retryable-key",
    )
    .await;
    let (_, claimed) = send_as_runner(
        &app,
        "POST",
        "/claim",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"claim_request_id":"claim-conflict-retryable","available_capacity":2,"wait_ms":1000}).to_string(),
    )
    .await;
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    // First batch commits checkpoint "cp-1" (previous_checkpoint absent, as
    // this is the first batch for the attempt).
    let (status, first) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/events"),
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"attempt_id":attempt_id,"fencing_token":fencing_token,"checkpoint":"cp-1","previous_checkpoint":Value::Null,"events":[{"event_id":"evt-1","sequence":1,"occurred_at":clock.now().to_rfc3339(),"source":"runner","kind":"progress","payload":{}}]}).to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");

    // A second batch that claims a stale `previous_checkpoint` (rather than
    // the now-committed "cp-1") no longer matches the attempt's stream
    // position — a benign, retryable resync, not a same-key idempotency
    // conflict.
    let (status, body) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/events"),
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"attempt_id":attempt_id,"fencing_token":fencing_token,"checkpoint":"cp-2","previous_checkpoint":"stale-checkpoint","events":[{"event_id":"evt-2","sequence":2,"occurred_at":clock.now().to_rfc3339(),"source":"runner","kind":"progress","payload":{}}]}).to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "conflict");
    assert_eq!(
        body["error"]["retryable"], true,
        "a `conflict` response must be retryable per StableErrorCode::retryable \
         (docs/contracts/runner-v1/errors/conflict.json): {body}"
    );
    assert_eq!(body["error"]["request_id"], "req_runner");

    // And it wrote nothing: the second event never landed, and the
    // committed checkpoint is still "cp-1" from the first, accepted batch.
    let event_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id=?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(event_rows, 1, "the rejected second batch wrote no events");
    let checkpoint: Option<String> =
        sqlx::query_scalar("SELECT event_checkpoint FROM execution_attempts WHERE id=?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(checkpoint.as_deref(), Some("cp-1"));
}

// ---------------------------------------------------------------------
// 10. The other half of the split: reusing the same idempotency-scoped
//     `(attempt_id, checkpoint)` key with genuinely different event content
//     is `idempotency_conflict` (`retryable: false`), never `conflict`. This
//     exact HTTP-layer path had no test before this — exactly how a bug
//     could survive undetected: the collapsed `ReplayConflict` was mapped
//     unconditionally to the retryable `conflict` code, so a runner reusing
//     a checkpoint with changed content was told to retry forever.
// ---------------------------------------------------------------------

#[tokio::test]
async fn event_batch_replay_changed_content_is_idempotency_conflict_and_writes_nothing() {
    let (app, repo, clock, item_id) = setup().await;
    enqueue_request(
        &repo,
        &clock,
        &item_id,
        RUNNER_ID,
        "profile-c2",
        "event-idempotency-key",
    )
    .await;
    let (_, claimed) = send_as_runner(
        &app,
        "POST",
        "/claim",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"claim_request_id":"claim-event-idem","available_capacity":2,"wait_ms":1000}).to_string(),
    )
    .await;
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    // First batch commits checkpoint "cp-1" with one event.
    let (status, first) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/events"),
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"attempt_id":attempt_id,"fencing_token":fencing_token,"checkpoint":"cp-1","previous_checkpoint":Value::Null,"events":[{"event_id":"evt-1","sequence":1,"occurred_at":clock.now().to_rfc3339(),"source":"runner","kind":"progress","payload":{"note":"original"}}]}).to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");

    // Reusing the exact same checkpoint "cp-1" (the idempotency-scoped key)
    // with the same `previous_checkpoint` but different event payload
    // content can never succeed by retrying — this is a genuine content
    // change, not an out-of-order resync.
    let (status, body) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/events"),
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"attempt_id":attempt_id,"fencing_token":fencing_token,"checkpoint":"cp-1","previous_checkpoint":Value::Null,"events":[{"event_id":"evt-1","sequence":1,"occurred_at":clock.now().to_rfc3339(),"source":"runner","kind":"progress","payload":{"note":"CHANGED"}}]}).to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "idempotency_conflict");
    assert_eq!(
        body["error"]["retryable"], false,
        "idempotency_conflict must be non-retryable per \
         docs/contracts/runner-v1/errors/idempotency-conflict.json: {body}"
    );

    // Nothing new was written: still exactly the one event from the first,
    // accepted batch, and the checkpoint is unchanged.
    let event_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id=?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(event_rows, 1, "the rejected replay wrote no second event");
    let stored_payload: String = sqlx::query_scalar(
        "SELECT payload FROM execution_events WHERE attempt_id=? AND event_id='evt-1'",
    )
    .bind(&attempt_id)
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert!(
        stored_payload.contains("original"),
        "the original event content is unchanged: {stored_payload}"
    );
}

// ---------------------------------------------------------------------
// 11. The completion analogue of test 10: reusing the same
//     idempotency-scoped `(attempt_id, completion_id)` key with genuinely
//     different terminal content is `idempotency_conflict`
//     (`retryable: false`), distinct from the benign, retryable `conflict`
//     a distinct completion_id racing a concurrent terminal write would
//     produce.
// ---------------------------------------------------------------------

#[tokio::test]
async fn completion_replay_changed_content_is_idempotency_conflict_and_writes_nothing() {
    let (app, repo, clock, item_id) = setup().await;
    enqueue_request(
        &repo,
        &clock,
        &item_id,
        RUNNER_ID,
        "profile-c2",
        "completion-idempotency-key",
    )
    .await;
    let (_, claimed) = send_as_runner(
        &app,
        "POST",
        "/claim",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"claim_request_id":"claim-completion-idem","available_capacity":2,"wait_ms":1000}).to_string(),
    )
    .await;
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    let started_at = clock.now() - Duration::minutes(1);
    let completion = completion_body(
        RUNNER_ID,
        &attempt_id,
        fencing_token,
        "completion-idem",
        started_at,
        clock.now(),
        None,
    );
    let (status, committed) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/completion"),
        completion.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{committed}");

    // Same `completion_id`, but a changed `terminal_state`: reusing the key
    // with different content.
    let changed = completion.replace("succeeded", "failed");
    let (status, body) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_id}/completion"),
        changed,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "idempotency_conflict");
    assert_eq!(
        body["error"]["retryable"], false,
        "idempotency_conflict must be non-retryable per \
         docs/contracts/runner-v1/errors/idempotency-conflict.json: {body}"
    );

    let replay_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_completion_replays WHERE attempt_id=?")
            .bind(&attempt_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(
        replay_count, 1,
        "the conflicting retry wrote no second replay record"
    );
    let state: String = sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id=?")
        .bind(&attempt_id)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(
        state, "succeeded",
        "the original committed outcome is unchanged"
    );
}

// ---------------------------------------------------------------------
// 12. Credential rotation uses a compare-and-set
//     `Repository::rotate_runner_credential`. A rotation whose
//     `expected_credential_hash` no longer matches the runner's current
//     credential (because a prior rotation already won) is rejected as a
//     retryable `conflict`, not silently applied last-writer-wins.
// ---------------------------------------------------------------------

#[tokio::test]
async fn refresh_rotation_with_stale_expected_hash_is_rejected_not_overwritten() {
    let (app, repo, clock, _item_id) = setup().await;

    // Two concurrent `/refresh` rotations, both still authenticated against
    // the same currently-valid `RUNNER_CREDENTIAL` bearer — the realistic
    // shape of the race this test proves is fixed: a retried or duplicated
    // rotation request. Each generates its own new raw credential
    // internally; only one write can win the CAS.
    //
    // Relying on bare `tokio::join!` poll order to prove the two requests'
    // underlying SQL writes actually overlap in time is unreliable — proven
    // so directly: an earlier version of this test using exactly that (and
    // a follow-up using only cooperative `yield_now` to sequence a held
    // transaction's release) both still failed intermittently, always in
    // the same way: one request's `authenticate` ran only *after* the
    // other's entire rotation had already committed, so it saw an
    // already-rotated hash and failed with a plain `unauthorized`, never
    // reaching the CAS at all — no race occurred. The fix: fully await
    // opening a manual `BEGIN IMMEDIATE` no-op write
    // against the runner row *before* either rotation request is even
    // constructed, `tokio::spawn` both rotation requests as independently
    // scheduled tasks (not nested inside one `join!`), and give the runtime
    // real wall-clock time (an actual `sleep`, not merely cooperative
    // yields) to drive both spawned tasks onto their own blocked DB read —
    // sqlx runs each SQLite connection's blocking calls on its own
    // dedicated OS thread, so a `sleep` on the test's own task genuinely
    // lets those threads reach and block on the held lock — before this
    // releases it. Because this test's `setup()` pool is `sqlite::memory:`
    // (shared-cache), a plain `SELECT` — including the first thing either
    // rotation does, `authenticate`'s credential lookup — blocks behind any
    // pending write to the same table, so both spawned tasks are guaranteed
    // to be genuinely blocked, not merely unpolled, when the hold releases.
    let mut holder = repo
        .pool()
        .begin_with("BEGIN IMMEDIATE")
        .await
        .expect("hold the runner row");
    sqlx::query("UPDATE agent_runners SET updated_at=updated_at WHERE id=?")
        .bind(RUNNER_ID)
        .execute(&mut *holder)
        .await
        .expect("no-op hold write");

    let rotate_body = || {
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"runner_name":"r","runner_version":"1","rotate_credential":true,"capabilities":full_capabilities(clock.now(),2,2)}).to_string()
    };
    let app_a = app.clone();
    let body_a = rotate_body();
    let task_a =
        tokio::spawn(async move { send_as_runner(&app_a, "POST", "/refresh", body_a).await });
    let app_b = app.clone();
    let body_b = rotate_body();
    let task_b =
        tokio::spawn(async move { send_as_runner(&app_b, "POST", "/refresh", body_b).await });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    holder.commit().await.expect("release the hold");

    let a = task_a.await.expect("rotation task a did not panic");
    let b = task_b.await.expect("rotation task b did not panic");
    let results = [a, b];
    let ok: Vec<_> = results
        .iter()
        .filter(|(status, _)| *status == StatusCode::OK)
        .collect();
    let conflicts: Vec<_> = results
        .iter()
        .filter(|(status, _)| *status == StatusCode::CONFLICT)
        .collect();
    assert_eq!(
        ok.len(),
        1,
        "exactly one concurrent rotation must win: {results:?}"
    );
    assert_eq!(
        conflicts.len(),
        1,
        "the loser must be rejected, not silently overwritten: {results:?}"
    );
    let (_, conflict_body) = conflicts[0];
    assert_eq!(conflict_body["error"]["code"], "conflict");
    assert_eq!(
        conflict_body["error"]["retryable"], true,
        "HashMismatch maps to the retryable `conflict` code, matching \
         docs/contracts/runner-v1/errors/conflict.json: {conflict_body}"
    );

    // The stored credential is exactly the winner's, never the loser's
    // (which was never persisted at all — a rejected rotation returns no
    // `runner_credential` for a caller to mistakenly treat as live).
    let (_, winner_body) = ok[0];
    let winner_credential = winner_body["runner_credential"].as_str().unwrap();
    let stored_hash: String =
        sqlx::query_scalar("SELECT credential_hash FROM agent_runners WHERE id=?")
            .bind(RUNNER_ID)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(
        stored_hash,
        runner_protocol::runner_auth::credential_hash(winner_credential),
        "the stored credential is exactly the winner's"
    );

    // The original bearer credential (captured by both requests' successful
    // `authenticate()` calls, before either write) is now stale either way —
    // proving the old credential was not left simultaneously valid alongside
    // the new one.
    let (old_status, _) = send_as_runner(
        &app,
        "POST",
        "/refresh",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"runner_name":"r","runner_version":"1","rotate_credential":false,"capabilities":full_capabilities(clock.now(),2,2)}).to_string(),
    )
    .await;
    assert_eq!(
        old_status,
        StatusCode::UNAUTHORIZED,
        "the pre-race credential no longer authenticates once either rotation committed"
    );
}

// ---------------------------------------------------------------------
// 12b. The same defect, reproduced deterministically instead of by
//     timing/luck. The test above (12) drives two rotations through a
//     genuinely concurrent SQLite lock race, which is realistic but, as its
//     own comment documents, cannot force a specific interleaving — locally
//     it lands on the CAS-level `HashMismatch` -> `conflict` outcome far more
//     often than the earlier, authenticate-level failure this test actually
//     targets, which is why CI's
//     more contended scheduler saw it and a local run of 32 iterations did
//     not.
//
//     The server has no way to distinguish "this stale credential belongs to
//     a request that lost a real concurrent race" from "this stale
//     credential is simply being reused after a rotation already committed"
//     — both hit the exact same code path: `authenticate`'s `SELECT ...
//     WHERE credential_hash=?` finds no row, because the only record of the
//     old hash was overwritten in place by the rotation UPDATE (see
//     `runner_auth::is_credential_not_recognized`'s doc comment). So this
//     test reproduces the identical defect without any lock or sleep, purely
//     sequentially: rotate once (the request that would have "won" a race),
//     then present the now-superseded original credential again with
//     `rotate_credential: true` (standing in for the request that would have
//     "lost" it) and assert the documented, retryable outcome.
// ---------------------------------------------------------------------

#[tokio::test]
async fn refresh_rotation_with_already_superseded_credential_returns_conflict_not_unauthorized() {
    let (app, repo, clock, _item_id) = setup().await;

    // The winner: rotates first and commits, exactly like task A above.
    let (winner_status, winner_body) = send_as_runner(
        &app,
        "POST",
        "/refresh",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"runner_name":"r","runner_version":"1","rotate_credential":true,"capabilities":full_capabilities(clock.now(),2,2)}).to_string(),
    )
    .await;
    assert_eq!(winner_status, StatusCode::OK, "the first rotation must win");
    let stored_hash_after_winner: String =
        sqlx::query_scalar("SELECT credential_hash FROM agent_runners WHERE id=?")
            .bind(RUNNER_ID)
            .fetch_one(repo.pool())
            .await
            .unwrap();

    // The loser: presents the pre-rotation `RUNNER_CREDENTIAL` (still what a
    // client would hold if its rotation request lost the race) and also asks
    // to rotate. `authenticate`'s hash lookup finds no row for it at all —
    // the exact failure `reclassify_refresh_auth_error` reclassifies.
    let (loser_status, loser_body) = send(
        &app,
        "POST",
        "/refresh",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"runner_name":"r","runner_version":"1","rotate_credential":true,"capabilities":full_capabilities(clock.now(),2,2)}).to_string(),
        &[("authorization", &format!("Bearer {RUNNER_CREDENTIAL}"))],
    )
    .await;

    assert_eq!(
        loser_status,
        StatusCode::CONFLICT,
        "a superseded credential attempting to rotate must be told it can retry, \
         not that it is permanently unauthorized: {loser_body}"
    );
    assert_eq!(loser_body["error"]["code"], "conflict");
    assert_eq!(
        loser_body["error"]["retryable"], true,
        "must match docs/contracts/runner-v1/errors/conflict.json: {loser_body}"
    );
    assert!(
        loser_body.get("runner_credential").is_none(),
        "a rejected rotation returns no credential for a caller to mistakenly treat as live"
    );

    // Writes nothing: the credential stored after the winner's rotation is
    // unchanged by the loser's rejected attempt.
    let stored_hash_after_loser: String =
        sqlx::query_scalar("SELECT credential_hash FROM agent_runners WHERE id=?")
            .bind(RUNNER_ID)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(
        stored_hash_after_winner, stored_hash_after_loser,
        "the rejected rotation must not have written anything"
    );
    assert_eq!(
        stored_hash_after_loser,
        runner_protocol::runner_auth::credential_hash(
            winner_body["runner_credential"].as_str().unwrap()
        ),
        "the stored credential remains exactly the winner's"
    );
}

// ---------------------------------------------------------------------
// 13. State-gate alignment. `submit_artifacts` must reject `lost`
//     and `needs_operator` attempts exactly like `create_decision` already
//     does — not just the three purely-terminal states. Both states exist
//     precisely to mean "stop trusting this runner's reports"; in both
//     cases here the lease itself has not expired (the
//     fake clock never advances), so a rejection can only come from the
//     state gate, not `stale_lease`.
// ---------------------------------------------------------------------

#[tokio::test]
async fn submit_artifacts_rejects_lost_and_needs_operator_states_and_writes_nothing() {
    let (app, repo, clock, item_id) = setup().await;

    // Attempt A: a proven pre-spawn-stopped recovery observation -> `lost`.
    enqueue_request(&repo, &clock, &item_id, RUNNER_ID, "profile-c2", "lost-key").await;
    let (_, claimed_a) = send_as_runner(
        &app,
        "POST",
        "/claim",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"claim_request_id":"claim-lost","available_capacity":2,"wait_ms":1000}).to_string(),
    )
    .await;
    let attempt_a = claimed_a["lease"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fence_a = claimed_a["lease"]["fencing_token"].as_i64().unwrap();
    let (status, recovered_a) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_a}/recovery-observation"),
        json!({
            "protocol_version": 1, "runner_id": RUNNER_ID, "attempt_id": attempt_a, "fencing_token": fence_a,
            "recovery_key": format!("recovery:{attempt_a}:{fence_a}:process_stopped"),
            "observation": "process_stopped",
            "details": {"journal_state": "prepared", "process_observed": false},
        })
        .to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{recovered_a}");
    assert_eq!(recovered_a["disposition"], "safe_pre_spawn_requeue");
    let state_a: String = sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id=?")
        .bind(&attempt_a)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(state_a, "lost");

    // Attempt B: an ambiguous/running-process recovery observation ->
    // `needs_operator`.
    enqueue_request(
        &repo,
        &clock,
        &item_id,
        RUNNER_ID,
        "profile-c2",
        "needs-operator-key",
    )
    .await;
    let (_, claimed_b) = send_as_runner(
        &app,
        "POST",
        "/claim",
        json!({"protocol_version":1,"runner_id":RUNNER_ID,"claim_request_id":"claim-needs-operator","available_capacity":2,"wait_ms":1000}).to_string(),
    )
    .await;
    let attempt_b = claimed_b["lease"]["attempt_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let fence_b = claimed_b["lease"]["fencing_token"].as_i64().unwrap();
    let (status, recovered_b) = send_as_runner(
        &app,
        "POST",
        &format!("/attempts/{attempt_b}/recovery-observation"),
        json!({
            "protocol_version": 1, "runner_id": RUNNER_ID, "attempt_id": attempt_b, "fencing_token": fence_b,
            "recovery_key": format!("recovery:{attempt_b}:{fence_b}:process_running"),
            "observation": "process_running",
            "details": {"journal_state": "process_observed_running", "process_observed": true},
        })
        .to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{recovered_b}");
    assert_eq!(recovered_b["disposition"], "needs_operator");
    let state_b: String = sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id=?")
        .bind(&attempt_b)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(state_b, "needs_operator");

    for (attempt_id, fencing_token) in [(&attempt_a, fence_a), (&attempt_b, fence_b)] {
        let (status, body) = send_as_runner(
            &app,
            "POST",
            &format!("/attempts/{attempt_id}/artifacts"),
            artifact_body(
                RUNNER_ID,
                attempt_id,
                fencing_token,
                "art-blocked",
                &"c".repeat(64),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["code"], "conflict", "{body}");
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM execution_artifacts WHERE attempt_id=?")
                .bind(attempt_id)
                .fetch_one(repo.pool())
                .await
                .unwrap();
        assert_eq!(
            count, 0,
            "no artifact must be written for attempt {attempt_id} in state that rejects writes"
        );
    }
}
