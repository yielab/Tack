//! Real-database proof for `Repository::purge_stale_execution_replays`,
//! `purge_stale_terminal_execution_events` and `execution_fleet_snapshot`.
//! Fixtures use plain SQL rather than the enqueue/claim protocol flow,
//! since these tables only need valid FK targets, not a fully-formed
//! request. The concurrency tests below use a file-backed pool, not the
//! shared in-memory harness: a shared in-memory pool's cache locking
//! accidentally serializes the exact race being proved.

use crate::common;

use chrono::{DateTime, Duration, Utc};
use tack_db::{Repository, init_pool, migrations};

fn rfc(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

/// Workspace -> project -> item -> one active runner -> one execution
/// request -> one execution attempt, all with real FK-satisfying rows via
/// direct SQL. Returns `(repo, item_id, request_id, attempt_id)`; the
/// runner is always `"runner-a"`.
async fn seed(repo: &Repository, now: DateTime<Utc>) -> (String, String, String) {
    let workspace = common::create_test_workspace(repo).await;
    let project = common::make_project(repo, workspace).await;
    let item = common::make_item(repo, &project).await;
    let item_id = item.id.to_string();
    let now_s = rfc(now);

    sqlx::query(
        "INSERT INTO agent_runners (id, name, credential_hash, state, labels, total_capacity, \
         available_capacity, capability_snapshot, protocol_version, created_at, updated_at) \
         VALUES ('runner-a', 'Runner A', 'hash', 'active', '{}', 1, 1, '{}', 1, ?, ?)",
    )
    .bind(&now_s)
    .bind(&now_s)
    .execute(repo.pool())
    .await
    .unwrap();

    let request_id = "request-a".to_string();
    sqlx::query(
        "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
         request_fingerprint, state, selector_kind, selector_id, agent_profile_snapshot, \
         repository_snapshot, permission_policy, created_at, updated_at) \
         VALUES (?, ?, 'item', 'key-a', 'fp', 'running', 'exact_runner', 'runner-a', '{}', '{}', '{}', ?, ?)",
    )
    .bind(&request_id)
    .bind(&item_id)
    .bind(&now_s)
    .bind(&now_s)
    .execute(repo.pool())
    .await
    .unwrap();

    let attempt_id = "attempt-a".to_string();
    sqlx::query(
        "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, \
         fencing_token, state, lease_issued_at, lease_expires_at, created_at, updated_at) \
         VALUES (?, ?, 1, 'runner-a', 1, 'running', ?, ?, ?, ?)",
    )
    .bind(&attempt_id)
    .bind(&request_id)
    .bind(&now_s)
    .bind(&now_s)
    .bind(&now_s)
    .bind(&now_s)
    .execute(repo.pool())
    .await
    .unwrap();

    (item_id, request_id, attempt_id)
}

#[allow(clippy::too_many_arguments)]
async fn insert_attempt(
    repo: &Repository,
    id: &str,
    request_id: &str,
    attempt_number: i64,
    fencing_token: i64,
    state: &str,
    lease_expires_at: DateTime<Utc>,
    ts: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, \
         fencing_token, state, lease_issued_at, lease_expires_at, created_at, updated_at) \
         VALUES (?, ?, ?, 'runner-a', ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(request_id)
    .bind(attempt_number)
    .bind(fencing_token)
    .bind(state)
    .bind(rfc(ts))
    .bind(rfc(lease_expires_at))
    .bind(rfc(ts))
    .bind(rfc(ts))
    .execute(repo.pool())
    .await
    .unwrap();
}

async fn insert_request(
    repo: &Repository,
    id: &str,
    item_id: &str,
    state: &str,
    ts: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
         request_fingerprint, state, selector_kind, selector_id, agent_profile_snapshot, \
         repository_snapshot, permission_policy, created_at, updated_at) \
         VALUES (?, ?, 'item', ?, 'fp', ?, 'exact_runner', 'runner-a', '{}', '{}', '{}', ?, ?)",
    )
    .bind(id)
    .bind(item_id)
    .bind(id)
    .bind(state)
    .bind(rfc(ts))
    .bind(rfc(ts))
    .execute(repo.pool())
    .await
    .unwrap();
}

async fn insert_event(
    repo: &Repository,
    id: &str,
    attempt_id: &str,
    sequence: i64,
    occurred_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO execution_events (id, attempt_id, event_id, sequence, source, kind, \
         payload, occurred_at, created_at) \
         VALUES (?, ?, ?, ?, 'runner', 'log', '{}', ?, ?)",
    )
    .bind(id)
    .bind(attempt_id)
    .bind(id)
    .bind(sequence)
    .bind(rfc(occurred_at))
    .bind(rfc(occurred_at))
    .execute(repo.pool())
    .await
    .unwrap();
}

