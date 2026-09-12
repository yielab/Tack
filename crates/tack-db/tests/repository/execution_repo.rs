use crate::common;

use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{
        AttemptTransitionInput, AttemptTransitionPhase, AttemptTransitionResult,
        CancellationObservation, CancellationObservationInput, Completion, CompletionResult,
        CredentialRotationResult, EnqueueResult, EnrollmentToken, EventApplyResult, EventBatch,
        ExecutionClock, HeartbeatBatchResult, HeartbeatLease, NewAgentProfile, NewArtifact,
        NewDecision, NewEvent, NewExecutionRequest, NewRunner, RecoveryDisposition,
        RecoveryObservation, RecoveryObservationInput, RecoveryObservationResult,
        RedeemEnrollmentResult, RequestSelection,
    },
};

struct FakeClock(Mutex<DateTime<Utc>>);

impl FakeClock {
    fn new() -> Self {
        Self(Mutex::new(
            DateTime::parse_from_rfc3339("2026-08-07T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        ))
    }
    fn advance(&self, duration: Duration) {
        *self.0.lock().unwrap() += duration;
    }
}

#[tokio::test]
async fn enrollment_token_is_single_use_and_revocation_fails_closed() {
    let (repo, _, clock) = ready_repo().await;
    sqlx::query("UPDATE agent_runners SET state = 'pending_enrollment' WHERE id = 'runner-a'")
        .execute(repo.pool())
        .await
        .unwrap();
    repo.issue_enrollment_token(
        EnrollmentToken {
            id: "token-1",
            runner_id: "runner-a",
            token_hash: "token-hash",
            expires_at: clock.now() + Duration::minutes(5),
        },
        &clock,
    )
    .await
    .unwrap();
    assert_eq!(
        repo.redeem_enrollment_token(
            "token-hash",
            "credential-hash",
            clock.now() + Duration::days(1),
            "0.1",
            "Runner A",
            "{}",
            1,
            1,
            "{}",
            1,
            &clock
        )
        .await
        .unwrap(),
        RedeemEnrollmentResult::Redeemed("runner-a".into())
    );
    assert_eq!(
        repo.redeem_enrollment_token(
            "token-hash",
            "other",
            clock.now() + Duration::days(1),
            "0.1",
            "Runner A",
            "{}",
            1,
            1,
            "{}",
            1,
            &clock
        )
        .await
        .unwrap(),
        RedeemEnrollmentResult::InvalidOrExpired
    );
    repo.issue_enrollment_token(
        EnrollmentToken {
            id: "token-2",
            runner_id: "runner-a",
            token_hash: "revoked",
            expires_at: clock.now() + Duration::minutes(5),
        },
        &clock,
    )
    .await
    .unwrap();
    assert!(
        repo.revoke_enrollment_token("revoked", &clock)
            .await
            .unwrap()
    );
    assert_eq!(
        repo.redeem_enrollment_token(
            "revoked",
            "other",
            clock.now() + Duration::days(1),
            "0.1",
            "Runner A",
            "{}",
            1,
            1,
            "{}",
            1,
            &clock
        )
        .await
        .unwrap(),
        RedeemEnrollmentResult::InvalidOrExpired
    );
}

#[tokio::test]
async fn concurrent_enrollment_redemption_has_one_winner() {
    let (repo, _, clock) = ready_repo().await;
    repo.create_pending_runner_and_issue_token(
        NewRunner {
            id: "runner-concurrent-enrollment",
            name: "Pending runner",
            credential_hash: "ignored-for-pending",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        EnrollmentToken {
            id: "token-concurrent-enrollment",
            runner_id: "runner-concurrent-enrollment",
            token_hash: "hash-concurrent-enrollment",
            expires_at: clock.now() + Duration::minutes(5),
        },
        &clock,
    )
    .await
    .unwrap();
    let redeem = || {
        repo.redeem_enrollment_token(
            "hash-concurrent-enrollment",
            "credential-hash",
            clock.now() + Duration::hours(1),
            "0.1",
            "Concurrent runner",
            "{}",
            1,
            1,
            "{}",
            1,
            &clock,
        )
    };
    let (left, right) = tokio::join!(redeem(), redeem());
    let results = [left.unwrap(), right.unwrap()];
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, RedeemEnrollmentResult::Redeemed(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, RedeemEnrollmentResult::InvalidOrExpired))
            .count(),
        1
    );
}

#[tokio::test]
async fn claim_request_replay_returns_the_original_lease() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
        .await
        .unwrap();
    let first = repo
        .claim_execution_idempotent_with_snapshot(
            "runner-a",
            "claim-a",
            "attempt-a",
            Duration::seconds(60),
            &clock,
            RequestSelection::Naive,
        )
        .await
        .unwrap();
    let replay = repo
        .claim_execution_idempotent_with_snapshot(
            "runner-a",
            "claim-a",
            "attempt-b",
            Duration::seconds(60),
            &clock,
            RequestSelection::Naive,
        )
        .await
        .unwrap();
    assert_eq!(first, replay);
    assert!(matches!(replay, Some(claim) if claim.request_snapshot["request_id"] == "request-a"));
}

#[tokio::test]
async fn heartbeat_replays_and_recovery_is_audited_once() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
        .await
        .unwrap();
    let lease = claim_lease(
        &repo,
        "runner-a",
        "attempt-a",
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap()
    .unwrap();
    let leases = [HeartbeatLease {
        attempt_id: "attempt-a",
        fencing_token: lease.fencing_token,
        state: "leased",
        journal_state: "prepared",
        last_event_checkpoint: None,
    }];
    let sent_at = clock.now();
    assert!(matches!(
        repo.heartbeat_batch(
            "runner-a",
            "hb-1",
            sent_at,
            0,
            &leases,
            Duration::seconds(60),
            &clock
        )
        .await
        .unwrap(),
        HeartbeatBatchResult::Accepted(_)
    ));
    assert!(matches!(
        repo.heartbeat_batch(
            "runner-a",
            "hb-1",
            sent_at,
            0,
            &leases,
            Duration::seconds(60),
            &clock
        )
        .await
        .unwrap(),
        HeartbeatBatchResult::Replayed(_)
    ));
    clock.advance(Duration::seconds(61));
    let recovery = RecoveryObservationInput {
        runner_id: "runner-a",
        attempt_id: "attempt-a",
        fencing_token: lease.fencing_token,
        recovery_key: "recover-1",
        observation: RecoveryObservation::ProcessStopped,
        details: r#"{"journal_state":"prepared","process_observed":false}"#,
    };
    assert!(matches!(
        repo.recover_attempt(recovery.clone(), &clock)
            .await
            .unwrap(),
        RecoveryObservationResult::Applied(_)
    ));
    assert!(matches!(
        repo.recover_attempt(recovery, &clock).await.unwrap(),
        RecoveryObservationResult::Replayed(_)
    ));
    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_recovery_audits WHERE attempt_id='attempt-a'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(audits, 1);
}

#[tokio::test]
async fn structured_events_and_cancellation_observation_replay() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(request("request-z", &item_id, "key-z", "same"), &clock)
        .await
        .unwrap();
    let lease = claim_lease(
        &repo,
        "runner-a",
        "attempt-z",
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap()
    .unwrap();
    let event = NewEvent {
        id: "row-z",
        event_id: "event-z",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: "{}",
        occurred_at: clock.now(),
    };
    let batch = EventBatch {
        runner_id: "runner-a",
        attempt_id: "attempt-z",
        fencing_token: lease.fencing_token,
        previous_checkpoint: None,
        checkpoint: "checkpoint-z",
    };
    let first = repo
        .append_execution_events_result(batch.clone(), std::slice::from_ref(&event), &clock)
        .await
        .unwrap();
    let EventApplyResult::Applied(first) = first else {
        panic!("first event batch must apply");
    };
    assert_eq!(first.accepted_event_ids, vec!["event-z"]);
    let replay = repo
        .append_execution_events_result(batch, &[event], &clock)
        .await
        .unwrap();
    let EventApplyResult::Applied(replay) = replay else {
        panic!("matching event batch must replay");
    };
    assert_eq!(replay.accepted_event_ids, vec!["event-z"]);
    assert!(replay.replayed);
    assert!(
        repo.request_execution_cancellation("request-z", &clock)
            .await
            .unwrap()
    );
    assert!(matches!(
        repo.observe_cancellation(
            CancellationObservationInput {
                runner_id: "runner-a",
                attempt_id: "attempt-z",
                fencing_token: lease.fencing_token,
                cancellation_request_id: "cancel-z",
                observed_at: clock.now(),
                details: "{}",
                observation: r#""process_stopped""#,
            },
            &clock
        )
        .await
        .unwrap(),
        CancellationObservation::Cancelled(_)
    ));
    assert!(matches!(
        repo.observe_cancellation(
            CancellationObservationInput {
                runner_id: "runner-a",
                attempt_id: "attempt-z",
                fencing_token: lease.fencing_token,
                cancellation_request_id: "cancel-z",
                observed_at: clock.now(),
                details: "{}",
                observation: r#""process_stopped""#,
            },
            &clock
        )
        .await
        .unwrap(),
        CancellationObservation::Replayed(_)
    ));
}

#[tokio::test]
async fn event_replay_canonicalizes_equivalent_json_payloads() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(
        request("request-canonical", &item_id, "key-canonical", "same"),
        &clock,
    )
    .await
    .unwrap();
    let lease = claim_lease(
        &repo,
        "runner-a",
        "attempt-canonical",
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap()
    .unwrap();
    let batch = EventBatch {
        runner_id: "runner-a",
        attempt_id: "attempt-canonical",
        fencing_token: lease.fencing_token,
        previous_checkpoint: None,
        checkpoint: "checkpoint-canonical",
    };
    let initial = NewEvent {
        id: "row-canonical",
        event_id: "event-canonical",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: r#"{"outer":{"b":2,"a":1},"z":0}"#,
        occurred_at: clock.now(),
    };
    let replay = NewEvent {
        id: "row-canonical-retry",
        payload: r#"{"z":0,"outer":{"a":1,"b":2}}"#,
        ..initial
    };
    assert!(matches!(
        repo.append_execution_events_result(batch.clone(), &[initial], &clock)
            .await
            .unwrap(),
        EventApplyResult::Applied(result) if !result.replayed
    ));
    assert!(matches!(
        repo.append_execution_events_result(batch, &[replay], &clock)
            .await
            .unwrap(),
        EventApplyResult::Applied(result) if result.replayed
    ));
}

// Defect 1 regression: a reused (attempt_id, checkpoint) idempotency-scoped
// key with different content must be the non-retryable `IdempotencyConflict`
// — never the benign, retryable `Conflict` a runner is expected to retry
// forever against a request that can never succeed. Contrasted in the same
// test against the genuinely benign out-of-order case (stale
// previous_checkpoint), which must remain `Conflict`.
#[tokio::test]
async fn event_replay_changed_payload_is_idempotency_conflict() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(
        request("request-conflict", &item_id, "key-conflict", "same"),
        &clock,
    )
    .await
    .unwrap();
    let lease = claim_lease(
        &repo,
        "runner-a",
        "attempt-conflict",
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap()
    .unwrap();
    let batch = EventBatch {
        runner_id: "runner-a",
        attempt_id: "attempt-conflict",
        fencing_token: lease.fencing_token,
        previous_checkpoint: None,
        checkpoint: "checkpoint-conflict",
    };
    let initial = NewEvent {
        id: "row-conflict",
        event_id: "event-conflict",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: r#"{"state":"original"}"#,
        occurred_at: clock.now(),
    };
    let changed = NewEvent {
        id: "row-conflict-retry",
        payload: r#"{"state":"changed"}"#,
        ..initial
    };
    repo.append_execution_events_result(batch.clone(), &[initial], &clock)
        .await
        .unwrap();
    // Same checkpoint (idempotency-scoped key), different event payload:
    // fingerprint mismatch. Non-retryable.
    assert_eq!(
        repo.append_execution_events_result(batch, &[changed], &clock)
            .await
            .unwrap(),
        EventApplyResult::IdempotencyConflict
    );
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id = 'attempt-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let checkpoint: Option<String> = sqlx::query_scalar(
        "SELECT event_checkpoint FROM execution_attempts WHERE id = 'attempt-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(event_count, 1);
    assert_eq!(checkpoint.as_deref(), Some("checkpoint-conflict"));

    // Contrast: a fresh checkpoint whose previous_checkpoint claim no longer
    // matches the attempt's actual stream position is a benign out-of-order
    // resync, not an idempotency-key reuse. Retryable — must stay `Conflict`,
    // and must remain distinct from the `IdempotencyConflict` case above.
    let out_of_order = EventBatch {
        runner_id: "runner-a",
        attempt_id: "attempt-conflict",
        fencing_token: lease.fencing_token,
        previous_checkpoint: Some("not-the-current-checkpoint"),
        checkpoint: "checkpoint-conflict-2",
    };
    let next = NewEvent {
        id: "row-conflict-next",
        event_id: "event-conflict-next",
        sequence: 2,
        source: "runner",
        kind: "progress",
        payload: r#"{"state":"next"}"#,
        occurred_at: clock.now(),
    };
    assert_eq!(
        repo.append_execution_events_result(out_of_order, &[next], &clock)
            .await
            .unwrap(),
        EventApplyResult::Conflict
    );
    let event_count_after_out_of_order: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id = 'attempt-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(
        event_count_after_out_of_order, 1,
        "out-of-order batch must not write"
    );
}

