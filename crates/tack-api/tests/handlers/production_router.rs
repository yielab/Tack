//! Integration tests against the real, fully wired production router.
//!
//! Unlike `executions_runner_admin.rs`/`runner_protocol/lifecycle.rs` (routers built
//! local to those files) and `runner_vertical_slice.rs` (a repository/runner-fake
//! seam), every test in this file drives
//! `tack_api::router::build_router` — exactly as `tack serve` would run it —
//! so the claims that the production router completes the mock vertical
//! slice, and that runner routes sit outside the operator auth exemption,
//! hold for the *actual* mounted app, not a stand-in.

use crate::common;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use chrono::Utc;
use serde_json::{Value, json};
use sha2::Digest;
use tack_api::config::AppConfig;
use tack_api::openapi::ApiDoc;
use tack_api::{AppState, orch_runtime::OrchRuntime, router::build_router};
use tack_core::models::{CreateItem, CreateProject, ProjectType};
use tack_db::{Repository, init_pool, migrations};
use tower::ServiceExt;
use utoipa::OpenApi;
use uuid::Uuid;

const OPERATOR_TOKEN: &str = "c5-operator-secret-token";

// ---------------------------------------------------------------------
// Setup: builds the real `AppState`/`build_router`, not a test-local
// stand-in, and keeps the `Repository`/`Uuid` handles a test needs for
// fixture setup, direct DB assertions, and simulating a restart by
// rebuilding a fresh router/AppState around the *same* pool.
// ---------------------------------------------------------------------

async fn app_state(config: AppConfig, pool: sqlx::SqlitePool, workspace_id: Uuid) -> AppState {
    let (tx, _rx) = tokio::sync::broadcast::channel(16);
    AppState {
        repo: Repository::new(pool),
        config: AppConfig {
            database_url: "sqlite::memory:".to_string(),
            ..config
        },
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        orch_runtime: OrchRuntime::new(),
        local_runner: None,
    }
}

/// Builds a fresh production router over a clean in-memory database, plus a
/// fixture project/item created directly through the repository (the same
/// setup precedent every other test file in this crate uses — the fixture
/// data itself is not what's under test here).
async fn setup(config: AppConfig) -> (axum::Router, Repository, sqlx::SqlitePool, Uuid, String) {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_all(&pool).await.expect("migrations");
    let workspace_id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'C5', '{}')")
        .bind(workspace_id.to_string())
        .execute(&pool)
        .await
        .expect("workspace");
    let repo = Repository::new(pool.clone());
    let project = repo
        .create_project(
            workspace_id,
            CreateProject {
                name: "C5 vertical slice".into(),
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
                title: "Prove the mounted router".into(),
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
    let state = app_state(config, pool.clone(), workspace_id).await;
    (
        build_router(state),
        repo,
        pool,
        workspace_id,
        item.id.to_string(),
    )
}

/// Rebuilds a fresh router/AppState around the *same* pool — every
/// in-process runtime value (broadcast channel, orch runtime, and the
/// router/middleware closures themselves) is newly constructed, while only
/// the database survives, exactly like restarting the `tack serve` process
/// against its existing (in production, file-backed) database.
async fn restart(pool: sqlx::SqlitePool, config: AppConfig, workspace_id: Uuid) -> axum::Router {
    let state = app_state(config, pool, workspace_id).await;
    build_router(state)
}

fn operator_headers() -> Vec<(&'static str, &'static str)> {
    vec![("authorization", "Bearer c5-operator-secret-token")]
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// Declares "codex"/"openai"/"opaque/model-c5" — every execution request
/// body in this file requests exactly that pair (see `requested_harness_kind`
/// call sites) — and uses the *real* current wall-clock time as
/// `reported_at`/`probed_at`, not a frozen fixture date: this file drives
/// the real production router (`SystemExecutionClock`), and
/// `tack_orch::scheduler::wiring` falls back to a capability report's own
/// `reported_at` as a liveness signal for a runner that has never sent a
/// `/heartbeat` yet (true of every runner here). A hardcoded past date
/// would make that fallback correctly, honestly judge the runner stale.
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
                "model_ids": ["opaque/model-c5"],
                "discovery": "reported"
            }],
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