async fn insert_claim_replay(
    repo: &Repository,
    claim_request_id: &str,
    attempt_id: &str,
    ts: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO execution_claim_replays (runner_id, claim_request_id, attempt_id, created_at) \
         VALUES ('runner-a', ?, ?, ?)",
    )
    .bind(claim_request_id)
    .bind(attempt_id)
    .bind(rfc(ts))
    .execute(repo.pool())
    .await
    .unwrap();
}

async fn insert_heartbeat_replay(repo: &Repository, heartbeat_id: &str, ts: DateTime<Utc>) {
    sqlx::query(
        "INSERT INTO execution_heartbeat_replays (runner_id, heartbeat_id, response, created_at, fingerprint) \
         VALUES ('runner-a', ?, '{}', ?, 'fp')",
    )
    .bind(heartbeat_id)
    .bind(rfc(ts))
    .execute(repo.pool())
    .await
    .unwrap();
}

async fn insert_cancellation_replay(
    repo: &Repository,
    attempt_id: &str,
    cancellation_request_id: &str,
    ts: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO execution_cancellation_replays \
         (attempt_id, cancellation_request_id, state, created_at, fingerprint, response) \
         VALUES (?, ?, 'observed', ?, 'fp', '{}')",
    )
    .bind(attempt_id)
    .bind(cancellation_request_id)
    .bind(rfc(ts))
    .execute(repo.pool())
    .await
    .unwrap();
}

async fn insert_event_batch_replay(
    repo: &Repository,
    attempt_id: &str,
    checkpoint: &str,
    ts: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO execution_event_batch_replays (attempt_id, checkpoint, fingerprint, response, created_at) \
         VALUES (?, ?, 'fp', '{}', ?)",
    )
    .bind(attempt_id)
    .bind(checkpoint)
    .bind(rfc(ts))
    .execute(repo.pool())
    .await
    .unwrap();
}

async fn insert_completion_replay(
    repo: &Repository,
    attempt_id: &str,
    completion_id: &str,
    ts: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO execution_completion_replays (attempt_id, completion_id, fingerprint, response, committed_at) \
         VALUES (?, ?, 'fp', '{}', ?)",
    )
    .bind(attempt_id)
    .bind(completion_id)
    .bind(rfc(ts))
    .execute(repo.pool())
    .await
    .unwrap();
}

async fn insert_recovery_audit(
    repo: &Repository,
    attempt_id: &str,
    recovery_key: &str,
    ts: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO execution_recovery_audits \
         (attempt_id, recovery_key, classification, details, created_at, fingerprint, response) \
         VALUES (?, ?, 'ambiguous', '{}', ?, 'fp', '{}')",
    )
    .bind(attempt_id)
    .bind(recovery_key)
    .bind(rfc(ts))
    .execute(repo.pool())
    .await
    .unwrap();
}

async fn count(repo: &Repository, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
        .fetch_one(repo.pool())
        .await
        .unwrap()
}

fn now_fixed() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-08-12T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

#[tokio::test]
async fn purge_replays_deletes_only_rows_older_than_cutoff() {
    let repo = common::setup_test_db().await;
    let now = now_fixed();
    let (_item_id, _request_id, attempt_id) = seed(&repo, now).await;

    let old = now - Duration::days(100);
    let fresh = now - Duration::days(1);
    let cutoff = now - Duration::days(90);

    insert_claim_replay(&repo, "claim-old", &attempt_id, old).await;
    insert_claim_replay(&repo, "claim-fresh", &attempt_id, fresh).await;
    insert_heartbeat_replay(&repo, "hb-old", old).await;
    insert_heartbeat_replay(&repo, "hb-fresh", fresh).await;
    insert_cancellation_replay(&repo, &attempt_id, "cancel-old", old).await;
    insert_cancellation_replay(&repo, &attempt_id, "cancel-fresh", fresh).await;
    insert_event_batch_replay(&repo, &attempt_id, "checkpoint-old", old).await;
    insert_event_batch_replay(&repo, &attempt_id, "checkpoint-fresh", fresh).await;
    insert_completion_replay(&repo, &attempt_id, "completion-old", old).await;
    insert_completion_replay(&repo, &attempt_id, "completion-fresh", fresh).await;
    insert_recovery_audit(&repo, &attempt_id, "recovery-old", old).await;
    insert_recovery_audit(&repo, &attempt_id, "recovery-fresh", fresh).await;

    let stats = repo
        .purge_stale_execution_replays(cutoff, 500)
        .await
        .unwrap();

    assert_eq!(stats.rows_purged, 6, "exactly one stale row per table");
    assert_eq!(count(&repo, "execution_claim_replays").await, 1);
    assert_eq!(count(&repo, "execution_heartbeat_replays").await, 1);
    assert_eq!(count(&repo, "execution_cancellation_replays").await, 1);
    assert_eq!(count(&repo, "execution_event_batch_replays").await, 1);
    assert_eq!(count(&repo, "execution_completion_replays").await, 1);
    assert_eq!(count(&repo, "execution_recovery_audits").await, 1);

    // Rows inside the retention window are untouched — not just "some
    // survived," but the *specific* fresh row, by content.
    let remaining_claim: String =
        sqlx::query_scalar("SELECT claim_request_id FROM execution_claim_replays")
            .fetch_one(repo.pool())
            .await
            .unwrap();
    assert_eq!(remaining_claim, "claim-fresh");
}

