//! The single-item `POST /api/items/{id}/dispatch` endpoint, the guard
//! that keeps it from colliding with the neutral runner-v1 scheduling plane
//! (`execution_requests`) when both are eligible to claim the same item,
//! and the project-level docket pipeline trigger
//! (`POST /api/projects/{id}/orch-dispatch`, ADR 0065).

#[path = "dispatch/dual_scheduling.rs"]
mod dual_scheduling;
#[path = "dispatch/item.rs"]
mod item;
#[path = "dispatch/pipeline.rs"]
mod pipeline;
