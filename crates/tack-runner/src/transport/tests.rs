use std::{collections::BTreeMap, sync::Mutex, time::UNIX_EPOCH};

use tack_orch::execution::{CapabilityLimits, CapabilityValue, Concurrency, FeatureCapabilities};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

use super::*;

// -----------------------------------------------------------------
// A local mock HTTP/1.1 server. Rule 8 allows local mock HTTP only —
// no live server and no secrets in CI. It records every request so a
// test can assert what actually went onto the wire, which is the only
// way to prove a credential was *not* placed somewhere.
// -----------------------------------------------------------------

#[derive(Debug, Clone)]
struct RecordedRequest {
    method: String,
    path: String,
    authorization: Option<String>,
    headers: BTreeMap<String, String>,
    body: Value,
    raw_body: String,
}

struct MockServer {
    base_url: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
}

/// Canned replies, consumed in order; the last one repeats.
fn spawn_mock(replies: Vec<(u16, String)>) -> MockServer {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&requests);
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("mock runtime");
        runtime.block_on(async move {
            let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
            let port = listener.local_addr().expect("addr").port();
            ready_tx.send(port).expect("ready");
            let mut served = 0_usize;
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 4096];
                let (head_end, content_length) = loop {
                    let read = stream.read(&mut chunk).await.unwrap_or(0);
                    if read == 0 {
                        break (None, 0);
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                    if let Some(position) = find_subslice(&buffer, b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buffer[..position]).to_string();
                        let length = head
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        break (Some(position + 4), length);
                    }
                };
                let Some(head_end) = head_end else { continue };
                while buffer.len() < head_end + content_length {
                    let read = stream.read(&mut chunk).await.unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                }
                let head = String::from_utf8_lossy(&buffer[..head_end - 4]).to_string();
                let raw_body =
                    String::from_utf8_lossy(&buffer[head_end..buffer.len()]).to_string();
                let mut lines = head.lines();
                let request_line = lines.next().unwrap_or_default().to_owned();
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or_default().to_owned();
                let path = parts.next().unwrap_or_default().to_owned();
                let mut headers = BTreeMap::new();
                for line in lines {
                    if let Some((name, value)) = line.split_once(':') {
                        headers.insert(
                            name.trim().to_ascii_lowercase(),
                            value.trim().to_owned(),
                        );
                    }
                }
                recorded.lock().expect("record").push(RecordedRequest {
                    method,
                    path,
                    authorization: headers.get("authorization").cloned(),
                    headers,
                    body: serde_json::from_str(&raw_body).unwrap_or(Value::Null),
                    raw_body,
                });
                let index = served.min(replies.len().saturating_sub(1));
                served += 1;
                let (status, payload) = replies
                    .get(index)
                    .cloned()
                    .unwrap_or((500, "{}".to_owned()));
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });
    });
    let port = ready_rx.recv().expect("mock port");
    MockServer {
        base_url: format!("http://127.0.0.1:{port}/api/runner/v1"),
        requests,
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/contracts/runner-v1")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|_| panic!("fixture {name} is readable"))
}

fn client(base_url: &str) -> HttpPullProtocol {
    HttpPullProtocol::new(
        base_url,
        Duration::from_secs(5),
        RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(2),
        },
    )
    .expect("client builds")
}

fn session() -> RunnerSession {
    RunnerSession::new(
        RunnerId::new("runr_01J00000000000000000000001"),
        RunnerCredential::new(SECRET_CREDENTIAL),
        Timestamp::new("2026-11-04T12:00:00Z"),
    )
}

const SECRET_CREDENTIAL: &str = "runner-credential-must-never-be-logged";
const SECRET_ENROLLMENT: &str = "enrollment-token-must-never-be-logged";

fn capability(support: &str) -> CapabilityValue {
    CapabilityValue {
        support: serde_json::from_value(json!(support)).expect("support"),
        reason: None,
        additional: BTreeMap::new(),
    }
}