#[tokio::test]
async fn purge_replays_respects_batch_bound() {
    let repo = common::setup_test_db().await;
    let now = now_fixed();
    let (_item_id, _request_id, attempt_id) = seed(&repo, now).await;
    let old = now - Duration::days(100);
    let cutoff = now - Duration::days(90);

    for i in 0..12 {
        insert_claim_replay(&repo, &format!("claim-{i}"), &attempt_id, old).await;
    }

    let stats = repo.purge_stale_execution_replays(cutoff, 5).await.unwrap();

    // 12 rows at batch_size=5: 5 + 5 + 2 — three transactions, never more
    // than 5 rows deleted in any one of them. The other five (empty)
    // tables contribute zero batches each — an empty candidate set commits
    // and breaks without ever counting as a "batch run" (see the method's
    // own loop: `batches_run` only increments after a non-empty delete).
    assert_eq!(stats.rows_purged, 12);
    assert_eq!(
        stats.batches_run, 3,
        "5 + 5 + 2 across three transactions; empty tables contribute no batches"
    );
    assert_eq!(count(&repo, "execution_claim_replays").await, 0);
}

#[tokio::test]
async fn purge_terminal_events_only_touches_terminal_attempts() {
    let repo = common::setup_test_db().await;
    let now = now_fixed();
    let (_item_id, request_id, terminal_attempt) = seed(&repo, now).await;
    sqlx::query("UPDATE execution_attempts SET state = 'succeeded' WHERE id = ?")
        .bind(&terminal_attempt)
        .execute(repo.pool())
        .await
        .unwrap();

    let active_attempt = "attempt-active".to_string();
    insert_attempt(
        &repo,
        &active_attempt,
        &request_id,
        2,
        2,
        "running",
        now + Duration::hours(1),
        now,
    )
    .await;

    let old = now - Duration::days(100);
    let fresh = now - Duration::days(1);
    let cutoff = now - Duration::days(90);

    insert_event(&repo, "ev-terminal-old", &terminal_attempt, 1, old).await;
    insert_event(&repo, "ev-terminal-fresh", &terminal_attempt, 2, fresh).await;
    insert_event(&repo, "ev-active-old", &active_attempt, 1, old).await;
    insert_event(&repo, "ev-active-fresh", &active_attempt, 2, fresh).await;

    let stats = repo
        .purge_stale_terminal_execution_events(cutoff, 500)
        .await
        .unwrap();

    assert_eq!(
        stats.rows_purged, 1,
        "only the old event on the terminal attempt is purged"
    );
    let remaining: Vec<String> = sqlx::query_scalar("SELECT id FROM execution_events ORDER BY id")
        .fetch_all(repo.pool())
        .await
        .unwrap();
    assert_eq!(
        remaining,
        vec![
            "ev-active-fresh".to_string(),
            "ev-active-old".to_string(),
            "ev-terminal-fresh".to_string(),
        ],
        "the active attempt's events survive regardless of age; only the terminal \
         attempt's stale event is gone"
    );
}

