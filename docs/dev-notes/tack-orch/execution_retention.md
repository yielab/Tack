# `crates/tack-orch/src/execution_retention.rs`

Moved out of the module preamble; trim or delete freely.

Cancellable retention sweep for the execution domain.

 # Why this is a sibling module, not `execution::retention`

 `crate::execution`'s own module doc says it "deliberately has no
 transport, persistence, or vendor adapter dependencies" — it is the pure
 runner-v1 protocol domain. This module is the opposite: it is nothing
 *but* persistence and a spawned background task. It lives next to
 `reconciler.rs` (which has the closest analog already: orch's own
 `spawn_retention_sweep`/`RetentionStore`, rolling `orch_events` into
 `orch_events_daily`) rather than inside `execution/`.

 # What's different from orch's own retention sweep

 `reconciler::spawn_retention_sweep` computes its cutoff from `Utc::now()`
 directly and has no cancellation signal at all — dropping its
 `JoinHandle` is the only way to stop it, which cannot prove "shutdown
 joins task". This module fixes both for
 the execution domain: [`RetentionClock`] makes "now" injectable (tests
 never depend on real wall-clock time to decide what counts as stale),
 and [`spawn_execution_retention_sweep`] takes a `stop_rx` raced against
 its inter-sweep sleep via `tokio::select!`, mirroring
 `reconciler::spawn_one`'s own cancellation shape exactly.

 # No roll-up table for `execution_events` exists yet

 Unlike orch (`orch_events` -> `orch_events_daily`), the execution domain
 has no daily-aggregate table for `execution_events`. See
 `tack_db::Repository::purge_stale_terminal_execution_events`'s doc
 comment: adding one would mirror `orch_events`/`orch_events_daily`, but
 until that migration lands, this purges terminal-attempt event rows
 outright rather than aggregating them first, and is documented as doing
 exactly that, not mislabeled as a "roll up."
