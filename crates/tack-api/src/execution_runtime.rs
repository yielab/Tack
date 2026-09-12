//! Runtime start/stop control for the execution-domain retention sweep and
//! health watch.
//!
//! Mirrors `orch_runtime.rs`'s own start/stop shape (a `tokio::sync::watch`
//! stop signal, one "generation" tracked at a time) with one deliberate
//! difference: [`ExecutionRuntime::stop`] *joins* both background tasks
//!
//! Design notes: docs/dev-notes/tack-api/execution_runtime.md

use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::Duration;
use tack_db::Repository;
use tack_orch::execution_observability::{
    self, ExecutionObservabilityConfig, RepoExecutionObservabilityStore,
};
use tack_orch::execution_retention::{self, ExecutionRetentionConfig, RepoExecutionRetentionStore};
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::config::AppConfig;
use crate::handlers::decisions;
use crate::handlers::runner_protocol::artifact_storage::ArtifactStorage;
use crate::handlers::runner_protocol::retention::{
    self as artifact_event_retention, RetentionPolicy,
};

/// What `server.rs` needs from `AppConfig` to start this runtime — a
/// deliberately narrow view (not `&AppConfig` itself) so this module never
/// grows a temptation to reach for an unrelated config field.
///
/// `storage_dir` is `String`, not `Copy` — this type dropped its
/// `Copy` derive accordingly; every existing call site already passes it by
/// value once per `start()` call, so this is a non-breaking widening, not a
/// behavior change.
#[derive(Debug, Clone)]
pub struct ExecutionRuntimeConfig {
    pub retention_enable: bool,
    pub retention_days: u32,
    pub retention_interval_secs: u64,
    pub health_enable: bool,
    pub health_interval_secs: u64,
    /// `AppConfig::storage_dir` (`TACK_STORAGE_DIR`) — the same value
    /// `router.rs::operator_execution_routes` derives its own artifact-
    /// download storage root from
    /// (`format!("{}/execution-artifacts", state.config.storage_dir)`).
    /// [`spawn_artifact_and_decision_sweep`] below builds its
    /// [`ArtifactStorage`] from this field using the **identical** `format!`
    /// expression so the two agree on exactly which directory holds the
    /// blobs — see that function's own doc comment.
    pub storage_dir: String,
}

impl From<&AppConfig> for ExecutionRuntimeConfig {
    fn from(config: &AppConfig) -> Self {
        Self {
            retention_enable: config.execution_retention_enable,
            retention_days: config.execution_retention_days,
            retention_interval_secs: config.execution_retention_interval_secs,
            health_enable: config.execution_health_enable,
            health_interval_secs: config.execution_health_interval_secs,
            storage_dir: config.storage_dir.clone(),
        }
    }
}

struct Running {
    retention_handle: Option<JoinHandle<()>>,
    /// The artifact/event sweep + decision-expiry loop — see
    /// [`spawn_artifact_and_decision_sweep`].
    artifact_decision_handle: Option<JoinHandle<()>>,
    health_handle: Option<JoinHandle<()>>,
    stop_tx: watch::Sender<bool>,
}

/// Handle to the execution-domain retention sweep + health watch. One
/// instance is constructed and started in `server.rs::serve()`; `stop()` is
/// called after `axum::serve(...)` returns so both background tasks are
/// guaranteed joined before the process exits.
///
/// Not stored on `AppState` (unlike [`crate::orch_runtime::OrchRuntime`]):
/// nothing in the current HTTP surface needs to toggle this at runtime. A
/// local variable in `server.rs::serve()` is sufficient; a future
/// runtime-toggle route would need to add the field to `AppState` in
/// `router.rs`.
pub struct ExecutionRuntime {
    inner: Mutex<Option<Running>>,
}

impl Default for ExecutionRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionRuntime {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Start the retention sweep, the artifact/event sweep + decision-expiry
    /// sweep, and/or the health watch, each independently gated by its own
    /// `*_enable` flag (the two retention-flavored sweeps below share
    /// `retention_enable` — see [`spawn_artifact_and_decision_sweep`]'s doc
    /// comment for why they must). Idempotent: a `start()` while a
    /// generation is already running is a no-op (mirrors `OrchRuntime::start`).
    pub async fn start(&self, repo: Repository, config: ExecutionRuntimeConfig) {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            return;
        }

