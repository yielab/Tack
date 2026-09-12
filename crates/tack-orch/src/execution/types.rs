//! Typed, serializable runner protocol v1 values.

use std::collections::BTreeMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::capabilities::FeatureCapabilities;
use super::lifecycle::LifecycleError;

macro_rules! opaque_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

opaque_id!(RunnerId, "An opaque runner identity.");
opaque_id!(ExecutionRequestId, "An opaque execution request identity.");
opaque_id!(AttemptId, "An opaque execution attempt identity.");
opaque_id!(ItemId, "An opaque Tack item identity.");
opaque_id!(AgentProfileId, "An opaque agent profile identity.");
opaque_id!(WorkspaceId, "An opaque isolated workspace identity.");
opaque_id!(ArtifactId, "An opaque artifact identity.");
opaque_id!(DecisionId, "An opaque decision identity.");
opaque_id!(EventId, "An opaque event identity.");
opaque_id!(ClaimRequestId, "An opaque claim idempotency identity.");
opaque_id!(CompletionId, "An opaque completion idempotency identity.");
opaque_id!(
    CancellationRequestId,
    "An opaque cancellation idempotency identity."
);
opaque_id!(
    RecoveryKey,
    "An opaque recovery-observation idempotency identity."
);
opaque_id!(Checkpoint, "An opaque event-stream checkpoint.");
opaque_id!(IdempotencyKey, "An opaque caller-scoped idempotency key.");
opaque_id!(
    HarnessKind,
    "An opaque harness kind selected by the scheduler."
);
opaque_id!(ModelProvider, "An opaque model provider identifier.");
opaque_id!(
    ModelId,
    "An opaque model identifier; never inspect or split it."
);
opaque_id!(
    RequestedModelProvider,
    "The provider requested by the scheduler; distinct from the observed provider."
);
opaque_id!(
    RequestedModelId,
    "The opaque model requested by the scheduler; distinct from the observed model."
);
opaque_id!(
    ActualModelProvider,
    "The provider actually observed during an attempt; distinct from the request."
);
opaque_id!(
    ActualModelId,
    "The opaque model actually observed during an attempt; distinct from the request."
);

/// The single protocol version this domain understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProtocolVersion(u16);

impl ProtocolVersion {
    pub const V1: Self = Self(1);

    pub const fn v1() -> Self {
        Self::V1
    }

    pub const fn as_u16(self) -> u16 {
        self.0
    }
}

impl Serialize for ProtocolVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(self.0)
    }
}

impl<'de> Deserialize<'de> for ProtocolVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let version = u16::deserialize(deserializer)?;
        if version == Self::V1.0 {
            Ok(Self::V1)
        } else {
            Err(serde::de::Error::custom(format!(
                "unsupported runner protocol version {version}"
            )))
        }
    }
}

/// A lease fencing token. It is numeric and cannot be confused with any ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FencingToken(pub u64);

/// Runner protocol v1's execution state vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Queued,
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

impl ExecutionState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// The non-secret local journal state a runner may attach as recovery evidence.
///
/// This evidence is useful for audit and diagnosis only; it never authorizes a
/// safe requeue. The server decides that disposition from the authenticated
/// runner/fence and its authoritative attempt state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryJournalState {
    Prepared,
    ProcessObservedRunning,
    CancellationRequested,
    RecoveryObserved,
    Reported,
    Quarantined,
}

/// What the runner observed while reconciling a prior local process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryObservation {
    ProcessStopped,
    ProcessRunning,
    Ambiguous,
}

/// Non-secret runner-local evidence attached to a recovery observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryDetails {
    pub journal_state: RecoveryJournalState,
    pub process_observed: bool,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// The server-authoritative outcome of a recovery observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryDisposition {
    SafePreSpawnRequeue,
    NeedsOperator,
    AlreadyTerminal,
}

impl RecoveryDisposition {
    /// The attempt transition implied by this disposition, if it is not a
    /// terminal acknowledgement.
    pub const fn attempt_transition(self) -> Option<ExecutionState> {
        match self {
            Self::SafePreSpawnRequeue => Some(ExecutionState::Lost),
            Self::NeedsOperator => Some(ExecutionState::NeedsOperator),
            Self::AlreadyTerminal => None,
        }
    }

