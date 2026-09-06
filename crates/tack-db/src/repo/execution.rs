//! Durable neutral execution queue.  This module intentionally uses opaque text
//! identifiers: runner-protocol ids are not UUIDs and callers must not parse
//! their example prefixes.  All runner-owned writes are fenced in SQL.

use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use tracing::instrument;

use super::Repository;

pub trait ExecutionClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Default)]
pub struct SystemExecutionClock;

impl ExecutionClock for SystemExecutionClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, Clone)]
pub struct NewRunner<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub credential_hash: &'a str,
    pub labels: &'a str,
    pub total_capacity: i64,
    pub available_capacity: i64,
    pub capability_snapshot: &'a str,
    pub protocol_version: i64,
}

#[derive(Debug, Clone)]
pub struct NewAgentProfile<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub instructions: &'a str,
    pub tool_policy: &'a str,
    pub limits: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewExecutionRequest<'a> {
    pub id: &'a str,
    pub item_id: &'a str,
    pub idempotency_scope: &'a str,
    pub idempotency_key: &'a str,
    /// A stable canonical representation of immutable request fields. Equal
    /// keys with different fingerprints are conflicts, never silent merges.
    pub request_fingerprint: &'a str,
    pub selector_kind: &'a str,
    pub selector_id: &'a str,
    pub agent_profile_id: Option<&'a str>,
    pub agent_profile_snapshot: &'a str,
    pub requested_harness_kind: Option<&'a str>,
    pub requested_model_provider: Option<&'a str>,
    pub requested_model_id: Option<&'a str>,
    pub repository_snapshot: &'a str,
    pub permission_policy: &'a str,
    pub timeout_seconds: Option<i64>,
    pub budgets: &'a str,
    pub status_map_policy_id: Option<&'a str>,
    pub environment: &'a str,
    pub metadata: &'a str,
    pub request_snapshot: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueResult {
    Created(String),
    Replayed(String),
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    pub attempt_id: String,
    pub request_id: String,
    pub attempt_number: i64,
    pub runner_id: String,
    pub fencing_token: i64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewEvent<'a> {
    pub id: &'a str,
    pub event_id: &'a str,
    pub sequence: i64,
    pub source: &'a str,
    pub kind: &'a str,
    pub payload: &'a str,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct EventBatch<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub previous_checkpoint: Option<&'a str>,
    pub checkpoint: &'a str,
}

#[derive(Debug, Clone)]
pub struct Completion<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub completion_id: &'a str,
    pub final_event_checkpoint: Option<&'a str>,
    pub terminal_state: &'a str,
    pub terminal_reason: &'a str,
    pub actual_execution: &'a str,
    pub usage: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewArtifact<'a> {
    pub id: &'a str,
    pub artifact_id: &'a str,
    pub kind: &'a str,
    pub name: &'a str,
    pub media_type: Option<&'a str>,
    pub size_bytes: i64,
    pub sha256: &'a str,
    pub content_disposition: Option<&'a str>,
    pub content_reference: Option<&'a str>,
    pub metadata: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewDecision<'a> {
    pub id: &'a str,
    pub decision_id: &'a str,
    pub kind: &'a str,
    pub prompt: &'a str,
    pub options: &'a str,
    pub metadata: &'a str,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct EnrollmentToken<'a> {
    pub id: &'a str,
    pub runner_id: &'a str,
    pub token_hash: &'a str,
    pub expires_at: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentTokenMetadata {
    pub id: String,
    pub runner_id: String,
    pub expires_at: String,
    pub consumed_at: Option<String>,
    pub revoked_at: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorRequeueResult {
    Requeued,
    Replayed,
    Conflict,
    InvalidTransition,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedeemEnrollmentResult {
    Redeemed(String),
    InvalidOrExpired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotatedCredential {
    pub runner_id: String,
    pub credential_expires_at: String,
    pub rotated_at: String,
}

/// Outcome of a compare-and-set credential rotation. `HashMismatch` covers
/// both "another rotation already won the race" and "the runner is no
/// longer active/is revoked" — either way the caller must not treat the
/// newly generated credential as live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialRotationResult {
    Rotated(RotatedCredential),
    HashMismatch,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedExecution {
    pub lease: Lease,
    pub request_snapshot: serde_json::Value,
}

/// How `claim_execution_idempotent_with_snapshot` picks *which* queued
/// request to attempt to claim, once its own capacity check has passed.
///
/// `tack-db` cannot depend on `tack-orch` (the dependency arrow points the
/// other way — see `crates/tack-orch/Cargo.toml`'s own header comment), so
/// the pure `tack_orch::scheduler` decision cannot be called from inside
/// this module or its transaction. Instead the caller (today, only
/// `tack_orch::scheduler::wiring::choose_request_for_runner`, invoked from
/// `crates/tack-api/src/handlers/runner_protocol.rs`'s `claim` handler)
/// resolves the decision *first*, against a read-only snapshot fetched via
/// [`Repository::fetch_runner_scheduling_snapshot`]/
/// [`Repository::list_eligible_queued_requests`], and hands the resulting
/// choice back in here so the actual fenced write stays exactly as strict
/// as it always was — pure
/// selection never grants the authoritative lease.
#[derive(Debug, Clone, Copy)]
pub enum RequestSelection<'a> {
    /// The pre-Wave-4 behavior: `ORDER BY created_at LIMIT 1` over every
    /// selector-eligible queued request, ignoring harness/model/label/
    /// heartbeat eligibility entirely. Kept only so every pre-existing test
    /// call site (none of which set up runner capability data) keeps its
    /// exact original behavior unmodified. The production runner-v1 claim
    /// handler never passes this variant.
    Naive,
    /// The caller already ran the pure scheduler against live candidate
    /// data and decided which single request, if any, this runner should
    /// attempt to claim. `Some(id)` names it — this method still re-checks
    /// that id is genuinely `queued` and selector-eligible for `runner_id`
    /// before leasing it (defense in depth against a stale decision).
    /// `None` means the scheduler considered every selector-eligible
    /// request and found none this runner is eligible for right now
    /// (including "there were no queued requests at all") — this call
    /// reports `no work` without falling back to naive selection, which
    /// would silently undo the scheduler's rejection.
    Scheduled(Option<&'a str>),
}

/// A read-only snapshot of one runner's scheduling-relevant state, used to
/// build a `tack_orch::scheduler::RunnerCandidate` one layer up (that type
/// cannot be constructed here — see [`RequestSelection`]'s doc comment for
/// why). Every field mirrors a live `agent_runners`/`agent_fleet_members`
/// column, not the runner's self-reported `capability_snapshot` alone,
/// because capacity/heartbeat are refreshed by every claim/heartbeat call
/// while the capability blob only changes on enroll/refresh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerSchedulingSnapshot {
    pub runner_id: String,
    /// `'pending_enrollment' | 'active' | 'revoked'` — see
    /// `crate::migrations`'s migration 040. Kept as the raw string; the
    /// caller owns mapping it to a typed enum (this crate must not depend
    /// on `tack-orch`'s `scheduler::RunnerState`).
    pub state: String,
    /// JSON object string (`agent_runners.labels`).
    pub labels: String,
    pub total_capacity: i64,
    pub available_capacity: i64,
    pub last_heartbeat_at: Option<String>,
    /// JSON string, validated at enroll/refresh time to parse as
    /// `tack_orch::execution::EmbeddedCapabilitySnapshot` — see
    /// `crates/tack-api/src/handlers/runner_protocol.rs`'s
    /// `validate_capability_payload`. A runner that has never enrolled
    /// carries the column default `'{}'`, which does *not* parse as that
    /// type; the caller treats a parse failure as "no declared harnesses"
    /// rather than an error: unknown is explicit, not a
    /// crash.
    pub capability_snapshot: String,
    /// Every `agent_fleets.id` this runner currently belongs to, via
    /// `agent_fleet_members`.
    pub fleet_ids: Vec<String>,
}

/// A read-only view of one queued `execution_requests` row, carrying only
/// the columns scheduling needs. Exists for the same layering reason as
/// [`RunnerSchedulingSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedRequestForScheduling {
    pub id: String,
    pub selector_kind: String,
    pub selector_id: String,
    /// Always `Some` for any request that passed `enqueue_execution`'s
    /// snapshot validation (the request snapshot's `requested_harness_kind`
    /// is a required field) — `Option` only because the raw column
    /// itself has no `NOT NULL` constraint (migration 044).
    pub requested_harness_kind: Option<String>,
    pub requested_model_provider: Option<String>,
    pub requested_model_id: Option<String>,
    pub created_at: String,
    /// JSON object string (`execution_requests.metadata`). The caller may
    /// read a `priority` key from this as a documented, best-effort
    /// convention — see `tack_orch::scheduler::wiring`'s module doc for why
    /// no dedicated column exists yet.
    pub metadata: String,
}

/// A fleet's configured concurrency ceiling alongside its current, observed
/// in-flight usage — `agent_fleets.concurrency_limit` (migration 039) has
/// never been enforced anywhere until this snapshot gave a caller something
/// to enforce it against. `in_use` is the sum of `total_capacity -
/// available_capacity` across every member runner: every unit of capacity a
/// member runner currently has reserved, regardless of which specific
/// request or selector claimed it — a fleet-wide ceiling is a statement
/// about the fleet's aggregate load, not only load arriving through the
/// `fleet` selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FleetConcurrencySnapshot {
    pub concurrency_limit: Option<i64>,
    pub in_use: i64,
}

/// One row of `GET /api/runners` — every column
/// `agent_runners` carries, minus `credential_hash`/`credential_expires_at`/
/// `credential_rotated_at` (never read back to an operator; enrollment
/// tokens/credentials are one-time-reveal by design), plus this runner's
/// current fleet roster from `agent_fleet_members`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerListingRow {
    pub id: String,
    pub name: String,
    pub state: String,
    pub labels: String,
    pub total_capacity: i64,
    pub available_capacity: i64,
    pub capability_snapshot: String,
    pub protocol_version: i64,
    pub runner_version: Option<String>,
    pub last_heartbeat_at: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub fleet_ids: Vec<String>,
}

/// One row of `GET /api/executions` (unscoped, one-item, or the
/// `item_ids` batch) or `GET /api/executions/{id}`'s detail — the five
/// scalar columns this surface exposes. Deliberately thinner than
/// `execution_requests`' full row (no selector, agent profile, requested
/// harness/model, repository/permission/budget/timeout/metadata columns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionListingRow {
    pub id: String,
    pub item_id: String,
    pub state: String,
    pub cancellation_requested_at: Option<String>,
    pub created_at: String,
}

/// One row of `GET /api/executions/{request_id}/attempts` — every column
/// `execution_attempts` carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptListingRow {
    pub id: String,
    pub request_id: String,
    pub attempt_number: i64,
    pub runner_id: String,
    pub fencing_token: i64,
    pub state: String,
    pub lease_issued_at: String,
    pub lease_expires_at: String,
    pub last_heartbeat_at: Option<String>,
    pub event_checkpoint: Option<String>,
    pub completion_id: Option<String>,
    pub workspace_id: Option<String>,
    pub base_revision: Option<String>,
    pub actual_execution: Option<String>,
    pub terminal_reason: Option<String>,
    pub usage: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One row of `GET /api/executions/{request_id}/attempts/{attempt_number}/events`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventListingRow {
    pub event_id: String,
    pub sequence: i64,
    pub source: String,
    pub kind: String,
    pub payload: String,
    pub occurred_at: String,
    pub created_at: String,
}

/// Every column `execution_artifacts` carries for one manifest row.
/// `content_reference` is `None` until [`Repository::set_execution_artifact_content_reference`]
/// commits a verified upload — its presence, not a separate status column
/// (none exists: that would just duplicate this field), is
/// the manifest-vs-content-verified distinction this crate exposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionArtifactRow {
    pub id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub kind: String,
    pub name: String,
    pub media_type: Option<String>,
    pub size_bytes: i64,
    pub sha256: String,
    pub content_disposition: Option<String>,
    pub content_reference: Option<String>,
    pub metadata: String,
    pub created_at: String,
}

/// Every column `execution_decisions` carries for one row. `options`,
/// `metadata`, `answer` and `resolved_by` stay raw JSON-string/`Option`
/// columns here, matching `ExecutionArtifactRow.metadata`'s convention —
/// the caller (an API handler) owns parsing them into a typed response,
/// this crate does not depend on the wire schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionDecisionRow {
    pub id: String,
    pub attempt_id: String,
    pub decision_id: String,
    pub kind: String,
    pub state: String,
    pub prompt: String,
    pub options: String,
    pub metadata: String,
    pub answer: Option<String>,
    pub expires_at: Option<String>,
    pub resolved_at: Option<String>,
    pub resolved_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Outcome of [`Repository::set_execution_artifact_content_reference`].
/// `AlreadySet` is distinct from `Committed` (rule 7: no structural
/// stand-in) — a second attempt to record content for the same
/// `(attempt_id, artifact_id)` is reported honestly rather than silently
/// treated as success or as the same thing as a fencing failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactContentCommitResult {
    Committed,
    AlreadySet,
    Stale,
}

// ─── Runtime retention and observability ───────────────────────────────────

/// Outcome of one bounded batch-purge call (which may run several bounded
/// transactions internally — see the doc comments on
/// [`Repository::purge_stale_execution_replays`] /
/// [`Repository::purge_stale_terminal_execution_events`]). Field names
/// mirror `tack_db::repo::orch::RollupStats`/`tack_orch::reconciler::RollupOutcome`
/// on purpose — same shape, same reason (a `tack-orch` trait impl backed by
/// this repo is a direct pass-through) — but this type is named `PurgeStats`,
/// not `RollupStats`: nothing here aggregates rows into a daily
/// table before deleting them (see each method's own doc comment for why).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PurgeStats {
    /// Total raw rows deleted.
    pub rows_purged: i64,
    /// Number of batch transactions it took (observability/tuning only).
    pub batches_run: i64,
}

/// A fleet-wide, id-free snapshot of execution runtime health, computed
/// fresh on every call from live tables (nothing cached). Every count is a
/// fixed-cardinality aggregate — keyed only by the small, closed
/// vocabularies of `agent_runners.state` (3 values, migration 040) and
/// `execution_requests.state` (the 10-value `ExecutionState` vocabulary)
/// — never by attempt/request/runner id, so nothing here can grow
/// an unbounded metric label set. See `crates/tack-orch/src/execution_observability.rs`
/// for the background task that logs alerts from this snapshot and the pure
/// `evaluate_alerts` function that decides what counts as "stuck"/"ambiguous."
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionFleetSnapshotRow {
    /// `agent_runners.state` -> count.
    pub runner_state_counts: std::collections::BTreeMap<String, i64>,
    /// `execution_requests.state` -> count.
    pub request_state_counts: std::collections::BTreeMap<String, i64>,
    /// Attempts whose lease has expired (`lease_expires_at < now`) but whose
    /// state is still one of `leased`/`preparing`/`running`/`waiting_decision`
    /// — a lease the recovery service has not yet resolved to `lost` or
    /// `needs_operator`. This is what makes a stale lease *observable*
    /// independent of whatever (if anything) later resolves it.
    pub stale_lease_count: i64,
    /// Age in seconds of the single oldest stale lease found above (by
    /// `lease_expires_at`) — `None` iff `stale_lease_count == 0`, never `0`
    /// standing in for "none found."
    pub oldest_stale_lease_age_secs: Option<i64>,
    /// `execution_requests` currently in `needs_operator` — an ambiguous
    /// state that is never automatically retried.
    pub needs_operator_count: i64,
    /// Age in seconds since the oldest `needs_operator` request's `updated_at`
    /// (i.e. since it became ambiguous) — `None` iff `needs_operator_count == 0`.
    pub oldest_needs_operator_age_secs: Option<i64>,
    /// Count of `execution_events` rows with `occurred_at` inside the
    /// caller-chosen trailing window ending at `now`. A coarse ingestion-rate
    /// signal — a single bounded total, never broken down by attempt/event id.
    pub events_ingested_in_window: i64,
}

