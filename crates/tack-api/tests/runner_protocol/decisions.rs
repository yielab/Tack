//! Tests for `handlers/decisions.rs` (operator-scoped decision resolution):
//! that it reads no runner credential, is fail-closed on expiry, is
//! idempotent/replay-safe, and never touches item status. Each test builds
//! its own router directly from `decisions::routes(...)`, bypassing
//! production `router.rs`/`require_token` layering.

// Loaded via `#[path]` for a directly-constructed router isolated from this
// module's own production mounting. The name collision with this file's own
// module path (`decisions::decisions`) is coincidental, not nesting.
#[allow(clippy::module_inception)]
#[path = "../../src/handlers/decisions.rs"]
mod decisions;

use std::sync::{Arc, Mutex};

use crate::common;
use crate::log_capture::{CaptureGuard, ensure_global_log_capture_installed};

use axum::{Router, http::StatusCode};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use tack_core::models::{CreateItem, CreateProject, ProjectType};
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{
        ExecutionClock, NewAgentProfile, NewDecision, NewExecutionRequest, NewRunner,
        RequestSelection,
    },
};
use uuid::Uuid;

const RUNNER_ID: &str = "runner-f1";
const PROFILE_ID: &str = "profile-f1";
/// The *raw* runner credential — `setup()` stores only its SHA-256 hash
/// (`runner_credential_hash()`), matching how `agent_runners.credential_hash`
/// is actually populated in production (`runner_protocol::runner_auth`'s
/// convention). Used by `self_resolution_via_a_valid_runner_bearer_credential_is_denied`
/// to present a cryptographically real, currently-active runner credential —
/// not just an arbitrary string — so that test's claim ("even a valid
/// credential grants nothing here") is airtight rather than a strawman.
const RAW_RUNNER_CREDENTIAL: &str = "raw-f1-runner-credential-never-read-by-this-module";

fn runner_credential_hash() -> String {
    hex::encode(Sha256::digest(RAW_RUNNER_CREDENTIAL.as_bytes()))
}

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

/// Seeds a workspace/project/item, one enrolled runner, and returns enough
/// to build execution requests/attempts/decisions against.
async fn setup() -> (Repository, FakeClock, String) {
    ensure_global_log_capture_installed();
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    seed_workspace(pool).await
}

