//! Repository-level tests: event-batch atomicity and event/artifact
//! retention sweep behavior. HTTP-level artifact-content (streaming,
//! checksum, path-safety) tests live in
//! `crates/tack-api/tests/runner_protocol/artifact_events.rs`; this file only proves
//! what belongs at the `tack-db` layer.

use crate::common;

use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use tack_db::{
    Repository, init_pool, migrations,
    repo::execution::{
        ArtifactContentCommitResult, AttemptTransitionInput, AttemptTransitionPhase,
        EventApplyResult, EventBatch, ExecutionClock, NewAgentProfile, NewArtifact, NewEvent,
        NewExecutionRequest, NewRunner, RequestSelection,
    },
};

struct FakeClock(Mutex<DateTime<Utc>>);

impl FakeClock {
    fn new() -> Self {
        Self(Mutex::new(
            DateTime::parse_from_rfc3339("2026-08-12T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        ))
    }
}

impl ExecutionClock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

async fn ready_repo() -> (Repository, String, FakeClock) {
    ready_repo_on(common::setup_test_db().await).await
}

/// Same seeding as [`ready_repo`], parameterized over an already-constructed
/// `Repository` — lets the concurrent-upload race tests below point it at a
/// *file-backed* pool instead (CLAUDE.md: "prove any new concurrency test
/// load-bearing against a file-backed DB"), without duplicating this whole
/// seeding block.
async fn ready_repo_on(repo: Repository) -> (Repository, String, FakeClock) {
    let workspace = common::create_test_workspace(&repo).await;
    let project = common::make_project(&repo, workspace).await;
    let item = common::make_item(&repo, &project).await;
    let clock = FakeClock::new();
    repo.register_runner(
        NewRunner {
            id: "runner-f2",
            name: "Runner F2",
            credential_hash: "hash-only",
            labels: "{}",
            total_capacity: 2,
            available_capacity: 2,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .unwrap();
    repo.create_agent_profile(
        NewAgentProfile {
            id: "profile-f2",
            name: "Profile F2",
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

/// File-backed pool (WAL, `mode=rwc` — the same setup production uses).
/// A shared in-memory pool can mask a race that only shows up against real
/// file I/O and locking — CLAUDE.md's own warning about this class of test.
///
/// The returned `TempDir` owns the database file and every sidecar SQLite or
/// the migration runner writes beside it; hold it for as long as the repo and
/// they all go away together, on a panicking test as much as a passing one.
async fn file_backed_repo(label: &str) -> (Repository, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temporary directory");
    let db_path = dir.path().join(format!("artifact-race-{label}.db"));
    let pool = init_pool(&format!("sqlite://{}?mode=rwc", db_path.display()))
        .await
        .expect("file-backed pool");
    migrations::run_all(&pool).await.expect("migrations");
    (Repository::new(pool), dir)
}

fn request<'a>(id: &'a str, item_id: &'a str, key: &'a str) -> NewExecutionRequest<'a> {
    let request_snapshot: &'static str = Box::leak(
        format!(
            r#"{{"request_id":"{id}","item_id":"{item_id}","idempotency_key":"{key}","created_by":{{"source":"operator","subject_id":"test"}},"created_at":"2026-08-12T12:00:00Z","selector":{{"kind":"exact_runner","runner_id":"runner-f2"}},"agent_profile_id":"profile-f2","resolved_agent_profile":{{"name":"Profile F2","instructions":"test","tool_policy":{{"mode":"safe"}},"timeout_seconds":60,"budgets":{{"limit":1}}}},"requested_harness_kind":"codex","requested_model_provider":"openai","requested_model_id":"opaque/model","repository":{{"kind":"git","remote":"https://example.test/repo.git","base_revision":"abc123","subdirectory":null}},"permission_policy":{{"tools":["shell"],"network":false}},"timeout_seconds":60,"budgets":{{"limit":1}},"status_map_policy_id":null,"environment":{{}},"metadata":{{}}}}"#
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
        selector_id: "runner-f2",
        agent_profile_id: Some("profile-f2"),
        agent_profile_snapshot: r#"{"name":"Profile F2","instructions":"test","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{"limit":1}}"#,
        requested_harness_kind: Some("codex"),
        requested_model_provider: Some("openai"),
        requested_model_id: Some("opaque/model"),
        repository_snapshot: r#"{"kind":"git","remote":"https://example.test/repo.git","base_revision":"abc123","subdirectory":null}"#,
        permission_policy: r#"{"tools":["shell"],"network":false}"#,
        timeout_seconds: Some(60),
        budgets: r#"{"limit":1}"#,
        status_map_policy_id: None,
        environment: "{}",
        metadata: "{}",
        request_snapshot,
    }
}

/// Enqueues, claims, and drives one attempt all the way to `running` so
/// event/artifact writes are eligible. Returns its fencing token.
async fn ready_running_attempt(
    repo: &Repository,
    item_id: &str,
    clock: &FakeClock,
    request_id: &str,
    attempt_id: &str,
) -> i64 {
    repo.enqueue_execution(request(request_id, item_id, request_id), clock)
        .await
        .unwrap();
    let claim = repo
        .claim_execution_idempotent_with_snapshot(
            "runner-f2",
            attempt_id,
            attempt_id,
            Duration::seconds(300),
            clock,
            RequestSelection::Naive,
        )
        .await
        .unwrap()
        .unwrap();
    let fence = claim.lease.fencing_token;
    repo.transition_attempt_with_facts(
        AttemptTransitionInput {
            runner_id: "runner-f2",
            attempt_id,
            fencing_token: fence,
            phase: AttemptTransitionPhase::Preparing,
            workspace_id: "workspace-1",
            base_revision: "abc123",
            process_id: None,
        },
        clock,
    )
    .await
    .unwrap();
    repo.transition_attempt_with_facts(
        AttemptTransitionInput {
            runner_id: "runner-f2",
            attempt_id,
            fencing_token: fence,
            phase: AttemptTransitionPhase::Running,
            workspace_id: "workspace-1",
            base_revision: "abc123",
            process_id: Some("pid-1"),
        },
        clock,
    )
    .await
    .unwrap();
    fence
}

fn event<'a>(
    id: &'a str,
    event_id: &'a str,
    sequence: i64,
    occurred_at: DateTime<Utc>,
) -> NewEvent<'a> {
    NewEvent {
        id,
        event_id,
        sequence,
        source: "runner",
        kind: "progress",
        payload: r#"{"phase":"test"}"#,
        occurred_at,
    }
}

/// A `kind: "patch"` artifact with the boilerplate fields this file never
/// asserts on (`name`, `media_type`, `size_bytes`, `content_disposition`)
/// pinned to one value; only what each test actually checks is a parameter.
fn patch_artifact<'a>(
    row_id: &'a str,
    artifact_id: &'a str,
    sha256: &'a str,
    content_reference: Option<&'a str>,
) -> NewArtifact<'a> {
    NewArtifact {
        id: row_id,
        artifact_id,
        kind: "patch",
        name: "changes.patch",
        media_type: Some("text/x-diff"),
        size_bytes: 4,
        sha256,
        content_disposition: Some("inline_upload"),
        content_reference,
        metadata: "{}",
    }
}

async fn backdate_artifact(
    repo: &Repository,
    attempt_id: &str,
    artifact_id: &str,
    ts: DateTime<Utc>,
) {
    sqlx::query(
        "UPDATE execution_artifacts SET created_at = ? WHERE attempt_id = ? AND artifact_id = ?",
    )
    .bind(ts.to_rfc3339())
    .bind(attempt_id)
    .bind(artifact_id)
    .execute(repo.pool())
    .await
    .unwrap();
}

async fn checkpoint_for(repo: &Repository, attempt_id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT event_checkpoint FROM execution_attempts WHERE id = ?")
        .bind(attempt_id)
        .fetch_one(repo.pool())
        .await
        .unwrap()
}

async fn event_count_for(repo: &Repository, attempt_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM execution_events WHERE attempt_id = ?")
        .bind(attempt_id)
        .fetch_one(repo.pool())
        .await
        .unwrap()
}

// ---------------------------------------------------------------------
// Acceptance: "checkpoint never advances after failed insert."
// ---------------------------------------------------------------------

struct CheckpointFailureCase {
    label: &'static str,
    attempt_id: &'static str,
    request_id: &'static str,
    /// `Some` seeds one successful batch (establishing `checkpoint-0001`)
    /// before the failing one; `None` means a fresh attempt with no prior
    /// checkpoint.
    seed_first_batch: bool,
    /// `Some(n)` fails only sequence `n`'s insert; `None` fails every insert
    /// in the batch.
    fail_sequence: Option<i64>,
    expected_checkpoint: Option<&'static str>,
}

struct CheckpointFailureOutcome {
    checkpoint: Option<String>,
    event_count: i64,
    replay_rows_for_failed_checkpoint: i64,
}

/// Runs one [`CheckpointFailureCase`] against a fresh repository: seeds the
/// prior batch if the case calls for one, installs the forced-failure
/// trigger, then submits the batch that must fail.
async fn run_checkpoint_failure_case(case: &CheckpointFailureCase) -> CheckpointFailureOutcome {
    let (repo, item_id, clock) = ready_repo().await;
    let fence =
        ready_running_attempt(&repo, &item_id, &clock, case.request_id, case.attempt_id).await;

    if case.seed_first_batch {
        let seeded = repo
            .append_execution_events_result(
                EventBatch {
                    runner_id: "runner-f2",
                    attempt_id: case.attempt_id,
                    fencing_token: fence,
                    previous_checkpoint: None,
                    checkpoint: "checkpoint-0001",
                },
                &[event("evt-row-1", "evt-1", 1, clock.now())],
                &clock,
            )
            .await
            .unwrap();
        assert!(
            matches!(seeded, EventApplyResult::Applied(_)),
            "{}: seed batch must apply",
            case.label
        );
    }

    let trigger_sql = match case.fail_sequence {
        Some(seq) => format!(
            "CREATE TRIGGER f2_fail_{} BEFORE INSERT ON execution_events \
             WHEN NEW.sequence = {seq} BEGIN SELECT RAISE(ABORT, 'forced failure'); END",
            case.label
        ),
        None => format!(
            "CREATE TRIGGER f2_fail_{} BEFORE INSERT ON execution_events \
             BEGIN SELECT RAISE(ABORT, 'forced failure'); END",
            case.label
        ),
    };
    sqlx::query(sqlx::AssertSqlSafe(trigger_sql))
        .execute(repo.pool())
        .await
        .unwrap();

    let (events, previous_checkpoint) = if case.seed_first_batch {
        (
            vec![
                event("evt-row-2", "evt-2", 2, clock.now()),
                event("evt-row-3", "evt-3", 3, clock.now()),
            ],
            Some("checkpoint-0001"),
        )
    } else {
        (vec![event("evt-row-x", "evt-x", 1, clock.now())], None)
    };
    let failing_checkpoint = "checkpoint-0002";
    let result = repo
        .append_execution_events_result(
            EventBatch {
                runner_id: "runner-f2",
                attempt_id: case.attempt_id,
                fencing_token: fence,
                previous_checkpoint,
                checkpoint: failing_checkpoint,
            },
            &events,
            &clock,
        )
        .await;
    assert!(
        result.is_err(),
        "{}: forced trigger must surface as a real error",
        case.label
    );

    let replay_rows_for_failed_checkpoint: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_event_batch_replays WHERE attempt_id = ? AND checkpoint = ?",
    )
    .bind(case.attempt_id)
    .bind(failing_checkpoint)
    .fetch_one(repo.pool())
    .await
    .unwrap();
    CheckpointFailureOutcome {
        checkpoint: checkpoint_for(&repo, case.attempt_id).await,
        event_count: event_count_for(&repo, case.attempt_id).await,
        replay_rows_for_failed_checkpoint,
    }
}

/// Forces an event's INSERT to fail via a `BEFORE INSERT` trigger inside a
/// batch that would otherwise succeed. Proves the whole batch rolls back —
/// zero of its rows land, and `event_checkpoint` stays at its last
/// committed value, never advancing to the failed batch's checkpoint and
/// never regressing — whether that prior value is a real checkpoint or
/// still `NULL` on a fresh attempt.
///
/// Load-bearing: moving the per-event INSERT outside the shared
/// `BEGIN IMMEDIATE` transaction reproduces both a failed assertion here
/// and a genuine self-deadlock (the detached insert blocks on the same
/// connection the outer transaction still holds) — the hazard CLAUDE.md's
/// "`BEGIN IMMEDIATE` is mandatory" rule warns about.
#[tokio::test]
async fn failed_event_batch_leaves_checkpoint_and_rows_untouched() {
    let cases = [
        CheckpointFailureCase {
            label: "prior_checkpoint_holds",
            attempt_id: "attempt-event-atomic",
            request_id: "request-event-atomic",
            seed_first_batch: true,
            fail_sequence: Some(2),
            expected_checkpoint: Some("checkpoint-0001"),
        },
        CheckpointFailureCase {
            label: "fresh_attempt_stays_null",
            attempt_id: "attempt-event-fresh-fail",
            request_id: "request-event-fresh-fail",
            seed_first_batch: false,
            fail_sequence: None,
            expected_checkpoint: None,
        },
    ];
    for case in cases {
        let outcome = run_checkpoint_failure_case(&case).await;
        assert_eq!(
            outcome.checkpoint.as_deref(),
            case.expected_checkpoint,
            "{}: checkpoint",
            case.label
        );
        let expected_events = if case.seed_first_batch { 1 } else { 0 };
        assert_eq!(
            outcome.event_count, expected_events,
            "{}: only rows from a successfully committed batch may exist",
            case.label
        );
        assert_eq!(
            outcome.replay_rows_for_failed_checkpoint, 0,
            "{}: no replay bookkeeping for the failed batch may be committed",
            case.label
        );
    }
}

// ---------------------------------------------------------------------
// `set_execution_artifact_content_reference`: immutability + fencing.
// ---------------------------------------------------------------------

#[tokio::test]
async fn content_reference_is_committed_once_and_never_overwritten() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_running_attempt(
        &repo,
        &item_id,
        &clock,
        "request-content-ref",
        "attempt-content-ref",
    )
    .await;
    let sha = "0".repeat(64);
    let written = repo
        .record_execution_artifact(
            "runner-f2",
            "attempt-content-ref",
            fence,
            patch_artifact("art-row-1", "art-1", &sha, None),
            &clock,
        )
        .await
        .unwrap();
    assert!(written);