fn parse_rfc3339_checked(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptTransitionPhase {
    Preparing,
    Running,
}

#[derive(Debug, Clone)]
pub struct AttemptTransitionInput<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub phase: AttemptTransitionPhase,
    pub workspace_id: &'a str,
    pub base_revision: &'a str,
    pub process_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptTransitionResponse {
    pub attempt_id: String,
    pub state: String,
    pub committed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttemptTransitionResult {
    Applied(AttemptTransitionResponse),
    Replayed(AttemptTransitionResponse),
    Conflict,
    Stale,
}

#[derive(Debug, Clone)]
pub struct HeartbeatLease<'a> {
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub state: &'a str,
    pub journal_state: &'a str,
    pub last_event_checkpoint: Option<&'a str>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatLeaseResponse {
    pub attempt_id: String,
    pub fencing_token: i64,
    pub state: String,
    pub journal_state: String,
    pub last_event_checkpoint: Option<String>,
    pub lease_expires_at: DateTime<Utc>,
    pub cancellation_requested: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatBatchResponse {
    pub heartbeat_id: String,
    pub accepted_at: DateTime<Utc>,
    pub leases: Vec<HeartbeatLeaseResponse>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeartbeatBatchResult {
    Accepted(HeartbeatBatchResponse),
    Replayed(HeartbeatBatchResponse),
    Conflict,
    StaleLease(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventBatchResult {
    pub accepted_event_ids: Vec<String>,
    pub duplicate_event_ids: Vec<String>,
    pub committed_checkpoint: String,
    pub replayed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventApplyResult {
    Applied(EventBatchResult),
    /// The stored replay fingerprint for this `(attempt_id, checkpoint)` key
    /// differs from the new request's fingerprint: the same idempotency-scoped
    /// key was reused with different content. This can never succeed by
    /// retrying with the same key — the wire mapping is the non-retryable
    /// `idempotency_conflict` stable error, never `conflict`.
    IdempotencyConflict,
    /// No replay row matched this checkpoint, but the attempt's event stream
    /// position was not where this batch expected it (checkpoint already
    /// advanced past it, `previous_checkpoint` mismatch) or a concurrent
    /// writer won the compare-and-set. A benign out-of-order resync — the
    /// wire mapping is the retryable `conflict` stable error.
    Conflict,
    Stale,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryClassification {
    SafePreSpawnRequeue,
    NeedsOperator,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryObservation {
    ProcessStopped,
    ProcessRunning,
    Ambiguous,
}
#[derive(Debug, Clone)]
pub struct RecoveryObservationInput<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub recovery_key: &'a str,
    pub observation: RecoveryObservation,
    pub details: &'a str,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryDisposition {
    SafePreSpawnRequeue,
    NeedsOperator,
    AlreadyTerminal,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryResponse {
    pub attempt_id: String,
    pub recovery_key: String,
    pub disposition: RecoveryDisposition,
    pub committed_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryObservationResult {
    Applied(RecoveryResponse),
    Replayed(RecoveryResponse),
    Conflict,
    Stale,
}
#[derive(Debug, Clone)]
pub struct CancellationObservationInput<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub cancellation_request_id: &'a str,
    pub observed_at: DateTime<Utc>,
    pub details: &'a str,
    pub observation: &'a str,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancellationResponse {
    pub attempt_id: String,
    pub cancellation_request_id: String,
    pub state: String,
    pub committed_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancellationObservation {
    Cancelled(CancellationResponse),
    Replayed(CancellationResponse),
    Conflict,
    Stale,
    AlreadyTerminal { state: String },
    Ambiguous { state: String },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionResponse {
    pub completion_id: String,
    pub terminal_state: String,
    pub final_event_checkpoint: Option<String>,
    pub committed_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionResult {
    Committed(CompletionResponse),
    Replayed(CompletionResponse),
    /// The stored replay fingerprint for this `(attempt_id, completion_id)`
    /// key differs from the new request's fingerprint: the same
    /// idempotency-scoped key was reused with different content. This can
    /// never succeed by retrying with the same key — the wire mapping is the
    /// non-retryable `idempotency_conflict` stable error, never `conflict`.
    IdempotencyConflict,
    /// No matching replay row: either the attempt is already terminal under
    /// a different completion (no authoritative historical response to
    /// replay, e.g. a pre-M055 terminal attempt) or a concurrent writer won
    /// the compare-and-set. The wire mapping is the retryable `conflict`
    /// stable error.
    Conflict,
    Stale,
}

fn stamp(clock: &dyn ExecutionClock) -> String {
    clock.now().to_rfc3339()
}

fn terminal(state: &str) -> bool {
    matches!(state, "succeeded" | "failed" | "cancelled")
}

fn event_batch_fingerprint(
    batch: &EventBatch<'_>,
    events: &[NewEvent<'_>],
) -> Result<String, sqlx::Error> {
    let values: Result<Vec<serde_json::Value>, sqlx::Error> = events
        .iter()
        .map(|event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload)
                .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
            Ok(serde_json::json!({
                "event_id": event.event_id,
                "sequence": event.sequence,
                "source": event.source,
                "kind": event.kind,
                "payload": canonical_json(payload),
                "occurred_at": event.occurred_at.to_rfc3339(),
            }))
        })
        .collect();
    serde_json::to_string(&serde_json::json!({
        "runner_id": batch.runner_id,
        "fencing_token": batch.fencing_token,
        "previous_checkpoint": batch.previous_checkpoint,
        "events": values?,
    }))
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))
}

fn canonical_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<_> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            serde_json::Value::Object(
                entries
                    .into_iter()
                    .map(|(k, v)| (k, canonical_json(v)))
                    .collect(),
            )
        }
        serde_json::Value::Array(v) => {
            serde_json::Value::Array(v.into_iter().map(canonical_json).collect())
        }
        v => v,
    }
}

fn canonical_json_or_string(value: &str) -> serde_json::Value {
    match serde_json::from_str(value) {
        Ok(value) => canonical_json(value),
        Err(_) => serde_json::Value::String(value.into()),
    }
}

fn completion_fingerprint(completion: &Completion<'_>) -> Result<String, sqlx::Error> {
    let actual_execution: serde_json::Value = serde_json::from_str(completion.actual_execution)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let usage: serde_json::Value = serde_json::from_str(completion.usage)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    serde_json::to_string(&serde_json::json!({
        "runner_id": completion.runner_id,
        "attempt_id": completion.attempt_id,
        "fencing_token": completion.fencing_token,
        "completion_id": completion.completion_id,
        "final_event_checkpoint": completion.final_event_checkpoint,
        "terminal_state": completion.terminal_state,
        "terminal_reason": canonical_json_or_string(completion.terminal_reason),
        "actual_execution": canonical_json(actual_execution),
        "usage": canonical_json(usage),
    }))
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))
}

fn replay_response(serialized: &str) -> Result<CompletionResponse, sqlx::Error> {
    let value: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| sqlx::Error::Protocol("invalid completion replay response".into()))?;
    let required = |field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                sqlx::Error::Protocol(format!("invalid completion replay response field: {field}"))
            })
    };
    let final_event_checkpoint = match object.get("final_event_checkpoint") {
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) => Some(value.clone()),
        _ => {
            return Err(sqlx::Error::Protocol(
                "invalid completion replay response field: final_event_checkpoint".into(),
            ));
        }
    };
    Ok(CompletionResponse {
        completion_id: required("completion_id")?,
        terminal_state: required("terminal_state")?,
        final_event_checkpoint,
        committed_at: required("committed_at")?,
    })
}

fn heartbeat_fingerprint(
    runner_id: &str,
    heartbeat_id: &str,
    sent_at: DateTime<Utc>,
    available_capacity: i64,
    leases: &[HeartbeatLease<'_>],
    lease_duration: Duration,
) -> Result<String, sqlx::Error> {
    let lease_duration_nanoseconds = lease_duration
        .num_nanoseconds()
        .ok_or_else(|| sqlx::Error::Protocol("heartbeat lease duration is out of range".into()))?;
    let mut leases = leases
        .iter()
        .map(|lease| {
            (
                lease.attempt_id,
                lease.fencing_token,
                lease.state,
                lease.journal_state,
                lease.last_event_checkpoint,
            )
        })
        .collect::<Vec<_>>();
    leases.sort_unstable();
    serde_json::to_string(&serde_json::json!({
        "runner_id": runner_id,
        "heartbeat_id": heartbeat_id,
        "sent_at": sent_at.to_rfc3339(),
        "available_capacity": available_capacity,
        "lease_duration_nanoseconds": lease_duration_nanoseconds,
        "leases": leases
            .into_iter()
            .map(|(attempt_id, fencing_token, state, journal_state, last_event_checkpoint)| serde_json::json!({
                "attempt_id": attempt_id,
                "fencing_token": fencing_token,
                "state": state,
                "journal_state": journal_state,
                "last_event_checkpoint": last_event_checkpoint,
            }))
            .collect::<Vec<_>>(),
    }))
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))
}

fn heartbeat_response(serialized: &str) -> Result<HeartbeatBatchResponse, sqlx::Error> {
    let value: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| sqlx::Error::Protocol("invalid heartbeat replay response".into()))?;
    let required_string = |object: &serde_json::Map<String, serde_json::Value>, field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                sqlx::Error::Protocol(format!("invalid heartbeat replay response field: {field}"))
            })
    };
    let accepted_at = DateTime::parse_from_rfc3339(&required_string(object, "accepted_at")?)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let leases = object
        .get("leases")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            sqlx::Error::Protocol("invalid heartbeat replay response field: leases".into())
        })?
        .iter()
        .map(|value| {
            let value = value.as_object().ok_or_else(|| {
                sqlx::Error::Protocol("invalid heartbeat replay response lease".into())
            })?;
            let lease_expires_at =
                DateTime::parse_from_rfc3339(&required_string(value, "lease_expires_at")?)
                    .map(|value| value.with_timezone(&Utc))
                    .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
            Ok(HeartbeatLeaseResponse {
                attempt_id: required_string(value, "attempt_id")?,
                fencing_token: value
                    .get("fencing_token")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or_else(|| {
                        sqlx::Error::Protocol(
                            "invalid heartbeat replay response field: fencing_token".into(),
                        )
                    })?,
                state: required_string(value, "state")?,
                journal_state: required_string(value, "journal_state")?,
                last_event_checkpoint: match value.get("last_event_checkpoint") {
                    Some(serde_json::Value::Null) => None,
                    Some(serde_json::Value::String(value)) => Some(value.clone()),
                    _ => {
                        return Err(sqlx::Error::Protocol(
                            "invalid heartbeat replay response field: last_event_checkpoint".into(),
                        ));
                    }
                },
                lease_expires_at,
                cancellation_requested: value
                    .get("cancellation_requested")
                    .and_then(serde_json::Value::as_bool)
                    .ok_or_else(|| {
                        sqlx::Error::Protocol(
                            "invalid heartbeat replay response field: cancellation_requested"
                                .into(),
                        )
                    })?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(HeartbeatBatchResponse {
        heartbeat_id: required_string(object, "heartbeat_id")?,
        accepted_at,
        leases,
    })
}

fn cancellation_fingerprint(
    input: &CancellationObservationInput<'_>,
) -> Result<String, sqlx::Error> {
    let details: serde_json::Value = serde_json::from_str(input.details)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let observation: serde_json::Value = serde_json::from_str(input.observation)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    if observation != serde_json::Value::String("process_stopped".into()) {
        return Err(sqlx::Error::Protocol(
            "cancellation observation must be JSON string process_stopped".into(),
        ));
    }
    serde_json::to_string(&serde_json::json!({
        "runner_id": input.runner_id,
        "attempt_id": input.attempt_id,
        "fencing_token": input.fencing_token,
        "cancellation_request_id": input.cancellation_request_id,
        "observed_at": input.observed_at.to_rfc3339(),
        "details": canonical_json(details),
        "observation": canonical_json(observation),
    }))
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))
}

fn cancellation_response(serialized: &str) -> Result<CancellationResponse, sqlx::Error> {
    let value: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| sqlx::Error::Protocol("invalid cancellation replay response".into()))?;
    let required = |field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                sqlx::Error::Protocol(format!(
                    "invalid cancellation replay response field: {field}"
                ))
            })
    };
    Ok(CancellationResponse {
        attempt_id: required("attempt_id")?,
        cancellation_request_id: required("cancellation_request_id")?,
        state: required("state")?,
        committed_at: required("committed_at")?,
    })
}

struct RecoveryDetails {
    journal_state: String,
    process_observed: bool,
}

fn recovery_details(details: &str) -> Result<RecoveryDetails, sqlx::Error> {
    let value: serde_json::Value =
        serde_json::from_str(details).map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| sqlx::Error::Protocol("invalid recovery details".into()))?;
    let journal_state = object
        .get("journal_state")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| sqlx::Error::Protocol("invalid recovery details journal_state".into()))?;
    let process_observed = object
        .get("process_observed")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| sqlx::Error::Protocol("invalid recovery details process_observed".into()))?;
    Ok(RecoveryDetails {
        journal_state,
        process_observed,
    })
}

fn recovery_observation_name(observation: RecoveryObservation) -> &'static str {
    match observation {
        RecoveryObservation::ProcessStopped => "process_stopped",
        RecoveryObservation::ProcessRunning => "process_running",
        RecoveryObservation::Ambiguous => "ambiguous",
    }
}

fn recovery_fingerprint(input: &RecoveryObservationInput<'_>) -> Result<String, sqlx::Error> {
    let details: serde_json::Value = serde_json::from_str(input.details)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    serde_json::to_string(&serde_json::json!({
        "runner_id": input.runner_id,
        "attempt_id": input.attempt_id,
        "fencing_token": input.fencing_token,
        "recovery_key": input.recovery_key,
        "observation": recovery_observation_name(input.observation),
        "details": canonical_json(details),
    }))
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))
}

fn recovery_response(serialized: &str) -> Result<RecoveryResponse, sqlx::Error> {
    let value: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| sqlx::Error::Protocol("invalid recovery replay response".into()))?;
    let required = |field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                sqlx::Error::Protocol(format!("invalid recovery replay response field: {field}"))
            })
    };
    let disposition = match required("disposition")?.as_str() {
        "safe_pre_spawn_requeue" => RecoveryDisposition::SafePreSpawnRequeue,
        "needs_operator" => RecoveryDisposition::NeedsOperator,
        "already_terminal" => RecoveryDisposition::AlreadyTerminal,
        _ => {
            return Err(sqlx::Error::Protocol(
                "invalid recovery replay response disposition".into(),
            ));
        }
    };
    Ok(RecoveryResponse {
        attempt_id: required("attempt_id")?,
        recovery_key: required("recovery_key")?,
        disposition,
        committed_at: required("committed_at")?,
    })
}

fn snapshot_error(message: impl Into<String>) -> sqlx::Error {
    sqlx::Error::Protocol(format!(
        "invalid execution request snapshot: {}",
        message.into()
    ))
}

fn snapshot_object<'a>(
    value: &'a serde_json::Value,
    field: &str,
) -> Result<&'a serde_json::Map<String, serde_json::Value>, sqlx::Error> {
    value
        .as_object()
        .ok_or_else(|| snapshot_error(format!("{field} must be an object")))
}

fn snapshot_field<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<&'a serde_json::Value, sqlx::Error> {
    object
        .get(field)
        .ok_or_else(|| snapshot_error(format!("missing {field}")))
}

fn snapshot_string(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<String, sqlx::Error> {
    snapshot_field(object, field)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| snapshot_error(format!("{field} must be a string")))
}

