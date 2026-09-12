//! Priority/fairness scheduling across several requests sharing one
//! candidate pool.
//!
//! [`select_runner`](super::select::select_runner) answers "which runner for
//! *this* request" in isolation. [`schedule`] answers the question a real
//! dispatch pass actually has: several queued requests, a shared, finite
//! pool of runners, deciding who goes first. It is still pure — no I/O, no
//! lease granted, `now` supplied by the caller — and still deterministic:
//! ordering never depends on the input slices' arrival order, only their
//! content.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::select::{SchedulingPolicy, select_runner};
use super::types::{RunnerCandidate, SchedulingRequest, SelectionOutcome};
use crate::execution::{ExecutionRequestId, RunnerId};

/// Schedules every request in `requests` against the shared `candidates`
/// pool, honoring [`super::types::Priority`] and FIFO fairness within a
/// priority tier.
///
/// # Ordering
///
/// Requests are considered highest [`super::types::Priority`] first; within
/// the same priority, oldest `created_at` first (FIFO — a request does not
/// get pushed back behind a same-priority request that arrived later); any
/// remaining tie (identical priority and `created_at`) is broken by
/// `request_id`, purely so the processing order — and therefore which
/// request wins a contested runner — never depends on `requests`' input
/// slice order.
///
/// # Capacity is consumed within the batch
///
/// A runner [`select_runner`] selects for an earlier request in this pass
/// has its `available_capacity` reduced by one for every later request in
/// the *same* call — this is what makes "fairness" mean something beyond a
/// single request: two same-priority requests that both fit a single-slot
/// runner do not both get told "yes, this runner." This adjustment is
/// entirely local to one `schedule` call; the real, authoritative capacity
/// ledger is `agent_runners.available_capacity`, decremented only by the
/// repository's fenced claim — the caller must re-derive
/// `candidates` from fresh state before a later `schedule` call, not reuse
/// this function's internal bookkeeping.
pub fn schedule(
    requests: &[SchedulingRequest],
    candidates: &[RunnerCandidate],
    now: DateTime<Utc>,
    policy: &SchedulingPolicy,
) -> Vec<(ExecutionRequestId, SelectionOutcome)> {
    let mut ordered: Vec<&SchedulingRequest> = requests.iter().collect();
    ordered.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.request_id.as_str().cmp(b.request_id.as_str()))
    });

    let mut remaining_capacity: BTreeMap<RunnerId, u32> = candidates
        .iter()
        .map(|c| (c.runner_id.clone(), c.available_capacity))
        .collect();

    let mut results = Vec::with_capacity(ordered.len());
    for request in ordered {
        let adjusted: Vec<RunnerCandidate> = candidates
            .iter()
            .map(|c| {
                let mut c = c.clone();
                c.available_capacity = remaining_capacity.get(&c.runner_id).copied().unwrap_or(0);
                c
            })
            .collect();

        let outcome = select_runner(request, &adjusted, now, policy);
        if let SelectionOutcome::Selected(selection) = &outcome
            && let Some(capacity) = remaining_capacity.get_mut(&selection.runner_id)
        {
            *capacity = capacity.saturating_sub(1);
        }
        results.push((request.request_id.clone(), outcome));
    }
    results
}

#[cfg(test)]
#[path = "batch/tests.rs"]
mod tests;
