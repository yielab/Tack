use std::sync::Mutex;

use crate::common;
use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use tack_core::{
    models::{CreateItem, CreateProject, ItemType, Priority, ProjectType},
    vocabulary,
};
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{
        ClaimedExecution, Completion, EnrollmentToken, EventApplyResult, EventBatch,
        ExecutionClock, HeartbeatBatchResult, HeartbeatLease, NewAgentProfile, NewEvent,
        NewExecutionRequest, NewRunner, RecoveryDisposition, RecoveryObservation,
        RecoveryObservationInput, RecoveryObservationResult, RedeemEnrollmentResult,
        RequestSelection,
    },
};
use uuid::Uuid;

struct FakeClock(Mutex<DateTime<Utc>>);

impl FakeClock {
    fn new() -> Self {
        Self(Mutex::new(
            DateTime::parse_from_rfc3339("2026-08-07T12:00:00Z")
                .expect("fixed timestamp")
                .with_timezone(&Utc),
        ))
    }
}

impl ExecutionClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().expect("clock lock")
    }
}

struct Fixture {
    repo: Repository,
    item_id: String,
    clock: FakeClock,
}

async fn fixture() -> Fixture {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let workspace_id = Uuid::new_v4();
    let default_vocabulary =
        serde_json::to_string(&vocabulary::default_vocabulary()).expect("vocabulary JSON");
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'Crash Matrix', ?)",
    )
    .bind(workspace_id.to_string())
    .bind(default_vocabulary)
    .execute(repo.pool())
    .await
    .expect("workspace");
    let project = repo
        .create_project(
            workspace_id,
            CreateProject {
                name: "Crash Matrix".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .expect("project");
    let status = project
        .workflow
        .initial_status()
        .expect("initial status")
        .to_owned();
    let item = repo
        .create_item(
            project.id,
            &status,
            CreateItem {
                title: "Crash-boundary work".into(),
                description: None,
                item_type: Some(ItemType::Task),
                parent_id: None,
                priority: Some(Priority::Medium),
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
    let clock = FakeClock::new();
    repo.register_runner(
        NewRunner {
            id: "runner-crash",
            name: "Crash Matrix Runner",
            credential_hash: "test-hash-only",
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
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-crash",
            name: "Crash Matrix Profile",
            instructions: "exercise failure boundaries",
            tool_policy: "{}",
            limits: "{}",
        },
        &clock,
    )
    .await
    .expect("profile");
    Fixture {
        repo,
        item_id: item.id.to_string(),
        clock,
    }
}

fn request<'a>(item_id: &'a str, request_snapshot: &'a str) -> NewExecutionRequest<'a> {
    NewExecutionRequest {
        id: "request-crash",
        item_id,
        idempotency_scope: "crash-matrix",
        idempotency_key: "request-1",
        request_fingerprint: "request-1-fingerprint",
        selector_kind: "exact_runner",
        selector_id: "runner-crash",
        agent_profile_id: Some("profile-crash"),
        agent_profile_snapshot: r#"{"name":"Crash Matrix Profile","instructions":"exercise failure boundaries","tool_policy":{},"budgets":{},"timeout_seconds":60}"#,
        requested_harness_kind: Some("mock"),
        requested_model_provider: None,
        requested_model_id: None,
        repository_snapshot: r#"{"kind":"git","remote":"https://example.invalid/repository.git","base_revision":"0123456789abcdef","subdirectory":null}"#,
        permission_policy: r#"{"tools":[],"network":false}"#,
        timeout_seconds: Some(60),
        budgets: "{}",
        status_map_policy_id: None,
        environment: "{}",
        metadata: "{}",
        request_snapshot,
    }
}

fn request_snapshot(item_id: &str, clock: &FakeClock) -> String {
    serde_json::json!({
        "request_id": "request-crash",
        "item_id": item_id,
        "idempotency_key": "request-1",
        "agent_profile_id": "profile-crash",
        "requested_harness_kind": "mock",
        "requested_model_provider": null,
        "requested_model_id": null,
        "created_by": {"source": "test", "subject_id": "crash-matrix"},
        "created_at": clock.now().to_rfc3339(),
        "selector": {"kind": "exact_runner", "runner_id": "runner-crash"},
        "resolved_agent_profile": {
            "name": "Crash Matrix Profile", "instructions": "exercise failure boundaries",
            "tool_policy": {}, "budgets": {}, "timeout_seconds": 60
        },
        "repository": {
            "kind": "git", "remote": "https://example.invalid/repository.git",
            "base_revision": "0123456789abcdef", "subdirectory": null
        },
        "permission_policy": {"tools": [], "network": false},
        "timeout_seconds": 60,
        "budgets": {}, "status_map_policy_id": null,
        "environment": {}, "metadata": {}
    })
    .to_string()
}

async fn enqueue(fixture: &Fixture) {
    let snapshot = request_snapshot(&fixture.item_id, &fixture.clock);
    fixture
        .repo
        .enqueue_execution(request(&fixture.item_id, &snapshot), &fixture.clock)
        .await
        .expect("enqueue");
}

/// `claim_execution_idempotent_with_snapshot` against the fixed `runner-crash`, varying
/// only the claim/attempt ids so a test can claim more than once without hard-coding the
/// other four arguments at every call site.
async fn try_claim(
    fixture: &Fixture,
    claim_request_id: &str,
    attempt_id: &str,
) -> Result<Option<ClaimedExecution>, sqlx::Error> {
    fixture
        .repo
        .claim_execution_idempotent_with_snapshot(
            "runner-crash",
            claim_request_id,
            attempt_id,
            Duration::seconds(60),
            &fixture.clock,
            RequestSelection::Naive,
        )
        .await
}

#[tokio::test]
async fn claim_crash_rolls_back_then_retry_keeps_fence_at_one() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    sqlx::query(
        "CREATE TRIGGER inject_claim_crash BEFORE INSERT ON execution_attempts BEGIN \
         SELECT RAISE(ABORT, 'injected claim crash'); END",
    )
    .execute(fixture.repo.pool())
    .await
    .expect("install trigger");

    let crashed = try_claim(&fixture, "claim-crash", "attempt-crash").await;
    assert!(crashed.is_err(), "fault injection must reach claim commit");

    let request_state: String =
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = 'request-crash'")
            .fetch_one(fixture.repo.pool())
            .await
            .expect("request state");
    let capacity: i64 = sqlx::query_scalar(
        "SELECT available_capacity FROM agent_runners WHERE id = 'runner-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("capacity");
    let attempts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_attempts")
        .fetch_one(fixture.repo.pool())
        .await
        .expect("attempt count");
    assert_eq!(request_state, "queued");
    assert_eq!(capacity, 1);
    assert_eq!(attempts, 0);

    sqlx::query("DROP TRIGGER inject_claim_crash")
        .execute(fixture.repo.pool())
        .await
        .expect("drop trigger");
    let lease = try_claim(&fixture, "claim-after-crash", "attempt-after-retry")
        .await
        .expect("claim retry")
        .expect("lease after retry");
    assert_eq!(lease.lease.fencing_token, 1, "rollback cannot burn a fence");
}

#[tokio::test]
async fn ambiguous_recovery_needs_operator_and_blocks_reclaim() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    let recovery = fixture
        .repo
        .recover_attempt(
            RecoveryObservationInput {
                runner_id: "runner-crash",
                attempt_id: "attempt-crash",
                fencing_token: 1,
                recovery_key: "recovery:attempt-crash:1:ambiguous",
                observation: RecoveryObservation::Ambiguous,
                details: r#"{"journal_state":"process_observed_running","process_observed":true}"#,
            },
            &fixture.clock,
        )
        .await
        .expect("record recovery");
    assert!(matches!(
        recovery,
        RecoveryObservationResult::Applied(ref response)
            if response.disposition == RecoveryDisposition::NeedsOperator
    ));
    let state: String =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = 'attempt-crash'")
            .fetch_one(fixture.repo.pool())
            .await
            .expect("attempt state");
    assert_eq!(state, "needs_operator");
    assert!(
        try_claim(
            &fixture,
            "claim-invalid-second-fence",
            "attempt-invalid-second-fence"
        )
        .await
        .expect("second claim query")
        .is_none(),
        "ambiguous side effects must never be blind-retried"
    );
    let fences: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT fencing_token) FROM execution_attempts WHERE request_id = 'request-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("fence count");
    assert_eq!(fences, 1);
}

/// Fixed two-event batch shared by the event-batch tests below; only the crash trigger
/// (or its absence) varies between them.
fn crash_events(fixture: &Fixture) -> [NewEvent<'static>; 2] {
    [
        NewEvent {
            id: "event-row-1",
            event_id: "event-1",
            sequence: 1,
            source: "runner",
            kind: "progress",
            payload: r#"{"phase":"spawn"}"#,
            occurred_at: fixture.clock.now(),
        },
        NewEvent {
            id: "event-row-2",
            event_id: "event-2",
            sequence: 2,
            source: "runner",
            kind: "progress",
            payload: r#"{"phase":"running"}"#,
            occurred_at: fixture.clock.now(),
        },
    ]
}

fn crash_event_batch() -> EventBatch<'static> {
    EventBatch {
        runner_id: "runner-crash",
        attempt_id: "attempt-crash",
        fencing_token: 1,
        previous_checkpoint: None,
        checkpoint: "checkpoint-1",
    }
}

