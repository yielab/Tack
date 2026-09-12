//! The deterministic fleet scheduler.
//!
//! This module is a pure decision library: given a candidate set of runners
//! (their current health/capacity/labels/declared harness and model
//! support) and a request (exact runner or fleet selector, required
//! harness, optional provider/model, priority), it returns either a
//!
//! Design notes: docs/dev-notes/tack-orch/scheduler/mod.md

pub mod batch;
pub mod select;
pub mod types;
pub mod wiring;

pub use batch::schedule;
pub use select::{SchedulingError, SchedulingPolicy, select_runner};
pub use types::{
    IneligibleReason, ModelSelector, Priority, RunnerCandidate, RunnerState, SchedulingRequest,
    Selection, SelectionOutcome,
};
pub use wiring::choose_request_for_runner;
