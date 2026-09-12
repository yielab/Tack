# `crates/tack-api/src/orch_runtime.rs`

Moved out of the module preamble; trim or delete freely.

Runtime start/stop control for the orchestration reconciler.

 This module makes the reconciler's enable flag a runtime setting, rather
 than a boot-time-only decision (`server.rs` spawning it once, gated on
 `TACK_ORCH_ENABLE`, with no other way to turn it on or off) — mirroring
 the Cloud Backup precedent in `handlers/settings.rs`: stored
 in `app_meta`, with the env var reduced to a deployment default. It
 gives `PUT /api/settings/orchestration` something to call so flipping
 the flag takes effect immediately, with no restart.

 # Start/stop design

 Each reconciler task (`tack_orch::reconciler::spawn_one`, one per
 registered control plane) loops: fetch → decide → persist → sleep.
 Stopping it cleanly needs a signal the task can observe at a safe point
 — never mid-HTTP-call to docket, and never while a SQLite write is open
 (unaffected either way, since persistence already happens strictly
 after the fetch phase completes and before the sleep).

 [`tokio::sync::watch`] is that signal: a single `bool` channel, `false`
 meaning "keep going". [`OrchRuntime::stop`] flips it to `true`; every
 task holds a cloned `Receiver` and races it against its poll-interval
 sleep with `tokio::select!` (`reconciler::spawn_one`), and also checks
 it at the top of the next loop iteration before starting a new fetch. A
 task mid-fetch when `stop()` is called finishes that one tick
 (fetch/persist, both already short and already in flight) and exits at
 its very next safe point — bounded by however long the in-flight HTTP
 call to docket takes, never longer, and never mid-transaction.

 No new dependency was added for this: `watch` is already part of
 `tokio`'s `full` feature, already a workspace dependency. `tokio-util`'s
 `CancellationToken` would work equally well but isn't needed for a
 single boolean flag with N cloned receivers.

 # No leaked tasks on repeated toggles

 [`OrchRuntime`] holds at most one "running" generation at a time behind
 a `tokio::sync::Mutex`. [`start`](OrchRuntime::start) is a no-op if a
 generation is already running (never spawns a second set on top of a
 live one); [`stop`](OrchRuntime::stop) takes the current generation out
 of the shared state before signalling it, so a `start()` that races a
 `stop()` can never observe "half stopped" state — see each method's own
 doc comment. Verified in `tack-orch`'s
 `reconciler::tests::repeated_global_start_stop_cycles_leave_no_task_running`
 (three consecutive start/stop cycles, asserting exactly one task per
 cycle) and, at the HTTP layer, in
 `tack-api`'s `tests/orchestration/control_plane/settings.rs`.

 # The list of planes isn't read once

 `start()` calls `reconciler::spawn_reconcilers_supervised`. The old
 `reconciler::spawn_reconcilers_cancellable` read `store.list_registered()`
 exactly once and spawned one `spawn_one` task per plane found at that
 instant — the list was never re-read. A control plane registered *after*
 `start()` (the natural
 "enable orchestration -> register a control plane -> link a project"
 setup order) would therefore never be polled: no task, no health updates, no
 error anywhere. `spawn_reconcilers_supervised` does the same initial
 snapshot synchronously (so a caller checking [`OrchRuntime::
 live_task_count`] right after `start()` still sees every
 already-registered plane immediately) but then keeps a background
 supervisor loop running that re-reads `list_registered()` every
 `config.supervisor_scan_secs` and starts/stops per-plane pollers to
 match — self-healing regardless of *how* `control_planes` changed
 (through the API, a bulk import, or a direct DB edit). See
 `tack_orch::reconciler`'s module doc, the section headed "Supervisor
", for the full design writeup and the alternative
 (handler-driven notification) that was considered and rejected. This
 module's own responsibilities are unchanged by that card: `OrchRuntime`
 still only owns the single global start/stop signal and the
 at-most-one-generation invariant above; per-plane lifecycle is entirely
 `tack-orch`'s concern now.
