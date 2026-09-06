//! HTTP-handler integration tests for `tack-api`: CRUD across every
//! operator-facing resource, the executions/runner-admin handlers exercised
//! directly against their own routes, the same lifecycle proven again
//! through the real production router, the two focused read-only
//! runner/attempt routes, the attempt-scoped artifact/decision list routes,
//! cross-execution scoping on the attempt-events and artifact-download
//! routes, economics reporting, optimistic item-version concurrency, and
//! template provisioning's rollback behavior.

mod common;

#[path = "handlers/attempt_lists.rs"]
mod attempt_lists;
#[path = "handlers/attempt_scoping.rs"]
mod attempt_scoping;
#[path = "handlers/crud.rs"]
mod crud;
#[path = "handlers/economics.rs"]
mod economics;
#[path = "handlers/executions_runner_admin.rs"]
mod executions_runner_admin;
#[path = "handlers/item_concurrency.rs"]
mod item_concurrency;
#[path = "handlers/local_runner.rs"]
mod local_runner;
#[path = "handlers/operator_read_routes.rs"]
mod operator_read_routes;
#[path = "handlers/production_router.rs"]
mod production_router;
#[path = "handlers/provisioning.rs"]
mod provisioning;
