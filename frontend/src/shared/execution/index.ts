// Public surface of the shared execution API/state layer. The fleet/runner
// management UI, the "Run with agent" + activity surfaces, and anything
// added later should import from this barrel rather than reaching into
// individual files, so the boundary of what's public stays visible in one
// place.

export type {
  ExecutionState,
  RunnerSelector,
  CapabilitySupport,
  CapabilityValue,
  FeatureCapabilities,
  Concurrency,
  ModelCombination,
  HarnessCapability,
  CapabilityLimits,
  RunnerCapabilities,
  MeasurementSource,
  Measurement,
  Usage,
  StableErrorCode,
} from './types';
export { TERMINAL_EXECUTION_STATES, isTerminalExecutionState } from './types';

export type {
  ExecutionSummary,
  ExecutionListResult,
  CreateExecutionInput,
  CreateExecutionResult,
  CancelExecutionResult,
  RequeueExecutionInput,
  RequeueExecutionResult,
  FleetSummary,
  FleetListResult,
  CreateFleetInput,
  CreateFleetResult,
  AgentProfileSummary,
  AgentProfileListResult,
  CreateAgentProfileInput,
  CreateAgentProfileResult,
  ModelProfileSummary,
  ModelProfileListResult,
  CreateModelProfileInput,
  CreateModelProfileResult,
  EnrollRunnerInput,
  EnrollRunnerResult,
  RevokeRunnerResult,
  RevokeEnrollmentTokenResult,
  RunnerSummary,
  RunnerListResult,
} from './api';
export { executionsApi, fleetsApi, agentProfilesApi, modelProfilesApi, runnersApi } from './api';

export type {
  ModelProvenance,
  RunnerTimeCost,
  UsageEconomics,
  AttemptSummary,
  AttemptListResult,
  EventSummary,
  EventListResult,
} from './attempts';
export { attemptsApi } from './attempts';

export type {
  DecisionOption,
  DecisionState,
  DecisionAnswer,
  DecisionResolvedBy,
  DecisionRecord,
  ResolveDecisionResult,
} from './decisions';
export {
  decisionsApi,
  decisionTokenStore,
  isDecisionTokenRejected,
  isDecisionExpired,
  isDecisionIdempotencyConflict,
  isDecisionNotFound,
  isDecisionInvalidOption,
} from './decisions';

export type { ArtifactRecord } from './artifacts';
export { artifactsApi, isArtifactNotFound, isArtifactContentNotVerified } from './artifacts';

export type { CapabilityGate, FeatureName, AggregatedModelCombination, CombinationAvailability } from './capabilities';
export {
  gateFeature,
  gateFeatureAcrossRunners,
  listReportedHarnessKinds,
  harnessProbeStatus,
  listModelCombinationsForHarness,
  isCombinationSupported,
} from './capabilities';

export { VersionedCache, SequenceAllocator } from './cache';

export type { ExecutionInvalidationEvent, ExecutionRealtimeStatus, ExecutionRealtimeOptions, ExecutionRealtime } from './realtime';
export { createExecutionRealtime } from './realtime';

export type {
  NormalizedExecutionError,
  ListStatus,
  CancellationState,
  ExecutionRequestRecord,
  AttemptAvailability,
  ExecutionStore,
} from './store';
export { createExecutionStore } from './store';
