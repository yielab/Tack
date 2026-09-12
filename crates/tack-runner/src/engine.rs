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
        match self.adapter.validate(&spec).await {
            Ok(()) => {}
            Err(HarnessError::Rejected { reason }) => {
                return self
                    .fail_before_spawn(session, &mut record, &spec, reason)
                    .await;
            }
            Err(error) => return Err(error.into()),
        }
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

    /// Turns a harness rejection that arrived after the attempt was already
    /// announced as `preparing` into a reported failure, through the same
    /// durable outbox every other terminal report goes through.
    ///
    /// Propagating the rejection as an error instead leaves the server holding
    /// an attempt in `preparing` under a lease nothing will ever heartbeat or
    /// complete, and the journal holding a `prepared` record with no process
    /// behind it — a hang only a later restart's recovery scan would resolve,
    /// with nothing anywhere saying why. The rejection is settled information:
    /// the adapter looked at the request and refused it before any process
    /// existed, so the honest report is `failed` with that refusal as the
    /// reason. Only [`HarnessError::Rejected`] is settled in this sense; a
    /// process or recovery failure says nothing about the request itself and
    /// keeps propagating.
    ///
    /// Nothing ran, and the report says so in the contract's own vocabulary:
    /// no harness version was observed, the model is the requested one marked
    /// as never confirmed (or `unknown`/`not_observed` when none was
    /// requested), every feature capability is `unsupported` because none was
    /// exercised, and every usage figure is `not_measured`.
    async fn fail_before_spawn(
        &self,
        session: &RunnerSession,
        record: &mut AttemptJournal,
        spec: &ExecutionSpec,
        reason: String,
    ) -> Result<RunCycle, EngineError> {
        use tack_orch::execution as domain;

        let now = chrono::DateTime::<chrono::Utc>::from(self.clock.now());
        let request = &spec.work.request;
        let unexercised = || domain::CapabilityValue {
            support: domain::CapabilitySupport::Unsupported,
            reason: Some("the harness rejected this execution before it started".to_owned()),
            additional: BTreeMap::new(),
        };
        fn not_measured<T>() -> domain::Measurement<T> {
            domain::Measurement {
                value: None,
                source: domain::MeasurementSource::NotMeasured,
                additional: BTreeMap::new(),
            }
        }
        let (model_provider, model_id, model_observation_source) = match (
            request.requested_model_provider.as_ref(),
            request.requested_model_id.as_ref(),
        ) {
            (Some(provider), Some(model)) => (
                provider.as_str().to_owned(),
                model.as_str().to_owned(),
                crate::harness::ModelObservationSource::RequestedNotConfirmed,
            ),
            _ => (
                "unknown".to_owned(),
                "unknown".to_owned(),
                crate::harness::ModelObservationSource::NotObserved,
            ),
        };

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
            terminal_state: AttemptState::Failed,
            terminal_reason: serde_json::json!({
                "code": "harness_rejected",
                "message": reason,
            }),
            final_event_checkpoint: record.last_event_checkpoint.clone(),
            actual_execution: domain::ActualExecution {
                harness_kind: request.requested_harness_kind.clone(),
                harness_version: String::new(),
                model_provider: domain::ActualModelProvider::new(model_provider),
                model_id: domain::ActualModelId::new(model_id),
                model_observation_source: model_observation_source.as_str().to_owned(),
                capability_snapshot: domain::FeatureCapabilities {
                    cancel: unexercised(),
                    resume: unexercised(),
                    decisions: unexercised(),
                    artifacts: unexercised(),
                    usage: unexercised(),
                    additional: BTreeMap::new(),
                },
                workspace_id: domain::WorkspaceId::new(record.workspace.workspace_id.as_str()),
                base_revision: record.workspace.base_revision.clone(),
                started_at: now,
                ended_at: now,
                additional: BTreeMap::new(),
            },
            usage: domain::Usage {
                tokens_in: not_measured(),
                tokens_out: not_measured(),
                duration_ms: not_measured(),
                cost_usd: not_measured(),
                additional: BTreeMap::new(),
            },
        };
        tracing::warn!(
            attempt_id = %record.attempt_id.as_str(),
            "harness rejected the execution before it started; reporting the attempt as failed"
        );
        self.persist_pending_terminal_report(
            record,
            PendingTerminalReportKind::Completion,
            &report,
        )?;
        let cycle = self.send_pending_terminal_report(session, record).await?;
        if matches!(cycle, RunCycle::Completed { .. }) {
            let _ = self.workspaces.cleanup(&spec.workspace);
        }
        Ok(cycle)
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
#[path = "engine/tests.rs"]
mod tests;
