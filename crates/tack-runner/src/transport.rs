//! The real HTTP transport for runner protocol v1.
//!
//! Without this module, [`crate::UnavailableProtocolClient`] is the only
//! production [`RunnerProtocolClient`] in the tree and `reqwest` is not
//! a dependency of this crate — so a packaged `tack-runner` binary could not
//! enroll, claim, heartbeat or report against a live server.
//!
//! Two seams live here and they are deliberately separate:
//!
//! - [`HttpPullProtocol`] implements [`PullProtocol`] (the eight engine-facing
//!   operations) **and** [`AttemptDataProtocol`] (events, decisions,
//!   decision polling, artifact manifests, artifact content). Together they
//!   cover all fourteen `/api/runner/v1` routes.
//! - [`HttpRunnerClient`] implements [`RunnerProtocolClient`]: the daemon
//!   loop that enrolls (or resumes a persisted session), replays unresolved
//!   journal records, then claims and heartbeats until shutdown.
//!
//! ## Authority
//!
//! `docs/contracts/runner-v1/` is the authority for every payload here. Where
//! a response body and a fixture could disagree this module follows the
//! fixture; the one place the wire carries information no fixture fixes — the
//! artifact content upload URL — it follows the server's own
//! `upload.path`/`upload.method` rather than reconstructing a path, because
//! `artifact.response.json` records that grant *as data*.
//!
//! ## Retry discipline
//!
//! Two independent conditions must both hold before anything is resent:
//!
//! 1. the failure is retryable — delegated to
//!    [`ProtocolClientError::is_retryable`], itself derived from
//!    `StableErrorCode::retryable` and thus from `errors/*.json`; and
//! 2. the operation is **replayable by construction** — its payload carries
//!    an idempotency key (`claim_request_id`, `heartbeat_id`, `completion_id`,
//!    `cancellation_request_id`, `recovery_key`, `decision_id`, `artifact_id`,
//!    an event `checkpoint`) or it is a pure read.
//!
//! [`Idempotency::SingleUse`] marks the one operation that fails both tests:
//! **enrollment**. Its token is redeemed exactly once server-side, so a
//! response lost in transit leaves an ambiguous state in which the server may
//! hold a credential the runner never received. Resending would burn a token
//! and could not recover the credential anyway. It is reported as a typed
//! transport failure instead — the same "never blind-retry an ambiguous
//! post-spawn state" principle, applied here to credentials instead of
//! process spawning.
//!
//! ## Secrets
//!
//! The enrollment token travels only in the enrollment request body; the
//! runner credential travels only in an `Authorization: Bearer` header. Neither
//! is ever logged, put in an error, or included in a `Debug` rendering —
//! [`RunnerCredential`] and [`crate::EnrollmentCredential`] redact
//! structurally, and this module never calls `expose()` outside the exact
//! place the byte is written onto the wire. `secrets_never_appear_in_logs_or_errors`
//! asserts it.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use reqwest::{Client, Method, StatusCode, header};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tack_orch::execution::{
    ProtocolErrorEnvelope, ProtocolVersion, RecoveryObservationRequest,
    RecoveryObservationResponse, RunnerCapabilities, StableErrorCode,
};

use crate::{
    Clock, EnrollmentCredential, RunnerConfig, RunnerError, Shutdown,
    client::engine::{RunCycle, RunnerEngine},
    client::{
        AttemptId, AttemptLease, AttemptState, CancellationReport, CancellationResponse,
        Checkpoint, ClaimRequest, ClaimRequestId, ClaimResult, ClaimedWork, CompletionReport,
        CompletionResponse, EnrollmentRequest, EnrollmentResponse, FencingToken, HeartbeatRequest,
        HeartbeatResponse, ProtocolClientError, PullProtocol, RefreshRequest, RefreshResponse,
        RunnerCredential, RunnerId, RunnerProtocolClient, RunnerSession, StartPhase, StartReport,
        Timestamp,
    },
};

/// `protocol.json`'s `base_path`. Fixed by the contract, so the client owns
/// it rather than requiring every operator to spell it into `--api-url`.
const PROTOCOL_BASE_PATH: &str = "/api/runner/v1";

/// `limits.json`'s `claim_wait_ms_max`. A larger `wait` is rejected by the
/// server with `payload_too_large`, so the client clamps rather than sends a
/// request it already knows is invalid.
const CLAIM_WAIT_MS_MAX: u64 = 30_000;

/// Header the server reads for the fencing token on artifact content uploads,
/// where the body is raw bytes and the token cannot travel in JSON. Not part
/// of any frozen fixture — mirrored from
/// `crates/tack-api/src/handlers/runner_protocol.rs`'s own constant, which
/// documents it as that route's addition.
const ARTIFACT_FENCING_TOKEN_HEADER: &str = "x-tack-fencing-token";

/// Filename under `state_dir` holding the enrolled session. Owner-only.
const SESSION_FILE: &str = "session.json";

