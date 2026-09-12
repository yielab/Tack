//! Proves `create_execution`'s model-policy resolution
//! (`tack_orch::model_policy::wiring::resolve_request_model_policy`) and
//! `GET /api/executions/{id}/attempts`'s `model_provenance`/
//! `usage_economics` fields work through the real production router
//! (`tack_api::router::build_router`), not a handler-local harness. Every
//! claim is checked against persisted database state or an exact JSON
//! shape, never a bare 2xx status.

use crate::common;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::{AppState, orch_runtime::OrchRuntime, router::build_router};
use tack_db::{Repository, init_pool, migrations};
use uuid::Uuid;

const OPERATOR_TOKEN: &str = "model-wiring-operator-token";
const BASE_REVISION: &str = "f6b0123456789abcdef0123456789abcdef0123";

async fn setup() -> (axum::Router, sqlx::SqlitePool, String) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'Model wiring', '{}')",
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
                name: "Model wiring".into(),
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

fn operator_headers() -> Vec<(&'static str, &'static str)> {
    vec![("authorization", "Bearer model-wiring-operator-token")]
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

/// A runner declaring exactly the `openai`/`opaque/model-f6b` combination —
/// every fixture in this file that needs a successful claim requests exactly
/// this pair.
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

/// `limits` carries the documented `{"default_model": ...}` convention
/// (`tack_orch::model_policy::wiring::DEFAULT_MODEL_KEY`), operator-settable
/// via this same route.
async fn create_agent_profile(app: &axum::Router, name: &str, limits: Value) -> String {
    let (status, profile) = common::send(
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
    let (status, fleet) = common::send(
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
        "repository_snapshot": {"kind": "git", "remote": "https://example.test/wiring.git", "base_revision": BASE_REVISION, "subdirectory": null},
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

async fn stored_requested_model_snapshot(pool: &sqlx::SqlitePool, request_id: &str) -> Value {
    let snapshot: String =
        sqlx::query_scalar("SELECT request_snapshot FROM execution_requests WHERE id = ?")
            .bind(request_id)
            .fetch_one(pool)
            .await
            .unwrap();
    serde_json::from_str(&snapshot).unwrap()
}

// =======================================================================
// `create_execution` resolves an absent/explicit model choice through the
// agent-profile/fleet/auto-select precedence tiers — proved against the
// persisted row and its idempotency-fingerprint snapshot.
// =======================================================================

struct ModelResolutionCase {
    name: &'static str,
    profile_limits: Value,
    fleet_default: Option<Value>,
    requested: Option<(&'static str, &'static str)>,
    expected: Option<(&'static str, &'static str)>,
}

/// Runs one [`ModelResolutionCase`] end to end and asserts its expectation —
/// factored out of the table-driven test so each case reads as one call.
async fn assert_model_resolution_case(i: usize, case: ModelResolutionCase) {
    let (app, pool, item_id) = setup().await;
    let agent_profile_id = create_agent_profile(&app, "profile", case.profile_limits).await;
    let (selector_kind, selector_id) = match case.fleet_default {
        Some(policy) => ("fleet", create_fleet(&app, "fleet", policy).await),
        None => ("exact_runner", enroll_runner(&app, "runner").await.0),
    };

    let (status, created) = common::send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(
            &item_id,
            &format!("model-precedence-{i}"),
            selector_kind,
            &selector_id,
            &agent_profile_id,
            case.requested.map(|(p, _)| p),
            case.requested.map(|(_, m)| m),
        ),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}: {created}", case.name);
    let request_id = created["request_id"].as_str().unwrap().to_owned();

    let (provider, model_id) = stored_requested_model(&pool, &request_id).await;
    match case.expected {
        Some((exp_provider, exp_model)) => {
            assert_eq!(provider.as_deref(), Some(exp_provider), "{}", case.name);
            assert_eq!(model_id.as_deref(), Some(exp_model), "{}", case.name);
            // The idempotency-fingerprint snapshot must agree with the stored
            // columns — no split-brain between the two.
            let snapshot = stored_requested_model_snapshot(&pool, &request_id).await;
            assert_eq!(
                snapshot["requested_model_provider"], exp_provider,
                "{}",
                case.name
            );
            assert_eq!(snapshot["requested_model_id"], exp_model, "{}", case.name);
        }
        None => {
            assert_eq!(provider, None, "{}: must persist as NULL", case.name);
            assert_eq!(model_id, None, "{}: must persist as NULL", case.name);
        }
    }
}

#[tokio::test]
async fn create_execution_resolves_model_by_precedence_tier() {
    let cases = [
        ModelResolutionCase {
            name: "agent profile default applies when neither client nor fleet has an opinion",
            profile_limits: json!({"default_model": {"provider": "openai", "model_id": "opaque/model-f6b"}}),
            fleet_default: None,
            requested: None,
            expected: Some(("openai", "opaque/model-f6b")),
        },
        ModelResolutionCase {
            name: "an explicit client choice overrides the agent profile's default",
            profile_limits: json!({"default_model": {"provider": "anthropic", "model_id": "opaque/should-never-be-used"}}),
            fleet_default: None,
            requested: Some(("openai", "opaque/model-f6b")),
            expected: Some(("openai", "opaque/model-f6b")),
        },
        ModelResolutionCase {
            name: "the fleet default applies only when the selector targets a fleet",
            profile_limits: json!({}),
            fleet_default: Some(
                json!({"default_model": {"provider": "openai", "model_id": "opaque/model-fleet-f6b"}}),
            ),
            requested: None,
            expected: Some(("openai", "opaque/model-fleet-f6b")),
        },
        ModelResolutionCase {
            name: "auto-select persists as NULL, never a placeholder string",
            profile_limits: json!({}),
            fleet_default: None,
            requested: None,
            expected: None,
        },
    ];

    for (i, case) in cases.into_iter().enumerate() {
        assert_model_resolution_case(i, case).await;
    }
}

// =======================================================================
// `GET /api/executions/{id}/attempts` carries honest
// `model_provenance`/`usage_economics` — proved end to end through
// claim → accept → start → completion.
// =======================================================================

struct LiveAttempt {
    request_id: String,
    attempt_id: String,
    runner_id: String,
    runner_auth: [(String, String); 1],
    agent_profile_id: String,
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
    let (runner_id, runner_auth_owned) = enroll_runner(app, "provenance runner").await;
    let runner_auth = headers_ref(&runner_auth_owned);
    let agent_profile_id = create_agent_profile(app, "provenance profile", json!({})).await;

    let (status, created) = common::send(
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

    let (status, claimed) = common::send(
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

    let (status, accepted) = common::send(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/accept"),
        json!({"protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token, "workspace_id": "ws-wiring", "base_revision": BASE_REVISION}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{accepted}");

    let (status, started) = common::send(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{attempt_id}/start"),
        json!({"protocol_version": 1, "runner_id": runner_id, "attempt_id": attempt_id, "fencing_token": fencing_token, "workspace_id": "ws-wiring", "base_revision": BASE_REVISION, "process_id": "pid-wiring"}),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");

    LiveAttempt {
        request_id,
        attempt_id,
        runner_id,
        runner_auth: runner_auth_owned,
        agent_profile_id,
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
            "workspace_id": "ws-wiring",
            "base_revision": BASE_REVISION,
            "started_at": "2026-08-08T12:00:00Z",
            "ended_at": "2026-08-08T12:05:00Z",
        },
        "usage": usage,
        "final_event_checkpoint": null,
    })
}

async fn get_attempts(app: &axum::Router, request_id: &str) -> Value {
    let (status, body) = common::send(
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
async fn attempt_summary_hides_provenance_and_costs_before_completion() {
    let (app, _pool, item_id) = setup().await;
    let live = claim_accept_start(
        &app,
        &item_id,
        "provenance-in-flight",
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

struct ProvenanceCase {
    name: &'static str,
    key: &'static str,
    actual_model_id: &'static str,
    usage: Value,
    expected_provenance: Value,
    expected_cost: Value,
}

/// Runs one [`ProvenanceCase`] end to end (claim → accept → start →
/// completion) and asserts its expected `model_provenance`/cost shape,
/// plus the cross-cutting invariants that hold regardless of which case:
/// the runner-time dollar dimension always stays `not_measured` (no
/// infra cost-rate exists in this schema, so it must never borrow the
/// harness's own figure or default to 0), and `wall_clock_ms` is computed
/// independently here — not trusted from the handler's own arithmetic —
/// and cross-checked against the database.
async fn assert_provenance_case(case: ProvenanceCase) {
    let (app, pool, item_id) = setup().await;
    let live = claim_accept_start(&app, &item_id, case.key, "openai", "opaque/model-f6b").await;
    let runner_auth = headers_ref(&live.runner_auth);

    let (status, completed) = common::send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/completion", live.attempt_id),
        completion_body(
            &live.runner_id,
            &live.attempt_id,
            live.fencing_token,
            &format!("{}-completion", case.key),
            "openai",
            case.actual_model_id,
            case.usage,
        ),
        &runner_auth,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{}: {completed}", case.name);

    let body = get_attempts(&app, &live.request_id).await;
    let attempt = &body["data"].as_array().expect("data array")[0];
    assert_eq!(
        attempt["model_provenance"], case.expected_provenance,
        "{}",
        case.name
    );
    assert_eq!(
        attempt["usage_economics"]["model_token_cost_usd_estimated"], case.expected_cost,
        "{}",
        case.name
    );

    let runner_cost = &attempt["usage_economics"]["runner_time_cost"]["cost_usd_estimated"];
    assert_eq!(
        runner_cost["value"],
        Value::Null,
        "{}: must be null, not 0",
        case.name
    );
    assert_eq!(runner_cost["source"], "not_measured");

    let started_at: DateTime<Utc> = attempt["started_at"].as_str().unwrap().parse().unwrap();
    let ended_at: DateTime<Utc> = attempt["ended_at"].as_str().unwrap().parse().unwrap();
    let expected_wall_clock_ms = ended_at
        .signed_duration_since(started_at)
        .num_milliseconds();
    assert_eq!(
        attempt["usage_economics"]["runner_time_cost"]["wall_clock_ms"],
        json!(expected_wall_clock_ms),
        "{}",
        case.name
    );
    let (db_started, db_ended): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT started_at, ended_at FROM execution_attempts WHERE id = ?")
            .bind(&live.attempt_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(db_started.is_some() && db_ended.is_some(), "{}", case.name);
}

#[tokio::test]
async fn attempt_summary_reports_provenance_and_cost_after_completion() {
    let cases = [
        ProvenanceCase {
            name: "matched: requested and actual model agree",
            key: "matched",
            actual_model_id: "opaque/model-f6b",
            usage: json!({
                "tokens_in": {"value": 1000, "source": "measured"},
                "tokens_out": {"value": 500, "source": "measured"},
                "duration_ms": {"value": 300000, "source": "measured"},
                "cost_usd": {"value": 0.42, "source": "measured"},
            }),
            expected_provenance: json!({"kind": "matched", "provider": "openai", "model_id": "opaque/model-f6b"}),
            expected_cost: json!({"value": 0.42, "source": "measured"}),
        },
        ProvenanceCase {
            name: "mismatched: the harness ran a different declared model than requested",
            key: "mismatched",
            actual_model_id: "opaque/model-f6b-mismatch",
            usage: json!({
                "tokens_in": {"value": null, "source": "not_measured"},
                "tokens_out": {"value": null, "source": "not_measured"},
                "duration_ms": {"value": null, "source": "not_measured"},
                "cost_usd": {"value": null, "source": "not_measured"},
            }),
            expected_provenance: json!({
                "kind": "mismatched",
                "requested_provider": "openai",
                "requested_model_id": "opaque/model-f6b",
                "actual_provider": "openai",
                "actual_model_id": "opaque/model-f6b-mismatch",
            }),
            expected_cost: json!({"value": null, "source": "not_measured"}),
        },
    ];

    for case in cases {
        assert_provenance_case(case).await;
    }
}

/// An item runs one attempt through to a terminal state, then the *same*
/// item is enqueued again with a fresh, never-used idempotency key, reusing
/// the same runner and agent profile a retry through the Run-with-agent
/// modal would. No invariant treats "this item already has a finished
/// attempt" as a reason to refuse a new request — the second call must
/// succeed exactly like the first.
#[tokio::test]
async fn create_execution_succeeds_for_item_with_a_finished_attempt() {
    let (app, _pool, item_id) = setup().await;
    let live =
        claim_accept_start(&app, &item_id, "repeat-first", "openai", "opaque/model-f6b").await;
    let runner_auth = headers_ref(&live.runner_auth);

    let (status, completed) = common::send(
        &app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/completion", live.attempt_id),
        completion_body(
            &live.runner_id,
            &live.attempt_id,
            live.fencing_token,
            "repeat-completion",
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
    let (status2, second) = common::send(
        &app,
        "POST",
        "/api/executions",
        execution_request_body(
            &item_id,
            "repeat-second",
            "exact_runner",
            &live.runner_id,
            &live.agent_profile_id,
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