/// Same seeding as `setup`, against a caller-supplied pool — shared with the
/// concurrency race test below, which needs a real file-backed database
/// rather than this suite's usual `:memory:`.
async fn seed_workspace(pool: sqlx::SqlitePool) -> (Repository, FakeClock, String) {
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let workspace = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id,name,default_vocabulary) VALUES (?, 'F1', '{}')")
        .bind(workspace.to_string())
        .execute(repo.pool())
        .await
        .expect("workspace");
    let project = repo
        .create_project(
            workspace,
            CreateProject {
                name: "F1".into(),
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
    let credential_hash = runner_credential_hash();
    repo.register_runner(
        NewRunner {
            id: RUNNER_ID,
            name: "F1 Runner",
            credential_hash: &credential_hash,
            labels: "{}",
            total_capacity: 2,
            available_capacity: 2,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .expect("runner");
    repo.create_agent_profile(
        NewAgentProfile {
            id: PROFILE_ID,
            name: "F1 Profile",
            instructions: "work",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: "{}",
        },
        &clock,
    )
    .await
    .expect("profile");
    (repo, clock, item.id.to_string())
}

/// Enqueues a minimal, `RequestSelection::Naive`-eligible execution request
/// (bypasses the real scheduler's capability matching entirely, so no
/// harness/model capability needs to be declared — decision resolution does
/// not touch scheduling).
fn new_request<'a>(id: &'a str, item_id: &'a str, key: &'a str) -> NewExecutionRequest<'a> {
    // `claim_execution_idempotent_with_snapshot` deserializes
    // `request_snapshot` into `tack_orch::execution::ExecutionRequestSnapshot`
    // and fails the claim if it doesn't round-trip, so this must be a fully
    // well-formed snapshot — mirrors
    // `crates/tack-db/tests/repository/execution_repo.rs`'s own
    // `request()` helper template exactly (field-for-field), rather than
    // inventing a new shape here.
    let request_snapshot: &'static str = Box::leak(
        format!(
            r#"{{"request_id":"{id}","item_id":"{item_id}","idempotency_key":"{key}","created_by":{{"source":"test","subject_id":"f1-test"}},"created_at":"2026-08-12T12:00:00Z","selector":{{"kind":"exact_runner","runner_id":"{RUNNER_ID}"}},"agent_profile_id":"{PROFILE_ID}","resolved_agent_profile":{{"name":"P","instructions":"work","tool_policy":{{"mode":"safe"}},"timeout_seconds":60,"budgets":{{}}}},"requested_harness_kind":"codex","requested_model_provider":null,"requested_model_id":null,"repository":{{"kind":"git","remote":"https://example.test/f1.git","base_revision":"abc123","subdirectory":null}},"permission_policy":{{"tools":[],"network":false}},"timeout_seconds":60,"budgets":{{}},"status_map_policy_id":null,"environment":{{}},"metadata":{{}}}}"#
        )
        .into_boxed_str(),
    );
    NewExecutionRequest {
        id,
        item_id,
        idempotency_scope: "item",
        idempotency_key: key,
        request_fingerprint: key,
        selector_kind: "exact_runner",
        selector_id: RUNNER_ID,
        agent_profile_id: Some(PROFILE_ID),
        agent_profile_snapshot: r#"{"name":"P","instructions":"work","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{}}"#,
        requested_harness_kind: Some("codex"),
        requested_model_provider: None,
        requested_model_id: None,
        repository_snapshot: r#"{"kind":"git","remote":"https://example.test/f1.git","base_revision":"abc123","subdirectory":null}"#,
        permission_policy: r#"{"tools":[],"network":false}"#,
        timeout_seconds: Some(60),
        budgets: "{}",
        status_map_policy_id: None,
        environment: "{}",
        metadata: "{}",
        request_snapshot,
    }
}

/// Claims a fresh attempt (via the real production claim path, `Naive`
/// selection) and bumps it straight to `running` — the same
/// state-independent shortcut
/// `crates/tack-db/tests/repository/execution_repo.rs`'s own tests use,
/// since decision resolution does not gate on attempt state (only on the
/// decision row's own `state`/`expires_at`).
async fn claim_running_attempt(
    repo: &Repository,
    clock: &FakeClock,
    item_id: &str,
    tag: &str,
) -> String {
    let request_id = format!("req-{tag}");
    repo.enqueue_execution(new_request(&request_id, item_id, &request_id), clock)
        .await
        .expect("enqueue");
    let attempt_id = format!("att-{tag}");
    let claim = repo
        .claim_execution_idempotent_with_snapshot(
            RUNNER_ID,
            &attempt_id,
            &attempt_id,
            Duration::seconds(300),
            clock,
            RequestSelection::Naive,
        )
        .await
        .expect("claim")
        .expect("work available");
    sqlx::query("UPDATE execution_attempts SET state='running' WHERE id=?")
        .bind(&attempt_id)
        .execute(repo.pool())
        .await
        .expect("bump to running");
    claim.lease.attempt_id
}

/// Inserts a decision via the real production insert path
/// (`create_execution_decision`) rather than duplicating decision *creation*
/// logic here — this file only exercises resolution.
#[allow(clippy::too_many_arguments)]
async fn seed_decision(
    repo: &Repository,
    clock: &FakeClock,
    attempt_id: &str,
    fence: i64,
    decision_id: &str,
    options: Value,
    expires_at: Option<DateTime<Utc>>,
) {
    let options_json = serde_json::to_string(&options).unwrap();
    let row_id = format!("row-{}", Uuid::new_v4());
    let written = repo
        .create_execution_decision(
            RUNNER_ID,
            attempt_id,
            fence,
            NewDecision {
                id: &row_id,
                decision_id,
                kind: "tool_permission",
                prompt: "Allow the harness to run the focused test suite?",
                options: &options_json,
                metadata: "{}",
                expires_at,
            },
            clock,
        )
        .await
        .expect("decision insert query");
    assert!(written, "seed_decision must actually land the row");
}