fn capabilities() -> RunnerCapabilities {
    RunnerCapabilities {
        protocol_version: Some(ProtocolVersion::v1()),
        runner_version: "0.1.0".to_owned(),
        reported_at: chrono::DateTime::<chrono::Utc>::from(UNIX_EPOCH),
        labels: BTreeMap::new(),
        concurrency: Concurrency {
            total: 1,
            available: 1,
            additional: BTreeMap::new(),
        },
        harnesses: Vec::new(),
        features: FeatureCapabilities {
            cancel: capability("advisory"),
            resume: capability("unsupported"),
            decisions: capability("supported"),
            artifacts: capability("supported"),
            usage: capability("advisory"),
            additional: BTreeMap::new(),
        },
        limits: CapabilityLimits {
            event_payload_bytes_max: 65_536,
            artifact_content_bytes_max: 52_428_800,
            additional: BTreeMap::new(),
        },
        additional: BTreeMap::new(),
    }
}

// -----------------------------------------------------------------
// Every operation is asserted against the frozen fixture bytes, never
// hand-written JSON.
// -----------------------------------------------------------------

#[tokio::test]
async fn enrollment_parses_the_frozen_response_and_carries_the_token_only_in_the_body() {
    let server = spawn_mock(vec![(200, fixture("enrollment.response.json"))]);
    let protocol = client(&server.base_url);
    let response = protocol
        .enroll(
            &EnrollmentCredential::new(SECRET_ENROLLMENT),
            EnrollmentRequest {
                runner_name: "dev-runner-01".into(),
                runner_version: "0.1.0".into(),
                capabilities: capabilities(),
            },
        )
        .await
        .expect("enrollment succeeds");

    assert_eq!(
        response.session.runner_id.as_str(),
        "runr_01J00000000000000000000001"
    );
    assert_eq!(
        response.session.credential().expose(),
        "example_runner_credential_returned_once"
    );
    assert_eq!(response.heartbeat_interval, Duration::from_secs(15));
    assert_eq!(response.lease_duration, Duration::from_secs(60));

    let recorded = server.requests.lock().expect("requests").clone();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].path, "/api/runner/v1/enroll");
    assert_eq!(recorded[0].body["enrollment_token"], SECRET_ENROLLMENT);
    assert_eq!(recorded[0].body["protocol_version"], 1);
    // The enrollment token is body-only; `protocol.json` names the
    // enrollment authentication `single_use_enrollment_token_in_request_body`,
    // and a bearer header here would be a second, unspecified channel.
    assert!(recorded[0].authorization.is_none());
    // `runner_version` is a sibling of `capabilities`, never nested in it.
    assert!(
        recorded[0].body["capabilities"]
            .get("runner_version")
            .is_none()
    );
    assert_eq!(recorded[0].body["runner_version"], "0.1.0");
}

#[tokio::test]
async fn claim_builds_the_lease_from_both_halves_of_the_frozen_response() {
    let server = spawn_mock(vec![(200, fixture("claim.response.json"))]);
    let protocol = client(&server.base_url);
    let result = protocol
        .claim(
            &session(),
            ClaimRequest {
                claim_request_id: ClaimRequestId::new("claim_01J00000000000000000000001"),
                available_capacity: 1,
                wait: Duration::from_secs(15),
            },
        )
        .await
        .expect("claim succeeds");

    let ClaimResult::Work(work) = result else {
        panic!("the fixture carries a lease");
    };
    // `attempt_number` and `state` exist only on the attempt snapshot;
    // reading them off the lease object would have produced a default.
    assert_eq!(work.lease.attempt_number, 1);
    assert_eq!(work.lease.state, AttemptState::Leased);
    assert_eq!(work.lease.fencing_token, FencingToken(7));
    // The redundancy check the engine relies on must pass on real data.
    let repository = work
        .workspace_repository()
        .expect("claim envelope is self-consistent");
    assert_eq!(
        repository.base_revision,
        "0123456789abcdef0123456789abcdef01234567"
    );

    let recorded = server.requests.lock().expect("requests").clone();
    assert_eq!(recorded[0].path, "/api/runner/v1/claim");
    assert_eq!(
        recorded[0].authorization.as_deref(),
        Some(format!("Bearer {SECRET_CREDENTIAL}").as_str())
    );
    assert_eq!(recorded[0].body["wait_ms"], 15_000);
}

