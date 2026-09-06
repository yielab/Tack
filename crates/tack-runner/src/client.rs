use std::{fmt, sync::Arc, time::Duration};

use async_trait::async_trait;
use tack_orch::execution::{
    ActualExecution, AttemptSnapshot, ExecutionRequestSnapshot, ExecutionState, ProtocolVersion,
    RecoveryObservationRequest, RecoveryObservationResponse, RunnerCapabilities, StableErrorCode,
    Usage,
};

use crate::{EnrollmentCredential, RunnerError, Shutdown};

#[path = "engine.rs"]
pub mod engine;
#[path = "journal.rs"]
pub mod journal;
#[path = "transport.rs"]
pub mod transport;
#[path = "workspace.rs"]
pub mod workspace;

pub use engine::{
    CancelObservation, CancellationEvidence, EngineError, HarnessAdapter, HarnessError,
    HarnessOutcome, LocalRunHandle, RunCycle, RunnerEngine,
};
pub use journal::{
    AttemptJournal, JournalError, JournalState, OwnerOnlyJournal, PendingTerminalReport,
    PendingTerminalReportKind, WorkspaceJournal,
};
pub use tack_orch::execution::RecoveryObservation;
pub use transport::{
    ArtifactManifestItem, ArtifactManifestReport, ArtifactUploadGrant, AttemptDataProtocol,
    DecisionAnswer, DecisionCreateReport, DecisionCreateResponse, DecisionOption,
    DecisionPollReport, DecisionPollResponse, EventBatchReport, EventBatchResponse,
    HttpPullProtocol, HttpRunnerClient, ProtocolEvent, ResolvedDecision, RetryPolicy,
    persisted_session_runner_id,
};
pub use workspace::{
    CleanupResult, UnavailableWorktreeProvisioner, Workspace, WorkspaceError, WorkspaceManager,
    WorktreeProvisioner,
};

macro_rules! opaque_value {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

opaque_value!(
    RunnerId,
    "An opaque runner identity used by this local client seam."
);
opaque_value!(AttemptId, "An opaque execution attempt identity.");
opaque_value!(ClaimRequestId, "An opaque claim idempotency identity.");
opaque_value!(CompletionId, "An opaque completion idempotency identity.");
opaque_value!(
    CancellationRequestId,
    "An opaque cancellation idempotency identity."
);
opaque_value!(WorkspaceId, "An opaque isolated workspace identity.");
opaque_value!(Checkpoint, "An opaque event-stream checkpoint.");
opaque_value!(
    Timestamp,
    "An RFC 3339 timestamp preserved without local interpretation."
);

/// A lease token is intentionally not interchangeable with an opaque ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct FencingToken(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    Leased,
    Preparing,
    Running,
    WaitingDecision,
    Succeeded,
    Failed,
    Cancelled,
    Lost,
    NeedsOperator,
}

impl AttemptState {
    const fn execution_state(self) -> ExecutionState {
        match self {
            Self::Leased => ExecutionState::Leased,
            Self::Preparing => ExecutionState::Preparing,
            Self::Running => ExecutionState::Running,
            Self::WaitingDecision => ExecutionState::WaitingDecision,
            Self::Succeeded => ExecutionState::Succeeded,
            Self::Failed => ExecutionState::Failed,
            Self::Cancelled => ExecutionState::Cancelled,
            Self::Lost => ExecutionState::Lost,
            Self::NeedsOperator => ExecutionState::NeedsOperator,
        }
    }
}

/// The runner's top-level protocol boundary: implementors run the whole
/// daemon loop inside `serve`, until `shutdown` fires.
#[async_trait]
pub trait RunnerProtocolClient: Send + Sync {
    async fn serve(&self, shutdown: Shutdown) -> Result<(), RunnerError>;
}

/// Credential returned once from enrollment. Formatting is always redacted.
#[derive(Clone, PartialEq, Eq)]
pub struct RunnerCredential(String);

impl RunnerCredential {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RunnerCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RunnerCredential([REDACTED])")
    }
}

impl fmt::Display for RunnerCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone)]
pub struct RunnerSession {
    pub runner_id: RunnerId,
    credential: RunnerCredential,
    credential_expires_at: Timestamp,
}

