//! Production-runtime proof for the real spawned background
//! tasks (`spawn_execution_retention_sweep`, `spawn_execution_health_watch`)
//! driving the real `tack_db::Repository`-backed stores against a real
//! SQLite database, with an injected clock — not the fake-store unit tests
//! in `execution_retention.rs`/`execution_observability.rs`'s own
//! `#[cfg(test)]` modules, and not calling the repository sweep functions
//! directly. CLAUDE.md is explicit that this distinction matters: "prove it
//! against the real spawned task with an injected clock — not by calling
//! the sweep function directly and asserting it returns Ok."

use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use tack_db::{Repository, init_pool, migrations};
use tack_orch::execution_observability::{
    ExecutionFleetSnapshot, ExecutionObservabilityConfig, ExecutionObservabilityStore,
    ObservabilityClock, RepoExecutionObservabilityStore, spawn_execution_health_watch,
};
use tack_orch::execution_retention::{
    ExecutionRetentionConfig, RepoExecutionRetentionStore, RetentionClock,
    spawn_execution_retention_sweep,
};
use tokio::sync::{Mutex as AsyncMutex, watch};

fn rfc(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

struct FixedClock(DateTime<Utc>);
impl RetentionClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}
impl ObservabilityClock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

fn now_fixed() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-08-12T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

async fn claim_replay_count(repo: &Repository) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM execution_claim_replays")
        .fetch_one(repo.pool())
        .await
        .unwrap()
}

async fn insert_claim_replay(
    repo: &Repository,
    claim_id: &str,
    attempt_id: &str,
    created_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO execution_claim_replays (runner_id, claim_request_id, attempt_id, created_at) \
         VALUES ('runner-a', ?, ?, ?)",
    )
    .bind(claim_id)
    .bind(attempt_id)
    .bind(rfc(created_at))
    .execute(repo.pool())
    .await
    .unwrap();
}

/// Polls `condition` every 10ms, returning `true` as soon as it holds, or
/// `false` once `timeout` elapses — the actual completion signal a spawned
/// background task produces, in place of a fixed sleep guessing how long a
/// tick takes.
async fn poll_until<F, Fut>(timeout: StdDuration, mut condition: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    poll_for(timeout, || {
        let condition = condition();
        async move { condition.await.then_some(()) }
    })
    .await
    .is_some()
}

/// Polls `probe` every 10ms until it returns `Some`, or `None` once
/// `timeout` elapses — for a condition that also carries the value the
/// completed background task produced.
async fn poll_for<F, Fut, T>(timeout: StdDuration, mut probe: F) -> Option<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    let mut ticker = tokio::time::interval(StdDuration::from_millis(10));
    loop {
        ticker.tick().await;
        if let Some(value) = probe().await {
            return Some(value);
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
    }
}

/// Fresh file-backed database (WAL, `mode=rwc` — production's own setup),
/// migrated, with one workspace/project/item/runner/request/attempt seeded
/// via direct SQL (this crate has no `tack-db` test-fixture harness to
/// import — see `crates/tack-db/tests/repository/execution_retention.rs`'s
/// own module doc for the identical reasoning). Returns `(repo, attempt_id,
/// dir)`; the `TempDir` owns the database file and every sidecar written
/// beside it, so holding it as long as the repo is the whole cleanup.
async fn seed_real_db(now: DateTime<Utc>) -> (Repository, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temporary directory");
    let db_path = dir.path().join("retention.db");
    let pool = init_pool(&format!("sqlite://{}?mode=rwc", db_path.display()))
        .await
        .expect("file-backed pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);
    let now_s = rfc(now);

    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES ('ws-a', 'Test Workspace', '{}')",
    )
    .execute(repo.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO projects (id, workspace_id, name, created_at, updated_at) \
         VALUES ('project-a', 'ws-a', 'Test Project', ?, ?)",
    )
    .bind(&now_s)
    .bind(&now_s)
    .execute(repo.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO items (id, project_id, title, item_type, status, priority, created_at, updated_at) \
         VALUES ('item-a', 'project-a', 'Test Item', 'task', 'todo', 'medium', ?, ?)",
    )
    .bind(&now_s)
    .bind(&now_s)
    .execute(repo.pool())
    .await
    .unwrap();
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
    sqlx::query(
        "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
         request_fingerprint, state, selector_kind, selector_id, agent_profile_snapshot, \
         repository_snapshot, permission_policy, created_at, updated_at) \
         VALUES ('request-a', 'item-a', 'item', 'key-a', 'fp', 'running', 'exact_runner', 'runner-a', '{}', '{}', '{}', ?, ?)",
    )
    .bind(&now_s)
    .bind(&now_s)
    .execute(repo.pool())
    .await
    .unwrap();
    let attempt_id = "attempt-a".to_string();
    sqlx::query(
        "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, \
         fencing_token, state, lease_issued_at, lease_expires_at, created_at, updated_at) \
         VALUES (?, 'request-a', 1, 'runner-a', 1, 'running', ?, ?, ?, ?)",
    )
    .bind(&attempt_id)
    .bind(&now_s)
    .bind(&now_s)
    .bind(&now_s)
    .bind(&now_s)
    .execute(repo.pool())
    .await
    .unwrap();

    (repo, attempt_id, dir)
}