fn two_options() -> Value {
    json!([
        {"option_id": "allow_once", "label": "Allow once"},
        {"option_id": "deny", "label": "Deny"},
    ])
}

async fn decision_row(repo: &Repository, attempt_id: &str, decision_id: &str) -> DecisionRow {
    let row = sqlx::query(
        "SELECT state, answer, resolved_at, resolved_by, updated_at FROM execution_decisions WHERE attempt_id=? AND decision_id=?",
    )
    .bind(attempt_id)
    .bind(decision_id)
    .fetch_one(repo.pool())
    .await
    .unwrap();
    DecisionRow {
        state: row.get("state"),
        answer: row.get("answer"),
        resolved_at: row.get("resolved_at"),
        resolved_by: row.get("resolved_by"),
        updated_at: row.get("updated_at"),
    }
}

struct DecisionRow {
    state: String,
    answer: Option<String>,
    resolved_at: Option<String>,
    resolved_by: Option<String>,
    updated_at: String,
}

async fn item_status(repo: &Repository, item_id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM items WHERE id=?")
        .bind(item_id)
        .fetch_one(repo.pool())
        .await
        .unwrap()
}

/// The test suite's configured `TACK_EXECUTION_DECISION_TOKEN` — every test
/// below that exercises real handler *behavior* (as opposed to the token
/// gate itself) presents this via [`DECISION_TOKEN`] in its header list, the
/// same way it presents `x-tack-principal` via [`OPERATOR`]. See
/// `require_decision_token`'s doc comment in `decisions.rs` for why an
/// unconfigured token fails closed.
const TEST_DECISION_TOKEN: &str = "f1-test-decision-token-never-a-real-secret";

fn app(repo: &Repository, clock: &FakeClock) -> Router {
    let state = decisions::DecisionOperatorState::with_clock(repo.clone(), Arc::new(clock.clone()))
        .with_decision_token(Some(TEST_DECISION_TOKEN.to_string()));
    decisions::routes(state)
}

/// A router built with `TACK_EXECUTION_DECISION_TOKEN` left unconfigured
/// (`None`) — the fail-closed default. Used only by the token-gate tests
/// themselves, never by a test proving ordinary resolve behavior.
fn app_without_decision_token(repo: &Repository, clock: &FakeClock) -> Router {
    let state = decisions::DecisionOperatorState::with_clock(repo.clone(), Arc::new(clock.clone()));
    decisions::routes(state)
}

fn resolve_uri(attempt_id: &str, decision_id: &str) -> String {
    format!("/attempts/{attempt_id}/decisions/{decision_id}/resolve")
}

/// Every real (non-token-gate) test presents both the operator principal
/// and the correct `TACK_EXECUTION_DECISION_TOKEN` — the token gate runs
/// first (see `resolve_decision`), so without this header every one of
/// these tests would now observe `403 forbidden` instead of whatever
/// principal-level/business-logic outcome it actually means to prove.
const OPERATOR: &[(&str, &str)] = &[
    ("x-tack-principal", "operator:local"),
    ("x-tack-decision-token", TEST_DECISION_TOKEN),
];

// ---------------------------------------------------------------------
// 1. Happy path.
// ---------------------------------------------------------------------

#[tokio::test]
async fn resolve_pending_decision_matches_operator_answer() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "happy").await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-1", two_options(), None).await;
    let app = app(&repo, &clock);

    let (status, body) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        json!({"answer": {"option_id": "allow_once", "text": null}}),
        OPERATOR,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "resolved");
    assert_eq!(body["answer"]["option_id"], "allow_once");
    assert_eq!(body["resolved_by"]["kind"], "operator");
    assert_eq!(body["resolved_by"]["subject_id"], "operator:local");
    assert_eq!(body["replayed"], false);

    let row = decision_row(&repo, &attempt_id, "dec-1").await;
    assert_eq!(row.state, "resolved");
    assert!(row.answer.is_some());
    assert!(row.resolved_by.is_some());
}