    let first = repo
        .set_execution_artifact_content_reference(
            "runner-f2",
            "attempt-content-ref",
            "art-1",
            fence,
            "attempt-content-ref-hex/blob-1",
            &clock,
        )
        .await
        .unwrap();
    assert_eq!(first, ArtifactContentCommitResult::Committed);

    // A second attempt to set it — even to a different value — is refused,
    // not silently overwritten.
    let second = repo
        .set_execution_artifact_content_reference(
            "runner-f2",
            "attempt-content-ref",
            "art-1",
            fence,
            "attempt-content-ref-hex/blob-DIFFERENT",
            &clock,
        )
        .await
        .unwrap();
    assert_eq!(second, ArtifactContentCommitResult::AlreadySet);

    let stored: Option<String> = sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts WHERE attempt_id='attempt-content-ref' AND artifact_id='art-1'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(stored.as_deref(), Some("attempt-content-ref-hex/blob-1"));
}

#[tokio::test]
async fn content_reference_write_with_stale_fence_is_rejected() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_running_attempt(
        &repo,
        &item_id,
        &clock,
        "request-content-stale",
        "attempt-content-stale",
    )
    .await;
    repo.record_execution_artifact(
        "runner-f2",
        "attempt-content-stale",
        fence,
        NewArtifact {
            id: "art-row-2",
            artifact_id: "art-2",
            kind: "log",
            name: "run.log",
            media_type: Some("text/plain"),
            size_bytes: 3,
            sha256: &"1".repeat(64),
            content_disposition: Some("inline_upload"),
            content_reference: None,
            metadata: "{}",
        },
        &clock,
    )
    .await
    .unwrap();

    let result = repo
        .set_execution_artifact_content_reference(
            "runner-f2",
            "attempt-content-stale",
            "art-2",
            fence + 1, // wrong fence
            "attempt-content-stale-hex/blob-2",
            &clock,
        )
        .await
        .unwrap();
    assert_eq!(result, ArtifactContentCommitResult::Stale);

    let stored: Option<String> = sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts WHERE attempt_id='attempt-content-stale' AND artifact_id='art-2'",
    )
    .fetch_one(repo.pool())
    .await
    .unwrap();
    assert_eq!(stored, None);
}

