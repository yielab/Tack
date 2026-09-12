use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct VersionedFixture {
    protocol_version: ProtocolVersion,
    #[serde(flatten)]
    fields: BTreeMap<String, serde_json::Value>,
}

fn assert_versioned_fixture_round_trip(raw: &str) {
    let original: serde_json::Value = serde_json::from_str(raw).expect("fixture JSON");
    let typed: VersionedFixture = serde_json::from_str(raw).expect("protocol v1 fixture");
    assert_eq!(
        serde_json::to_value(typed).expect("serialize fixture"),
        original,
        "fixture must round-trip without dropping additive fields"
    );
}

#[test]
fn requested_and_actual_model_values_have_distinct_types() {
    fn takes_requested(_model: Option<RequestedModelId>) {}
    fn takes_actual(_model: ActualModelId) {}

    takes_requested(Some(RequestedModelId::new("opaque/model-alpha")));
    takes_actual(ActualModelId::new("opaque/model-alpha"));
}

#[test]
fn protocol_rejects_non_v1() {
    assert!(serde_json::from_str::<ProtocolVersion>("2").is_err());
}

#[test]
fn stale_lease_code_is_stable() {
    let error = ExecutionError::StaleLease {
        attempt_id: AttemptId::new("att_future"),
        current_fencing_token: FencingToken(8),
    };
    assert_eq!(error.code(), "stale_lease");
}

#[test]
fn every_frozen_fixture_round_trips_and_every_error_code_is_typed() {
    for fixture in [
        include_str!("../../../../../docs/contracts/runner-v1/artifact.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/artifact.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/cancellation.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/cancellation.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/capabilities.json"),
        include_str!("../../../../../docs/contracts/runner-v1/claim.no-work.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/claim.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/claim.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/completion.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/completion.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/decision.create.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/decision.create.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/decision.poll.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/decision.poll.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/enrollment.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/enrollment.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/event-batch.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/event-batch.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/heartbeat.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/heartbeat.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/limits.json"),
        include_str!("../../../../../docs/contracts/runner-v1/protocol.json"),
        include_str!("../../../../../docs/contracts/runner-v1/recovery-observation.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/recovery-observation.response.json"),
        include_str!("../../../../../docs/contracts/runner-v1/refresh.request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/refresh.response.json"),
    ] {
        assert_versioned_fixture_round_trip(fixture);
    }

    for fixture in [
        include_str!(
            "../../../../../docs/contracts/runner-v1/errors/artifact-checksum-mismatch.json"
        ),
        include_str!("../../../../../docs/contracts/runner-v1/errors/conflict.json"),
        include_str!("../../../../../docs/contracts/runner-v1/errors/decision-expired.json"),
        include_str!("../../../../../docs/contracts/runner-v1/errors/forbidden.json"),
        include_str!("../../../../../docs/contracts/runner-v1/errors/idempotency-conflict.json"),
        include_str!("../../../../../docs/contracts/runner-v1/errors/internal-error.json"),
        include_str!("../../../../../docs/contracts/runner-v1/errors/invalid-request.json"),
        include_str!("../../../../../docs/contracts/runner-v1/errors/invalid-transition.json"),
        include_str!("../../../../../docs/contracts/runner-v1/errors/not-found.json"),
        include_str!("../../../../../docs/contracts/runner-v1/errors/payload-too-large.json"),
        include_str!("../../../../../docs/contracts/runner-v1/errors/rate-limited.json"),
        include_str!("../../../../../docs/contracts/runner-v1/errors/runner-revoked.json"),
        include_str!("../../../../../docs/contracts/runner-v1/errors/stale-lease.json"),
        include_str!("../../../../../docs/contracts/runner-v1/errors/unauthorized.json"),
        include_str!("../../../../../docs/contracts/runner-v1/errors/unsupported-protocol.json"),
    ] {
        let original: serde_json::Value = serde_json::from_str(fixture).expect("fixture JSON");
        let typed: ProtocolErrorEnvelope = serde_json::from_str(fixture).expect("typed error");
        assert_eq!(
            serde_json::to_value(typed).expect("serialize error"),
            original
        );
    }
}