    /// The request transition implied by this disposition, if it is not a
    /// terminal acknowledgement.
    pub const fn request_transition(self) -> Option<ExecutionState> {
        match self {
            Self::SafePreSpawnRequeue => Some(ExecutionState::Queued),
            Self::NeedsOperator => Some(ExecutionState::NeedsOperator),
            Self::AlreadyTerminal => None,
        }
    }

    /// Checks only the public lifecycle/observation preconditions. A true
    /// result for safe requeue is necessary, not sufficient: authoritative
    /// process absence and `started_at` absence remain server-side checks.
    pub const fn is_compatible_with(
        self,
        state: ExecutionState,
        observation: RecoveryObservation,
    ) -> bool {
        let active = matches!(
            state,
            ExecutionState::Leased
                | ExecutionState::Preparing
                | ExecutionState::Running
                | ExecutionState::WaitingDecision
        );
        match self {
            Self::SafePreSpawnRequeue => {
                active && matches!(observation, RecoveryObservation::ProcessStopped)
            }
            Self::NeedsOperator => active,
            Self::AlreadyTerminal => state.is_terminal(),
        }
    }
}

/// The additive runner-v1 recovery-observation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryObservationRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub recovery_key: RecoveryKey,
    pub observation: RecoveryObservation,
    pub details: RecoveryDetails,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// The authoritative response to a recovery observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryObservationResponse {
    pub protocol_version: ProtocolVersion,
    pub attempt_id: AttemptId,
    pub recovery_key: RecoveryKey,
    pub disposition: RecoveryDisposition,
    pub replayed: bool,
    pub committed_at: DateTime<Utc>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// A requested runner/fleet placement. This is intentionally tagged so a
/// runner id cannot be mistaken for a fleet selector by a caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunnerSelector {
    ExactRunner { runner_id: RunnerId },
    Fleet { fleet_id: String },
    Any,
}

/// The immutable resolved agent profile copied into an execution request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentProfileSnapshot {
    pub name: String,
    pub instructions: String,
    pub tool_policy: serde_json::Value,
    pub timeout_seconds: u64,
    pub budgets: serde_json::Value,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// A repository/workspace input snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositorySnapshot {
    pub kind: String,
    pub remote: String,
    pub base_revision: String,
    pub subdirectory: Option<String>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// A requested tool/network policy snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionPolicy {
    #[serde(default)]
    pub tools: Vec<String>,
    pub network: bool,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// An environment value or a secret reference, never a raw runner credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentValue {
    pub value: Option<String>,
    pub secret_reference: Option<String>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Immutable request data copied into every execution attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionRequestSnapshot {
    pub request_id: ExecutionRequestId,
    pub item_id: ItemId,
    pub idempotency_key: IdempotencyKey,
    pub created_by: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub selector: RunnerSelector,
    pub agent_profile_id: AgentProfileId,
    pub resolved_agent_profile: AgentProfileSnapshot,
    pub requested_harness_kind: HarnessKind,
    pub requested_model_provider: Option<RequestedModelProvider>,
    pub requested_model_id: Option<RequestedModelId>,
    pub repository: RepositorySnapshot,
    pub permission_policy: PermissionPolicy,
    pub timeout_seconds: u64,
    pub budgets: serde_json::Value,
    pub status_map_policy_id: Option<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, EnvironmentValue>,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Point-in-time attempt values allocated when a request is claimed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptSnapshot {
    pub attempt_id: AttemptId,
    pub request_id: ExecutionRequestId,
    pub attempt_number: u32,
    pub runner_id: RunnerId,
    pub fencing_token: FencingToken,
    pub state: ExecutionState,
    pub workspace_id: Option<WorkspaceId>,
    pub base_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_issued_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// How a numeric usage figure was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementSource {
    Measured,
    Estimated,
    NotMeasured,
}

/// A nullable metric paired with its provenance, avoiding fabricated zeros.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measurement<T> {
    pub value: Option<T>,
    pub source: MeasurementSource,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Token, duration and cost usage with explicit provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub tokens_in: Measurement<u64>,
    pub tokens_out: Measurement<u64>,
    pub duration_ms: Measurement<u64>,
    pub cost_usd: Measurement<f64>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Actual, as-observed execution facts reported at completion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActualExecution {
    pub harness_kind: HarnessKind,
    pub harness_version: String,
    pub model_provider: ActualModelProvider,
    pub model_id: ActualModelId,
    pub model_observation_source: String,
    pub capability_snapshot: FeatureCapabilities,
    pub workspace_id: WorkspaceId,
    pub base_revision: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

/// Stable protocol error codes. These are the only values clients may branch
/// on; error text and `details` remain display/diagnostic data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StableErrorCode {
    InvalidRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    IdempotencyConflict,
    InvalidTransition,
    StaleLease,
    RunnerRevoked,
    DecisionExpired,
    ArtifactChecksumMismatch,
    PayloadTooLarge,
    RateLimited,
    UnsupportedProtocol,
    InternalError,
}

impl StableErrorCode {
    /// Whether a conformant client may safely retry this error, taken
    /// directly from `docs/contracts/runner-v1/errors/*.json` (the frozen
    /// authority; see `README.md` there). This is the
    /// single source of truth for retryability — no other module, handler or
    /// hand-rolled envelope may re-derive or override it.
    ///
    /// `true` for `conflict`, `internal_error`, `rate_limited` and
    /// `artifact_checksum_mismatch`; `false` for every other stable code.
    /// The conformance test below reads every fixture in that directory and
    /// fails if this mapping and the fixtures ever disagree.
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::Conflict
                | Self::InternalError
                | Self::RateLimited
                | Self::ArtifactChecksumMismatch
        )
    }
}