async fn event_checkpoint_state(fixture: &Fixture) -> SqliteRow {
    sqlx::query(
        "SELECT event_checkpoint, (SELECT COUNT(*) FROM execution_events) AS event_count \
         FROM execution_attempts WHERE id = 'attempt-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("checkpoint state")
}

/// One crash-injection point for the event-batch fault matrix: where the trigger fires
/// and how to remove it once the rolled-back state has been checked.
struct EventBatchFault {
    label: &'static str,
    install: &'static str,
    remove: &'static str,
}

const EVENT_BATCH_FAULTS: &[EventBatchFault] = &[
    EventBatchFault {
        label: "second_event_insert",
        install: "CREATE TRIGGER inject_second_event_crash BEFORE INSERT ON execution_events \
                   WHEN NEW.sequence = 2 BEGIN SELECT RAISE(ABORT, 'injected second event crash'); END",
        remove: "DROP TRIGGER inject_second_event_crash",
    },
    EventBatchFault {
        label: "checkpoint_update",
        install: "CREATE TRIGGER inject_checkpoint_crash BEFORE UPDATE OF event_checkpoint \
                   ON execution_attempts BEGIN SELECT RAISE(ABORT, 'injected checkpoint crash'); END",
        remove: "DROP TRIGGER inject_checkpoint_crash",
    },
];

async fn assert_event_batch_fault_rolls_back(fault: &EventBatchFault) {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    sqlx::query(fault.install)
        .execute(fixture.repo.pool())
        .await
        .expect("install trigger");
    let events = crash_events(&fixture);
    assert!(
        fixture
            .repo
            .append_execution_events_result(crash_event_batch(), &events, &fixture.clock)
            .await
            .is_err(),
        "{}",
        fault.label
    );
    let row = event_checkpoint_state(&fixture).await;
    assert_eq!(
        row.get::<Option<String>, _>("event_checkpoint"),
        None,
        "{}",
        fault.label
    );
    assert_eq!(row.get::<i64, _>("event_count"), 0, "{}", fault.label);
    sqlx::query(fault.remove)
        .execute(fixture.repo.pool())
        .await
        .expect("drop trigger");
}

#[tokio::test]
async fn event_batch_fault_leaves_no_partial_write() {
    for fault in EVENT_BATCH_FAULTS {
        assert_event_batch_fault_rolls_back(fault).await;
    }
}

#[tokio::test]
async fn event_batch_replay_writes_events_exactly_once() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    let events = crash_events(&fixture);
    assert!(matches!(
        fixture
            .repo
            .append_execution_events_result(crash_event_batch(), &events, &fixture.clock)
            .await
            .expect("first report"),
        EventApplyResult::Applied(ref result) if !result.replayed
    ));
    assert!(matches!(
        fixture
            .repo
            .append_execution_events_result(crash_event_batch(), &events, &fixture.clock)
            .await
            .expect("replay"),
        EventApplyResult::Applied(ref result) if result.replayed
    ));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_events")
        .fetch_one(fixture.repo.pool())
        .await
        .expect("event count");
    assert_eq!(count, 2);
}