#[tokio::test]
async fn claim_no_work_is_not_an_error() {
    let server = spawn_mock(vec![(200, fixture("claim.no-work.response.json"))]);
    let result = client(&server.base_url)
        .claim(
            &session(),
            ClaimRequest {
                claim_request_id: ClaimRequestId::new("claim_01J00000000000000000000002"),
                available_capacity: 1,
                wait: Duration::from_secs(1),
            },
        )
        .await
        .expect("no-work is a successful exchange");
    assert!(matches!(
        result,
        ClaimResult::NoWork { retry_after, ref reason }
            if retry_after == Duration::from_millis(5_000) && reason == "no_eligible_work"
    ));
}

#[tokio::test]
async fn claim_wait_is_clamped_to_the_contract_maximum() {
    let server = spawn_mock(vec![(200, fixture("claim.no-work.response.json"))]);
    client(&server.base_url)
        .claim(
            &session(),
            ClaimRequest {
                claim_request_id: ClaimRequestId::new("claim_x"),
                available_capacity: 1,
                wait: Duration::from_secs(600),
            },
        )
        .await
        .expect("claim succeeds");
    let recorded = server.requests.lock().expect("requests").clone();
    assert_eq!(recorded[0].body["wait_ms"], CLAIM_WAIT_MS_MAX);
}

#[tokio::test]
async fn heartbeat_round_trips_the_frozen_request_and_response() {
    let server = spawn_mock(vec![(200, fixture("heartbeat.response.json"))]);
    let request: HeartbeatRequest =
        serde_json::from_str(&fixture("heartbeat.request.json")).expect("frozen request");
    let response = client(&server.base_url)
        .heartbeat(&session(), request.clone())
        .await
        .expect("heartbeat succeeds");
    assert_eq!(response.heartbeat_id, request.heartbeat_id);
    assert_eq!(response.lease_results.len(), 1);
    assert!(!response.lease_results[0].cancellation_requested);

    let recorded = server.requests.lock().expect("requests").clone();
    let frozen: Value =
        serde_json::from_str(&fixture("heartbeat.request.json")).expect("frozen json");
    assert_eq!(recorded[0].body, frozen, "the request must be the fixture");
}

#[tokio::test]
async fn accept_and_start_use_the_two_attempt_scoped_routes() {
    let server = spawn_mock(vec![
        (200, fixture("accept.response.json")),
        (200, fixture("start.response.json")),
    ]);
    let protocol = client(&server.base_url);
    let attempt = AttemptId::new("att_01J00000000000000000000001");
    protocol
        .report_start(
            &session(),
            StartReport {
                attempt_id: attempt.clone(),
                fencing_token: FencingToken(7),
                phase: StartPhase::Preparing,
                workspace_id: Some(crate::client::WorkspaceId::new(
                    "ws_01J0000000000000000000000001",
                )),
                base_revision: Some("0123456789abcdef0123456789abcdef01234567".into()),
                process_id: None,
            },
        )
        .await
        .expect("accept succeeds");
    protocol
        .report_start(
            &session(),
            StartReport {
                attempt_id: attempt.clone(),
                fencing_token: FencingToken(7),
                phase: StartPhase::ProcessObservedRunning,
                workspace_id: Some(crate::client::WorkspaceId::new(
                    "ws_01J0000000000000000000000001",
                )),
                base_revision: Some("0123456789abcdef0123456789abcdef01234567".into()),
                process_id: Some("40213".into()),
            },
        )
        .await
        .expect("start succeeds");

    let recorded = server.requests.lock().expect("requests").clone();
    assert_eq!(
        recorded[0].path,
        "/api/runner/v1/attempts/att_01J00000000000000000000001/accept"
    );
    assert_eq!(
        recorded[1].path,
        "/api/runner/v1/attempts/att_01J00000000000000000000001/start"
    );
    let accept_fixture: Value =
        serde_json::from_str(&fixture("accept.request.json")).expect("frozen accept");
    assert_eq!(recorded[0].body, accept_fixture);
    let start_fixture: Value =
        serde_json::from_str(&fixture("start.request.json")).expect("frozen start");
    assert_eq!(recorded[1].body, start_fixture);
}