/// Every file under `docs/contracts/runner-v1/errors/`, paired with its
/// bare filename. This is the single list the two tests below share: the
/// retryable/constructor conformance test walks the fixture bytes, and
/// the coverage test below checks this list against the real directory
/// so a fixture added on disk without a matching entry here is a build
/// failure, not a silent gap.
const ERROR_FIXTURES: &[(&str, &str)] = &[
    (
        "artifact-checksum-mismatch.json",
        include_str!(
            "../../../../../docs/contracts/runner-v1/errors/artifact-checksum-mismatch.json"
        ),
    ),
    (
        "conflict.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/conflict.json"),
    ),
    (
        "decision-expired.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/decision-expired.json"),
    ),
    (
        "forbidden.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/forbidden.json"),
    ),
    (
        "idempotency-conflict.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/idempotency-conflict.json"),
    ),
    (
        "internal-error.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/internal-error.json"),
    ),
    (
        "invalid-request.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/invalid-request.json"),
    ),
    (
        "invalid-transition.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/invalid-transition.json"),
    ),
    (
        "not-found.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/not-found.json"),
    ),
    (
        "payload-too-large.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/payload-too-large.json"),
    ),
    (
        "rate-limited.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/rate-limited.json"),
    ),
    (
        "runner-revoked.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/runner-revoked.json"),
    ),
    (
        "stale-lease.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/stale-lease.json"),
    ),
    (
        "unauthorized.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/unauthorized.json"),
    ),
    (
        "unsupported-protocol.json",
        include_str!("../../../../../docs/contracts/runner-v1/errors/unsupported-protocol.json"),
    ),
];

#[test]
fn every_error_fixture_file_on_disk_is_in_the_conformance_list() {
    // Reads the real directory at test time (not compile time) so a
    // fixture file added to disk without a matching `ERROR_FIXTURES`
    // entry fails this test, rather than silently skipping the new code.
    let dir = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/contracts/runner-v1/errors"
    ));
    let mut on_disk: Vec<String> = std::fs::read_dir(dir)
        .expect("docs/contracts/runner-v1/errors must exist")
        .map(|entry| {
            entry
                .expect("readable directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".json"))
        .collect();
    on_disk.sort();

    let mut listed: Vec<&str> = ERROR_FIXTURES.iter().map(|(name, _)| *name).collect();
    listed.sort_unstable();

    assert_eq!(
        on_disk, listed,
        "docs/contracts/runner-v1/errors/ changed; add or remove the matching \
         entry in ERROR_FIXTURES (crates/tack-orch/src/execution/types.rs)"
    );
}

#[test]
fn stable_error_code_retryable_matches_every_fixture_and_constructor() {
    for (name, fixture) in ERROR_FIXTURES {
        let envelope: ProtocolErrorEnvelope =
            serde_json::from_str(fixture).unwrap_or_else(|e| panic!("{name}: {e}"));
        let parsed = envelope.error;

        assert_eq!(
            parsed.code.retryable(),
            parsed.retryable,
            "{name}: StableErrorCode::retryable() for {:?} must match the \
             fixture's `retryable` value",
            parsed.code
        );

        let constructed = ProtocolError::new(
            parsed.code,
            parsed.message.clone(),
            parsed.request_id.clone(),
            parsed.details.clone(),
        );
        assert_eq!(
            constructed, parsed,
            "{name}: ProtocolError::new must reproduce the fixture exactly, \
             including `retryable`"
        );
    }
}

#[test]
fn core_domain_snapshots_match_their_exact_fixture_shapes() {
    let capabilities: crate::execution::RunnerCapabilities = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/capabilities.json"
    ))
    .expect("capability fixture");
    assert_eq!(
        serde_json::to_value(capabilities).expect("serialize capabilities"),
        serde_json::from_str::<serde_json::Value>(include_str!(
            "../../../../../docs/contracts/runner-v1/capabilities.json"
        ))
        .expect("capability fixture JSON")
    );

    let claim: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture JSON");
    let request: ExecutionRequestSnapshot =
        serde_json::from_value(claim["request"].clone()).expect("request snapshot");
    let attempt: AttemptSnapshot =
        serde_json::from_value(claim["attempt"].clone()).expect("attempt snapshot");
    assert_eq!(
        serde_json::to_value(request).expect("serialize request"),
        claim["request"]
    );
    assert_eq!(
        serde_json::to_value(attempt).expect("serialize attempt"),
        claim["attempt"]
    );

    let completion: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/completion.request.json"
    ))
    .expect("completion fixture JSON");
    let actual: ActualExecution = serde_json::from_value(completion["actual_execution"].clone())
        .expect("actual execution snapshot");
    let usage: Usage = serde_json::from_value(completion["usage"].clone()).expect("usage");
    assert_eq!(
        serde_json::to_value(actual).expect("serialize actual execution"),
        completion["actual_execution"]
    );
    assert_eq!(
        serde_json::to_value(usage).expect("serialize usage"),
        completion["usage"]
    );
}