fn completion_crash() -> Completion<'static> {
    Completion {
        runner_id: "runner-crash",
        attempt_id: "attempt-crash",
        fencing_token: 1,
        completion_id: "completion-1",
        final_event_checkpoint: None,
        terminal_state: "succeeded",
        terminal_reason: "completed",
        actual_execution: "{}",
        usage: "{}",
    }
}

#[tokio::test]
async fn completion_crash_rolls_back_attempt_and_request() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    sqlx::query(
        "CREATE TRIGGER inject_completion_crash BEFORE UPDATE OF state ON execution_requests \
         WHEN NEW.state = 'succeeded' BEGIN SELECT RAISE(ABORT, 'injected completion crash'); END",
    )
    .execute(fixture.repo.pool())
    .await
    .expect("install trigger");

    assert!(
        fixture
            .repo
            .complete_execution(completion_crash(), &fixture.clock)
            .await
            .is_err()
    );
    let row = sqlx::query(
        "SELECT a.state AS attempt_state, a.completion_id, r.state AS request_state \
         FROM execution_attempts a JOIN execution_requests r ON r.id = a.request_id \
         WHERE a.id = 'attempt-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("terminal state");
    assert_eq!(row.get::<String, _>("attempt_state"), "leased");
    assert_eq!(row.get::<Option<String>, _>("completion_id"), None);
    assert_eq!(row.get::<String, _>("request_state"), "leased");
}

