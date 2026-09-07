//! Proves two wiring points in
//! `crates/tack-api/src/handlers/executions.rs` work through the *real*
//! production router (`tack_api::router::build_router` — exactly what `tack
//! serve` mounts), not just a handler-local test harness.
//!
//! 1. `create_execution` resolves an absent client model choice via
//!    `tack_orch::model_policy::wiring::resolve_request_model_policy`.
//! 2. `GET /api/executions/{id}/attempts` carries `model_provenance` and
//!    `usage_economics` on each `AttemptSummary`, built via
//!    `derive_attempt_facts`.
//!
//! Every claim below is proved against persisted database state or an exact
//! JSON shape, not merely a 2xx status code.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, orch_runtime::OrchRuntime, router::build_router};
use tack_db::{Repository, init_pool, migrations};
use tower::ServiceExt;
use uuid::Uuid;

const OPERATOR_TOKEN: &str = "f6b-model-wiring-operator-token";
const BASE_REVISION: &str = "f6b0123456789abcdef0123456789abcdef0123";

// ---------------------------------------------------------------------
// Infrastructure — same shape as `wave2_gate.rs`/`handlers/operator_read_routes.rs`: a
// fresh in-memory database and the real production router, so this file's
// assertions can read persisted rows directly rather than trusting a
// handler's own return value.
// ---------------------------------------------------------------------

async fn setup() -> (axum::Router, sqlx::SqlitePool, String) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'F6b model wiring', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert workspace");
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
                name: "F6b model wiring".into(),
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
                title: "Prove the model-policy/provenance wiring".into(),
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
    (app, pool, item.id.to_string())
}

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Value,
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
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

fn operator_headers() -> Vec<(&'static str, &'static str)> {
    vec![("authorization", "Bearer f6b-model-wiring-operator-token")]
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// A runner declaring exactly the `openai`/`opaque/model-f6b` combination —
/// every fixture in this file that needs a successful claim requests exactly
/// this pair (matching `wave2_gate.rs`'s pattern of a scheduler that
/// actually checks declared capability, not a naive FIFO match).
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
                "model_ids": ["opaque/model-f6b", "opaque/model-f6b-mismatch"],
                "discovery": "reported"
            }],
        }],
        "features": {},
        "limits": {"event_payload_bytes_max": 65536, "artifact_content_bytes_max": 52428800},
    })
}

