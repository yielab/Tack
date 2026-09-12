use std::collections::{BTreeMap, BTreeSet};

use chrono::TimeZone;

use super::*;
use crate::execution::{
    ExecutionRequestId, HarnessCapability, ModelCombination, ModelId, ModelProvider,
    RequestedModelId, RequestedModelProvider,
};
use crate::scheduler::types::Priority;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 10, 12, 0, 0).unwrap()
}

fn harness(
    kind: &str,
    probe_error: Option<&str>,
    combos: Vec<(&str, &[&str])>,
) -> HarnessCapability {
    HarnessCapability {
        harness_kind: HarnessKind::new(kind),
        installed_version: "1.0.0".to_string(),
        probe_error: probe_error.map(str::to_string),
        probed_at: now(),
        model_combinations: combos
            .into_iter()
            .map(|(provider, models)| ModelCombination {
                model_provider: ModelProvider::new(provider),
                model_ids: models.iter().map(|m| ModelId::new(*m)).collect(),
                discovery: "reported".to_string(),
                model_metadata: BTreeMap::new(),
                additional: BTreeMap::new(),
            })
            .collect(),
        model_passthrough: None,
        additional: BTreeMap::new(),
    }
}

fn candidate(id: &str) -> RunnerCandidate {
    RunnerCandidate {
        runner_id: RunnerId::new(id),
        state: RunnerState::Active,
        fleet_memberships: BTreeSet::new(),
        labels: BTreeMap::new(),
        total_capacity: 2,
        available_capacity: 2,
        last_heartbeat_at: Some(now()),
        harnesses: vec![harness(
            "claude_code",
            None,
            vec![("anthropic", &["opaque/sonnet"])],
        )],
    }
}

fn request() -> SchedulingRequest {
    SchedulingRequest {
        request_id: ExecutionRequestId::new("req-1"),
        selector: RunnerSelector::Any,
        priority: Priority::Normal,
        requested_harness_kind: HarnessKind::new("claude_code"),
        requested_model: ModelSelector::Explicit {
            provider: RequestedModelProvider::new("anthropic"),
            model_id: RequestedModelId::new("opaque/sonnet"),
        },
        required_labels: BTreeMap::new(),
        created_at: now(),
    }
}

#[test]
fn empty_candidate_list_yields_no_eligible_runner_with_no_reasons() {
    let outcome = select_runner(&request(), &[], now(), &SchedulingPolicy::default());
    assert_eq!(
        outcome,
        SelectionOutcome::NoEligibleRunner { reasons: vec![] }
    );
}

#[test]
fn single_healthy_candidate_is_selected() {
    let candidates = [candidate("runner-a")];
    let outcome = select_runner(&request(), &candidates, now(), &SchedulingPolicy::default());
    assert_eq!(
        outcome,
        SelectionOutcome::Selected(Selection {
            runner_id: RunnerId::new("runner-a"),
            matched_harness: HarnessKind::new("claude_code"),
        })
    );
}

#[test]
fn stale_heartbeat_is_named_and_excludes_the_candidate() {
    let mut c = candidate("runner-a");
    c.last_heartbeat_at = Some(now() - Duration::seconds(61));
    let outcome = select_runner(&request(), &[c], now(), &SchedulingPolicy::default());
    match outcome {
        SelectionOutcome::NoEligibleRunner { reasons } => {
            assert_eq!(reasons.len(), 1);
            assert!(matches!(
                reasons[0].1,
                IneligibleReason::HeartbeatStale { .. }
            ));
        }
        other => panic!("expected NoEligibleRunner, got {other:?}"),
    }
}

#[test]
fn missing_heartbeat_is_stale_not_a_panic() {
    let mut c = candidate("runner-a");
    c.last_heartbeat_at = None;
    let outcome = select_runner(&request(), &[c], now(), &SchedulingPolicy::default());
    match outcome {
        SelectionOutcome::NoEligibleRunner { reasons } => {
            assert!(matches!(
                reasons[0].1,
                IneligibleReason::HeartbeatStale {
                    last_heartbeat_at: None,
                    ..
                }
            ));
        }
        other => panic!("expected NoEligibleRunner, got {other:?}"),
    }
}

#[test]
fn saturated_fleet_names_no_available_capacity() {
    let mut c = candidate("runner-a");
    c.available_capacity = 0;
    let outcome = select_runner(&request(), &[c], now(), &SchedulingPolicy::default());
    match outcome {
        SelectionOutcome::NoEligibleRunner { reasons } => {
            assert_eq!(
                reasons[0].1,
                IneligibleReason::NoAvailableCapacity { total: 2 }
            );
        }
        other => panic!("expected NoEligibleRunner, got {other:?}"),
    }
}