#[tokio::test]
async fn event_replay_foreign_fence_is_stale_and_does_not_write() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(
        request("request-stale", &item_id, "key-stale", "same"),
        &clock,
    )
    .await
    .unwrap();
    let lease = claim_lease(
        &repo,
        "runner-a",
        "attempt-stale",
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap()
    .unwrap();
    let result = repo
        .append_execution_events_result(
            EventBatch {
                runner_id: "runner-a",
                attempt_id: "attempt-stale",
                fencing_token: lease.fencing_token + 1,
                previous_checkpoint: None,
                checkpoint: "checkpoint-stale",
            },
            &[NewEvent {
                id: "row-stale",
                event_id: "event-stale",
                sequence: 1,
                source: "runner",
                kind: "progress",
                payload: "{}",
                occurred_at: clock.now(),
            }],
            &clock,
        )
        .await
        .unwrap();
    assert_eq!(result, EventApplyResult::Stale);
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id = 'attempt-stale'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(event_count, 0);
}

#[tokio::test]
async fn token_guards_and_operator_requeue_are_idempotent() {
    let (repo, item_id, clock) = ready_repo().await;
    let bad = repo
        .create_pending_runner_and_issue_token(
            NewRunner {
                id: "pending",
                name: "pending",
                credential_hash: "ignored",
                labels: "{}",
                total_capacity: 1,
                available_capacity: 2,
                capability_snapshot: "{}",
                protocol_version: 1,
            },
            EnrollmentToken {
                id: "token",
                runner_id: "other",
                token_hash: "hash",
                expires_at: clock.now(),
            },
            &clock,
        )
        .await;
    assert!(
        bad.is_err(),
        "mismatched/expired/invalid capacity token issue fails before writes"
    );
    repo.enqueue_execution(request("request-r", &item_id, "key-r", "same"), &clock)
        .await
        .unwrap();
    let lease = claim_lease(
        &repo,
        "runner-a",
        "attempt-r",
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(matches!(
        repo.recover_attempt(
            recovery_input(
                "attempt-r",
                lease.fencing_token,
                "recovery-r",
                RecoveryObservation::Ambiguous,
            ),
            &clock,
        )
        .await
        .unwrap(),
        RecoveryObservationResult::Applied(response)
            if response.disposition == RecoveryDisposition::NeedsOperator
    ));
    sqlx::query("UPDATE execution_requests SET cancellation_requested_at='x' WHERE id='request-r'")
        .execute(repo.pool())
        .await
        .unwrap();
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id='runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    use tack_db::repo::execution::OperatorRequeueResult;
    assert_eq!(
        repo.operator_requeue_needs_operator(
            "request-r",
            "client-key",
            "operator-a",
            "reason-a",
            &clock
        )
        .await
        .unwrap(),
        OperatorRequeueResult::Requeued
    );
    assert_eq!(
        repo.operator_requeue_needs_operator(
            "request-r",
            "client-key",
            "operator-a",
            "reason-a",
            &clock
        )
        .await
        .unwrap(),
        OperatorRequeueResult::Replayed
    );
    let cleared: Option<String> = sqlx::query_scalar(
        "SELECT cancellation_requested_at FROM execution_requests WHERE id='request-r'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert!(cleared.is_none());
    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_recovery_audits WHERE attempt_id='attempt-r'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(audits, 2);
    let after: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id='runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(capacity, after);
}

#[tokio::test]
async fn operator_requeue_rejects_without_authoritative_recovery() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(request("request-r", &item_id, "key-r", "same"), &clock)
        .await
        .unwrap();
    claim_lease(
        &repo,
        "runner-a",
        "attempt-r",
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap()
    .unwrap();
    sqlx::query("UPDATE execution_attempts SET state='needs_operator' WHERE id='attempt-r'")
        .execute(repo.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE execution_requests SET state='needs_operator' WHERE id='request-r'")
        .execute(repo.pool())
        .await
        .unwrap();

    use tack_db::repo::execution::OperatorRequeueResult;
    assert_eq!(
        repo.operator_requeue_needs_operator(
            "request-r",
            "client-key",
            "operator-a",
            "reason-a",
            &clock,
        )
        .await
        .unwrap(),
        OperatorRequeueResult::InvalidTransition
    );
    let state: String =
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE id='request-r'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id='runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(state, "needs_operator");
    assert_eq!(capacity, 0);
}

impl ExecutionClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

async fn ready_repo() -> (Repository, String, FakeClock) {
    let repo = common::setup_test_db().await;
    let workspace = common::create_test_workspace(&repo).await;
    let project = common::make_project(&repo, workspace).await;
    let item = common::make_item(&repo, &project).await;
    let clock = FakeClock::new();
    repo.register_runner(
        NewRunner {
            id: "runner-a",
            name: "Runner A",
            credential_hash: "hash-only",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .unwrap();
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-a",
            name: "Profile A",
            instructions: "test",
            tool_policy: "{}",
            limits: "{}",
        },
        &clock,
    )
    .await
    .unwrap();
    (repo, item.id.to_string(), clock)
}

fn request<'a>(
    id: &'a str,
    item_id: &'a str,
    key: &'a str,
    fingerprint: &'a str,
) -> NewExecutionRequest<'a> {
    let request_snapshot: &'static str = Box::leak(
        format!(
            r#"{{"request_id":"{id}","item_id":"{item_id}","idempotency_key":"{key}","created_by":{{"source":"operator","subject_id":"test"}},"created_at":"2026-08-07T12:00:00Z","selector":{{"kind":"exact_runner","runner_id":"runner-a"}},"agent_profile_id":"profile-a","resolved_agent_profile":{{"name":"Profile A","instructions":"test","tool_policy":{{"mode":"safe"}},"timeout_seconds":60,"budgets":{{"limit":1}}}},"requested_harness_kind":"codex","requested_model_provider":"openai","requested_model_id":"opaque/model","repository":{{"kind":"git","remote":"https://example.test/repo.git","base_revision":"abc123","subdirectory":null}},"permission_policy":{{"tools":["shell"],"network":false}},"timeout_seconds":60,"budgets":{{"limit":1}},"status_map_policy_id":null,"environment":{{"MODE":{{"value":"test","secret_reference":null}}}},"metadata":{{"source":"test"}}}}"#
        )
        .into_boxed_str(),
    );
    NewExecutionRequest {
        id,
        item_id,
        idempotency_scope: "item",
        idempotency_key: key,
        request_fingerprint: fingerprint,
        selector_kind: "exact_runner",
        selector_id: "runner-a",
        agent_profile_id: Some("profile-a"),
        agent_profile_snapshot: r#"{"name":"Profile A","instructions":"test","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{"limit":1}}"#,
        requested_harness_kind: Some("codex"),
        requested_model_provider: Some("openai"),
        requested_model_id: Some("opaque/model"),
        repository_snapshot: r#"{"kind":"git","remote":"https://example.test/repo.git","base_revision":"abc123","subdirectory":null}"#,
        permission_policy: r#"{"tools":["shell"],"network":false}"#,
        timeout_seconds: Some(60),
        budgets: r#"{"limit":1}"#,
        status_map_policy_id: None,
        environment: r#"{"MODE":{"value":"test","secret_reference":null}}"#,
        metadata: r#"{"source":"test"}"#,
        request_snapshot,
    }
}

#[tokio::test]
async fn enqueue_rejects_malformed_and_contradictory_snapshots() {
    let (repo, item_id, clock) = ready_repo().await;
    let mut missing = request("snapshot-missing", &item_id, "snapshot-missing", "same");
    missing.request_snapshot = Box::leak(
        missing
            .request_snapshot
            .replace("\"metadata\":{\"source\":\"test\"}", "")
            .replace(",,", ",")
            .replace(",}", "}")
            .into_boxed_str(),
    );
    assert!(repo.enqueue_execution(missing, &clock).await.is_err());

    let mut malformed = request("snapshot-malformed", &item_id, "snapshot-malformed", "same");
    malformed.request_snapshot = "{not-json";
    assert!(repo.enqueue_execution(malformed, &clock).await.is_err());

    let mut contradictory = request("snapshot-cross", &item_id, "snapshot-cross", "same");
    contradictory.request_snapshot = Box::leak(
        contradictory
            .request_snapshot
            .replace(
                "\"request_id\":\"snapshot-cross\"",
                "\"request_id\":\"other\"",
            )
            .into_boxed_str(),
    );
    assert!(repo.enqueue_execution(contradictory, &clock).await.is_err());

    let mut wrong_created_at = request("snapshot-clock", &item_id, "snapshot-clock", "same");
    wrong_created_at.request_snapshot = Box::leak(
        wrong_created_at
            .request_snapshot
            .replace("2026-08-07T12:00:00Z", "2026-08-07T12:00:01Z")
            .into_boxed_str(),
    );
    assert!(
        repo.enqueue_execution(wrong_created_at, &clock)
            .await
            .is_err()
    );

    for (suffix, from, to) in [
        (
            "created-by",
            r#""created_by":{"source":"operator","subject_id":"test"}"#,
            r#""created_by":"operator""#,
        ),
        (
            "profile-policy",
            r#""tool_policy":{"mode":"safe"}"#,
            r#""tool_policy":"safe""#,
        ),
        ("budgets", r#""budgets":{"limit":1}"#, r#""budgets":1"#),
        ("repository-kind", r#""kind":"git""#, r#""kind":1"#),
        (
            "metadata",
            r#""metadata":{"source":"test"}"#,
            r#""metadata":false"#,
        ),
        (
            "environment",
            r#""value":"test","secret_reference":null"#,
            r#""value":"test","secret_reference":"secret://mode""#,
        ),
    ] {
        let id = Box::leak(format!("snapshot-{suffix}").into_boxed_str());
        let mut invalid = request(id, &item_id, id, "same");
        invalid.request_snapshot =
            Box::leak(invalid.request_snapshot.replace(from, to).into_boxed_str());
        assert!(
            repo.enqueue_execution(invalid, &clock).await.is_err(),
            "{suffix}"
        );
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_requests")
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn m060_quarantines_all_nonterminal_malformed_legacy_snapshots() {
    let pool = init_pool("sqlite::memory:").await.unwrap();
    migrations::run_up_to(&pool, "052_execution_report_replays")
        .await
        .unwrap();
    let mut connection = pool.acquire().await.unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *connection)
        .await
        .unwrap();
    sqlx::query("INSERT INTO execution_requests(id,item_id,idempotency_scope,idempotency_key,request_fingerprint,state,selector_kind,selector_id,agent_profile_snapshot,repository_snapshot,permission_policy,created_at,updated_at) VALUES('legacy-request','legacy-item','legacy','legacy-key','legacy-fingerprint','queued','exact_runner','runner-a','{}','{}','{}','2026-08-07T12:00:00Z','2026-08-07T12:00:00Z')")
        .execute(&mut *connection)
        .await
        .unwrap();
    for (id, key, state) in [
        ("legacy-partial", "legacy-partial-key", "leased"),
        ("legacy-malformed", "legacy-malformed-key", "running"),
        ("legacy-terminal", "legacy-terminal-key", "succeeded"),
        ("legacy-created-at", "legacy-created-at-key", "queued"),
        (
            "legacy-negative-timeout",
            "legacy-negative-timeout-key",
            "queued",
        ),
        (
            "legacy-fractional-timeout",
            "legacy-fractional-timeout-key",
            "queued",
        ),
        ("legacy-valid", "legacy-valid-key", "queued"),
    ] {
        sqlx::query("INSERT INTO execution_requests(id,item_id,idempotency_scope,idempotency_key,request_fingerprint,state,selector_kind,selector_id,agent_profile_snapshot,repository_snapshot,permission_policy,created_at,updated_at) VALUES(?, 'legacy-item', 'legacy', ?, 'legacy-fingerprint', ?, 'exact_runner', 'runner-a', '{}', '{}', '{}', '2026-08-07T12:00:00Z', '2026-08-07T12:00:00Z')")
            .bind(id)
            .bind(key)
            .bind(state)
            .execute(&mut *connection)
            .await
            .unwrap();
    }
    drop(connection);
    migrations::run_up_to(&pool, "058_execution_recovery_replay_response")
        .await
        .unwrap();
    sqlx::query("UPDATE execution_requests SET request_snapshot = '{\"created_by\":{},\"selector\":{},\"repository\":{}}' WHERE id = 'legacy-partial'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE execution_requests SET request_snapshot = '{not-json' WHERE id IN ('legacy-malformed', 'legacy-terminal')")
        .execute(&pool)
        .await
        .unwrap();
    for (id, key, snapshot) in [
        (
            "legacy-created-at",
            "legacy-created-at-key",
            request(
                "legacy-created-at",
                "legacy-item",
                "legacy-created-at-key",
                "same",
            )
            .request_snapshot
            .replace("2026-08-07T12:00:00Z", "2026-08-07 12:00:00"),
        ),
        (
            "legacy-negative-timeout",
            "legacy-negative-timeout-key",
            request(
                "legacy-negative-timeout",
                "legacy-item",
                "legacy-negative-timeout-key",
                "same",
            )
            .request_snapshot
            .replace("\"timeout_seconds\":60", "\"timeout_seconds\":-1"),
        ),
        (
            "legacy-fractional-timeout",
            "legacy-fractional-timeout-key",
            request(
                "legacy-fractional-timeout",
                "legacy-item",
                "legacy-fractional-timeout-key",
                "same",
            )
            .request_snapshot
            .replace("\"timeout_seconds\":60", "\"timeout_seconds\":60.5"),
        ),
        (
            "legacy-valid",
            "legacy-valid-key",
            request("legacy-valid", "legacy-item", "legacy-valid-key", "same")
                .request_snapshot
                .to_owned(),
        ),
    ] {
        sqlx::query(
            "UPDATE execution_requests SET request_snapshot=? WHERE id=? AND idempotency_key=?",
        )
        .bind(snapshot)
        .bind(id)
        .bind(key)
        .execute(&pool)
        .await
        .unwrap();
    }
    migrations::run_up_to(&pool, "059_quarantine_legacy_execution_request_snapshots")
        .await
        .unwrap();
    let m059_cohort: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, state FROM execution_requests WHERE id LIKE 'legacy-%timeout' OR id = 'legacy-created-at' OR id = 'legacy-valid' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        m059_cohort,
        vec![
            ("legacy-created-at".into(), "queued".into()),
            ("legacy-fractional-timeout".into(), "queued".into()),
            ("legacy-negative-timeout".into(), "queued".into()),
            ("legacy-valid".into(), "queued".into()),
        ]
    );
    migrations::run_all(&pool).await.unwrap();
    let after: Vec<(String, String)> =
        sqlx::query_as("SELECT id, state FROM execution_requests ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        after,
        vec![
            ("legacy-created-at".into(), "needs_operator".into()),
            ("legacy-fractional-timeout".into(), "needs_operator".into()),
            ("legacy-malformed".into(), "needs_operator".into()),
            ("legacy-negative-timeout".into(), "needs_operator".into()),
            ("legacy-partial".into(), "needs_operator".into()),
            ("legacy-request".into(), "needs_operator".into()),
            ("legacy-terminal".into(), "succeeded".into()),
            ("legacy-valid".into(), "queued".into()),
        ]
    );
    let repo = Repository::new(pool);
    let clock = FakeClock::new();
    repo.register_runner(
        NewRunner {
            id: "runner-a",
            name: "Runner A",
            credential_hash: "hash-only",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .unwrap();
    let claimed = repo
        .claim_execution_idempotent_with_snapshot(
            "runner-a",
            "claim-legacy-valid",
            "attempt-legacy-valid",
            Duration::seconds(60),
            &clock,
            RequestSelection::Naive,
        )
        .await
        .unwrap()
        .expect("the later valid queued row remains claimable");
    assert_eq!(claimed.lease.request_id, "legacy-valid");
}

async fn ready_completion_attempt(
    repo: &Repository,
    item_id: &str,
    clock: &FakeClock,
    request_id: &str,
    attempt_id: &str,
) -> i64 {
    repo.enqueue_execution(request(request_id, item_id, request_id, "same"), clock)
        .await
        .unwrap();
    claim_lease(repo, "runner-a", attempt_id, Duration::seconds(60), clock)
        .await
        .unwrap()
        .unwrap()
        .fencing_token
}

async fn claim_lease(
    repo: &Repository,
    runner_id: &str,
    attempt_id: &str,
    lease_duration: Duration,
    clock: &FakeClock,
) -> Result<Option<tack_db::repo::execution::Lease>, sqlx::Error> {
    let claim = repo
        .claim_execution_idempotent_with_snapshot(
            runner_id,
            attempt_id,
            attempt_id,
            lease_duration,
            clock,
            RequestSelection::Naive,
        )
        .await?;
    Ok(claim.map(|claim| claim.lease))
}

fn completion_input<'a>(
    attempt_id: &'a str,
    fencing_token: i64,
    completion_id: &'a str,
    final_event_checkpoint: Option<&'a str>,
) -> Completion<'a> {
    Completion {
        runner_id: "runner-a",
        attempt_id,
        fencing_token,
        completion_id,
        final_event_checkpoint,
        terminal_state: "succeeded",
        terminal_reason: "completed",
        actual_execution: r#"{"result":{"b":2,"a":1}}"#,
        usage: r#"{"output_tokens":2,"input_tokens":1}"#,
    }
}

fn cancellation_input<'a>(
    attempt_id: &'a str,
    fencing_token: i64,
    cancellation_request_id: &'a str,
    observed_at: DateTime<Utc>,
) -> CancellationObservationInput<'a> {
    CancellationObservationInput {
        runner_id: "runner-a",
        attempt_id,
        fencing_token,
        cancellation_request_id,
        observed_at,
        details: r#"{"detail":{"b":2,"a":1}}"#,
        observation: r#""process_stopped""#,
    }
}