#[test]
fn recovery_observation_fixtures_round_trip_exactly_and_preserve_additions() {
    let request_json: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/recovery-observation.request.json"
    ))
    .expect("recovery request fixture JSON");
    let request: RecoveryObservationRequest =
        serde_json::from_value(request_json.clone()).expect("typed recovery request");
    assert_eq!(
        serde_json::to_value(&request).expect("serialize recovery request"),
        request_json
    );
    assert_eq!(request.observation, RecoveryObservation::ProcessStopped);
    assert_eq!(
        request.details.journal_state,
        RecoveryJournalState::Prepared
    );
    assert!(!request.details.process_observed);

    let response_json: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/recovery-observation.response.json"
    ))
    .expect("recovery response fixture JSON");
    let response: RecoveryObservationResponse =
        serde_json::from_value(response_json.clone()).expect("typed recovery response");
    assert_eq!(
        serde_json::to_value(&response).expect("serialize recovery response"),
        response_json
    );
    assert_eq!(
        response.disposition,
        RecoveryDisposition::SafePreSpawnRequeue
    );
    assert!(!response.replayed);

    let mut additive_request = serde_json::to_value(request).expect("serialize request");
    additive_request["future_request_field"] = serde_json::json!({"kept": true});
    additive_request["details"]["future_evidence"] = serde_json::json!("kept");
    let parsed: RecoveryObservationRequest =
        serde_json::from_value(additive_request.clone()).expect("parse additive request");
    assert_eq!(
        serde_json::to_value(parsed).expect("serialize additive request"),
        additive_request
    );

    let mut additive_response = serde_json::to_value(response).expect("serialize response");
    additive_response["future_response_field"] = serde_json::json!(42);
    let parsed: RecoveryObservationResponse =
        serde_json::from_value(additive_response.clone()).expect("parse additive response");
    assert_eq!(
        serde_json::to_value(parsed).expect("serialize additive response"),
        additive_response
    );
}

#[test]
fn recovery_dispositions_follow_lifecycle_and_observation_invariants() {
    use crate::execution::{TransitionActor, validate_transition};

    let active = [
        ExecutionState::Leased,
        ExecutionState::Preparing,
        ExecutionState::Running,
        ExecutionState::WaitingDecision,
    ];
    for state in active {
        assert!(
            RecoveryDisposition::SafePreSpawnRequeue
                .is_compatible_with(state, RecoveryObservation::ProcessStopped)
        );
        assert!(
            validate_transition(
                state,
                RecoveryDisposition::SafePreSpawnRequeue
                    .attempt_transition()
                    .expect("safe recovery transitions attempt"),
                TransitionActor::RecoveryService,
            )
            .is_ok()
        );
        assert!(
            RecoveryDisposition::NeedsOperator
                .is_compatible_with(state, RecoveryObservation::ProcessStopped)
        );
        assert!(
            RecoveryDisposition::NeedsOperator
                .is_compatible_with(state, RecoveryObservation::ProcessRunning)
        );
        assert!(
            RecoveryDisposition::NeedsOperator
                .is_compatible_with(state, RecoveryObservation::Ambiguous)
        );
        assert!(
            validate_transition(
                state,
                RecoveryDisposition::NeedsOperator
                    .attempt_transition()
                    .expect("needs-operator recovery transitions attempt"),
                TransitionActor::RecoveryService,
            )
            .is_ok()
        );
    }

    assert!(
        !RecoveryDisposition::SafePreSpawnRequeue
            .is_compatible_with(ExecutionState::Running, RecoveryObservation::ProcessRunning)
    );
    assert!(
        !RecoveryDisposition::SafePreSpawnRequeue.is_compatible_with(
            ExecutionState::Succeeded,
            RecoveryObservation::ProcessStopped
        )
    );
    for state in [
        ExecutionState::Succeeded,
        ExecutionState::Failed,
        ExecutionState::Cancelled,
    ] {
        assert!(
            RecoveryDisposition::AlreadyTerminal
                .is_compatible_with(state, RecoveryObservation::Ambiguous)
        );
    }
    assert!(
        !RecoveryDisposition::AlreadyTerminal
            .is_compatible_with(ExecutionState::Leased, RecoveryObservation::ProcessStopped)
    );
    assert_eq!(
        RecoveryDisposition::SafePreSpawnRequeue.request_transition(),
        Some(ExecutionState::Queued)
    );
    assert_eq!(
        RecoveryDisposition::NeedsOperator.request_transition(),
        Some(ExecutionState::NeedsOperator)
    );
    assert_eq!(
        RecoveryDisposition::AlreadyTerminal.attempt_transition(),
        None
    );
    assert_eq!(
        RecoveryDisposition::AlreadyTerminal.request_transition(),
        None
    );
}