/// The headline claim: "stale raw rows roll up/purge
/// in production runtime." This spawns the *real* task
/// (`spawn_execution_retention_sweep`) wired to the *real*
/// `RepoExecutionRetentionStore` over a *real*, file-backed
/// `tack_db::Repository`, with an injected clock so staleness is
/// deterministic — and separately proves "shutdown joins task": the
/// `JoinHandle` is genuinely awaited to completion, and a row inserted
/// *after* that join produces no further purge, ever (nothing left running
/// to purge it).
#[tokio::test]
async fn spawned_retention_sweep_purges_stale_rows_and_joins_on_stop() {
    let now = now_fixed();
    let (repo, attempt_id, _db_dir) = seed_real_db(now).await;
    let old = now - Duration::days(100);
    insert_claim_replay(&repo, "claim-old", &attempt_id, old).await;
    assert_eq!(
        claim_replay_count(&repo).await,
        1,
        "the stale row exists before the sweep runs"
    );

    let store = Arc::new(RepoExecutionRetentionStore(repo.clone()));
    let clock: Arc<dyn RetentionClock> = Arc::new(FixedClock(now));
    let (stop_tx, stop_rx) = watch::channel(false);
    let config = ExecutionRetentionConfig {
        retention_days: 90,
        batch_size: 500,
        sweep_interval_secs: 3600,
    };

    let handle = spawn_execution_retention_sweep(true, store, clock, config, stop_rx)
        .expect("enabled sweep spawns a task");

    // Real task, real tokio scheduler, real database — wait for the first
    // immediate tick to purge the real row.
    let purged = poll_until(StdDuration::from_secs(3), || async {
        claim_replay_count(&repo).await == 0
    })
    .await;
    assert!(
        purged,
        "the real spawned task purged the real stale row through the real store within 3s"
    );

    let _ = stop_tx.send(true);
    handle
        .await
        .expect("shutdown joins the task: the JoinHandle actually completes");

    // Insert a fresh stale row *after* the task has been joined. `handle`
    // already resolved above, so no wait is needed here: a `JoinHandle`
    // only resolves once its task has fully stopped, and a stopped task
    // cannot run a later tick no matter how long this test waits.
    insert_claim_replay(&repo, "claim-old-2", &attempt_id, old).await;
    assert_eq!(
        claim_replay_count(&repo).await,
        1,
        "no purge happens after the join handle completed — the task is truly gone"
    );
}

/// A thin spy wrapping the real, production `RepoExecutionObservabilityStore`
/// — every call is forwarded to the real repo/real database, and the
/// resulting snapshot is also recorded so the test can assert on it. This
/// is not a fake: the snapshot values it captures are exactly what the real
/// spawned task saw from the real database.
struct SpyObservabilityStore {
    inner: RepoExecutionObservabilityStore,
    last_snapshot: AsyncMutex<Option<ExecutionFleetSnapshot>>,
}

#[async_trait::async_trait]
impl ExecutionObservabilityStore for SpyObservabilityStore {
    async fn execution_fleet_snapshot(
        &self,
        now: DateTime<Utc>,
        event_window: Duration,
    ) -> Result<ExecutionFleetSnapshot, tack_orch::OrchError> {
        let snapshot = self
            .inner
            .execution_fleet_snapshot(now, event_window)
            .await?;
        *self.last_snapshot.lock().await = Some(snapshot.clone());
        Ok(snapshot)
    }
}

/// Proves "stale lease and `needs_operator` are observable" against the
/// real spawned health-watch task and a real database: seeds a genuinely
/// expired, non-terminal lease and a genuinely `needs_operator` request,
/// then confirms the real task's real snapshot (captured via the spy above,
/// not asserted by calling the repo method directly) reports both.
#[tokio::test]
async fn spawned_health_watch_reports_stale_lease_and_needs_operator() {
    let now = now_fixed();
    let (repo, attempt_id, _db_dir) = seed_real_db(now).await;

    // Expire this attempt's lease without changing its (non-terminal) state
    // — exactly the "recovery service hasn't caught up yet" scenario.
    sqlx::query("UPDATE execution_attempts SET lease_expires_at = ? WHERE id = ?")
        .bind(rfc(now - Duration::minutes(5)))
        .bind(&attempt_id)
        .execute(repo.pool())
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
         request_fingerprint, state, selector_kind, selector_id, agent_profile_snapshot, \
         repository_snapshot, permission_policy, created_at, updated_at) \
         VALUES ('request-needs-operator', 'item-a', 'item', 'key-b', 'fp', 'needs_operator', \
         'exact_runner', 'runner-a', '{}', '{}', '{}', ?, ?)",
    )
    .bind(rfc(now - Duration::hours(1)))
    .bind(rfc(now - Duration::hours(1)))
    .execute(repo.pool())
    .await
    .unwrap();

    let spy = Arc::new(SpyObservabilityStore {
        inner: RepoExecutionObservabilityStore(repo.clone()),
        last_snapshot: AsyncMutex::new(None),
    });
    let clock: Arc<dyn ObservabilityClock> = Arc::new(FixedClock(now));
    let (stop_tx, stop_rx) = watch::channel(false);
    let config = ExecutionObservabilityConfig {
        check_interval_secs: 3600,
        event_window_secs: 3600,
    };

    let handle = spawn_execution_health_watch(true, spy.clone(), clock, config, stop_rx)
        .expect("enabled watch spawns a task");

    let snapshot = poll_for(StdDuration::from_secs(3), || async {
        spy.last_snapshot.lock().await.clone()
    })
    .await
    .expect("the real spawned task captured a snapshot within 3s");

    assert_eq!(
        snapshot.stale_lease_count, 1,
        "the real spawned task observed the real expired, non-terminal lease"
    );
    assert_eq!(
        snapshot.needs_operator_count, 1,
        "the real spawned task observed the real needs_operator request"
    );

    let _ = stop_tx.send(true);
    handle.await.expect("shutdown joins the task");
}