fn recovery_input<'a>(
    attempt_id: &'a str,
    fencing_token: i64,
    recovery_key: &'a str,
    observation: RecoveryObservation,
) -> RecoveryObservationInput<'a> {
    RecoveryObservationInput {
        runner_id: "runner-a",
        attempt_id,
        fencing_token,
        recovery_key,
        observation,
        details: r#"{"journal_state":"prepared","process_observed":false,"observer":{"b":2,"a":1}}"#,
    }
}

#[tokio::test]
async fn recovery_stopped_without_start_requeues_fence_and_replays() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-recovery-safe",
        "attempt-recovery-safe",
    )
    .await;
    let input = recovery_input(
        "attempt-recovery-safe",
        fence,
        "recovery-safe",
        RecoveryObservation::ProcessStopped,
    );
    let applied = repo.recover_attempt(input.clone(), &clock).await.unwrap();
    let RecoveryObservationResult::Applied(applied) = applied else {
        panic!("current-fence stopped process with no start must requeue");
    };
    assert_eq!(
        applied.disposition,
        RecoveryDisposition::SafePreSpawnRequeue
    );
    let semantic_retry = RecoveryObservationInput {
        details: r#"{"observer":{"a":1,"b":2},"process_observed":false,"journal_state":"prepared"}"#,
        ..input.clone()
    };
    let replayed = repo.recover_attempt(semantic_retry, &clock).await.unwrap();
    let RecoveryObservationResult::Replayed(replayed) = replayed else {
        panic!("canonical recovery retry must replay");
    };
    assert_eq!(replayed, applied);
    let changed = RecoveryObservationInput {
        details: r#"{"journal_state":"prepared","process_observed":true,"observer":"changed"}"#,
        ..input
    };
    assert_eq!(
        repo.recover_attempt(changed, &clock).await.unwrap(),
        RecoveryObservationResult::Conflict
    );
    let attempt_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_attempts WHERE id = 'attempt-recovery-safe'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let request_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_requests WHERE id = 'request-recovery-safe'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(attempt_state, "lost");
    assert_eq!(request_state, "queued");
}

#[tokio::test]
async fn recovery_running_ambiguous_and_spawn_evidence_need_operator() {
    let (repo, item_id, clock) = ready_repo().await;
    sqlx::query(
        "UPDATE agent_runners SET total_capacity = 4, available_capacity = 4 WHERE id = 'runner-a'",
    )
    .execute(repo.pool())
    .await
    .unwrap();
    let stopped_fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-recovery-started",
        "attempt-recovery-started",
    )
    .await;
    let running_fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-recovery-running",
        "attempt-recovery-running",
    )
    .await;
    let ambiguous_fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-recovery-ambiguous",
        "attempt-recovery-ambiguous",
    )
    .await;
    let post_spawn_fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-recovery-post-spawn",
        "attempt-recovery-post-spawn",
    )
    .await;
    sqlx::query(
        "UPDATE execution_attempts SET started_at = ? WHERE id = 'attempt-recovery-started'",
    )
    .bind(clock.now().to_rfc3339())
    .execute(repo.pool())
    .await
    .unwrap();
    for (attempt_id, fence, key, observation) in [
        (
            "attempt-recovery-started",
            stopped_fence,
            "recovery-started",
            RecoveryObservation::ProcessStopped,
        ),
        (
            "attempt-recovery-running",
            running_fence,
            "recovery-running",
            RecoveryObservation::ProcessRunning,
        ),
        (
            "attempt-recovery-ambiguous",
            ambiguous_fence,
            "recovery-ambiguous",
            RecoveryObservation::Ambiguous,
        ),
    ] {
        assert!(matches!(
            repo.recover_attempt(recovery_input(attempt_id, fence, key, observation), &clock)
                .await
                .unwrap(),
            RecoveryObservationResult::Applied(response)
                if response.disposition == RecoveryDisposition::NeedsOperator
        ));
    }
    assert!(matches!(
        repo.recover_attempt(
            RecoveryObservationInput {
                runner_id: "runner-a",
                attempt_id: "attempt-recovery-post-spawn",
                fencing_token: post_spawn_fence,
                recovery_key: "recovery-post-spawn",
                observation: RecoveryObservation::ProcessStopped,
                details: r#"{"journal_state":"spawned","process_observed":false}"#,
            },
            &clock,
        )
        .await
        .unwrap(),
        RecoveryObservationResult::Applied(response)
            if response.disposition == RecoveryDisposition::NeedsOperator
    ));
    let needs_operator: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_attempts WHERE state = 'needs_operator'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(needs_operator, 4);
}

#[tokio::test]
async fn recovery_foreign_revoked_and_terminal_are_not_recovered() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-recovery-stale",
        "attempt-recovery-stale",
    )
    .await;
    assert_eq!(
        repo.recover_attempt(
            recovery_input(
                "attempt-recovery-stale",
                fence + 1,
                "recovery-foreign",
                RecoveryObservation::Ambiguous,
            ),
            &clock,
        )
        .await
        .unwrap(),
        RecoveryObservationResult::Stale
    );
    sqlx::query(
        "UPDATE execution_attempts SET state = 'succeeded' WHERE id = 'attempt-recovery-stale'",
    )
    .execute(repo.pool())
    .await
    .unwrap();
    let terminal = recovery_input(
        "attempt-recovery-stale",
        fence,
        "recovery-terminal",
        RecoveryObservation::Ambiguous,
    );
    assert!(matches!(
        repo.recover_attempt(terminal.clone(), &clock).await.unwrap(),
        RecoveryObservationResult::Applied(response)
            if response.disposition == RecoveryDisposition::AlreadyTerminal
    ));
    assert!(matches!(
        repo.recover_attempt(terminal.clone(), &clock).await.unwrap(),
        RecoveryObservationResult::Replayed(response)
            if response.disposition == RecoveryDisposition::AlreadyTerminal
    ));
    assert_eq!(
        repo.recover_attempt(
            RecoveryObservationInput {
                details: r#"{"journal_state":"prepared","process_observed":true}"#,
                ..terminal
            },
            &clock,
        )
        .await
        .unwrap(),
        RecoveryObservationResult::Conflict
    );
    repo.revoke_runner("runner-a", &clock).await.unwrap();
    assert_eq!(
        repo.recover_attempt(
            recovery_input(
                "attempt-recovery-stale",
                fence,
                "recovery-revoked",
                RecoveryObservation::Ambiguous,
            ),
            &clock,
        )
        .await
        .unwrap(),
        RecoveryObservationResult::Stale
    );
}

#[tokio::test]
async fn recovery_capacity_release_is_capped_and_replay_safe() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-recovery-cap",
        "attempt-recovery-cap",
    )
    .await;
    sqlx::query(
        "UPDATE agent_runners SET available_capacity = total_capacity WHERE id = 'runner-a'",
    )
    .execute(repo.pool())
    .await
    .unwrap();
    let input = recovery_input(
        "attempt-recovery-cap",
        fence,
        "recovery-cap",
        RecoveryObservation::Ambiguous,
    );
    repo.recover_attempt(input.clone(), &clock).await.unwrap();
    repo.recover_attempt(input, &clock).await.unwrap();
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(capacity, 1);
}

#[tokio::test]
async fn recovery_replay_insert_failure_rolls_back_lifecycle() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-recovery-rollback",
        "attempt-recovery-rollback",
    )
    .await;
    sqlx::query("CREATE TRIGGER fail_recovery_replay BEFORE INSERT ON execution_recovery_audits BEGIN SELECT RAISE(ABORT, 'forced recovery replay failure'); END")
        .execute(repo.pool())
        .await
        .unwrap();
    assert!(
        repo.recover_attempt(
            recovery_input(
                "attempt-recovery-rollback",
                fence,
                "recovery-rollback",
                RecoveryObservation::Ambiguous,
            ),
            &clock,
        )
        .await
        .is_err()
    );
    let attempt_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_attempts WHERE id = 'attempt-recovery-rollback'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let request_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_requests WHERE id = 'request-recovery-rollback'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(attempt_state, "leased");
    assert_eq!(request_state, "leased");
    assert_eq!(capacity, 0);
}

#[tokio::test]
async fn cancellation_response_loss_replays_after_time_advance() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-cancellation-loss",
        "attempt-cancellation-loss",
    )
    .await;
    assert!(
        repo.request_execution_cancellation("request-cancellation-loss", &clock)
            .await
            .unwrap()
    );
    let observed_at = clock.now();
    let input = cancellation_input(
        "attempt-cancellation-loss",
        fence,
        "cancellation-loss",
        observed_at,
    );
    let first = repo
        .observe_cancellation(input.clone(), &clock)
        .await
        .unwrap();
    let CancellationObservation::Cancelled(first) = first else {
        panic!("first cancellation observation must commit");
    };
    clock.advance(Duration::seconds(61));
    let semantic_retry = CancellationObservationInput {
        details: r#"{"detail":{"a":1,"b":2}}"#,
        observation: r#""process_stopped""#,
        ..input
    };
    let replay = repo
        .observe_cancellation(semantic_retry, &clock)
        .await
        .unwrap();
    let CancellationObservation::Replayed(replay) = replay else {
        panic!("lost response retry must replay");
    };
    assert_eq!(replay, first);
}