        let (stop_tx, stop_rx) = watch::channel(false);

        let retention_store = Arc::new(RepoExecutionRetentionStore(repo.clone()));
        let retention_clock = Arc::new(execution_retention::SystemClock);
        let retention_handle = execution_retention::spawn_execution_retention_sweep(
            config.retention_enable,
            retention_store,
            retention_clock,
            ExecutionRetentionConfig {
                retention_days: config.retention_days,
                batch_size: execution_retention::DEFAULT_EXECUTION_RETENTION_BATCH_SIZE,
                sweep_interval_secs: config.retention_interval_secs,
            },
            stop_rx.clone(),
        );

        // Same storage-root expression `router.rs`'s
        // `operator_execution_routes` uses for its own artifact-download
        // `ArtifactStorage` — see `ExecutionRuntimeConfig::storage_dir`'s doc
        // comment for why these two independently-constructed instances must
        // agree.
        let artifact_storage = Arc::new(ArtifactStorage::new(format!(
            "{}/execution-artifacts",
            config.storage_dir
        )));
        let artifact_decision_handle = spawn_artifact_and_decision_sweep(
            config.retention_enable,
            repo.clone(),
            artifact_storage,
            config.retention_days,
            config.retention_interval_secs,
            execution_retention::DEFAULT_EXECUTION_RETENTION_BATCH_SIZE,
            stop_rx.clone(),
        );

        let health_store = Arc::new(RepoExecutionObservabilityStore(repo));
        let health_clock = Arc::new(execution_observability::SystemClock);
        let health_handle = execution_observability::spawn_execution_health_watch(
            config.health_enable,
            health_store,
            health_clock,
            ExecutionObservabilityConfig {
                check_interval_secs: config.health_interval_secs,
                event_window_secs: execution_observability::DEFAULT_EVENT_WINDOW_SECS,
            },
            stop_rx,
        );

        *guard = Some(Running {
            retention_handle,
            artifact_decision_handle,
            health_handle,
            stop_tx,
        });
    }

    /// Signal every task to stop, then **join** whichever of them were
    /// actually spawned (a disabled task never had a handle to join in the
    /// first place — that's not a failure to join, it's nothing to join). A
    /// no-op when nothing is running, mirroring `start()`'s idempotency.
    ///
    /// This is the load-bearing difference from `OrchRuntime::stop`: it
    /// really does block until each live task's loop has observed the stop
    /// signal and returned, at whatever safe point that task defines (never
    /// mid-transaction — see `execution_retention`/`execution_observability`'s
    /// own doc comments, and [`spawn_artifact_and_decision_sweep`]'s for the
    /// third).
    pub async fn stop(&self) {
        let mut guard = self.inner.lock().await;
        if let Some(running) = guard.take() {
            let _ = running.stop_tx.send(true);
            if let Some(handle) = running.retention_handle {
                let _ = handle.await;
            }
            if let Some(handle) = running.artifact_decision_handle {
                let _ = handle.await;
            }
            if let Some(handle) = running.health_handle {
                let _ = handle.await;
            }
        }
    }
}

