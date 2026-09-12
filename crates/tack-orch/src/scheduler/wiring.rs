//! Live wiring between the pure [`super::select`]/[`super::batch`] decision
//! functions and real `agent_runners`/`agent_fleet_members`/
//! `execution_requests` rows.
//!
//! [`choose_request_for_runner`] is the one entry point:
//! `crates/tack-api/src/handlers/runner_protocol.rs`'s `claim` handler calls
//!
//! Design notes: docs/dev-notes/tack-orch/scheduler/wiring.md

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use tack_db::Repository;
use tack_db::repo::execution::{FleetConcurrencySnapshot, QueuedRequestForScheduling};

use super::select::{SchedulingPolicy, select_runner};
use super::types::{
    ModelSelector, Priority, RunnerCandidate, RunnerState, SchedulingRequest, SelectionOutcome,
};
use crate::execution::{
    EmbeddedCapabilitySnapshot, ExecutionRequestId, HarnessKind, RequestedModelId,
    RequestedModelProvider, RunnerId,
};

/// Maps `agent_runners.state`'s three literal values
/// (`crates/tack-db/src/migrations.rs`, migration 040) to the typed
/// [`RunnerState`]. An unrecognised string (never written by this codebase,
/// but a raw `TEXT` column has no enum constraint) maps to
/// [`RunnerState::Revoked`] — the most conservative reading: unsupported is
/// typed, unknown is explicit, rather than treating unknown data as
/// schedulable.
fn runner_state_from_str(state: &str) -> RunnerState {
    match state {
        "pending_enrollment" => RunnerState::PendingEnrollment,
        "active" => RunnerState::Active,
        _ => RunnerState::Revoked,
    }
}

/// Reads an optional `{"priority": "low"|"normal"|"high"}` convention out of
/// a request's `metadata` JSON — see this module's doc comment. Never
/// errors: anything that isn't exactly one of the three recognised strings
/// (missing key, wrong type, malformed JSON, unrecognised value) yields
/// [`Priority::Normal`].
fn priority_from_metadata(metadata_json: &str) -> Priority {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(metadata_json) else {
        return Priority::Normal;
    };
    match value.get("priority").and_then(serde_json::Value::as_str) {
        Some(raw) if raw.eq_ignore_ascii_case("low") => Priority::Low,
        Some(raw) if raw.eq_ignore_ascii_case("high") => Priority::High,
        _ => Priority::Normal,
    }
}

/// Parses `created_at` (always RFC 3339 — every write path in
/// `crates/tack-db/src/repo/execution.rs` stores `DateTime<Utc>::to_rfc3339()`)
/// falling back to `now` only if a row is somehow malformed, so a single bad
/// row degrades to "scheduled as if it just arrived" instead of panicking or
/// dropping it from consideration.
fn parse_created_at(raw: &str, now: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or(now)
}

/// Whether `fleet_id`'s configured `concurrency_limit` has already been
/// reached or exceeded by its members' aggregate in-use capacity. `None`
/// `concurrency_limit` (the default — migration 039) means "no ceiling,"
/// never saturated. A fleet with no `FleetConcurrencySnapshot` at all
/// (should not happen for a `fleet`-selector request that passed
/// `POST /executions`' existing validation, but data can outlive validation)
/// is treated as saturated — the conservative reading for "we could not
/// prove this fleet has room."
fn fleet_is_saturated(snapshot: Option<&FleetConcurrencySnapshot>) -> bool {
    match snapshot {
        None => true,
        Some(FleetConcurrencySnapshot {
            concurrency_limit: None,
            ..
        }) => false,
        Some(FleetConcurrencySnapshot {
            concurrency_limit: Some(limit),
            in_use,
        }) => *in_use >= *limit,
    }
}

/// Builds a [`SchedulingRequest`] from a raw queued-request row, or `None`
/// if the row cannot be turned into one — a malformed/partial model
/// selector (`ModelSelector::from_parts`'s `SchedulingError`) or an absent
/// `requested_harness_kind` (never produced by `enqueue_execution`'s own
/// snapshot validation, but the raw column has no `NOT NULL`; see
/// `QueuedRequestForScheduling`'s doc comment). Either case removes this one
/// request from consideration rather than failing the whole claim — the
/// request simply stays queued, visible to an operator via `GET
/// /executions`, until it is fixed or explicitly requeued.
fn build_scheduling_request(
    row: &QueuedRequestForScheduling,
    now: DateTime<Utc>,
) -> Option<SchedulingRequest> {
    let harness_kind = row.requested_harness_kind.as_deref()?;
    let requested_model = ModelSelector::from_parts(
        row.requested_model_provider
            .as_deref()
            .map(RequestedModelProvider::new),
        row.requested_model_id.as_deref().map(RequestedModelId::new),
    )
    .ok()?;
    let selector = match row.selector_kind.as_str() {
        "exact_runner" => crate::execution::RunnerSelector::ExactRunner {
            runner_id: RunnerId::new(row.selector_id.clone()),
        },
        "fleet" => crate::execution::RunnerSelector::Fleet {
            fleet_id: row.selector_id.clone(),
        },
        _ => return None,
    };
    Some(SchedulingRequest {
        request_id: ExecutionRequestId::new(row.id.clone()),
        selector,
        priority: priority_from_metadata(&row.metadata),
        requested_harness_kind: HarnessKind::new(harness_kind),
        requested_model,
        required_labels: BTreeMap::new(),
        created_at: parse_created_at(&row.created_at, now),
    })
}