// ---------------------------------------------------------------------
// Retention: `purge_execution_events_older_than` /
// `list_execution_artifacts_older_than` / `delete_execution_artifacts_by_row_ids`.
// ---------------------------------------------------------------------

#[tokio::test]
async fn event_retention_purges_rows_older_than_cutoff_in_batches() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_running_attempt(
        &repo,
        &item_id,
        &clock,
        "request-retention-events",
        "attempt-retention-events",
    )
    .await;
    let old_batch = repo
        .append_execution_events_result(
            EventBatch {
                runner_id: "runner-f2",
                attempt_id: "attempt-retention-events",
                fencing_token: fence,
                previous_checkpoint: None,
                checkpoint: "checkpoint-0001",
            },
            &[
                event("evt-row-old-1", "evt-old-1", 1, clock.now()),
                event("evt-row-old-2", "evt-old-2", 2, clock.now()),
                event("evt-row-old-3", "evt-old-3", 3, clock.now()),
            ],
            &clock,
        )
        .await
        .unwrap();
    assert!(matches!(old_batch, EventApplyResult::Applied(_)));
    let fresh_batch = repo
        .append_execution_events_result(
            EventBatch {
                runner_id: "runner-f2",
                attempt_id: "attempt-retention-events",
                fencing_token: fence,
                previous_checkpoint: Some("checkpoint-0001"),
                checkpoint: "checkpoint-0002",
            },
            &[event("evt-row-fresh-1", "evt-fresh-1", 4, clock.now())],
            &clock,
        )
        .await
        .unwrap();
    assert!(matches!(fresh_batch, EventApplyResult::Applied(_)));

    // Backdate the first batch's rows directly rather than advancing the
    // shared clock 40 days (which would also expire the lease this test's
    // *second* insert still needs to succeed through — retention age and
    // lease liveness are deliberately independent concerns).
    sqlx::query(
        "UPDATE execution_events SET created_at = ? WHERE attempt_id = 'attempt-retention-events' AND sequence IN (1,2,3)",
    )
    .bind((clock.now() - Duration::days(40)).to_rfc3339())
    .execute(repo.pool())
    .await
    .unwrap();

    // Cutoff: 30 days before "now", so the first batch (backdated to 40 days
    // ago) is expired and the second (created just now) is not. Bounded
    // batches of 2: 2 + 1 + 0 across three passes, converging to "caught up."
    let cutoff = clock.now() - Duration::days(30);
    for (pass, expected) in [2, 1, 0].into_iter().enumerate() {
        let purged = repo
            .purge_execution_events_older_than(cutoff, 2)
            .await
            .unwrap();
        assert_eq!(purged, expected, "pass {pass}");
    }

    let remaining: Vec<String> = sqlx::query_scalar(
        "SELECT event_id FROM execution_events WHERE attempt_id='attempt-retention-events' ORDER BY sequence",
    )
    .fetch_all(repo.pool())
    .await
    .unwrap();
    assert_eq!(remaining, vec!["evt-fresh-1".to_string()]);
}