#[test]
fn heterogeneous_fleet_selects_only_the_qualifying_member() {
    let mut wrong_harness = candidate("runner-codex");
    wrong_harness.harnesses = vec![harness("codex", None, vec![])];

    let mut saturated = candidate("runner-full");
    saturated.available_capacity = 0;

    let qualifying = candidate("runner-good");

    let candidates = [wrong_harness, saturated, qualifying];
    let outcome = select_runner(&request(), &candidates, now(), &SchedulingPolicy::default());
    assert_eq!(
        outcome,
        SelectionOutcome::Selected(Selection {
            runner_id: RunnerId::new("runner-good"),
            matched_harness: HarnessKind::new("claude_code"),
        })
    );
}

#[test]
fn tied_fleet_breaks_ties_by_runner_id_deterministically() {
    let a = candidate("runner-b");
    let b = candidate("runner-a");
    // Fully tied on available_capacity (both 2/2) — lexical id decides.
    let outcome_1 = select_runner(
        &request(),
        &[a.clone(), b.clone()],
        now(),
        &SchedulingPolicy::default(),
    );
    let outcome_2 = select_runner(&request(), &[b, a], now(), &SchedulingPolicy::default());
    assert_eq!(outcome_1, outcome_2);
    assert_eq!(
        outcome_1,
        SelectionOutcome::Selected(Selection {
            runner_id: RunnerId::new("runner-a"),
            matched_harness: HarnessKind::new("claude_code"),
        })
    );
}

#[test]
fn higher_available_capacity_wins_over_lexically_earlier_id() {
    let mut lower_capacity = candidate("runner-a");
    lower_capacity.available_capacity = 1;
    let higher_capacity = candidate("runner-z");
    let outcome = select_runner(
        &request(),
        &[lower_capacity, higher_capacity],
        now(),
        &SchedulingPolicy::default(),
    );
    assert_eq!(
        outcome,
        SelectionOutcome::Selected(Selection {
            runner_id: RunnerId::new("runner-z"),
            matched_harness: HarnessKind::new("claude_code"),
        })
    );
}

#[test]
fn exact_runner_selector_rejects_every_other_candidate_by_name() {
    let mut req = request();
    req.selector = RunnerSelector::ExactRunner {
        runner_id: RunnerId::new("runner-target"),
    };
    let target = candidate("runner-target");
    let other = candidate("runner-other");
    let outcome = select_runner(
        &req,
        &[other.clone(), target.clone()],
        now(),
        &SchedulingPolicy::default(),
    );
    assert_eq!(
        outcome,
        SelectionOutcome::Selected(Selection {
            runner_id: RunnerId::new("runner-target"),
            matched_harness: HarnessKind::new("claude_code"),
        })
    );
}

#[test]
fn exact_runner_selector_names_unknown_runner_distinctly() {
    let mut req = request();
    req.selector = RunnerSelector::ExactRunner {
        runner_id: RunnerId::new("runner-ghost"),
    };
    let candidates = [candidate("runner-real")];
    let outcome = select_runner(&req, &candidates, now(), &SchedulingPolicy::default());
    assert_eq!(
        outcome,
        SelectionOutcome::UnknownRunner {
            runner_id: RunnerId::new("runner-ghost")
        }
    );
}

#[test]
fn exact_runner_present_but_ineligible_is_no_eligible_runner_not_unknown() {
    let mut req = request();
    req.selector = RunnerSelector::ExactRunner {
        runner_id: RunnerId::new("runner-target"),
    };
    let mut target = candidate("runner-target");
    target.state = RunnerState::Revoked;
    let outcome = select_runner(&req, &[target], now(), &SchedulingPolicy::default());
    match outcome {
        SelectionOutcome::NoEligibleRunner { reasons } => {
            assert_eq!(
                reasons,
                vec![(
                    RunnerId::new("runner-target"),
                    IneligibleReason::RunnerNotActive {
                        state: RunnerState::Revoked
                    }
                )]
            );
        }
        other => panic!("expected NoEligibleRunner, got {other:?}"),
    }
}

#[test]
fn fleet_selector_excludes_non_members_by_name() {
    let mut req = request();
    req.selector = RunnerSelector::Fleet {
        fleet_id: "fleet-a".to_string(),
    };
    let mut member = candidate("runner-member");
    member.fleet_memberships.insert("fleet-a".to_string());
    let non_member = candidate("runner-outsider");

    let outcome = select_runner(
        &req,
        &[non_member.clone(), member.clone()],
        now(),
        &SchedulingPolicy::default(),
    );
    assert_eq!(
        outcome,
        SelectionOutcome::Selected(Selection {
            runner_id: RunnerId::new("runner-member"),
            matched_harness: HarnessKind::new("claude_code"),
        })
    );

    let only_outsider = select_runner(&req, &[non_member], now(), &SchedulingPolicy::default());
    match only_outsider {
        SelectionOutcome::NoEligibleRunner { reasons } => {
            assert_eq!(
                reasons,
                vec![(
                    RunnerId::new("runner-outsider"),
                    IneligibleReason::NotFleetMember {
                        fleet_id: "fleet-a".to_string()
                    }
                )]
            );
        }
        other => panic!("expected NoEligibleRunner, got {other:?}"),
    }
}