// ---------------------------------------------------------------------
// 2/3. Replay / idempotency conflict.
// ---------------------------------------------------------------------

#[tokio::test]
async fn resolving_twice_with_same_answer_is_idempotent() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "replay").await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-1", two_options(), None).await;
    let app = app(&repo, &clock);
    let answer = json!({"answer": {"option_id": "allow_once", "text": null}});

    let (status1, body1) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        answer.clone(),
        OPERATOR,
    )
    .await;
    assert_eq!(status1, StatusCode::OK);
    let first_resolved_at = body1["resolved_at"].as_str().unwrap().to_string();
    let row_after_first = decision_row(&repo, &attempt_id, "dec-1").await;

    // Advance the clock so a second, buggy write would visibly change
    // `resolved_at`/`updated_at` if one happened.
    clock.advance(Duration::seconds(30));

    let (status2, body2) =
        common::send_post_strict(&app, &resolve_uri(&attempt_id, "dec-1"), answer, OPERATOR).await;
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(body2["replayed"], true);
    assert_eq!(body2["resolved_at"], first_resolved_at);

    let row_after_second = decision_row(&repo, &attempt_id, "dec-1").await;
    assert_eq!(
        row_after_second.updated_at, row_after_first.updated_at,
        "a replayed resolve must not be a second write"
    );
    assert_eq!(row_after_second.resolved_at, row_after_first.resolved_at);
    assert_eq!(row_after_second.answer, row_after_first.answer);
}

#[tokio::test]
async fn resolving_with_different_answer_after_resolve_is_conflict() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "conflict").await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-1", two_options(), None).await;
    let app = app(&repo, &clock);

    let (status1, _) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        json!({"answer": {"option_id": "allow_once", "text": null}}),
        OPERATOR,
    )
    .await;
    assert_eq!(status1, StatusCode::OK);
    let row_after_first = decision_row(&repo, &attempt_id, "dec-1").await;

    let (status2, body2) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        json!({"answer": {"option_id": "deny", "text": null}}),
        OPERATOR,
    )
    .await;
    assert_eq!(status2, StatusCode::CONFLICT);
    assert_eq!(body2["error"]["code"], "idempotency_conflict");

    let row_after_second = decision_row(&repo, &attempt_id, "dec-1").await;
    assert_eq!(
        row_after_second.answer, row_after_first.answer,
        "the losing resolve must not overwrite the recorded answer"
    );
    assert_eq!(row_after_second.updated_at, row_after_first.updated_at);
}

// ---------------------------------------------------------------------
// 4/5. Not found / cross-attempt.
// ---------------------------------------------------------------------

#[tokio::test]
async fn cross_attempt_decision_id_is_not_found_and_writes_nothing() {
    let (repo, clock, item_id) = setup().await;
    let attempt_a = claim_running_attempt(&repo, &clock, &item_id, "cross-a").await;
    let attempt_b = claim_running_attempt(&repo, &clock, &item_id, "cross-b").await;
    seed_decision(
        &repo,
        &clock,
        &attempt_b,
        1,
        "dec-shared",
        two_options(),
        None,
    )
    .await;
    let app = app(&repo, &clock);

    // Attempt A has no decision named "dec-shared" — it exists only under B.
    let (status, body) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_a, "dec-shared"),
        json!({"answer": {"option_id": "allow_once", "text": null}}),
        OPERATOR,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");

    let row_b = decision_row(&repo, &attempt_b, "dec-shared").await;
    assert_eq!(
        row_b.state, "pending",
        "the real owning attempt's decision must be untouched by a cross-attempt attempt"
    );
    assert!(row_b.resolved_by.is_none());
    assert!(row_b.answer.is_none());
}

#[tokio::test]
async fn unknown_decision_under_a_real_attempt_is_not_found() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "unknown-dec").await;
    let app = app(&repo, &clock);

    let (status, body) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "never-existed"),
        json!({"answer": {"option_id": "allow_once", "text": null}}),
        OPERATOR,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

// ---------------------------------------------------------------------
// 6/7. Self-resolution structurally denied.
// ---------------------------------------------------------------------