#[tokio::test]
async fn cancellation_changed_same_id_conflicts_without_write() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-cancellation-conflict",
        "attempt-cancellation-conflict",
    )
    .await;
    repo.request_execution_cancellation("request-cancellation-conflict", &clock)
        .await
        .unwrap();
    let input = cancellation_input(
        "attempt-cancellation-conflict",
        fence,
        "cancellation-conflict",
        clock.now(),
    );
    repo.observe_cancellation(input.clone(), &clock)
        .await
        .unwrap();
    let changed = CancellationObservationInput {
        details: r#"{"detail":"changed"}"#,
        ..input
    };
    assert_eq!(
        repo.observe_cancellation(changed, &clock).await.unwrap(),
        CancellationObservation::Conflict
    );
    let completion_id: String = sqlx::query_scalar(
        "SELECT completion_id FROM execution_attempts WHERE id = 'attempt-cancellation-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(completion_id, "cancellation-conflict");
    assert_eq!(capacity, 1);
}

#[tokio::test]
async fn cancellation_foreign_fence_is_stale_before_replay_lookup() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-cancellation-stale",
        "attempt-cancellation-stale",
    )
    .await;
    repo.request_execution_cancellation("request-cancellation-stale", &clock)
        .await
        .unwrap();
    let input = cancellation_input(
        "attempt-cancellation-stale",
        fence,
        "cancellation-stale",
        clock.now(),
    );
    repo.observe_cancellation(input.clone(), &clock)
        .await
        .unwrap();
    let foreign = CancellationObservationInput {
        fencing_token: fence + 1,
        ..input
    };
    assert_eq!(
        repo.observe_cancellation(foreign, &clock).await.unwrap(),
        CancellationObservation::Stale
    );
}

#[tokio::test]
async fn cancellation_corrupt_replay_response_fails_closed() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-cancellation-corrupt",
        "attempt-cancellation-corrupt",
    )
    .await;
    repo.request_execution_cancellation("request-cancellation-corrupt", &clock)
        .await
        .unwrap();
    let input = cancellation_input(
        "attempt-cancellation-corrupt",
        fence,
        "cancellation-corrupt",
        clock.now(),
    );
    repo.observe_cancellation(input.clone(), &clock)
        .await
        .unwrap();
    sqlx::query("UPDATE execution_cancellation_replays SET response = '{}' WHERE attempt_id = 'attempt-cancellation-corrupt'")
        .execute(repo.pool())
        .await
        .unwrap();
    assert!(repo.observe_cancellation(input, &clock).await.is_err());
}

#[tokio::test]
async fn cancellation_requires_exact_process_stopped_observation() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-cancellation-observation",
        "attempt-cancellation-observation",
    )
    .await;
    repo.request_execution_cancellation("request-cancellation-observation", &clock)
        .await
        .unwrap();
    let invalid = CancellationObservationInput {
        observation: r#""process_running""#,
        ..cancellation_input(
            "attempt-cancellation-observation",
            fence,
            "cancellation-observation",
            clock.now(),
        )
    };
    assert!(repo.observe_cancellation(invalid, &clock).await.is_err());
    let attempt_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_attempts WHERE id = 'attempt-cancellation-observation'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let replay_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_cancellation_replays WHERE attempt_id = 'attempt-cancellation-observation'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(attempt_state, "leased");
    assert_eq!(replay_count, 0);
}

#[tokio::test]
async fn cancellation_capacity_restore_is_capped_and_replay_safe() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-cancellation-cap",
        "attempt-cancellation-cap",
    )
    .await;
    repo.request_execution_cancellation("request-cancellation-cap", &clock)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE agent_runners SET available_capacity = total_capacity WHERE id = 'runner-a'",
    )
    .execute(repo.pool())
    .await
    .unwrap();
    let input = cancellation_input(
        "attempt-cancellation-cap",
        fence,
        "cancellation-cap",
        clock.now(),
    );
    repo.observe_cancellation(input.clone(), &clock)
        .await
        .unwrap();
    repo.observe_cancellation(input, &clock).await.unwrap();
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(capacity, 1);
}

#[tokio::test]
async fn cancellation_replay_insert_failure_rolls_back_terminal() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-cancellation-rollback",
        "attempt-cancellation-rollback",
    )
    .await;
    repo.request_execution_cancellation("request-cancellation-rollback", &clock)
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER fail_cancellation_replay BEFORE INSERT ON execution_cancellation_replays BEGIN SELECT RAISE(ABORT, 'forced cancellation replay failure'); END")
        .execute(repo.pool())
        .await
        .unwrap();
    assert!(
        repo.observe_cancellation(
            cancellation_input(
                "attempt-cancellation-rollback",
                fence,
                "cancellation-rollback",
                clock.now(),
            ),
            &clock,
        )
        .await
        .is_err()
    );
    let attempt_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_attempts WHERE id = 'attempt-cancellation-rollback'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let request_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_requests WHERE id = 'request-cancellation-rollback'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(attempt_state, "leased");
    assert_eq!(request_state, "leased");
    assert_eq!(capacity, 0);
}

#[tokio::test]
async fn cancellation_on_terminal_or_missing_request_is_not_success() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-cancellation-outcomes",
        "attempt-cancellation-outcomes",
    )
    .await;
    let ambiguous = repo
        .observe_cancellation(
            cancellation_input(
                "attempt-cancellation-outcomes",
                fence,
                "cancellation-ambiguous",
                clock.now(),
            ),
            &clock,
        )
        .await
        .unwrap();
    assert_eq!(
        ambiguous,
        CancellationObservation::Ambiguous {
            state: "leased".into()
        }
    );
    sqlx::query("UPDATE execution_attempts SET state = 'succeeded' WHERE id = 'attempt-cancellation-outcomes'")
        .execute(repo.pool())
        .await
        .unwrap();
    let terminal = repo
        .observe_cancellation(
            cancellation_input(
                "attempt-cancellation-outcomes",
                fence,
                "cancellation-terminal",
                clock.now(),
            ),
            &clock,
        )
        .await
        .unwrap();
    assert_eq!(
        terminal,
        CancellationObservation::AlreadyTerminal {
            state: "succeeded".into()
        }
    );
}

#[tokio::test]
async fn heartbeat_rejects_false_free_capacity_during_active_lease() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-free",
        "attempt-heartbeat-free",
    )
    .await;
    assert_eq!(
        repo.heartbeat_batch(
            "runner-a",
            "heartbeat-free",
            clock.now(),
            1,
            &[HeartbeatLease {
                attempt_id: "attempt-heartbeat-free",
                fencing_token: fence,
                state: "leased",
                journal_state: "prepared",
                last_event_checkpoint: None,
            }],
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap(),
        HeartbeatBatchResult::Conflict
    );
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(capacity, 0);
}

#[tokio::test]
async fn heartbeat_multi_lease_replay_is_canonical_and_authoritative() {
    let (repo, item_id, clock) = ready_repo().await;
    sqlx::query(
        "UPDATE agent_runners SET total_capacity = 2, available_capacity = 2 WHERE id = 'runner-a'",
    )
    .execute(repo.pool())
    .await
    .unwrap();
    let first_fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-multi-a",
        "attempt-heartbeat-multi-a",
    )
    .await;
    let second_fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-multi-b",
        "attempt-heartbeat-multi-b",
    )
    .await;
    let leases = [
        HeartbeatLease {
            attempt_id: "attempt-heartbeat-multi-a",
            fencing_token: first_fence,
            state: "leased",
            journal_state: "prepared",
            last_event_checkpoint: None,
        },
        HeartbeatLease {
            attempt_id: "attempt-heartbeat-multi-b",
            fencing_token: second_fence,
            state: "leased",
            journal_state: "prepared",
            last_event_checkpoint: None,
        },
    ];
    let sent_at = clock.now();
    let accepted = repo
        .heartbeat_batch(
            "runner-a",
            "heartbeat-multi",
            sent_at,
            0,
            &leases,
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap();
    let HeartbeatBatchResult::Accepted(accepted) = accepted else {
        panic!("multi-lease heartbeat must be accepted");
    };
    assert_eq!(accepted.leases.len(), 2);
    let replayed = repo
        .heartbeat_batch(
            "runner-a",
            "heartbeat-multi",
            sent_at,
            0,
            &[leases[1].clone(), leases[0].clone()],
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap();
    let HeartbeatBatchResult::Replayed(replayed) = replayed else {
        panic!("canonical lease ordering must replay");
    };
    assert_eq!(replayed, accepted);
}

#[tokio::test]
async fn heartbeat_exact_replay_after_clock_advance_returns_fields() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-time",
        "attempt-heartbeat-time",
    )
    .await;
    let leases = [HeartbeatLease {
        attempt_id: "attempt-heartbeat-time",
        fencing_token: fence,
        state: "leased",
        journal_state: "prepared",
        last_event_checkpoint: None,
    }];
    let sent_at = clock.now();
    let accepted = repo
        .heartbeat_batch(
            "runner-a",
            "heartbeat-time",
            sent_at,
            0,
            &leases,
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap();
    let HeartbeatBatchResult::Accepted(accepted) = accepted else {
        panic!("first heartbeat must be accepted");
    };
    clock.advance(Duration::seconds(61));
    let replayed = repo
        .heartbeat_batch(
            "runner-a",
            "heartbeat-time",
            sent_at,
            0,
            &leases,
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap();
    let HeartbeatBatchResult::Replayed(replayed) = replayed else {
        panic!("exact heartbeat retry must replay after lease expiry");
    };
    assert_eq!(replayed, accepted);
}

#[tokio::test]
async fn heartbeat_frozen_request_mutations_conflict_without_write() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-conflict",
        "attempt-heartbeat-conflict",
    )
    .await;
    let leases = [HeartbeatLease {
        attempt_id: "attempt-heartbeat-conflict",
        fencing_token: fence,
        state: "leased",
        journal_state: "prepared",
        last_event_checkpoint: None,
    }];
    let sent_at = clock.now();
    repo.heartbeat_batch(
        "runner-a",
        "heartbeat-conflict",
        sent_at,
        0,
        &leases,
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap();
    let before: String = sqlx::query_scalar(
        "SELECT lease_expires_at FROM execution_attempts WHERE id = 'attempt-heartbeat-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    clock.advance(Duration::seconds(1));
    assert_eq!(
        repo.heartbeat_batch(
            "runner-a",
            "heartbeat-conflict",
            sent_at + Duration::seconds(1),
            0,
            &leases,
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap(),
        HeartbeatBatchResult::Conflict
    );
    for mutated_leases in [
        [HeartbeatLease {
            state: "running",
            ..leases[0].clone()
        }],
        [HeartbeatLease {
            journal_state: "spawned",
            ..leases[0].clone()
        }],
        [HeartbeatLease {
            last_event_checkpoint: Some("checkpoint-mutated"),
            ..leases[0].clone()
        }],
    ] {
        assert_eq!(
            repo.heartbeat_batch(
                "runner-a",
                "heartbeat-conflict",
                sent_at,
                0,
                &mutated_leases,
                Duration::seconds(60),
                &clock,
            )
            .await
            .unwrap(),
            HeartbeatBatchResult::Conflict
        );
    }
    let after: String = sqlx::query_scalar(
        "SELECT lease_expires_at FROM execution_attempts WHERE id = 'attempt-heartbeat-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(after, before);
}

#[tokio::test]
async fn heartbeat_stale_lease_returns_typed_stale_result() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-stale",
        "attempt-heartbeat-stale",
    )
    .await;
    clock.advance(Duration::seconds(61));
    assert_eq!(
        repo.heartbeat_batch(
            "runner-a",
            "heartbeat-stale",
            clock.now(),
            0,
            &[HeartbeatLease {
                attempt_id: "attempt-heartbeat-stale",
                fencing_token: fence,
                state: "leased",
                journal_state: "prepared",
                last_event_checkpoint: None,
            }],
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap(),
        HeartbeatBatchResult::StaleLease("attempt-heartbeat-stale".into())
    );
}

#[tokio::test]
async fn expired_unrecovered_attempt_blocks_capacity_and_claims() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-expired",
        "attempt-heartbeat-expired",
    )
    .await;
    clock.advance(Duration::seconds(61));
    assert_eq!(
        repo.heartbeat_batch(
            "runner-a",
            "heartbeat-expired",
            clock.now(),
            1,
            &[],
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap(),
        HeartbeatBatchResult::Conflict
    );
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(capacity, 0);
    let mut blocked = request(
        "request-heartbeat-blocked",
        &item_id,
        "key-heartbeat-blocked",
        "same",
    );
    blocked.request_snapshot = Box::leak(
        blocked
            .request_snapshot
            .replace("2026-08-07T12:00:00Z", &clock.now().to_rfc3339())
            .into_boxed_str(),
    );
    repo.enqueue_execution(blocked, &clock).await.unwrap();
    assert!(
        claim_lease(
            &repo,
            "runner-a",
            "attempt-heartbeat-blocked",
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap()
        .is_none()
    );
    let _ = fence;
}

#[tokio::test]
async fn heartbeat_replay_insert_failure_rolls_back_all_updates() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-heartbeat-rollback",
        "attempt-heartbeat-rollback",
    )
    .await;
    sqlx::query("CREATE TRIGGER fail_heartbeat_replay BEFORE INSERT ON execution_heartbeat_replays BEGIN SELECT RAISE(ABORT, 'forced heartbeat replay failure'); END")
        .execute(repo.pool())
        .await
        .unwrap();
    let result = repo
        .heartbeat_batch(
            "runner-a",
            "heartbeat-rollback",
            clock.now(),
            0,
            &[HeartbeatLease {
                attempt_id: "attempt-heartbeat-rollback",
                fencing_token: fence,
                state: "leased",
                journal_state: "prepared",
                last_event_checkpoint: None,
            }],
            Duration::seconds(60),
            &clock,
        )
        .await;
    assert!(result.is_err());
    let last_heartbeat: Option<String> = sqlx::query_scalar(
        "SELECT last_heartbeat_at FROM execution_attempts WHERE id = 'attempt-heartbeat-rollback'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let runner_heartbeat: Option<String> =
        sqlx::query_scalar("SELECT last_heartbeat_at FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    let replay_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_heartbeat_replays WHERE heartbeat_id = 'heartbeat-rollback'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert!(last_heartbeat.is_none());
    assert!(runner_heartbeat.is_none());
    assert_eq!(replay_count, 0);
}

#[tokio::test]
async fn completion_response_loss_replays_authoritative_response() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-loss",
        "attempt-completion-loss",
    )
    .await;
    let completion = completion_input("attempt-completion-loss", fence, "completion-loss", None);
    let committed = repo
        .complete_execution_result(completion.clone(), &clock)
        .await
        .unwrap();
    let CompletionResult::Committed(committed) = committed else {
        panic!("first completion must commit");
    };
    let replayed = repo
        .complete_execution_result(completion, &clock)
        .await
        .unwrap();
    let CompletionResult::Replayed(replayed) = replayed else {
        panic!("lost response retry must replay");
    };
    assert_eq!(replayed, committed);
}