fn completion_body(runner_id: &str, attempt_id: &str, fencing_token: i64) -> Value {
    json!({
        "protocol_version": 1,
        "runner_id": runner_id,
        "attempt_id": attempt_id,
        "fencing_token": fencing_token,
        "completion_id": "c5-completion-1",
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
            "started_at": "2026-08-08T11:55:00Z",
            "ended_at": "2026-08-08T12:00:00Z",
        },
        "usage": {
            "tokens_in": {"value": 1234, "source": "measured"},
            "tokens_out": {"value": 456, "source": "measured"},
            "duration_ms": {"value": 295000, "source": "measured"},
            "cost_usd": {"value": null, "source": "not_measured"},
        },
        "final_event_checkpoint": "checkpoint-0001",
    })
}

// ---------------------------------------------------------------------
// 1. A mock runner enrolled on a clean database, through the *production*
//    router, can claim, start, stream, complete, and the result survives an
//    API restart.
// ---------------------------------------------------------------------

#[tokio::test]
async fn mock_vertical_slice_completes_and_survives_restart() {
    let config = AppConfig {
        api_token: Some(OPERATOR_TOKEN.into()),
        ..AppConfig::default()
    };
    let (app, repo, pool, workspace_id, item_id) = setup(config.clone()).await;
    let op = operator_headers();

    // Operator: create an agent profile.
    let (status, profile) = common::send(
        &app,
        "POST",
        "/api/agent-profiles",
        json!({"name": "C5 profile", "instructions": "work safely"}),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    let agent_profile_id = profile["agent_profile_id"].as_str().unwrap().to_owned();

    // Operator: create a pending runner and issue a one-time enrollment token.
    let (status, pending) = common::send(
        &app,
        "POST",
        "/api/runners/enrollment",
        json!({"name": "C5 runner", "total_capacity": 1, "available_capacity": 1}),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pending}");
    let runner_id = pending["runner_id"].as_str().unwrap().to_owned();
    let raw_enrollment_token = pending["enrollment_token"].as_str().unwrap().to_owned();

    // Runner: enroll (no operator credential; the enrollment token in the
    // body is the only authentication for this one exchange).
    let (status, enrolled) = common::send(
        &app,
        "POST",
        "/api/runner/v1/enroll",
        json!({
            "protocol_version": 1,
            "enrollment_token": raw_enrollment_token,
            "runner_name": "C5 runner",
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(),
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    assert_eq!(enrolled["runner_id"], runner_id);
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();
    let runner_auth = [("authorization", bearer(&credential))];
    let runner_auth_ref: Vec<(&str, &str)> =
        runner_auth.iter().map(|(k, v)| (*k, v.as_str())).collect();

    // Operator: create the execution request, exact-runner selected.
    let (status, created) = common::send(
        &app,
        "POST",
        "/api/executions",
        json!({
            "item_id": item_id,
            "idempotency_key": "c5-vertical-slice",
            "selector_kind": "exact_runner",
            "selector_id": runner_id,
            "agent_profile_id": agent_profile_id,
            "requested_harness_kind": "codex",
            "requested_model_provider": "openai",
            "requested_model_id": "opaque/model-c5",
            "agent_profile_snapshot": {"name":"C5 profile","instructions":"work safely","tool_policy":{},"timeout_seconds":60,"budgets":{}},
            "repository_snapshot": {"kind":"git","remote":"https://example.test/c5.git","base_revision":"abc123def456abc123def456abc123def456abc","subdirectory":null},
            "permission_policy": {"tools":["shell"],"network": false},
            "timeout_seconds": 60,
            "budgets": {},
            "environment": {},
            "metadata": {},
        }),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();
    assert_eq!(created["state"], "queued");

    // Runner: claim.
    let (status, claimed) = common::send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": "c5-claim", "available_capacity": 1, "wait_ms": 0}),
        &runner_auth_ref,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    assert_eq!(claimed["request"]["request_id"], request_id);
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    // Runner: accept (preparing) then start (running).
    let (status, accepted) = common::send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/accept"),
        json!({"protocol_version":1,"runner_id":runner_id,"attempt_id":attempt_id,"fencing_token":fencing_token,"workspace_id":"ws-1","base_revision":"abc123def456abc123def456abc123def456abc"}),
        &runner_auth_ref,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");
    assert_eq!(accepted["state"], "preparing");

    let (status, started) = common::send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/start"),
        json!({"protocol_version":1,"runner_id":runner_id,"attempt_id":attempt_id,"fencing_token":fencing_token,"workspace_id":"ws-1","base_revision":"abc123def456abc123def456abc123def456abc","process_id":"pid-1"}),
        &runner_auth_ref,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(started["state"], "running");

    // Runner: stream one event batch.
    let (status, batch) = common::send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/events"),
        json!({
            "protocol_version":1,"runner_id":runner_id,"attempt_id":attempt_id,"fencing_token":fencing_token,
            "previous_checkpoint": Value::Null, "checkpoint": "checkpoint-0001",
            "events": [{"event_id":"evt-1","sequence":1,"occurred_at":"2026-08-08T11:59:00Z","source":"runner","kind":"progress","payload":{"phase":"testing"}}],
        }),
        &runner_auth_ref,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{batch}");
    assert_eq!(batch["committed_checkpoint"], "checkpoint-0001");

    // Runner: complete.
    let (status, completed) = common::send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/completion"),
        completion_body(&runner_id, &attempt_id, fencing_token),
        &runner_auth_ref,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["state"], "succeeded");
    assert_eq!(completed["replayed"], false);

    // Operator: the production router reflects the terminal state.
    let (status, detail) = common::send(
        &app,
        "GET",
        &format!("/api/executions/{request_id}"),
        Value::Null,
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["state"], "succeeded");

    // --- Restart: rebuild the router/AppState from the same pool. ---
    let app = restart(pool, config, workspace_id).await;

    let (status, detail_after_restart) = common::send(
        &app,
        "GET",
        &format!("/api/executions/{request_id}"),
        Value::Null,
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail_after_restart}");
    assert_eq!(detail_after_restart["state"], "succeeded");

    // A runner completion replay against the *new* router instance is still
    // idempotent — proves the terminal record and fencing state genuinely
    // live in the database, not in anything the old process held in memory.
    let (status, replayed) = common::send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/completion"),
        completion_body(&runner_id, &attempt_id, fencing_token),
        &runner_auth_ref,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replayed}");
    assert_eq!(replayed["replayed"], true);
    assert_eq!(replayed["committed_at"], completed["committed_at"]);

    // No credential material ever appeared in a response body across the
    // whole lifecycle — the redaction guarantee, verified here end-to-end
    // through the real mounted router.
    let stored_hash: String =
        sqlx::query_scalar("SELECT credential_hash FROM agent_runners WHERE id=?")
            .bind(&runner_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    let token_hash: String = sqlx::query_scalar(
        "SELECT token_hash FROM agent_enrollment_tokens WHERE runner_id=? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&runner_id)
    .fetch_one(repo.pool())
    .await
    .unwrap();
    // `pending` and `enrolled` are the two documented, intentional
    // one-time issuance points (the raw enrollment token and the raw
    // runner credential, respectively) — everything *else* in the
    // lifecycle must never repeat them.
    let bodies_after_issuance = format!(
        "{profile} {created} {claimed} {accepted} {started} {batch} {completed} {detail} {detail_after_restart} {replayed}"
    );
    assert!(
        !bodies_after_issuance.contains(&credential),
        "raw runner credential leaked into a response after its one-time issuance"
    );
    assert!(
        !bodies_after_issuance.contains(&raw_enrollment_token),
        "raw enrollment token leaked into a response after its one-time issuance"
    );
    assert!(
        !pending.to_string().contains(&credential),
        "the enrollment-issuance response must not also carry the (not-yet-issued) runner credential"
    );
    let all_bodies = format!("{pending} {enrolled} {bodies_after_issuance}");
    assert!(
        !all_bodies.contains(&stored_hash),
        "credential hash leaked into a response"
    );
    assert!(
        !all_bodies.contains(&token_hash),
        "enrollment token hash leaked into a response"
    );
}

// ---------------------------------------------------------------------
// 2. `x-tack-principal` cannot be injected by an external client.
// ---------------------------------------------------------------------

#[tokio::test]
async fn x_tack_principal_from_client_is_stripped_and_overridden() {
    let config = AppConfig {
        api_token: Some(OPERATOR_TOKEN.into()),
        ..AppConfig::default()
    };
    let (app, repo, _pool, _workspace_id, item_id) = setup(config).await;

    let (status, profile) = common::send(
        &app,
        "POST",
        "/api/agent-profiles",
        json!({"name": "spoof-profile", "instructions": "work safely"}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    let agent_profile_id = profile["agent_profile_id"].as_str().unwrap().to_owned();

    let body = |key: &str| {
        json!({
            "item_id": item_id,
            "idempotency_key": key,
            "selector_kind": "fleet",
            "selector_id": "fleet-does-not-need-to-exist-for-fleet-selector",
            "agent_profile_id": agent_profile_id,
            "requested_harness_kind": "codex",
            "agent_profile_snapshot": {"name":"spoof-profile","instructions":"work safely","tool_policy":{},"timeout_seconds":60,"budgets":{}},
            "repository_snapshot": {"kind":"git","remote":"https://example.test/spoof.git","base_revision":"abc123def456abc123def456abc123def456abc","subdirectory":null},
            "permission_policy": {"tools":[],"network": false},
            "timeout_seconds": 60,
            "budgets": {},
            "environment": {},
            "metadata": {},
        })
    };

    // A client claiming to be "victim" via the header a real client can set.
    let mut victim_headers = operator_headers();
    victim_headers.push(("x-tack-principal", "victim"));
    let (status, as_victim) = common::send(
        &app,
        "POST",
        "/api/executions",
        body("spoof-key"),
        &victim_headers,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{as_victim}");

    // The same idempotency key, but the client now claims to be "attacker"
    // instead. If the header were trusted, this would be scoped to a
    // *different* principal and therefore create a brand new, unrelated
    // request. It must instead be recognized as an exact replay of the
    // first — proof the server never read the client's claimed identity.
    let mut attacker_headers = operator_headers();
    attacker_headers.push(("x-tack-principal", "attacker"));
    let (status, as_attacker) = common::send(
        &app,
        "POST",
        "/api/executions",
        body("spoof-key"),
        &attacker_headers,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{as_attacker}");
    assert_eq!(
        as_attacker["request_id"], as_victim["request_id"],
        "a spoofed x-tack-principal must not create a separate idempotency scope"
    );
    assert_eq!(as_attacker["replayed"], true);

    // Sending no header at all produces the identical persisted principal —
    // the header's presence or absence never changes the outcome, because
    // it is unconditionally overwritten before the handler ever runs.
    let (status, no_header) = common::send(
        &app,
        "POST",
        "/api/executions",
        body("spoof-key"),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{no_header}");
    assert_eq!(no_header["request_id"], as_victim["request_id"]);
    assert_eq!(no_header["replayed"], true);

    // Directly inspect the persisted immutable snapshot: the recorded
    // principal is neither client-supplied string.
    let request_id = as_victim["request_id"].as_str().unwrap();
    let snapshot: String =
        sqlx::query_scalar("SELECT request_snapshot FROM execution_requests WHERE id=?")
            .bind(request_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    let subject_id = snapshot["created_by"]["subject_id"].as_str().unwrap();
    assert_ne!(subject_id, "victim");
    assert_ne!(subject_id, "attacker");

    // A genuinely different idempotency key from the same (real, injected)
    // principal creates a distinct request — proves scoping still works at
    // all, just never from client input.
    let (status, second) = common::send(
        &app,
        "POST",
        "/api/executions",
        body("spoof-key-2"),
        &victim_headers,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_ne!(second["request_id"], as_victim["request_id"]);
}

// ---------------------------------------------------------------------
// 3. Distinct, non-substitutable authentication on the production router.
// ---------------------------------------------------------------------

#[tokio::test]
async fn operator_and_runner_credentials_are_not_substitutable() {
    let config = AppConfig {
        api_token: Some(OPERATOR_TOKEN.into()),
        ..AppConfig::default()
    };
    let (app, repo, _pool, _workspace_id, _item_id) = setup(config).await;

    // Register one real runner so its credential is a genuinely valid
    // runner bearer value — the strongest version of this test uses a real
    // credential, not a guess, so the negative results below cannot be
    // explained away as "it was just an unrecognized token either way".
    let clock = tack_db::repo::execution::SystemExecutionClock;
    let credential_hash = hex::encode(sha2::Sha256::digest(b"c5-real-runner-credential"));
    repo.register_runner(
        tack_db::repo::execution::NewRunner {
            id: "runner-real",
            name: "Real runner",
            credential_hash: &credential_hash,
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .expect("runner");

    // The operator's own valid API token, presented to a runner-v1 route:
    // rejected by `runner_auth`, which never even looks at whether the
    // bearer value equals TACK_API_TOKEN — it looks up a runner by hashed
    // credential and finds none.
    let (status, body) = common::send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version":1,"runner_id":"runner-real","claim_request_id":"x","available_capacity":1,"wait_ms":0}),
        &[("authorization", &bearer(OPERATOR_TOKEN))],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["error"]["code"], "unauthorized");

    // A real runner's own bearer credential, presented to an operator
    // route: rejected by `require_token`, which never looks up runner
    // credentials at all — it does a constant-time compare against
    // TACK_API_TOKEN and nothing else.
    let (status, _) = common::send(
        &app,
        "POST",
        "/api/executions",
        json!({}),
        &[("authorization", &bearer("c5-real-runner-credential"))],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // No credential at all on either family: both reject, neither silently
    // opens up.
    let (status, _) = common::send(&app, "POST", "/api/executions", json!({}), &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, body) = common::send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version":1,"runner_id":"runner-real","claim_request_id":"x","available_capacity":1,"wait_ms":0}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");
}

// ---------------------------------------------------------------------
// 4. Exact route/auth enumeration and OpenAPI drift.
// ---------------------------------------------------------------------

/// Mirrors `middleware.rs`'s own `no_runner_or_execution_path_is_publicly_exempt`
/// unit test, but *live*: drives the real production router with no
/// Authorization header at all (operator token configured) and confirms
/// every operator and runner-v1 route genuinely requires authentication —
/// not merely that the exemption list's source text excludes them.
#[tokio::test]
async fn every_execution_and_runner_v1_path_requires_auth_live() {
    let config = AppConfig {
        api_token: Some(OPERATOR_TOKEN.into()),
        ..AppConfig::default()
    };
    let (app, _repo, _pool, _workspace_id, _item_id) = setup(config).await;

    for (method, path) in [
        ("POST", "/api/executions"),
        ("GET", "/api/executions"),
        ("GET", "/api/executions/exec_missing"),
        ("POST", "/api/executions/exec_missing/cancel"),
        ("POST", "/api/executions/exec_missing/requeue"),
        ("POST", "/api/runner-fleets"),
        ("GET", "/api/runner-fleets"),
        ("POST", "/api/runners/enrollment"),
        ("POST", "/api/runners/runr_missing/revoke"),
        ("POST", "/api/agent-profiles"),
        ("GET", "/api/agent-profiles"),
        ("POST", "/api/model-profiles"),
        ("GET", "/api/model-profiles"),
    ] {
        let (status, body) = common::send(&app, method, path, json!({}), &[]).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {path} -> {body}"
        );
    }

    for (method, path) in [
        ("POST", "/api/runner/v1/claim"),
        ("POST", "/api/runner/v1/refresh"),
        ("POST", "/api/runner/v1/heartbeat"),
        ("POST", "/api/runner/v1/attempts/att_missing/accept"),
        ("POST", "/api/runner/v1/attempts/att_missing/events"),
        ("POST", "/api/runner/v1/attempts/att_missing/completion"),
    ] {
        let (status, body) = common::send(&app, method, path, json!({}), &[]).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {path} -> {body}"
        );
        assert_eq!(body["error"]["code"], "unauthorized", "{method} {path}");
    }

    // `/enroll` is the one runner route that is not attempt/runner scoped —
    // an empty body still reaches `runner_auth`-free JSON parsing, but never
    // succeeds without a valid single-use enrollment token, and is never
    // treated as a public/no-auth route either.
    let (status, _) = common::send(&app, "POST", "/api/runner/v1/enroll", json!({}), &[]).await;
    assert_ne!(status, StatusCode::OK);
}

/// Cross-checks the generated OpenAPI document's route surface against
/// what's actually mounted: every operator/runner-v1 route appears at
/// exactly the expected, fully-composed location.
#[tokio::test]
async fn openapi_enumerates_mounted_operator_and_runner_v1_routes() {
    let doc = ApiDoc::openapi();
    let raw = serde_json::to_value(&doc).unwrap();
    let paths = raw["paths"].as_object().expect("paths object");

    for path in [
        "/api/executions",
        "/api/executions/{request_id}",
        "/api/executions/{request_id}/cancel",
        "/api/executions/{request_id}/requeue",
        "/api/runner-fleets",
        "/api/runners/enrollment",
        "/api/runners/{runner_id}/enrollment-tokens/{token_id}/revoke",
        "/api/runners/{runner_id}/revoke",
        "/api/agent-profiles",
        "/api/model-profiles",
    ] {
        assert!(
            paths.contains_key(path),
            "operator path missing from OpenAPI: {path}"
        );
    }
    for path in [
        "/api/runner/v1/enroll",
        "/api/runner/v1/refresh",
        "/api/runner/v1/claim",
        "/api/runner/v1/heartbeat",
        "/api/runner/v1/attempts/{attempt_id}/accept",
        "/api/runner/v1/attempts/{attempt_id}/start",
        "/api/runner/v1/attempts/{attempt_id}/events",
        "/api/runner/v1/attempts/{attempt_id}/decisions",
        "/api/runner/v1/attempts/{attempt_id}/decisions/poll",
        "/api/runner/v1/attempts/{attempt_id}/artifacts",
        "/api/runner/v1/attempts/{attempt_id}/completion",
        "/api/runner/v1/attempts/{attempt_id}/cancellation-observation",
        "/api/runner/v1/attempts/{attempt_id}/recovery-observation",
    ] {
        assert!(
            paths.contains_key(path),
            "runner-v1 path missing from OpenAPI: {path}"
        );
    }

    // No response schema anywhere in the document's raw JSON exposes a
    // credential-shaped field name outside the one documented, intentional
    // one-time issuance points (`runner_credential`/`enrollment_token` — the
    // *field name* appears in the free-form-JSON description text, never as
    // a distinct schema forcing every response to carry it).
    let serialized = raw.to_string();
    assert!(
        !serialized.contains("credential_hash"),
        "OpenAPI document must never reference a stored credential hash field"
    );
    assert!(
        !serialized.contains("token_hash"),
        "OpenAPI document must never reference a stored token hash field"
    );
}

// ---------------------------------------------------------------------
// 5. CORS: the runner-v1 nest inherits the same global CORS layer as the
//    rest of the app (it sits outside `require_token`, not outside CORS).
// ---------------------------------------------------------------------

#[tokio::test]
async fn runner_v1_and_execution_routes_share_global_cors_policy() {
    let (app, _repo, _pool, _workspace_id, _item_id) = setup(AppConfig::default()).await;
    let allowed_origin = "http://localhost:8080"; // AppConfig::default()'s allow-list.

    for uri in ["/api/runner/v1/claim", "/api/executions"] {
        let req = Request::builder()
            .method("OPTIONS")
            .uri(uri)
            .header(header::ORIGIN, allowed_origin)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
            .header(
                header::ACCESS_CONTROL_REQUEST_HEADERS,
                "content-type, authorization",
            )
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        let allow_origin = response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert_eq!(allow_origin, allowed_origin, "{uri}");
    }
}

// ---------------------------------------------------------------------
// 6. Runner-v1 body limit precedence: without `effective_body_limit_bytes`,
//    the runner-v1 sub-router's own, more-specific `DefaultBodyLimit` layer
//    (a fixed 4 MiB protocol ceiling) always wins over the plain global
//    `DefaultBodyLimit` layered on `outer` in `router.rs`, regardless of
//    `TACK_MAX_BODY_SIZE`/`AppConfig::max_body_size_bytes`. An operator
//    hardening a deployment below 4 MiB could configure a tighter global
//    limit and it would silently have no effect on `/api/runner/v1/*` —
//    proven live: with the global limit configured to 2 KiB, a 512 KiB body
//    to `/api/runner/v1/claim` was read in full and the handler ran (a bad
//    credential returned 401, not 413).
// ---------------------------------------------------------------------

/// Shared setup for both precedence-direction tests below: an operator
/// creates an agent profile and a pending runner, the runner enrolls (a
/// real bearer credential, not a stub), and the operator queues one
/// claimable execution request selecting that exact runner. Returns the
/// runner id, the queued request id, and the runner's raw credential — the
/// caller drives `/api/runner/v1/claim` directly (both as a raw oversized
/// request and, for the sanity check, through `send`).
async fn queue_one_claimable_runner_v1_request(
    app: &axum::Router,
    item_id: &str,
) -> (String, String, String) {
    let op = operator_headers();
    let (status, profile) = common::send(
        app,
        "POST",
        "/api/agent-profiles",
        json!({"name": "body-limit profile", "instructions": "work safely"}),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    let agent_profile_id = profile["agent_profile_id"].as_str().unwrap().to_owned();

    let (status, pending) = common::send(
        app,
        "POST",
        "/api/runners/enrollment",
        json!({"name": "body-limit runner", "total_capacity": 1, "available_capacity": 1}),
        &op,
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
            "runner_name": "body-limit runner",
            "runner_version": "0.1.0",
            "capabilities": full_capabilities(),
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{enrolled}");
    assert_eq!(enrolled["runner_id"], runner_id);
    let credential = enrolled["runner_credential"].as_str().unwrap().to_owned();

    let (status, created) = common::send(
        app,
        "POST",
        "/api/executions",
        json!({
            "item_id": item_id,
            "idempotency_key": "body-limit-precedence",
            "selector_kind": "exact_runner",
            "selector_id": runner_id,
            "agent_profile_id": agent_profile_id,
            "requested_harness_kind": "codex",
            "requested_model_provider": "openai",
            "requested_model_id": "opaque/model-c5",
            "agent_profile_snapshot": {"name":"body-limit profile","instructions":"work safely","tool_policy":{},"timeout_seconds":60,"budgets":{}},
            "repository_snapshot": {"kind":"git","remote":"https://example.test/body-limit.git","base_revision":"abc123def456abc123def456abc123def456abc","subdirectory":null},
            "permission_policy": {"tools":["shell"],"network": false},
            "timeout_seconds": 60,
            "budgets": {},
            "environment": {},
            "metadata": {},
        }),
        &op,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();
    assert_eq!(created["state"], "queued");

    (runner_id, request_id, credential)
}

/// Sends a raw, oversized `/api/runner/v1/claim` body (not through `send`,
/// which serializes a small `Value` — this needs an exact, large byte count)
/// and returns the response status plus its raw body bytes.
async fn post_oversized_claim(
    app: &axum::Router,
    runner_id: &str,
    credential: &str,
    claim_request_id: &str,
    padding_bytes: usize,
) -> (StatusCode, axum::body::Bytes) {
    let body = json!({
        "protocol_version": 1,
        "runner_id": runner_id,
        "claim_request_id": claim_request_id,
        "available_capacity": 1,
        "wait_ms": 0,
        // Padding, not a real field `claim.request.json` defines — its only
        // purpose is to inflate the body to an exact, controlled size. A
        // well-formed request that happens to be oversized is the honest
        // reproduction of the live defect (an operator's real 2 KiB-sized
        // config rejecting a real, if bloated, request) rather than garbage
        // bytes that would fail JSON parsing for an unrelated reason if the
        // handler ever ran.
        "padding": "a".repeat(padding_bytes),
    });
    let request = Request::builder()
        .method("POST")
        .uri("/api/runner/v1/claim")
        .header("content-type", "application/json")
        .header("authorization", bearer(credential))
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    (status, bytes)
}

#[tokio::test]
async fn runner_v1_body_limit_is_lesser_of_configured_and_ceiling() {
    // Two directions on the same claim, "the runner-v1 body limit is
    // min(configured, 4 MiB ceiling)": a configured limit *below* the
    // ceiling is genuinely enforced (before this fix, the router's own
    // 4 MiB layer always won, so a 512 KiB body sailed through a 2 KiB
    // configured limit); a configured limit *above* the ceiling never
    // loosens the runner-v1 surface past the protocol's own cap.
    let cases = [
        ("configured limit below the ceiling", 2 * 1024, 512 * 1024),
        (
            "configured limit above the ceiling",
            10 * 1024 * 1024,
            5 * 1024 * 1024,
        ),
    ];
    for (name, configured_limit, oversized_padding) in cases {
        let config = AppConfig {
            max_body_size_bytes: configured_limit,
            ..AppConfig::default()
        };
        let (app, repo, _pool, _workspace_id, item_id) = setup(config).await;
        let (runner_id, request_id, credential) =
            queue_one_claimable_runner_v1_request(&app, &item_id).await;

        let (status, body_bytes) = post_oversized_claim(
            &app,
            &runner_id,
            &credential,
            "oversized",
            oversized_padding,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::PAYLOAD_TOO_LARGE,
            "{name}: {}",
            String::from_utf8_lossy(&body_bytes)
        );
        // Genuine layer-level rejection, not merely a handler-level error:
        // every error this handler itself produces (including its own
        // `payload_too_large` for `json_body_bytes_max`) is a JSON envelope
        // with an `error` object and `request_id: "req_runner"`
        // (`runner_auth::protocol_error`). Axum's own body-extraction
        // rejection is not that shape at all, proving the handler — and
        // therefore `runner_auth::authenticate` — never ran.
        let as_value: Option<Value> = serde_json::from_slice(&body_bytes).ok();
        assert!(
            as_value.as_ref().and_then(|v| v.get("error")).is_none(),
            "{name}: expected axum's own body-limit rejection, not the handler's JSON error envelope: {}",
            String::from_utf8_lossy(&body_bytes)
        );

        // Wrote nothing: the request is still queued and no attempt/lease
        // exists — a completed claim would have moved the request to
        // 'leased' and inserted an `execution_attempts` row, so this is
        // direct proof the handler body never executed, not just that the
        // HTTP layer said no.
        let state: String = sqlx::query_scalar("SELECT state FROM execution_requests WHERE id=?")
            .bind(&request_id)
            .fetch_one(repo.pool())
            .await
            .unwrap();
        assert_eq!(state, "queued", "{name}");
        let attempt_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM execution_attempts WHERE request_id=?")
                .bind(&request_id)
                .fetch_one(repo.pool())
                .await
                .unwrap();
        assert_eq!(attempt_count, 0, "{name}");

        // Sanity: the identical runner/request, with a normal small body,
        // succeeds — proving the rejection above was genuinely about size
        // and not a broken fixture.
        let (status, claimed) = common::send(
            &app,
            "POST",
            "/api/runner/v1/claim",
            json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": "normal-sized", "available_capacity": 1, "wait_ms": 0}),
            &[("authorization", bearer(&credential).as_str())],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{name}: {claimed}");
        assert_eq!(claimed["request"]["request_id"], request_id, "{name}");
    }
}