fn snapshot_nullable_string(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Option<String>, sqlx::Error> {
    match snapshot_field(object, field)? {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(value) => Ok(Some(value.clone())),
        _ => Err(snapshot_error(format!("{field} must be string or null"))),
    }
}

fn parse_execution_request_snapshot(serialized: &str) -> Result<serde_json::Value, sqlx::Error> {
    let value: serde_json::Value =
        serde_json::from_str(serialized).map_err(|error| snapshot_error(error.to_string()))?;
    let root = snapshot_object(&value, "snapshot")?;
    for field in [
        "request_id",
        "item_id",
        "idempotency_key",
        "agent_profile_id",
        "requested_harness_kind",
    ] {
        snapshot_string(root, field)?;
    }
    let created_by = snapshot_object(snapshot_field(root, "created_by")?, "created_by")?;
    snapshot_string(created_by, "source")?;
    snapshot_string(created_by, "subject_id")?;
    DateTime::parse_from_rfc3339(&snapshot_string(root, "created_at")?)
        .map_err(|error| snapshot_error(format!("created_at must be RFC3339: {error}")))?;
    let selector = snapshot_object(snapshot_field(root, "selector")?, "selector")?;
    match snapshot_string(selector, "kind")?.as_str() {
        "exact_runner" => {
            snapshot_string(selector, "runner_id")?;
        }
        "fleet" => {
            snapshot_string(selector, "fleet_id")?;
        }
        "any" => {}
        _ => return Err(snapshot_error("selector.kind is unsupported")),
    }
    let profile = snapshot_object(
        snapshot_field(root, "resolved_agent_profile")?,
        "resolved_agent_profile",
    )?;
    snapshot_string(profile, "name")?;
    snapshot_string(profile, "instructions")?;
    snapshot_object(
        snapshot_field(profile, "tool_policy")?,
        "resolved_agent_profile.tool_policy",
    )?;
    snapshot_object(
        snapshot_field(profile, "budgets")?,
        "resolved_agent_profile.budgets",
    )?;
    snapshot_field(profile, "timeout_seconds")?
        .as_u64()
        .ok_or_else(|| snapshot_error("resolved_agent_profile.timeout_seconds must be u64"))?;
    snapshot_nullable_string(root, "requested_model_provider")?;
    snapshot_nullable_string(root, "requested_model_id")?;
    let repository = snapshot_object(snapshot_field(root, "repository")?, "repository")?;
    for field in ["kind", "remote", "base_revision"] {
        snapshot_string(repository, field)?;
    }
    snapshot_nullable_string(repository, "subdirectory")?;
    let policy = snapshot_object(
        snapshot_field(root, "permission_policy")?,
        "permission_policy",
    )?;
    let tools = snapshot_field(policy, "tools")?
        .as_array()
        .ok_or_else(|| snapshot_error("permission_policy.tools must be an array"))?;
    if tools.iter().any(|tool| tool.as_str().is_none()) {
        return Err(snapshot_error(
            "permission_policy.tools entries must be strings",
        ));
    }
    snapshot_field(policy, "network")?
        .as_bool()
        .ok_or_else(|| snapshot_error("permission_policy.network must be bool"))?;
    snapshot_field(root, "timeout_seconds")?
        .as_u64()
        .ok_or_else(|| snapshot_error("timeout_seconds must be u64"))?;
    snapshot_object(snapshot_field(root, "budgets")?, "budgets")?;
    snapshot_nullable_string(root, "status_map_policy_id")?;
    let environment = snapshot_object(snapshot_field(root, "environment")?, "environment")?;
    for value in environment.values() {
        let environment_value = snapshot_object(value, "environment value")?;
        let value = snapshot_nullable_string(environment_value, "value")?;
        let secret_reference = snapshot_nullable_string(environment_value, "secret_reference")?;
        if value.is_some() == secret_reference.is_some() {
            return Err(snapshot_error(
                "environment entry must contain exactly one value or secret_reference",
            ));
        }
    }
    snapshot_object(snapshot_field(root, "metadata")?, "metadata")?;
    Ok(value)
}

fn validate_execution_request_snapshot(
    input: &NewExecutionRequest<'_>,
) -> Result<String, sqlx::Error> {
    let snapshot = parse_execution_request_snapshot(input.request_snapshot)?;
    let root = snapshot_object(&snapshot, "snapshot")?;
    let matches = |field: &str, expected: &str| -> Result<(), sqlx::Error> {
        if snapshot_string(root, field)? == expected {
            Ok(())
        } else {
            Err(snapshot_error(format!(
                "{field} contradicts normalized request"
            )))
        }
    };
    matches("request_id", input.id)?;
    matches("item_id", input.item_id)?;
    matches("idempotency_key", input.idempotency_key)?;
    let selector = snapshot_object(snapshot_field(root, "selector")?, "selector")?;
    if snapshot_string(selector, "kind")? != input.selector_kind {
        return Err(snapshot_error("selector contradicts normalized request"));
    }
    let selector_matches = match input.selector_kind {
        "exact_runner" => snapshot_string(selector, "runner_id")? == input.selector_id,
        "fleet" => snapshot_string(selector, "fleet_id")? == input.selector_id,
        "any" => input.selector_id.is_empty(),
        _ => false,
    };
    if !selector_matches {
        return Err(snapshot_error("selector contradicts normalized request"));
    }
    if input.agent_profile_id != Some(snapshot_string(root, "agent_profile_id")?.as_str())
        || input.requested_harness_kind
            != Some(snapshot_string(root, "requested_harness_kind")?.as_str())
        || input.requested_model_provider.map(str::to_owned)
            != snapshot_nullable_string(root, "requested_model_provider")?
        || input.requested_model_id.map(str::to_owned)
            != snapshot_nullable_string(root, "requested_model_id")?
        || input.status_map_policy_id.map(str::to_owned)
            != snapshot_nullable_string(root, "status_map_policy_id")?
    {
        return Err(snapshot_error(
            "requested fields contradict normalized request",
        ));
    }
    let timeout = input
        .timeout_seconds
        .filter(|value| *value >= 0)
        .map(|value| value as u64);
    if timeout != snapshot_field(root, "timeout_seconds")?.as_u64() {
        return Err(snapshot_error(
            "timeout_seconds contradicts normalized request",
        ));
    }
    for (snapshot_field_name, normalized) in [
        ("resolved_agent_profile", input.agent_profile_snapshot),
        ("repository", input.repository_snapshot),
        ("permission_policy", input.permission_policy),
        ("budgets", input.budgets),
        ("environment", input.environment),
        ("metadata", input.metadata),
    ] {
        let normalized: serde_json::Value =
            serde_json::from_str(normalized).map_err(|error| snapshot_error(error.to_string()))?;
        if snapshot_field(root, snapshot_field_name)? != &normalized {
            return Err(snapshot_error(format!(
                "{snapshot_field_name} contradicts normalized request"
            )));
        }
    }
    serde_json::to_string(&canonical_json(snapshot))
        .map_err(|error| snapshot_error(error.to_string()))
}

fn snapshot_created_at_matches_now(snapshot: &str, now: &str) -> Result<(), sqlx::Error> {
    let snapshot = parse_execution_request_snapshot(snapshot)?;
    let root = snapshot_object(&snapshot, "snapshot")?;
    let snapshot_created_at =
        DateTime::parse_from_rfc3339(&snapshot_string(root, "created_at")?)
            .map_err(|error| snapshot_error(format!("created_at must be RFC3339: {error}")))?;
    let normalized_created_at = DateTime::parse_from_rfc3339(now).map_err(|error| {
        snapshot_error(format!("normalized created_at must be RFC3339: {error}"))
    })?;
    if snapshot_created_at != normalized_created_at {
        return Err(snapshot_error("created_at contradicts normalized request"));
    }
    Ok(())
}

fn lease_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Lease, sqlx::Error> {
    let issued: String = row.get("lease_issued_at");
    let expires: String = row.get("lease_expires_at");
    let parse = |value: String| {
        DateTime::parse_from_rfc3339(&value)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))
    };
    Ok(Lease {
        attempt_id: row.get("id"),
        request_id: row.get("request_id"),
        attempt_number: row.get("attempt_number"),
        runner_id: row.get("runner_id"),
        fencing_token: row.get("fencing_token"),
        issued_at: parse(issued)?,
        expires_at: parse(expires)?,
    })
}

fn snapshot(row: &sqlx::sqlite::SqliteRow) -> Result<serde_json::Value, sqlx::Error> {
    parse_execution_request_snapshot(&row.get::<String, _>("request_snapshot"))
}

/// Outcome of [`Repository::add_fleet_member`] — distinguishes an idempotent
/// no-op (`AlreadyMember`) from either side of the pair not existing, so the
/// handler can report a precise 404 instead of collapsing every failure into
/// one generic error. `agent_fleet_members` (migration 041) has
/// existed as a scheduling *read* input
/// (`fetch_runner_scheduling_snapshot`, `fetch_fleet_concurrency`, the
/// claimable-request query below) but had no write path — an operator could
/// never actually populate a fleet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddFleetMemberOutcome {
    Added,
    AlreadyMember,
    FleetNotFound,
    RunnerNotFound,
}