#[tokio::test]
async fn completion_replay_foreign_fence_is_stale() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-stale",
        "attempt-completion-stale",
    )
    .await;
    let completion = completion_input("attempt-completion-stale", fence, "completion-stale", None);
    repo.complete_execution_result(completion.clone(), &clock)
        .await
        .unwrap();
    let foreign = Completion {
        fencing_token: fence + 1,
        ..completion
    };
    assert_eq!(
        repo.complete_execution_result(foreign, &clock)
            .await
            .unwrap(),
        CompletionResult::Stale
    );
}

#[tokio::test]
async fn completion_replay_canonicalizes_structured_json() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-canonical",
        "attempt-completion-canonical",
    )
    .await;
    let completion = completion_input(
        "attempt-completion-canonical",
        fence,
        "completion-canonical",
        None,
    );
    repo.complete_execution_result(completion.clone(), &clock)
        .await
        .unwrap();
    let canonical_retry = Completion {
        actual_execution: r#"{"result":{"a":1,"b":2}}"#,
        usage: r#"{"input_tokens":1,"output_tokens":2}"#,
        ..completion
    };
    assert!(matches!(
        repo.complete_execution_result(canonical_retry, &clock)
            .await
            .unwrap(),
        CompletionResult::Replayed(_)
    ));
}

#[tokio::test]
async fn completion_replay_canonicalizes_structured_terminal_reason() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-reason",
        "attempt-completion-reason",
    )
    .await;
    let completion = Completion {
        terminal_reason: r#"{"detail":{"b":2,"a":1},"code":"done"}"#,
        ..completion_input(
            "attempt-completion-reason",
            fence,
            "completion-reason",
            None,
        )
    };
    repo.complete_execution_result(completion.clone(), &clock)
        .await
        .unwrap();
    let semantic_retry = Completion {
        terminal_reason: r#"{"code":"done","detail":{"a":1,"b":2}}"#,
        ..completion
    };
    assert!(matches!(
        repo.complete_execution_result(semantic_retry, &clock)
            .await
            .unwrap(),
        CompletionResult::Replayed(_)
    ));
}

// Defect 1 regression: distinguishes the benign, retryable `Conflict` (a lost
// optimistic-concurrency compare-and-set, before any replay row exists) from
// the non-retryable `IdempotencyConflict` (the same completion_id — an
// idempotency-scoped key — reused with different content once a replay row
// exists). Collapsing these into one variant is exactly how a runner ends up
// told to retry a request that can never succeed.
#[tokio::test]
async fn completion_and_idempotency_conflicts_are_distinguished() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-conflict",
        "attempt-completion-conflict",
    )
    .await;
    let base = completion_input(
        "attempt-completion-conflict",
        fence,
        "completion-conflict",
        None,
    );
    // No replay row exists yet: this is a lost compare-and-set against the
    // live attempt row (final_event_checkpoint doesn't match), not a reused
    // idempotency key. Benign/retryable.
    let wrong_checkpoint = Completion {
        final_event_checkpoint: Some("not-current"),
        ..base.clone()
    };
    assert_eq!(
        repo.complete_execution_result(wrong_checkpoint, &clock)
            .await
            .unwrap(),
        CompletionResult::Conflict
    );
    let state: String = sqlx::query_scalar(
        "SELECT state FROM execution_attempts WHERE id = 'attempt-completion-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(state, "leased");
    repo.complete_execution_result(base.clone(), &clock)
        .await
        .unwrap();
    // Same completion_id (idempotency-scoped key) as `base`, now that its
    // replay row exists, but different terminal_reason/final_event_checkpoint:
    // fingerprint mismatch. Non-retryable.
    let changed_fields = Completion {
        terminal_reason: "changed",
        ..base.clone()
    };
    let changed_checkpoint = Completion {
        final_event_checkpoint: Some("changed"),
        ..base
    };
    assert_eq!(
        repo.complete_execution_result(changed_fields, &clock)
            .await
            .unwrap(),
        CompletionResult::IdempotencyConflict
    );
    assert_eq!(
        repo.complete_execution_result(changed_checkpoint, &clock)
            .await
            .unwrap(),
        CompletionResult::IdempotencyConflict
    );
    let completion_id: String = sqlx::query_scalar(
        "SELECT completion_id FROM execution_attempts WHERE id = 'attempt-completion-conflict'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(completion_id, "completion-conflict");
}

#[tokio::test]
async fn completion_replay_corrupt_response_fails_closed() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-corrupt",
        "attempt-completion-corrupt",
    )
    .await;
    let completion = completion_input(
        "attempt-completion-corrupt",
        fence,
        "completion-corrupt",
        None,
    );
    repo.complete_execution_result(completion.clone(), &clock)
        .await
        .unwrap();
    sqlx::query("UPDATE execution_completion_replays SET response = '{}' WHERE attempt_id = 'attempt-completion-corrupt'")
        .execute(repo.pool())
        .await
        .unwrap();
    assert!(
        repo.complete_execution_result(completion, &clock)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn completion_replay_restores_capacity_once_and_never_above_cap() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-capacity",
        "attempt-completion-capacity",
    )
    .await;
    sqlx::query(
        "UPDATE agent_runners SET available_capacity = total_capacity WHERE id = 'runner-a'",
    )
    .execute(repo.pool())
    .await
    .unwrap();
    let completion = completion_input(
        "attempt-completion-capacity",
        fence,
        "completion-capacity",
        None,
    );
    repo.complete_execution_result(completion.clone(), &clock)
        .await
        .unwrap();
    repo.complete_execution_result(completion, &clock)
        .await
        .unwrap();
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(capacity, 1);

    let second_fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-once",
        "attempt-completion-once",
    )
    .await;
    let second = completion_input(
        "attempt-completion-once",
        second_fence,
        "completion-once",
        None,
    );
    repo.complete_execution_result(second.clone(), &clock)
        .await
        .unwrap();
    let after_commit: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    repo.complete_execution_result(second, &clock)
        .await
        .unwrap();
    let after_replay: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(after_commit, 1);
    assert_eq!(after_replay, 1);
}

#[tokio::test]
async fn completion_replay_insert_failure_rolls_back_terminal() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-completion-rollback",
        "attempt-completion-rollback",
    )
    .await;
    sqlx::query("CREATE TRIGGER fail_completion_replay BEFORE INSERT ON execution_completion_replays BEGIN SELECT RAISE(ABORT, 'forced completion replay failure'); END")
        .execute(repo.pool())
        .await
        .unwrap();
    let error = repo
        .complete_execution_result(
            completion_input(
                "attempt-completion-rollback",
                fence,
                "completion-rollback",
                None,
            ),
            &clock,
        )
        .await;
    assert!(error.is_err());
    let attempt_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_attempts WHERE id = 'attempt-completion-rollback'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let request_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_requests WHERE id = 'request-completion-rollback'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(attempt_state, "leased");
    assert_eq!(request_state, "leased");
    assert_eq!(capacity, 0);
}

#[tokio::test]
async fn old_schema_upgrades_to_all_ten_execution_tables() {
    let pool = init_pool("sqlite::memory:").await.unwrap();
    migrations::run_up_to(&pool, "038_orch_approvals_rebuild")
        .await
        .unwrap();
    migrations::run_all(&pool).await.unwrap();
    for table in [
        "agent_fleets",
        "agent_runners",
        "agent_fleet_members",
        "agent_profiles",
        "model_profiles",
        "execution_requests",
        "execution_attempts",
        "execution_events",
        "execution_artifacts",
        "execution_decisions",
    ] {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(exists, "{table} missing after upgrade");
    }
    let migration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations WHERE name BETWEEN '039_agent_fleets' AND '048_execution_decisions'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(migration_count, 10);
}