#[tokio::test]
async fn reporting_running_without_a_process_id_is_typed_not_sent() {
    let server = spawn_mock(vec![(200, fixture("start.response.json"))]);
    let error = client(&server.base_url)
        .report_start(
            &session(),
            StartReport {
                attempt_id: AttemptId::new("att_1"),
                fencing_token: FencingToken(7),
                phase: StartPhase::ProcessObservedRunning,
                workspace_id: Some(crate::client::WorkspaceId::new("ws_1")),
                base_revision: Some("rev".into()),
                process_id: None,
            },
        )
        .await
        .expect_err("a running report without a process id is invalid");
    assert_eq!(
        error,
        ProtocolClientError::Protocol {
            code: StableErrorCode::InvalidRequest
        }
    );
    // Proving the absence directly: nothing reached the server at all.
    assert!(server.requests.lock().expect("requests").is_empty());
}

#[tokio::test]
async fn completion_and_cancellation_and_recovery_parse_their_frozen_responses() {
    let server = spawn_mock(vec![
        (200, fixture("completion.response.json")),
        (200, fixture("cancellation.response.json")),
        (200, fixture("recovery-observation.response.json")),
    ]);
    let protocol = client(&server.base_url);
    let completion: CompletionReport =
        serde_json::from_str(&fixture("completion.request.json")).expect("frozen completion");
    let response = protocol
        .report_completion(&session(), completion)
        .await
        .expect("completion succeeds");
    assert_eq!(response.state, AttemptState::Succeeded);
    assert!(!response.replayed);

    let cancellation: CancellationReport =
        serde_json::from_str(&fixture("cancellation.request.json")).expect("frozen cancellation");
    let response = protocol
        .report_cancellation(&session(), cancellation)
        .await
        .expect("cancellation succeeds");
    assert_eq!(response.state, AttemptState::Cancelled);

    let recovery: RecoveryObservationRequest =
        serde_json::from_str(&fixture("recovery-observation.request.json"))
            .expect("frozen recovery");
    let response = protocol
        .observe_recovery(&session(), recovery)
        .await
        .expect("recovery succeeds");
    assert!(!response.replayed);

    let recorded = server.requests.lock().expect("requests").clone();
    assert_eq!(
        recorded[0].path,
        "/api/runner/v1/attempts/att_01J00000000000000000000001/completion"
    );
    assert_eq!(
        recorded[1].path,
        "/api/runner/v1/attempts/att_01J00000000000000000000001/cancellation-observation"
    );
    assert_eq!(
        recorded[2].path,
        "/api/runner/v1/attempts/att_01J00000000000000000000001/recovery-observation"
    );
}

