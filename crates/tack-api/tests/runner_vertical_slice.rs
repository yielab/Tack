//! Production-repository crash matrix.
//!
//! Exercises `Repository` directly at the repository seam — crash/fault
//! injection around claim, event batching, completion, cancellation,
//! enrollment, and heartbeat, proving each rolls back and replays exactly
//! once — rather than through HTTP; `handlers/production_router.rs` and
//! `wave2_gate.rs` cover the same lifecycle through the real production
//! router.

mod common;

#[path = "runner_vertical_slice/repository_crash.rs"]
mod repository_crash;