impl Repository {
    pub async fn complete_execution_result(
        &self,
        completion: Completion<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<CompletionResult, sqlx::Error> {
        if !terminal(completion.terminal_state) {
            return Ok(CompletionResult::Conflict);
        }
        let now = stamp(clock);
        let fingerprint = completion_fingerprint(&completion)?;
        // Serialize duplicate completion reports (e.g. a runner retrying an
        // unacknowledged terminal POST) before either transaction reads this
        // attempt row. Two deferred readers can otherwise deadlock while
        // both try to upgrade to writers. Mirrors the redeem_enrollment_token
        // and claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query(
            "SELECT request_id,runner_id,state,lease_expires_at,event_checkpoint FROM execution_attempts WHERE id=? AND runner_id=? AND fencing_token=?",
        )
        .bind(completion.attempt_id)
        .bind(completion.runner_id)
        .bind(completion.fencing_token)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(CompletionResult::Stale);
        };
        if let Some(row) = sqlx::query(
            "SELECT fingerprint,response FROM execution_completion_replays WHERE attempt_id=? AND completion_id=?",
        )
        .bind(completion.attempt_id)
        .bind(completion.completion_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_fingerprint: String = row.get("fingerprint");
            if stored_fingerprint != fingerprint {
                // Same (attempt_id, completion_id) idempotency-scoped key,
                // different content: this can never succeed by retrying.
                tx.commit().await?;
                return Ok(CompletionResult::IdempotencyConflict);
            }
            let response: String = row.get("response");
            let response = replay_response(&response)?;
            tx.commit().await?;
            return Ok(CompletionResult::Replayed(response));
        }
        let state: String = row.get("state");
        let expires: String = row.get("lease_expires_at");
        if terminal(&state) {
            // Pre-M055 terminal attempts have no authoritative response to
            // replay. This is a distinct completion_id from whichever one
            // terminated the attempt, not a reused idempotency key with
            // different content, so it is the benign/retryable `Conflict`.
            tx.commit().await?;
            return Ok(CompletionResult::Conflict);
        }
        if expires <= now {
            tx.commit().await?;
            return Ok(CompletionResult::Stale);
        }
        let request: String = row.get("request_id");
        let runner: String = row.get("runner_id");
        let updated = sqlx::query("UPDATE execution_attempts SET state=?,completion_id=?,terminal_reason=?,actual_execution=?,usage=?,ended_at=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND event_checkpoint IS ? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at > ?")
            .bind(completion.terminal_state).bind(completion.completion_id).bind(completion.terminal_reason).bind(completion.actual_execution).bind(completion.usage).bind(&now).bind(&now).bind(completion.attempt_id).bind(completion.runner_id).bind(completion.fencing_token).bind(completion.final_event_checkpoint).bind(&now).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            // Defensive: near-unreachable under the BEGIN IMMEDIATE this
            // function already opens, but a lost compare-and-set is a
            // benign/retryable `Conflict`, not an idempotency-key reuse.
            tx.rollback().await?;
            return Ok(CompletionResult::Conflict);
        }
        sqlx::query("UPDATE execution_requests SET state=?,updated_at=? WHERE id=?")
            .bind(completion.terminal_state)
            .bind(&now)
            .bind(request)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE agent_runners SET available_capacity=MIN(total_capacity,available_capacity+1),updated_at=? WHERE id=?").bind(&now).bind(runner).execute(&mut *tx).await?;
        let response = CompletionResponse {
            completion_id: completion.completion_id.into(),
            terminal_state: completion.terminal_state.into(),
            final_event_checkpoint: completion.final_event_checkpoint.map(str::to_owned),
            committed_at: now.clone(),
        };
        let serialized_response = serde_json::json!({
            "completion_id": response.completion_id,
            "terminal_state": response.terminal_state,
            "final_event_checkpoint": response.final_event_checkpoint,
            "committed_at": response.committed_at,
        })
        .to_string();
        sqlx::query("INSERT INTO execution_completion_replays(attempt_id,completion_id,fingerprint,response,committed_at) VALUES(?,?,?,?,?)").bind(completion.attempt_id).bind(completion.completion_id).bind(fingerprint).bind(serialized_response).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(CompletionResult::Committed(response))
    }
    pub async fn create_pending_runner_and_issue_token(
        &self,
        runner: NewRunner<'_>,
        token: EnrollmentToken<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<(), sqlx::Error> {
        if token.runner_id != runner.id
            || token.expires_at <= clock.now()
            || runner.total_capacity < 0
            || runner.available_capacity < 0
            || runner.available_capacity > runner.total_capacity
        {
            return Err(sqlx::Error::Protocol(
                "invalid pending runner/token input".into(),
            ));
        }
        let now = stamp(clock);
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO agent_runners(id,name,credential_hash,state,labels,total_capacity,available_capacity,capability_snapshot,protocol_version,created_at,updated_at) VALUES(?,?,?,'pending_enrollment',?,?,?,?,?,?,?)").bind(runner.id).bind(runner.name).bind("pending:no-credential").bind(runner.labels).bind(runner.total_capacity).bind(runner.available_capacity).bind(runner.capability_snapshot).bind(runner.protocol_version).bind(&now).bind(&now).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO agent_enrollment_tokens(id,runner_id,token_hash,expires_at,created_at) VALUES(?,?,?,?,?)").bind(token.id).bind(token.runner_id).bind(token.token_hash).bind(token.expires_at.to_rfc3339()).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn enrollment_token_metadata(
        &self,
        runner_id: &str,
        token_id: &str,
    ) -> Result<Option<EnrollmentTokenMetadata>, sqlx::Error> {
        sqlx::query("SELECT id,runner_id,expires_at,consumed_at,revoked_at FROM agent_enrollment_tokens WHERE runner_id=? AND id=?").bind(runner_id).bind(token_id).fetch_optional(self.pool()).await?.map(|r|Ok(EnrollmentTokenMetadata{id:r.get("id"),runner_id:r.get("runner_id"),expires_at:r.get("expires_at"),consumed_at:r.get("consumed_at"),revoked_at:r.get("revoked_at")})).transpose()
    }

    pub async fn revoke_enrollment_token_by_id(
        &self,
        runner_id: &str,
        token_id: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let r=sqlx::query("UPDATE agent_enrollment_tokens SET revoked_at=COALESCE(revoked_at,?) WHERE runner_id=? AND id=? AND consumed_at IS NULL").bind(stamp(clock)).bind(runner_id).bind(token_id).execute(self.pool()).await?;
        Ok(r.rows_affected() == 1)
    }

    pub async fn revoke_runner(
        &self,
        runner_id: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        let mut tx = self.pool().begin().await?;
        let r=sqlx::query("UPDATE agent_runners SET state='revoked',revoked_at=COALESCE(revoked_at,?),updated_at=? WHERE id=?").bind(&now).bind(&now).bind(runner_id).execute(&mut *tx).await?;
        if r.rows_affected() == 1 {
            sqlx::query("UPDATE agent_enrollment_tokens SET revoked_at=COALESCE(revoked_at,?) WHERE runner_id=? AND consumed_at IS NULL").bind(&now).bind(runner_id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(r.rows_affected() == 1)
    }

    /// Adds `runner_id` to `fleet_id`'s roster (`agent_fleet_members`).
    /// Idempotent: adding a runner that is already a member is a successful
    /// no-op (`AlreadyMember`), not a conflict — re-populating a fleet with
    /// the same membership twice is the expected operator workflow, not an
    /// error case. Existence of both sides is checked explicitly first (two
    /// reads outside a transaction — this table has no other writer racing
    /// against it, unlike credential rotation) so the caller gets a precise
    /// `FleetNotFound`/`RunnerNotFound` distinction rather than a generic
    /// foreign-key failure from the insert.
    pub async fn add_fleet_member(
        &self,
        fleet_id: &str,
        runner_id: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<AddFleetMemberOutcome, sqlx::Error> {
        let fleet_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_fleets WHERE id = ?)")
                .bind(fleet_id)
                .fetch_one(self.pool())
                .await?;
        if !fleet_exists {
            return Ok(AddFleetMemberOutcome::FleetNotFound);
        }
        let runner_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_runners WHERE id = ?)")
                .bind(runner_id)
                .fetch_one(self.pool())
                .await?;
        if !runner_exists {
            return Ok(AddFleetMemberOutcome::RunnerNotFound);
        }
        let now = stamp(clock);
        let result = sqlx::query(
            "INSERT OR IGNORE INTO agent_fleet_members (fleet_id,runner_id,created_at) VALUES (?,?,?)",
        )
        .bind(fleet_id)
        .bind(runner_id)
        .bind(&now)
        .execute(self.pool())
        .await?;
        Ok(if result.rows_affected() == 1 {
            AddFleetMemberOutcome::Added
        } else {
            AddFleetMemberOutcome::AlreadyMember
        })
    }

    /// Removes `runner_id` from `fleet_id`'s roster. Returns `false` if the
    /// pair was never a member (including either id not existing at all) —
    /// the handler maps that to `not_found` rather than treating a no-op
    /// delete as success, matching `revoke_runner`'s own
    /// exists-vs-no-op-true convention just above.
    pub async fn remove_fleet_member(
        &self,
        fleet_id: &str,
        runner_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let result =
            sqlx::query("DELETE FROM agent_fleet_members WHERE fleet_id = ? AND runner_id = ?")
                .bind(fleet_id)
                .bind(runner_id)
                .execute(self.pool())
                .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Compare-and-set credential rotation: the write only applies if
    /// `expected_credential_hash` still matches the runner's currently
    /// stored hash at commit time. Two concurrent or retried rotations from
    /// the same runner authenticate against the same still-valid old hash;
    /// without this compare-and-set the second writer silently overwrites
    /// the first, discarding a credential the runner may already have
    /// cached and persisted (unrecoverable without a fresh operator-issued
    /// enrollment token). Every other mutating path in this protocol has a
    /// compare-and-set or a replay-dedup table — this closes the one gap.
    pub async fn rotate_runner_credential(
        &self,
        runner_id: &str,
        expected_credential_hash: &str,
        new_credential_hash: &str,
        credential_expires_at: DateTime<Utc>,
        clock: &dyn ExecutionClock,
    ) -> Result<CredentialRotationResult, sqlx::Error> {
        let now = stamp(clock);
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let updated = sqlx::query(
            "UPDATE agent_runners SET credential_hash=?,credential_expires_at=?,credential_rotated_at=?,updated_at=? \
             WHERE id=? AND state='active' AND revoked_at IS NULL AND credential_hash=?",
        )
        .bind(new_credential_hash)
        .bind(credential_expires_at.to_rfc3339())
        .bind(&now)
        .bind(&now)
        .bind(runner_id)
        .bind(expected_credential_hash)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(CredentialRotationResult::HashMismatch);
        }
        tx.commit().await?;
        Ok(CredentialRotationResult::Rotated(RotatedCredential {
            runner_id: runner_id.into(),
            credential_expires_at: credential_expires_at.to_rfc3339(),
            rotated_at: now,
        }))
    }

    pub async fn operator_requeue_needs_operator(
        &self,
        request_id: &str,
        recovery_key: &str,
        actor: &str,
        reason_fingerprint: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<OperatorRequeueResult, sqlx::Error> {
        let now = stamp(clock);
        // Serialize duplicate operator requeues for the same request (e.g. a
        // double-clicked retry) before either transaction reads the
        // needs_operator attempt row. Two deferred readers can otherwise
        // deadlock while both try to upgrade to writers. Mirrors the
        // redeem_enrollment_token and claim_execution_idempotent_with_snapshot
        // fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let attempt:Option<String>=sqlx::query_scalar("SELECT id FROM execution_attempts WHERE request_id=? AND state='needs_operator' ORDER BY attempt_number DESC LIMIT 1").bind(request_id).fetch_optional(&mut *tx).await?;
        let Some(attempt) = attempt else {
            tx.commit().await?;
            return Ok(OperatorRequeueResult::InvalidTransition);
        };
        let details = format!("actor={actor};reason_fingerprint={reason_fingerprint}");
        if let Some(old) = sqlx::query_scalar::<_, String>(
            "SELECT details FROM execution_recovery_audits WHERE attempt_id=? AND recovery_key=?",
        )
        .bind(&attempt)
        .bind(recovery_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            tx.commit().await?;
            return Ok(if old == details {
                OperatorRequeueResult::Replayed
            } else {
                OperatorRequeueResult::Conflict
            });
        };
        let authoritative_recovery: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM execution_recovery_audits WHERE attempt_id=? AND classification='needs_operator' AND fingerprint <> '' AND response <> '')",
        )
        .bind(&attempt)
        .fetch_one(&mut *tx)
        .await?;
        if !authoritative_recovery {
            tx.commit().await?;
            return Ok(OperatorRequeueResult::InvalidTransition);
        }
        if sqlx::query("UPDATE execution_requests SET state='queued',cancellation_requested_at=NULL,updated_at=? WHERE id=? AND state='needs_operator'").bind(&now).bind(request_id).execute(&mut *tx).await?.rows_affected()!=1{tx.rollback().await?;return Ok(OperatorRequeueResult::InvalidTransition)};
        sqlx::query("INSERT INTO execution_recovery_audits(attempt_id,recovery_key,classification,details,created_at) VALUES(?,?, 'operator_requeue', ?,?)").bind(&attempt).bind(recovery_key).bind(details).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(OperatorRequeueResult::Requeued)
    }
    /// Persist the two runner start acknowledgements with natural idempotency.
    /// Workspace facts are immutable once preparation is accepted; observing
    /// a process is the only transition that may add `process_id`.
    pub async fn transition_attempt_with_facts(
        &self,
        input: AttemptTransitionInput<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<AttemptTransitionResult, sqlx::Error> {
        if input.workspace_id.is_empty()
            || input.base_revision.is_empty()
            || matches!(input.phase, AttemptTransitionPhase::Preparing)
                && input.process_id.is_some()
            || matches!(input.phase, AttemptTransitionPhase::Running)
                && (input.process_id.is_none() || input.process_id.is_some_and(str::is_empty))
        {
            return Ok(AttemptTransitionResult::Conflict);
        }

        let now = stamp(clock);
        // Serialize duplicate transition acknowledgements for the same
        // attempt (e.g. a runner retrying an unacknowledged "preparing" or
        // "running" report) before either transaction reads this attempt
        // row. Two deferred readers can otherwise deadlock while both try to
        // upgrade to writers. Mirrors the redeem_enrollment_token and
        // claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query(
            "SELECT a.state,a.workspace_id,a.base_revision,a.process_id,a.prepared_at,a.started_at \
             FROM execution_attempts a JOIN agent_runners r ON r.id=a.runner_id \
             WHERE a.id=? AND a.runner_id=? AND a.fencing_token=? \
             AND a.lease_expires_at>? AND r.state='active' AND r.revoked_at IS NULL",
        )
        .bind(input.attempt_id)
        .bind(input.runner_id)
        .bind(input.fencing_token)
        .bind(&now)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(AttemptTransitionResult::Stale);
        };

        let state: String = row.get("state");
        let workspace_id: Option<String> = row.get("workspace_id");
        let base_revision: Option<String> = row.get("base_revision");
        let process_id: Option<String> = row.get("process_id");
        let prepared_at: Option<String> = row.get("prepared_at");
        let started_at: Option<String> = row.get("started_at");
        let exact_workspace = workspace_id.as_deref() == Some(input.workspace_id)
            && base_revision.as_deref() == Some(input.base_revision);

        match input.phase {
            AttemptTransitionPhase::Preparing if exact_workspace && prepared_at.is_some() => {
                tx.commit().await?;
                let Some(committed_at) = prepared_at else {
                    return Ok(AttemptTransitionResult::Conflict);
                };
                return Ok(AttemptTransitionResult::Replayed(
                    AttemptTransitionResponse {
                        attempt_id: input.attempt_id.into(),
                        state: "preparing".into(),
                        committed_at,
                    },
                ));
            }
            AttemptTransitionPhase::Running
                if exact_workspace
                    && process_id.as_deref() == input.process_id
                    && started_at.is_some() =>
            {
                tx.commit().await?;
                let Some(committed_at) = started_at else {
                    return Ok(AttemptTransitionResult::Conflict);
                };
                return Ok(AttemptTransitionResult::Replayed(
                    AttemptTransitionResponse {
                        attempt_id: input.attempt_id.into(),
                        state: "running".into(),
                        committed_at,
                    },
                ));
            }
            AttemptTransitionPhase::Preparing if state == "leased" => {
                let updated = sqlx::query(
                    "UPDATE execution_attempts SET state='preparing',workspace_id=?,base_revision=?,prepared_at=?,updated_at=? \
                     WHERE id=? AND runner_id=? AND fencing_token=? AND state='leased' AND lease_expires_at>?",
                )
                .bind(input.workspace_id)
                .bind(input.base_revision)
                .bind(&now)
                .bind(&now)
                .bind(input.attempt_id)
                .bind(input.runner_id)
                .bind(input.fencing_token)
                .bind(&now)
                .execute(&mut *tx)
                .await?;
                if updated.rows_affected() != 1 {
                    tx.rollback().await?;
                    return Ok(AttemptTransitionResult::Stale);
                }
                tx.commit().await?;
                return Ok(AttemptTransitionResult::Applied(
                    AttemptTransitionResponse {
                        attempt_id: input.attempt_id.into(),
                        state: "preparing".into(),
                        committed_at: now,
                    },
                ));
            }
            AttemptTransitionPhase::Running if state == "preparing" && exact_workspace => {
                let updated = sqlx::query(
                    "UPDATE execution_attempts SET state='running',process_id=?,started_at=?,updated_at=? \
                     WHERE id=? AND runner_id=? AND fencing_token=? AND state='preparing' \
                     AND workspace_id=? AND base_revision=? AND lease_expires_at>?",
                )
                .bind(input.process_id)
                .bind(&now)
                .bind(&now)
                .bind(input.attempt_id)
                .bind(input.runner_id)
                .bind(input.fencing_token)
                .bind(input.workspace_id)
                .bind(input.base_revision)
                .bind(&now)
                .execute(&mut *tx)
                .await?;
                if updated.rows_affected() != 1 {
                    tx.rollback().await?;
                    return Ok(AttemptTransitionResult::Stale);
                }
                tx.commit().await?;
                return Ok(AttemptTransitionResult::Applied(
                    AttemptTransitionResponse {
                        attempt_id: input.attempt_id.into(),
                        state: "running".into(),
                        committed_at: now,
                    },
                ));
            }
            _ => {}
        }

        tx.commit().await?;
        Ok(AttemptTransitionResult::Conflict)
    }

    #[allow(clippy::too_many_arguments)] // protocol batch fields must fingerprint together
    pub async fn heartbeat_batch(
        &self,
        runner_id: &str,
        heartbeat_id: &str,
        sent_at: DateTime<Utc>,
        available_capacity: i64,
        leases: &[HeartbeatLease<'_>],
        lease_duration: Duration,
        clock: &dyn ExecutionClock,
    ) -> Result<HeartbeatBatchResult, sqlx::Error> {
        let now = clock.now();
        let now_s = now.to_rfc3339();
        let expires = now + lease_duration;
        let fingerprint = heartbeat_fingerprint(
            runner_id,
            heartbeat_id,
            sent_at,
            available_capacity,
            leases,
            lease_duration,
        )?;
        // Serialize duplicate heartbeat batches for the same runner (e.g. a
        // runner retrying an unacknowledged heartbeat POST) before either
        // transaction reads the runner's capacity row. Two deferred readers
        // can otherwise deadlock while both try to upgrade to writers.
        // Mirrors the redeem_enrollment_token and
        // claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let capacity: Option<i64> = sqlx::query_scalar(
            "SELECT total_capacity FROM agent_runners WHERE id=? AND state='active' AND revoked_at IS NULL",
        )
        .bind(runner_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(capacity) = capacity else {
            tx.commit().await?;
            return Ok(HeartbeatBatchResult::StaleLease(runner_id.into()));
        };
        if let Some(row) = sqlx::query(
            "SELECT fingerprint,response FROM execution_heartbeat_replays WHERE runner_id=? AND heartbeat_id=?",
        )
        .bind(runner_id)
        .bind(heartbeat_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_fingerprint: String = row.get("fingerprint");
            if stored_fingerprint != fingerprint {
                tx.commit().await?;
                return Ok(HeartbeatBatchResult::Conflict);
            }
            let response: String = row.get("response");
            let response = heartbeat_response(&response)?;
            tx.commit().await?;
            return Ok(HeartbeatBatchResult::Replayed(response));
        }
        let active_reservations: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM execution_attempts WHERE runner_id=? AND state IN ('leased','preparing','running','waiting_decision')",
        )
        .bind(runner_id)
        .fetch_one(&mut *tx)
        .await?;
        let expected_available_capacity = capacity - active_reservations;
        if expected_available_capacity < 0 || available_capacity != expected_available_capacity {
            tx.rollback().await?;
            return Ok(HeartbeatBatchResult::Conflict);
        }
        let mut result = Vec::with_capacity(leases.len());
        for lease in leases {
            let row=sqlx::query("SELECT r.cancellation_requested_at FROM execution_attempts a JOIN execution_requests r ON r.id=a.request_id WHERE a.id=? AND a.runner_id=? AND a.fencing_token=? AND a.state IN ('leased','preparing','running','waiting_decision') AND a.lease_expires_at>?").bind(lease.attempt_id).bind(runner_id).bind(lease.fencing_token).bind(&now_s).fetch_optional(&mut *tx).await?;
            let Some(row) = row else {
                tx.rollback().await?;
                return Ok(HeartbeatBatchResult::StaleLease(lease.attempt_id.into()));
            };
            let cancellation: Option<String> = row.get("cancellation_requested_at");
            sqlx::query("UPDATE execution_attempts SET last_heartbeat_at=?,lease_expires_at=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=?").bind(&now_s).bind(expires.to_rfc3339()).bind(&now_s).bind(lease.attempt_id).bind(runner_id).bind(lease.fencing_token).execute(&mut *tx).await?;
            result.push(HeartbeatLeaseResponse {
                attempt_id: lease.attempt_id.into(),
                fencing_token: lease.fencing_token,
                state: lease.state.into(),
                journal_state: lease.journal_state.into(),
                last_event_checkpoint: lease.last_event_checkpoint.map(str::to_owned),
                lease_expires_at: expires,
                cancellation_requested: cancellation.is_some(),
            });
        }
        sqlx::query("UPDATE agent_runners SET available_capacity=?,last_heartbeat_at=?,updated_at=? WHERE id=?").bind(available_capacity).bind(&now_s).bind(&now_s).bind(runner_id).execute(&mut *tx).await?;
        let response = HeartbeatBatchResponse {
            heartbeat_id: heartbeat_id.into(),
            accepted_at: now,
            leases: result,
        };
        let stored = serde_json::json!({
            "heartbeat_id": response.heartbeat_id,
            "accepted_at": response.accepted_at.to_rfc3339(),
            "leases": response.leases.iter().map(|lease| serde_json::json!({
                "attempt_id": lease.attempt_id,
                "fencing_token": lease.fencing_token,
                "state": lease.state,
                "journal_state": lease.journal_state,
                "last_event_checkpoint": lease.last_event_checkpoint,
                "lease_expires_at": lease.lease_expires_at.to_rfc3339(),
                "cancellation_requested": lease.cancellation_requested,
            })).collect::<Vec<_>>(),
        })
        .to_string();
        sqlx::query("INSERT INTO execution_heartbeat_replays(runner_id,heartbeat_id,fingerprint,response,created_at) VALUES(?,?,?,?,?)").bind(runner_id).bind(heartbeat_id).bind(fingerprint).bind(&stored).bind(&now_s).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(HeartbeatBatchResult::Accepted(response))
    }

    pub async fn recover_attempt(
        &self,
        input: RecoveryObservationInput<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<RecoveryObservationResult, sqlx::Error> {
        let now = stamp(clock);
        let details = recovery_details(input.details)?;
        let fingerprint = recovery_fingerprint(&input)?;
        // Serialize duplicate recovery observations for the same attempt
        // (e.g. a runner or reconciler retrying an unacknowledged recovery
        // report) before either transaction reads this attempt row. Two
        // deferred readers can otherwise deadlock while both try to upgrade
        // to writers. Mirrors the redeem_enrollment_token and
        // claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT a.request_id,a.runner_id,a.state,a.started_at FROM execution_attempts a JOIN agent_runners r ON r.id=a.runner_id WHERE a.id=? AND a.runner_id=? AND a.fencing_token=? AND r.state='active' AND r.revoked_at IS NULL")
            .bind(input.attempt_id).bind(input.runner_id).bind(input.fencing_token).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(RecoveryObservationResult::Stale);
        };
        if let Some(row) = sqlx::query("SELECT fingerprint,response FROM execution_recovery_audits WHERE attempt_id=? AND recovery_key=?")
            .bind(input.attempt_id).bind(input.recovery_key).fetch_optional(&mut *tx).await? {
            let stored_fingerprint: String = row.get("fingerprint");
            if stored_fingerprint != fingerprint {
                tx.commit().await?;
                return Ok(RecoveryObservationResult::Conflict);
            }
            let response: String = row.get("response");
            let response = recovery_response(&response)?;
            tx.commit().await?;
            return Ok(RecoveryObservationResult::Replayed(response));
        }
        let request: String = row.get("request_id");
        let runner: String = row.get("runner_id");
        let state: String = row.get("state");
        if terminal(&state) {
            let response = RecoveryResponse {
                attempt_id: input.attempt_id.into(),
                recovery_key: input.recovery_key.into(),
                disposition: RecoveryDisposition::AlreadyTerminal,
                committed_at: now.clone(),
            };
            let serialized_response = serde_json::json!({
                "attempt_id": response.attempt_id,
                "recovery_key": response.recovery_key,
                "disposition": "already_terminal",
                "committed_at": response.committed_at,
            })
            .to_string();
            sqlx::query("INSERT INTO execution_recovery_audits(attempt_id,recovery_key,classification,details,fingerprint,response,created_at) VALUES(?,?, 'already_terminal',?,?,?,?)")
                .bind(input.attempt_id).bind(input.recovery_key).bind(input.details).bind(fingerprint).bind(serialized_response).bind(&now).execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(RecoveryObservationResult::Applied(response));
        }
        if !matches!(
            state.as_str(),
            "leased" | "preparing" | "running" | "waiting_decision"
        ) {
            tx.commit().await?;
            return Ok(RecoveryObservationResult::Stale);
        }
        let started: Option<String> = row.get("started_at");
        let disposition = match input.observation {
            RecoveryObservation::ProcessStopped
                if started.is_none()
                    && details.journal_state == "prepared"
                    && !details.process_observed =>
            {
                RecoveryDisposition::SafePreSpawnRequeue
            }
            RecoveryObservation::ProcessStopped
            | RecoveryObservation::ProcessRunning
            | RecoveryObservation::Ambiguous => RecoveryDisposition::NeedsOperator,
        };
        let (attempt_state, request_state, classification) = match disposition {
            RecoveryDisposition::SafePreSpawnRequeue => {
                ("lost", "queued", "safe_pre_spawn_requeue")
            }
            RecoveryDisposition::NeedsOperator => {
                ("needs_operator", "needs_operator", "needs_operator")
            }
            RecoveryDisposition::AlreadyTerminal => unreachable!("terminal states return above"),
        };
        let updated = sqlx::query("UPDATE execution_attempts SET state=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND state IN ('leased','preparing','running','waiting_decision')")
            .bind(attempt_state)
            .bind(&now)
            .bind(input.attempt_id)
            .bind(input.runner_id)
            .bind(input.fencing_token)
            .execute(&mut *tx)
            .await?;
        if updated.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(RecoveryObservationResult::Stale);
        }
        sqlx::query("UPDATE execution_requests SET state=?,cancellation_requested_at=CASE WHEN ?='queued' THEN NULL ELSE cancellation_requested_at END,updated_at=? WHERE id=?")
            .bind(request_state)
            .bind(request_state)
            .bind(&now)
            .bind(&request)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE agent_runners SET available_capacity=MIN(total_capacity,available_capacity+1),updated_at=? WHERE id=?").bind(&now).bind(&runner).execute(&mut *tx).await?;
        let response = RecoveryResponse {
            attempt_id: input.attempt_id.into(),
            recovery_key: input.recovery_key.into(),
            disposition: disposition.clone(),
            committed_at: now.clone(),
        };
        let serialized_response = serde_json::json!({
            "attempt_id": response.attempt_id,
            "recovery_key": response.recovery_key,
            "disposition": match response.disposition {
                RecoveryDisposition::SafePreSpawnRequeue => "safe_pre_spawn_requeue",
                RecoveryDisposition::NeedsOperator => "needs_operator",
                RecoveryDisposition::AlreadyTerminal => "already_terminal",
            },
            "committed_at": response.committed_at,
        })
        .to_string();
        sqlx::query("INSERT INTO execution_recovery_audits(attempt_id,recovery_key,classification,details,fingerprint,response,created_at) VALUES(?,?,?,?,?,?,?)")
            .bind(input.attempt_id).bind(input.recovery_key).bind(classification).bind(input.details).bind(fingerprint).bind(serialized_response).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(RecoveryObservationResult::Applied(response))
    }

    pub async fn observe_cancellation(
        &self,
        input: CancellationObservationInput<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<CancellationObservation, sqlx::Error> {
        let now = stamp(clock);
        let fingerprint = cancellation_fingerprint(&input)?;
        // Serialize duplicate cancellation observations for the same attempt
        // (e.g. a runner retrying an unacknowledged cancellation report)
        // before either transaction reads this attempt/request row. Two
        // deferred readers can otherwise deadlock while both try to upgrade
        // to writers. Mirrors the redeem_enrollment_token and
        // claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT a.request_id,a.runner_id,a.state,a.lease_expires_at,r.cancellation_requested_at FROM execution_attempts a JOIN execution_requests r ON r.id=a.request_id WHERE a.id=? AND a.runner_id=? AND a.fencing_token=?")
            .bind(input.attempt_id).bind(input.runner_id).bind(input.fencing_token).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(CancellationObservation::Stale);
        };
        if let Some(row) = sqlx::query("SELECT fingerprint,response FROM execution_cancellation_replays WHERE attempt_id=? AND cancellation_request_id=?")
            .bind(input.attempt_id).bind(input.cancellation_request_id).fetch_optional(&mut *tx).await? {
            let stored_fingerprint: String = row.get("fingerprint");
            if stored_fingerprint != fingerprint {
                tx.commit().await?;
                return Ok(CancellationObservation::Conflict);
            }
            let response: String = row.get("response");
            let response = cancellation_response(&response)?;
            tx.commit().await?;
            return Ok(CancellationObservation::Replayed(response));
        }
        let request: String = row.get("request_id");
        let owner: String = row.get("runner_id");
        let state: String = row.get("state");
        if terminal(&state) {
            tx.commit().await?;
            return Ok(CancellationObservation::AlreadyTerminal { state });
        }
        if !matches!(
            state.as_str(),
            "leased" | "preparing" | "running" | "waiting_decision"
        ) {
            tx.commit().await?;
            return Ok(CancellationObservation::Ambiguous { state });
        }
        let expires: String = row.get("lease_expires_at");
        if expires <= now {
            tx.commit().await?;
            return Ok(CancellationObservation::Stale);
        }
        let cancellation_requested_at: Option<String> = row.get("cancellation_requested_at");
        if cancellation_requested_at.is_none() {
            tx.commit().await?;
            return Ok(CancellationObservation::Ambiguous { state });
        }
        let updated = sqlx::query("UPDATE execution_attempts SET state='cancelled',completion_id=?,ended_at=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at>?")
            .bind(input.cancellation_request_id).bind(&now).bind(&now).bind(input.attempt_id).bind(input.runner_id).bind(input.fencing_token).bind(&now).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(CancellationObservation::Ambiguous { state });
        }
        sqlx::query("UPDATE execution_requests SET state='cancelled',updated_at=? WHERE id=?")
            .bind(&now)
            .bind(request)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE agent_runners SET available_capacity=MIN(total_capacity,available_capacity+1),updated_at=? WHERE id=?").bind(&now).bind(owner).execute(&mut *tx).await?;
        let response = CancellationResponse {
            attempt_id: input.attempt_id.into(),
            cancellation_request_id: input.cancellation_request_id.into(),
            state: "cancelled".into(),
            committed_at: now.clone(),
        };
        let serialized_response = serde_json::json!({
            "attempt_id": response.attempt_id,
            "cancellation_request_id": response.cancellation_request_id,
            "state": response.state,
            "committed_at": response.committed_at,
        })
        .to_string();
        sqlx::query("INSERT INTO execution_cancellation_replays(attempt_id,cancellation_request_id,state,fingerprint,response,created_at) VALUES(?,?, 'cancelled',?,?,?)")
            .bind(input.attempt_id).bind(input.cancellation_request_id).bind(fingerprint).bind(serialized_response).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(CancellationObservation::Cancelled(response))
    }
    /// Store only a token hash. Tokens are tied to a pre-created runner so a
    /// redemption can atomically consume the token and activate that identity.
    pub async fn issue_enrollment_token(
        &self,
        token: EnrollmentToken<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO agent_enrollment_tokens (id, runner_id, token_hash, expires_at, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(token.id).bind(token.runner_id).bind(token.token_hash).bind(token.expires_at.to_rfc3339()).bind(stamp(clock))
            .execute(self.pool()).await?;
        Ok(())
    }

    pub async fn revoke_enrollment_token(
        &self,
        token_hash: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("UPDATE agent_enrollment_tokens SET revoked_at = COALESCE(revoked_at, ?) WHERE token_hash = ? AND consumed_at IS NULL")
            .bind(stamp(clock)).bind(token_hash).execute(self.pool()).await?;
        Ok(result.rows_affected() == 1)
    }

    // `runner_name` is accepted (the runner-v1 protocol requires the
    // field and validates its presence upstream in
    // `crates/tack-api/src/handlers/runner_protocol.rs::enroll`) but
    // deliberately NOT written to `agent_runners.name` here. `name` carries a
    // `UNIQUE` constraint (migration 040); the operator already assigned it,
    // uniquely, when the pending-runner row was created
    // (`create_pending_runner_and_issue_token`). Two default-configured
    // runners on one host self-report the same `runner_name` (both default it
    // from `TACK_RUNNER_ID`), so letting the self-reported value overwrite
    // the operator-assigned one made the *second* runner's enrollment race
    // the first's for that string and fail a `UNIQUE` constraint the caller
    // never intended to touch — surfaced as an unhandled 500.
    // The operator-assigned name is authoritative for identity;
    // the self-report is accepted for protocol-shape validation only.
    #[allow(clippy::too_many_arguments)] // protocol exchange fields must commit together
    pub async fn redeem_enrollment_token(
        &self,
        token_hash: &str,
        credential_hash: &str,
        credential_expires_at: DateTime<Utc>,
        runner_version: &str,
        _runner_name: &str,
        labels: &str,
        total_capacity: i64,
        available_capacity: i64,
        capability_snapshot: &str,
        protocol_version: i64,
        clock: &dyn ExecutionClock,
    ) -> Result<RedeemEnrollmentResult, sqlx::Error> {
        let now = stamp(clock);
        // Serialize token consumers before either transaction reads the
        // single-use row. Two deferred readers can otherwise deadlock while
        // both try to upgrade to writers, surfacing a database error instead
        // of one authoritative winner and one invalid/expired result.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let token = sqlx::query("SELECT runner_id FROM agent_enrollment_tokens WHERE token_hash = ? AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at > ?")
            .bind(token_hash).bind(&now).fetch_optional(&mut *tx).await?;
        let Some(token) = token else {
            tx.commit().await?;
            return Ok(RedeemEnrollmentResult::InvalidOrExpired);
        };
        let runner_id: String = token.get("runner_id");
        let used = sqlx::query("UPDATE agent_enrollment_tokens SET consumed_at = ? WHERE token_hash = ? AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at > ?")
            .bind(&now).bind(token_hash).bind(&now).execute(&mut *tx).await?;
        if used.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(RedeemEnrollmentResult::InvalidOrExpired);
        }
        let runner = sqlx::query("UPDATE agent_runners SET credential_hash=?, credential_expires_at=?, credential_rotated_at=?, runner_version=?, labels=?, total_capacity=?, available_capacity=?, capability_snapshot=?, protocol_version=?, state='active', updated_at=? WHERE id=? AND state='pending_enrollment' AND revoked_at IS NULL")
            .bind(credential_hash).bind(credential_expires_at.to_rfc3339()).bind(&now).bind(runner_version).bind(labels).bind(total_capacity).bind(available_capacity).bind(capability_snapshot).bind(protocol_version).bind(&now).bind(&runner_id).execute(&mut *tx).await?;
        if runner.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(RedeemEnrollmentResult::InvalidOrExpired);
        }
        tx.commit().await?;
        Ok(RedeemEnrollmentResult::Redeemed(runner_id))
    }

    /// Strong claim: replay lookup, capacity reservation, attempt and replay
    /// record are committed together, so a crash cannot lose the replay key.
    ///
    /// `selection` decides which queued request (if any) this call attempts
    /// to lease once the capacity check above has passed — see
    /// [`RequestSelection`]'s doc comment for why that decision is made by
    /// the caller rather than this module.
    pub async fn claim_execution_idempotent_with_snapshot(
        &self,
        runner_id: &str,
        claim_request_id: &str,
        attempt_id: &str,
        lease_duration: Duration,
        clock: &dyn ExecutionClock,
        selection: RequestSelection<'_>,
    ) -> Result<Option<ClaimedExecution>, sqlx::Error> {
        let now = clock.now();
        let now_s = now.to_rfc3339();
        let expires = now + lease_duration;
        // Serialize claimants before either transaction reads the runner's
        // shared capacity row. Two deferred readers can otherwise deadlock
        // while both try to upgrade to writers, surfacing a database error
        // instead of one authoritative lease and one well-typed no-work
        // result. Mirrors the redeem_enrollment_token fix above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let replay=sqlx::query("SELECT a.id,a.request_id,a.attempt_number,a.runner_id,a.fencing_token,a.lease_issued_at,a.lease_expires_at,r.request_snapshot FROM execution_claim_replays c JOIN execution_attempts a ON a.id=c.attempt_id JOIN execution_requests r ON r.id=a.request_id WHERE c.runner_id=? AND c.claim_request_id=?").bind(runner_id).bind(claim_request_id).fetch_optional(&mut *tx).await?;
        if let Some(row) = replay {
            let lease = lease_from_row(&row)?;
            let snapshot = snapshot(&row)?;
            tx.commit().await?;
            return Ok(Some(ClaimedExecution {
                lease,
                request_snapshot: snapshot,
            }));
        }
        if sqlx::query("UPDATE agent_runners SET available_capacity=available_capacity-1,updated_at=? WHERE id=? AND state='active' AND revoked_at IS NULL AND available_capacity>0").bind(&now_s).bind(runner_id).execute(&mut *tx).await?.rows_affected()!=1 { tx.commit().await?; return Ok(None); }
        // `Scheduled(None)` means the scheduler already looked at every
        // selector-eligible request and found none this runner is eligible
        // for — reported as "no work" (after undoing the capacity
        // reservation above) without ever issuing the naive query below,
        // which would silently override that rejection.
        if matches!(selection, RequestSelection::Scheduled(None)) {
            tx.rollback().await?;
            return Ok(None);
        }
        let request = match selection {
            RequestSelection::Naive => {
                sqlx::query("SELECT id,request_snapshot FROM execution_requests WHERE state='queued' AND ((selector_kind='exact_runner' AND selector_id=?) OR (selector_kind='fleet' AND EXISTS(SELECT 1 FROM agent_fleet_members m WHERE m.fleet_id=selector_id AND m.runner_id=?))) ORDER BY created_at LIMIT 1")
                    .bind(runner_id).bind(runner_id).fetch_optional(&mut *tx).await?
            }
            RequestSelection::Scheduled(Some(chosen_id)) => {
                // Defense in depth: re-verify the scheduler's chosen id is
                // still `queued` and still selector-eligible for this
                // runner, exactly as the naive path always has, rather than
                // trusting the earlier read-only snapshot blindly.
                sqlx::query("SELECT id,request_snapshot FROM execution_requests WHERE id=? AND state='queued' AND ((selector_kind='exact_runner' AND selector_id=?) OR (selector_kind='fleet' AND EXISTS(SELECT 1 FROM agent_fleet_members m WHERE m.fleet_id=selector_id AND m.runner_id=?)))")
                    .bind(chosen_id).bind(runner_id).bind(runner_id).fetch_optional(&mut *tx).await?
            }
            RequestSelection::Scheduled(None) => unreachable!("handled above"),
        };
        let Some(request) = request else {
            tx.rollback().await?;
            return Ok(None);
        };
        let request_id: String = request.get("id");
        if sqlx::query("UPDATE execution_requests SET state='leased',updated_at=? WHERE id=? AND state='queued'").bind(&now_s).bind(&request_id).execute(&mut *tx).await?.rows_affected()!=1 { tx.rollback().await?; return Ok(None); }
        let n: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(attempt_number),0)+1 FROM execution_attempts WHERE request_id=?",
        )
        .bind(&request_id)
        .fetch_one(&mut *tx)
        .await?;
        let f: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(fencing_token),0)+1 FROM execution_attempts WHERE request_id=?",
        )
        .bind(&request_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO execution_attempts(id,request_id,attempt_number,runner_id,fencing_token,lease_issued_at,lease_expires_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(attempt_id).bind(&request_id).bind(n).bind(runner_id).bind(f).bind(&now_s).bind(expires.to_rfc3339()).bind(&now_s).bind(&now_s).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO execution_claim_replays(runner_id,claim_request_id,attempt_id,created_at) VALUES(?,?,?,?)").bind(runner_id).bind(claim_request_id).bind(attempt_id).bind(&now_s).execute(&mut *tx).await?;
        let lease = Lease {
            attempt_id: attempt_id.into(),
            request_id,
            attempt_number: n,
            runner_id: runner_id.into(),
            fencing_token: f,
            issued_at: now,
            expires_at: expires,
        };
        let snapshot = snapshot(&request)?;
        tx.commit().await?;
        Ok(Some(ClaimedExecution {
            lease,
            request_snapshot: snapshot,
        }))
    }

    /// Read-only scheduling input: this runner's own current state plus its
    /// fleet memberships. `None` if `runner_id` does not exist. See
    /// [`RequestSelection`]'s doc comment for why this is a plain read
    /// rather than something `tack-orch` fetches itself.
    pub async fn fetch_runner_scheduling_snapshot(
        &self,
        runner_id: &str,
    ) -> Result<Option<RunnerSchedulingSnapshot>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT state, labels, total_capacity, available_capacity, last_heartbeat_at, \
             capability_snapshot FROM agent_runners WHERE id = ?",
        )
        .bind(runner_id)
        .fetch_optional(self.pool())
        .await?;
        let Some(row) = row else { return Ok(None) };
        let fleet_ids: Vec<String> =
            sqlx::query_scalar("SELECT fleet_id FROM agent_fleet_members WHERE runner_id = ?")
                .bind(runner_id)
                .fetch_all(self.pool())
                .await?;
        Ok(Some(RunnerSchedulingSnapshot {
            runner_id: runner_id.to_string(),
            state: row.get("state"),
            labels: row.get("labels"),
            total_capacity: row.get("total_capacity"),
            available_capacity: row.get("available_capacity"),
            last_heartbeat_at: row.get("last_heartbeat_at"),
            capability_snapshot: row.get("capability_snapshot"),
            fleet_ids,
        }))
    }

    /// Read-only scheduling input: every `queued` request this runner is
    /// selector-eligible for (exact match, or fleet membership) — the exact
    /// same `WHERE` clause `claim_execution_idempotent_with_snapshot`'s
    /// naive path uses, so the scheduler considers precisely the same
    /// candidate pool the pre-Wave-4 query did, only with real eligibility
    /// filtering applied on top instead of a bare `ORDER BY created_at`.
    pub async fn list_eligible_queued_requests(
        &self,
        runner_id: &str,
    ) -> Result<Vec<QueuedRequestForScheduling>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, selector_kind, selector_id, requested_harness_kind, \
             requested_model_provider, requested_model_id, created_at, metadata \
             FROM execution_requests WHERE state='queued' AND \
             ((selector_kind='exact_runner' AND selector_id=?) OR \
             (selector_kind='fleet' AND EXISTS(SELECT 1 FROM agent_fleet_members m \
             WHERE m.fleet_id=selector_id AND m.runner_id=?))) ORDER BY created_at",
        )
        .bind(runner_id)
        .bind(runner_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| QueuedRequestForScheduling {
                id: row.get("id"),
                selector_kind: row.get("selector_kind"),
                selector_id: row.get("selector_id"),
                requested_harness_kind: row.get("requested_harness_kind"),
                requested_model_provider: row.get("requested_model_provider"),
                requested_model_id: row.get("requested_model_id"),
                created_at: row.get("created_at"),
                metadata: row.get("metadata"),
            })
            .collect())
    }

    /// Read-only scheduling input: `fleet_id`'s configured
    /// `concurrency_limit` alongside its current aggregate in-use capacity.
    /// `None` if `fleet_id` does not exist.
    pub async fn fetch_fleet_concurrency(
        &self,
        fleet_id: &str,
    ) -> Result<Option<FleetConcurrencySnapshot>, sqlx::Error> {
        let row = sqlx::query("SELECT concurrency_limit FROM agent_fleets WHERE id = ?")
            .bind(fleet_id)
            .fetch_optional(self.pool())
            .await?;
        let Some(row) = row else { return Ok(None) };
        let concurrency_limit: Option<i64> = row.get("concurrency_limit");
        let in_use: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(r.total_capacity - r.available_capacity),0) FROM agent_runners r \
             JOIN agent_fleet_members m ON m.runner_id = r.id WHERE m.fleet_id = ?",
        )
        .bind(fleet_id)
        .fetch_one(self.pool())
        .await?;
        Ok(Some(FleetConcurrencySnapshot {
            concurrency_limit,
            in_use,
        }))
    }

    /// Read-only model-policy input: `agent_profile_id`'s raw
    /// `limits` JSON blob (migration 042), unparsed — the convention read
    /// out of it (`{"default_model": ...}`) lives in
    /// `tack_orch::model_policy::wiring`, which cannot be called from here
    /// (`tack-db` cannot depend on `tack-orch`; see [`RequestSelection`]'s
    /// doc comment for the same layering reason). `None` if
    /// `agent_profile_id` does not exist.
    pub async fn fetch_agent_profile_limits(
        &self,
        agent_profile_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT limits FROM agent_profiles WHERE id = ?")
            .bind(agent_profile_id)
            .fetch_optional(self.pool())
            .await
    }

    /// Read-only model-policy input: `fleet_id`'s raw
    /// `default_policy` JSON blob (migration 039), unparsed — same
    /// convention/layering note as [`Self::fetch_agent_profile_limits`].
    /// `None` if `fleet_id` does not exist.
    pub async fn fetch_fleet_default_policy(
        &self,
        fleet_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT default_policy FROM agent_fleets WHERE id = ?")
            .bind(fleet_id)
            .fetch_optional(self.pool())
            .await
    }

    /// Read-only model-policy input: `project_id`'s raw `default_model` JSON
    /// blob (migration 062). Unlike `fetch_agent_profile_limits`/
    /// `fetch_fleet_default_policy`'s untyped `{"default_model": ...}`
    /// convention, this column only ever holds the exact serialization of a
    /// validated `tack_core::models::ProjectModelDefault` (written by
    /// `Repository::update_project`), so its caller doesn't need the same
    /// tolerant, malformed-input-is-"no opinion" parse those two use.
    /// `None` if `project_id` does not exist or has no default configured.
    pub async fn fetch_project_default_model(
        &self,
        project_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let raw: Option<Option<String>> =
            sqlx::query_scalar("SELECT default_model FROM projects WHERE id = ?")
                .bind(project_id)
                .fetch_optional(self.pool())
                .await?;
        Ok(raw.flatten())
    }

    /// Every enrolled runner, newest-created last, with its current fleet
    /// roster. Backs `GET /api/runners` — `agent_runners`
    /// itself has held this data since migration 040; nothing read it back
    /// to an operator before this.
    pub async fn list_runners(&self) -> Result<Vec<RunnerListingRow>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, name, state, labels, total_capacity, available_capacity, \
             capability_snapshot, protocol_version, runner_version, last_heartbeat_at, \
             revoked_at, created_at, updated_at FROM agent_runners ORDER BY created_at",
        )
        .fetch_all(self.pool())
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.get("id");
            let fleet_ids: Vec<String> =
                sqlx::query_scalar("SELECT fleet_id FROM agent_fleet_members WHERE runner_id = ?")
                    .bind(&id)
                    .fetch_all(self.pool())
                    .await?;
            out.push(RunnerListingRow {
                id,
                name: row.get("name"),
                state: row.get("state"),
                labels: row.get("labels"),
                total_capacity: row.get("total_capacity"),
                available_capacity: row.get("available_capacity"),
                capability_snapshot: row.get("capability_snapshot"),
                protocol_version: row.get("protocol_version"),
                runner_version: row.get("runner_version"),
                last_heartbeat_at: row.get("last_heartbeat_at"),
                revoked_at: row.get("revoked_at"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                fleet_ids,
            });
        }
        Ok(out)
    }

    /// Backs the unscoped and one-item shapes of `GET /api/executions` —
    /// newest first, bounded by `limit` either way. `item_id`, when given,
    /// scopes the result to that one item's requests instead of every
    /// request the install has ever recorded.
    #[instrument(skip(self))]
    pub async fn list_executions(
        &self,
        item_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ExecutionListingRow>, sqlx::Error> {
        let rows = match item_id {
            Some(item_id) => {
                sqlx::query("SELECT id, item_id, state, cancellation_requested_at, created_at FROM execution_requests WHERE item_id = ? ORDER BY created_at DESC LIMIT ?")
                    .bind(item_id)
                    .bind(limit)
                    .fetch_all(self.pool())
                    .await?
            }
            None => {
                sqlx::query("SELECT id, item_id, state, cancellation_requested_at, created_at FROM execution_requests ORDER BY created_at DESC LIMIT ?")
                    .bind(limit)
                    .fetch_all(self.pool())
                    .await?
            }
        };
        Ok(rows
            .into_iter()
            .map(|row| ExecutionListingRow {
                id: row.get("id"),
                item_id: row.get("item_id"),
                state: row.get("state"),
                cancellation_requested_at: row.get("cancellation_requested_at"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    /// Backs the `item_ids` batch shape of `GET /api/executions` — exactly
    /// one row per id in `item_ids` that has at least one execution
    /// request (an id with none is simply absent, never padded with a
    /// null row), carrying only its most recent request by `created_at`.
    /// A window function, not `GROUP BY item_id HAVING MAX(created_at)`:
    /// two requests for the same item can share a `created_at` value, and
    /// only `id DESC` as a tiebreak keeps the choice deterministic across
    /// repeated calls. `Ok(vec![])` for an empty `item_ids` rather than
    /// issuing a query with an empty `IN ()`, which is invalid SQL —
    /// matching [`Repository::list_dependencies_for_items`]'s guard.
    #[instrument(skip(self))]
    pub async fn list_latest_executions_for_items(
        &self,
        item_ids: &[String],
    ) -> Result<Vec<ExecutionListingRow>, sqlx::Error> {
        if item_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = item_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT id, item_id, state, cancellation_requested_at, created_at FROM ( \
                 SELECT id, item_id, state, cancellation_requested_at, created_at, \
                        ROW_NUMBER() OVER (PARTITION BY item_id ORDER BY created_at DESC, id DESC) AS rn \
                 FROM execution_requests WHERE item_id IN ({placeholders}) \
             ) WHERE rn = 1"
        );
        let mut q = sqlx::query(&query);
        for id in item_ids {
            q = q.bind(id);
        }
        let rows = q.fetch_all(self.pool()).await?;
        Ok(rows
            .into_iter()
            .map(|row| ExecutionListingRow {
                id: row.get("id"),
                item_id: row.get("item_id"),
                state: row.get("state"),
                cancellation_requested_at: row.get("cancellation_requested_at"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    /// Every attempt ever made against `request_id`, oldest first. Backs
    /// `GET /api/executions/{request_id}/attempts` — `execution_attempts`
    /// (migration 045) has been written by the runner-v1 protocol with no
    /// operator read path until now.
    pub async fn list_attempts_for_request(
        &self,
        request_id: &str,
    ) -> Result<Vec<AttemptListingRow>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, request_id, attempt_number, runner_id, fencing_token, state, \
             lease_issued_at, lease_expires_at, last_heartbeat_at, event_checkpoint, \
             completion_id, workspace_id, base_revision, actual_execution, terminal_reason, \
             usage, started_at, ended_at, created_at, updated_at FROM execution_attempts \
             WHERE request_id = ? ORDER BY attempt_number",
        )
        .bind(request_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| AttemptListingRow {
                id: row.get("id"),
                request_id: row.get("request_id"),
                attempt_number: row.get("attempt_number"),
                runner_id: row.get("runner_id"),
                fencing_token: row.get("fencing_token"),
                state: row.get("state"),
                lease_issued_at: row.get("lease_issued_at"),
                lease_expires_at: row.get("lease_expires_at"),
                last_heartbeat_at: row.get("last_heartbeat_at"),
                event_checkpoint: row.get("event_checkpoint"),
                completion_id: row.get("completion_id"),
                workspace_id: row.get("workspace_id"),
                base_revision: row.get("base_revision"),
                actual_execution: row.get("actual_execution"),
                terminal_reason: row.get("terminal_reason"),
                usage: row.get("usage"),
                started_at: row.get("started_at"),
                ended_at: row.get("ended_at"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect())
    }

    /// Every event recorded for one specific attempt (identified by its
    /// parent request id + 1-based attempt number, matching how
    /// `list_attempts_for_request` already reports attempts), oldest first.
    /// `Ok(None)` means no attempt with that number exists for this
    /// request — distinct from `Ok(Some(vec![]))`, an attempt that simply
    /// has not reported any events yet. Backs `GET
    /// /api/executions/{request_id}/attempts/{attempt_number}/events`.
    pub async fn list_events_for_attempt_number(
        &self,
        request_id: &str,
        attempt_number: i64,
    ) -> Result<Option<Vec<EventListingRow>>, sqlx::Error> {
        let attempt_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM execution_attempts WHERE request_id = ? AND attempt_number = ?",
        )
        .bind(request_id)
        .bind(attempt_number)
        .fetch_optional(self.pool())
        .await?;
        let Some(attempt_id) = attempt_id else {
            return Ok(None);
        };
        let rows = sqlx::query(
            "SELECT event_id, sequence, source, kind, payload, occurred_at, created_at \
             FROM execution_events WHERE attempt_id = ? ORDER BY sequence",
        )
        .bind(&attempt_id)
        .fetch_all(self.pool())
        .await?;
        Ok(Some(
            rows.into_iter()
                .map(|row| EventListingRow {
                    event_id: row.get("event_id"),
                    sequence: row.get("sequence"),
                    source: row.get("source"),
                    kind: row.get("kind"),
                    payload: row.get("payload"),
                    occurred_at: row.get("occurred_at"),
                    created_at: row.get("created_at"),
                })
                .collect(),
        ))
    }

    #[instrument(skip(self, input, clock))]
    pub async fn register_runner(
        &self,
        input: NewRunner<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<(), sqlx::Error> {
        let now = stamp(clock);
        sqlx::query(
            "INSERT INTO agent_runners (id, name, credential_hash, labels, total_capacity, \
             available_capacity, capability_snapshot, protocol_version, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(input.id)
        .bind(input.name)
        .bind(input.credential_hash)
        .bind(input.labels)
        .bind(input.total_capacity)
        .bind(input.available_capacity)
        .bind(input.capability_snapshot)
        .bind(input.protocol_version)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    #[instrument(skip(self, input, clock))]
    pub async fn create_agent_profile(
        &self,
        input: NewAgentProfile<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<(), sqlx::Error> {
        let now = stamp(clock);
        sqlx::query(
            "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(input.id).bind(input.name).bind(input.instructions)
        .bind(input.tool_policy).bind(input.limits).bind(&now).bind(&now)
        .execute(self.pool()).await?;
        Ok(())
    }

    #[instrument(skip(self, input, clock))]
    pub async fn enqueue_execution(
        &self,
        input: NewExecutionRequest<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<EnqueueResult, sqlx::Error> {
        let now = stamp(clock);
        let request_snapshot = validate_execution_request_snapshot(&input)?;
        // Serialize duplicate enqueue requests for the same idempotency key
        // (e.g. a client retrying an unacknowledged enqueue POST) before
        // either transaction reads the idempotency-scope row. Two deferred
        // readers can otherwise deadlock while both try to upgrade to
        // writers. Mirrors the redeem_enrollment_token and
        // claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let existing: Option<(String, String, String)> = sqlx::query_as(
            "SELECT id, request_fingerprint, request_snapshot FROM execution_requests \
             WHERE idempotency_scope = ? AND idempotency_key = ?",
        )
        .bind(input.idempotency_scope)
        .bind(input.idempotency_key)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((id, fingerprint, stored_snapshot)) = existing {
            tx.commit().await?;
            return Ok(
                if fingerprint == input.request_fingerprint && stored_snapshot == request_snapshot {
                    EnqueueResult::Replayed(id)
                } else {
                    EnqueueResult::Conflict
                },
            );
        }
        snapshot_created_at_matches_now(&request_snapshot, &now)?;
        sqlx::query(
            "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
             request_fingerprint, selector_kind, selector_id, agent_profile_id, agent_profile_snapshot, \
             requested_harness_kind, requested_model_provider, requested_model_id, repository_snapshot, \
             permission_policy, timeout_seconds, budgets, status_map_policy_id, environment, metadata, request_snapshot, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(input.id).bind(input.item_id).bind(input.idempotency_scope).bind(input.idempotency_key)
        .bind(input.request_fingerprint).bind(input.selector_kind).bind(input.selector_id)
        .bind(input.agent_profile_id).bind(input.agent_profile_snapshot).bind(input.requested_harness_kind)
        .bind(input.requested_model_provider).bind(input.requested_model_id).bind(input.repository_snapshot)
        .bind(input.permission_policy).bind(input.timeout_seconds).bind(input.budgets)
        .bind(input.status_map_policy_id).bind(input.environment).bind(input.metadata).bind(request_snapshot).bind(&now).bind(&now)
        .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(EnqueueResult::Created(input.id.to_string()))
    }

    #[instrument(skip(self, events, clock))]
    pub async fn append_execution_events_result(
        &self,
        batch: EventBatch<'_>,
        events: &[NewEvent<'_>],
        clock: &dyn ExecutionClock,
    ) -> Result<EventApplyResult, sqlx::Error> {
        let now = stamp(clock);
        let fingerprint = event_batch_fingerprint(&batch, events)?;
        // Serialize duplicate/replayed event batches for the same attempt
        // (e.g. a runner retrying an unacknowledged event-batch POST) before
        // either transaction reads this attempt row. Two deferred readers
        // can otherwise deadlock while both try to upgrade to writers.
        // Mirrors the redeem_enrollment_token and
        // claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let row = sqlx::query("SELECT event_checkpoint FROM execution_attempts WHERE id=? AND runner_id=? AND fencing_token=? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at>?")
            .bind(batch.attempt_id).bind(batch.runner_id).bind(batch.fencing_token).bind(&now).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(EventApplyResult::Stale);
        };
        if let Some(row) = sqlx::query(
            "SELECT fingerprint,response FROM execution_event_batch_replays WHERE attempt_id=? AND checkpoint=?",
        )
        .bind(batch.attempt_id)
        .bind(batch.checkpoint)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored: String = row.get("fingerprint");
            if stored != fingerprint {
                // Same (attempt_id, checkpoint) idempotency-scoped key,
                // different content: this can never succeed by retrying.
                tx.commit().await?;
                return Ok(EventApplyResult::IdempotencyConflict);
            }
            let response: String = row.get("response");
            let value: serde_json::Value = serde_json::from_str(&response)
                .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
            let accepted = value["accepted_event_ids"]
                .as_array()
                .ok_or_else(|| sqlx::Error::Protocol("invalid event replay response".into()))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| sqlx::Error::Protocol("invalid event replay id".into()))
                        .map(str::to_owned)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let duplicate = value["duplicate_event_ids"]
                .as_array()
                .ok_or_else(|| sqlx::Error::Protocol("invalid event replay response".into()))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| sqlx::Error::Protocol("invalid event replay id".into()))
                        .map(str::to_owned)
                })
                .collect::<Result<Vec<_>, _>>()?;
            tx.commit().await?;
            return Ok(EventApplyResult::Applied(EventBatchResult {
                accepted_event_ids: accepted,
                duplicate_event_ids: duplicate,
                committed_checkpoint: batch.checkpoint.into(),
                replayed: true,
            }));
        }
        let current: Option<String> = row.get("event_checkpoint");
        if current.as_deref() == Some(batch.checkpoint) {
            // Legacy pre-054 attempt whose checkpoint already advanced but has
            // no replay row: no fingerprint to compare, so this is not proven
            // to be a reused key with different content. Benign/retryable.
            tx.commit().await?;
            return Ok(EventApplyResult::Conflict);
        }
        if current.as_deref() != batch.previous_checkpoint {
            // The batch's claimed previous_checkpoint no longer matches the
            // attempt's actual stream position: a benign out-of-order resync,
            // not an idempotency-key reuse. Retryable once the caller re-syncs.
            tx.commit().await?;
            return Ok(EventApplyResult::Conflict);
        }
        let mut accepted = Vec::new();
        let mut duplicate = Vec::new();
        for event in events {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM execution_events WHERE attempt_id=? AND event_id=?)",
            )
            .bind(batch.attempt_id)
            .bind(event.event_id)
            .fetch_one(&mut *tx)
            .await?;
            if exists {
                duplicate.push(event.event_id.into())
            } else {
                sqlx::query("INSERT INTO execution_events(id,attempt_id,event_id,sequence,source,kind,payload,occurred_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(event.id).bind(batch.attempt_id).bind(event.event_id).bind(event.sequence).bind(event.source).bind(event.kind).bind(event.payload).bind(event.occurred_at.to_rfc3339()).bind(&now).execute(&mut *tx).await?;
                accepted.push(event.event_id.into())
            }
        }
        let changed=sqlx::query("UPDATE execution_attempts SET event_checkpoint=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND event_checkpoint IS ?").bind(batch.checkpoint).bind(&now).bind(batch.attempt_id).bind(batch.runner_id).bind(batch.fencing_token).bind(batch.previous_checkpoint).execute(&mut *tx).await?;
        if changed.rows_affected() != 1 {
            // Defensive: near-unreachable under the BEGIN IMMEDIATE this
            // function already opens, but a lost compare-and-set is a
            // benign/retryable Conflict, not an idempotency-key reuse.
            tx.rollback().await?;
            return Ok(EventApplyResult::Conflict);
        }
        let result = EventBatchResult {
            accepted_event_ids: accepted,
            duplicate_event_ids: duplicate,
            committed_checkpoint: batch.checkpoint.into(),
            replayed: false,
        };
        let response=serde_json::json!({"accepted_event_ids":result.accepted_event_ids,"duplicate_event_ids":result.duplicate_event_ids}).to_string();
        sqlx::query("INSERT INTO execution_event_batch_replays(attempt_id,checkpoint,fingerprint,response,created_at) VALUES(?,?,?,?,?)").bind(batch.attempt_id).bind(batch.checkpoint).bind(fingerprint).bind(response).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(EventApplyResult::Applied(result))
    }

    #[instrument(skip(self, clock))]
    pub async fn complete_execution(
        &self,
        completion: Completion<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        Ok(matches!(
            self.complete_execution_result(completion, clock).await?,
            CompletionResult::Committed(_) | CompletionResult::Replayed(_)
        ))
    }

    #[instrument(skip(self, clock))]
    pub async fn request_execution_cancellation(
        &self,
        request_id: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        let result = sqlx::query("UPDATE execution_requests SET cancellation_requested_at = COALESCE(cancellation_requested_at, ?), updated_at = ? WHERE id = ? AND state NOT IN ('succeeded','failed','cancelled')")
            .bind(&now).bind(&now).bind(request_id).execute(self.pool()).await?;
        Ok(result.rows_affected() == 1)
    }

    #[instrument(skip(self, artifact, clock))]
    pub async fn record_execution_artifact(
        &self,
        runner_id: &str,
        attempt_id: &str,
        fence: i64,
        artifact: NewArtifact<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        // Serialize the eligibility check and the insert against a concurrent
        // completion, cancellation, or recovery transition for the same
        // attempt (all of which already open BEGIN IMMEDIATE). Without this,
        // the SELECT and INSERT were two separate un-transacted statements: a
        // concurrent terminal transition could commit between them, letting
        // an artifact land against an attempt that has already gone
        // terminal, lost, or needs_operator.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_attempts WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state NOT IN ('succeeded','failed','cancelled') AND lease_expires_at > ?)")
            .bind(attempt_id).bind(runner_id).bind(fence).bind(&now).fetch_one(&mut *tx).await?;
        if !valid {
            tx.commit().await?;
            return Ok(false);
        }
        sqlx::query("INSERT INTO execution_artifacts (id, attempt_id, artifact_id, kind, name, media_type, size_bytes, sha256, content_disposition, content_reference, metadata, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(attempt_id, artifact_id) DO NOTHING")
            .bind(artifact.id).bind(attempt_id).bind(artifact.artifact_id).bind(artifact.kind).bind(artifact.name).bind(artifact.media_type).bind(artifact.size_bytes).bind(artifact.sha256).bind(artifact.content_disposition).bind(artifact.content_reference).bind(artifact.metadata).bind(now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Looks up one manifest row by its natural key. `Ok(None)`
    /// means no such artifact was ever manifested for this attempt —
    /// distinct from a manifested-but-not-yet-content-verified row (which is
    /// `Some` with `content_reference: None`).
    #[instrument(skip(self))]
    pub async fn get_execution_artifact(
        &self,
        attempt_id: &str,
        artifact_id: &str,
    ) -> Result<Option<ExecutionArtifactRow>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, attempt_id, artifact_id, kind, name, media_type, size_bytes, sha256, \
             content_disposition, content_reference, metadata, created_at \
             FROM execution_artifacts WHERE attempt_id = ? AND artifact_id = ?",
        )
        .bind(attempt_id)
        .bind(artifact_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map(|row| ExecutionArtifactRow {
            id: row.get("id"),
            attempt_id: row.get("attempt_id"),
            artifact_id: row.get("artifact_id"),
            kind: row.get("kind"),
            name: row.get("name"),
            media_type: row.get("media_type"),
            size_bytes: row.get("size_bytes"),
            sha256: row.get("sha256"),
            content_disposition: row.get("content_disposition"),
            content_reference: row.get("content_reference"),
            metadata: row.get("metadata"),
            created_at: row.get("created_at"),
        }))
    }

    /// Resolves an artifact the same way
    /// [`Repository::list_events_for_attempt_number`] resolves events — by
    /// the operator-facing `(request_id, attempt_number)` pair rather than
    /// the internal opaque `attempt_id` — so `GET
    /// /api/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content`
    /// never needs
    /// to expose `attempt_id` to an operator caller. `Ok(None)` collapses
    /// "no such attempt" and "no such artifact on that attempt" into one
    /// not-found outcome, matching every other operator lookup in this file.
    #[instrument(skip(self))]
    pub async fn get_execution_artifact_by_attempt_number(
        &self,
        request_id: &str,
        attempt_number: i64,
        artifact_id: &str,
    ) -> Result<Option<ExecutionArtifactRow>, sqlx::Error> {
        let attempt_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM execution_attempts WHERE request_id = ? AND attempt_number = ?",
        )
        .bind(request_id)
        .bind(attempt_number)
        .fetch_optional(self.pool())
        .await?;
        let Some(attempt_id) = attempt_id else {
            return Ok(None);
        };
        self.get_execution_artifact(&attempt_id, artifact_id).await
    }

    /// Every artifact manifested for one specific attempt (identified by its
    /// parent request id + 1-based attempt number, matching how
    /// [`Repository::list_events_for_attempt_number`] resolves the same
    /// pair), oldest first. `Ok(None)` means no attempt with that number
    /// exists for this request — distinct from `Ok(Some(vec![]))`, an
    /// attempt that has manifested no artifacts yet. Backs `GET
    /// /api/executions/{request_id}/attempts/{attempt_number}/artifacts`.
    #[instrument(skip(self))]
    pub async fn list_execution_artifacts_for_attempt_number(
        &self,
        request_id: &str,
        attempt_number: i64,
    ) -> Result<Option<Vec<ExecutionArtifactRow>>, sqlx::Error> {
        let attempt_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM execution_attempts WHERE request_id = ? AND attempt_number = ?",
        )
        .bind(request_id)
        .bind(attempt_number)
        .fetch_optional(self.pool())
        .await?;
        let Some(attempt_id) = attempt_id else {
            return Ok(None);
        };
        let rows = sqlx::query(
            "SELECT id, attempt_id, artifact_id, kind, name, media_type, size_bytes, sha256, \
             content_disposition, content_reference, metadata, created_at \
             FROM execution_artifacts WHERE attempt_id = ? ORDER BY created_at",
        )
        .bind(&attempt_id)
        .fetch_all(self.pool())
        .await?;
        Ok(Some(
            rows.into_iter()
                .map(|row| ExecutionArtifactRow {
                    id: row.get("id"),
                    attempt_id: row.get("attempt_id"),
                    artifact_id: row.get("artifact_id"),
                    kind: row.get("kind"),
                    name: row.get("name"),
                    media_type: row.get("media_type"),
                    size_bytes: row.get("size_bytes"),
                    sha256: row.get("sha256"),
                    content_disposition: row.get("content_disposition"),
                    content_reference: row.get("content_reference"),
                    metadata: row.get("metadata"),
                    created_at: row.get("created_at"),
                })
                .collect(),
        ))
    }

    /// Commits a verified artifact's storage reference. Mirrors
    /// `record_execution_artifact`'s own `BEGIN IMMEDIATE` eligibility
    /// pattern (CLAUDE.md: read-then-write requires `BEGIN IMMEDIATE`) so a
    /// concurrent terminal transition cannot land a content reference
    /// against an attempt that has already gone terminal, lost, or
    /// needs_operator between the caller's own fencing check (at manifest
    /// time) and this call (after the — potentially long — upload finished
    /// streaming). `content_reference IS NULL` in the `UPDATE`'s `WHERE`
    /// makes a committed reference immutable: a second call for the same
    /// `(attempt_id, artifact_id)` — even with byte-identical content —
    /// reports `AlreadySet` rather than silently re-writing, so "checksum
    /// mismatch stages nothing" can never be defeated by racing two uploads
    /// for the same artifact_id.
    #[instrument(skip(self, clock))]
    pub async fn set_execution_artifact_content_reference(
        &self,
        runner_id: &str,
        attempt_id: &str,
        artifact_id: &str,
        fence: i64,
        content_reference: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<ArtifactContentCommitResult, sqlx::Error> {
        let now = stamp(clock);
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_attempts WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state NOT IN ('succeeded','failed','cancelled') AND lease_expires_at > ?)")
            .bind(attempt_id).bind(runner_id).bind(fence).bind(&now).fetch_one(&mut *tx).await?;
        if !valid {
            tx.commit().await?;
            return Ok(ArtifactContentCommitResult::Stale);
        }
        let changed = sqlx::query(
            "UPDATE execution_artifacts SET content_reference = ? WHERE attempt_id = ? AND artifact_id = ? AND content_reference IS NULL",
        )
        .bind(content_reference)
        .bind(attempt_id)
        .bind(artifact_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        if changed.rows_affected() == 1 {
            Ok(ArtifactContentCommitResult::Committed)
        } else {
            Ok(ArtifactContentCommitResult::AlreadySet)
        }
    }

    /// Retention (behavior only — startup/shutdown wiring, metrics and
    /// alerting are the recurring background task's own concern). Bounded batch:
    /// the subquery `LIMIT` keeps one sweep pass cheap and interruptible
    /// rather than locking the table for an unbounded delete; the caller
    /// loops until `0` is returned. Deletes purely by `created_at` age, the
    /// same column `limits.json`'s `retention_event_days_default` is
    /// documented against.
    #[instrument(skip(self))]
    pub async fn purge_execution_events_older_than(
        &self,
        cutoff: DateTime<Utc>,
        batch_limit: i64,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM execution_events WHERE id IN (SELECT id FROM execution_events WHERE created_at < ? LIMIT ?)",
        )
        .bind(cutoff.to_rfc3339())
        .bind(batch_limit)
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// The read half of the two-phase artifact sweep — the
    /// blob referenced by `content_reference` must be unlinked from disk
    /// (a filesystem concern this crate does not own) *before* the row
    /// naming it disappears, so the caller fetches full rows here, deletes
    /// each blob it can reach, and only then calls
    /// [`Repository::delete_execution_artifacts_by_row_ids`]. A manifest row
    /// with no verified content (`content_reference: None`) is included —
    /// there is no blob to unlink, but the manifest row itself is still
    /// subject to the same age-based retention.
    #[instrument(skip(self))]
    pub async fn list_execution_artifacts_older_than(
        &self,
        cutoff: DateTime<Utc>,
        batch_limit: i64,
    ) -> Result<Vec<ExecutionArtifactRow>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, attempt_id, artifact_id, kind, name, media_type, size_bytes, sha256, \
             content_disposition, content_reference, metadata, created_at \
             FROM execution_artifacts WHERE created_at < ? LIMIT ?",
        )
        .bind(cutoff.to_rfc3339())
        .bind(batch_limit)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| ExecutionArtifactRow {
                id: row.get("id"),
                attempt_id: row.get("attempt_id"),
                artifact_id: row.get("artifact_id"),
                kind: row.get("kind"),
                name: row.get("name"),
                media_type: row.get("media_type"),
                size_bytes: row.get("size_bytes"),
                sha256: row.get("sha256"),
                content_disposition: row.get("content_disposition"),
                content_reference: row.get("content_reference"),
                metadata: row.get("metadata"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    /// The write half — deletes exactly the rows named by
    /// `ids` (each `execution_artifacts.id`, not `artifact_id`), so a
    /// caller that already unlinked their blobs cannot accidentally sweep a
    /// row it never inspected.
    #[instrument(skip(self, ids))]
    pub async fn delete_execution_artifacts_by_row_ids(
        &self,
        ids: &[String],
    ) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut query = String::from("DELETE FROM execution_artifacts WHERE id IN (");
        for (index, _) in ids.iter().enumerate() {
            if index > 0 {
                query.push(',');
            }
            query.push('?');
        }
        query.push(')');
        let mut built = sqlx::query(&query);
        for id in ids {
            built = built.bind(id);
        }
        let result = built.execute(self.pool()).await?;
        Ok(result.rows_affected())
    }

    /// Conditional counterpart to
    /// [`Repository::delete_execution_artifacts_by_row_ids`], scoped to rows
    /// whose `content_reference` is *still* `NULL` — i.e. rows the caller
    /// observed at list-time (`list_execution_artifacts_older_than`) as
    /// having no blob to unlink.
    ///
    /// # Why this exists: the race the plain by-id delete cannot see
    ///
    /// `set_execution_artifact_content_reference`'s own `UPDATE` is guarded
    /// `WHERE content_reference IS NULL`, so once a row's reference is set it
    /// can never change again — a row observed with `Some(reference)` at
    /// list-time is safe to delete unconditionally by id (this crate's
    /// existing method already does that correctly). But a row observed with
    /// `None` can race: a runner's real content upload
    /// (`put_artifact_content` → `store_streaming` then this same
    /// `set_execution_artifact_content_reference` call) can land *between*
    /// the sweep's read and its delete, writing a real blob to disk and
    /// setting `content_reference` — and an unconditional
    /// `delete_execution_artifacts_by_row_ids` would still delete that row
    /// despite the fresh blob, permanently orphaning it (nothing will ever
    /// reference it again once the row is gone). This method closes that
    /// window to the width of one atomic SQL statement (no separate read
    /// step here to race against) by re-checking `content_reference IS NULL`
    /// as part of the same `DELETE`: a row that raced past `NULL` in the
    /// meantime simply is not matched and survives this pass, to be picked
    /// up correctly (blob removed, then deleted) on the next one, once its
    /// `content_reference` is visible as `Some` to a fresh
    /// `list_execution_artifacts_older_than` read.
    ///
    /// See `handlers/runner_protocol/retention.rs::sweep_artifacts` (the
    /// only caller) for how the two delete methods are split across a listed
    /// batch, and `crates/tack-db/tests/repository/event_artifact_retention.rs`
    /// for the deterministic proof of both this guard's effect and what the
    /// unconditional method would have done to the same racing row.
    #[instrument(skip(self, ids))]
    pub async fn delete_unresolved_execution_artifacts_by_row_ids(
        &self,
        ids: &[String],
    ) -> Result<u64, sqlx::Error> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut query = String::from(
            "DELETE FROM execution_artifacts WHERE content_reference IS NULL AND id IN (",
        );
        for (index, _) in ids.iter().enumerate() {
            if index > 0 {
                query.push(',');
            }
            query.push('?');
        }
        query.push(')');
        let mut built = sqlx::query(&query);
        for id in ids {
            built = built.bind(id);
        }
        let result = built.execute(self.pool()).await?;
        Ok(result.rows_affected())
    }

    #[instrument(skip(self, decision, clock))]
    pub async fn create_execution_decision(
        &self,
        runner_id: &str,
        attempt_id: &str,
        fence: i64,
        decision: NewDecision<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        // Serialize the eligibility check and the insert against a concurrent
        // completion, cancellation, or recovery transition for the same
        // attempt (all of which already open BEGIN IMMEDIATE). Without this,
        // the SELECT and INSERT were two separate un-transacted statements: a
        // concurrent terminal transition could commit between them, letting
        // a decision land against an attempt that has already gone terminal,
        // lost, or needs_operator.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_attempts WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state IN ('running','waiting_decision') AND lease_expires_at > ?)")
            .bind(attempt_id).bind(runner_id).bind(fence).bind(&now).fetch_one(&mut *tx).await?;
        if !valid {
            tx.commit().await?;
            return Ok(false);
        }
        sqlx::query("INSERT INTO execution_decisions (id, attempt_id, decision_id, kind, prompt, options, metadata, expires_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(attempt_id, decision_id) DO NOTHING")
        .bind(decision.id).bind(attempt_id).bind(decision.decision_id).bind(decision.kind).bind(decision.prompt).bind(decision.options).bind(decision.metadata).bind(decision.expires_at.map(|v| v.to_rfc3339())).bind(&now).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Every decision raised against one specific attempt (identified by its
    /// parent request id + 1-based attempt number, matching how
    /// [`Repository::list_events_for_attempt_number`] resolves the same
    /// pair), oldest first. `Ok(None)` means no attempt with that number
    /// exists for this request — distinct from `Ok(Some(vec![]))`, an
    /// attempt that has raised no decisions yet. Backs `GET
    /// /api/executions/{request_id}/attempts/{attempt_number}/decisions`.
    /// Read-only and reachable from the operator surface — distinct from
    /// [`Self::create_execution_decision`] (runner-credential-only) and
    /// decision *resolution* (`handlers::decisions`, gated separately behind
    /// `TACK_EXECUTION_DECISION_TOKEN`).
    #[instrument(skip(self))]
    pub async fn list_execution_decisions_for_attempt_number(
        &self,
        request_id: &str,
        attempt_number: i64,
    ) -> Result<Option<Vec<ExecutionDecisionRow>>, sqlx::Error> {
        let attempt_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM execution_attempts WHERE request_id = ? AND attempt_number = ?",
        )
        .bind(request_id)
        .bind(attempt_number)
        .fetch_optional(self.pool())
        .await?;
        let Some(attempt_id) = attempt_id else {
            return Ok(None);
        };
        let rows = sqlx::query(
            "SELECT id, attempt_id, decision_id, kind, state, prompt, options, metadata, \
             answer, expires_at, resolved_at, resolved_by, created_at, updated_at \
             FROM execution_decisions WHERE attempt_id = ? ORDER BY created_at",
        )
        .bind(&attempt_id)
        .fetch_all(self.pool())
        .await?;
        Ok(Some(
            rows.into_iter()
                .map(|row| ExecutionDecisionRow {
                    id: row.get("id"),
                    attempt_id: row.get("attempt_id"),
                    decision_id: row.get("decision_id"),
                    kind: row.get("kind"),
                    state: row.get("state"),
                    prompt: row.get("prompt"),
                    options: row.get("options"),
                    metadata: row.get("metadata"),
                    answer: row.get("answer"),
                    expires_at: row.get("expires_at"),
                    resolved_at: row.get("resolved_at"),
                    resolved_by: row.get("resolved_by"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                })
                .collect(),
        ))
    }

    // ─── Runtime retention and observability ───────────────────────────────

    /// Deletes stale rows from the six idempotency/replay bookkeeping tables
    /// (`execution_claim_replays`, `execution_heartbeat_replays`,
    /// `execution_cancellation_replays`, `execution_event_batch_replays`,
    /// `execution_completion_replays`, `execution_recovery_audits`), batched
    /// at up to `batch_size` rows per table per transaction, looping per
    /// table until nothing older than `cutoff` remains.
    ///
    /// **Purge, not roll-up.** Every row in these six tables exists solely
    /// to answer "have I already processed this exact retried write?" for a
    /// fencing/lease/heartbeat replay window measured in seconds to low
    /// minutes. Once `cutoff` (typically 90 days out) has passed,
    /// there is no future question these rows could ever answer — unlike
    /// `execution_events` (see [`Self::purge_stale_terminal_execution_events`]),
    /// there is no meaningful aggregate to preserve first, so plain deletion
    /// loses nothing of value. Read "purge" here literally, not as a
    /// synonym for "roll up."
    ///
    /// **`BEGIN IMMEDIATE`, not a deferred transaction:** every one of these
    /// tables is also written concurrently by live runner-protocol traffic
    /// (claim/heartbeat/event-batch/completion/cancellation/recovery each
    /// insert into one of these six tables on every call). A deferred
    /// transaction that `SELECT`s candidate rows and only later `DELETE`s
    /// them is exactly the read-then-write shape CLAUDE.md calls out: two
    /// concurrent deferred transactions can both acquire a shared read lock
    /// and then race to upgrade to a write lock, and SQLite returns
    /// `SQLITE_LOCKED` rather than queuing one behind the other.
    /// `BEGIN IMMEDIATE` takes the write lock up front instead, serializing
    /// this sweep against every other writer to these tables rather than
    /// deadlocking against them. Proved load-bearing (reverted, watched the
    /// concurrency test fail, restored) in
    /// `crates/tack-db/tests/repository/execution_retention.rs`.
    #[instrument(skip(self))]
    pub async fn purge_stale_execution_replays(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<PurgeStats, sqlx::Error> {
        const TABLES: [(&str, &str); 6] = [
            ("execution_claim_replays", "created_at"),
            ("execution_heartbeat_replays", "created_at"),
            ("execution_cancellation_replays", "created_at"),
            ("execution_event_batch_replays", "created_at"),
            ("execution_completion_replays", "committed_at"),
            ("execution_recovery_audits", "created_at"),
        ];
        let cutoff_str = cutoff.to_rfc3339();
        let mut stats = PurgeStats::default();

        for (table, ts_col) in TABLES {
            loop {
                let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;

                let select_sql = format!(
                    "SELECT rowid FROM {table} WHERE {ts_col} < ? ORDER BY {ts_col} ASC LIMIT ?"
                );
                let ids: Vec<i64> = sqlx::query_scalar(&select_sql)
                    .bind(&cutoff_str)
                    .bind(batch_size)
                    .fetch_all(&mut *tx)
                    .await?;

                if ids.is_empty() {
                    tx.commit().await?;
                    break;
                }

                let placeholders = vec!["?"; ids.len()].join(",");
                let delete_sql = format!("DELETE FROM {table} WHERE rowid IN ({placeholders})");
                let mut q = sqlx::query(&delete_sql);
                for id in &ids {
                    q = q.bind(id);
                }
                q.execute(&mut *tx).await?;

                tx.commit().await?;

                let batch_len = ids.len() as i64;
                stats.rows_purged += batch_len;
                stats.batches_run += 1;

                if batch_len < batch_size {
                    break;
                }
            }
        }

        Ok(stats)
    }

    /// Deletes `execution_events` rows once both (a) `occurred_at < cutoff`
    /// and (b) the owning attempt has reached a genuinely terminal state
    /// (`succeeded`/`failed`/`cancelled` — the same three states
    /// `tack_orch::execution::ExecutionState::is_terminal()` names).
    /// Deliberately excludes `lost`/`needs_operator`: both remain
    /// actionable/ambiguous and are themselves
    /// observability targets ([`Self::execution_fleet_snapshot`]) — an
    /// attempt an operator might still requeue or investigate must never
    /// have its event history silently swept out from under it.
    ///
    /// **Purge only — not a roll-up.** No `execution_events_daily` (or
    /// equivalent) aggregate table exists in this schema today. Adding one
    /// would mirror
    /// `orch_events`/`orch_events_daily`
    /// (`crates/tack-db/src/repo/orch.rs::rollup_and_purge_orch_events`).
    /// Until that migration lands, this purges raw rows outright — no
    /// day/kind/count aggregate survives; that information is only
    /// recoverable by adding the rollup table before this method ever runs
    /// against a given row. This is an explicit, documented trade, not a
    /// silent one — no structural zero, no fake success:
    /// this repo's `Repository` API has exactly one method for this table
    /// and its own doc comment says exactly what it does.
    ///
    /// Same `BEGIN IMMEDIATE` batching rationale as
    /// [`Self::purge_stale_execution_replays`] — `execution_events` receives
    /// live inserts from every in-flight attempt's event-batch reports.
    #[instrument(skip(self))]
    pub async fn purge_stale_terminal_execution_events(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<PurgeStats, sqlx::Error> {
        let cutoff_str = cutoff.to_rfc3339();
        let mut stats = PurgeStats::default();

        loop {
            let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;

            let ids: Vec<String> = sqlx::query_scalar(
                "SELECT ee.id FROM execution_events ee \
                 JOIN execution_attempts ea ON ea.id = ee.attempt_id \
                 WHERE ee.occurred_at < ? AND ea.state IN ('succeeded','failed','cancelled') \
                 ORDER BY ee.occurred_at ASC LIMIT ?",
            )
            .bind(&cutoff_str)
            .bind(batch_size)
            .fetch_all(&mut *tx)
            .await?;

            if ids.is_empty() {
                tx.commit().await?;
                break;
            }

            let placeholders = vec!["?"; ids.len()].join(",");
            let delete_sql = format!("DELETE FROM execution_events WHERE id IN ({placeholders})");
            let mut q = sqlx::query(&delete_sql);
            for id in &ids {
                q = q.bind(id);
            }
            q.execute(&mut *tx).await?;

            tx.commit().await?;

            let batch_len = ids.len() as i64;
            stats.rows_purged += batch_len;
            stats.batches_run += 1;

            if batch_len < batch_size {
                break;
            }
        }

        Ok(stats)
    }

    /// See [`ExecutionFleetSnapshotRow`]'s own doc comment for the shape and
    /// the cardinality guarantee. Independent read-only queries — nothing
    /// here writes, so no transaction is needed; a snapshot that is
    /// eventually-consistent across its own several reads by a few
    /// milliseconds is an acceptable trade for an observability signal that
    /// is only ever logged/alerted on, never used to gate a safety decision.
    #[instrument(skip(self))]
    pub async fn execution_fleet_snapshot(
        &self,
        now: DateTime<Utc>,
        event_window: Duration,
    ) -> Result<ExecutionFleetSnapshotRow, sqlx::Error> {
        let now_s = now.to_rfc3339();

        let runner_rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT state, COUNT(*) FROM agent_runners GROUP BY state")
                .fetch_all(self.pool())
                .await?;
        let runner_state_counts = runner_rows.into_iter().collect();

        let request_rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT state, COUNT(*) FROM execution_requests GROUP BY state")
                .fetch_all(self.pool())
                .await?;
        let request_state_counts = request_rows.into_iter().collect();

        let stale_lease_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM execution_attempts WHERE lease_expires_at < ? \
             AND state IN ('leased','preparing','running','waiting_decision')",
        )
        .bind(&now_s)
        .fetch_one(self.pool())
        .await?;
        let oldest_stale_lease_expires_at: Option<String> = sqlx::query_scalar(
            "SELECT lease_expires_at FROM execution_attempts WHERE lease_expires_at < ? \
             AND state IN ('leased','preparing','running','waiting_decision') \
             ORDER BY lease_expires_at ASC LIMIT 1",
        )
        .bind(&now_s)
        .fetch_optional(self.pool())
        .await?;
        let oldest_stale_lease_age_secs = oldest_stale_lease_expires_at
            .and_then(|s| parse_rfc3339_checked(&s))
            .map(|expired_at| (now - expired_at).num_seconds().max(0));

        let needs_operator_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM execution_requests WHERE state = 'needs_operator'",
        )
        .fetch_one(self.pool())
        .await?;
        let oldest_needs_operator_updated_at: Option<String> = sqlx::query_scalar(
            "SELECT updated_at FROM execution_requests WHERE state = 'needs_operator' \
             ORDER BY updated_at ASC LIMIT 1",
        )
        .fetch_optional(self.pool())
        .await?;
        let oldest_needs_operator_age_secs = oldest_needs_operator_updated_at
            .and_then(|s| parse_rfc3339_checked(&s))
            .map(|since| (now - since).num_seconds().max(0));

        let window_start = (now - event_window).to_rfc3339();
        let events_ingested_in_window: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM execution_events WHERE occurred_at >= ? AND occurred_at <= ?",
        )
        .bind(&window_start)
        .bind(&now_s)
        .fetch_one(self.pool())
        .await?;

        Ok(ExecutionFleetSnapshotRow {
            runner_state_counts,
            request_state_counts,
            stale_lease_count,
            oldest_stale_lease_age_secs,
            needs_operator_count,
            oldest_needs_operator_age_secs,
            events_ingested_in_window,
        })
    }
}