#[tokio::test]
async fn events_decisions_and_artifacts_use_their_routes_and_frozen_shapes() {
    let server = spawn_mock(vec![
        (200, fixture("event-batch.response.json")),
        (200, fixture("decision.create.response.json")),
        (200, fixture("decision.poll.response.json")),
        (200, fixture("artifact.response.json")),
    ]);
    let protocol = client(&server.base_url);
    let attempt = AttemptId::new("att_01J00000000000000000000001");

    let frozen_events: Value =
        serde_json::from_str(&fixture("event-batch.request.json")).expect("frozen events");
    let events: Vec<ProtocolEvent> =
        serde_json::from_value(frozen_events["events"].clone()).expect("frozen event list");
    let response = protocol
        .submit_events(
            &session(),
            EventBatchReport {
                attempt_id: attempt.clone(),
                fencing_token: FencingToken(7),
                previous_checkpoint: Some(Checkpoint::new("checkpoint-0002")),
                checkpoint: Checkpoint::new("checkpoint-0004"),
                events,
            },
        )
        .await
        .expect("event batch succeeds");
    assert_eq!(response.accepted_event_ids.len(), 2);
    assert_eq!(
        response.committed_checkpoint,
        Some(Checkpoint::new("checkpoint-0004"))
    );

    let frozen_decision: Value =
        serde_json::from_str(&fixture("decision.create.request.json")).expect("frozen decision");
    let created = protocol
        .create_decision(
            &session(),
            DecisionCreateReport {
                attempt_id: attempt.clone(),
                fencing_token: FencingToken(7),
                decision_id: "dec_01J000000000000000000000001".into(),
                kind: "tool_permission".into(),
                prompt: "Allow the harness to run the focused database test?".into(),
                options: serde_json::from_value(frozen_decision["options"].clone())
                    .expect("frozen options"),
                expires_at: Timestamp::new("2026-08-06T12:30:00Z"),
                metadata: frozen_decision["metadata"]
                    .as_object()
                    .cloned()
                    .expect("frozen metadata"),
            },
        )
        .await
        .expect("decision creation succeeds");
    assert_eq!(created.state, "pending");

    let polled = protocol
        .poll_decisions(
            &session(),
            DecisionPollReport {
                attempt_id: attempt.clone(),
                fencing_token: FencingToken(7),
                after: Some(Timestamp::new("2026-08-06T12:20:59Z")),
            },
        )
        .await
        .expect("decision poll succeeds");
    assert_eq!(polled.decisions.len(), 1);
    assert_eq!(
        polled.decisions[0]
            .answer
            .as_ref()
            .and_then(|answer| answer.option_id.as_deref()),
        Some("allow_once")
    );

    let frozen_manifest: Value =
        serde_json::from_str(&fixture("artifact.request.json")).expect("frozen manifest");
    let grants = protocol
        .submit_artifact_manifest(
            &session(),
            ArtifactManifestReport {
                attempt_id: attempt.clone(),
                fencing_token: FencingToken(7),
                artifacts: serde_json::from_value(frozen_manifest["artifacts"].clone())
                    .expect("frozen artifact list"),
            },
        )
        .await
        .expect("manifest succeeds");
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].state, "manifest_accepted");
    assert_eq!(grants[0].method, "PUT");

    let recorded = server.requests.lock().expect("requests").clone();
    let paths: Vec<&str> = recorded.iter().map(|entry| entry.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "/api/runner/v1/attempts/att_01J00000000000000000000001/events",
            "/api/runner/v1/attempts/att_01J00000000000000000000001/decisions",
            "/api/runner/v1/attempts/att_01J00000000000000000000001/decisions/poll",
            "/api/runner/v1/attempts/att_01J00000000000000000000001/artifacts",
        ]
    );
    assert_eq!(recorded[0].body, frozen_events);
    assert_eq!(recorded[3].body, frozen_manifest);
}

#[tokio::test]
async fn artifact_content_follows_the_server_grant_and_carries_the_fence_header() {
    let server = spawn_mock(vec![
        (200, fixture("artifact.response.json")),
        (204, String::new()),
    ]);
    let protocol = client(&server.base_url);
    let frozen_manifest: Value =
        serde_json::from_str(&fixture("artifact.request.json")).expect("frozen manifest");
    let grants = protocol
        .submit_artifact_manifest(
            &session(),
            ArtifactManifestReport {
                attempt_id: AttemptId::new("att_01J00000000000000000000001"),
                fencing_token: FencingToken(7),
                artifacts: serde_json::from_value(frozen_manifest["artifacts"].clone())
                    .expect("frozen artifact list"),
            },
        )
        .await
        .expect("manifest succeeds");
    protocol
        .put_artifact_content(
            &session(),
            FencingToken(7),
            &grants[0],
            Some("text/x-diff"),
            b"hello world\n".to_vec(),
        )
        .await
        .expect("content upload succeeds");

    let recorded = server.requests.lock().expect("requests").clone();
    assert_eq!(recorded[1].method, "PUT");
    // The path is the server's own grant, not a reconstruction.
    assert_eq!(recorded[1].path, grants[0].path);
    assert_eq!(
        recorded[1].headers.get(ARTIFACT_FENCING_TOKEN_HEADER),
        Some(&"7".to_owned())
    );
    assert_eq!(
        recorded[1].headers.get("content-type"),
        Some(&"text/x-diff".to_owned())
    );
    assert_eq!(recorded[1].raw_body, "hello world\n");
}