#[tokio::test]
async fn enqueue_is_idempotent_and_conflicting_reuse_is_rejected() {
    let (repo, item_id, clock) = ready_repo().await;
    assert_eq!(
        repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
            .await
            .unwrap(),
        EnqueueResult::Created("request-a".into())
    );
    assert_eq!(
        repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
            .await
            .unwrap(),
        EnqueueResult::Replayed("request-a".into())
    );
    assert_eq!(
        repo.enqueue_execution(request("request-c", &item_id, "key-a", "different"), &clock)
            .await
            .unwrap(),
        EnqueueResult::Conflict
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_requests")
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn enqueue_replay_compares_frozen_snapshot_before_current_time() {
    let (repo, item_id, clock) = ready_repo().await;
    assert_eq!(
        repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
            .await
            .unwrap(),
        EnqueueResult::Created("request-a".into())
    );
    clock.advance(Duration::seconds(1));
    assert_eq!(
        repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
            .await
            .unwrap(),
        EnqueueResult::Replayed("request-a".into()),
        "an exact retry uses the original frozen snapshot rather than the new clock"
    );
    let mut changed_timestamp = request("request-a", &item_id, "key-a", "same");
    changed_timestamp.request_snapshot = Box::leak(
        changed_timestamp
            .request_snapshot
            .replace("2026-08-07T12:00:00Z", "2026-08-07T12:00:01Z")
            .into_boxed_str(),
    );
    assert_eq!(
        repo.enqueue_execution(changed_timestamp, &clock)
            .await
            .unwrap(),
        EnqueueResult::Conflict,
        "same idempotency key with a changed frozen timestamp is not an exact replay"
    );
}

#[tokio::test]
async fn claim_fence_replay_and_terminal_state_are_atomic() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
        .await
        .unwrap();
    let lease = claim_lease(
        &repo,
        "runner-a",
        "attempt-a",
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap()
    .expect("first claim");
    assert_eq!(lease.fencing_token, 1);
    assert!(
        claim_lease(
            &repo,
            "runner-a",
            "attempt-b",
            Duration::seconds(60),
            &clock
        )
        .await
        .unwrap()
        .is_none(),
        "capacity/request state prevent a second valid lease"
    );
    assert_eq!(
        repo.heartbeat_batch(
            "runner-a",
            "heartbeat-stale",
            clock.now(),
            0,
            &[HeartbeatLease {
                attempt_id: "attempt-a",
                fencing_token: 999,
                state: "leased",
                journal_state: "prepared",
                last_event_checkpoint: None,
            }],
            Duration::seconds(60),
            &clock,
        )
        .await
        .unwrap(),
        HeartbeatBatchResult::StaleLease("attempt-a".into()),
        "stale fence writes nothing"
    );
    let event = NewEvent {
        id: "row-1",
        event_id: "event-1",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: "{}",
        occurred_at: clock.now(),
    };
    assert!(matches!(
        repo.append_execution_events_result(
            EventBatch {
                runner_id: "runner-a",
                attempt_id: "attempt-a",
                fencing_token: 1,
                previous_checkpoint: None,
                checkpoint: "checkpoint-1"
            },
            std::slice::from_ref(&event),
            &clock
        )
        .await
        .unwrap(),
        EventApplyResult::Applied(result) if !result.replayed
    ));
    assert!(
        matches!(
            repo.append_execution_events_result(
                EventBatch {
                    runner_id: "runner-a",
                    attempt_id: "attempt-a",
                    fencing_token: 1,
                    previous_checkpoint: None,
                    checkpoint: "checkpoint-1"
                },
                &[event],
                &clock
            )
            .await
            .unwrap(),
            EventApplyResult::Applied(result) if result.replayed
        ),
        "same checkpoint is a replay"
    );
    let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_events")
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(event_count, 1);
    assert!(
        repo.complete_execution(
            Completion {
                runner_id: "runner-a",
                attempt_id: "attempt-a",
                fencing_token: 1,
                completion_id: "complete-1",
                final_event_checkpoint: Some("checkpoint-1"),
                terminal_state: "succeeded",
                terminal_reason: "completed",
                actual_execution: "{}",
                usage: "{}"
            },
            &clock
        )
        .await
        .unwrap()
    );
    assert!(
        repo.complete_execution(
            Completion {
                runner_id: "runner-a",
                attempt_id: "attempt-a",
                fencing_token: 1,
                completion_id: "complete-1",
                final_event_checkpoint: Some("checkpoint-1"),
                terminal_state: "succeeded",
                terminal_reason: "completed",
                actual_execution: "{}",
                usage: "{}"
            },
            &clock
        )
        .await
        .unwrap(),
        "same completion is replay-safe"
    );
    assert!(
        !repo
            .complete_execution(
                Completion {
                    runner_id: "runner-a",
                    attempt_id: "attempt-a",
                    fencing_token: 1,
                    completion_id: "complete-2",
                    final_event_checkpoint: Some("checkpoint-1"),
                    terminal_state: "failed",
                    terminal_reason: "changed",
                    actual_execution: "{}",
                    usage: "{}"
                },
                &clock
            )
            .await
            .unwrap(),
        "terminal attempt cannot reopen"
    );
}

#[tokio::test]
async fn concurrent_claimers_receive_exactly_one_valid_lease() {
    let (repo, item_id, clock) = ready_repo().await;
    // Capacity must not be the reason the second claimer loses: both can enter
    // the scheduler, but the request's queued → leased compare-and-set wins once.
    sqlx::query("UPDATE agent_runners SET total_capacity = 2, available_capacity = 2")
        .execute(repo.pool())
        .await
        .unwrap();
    repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
        .await
        .unwrap();
    let left = repo.clone();
    let right = repo.clone();
    let (first, second) = tokio::join!(
        claim_lease(
            &left,
            "runner-a",
            "attempt-a",
            Duration::seconds(60),
            &clock
        ),
        claim_lease(
            &right,
            "runner-a",
            "attempt-b",
            Duration::seconds(60),
            &clock
        ),
    );
    // BEGIN IMMEDIATE now serializes claimants at the write lock instead of
    // letting two deferred readers race to upgrade. There is no legitimate
    // reason for either branch to fail at the sqlx level any more, so a raw
    // Err here (SQLITE_LOCKED "database is deadlocked" without BEGIN
    // IMMEDIATE, observed on ~100% of unfixed runs) is a hard test failure
    // -- no retry fallback, no swallowing.
    let first = first.expect("first claimant must succeed at the sqlx level");
    let second = second.expect("second claimant must succeed at the sqlx level");

    let leases: Vec<_> = [first.clone(), second.clone()]
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(
        leases.len(),
        1,
        "exactly one concurrent claimant receives a valid lease for one request"
    );
    let winner = leases.into_iter().next().unwrap();
    assert_eq!(winner.request_id, "request-a");
    assert_eq!(winner.attempt_number, 1);
    assert_eq!(
        winner.fencing_token, 1,
        "the sole winner takes the first fencing token"
    );

    // The losing branch is not a swallowed error and not a second lease: it
    // is the typed "no eligible work" outcome (None), because the winner
    // already consumed the queued → leased compare-and-set.
    assert!(
        matches!((&first, &second), (Some(_), None) | (None, Some(_))),
        "the losing claimant must observe a well-typed no-work result, not a second lease: {first:?} / {second:?}"
    );

    // Capacity end-state must be coherent: only the winner's reservation
    // survives. If the loser's capacity decrement (or its now-rolled-back
    // request CAS) leaked outside its transaction, this would read 0 instead
    // of 1 even though total_capacity started at 2.
    let available_capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id = 'runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(
        available_capacity, 1,
        "only the winning claim consumes a capacity slot"
    );

    let request_state: String =
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = 'request-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(request_state, "leased");

    let attempt_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_attempts WHERE request_id = 'request-a'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(
        attempt_count, 1,
        "exactly one attempt row exists for the request"
    );
}

#[tokio::test]
async fn concurrent_claimers_deadlock_fix_holds_under_load() {
    // Regression proof for the claim_execution_idempotent_with_snapshot
    // BEGIN IMMEDIATE fix: run the same concurrent-claim shape many times in
    // one process and demand zero sqlx-level errors and exactly one lease
    // per iteration. A single flaky iteration means the serialization
    // regressed back to the deferred-reader deadlock.
    for i in 0..25 {
        let (repo, item_id, clock) = ready_repo().await;
        sqlx::query("UPDATE agent_runners SET total_capacity = 2, available_capacity = 2")
            .execute(repo.pool())
            .await
            .unwrap();
        let request_id = format!("request-{i}");
        repo.enqueue_execution(
            request(&request_id, &item_id, &format!("key-{i}"), "same"),
            &clock,
        )
        .await
        .unwrap();
        let left = repo.clone();
        let right = repo.clone();
        let attempt_a = format!("attempt-{i}-a");
        let attempt_b = format!("attempt-{i}-b");
        let (first, second) = tokio::join!(
            claim_lease(&left, "runner-a", &attempt_a, Duration::seconds(60), &clock),
            claim_lease(
                &right,
                "runner-a",
                &attempt_b,
                Duration::seconds(60),
                &clock
            ),
        );
        let first = first.unwrap_or_else(|e| panic!("iteration {i}: first claimant errored: {e}"));
        let second =
            second.unwrap_or_else(|e| panic!("iteration {i}: second claimant errored: {e}"));
        let leases = [first, second].into_iter().flatten().count();
        assert_eq!(leases, 1, "iteration {i}: expected exactly one lease");
    }
}

#[tokio::test]
async fn foreign_keys_and_expiry_recovery_fail_closed() {
    let (repo, item_id, clock) = ready_repo().await;
    let orphan = repo
        .enqueue_execution(request("orphan", "missing-item", "orphan-key", "x"), &clock)
        .await;
    assert!(orphan.is_err(), "request item FK is enforced");
    repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
        .await
        .unwrap();
    let lease = claim_lease(&repo, "runner-a", "attempt-a", Duration::seconds(5), &clock)
        .await
        .unwrap()
        .unwrap();
    clock.advance(Duration::seconds(6));
    assert!(matches!(
        repo.recover_attempt(
            recovery_input(
                "attempt-a",
                lease.fencing_token,
                "recovery-expiry",
                RecoveryObservation::Ambiguous,
            ),
            &clock,
        )
        .await
        .unwrap(),
        RecoveryObservationResult::Applied(response)
            if response.disposition == RecoveryDisposition::NeedsOperator
    ));
    let state: String = sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = ?")
        .bind(&lease.attempt_id)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(state, "needs_operator");
}

#[tokio::test]
async fn artifact_and_decision_reject_lease_expiry_equality() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(
        request("request-boundary", &item_id, "key-boundary", "same"),
        &clock,
    )
    .await
    .unwrap();
    let lease = claim_lease(
        &repo,
        "runner-a",
        "attempt-boundary",
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap()
    .unwrap();
    sqlx::query(
        "UPDATE execution_attempts SET state='running', lease_expires_at=? WHERE id='attempt-boundary'",
    )
    .bind(clock.now().to_rfc3339())
    .execute(repo.pool())
    .await
    .unwrap();

    assert!(
        !repo
            .record_execution_artifact(
                "runner-a",
                "attempt-boundary",
                lease.fencing_token,
                NewArtifact {
                    id: "artifact-row",
                    artifact_id: "artifact-boundary",
                    kind: "log",
                    name: "log",
                    media_type: None,
                    size_bytes: 0,
                    sha256: "hash",
                    content_disposition: None,
                    content_reference: None,
                    metadata: "{}",
                },
                &clock,
            )
            .await
            .unwrap()
    );
    assert!(
        !repo
            .create_execution_decision(
                "runner-a",
                "attempt-boundary",
                lease.fencing_token,
                NewDecision {
                    id: "decision-row",
                    decision_id: "decision-boundary",
                    kind: "approval",
                    prompt: "Continue?",
                    options: "[]",
                    metadata: "{}",
                    expires_at: None,
                },
                &clock,
            )
            .await
            .unwrap()
    );
    let artifact_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_artifacts")
        .fetch_one(repo.pool())
        .await
        .unwrap();
    let decision_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_decisions")
        .fetch_one(repo.pool())
        .await
        .unwrap();
    assert_eq!(artifact_count, 0);
    assert_eq!(decision_count, 0);
}

// Defect 2 regression: `record_execution_artifact` and
// `create_execution_decision` must not run their eligibility SELECT and
// their INSERT as two separate un-transacted statements — a concurrent
// terminal transition could commit between them and let the write land
// against an attempt that had already gone terminal.
//
// Uses a file-backed database, not this suite's shared in-memory
// `ready_repo()` harness: SQLite's shared-cache mode (what a pooled
// `:memory:` database uses) imposes table-level locking that blocks a
// plain SELECT behind any pending write, which accidentally serializes
// the SELECT-then-INSERT gap this test needs to expose on both fixed and
// unfixed code. Fully awaits opening `BEGIN IMMEDIATE` and the terminal
// UPDATE before either writer is even constructed, so the hold is
// unconditionally in place before the race starts; `tokio::join!` alone
// would only control poll order, not which SQL reaches SQLite first.
#[tokio::test]
async fn artifact_and_decision_reject_concurrently_terminal_attempt() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let db_path = dir.path().join("terminal-attempt-race.db");
    let pool = init_pool(&format!("sqlite://{}?mode=rwc", db_path.display()))
        .await
        .expect("file-backed pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let workspace = common::create_test_workspace(&repo).await;
    let project = common::make_project(&repo, workspace).await;
    let item = common::make_item(&repo, &project).await;
    let item_id = item.id.to_string();
    let clock = FakeClock::new();
    repo.register_runner(
        NewRunner {
            id: "runner-a",
            name: "Runner A",
            credential_hash: "hash-only",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .unwrap();
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-a",
            name: "Profile A",
            instructions: "test",
            tool_policy: "{}",
            limits: "{}",
        },
        &clock,
    )
    .await
    .unwrap();
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-race-terminal",
        "attempt-race-terminal",
    )
    .await;
    sqlx::query("UPDATE execution_attempts SET state='running' WHERE id='attempt-race-terminal'")
        .execute(repo.pool())
        .await
        .unwrap();

    // Fully establish the held write lock and the terminal UPDATE *before*
    // either writer starts, so there is no race for who reaches the SQLite
    // engine first.
    let mut manual_tx = repo.pool().begin_with("BEGIN IMMEDIATE").await.unwrap();
    let terminal_now = clock.now().to_rfc3339();
    sqlx::query(
        "UPDATE execution_attempts SET state='succeeded', ended_at=?, updated_at=? WHERE id='attempt-race-terminal'",
    )
    .bind(&terminal_now)
    .bind(&terminal_now)
    .execute(&mut *manual_tx)
    .await
    .unwrap();

    let artifact_write = repo.record_execution_artifact(
        "runner-a",
        "attempt-race-terminal",
        fence,
        NewArtifact {
            id: "artifact-race-row",
            artifact_id: "artifact-race",
            kind: "log",
            name: "log",
            media_type: None,
            size_bytes: 0,
            sha256: "hash",
            content_disposition: None,
            content_reference: None,
            metadata: "{}",
        },
        &clock,
    );
    let decision_write = repo.create_execution_decision(
        "runner-a",
        "attempt-race-terminal",
        fence,
        NewDecision {
            id: "decision-race-row",
            decision_id: "decision-race",
            kind: "approval",
            prompt: "Continue?",
            options: "[]",
            metadata: "{}",
            expires_at: None,
        },
        &clock,
    );
    // Poll (bounded, not a fixed wait) until both writers have checked a
    // connection out of the pool — each has begun its own `BEGIN IMMEDIATE`
    // and is blocked on this lock — before releasing it.
    let idle_before_writers = repo.pool().num_idle();
    let delayed_release = async {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        while repo.pool().num_idle() > idle_before_writers.saturating_sub(2) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "writers never both blocked on the lock"
            );
            tokio::task::yield_now().await;
        }
        manual_tx.commit().await.unwrap();
    };

    let (artifact_accepted, decision_accepted, _) =
        tokio::join!(artifact_write, decision_write, delayed_release);
    let artifact_accepted =
        artifact_accepted.expect("artifact write must succeed at the sqlx level");
    let decision_accepted =
        decision_accepted.expect("decision write must succeed at the sqlx level");

    assert!(
        !artifact_accepted,
        "artifact must not be recorded once the attempt has gone terminal concurrently"
    );
    assert!(
        !decision_accepted,
        "decision must not be recorded once the attempt has gone terminal concurrently"
    );

    let artifact_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_artifacts WHERE attempt_id='attempt-race-terminal'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let decision_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_decisions WHERE attempt_id='attempt-race-terminal'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(artifact_count, 0, "no artifact row may exist");
    assert_eq!(decision_count, 0, "no decision row may exist");

    let state: String =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id='attempt-race-terminal'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(state, "succeeded");
}