impl RunnerSession {
    pub fn new(
        runner_id: RunnerId,
        credential: RunnerCredential,
        credential_expires_at: Timestamp,
    ) -> Self {
        Self {
            runner_id,
            credential,
            credential_expires_at,
        }
    }

    pub fn credential(&self) -> &RunnerCredential {
        &self.credential
    }

    pub fn credential_expires_at(&self) -> &Timestamp {
        &self.credential_expires_at
    }
}

impl fmt::Debug for RunnerSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunnerSession")
            .field("runner_id", &self.runner_id)
            .field("credential", &self.credential)
            .field("credential_expires_at", &self.credential_expires_at)
            .finish()
    }
}

/// The small, immutable repository subset needed to reserve a local worktree.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RepositorySpec {
    pub remote: String,
    pub base_revision: String,
}

#[derive(Debug, Clone)]
pub struct EnrollmentRequest {
    pub runner_name: String,
    pub runner_version: String,
    pub capabilities: RunnerCapabilities,
}

#[derive(Debug, Clone)]
pub struct EnrollmentResponse {
    pub session: RunnerSession,
    pub heartbeat_interval: Duration,
    pub lease_duration: Duration,
    pub server_time: Timestamp,
}

#[derive(Debug, Clone)]
pub struct RefreshRequest {
    pub runner_name: String,
    pub runner_version: String,
    pub rotate_credential: bool,
    pub capabilities: RunnerCapabilities,
}

#[derive(Debug, Clone)]
pub struct RefreshResponse {
    /// Contains the retained credential when no rotation was requested and the
    /// replacement credential when rotation succeeded.
    pub session: RunnerSession,
    pub accepted_at: Timestamp,
}

#[derive(Debug, Clone)]
pub struct ClaimRequest {
    pub claim_request_id: ClaimRequestId,
    pub available_capacity: u32,
    pub wait: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AttemptLease {
    pub attempt_id: AttemptId,
    pub runner_id: RunnerId,
    pub fencing_token: FencingToken,
    pub attempt_number: u32,
    pub state: AttemptState,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone)]
pub struct ClaimedWork {
    pub claim_request_id: ClaimRequestId,
    pub lease: AttemptLease,
    /// The complete immutable request snapshot returned by the claim response.
    pub request: ExecutionRequestSnapshot,
    /// The complete point-in-time attempt snapshot returned by the claim response.
    pub attempt: AttemptSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClaimedWorkError {
    #[error("claim attempt facts do not agree with the lease")]
    AttemptLeaseMismatch,
    #[error("claim attempt does not belong to its request snapshot")]
    AttemptRequestMismatch,
    #[error("claim repository revision does not agree with the attempt snapshot")]
    RepositoryRevisionMismatch,
}

impl ClaimedWork {
    /// Validates the redundant lease envelope against its preserved snapshots,
    /// then derives the only repository input a workspace may consume.
    pub fn workspace_repository(&self) -> Result<RepositorySpec, ClaimedWorkError> {
        if self.attempt.request_id.as_str() != self.request.request_id.as_str() {
            return Err(ClaimedWorkError::AttemptRequestMismatch);
        }
        if self.attempt.attempt_id.as_str() != self.lease.attempt_id.as_str()
            || self.attempt.runner_id.as_str() != self.lease.runner_id.as_str()
            || self.attempt.fencing_token.0 != self.lease.fencing_token.0
            || self.attempt.attempt_number != self.lease.attempt_number
            || self.attempt.state != self.lease.state.execution_state()
        {
            return Err(ClaimedWorkError::AttemptLeaseMismatch);
        }
        if self.attempt.base_revision != self.request.repository.base_revision {
            return Err(ClaimedWorkError::RepositoryRevisionMismatch);
        }
        Ok(RepositorySpec {
            remote: self.request.repository.remote.clone(),
            base_revision: self.request.repository.base_revision.clone(),
        })
    }
}

#[derive(Debug, Clone)]
pub enum ClaimResult {
    Work(Box<ClaimedWork>),
    NoWork {
        retry_after: Duration,
        reason: String,
    },
}

/// The exact v1 heartbeat attempt fact sent to the runner API.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActiveAttempt {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub state: AttemptState,
    pub journal_state: JournalState,
    pub last_event_checkpoint: Option<Checkpoint>,
}

