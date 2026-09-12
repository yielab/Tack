# `crates/tack-api/src/execution_runtime.rs`

Moved out of the module preamble; trim or delete freely.

Runtime start/stop control for the execution-domain retention sweep and
 health watch.

 Mirrors `orch_runtime.rs`'s own start/stop shape (a `tokio::sync::watch`
 stop signal, one "generation" tracked at a time) with one deliberate
 difference: [`ExecutionRuntime::stop`] *joins* both background tasks
 before returning, where `OrchRuntime::stop` explicitly does not (its own
 doc comment: "does not block waiting for the tasks to actually exit").
 Shutdown here must join the task — a real `.await` on the `JoinHandle`,
 not merely a signal-and-return — so this type cannot reuse that
 precedent's semantics even though the surrounding shape is the same.
 `server.rs` calls this once, after `axum::serve(...)` returns from
 graceful shutdown, so a slightly-blocking join here costs nothing: by
 that point every HTTP request has already stopped.

 # Why this file is thin

 All retention/observability *logic* — the cancellable spawn loops, the
 `ExecutionRetentionStore`/`ExecutionObservabilityStore` traits, and their
 real `tack_db::Repository`-backed implementations — lives in
 `tack_orch::execution_retention`/`execution_observability` (that crate
 already depends on `tack-db` directly; see those modules' own doc
 comments for why the concrete adapters live there rather than being
 re-implemented in this crate). This module only wires configuration +
 the repository into those spawn functions and gives `server.rs` one
 `start()`/`stop()` pair to call.

 # A second, `tack-api`-local sweep

 `sweep_artifacts`/`sweep_events` (`handlers/runner_protocol/retention.rs`)
 and `expire_overdue_decisions` (`handlers/decisions.rs`) are both built
 and tested in isolation, with no recurring-task wiring of their own —
 this module is that wiring. They cannot be added to
 `tack_orch::execution_retention::spawn_execution_retention_sweep` the way
 the "why this file is thin" section above describes, because both live in
 `tack-api`, and `tack-orch` must never depend on `tack-api` (CLAUDE.md:
 "the dependency points inward, `tack-api` depends on this crate"). So
 `spawn_artifact_and_decision_sweep` below is a second, `tack-api`-local
 spawn loop, structurally mirroring `execution_retention`'s own (same
 `watch`-based stop signal, same immediate-first-tick-then-interval
 shape, same "log and retry next cycle" error handling) but calling
 `tack-api`'s own functions directly instead of going through a
 cross-crate trait. It rides the exact same `TACK_EXECUTION_RETENTION_*`
 config (`enable`/`days`/`interval_secs`) as the sweep above — one
 schedule, one gate, for every deletion this domain performs. This file
 is therefore no longer *only* wiring in the narrowest sense (it now owns
 one real loop's control flow), but the loop's body is still nothing but
 calls into other modules' already-tested functions — no new retention
 *policy* is decided here.