// -----------------------------------------------------------------
// Error mapping — asserted against errors/*.json, not hand-written JSON.
// -----------------------------------------------------------------

#[tokio::test]
async fn every_frozen_error_fixture_maps_to_its_typed_variant() {
    let expectations: Vec<(&str, u16, ProtocolClientError)> = vec![
        ("stale-lease.json", 409, ProtocolClientError::StaleLease),
        (
            "runner-revoked.json",
            403,
            ProtocolClientError::RunnerRevoked,
        ),
        (
            "conflict.json",
            409,
            ProtocolClientError::Protocol {
                code: StableErrorCode::Conflict,
            },
        ),
        (
            "idempotency-conflict.json",
            409,
            ProtocolClientError::Protocol {
                code: StableErrorCode::IdempotencyConflict,
            },
        ),
        (
            "unauthorized.json",
            401,
            ProtocolClientError::Protocol {
                code: StableErrorCode::Unauthorized,
            },
        ),
        (
            "forbidden.json",
            403,
            ProtocolClientError::Protocol {
                code: StableErrorCode::Forbidden,
            },
        ),
        (
            "not-found.json",
            404,
            ProtocolClientError::Protocol {
                code: StableErrorCode::NotFound,
            },
        ),
        (
            "invalid-request.json",
            400,
            ProtocolClientError::Protocol {
                code: StableErrorCode::InvalidRequest,
            },
        ),
        (
            "invalid-transition.json",
            409,
            ProtocolClientError::Protocol {
                code: StableErrorCode::InvalidTransition,
            },
        ),
        (
            "decision-expired.json",
            409,
            ProtocolClientError::Protocol {
                code: StableErrorCode::DecisionExpired,
            },
        ),
        (
            "artifact-checksum-mismatch.json",
            422,
            ProtocolClientError::Protocol {
                code: StableErrorCode::ArtifactChecksumMismatch,
            },
        ),
        (
            "payload-too-large.json",
            413,
            ProtocolClientError::Protocol {
                code: StableErrorCode::PayloadTooLarge,
            },
        ),
        (
            "rate-limited.json",
            429,
            ProtocolClientError::Protocol {
                code: StableErrorCode::RateLimited,
            },
        ),
        (
            "unsupported-protocol.json",
            400,
            ProtocolClientError::Protocol {
                code: StableErrorCode::UnsupportedProtocol,
            },
        ),
        (
            "internal-error.json",
            500,
            ProtocolClientError::Protocol {
                code: StableErrorCode::InternalError,
            },
        ),
    ];
    // Every stable code in `protocol.json` must be covered, so a code
    // added later cannot silently go unmapped.
    assert_eq!(expectations.len(), 15);

    for (name, status, expected) in expectations {
        let body = fixture(&format!("errors/{name}"));
        let mapped = map_error_body(
            StatusCode::from_u16(status).expect("status"),
            body.as_bytes(),
        );
        assert_eq!(mapped, expected, "{name} must map to its typed variant");
    }
}

#[tokio::test]
async fn a_stale_lease_never_arrives_as_a_generic_conflict() {
    // `stale_lease`, `conflict` and `idempotency_conflict` all arrive as
    // HTTP 409. Branching on the status line would collapse them; the
    // body's stable code is what keeps them distinct.
    let server = spawn_mock(vec![(409, fixture("errors/stale-lease.json"))]);
    let error = client(&server.base_url)
        .report_start(
            &session(),
            StartReport {
                attempt_id: AttemptId::new("att_01J00000000000000000000001"),
                fencing_token: FencingToken(7),
                phase: StartPhase::Preparing,
                workspace_id: Some(crate::client::WorkspaceId::new("ws_1")),
                base_revision: Some("rev".into()),
                process_id: None,
            },
        )
        .await
        .expect_err("a stale fence is refused");
    assert_eq!(error, ProtocolClientError::StaleLease);
    assert_ne!(
        error,
        ProtocolClientError::Protocol {
            code: StableErrorCode::Conflict
        }
    );
}