#[tokio::test]
async fn missing_operator_principal_is_denied_and_writes_nothing() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "no-principal").await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-1", two_options(), None).await;
    let app = app(&repo, &clock);

    let (status, body) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        json!({"answer": {"option_id": "allow_once", "text": null}}),
        // Correct decision token, no principal — isolates the principal
        // check from the (separate, already-passing) token gate.
        &[("x-tack-decision-token", TEST_DECISION_TOKEN)],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");

    let row = decision_row(&repo, &attempt_id, "dec-1").await;
    assert_eq!(row.state, "pending");
    assert!(row.resolved_by.is_none());
}

/// The sharpest form of "a runner may never resolve its own decision": this
/// module never reads `Authorization` at all (see `decisions.rs`'s module
/// doc comment), so presenting a real, currently-valid runner bearer
/// credential grants no privilege whatsoever here — it is simply never
/// looked at. Only `x-tack-principal` (settable exclusively by the
/// operator-only `inject_operator_principal` middleware once this router is
/// mounted) is consulted, and it is absent here, so this is rejected exactly
/// like any other unauthenticated request.
#[tokio::test]
async fn valid_runner_credential_cannot_self_resolve_decision() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "self-resolve").await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-1", two_options(), None).await;
    let app = app(&repo, &clock);

    let auth_header = format!("Bearer {RAW_RUNNER_CREDENTIAL}");
    let (status, body) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        json!({"answer": {"option_id": "allow_once", "text": null}}),
        // Correct decision token (proving the token gate alone is not what
        // stops this credential) plus the runner's own bearer credential in
        // `authorization` instead of `x-tack-principal`.
        &[
            ("x-tack-decision-token", TEST_DECISION_TOKEN),
            ("authorization", auth_header.as_str()),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");

    let row = decision_row(&repo, &attempt_id, "dec-1").await;
    assert_eq!(
        row.state, "pending",
        "a runner credential must never resolve its own decision"
    );
    assert!(row.resolved_by.is_none());
}

// ---------------------------------------------------------------------
// 8/9. Fail-closed expiry.
// ---------------------------------------------------------------------

#[tokio::test]
async fn expiry_denies_with_audit_and_never_marks_item_done() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "expiry").await;
    let expires_at = clock.now() - Duration::seconds(1); // already overdue
    seed_decision(
        &repo,
        &clock,
        &attempt_id,
        1,
        "dec-1",
        two_options(),
        Some(expires_at),
    )
    .await;
    let status_before = item_status(&repo, &item_id).await;
    let app = app(&repo, &clock);

    // A syntactically valid, option-matching "allow" answer — proves expiry
    // wins even against an answer that would otherwise have succeeded.
    let (status, body) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        json!({"answer": {"option_id": "allow_once", "text": null}}),
        OPERATOR,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "decision_expired");

    let row = decision_row(&repo, &attempt_id, "dec-1").await;
    assert_eq!(row.state, "expired");
    assert!(
        row.answer.is_none(),
        "an expired decision must never carry the operator's late answer"
    );
    assert!(row.resolved_at.is_some());
    let resolved_by: Value = serde_json::from_str(&row.resolved_by.unwrap()).unwrap();
    assert_eq!(resolved_by["kind"], "system");
    assert_eq!(resolved_by["subject_id"], "expiry");

    assert_eq!(
        item_status(&repo, &item_id).await,
        status_before,
        "expiry must never change item status"
    );
}

#[tokio::test]
async fn bulk_sweep_denies_only_overdue_pending_decisions() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "sweep").await;
    let overdue = clock.now() - Duration::seconds(5);
    let future = clock.now() + Duration::seconds(5);
    let rows = [
        ("dec-overdue-1", Some(overdue), "expired"),
        ("dec-overdue-2", Some(overdue), "expired"),
        ("dec-future", Some(future), "pending"),
        ("dec-no-expiry", None, "pending"),
    ];
    for (decision_id, expires_at, _) in rows {
        seed_decision(
            &repo,
            &clock,
            &attempt_id,
            1,
            decision_id,
            two_options(),
            expires_at,
        )
        .await;
    }
    let status_before = item_status(&repo, &item_id).await;

    let affected = decisions::expire_overdue_decisions(repo.pool(), clock.now())
        .await
        .expect("sweep");
    assert_eq!(affected, 2);

    for (decision_id, _, expected_state) in rows {
        assert_eq!(
            decision_row(&repo, &attempt_id, decision_id).await.state,
            expected_state,
            "{decision_id}"
        );
    }
    assert_eq!(item_status(&repo, &item_id).await, status_before);
}