/// A second, pending-enrollment runner and a revoked one, so
/// `runner_state_counts` exercises all three states beyond `seed`'s one
/// active runner.
#[tokio::test]
async fn runner_state_counts_report_all_three_states() {
    let repo = common::setup_test_db().await;
    let now = now_fixed();
    let (_item_id, _request_id, _running_attempt) = seed(&repo, now).await;

    sqlx::query(
        "INSERT INTO agent_runners (id, name, credential_hash, state, labels, total_capacity, \
         available_capacity, capability_snapshot, protocol_version, created_at, updated_at) \
         VALUES ('runner-b', 'Runner B', 'hash', 'pending_enrollment', '{}', 1, 0, '{}', 1, ?, ?)",
    )
    .bind(rfc(now))
    .bind(rfc(now))
    .execute(repo.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_runners (id, name, credential_hash, state, labels, total_capacity, \
         available_capacity, capability_snapshot, protocol_version, revoked_at, created_at, updated_at) \
         VALUES ('runner-c', 'Runner C', 'hash', 'revoked', '{}', 1, 0, '{}', 1, ?, ?, ?)",
    )
    .bind(rfc(now))
    .bind(rfc(now))
    .bind(rfc(now))
    .execute(repo.pool())
    .await
    .unwrap();

    let snapshot = repo
        .execution_fleet_snapshot(now, Duration::hours(1))
        .await
        .unwrap();

    assert_eq!(snapshot.runner_state_counts.get("active").copied(), Some(1));
    assert_eq!(
        snapshot
            .runner_state_counts
            .get("pending_enrollment")
            .copied(),
        Some(1)
    );
    assert_eq!(
        snapshot.runner_state_counts.get("revoked").copied(),
        Some(1)
    );
    assert_eq!(
        snapshot.runner_state_counts.len(),
        3,
        "bounded to the known state vocabulary"
    );
}

/// Two more requests beyond `seed`'s one `running` request: one
/// `needs_operator` (ambiguous, 2 hours old) and one `queued`. Both the
/// state counts and the needs-operator age come from the same real rows.
#[tokio::test]
async fn request_state_counts_include_needs_operator_and_queued() {
    let repo = common::setup_test_db().await;
    let now = now_fixed();
    let (item_id, _request_id, _running_attempt) = seed(&repo, now).await;

    insert_request(
        &repo,
        "request-needs-operator",
        &item_id,
        "needs_operator",
        now - Duration::hours(2),
    )
    .await;
    insert_request(&repo, "request-queued", &item_id, "queued", now).await;

    let snapshot = repo
        .execution_fleet_snapshot(now, Duration::hours(1))
        .await
        .unwrap();

    assert_eq!(
        snapshot.request_state_counts.get("needs_operator").copied(),
        Some(1)
    );
    assert_eq!(
        snapshot.request_state_counts.get("queued").copied(),
        Some(1)
    );
    assert_eq!(
        snapshot.request_state_counts.get("running").copied(),
        Some(1)
    );
    assert_eq!(snapshot.needs_operator_count, 1);
    assert_eq!(snapshot.oldest_needs_operator_age_secs, Some(2 * 3600));
}

/// A stale lease (expired 5 minutes ago, still `running` — the recovery
/// service hasn't caught up) counts; a second attempt on the same request
/// whose lease is also expired but whose state is already terminal must not.
#[tokio::test]
async fn stale_lease_count_excludes_terminal_attempts() {
    let repo = common::setup_test_db().await;
    let now = now_fixed();
    let (_item_id, request_id, running_attempt) = seed(&repo, now).await;

    sqlx::query("UPDATE execution_attempts SET lease_expires_at = ? WHERE id = ?")
        .bind(rfc(now - Duration::minutes(5)))
        .bind(&running_attempt)
        .execute(repo.pool())
        .await
        .unwrap();

    insert_attempt(
        &repo,
        "attempt-terminal-expired-lease",
        &request_id,
        2,
        2,
        "succeeded",
        now - Duration::minutes(30),
        now,
    )
    .await;

    let snapshot = repo
        .execution_fleet_snapshot(now, Duration::hours(1))
        .await
        .unwrap();

    assert_eq!(
        snapshot.stale_lease_count, 1,
        "only the non-terminal expired lease counts"
    );
    assert_eq!(snapshot.oldest_stale_lease_age_secs, Some(300));
}

/// Two events inside a 1-hour trailing window, one outside it.
#[tokio::test]
async fn events_ingested_in_window_counts_only_recent_events() {
    let repo = common::setup_test_db().await;
    let now = now_fixed();
    let (_item_id, _request_id, running_attempt) = seed(&repo, now).await;

    insert_event(
        &repo,
        "ev-in-window-1",
        &running_attempt,
        10,
        now - Duration::minutes(10),
    )
    .await;
    insert_event(
        &repo,
        "ev-in-window-2",
        &running_attempt,
        11,
        now - Duration::minutes(30),
    )
    .await;
    insert_event(
        &repo,
        "ev-outside-window",
        &running_attempt,
        12,
        now - Duration::hours(3),
    )
    .await;

    let snapshot = repo
        .execution_fleet_snapshot(now, Duration::hours(1))
        .await
        .unwrap();

    assert_eq!(snapshot.events_ingested_in_window, 2);
}