#[tokio::test]
async fn completion_replay_is_idempotent_and_terminal_once() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    assert!(
        fixture
            .repo
            .complete_execution(completion_crash(), &fixture.clock)
            .await
            .expect("completion")
    );
    assert!(
        fixture
            .repo
            .complete_execution(completion_crash(), &fixture.clock)
            .await
            .expect("completion replay")
    );
    let terminal_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_attempts WHERE completion_id = 'completion-1' AND state = 'succeeded'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("terminal count");
    assert_eq!(terminal_rows, 1);
}

#[tokio::test]
async fn cancellation_crash_retries_without_false_terminal_state() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    sqlx::query(
        "CREATE TRIGGER inject_cancel_crash BEFORE UPDATE OF cancellation_requested_at \
         ON execution_requests BEGIN SELECT RAISE(ABORT, 'injected cancellation crash'); END",
    )
    .execute(fixture.repo.pool())
    .await
    .expect("install trigger");

    assert!(
        fixture
            .repo
            .request_execution_cancellation("request-crash", &fixture.clock)
            .await
            .is_err()
    );
    let row = sqlx::query(
        "SELECT state, cancellation_requested_at FROM execution_requests WHERE id = 'request-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("cancellation state");
    assert_eq!(row.get::<String, _>("state"), "leased");
    assert_eq!(
        row.get::<Option<String>, _>("cancellation_requested_at"),
        None
    );

    sqlx::query("DROP TRIGGER inject_cancel_crash")
        .execute(fixture.repo.pool())
        .await
        .expect("drop trigger");
    assert!(
        fixture
            .repo
            .request_execution_cancellation("request-crash", &fixture.clock)
            .await
            .expect("cancellation retry")
    );
    let row = sqlx::query(
        "SELECT state, cancellation_requested_at FROM execution_requests WHERE id = 'request-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("cancellation state after retry");
    assert_eq!(row.get::<String, _>("state"), "leased");
    assert!(
        row.get::<Option<String>, _>("cancellation_requested_at")
            .is_some(),
        "requesting cancellation must not pretend the attempt is terminal"
    );
}

fn pending_enroll_runner() -> NewRunner<'static> {
    NewRunner {
        id: "runner-enroll-crash",
        name: "Pending crash runner",
        credential_hash: "ignored-for-pending",
        labels: "{}",
        total_capacity: 1,
        available_capacity: 1,
        capability_snapshot: "{}",
        protocol_version: 1,
    }
}

fn pending_enroll_token(clock: &FakeClock) -> EnrollmentToken<'static> {
    EnrollmentToken {
        id: "token-enroll-crash",
        runner_id: "runner-enroll-crash",
        token_hash: "hash:enroll-crash",
        expires_at: clock.now() + Duration::minutes(5),
    }
}