// ---------------------------------------------------------------------
// 11. Restart preserves pending state.
// ---------------------------------------------------------------------

#[tokio::test]
async fn restart_preserves_pending_decision_still_resolvable() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "restart").await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-a", two_options(), None).await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-b", two_options(), None).await;

    {
        // "Process instance #1" — dropped at the end of this block without
        // resolving anything.
        let _first_app = app(&repo, &clock);
    }

    // "Process instance #2" — a brand-new Router/state built from the same
    // underlying pool, simulating a restart. No in-memory handler state
    // could have survived; only what is in SQLite can.
    let second_app = app(&repo, &clock);
    let (status, body) = common::send_post_strict(
        &second_app,
        &resolve_uri(&attempt_id, "dec-a"),
        json!({"answer": {"option_id": "allow_once", "text": null}}),
        OPERATOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "resolved");

    // The still-untouched decision also survived and is independently
    // queryable/resolvable through the new instance.
    let row_b = decision_row(&repo, &attempt_id, "dec-b").await;
    assert_eq!(row_b.state, "pending");
    let (status_b, _) = common::send_post_strict(
        &second_app,
        &resolve_uri(&attempt_id, "dec-b"),
        json!({"answer": {"option_id": "deny", "text": null}}),
        OPERATOR,
    )
    .await;
    assert_eq!(status_b, StatusCode::OK);
}

// ---------------------------------------------------------------------
// 12/13/14/15. Request validation.
// ---------------------------------------------------------------------

#[tokio::test]
async fn invalid_answer_shapes_are_rejected_and_write_nothing() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "validation").await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-1", two_options(), None).await;
    let app = app(&repo, &clock);

    for bad_body in [
        json!({}),
        json!({"answer": "not-an-object"}),
        json!({"answer": {"option_id": ""}}),
        json!({"answer": {"option_id": "allow_once", "text": 5}}),
    ] {
        let (status, body) =
            common::send_post_strict(&app, &resolve_uri(&attempt_id, "dec-1"), bad_body, OPERATOR)
                .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "invalid_request");
    }

    let row = decision_row(&repo, &attempt_id, "dec-1").await;
    assert_eq!(row.state, "pending");
}

#[tokio::test]
async fn answer_option_id_must_match_declared_options() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "option-check").await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-1", two_options(), None).await;
    let app = app(&repo, &clock);

    let (status, body) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        json!({"answer": {"option_id": "bogus_choice", "text": null}}),
        OPERATOR,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");

    let row = decision_row(&repo, &attempt_id, "dec-1").await;
    assert_eq!(row.state, "pending");
}

#[tokio::test]
async fn freeform_decision_accepts_any_non_empty_option_id() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "freeform").await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-1", json!([]), None).await;
    let app = app(&repo, &clock);

    let (status, body) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        json!({"answer": {"option_id": "custom-token", "text": "free text"}}),
        OPERATOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["answer"]["option_id"], "custom-token");
}

#[tokio::test]
async fn answer_over_byte_limit_is_payload_too_large() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "oversize").await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-1", json!([]), None).await;
    let app = app(&repo, &clock);

    let huge_text = "x".repeat(40_000);
    let (status, body) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        json!({"answer": {"option_id": "allow_once", "text": huge_text}}),
        OPERATOR,
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(body["error"]["code"], "payload_too_large");

    let row = decision_row(&repo, &attempt_id, "dec-1").await;
    assert_eq!(row.state, "pending");
}

// ---------------------------------------------------------------------
// 16. Redaction: logs carry ids only.
// ---------------------------------------------------------------------

