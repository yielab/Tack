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
#[path = "select/tests.rs"]
mod tests;
