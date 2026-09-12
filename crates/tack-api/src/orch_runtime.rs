//! Runtime start/stop control for the orchestration reconciler.
//!
//! This module makes the reconciler's enable flag a runtime setting, rather
//! than a boot-time-only decision (`server.rs` spawning it once, gated on
//! `TACK_ORCH_ENABLE`, with no other way to turn it on or off) — mirroring
//! the Cloud Backup precedent in `handlers/settings.rs`: stored
//!
//! Design notes: docs/dev-notes/tack-api/orch_runtime.md

use std::sync::Arc;

use tokio::sync::{Mutex, watch};

use tack_orch::reconciler::{self, ControlPlaneStore, ReconcilerConfig, SupervisedReconciler};

/// A live supervised reconciler run plus the shutdown signal that stops it
/// (and, transitively, every per-plane poller it's currently tracking — see
/// `reconciler::supervisor_loop`'s doc comment).
struct Running {
    reconciler: SupervisedReconciler,
    stop_tx: watch::Sender<bool>,
}

/// Shared, toggleable handle to the orchestration reconciler. One instance
/// lives on `AppState` (`Clone`, cheap — an `Arc<Mutex<..>>` underneath) so
/// both the boot path (`server.rs`) and `PUT /api/settings/orchestration`
/// (`handlers/settings.rs`) start and stop the exact same set of tasks.
#[derive(Clone)]
pub struct OrchRuntime {
    inner: Arc<Mutex<Option<Running>>>,
}

impl Default for OrchRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl OrchRuntime {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    /// Start a self-healing reconciler run: one poller per
    /// currently-registered control plane, kept in sync with
    /// `control_planes` for as long as this generation stays running — see
    /// `reconciler::spawn_reconcilers_supervised`'s doc comment for why a
    /// one-time snapshot isn't enough: a control plane registered *after*
    /// `start()` would otherwise never get polled, silently.
    /// Idempotent: a `start()` while a generation is already running is a
    /// no-op — it does not spawn a duplicate set alongside the live one.
    /// (Calling `start()` twice in a row happens naturally if the operator
    /// sends `PUT {"enabled": true}` more than once, or the value was
    /// already `true` from the environment at boot.)
    pub async fn start(&self, store: Arc<dyn ControlPlaneStore>, config: ReconcilerConfig) {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            return;
        }
        let (stop_tx, stop_rx) = watch::channel(false);
        let reconciler = reconciler::spawn_reconcilers_supervised(store, config, stop_rx).await;
        *guard = Some(Running {
            reconciler,
            stop_tx,
        });
    }

    /// Signal every running task to stop at its next safe point, and drop
    /// this runtime's reference to them. A no-op (not an error) when
    /// nothing is running — mirrors `start()`'s idempotency.
    ///
    /// Does not block waiting for the tasks to actually exit: a toggle-off
    /// HTTP request must not hang on whatever docket's response latency
    /// happens to be for an in-flight poll. Signalling `stop_tx` stops both
    /// the supervisor loop itself (so it starts polling no *new* planes)
    /// and, via the supervisor's own shutdown path, every per-plane poller
    /// it was tracking at that moment — see the module doc's start/stop
    /// design section and `reconciler::supervisor_loop`'s doc comment.
    pub async fn stop(&self) {
        let mut guard = self.inner.lock().await;
        if let Some(running) = guard.take() {
            // The supervisor (and its pollers) may already have exited on
            // their own in principle (they don't today — reconciler tasks
            // don't exit on poll failure — but this keeps `send` from being
            // treated as a bug if a future change ever makes one). Ignore a
            // failed send: every receiver being gone just means everything
            // already stopped.
            let _ = running.stop_tx.send(true);
        }
    }

    /// Number of per-plane pollers currently alive (spawned and not yet
    /// observed to have exited). `0` both when disabled and when enabled
    /// with zero registered control planes — this method reports whether a
    /// task is actually polling something, not whether the feature is
    /// switched on. `GET /api/settings/orchestration`'s `reconciler_running`
    /// is `live_task_count() > 0`; see `handlers/settings.rs`.
    pub async fn live_task_count(&self) -> usize {
        let guard = self.inner.lock().await;
        match guard.as_ref() {
            Some(running) => running.reconciler.live_task_count().await,
            None => 0,
        }
    }
}

#[cfg(test)]
#[path = "orch_runtime/tests.rs"]
mod tests;