/// The stable runner-protocol error envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolErrorEnvelope {
    pub error: ProtocolError,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl ProtocolErrorEnvelope {
    /// Builds the full `{"error": {...}}` envelope via [`ProtocolError::new`],
    /// so `retryable` is always derived from `code`. Handler cards that return
    /// a JSON body directly (rather than a typed [`ProtocolError`]) should
    /// call this and serialize the result instead of hand-rolling
    /// `serde_json::json!` with a literal `retryable` value.
    pub fn new(
        code: StableErrorCode,
        message: impl Into<String>,
        request_id: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            error: ProtocolError::new(code, message, request_id, details),
            additional: BTreeMap::new(),
        }
    }
}

/// The structured error body inside [`ProtocolErrorEnvelope`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolError {
    pub code: StableErrorCode,
    pub message: String,
    pub request_id: String,
    pub retryable: bool,
    #[serde(default)]
    pub details: serde_json::Value,
    #[serde(flatten, default)]
    pub additional: BTreeMap<String, serde_json::Value>,
}

impl ProtocolError {
    /// Builds a stable protocol error with `retryable` set from
    /// [`StableErrorCode::retryable`], so a caller cannot supply an
    /// inconsistent value. `details` should follow the per-code shape
    /// documented in `docs/contracts/runner-v1/README.md` (e.g. `not_found`
    /// carries `{"resource": ...}`; `conflict`, `internal_error` and
    /// `unauthorized` carry `{}`).
    pub fn new(
        code: StableErrorCode,
        message: impl Into<String>,
        request_id: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            retryable: code.retryable(),
            code,
            message: message.into(),
            request_id: request_id.into(),
            details,
            additional: BTreeMap::new(),
        }
    }
}

/// Typed protocol errors. `stale_lease` remains a fixed machine-readable code
/// so a runner can safely abandon its obsolete fence without parsing prose.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExecutionError {
    #[error("unsupported protocol version {received}")]
    UnsupportedProtocol { received: u16 },
    #[error("invalid request: {field}")]
    InvalidRequest { field: String },
    #[error("{reason}: {from:?} -> {to:?}")]
    InvalidTransition {
        from: ExecutionState,
        to: ExecutionState,
        reason: &'static str,
    },
    #[error("stale lease for attempt {attempt_id}")]
    StaleLease {
        attempt_id: AttemptId,
        current_fencing_token: FencingToken,
    },
}

impl ExecutionError {
    pub const STALE_LEASE_CODE: &'static str = "stale_lease";

    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedProtocol { .. } => "unsupported_protocol",
            Self::InvalidRequest { .. } => "invalid_request",
            Self::InvalidTransition { .. } => "invalid_transition",
            Self::StaleLease { .. } => Self::STALE_LEASE_CODE,
        }
    }
}

impl From<LifecycleError> for ExecutionError {
    fn from(error: LifecycleError) -> Self {
        Self::InvalidTransition {
            from: error.from,
            to: error.to,
            reason: error.reason,
        }
    }
}

#[cfg(test)]
#[path = "types/tests.rs"]
mod tests;