#[tokio::test]
async fn artifact_retention_lists_and_deletes_expired_rows() {
    let (repo, item_id, clock) = ready_repo().await;
    let fence = ready_running_attempt(
        &repo,
        &item_id,
        &clock,
        "request-retention-artifacts",
        "attempt-retention-artifacts",
    )
    .await;
    let sha_old = "2".repeat(64);
    let sha_fresh = "3".repeat(64);
    repo.record_execution_artifact(
        "runner-f2",
        "attempt-retention-artifacts",
        fence,
        patch_artifact(
            "art-row-old",
            "art-old",
            &sha_old,
            Some("attempt-retention-artifacts-hex/old.blob"),
        ),
        &clock,
    )
    .await
    .unwrap();
    repo.record_execution_artifact(
        "runner-f2",
        "attempt-retention-artifacts",
        fence,
        patch_artifact(
            "art-row-fresh",
            "art-fresh",
            &sha_fresh,
            Some("attempt-retention-artifacts-hex/fresh.blob"),
        ),
        &clock,
    )
    .await
    .unwrap();

    // Backdate only the first artifact's row (advancing the shared clock
    // instead would also expire the still-needed lease).
    backdate_artifact(
        &repo,
        "attempt-retention-artifacts",
        "art-old",
        clock.now() - Duration::days(2),
    )
    .await;
    let cutoff = clock.now() - Duration::days(1);

    let expired = repo
        .list_execution_artifacts_older_than(cutoff, 100)
        .await
        .unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].artifact_id, "art-old");

    let ids: Vec<String> = expired.iter().map(|row| row.id.clone()).collect();
    let deleted = repo
        .delete_execution_artifacts_by_row_ids(&ids)
        .await
        .unwrap();
    assert_eq!(deleted, 1);

    let remaining: Vec<String> = sqlx::query_scalar(
        "SELECT artifact_id FROM execution_artifacts WHERE attempt_id='attempt-retention-artifacts' ORDER BY artifact_id",
    )
    .fetch_all(repo.pool())
    .await
    .unwrap();
    assert_eq!(remaining, vec!["art-fresh".to_string()]);
}