#[tokio::test]
async fn logs_never_contain_the_raw_answer_text_or_prompt_only_ids() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "redaction").await;
    let secret_text = "UNIQUE_SENTINEL_ANSWER_TEXT_never_in_logs_9f3a";
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-1", json!([]), None).await;
    let app = app(&repo, &clock);

    let (guard, buffer) = CaptureGuard::start();
    let (status, _) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        json!({"answer": {"option_id": "allow_once", "text": secret_text}}),
        OPERATOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    drop(guard);

    let log_text = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(
        !log_text.contains(secret_text),
        "log output must never contain the raw answer text: {log_text}"
    );
    assert!(
        !log_text.contains("Allow the harness to run the focused test suite"),
        "log output must never contain the decision prompt: {log_text}"
    );
    assert!(
        log_text.contains(&attempt_id),
        "log output should still carry the attempt id"
    );
    assert!(
        log_text.contains("dec-1"),
        "log output should still carry the decision id"
    );
}

// ---------------------------------------------------------------------
// 18/19/20. TACK_EXECUTION_DECISION_TOKEN is fail-closed, distinct from and
// layered on top of the ordinary operator principal gate.
// ---------------------------------------------------------------------

#[tokio::test]
async fn unconfigured_decision_token_rejects_every_resolve() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "no-token-configured").await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-1", two_options(), None).await;
    // No `TACK_EXECUTION_DECISION_TOKEN` configured on this server at all —
    // the fail-closed default (see `require_decision_token`'s doc comment:
    // "no secret configured" must never mean "anyone holding the ordinary
    // API token can").
    let app = app_without_decision_token(&repo, &clock);

    // Even a perfectly well-formed operator principal cannot help here —
    // the token gate runs first and there is no token to ever satisfy.
    let (status, body) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        json!({"answer": {"option_id": "allow_once", "text": null}}),
        OPERATOR,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");
    assert_eq!(
        body["error"]["details"]["required_scope"],
        "operator:decisions"
    );

    let row = decision_row(&repo, &attempt_id, "dec-1").await;
    assert_eq!(row.state, "pending");
    assert!(row.resolved_by.is_none());
    assert!(row.answer.is_none());
}

#[tokio::test]
async fn wrong_decision_token_rejects_the_resolve() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "wrong-token").await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-1", two_options(), None).await;
    let app = app(&repo, &clock);

    let (status, body) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        json!({"answer": {"option_id": "allow_once", "text": null}}),
        &[
            ("x-tack-principal", "operator:local"),
            (
                "x-tack-decision-token",
                "definitely-not-the-configured-token",
            ),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");
    assert_eq!(
        body["error"]["details"]["required_scope"],
        "operator:decisions"
    );

    let row = decision_row(&repo, &attempt_id, "dec-1").await;
    assert_eq!(
        row.state, "pending",
        "a wrong decision token must never resolve the decision"
    );
    assert!(row.resolved_by.is_none());
    assert!(row.answer.is_none());
}

#[tokio::test]
async fn correct_decision_token_with_valid_principal_resolves() {
    let (repo, clock, item_id) = setup().await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "correct-token").await;
    seed_decision(&repo, &clock, &attempt_id, 1, "dec-1", two_options(), None).await;
    let app = app(&repo, &clock);

    // `OPERATOR` carries both the operator principal and the exact
    // `TEST_DECISION_TOKEN` the router above was constructed with.
    let (status, body) = common::send_post_strict(
        &app,
        &resolve_uri(&attempt_id, "dec-1"),
        json!({"answer": {"option_id": "allow_once", "text": null}}),
        OPERATOR,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "resolved");

    let row = decision_row(&repo, &attempt_id, "dec-1").await;
    assert_eq!(row.state, "resolved");
    assert!(row.resolved_by.is_some());
    assert!(row.answer.is_some());
}

