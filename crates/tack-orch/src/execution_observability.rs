//! Execution fleet health/observability.
//!
//! Computes a periodic, id-free snapshot of runner/queue/lease/event counts
//! and logs stuck/ambiguous alerts from it. See `execution_retention.rs`'s
//! module doc for why this lives here (a `tack-db`-backed background task)
//! rather than inside `crate::execution` (deliberately I/O-free).
//!
//! # Cardinality is the whole point
//!
//! [`ExecutionFleetSnapshot`] is keyed only by the two small, closed state
//! vocabularies this domain has (`agent_runners.state`: 3 values;
//! `execution_requests.state`: the 10-value `ExecutionState` vocabulary) —
//! never by attempt/request/runner id. That is not an incidental design
//! choice: no prompt/model contents belong in metric labels, and nothing
//! here labels by attempt id, decision id, or anything else unbounded — an
//! unbounded label set is a defect, not a style preference.
//! `tests::snapshot_label_set_is_bounded_and_id_free` proves it.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::watch;
use tracing::{debug, info, warn};

use crate::OrchError;

/// Mirrors `tack_db::repo::execution::ExecutionFleetSnapshotRow`
/// field-for-field — see that type's doc comment for the cardinality
/// guarantee. `Default` is the honest "queried nothing yet" starting point
/// for a background task's own bookkeeping (e.g. `last_alert` in
/// [`spawn_execution_health_watch`]), never presented as a real reading.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionFleetSnapshot {
    pub runner_state_counts: BTreeMap<String, i64>,
    pub request_state_counts: BTreeMap<String, i64>,
    pub stale_lease_count: i64,
    pub oldest_stale_lease_age_secs: Option<i64>,
    pub needs_operator_count: i64,
    pub oldest_needs_operator_age_secs: Option<i64>,
    pub events_ingested_in_window: i64,
}

impl From<tack_db::repo::execution::ExecutionFleetSnapshotRow> for ExecutionFleetSnapshot {
    fn from(row: tack_db::repo::execution::ExecutionFleetSnapshotRow) -> Self {
        Self {
            runner_state_counts: row.runner_state_counts,
            request_state_counts: row.request_state_counts,
            stale_lease_count: row.stale_lease_count,
            oldest_stale_lease_age_secs: row.oldest_stale_lease_age_secs,
            needs_operator_count: row.needs_operator_count,
            oldest_needs_operator_age_secs: row.oldest_needs_operator_age_secs,
            events_ingested_in_window: row.events_ingested_in_window,
        }
    }
}

/// The narrow persistence interface the health watch needs.
#[async_trait]
pub trait ExecutionObservabilityStore: Send + Sync {
    async fn execution_fleet_snapshot(
        &self,
        now: DateTime<Utc>,
        event_window: Duration,
    ) -> Result<ExecutionFleetSnapshot, OrchError>;
}

/// The real, production implementation — see
/// `execution_retention::RepoExecutionRetentionStore`'s doc comment for why
/// this lives here rather than in `tack-api`.
#[derive(Clone)]
pub struct RepoExecutionObservabilityStore(pub tack_db::Repository);

#[async_trait]
impl ExecutionObservabilityStore for RepoExecutionObservabilityStore {
    async fn execution_fleet_snapshot(
        &self,
        now: DateTime<Utc>,
        event_window: Duration,
    ) -> Result<ExecutionFleetSnapshot, OrchError> {
        self.0
            .execution_fleet_snapshot(now, event_window)
            .await
            .map(ExecutionFleetSnapshot::from)
            .map_err(|e| OrchError::Unavailable(format!("execution fleet snapshot failed: {e}")))
    }
}

/// What one snapshot implies about operator attention needed right now.
/// Pure, no I/O — fully unit-testable without a store or a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AlertSummary {
    pub stale_lease_alert: bool,
    pub needs_operator_alert: bool,
}

impl AlertSummary {
    pub fn any(&self) -> bool {
        self.stale_lease_alert || self.needs_operator_alert
    }
}

/// A stale lease or a `needs_operator` request is *always* worth surfacing
/// — both are, by definition, states nothing else in the system resolves
/// on its own (a stale lease has already outlived the recovery
/// service's own window without being reconciled; `needs_operator` is never
/// automatically retried). `count > 0` is deliberately the entire
/// condition — there is no "acceptable" number of ambiguous requests to
/// stay silent about. Log-spam control is a *frequency* decision made by
/// the caller ([`spawn_execution_health_watch`]'s transition-only `warn!`),
/// not a *threshold* decision made here.
pub fn evaluate_alerts(snapshot: &ExecutionFleetSnapshot) -> AlertSummary {
    AlertSummary {
        stale_lease_alert: snapshot.stale_lease_count > 0,
        needs_operator_alert: snapshot.needs_operator_count > 0,
    }
}

