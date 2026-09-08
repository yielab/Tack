//! Read-facing endpoints that surface what orchestration has already done:
//! per-item/per-project agent activity, budget/policy cost and denial-rate
//! reporting, the fleet-wide approvals inbox plus its decision endpoint,
//! and a dispatched pipeline run's mirrored state read back by its own id.

#[path = "reporting/agent_activity.rs"]
mod agent_activity;
#[path = "reporting/approvals.rs"]
mod approvals;
#[path = "reporting/budget_policy.rs"]
mod budget_policy;
#[path = "reporting/run_readback.rs"]
mod run_readback;
