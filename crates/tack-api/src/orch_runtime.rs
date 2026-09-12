//! Runtime start/stop control for the orchestration reconciler.
//!
//! This module makes the reconciler's enable flag a runtime setting, rather
//! than a boot-time-only decision (`server.rs` spawning it once, gated on
//! `TACK_ORCH_ENABLE`, with no other way to turn it on or off) — mirroring
//! the Cloud Backup precedent in `handlers/settings.rs`: stored
//! in `app_meta`, with the env var reduced to a deployment default. It
//! gives `PUT /api/settings/orchestration` something to call so flipping
//! the flag takes effect immediately, with no restart.
//!
//! # Start/stop design
//!
//! Each reconciler task (`tack_orch::reconciler::spawn_one`, one per
//! registered control plane) loops: fetch → decide → persist → sleep.
//! Stopping it cleanly needs a signal the task can observe at a safe point
//! — never mid-HTTP-call to docket, and never while a SQLite write is open
//! (unaffected either way, since persistence already happens strictly
//! after the fetch phase completes and before the sleep).
//!
//! [`tokio::sync::watch`] is that signal: a single `bool` channel, `false`
//! meaning "keep going". [`OrchRuntime::stop`] flips it to `true`; every
//! task holds a cloned `Receiver` and races it against its poll-interval
//! sleep with `tokio::select!` (`reconciler::spawn_one`), and also checks
//! it at the top of the next loop iteration before starting a new fetch. A
//! task mid-fetch when `stop()` is called finishes that one tick
//! (fetch/persist, both already short and already in flight) and exits at
//! its very next safe point — bounded by however long the in-flight HTTP
//! call to docket takes, never longer, and never mid-transaction.
//!
//! No new dependency was added for this: `watch` is already part of
//! `tokio`'s `full` feature, already a workspace dependency. `tokio-util`'s
//! `CancellationToken` would work equally well but isn't needed for a
//! single boolean flag with N cloned receivers.
//!
//! # No leaked tasks on repeated toggles
//!
//! [`OrchRuntime`] holds at most one "running" generation at a time behind
//! a `tokio::sync::Mutex`. [`start`](OrchRuntime::start) is a no-op if a
//! generation is already running (never spawns a second set on top of a
//! live one); [`stop`](OrchRuntime::stop) takes the current generation out
//! of the shared state before signalling it, so a `start()` that races a
//! `stop()` can never observe "half stopped" state — see each method's own
//! doc comment. Verified in `tack-orch`'s
//! `reconciler::tests::repeated_global_start_stop_cycles_leave_no_task_running`
//! (three consecutive start/stop cycles, asserting exactly one task per
//! cycle) and, at the HTTP layer, in
//! `tack-api`'s `tests/orchestration/control_plane/settings.rs`.
//!
//! # The list of planes isn't read once
//!
//! `start()` calls `reconciler::spawn_reconcilers_supervised`. The old
//! `reconciler::spawn_reconcilers_cancellable` read `store.list_registered()`
//! exactly once and spawned one `spawn_one` task per plane found at that
//! instant — the list was never re-read. A control plane registered *after*
//! `start()` (the natural
//! "enable orchestration -> register a control plane -> link a project"
//! setup order) would therefore never be polled: no task, no health updates, no
//! error anywhere. `spawn_reconcilers_supervised` does the same initial
//! snapshot synchronously (so a caller checking [`OrchRuntime::
//! live_task_count`] right after `start()` still sees every
//! already-registered plane immediately) but then keeps a background
//! supervisor loop running that re-reads `list_registered()` every
//! `config.supervisor_scan_secs` and starts/stops per-plane pollers to
//! match — self-healing regardless of *how* `control_planes` changed
//! (through the API, a bulk import, or a direct DB edit). See
//! `tack_orch::reconciler`'s module doc, the section headed "Supervisor
//!", for the full design writeup and the alternative
//! (handler-driven notification) that was considered and rejected. This
//! module's own responsibilities are unchanged by that card: `OrchRuntime`
//! still only owns the single global start/stop signal and the
//! at-most-one-generation invariant above; per-plane lifecycle is entirely
//! `tack-orch`'s concern now.

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