#[tokio::test]
async fn a_non_envelope_error_body_claims_no_stable_code() {
    let server = spawn_mock(vec![(400, "<html>proxy error</html>".to_owned())]);
    let error = client(&server.base_url)
        .claim(
            &session(),
            ClaimRequest {
                claim_request_id: ClaimRequestId::new("claim_1"),
                available_capacity: 1,
                wait: Duration::from_millis(1),
            },
        )
        .await
        .expect_err("an unparseable error body is still an error");
    assert_eq!(error, ProtocolClientError::Rejected);
}

// -----------------------------------------------------------------
// Retry discipline.
// -----------------------------------------------------------------

#[tokio::test]
async fn a_retryable_code_is_resent_only_up_to_the_bound() {
    let server = spawn_mock(vec![(500, fixture("errors/internal-error.json"))]);
    let error = client(&server.base_url)
        .heartbeat(
            &session(),
            serde_json::from_str(&fixture("heartbeat.request.json")).expect("frozen request"),
        )
        .await
        .expect_err("internal_error exhausts the bound");
    assert_eq!(
        error,
        ProtocolClientError::Protocol {
            code: StableErrorCode::InternalError
        }
    );
    // max_attempts = 3 means three sends, never an unbounded loop.
    assert_eq!(server.requests.lock().expect("requests").len(), 3);
}

#[tokio::test]
async fn a_non_retryable_code_is_sent_exactly_once() {
    let server = spawn_mock(vec![(409, fixture("errors/stale-lease.json"))]);
    let error = client(&server.base_url)
        .heartbeat(
            &session(),
            serde_json::from_str(&fixture("heartbeat.request.json")).expect("frozen request"),
        )
        .await
        .expect_err("a stale fence is not retryable");
    assert_eq!(error, ProtocolClientError::StaleLease);
    assert_eq!(server.requests.lock().expect("requests").len(), 1);
}

#[tokio::test]
async fn enrollment_is_never_resent_even_when_the_failure_is_retryable() {
    // The token is redeemed exactly once server-side: a lost response is
    // ambiguous, and resending would burn a second token without being
    // able to recover the credential. `internal_error` is retryable by
    // code, so this proves the *idempotency* half of the rule.
    let server = spawn_mock(vec![(500, fixture("errors/internal-error.json"))]);
    let error = client(&server.base_url)
        .enroll(
            &EnrollmentCredential::new(SECRET_ENROLLMENT),
            EnrollmentRequest {
                runner_name: "dev-runner-01".into(),
                runner_version: "0.1.0".into(),
                capabilities: capabilities(),
            },
        )
        .await
        .expect_err("enrollment fails");
    assert_eq!(
        error,
        ProtocolClientError::Protocol {
            code: StableErrorCode::InternalError
        }
    );
    assert_eq!(
        server.requests.lock().expect("requests").len(),
        1,
        "a single-use token must never be resent"
    );
}

// -----------------------------------------------------------------
// Secrets.
// -----------------------------------------------------------------

#[test]
fn secrets_never_appear_in_logs_or_errors() {
    let session = session();
    assert!(!format!("{session:?}").contains(SECRET_CREDENTIAL));
    assert!(!format!("{}", session.credential()).contains(SECRET_CREDENTIAL));
    assert!(!format!("{:?}", session.credential()).contains(SECRET_CREDENTIAL));

    let enrollment = EnrollmentCredential::new(SECRET_ENROLLMENT);
    assert!(!format!("{enrollment:?}").contains(SECRET_ENROLLMENT));

    // Every typed protocol error is rendered by the daemon's `tracing`
    // calls; none of them may carry credential material or a URL.
    for error in [
        ProtocolClientError::StaleLease,
        ProtocolClientError::RunnerRevoked,
        ProtocolClientError::Rejected,
        ProtocolClientError::Transport,
        ProtocolClientError::Protocol {
            code: StableErrorCode::Unauthorized,
        },
    ] {
        let rendered = format!("{error}");
        assert!(!rendered.contains(SECRET_CREDENTIAL));
        assert!(!rendered.contains(SECRET_ENROLLMENT));
    }
}

