//! The pure single-request selection algorithm.
//!
//! [`select_runner`] performs no I/O, reads no clock (`now` is a
//! caller-supplied parameter, so tests inject fake time instead of blocking
//! on a real sleep), and never grants a lease. It is deterministic: the same
//! `(request, candidates, now, policy)` tuple always produces the same
//! [`SelectionOutcome`], and the outcome does not depend on `candidates`'
//! slice order (see the order-independence tests in
//! `crates/tack-orch/tests/scheduling/scheduler.rs`) — only on its *content*.

use chrono::{DateTime, Duration, Utc};

use super::types::{
    IneligibleReason, ModelSelector, RunnerCandidate, RunnerState, SchedulingRequest, Selection,
    SelectionOutcome,
};
use crate::execution::{CapabilitySupport, HarnessKind, RunnerId, RunnerSelector};

/// A request-level defect that disqualifies every candidate identically, so
/// it is reported once rather than as N copies of the same
/// [`IneligibleReason`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SchedulingError {
    /// `ModelSelector::from_parts` was given exactly one of
    /// provider/model_id. See that function's doc comment.
    #[error("requested_model_provider and requested_model_id must both be set or both be absent")]
    PartialModelSelector,
}

/// Configurable thresholds the pure selection functions read but never
/// invent internally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulingPolicy {
    /// A candidate whose `last_heartbeat_at` is older than this (relative to
    /// the caller's `now`), or absent, is [`IneligibleReason::HeartbeatStale`].
    pub max_heartbeat_age: Duration,
}

impl Default for SchedulingPolicy {
    /// 60 seconds — `docs/contracts/runner-v1/limits.json`'s
    /// `heartbeat_interval_seconds` (15) + `heartbeat_grace_seconds` (45),
    /// the frozen contract's own definition of "a runner that has stopped
    /// heartbeating." This also matches `lease_duration_seconds` (60) in the
    /// same fixture, so a runner judged stale here is a runner whose lease
    /// the API would independently be entitled to treat as expired — not an
    /// independently invented number.
    fn default() -> Self {
        Self {
            max_heartbeat_age: Duration::seconds(60),
        }
    }
}

/// Evaluates one candidate against one request. `Ok` carries the harness
/// this candidate would actually run the request under (always
/// `request.requested_harness_kind` today — the return type exists so a
/// future "supported alias" harness match, if ever added, has somewhere to
/// report the resolved kind without changing this function's signature).
fn evaluate_candidate(
    request: &SchedulingRequest,
    candidate: &RunnerCandidate,
    now: DateTime<Utc>,
    policy: &SchedulingPolicy,
) -> Result<HarnessKind, IneligibleReason> {
    match &request.selector {
        RunnerSelector::ExactRunner { runner_id } => {
            if &candidate.runner_id != runner_id {
                return Err(IneligibleReason::NotRequestedRunner);
            }
        }
        RunnerSelector::Fleet { fleet_id } => {
            if !candidate.fleet_memberships.contains(fleet_id) {
                return Err(IneligibleReason::NotFleetMember {
                    fleet_id: fleet_id.clone(),
                });
            }
        }
        RunnerSelector::Any => {}
    }

    if candidate.state != RunnerState::Active {
        return Err(IneligibleReason::RunnerNotActive {
            state: candidate.state,
        });
    }

    let stale = match candidate.last_heartbeat_at {
        Some(last) => now.signed_duration_since(last) > policy.max_heartbeat_age,
        None => true,
    };
    if stale {
        return Err(IneligibleReason::HeartbeatStale {
            last_heartbeat_at: candidate.last_heartbeat_at,
            max_age: policy.max_heartbeat_age,
        });
    }

    if candidate.available_capacity == 0 {
        return Err(IneligibleReason::NoAvailableCapacity {
            total: candidate.total_capacity,
        });
    }

    for (key, expected) in &request.required_labels {
        match candidate.labels.get(key) {
            Some(actual) if actual == expected => {}
            Some(actual) => {
                return Err(IneligibleReason::MissingLabel {
                    key: key.clone(),
                    expected: expected.clone(),
                    actual: Some(actual.clone()),
                });
            }
            None => {
                return Err(IneligibleReason::MissingLabel {
                    key: key.clone(),
                    expected: expected.clone(),
                    actual: None,
                });
            }
        }
    }

    let harness = candidate
        .harnesses
        .iter()
        .find(|h| h.harness_kind == request.requested_harness_kind)
        .ok_or_else(|| IneligibleReason::HarnessNotDeclared {
            harness: request.requested_harness_kind.clone(),
        })?;

    if let Some(error) = &harness.probe_error {
        return Err(IneligibleReason::HarnessProbeError {
            harness: request.requested_harness_kind.clone(),
            error: error.clone(),
        });
    }

    match &request.requested_model {
        // No runner-v1 v1 capability field attests that a harness accepts
        // an unspecified model — see IneligibleReason::AutoSelectNotVerified's
        // doc comment. Every candidate is rejected identically rather than
        // the scheduler guessing which harness is safe.
        ModelSelector::AutoSelect => {
            return Err(IneligibleReason::AutoSelectNotVerified {
                harness: request.requested_harness_kind.clone(),
            });
        }
        ModelSelector::Explicit { provider, model_id } => {
            let declared = harness.model_combinations.iter().any(|combo| {
                combo.model_provider.as_str() == provider.as_str()
                    && combo
                        .model_ids
                        .iter()
                        .any(|declared_id| declared_id.as_str() == model_id.as_str())
            });
            // An undeclared pairing is still eligible when the harness
            // attests `model_passthrough: supported` — the adapter forwards
            // the operator's model verbatim and the harness itself
            // validates it at run time. Only `Supported` schedules;
            // `Advisory` is an unverified claim and capability claims are
            // load-bearing, so it is rejected exactly like `Unsupported`
            // and like an absent attestation.
            let passthrough = harness
                .model_passthrough
                .as_ref()
                .is_some_and(|cap| cap.support == CapabilitySupport::Supported);
            if !declared && !passthrough {
                return Err(IneligibleReason::ModelCombinationNotDeclared {
                    harness: request.requested_harness_kind.clone(),
                    provider: provider.as_str().to_string(),
                    model_id: model_id.as_str().to_string(),
                });
            }
        }
    }

    Ok(request.requested_harness_kind.clone())
}