#[tokio::test]
async fn delete_by_row_ids_is_a_no_op_on_empty_list() {
    let repo = common::setup_test_db().await;
    assert_eq!(
        repo.delete_execution_artifacts_by_row_ids(&[])
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        repo.delete_unresolved_execution_artifacts_by_row_ids(&[])
            .await
            .unwrap(),
        0
    );
}

// ---------------------------------------------------------------------
// The concurrent-upload race `delete_unresolved_execution_artifacts_by_row_ids`
// closes.
// ---------------------------------------------------------------------

struct GuardCase {
    label: &'static str,
    db_label: &'static str,
    artifact_id: &'static str,
    /// A real content upload lands between the sweep's list and its delete.
    race_happens: bool,
    /// `true` exercises the guarded method; `false` the plain, unconditional
    /// one `sweep_artifacts` uses only for already-resolved rows.
    use_guarded_delete: bool,
    expected_deleted: u64,
}

struct GuardOutcome {
    deleted: u64,
    remaining: i64,
    /// `content_reference` after the delete, only meaningful when the row
    /// survived.
    stored_reference: Option<String>,
}

/// Runs one [`GuardCase`] against a fresh file-backed database: seeds one
/// unresolved artifact, backdates it past the cutoff, lists it, optionally
/// races a real content-reference write against it, then runs the case's
/// chosen delete primitive.
async fn run_guard_case(case: &GuardCase) -> GuardOutcome {
    let (file_repo, _db_dir) = file_backed_repo(case.db_label).await;
    let (repo, item_id, clock) = ready_repo_on(file_repo).await;
    let request_id = format!("request-{}", case.db_label);
    let attempt_id = format!("attempt-{}", case.db_label);
    let fence = ready_running_attempt(&repo, &item_id, &clock, &request_id, &attempt_id).await;

    let sha = "4".repeat(64);
    repo.record_execution_artifact(
        "runner-f2",
        &attempt_id,
        fence,
        // No content yet — exactly the "manifest created, upload not
        // finished" state `sweep_artifacts` must treat as racy.
        patch_artifact(
            &format!("art-row-{}", case.db_label),
            case.artifact_id,
            &sha,
            None,
        ),
        &clock,
    )
    .await
    .unwrap();
    backdate_artifact(
        &repo,
        &attempt_id,
        case.artifact_id,
        clock.now() - Duration::days(2),
    )
    .await;
    let cutoff = clock.now() - Duration::days(1);

    let expired = repo
        .list_execution_artifacts_older_than(cutoff, 100)
        .await
        .unwrap();
    assert_eq!(expired.len(), 1, "{}: backdated row is listed", case.label);
    let ids: Vec<String> = expired.iter().map(|row| row.id.clone()).collect();

    if case.race_happens {
        let result = repo
            .set_execution_artifact_content_reference(
                "runner-f2",
                &attempt_id,
                case.artifact_id,
                fence,
                &format!("{}-hex/real-upload.blob", case.artifact_id),
                &clock,
            )
            .await
            .unwrap();
        assert_eq!(
            result,
            ArtifactContentCommitResult::Committed,
            "{}: race write must land",
            case.label
        );
    }

    let deleted = if case.use_guarded_delete {
        repo.delete_unresolved_execution_artifacts_by_row_ids(&ids)
            .await
            .unwrap()
    } else {
        repo.delete_execution_artifacts_by_row_ids(&ids)
            .await
            .unwrap()
    };

    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_artifacts WHERE attempt_id = ? AND artifact_id = ?",
    )
    .bind(&attempt_id)
    .bind(case.artifact_id)
    .fetch_one(repo.pool())
    .await
    .unwrap();
    let stored_reference: Option<String> = sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts WHERE attempt_id = ? AND artifact_id = ?",
    )
    .bind(&attempt_id)
    .bind(case.artifact_id)
    .fetch_optional(repo.pool())
    .await
    .unwrap()
    .flatten();
    GuardOutcome {
        deleted,
        remaining,
        stored_reference,
    }
}

