//! Local pull-runner engine.
//!
//! The journal is persisted before `HarnessAdapter::start`. Once a local
//! process might exist, any failed fence/report/recovery operation is treated
//! as ambiguous and quarantined rather than retried.

use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;
use tack_orch::execution::{
    AttemptId as DomainAttemptId, FencingToken as DomainFencingToken, ProtocolVersion,
    RecoveryDetails as DomainRecoveryDetails, RecoveryDisposition, RecoveryJournalState,
    RecoveryKey, RecoveryObservation, RecoveryObservationRequest, RecoveryObservationResponse,
    RunnerId as DomainRunnerId,
};
use thiserror::Error;

use super::{
    ActiveAttempt, ArtifactManifestItem, ArtifactManifestReport, AttemptDataProtocol, AttemptId,
    AttemptState, CancellationReport, CancellationRequestId, Checkpoint, ClaimRequest, ClaimResult,
    ClaimedWork, ClaimedWorkError, CompletionId, CompletionReport, CompletionResponse,
    EnrollmentRequest, EnrollmentResponse, EventBatchReport, HeartbeatRequest,
    PendingTerminalReport, PendingTerminalReportKind, ProtocolClientError, ProtocolEvent,
    PullProtocol, RefreshRequest, RefreshResponse, RunnerSession, StartPhase, StartReport,
    Timestamp,
    journal::{AttemptJournal, JournalError, JournalState, OwnerOnlyJournal},
    workspace::{Workspace, WorkspaceError, WorkspaceManager, WorktreeProvisioner},
};

/// How often [`RunnerEngine::wait_with_lease_renewal`] re-heartbeats a
/// still-running attempt. Must stay comfortably under the server's
/// lease-grant window (`request_timeout_seconds_max` allows harness runs far
/// longer than any single lease) so a heartbeat lands well before the
/// previous one's grant would expire, including a missed cycle or two under
/// transient network trouble.
const LEASE_RENEWAL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(20);