#[tokio::test]
async fn queue_and_history_indexes_are_used() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(request("request-a", &item_id, "key-a", "same"), &clock)
        .await
        .unwrap();
    let queue_plan = sqlx::query("EXPLAIN QUERY PLAN SELECT id FROM execution_requests WHERE state = 'queued' ORDER BY created_at")
        .fetch_all(repo.pool()).await.unwrap();
    assert!(queue_plan.iter().any(|row| {
        row.get::<String, _>(3)
            .contains("idx_execution_requests_queue")
    }));
    let lease = claim_lease(
        &repo,
        "runner-a",
        "attempt-a",
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap()
    .unwrap();
    let event = NewEvent {
        id: "row-1",
        event_id: "event-1",
        sequence: 1,
        source: "runner",
        kind: "progress",
        payload: "{}",
        occurred_at: clock.now(),
    };
    assert!(matches!(
        repo.append_execution_events_result(
            EventBatch {
                runner_id: "runner-a",
                attempt_id: "attempt-a",
                fencing_token: lease.fencing_token,
                previous_checkpoint: None,
                checkpoint: "checkpoint-1",
            },
            &[event],
            &clock,
        )
        .await
        .unwrap(),
        EventApplyResult::Applied(_)
    ));
    let history_plan = sqlx::query("EXPLAIN QUERY PLAN SELECT * FROM execution_events WHERE attempt_id = 'attempt-a' ORDER BY sequence")
        .fetch_all(repo.pool()).await.unwrap();
    assert!(history_plan.iter().any(|row| {
        row.get::<String, _>(3)
            .contains("idx_execution_events_timeline")
    }));
}

#[tokio::test]
async fn attempt_start_transitions_are_idempotent_and_freeze_facts() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(
        request("request-start", &item_id, "key-start", "same"),
        &clock,
    )
    .await
    .unwrap();
    let lease = claim_lease(
        &repo,
        "runner-a",
        "attempt-start",
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap()
    .unwrap();

    let preparing = AttemptTransitionInput {
        runner_id: "runner-a",
        attempt_id: "attempt-start",
        fencing_token: lease.fencing_token,
        phase: AttemptTransitionPhase::Preparing,
        workspace_id: "workspace-1",
        base_revision: "abc123",
        process_id: None,
    };
    let prepared_at = match repo
        .transition_attempt_with_facts(preparing.clone(), &clock)
        .await
        .unwrap()
    {
        AttemptTransitionResult::Applied(response) => response.committed_at,
        result => panic!("expected preparation to apply, got {result:?}"),
    };
    clock.advance(Duration::seconds(1));
    assert!(matches!(
        repo.transition_attempt_with_facts(preparing, &clock)
            .await
            .unwrap(),
        AttemptTransitionResult::Replayed(response) if response.committed_at == prepared_at
    ));
    assert_eq!(
        repo.transition_attempt_with_facts(
            AttemptTransitionInput {
                runner_id: "runner-a",
                attempt_id: "attempt-start",
                fencing_token: lease.fencing_token,
                phase: AttemptTransitionPhase::Preparing,
                workspace_id: "workspace-changed",
                base_revision: "abc123",
                process_id: None,
            },
            &clock,
        )
        .await
        .unwrap(),
        AttemptTransitionResult::Conflict
    );

    let running = AttemptTransitionInput {
        runner_id: "runner-a",
        attempt_id: "attempt-start",
        fencing_token: lease.fencing_token,
        phase: AttemptTransitionPhase::Running,
        workspace_id: "workspace-1",
        base_revision: "abc123",
        process_id: Some("process-1"),
    };
    let started_at = match repo
        .transition_attempt_with_facts(running.clone(), &clock)
        .await
        .unwrap()
    {
        AttemptTransitionResult::Applied(response) => response.committed_at,
        result => panic!("expected start to apply, got {result:?}"),
    };
    clock.advance(Duration::seconds(1));
    assert!(matches!(
        repo.transition_attempt_with_facts(
            AttemptTransitionInput {
                phase: AttemptTransitionPhase::Preparing,
                process_id: None,
                ..running.clone()
            },
            &clock,
        )
        .await
        .unwrap(),
        AttemptTransitionResult::Replayed(response)
            if response.state == "preparing" && response.committed_at == prepared_at
    ));
    assert!(matches!(
        repo.transition_attempt_with_facts(running, &clock)
            .await
            .unwrap(),
        AttemptTransitionResult::Replayed(response) if response.committed_at == started_at
    ));
    assert_eq!(
        repo.transition_attempt_with_facts(
            AttemptTransitionInput {
                runner_id: "runner-a",
                attempt_id: "attempt-start",
                fencing_token: lease.fencing_token,
                phase: AttemptTransitionPhase::Running,
                workspace_id: "workspace-1",
                base_revision: "abc123",
                process_id: Some("process-changed"),
            },
            &clock,
        )
        .await
        .unwrap(),
        AttemptTransitionResult::Conflict
    );

    let row = sqlx::query(
        "SELECT state,workspace_id,base_revision,process_id,prepared_at,started_at FROM execution_attempts WHERE id='attempt-start'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("state"), "running");
    assert_eq!(row.get::<String, _>("workspace_id"), "workspace-1");
    assert_eq!(row.get::<String, _>("base_revision"), "abc123");
    assert_eq!(row.get::<String, _>("process_id"), "process-1");
    assert_eq!(row.get::<String, _>("prepared_at"), prepared_at);
    assert_eq!(row.get::<String, _>("started_at"), started_at);
}

#[tokio::test]
async fn attempt_start_rejects_wrong_order_and_stale_authority() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(
        request("request-order", &item_id, "key-order", "same"),
        &clock,
    )
    .await
    .unwrap();
    let lease = claim_lease(
        &repo,
        "runner-a",
        "attempt-order",
        Duration::seconds(5),
        &clock,
    )
    .await
    .unwrap()
    .unwrap();

    let running = AttemptTransitionInput {
        runner_id: "runner-a",
        attempt_id: "attempt-order",
        fencing_token: lease.fencing_token,
        phase: AttemptTransitionPhase::Running,
        workspace_id: "workspace-1",
        base_revision: "abc123",
        process_id: Some("process-1"),
    };
    assert_eq!(
        repo.transition_attempt_with_facts(running.clone(), &clock)
            .await
            .unwrap(),
        AttemptTransitionResult::Conflict
    );
    assert_eq!(
        repo.transition_attempt_with_facts(
            AttemptTransitionInput {
                fencing_token: lease.fencing_token + 1,
                ..running.clone()
            },
            &clock,
        )
        .await
        .unwrap(),
        AttemptTransitionResult::Stale
    );
    clock.advance(Duration::seconds(6));
    assert_eq!(
        repo.transition_attempt_with_facts(
            AttemptTransitionInput {
                phase: AttemptTransitionPhase::Preparing,
                process_id: None,
                ..running
            },
            &clock,
        )
        .await
        .unwrap(),
        AttemptTransitionResult::Stale
    );

    let row = sqlx::query(
        "SELECT state,workspace_id,base_revision,process_id,prepared_at,started_at FROM execution_attempts WHERE id='attempt-order'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("state"), "leased");
    assert!(row.get::<Option<String>, _>("workspace_id").is_none());
    assert!(row.get::<Option<String>, _>("base_revision").is_none());
    assert!(row.get::<Option<String>, _>("process_id").is_none());
    assert!(row.get::<Option<String>, _>("prepared_at").is_none());
    assert!(row.get::<Option<String>, _>("started_at").is_none());
}

// Concurrency regressions: an independent audit of every transaction in this
// module found the same deferred-reader-upgrade deadlock hazard (SQLITE_LOCKED
// "database is deadlocked") in three more functions, each hit by a plausible
// duplicate/retry caller (a runner or operator resending an unacknowledged
// request). The fencing-token/state WHERE clauses on their writes guard the
// *result*, not the SQLite lock-upgrade race, so each needed the same
// BEGIN IMMEDIATE fix as redeem_enrollment_token and
// claim_execution_idempotent_with_snapshot. These tests assert both
// concurrent branches succeed at the sqlx level (a raw Err is a hard
// failure) and that exactly one is authoritative.

#[tokio::test]
async fn concurrent_duplicate_completions_have_one_committed_writer() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-concurrent-complete",
        "attempt-concurrent-complete",
    )
    .await;
    let completion = completion_input(
        "attempt-concurrent-complete",
        fence,
        "completion-concurrent",
        None,
    );
    let left = repo.clone();
    let right = repo.clone();
    let (a, b) = tokio::join!(
        left.complete_execution_result(completion.clone(), &clock),
        right.complete_execution_result(completion, &clock),
    );
    let a = a.expect("first completion report must succeed at the sqlx level");
    let b = b.expect("second completion report must succeed at the sqlx level");

    let committed = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, CompletionResult::Committed(_)))
        .count();
    let replayed = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, CompletionResult::Replayed(_)))
        .count();
    assert_eq!(
        committed, 1,
        "exactly one duplicate report commits: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate report replays: {a:?} / {b:?}"
    );

    // Capacity is restored by the terminal transition exactly once, even
    // though both branches raced to report the same completion.
    let available_capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id='runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(available_capacity, 1, "capacity is restored exactly once");
}

#[tokio::test]
async fn concurrent_duplicate_requeues_have_one_writer() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(
        request(
            "request-concurrent-requeue",
            &item_id,
            "key-concurrent-requeue",
            "same",
        ),
        &clock,
    )
    .await
    .unwrap();
    let lease = claim_lease(
        &repo,
        "runner-a",
        "attempt-concurrent-requeue",
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(matches!(
        repo.recover_attempt(
            recovery_input(
                "attempt-concurrent-requeue",
                lease.fencing_token,
                "recovery-concurrent-requeue",
                RecoveryObservation::Ambiguous,
            ),
            &clock,
        )
        .await
        .unwrap(),
        RecoveryObservationResult::Applied(_)
    ));
    let left = repo.clone();
    let right = repo.clone();
    let (a, b) = tokio::join!(
        left.operator_requeue_needs_operator(
            "request-concurrent-requeue",
            "client-key",
            "operator-a",
            "reason-a",
            &clock,
        ),
        right.operator_requeue_needs_operator(
            "request-concurrent-requeue",
            "client-key",
            "operator-a",
            "reason-a",
            &clock,
        ),
    );
    let a = a.expect("first operator requeue must succeed at the sqlx level");
    let b = b.expect("second operator requeue must succeed at the sqlx level");

    use tack_db::repo::execution::OperatorRequeueResult;
    assert!(
        matches!(
            (&a, &b),
            (
                OperatorRequeueResult::Requeued,
                OperatorRequeueResult::Replayed
            ) | (
                OperatorRequeueResult::Replayed,
                OperatorRequeueResult::Requeued
            )
        ),
        "exactly one duplicate requeue is authoritative: {a:?} / {b:?}"
    );

    let request_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_requests WHERE id='request-concurrent-requeue'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(request_state, "queued");

    // One audit from recover_attempt, one from the winning requeue; the
    // replayed duplicate must not write a second requeue audit.
    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_recovery_audits WHERE attempt_id='attempt-concurrent-requeue'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(audits, 2);
}

#[tokio::test]
async fn concurrent_duplicate_transitions_have_one_applied_writer() {
    let (repo, item_id, clock) = ready_repo().await;
    repo.enqueue_execution(
        request(
            "request-concurrent-transition",
            &item_id,
            "key-concurrent-transition",
            "same",
        ),
        &clock,
    )
    .await
    .unwrap();
    let lease = claim_lease(
        &repo,
        "runner-a",
        "attempt-concurrent-transition",
        Duration::seconds(60),
        &clock,
    )
    .await
    .unwrap()
    .unwrap();
    let preparing = AttemptTransitionInput {
        runner_id: "runner-a",
        attempt_id: "attempt-concurrent-transition",
        fencing_token: lease.fencing_token,
        phase: AttemptTransitionPhase::Preparing,
        workspace_id: "workspace-1",
        base_revision: "abc123",
        process_id: None,
    };
    let left = repo.clone();
    let right = repo.clone();
    let (a, b) = tokio::join!(
        left.transition_attempt_with_facts(preparing.clone(), &clock),
        right.transition_attempt_with_facts(preparing, &clock),
    );
    let a = a.expect("first transition report must succeed at the sqlx level");
    let b = b.expect("second transition report must succeed at the sqlx level");

    let applied = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, AttemptTransitionResult::Applied(_)))
        .count();
    let replayed = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, AttemptTransitionResult::Replayed(_)))
        .count();
    assert_eq!(
        applied, 1,
        "exactly one duplicate report applies: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate report replays: {a:?} / {b:?}"
    );

    let committed_at = |r: &AttemptTransitionResult| match r {
        AttemptTransitionResult::Applied(resp) | AttemptTransitionResult::Replayed(resp) => {
            resp.committed_at.clone()
        }
        other => panic!("expected applied/replayed, got {other:?}"),
    };
    assert_eq!(
        committed_at(&a),
        committed_at(&b),
        "both branches observe the single committed timestamp"
    );

    let row = sqlx::query(
        "SELECT state,prepared_at FROM execution_attempts WHERE id='attempt-concurrent-transition'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("state"), "preparing");
    assert!(row.get::<Option<String>, _>("prepared_at").is_some());
}