async fn enroll_runner(app: &axum::Router, name: &str) -> (String, [(String, String); 1]) {
    let (status, pending) = send(
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

    let (status, enrolled) = send(
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

/// `limits` carries the documented `{"default_model": ...}` convention
/// (`tack_orch::model_policy::wiring::DEFAULT_MODEL_KEY`), operator-settable
/// via this same route.
async fn create_agent_profile(app: &axum::Router, name: &str, limits: Value) -> String {
    let (status, profile) = send(
        app,
        "POST",
        "/api/agent-profiles",
        json!({"name": name, "instructions": "work safely", "limits": limits}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{profile}");
    profile["agent_profile_id"].as_str().unwrap().to_owned()
}

/// Same convention, read from `agent_fleets.default_policy` instead.
async fn create_fleet(app: &axum::Router, name: &str, default_policy: Value) -> String {
    let (status, fleet) = send(
        app,
        "POST",
        "/api/runner-fleets",
        json!({"name": name, "default_policy": default_policy}),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fleet}");
    fleet["fleet_id"].as_str().unwrap().to_owned()
}

/// `requested_model_provider`/`requested_model_id` are the two fields under
/// test — `None` reproduces "client expressed no opinion", the case wired
/// to the resolver.
#[allow(clippy::too_many_arguments)]
fn execution_request_body(
    item_id: &str,
    key: &str,
    selector_kind: &str,
    selector_id: &str,
    agent_profile_id: &str,
    requested_model_provider: Option<&str>,
    requested_model_id: Option<&str>,
) -> Value {
    json!({
        "item_id": item_id,
        "idempotency_key": key,
        "selector_kind": selector_kind,
        "selector_id": selector_id,
        "agent_profile_id": agent_profile_id,
        "requested_harness_kind": "codex",
        "requested_model_provider": requested_model_provider,
        "requested_model_id": requested_model_id,
        "agent_profile_snapshot": {"name": "profile", "instructions": "work safely", "tool_policy": {}, "timeout_seconds": 60, "budgets": {}},
        "repository_snapshot": {"kind": "git", "remote": "https://example.test/f6b.git", "base_revision": BASE_REVISION, "subdirectory": null},
        "permission_policy": {"tools": ["shell"], "network": false},
        "timeout_seconds": 60,
        "budgets": {},
        "environment": {},
        "metadata": {},
    })
}

async fn stored_requested_model(
    pool: &sqlx::SqlitePool,
    request_id: &str,
) -> (Option<String>, Option<String>) {
    sqlx::query_as(
        "SELECT requested_model_provider, requested_model_id FROM execution_requests WHERE id = ?",
    )
    .bind(request_id)
    .fetch_one(pool)
    .await
    .expect("execution_requests row must exist")
}

// =======================================================================
// 1. `create_execution` resolves an absent model choice via
//    `resolve_request_model_policy` — proved against the persisted row.
// =======================================================================

#[tokio::test]
async fn create_execution_resolves_agent_profile_default_when_client_omits_both_fields() {
    let (app, pool, item_id) = setup().await;
    let (runner_id, _auth) = enroll_runner(&app, "F6b agent-profile runner").await;
    let agent_profile_id = create_agent_profile(
        &app,
        "F6b profile with default",
        json!({"default_model": {"provider": "openai", "model_id": "opaque/model-f6b"}}),
    )
    .await;

    let (status, created) = send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(
            &item_id,
            "f6b-agent-profile-default",
            "exact_runner",
            &runner_id,
            &agent_profile_id,
            None,
            None,
        ),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    // The load-bearing assertion: the *stored row*, not just a 2xx. Without
    // the resolver wiring, this column pair would be NULL/NULL (the client
    // sent no explicit choice and nothing ever called the resolver).
    let (provider, model_id) = stored_requested_model(&pool, &request_id).await;
    assert_eq!(
        provider.as_deref(),
        Some("openai"),
        "the agent profile's configured default_model.provider must be persisted"
    );
    assert_eq!(
        model_id.as_deref(),
        Some("opaque/model-f6b"),
        "the agent profile's configured default_model.model_id must be persisted"
    );

    // The request snapshot (used for idempotency fingerprinting/replay) must
    // agree with the stored columns — no split-brain between the two.
    let snapshot: String =
        sqlx::query_scalar("SELECT request_snapshot FROM execution_requests WHERE id = ?")
            .bind(&request_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let snapshot: Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(snapshot["requested_model_provider"], "openai");
    assert_eq!(snapshot["requested_model_id"], "opaque/model-f6b");
}

#[tokio::test]
async fn create_execution_does_not_override_an_explicit_client_supplied_model() {
    let (app, pool, item_id) = setup().await;
    let (runner_id, _auth) = enroll_runner(&app, "F6b explicit-wins runner").await;
    // The agent profile's default deliberately names a *different* model
    // than the client will request — if the wiring ever ignored request
    // precedence, this test would see the profile's default leak through.
    let agent_profile_id = create_agent_profile(
        &app,
        "F6b profile with a decoy default",
        json!({"default_model": {"provider": "anthropic", "model_id": "opaque/should-never-be-used"}}),
    )
    .await;

    let (status, created) = send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(
            &item_id,
            "f6b-explicit-wins",
            "exact_runner",
            &runner_id,
            &agent_profile_id,
            Some("openai"),
            Some("opaque/model-f6b"),
        ),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    let (provider, model_id) = stored_requested_model(&pool, &request_id).await;
    assert_eq!(
        provider.as_deref(),
        Some("openai"),
        "the client's explicit provider must win over the agent profile's default"
    );
    assert_eq!(
        model_id.as_deref(),
        Some("opaque/model-f6b"),
        "the client's explicit model_id must win over the agent profile's default"
    );
}

#[tokio::test]
async fn create_execution_resolves_fleet_default_only_when_selector_is_fleet() {
    let (app, pool, item_id) = setup().await;
    let fleet_id = create_fleet(
        &app,
        "F6b fleet with default",
        json!({"default_model": {"provider": "openai", "model_id": "opaque/model-fleet-f6b"}}),
    )
    .await;
    // No agent-profile default configured — the fleet tier must be the one
    // that supplies the resolved value.
    let agent_profile_id =
        create_agent_profile(&app, "F6b profile with no default", json!({})).await;

    let (status, created) = send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(
            &item_id,
            "f6b-fleet-default",
            "fleet",
            &fleet_id,
            &agent_profile_id,
            None,
            None,
        ),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    let (provider, model_id) = stored_requested_model(&pool, &request_id).await;
    assert_eq!(provider.as_deref(), Some("openai"));
    assert_eq!(model_id.as_deref(), Some("opaque/model-fleet-f6b"));
}

#[tokio::test]
async fn create_execution_resolves_to_auto_select_when_no_tier_is_configured() {
    let (app, pool, item_id) = setup().await;
    let (runner_id, _auth) = enroll_runner(&app, "F6b auto-select runner").await;
    // Empty `limits` — the agent profile expresses no default_model opinion,
    // and there is no fleet in play (`exact_runner` selector).
    let agent_profile_id =
        create_agent_profile(&app, "F6b profile with empty limits", json!({})).await;

    let (status, created) = send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(
            &item_id,
            "f6b-auto-select",
            "exact_runner",
            &runner_id,
            &agent_profile_id,
            None,
            None,
        ),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    // `ModelSelector::AutoSelect` converts back to *both* wire fields being
    // `NULL` — never a fabricated "unknown" provider/model string.
    let (provider, model_id) = stored_requested_model(&pool, &request_id).await;
    assert_eq!(
        provider, None,
        "auto-select must persist as NULL, not a placeholder string"
    );
    assert_eq!(
        model_id, None,
        "auto-select must persist as NULL, not a placeholder string"
    );
}

// =======================================================================
// 2. `GET /api/executions/{id}/attempts` carries honest
//    `model_provenance`/`usage_economics` — proved end to end through
//    claim → accept → start → completion.
// =======================================================================

struct LiveAttempt {
    request_id: String,
    attempt_id: String,
    runner_id: String,
    runner_auth: [(String, String); 1],
    fencing_token: i64,
}

/// Drives an execution request through claim, accept and start — stopping
/// short of completion so callers can inspect the in-flight
/// `model_provenance`/`usage_economics` shape (still honestly absent) before
/// choosing how to complete it.
async fn claim_accept_start(
    app: &axum::Router,
    item_id: &str,
    idempotency_key: &str,
    requested_model_provider: &str,
    requested_model_id: &str,
) -> LiveAttempt {
    let (runner_id, runner_auth_owned) = enroll_runner(app, "F6b provenance runner").await;
    let runner_auth = headers_ref(&runner_auth_owned);
    let agent_profile_id = create_agent_profile(app, "F6b provenance profile", json!({})).await;

    let (status, created) = send(
        app,
        "POST",
        "/api/executions",
        execution_request_body(
            item_id,
            idempotency_key,
            "exact_runner",
            &runner_id,
            &agent_profile_id,
            Some(requested_model_provider),
            Some(requested_model_id),
        ),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    let (status, claimed) = send(
        app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": format!("{idempotency_key}-claim"), "available_capacity": 1, "wait_ms": 0}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    let (status, accepted) = send(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/accept"),
        json!({"protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token, "workspace_id": "ws-f6b", "base_revision": BASE_REVISION}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");

    let (status, started) = send(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/start"),
        json!({"protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token, "workspace_id": "ws-f6b", "base_revision": BASE_REVISION, "process_id": "pid-f6b"}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");

    LiveAttempt {
        request_id,
        attempt_id,
        runner_id,
        runner_auth: runner_auth_owned,
        fencing_token,
    }
}

#[allow(clippy::too_many_arguments)]
fn completion_body(
    runner_id: &str,
    attempt_id: &str,
    fencing_token: i64,
    completion_id: &str,
    actual_model_provider: &str,
    actual_model_id: &str,
    usage: Value,
) -> Value {
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
            "harness_version": "1.0.0",
            "model_provider": actual_model_provider,
            "model_id": actual_model_id,
            "model_observation_source": "harness_reported",
            "capability_snapshot": {
                "cancel": {"support": "advisory", "reason": null},
                "resume": {"support": "unsupported", "reason": "no resumable session contract"},
                "decisions": {"support": "supported", "reason": null},
                "artifacts": {"support": "supported", "reason": null},
                "usage": {"support": "advisory", "reason": "usage may be absent"},
            },
            "workspace_id": "ws-f6b",
            "base_revision": BASE_REVISION,
            "started_at": "2026-08-08T12:00:00Z",
            "ended_at": "2026-08-08T12:05:00Z",
        },
        "usage": usage,
        "final_event_checkpoint": null,
    })
}

async fn get_attempts(app: &axum::Router, request_id: &str) -> Value {
    let (status, body) = send(
        app,
        "GET",
        &format!("/api/executions/{request_id}/attempts"),
        Value::Null,
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn attempt_summary_omits_provenance_and_shows_no_wall_clock_before_completion() {
    let (app, _pool, item_id) = setup().await;
    let live = claim_accept_start(
        &app,
        &item_id,
        "f6b-provenance-in-flight",
        "openai",
        "opaque/model-f6b",
    )
    .await;

    let body = get_attempts(&app, &live.request_id).await;
    let data = body["data"].as_array().expect("data array");
    assert_eq!(data.len(), 1);
    let attempt = &data[0];
    assert_eq!(
        attempt["model_provenance"],
        Value::Null,
        "no actual_execution has been reported yet — provenance must be honestly absent, not guessed"
    );
    assert_eq!(
        attempt["usage_economics"]["runner_time_cost"]["wall_clock_ms"],
        Value::Null,
        "ended_at is not yet known — wall_clock_ms must stay null, never a fabricated partial duration"
    );
    assert_eq!(
        attempt["usage_economics"]["runner_time_cost"]["cost_usd_estimated"],
        json!({"value": null, "source": "not_measured"})
    );
    assert_eq!(
        attempt["usage_economics"]["model_token_cost_usd_estimated"],
        json!({"value": null, "source": "not_measured"})
    );
}

#[tokio::test]
async fn attempt_summary_reports_matched_provenance_and_honest_runner_time_cost() {
    let (app, pool, item_id) = setup().await;
    let live =
        claim_accept_start(&app, &item_id, "f6b-matched", "openai", "opaque/model-f6b").await;
    let runner_auth = headers_ref(&live.runner_auth);

    // The harness's own self-reported dollar figure — measured, non-null —
    // to prove `model_token_cost_usd_estimated` is a real pass-through, not
    // vacuously always null.
    let (status, completed) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/completion", live.attempt_id),
        completion_body(
            &live.runner_id,
            &live.attempt_id,
            live.fencing_token,
            "f6b-matched-completion",
            "openai",
            "opaque/model-f6b",
            json!({
                "tokens_in": {"value": 1000, "source": "measured"},
                "tokens_out": {"value": 500, "source": "measured"},
                "duration_ms": {"value": 300000, "source": "measured"},
                "cost_usd": {"value": 0.42, "source": "measured"},
            }),
        ),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");

    let body = get_attempts(&app, &live.request_id).await;
    let data = body["data"].as_array().expect("data array");
    assert_eq!(data.len(), 1);
    let attempt = &data[0];

    assert_eq!(
        attempt["model_provenance"],
        json!({"kind": "matched", "provider": "openai", "model_id": "opaque/model-f6b"}),
        "requested and actual agree — this must read as matched, not silently reconciled away"
    );

    // The harness's own self-report passes through honestly...
    assert_eq!(
        attempt["usage_economics"]["model_token_cost_usd_estimated"],
        json!({"value": 0.42, "source": "measured"})
    );
    // ...while the runner-time dimension stays independently `not_measured`
    // — no infra cost-rate is stored anywhere in this schema today, so this
    // must never silently borrow the harness's own figure or default to 0.
    // Asserted as an explicit null, not merely
    // "falsy" — a structural zero here would be the exact bug this project's
    // rule 7 forbids.
    let runner_cost = &attempt["usage_economics"]["runner_time_cost"]["cost_usd_estimated"];
    assert_eq!(
        runner_cost["value"],
        Value::Null,
        "must be null, not 0 or 0.0"
    );
    assert_eq!(runner_cost["source"], "not_measured");
    assert_ne!(
        runner_cost["value"], attempt["usage_economics"]["model_token_cost_usd_estimated"]["value"],
        "the two dollar dimensions must never be conflated"
    );

    // Positive control: wall_clock_ms *is* known (both `execution_attempts`
    // timestamps exist after completion) even though the dollar estimate
    // above is not — proving the null case above is not a vacuous
    // "always null" implementation. Computed independently here from the
    // attempt's own `started_at`/`ended_at` wire fields so this assertion
    // does not simply trust the handler's own arithmetic.
    let started_at: DateTime<Utc> = attempt["started_at"]
        .as_str()
        .unwrap()
        .parse()
        .expect("started_at must be a real RFC3339 timestamp by now");
    let ended_at: DateTime<Utc> = attempt["ended_at"]
        .as_str()
        .unwrap()
        .parse()
        .expect("ended_at must be a real RFC3339 timestamp by now");
    let expected_wall_clock_ms = ended_at
        .signed_duration_since(started_at)
        .num_milliseconds();
    assert_eq!(
        attempt["usage_economics"]["runner_time_cost"]["wall_clock_ms"],
        json!(expected_wall_clock_ms)
    );

    // Also confirm directly against the database — the attempt row really
    // does carry both timestamps now, independent of what the handler chose
    // to render.
    let (db_started, db_ended): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT started_at, ended_at FROM execution_attempts WHERE id = ?")
            .bind(&live.attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(db_started.is_some());
    assert!(db_ended.is_some());
}

#[tokio::test]
async fn attempt_summary_reports_mismatched_provenance_with_both_sides_visible() {
    let (app, _pool, item_id) = setup().await;
    // Request `opaque/model-f6b`, but the harness actually runs
    // `opaque/model-f6b-mismatch` (also declared by this runner's
    // capabilities, so the claim itself is unaffected — a harness choosing a
    // different concrete model than requested is exactly the scenario this
    // field exists to surface, not a claim-time eligibility failure).
    let live = claim_accept_start(
        &app,
        &item_id,
        "f6b-mismatched",
        "openai",
        "opaque/model-f6b",
    )
    .await;
    let runner_auth = headers_ref(&live.runner_auth);

    let (status, completed) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/completion", live.attempt_id),
        completion_body(
            &live.runner_id,
            &live.attempt_id,
            live.fencing_token,
            "f6b-mismatched-completion",
            "openai",
            "opaque/model-f6b-mismatch",
            json!({
                "tokens_in": {"value": null, "source": "not_measured"},
                "tokens_out": {"value": null, "source": "not_measured"},
                "duration_ms": {"value": null, "source": "not_measured"},
                "cost_usd": {"value": null, "source": "not_measured"},
            }),
        ),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");

    let body = get_attempts(&app, &live.request_id).await;
    let attempt = &body["data"].as_array().expect("data array")[0];

    assert_eq!(
        attempt["model_provenance"],
        json!({
            "kind": "mismatched",
            "requested_provider": "openai",
            "requested_model_id": "opaque/model-f6b",
            "actual_provider": "openai",
            "actual_model_id": "opaque/model-f6b-mismatch",
        }),
        "both the requested and actual model_id must be visible simultaneously, never silently reconciled"
    );
    assert_ne!(
        attempt["model_provenance"]["requested_model_id"],
        attempt["model_provenance"]["actual_model_id"]
    );
}

/// An item runs one attempt through to a terminal state, then the *same*
/// item is enqueued again with a fresh, never-used idempotency key, reusing
/// the same runner and agent profile a retry through the Run-with-agent
/// modal would. Guards against a regression that would make
/// `execution_requests`/`execution_attempts` treat "this item already has a
/// finished attempt" as a reason to refuse a new request — no such
/// invariant exists, and the second call must succeed exactly like the
/// first.
#[tokio::test]
async fn create_execution_succeeds_again_for_an_item_with_a_finished_attempt() {
    let (app, _pool, item_id) = setup().await;
    let (runner_id, runner_auth_owned) = enroll_runner(&app, "C36 repeat-enqueue runner").await;
    let runner_auth = headers_ref(&runner_auth_owned);
    let agent_profile_id =
        create_agent_profile(&app, "C36 repeat-enqueue profile", json!({})).await;

    let (status, created) = send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(
            &item_id,
            "c36-first",
            "exact_runner",
            &runner_id,
            &agent_profile_id,
            Some("openai"),
            Some("opaque/model-f6b"),
        ),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");

    let (status, claimed) = send(
        &app,
        "POST",
        "/api/runner/v1/claim",
        json!({"protocol_version": 1, "runner_id": runner_id, "claim_request_id": "c36-claim", "available_capacity": 1, "wait_ms": 0}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let attempt_id = claimed["lease"]["attempt_id"].as_str().unwrap().to_owned();
    let fencing_token = claimed["lease"]["fencing_token"].as_i64().unwrap();

    let (status, accepted) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/accept"),
        json!({"protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token, "workspace_id": "ws-c36", "base_revision": BASE_REVISION}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");

    let (status, started) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/start"),
        json!({"protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token, "workspace_id": "ws-c36", "base_revision": BASE_REVISION, "process_id": "pid-c36"}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");

    let (status, completed) = send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/completion"),
        completion_body(
            &runner_id,
            &attempt_id,
            fencing_token,
            "c36-completion",
            "openai",
            "opaque/model-f6b",
            json!({
                "tokens_in": {"value": null, "source": "not_measured"},
                "tokens_out": {"value": null, "source": "not_measured"},
                "duration_ms": {"value": null, "source": "not_measured"},
                "cost_usd": {"value": null, "source": "not_measured"},
            }),
        ),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");

    // The item's one attempt is now terminal (`succeeded`). A fresh
    // idempotency key against the same item, same runner, same profile.
    let (status2, second) = send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(
            &item_id,
            "c36-second",
            "exact_runner",
            &runner_id,
            &agent_profile_id,
            Some("openai"),
            Some("opaque/model-f6b"),
        ),
        &operator_headers(),
    )
    .await;
    assert_eq!(
        status2,
        StatusCode::OK,
        "second enqueue for an item with a finished attempt must not fail: {second}"
    );
    assert_eq!(second["state"], "queued");
    assert_eq!(second["replayed"], false);
}