/// The exact v1 heartbeat request payload.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HeartbeatRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub heartbeat_id: String,
    /// RFC 3339 instant originated from the runner's injected `Clock`.
    pub sent_at: Timestamp,
    pub available_capacity: u32,
    pub active_attempts: Vec<ActiveAttempt>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LeaseResult {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub lease_expires_at: Timestamp,
    pub cancellation_requested: bool,
}

/// The exact v1 heartbeat response payload.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HeartbeatResponse {
    pub protocol_version: ProtocolVersion,
    pub heartbeat_id: String,
    pub accepted_at: Timestamp,
    pub lease_results: Vec<LeaseResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartPhase {
    Preparing,
    ProcessObservedRunning,
}

#[derive(Debug, Clone)]
pub struct StartReport {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub phase: StartPhase,
    /// The planned isolated workspace, required while the server accepts preparation.
    pub workspace_id: Option<WorkspaceId>,
    /// The repository revision reserved for that workspace.
    pub base_revision: Option<String>,
    /// Present only after a local process has been observed running.
    pub process_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompletionReport {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub completion_id: CompletionId,
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub terminal_state: AttemptState,
    pub terminal_reason: serde_json::Value,
    pub final_event_checkpoint: Option<Checkpoint>,
    pub actual_execution: ActualExecution,
    pub usage: Usage,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CancellationReport {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub cancellation_request_id: CancellationRequestId,
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub observation: CancelObservation,
    /// Preserved adapter observation time; the engine never fabricates it.
    pub observed_at: Timestamp,
    /// Non-secret adapter evidence carried verbatim to the protocol boundary.
    pub details: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct CancellationResponse {
    pub protocol_version: ProtocolVersion,
    pub attempt_id: AttemptId,
    pub cancellation_request_id: CancellationRequestId,
    pub state: AttemptState,
    pub replayed: bool,
    pub committed_at: Timestamp,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub protocol_version: ProtocolVersion,
    pub attempt_id: AttemptId,
    pub completion_id: CompletionId,
    pub state: AttemptState,
    pub replayed: bool,
    pub committed_at: Timestamp,
}

/// Typed transport seam: the frozen fixtures specify payloads but not an
/// operation-path map, so this trait defines no HTTP route authority —
/// only implementations like `transport::HttpPullProtocol` do.
#[async_trait]
pub trait PullProtocol: Send + Sync {
    async fn enroll(
        &self,
        enrollment_credential: &EnrollmentCredential,
        request: EnrollmentRequest,
    ) -> Result<EnrollmentResponse, ProtocolClientError>;

    async fn refresh(
        &self,
        session: &RunnerSession,
        request: RefreshRequest,
    ) -> Result<RefreshResponse, ProtocolClientError>;

    async fn claim(
        &self,
        session: &RunnerSession,
        request: ClaimRequest,
    ) -> Result<ClaimResult, ProtocolClientError>;

    async fn heartbeat(
        &self,
        session: &RunnerSession,
        request: HeartbeatRequest,
    ) -> Result<HeartbeatResponse, ProtocolClientError>;

    async fn report_start(
        &self,
        session: &RunnerSession,
        report: StartReport,
    ) -> Result<(), ProtocolClientError>;

    async fn report_completion(
        &self,
        session: &RunnerSession,
        report: CompletionReport,
    ) -> Result<CompletionResponse, ProtocolClientError>;

    async fn report_cancellation(
        &self,
        session: &RunnerSession,
        report: CancellationReport,
    ) -> Result<CancellationResponse, ProtocolClientError>;

    async fn observe_recovery(
        &self,
        session: &RunnerSession,
        report: RecoveryObservationRequest,
    ) -> Result<RecoveryObservationResponse, ProtocolClientError>;
}

/// Shared ownership of one transport by the engine and the daemon loop that
/// also needs it for idle heartbeats. Without this the engine would have to
/// take the only copy.
#[async_trait]
impl<T> PullProtocol for Arc<T>
where
    T: PullProtocol + ?Sized,
{
    async fn enroll(
        &self,
        enrollment_credential: &EnrollmentCredential,
        request: EnrollmentRequest,
    ) -> Result<EnrollmentResponse, ProtocolClientError> {
        (**self).enroll(enrollment_credential, request).await
    }

    async fn refresh(
        &self,
        session: &RunnerSession,
        request: RefreshRequest,
    ) -> Result<RefreshResponse, ProtocolClientError> {
        (**self).refresh(session, request).await
    }

    async fn claim(
        &self,
        session: &RunnerSession,
        request: ClaimRequest,
    ) -> Result<ClaimResult, ProtocolClientError> {
        (**self).claim(session, request).await
    }

    async fn heartbeat(
        &self,
        session: &RunnerSession,
        request: HeartbeatRequest,
    ) -> Result<HeartbeatResponse, ProtocolClientError> {
        (**self).heartbeat(session, request).await
    }

    async fn report_start(
        &self,
        session: &RunnerSession,
        report: StartReport,
    ) -> Result<(), ProtocolClientError> {
        (**self).report_start(session, report).await
    }

    async fn report_completion(
        &self,
        session: &RunnerSession,
        report: CompletionReport,
    ) -> Result<CompletionResponse, ProtocolClientError> {
        (**self).report_completion(session, report).await
    }

    async fn report_cancellation(
        &self,
        session: &RunnerSession,
        report: CancellationReport,
    ) -> Result<CancellationResponse, ProtocolClientError> {
        (**self).report_cancellation(session, report).await
    }

    async fn observe_recovery(
        &self,
        session: &RunnerSession,
        report: RecoveryObservationRequest,
    ) -> Result<RecoveryObservationResponse, ProtocolClientError> {
        (**self).observe_recovery(session, report).await
    }
}

/// Every failure a [`PullProtocol`] implementation may report.
///
/// `StaleLease` and `RunnerRevoked` stay separate variants because the
/// engine's fencing and revocation handling branch on them by name;
/// [`ProtocolClientError::Protocol`] carries the other thirteen
/// `docs/contracts/runner-v1/` stable codes **typed** rather than collapsed
/// into the bare `Rejected` a pre-transport seam had to use.
/// `Rejected` is retained with a narrowed meaning: the server refused the
/// request but its body was not a parseable v1 error envelope, so there is
/// no stable code to report. That is a real, distinguishable condition — a
/// proxy or a non-v1 server — and must not be silently reported as one of
/// the fifteen contract codes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolClientError {
    #[error("stale_lease")]
    StaleLease,
    #[error("runner_revoked")]
    RunnerRevoked,
    /// A stable v1 error code other than `stale_lease`/`runner_revoked`.
    #[error("protocol rejected the request: {code:?}")]
    Protocol { code: StableErrorCode },
    #[error("protocol rejected the request")]
    Rejected,
    #[error("runner protocol transport failed")]
    Transport,
}

impl ProtocolClientError {
    /// Maps a stable contract code onto this enum, preserving the two
    /// variants the engine branches on by name. `stale_lease` can therefore
    /// never surface as a generic conflict.
    pub fn from_stable_code(code: StableErrorCode) -> Self {
        match code {
            StableErrorCode::StaleLease => Self::StaleLease,
            StableErrorCode::RunnerRevoked => Self::RunnerRevoked,
            other => Self::Protocol { code: other },
        }
    }

    /// Whether a conformant client may retry, delegating entirely to
    /// [`StableErrorCode::retryable`] — the single authority derived from
    /// `docs/contracts/runner-v1/errors/*.json`. A transport failure is
    /// retryable only for operations whose payload carries an idempotency
    /// key; that second condition lives in `transport.rs`, not here.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::StaleLease => StableErrorCode::StaleLease.retryable(),
            Self::RunnerRevoked => StableErrorCode::RunnerRevoked.retryable(),
            Self::Protocol { code } => code.retryable(),
            Self::Rejected => false,
            Self::Transport => true,
        }
    }
}

/// Fails explicitly until a route-owning integration supplies a transport.
#[derive(Debug, Default)]
pub struct UnavailableProtocolClient;

#[async_trait]
impl RunnerProtocolClient for UnavailableProtocolClient {
    async fn serve(&self, _shutdown: Shutdown) -> Result<(), RunnerError> {
        Err(RunnerError::ProtocolUnavailable)
    }
}