#[tokio::test]
async fn concurrent_duplicate_heartbeats_have_one_writer() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-concurrent-heartbeat",
        "attempt-concurrent-heartbeat",
    )
    .await;
    let leases = vec![HeartbeatLease {
        attempt_id: "attempt-concurrent-heartbeat",
        fencing_token: fence,
        state: "running",
        journal_state: "running",
        last_event_checkpoint: None,
    }];
    let sent_at = clock.now();
    let left = repo.clone();
    let right = repo.clone();
    let (a, b) = tokio::join!(
        left.heartbeat_batch(
            "runner-a",
            "heartbeat-concurrent",
            sent_at,
            0,
            &leases,
            Duration::seconds(30),
            &clock,
        ),
        right.heartbeat_batch(
            "runner-a",
            "heartbeat-concurrent",
            sent_at,
            0,
            &leases,
            Duration::seconds(30),
            &clock,
        ),
    );
    let a = a.expect("first heartbeat report must succeed at the sqlx level");
    let b = b.expect("second heartbeat report must succeed at the sqlx level");

    let accepted = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, HeartbeatBatchResult::Accepted(_)))
        .count();
    let replayed = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, HeartbeatBatchResult::Replayed(_)))
        .count();
    assert_eq!(
        accepted, 1,
        "exactly one duplicate heartbeat accepts: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate heartbeat replays: {a:?} / {b:?}"
    );

    let response = |r: &HeartbeatBatchResult| match r {
        HeartbeatBatchResult::Accepted(resp) | HeartbeatBatchResult::Replayed(resp) => resp.clone(),
        other => panic!("expected accepted/replayed, got {other:?}"),
    };
    assert_eq!(
        response(&a),
        response(&b),
        "both branches observe the single committed heartbeat response"
    );

    // Capacity is set exactly once, even though both branches raced to
    // report the same duplicate heartbeat.
    let available_capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id='runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(available_capacity, 0);

    let replay_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_heartbeat_replays WHERE runner_id='runner-a' AND heartbeat_id='heartbeat-concurrent'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(
        replay_rows, 1,
        "exactly one durable replay record is written"
    );
}

#[tokio::test]
async fn concurrent_duplicate_recoveries_have_one_writer() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-concurrent-recovery",
        "attempt-concurrent-recovery",
    )
    .await;
    let input = recovery_input(
        "attempt-concurrent-recovery",
        fence,
        "recovery-concurrent",
        RecoveryObservation::Ambiguous,
    );
    let left = repo.clone();
    let right = repo.clone();
    let (a, b) = tokio::join!(
        left.recover_attempt(input.clone(), &clock),
        right.recover_attempt(input, &clock),
    );
    let a = a.expect("first recovery report must succeed at the sqlx level");
    let b = b.expect("second recovery report must succeed at the sqlx level");

    let applied = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, RecoveryObservationResult::Applied(_)))
        .count();
    let replayed = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, RecoveryObservationResult::Replayed(_)))
        .count();
    assert_eq!(
        applied, 1,
        "exactly one duplicate recovery report applies: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate recovery report replays: {a:?} / {b:?}"
    );

    let response = |r: &RecoveryObservationResult| match r {
        RecoveryObservationResult::Applied(resp) | RecoveryObservationResult::Replayed(resp) => {
            resp.clone()
        }
        other => panic!("expected applied/replayed, got {other:?}"),
    };
    let applied_response = response(&a);
    assert_eq!(
        applied_response,
        response(&b),
        "both branches observe the single committed recovery response"
    );
    assert_eq!(
        applied_response.disposition,
        RecoveryDisposition::NeedsOperator
    );

    let attempt_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_attempts WHERE id='attempt-concurrent-recovery'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(attempt_state, "needs_operator");

    // Capacity is restored by the recovery transition exactly once, even
    // though both branches raced to report the same duplicate observation.
    let available_capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id='runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(available_capacity, 1, "capacity is restored exactly once");

    let audits: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_recovery_audits WHERE attempt_id='attempt-concurrent-recovery' AND recovery_key='recovery-concurrent'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(audits, 1, "exactly one durable audit record is written");
}

#[tokio::test]
async fn concurrent_duplicate_cancellations_have_one_writer() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-concurrent-cancel",
        "attempt-concurrent-cancel",
    )
    .await;
    repo.request_execution_cancellation("request-concurrent-cancel", &clock)
        .await
        .unwrap();
    let input = cancellation_input(
        "attempt-concurrent-cancel",
        fence,
        "cancel-concurrent",
        clock.now(),
    );
    let left = repo.clone();
    let right = repo.clone();
    let (a, b) = tokio::join!(
        left.observe_cancellation(input.clone(), &clock),
        right.observe_cancellation(input, &clock),
    );
    let a = a.expect("first cancellation report must succeed at the sqlx level");
    let b = b.expect("second cancellation report must succeed at the sqlx level");

    let cancelled = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, CancellationObservation::Cancelled(_)))
        .count();
    let replayed = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, CancellationObservation::Replayed(_)))
        .count();
    assert_eq!(
        cancelled, 1,
        "exactly one duplicate cancellation report is authoritative: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate cancellation report replays: {a:?} / {b:?}"
    );

    let response = |r: &CancellationObservation| match r {
        CancellationObservation::Cancelled(resp) | CancellationObservation::Replayed(resp) => {
            resp.clone()
        }
        other => panic!("expected cancelled/replayed, got {other:?}"),
    };
    assert_eq!(
        response(&a),
        response(&b),
        "both branches observe the single committed cancellation response"
    );

    let attempt_state: String = sqlx::query_scalar(
        "SELECT state FROM execution_attempts WHERE id='attempt-concurrent-cancel'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(attempt_state, "cancelled");

    // Capacity is restored by the terminal transition exactly once, even
    // though both branches raced to report the same duplicate observation.
    let available_capacity: i64 =
        sqlx::query_scalar("SELECT available_capacity FROM agent_runners WHERE id='runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(available_capacity, 1, "capacity is restored exactly once");

    let replay_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_cancellation_replays WHERE attempt_id='attempt-concurrent-cancel' AND cancellation_request_id='cancel-concurrent'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(
        replay_rows, 1,
        "exactly one durable replay record is written"
    );
}

#[tokio::test]
async fn concurrent_duplicate_enqueues_have_one_authoritative_writer() {
    let (repo, item_id, clock) = ready_repo().await;
    let left = repo.clone();
    let right = repo.clone();
    let (a, b) = tokio::join!(
        left.enqueue_execution(
            request(
                "request-concurrent-enqueue",
                &item_id,
                "key-concurrent-enqueue",
                "same",
            ),
            &clock,
        ),
        right.enqueue_execution(
            request(
                "request-concurrent-enqueue",
                &item_id,
                "key-concurrent-enqueue",
                "same",
            ),
            &clock,
        ),
    );
    let a = a.expect("first enqueue report must succeed at the sqlx level");
    let b = b.expect("second enqueue report must succeed at the sqlx level");

    let created = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, EnqueueResult::Created(_)))
        .count();
    let replayed = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, EnqueueResult::Replayed(_)))
        .count();
    assert_eq!(
        created, 1,
        "exactly one duplicate enqueue creates: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate enqueue replays: {a:?} / {b:?}"
    );

    let id = |r: &EnqueueResult| match r {
        EnqueueResult::Created(id) | EnqueueResult::Replayed(id) => id.clone(),
        other => panic!("expected created/replayed, got {other:?}"),
    };
    assert_eq!(
        id(&a),
        id(&b),
        "both branches observe the single committed request id"
    );
    assert_eq!(id(&a), "request-concurrent-enqueue");

    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_requests WHERE id='request-concurrent-enqueue'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(rows, 1, "exactly one execution request row is written");
}

#[tokio::test]
async fn concurrent_duplicate_event_batches_have_one_writer() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_completion_attempt(
        &repo,
        &item_id,
        &clock,
        "request-concurrent-events",
        "attempt-concurrent-events",
    )
    .await;
    let events = vec![NewEvent {
        id: "event-row-concurrent",
        event_id: "event-concurrent",
        sequence: 1,
        source: "runner",
        kind: "log",
        payload: r#"{"line":"hello"}"#,
        occurred_at: clock.now(),
    }];
    let batch = EventBatch {
        runner_id: "runner-a",
        attempt_id: "attempt-concurrent-events",
        fencing_token: fence,
        previous_checkpoint: None,
        checkpoint: "checkpoint-concurrent",
    };
    let left = repo.clone();
    let right = repo.clone();
    let (a, b) = tokio::join!(
        left.append_execution_events_result(batch.clone(), &events, &clock),
        right.append_execution_events_result(batch, &events, &clock),
    );
    let a = a.expect("first event batch report must succeed at the sqlx level");
    let b = b.expect("second event batch report must succeed at the sqlx level");

    let fresh = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, EventApplyResult::Applied(result) if !result.replayed))
        .count();
    let replayed = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, EventApplyResult::Applied(result) if result.replayed))
        .count();
    assert_eq!(
        fresh, 1,
        "exactly one duplicate event batch applies fresh: {a:?} / {b:?}"
    );
    assert_eq!(
        replayed, 1,
        "the other duplicate event batch replays: {a:?} / {b:?}"
    );

    let result = |r: &EventApplyResult| match r {
        EventApplyResult::Applied(result) => result.clone(),
        other => panic!("expected applied, got {other:?}"),
    };
    let a_result = result(&a);
    let b_result = result(&b);
    assert_eq!(a_result.accepted_event_ids, b_result.accepted_event_ids);
    assert_eq!(a_result.duplicate_event_ids, b_result.duplicate_event_ids);
    assert_eq!(a_result.committed_checkpoint, b_result.committed_checkpoint);
    assert_eq!(
        a_result.accepted_event_ids,
        vec!["event-concurrent".to_string()]
    );

    let event_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_events WHERE attempt_id='attempt-concurrent-events' AND event_id='event-concurrent'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(event_rows, 1, "the event is persisted exactly once");

    let checkpoint: String = sqlx::query_scalar(
        "SELECT event_checkpoint FROM execution_attempts WHERE id='attempt-concurrent-events'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(checkpoint, "checkpoint-concurrent");

    let replay_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_event_batch_replays WHERE attempt_id='attempt-concurrent-events' AND checkpoint='checkpoint-concurrent'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(
        replay_rows, 1,
        "exactly one durable replay record is written"
    );
}

// Defect 3 regression: two concurrent or retried credential rotations from
// the same runner both authenticate against the same still-valid old hash
// (that is the whole point of a rotation race — neither has learned the
// other's new hash yet). Without a compare-and-set against the hash that was
// actually authenticated, last-writer-wins would silently discard one
// rotation's result, leaving its caller holding a credential the server no
// longer accepts and with no way to recover short of a fresh operator-issued
// enrollment token. `rotate_runner_credential` must let exactly one of two
// concurrent rotations against the same expected hash win.
#[tokio::test]
async fn concurrent_credential_rotations_have_exactly_one_winner() {
    let (repo, _item_id, clock) = ready_repo().await;
    // `ready_repo()` registers "runner-a" with credential_hash "hash-only".
    let left = repo.clone();
    let right = repo.clone();
    let (a, b) = tokio::join!(
        left.rotate_runner_credential(
            "runner-a",
            "hash-only",
            "hash-rotated-by-left",
            clock.now() + Duration::days(30),
            &clock,
        ),
        right.rotate_runner_credential(
            "runner-a",
            "hash-only",
            "hash-rotated-by-right",
            clock.now() + Duration::days(30),
            &clock,
        ),
    );
    let a = a.expect("first rotation must succeed at the sqlx level");
    let b = b.expect("second rotation must succeed at the sqlx level");

    let rotated = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, CredentialRotationResult::Rotated(_)))
        .count();
    let mismatched = [&a, &b]
        .into_iter()
        .filter(|r| matches!(r, CredentialRotationResult::HashMismatch))
        .count();
    assert_eq!(
        rotated, 1,
        "exactly one concurrent rotation against the same expected hash wins: {a:?} / {b:?}"
    );
    assert_eq!(
        mismatched, 1,
        "the other concurrent rotation observes its expected hash no longer matches: {a:?} / {b:?}"
    );

    let stored_hash: String =
        sqlx::query_scalar("SELECT credential_hash FROM agent_runners WHERE id='runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    // Whichever branch won, the stored hash must be exactly that branch's new
    // hash — never the loser's, and never some third, corrupted value.
    let winner_is_left = matches!(a, CredentialRotationResult::Rotated(_));
    let expected = if winner_is_left {
        "hash-rotated-by-left"
    } else {
        "hash-rotated-by-right"
    };
    assert_eq!(
        stored_hash, expected,
        "stored hash must match whichever branch's CredentialRotationResult::Rotated fired"
    );

    // A retry against the now-stale original hash (e.g. a naive client that
    // didn't observe either response and blindly retries with what it still
    // believes is current) must not be able to rotate again.
    let stale_retry = repo
        .rotate_runner_credential(
            "runner-a",
            "hash-only",
            "hash-rotated-by-stale-retry",
            clock.now() + Duration::days(30),
            &clock,
        )
        .await
        .unwrap();
    assert_eq!(stale_retry, CredentialRotationResult::HashMismatch);
    let stored_hash_after_retry: String =
        sqlx::query_scalar("SELECT credential_hash FROM agent_runners WHERE id='runner-a'")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(
        stored_hash_after_retry, expected,
        "a stale-hash retry must not overwrite the winning rotation"
    );
}
