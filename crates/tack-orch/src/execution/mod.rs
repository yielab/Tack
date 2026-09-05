//! Harness-neutral execution protocol domain for runner protocol v1.
//!
//! This module deliberately has no transport, persistence, or vendor adapter
//! dependencies. It represents the immutable values exchanged by the runner
//! protocol and is the seam consumed by the API, database, and runner
//! layers.

mod capabilities;
mod lifecycle;
mod types;

// `CapabilityLimits` is exported here because it's the type of the public
// field `RunnerCapabilities::limits` — without it, no downstream crate can
// name the type to construct a capability report.
pub use capabilities::{
    CapabilityLimits, CapabilitySupport, CapabilityValue, Concurrency, EmbeddedCapabilitySnapshot,
    FeatureCapabilities, HarnessCapability, ModelCombination, ModelMetadata, RunnerCapabilities,
};
pub use lifecycle::{LifecycleError, TransitionActor, validate_transition};
pub use types::{
    ActualExecution, ActualModelId, ActualModelProvider, AgentProfileId, AgentProfileSnapshot,
    ArtifactId, AttemptId, AttemptSnapshot, CancellationRequestId, Checkpoint, ClaimRequestId,
    CompletionId, DecisionId, EnvironmentValue, EventId, ExecutionError, ExecutionRequestId,
    ExecutionRequestSnapshot, ExecutionState, FencingToken, HarnessKind, IdempotencyKey, ItemId,
    Measurement, MeasurementSource, ModelId, ModelProvider, PermissionPolicy, ProtocolError,
    ProtocolErrorEnvelope, ProtocolVersion, RecoveryDetails, RecoveryDisposition,
    RecoveryJournalState, RecoveryKey, RecoveryObservation, RecoveryObservationRequest,
    RecoveryObservationResponse, RepositorySnapshot, RequestedModelId, RequestedModelProvider,
    RunnerId, RunnerSelector, StableErrorCode, Usage, WorkspaceId,
};