#[tokio::test]
async fn enrollment_redeem_has_one_winner_and_keeps_hash_only_token() {
    let fixture = fixture().await;
    fixture
        .repo
        .create_pending_runner_and_issue_token(
            pending_enroll_runner(),
            pending_enroll_token(&fixture.clock),
            &fixture.clock,
        )
        .await
        .expect("pending runner and token");
    let redeem = || {
        fixture.repo.redeem_enrollment_token(
            "hash:enroll-crash",
            "credential-hash-only",
            fixture.clock.now() + Duration::hours(1),
            "test-runner",
            "Enrolled crash runner",
            "{}",
            1,
            1,
            "{}",
            1,
            &fixture.clock,
        )
    };
    let (left, right) = tokio::join!(redeem(), redeem());
    let winners = [
        left.expect("left redemption"),
        right.expect("right redemption"),
    ]
    .into_iter()
    .filter(|result| matches!(result, RedeemEnrollmentResult::Redeemed(_)))
    .count();
    assert_eq!(
        winners, 1,
        "only one concurrent redemption can consume the token"
    );

    let metadata = fixture
        .repo
        .enrollment_token_metadata("runner-enroll-crash", "token-enroll-crash")
        .await
        .expect("token metadata")
        .expect("token row");
    assert!(metadata.consumed_at.is_some());
    let token_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_enrollment_tokens WHERE token_hash = 'hash:enroll-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("hash-only token row");
    assert_eq!(token_rows, 1);
}

fn crash_heartbeat_lease() -> [HeartbeatLease<'static>; 1] {
    [HeartbeatLease {
        attempt_id: "attempt-crash",
        fencing_token: 1,
        state: "running",
        journal_state: "process_observed_running",
        last_event_checkpoint: None,
    }]
}

/// `heartbeat_batch` against the fixed `runner-crash`/`hb-crash-1` lease, varying only the
/// observed timestamp — keeping call sites to two arguments dodges rustfmt exploding the
/// underlying 7-argument call onto one line per argument.
async fn crash_heartbeat(
    fixture: &Fixture,
    observed_at: DateTime<Utc>,
) -> Result<HeartbeatBatchResult, sqlx::Error> {
    fixture
        .repo
        .heartbeat_batch(
            "runner-crash",
            "hb-crash-1",
            observed_at,
            0,
            &crash_heartbeat_lease(),
            Duration::seconds(60),
            &fixture.clock,
        )
        .await
}

#[tokio::test]
async fn heartbeat_crash_rolls_back_and_writes_no_replay_row() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    sqlx::query(
        "CREATE TRIGGER inject_heartbeat_crash BEFORE UPDATE OF last_heartbeat_at \
         ON execution_attempts BEGIN SELECT RAISE(ABORT, 'injected heartbeat crash'); END",
    )
    .execute(fixture.repo.pool())
    .await
    .expect("heartbeat fault trigger");
    assert!(
        crash_heartbeat(&fixture, fixture.clock.now())
            .await
            .is_err()
    );
    let replays: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_heartbeat_replays")
        .fetch_one(fixture.repo.pool())
        .await
        .expect("no failed replay row");
    assert_eq!(replays, 0);
}

#[tokio::test]
async fn heartbeat_replay_is_idempotent_then_conflicts_on_new_state() {
    let fixture = fixture().await;
    enqueue(&fixture).await;
    common::claim_execution_for_test(&fixture.repo, &fixture.clock).await;
    let now = fixture.clock.now();
    let accepted = crash_heartbeat(&fixture, now).await.expect("heartbeat");
    let replay = crash_heartbeat(&fixture, now)
        .await
        .expect("heartbeat replay");
    assert!(matches!(accepted, HeartbeatBatchResult::Accepted(_)));
    assert!(matches!(replay, HeartbeatBatchResult::Replayed(_)));
    let conflict = crash_heartbeat(&fixture, now + Duration::seconds(1))
        .await
        .expect("heartbeat replay conflict");
    assert!(matches!(conflict, HeartbeatBatchResult::Conflict));
    let capacity: i64 = sqlx::query_scalar(
        "SELECT available_capacity FROM agent_runners WHERE id = 'runner-crash'",
    )
    .fetch_one(fixture.repo.pool())
    .await
    .expect("capacity after replay");
    assert_eq!(
        capacity, 0,
        "replay must not restore or double-write capacity"
    );
}
