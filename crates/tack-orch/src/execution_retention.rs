//! Cancellable retention sweep for the execution domain.
//!
//! # Why this is a sibling module, not `execution::retention`
//!
//! `crate::execution`'s own module doc says it "deliberately has no
//! transport, persistence, or vendor adapter dependencies" — it is the pure
//!
//! Design notes: docs/dev-notes/tack-orch/execution_retention.md

use std::sync::Arc;
use std::time::Duration as StdDuration;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::watch;
use tracing::{debug, info, warn};

use crate::OrchError;

/// Outcome of one table-group purge call. Mirrors
/// `tack_db::repo::execution::PurgeStats` field-for-field so
/// [`RepoExecutionRetentionStore`] is a direct pass-through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PurgeOutcome {
    pub rows_purged: i64,
    pub batches_run: i64,
}

impl From<tack_db::repo::execution::PurgeStats> for PurgeOutcome {
    fn from(stats: tack_db::repo::execution::PurgeStats) -> Self {
        Self {
            rows_purged: stats.rows_purged,
            batches_run: stats.batches_run,
        }
    }
}

/// The narrow persistence interface the execution retention sweep needs.
/// Deliberately its own trait, not a method bolted onto an existing one:
/// retention runs fleet-wide, independent of any single execution request
/// or attempt, and needs none of a scheduling/observability seam's other
/// machinery. Mirrors `reconciler::RetentionStore`'s own shape and reasoning
/// for the orch domain.
#[async_trait]
pub trait ExecutionRetentionStore: Send + Sync {
    /// Purge stale replay/idempotency bookkeeping rows older than `cutoff`.
    /// See `tack_db::Repository::purge_stale_execution_replays`'s doc
    /// comment for which six tables, and why plain deletion (not roll-up)
    /// is the *correct* behavior there, not a shortcut.
    async fn purge_stale_replays(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<PurgeOutcome, OrchError>;

    /// Purge `execution_events` rows belonging to terminated attempts, older
    /// than `cutoff`. See
    /// `tack_db::Repository::purge_stale_terminal_execution_events`'s doc
    /// comment for the terminal-only scoping and the roll-up migration that
    /// would be needed to preserve an aggregate instead of discarding one.
    async fn purge_stale_terminal_events(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<PurgeOutcome, OrchError>;
}

/// The real, production implementation: a thin pass-through to
/// `tack_db::Repository`. Lives here (not in `tack-api`) because `tack-orch`
/// already depends on `tack-db` directly (`scheduler::wiring` needs it too)
/// — a second, `tack-api`-side wrapper would just be forwarding calls with
/// no logic of its own.
#[derive(Clone)]
pub struct RepoExecutionRetentionStore(pub tack_db::Repository);

#[async_trait]
impl ExecutionRetentionStore for RepoExecutionRetentionStore {
    async fn purge_stale_replays(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<PurgeOutcome, OrchError> {
        self.0
            .purge_stale_execution_replays(cutoff, batch_size)
            .await
            .map(PurgeOutcome::from)
            .map_err(|e| OrchError::Unavailable(format!("execution replay purge failed: {e}")))
    }

    async fn purge_stale_terminal_events(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<PurgeOutcome, OrchError> {
        self.0
            .purge_stale_terminal_execution_events(cutoff, batch_size)
            .await
            .map(PurgeOutcome::from)
            .map_err(|e| OrchError::Unavailable(format!("execution event purge failed: {e}")))
    }
}

/// Default retention window in days — mirrors the orch precedent
/// (`TACK_ORCH_EVENT_RETENTION_DAYS`, also 90) and
/// `AppConfig::execution_retention_days`'s own default
/// (`crates/tack-api/src/config.rs`).
pub const DEFAULT_EXECUTION_RETENTION_DAYS: u32 = 90;

/// Rows processed per purge transaction — see
/// `tack_db::Repository::purge_stale_execution_replays`'s doc comment for
/// why this is bounded rather than one transaction for the whole backlog.
pub const DEFAULT_EXECUTION_RETENTION_BATCH_SIZE: i64 = 500;

/// A clock the retention sweep asks for "now" every tick, so tests can
/// inject a fixed instant instead of depending on real wall-clock time to
/// decide what counts as stale (CLAUDE.md: "inject time"). Production uses
/// [`SystemClock`].
pub trait RetentionClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl RetentionClock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ExecutionRetentionConfig {
    pub retention_days: u32,
    pub batch_size: i64,
    pub sweep_interval_secs: u64,
}

impl Default for ExecutionRetentionConfig {
    fn default() -> Self {
        Self {
            retention_days: DEFAULT_EXECUTION_RETENTION_DAYS,
            batch_size: DEFAULT_EXECUTION_RETENTION_BATCH_SIZE,
            sweep_interval_secs: 3600,
        }
    }
}

/// Resolves once `stop_rx` carries `true` — either because it already did
/// when called, or because a later `send(true)` changes it. Resolves
/// (rather than hanging forever) if the sender is dropped without ever
/// sending `true`. Mirrors `reconciler::wait_until_stopped` exactly
/// (private there, four lines — duplicated rather than exported across an
/// unrelated module boundary for something this small).
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

/// Spawn the cancellable execution-retention sweep, or don't — the same
/// off-by-default-when-disabled contract as `reconciler::spawn_retention_sweep`:
/// `enabled = false` returns `None` immediately without ever calling `store`.
///
/// Runs immediately (no initial delay), then every
/// `config.sweep_interval_secs`, until `stop_rx` carries `true` — checked at
/// the top of every loop iteration and raced against the inter-sweep sleep
/// via `tokio::select!`. Both are safe points: a purge batch is already a
/// short, bounded, independently-committed transaction (see
/// `ExecutionRetentionStore`'s doc comment), so a stop signal is observed
/// between sweeps, never mid-transaction.
///
/// This function only arranges for the task to *notice* the stop signal and
/// exit; it does not join the returned handle itself. The caller proves
/// "shutdown joins task" by awaiting the handle after signalling —
/// `crates/tack-api/src/execution_runtime.rs::ExecutionRuntime::stop` does
/// exactly that.
pub fn spawn_execution_retention_sweep(
    enabled: bool,
    store: Arc<dyn ExecutionRetentionStore>,
    clock: Arc<dyn RetentionClock>,
    config: ExecutionRetentionConfig,
    mut stop_rx: watch::Receiver<bool>,
) -> Option<tokio::task::JoinHandle<()>> {
    if !enabled {
        return None;
    }

    Some(tokio::spawn(async move {
        let interval_secs = config.sweep_interval_secs.max(1);
        loop {
            if *stop_rx.borrow() {
                info!("execution retention sweep stopping");
                return;
            }

            let cutoff = clock.now() - Duration::days(config.retention_days as i64);

            match store.purge_stale_replays(cutoff, config.batch_size).await {
                Ok(outcome) if outcome.rows_purged > 0 => info!(
                    rows_purged = outcome.rows_purged,
                    batches = outcome.batches_run,
                    "execution retention: replay bookkeeping purged"
                ),
                Ok(_) => debug!("execution retention: no stale replay bookkeeping to purge"),
                Err(e) => warn!(
                    error = %e,
                    "execution retention: replay purge failed; will retry next cycle"
                ),
            }

            match store
                .purge_stale_terminal_events(cutoff, config.batch_size)
                .await
            {
                Ok(outcome) if outcome.rows_purged > 0 => info!(
                    rows_purged = outcome.rows_purged,
                    batches = outcome.batches_run,
                    "execution retention: terminal execution_events purged"
                ),
                Ok(_) => debug!("execution retention: no stale terminal events to purge"),
                Err(e) => warn!(
                    error = %e,
                    "execution retention: event purge failed; will retry next cycle"
                ),
            }

            tokio::select! {
                _ = tokio::time::sleep(StdDuration::from_secs(interval_secs)) => {}
                _ = wait_until_stopped(&mut stop_rx) => {
                    info!("execution retention sweep stopping");
                    return;
                }
            }
        }
    }))
}

#[cfg(test)]
#[path = "execution_retention/tests.rs"]
mod tests;