#[tokio::test]
async fn the_bearer_header_is_the_only_place_a_credential_is_written() {
    let server = spawn_mock(vec![(200, fixture("claim.no-work.response.json"))]);
    client(&server.base_url)
        .claim(
            &session(),
            ClaimRequest {
                claim_request_id: ClaimRequestId::new("claim_1"),
                available_capacity: 1,
                wait: Duration::from_millis(1),
            },
        )
        .await
        .expect("claim succeeds");
    let recorded = server.requests.lock().expect("requests").clone();
    assert!(!recorded[0].raw_body.contains(SECRET_CREDENTIAL));
    assert!(!recorded[0].path.contains(SECRET_CREDENTIAL));
    assert_eq!(
        recorded[0].authorization.as_deref(),
        Some(format!("Bearer {SECRET_CREDENTIAL}").as_str())
    );
}

// -----------------------------------------------------------------
// Session persistence.
// -----------------------------------------------------------------

#[test]
fn a_persisted_session_round_trips_and_is_owner_only() {
    let guard = tempfile::tempdir().expect("temporary directory");
    let directory = guard.path();
    let session = session();
    store_session(directory, &session).expect("session is stored");

    let loaded = load_session(directory).expect("session is readable");
    assert_eq!(loaded.runner_id, session.runner_id);
    assert_eq!(loaded.credential().expose(), SECRET_CREDENTIAL);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(session_path(directory))
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o600,
            "a live credential must not be group/world readable"
        );
    }
}

#[test]
fn a_missing_session_file_is_absent_not_an_error() {
    let guard = tempfile::tempdir().expect("temporary directory");
    // A path under the guard that was never created: an absent session
    // directory must read as absent, not as an error.
    let directory = guard.path().join("absent");
    assert!(load_session(&directory).is_none());
}

#[test]
fn persisted_session_runner_id_reads_the_id_without_a_full_session() {
    let guard = tempfile::tempdir().expect("temporary directory");
    let directory = guard.path();
    let session = session();
    store_session(directory, &session).expect("session is stored");

    assert_eq!(
        persisted_session_runner_id(directory).as_deref(),
        Some(session.runner_id.as_str())
    );
}

#[test]
fn persisted_session_runner_id_is_none_for_a_missing_or_unparseable_session() {
    let guard = tempfile::tempdir().expect("temporary directory");
    let missing = guard.path().join("absent");
    assert!(persisted_session_runner_id(&missing).is_none());

    let malformed = tempfile::tempdir().expect("temporary directory");
    fs::write(session_path(malformed.path()), b"not json").expect("write malformed session");
    assert!(persisted_session_runner_id(malformed.path()).is_none());
}

#[test]
fn base_url_normalization_and_origin_extraction_are_exact() {
    assert_eq!(
        normalize_base_url("http://127.0.0.1:3210/api/runner/v1/"),
        "http://127.0.0.1:3210/api/runner/v1"
    );
    // An operator who supplies only the server origin gets the frozen
    // base path appended, never a request to `/enroll` at the root.
    assert_eq!(
        normalize_base_url("http://127.0.0.1:3210"),
        "http://127.0.0.1:3210/api/runner/v1"
    );
    assert_eq!(
        normalize_base_url("https://tack.test/"),
        "https://tack.test/api/runner/v1"
    );
    assert_eq!(
        origin_of("http://127.0.0.1:3210/api/runner/v1").expect("origin"),
        "http://127.0.0.1:3210"
    );
    assert_eq!(
        origin_of("https://tack.test/api/runner/v1").expect("origin"),
        "https://tack.test"
    );
    assert!(origin_of("not-a-url").is_err());
}

#[test]
fn retry_backoff_is_bounded() {
    let policy = RetryPolicy::default();
    assert_eq!(policy.backoff_for(1), Duration::from_millis(250));
    assert_eq!(policy.backoff_for(2), Duration::from_millis(500));
    assert_eq!(policy.backoff_for(30), policy.max_backoff);
}