/// A clock the health watch asks for "now" every tick — see
/// `execution_retention::RetentionClock`'s doc comment for the identical
/// "inject time" rationale.
pub trait ObservabilityClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl ObservabilityClock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Trailing window used for `events_ingested_in_window` — a coarse
/// ingestion-rate signal, not a precise metric. One hour by default.
pub const DEFAULT_EVENT_WINDOW_SECS: i64 = 3600;

#[derive(Debug, Clone, Copy)]
pub struct ExecutionObservabilityConfig {
    pub check_interval_secs: u64,
    pub event_window_secs: i64,
}

impl Default for ExecutionObservabilityConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 60,
            event_window_secs: DEFAULT_EVENT_WINDOW_SECS,
        }
    }
}

/// See `execution_retention::wait_until_stopped`'s doc comment — identical
/// four-line helper, duplicated rather than shared across an unrelated
/// module boundary.
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

/// Spawn the cancellable execution health watch, or don't (`enabled = false`
/// returns `None` without ever calling `store`) — same shape and
/// cancellation contract as
/// [`execution_retention::spawn_execution_retention_sweep`](crate::execution_retention::spawn_execution_retention_sweep).
///
/// Logs a `warn!` only on the *transition into* an alert condition (and an
/// `info!` on the transition back out), matching `reconciler.rs`'s own
/// "logs backoff at warn without spam" convention: a sustained stuck
/// condition produces one warn on onset, not one every tick, no matter how
/// long it lasts. A `debug!`-level snapshot line is emitted every tick
/// regardless, for anyone tailing logs at debug level.
pub fn spawn_execution_health_watch(
    enabled: bool,
    store: Arc<dyn ExecutionObservabilityStore>,
    clock: Arc<dyn ObservabilityClock>,
    config: ExecutionObservabilityConfig,
    mut stop_rx: watch::Receiver<bool>,
) -> Option<tokio::task::JoinHandle<()>> {
    if !enabled {
        return None;
    }

    Some(tokio::spawn(async move {
        let interval_secs = config.check_interval_secs.max(1);
        let mut last_alert = AlertSummary::default();

        loop {
            if *stop_rx.borrow() {
                info!("execution health watch stopping");
                return;
            }

            let now = clock.now();
            match store
                .execution_fleet_snapshot(now, Duration::seconds(config.event_window_secs))
                .await
            {
                Ok(snapshot) => {
                    let alerts = evaluate_alerts(&snapshot);

                    if alerts.stale_lease_alert && !last_alert.stale_lease_alert {
                        warn!(
                            stale_lease_count = snapshot.stale_lease_count,
                            oldest_stale_lease_age_secs = snapshot.oldest_stale_lease_age_secs,
                            "execution health: stale lease(s) detected"
                        );
                    } else if !alerts.stale_lease_alert && last_alert.stale_lease_alert {
                        info!("execution health: stale lease alert cleared");
                    }

                    if alerts.needs_operator_alert && !last_alert.needs_operator_alert {
                        warn!(
                            needs_operator_count = snapshot.needs_operator_count,
                            oldest_needs_operator_age_secs =
                                snapshot.oldest_needs_operator_age_secs,
                            "execution health: request(s) in needs_operator"
                        );
                    } else if !alerts.needs_operator_alert && last_alert.needs_operator_alert {
                        info!("execution health: needs_operator alert cleared");
                    }

                    debug!(
                        runner_state_counts = ?snapshot.runner_state_counts,
                        request_state_counts = ?snapshot.request_state_counts,
                        events_ingested_in_window = snapshot.events_ingested_in_window,
                        "execution health snapshot"
                    );

                    last_alert = alerts;
                }
                Err(e) => warn!(
                    error = %e,
                    "execution health: snapshot query failed; will retry next cycle"
                ),
            }

            tokio::select! {
                _ = tokio::time::sleep(StdDuration::from_secs(interval_secs)) => {}
                _ = wait_until_stopped(&mut stop_rx) => {
                    info!("execution health watch stopping");
                    return;
                }
            }
        }
    }))
}

#[cfg(test)]
#[path = "execution_observability/tests.rs"]
mod tests;
