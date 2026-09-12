//! Runner-protocol surface: the runner-facing HTTP handlers (enroll,
//! refresh, claim, heartbeat, decisions, artifact events), the operator
//! decision-resolution endpoint, and artifact upload/download — each proved
//! against its own directly-constructed router rather than the production
//! one.

mod common;

#[path = "runner_protocol/artifact_events.rs"]
mod artifact_events;
#[path = "runner_protocol/decisions.rs"]
mod decisions;
#[path = "runner_protocol/lifecycle.rs"]
mod lifecycle;
#[path = "runner_protocol/log_capture.rs"]
mod log_capture;
