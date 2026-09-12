//! Runs/approvals ingestion, trace ingestion, and the production retention
//! and health-watch background tasks, grouped into one nextest binary: all
//! three drive the real `reconciler`/`execution_retention`/
//! `execution_observability` machinery against a real, migrated, in-memory
//! `tack_db::Repository` rather than the fake-store unit tests each module
//! keeps under its own `#[cfg(test)]`.

#[path = "common/mod.rs"]
mod common;
#[path = "ingestion/retention.rs"]
mod retention;
#[path = "ingestion/runs.rs"]
mod runs;
#[path = "ingestion/support.rs"]
mod support;
#[path = "ingestion/traces.rs"]
mod traces;