// ---------------------------------------------------------------------
// 17. Concurrency: BEGIN IMMEDIATE serializes competing resolves.
//
// Modeled directly on
// `crates/tack-db/tests/repository/execution_repo.rs`'s
// `artifact_and_decision_cannot_land_against_concurrently_terminal_attempt`:
// a manually-held `BEGIN IMMEDIATE` transaction forces both racing resolves
// to queue behind it before either can even read the row, closing the
// "who reaches SQLite first" nondeterminism `join!`'s poll order alone
// cannot close. Uses a genuine file-backed database (not this suite's
// `:memory:` elsewhere) — see CLAUDE.md's own note that the in-memory
// harness can mask exactly this class of race.
//
// Manually verified load-bearing: temporarily changing
// `resolve_decision_row`'s `"BEGIN IMMEDIATE"` to `"BEGIN"` and re-running
// only this test reproduces the race and fails, before reverting back to
// the committed `BEGIN IMMEDIATE` form below.
// ---------------------------------------------------------------------

#[tokio::test]
async fn concurrent_resolves_serialize_to_exactly_one_winner() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let db_path = dir.path().join("resolve-race.db");
    let pool = init_pool(&format!("sqlite://{}?mode=rwc", db_path.display()))
        .await
        .expect("file-backed pool");
    let (repo, clock, item_id) = seed_workspace(pool).await;
    let attempt_id = claim_running_attempt(&repo, &clock, &item_id, "race").await;
    seed_decision(
        &repo,
        &clock,
        &attempt_id,
        1,
        "dec-race",
        two_options(),
        None,
    )
    .await;

    let mut manual_tx = repo.pool().begin_with("BEGIN IMMEDIATE").await.unwrap();
    // Touch an unrelated row so the lock is real without pre-empting either
    // racer's own view of the decision row's `pending` state.
    sqlx::query("UPDATE agent_runners SET updated_at=updated_at WHERE id=?")
        .bind(RUNNER_ID)
        .execute(&mut *manual_tx)
        .await
        .unwrap();

    let allow_answer = json!({"option_id": "allow_once", "text": null});
    let deny_answer = json!({"option_id": "deny", "text": null});
    let operator_a = json!({"kind": "operator", "subject_id": "operator-a"});
    let operator_b = json!({"kind": "operator", "subject_id": "operator-b"});
    let now = clock.now();

    let resolve_a = decisions::resolve_decision_row(
        repo.pool(),
        &attempt_id,
        "dec-race",
        &allow_answer,
        &operator_a,
        now,
    );
    let resolve_b = decisions::resolve_decision_row(
        repo.pool(),
        &attempt_id,
        "dec-race",
        &deny_answer,
        &operator_b,
        now,
    );
    let delayed_release = async {
        for _ in 0..512 {
            tokio::task::yield_now().await;
        }
        manual_tx.commit().await.unwrap();
    };

    let (outcome_a, outcome_b, _) = tokio::join!(resolve_a, resolve_b, delayed_release);
    let outcome_a = outcome_a.expect("resolve A must not error under BEGIN IMMEDIATE");
    let outcome_b = outcome_b.expect("resolve B must not error under BEGIN IMMEDIATE");

    let final_row = decision_row(&repo, &attempt_id, "dec-race").await;
    assert_eq!(final_row.state, "resolved");
    let final_answer: Value = serde_json::from_str(&final_row.answer.unwrap()).unwrap();

    // Exactly one of the two calls actually won the write; the other must
    // have observed the (by-then-committed) resolved row and reported a
    // conflict, never a silent second write.
    let outcomes = [outcome_a, outcome_b];
    let resolved_count = outcomes
        .iter()
        .filter(|o| {
            matches!(
                o,
                decisions::ResolveOutcome::Resolved {
                    replayed: false,
                    ..
                }
            )
        })
        .count();
    let conflict_count = outcomes
        .iter()
        .filter(|o| matches!(o, decisions::ResolveOutcome::IdempotencyConflict { .. }))
        .count();
    assert_eq!(
        resolved_count, 1,
        "exactly one racer must land the fresh write: {outcomes:?}"
    );
    assert_eq!(
        conflict_count, 1,
        "the loser must observe idempotency_conflict, not silently succeed: {outcomes:?}"
    );
    assert!(
        final_answer["option_id"] == allow_answer["option_id"]
            || final_answer["option_id"] == deny_answer["option_id"]
    );
}