/// Selects the best eligible runner for `request` out of `candidates`, or
/// reports why none qualify. Never mutates its inputs, never performs I/O,
/// and never grants a lease — see this module's and [`super::types`]'s doc
/// comments.
///
/// Tie-break among eligible candidates (the "fairness" half of
/// priority/fairness selection, applied here per single request;
/// batch-level request ordering is [`super::batch::schedule`]'s job): the
/// candidate with the most `available_capacity` wins, spreading load rather
/// than always picking a fixed favorite; a true tie is broken by ascending
/// `runner_id`, purely for full, order-independent determinism — identical
/// input selects identically.
pub fn select_runner(
    request: &SchedulingRequest,
    candidates: &[RunnerCandidate],
    now: DateTime<Utc>,
    policy: &SchedulingPolicy,
) -> SelectionOutcome {
    if let RunnerSelector::ExactRunner { runner_id } = &request.selector
        && !candidates.iter().any(|c| &c.runner_id == runner_id)
    {
        return SelectionOutcome::UnknownRunner {
            runner_id: runner_id.clone(),
        };
    }

    let mut reasons: Vec<(RunnerId, IneligibleReason)> = Vec::new();
    let mut eligible: Vec<(&RunnerCandidate, HarnessKind)> = Vec::new();

    for candidate in candidates {
        match evaluate_candidate(request, candidate, now, policy) {
            Ok(harness) => eligible.push((candidate, harness)),
            Err(reason) => reasons.push((candidate.runner_id.clone(), reason)),
        }
    }

    eligible.sort_by(|(a, _), (b, _)| {
        b.available_capacity
            .cmp(&a.available_capacity)
            .then_with(|| a.runner_id.as_str().cmp(b.runner_id.as_str()))
    });

    if let Some((candidate, harness)) = eligible.into_iter().next() {
        return SelectionOutcome::Selected(Selection {
            runner_id: candidate.runner_id.clone(),
            matched_harness: harness,
        });
    }

    // Sorted by runner_id so the whole outcome — not just a successful
    // Selected pick — is independent of the order `candidates` arrived in.
    reasons.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    SelectionOutcome::NoEligibleRunner { reasons }
}

#[cfg(test)]
mod tests {
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
}
