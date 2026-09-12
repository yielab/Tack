use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, TimeZone};

use super::*;
use crate::execution::{HarnessCapability, HarnessKind, ModelCombination, ModelId, ModelProvider};
use crate::scheduler::types::{ModelSelector, Priority, RunnerState};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 10, 12, 0, 0).unwrap()
}

fn candidate(id: &str, capacity: u32) -> RunnerCandidate {
    RunnerCandidate {
        runner_id: RunnerId::new(id),
        state: RunnerState::Active,
        fleet_memberships: BTreeSet::new(),
        labels: BTreeMap::new(),
        total_capacity: capacity,
        available_capacity: capacity,
        last_heartbeat_at: Some(now()),
        harnesses: vec![HarnessCapability {
            harness_kind: HarnessKind::new("claude_code"),
            installed_version: "1.0.0".to_string(),
            probe_error: None,
            probed_at: now(),
            model_combinations: vec![ModelCombination {
                model_provider: ModelProvider::new("anthropic"),
                model_ids: vec![ModelId::new("opaque/sonnet")],
                discovery: "reported".to_string(),
                model_metadata: BTreeMap::new(),
                additional: BTreeMap::new(),
            }],
            model_passthrough: None,
            additional: BTreeMap::new(),
        }],
    }
}

fn request(id: &str, priority: Priority, created_at: DateTime<Utc>) -> SchedulingRequest {
    use crate::execution::{RequestedModelId, RequestedModelProvider, RunnerSelector};
    SchedulingRequest {
        request_id: ExecutionRequestId::new(id),
        selector: RunnerSelector::Any,
        priority,
        requested_harness_kind: HarnessKind::new("claude_code"),
        requested_model: ModelSelector::Explicit {
            provider: RequestedModelProvider::new("anthropic"),
            model_id: RequestedModelId::new("opaque/sonnet"),
        },
        required_labels: BTreeMap::new(),
        created_at,
    }
}

#[test]
fn higher_priority_request_claims_the_single_slot_runner_first() {
    let candidates = [candidate("runner-a", 1)];
    let requests = [
        request("req-low", Priority::Low, now()),
        request("req-high", Priority::High, now() + Duration::seconds(5)),
    ];
    let results = schedule(&requests, &candidates, now(), &SchedulingPolicy::default());

    let high = results
        .iter()
        .find(|(id, _)| id.as_str() == "req-high")
        .unwrap();
    let low = results
        .iter()
        .find(|(id, _)| id.as_str() == "req-low")
        .unwrap();

    assert!(matches!(high.1, SelectionOutcome::Selected(_)));
    assert!(matches!(low.1, SelectionOutcome::NoEligibleRunner { .. }));
}

#[test]
fn same_priority_requests_are_served_fifo_by_created_at() {
    let candidates = [candidate("runner-a", 1)];
    let requests = [
        request(
            "req-second",
            Priority::Normal,
            now() + Duration::seconds(10),
        ),
        request("req-first", Priority::Normal, now()),
    ];
    let results = schedule(&requests, &candidates, now(), &SchedulingPolicy::default());

    let first = results
        .iter()
        .find(|(id, _)| id.as_str() == "req-first")
        .unwrap();
    let second = results
        .iter()
        .find(|(id, _)| id.as_str() == "req-second")
        .unwrap();

    assert!(matches!(first.1, SelectionOutcome::Selected(_)));
    assert!(matches!(
        second.1,
        SelectionOutcome::NoEligibleRunner { .. }
    ));
}

#[test]
fn capacity_is_consumed_across_the_batch_not_reevaluated_per_request() {
    let candidates = [candidate("runner-a", 2)];
    let requests = [
        request("req-1", Priority::Normal, now()),
        request("req-2", Priority::Normal, now() + Duration::seconds(1)),
        request("req-3", Priority::Normal, now() + Duration::seconds(2)),
    ];
    let results = schedule(&requests, &candidates, now(), &SchedulingPolicy::default());

    let selected_count = results
        .iter()
        .filter(|(_, outcome)| matches!(outcome, SelectionOutcome::Selected(_)))
        .count();
    // Only 2 units of capacity exist; exactly 2 of the 3 requests land.
    assert_eq!(selected_count, 2);
}

#[test]
fn schedule_output_does_not_depend_on_request_input_order() {
    let candidates = [candidate("runner-a", 1)];
    let requests_a = [
        request("req-low", Priority::Low, now()),
        request("req-high", Priority::High, now()),
    ];
    let requests_b = [
        request("req-high", Priority::High, now()),
        request("req-low", Priority::Low, now()),
    ];

    let mut results_a = schedule(
        &requests_a,
        &candidates,
        now(),
        &SchedulingPolicy::default(),
    );
    let mut results_b = schedule(
        &requests_b,
        &candidates,
        now(),
        &SchedulingPolicy::default(),
    );
    results_a.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    results_b.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    assert_eq!(results_a, results_b);
}

#[test]
fn empty_request_batch_returns_empty_results() {
    let candidates = [candidate("runner-a", 1)];
    let results = schedule(&[], &candidates, now(), &SchedulingPolicy::default());
    assert!(results.is_empty());
}