/// `Rejected` carries a `reason`.
///
/// Two harness adapters independently hit the same gap: `validate`/`start`
/// have several genuinely
/// distinct pre-spawn rejection reasons (wrong harness kind, an
/// auto-selected model this adapter cannot honestly confirm, an unresolvable
/// binary, an unsupported provider, a provider/model pairing claude-code itself
/// does not offer, ...) that all collapsed to the same bare `Rejected` at
/// this trait boundary. Both worked around it with a `tracing::warn!`
/// immediately before returning the error — which means the reason reached
/// a log line, never the caller or the operator who actually needs it to
/// decide what to do next. This is the smallest fix that carries the reason
/// across the boundary itself: a plain `String`, not a new taxonomy of
/// typed sub-variants — `HarnessError` is a closed,
/// deliberately small enum; widening it to a fourth *kind* of error was
/// evaluated and rejected for the same reason. `Process`/`RecoveryUnavailable`
/// are untouched: nothing reported an analogous need for them.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HarnessError {
    #[error("harness rejected this execution: {reason}")]
    Rejected { reason: String },
    #[error("harness process operation failed")]
    Process,
    #[error("harness recovery observation is unavailable")]
    RecoveryUnavailable,
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    Protocol(#[from] ProtocolClientError),
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error(transparent)]
    Claim(#[from] ClaimedWorkError),
    #[error(transparent)]
    Harness(#[from] HarnessError),
    #[error("claimed lease belongs to another runner")]
    RunnerMismatch,
    #[error("harness outcome is not terminal")]
    NonTerminalOutcome,
    #[error("terminal report could not be encoded as canonical JSON")]
    TerminalReportSerialization,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRunHandle {
    pub process_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelObservation {
    ProcessStopped,
    AlreadyTerminal,
    Ambiguous,
}

/// Typed, non-secret evidence supplied by the harness after a cancellation
/// attempt. The adapter owns the observation time and details; the engine only
/// carries them to the cancellation transport unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct CancellationEvidence {
    pub observation: CancelObservation,
    pub observed_at: Timestamp,
    pub details: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HarnessOutcome {
    pub terminal_state: AttemptState,
    pub terminal_reason: serde_json::Value,
    pub final_checkpoint: Option<super::Checkpoint>,
    pub actual_execution: tack_orch::execution::ActualExecution,
    pub usage: tack_orch::execution::Usage,
}

impl HarnessOutcome {
    /// Workspace ownership belongs to the engine, not a harness adapter. Normalize
    /// the adapter outcome before it is projected into the transport report so the
    /// report has exactly one source for actual execution and usage facts.
    fn normalize_workspace_facts(mut self, workspace: &Workspace) -> Self {
        self.actual_execution.workspace_id =
            tack_orch::execution::WorkspaceId::new(workspace.id.as_str());
        self.actual_execution.base_revision = workspace.base_revision.clone();
        self
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionSpec {
    pub work: ClaimedWork,
    pub workspace: Workspace,
}

/// Shared local process seam. It has no implementation in C3; C3 tests use a
/// fake adapter and later harness cards implement this contract in their own
/// files without changing engine ownership.
#[async_trait]
pub trait HarnessAdapter: Send + Sync {
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError>;
    async fn start(&self, spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError>;
    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError>;
    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError>;
    async fn reconcile(
        &self,
        journal: &AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunCycle {
    NoWork,
    Completed { attempt_id: AttemptId },
    Cancelled { attempt_id: AttemptId },
    Quarantined { attempt_id: AttemptId },
    RecoveryPending { attempt_id: AttemptId },
    TerminalReportPending { attempt_id: AttemptId },
}

pub struct RunnerEngine<P, A, W, C = crate::SystemClock> {
    protocol: P,
    adapter: A,
    journal: OwnerOnlyJournal,
    workspaces: WorkspaceManager<W>,
    clock: C,
    /// The events/decisions/artifacts transport
    /// (`AttemptDataProtocol`). Optional and defaulted to `None` by every
    /// existing constructor so no caller (`main.rs`
    /// aside, see [`Self::with_data_protocol`]) is forced to supply one —
    /// `crash_matrix.rs` and `h3_checkout.rs` keep
    /// compiling unchanged. When absent, the engine behaves exactly as
    /// before: no event/artifact submission is attempted.
    data_protocol: Option<Arc<dyn AttemptDataProtocol>>,
}

impl<P, A, W> RunnerEngine<P, A, W, crate::SystemClock>
where
    P: PullProtocol,
    A: HarnessAdapter,
    W: WorktreeProvisioner,
{
    pub fn new(
        protocol: P,
        adapter: A,
        journal: OwnerOnlyJournal,
        workspaces: WorkspaceManager<W>,
    ) -> Self {
        Self::with_clock(protocol, adapter, journal, workspaces, crate::SystemClock)
    }
}

impl<P, A, W, C> RunnerEngine<P, A, W, C>
where
    P: PullProtocol,
    A: HarnessAdapter,
    W: WorktreeProvisioner,
    C: crate::Clock,
{
    /// Injects the local clock so heartbeat timestamps can be deterministic in
    /// tests without introducing lifecycle sleeps.
    pub fn with_clock(
        protocol: P,
        adapter: A,
        journal: OwnerOnlyJournal,
        workspaces: WorkspaceManager<W>,
        clock: C,
    ) -> Self {
        Self {
            protocol,
            adapter,
            journal,
            workspaces,
            clock,
            data_protocol: None,
        }
    }

    /// Attaches the events/decisions/artifacts transport. A
    /// separate builder method, not a `new`/`with_clock` parameter, so every
    /// pre-existing construction site keeps
    /// compiling untouched; only `main.rs` (the one production wiring point)
    /// calls it today.
    pub fn with_data_protocol(mut self, data_protocol: Arc<dyn AttemptDataProtocol>) -> Self {
        self.data_protocol = Some(data_protocol);
        self
    }

    pub async fn enroll(
        &self,
        enrollment_credential: &crate::EnrollmentCredential,
        request: EnrollmentRequest,
    ) -> Result<EnrollmentResponse, EngineError> {
        Ok(self.protocol.enroll(enrollment_credential, request).await?)
    }

    pub async fn refresh(
        &self,
        session: &RunnerSession,
        request: RefreshRequest,
    ) -> Result<RefreshResponse, EngineError> {
        Ok(self.protocol.refresh(session, request).await?)
    }

    /// Performs one bounded claim/run/report cycle. The caller owns pacing;
    /// this avoids an untestable retry loop and means no work is never treated
    /// as a successful harness execution.
    pub async fn run_once(
        &self,
        session: &RunnerSession,
        claim: ClaimRequest,
    ) -> Result<RunCycle, EngineError> {
        match self.protocol.claim(session, claim).await? {
            ClaimResult::NoWork { .. } => Ok(RunCycle::NoWork),
            ClaimResult::Work(work) => self.run_claimed(session, *work).await,
        }
    }

    pub async fn recover(&self, session: &RunnerSession) -> Result<Vec<RunCycle>, EngineError> {
        let mut outcomes = Vec::new();
        for mut record in self.journal.unresolved()? {
            match (
                record.state == JournalState::TerminalReportPending,
                record.pending_terminal_report.is_some(),
            ) {
                (true, true) => {
                    let outcome = self
                        .send_pending_terminal_report(session, &mut record)
                        .await?;
                    if matches!(
                        outcome,
                        RunCycle::Completed { .. } | RunCycle::Cancelled { .. }
                    ) {
                        // A terminal replay may only clean up after its
                        // acknowledgement has been fsynced as `Reported`.
                        // `cleanup` revalidates this journal-derived path,
                        // marker, and deterministic attempt location.
                        let _ = self
                            .workspaces
                            .cleanup(&Self::workspace_from_journal(&record));
                    }
                    outcomes.push(outcome);
                    continue;
                }
                (true, false) | (false, true) => {
                    return Err(EngineError::Journal(JournalError::Malformed));
                }
                (false, false) => {}
            }
            let observation = match self.adapter.reconcile(&record).await {
                Ok(observation) => observation,
                Err(_) => RecoveryObservation::Ambiguous,
            };
            let outcome = self
                .report_recovery_and_apply_disposition(session, &mut record, observation)
                .await?;
            // A settled recovery owns the same cleanup duty as a
            // terminal replay. Without this the checkout a killed runner left
            // behind survives every restart, because nothing else ever revisits
            // that attempt's directory. A `Quarantined` or `RecoveryPending`
            // outcome deliberately keeps the checkout: it is the evidence an
            // operator needs, and the attempt is not settled.
            if matches!(outcome, RunCycle::Completed { .. }) {
                let _ = self
                    .workspaces
                    .cleanup(&Self::workspace_from_journal(&record));
            }
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }

    async fn run_claimed(
        &self,
        session: &RunnerSession,
        work: ClaimedWork,
    ) -> Result<RunCycle, EngineError> {
        let repository = work.workspace_repository()?;
        if work.lease.runner_id != session.runner_id {
            return Err(EngineError::RunnerMismatch);
        }

        let workspace = self.workspaces.plan(&work.lease, &repository)?;
        let mut record = AttemptJournal::prepared(&work.lease, workspace.journal());
        // This is the hard ownership boundary: no worktree or adapter method
        // with a local side effect is called before create+fsync succeeds.
        self.journal.persist_before_spawn(&record)?;
        self.protocol
            .report_start(
                session,
                StartReport {
                    attempt_id: record.attempt_id.clone(),
                    fencing_token: record.fencing_token,
                    phase: StartPhase::Preparing,
                    workspace_id: Some(workspace.id.clone()),
                    base_revision: Some(workspace.base_revision.clone()),
                    process_id: None,
                },
            )
            .await?;
        self.workspaces.provision(&workspace, &repository).await?;

        let spec = ExecutionSpec {
            work: work.clone(),
            workspace,
        };
        self.adapter.validate(&spec).await?;
        let handle = self.adapter.start(&spec).await?;
        record.state = JournalState::ProcessObservedRunning;
        record.process_id = Some(handle.process_id.clone());
        if self.journal.update(&record).is_err() {
            return self.quarantine_after_spawn(session, &record, &handle).await;
        }
        if self
            .protocol
            .report_start(
                session,
                StartReport {
                    attempt_id: record.attempt_id.clone(),
                    fencing_token: record.fencing_token,
                    phase: StartPhase::ProcessObservedRunning,
                    workspace_id: Some(spec.workspace.id.clone()),
                    base_revision: Some(spec.workspace.base_revision.clone()),
                    process_id: record.process_id.clone(),
                },
            )
            .await
            .is_err()
        {
            return self.quarantine_after_spawn(session, &record, &handle).await;
        }

        let heartbeat_request = self.heartbeat_request(session, &record);
        let heartbeat = match self
            .protocol
            .heartbeat(session, heartbeat_request.clone())
            .await
        {
            Ok(response) => response,
            Err(_) => return self.quarantine_after_spawn(session, &record, &handle).await,
        };
        if heartbeat.protocol_version != ProtocolVersion::v1()
            || heartbeat.heartbeat_id != heartbeat_request.heartbeat_id
        {
            return self.quarantine_after_spawn(session, &record, &handle).await;
        }
        let lease = heartbeat.lease_results.into_iter().find(|result| {
            result.attempt_id == record.attempt_id && result.fencing_token == record.fencing_token
        });
        let Some(lease) = lease else {
            return self.quarantine_after_spawn(session, &record, &handle).await;
        };

        if lease.cancellation_requested {
            record.state = JournalState::CancellationRequested;
            if self.journal.update(&record).is_err() {
                return self.quarantine_after_spawn(session, &record, &handle).await;
            }
            let evidence = match self.adapter.cancel(&handle).await {
                Ok(evidence) => evidence,
                Err(_) => return self.quarantine_after_spawn(session, &record, &handle).await,
            };
            if evidence.observation != CancelObservation::ProcessStopped {
                // A terminal acknowledgement or an ambiguous adapter result
                // is not proof that this cancellation committed. Keep the
                // local record on the recovery/quarantine path instead of
                // fabricating a cancelled success.
                return self.report_or_retain_ambiguity(session, &record).await;
            }
            // Real evidence that a cancellation actually happened,
            // submitted through `AttemptDataProtocol` before the terminal
            // report — best-effort, see `submit_event`'s own doc comment.
            self.submit_cancellation_event(session, &mut record, &evidence)
                .await;
            let report = CancellationReport {
                protocol_version: ProtocolVersion::v1(),
                runner_id: session.runner_id.clone(),
                cancellation_request_id: CancellationRequestId::new(format!(
                    "cancel:{}:{}",
                    record.attempt_id.as_str(),
                    record.fencing_token.0
                )),
                attempt_id: record.attempt_id.clone(),
                fencing_token: record.fencing_token,
                observation: evidence.observation,
                observed_at: evidence.observed_at,
                details: evidence.details,
            };
            self.persist_pending_terminal_report(
                &mut record,
                PendingTerminalReportKind::Cancellation,
                &report,
            )?;
            let cycle = self
                .send_pending_terminal_report(session, &mut record)
                .await?;
            if matches!(cycle, RunCycle::Cancelled { .. }) {
                let _ = self.workspaces.cleanup(&spec.workspace);
            }
            return Ok(cycle);
        }

        let outcome = match self
            .wait_with_lease_renewal(session, &record, &handle)
            .await
        {
            Ok(outcome) => outcome,
            Err(_) => return self.quarantine_after_spawn(session, &record, &handle).await,
        };
        if !matches!(
            outcome.terminal_state,
            AttemptState::Succeeded | AttemptState::Failed | AttemptState::Cancelled
        ) {
            return self.quarantine_after_spawn(session, &record, &handle).await;
        }
        let outcome = outcome.normalize_workspace_facts(&spec.workspace);
        // The runner's only call site for events and artifacts.
        // `outcome.terminal_reason` is the exact JSON the harness adapters
        // already produce (including the `artifact` key their `wait()`
        // implementations stage via `harness::artifact::ArtifactStager` —
        // see e.g. `codex.rs::stage_run_log`), so this reads real evidence,
        // never a fabricated summary. Runs before the completion report so
        // an operator inspecting the timeline after `succeeded` sees the
        // event/artifact already there.
        self.submit_terminal_evidence(session, &mut record, &outcome)
            .await;
        let report = CompletionReport {
            protocol_version: ProtocolVersion::v1(),
            runner_id: session.runner_id.clone(),
            completion_id: CompletionId::new(format!(
                "completion:{}:{}",
                record.attempt_id.as_str(),
                record.fencing_token.0
            )),
            attempt_id: record.attempt_id.clone(),
            fencing_token: record.fencing_token,
            terminal_state: outcome.terminal_state,
            terminal_reason: outcome.terminal_reason,
            // NOT `outcome.final_checkpoint` (always `None` — no
            // adapter ever sets it). `complete_execution_result`'s own
            // compare-and-set requires this to equal the attempt row's
            // *current* `event_checkpoint` column exactly
            // (`crates/tack-db/src/repo/execution.rs`, the `UPDATE ... AND
            // event_checkpoint IS ?` guard) — and `submit_terminal_evidence`
            // above just moved that column from `NULL` to a real value by
            // submitting an event. Sending the adapter's stale `None` here
            // mismatches the row's now-current value and the update's
            // `rows_affected() != 1` branch turns every completion into a
            // `Conflict`, forever — proven the hard way against a live
            // server (`./scripts/smoke.sh`): the
            // event/artifact evidence reached
            // the server but the attempt's own completion never did.
            // `record.last_event_checkpoint` is exactly what the server
            // committed (or still `None` if nothing was ever submitted),
            // so it is always the value this compare-and-set expects.
            final_event_checkpoint: record.last_event_checkpoint.clone(),
            actual_execution: outcome.actual_execution,
            usage: outcome.usage,
        };
        self.persist_pending_terminal_report(
            &mut record,
            PendingTerminalReportKind::Completion,
            &report,
        )?;
        let cycle = self
            .send_pending_terminal_report(session, &mut record)
            .await?;
        if matches!(cycle, RunCycle::Completed { .. }) {
            let _ = self.workspaces.cleanup(&spec.workspace);
        }
        Ok(cycle)
    }

    fn heartbeat_request(
        &self,
        session: &RunnerSession,
        record: &AttemptJournal,
    ) -> HeartbeatRequest {
        let sent_at = chrono::DateTime::<chrono::Utc>::from(self.clock.now())
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        HeartbeatRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: session.runner_id.clone(),
            heartbeat_id: Self::heartbeat_id(record, &sent_at),
            sent_at: Timestamp::new(sent_at),
            available_capacity: 0,
            active_attempts: vec![ActiveAttempt {
                attempt_id: record.attempt_id.clone(),
                fencing_token: record.fencing_token,
                state: AttemptState::Running,
                journal_state: record.state,
                last_event_checkpoint: record.last_event_checkpoint.clone(),
            }],
        }
    }

    /// The logical heartbeat send time separates periodic sends for the same
    /// attempt/fence, while reusing the exact frozen payload preserves its
    /// idempotency ID for a retry.
    fn heartbeat_id(record: &AttemptJournal, sent_at: &str) -> String {
        let material = format!(
            "{}:{}:{sent_at}",
            record.attempt_id.as_str(),
            record.fencing_token.0
        );
        let opaque = material
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        format!("hb_{opaque}")
    }

    /// Runs `self.adapter.wait(handle)` to completion while sending a
    /// heartbeat for `record`'s attempt every [`LEASE_RENEWAL_INTERVAL`].
    ///
    /// The lease this attempt holds is granted for a bounded window at claim
    /// time and only extended by a subsequent heartbeat that names the
    /// attempt (see `heartbeat_request`). A harness run can take up to the
    /// request's own `timeout_seconds` (bounded at 86,400 by
    /// `request_timeout_seconds_max`), so waiting on the harness without
    /// heartbeating meanwhile lets the lease expire long before the process
    /// exits: the eventual completion report is then rejected as a stale
    /// lease, and nothing durably retries it during live operation (only a
    /// runner restart replays a journaled `TerminalReportPending` record,
    /// and that replay carries the same now-expired fencing token so it
    /// fails identically) — the attempt is stuck `running` forever from the
    /// API's point of view. Renewing periodically here, concurrently with
    /// the wait, keeps the lease alive for the run's real duration. A failed
    /// renewal is logged and does not interrupt the wait: the harness is
    /// still running regardless, and the worst case (a renewal never lands
    /// again before the harness exits) is the same stale-lease outcome this
    /// loop exists to avoid, not a new failure mode.
    async fn wait_with_lease_renewal(
        &self,
        session: &RunnerSession,
        record: &AttemptJournal,
        handle: &LocalRunHandle,
    ) -> Result<HarnessOutcome, HarnessError> {
        let mut wait_future = self.adapter.wait(handle);
        loop {
            tokio::select! {
                biased;
                outcome = &mut wait_future => return outcome,
                () = tokio::time::sleep(LEASE_RENEWAL_INTERVAL) => {
                    let request = self.heartbeat_request(session, record);
                    if let Err(error) = self.protocol.heartbeat(session, request).await {
                        tracing::warn!(
                            %error,
                            attempt_id = record.attempt_id.as_str(),
                            "lease-renewal heartbeat failed while the harness is still running"
                        );
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // Events, decisions and artifacts.
    //
    // The methods below are the call sites: one runner-sourced
    // event per terminal outcome (completion or cancellation), plus a
    // best-effort upload of whatever artifact an adapter already staged
    // locally (`terminal_reason.artifact`, each harness adapter's own
    // convention — see e.g. `harness::codex::CodexAdapter::stage_run_log`).
    //
    // Submission is deliberately best-effort: a transport failure here is
    // logged (ids only) and never turns a real harness result into a failed
    // attempt or blocks the terminal report. It does not participate in the
    // crash-safe replay `send_pending_terminal_report` implements for
    // completion/cancellation — a restart does not resend a lost event or
    // artifact. That is a known limitation, not hidden here.
    // -----------------------------------------------------------------

    async fn submit_terminal_evidence(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        outcome: &HarnessOutcome,
    ) {
        self.submit_event(
            session,
            record,
            "attempt.terminal",
            outcome.terminal_reason.clone(),
        )
        .await;
        if let Some(artifact) = outcome.terminal_reason.get("artifact") {
            self.submit_staged_artifact(session, record, artifact).await;
        }
    }

    async fn submit_cancellation_event(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        evidence: &CancellationEvidence,
    ) {
        let payload = serde_json::json!({
            "observation": evidence.observation,
            "details": serde_json::Value::Object(evidence.details.clone()),
        });
        self.submit_event(session, record, "attempt.cancelled", payload)
            .await;
    }

    /// Submits one runner-sourced event carrying `payload`. `event_id` is
    /// derived deterministically from `(attempt_id, fencing_token, kind)` —
    /// never random or clock-based — so a caller that retries this exact
    /// submission (e.g. after a transient transport error) reuses the
    /// identical id instead of manufacturing a duplicate under a new
    /// identity. `docs/contracts/runner-v1/event-batch.request.json`'s own
    /// idempotency rule is a unique `(attempt_id, event_id)`; the server is
    /// the authority on treating a resend of the same id as a no-op
    /// (`duplicate_event_ids`), proved from this side in
    /// `submitting_the_same_terminal_event_twice_is_idempotent` below.
    async fn submit_event(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        kind: &str,
        payload: serde_json::Value,
    ) {
        let Some(data_protocol) = self.data_protocol.as_ref() else {
            return;
        };
        let sent_at = chrono::DateTime::<chrono::Utc>::from(self.clock.now())
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let event_id = Self::event_id(record, kind);
        let checkpoint = Checkpoint::new(format!(
            "chk:{}:{}:{kind}",
            record.attempt_id.as_str(),
            record.fencing_token.0
        ));
        let report = EventBatchReport {
            attempt_id: record.attempt_id.clone(),
            fencing_token: record.fencing_token,
            previous_checkpoint: record.last_event_checkpoint.clone(),
            checkpoint: checkpoint.clone(),
            events: vec![ProtocolEvent {
                event_id,
                // Exactly one event is ever submitted per checkpoint in this
                // card's wiring (completion XOR cancellation, never both),
                // so a fixed `0` is honest, not a placeholder — see the
                // module doc above on what still needs a real sequence
                // counter if a future card submits more than one event per
                // attempt.
                sequence: 0,
                occurred_at: Timestamp::new(sent_at),
                source: "runner".to_owned(),
                kind: kind.to_owned(),
                payload,
            }],
        };
        match data_protocol.submit_events(session, report).await {
            Ok(response) => {
                record.last_event_checkpoint = response.committed_checkpoint.or(Some(checkpoint));
                // Best-effort: the event already reached the server; a local
                // journal-write failure here only affects this runner's own
                // `last_event_checkpoint` bookkeeping (used for a future
                // resumed batch), not the event's durability on the server.
                let _ = self.journal.update(record);
            }
            Err(_error) => {
                tracing::warn!(
                    attempt_id = record.attempt_id.as_str(),
                    "event submission failed"
                );
            }
        }
    }

    fn event_id(record: &AttemptJournal, kind: &str) -> String {
        let material = format!(
            "{}:{}:{kind}",
            record.attempt_id.as_str(),
            record.fencing_token.0
        );
        format!("evt_{}", Self::hex(material.as_bytes()))
    }

    fn artifact_id(record: &AttemptJournal, sha256: &str) -> String {
        let material = format!(
            "{}:{}:{sha256}",
            record.attempt_id.as_str(),
            record.fencing_token.0
        );
        format!("art_{}", Self::hex(material.as_bytes()))
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// Best-effort upload of a locally staged artifact. `artifact` is the
    /// exact JSON object the harness adapters already produce at
    /// `terminal_reason.artifact` (`kind`, `name`, `media_type`, `sha256`,
    /// `staged_path`, ...) via `harness::artifact::ArtifactStager` — this
    /// method is the first thing in the tree that ever reads it back out.
    /// The file's bytes and size are read fresh from `staged_path` rather
    /// than trusted from the JSON, mirroring `ArtifactStager`'s own rule
    /// that a checksum/size must come from bytes actually handled, never a
    /// value merely reported.
    async fn submit_staged_artifact(
        &self,
        session: &RunnerSession,
        record: &AttemptJournal,
        artifact: &serde_json::Value,
    ) {
        let Some(data_protocol) = self.data_protocol.as_ref() else {
            return;
        };
        let (Some(kind), Some(name), Some(media_type), Some(sha256), Some(staged_path)) = (
            artifact.get("kind").and_then(|value| value.as_str()),
            artifact.get("name").and_then(|value| value.as_str()),
            artifact.get("media_type").and_then(|value| value.as_str()),
            artifact.get("sha256").and_then(|value| value.as_str()),
            artifact.get("staged_path").and_then(|value| value.as_str()),
        ) else {
            tracing::warn!(
                attempt_id = record.attempt_id.as_str(),
                "staged artifact JSON is missing an expected field; not uploaded"
            );
            return;
        };
        let content = match std::fs::read(staged_path) {
            Ok(bytes) => bytes,
            Err(_) => {
                tracing::warn!(
                    attempt_id = record.attempt_id.as_str(),
                    "staged artifact could not be read for upload"
                );
                return;
            }
        };
        let artifact_id = Self::artifact_id(record, sha256);
        let manifest = ArtifactManifestReport {
            attempt_id: record.attempt_id.clone(),
            fencing_token: record.fencing_token,
            artifacts: vec![ArtifactManifestItem {
                artifact_id: artifact_id.clone(),
                kind: kind.to_owned(),
                name: name.to_owned(),
                media_type: Some(media_type.to_owned()),
                size_bytes: content.len() as u64,
                sha256: sha256.to_owned(),
                content_disposition: "inline_upload".to_owned(),
                metadata: Default::default(),
            }],
        };
        let grants = match data_protocol
            .submit_artifact_manifest(session, manifest)
            .await
        {
            Ok(grants) => grants,
            Err(_error) => {
                tracing::warn!(
                    attempt_id = record.attempt_id.as_str(),
                    "artifact manifest submission failed"
                );
                return;
            }
        };
        let Some(grant) = grants
            .into_iter()
            .find(|grant| grant.artifact_id == artifact_id)
        else {
            tracing::warn!(
                attempt_id = record.attempt_id.as_str(),
                "server granted no upload for the submitted artifact"
            );
            return;
        };
        if data_protocol
            .put_artifact_content(
                session,
                record.fencing_token,
                &grant,
                Some(media_type),
                content,
            )
            .await
            .is_err()
        {
            tracing::warn!(
                attempt_id = record.attempt_id.as_str(),
                "artifact content upload failed"
            );
        }
    }

    fn persist_pending_terminal_report<T: serde::Serialize>(
        &self,
        record: &mut AttemptJournal,
        kind: PendingTerminalReportKind,
        report: &T,
    ) -> Result<(), EngineError> {
        let canonical_json =
            serde_json::to_string(report).map_err(|_| EngineError::TerminalReportSerialization)?;
        let mut pending = record.clone();
        pending.state = JournalState::TerminalReportPending;
        pending.pending_terminal_report = Some(PendingTerminalReport {
            kind,
            canonical_json,
        });
        // `update` uses atomic replacement and fsync. Do not mutate the live
        // record until that durable intent succeeds.
        self.journal.update(&pending)?;
        *record = pending;
        Ok(())
    }

    async fn send_pending_terminal_report(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
    ) -> Result<RunCycle, EngineError> {
        if record.runner_id != session.runner_id {
            return Err(EngineError::Journal(JournalError::Malformed));
        }
        let Some(pending) = record.pending_terminal_report.as_ref() else {
            return Err(EngineError::Journal(JournalError::Malformed));
        };
        match pending.kind {
            PendingTerminalReportKind::Completion => {
                let report: CompletionReport = serde_json::from_str(&pending.canonical_json)
                    .map_err(|_| EngineError::Journal(JournalError::Malformed))?;
                if report.protocol_version != ProtocolVersion::v1()
                    || report.runner_id != session.runner_id
                    || report.attempt_id != record.attempt_id
                    || report.fencing_token != record.fencing_token
                    || report.completion_id
                        != CompletionId::new(format!(
                            "completion:{}:{}",
                            record.attempt_id.as_str(),
                            record.fencing_token.0
                        ))
                    || report.actual_execution.workspace_id.as_str()
                        != record.workspace.workspace_id.as_str()
                    || report.actual_execution.base_revision != record.workspace.base_revision
                    || !matches!(
                        report.terminal_state,
                        AttemptState::Succeeded | AttemptState::Failed | AttemptState::Cancelled
                    )
                {
                    return Err(EngineError::Journal(JournalError::Malformed));
                }
                let acknowledged = matches!(
                    self.protocol.report_completion(session, report.clone()).await,
                    Ok(CompletionResponse {
                        protocol_version,
                        attempt_id,
                        completion_id,
                        state,
                        ..
                    }) if protocol_version == ProtocolVersion::v1()
                        && attempt_id == report.attempt_id
                        && completion_id == report.completion_id
                        && state == report.terminal_state
                );
                if !acknowledged {
                    return Ok(RunCycle::TerminalReportPending {
                        attempt_id: record.attempt_id.clone(),
                    });
                }
                self.acknowledge_pending_terminal_report(
                    record,
                    RunCycle::Completed {
                        attempt_id: record.attempt_id.clone(),
                    },
                )
            }
            PendingTerminalReportKind::Cancellation => {
                let report: CancellationReport = serde_json::from_str(&pending.canonical_json)
                    .map_err(|_| EngineError::Journal(JournalError::Malformed))?;
                if report.protocol_version != ProtocolVersion::v1()
                    || report.runner_id != session.runner_id
                    || report.attempt_id != record.attempt_id
                    || report.fencing_token != record.fencing_token
                    || report.cancellation_request_id
                        != CancellationRequestId::new(format!(
                            "cancel:{}:{}",
                            record.attempt_id.as_str(),
                            record.fencing_token.0
                        ))
                    || report.observation != CancelObservation::ProcessStopped
                {
                    return Err(EngineError::Journal(JournalError::Malformed));
                }
                let acknowledged = matches!(
                    self.protocol.report_cancellation(session, report.clone()).await,
                    Ok(response)
                        if response.protocol_version == ProtocolVersion::v1()
                            && response.attempt_id == report.attempt_id
                            && response.cancellation_request_id == report.cancellation_request_id
                            && response.state == AttemptState::Cancelled
                );
                if !acknowledged {
                    return Ok(RunCycle::TerminalReportPending {
                        attempt_id: record.attempt_id.clone(),
                    });
                }
                self.acknowledge_pending_terminal_report(
                    record,
                    RunCycle::Cancelled {
                        attempt_id: record.attempt_id.clone(),
                    },
                )
            }
        }
    }

    fn acknowledge_pending_terminal_report(
        &self,
        record: &mut AttemptJournal,
        cycle: RunCycle,
    ) -> Result<RunCycle, EngineError> {
        let mut acknowledged = record.clone();
        acknowledged.pending_terminal_report = None;
        acknowledged.state = JournalState::Reported;
        // If the post-ack write fails, retain the in-memory and on-disk
        // pending payload for an exact replay on restart.
        if self.journal.update(&acknowledged).is_err() {
            return Ok(RunCycle::TerminalReportPending {
                attempt_id: record.attempt_id.clone(),
            });
        }
        *record = acknowledged;
        Ok(cycle)
    }

    fn workspace_from_journal(record: &AttemptJournal) -> Workspace {
        Workspace {
            attempt_id: record.attempt_id.clone(),
            id: record.workspace.workspace_id.clone(),
            path: record.workspace.path.clone(),
            base_revision: record.workspace.base_revision.clone(),
        }
    }

    async fn quarantine_after_spawn(
        &self,
        session: &RunnerSession,
        record: &AttemptJournal,
        handle: &LocalRunHandle,
    ) -> Result<RunCycle, EngineError> {
        // Report before moving evidence out of restart scans. If delivery
        // fails, the journal remains unresolved for a later recovery pass.
        let outcome = self.report_or_retain_ambiguity(session, record).await;
        let _ = self.adapter.cancel(handle).await;
        outcome
    }

    async fn report_or_retain_ambiguity(
        &self,
        session: &RunnerSession,
        record: &AttemptJournal,
    ) -> Result<RunCycle, EngineError> {
        let mut record = record.clone();
        self.report_recovery_and_apply_disposition(
            session,
            &mut record,
            RecoveryObservation::Ambiguous,
        )
        .await
    }

    async fn report_recovery_and_apply_disposition(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        observation: RecoveryObservation,
    ) -> Result<RunCycle, EngineError> {
        let request = Self::recovery_request(session, record, observation);
        let response = match self
            .protocol
            .observe_recovery(session, request.clone())
            .await
        {
            Ok(response)
                if response.attempt_id == request.attempt_id
                    && response.recovery_key == request.recovery_key =>
            {
                response
            }
            Err(ProtocolClientError::StaleLease) => {
                // `stale_lease` is the one answer that settles this rather
                // than leaving it open: the server has no active lease
                // under this attempt id, runner id and fencing token at
                // all, and (this fencing token being fixed for the life of
                // an attempt) never will again. That is different in kind
                // from an absent reply or a transport failure, which say
                // nothing about whether the attempt exists. Quarantine it
                // exactly like an operator-facing disposition: the record
                // leaves the restart scan, and its checkout is left on disk
                // as the evidence an operator would need.
                self.journal.quarantine(record)?;
                return Ok(RunCycle::Quarantined {
                    attempt_id: record.attempt_id.clone(),
                });
            }
            Ok(_) | Err(_) => {
                // An absent or mismatched acknowledgement cannot settle local
                // process evidence; leave the journal in the restart scan.
                return Ok(RunCycle::RecoveryPending {
                    attempt_id: record.attempt_id.clone(),
                });
            }
        };

        self.apply_recovery_disposition(record, observation, response)
    }

    fn apply_recovery_disposition(
        &self,
        record: &mut AttemptJournal,
        observation: RecoveryObservation,
        response: RecoveryObservationResponse,
    ) -> Result<RunCycle, EngineError> {
        let settled = match response.disposition {
            RecoveryDisposition::SafePreSpawnRequeue => {
                observation == RecoveryObservation::ProcessStopped
                    && record.state == JournalState::Prepared
                    && record.process_id.is_none()
            }
            RecoveryDisposition::AlreadyTerminal => {
                observation == RecoveryObservation::ProcessStopped
            }
            RecoveryDisposition::NeedsOperator => false,
        };
        if settled {
            record.state = JournalState::RecoveryObserved;
            self.journal.update(record)?;
            return Ok(RunCycle::Completed {
                attempt_id: record.attempt_id.clone(),
            });
        }

        // `needs_operator` is always durable quarantine. A disposition that
        // contradicts local evidence is treated just as conservatively.
        self.journal.quarantine(record)?;
        Ok(RunCycle::Quarantined {
            attempt_id: record.attempt_id.clone(),
        })
    }

    fn recovery_request(
        session: &RunnerSession,
        record: &AttemptJournal,
        observation: RecoveryObservation,
    ) -> RecoveryObservationRequest {
        RecoveryObservationRequest {
            protocol_version: ProtocolVersion::v1(),
            runner_id: DomainRunnerId::new(session.runner_id.as_str()),
            attempt_id: DomainAttemptId::new(record.attempt_id.as_str()),
            fencing_token: DomainFencingToken(record.fencing_token.0),
            recovery_key: RecoveryKey::new(format!(
                "recovery:{}:{}:{}",
                record.attempt_id.as_str(),
                record.fencing_token.0,
                match observation {
                    RecoveryObservation::ProcessStopped => "process_stopped",
                    RecoveryObservation::ProcessRunning => "process_running",
                    RecoveryObservation::Ambiguous => "ambiguous",
                }
            )),
            observation,
            details: DomainRecoveryDetails {
                journal_state: Self::recovery_journal_state(record.state),
                process_observed: record.process_id.is_some(),
                additional: BTreeMap::new(),
            },
            additional: BTreeMap::new(),
        }
    }

    const fn recovery_journal_state(state: JournalState) -> RecoveryJournalState {
        match state {
            JournalState::Prepared => RecoveryJournalState::Prepared,
            JournalState::ProcessObservedRunning => RecoveryJournalState::ProcessObservedRunning,
            JournalState::CancellationRequested => RecoveryJournalState::CancellationRequested,
            // Pending terminal payloads are handled before recovery mapping.
            JournalState::TerminalReportPending => RecoveryJournalState::Reported,
            JournalState::RecoveryObserved => RecoveryJournalState::RecoveryObserved,
            JournalState::Reported => RecoveryJournalState::Reported,
            JournalState::Quarantined => RecoveryJournalState::Quarantined,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        path::{Path, PathBuf},
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        time::{Duration, SystemTime},
    };

    use super::*;
    use crate::client::{
        ArtifactUploadGrant, AttemptLease, CancellationResponse, ClaimRequestId, ClaimResult,
        ClaimedWork, CompletionResponse, DecisionCreateReport, DecisionCreateResponse,
        DecisionPollReport, DecisionPollResponse, EventBatchResponse, FencingToken, LeaseResult,
        ProtocolClientError, RunnerCredential, RunnerId, Timestamp,
    };
    use tack_orch::execution::{
        AttemptId as DomainAttemptId, AttemptSnapshot, ExecutionRequestSnapshot,
        RecoveryDisposition, RecoveryJournalState, RecoveryObservationRequest,
        RecoveryObservationResponse, RunnerId as DomainRunnerId,
    };

    #[derive(Clone)]
    struct RecoveryResponseConfig {
        disposition: RecoveryDisposition,
        replayed: bool,
        additional: BTreeMap<String, serde_json::Value>,
    }

    #[derive(Clone, Copy)]
    enum CancellationAckMismatch {
        None,
        Attempt,
        Request,
        State,
    }

    #[derive(Clone, Copy)]
    struct CancellationResponseConfig {
        replayed: bool,
        mismatch: CancellationAckMismatch,
    }

    #[derive(Clone, Copy)]
    enum CompletionAckMismatch {
        None,
        Attempt,
        Completion,
        State,
    }

    #[derive(Clone, Copy)]
    struct CompletionResponseConfig {
        replayed: bool,
        mismatch: CompletionAckMismatch,
    }

    #[derive(Clone, Copy)]
    struct FixedClock(SystemTime);

    impl crate::Clock for FixedClock {
        fn now(&self) -> SystemTime {
            self.0
        }
    }

    #[derive(Clone)]
    struct AdvancingClock {
        base: SystemTime,
        calls: Arc<AtomicUsize>,
    }

    impl crate::Clock for AdvancingClock {
        fn now(&self) -> SystemTime {
            self.base + Duration::from_secs(self.calls.fetch_add(1, Ordering::SeqCst) as u64)
        }
    }

    #[derive(Clone)]
    struct FakeProtocol {
        claim: Arc<Mutex<Option<ClaimResult>>>,
        cancellation_requested: bool,
        heartbeat_echo_matches: Arc<AtomicBool>,
        accepted_heartbeat_ids: Arc<Mutex<BTreeSet<String>>>,
        stale_completion: Arc<AtomicBool>,
        fail_running_start: bool,
        fail_cancellation_report: Arc<AtomicBool>,
        start_reports: Arc<AtomicUsize>,
        reported_starts: Arc<Mutex<Vec<StartReport>>>,
        received_heartbeats: Arc<Mutex<Vec<HeartbeatRequest>>>,
        reported_heartbeats: Arc<Mutex<Vec<super::super::HeartbeatResponse>>>,
        completion_reports: Arc<AtomicUsize>,
        reported_completions: Arc<Mutex<Vec<CompletionReport>>>,
        completion_response: Arc<Mutex<CompletionResponseConfig>>,
        fail_completion_ack_update: Arc<Mutex<Option<OwnerOnlyJournal>>>,
        cancellation_reports: Arc<AtomicUsize>,
        reported_cancellations: Arc<Mutex<Vec<CancellationReport>>>,
        cancellation_response: Arc<Mutex<CancellationResponseConfig>>,
        fail_cancellation_ack_update: Arc<Mutex<Option<OwnerOnlyJournal>>>,
        terminal_journal_at_send: Arc<Mutex<Option<OwnerOnlyJournal>>>,
        terminal_payload_was_durable: Arc<AtomicBool>,
        recovery_reports: Arc<AtomicUsize>,
        reported_recoveries: Arc<Mutex<Vec<RecoveryObservationRequest>>>,
        recovery_failures_remaining: Arc<AtomicUsize>,
        // Set to make every `observe_recovery` call fail with this exact
        // error instead of consulting `recovery_failures_remaining` or
        // `recovery_response` -- lets a test hold a single failure mode
        // (e.g. `StaleLease`, standing in for the server settling "no such
        // attempt") indefinitely across restarts, distinct from the finite,
        // eventually-recovering retries `recovery_failures_remaining` models.
        recovery_error: Arc<Mutex<Option<ProtocolClientError>>>,
        recovery_response: Arc<Mutex<RecoveryResponseConfig>>,
        refresh_requests: Arc<Mutex<Vec<RefreshRequest>>>,
    }

    #[async_trait]
    impl PullProtocol for FakeProtocol {
        async fn enroll(
            &self,
            _credential: &crate::EnrollmentCredential,
            _request: EnrollmentRequest,
        ) -> Result<EnrollmentResponse, ProtocolClientError> {
            Ok(EnrollmentResponse {
                session: session(),
                heartbeat_interval: std::time::Duration::from_secs(15),
                lease_duration: std::time::Duration::from_secs(60),
                server_time: Timestamp::new("2026-08-06T12:00:01Z"),
            })
        }

        async fn refresh(
            &self,
            _session: &RunnerSession,
            request: RefreshRequest,
        ) -> Result<RefreshResponse, ProtocolClientError> {
            self.refresh_requests
                .lock()
                .expect("fake protocol lock")
                .push(request);
            Ok(RefreshResponse {
                session: RunnerSession::new(
                    RunnerId::new("runner"),
                    RunnerCredential::new("rotated-never-log"),
                    Timestamp::new("2026-08-06T13:00:00Z"),
                ),
                accepted_at: Timestamp::new("2026-08-06T12:30:00Z"),
            })
        }

        async fn claim(
            &self,
            _session: &RunnerSession,
            _request: ClaimRequest,
        ) -> Result<ClaimResult, ProtocolClientError> {
            self.claim
                .lock()
                .expect("fake protocol lock")
                .take()
                .ok_or(ProtocolClientError::Rejected)
        }

        async fn heartbeat(
            &self,
            _session: &RunnerSession,
            request: HeartbeatRequest,
        ) -> Result<super::super::HeartbeatResponse, ProtocolClientError> {
            if !self
                .accepted_heartbeat_ids
                .lock()
                .expect("fake protocol lock")
                .insert(request.heartbeat_id.clone())
            {
                return Err(ProtocolClientError::Rejected);
            }
            self.received_heartbeats
                .lock()
                .expect("fake protocol lock")
                .push(request.clone());
            let active = request
                .active_attempts
                .into_iter()
                .next()
                .expect("active attempt");
            let response = super::super::HeartbeatResponse {
                protocol_version: ProtocolVersion::v1(),
                heartbeat_id: if self.heartbeat_echo_matches.load(Ordering::SeqCst) {
                    request.heartbeat_id
                } else {
                    "mismatched-heartbeat".into()
                },
                accepted_at: Timestamp::new("2026-08-06T12:20:16Z"),
                lease_results: vec![LeaseResult {
                    attempt_id: active.attempt_id,
                    fencing_token: active.fencing_token,
                    lease_expires_at: Timestamp::new("2026-08-06T12:21:16Z"),
                    cancellation_requested: self.cancellation_requested,
                }],
            };
            self.reported_heartbeats
                .lock()
                .expect("fake protocol lock")
                .push(response.clone());
            Ok(response)
        }

        async fn report_start(
            &self,
            _session: &RunnerSession,
            report: StartReport,
        ) -> Result<(), ProtocolClientError> {
            self.start_reports.fetch_add(1, Ordering::SeqCst);
            self.reported_starts
                .lock()
                .expect("fake protocol lock")
                .push(report.clone());
            if self.fail_running_start && report.phase == StartPhase::ProcessObservedRunning {
                Err(ProtocolClientError::Transport)
            } else {
                Ok(())
            }
        }

        async fn report_completion(
            &self,
            _session: &RunnerSession,
            report: CompletionReport,
        ) -> Result<CompletionResponse, ProtocolClientError> {
            self.assert_terminal_payload_is_durable(
                &report.attempt_id,
                PendingTerminalReportKind::Completion,
                &report,
            );
            self.completion_reports.fetch_add(1, Ordering::SeqCst);
            self.reported_completions
                .lock()
                .expect("fake protocol lock")
                .push(report.clone());
            if self.stale_completion.load(Ordering::SeqCst) {
                Err(ProtocolClientError::StaleLease)
            } else {
                let config = *self.completion_response.lock().expect("fake protocol lock");
                let mut response = CompletionResponse {
                    protocol_version: ProtocolVersion::v1(),
                    attempt_id: report.attempt_id,
                    completion_id: report.completion_id,
                    state: report.terminal_state,
                    replayed: config.replayed,
                    committed_at: Timestamp::new("2026-08-06T12:25:01Z"),
                };
                match config.mismatch {
                    CompletionAckMismatch::None => {}
                    CompletionAckMismatch::Attempt => {
                        response.attempt_id = AttemptId::new("wrong-attempt")
                    }
                    CompletionAckMismatch::Completion => {
                        response.completion_id = CompletionId::new("wrong-completion")
                    }
                    CompletionAckMismatch::State => response.state = AttemptState::Running,
                }
                if let Some(journal) = self
                    .fail_completion_ack_update
                    .lock()
                    .expect("fake protocol lock")
                    .take()
                {
                    journal.fail_next_update_for_test();
                }
                Ok(response)
            }
        }

        async fn report_cancellation(
            &self,
            _session: &RunnerSession,
            report: CancellationReport,
        ) -> Result<CancellationResponse, ProtocolClientError> {
            self.assert_terminal_payload_is_durable(
                &report.attempt_id,
                PendingTerminalReportKind::Cancellation,
                &report,
            );
            self.cancellation_reports.fetch_add(1, Ordering::SeqCst);
            self.reported_cancellations
                .lock()
                .expect("fake protocol lock")
                .push(report.clone());
            if self.fail_cancellation_report.load(Ordering::SeqCst) {
                Err(ProtocolClientError::StaleLease)
            } else {
                let config = *self
                    .cancellation_response
                    .lock()
                    .expect("fake protocol lock");
                let mut response = CancellationResponse {
                    protocol_version: ProtocolVersion::v1(),
                    attempt_id: report.attempt_id,
                    cancellation_request_id: report.cancellation_request_id,
                    state: AttemptState::Cancelled,
                    replayed: config.replayed,
                    committed_at: Timestamp::new("2026-08-06T12:24:01Z"),
                };
                match config.mismatch {
                    CancellationAckMismatch::None => {}
                    CancellationAckMismatch::Attempt => {
                        response.attempt_id = AttemptId::new("wrong-attempt")
                    }
                    CancellationAckMismatch::Request => {
                        response.cancellation_request_id =
                            CancellationRequestId::new("wrong-cancel")
                    }
                    CancellationAckMismatch::State => response.state = AttemptState::Running,
                }
                if let Some(journal) = self
                    .fail_cancellation_ack_update
                    .lock()
                    .expect("fake protocol lock")
                    .take()
                {
                    journal.fail_next_update_for_test();
                }
                Ok(response)
            }
        }

        async fn observe_recovery(
            &self,
            _session: &RunnerSession,
            report: RecoveryObservationRequest,
        ) -> Result<RecoveryObservationResponse, ProtocolClientError> {
            self.recovery_reports.fetch_add(1, Ordering::SeqCst);
            self.reported_recoveries
                .lock()
                .expect("fake protocol lock")
                .push(report.clone());
            if let Some(error) = self
                .recovery_error
                .lock()
                .expect("fake protocol lock")
                .clone()
            {
                return Err(error);
            }
            let remaining = self.recovery_failures_remaining.load(Ordering::SeqCst);
            if remaining > 0 {
                self.recovery_failures_remaining
                    .fetch_sub(1, Ordering::SeqCst);
                Err(ProtocolClientError::Transport)
            } else {
                let config = self
                    .recovery_response
                    .lock()
                    .expect("fake protocol lock")
                    .clone();
                let mut response: RecoveryObservationResponse = serde_json::from_str(include_str!(
                    "../../../docs/contracts/runner-v1/recovery-observation.response.json"
                ))
                .expect("recovery response fixture");
                response.attempt_id = report.attempt_id;
                response.recovery_key = report.recovery_key;
                response.disposition = config.disposition;
                response.replayed = config.replayed;
                response.additional = config.additional;
                Ok(response)
            }
        }
    }

    impl FakeProtocol {
        fn assert_terminal_payload_is_durable<T: serde::Serialize>(
            &self,
            attempt_id: &AttemptId,
            kind: PendingTerminalReportKind,
            report: &T,
        ) {
            let Some(journal) = self
                .terminal_journal_at_send
                .lock()
                .expect("fake protocol lock")
                .clone()
            else {
                return;
            };
            let record = journal.load(attempt_id).expect("durable terminal journal");
            let pending = record
                .pending_terminal_report
                .expect("pending terminal report before send");
            assert_eq!(record.state, JournalState::TerminalReportPending);
            assert_eq!(pending.kind, kind);
            assert_eq!(
                pending.canonical_json,
                serde_json::to_string(report).expect("canonical report JSON")
            );
            self.terminal_payload_was_durable
                .store(true, Ordering::SeqCst);
        }
    }

    #[derive(Clone)]
    struct FakeAdapter {
        expected_journal: PathBuf,
        start_after_journal: Arc<AtomicBool>,
        cancel_calls: Arc<AtomicUsize>,
        cancellation_evidence: CancellationEvidence,
        recovery_observation: RecoveryObservation,
        reconcile_fails: bool,
        completion_actual_execution: tack_orch::execution::ActualExecution,
        // Overridable so a test can prove the engine reads a real
        // `terminal_reason.artifact` object (the exact shape the real
        // harness adapters already stage) without touching any other test's fixed
        // expectation of the plain `{code, message}` default.
        completion_terminal_reason: serde_json::Value,
        // Zero for every existing test (no behavior change): `wait()` only
        // sleeps when a test explicitly sets this, to simulate a harness
        // that outlives one or more lease-renewal intervals.
        wait_delay: std::time::Duration,
    }

    #[async_trait]
    impl HarnessAdapter for FakeAdapter {
        async fn validate(&self, _spec: &ExecutionSpec) -> Result<(), HarnessError> {
            Ok(())
        }

        async fn start(&self, _spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
            self.start_after_journal
                .store(self.expected_journal.exists(), Ordering::SeqCst);
            Ok(LocalRunHandle {
                process_id: "fake-process".into(),
            })
        }

        async fn cancel(
            &self,
            _handle: &LocalRunHandle,
        ) -> Result<CancellationEvidence, HarnessError> {
            self.cancel_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.cancellation_evidence.clone())
        }

        async fn wait(&self, _handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
            if !self.wait_delay.is_zero() {
                tokio::time::sleep(self.wait_delay).await;
            }
            Ok(HarnessOutcome {
                terminal_state: AttemptState::Succeeded,
                terminal_reason: self.completion_terminal_reason.clone(),
                final_checkpoint: None,
                actual_execution: self.completion_actual_execution.clone(),
                usage: usage(),
            })
        }

        async fn reconcile(
            &self,
            _journal: &AttemptJournal,
        ) -> Result<RecoveryObservation, HarnessError> {
            if self.reconcile_fails {
                Err(HarnessError::RecoveryUnavailable)
            } else {
                Ok(self.recovery_observation)
            }
        }
    }

    #[derive(Clone)]
    struct FakeWorktree {
        expected_journal: PathBuf,
        provision_after_journal: Arc<AtomicBool>,
    }

    #[async_trait]
    impl WorktreeProvisioner for FakeWorktree {
        async fn provision(
            &self,
            _workspace: &Workspace,
            _repository: &super::super::RepositorySpec,
        ) -> Result<(), WorkspaceError> {
            self.provision_after_journal
                .store(self.expected_journal.exists(), Ordering::SeqCst);
            Ok(())
        }
    }

    /// A scratch directory that removes itself, and everything written under
    /// it, when the returned guard drops — including when an assertion panics
    /// first.
    fn temporary_root(label: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(label)
            .tempdir()
            .expect("temporary directory")
    }

    fn session() -> RunnerSession {
        RunnerSession::new(
            RunnerId::new("runner"),
            RunnerCredential::new("never-log"),
            Timestamp::new("2026-08-06T13:00:00Z"),
        )
    }

    fn capabilities() -> tack_orch::execution::RunnerCapabilities {
        serde_json::from_str(
            r#"{
                "runner_version":"test-runner",
                "reported_at":"2026-08-06T12:00:00Z",
                "labels":{},
                "concurrency":{"total":1,"available":1},
                "harnesses":[],
                "features":{
                    "cancel":{"support":"supported","reason":null},
                    "resume":{"support":"unsupported","reason":"no resume"},
                    "decisions":{"support":"supported","reason":null},
                    "artifacts":{"support":"supported","reason":null},
                    "usage":{"support":"advisory","reason":"partial"}
                },
                "limits":{"event_payload_bytes_max":65536,"artifact_content_bytes_max":52428800}
            }"#,
        )
        .expect("capabilities fixture")
    }

    fn actual_execution() -> tack_orch::execution::ActualExecution {
        serde_json::from_str(
            r#"{
                "harness_kind":"fake",
                "harness_version":"1.0.0",
                "model_provider":"test-provider",
                "model_id":"test-model",
                "model_observation_source":"harness_reported",
                "capability_snapshot":{
                    "cancel":{"support":"supported","reason":null},
                    "resume":{"support":"unsupported","reason":"no resume"},
                    "decisions":{"support":"supported","reason":null},
                    "artifacts":{"support":"supported","reason":null},
                    "usage":{"support":"advisory","reason":"partial"}
                },
                "workspace_id":"ws_617474656d7074",
                "base_revision":"revision",
                "started_at":"2026-08-06T12:20:00Z",
                "ended_at":"2026-08-06T12:25:00Z"
            }"#,
        )
        .expect("actual execution fixture")
    }

    fn usage() -> tack_orch::execution::Usage {
        serde_json::from_str(
            r#"{
                "tokens_in":{"value":1,"source":"measured"},
                "tokens_out":{"value":2,"source":"measured"},
                "duration_ms":{"value":3,"source":"measured"},
                "cost_usd":{"value":null,"source":"not_measured"}
            }"#,
        )
        .expect("usage fixture")
    }

    fn mismatched_actual_execution() -> tack_orch::execution::ActualExecution {
        let mut actual = actual_execution();
        actual.workspace_id = tack_orch::execution::WorkspaceId::new("ws_adapter_mismatch");
        actual.base_revision = "adapter-mismatch".into();
        actual
    }

    fn work() -> ClaimedWork {
        let claim: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/contracts/runner-v1/claim.response.json"
        ))
        .expect("claim fixture");
        let mut request: ExecutionRequestSnapshot =
            serde_json::from_value(claim["request"].clone()).expect("request fixture");
        let mut attempt: AttemptSnapshot =
            serde_json::from_value(claim["attempt"].clone()).expect("attempt fixture");
        // Keep the production fixture's full shape while aligning the shared
        // facts with this focused engine fixture.
        request.repository.base_revision = "revision".into();
        attempt.attempt_id = DomainAttemptId::new("attempt");
        attempt.runner_id = DomainRunnerId::new("runner");
        attempt.base_revision = request.repository.base_revision.clone();
        ClaimedWork {
            claim_request_id: ClaimRequestId::new("claim"),
            lease: AttemptLease {
                attempt_id: AttemptId::new("attempt"),
                runner_id: RunnerId::new("runner"),
                fencing_token: FencingToken(7),
                attempt_number: 1,
                state: AttemptState::Leased,
                issued_at: Timestamp::new("2026-08-06T12:20:00Z"),
                expires_at: Timestamp::new("2026-08-06T12:21:00Z"),
            },
            request,
            attempt,
        }
    }

    fn protocol(
        work: ClaimedWork,
        cancellation_requested: bool,
        stale_completion: bool,
    ) -> FakeProtocol {
        FakeProtocol {
            claim: Arc::new(Mutex::new(Some(ClaimResult::Work(Box::new(work))))),
            cancellation_requested,
            heartbeat_echo_matches: Arc::new(AtomicBool::new(true)),
            accepted_heartbeat_ids: Arc::new(Mutex::new(BTreeSet::new())),
            stale_completion: Arc::new(AtomicBool::new(stale_completion)),
            fail_running_start: false,
            fail_cancellation_report: Arc::new(AtomicBool::new(false)),
            start_reports: Arc::new(AtomicUsize::new(0)),
            reported_starts: Arc::new(Mutex::new(Vec::new())),
            received_heartbeats: Arc::new(Mutex::new(Vec::new())),
            reported_heartbeats: Arc::new(Mutex::new(Vec::new())),
            completion_reports: Arc::new(AtomicUsize::new(0)),
            reported_completions: Arc::new(Mutex::new(Vec::new())),
            completion_response: Arc::new(Mutex::new(CompletionResponseConfig {
                replayed: false,
                mismatch: CompletionAckMismatch::None,
            })),
            fail_completion_ack_update: Arc::new(Mutex::new(None)),
            cancellation_reports: Arc::new(AtomicUsize::new(0)),
            reported_cancellations: Arc::new(Mutex::new(Vec::new())),
            cancellation_response: Arc::new(Mutex::new(CancellationResponseConfig {
                replayed: false,
                mismatch: CancellationAckMismatch::None,
            })),
            fail_cancellation_ack_update: Arc::new(Mutex::new(None)),
            terminal_journal_at_send: Arc::new(Mutex::new(None)),
            terminal_payload_was_durable: Arc::new(AtomicBool::new(false)),
            recovery_reports: Arc::new(AtomicUsize::new(0)),
            reported_recoveries: Arc::new(Mutex::new(Vec::new())),
            recovery_failures_remaining: Arc::new(AtomicUsize::new(0)),
            recovery_error: Arc::new(Mutex::new(None)),
            recovery_response: Arc::new(Mutex::new(RecoveryResponseConfig {
                disposition: RecoveryDisposition::SafePreSpawnRequeue,
                replayed: false,
                additional: BTreeMap::new(),
            })),
            refresh_requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn adapter(expected_journal: PathBuf) -> FakeAdapter {
        FakeAdapter {
            expected_journal,
            start_after_journal: Arc::new(AtomicBool::new(false)),
            cancel_calls: Arc::new(AtomicUsize::new(0)),
            cancellation_evidence: CancellationEvidence {
                observation: CancelObservation::ProcessStopped,
                observed_at: Timestamp::new("2026-08-06T12:24:00Z"),
                details: serde_json::Map::from_iter([
                    ("exit_code".into(), serde_json::json!(130)),
                    ("signal".into(), serde_json::json!("SIGTERM")),
                ]),
            },
            recovery_observation: RecoveryObservation::ProcessStopped,
            reconcile_fails: false,
            completion_actual_execution: actual_execution(),
            completion_terminal_reason: serde_json::json!({
                "code": "completed",
                "message": "Harness exited successfully"
            }),
            wait_delay: std::time::Duration::ZERO,
        }
    }

    #[test]
    fn fixture_shaped_claim_preserves_snapshots_and_rejects_divergent_workspace_facts() {
        let work = work();
        assert_eq!(
            work.request
                .requested_model_id
                .as_ref()
                .map(|id| id.as_str()),
            Some("opaque/model-alpha")
        );
        assert_eq!(
            work.attempt.request_id.as_str(),
            work.request.request_id.as_str()
        );
        assert_eq!(
            work.attempt.attempt_id.as_str(),
            work.lease.attempt_id.as_str()
        );
        let repository = work.workspace_repository().expect("matching claim facts");
        assert_eq!(repository.remote, "https://example.invalid/org/repo.git");
        assert_eq!(repository.base_revision, "revision");

        let mut revision_mismatch = work.clone();
        revision_mismatch.attempt.base_revision = "other-revision".into();
        assert!(matches!(
            revision_mismatch.workspace_repository(),
            Err(super::super::ClaimedWorkError::RepositoryRevisionMismatch)
        ));

        let mut lease_mismatch = work;
        lease_mismatch.lease.fencing_token = FencingToken(8);
        assert!(matches!(
            lease_mismatch.workspace_repository(),
            Err(super::super::ClaimedWorkError::AttemptLeaseMismatch)
        ));
    }

    #[test]
    fn completion_report_round_trips_the_frozen_terminal_payload_shape() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/contracts/runner-v1/completion.request.json"
        ))
        .expect("completion fixture");
        let report: CompletionReport =
            serde_json::from_value(fixture.clone()).expect("typed completion fixture");
        assert_eq!(report.protocol_version.as_u16(), 1);
        assert_eq!(report.runner_id.as_str(), "runr_01J00000000000000000000001");
        assert_eq!(
            report.terminal_reason,
            serde_json::json!({
                "code": "completed",
                "message": "Harness exited successfully"
            })
        );
        assert_eq!(
            serde_json::to_value(report).expect("serialize completion fixture"),
            fixture,
            "outbox JSON field names and values remain fixture-authoritative"
        );
    }

    #[test]
    fn cancellation_report_round_trips_the_frozen_terminal_payload_shape() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/contracts/runner-v1/cancellation.request.json"
        ))
        .expect("cancellation fixture");
        let report: CancellationReport =
            serde_json::from_value(fixture.clone()).expect("typed cancellation fixture");
        assert_eq!(report.observation, CancelObservation::ProcessStopped);
        assert_eq!(report.observed_at.as_str(), "2026-08-06T12:24:00Z");
        assert_eq!(
            serde_json::to_value(report).expect("serialize cancellation fixture"),
            fixture,
            "outbox JSON field names and values remain fixture-authoritative"
        );
    }

    #[test]
    fn heartbeat_dtos_round_trip_the_frozen_v1_payloads_and_reject_other_versions() {
        let request_fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/contracts/runner-v1/heartbeat.request.json"
        ))
        .expect("heartbeat request fixture");
        let request: HeartbeatRequest =
            serde_json::from_value(request_fixture.clone()).expect("typed heartbeat request");
        assert_eq!(
            serde_json::to_value(request).expect("serialize heartbeat request"),
            request_fixture
        );

        let response_fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/contracts/runner-v1/heartbeat.response.json"
        ))
        .expect("heartbeat response fixture");
        let response: super::super::HeartbeatResponse =
            serde_json::from_value(response_fixture.clone()).expect("typed heartbeat response");
        assert_eq!(
            serde_json::to_value(response).expect("serialize heartbeat response"),
            response_fixture
        );

        let mut unsupported = response_fixture;
        unsupported["protocol_version"] = serde_json::json!(2);
        assert!(serde_json::from_value::<super::super::HeartbeatResponse>(unsupported).is_err());
    }

    #[test]
    fn heartbeat_retries_keep_a_canonical_payload_for_the_same_clock_instant() {
        let root_dir = temporary_root("heartbeat-canonical-retry");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let fixed_at: SystemTime = chrono::DateTime::parse_from_rfc3339("2026-08-06T12:20:15Z")
            .expect("timestamp")
            .into();
        let engine = RunnerEngine::with_clock(
            protocol(work(), false, false),
            adapter(journal.journal_path(&AttemptId::new("attempt"))),
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
            FixedClock(fixed_at),
        );
        let claimed = work();
        let record = AttemptJournal::prepared(
            &claimed.lease,
            super::super::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws_617474656d7074"),
                path: root.join("workspaces/617474656d7074"),
                base_revision: "revision".into(),
            },
        );
        let first = engine.heartbeat_request(&session(), &record);
        let retry = engine.heartbeat_request(&session(), &record);
        assert_eq!(
            serde_json::to_string(&first).expect("first heartbeat"),
            serde_json::to_string(&retry).expect("retry heartbeat")
        );
        assert_eq!(first.sent_at.as_str(), "2026-08-06T12:20:15Z");
    }

    #[tokio::test]
    async fn periodic_heartbeats_advance_ids_without_replay_conflicts() {
        let root_dir = temporary_root("periodic-heartbeat-ids");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let protocol = protocol(work(), false, false);
        let base: SystemTime = chrono::DateTime::parse_from_rfc3339("2026-08-06T12:20:15Z")
            .expect("timestamp")
            .into();
        let engine = RunnerEngine::with_clock(
            protocol.clone(),
            adapter(journal.journal_path(&AttemptId::new("attempt"))),
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
            AdvancingClock {
                base,
                calls: Arc::new(AtomicUsize::new(0)),
            },
        );
        let claimed = work();
        let record = AttemptJournal::prepared(
            &claimed.lease,
            super::super::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws_617474656d7074"),
                path: root.join("workspaces/617474656d7074"),
                base_revision: "revision".into(),
            },
        );
        let first = engine.heartbeat_request(&session(), &record);
        let second = engine.heartbeat_request(&session(), &record);

        assert_ne!(first.heartbeat_id, second.heartbeat_id);
        assert_ne!(first.sent_at, second.sent_at);
        protocol
            .heartbeat(&session(), first)
            .await
            .expect("first logical heartbeat accepted");
        protocol
            .heartbeat(&session(), second)
            .await
            .expect("advanced logical heartbeat is not a replay conflict");
    }

    #[tokio::test]
    async fn heartbeat_sent_at_comes_from_the_injected_clock() {
        let root_dir = temporary_root("heartbeat-clock");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let protocol = protocol(work(), false, false);
        let fixed_at: SystemTime = chrono::DateTime::parse_from_rfc3339("2026-08-06T12:20:15Z")
            .expect("timestamp")
            .into();
        let engine = RunnerEngine::with_clock(
            protocol.clone(),
            adapter(journal.journal_path(&AttemptId::new("attempt"))),
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
            FixedClock(fixed_at),
        );
        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::Completed { .. }
        ));
        let heartbeats = protocol
            .received_heartbeats
            .lock()
            .expect("fake protocol lock");
        assert_eq!(heartbeats[0].protocol_version, ProtocolVersion::v1());
        assert_eq!(heartbeats[0].runner_id.as_str(), "runner");
        assert_eq!(heartbeats[0].sent_at.as_str(), "2026-08-06T12:20:15Z");
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    /// Acceptance: a harness that outlives several lease-renewal intervals
    /// still gets its lease renewed throughout, not just once at the start.
    /// Uses tokio's paused/auto-advancing clock so the wait genuinely spans
    /// multiple [`LEASE_RENEWAL_INTERVAL`] ticks without the test itself
    /// taking minutes; `FakeAdapter::wait`'s own sleep and the engine's
    /// renewal sleep race on the same virtual clock, so the assertion is
    /// exercising the real `tokio::select!` loop, not a mocked timer.
    #[tokio::test(start_paused = true)]
    async fn wait_periodically_renews_the_lease_while_the_harness_still_runs() {
        let root_dir = temporary_root("lease-renewal");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let protocol = protocol(work(), false, false);
        let long_running_adapter = FakeAdapter {
            wait_delay: LEASE_RENEWAL_INTERVAL * 3 + std::time::Duration::from_secs(1),
            ..adapter(journal.journal_path(&AttemptId::new("attempt")))
        };
        // `AdvancingClock` (not the default `SystemClock`) so each heartbeat
        // this test observes carries a distinct `sent_at`/`heartbeat_id`:
        // tokio's paused clock advances *tokio* timers, not `SystemTime`, so
        // a real per-call clock is needed for the renewal loop's repeated
        // heartbeats to be distinguishable rather than rejected as replays.
        let base: SystemTime = chrono::DateTime::parse_from_rfc3339("2026-08-06T12:20:15Z")
            .expect("timestamp")
            .into();
        let engine = RunnerEngine::with_clock(
            protocol.clone(),
            long_running_adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
            AdvancingClock {
                base,
                calls: Arc::new(AtomicUsize::new(0)),
            },
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::Completed { .. }
        ));

        // One heartbeat right after start, plus at least three renewals
        // while `wait()` was still pending.
        let heartbeat_count = protocol
            .received_heartbeats
            .lock()
            .expect("fake protocol lock")
            .len();
        assert!(
            heartbeat_count >= 4,
            "expected the lease to be renewed periodically during a long wait, got {heartbeat_count} heartbeat(s)"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn mismatched_heartbeat_echo_quarantines_before_applying_lease_facts() {
        let root_dir = temporary_root("heartbeat-echo-mismatch");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let protocol = protocol(work(), true, false);
        protocol
            .heartbeat_echo_matches
            .store(false, Ordering::SeqCst);
        let cancellations = Arc::new(AtomicUsize::new(0));
        let mut fake_adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        fake_adapter.cancel_calls = Arc::clone(&cancellations);
        let engine = RunnerEngine::new(
            protocol.clone(),
            fake_adapter,
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );
        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::Quarantined { .. }
        ));
        assert_eq!(cancellations.load(Ordering::SeqCst), 1);
        assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 0);
        assert_eq!(protocol.completion_reports.load(Ordering::SeqCst), 0);
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn tampered_terminal_outbox_bindings_are_rejected_before_replay_transport() {
        for tamper in [
            "journal_runner",
            "runner",
            "completion_id",
            "workspace",
            "cancellation_id",
        ] {
            let root_dir = temporary_root(tamper);
            let root = root_dir.path();
            let journal = OwnerOnlyJournal::new(root);
            let lease = work().lease;
            let mut record = AttemptJournal::prepared(
                &lease,
                super::super::WorkspaceJournal {
                    workspace_id: super::super::WorkspaceId::new("ws_617474656d7074"),
                    path: root.join("workspaces/617474656d7074"),
                    base_revision: "revision".into(),
                },
            );
            record.state = JournalState::TerminalReportPending;
            if tamper == "journal_runner" {
                record.runner_id = RunnerId::new("other-runner");
            }
            record.pending_terminal_report = Some(match tamper {
                "cancellation_id" => {
                    let report = CancellationReport {
                        protocol_version: ProtocolVersion::v1(),
                        runner_id: session().runner_id,
                        cancellation_request_id: CancellationRequestId::new("wrong-cancel"),
                        attempt_id: record.attempt_id.clone(),
                        fencing_token: record.fencing_token,
                        observation: CancelObservation::ProcessStopped,
                        observed_at: Timestamp::new("2026-08-06T12:24:00Z"),
                        details: serde_json::Map::new(),
                    };
                    PendingTerminalReport {
                        kind: PendingTerminalReportKind::Cancellation,
                        canonical_json: serde_json::to_string(&report).expect("cancel payload"),
                    }
                }
                _ => {
                    let mut report = CompletionReport {
                        protocol_version: ProtocolVersion::v1(),
                        runner_id: session().runner_id,
                        completion_id: CompletionId::new("completion:attempt:7"),
                        attempt_id: record.attempt_id.clone(),
                        fencing_token: record.fencing_token,
                        terminal_state: AttemptState::Succeeded,
                        terminal_reason: serde_json::json!({"code":"completed"}),
                        final_event_checkpoint: None,
                        actual_execution: actual_execution(),
                        usage: usage(),
                    };
                    match tamper {
                        "journal_runner" => {}
                        "runner" => report.runner_id = RunnerId::new("other-runner"),
                        "completion_id" => report.completion_id = CompletionId::new("wrong"),
                        "workspace" => {
                            report.actual_execution.workspace_id =
                                tack_orch::execution::WorkspaceId::new("ws_wrong")
                        }
                        _ => unreachable!(),
                    }
                    PendingTerminalReport {
                        kind: PendingTerminalReportKind::Completion,
                        canonical_json: serde_json::to_string(&report).expect("completion payload"),
                    }
                }
            });
            journal
                .persist_before_spawn(&record)
                .expect("tampered pending journal");
            let protocol = protocol(work(), false, false);
            let engine = RunnerEngine::new(
                protocol.clone(),
                adapter(journal.journal_path(&AttemptId::new("attempt"))),
                journal,
                WorkspaceManager::new(
                    root.join("workspaces"),
                    FakeWorktree {
                        expected_journal: OwnerOnlyJournal::new(root)
                            .journal_path(&AttemptId::new("attempt")),
                        provision_after_journal: Arc::new(AtomicBool::new(false)),
                    },
                ),
            );
            assert!(matches!(
                engine.recover(&session()).await,
                Err(EngineError::Journal(JournalError::Malformed))
            ));
            assert_eq!(protocol.completion_reports.load(Ordering::SeqCst), 0);
            assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 0);
            std::fs::remove_dir_all(root).expect("remove temporary root");
        }
    }

    fn claim_request() -> ClaimRequest {
        ClaimRequest {
            claim_request_id: ClaimRequestId::new("claim"),
            available_capacity: 1,
            wait: std::time::Duration::ZERO,
        }
    }

    #[tokio::test]
    async fn refresh_carries_capabilities_and_returns_expiring_session() {
        let root_dir = temporary_root("refresh");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let protocol = protocol(work(), false, false);
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter(journal.journal_path(&AttemptId::new("attempt"))),
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );
        let response = engine
            .refresh(
                &session(),
                RefreshRequest {
                    runner_name: "runner".into(),
                    runner_version: "test-runner".into(),
                    rotate_credential: true,
                    capabilities: capabilities(),
                },
            )
            .await
            .expect("refresh");

        assert_eq!(
            response.session.credential_expires_at().as_str(),
            "2026-08-06T13:00:00Z"
        );
        assert_eq!(response.accepted_at.as_str(), "2026-08-06T12:30:00Z");
        let refreshes = protocol
            .refresh_requests
            .lock()
            .expect("fake protocol lock");
        assert_eq!(refreshes.len(), 1);
        assert!(refreshes[0].rotate_credential);
        assert_eq!(refreshes[0].capabilities.runner_version, "test-runner");
    }

    #[tokio::test]
    async fn cancellation_is_coordinated_after_journal_precedes_spawn() {
        let root_dir = temporary_root("cancel");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let expected = journal.journal_path(&AttemptId::new("attempt"));
        let protocol = protocol(work(), true, false);
        let adapter = adapter(expected);
        let started = Arc::clone(&adapter.start_after_journal);
        let cancellations = Arc::clone(&adapter.cancel_calls);
        let provisioned = Arc::new(AtomicBool::new(false));
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::clone(&provisioned),
                },
            ),
        );

        let result = engine
            .run_once(&session(), claim_request())
            .await
            .expect("cycle");
        assert!(matches!(result, RunCycle::Cancelled { .. }));
        assert!(
            started.load(Ordering::SeqCst),
            "journal existed before adapter start"
        );
        assert!(
            provisioned.load(Ordering::SeqCst),
            "journal existed before worktree provision"
        );
        assert_eq!(protocol.start_reports.load(Ordering::SeqCst), 2);
        let start_reports = protocol.reported_starts.lock().expect("fake protocol lock");
        let preparing = start_reports
            .iter()
            .find(|report| report.phase == StartPhase::Preparing)
            .expect("preparing report");
        assert_eq!(
            preparing.workspace_id.as_ref().map(|id| id.as_str()),
            Some("ws_617474656d7074")
        );
        assert_eq!(preparing.base_revision.as_deref(), Some("revision"));
        assert_eq!(preparing.process_id, None);
        let running = start_reports
            .iter()
            .find(|report| report.phase == StartPhase::ProcessObservedRunning)
            .expect("running report");
        assert_eq!(
            running.workspace_id.as_ref().map(|id| id.as_str()),
            Some("ws_617474656d7074")
        );
        assert_eq!(running.base_revision.as_deref(), Some("revision"));
        assert_eq!(running.process_id.as_deref(), Some("fake-process"));
        let heartbeats = protocol
            .reported_heartbeats
            .lock()
            .expect("fake protocol lock");
        assert_eq!(heartbeats.len(), 1);
        assert!(heartbeats[0].heartbeat_id.starts_with("hb_"));
        assert_eq!(heartbeats[0].accepted_at.as_str(), "2026-08-06T12:20:16Z");
        assert_eq!(
            heartbeats[0].lease_results[0].lease_expires_at.as_str(),
            "2026-08-06T12:21:16Z"
        );
        assert_eq!(cancellations.load(Ordering::SeqCst), 1);
        assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 1);
        let cancellation_reports = protocol
            .reported_cancellations
            .lock()
            .expect("fake protocol lock");
        assert_eq!(cancellation_reports.len(), 1);
        let report = &cancellation_reports[0];
        assert_eq!(report.protocol_version.as_u16(), 1);
        assert_eq!(report.runner_id.as_str(), "runner");
        assert_eq!(report.attempt_id.as_str(), "attempt");
        assert_eq!(report.fencing_token.0, 7);
        assert_eq!(report.cancellation_request_id.as_str(), "cancel:attempt:7");
        assert_eq!(report.observation, CancelObservation::ProcessStopped);
        assert_eq!(report.observed_at.as_str(), "2026-08-06T12:24:00Z");
        assert_eq!(report.details["exit_code"], serde_json::json!(130));
        assert_eq!(report.details["signal"], serde_json::json!("SIGTERM"));
        assert_eq!(protocol.completion_reports.load(Ordering::SeqCst), 0);
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn replayed_cancellation_ack_settles_stopped_evidence() {
        let root_dir = temporary_root("replayed-cancellation");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let protocol = protocol(work(), true, false);
        protocol
            .cancellation_response
            .lock()
            .expect("fake protocol lock")
            .replayed = true;
        let first_adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            first_adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::Cancelled { .. }
        ));
        assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 1);
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 0);
        assert_eq!(
            journal
                .load(&AttemptId::new("attempt"))
                .expect("journal")
                .state,
            JournalState::Reported
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn mismatched_cancellation_ack_stays_in_terminal_outbox() {
        for (label, mismatch) in [
            ("cancel-mismatch-attempt", CancellationAckMismatch::Attempt),
            ("cancel-mismatch-request", CancellationAckMismatch::Request),
            ("cancel-mismatch-state", CancellationAckMismatch::State),
        ] {
            let root_dir = temporary_root(label);
            let root = root_dir.path();
            let journal = OwnerOnlyJournal::new(root);
            let protocol = protocol(work(), true, false);
            protocol
                .cancellation_response
                .lock()
                .expect("fake protocol lock")
                .mismatch = mismatch;
            let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
            let engine = RunnerEngine::new(
                protocol.clone(),
                adapter,
                journal.clone(),
                WorkspaceManager::new(
                    root.join("workspaces"),
                    FakeWorktree {
                        expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                        provision_after_journal: Arc::new(AtomicBool::new(false)),
                    },
                ),
            );

            assert!(matches!(
                engine
                    .run_once(&session(), claim_request())
                    .await
                    .expect("cycle"),
                RunCycle::TerminalReportPending { .. }
            ));
            assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 1);
            assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 0);
            let pending = journal.load(&AttemptId::new("attempt")).expect("journal");
            assert_eq!(pending.state, JournalState::TerminalReportPending);
            assert!(pending.pending_terminal_report.is_some());
            std::fs::remove_dir_all(root).expect("remove temporary root");
        }
    }

    #[tokio::test]
    async fn non_stopped_cancellation_evidence_skips_cancellation_transport() {
        for (label, observation) in [
            (
                "cancel-already-terminal",
                CancelObservation::AlreadyTerminal,
            ),
            ("cancel-ambiguous", CancelObservation::Ambiguous),
        ] {
            let root_dir = temporary_root(label);
            let root = root_dir.path();
            let journal = OwnerOnlyJournal::new(root);
            let protocol = protocol(work(), true, false);
            let mut adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
            adapter.cancellation_evidence.observation = observation;
            let engine = RunnerEngine::new(
                protocol.clone(),
                adapter,
                journal.clone(),
                WorkspaceManager::new(
                    root.join("workspaces"),
                    FakeWorktree {
                        expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                        provision_after_journal: Arc::new(AtomicBool::new(false)),
                    },
                ),
            );

            assert!(matches!(
                engine
                    .run_once(&session(), claim_request())
                    .await
                    .expect("cycle"),
                RunCycle::Quarantined { .. }
            ));
            assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 0);
            assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
            assert!(journal.unresolved().expect("scanned journal").is_empty());
            std::fs::remove_dir_all(root).expect("remove temporary root");
        }
    }

    #[tokio::test]
    async fn completion_transport_loss_stays_in_terminal_outbox() {
        let root_dir = temporary_root("stale");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let protocol = protocol(work(), false, true);
        let mut adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        adapter.completion_actual_execution = mismatched_actual_execution();
        let cancellations = Arc::clone(&adapter.cancel_calls);
        let provisioned = Arc::new(AtomicBool::new(false));
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: provisioned,
                },
            ),
        );

        let result = engine
            .run_once(&session(), claim_request())
            .await
            .expect("cycle");
        assert!(matches!(result, RunCycle::TerminalReportPending { .. }));
        assert_eq!(
            protocol.completion_reports.load(Ordering::SeqCst),
            1,
            "no retry after stale fence"
        );
        let completions = protocol
            .reported_completions
            .lock()
            .expect("fake protocol lock");
        assert_eq!(completions.len(), 1);
        assert_eq!(
            completions[0].actual_execution.workspace_id.as_str(),
            "ws_617474656d7074"
        );
        assert_eq!(completions[0].actual_execution.base_revision, "revision");
        assert_eq!(completions[0].usage.duration_ms.value, Some(3));
        assert_eq!(completions[0].terminal_state, AttemptState::Succeeded);
        assert_eq!(
            completions[0].terminal_reason,
            serde_json::json!({
                "code": "completed",
                "message": "Harness exited successfully"
            })
        );
        assert_eq!(completions[0].final_event_checkpoint, None);
        assert_eq!(cancellations.load(Ordering::SeqCst), 0);
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 0);
        let pending = journal.load(&AttemptId::new("attempt")).expect("journal");
        assert_eq!(pending.state, JournalState::TerminalReportPending);
        assert!(pending.pending_terminal_report.is_some());
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn completion_outbox_replays_exact_payload_after_response_loss_without_respawn() {
        let root_dir = temporary_root("completion-outbox-replay");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let protocol = protocol(work(), false, true);
        *protocol
            .terminal_journal_at_send
            .lock()
            .expect("fake protocol lock") = Some(journal.clone());
        let first_adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            first_adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("first delivery"),
            RunCycle::TerminalReportPending { .. }
        ));
        let workspace_path = root.join("workspaces/617474656d7074");
        assert!(
            workspace_path.exists(),
            "pending replay retains the workspace"
        );
        assert!(
            protocol.terminal_payload_was_durable.load(Ordering::SeqCst),
            "terminal payload is fsynced before the first send"
        );
        let first_payload = journal
            .load(&AttemptId::new("attempt"))
            .expect("pending journal")
            .pending_terminal_report
            .expect("pending completion")
            .canonical_json;
        assert!(!first_payload.contains("never-log"));
        protocol.stale_completion.store(false, Ordering::SeqCst);
        protocol
            .completion_response
            .lock()
            .expect("fake protocol lock")
            .replayed = true;
        let restarted_adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let never_respawned = Arc::clone(&restarted_adapter.start_after_journal);
        let restarted = RunnerEngine::new(
            protocol.clone(),
            restarted_adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            restarted
                .recover(&session())
                .await
                .expect("replay")
                .as_slice(),
            [RunCycle::Completed { .. }]
        ));
        assert_eq!(protocol.completion_reports.load(Ordering::SeqCst), 2);
        let sent = protocol
            .reported_completions
            .lock()
            .expect("fake protocol lock");
        assert_eq!(
            serde_json::to_string(&sent[0]).expect("first payload"),
            serde_json::to_string(&sent[1]).expect("replayed payload")
        );
        assert_eq!(
            serde_json::to_string(&sent[1]).expect("replayed payload"),
            first_payload
        );
        assert!(!never_respawned.load(Ordering::SeqCst));
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 0);
        let settled = journal
            .load(&AttemptId::new("attempt"))
            .expect("settled journal");
        assert_eq!(settled.state, JournalState::Reported);
        assert!(settled.pending_terminal_report.is_none());
        assert!(
            !workspace_path.exists(),
            "restart replay cleans only after the Reported acknowledgement"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn completion_bad_ack_stays_in_terminal_outbox() {
        for (label, mismatch) in [
            (
                "completion-mismatch-attempt",
                CompletionAckMismatch::Attempt,
            ),
            ("completion-mismatch-id", CompletionAckMismatch::Completion),
            ("completion-mismatch-state", CompletionAckMismatch::State),
        ] {
            let root_dir = temporary_root(label);
            let root = root_dir.path();
            let journal = OwnerOnlyJournal::new(root);
            let protocol = protocol(work(), false, false);
            protocol
                .completion_response
                .lock()
                .expect("fake protocol lock")
                .mismatch = mismatch;
            let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
            let engine = RunnerEngine::new(
                protocol.clone(),
                adapter,
                journal.clone(),
                WorkspaceManager::new(
                    root.join("workspaces"),
                    FakeWorktree {
                        expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                        provision_after_journal: Arc::new(AtomicBool::new(false)),
                    },
                ),
            );

            assert!(matches!(
                engine
                    .run_once(&session(), claim_request())
                    .await
                    .expect("cycle"),
                RunCycle::TerminalReportPending { .. }
            ));
            assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 0);
            let pending = journal.load(&AttemptId::new("attempt")).expect("journal");
            assert_eq!(pending.state, JournalState::TerminalReportPending);
            std::fs::remove_dir_all(root).expect("remove temporary root");
        }
    }

    #[tokio::test]
    async fn completion_ack_then_journal_failure_replays_pending_payload() {
        let root_dir = temporary_root("completion-ack-write-failure");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let protocol = protocol(work(), false, false);
        *protocol
            .fail_completion_ack_update
            .lock()
            .expect("fake protocol lock") = Some(journal.clone());
        let first_adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            first_adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("ack write failure"),
            RunCycle::TerminalReportPending { .. }
        ));
        let workspace_path = root.join("workspaces/617474656d7074");
        assert!(
            workspace_path.exists(),
            "pending replay retains the workspace"
        );
        assert_eq!(protocol.completion_reports.load(Ordering::SeqCst), 1);
        assert_eq!(
            journal
                .load(&AttemptId::new("attempt"))
                .expect("pending journal")
                .state,
            JournalState::TerminalReportPending
        );
        let restarted = RunnerEngine::new(
            protocol.clone(),
            adapter(journal.journal_path(&AttemptId::new("attempt"))),
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );
        assert!(matches!(
            restarted
                .recover(&session())
                .await
                .expect("replay")
                .as_slice(),
            [RunCycle::Completed { .. }]
        ));
        assert_eq!(protocol.completion_reports.load(Ordering::SeqCst), 2);
        assert!(
            !workspace_path.exists(),
            "restart replay cleans only after the Reported acknowledgement"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn restart_reports_unresolved_journal_observation_without_respawn() {
        let root_dir = temporary_root("recovery");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let lease = work().lease;
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("persist prior journal");
        let protocol = protocol(work(), false, false);
        let first_adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            first_adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        let outcomes = engine.recover(&session()).await.expect("recover");
        assert!(matches!(outcomes.as_slice(), [RunCycle::Completed { .. }]));
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
        let recoveries = protocol
            .reported_recoveries
            .lock()
            .expect("fake protocol lock");
        assert_eq!(recoveries.len(), 1);
        assert_eq!(
            recoveries[0].recovery_key.as_str(),
            "recovery:attempt:7:process_stopped"
        );
        assert_eq!(recoveries[0].protocol_version.as_u16(), 1);
        assert_eq!(recoveries[0].runner_id.as_str(), "runner");
        assert_eq!(recoveries[0].attempt_id.as_str(), "attempt");
        assert_eq!(recoveries[0].fencing_token.0, 7);
        assert_eq!(
            recoveries[0].details.journal_state,
            RecoveryJournalState::Prepared
        );
        assert!(!recoveries[0].details.process_observed);
        assert!(recoveries[0].additional.is_empty());
        assert!(recoveries[0].details.additional.is_empty());
        assert_eq!(
            journal
                .load(&AttemptId::new("attempt"))
                .expect("journal")
                .state,
            JournalState::RecoveryObserved
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn needs_operator_response_durably_quarantines_stopped_pre_spawn_recovery() {
        let root_dir = temporary_root("needs-operator-recovery");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let lease = work().lease;
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        let protocol = protocol(work(), false, false);
        protocol
            .recovery_response
            .lock()
            .expect("fake protocol lock")
            .disposition = RecoveryDisposition::NeedsOperator;
        let first_adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            first_adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .recover(&session())
                .await
                .expect("recovery")
                .as_slice(),
            [RunCycle::Quarantined { .. }]
        ));
        assert!(journal.unresolved().expect("scanned journal").is_empty());
        assert!(
            root.join("quarantine")
                .read_dir()
                .expect("quarantine")
                .next()
                .is_some(),
            "operator disposition moves evidence out of restart scans"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn stale_lease_on_recovery_retires_the_record_and_keeps_the_checkout() {
        let root_dir = temporary_root("stale-lease-recovery");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let lease = work().lease;
        let workspace_path = root.join("workspaces/attempt");
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: workspace_path.clone(),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        // Stands in for the checkout a real harness would have left behind;
        // nothing in this path is supposed to touch it.
        std::fs::create_dir_all(&workspace_path).expect("fake checkout");
        std::fs::write(workspace_path.join("evidence.txt"), b"operator evidence")
            .expect("fake checkout file");
        let protocol = protocol(work(), false, false);
        *protocol.recovery_error.lock().expect("fake protocol lock") =
            Some(ProtocolClientError::StaleLease);
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter(journal.journal_path(&AttemptId::new("attempt"))),
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .recover(&session())
                .await
                .expect("recovery")
                .as_slice(),
            [RunCycle::Quarantined { .. }]
        ));
        assert!(
            journal.unresolved().expect("scanned journal").is_empty(),
            "an attempt the server has no lease for is retired from the restart scan, not rescanned"
        );
        assert!(
            root.join("quarantine")
                .read_dir()
                .expect("quarantine")
                .next()
                .is_some(),
            "the record is retired into quarantine, not deleted outright"
        );
        assert!(
            workspace_path.join("evidence.txt").exists(),
            "retiring the record must not destroy the checkout an operator would need"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn unreachable_server_on_recovery_never_retires_the_record() {
        let root_dir = temporary_root("unreachable-recovery");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let lease = work().lease;
        let workspace_path = root.join("workspaces/attempt");
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: workspace_path.clone(),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        std::fs::create_dir_all(&workspace_path).expect("fake checkout");
        std::fs::write(workspace_path.join("evidence.txt"), b"operator evidence")
            .expect("fake checkout file");
        let protocol = protocol(work(), false, false);
        // A transport failure means the server was never reached at all --
        // as distinct from `StaleLease` above, this must never settle
        // anything. Proven across two separate restarts, not one, so a
        // fluke single-boot pass can't hide a "quarantine after N tries"
        // regression.
        *protocol.recovery_error.lock().expect("fake protocol lock") =
            Some(ProtocolClientError::Transport);

        for attempt_number in 0..2 {
            let engine = RunnerEngine::new(
                protocol.clone(),
                adapter(journal.journal_path(&AttemptId::new("attempt"))),
                journal.clone(),
                WorkspaceManager::new(
                    root.join("workspaces"),
                    FakeWorktree {
                        expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                        provision_after_journal: Arc::new(AtomicBool::new(false)),
                    },
                ),
            );
            assert!(
                matches!(
                    engine
                        .recover(&session())
                        .await
                        .expect("recovery")
                        .as_slice(),
                    [RunCycle::RecoveryPending { .. }]
                ),
                "restart {attempt_number} must stay pending, never quarantined, on a bare transport failure"
            );
        }
        assert_eq!(
            journal.unresolved().expect("scanned journal").len(),
            1,
            "an unanswered server is never grounds to retire the record"
        );
        assert!(
            root.join("quarantine").read_dir().is_err()
                || root
                    .join("quarantine")
                    .read_dir()
                    .expect("quarantine")
                    .next()
                    .is_none(),
            "a transport failure must never move the record into quarantine"
        );
        assert!(
            workspace_path.join("evidence.txt").exists(),
            "the checkout is untouched while recovery is still unresolved"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn replayed_already_terminal_response_settles_only_stopped_evidence() {
        let root_dir = temporary_root("terminal-replay-recovery");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let lease = work().lease;
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        let protocol = protocol(work(), false, false);
        {
            let mut response = protocol
                .recovery_response
                .lock()
                .expect("fake protocol lock");
            response.disposition = RecoveryDisposition::AlreadyTerminal;
            response.replayed = true;
            response
                .additional
                .insert("future_response_field".into(), serde_json::json!(42));
        }
        let first_adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            first_adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .recover(&session())
                .await
                .expect("recovery")
                .as_slice(),
            [RunCycle::Completed { .. }]
        ));
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
        assert_eq!(
            journal
                .load(&AttemptId::new("attempt"))
                .expect("journal")
                .state,
            JournalState::RecoveryObserved
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn already_terminal_response_quarantines_running_or_ambiguous_evidence() {
        for (label, running, reconcile_fails) in [
            ("terminal-running-recovery", true, false),
            ("terminal-ambiguous-recovery", false, true),
        ] {
            let root_dir = temporary_root(label);
            let root = root_dir.path();
            let journal = OwnerOnlyJournal::new(root);
            let lease = work().lease;
            let record = AttemptJournal::prepared(
                &lease,
                super::super::journal::WorkspaceJournal {
                    workspace_id: super::super::WorkspaceId::new("ws"),
                    path: root.join("workspaces/attempt"),
                    base_revision: "revision".into(),
                },
            );
            journal
                .persist_before_spawn(&record)
                .expect("prior journal");
            let protocol = protocol(work(), false, false);
            protocol
                .recovery_response
                .lock()
                .expect("fake protocol lock")
                .disposition = RecoveryDisposition::AlreadyTerminal;
            let mut adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
            adapter.reconcile_fails = reconcile_fails;
            if running {
                adapter.recovery_observation = RecoveryObservation::ProcessRunning;
            }
            let engine = RunnerEngine::new(
                protocol,
                adapter,
                journal.clone(),
                WorkspaceManager::new(
                    root.join("workspaces"),
                    FakeWorktree {
                        expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                        provision_after_journal: Arc::new(AtomicBool::new(false)),
                    },
                ),
            );

            assert!(matches!(
                engine
                    .recover(&session())
                    .await
                    .expect("recovery")
                    .as_slice(),
                [RunCycle::Quarantined { .. }]
            ));
            assert!(journal.unresolved().expect("scanned journal").is_empty());
            std::fs::remove_dir_all(root).expect("remove temporary root");
        }
    }

    #[tokio::test]
    async fn safe_requeue_response_never_settles_post_spawn_stopped_evidence() {
        let root_dir = temporary_root("safe-post-spawn-recovery");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let lease = work().lease;
        let mut record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        record.state = JournalState::ProcessObservedRunning;
        record.process_id = Some("former-process".into());
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        let protocol = protocol(work(), false, false);
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol,
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .recover(&session())
                .await
                .expect("recovery")
                .as_slice(),
            [RunCycle::Quarantined { .. }]
        ));
        assert!(journal.unresolved().expect("scanned journal").is_empty());
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn post_spawn_start_ack_failure_reports_ambiguity_and_quarantines() {
        let root_dir = temporary_root("start-ack");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let mut protocol = protocol(work(), false, false);
        protocol.fail_running_start = true;
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::Quarantined { .. }
        ));
        assert_eq!(protocol.start_reports.load(Ordering::SeqCst), 2);
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
        assert_eq!(protocol.completion_reports.load(Ordering::SeqCst), 0);
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn cancellation_transport_loss_stays_in_terminal_outbox() {
        let root_dir = temporary_root("cancel-report");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let protocol = protocol(work(), true, false);
        protocol
            .fail_cancellation_report
            .store(true, Ordering::SeqCst);
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::TerminalReportPending { .. }
        ));
        assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 1);
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 0);
        let pending = journal.load(&AttemptId::new("attempt")).expect("journal");
        assert_eq!(pending.state, JournalState::TerminalReportPending);
        assert!(pending.pending_terminal_report.is_some());
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn cancellation_outbox_replays_exact_payload_after_response_loss_without_respawn() {
        let root_dir = temporary_root("cancellation-outbox-replay");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let protocol = protocol(work(), true, false);
        protocol
            .fail_cancellation_report
            .store(true, Ordering::SeqCst);
        *protocol
            .terminal_journal_at_send
            .lock()
            .expect("fake protocol lock") = Some(journal.clone());
        let first_adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            first_adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("first delivery"),
            RunCycle::TerminalReportPending { .. }
        ));
        let workspace_path = root.join("workspaces/617474656d7074");
        assert!(
            workspace_path.exists(),
            "pending replay retains the workspace"
        );
        assert!(protocol.terminal_payload_was_durable.load(Ordering::SeqCst));
        let first_payload = journal
            .load(&AttemptId::new("attempt"))
            .expect("pending journal")
            .pending_terminal_report
            .expect("pending cancellation")
            .canonical_json;
        protocol
            .fail_cancellation_report
            .store(false, Ordering::SeqCst);
        protocol
            .cancellation_response
            .lock()
            .expect("fake protocol lock")
            .replayed = true;
        let restarted_adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let never_respawned = Arc::clone(&restarted_adapter.start_after_journal);
        let restarted = RunnerEngine::new(
            protocol.clone(),
            restarted_adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            restarted
                .recover(&session())
                .await
                .expect("replay")
                .as_slice(),
            [RunCycle::Cancelled { .. }]
        ));
        assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 2);
        let sent = protocol
            .reported_cancellations
            .lock()
            .expect("fake protocol lock");
        assert_eq!(
            serde_json::to_string(&sent[0]).expect("first payload"),
            serde_json::to_string(&sent[1]).expect("replayed payload")
        );
        assert_eq!(
            serde_json::to_string(&sent[1]).expect("replayed payload"),
            first_payload
        );
        assert!(!never_respawned.load(Ordering::SeqCst));
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 0);
        let settled = journal
            .load(&AttemptId::new("attempt"))
            .expect("settled journal");
        assert_eq!(settled.state, JournalState::Reported);
        assert!(settled.pending_terminal_report.is_none());
        assert!(
            !workspace_path.exists(),
            "restart replay cleans only after the Reported acknowledgement"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn cancellation_ack_then_journal_failure_replays_pending_payload() {
        let root_dir = temporary_root("cancellation-ack-write-failure");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let protocol = protocol(work(), true, false);
        *protocol
            .fail_cancellation_ack_update
            .lock()
            .expect("fake protocol lock") = Some(journal.clone());
        let first_adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let engine = RunnerEngine::new(
            protocol.clone(),
            first_adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("ack write failure"),
            RunCycle::TerminalReportPending { .. }
        ));
        assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 1);
        assert_eq!(
            journal
                .load(&AttemptId::new("attempt"))
                .expect("pending journal")
                .state,
            JournalState::TerminalReportPending
        );
        let workspace_path = root.join("workspaces/617474656d7074");
        assert!(
            workspace_path.exists(),
            "ack write failure retains the workspace"
        );
        let restarted = RunnerEngine::new(
            protocol.clone(),
            adapter(journal.journal_path(&AttemptId::new("attempt"))),
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );
        assert!(matches!(
            restarted
                .recover(&session())
                .await
                .expect("replay")
                .as_slice(),
            [RunCycle::Cancelled { .. }]
        ));
        assert_eq!(protocol.cancellation_reports.load(Ordering::SeqCst), 2);
        assert!(
            !workspace_path.exists(),
            "restart replay cleans only after the Reported acknowledgement"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn failed_ambiguity_delivery_is_retried_on_restart_without_respawn() {
        let root_dir = temporary_root("retry-recovery");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let lease = work().lease;
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        let protocol = protocol(work(), false, false);
        protocol
            .recovery_failures_remaining
            .store(1, Ordering::SeqCst);
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let never_started = Arc::clone(&adapter.start_after_journal);
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .recover(&session())
                .await
                .expect("first recovery")
                .as_slice(),
            [RunCycle::RecoveryPending { .. }]
        ));
        assert_eq!(journal.unresolved().expect("still scanned").len(), 1);
        assert!(matches!(
            engine
                .recover(&session())
                .await
                .expect("second recovery")
                .as_slice(),
            [RunCycle::Completed { .. }]
        ));
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 2);
        assert!(
            !never_started.load(Ordering::SeqCst),
            "recovery never respawns"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn running_recovery_observation_is_quarantined_not_completed() {
        let root_dir = temporary_root("running-recovery");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let lease = work().lease;
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        let protocol = protocol(work(), false, false);
        let mut adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        adapter.recovery_observation = RecoveryObservation::ProcessRunning;
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: journal.journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .recover(&session())
                .await
                .expect("recovery")
                .as_slice(),
            [RunCycle::Quarantined { .. }]
        ));
        assert!(journal.unresolved().expect("scanned journal").is_empty());
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn duplicate_claim_for_quarantined_attempt_cannot_start_again() {
        let root_dir = temporary_root("duplicate-quarantine");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let lease = work().lease;
        let record = AttemptJournal::prepared(
            &lease,
            super::super::journal::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws"),
                path: root.join("workspaces/attempt"),
                base_revision: "revision".into(),
            },
        );
        journal
            .persist_before_spawn(&record)
            .expect("prior journal");
        journal.quarantine(&record).expect("quarantine");
        let protocol = protocol(work(), false, false);
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let started = Arc::clone(&adapter.start_after_journal);
        let engine = RunnerEngine::new(
            protocol,
            adapter,
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine.run_once(&session(), claim_request()).await,
            Err(EngineError::Journal(JournalError::AlreadyExists))
        ));
        assert!(
            !started.load(Ordering::SeqCst),
            "duplicate claim did not start a process"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn post_spawn_journal_update_failure_reports_ambiguity_and_cancels() {
        let root_dir = temporary_root("journal-update");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        journal.fail_next_update_for_test();
        let protocol = protocol(work(), false, false);
        let adapter = adapter(journal.journal_path(&AttemptId::new("attempt")));
        let cancellations = Arc::clone(&adapter.cancel_calls);
        let engine = RunnerEngine::new(
            protocol.clone(),
            adapter,
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::Quarantined { .. }
        ));
        assert_eq!(protocol.recovery_reports.load(Ordering::SeqCst), 1);
        assert_eq!(cancellations.load(Ordering::SeqCst), 1);
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    // -----------------------------------------------------------------
    // The engine actually submits events, decisions and
    // artifacts through `AttemptDataProtocol`, and a resubmission is
    // idempotent. `FakeDataProtocol` below is a second fake transport,
    // deliberately independent of `FakeProtocol` (which only ever
    // implements `PullProtocol`) — proving `RunnerEngine::with_data_protocol`
    // wires a genuinely separate seam, not a coincidental reuse of the
    // lifecycle fake.
    // -----------------------------------------------------------------

    #[derive(Default)]
    struct FakeDataProtocolState {
        events: Vec<EventBatchReport>,
        accepted_event_ids: BTreeSet<String>,
        manifests: Vec<ArtifactManifestReport>,
        uploads: Vec<(String, Vec<u8>, Option<String>)>,
    }

    #[derive(Clone, Default)]
    struct FakeDataProtocol {
        state: Arc<Mutex<FakeDataProtocolState>>,
        events_fail: Arc<AtomicBool>,
        manifest_fails: Arc<AtomicBool>,
        upload_fails: Arc<AtomicBool>,
    }

    impl FakeDataProtocol {
        fn new() -> Self {
            Self::default()
        }
    }

    #[async_trait]
    impl AttemptDataProtocol for FakeDataProtocol {
        async fn submit_events(
            &self,
            _session: &RunnerSession,
            report: EventBatchReport,
        ) -> Result<EventBatchResponse, ProtocolClientError> {
            if self.events_fail.load(Ordering::SeqCst) {
                return Err(ProtocolClientError::Transport);
            }
            let mut state = self.state.lock().expect("fake data protocol lock");
            let mut accepted_event_ids = Vec::new();
            let mut duplicate_event_ids = Vec::new();
            for event in &report.events {
                // Mirrors the server's own idempotency contract
                // (`docs/contracts/runner-v1/event-batch.*`, unique
                // `(attempt_id, event_id)`): a previously seen id is a
                // no-op duplicate, never a second row.
                if state.accepted_event_ids.insert(event.event_id.clone()) {
                    accepted_event_ids.push(event.event_id.clone());
                } else {
                    duplicate_event_ids.push(event.event_id.clone());
                }
            }
            let response = EventBatchResponse {
                attempt_id: report.attempt_id.clone(),
                accepted_event_ids,
                duplicate_event_ids,
                committed_checkpoint: Some(report.checkpoint.clone()),
            };
            state.events.push(report);
            Ok(response)
        }

        async fn create_decision(
            &self,
            _session: &RunnerSession,
            report: DecisionCreateReport,
        ) -> Result<DecisionCreateResponse, ProtocolClientError> {
            Ok(DecisionCreateResponse {
                decision_id: report.decision_id,
                state: "open".into(),
                created_at: Timestamp::new("2026-08-20T00:00:00Z"),
            })
        }

        async fn poll_decisions(
            &self,
            _session: &RunnerSession,
            _report: DecisionPollReport,
        ) -> Result<DecisionPollResponse, ProtocolClientError> {
            Ok(DecisionPollResponse {
                decisions: Vec::new(),
                next_after: None,
            })
        }

        async fn submit_artifact_manifest(
            &self,
            _session: &RunnerSession,
            report: ArtifactManifestReport,
        ) -> Result<Vec<ArtifactUploadGrant>, ProtocolClientError> {
            if self.manifest_fails.load(Ordering::SeqCst) {
                return Err(ProtocolClientError::Transport);
            }
            let grants = report
                .artifacts
                .iter()
                .map(|item| ArtifactUploadGrant {
                    artifact_id: item.artifact_id.clone(),
                    state: "manifest_accepted".into(),
                    method: "PUT".into(),
                    path: format!("/api/runner/v1/artifacts/{}/content", item.artifact_id),
                    expires_at: Some(Timestamp::new("2026-08-20T01:00:00Z")),
                })
                .collect();
            self.state
                .lock()
                .expect("fake data protocol lock")
                .manifests
                .push(report);
            Ok(grants)
        }

        async fn put_artifact_content(
            &self,
            _session: &RunnerSession,
            _fencing_token: FencingToken,
            grant: &ArtifactUploadGrant,
            media_type: Option<&str>,
            content: Vec<u8>,
        ) -> Result<(), ProtocolClientError> {
            if self.upload_fails.load(Ordering::SeqCst) {
                return Err(ProtocolClientError::Transport);
            }
            self.state
                .lock()
                .expect("fake data protocol lock")
                .uploads
                .push((
                    grant.artifact_id.clone(),
                    content,
                    media_type.map(str::to_owned),
                ));
            Ok(())
        }
    }

    /// Builds an engine identical to the other happy-path tests but with a
    /// [`FakeDataProtocol`] attached, and an adapter whose staged artifact
    /// points at a real file this test writes itself — so the assertions
    /// below read genuine bytes/sha256 back out, never a value the test
    /// merely asserts against itself.
    fn engine_with_data_protocol(
        root: &Path,
        journal: OwnerOnlyJournal,
        data_protocol: FakeDataProtocol,
        completion_terminal_reason: serde_json::Value,
    ) -> RunnerEngine<FakeProtocol, FakeAdapter, FakeWorktree> {
        RunnerEngine::new(
            protocol(work(), false, false),
            FakeAdapter {
                completion_terminal_reason,
                ..adapter(journal.journal_path(&AttemptId::new("attempt")))
            },
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        )
        .with_data_protocol(Arc::new(data_protocol))
    }

    #[tokio::test]
    async fn run_once_with_a_data_protocol_submits_the_terminal_event_and_uploads_the_staged_artifact()
     {
        let root_dir = temporary_root("data-protocol-terminal");
        let root = root_dir.path();
        std::fs::create_dir_all(root).expect("test root");
        let staged_path = root.join("staged-artifact.log");
        let content = b"real staged artifact bytes, not a placeholder".to_vec();
        std::fs::write(&staged_path, &content).expect("write staged artifact");
        let sha256 = crate::harness::sha256::sha256_hex(&content);

        let journal = OwnerOnlyJournal::new(root);
        let data_protocol = FakeDataProtocol::new();
        let engine = engine_with_data_protocol(
            root,
            journal,
            data_protocol.clone(),
            serde_json::json!({
                "code": "completed",
                "message": "Harness exited successfully",
                "artifact": {
                    "kind": "log",
                    "name": "staged-artifact.log",
                    "media_type": "text/plain",
                    "size_bytes": content.len(),
                    "sha256": sha256,
                    "staged_path": staged_path.display().to_string(),
                }
            }),
        );

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::Completed { .. }
        ));

        let state = data_protocol.state.lock().expect("fake data protocol lock");
        assert_eq!(state.events.len(), 1, "exactly one event batch submitted");
        let submitted = &state.events[0];
        assert_eq!(submitted.events.len(), 1);
        assert_eq!(submitted.events[0].kind, "attempt.terminal");
        assert_eq!(submitted.events[0].source, "runner");
        assert_eq!(submitted.events[0].payload["code"], "completed");
        assert_eq!(submitted.previous_checkpoint, None, "first submission ever");
        assert!(
            state
                .accepted_event_ids
                .contains(&submitted.events[0].event_id)
        );

        assert_eq!(state.manifests.len(), 1, "exactly one artifact manifest");
        let manifest_item = &state.manifests[0].artifacts[0];
        assert_eq!(manifest_item.sha256, sha256);
        assert_eq!(manifest_item.size_bytes, content.len() as u64);
        assert_eq!(manifest_item.name, "staged-artifact.log");

        assert_eq!(state.uploads.len(), 1, "exactly one artifact upload");
        assert_eq!(state.uploads[0].0, manifest_item.artifact_id);
        assert_eq!(
            state.uploads[0].1, content,
            "the exact bytes read from the staged file were uploaded"
        );
        assert_eq!(state.uploads[0].2.as_deref(), Some("text/plain"));

        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    #[tokio::test]
    async fn run_once_with_a_data_protocol_submits_a_cancellation_event() {
        let root_dir = temporary_root("data-protocol-cancellation");
        let root = root_dir.path();
        std::fs::create_dir_all(root).expect("test root");
        let journal = OwnerOnlyJournal::new(root);
        let data_protocol = FakeDataProtocol::new();
        let engine = RunnerEngine::new(
            protocol(work(), true, false),
            adapter(journal.journal_path(&AttemptId::new("attempt"))),
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        )
        .with_data_protocol(Arc::new(data_protocol.clone()));

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::Cancelled { .. }
        ));

        let state = data_protocol.state.lock().expect("fake data protocol lock");
        assert_eq!(state.events.len(), 1);
        assert_eq!(state.events[0].events[0].kind, "attempt.cancelled");
        assert_eq!(
            state.events[0].events[0].payload["observation"],
            serde_json::json!("process_stopped")
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    /// No `AttemptDataProtocol` configured is exactly the behavior
    /// without the seam at all — the attempt still completes, and nothing about
    /// the lifecycle depends on the new seam being present.
    #[tokio::test]
    async fn without_a_data_protocol_the_attempt_still_completes_and_nothing_is_submitted() {
        let root_dir = temporary_root("data-protocol-absent");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let engine = RunnerEngine::new(
            protocol(work(), false, false),
            adapter(journal.journal_path(&AttemptId::new("attempt"))),
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        );
        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle"),
            RunCycle::Completed { .. }
        ));
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    /// Acceptance: a transport failure on the events/artifacts seam is
    /// real (logged, ids only — no payload) and never turns into a hidden
    /// fake success, but it also never blocks the attempt's own terminal
    /// report — the harness genuinely succeeded, and that fact must still
    /// reach the server even if this best-effort evidence upload could not.
    #[tokio::test]
    async fn data_protocol_transport_failure_does_not_block_the_attempts_own_completion() {
        let root_dir = temporary_root("data-protocol-failure");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let data_protocol = FakeDataProtocol::new();
        data_protocol.events_fail.store(true, Ordering::SeqCst);
        let engine = RunnerEngine::new(
            protocol(work(), false, false),
            adapter(journal.journal_path(&AttemptId::new("attempt"))),
            journal,
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        )
        .with_data_protocol(Arc::new(data_protocol.clone()));

        assert!(matches!(
            engine
                .run_once(&session(), claim_request())
                .await
                .expect("cycle — harness success is not held hostage by evidence upload"),
            RunCycle::Completed { .. }
        ));
        assert!(
            data_protocol
                .state
                .lock()
                .expect("fake data protocol lock")
                .events
                .is_empty(),
            "the failed submission attempt left no accepted event behind"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }

    /// Retrying the
    /// identical terminal-event submission — the exact scenario a runner
    /// hits after a transient transport error, or a crash between a
    /// server-side commit and the local checkpoint write — reuses the same
    /// `event_id` both times (never a random or clock-derived one), so the
    /// server-side dedup this fake mirrors treats the retry as a genuine
    /// no-op: the accepted set never grows past one member.
    #[tokio::test]
    async fn resubmitting_the_same_terminal_event_is_idempotent() {
        let root_dir = temporary_root("data-protocol-idempotent-retry");
        let root = root_dir.path();
        let journal = OwnerOnlyJournal::new(root);
        let data_protocol = FakeDataProtocol::new();
        let engine = RunnerEngine::new(
            protocol(work(), false, false),
            adapter(journal.journal_path(&AttemptId::new("attempt"))),
            journal.clone(),
            WorkspaceManager::new(
                root.join("workspaces"),
                FakeWorktree {
                    expected_journal: OwnerOnlyJournal::new(root)
                        .journal_path(&AttemptId::new("attempt")),
                    provision_after_journal: Arc::new(AtomicBool::new(false)),
                },
            ),
        )
        .with_data_protocol(Arc::new(data_protocol.clone()));

        let claimed = work();
        let mut record = AttemptJournal::prepared(
            &claimed.lease,
            super::super::WorkspaceJournal {
                workspace_id: super::super::WorkspaceId::new("ws_617474656d7074"),
                path: root.join("workspaces/617474656d7074"),
                base_revision: "revision".into(),
            },
        );

        let first_payload = serde_json::json!({"code": "completed"});
        engine
            .submit_event(&session(), &mut record, "attempt.terminal", first_payload)
            .await;
        let second_payload = serde_json::json!({"code": "completed"});
        engine
            .submit_event(&session(), &mut record, "attempt.terminal", second_payload)
            .await;

        let state = data_protocol.state.lock().expect("fake data protocol lock");
        assert_eq!(state.events.len(), 2, "the engine did submit twice");
        let first_event_id = &state.events[0].events[0].event_id;
        let second_event_id = &state.events[1].events[0].event_id;
        assert_eq!(
            first_event_id, second_event_id,
            "identical (attempt_id, fencing_token, kind) must yield the identical event_id"
        );
        assert_eq!(
            state.accepted_event_ids.len(),
            1,
            "server-side dedup (mirrored by the fake) sees one logical event, not two"
        );
        std::fs::remove_dir_all(root).expect("remove temporary root");
    }
}