/// **Load-bearing concurrency proof for the six-table purge's `BEGIN
/// IMMEDIATE` fix.** Races many concurrent `purge_stale_execution_replays`
/// calls against a *file-backed* database (WAL, `mode=rwc` — the same setup
/// production uses; see the module doc for why a shared in-memory pool
/// would accidentally mask this) with a shared pool of stale rows split
/// into small batches, so both callers' `SELECT`+`DELETE` windows
/// realistically overlap.
///
/// Verified manually per CLAUDE.md's own
/// instruction ("prove any new concurrency test load-bearing... by
/// reverting the fix and watching it fail"): temporarily replacing
/// `self.pool().begin_with("BEGIN IMMEDIATE")` with `self.pool().begin()` in
/// `purge_stale_execution_replays` made this test fail nondeterministically
/// with `sqlx::Error::Database(... "database table is locked" ...)` within
/// the first handful of iterations; restoring `BEGIN IMMEDIATE` made it pass
/// consistently again.
#[tokio::test]
async fn concurrent_purges_never_deadlock_on_file_backed_db() {
    for i in 0..8 {
        let dir = tempfile::tempdir().expect("temporary directory");
        let db_path = dir.path().join("retention-race.db");
        let pool = init_pool(&format!("sqlite://{}?mode=rwc", db_path.display()))
            .await
            .expect("file-backed pool");
        migrations::run_all(&pool).await.expect("migrations");
        let repo = Repository::new(pool);

        let now = now_fixed();
        let (_item_id, _request_id, attempt_id) = seed(&repo, now).await;
        let old = now - Duration::days(100);
        let cutoff = now - Duration::days(90);
        for row in 0..20 {
            insert_claim_replay(&repo, &format!("claim-{row}"), &attempt_id, old).await;
        }

        let (left, right) = tokio::join!(
            repo.purge_stale_execution_replays(cutoff, 3),
            repo.purge_stale_execution_replays(cutoff, 3),
        );
        let left = left.unwrap_or_else(|e| panic!("iteration {i}: left purge errored: {e}"));
        let right = right.unwrap_or_else(|e| panic!("iteration {i}: right purge errored: {e}"));

        assert_eq!(
            left.rows_purged + right.rows_purged,
            20,
            "iteration {i}: every stale row purged exactly once across both racers"
        );
        assert_eq!(
            count(&repo, "execution_claim_replays").await,
            0,
            "iteration {i}: no stale row survives the race"
        );
    }
}

/// Same load-bearing proof as
/// `concurrent_purges_never_deadlock_against_a_file_backed_database`, for
/// `purge_stale_terminal_execution_events`'s own independent `BEGIN
/// IMMEDIATE` transaction. Verified manually the same way: reverting that
/// method's `begin_with("BEGIN IMMEDIATE")` to a plain `.begin()` reproduces
/// `database is locked` under this exact race; restoring it passes
/// consistently.
#[tokio::test]
async fn concurrent_event_purges_never_deadlock_on_file_backed_db() {
    for i in 0..8 {
        let dir = tempfile::tempdir().expect("temporary directory");
        let db_path = dir.path().join("event-race.db");
        let pool = init_pool(&format!("sqlite://{}?mode=rwc", db_path.display()))
            .await
            .expect("file-backed pool");
        migrations::run_all(&pool).await.expect("migrations");
        let repo = Repository::new(pool);

        let now = now_fixed();
        let (_item_id, _request_id, attempt_id) = seed(&repo, now).await;
        sqlx::query("UPDATE execution_attempts SET state = 'succeeded' WHERE id = ?")
            .bind(&attempt_id)
            .execute(repo.pool())
            .await
            .unwrap();
        let old = now - Duration::days(100);
        let cutoff = now - Duration::days(90);
        for row in 0..20 {
            insert_event(&repo, &format!("ev-{row}"), &attempt_id, row, old).await;
        }

        let (left, right) = tokio::join!(
            repo.purge_stale_terminal_execution_events(cutoff, 3),
            repo.purge_stale_terminal_execution_events(cutoff, 3),
        );
        let left = left.unwrap_or_else(|e| panic!("iteration {i}: left purge errored: {e}"));
        let right = right.unwrap_or_else(|e| panic!("iteration {i}: right purge errored: {e}"));

        assert_eq!(
            left.rows_purged + right.rows_purged,
            20,
            "iteration {i}: every stale event purged exactly once across both racers"
        );
        assert_eq!(
            count(&repo, "execution_events").await,
            0,
            "iteration {i}: no stale event survives the race"
        );
    }
}