#[test]
fn missing_label_is_named_with_expected_and_actual() {
    let mut req = request();
    req.required_labels
        .insert("trust".to_string(), "local".to_string());
    let mut wrong_value = candidate("runner-a");
    wrong_value
        .labels
        .insert("trust".to_string(), "remote".to_string());
    let outcome = select_runner(&req, &[wrong_value], now(), &SchedulingPolicy::default());
    match outcome {
        SelectionOutcome::NoEligibleRunner { reasons } => {
            assert_eq!(
                reasons[0].1,
                IneligibleReason::MissingLabel {
                    key: "trust".to_string(),
                    expected: "local".to_string(),
                    actual: Some("remote".to_string()),
                }
            );
        }
        other => panic!("expected NoEligibleRunner, got {other:?}"),
    }
}

#[test]
fn harness_probe_error_excludes_a_declared_but_broken_harness() {
    let mut c = candidate("runner-a");
    c.harnesses = vec![harness(
        "claude_code",
        Some("binary not found on PATH"),
        vec![("anthropic", &["opaque/sonnet"])],
    )];
    let outcome = select_runner(&request(), &[c], now(), &SchedulingPolicy::default());
    match outcome {
        SelectionOutcome::NoEligibleRunner { reasons } => {
            assert_eq!(
                reasons[0].1,
                IneligibleReason::HarnessProbeError {
                    harness: HarnessKind::new("claude_code"),
                    error: "binary not found on PATH".to_string(),
                }
            );
        }
        other => panic!("expected NoEligibleRunner, got {other:?}"),
    }
}

#[test]
fn undeclared_model_combination_is_named_not_a_bare_bool() {
    let mut req = request();
    req.requested_model = ModelSelector::Explicit {
        provider: RequestedModelProvider::new("anthropic"),
        model_id: RequestedModelId::new("opaque/does-not-exist"),
    };
    let outcome = select_runner(
        &req,
        &[candidate("runner-a")],
        now(),
        &SchedulingPolicy::default(),
    );
    match outcome {
        SelectionOutcome::NoEligibleRunner { reasons } => {
            assert_eq!(
                reasons[0].1,
                IneligibleReason::ModelCombinationNotDeclared {
                    harness: HarnessKind::new("claude_code"),
                    provider: "anthropic".to_string(),
                    model_id: "opaque/does-not-exist".to_string(),
                }
            );
        }
        other => panic!("expected NoEligibleRunner, got {other:?}"),
    }
}

#[test]
fn auto_select_is_rejected_with_a_named_reason_not_an_empty_list() {
    let mut req = request();
    req.requested_model = ModelSelector::AutoSelect;
    let outcome = select_runner(
        &req,
        &[candidate("runner-a")],
        now(),
        &SchedulingPolicy::default(),
    );
    match outcome {
        SelectionOutcome::NoEligibleRunner { reasons } => {
            assert_eq!(reasons.len(), 1);
            assert_eq!(
                reasons[0].1,
                IneligibleReason::AutoSelectNotVerified {
                    harness: HarnessKind::new("claude_code"),
                }
            );
        }
        other => panic!("expected NoEligibleRunner, got {other:?}"),
    }
}

#[test]
fn undeclared_harness_is_named_distinctly_from_a_probe_error() {
    let mut c = candidate("runner-a");
    c.harnesses = vec![harness("codex", None, vec![])];
    let outcome = select_runner(&request(), &[c], now(), &SchedulingPolicy::default());
    match outcome {
        SelectionOutcome::NoEligibleRunner { reasons } => {
            assert_eq!(
                reasons[0].1,
                IneligibleReason::HarnessNotDeclared {
                    harness: HarnessKind::new("claude_code"),
                }
            );
        }
        other => panic!("expected NoEligibleRunner, got {other:?}"),
    }
}

#[test]
fn identical_input_selects_identically_across_repeated_calls() {
    let candidates = [candidate("runner-a"), candidate("runner-b")];
    let req = request();
    let policy = SchedulingPolicy::default();
    let first = select_runner(&req, &candidates, now(), &policy);
    for _ in 0..25 {
        assert_eq!(select_runner(&req, &candidates, now(), &policy), first);
    }
}

#[test]
fn model_selector_from_parts_rejects_partial_input() {
    assert_eq!(
        ModelSelector::from_parts(Some(RequestedModelProvider::new("anthropic")), None),
        Err(SchedulingError::PartialModelSelector)
    );
    assert_eq!(
        ModelSelector::from_parts(None, Some(RequestedModelId::new("opaque/sonnet"))),
        Err(SchedulingError::PartialModelSelector)
    );
    assert_eq!(
        ModelSelector::from_parts(None, None),
        Ok(ModelSelector::AutoSelect)
    );
    assert_eq!(
        ModelSelector::from_parts(
            Some(RequestedModelProvider::new("anthropic")),
            Some(RequestedModelId::new("opaque/sonnet"))
        ),
        Ok(ModelSelector::Explicit {
            provider: RequestedModelProvider::new("anthropic"),
            model_id: RequestedModelId::new("opaque/sonnet"),
        })
    );
}