/// How much longer than the server-side long-poll window a claim request may
/// take before the client gives up on it.
const CLAIM_TIMEOUT_MARGIN: Duration = Duration::from_secs(15);

// ---------------------------------------------------------------------
// Retry policy
// ---------------------------------------------------------------------

/// Bounded retry. `max_attempts` counts total sends, so `1` disables retry
/// entirely; there is no unbounded mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(250),
            max_backoff: Duration::from_secs(5),
        }
    }
}

impl RetryPolicy {
    fn backoff_for(&self, already_sent: u32) -> Duration {
        let factor = 1_u32 << already_sent.saturating_sub(1).min(16);
        self.initial_backoff
            .saturating_mul(factor)
            .min(self.max_backoff)
    }
}

/// Whether an operation's payload makes a resend safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Idempotency {
    /// Carries an idempotency key, a fencing token or is a pure read.
    Replayable,
    /// Cannot be resent without risking an ambiguous outcome. Today this is
    /// enrollment alone (single-use token).
    SingleUse,
}

// ---------------------------------------------------------------------
// The client
// ---------------------------------------------------------------------

/// A real `/api/runner/v1` client.
pub struct HttpPullProtocol {
    http: Client,
    base_url: String,
    retry: RetryPolicy,
}

impl HttpPullProtocol {
    /// `api_base_url` is the protocol base (`.../api/runner/v1`); a trailing
    /// slash is tolerated. `request_timeout` bounds every non-claim call.
    pub fn new(
        api_base_url: &str,
        request_timeout: Duration,
        retry: RetryPolicy,
    ) -> Result<Self, RunnerError> {
        let http = Client::builder()
            .timeout(request_timeout)
            .connect_timeout(request_timeout.min(Duration::from_secs(10)))
            .user_agent(concat!("tack-runner/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| RunnerError::ProtocolTransport)?;
        Ok(Self {
            http,
            base_url: normalize_base_url(api_base_url),
            retry,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// One JSON exchange, with the two-condition retry discipline documented
    /// at module level.
    async fn send_json(
        &self,
        method: Method,
        path: &str,
        credential: Option<&RunnerCredential>,
        body: &Value,
        idempotency: Idempotency,
        timeout: Option<Duration>,
    ) -> Result<Value, ProtocolClientError> {
        let url = self.url(path);
        let mut sent = 0_u32;
        loop {
            sent += 1;
            let mut request = self.http.request(method.clone(), &url).json(body);
            if let Some(credential) = credential {
                // The only place a runner credential is written onto the
                // wire. It is never formatted anywhere else.
                request = request.bearer_auth(credential.expose());
            }
            if let Some(timeout) = timeout {
                request = request.timeout(timeout);
            }
            let outcome = match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    let bytes = response.bytes().await.unwrap_or_default();
                    if status.is_success() {
                        return serde_json::from_slice::<Value>(&bytes)
                            .map_err(|_| ProtocolClientError::Rejected);
                    }
                    map_error_body(status, &bytes)
                }
                // `reqwest`'s error carries the URL and can carry a source
                // chain; it is deliberately dropped rather than wrapped, so
                // nothing about the request can leak into a log line.
                Err(_) => ProtocolClientError::Transport,
            };

            let may_retry = matches!(idempotency, Idempotency::Replayable)
                && outcome.is_retryable()
                && sent < self.retry.max_attempts;
            if !may_retry {
                tracing::debug!(
                    operation = %path,
                    attempts = sent,
                    error = %outcome,
                    "runner protocol call failed"
                );
                return Err(outcome);
            }
            tokio::time::sleep(self.retry.backoff_for(sent)).await;
        }
    }
}

/// Resolves a configured URL to the protocol base.
///
/// `protocol.json` fixes `base_path: "/api/runner/v1"`, so the path is the
/// *client's* knowledge, not the operator's: `--api-url http://host:3210` and
/// `--api-url http://host:3210/api/runner/v1` must both work, and an operator
/// who only knows where the server listens must not have to memorise a
/// contract detail. Appending only when the suffix is absent keeps the
/// existing default (which spells it out) exactly as it was.
fn normalize_base_url(raw: &str) -> String {
    let trimmed = raw.trim_end_matches('/');
    if trimmed.ends_with(PROTOCOL_BASE_PATH) {
        trimmed.to_owned()
    } else {
        format!("{trimmed}{PROTOCOL_BASE_PATH}")
    }
}

/// Maps an HTTP error response onto the typed enum.
///
/// The **body's** stable code wins over the status line: two different codes
/// share a status (`conflict` and `idempotency_conflict` are both 409;
/// `stale_lease` is 409 too), so branching on status alone would misreport
/// `stale_lease` as a generic conflict.
/// A body that is not a v1 envelope yields [`ProtocolClientError::Rejected`],
/// which claims no stable code at all rather than guessing one.
fn map_error_body(status: StatusCode, body: &[u8]) -> ProtocolClientError {
    match serde_json::from_slice::<ProtocolErrorEnvelope>(body) {
        Ok(envelope) => ProtocolClientError::from_stable_code(envelope.error.code),
        Err(_) => {
            if status.is_server_error() {
                // A 5xx with an unparseable body is still a server fault and
                // still retryable; typing it as `internal_error` would claim
                // the server said something it did not.
                ProtocolClientError::Transport
            } else {
                ProtocolClientError::Rejected
            }
        }
    }
}

fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value, ProtocolClientError> {
    value.get(name).ok_or(ProtocolClientError::Rejected)
}

fn as_str(value: &Value, name: &str) -> Result<String, ProtocolClientError> {
    field(value, name)?
        .as_str()
        .map(str::to_owned)
        .ok_or(ProtocolClientError::Rejected)
}

fn as_u64(value: &Value, name: &str) -> Result<u64, ProtocolClientError> {
    field(value, name)?
        .as_u64()
        .ok_or(ProtocolClientError::Rejected)
}

fn optional_str(value: &Value, name: &str) -> Option<String> {
    value.get(name).and_then(Value::as_str).map(str::to_owned)
}

fn typed<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, ProtocolClientError> {
    serde_json::from_value(value.clone()).map_err(|_| ProtocolClientError::Rejected)
}

/// Rejects a response whose `protocol_version` is not v1 rather than parsing
/// a payload whose semantics this client cannot vouch for. `protocol.json`
/// fixes `semantic_changes: require_new_protocol_version`, so a different
/// version is a genuinely different contract.
fn require_v1(value: &Value) -> Result<(), ProtocolClientError> {
    match value.get("protocol_version").and_then(Value::as_u64) {
        Some(1) => Ok(()),
        Some(_) => Err(ProtocolClientError::Protocol {
            code: StableErrorCode::UnsupportedProtocol,
        }),
        None => Err(ProtocolClientError::Rejected),
    }
}

/// Serializes a [`RunnerCapabilities`] into the *embedded* snapshot shape
/// `enrollment.request.json`/`refresh.request.json` carry under
/// `capabilities`. `runner_version` and `protocol_version` are siblings of
/// `capabilities` in those envelopes, never nested inside it (see
/// `EmbeddedCapabilitySnapshot`'s own doc comment in tack-orch), so they are
/// dropped here rather than emitted as extra keys the server would have to
/// absorb into its additive-field bucket.
fn embedded_capabilities(capabilities: &RunnerCapabilities) -> Result<Value, ProtocolClientError> {
    let mut value =
        serde_json::to_value(capabilities).map_err(|_| ProtocolClientError::Rejected)?;
    if let Some(object) = value.as_object_mut() {
        object.remove("runner_version");
        object.remove("protocol_version");
    }
    Ok(value)
}

fn attempt_state_from(value: &Value, name: &str) -> Result<AttemptState, ProtocolClientError> {
    typed(field(value, name)?)
}

#[async_trait]
impl PullProtocol for HttpPullProtocol {
    async fn enroll(
        &self,
        enrollment_credential: &EnrollmentCredential,
        request: EnrollmentRequest,
    ) -> Result<EnrollmentResponse, ProtocolClientError> {
        let body = json!({
            "protocol_version": 1,
            // The only place the enrollment token is written onto the wire.
            "enrollment_token": enrollment_credential.expose(),
            "runner_name": request.runner_name,
            "runner_version": request.runner_version,
            "capabilities": embedded_capabilities(&request.capabilities)?,
        });
        let value = self
            .send_json(
                Method::POST,
                "/enroll",
                None,
                &body,
                Idempotency::SingleUse,
                None,
            )
            .await?;
        require_v1(&value)?;
        Ok(EnrollmentResponse {
            session: RunnerSession::new(
                RunnerId::new(as_str(&value, "runner_id")?),
                RunnerCredential::new(as_str(&value, "runner_credential")?),
                Timestamp::new(as_str(&value, "credential_expires_at")?),
            ),
            heartbeat_interval: Duration::from_secs(as_u64(&value, "heartbeat_interval_seconds")?),
            lease_duration: Duration::from_secs(as_u64(&value, "lease_duration_seconds")?),
            server_time: Timestamp::new(as_str(&value, "server_time")?),
        })
    }

    async fn refresh(
        &self,
        session: &RunnerSession,
        request: RefreshRequest,
    ) -> Result<RefreshResponse, ProtocolClientError> {
        let body = json!({
            "protocol_version": 1,
            "runner_id": session.runner_id,
            "runner_name": request.runner_name,
            "runner_version": request.runner_version,
            "rotate_credential": request.rotate_credential,
            "capabilities": embedded_capabilities(&request.capabilities)?,
        });
        let value = self
            .send_json(
                Method::POST,
                "/refresh",
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        // `refresh.response.json` carries `runner_credential: null` when no
        // rotation was requested. Retaining the current credential in that
        // case is the fixture's own documented meaning — not a fallback.
        let credential = match value.get("runner_credential").and_then(Value::as_str) {
            Some(rotated) => RunnerCredential::new(rotated),
            None => session.credential().clone(),
        };
        Ok(RefreshResponse {
            session: RunnerSession::new(
                RunnerId::new(as_str(&value, "runner_id")?),
                credential,
                Timestamp::new(as_str(&value, "credential_expires_at")?),
            ),
            accepted_at: Timestamp::new(as_str(&value, "accepted_at")?),
        })
    }

    async fn claim(
        &self,
        session: &RunnerSession,
        request: ClaimRequest,
    ) -> Result<ClaimResult, ProtocolClientError> {
        let wait_ms = u64::try_from(request.wait.as_millis())
            .unwrap_or(CLAIM_WAIT_MS_MAX)
            .min(CLAIM_WAIT_MS_MAX);
        let body = json!({
            "protocol_version": 1,
            "runner_id": session.runner_id,
            "claim_request_id": request.claim_request_id,
            "available_capacity": request.available_capacity,
            "wait_ms": wait_ms,
        });
        let value = self
            .send_json(
                Method::POST,
                "/claim",
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                Some(Duration::from_millis(wait_ms) + CLAIM_TIMEOUT_MARGIN),
            )
            .await?;
        require_v1(&value)?;
        let lease_value = field(&value, "lease")?;
        if lease_value.is_null() {
            return Ok(ClaimResult::NoWork {
                retry_after: Duration::from_millis(as_u64(&value, "retry_after_ms")?),
                reason: as_str(&value, "reason")?,
            });
        }

        let attempt_value = field(&value, "attempt")?;
        // `claim.response.json`'s `lease` carries neither `attempt_number`
        // nor `state`; both live on the attempt snapshot beside it. Building
        // the engine-facing `AttemptLease` from both halves is what makes
        // `ClaimedWork::workspace_repository`'s redundancy check meaningful
        // — it re-validates the two against each other before any local
        // side effect.
        let lease = AttemptLease {
            attempt_id: AttemptId::new(as_str(lease_value, "attempt_id")?),
            runner_id: RunnerId::new(as_str(lease_value, "runner_id")?),
            fencing_token: FencingToken(as_u64(lease_value, "fencing_token")?),
            attempt_number: u32::try_from(as_u64(attempt_value, "attempt_number")?)
                .map_err(|_| ProtocolClientError::Rejected)?,
            state: attempt_state_from(attempt_value, "state")?,
            issued_at: Timestamp::new(as_str(lease_value, "issued_at")?),
            expires_at: Timestamp::new(as_str(lease_value, "expires_at")?),
        };
        Ok(ClaimResult::Work(Box::new(ClaimedWork {
            claim_request_id: ClaimRequestId::new(as_str(&value, "claim_request_id")?),
            lease,
            request: typed(field(&value, "request")?)?,
            attempt: typed(attempt_value)?,
        })))
    }

    async fn heartbeat(
        &self,
        session: &RunnerSession,
        request: HeartbeatRequest,
    ) -> Result<HeartbeatResponse, ProtocolClientError> {
        let body = serde_json::to_value(&request).map_err(|_| ProtocolClientError::Rejected)?;
        let value = self
            .send_json(
                Method::POST,
                "/heartbeat",
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        typed(&value)
    }

    async fn report_start(
        &self,
        session: &RunnerSession,
        report: StartReport,
    ) -> Result<(), ProtocolClientError> {
        // Both phases are required by the server to carry `workspace_id` and
        // `base_revision`. `StartReport` models them as optional because the
        // type predates that requirement; an absent value is reported as
        // a typed invalid_request instead of being sent as `null` for the
        // server to reject with a less specific message.
        let (workspace_id, base_revision) = match (&report.workspace_id, &report.base_revision) {
            (Some(workspace_id), Some(base_revision)) => (workspace_id, base_revision),
            _ => {
                return Err(ProtocolClientError::Protocol {
                    code: StableErrorCode::InvalidRequest,
                });
            }
        };
        let mut body = json!({
            "protocol_version": 1,
            "runner_id": session.runner_id,
            "attempt_id": report.attempt_id,
            "fencing_token": report.fencing_token,
            "workspace_id": workspace_id,
            "base_revision": base_revision,
        });
        let path = match report.phase {
            StartPhase::Preparing => {
                format!("/attempts/{}/accept", report.attempt_id)
            }
            StartPhase::ProcessObservedRunning => {
                let Some(process_id) = report.process_id.as_deref() else {
                    return Err(ProtocolClientError::Protocol {
                        code: StableErrorCode::InvalidRequest,
                    });
                };
                body["process_id"] = json!(process_id);
                format!("/attempts/{}/start", report.attempt_id)
            }
        };
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        Ok(())
    }

    async fn report_completion(
        &self,
        session: &RunnerSession,
        report: CompletionReport,
    ) -> Result<CompletionResponse, ProtocolClientError> {
        let body = serde_json::to_value(&report).map_err(|_| ProtocolClientError::Rejected)?;
        let path = format!("/attempts/{}/completion", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        Ok(CompletionResponse {
            protocol_version: ProtocolVersion::v1(),
            attempt_id: AttemptId::new(as_str(&value, "attempt_id")?),
            completion_id: crate::client::CompletionId::new(as_str(&value, "completion_id")?),
            state: attempt_state_from(&value, "state")?,
            replayed: field(&value, "replayed")?
                .as_bool()
                .ok_or(ProtocolClientError::Rejected)?,
            committed_at: Timestamp::new(as_str(&value, "committed_at")?),
        })
    }

    async fn report_cancellation(
        &self,
        session: &RunnerSession,
        report: CancellationReport,
    ) -> Result<CancellationResponse, ProtocolClientError> {
        let body = serde_json::to_value(&report).map_err(|_| ProtocolClientError::Rejected)?;
        let path = format!("/attempts/{}/cancellation-observation", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        Ok(CancellationResponse {
            protocol_version: ProtocolVersion::v1(),
            attempt_id: AttemptId::new(as_str(&value, "attempt_id")?),
            cancellation_request_id: crate::client::CancellationRequestId::new(as_str(
                &value,
                "cancellation_request_id",
            )?),
            state: attempt_state_from(&value, "state")?,
            replayed: field(&value, "replayed")?
                .as_bool()
                .ok_or(ProtocolClientError::Rejected)?,
            committed_at: Timestamp::new(as_str(&value, "committed_at")?),
        })
    }

    async fn observe_recovery(
        &self,
        session: &RunnerSession,
        report: RecoveryObservationRequest,
    ) -> Result<RecoveryObservationResponse, ProtocolClientError> {
        let body = serde_json::to_value(&report).map_err(|_| ProtocolClientError::Rejected)?;
        let path = format!("/attempts/{}/recovery-observation", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        typed(&value)
    }
}

// ---------------------------------------------------------------------
// Events, decisions and artifacts
// ---------------------------------------------------------------------

/// The five attempt-scoped operations [`PullProtocol`] does not model.
///
/// A **separate trait**, not new methods on `PullProtocol`: widening
/// `PullProtocol` would force edits to every test fake for methods some of
/// them never call. `engine.rs` calls `submit_events` and
/// `submit_terminal_evidence` (artifacts) through this trait;
/// `create_decision`/`poll_decisions` are implemented but still have no
/// caller — no harness adapter in this tree ever asks a question a decision
/// could answer.
#[async_trait]
pub trait AttemptDataProtocol: Send + Sync {
    async fn submit_events(
        &self,
        session: &RunnerSession,
        report: EventBatchReport,
    ) -> Result<EventBatchResponse, ProtocolClientError>;

    async fn create_decision(
        &self,
        session: &RunnerSession,
        report: DecisionCreateReport,
    ) -> Result<DecisionCreateResponse, ProtocolClientError>;

    async fn poll_decisions(
        &self,
        session: &RunnerSession,
        report: DecisionPollReport,
    ) -> Result<DecisionPollResponse, ProtocolClientError>;

    async fn submit_artifact_manifest(
        &self,
        session: &RunnerSession,
        report: ArtifactManifestReport,
    ) -> Result<Vec<ArtifactUploadGrant>, ProtocolClientError>;

    /// Uploads the bytes for a manifest-accepted artifact, following the
    /// `method`/`path` the server returned in [`ArtifactUploadGrant`].
    async fn put_artifact_content(
        &self,
        session: &RunnerSession,
        fencing_token: FencingToken,
        grant: &ArtifactUploadGrant,
        media_type: Option<&str>,
        content: Vec<u8>,
    ) -> Result<(), ProtocolClientError>;
}

/// One `event-batch.request.json` event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolEvent {
    pub event_id: String,
    pub sequence: u64,
    pub occurred_at: Timestamp,
    pub source: String,
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventBatchReport {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub previous_checkpoint: Option<Checkpoint>,
    pub checkpoint: Checkpoint,
    pub events: Vec<ProtocolEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventBatchResponse {
    pub attempt_id: AttemptId,
    pub accepted_event_ids: Vec<String>,
    pub duplicate_event_ids: Vec<String>,
    pub committed_checkpoint: Option<Checkpoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionOption {
    pub option_id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionCreateReport {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub decision_id: String,
    pub kind: String,
    pub prompt: String,
    pub options: Vec<DecisionOption>,
    pub expires_at: Timestamp,
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionCreateResponse {
    pub decision_id: String,
    pub state: String,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionPollReport {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    /// `null` on the first poll: `decision.poll.request.json` models "from
    /// the beginning" as an absent cursor, never as a zero timestamp.
    pub after: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionAnswer {
    #[serde(default)]
    pub option_id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedDecision {
    pub decision_id: String,
    pub state: String,
    #[serde(default)]
    pub answer: Option<DecisionAnswer>,
    #[serde(default)]
    pub resolved_at: Option<Timestamp>,
    #[serde(default)]
    pub resolved_by: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionPollResponse {
    pub decisions: Vec<ResolvedDecision>,
    #[serde(default)]
    pub next_after: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactManifestItem {
    pub artifact_id: String,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub media_type: Option<String>,
    pub size_bytes: u64,
    pub sha256: String,
    pub content_disposition: String,
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactManifestReport {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub artifacts: Vec<ArtifactManifestItem>,
}

/// The server's own upload grant. `path` and `method` are followed verbatim
/// rather than reconstructed — `artifact.response.json` records the grant as
/// data, and the server is the authority on its own route layout.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactUploadGrant {
    pub artifact_id: String,
    pub state: String,
    pub method: String,
    pub path: String,
    pub expires_at: Option<Timestamp>,
}

#[async_trait]
impl AttemptDataProtocol for HttpPullProtocol {
    async fn submit_events(
        &self,
        session: &RunnerSession,
        report: EventBatchReport,
    ) -> Result<EventBatchResponse, ProtocolClientError> {
        let body = json!({
            "protocol_version": 1,
            "runner_id": session.runner_id,
            "attempt_id": report.attempt_id,
            "fencing_token": report.fencing_token,
            "previous_checkpoint": report.previous_checkpoint,
            "checkpoint": report.checkpoint,
            "events": report.events,
        });
        let path = format!("/attempts/{}/events", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        typed(&value)
    }

    async fn create_decision(
        &self,
        session: &RunnerSession,
        report: DecisionCreateReport,
    ) -> Result<DecisionCreateResponse, ProtocolClientError> {
        let body = json!({
            "protocol_version": 1,
            "runner_id": session.runner_id,
            "attempt_id": report.attempt_id,
            "fencing_token": report.fencing_token,
            "decision_id": report.decision_id,
            "kind": report.kind,
            "prompt": report.prompt,
            "options": report.options,
            "expires_at": report.expires_at,
            "metadata": report.metadata,
        });
        let path = format!("/attempts/{}/decisions", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        typed(&value)
    }

    async fn poll_decisions(
        &self,
        session: &RunnerSession,
        report: DecisionPollReport,
    ) -> Result<DecisionPollResponse, ProtocolClientError> {
        let body = json!({
            "protocol_version": 1,
            "runner_id": session.runner_id,
            "attempt_id": report.attempt_id,
            "fencing_token": report.fencing_token,
            "after": report.after,
        });
        let path = format!("/attempts/{}/decisions/poll", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        typed(&value)
    }

    async fn submit_artifact_manifest(
        &self,
        session: &RunnerSession,
        report: ArtifactManifestReport,
    ) -> Result<Vec<ArtifactUploadGrant>, ProtocolClientError> {
        let body = json!({
            "protocol_version": 1,
            "runner_id": session.runner_id,
            "attempt_id": report.attempt_id,
            "fencing_token": report.fencing_token,
            "artifacts": report.artifacts,
        });
        let path = format!("/attempts/{}/artifacts", report.attempt_id);
        let value = self
            .send_json(
                Method::POST,
                &path,
                Some(session.credential()),
                &body,
                Idempotency::Replayable,
                None,
            )
            .await?;
        require_v1(&value)?;
        let entries = field(&value, "artifacts")?
            .as_array()
            .ok_or(ProtocolClientError::Rejected)?
            .clone();
        entries
            .iter()
            .map(|entry| {
                let upload = field(entry, "upload")?;
                Ok(ArtifactUploadGrant {
                    artifact_id: as_str(entry, "artifact_id")?,
                    state: as_str(entry, "state")?,
                    method: as_str(upload, "method")?,
                    path: as_str(upload, "path")?,
                    expires_at: optional_str(upload, "expires_at").map(Timestamp::new),
                })
            })
            .collect()
    }

    async fn put_artifact_content(
        &self,
        session: &RunnerSession,
        fencing_token: FencingToken,
        grant: &ArtifactUploadGrant,
        media_type: Option<&str>,
        content: Vec<u8>,
    ) -> Result<(), ProtocolClientError> {
        let method = Method::from_bytes(grant.method.as_bytes())
            .map_err(|_| ProtocolClientError::Rejected)?;
        // The grant's `path` is server-absolute (`/api/runner/v1/...`), so it
        // is joined to the origin, not to the protocol base.
        let url = origin_of(&self.base_url)? + &grant.path;
        let mut request = self
            .http
            .request(method, &url)
            .bearer_auth(session.credential().expose())
            .header(ARTIFACT_FENCING_TOKEN_HEADER, fencing_token.0.to_string())
            .body(content);
        if let Some(media_type) = media_type {
            request = request.header(header::CONTENT_TYPE, media_type);
        }
        // Deliberately no retry: the server refuses a second PUT for an
        // artifact whose content is already recorded (content is immutable
        // once verified), so a resend after an ambiguous first send would
        // report `conflict` and mask a success. The manifest exchange above
        // is the replayable half; the byte upload is not.
        match request.send().await {
            Ok(response) if response.status().is_success() => Ok(()),
            Ok(response) => {
                let status = response.status();
                let bytes = response.bytes().await.unwrap_or_default();
                Err(map_error_body(status, &bytes))
            }
            Err(_) => Err(ProtocolClientError::Transport),
        }
    }
}

/// Extracts `scheme://host[:port]` from the configured base URL, without a
/// URL-parsing dependency: the origin ends at the first `/` after `://`.
fn origin_of(base_url: &str) -> Result<String, ProtocolClientError> {
    let scheme_end = base_url.find("://").ok_or(ProtocolClientError::Rejected)? + 3;
    let rest = &base_url[scheme_end..];
    let authority_len = rest.find('/').unwrap_or(rest.len());
    Ok(base_url[..scheme_end + authority_len].to_owned())
}

// ---------------------------------------------------------------------
// Persisted session
// ---------------------------------------------------------------------

/// The enrolled identity, persisted so a restarted runner resumes instead of
/// burning a second single-use enrollment token (which an operator would have
/// to issue by hand). Written owner-only; the file holds a live credential.
#[derive(Serialize, Deserialize)]
struct PersistedSession {
    runner_id: String,
    runner_credential: String,
    credential_expires_at: String,
}

fn session_path(state_dir: &Path) -> PathBuf {
    state_dir.join(SESSION_FILE)
}

fn load_session(state_dir: &Path) -> Option<RunnerSession> {
    let raw = fs::read_to_string(session_path(state_dir)).ok()?;
    let persisted: PersistedSession = serde_json::from_str(&raw).ok()?;
    Some(RunnerSession::new(
        RunnerId::new(persisted.runner_id),
        RunnerCredential::new(persisted.runner_credential),
        Timestamp::new(persisted.credential_expires_at),
    ))
}

/// The runner id a persisted session under `state_dir` claims, without
/// validating whether the server this process talks to still recognises
/// it — only [`establish_session`]'s own `refresh` call settles that.
/// `None` when no session is persisted there, or its contents cannot be
/// parsed; a caller with no persisted session to compare has nothing to
/// distinguish, and a caller of this function decides what "no answer"
/// means for its own case rather than this reporting a guess.
pub fn persisted_session_runner_id(state_dir: &Path) -> Option<String> {
    load_session(state_dir).map(|session| session.runner_id.as_str().to_owned())
}

fn store_session(state_dir: &Path, session: &RunnerSession) -> Result<(), RunnerError> {
    let persisted = PersistedSession {
        runner_id: session.runner_id.as_str().to_owned(),
        runner_credential: session.credential().expose().to_owned(),
        credential_expires_at: session.credential_expires_at().as_str().to_owned(),
    };
    let encoded = serde_json::to_string(&persisted).map_err(|_| RunnerError::Filesystem)?;
    let path = session_path(state_dir);
    let temporary = path.with_extension("json.tmp");
    write_owner_only(&temporary, encoded.as_bytes())?;
    fs::rename(&temporary, &path).map_err(|_| RunnerError::Filesystem)?;
    Ok(())
}

#[cfg(unix)]
fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), RunnerError> {
    use std::os::unix::fs::OpenOptionsExt;

    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| RunnerError::Filesystem)?;
    file.write_all(bytes).map_err(|_| RunnerError::Filesystem)?;
    file.sync_all().map_err(|_| RunnerError::Filesystem)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), RunnerError> {
    fs::write(path, bytes).map_err(|_| RunnerError::Filesystem)
}

// ---------------------------------------------------------------------
// The daemon
// ---------------------------------------------------------------------

/// The production [`RunnerProtocolClient`]: enroll (or resume), replay
/// unresolved journal records, then claim and heartbeat until shutdown.
pub struct HttpRunnerClient<A, W, C> {
    protocol: Arc<HttpPullProtocol>,
    engine: RunnerEngine<Arc<HttpPullProtocol>, A, W, C>,
    config: RunnerConfig,
    clock: C,
    capabilities: RunnerCapabilities,
    sequence: AtomicU64,
}

impl<A, W, C> HttpRunnerClient<A, W, C>
where
    A: crate::client::HarnessAdapter,
    W: crate::client::WorktreeProvisioner,
    C: Clock,
{
    pub fn new(
        protocol: Arc<HttpPullProtocol>,
        engine: RunnerEngine<Arc<HttpPullProtocol>, A, W, C>,
        config: RunnerConfig,
        clock: C,
        capabilities: RunnerCapabilities,
    ) -> Self {
        Self {
            protocol,
            engine,
            config,
            clock,
            capabilities,
            sequence: AtomicU64::new(0),
        }
    }

    fn now_rfc3339(&self) -> String {
        chrono::DateTime::<chrono::Utc>::from(self.clock.now())
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    /// Opaque, per-process-unique identity for one idempotent operation. The
    /// counter (not the clock alone) is what makes two operations issued in
    /// the same second distinct.
    fn next_id(&self, prefix: &str) -> String {
        let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);
        format!(
            "{prefix}_{}_{sequence}",
            self.now_rfc3339().replace([':', '-', '.'], "")
        )
    }

    fn capability_report(&self) -> RunnerCapabilities {
        self.capabilities.clone()
    }

    /// Resumes a persisted session, or enrolls. A persisted session is
    /// validated with a non-rotating `refresh` before it is trusted; if the
    /// server no longer recognises it, and only then, the enrollment token is
    /// spent.
    async fn establish_session(&self) -> Result<RunnerSession, RunnerError> {
        let name = self.config.runner_id.clone();
        let version = env!("CARGO_PKG_VERSION").to_owned();

        if let Some(persisted) = load_session(&self.config.state_dir) {
            match self
                .engine
                .refresh(
                    &persisted,
                    RefreshRequest {
                        runner_name: name.clone(),
                        runner_version: version.clone(),
                        rotate_credential: false,
                        capabilities: self.capability_report(),
                    },
                )
                .await
            {
                Ok(refreshed) => {
                    tracing::info!(runner_id = %refreshed.session.runner_id, "runner session resumed");
                    store_session(&self.config.state_dir, &refreshed.session)?;
                    return Ok(refreshed.session);
                }
                Err(error) => {
                    tracing::warn!(
                        %error,
                        "persisted runner session was refused; falling back to enrollment"
                    );
                }
            }
        }

        let credential = self.config.require_enrollment_credential()?;
        let enrolled = self
            .engine
            .enroll(
                credential,
                EnrollmentRequest {
                    runner_name: name,
                    runner_version: version,
                    capabilities: self.capability_report(),
                },
            )
            .await
            .map_err(|_| RunnerError::ProtocolTransport)?;
        tracing::info!(runner_id = %enrolled.session.runner_id, "runner enrolled");
        store_session(&self.config.state_dir, &enrolled.session)?;
        Ok(enrolled.session)
    }

    /// Keeps `agent_runners.last_heartbeat_at` fresh while the runner holds
    /// no attempt. The engine heartbeats for attempts it is running; nothing
    /// else does, and a runner that never heartbeats is excluded from
    /// scheduling once `max_heartbeat_age` passes.
    async fn idle_heartbeat(&self, session: &RunnerSession) -> Result<(), ProtocolClientError> {
        let sent_at = self.now_rfc3339();
        let request = HeartbeatRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: session.runner_id.clone(),
            heartbeat_id: self.next_id("hb"),
            sent_at: Timestamp::new(sent_at),
            available_capacity: 1,
            active_attempts: Vec::new(),
        };
        self.protocol.heartbeat(session, request).await?;
        Ok(())
    }
}

#[async_trait]
impl<A, W, C> RunnerProtocolClient for HttpRunnerClient<A, W, C>
where
    A: crate::client::HarnessAdapter + 'static,
    W: crate::client::WorktreeProvisioner + 'static,
    C: Clock + 'static,
{
    async fn serve(&self, mut shutdown: Shutdown) -> Result<(), RunnerError> {
        let session = self.establish_session().await?;

        // Replay before claiming anything new: an unresolved journal record
        // is a crash-recovery obligation, and taking fresh work first would
        // let a second attempt race the observation of the first.
        match self.engine.recover(&session).await {
            Ok(outcomes) if !outcomes.is_empty() => {
                tracing::info!(recovered = outcomes.len(), "replayed unresolved attempts");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "recovery pass failed; not claiming new work this cycle");
            }
        }

        let claim_wait = Duration::from_secs(10);
        loop {
            if shutdown.is_requested() {
                return Ok(());
            }
            let claim = ClaimRequest {
                claim_request_id: ClaimRequestId::new(self.next_id("claim")),
                available_capacity: 1,
                wait: claim_wait,
            };
            let cycle = tokio::select! {
                biased;
                () = shutdown.requested() => return Ok(()),
                cycle = self.engine.run_once(&session, claim) => cycle,
            };
            match cycle {
                Ok(RunCycle::NoWork) => {
                    if let Err(error) = self.idle_heartbeat(&session).await {
                        // Same rationale as the `Err(error)` arm below: a
                        // failing idle heartbeat paired with a claim that
                        // returns immediately (nothing queued) would
                        // otherwise spin this loop as fast as the server can
                        // reject each attempt.
                        tracing::warn!(%error, "idle heartbeat failed");
                        tokio::time::sleep(self.protocol.retry.max_backoff).await;
                    }
                }
                Ok(outcome) => tracing::info!(?outcome, "run cycle finished"),
                Err(error) => {
                    // A failed cycle is logged and paced, never retried
                    // immediately: the engine has already journaled whatever
                    // durable state the attempt reached, and a tight retry
                    // loop against a failing server is itself a fault.
                    tracing::warn!(%error, "run cycle failed");
                    tokio::time::sleep(self.protocol.retry.max_backoff).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Mutex, time::UNIX_EPOCH};

    use tack_orch::execution::{
        CapabilityLimits, CapabilityValue, Concurrency, FeatureCapabilities,
    };
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
            serde_json::from_str(&fixture("cancellation.request.json"))
                .expect("frozen cancellation");
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

        let frozen_decision: Value = serde_json::from_str(&fixture("decision.create.request.json"))
            .expect("frozen decision");
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
}