/// `delete_unresolved_execution_artifacts_by_row_ids` closes a race in
/// `sweep_artifacts` (`handlers/runner_protocol/retention.rs`): a row listed
/// with `content_reference: None` can have a real upload land between the
/// list and the delete, and an unconditional delete would permanently
/// orphan that blob. Exercises the two repository primitives directly, in
/// the exact interleaved order the race requires — list, then a simulated
/// concurrent update, then delete — since the property under test is "does
/// the delete re-check state," not "who wins a scheduling race." Runs
/// against a file-backed database per CLAUDE.md's rule for
/// concurrency-adjacent tests.
#[tokio::test]
async fn artifact_delete_guard_skips_only_racily_resolved_row() {
    let cases = [
        GuardCase {
            label: "guard_skips_raced_row",
            db_label: "guard-skips",
            artifact_id: "art-race-guard",
            race_happens: true,
            use_guarded_delete: true,
            expected_deleted: 0,
        },
        GuardCase {
            label: "unconditional_delete_orphans_raced_row",
            db_label: "guard-counterexample",
            artifact_id: "art-race-counterexample",
            race_happens: true,
            use_guarded_delete: false,
            expected_deleted: 1,
        },
        GuardCase {
            label: "guard_still_deletes_when_nothing_raced",
            db_label: "guard-no-race",
            artifact_id: "art-race-none",
            race_happens: false,
            use_guarded_delete: true,
            expected_deleted: 1,
        },
    ];

    for case in cases {
        let outcome = run_guard_case(&case).await;
        assert_eq!(
            outcome.deleted, case.expected_deleted,
            "{}: rows deleted",
            case.label
        );
        assert_eq!(
            outcome.remaining,
            1 - case.expected_deleted as i64,
            "{}: row survives iff it was not deleted",
            case.label
        );
        if case.race_happens && case.expected_deleted == 0 {
            assert_eq!(
                outcome.stored_reference,
                Some(format!("{}-hex/real-upload.blob", case.artifact_id)),
                "{}: the raced reference survives",
                case.label
            );
        }
    }
}