/// Fetches this runner's own scheduling state plus every request it is
/// selector-eligible for, runs the pure scheduler, and returns the single
/// request id (if any) this runner should attempt to claim.
///
/// Returns `Ok(None)` for "no eligible work" (the runner does not exist,
/// reported no harnesses, is not `active`, every candidate request is
/// ineligible, or there were no queued requests at all) — the caller must
/// treat that identically to "report `no work`," never a naive fallback
/// (see `tack_db::repo::execution::RequestSelection::Scheduled`'s doc
/// comment). Returns `Err` only for a genuine database error, which the
/// caller should map to `internal_error`, exactly as it already does for
/// every other `sqlx::Error` in the claim handler.
pub async fn choose_request_for_runner(
    repo: &Repository,
    runner_id: &str,
    now: DateTime<Utc>,
    policy: &SchedulingPolicy,
) -> Result<Option<String>, sqlx::Error> {
    let Some(runner) = repo.fetch_runner_scheduling_snapshot(runner_id).await? else {
        return Ok(None);
    };
    let queued = repo.list_eligible_queued_requests(runner_id).await?;
    if queued.is_empty() {
        return Ok(None);
    }

    let parsed_capabilities =
        serde_json::from_str::<EmbeddedCapabilitySnapshot>(&runner.capability_snapshot).ok();
    // A runner that has never enrolled/refreshed carries the column default
    // `'{}'`, which does not parse as `EmbeddedCapabilitySnapshot` (several
    // fields are required) — "no declared harnesses" is the honest reading,
    // not a database error.
    let harnesses = parsed_capabilities
        .as_ref()
        .map(|snapshot| snapshot.harnesses.clone())
        .unwrap_or_default();
    // `agent_runners.last_heartbeat_at` is set only by the runner-v1
    // `/heartbeat` batch, which reports *active attempt lease* renewals
    // (`crates/tack-db/src/repo/execution.rs`'s `heartbeat_batch`) — a
    // runner with zero active attempts never calls it, so a freshly
    // enrolled runner polling for its very first claim has `NULL` here.
    // Reading that `NULL` as "stale" (`evaluate_candidate` does, for
    // any `None`) would make every runner permanently unschedulable until
    // it had already been granted a lease once — a deadlock this module
    // resolves by falling back to the capability snapshot's own
    // `reported_at`: enroll/refresh already requires the runner to attest
    // "as of this instant, I am alive and this is what I support"
    // (`docs/contracts/runner-v1/enrollment.request.json`/
    // `refresh.request.json`), which is exactly the same liveness claim a
    // heartbeat makes, just under a different operation name. Both are
    // still subject to `policy.max_heartbeat_age` — a runner that neither
    // heartbeats nor refreshes for that long still goes stale.
    let last_heartbeat_at = runner
        .last_heartbeat_at
        .as_deref()
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|value| value.with_timezone(&Utc))
        .or_else(|| {
            parsed_capabilities
                .as_ref()
                .map(|snapshot| snapshot.reported_at)
        });
    let labels: BTreeMap<String, String> = serde_json::from_str(&runner.labels).unwrap_or_default();
    let candidate = RunnerCandidate {
        runner_id: RunnerId::new(runner.runner_id.clone()),
        state: runner_state_from_str(&runner.state),
        fleet_memberships: runner.fleet_ids.iter().cloned().collect(),
        labels,
        total_capacity: u32::try_from(runner.total_capacity).unwrap_or(0),
        available_capacity: u32::try_from(runner.available_capacity).unwrap_or(0),
        last_heartbeat_at,
        harnesses,
    };

    // Pre-filter fleet-selector requests whose target fleet is already at
    // (or over) its `concurrency_limit` — see this module's doc comment for
    // why this happens here rather than inside the pure scheduler. Fleet
    // snapshots are fetched once per distinct fleet id referenced, not once
    // per request.
    let mut fleet_status: BTreeMap<String, Option<FleetConcurrencySnapshot>> = BTreeMap::new();
    for row in &queued {
        if row.selector_kind == "fleet" && !fleet_status.contains_key(&row.selector_id) {
            let snapshot = repo.fetch_fleet_concurrency(&row.selector_id).await?;
            fleet_status.insert(row.selector_id.clone(), snapshot);
        }
    }

    let requests: Vec<SchedulingRequest> = queued
        .iter()
        .filter(|row| {
            row.selector_kind != "fleet"
                || !fleet_is_saturated(fleet_status.get(&row.selector_id).and_then(Option::as_ref))
        })
        .filter_map(|row| build_scheduling_request(row, now))
        .collect();

    if requests.len() == 1 {
        // The common case — avoid `batch::schedule`'s capacity-ledger
        // machinery (built for many requests sharing many runners) for the
        // overwhelmingly common single-candidate poll.
        return Ok(
            match select_runner(&requests[0], &[candidate], now, policy) {
                SelectionOutcome::Selected(_) => Some(requests[0].request_id.as_str().to_string()),
                SelectionOutcome::NoEligibleRunner { .. }
                | SelectionOutcome::UnknownRunner { .. } => None,
            },
        );
    }

    let outcomes = super::batch::schedule(&requests, &[candidate], now, policy);
    Ok(outcomes.into_iter().find_map(|(request_id, outcome)| {
        matches!(outcome, SelectionOutcome::Selected(_)).then(|| request_id.into_inner())
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_from_metadata_reads_the_documented_convention() {
        assert_eq!(
            priority_from_metadata(r#"{"priority":"high"}"#),
            Priority::High
        );
        assert_eq!(
            priority_from_metadata(r#"{"priority":"LOW"}"#),
            Priority::Low
        );
        assert_eq!(
            priority_from_metadata(r#"{"priority":"normal"}"#),
            Priority::Normal
        );
        assert_eq!(priority_from_metadata(r#"{}"#), Priority::Normal);
        assert_eq!(
            priority_from_metadata(r#"{"priority":"urgent"}"#),
            Priority::Normal
        );
        assert_eq!(priority_from_metadata("not json"), Priority::Normal);
        assert_eq!(
            priority_from_metadata(r#"{"priority":5}"#),
            Priority::Normal
        );
    }

    #[test]
    fn fleet_saturation_reads_the_snapshot_honestly() {
        assert!(fleet_is_saturated(None), "no snapshot proves no room");
        assert!(!fleet_is_saturated(Some(&FleetConcurrencySnapshot {
            concurrency_limit: None,
            in_use: 999,
        })));
        assert!(!fleet_is_saturated(Some(&FleetConcurrencySnapshot {
            concurrency_limit: Some(5),
            in_use: 4,
        })));
        assert!(fleet_is_saturated(Some(&FleetConcurrencySnapshot {
            concurrency_limit: Some(5),
            in_use: 5,
        })));
        assert!(fleet_is_saturated(Some(&FleetConcurrencySnapshot {
            concurrency_limit: Some(5),
            in_use: 6,
        })));
    }

    #[test]
    fn unknown_runner_state_maps_to_the_conservative_revoked_reading() {
        assert_eq!(runner_state_from_str("active"), RunnerState::Active);
        assert_eq!(
            runner_state_from_str("pending_enrollment"),
            RunnerState::PendingEnrollment
        );
        assert_eq!(runner_state_from_str("revoked"), RunnerState::Revoked);
        assert_eq!(runner_state_from_str("something_new"), RunnerState::Revoked);
    }

    #[test]
    fn build_scheduling_request_skips_a_partial_model_selector_row_instead_of_panicking() {
        let row = QueuedRequestForScheduling {
            id: "req-1".into(),
            selector_kind: "exact_runner".into(),
            selector_id: "runner-1".into(),
            requested_harness_kind: Some("codex".into()),
            requested_model_provider: Some("openai".into()),
            requested_model_id: None,
            created_at: "2026-08-10T00:00:00Z".into(),
            metadata: "{}".into(),
        };
        assert!(build_scheduling_request(&row, Utc::now()).is_none());
    }

    #[test]
    fn build_scheduling_request_skips_a_row_with_no_harness_kind() {
        let row = QueuedRequestForScheduling {
            id: "req-1".into(),
            selector_kind: "exact_runner".into(),
            selector_id: "runner-1".into(),
            requested_harness_kind: None,
            requested_model_provider: None,
            requested_model_id: None,
            created_at: "2026-08-10T00:00:00Z".into(),
            metadata: "{}".into(),
        };
        assert!(build_scheduling_request(&row, Utc::now()).is_none());
    }
}