/// Resolves once `stop_rx` carries `true`. Duplicated (not imported) from
/// `tack_orch::execution_retention`'s own private, identically-shaped
/// `wait_until_stopped` — that one is `tack-orch`-internal and this loop
/// cannot depend on it any more than it can depend on the rest of that
/// crate's spawn function; see this file's own "why `tack-orch` cannot grow
/// this" reasoning above. Four lines, same duplication precedent
/// `execution_retention.rs`'s own doc comment already established relative
/// to `reconciler::wait_until_stopped`.
async fn wait_until_stopped(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

/// Spawns the recurring caller `sweep_events`/`sweep_artifacts`
/// (`handlers/runner_protocol/retention.rs`) and
/// `expire_overdue_decisions` (`handlers/decisions.rs`) otherwise lack —
/// both are built and tested in isolation, with no task/interval wiring of
/// their own; this loop is that wiring. Returns `None` (spawns nothing,
/// touches neither the repo nor the filesystem) when `enabled` is `false`,
/// exactly like `execution_retention::spawn_execution_retention_sweep`.
///
/// # Why one shared `enabled`/schedule for three different sweeps
///
/// `enabled` is `config.retention_enable` (`TACK_EXECUTION_RETENTION_ENABLE`,
/// off by default — CLAUDE.md's config table). Artifact/event purging is
/// real data deletion (rows *and* on-disk blobs), so it must never be more
/// permissive than the replay/idempotency purge already gated behind this
/// same flag — riding the identical gate, rather than inventing a second,
/// looser one, is what keeps that true. Decision expiry is not itself a
/// deletion (a `pending` row becomes `expired` in place; nothing is dropped)
/// but is folded into the same task and gate for a narrower reason: it needs
/// *some* recurring caller, this is the only recurring, cancellable,
/// join-on-shutdown task this domain has, and reusing it is simpler and no
/// less safe than inventing a fourth independently-configured background
/// task for one `UPDATE` statement. If a future operator need ever requires
/// decision expiry to run under conditions artifact deletion does not (e.g.
/// on by default), that must be a new, explicit flag — never a loosening of
/// this one, which stays artifact-deletion's floor.
///
/// # Artifact storage root
///
/// `artifact_storage` must be rooted at the exact same directory
/// `router.rs`'s `operator_execution_routes` uses for its own downloads —
/// see [`ExecutionRuntimeConfig::storage_dir`]'s doc comment. Two
/// independently-constructed `ArtifactStorage` values pointed at the same
/// path is intentional (no shared `Arc<ArtifactStorage>` crosses from
/// `router.rs` into this module); they never race against each other,
/// because both open files by path, not by holding any in-process lock this
/// module could instead share.
///
/// # No injected clock
///
/// Unlike `tack_orch::execution_retention` (whose `RetentionClock` seam
/// exists because its own tests need a fixed "now"), this loop reads real
/// wall-clock time (`chrono::Utc::now()`) directly, matching
/// `execution_observability`'s health watch's own precedent in this same
/// file (no injected clock there either). Tests needing an "old" fixture
/// backdate `created_at`/`expires_at` directly via SQL against real
/// wall-clock `now()`, the same technique
/// `crates/tack-db/tests/repository/event_artifact_retention.rs` already uses.
#[allow(clippy::too_many_arguments)]
fn spawn_artifact_and_decision_sweep(
    enabled: bool,
    repo: Repository,
    artifact_storage: Arc<ArtifactStorage>,
    retention_days: u32,
    interval_secs: u64,
    batch_limit: i64,
    mut stop_rx: watch::Receiver<bool>,
) -> Option<JoinHandle<()>> {
    if !enabled {
        return None;
    }

    Some(tokio::spawn(async move {
        let interval_secs = interval_secs.max(1);
        let policy = RetentionPolicy {
            event_retention: Duration::days(retention_days as i64),
            artifact_retention: Duration::days(retention_days as i64),
        };
        loop {
            if *stop_rx.borrow() {
                info!("execution artifact/decision sweep stopping");
                return;
            }

            let now = chrono::Utc::now();

            match artifact_event_retention::sweep_events(&repo, now, &policy, batch_limit).await {
                Ok(n) if n > 0 => {
                    info!(events_deleted = n, "execution retention: events swept")
                }
                Ok(_) => debug!("execution retention: no stale events to sweep"),
                Err(e) => warn!(
                    error = %e,
                    "execution retention: event sweep failed; will retry next cycle"
                ),
            }

            match artifact_event_retention::sweep_artifacts(
                &repo,
                &artifact_storage,
                now,
                &policy,
                batch_limit,
            )
            .await
            {
                Ok(outcome)
                    if outcome.artifacts_deleted > 0 || outcome.artifacts_without_a_blob > 0 =>
                {
                    info!(
                        artifacts_deleted = outcome.artifacts_deleted,
                        artifacts_without_a_blob = outcome.artifacts_without_a_blob,
                        "execution retention: artifacts swept"
                    )
                }
                Ok(_) => debug!("execution retention: no stale artifacts to sweep"),
                Err(e) => warn!(
                    error = %e,
                    "execution retention: artifact sweep failed; will retry next cycle"
                ),
            }

            match decisions::expire_overdue_decisions(repo.pool(), now).await {
                Ok(n) if n > 0 => info!(
                    decisions_expired = n,
                    "execution retention: overdue decisions expired"
                ),
                Ok(_) => debug!("execution retention: no overdue decisions to expire"),
                Err(e) => warn!(
                    error = %e,
                    "execution retention: decision expiry failed; will retry next cycle"
                ),
            }

            tokio::select! {
                _ = tokio::time::sleep(StdDuration::from_secs(interval_secs)) => {}
                _ = wait_until_stopped(&mut stop_rx) => {
                    info!("execution artifact/decision sweep stopping");
                    return;
                }
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tack_db::{init_pool, migrations};

    async fn test_repo() -> Repository {
        let pool = init_pool("sqlite::memory:").await.unwrap();
        migrations::run_all(&pool).await.unwrap();
        Repository::new(pool)
    }

    /// An arbitrary, never-created path is safe here: every test below
    /// starts from an empty in-memory DB, so `sweep_artifacts`'s
    /// `list_execution_artifacts_older_than` read returns nothing and the
    /// function returns before ever touching `ArtifactStorage`/the
    /// filesystem — see `spawn_artifact_and_decision_sweep`'s own doc
    /// comment. Real on-disk behavior is proved separately in
    /// `crates/tack-api/tests/wiring/execution_sweep.rs`.
    fn unused_storage_dir() -> String {
        "/tmp/tack-api-execution-runtime-test-unused-storage-dir".to_string()
    }

    #[tokio::test]
    async fn start_then_stop_leaves_nothing_running_and_stop_actually_joins() {
        let runtime = ExecutionRuntime::new();
        let repo = test_repo().await;
        runtime
            .start(
                repo,
                ExecutionRuntimeConfig {
                    retention_enable: true,
                    retention_days: 90,
                    retention_interval_secs: 3600,
                    health_enable: true,
                    health_interval_secs: 3600,
                    storage_dir: unused_storage_dir(),
                },
            )
            .await;

        // `stop()` returning at all (rather than hanging) is itself part of
        // the proof: every spawned task's `tokio::select!` loop must have
        // observed the stop signal and returned for their `JoinHandle`s to
        // resolve.
        runtime.stop().await;

        // A second stop() is a harmless no-op (nothing left to signal or
        // join) — mirrors OrchRuntime's own idempotency guarantee.
        runtime.stop().await;
    }

    #[tokio::test]
    async fn disabled_config_starts_no_tasks_and_stop_is_still_a_harmless_no_op() {
        let runtime = ExecutionRuntime::new();
        let repo = test_repo().await;
        runtime
            .start(
                repo,
                ExecutionRuntimeConfig {
                    retention_enable: false,
                    retention_days: 90,
                    retention_interval_secs: 3600,
                    health_enable: false,
                    health_interval_secs: 3600,
                    storage_dir: unused_storage_dir(),
                },
            )
            .await;
        runtime.stop().await;
    }

    #[tokio::test]
    async fn start_is_idempotent_while_already_running() {
        let runtime = ExecutionRuntime::new();
        let repo = test_repo().await;
        let config = ExecutionRuntimeConfig {
            retention_enable: true,
            retention_days: 90,
            retention_interval_secs: 3600,
            health_enable: true,
            health_interval_secs: 3600,
            storage_dir: unused_storage_dir(),
        };
        runtime.start(repo.clone(), config.clone()).await;
        // A second start() while the first generation is live must not
        // spawn a second set of tasks (would leak, and a subsequent single
        // stop() would only join one generation's handles) — this only
        // proves it doesn't hang/panic; the no-duplicate-generation
        // invariant itself mirrors `OrchRuntime::start`'s own tested
        // behavior (`repeated_global_start_stop_cycles_leave_no_task_running`
        // in `tack-orch`), same `guard.is_some()` early-return shape.
        runtime.start(repo, config).await;
        runtime.stop().await;
    }
}
