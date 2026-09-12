use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use tack_orch::execution::{
    AttemptId as DomainAttemptId, AttemptSnapshot, ExecutionRequestSnapshot, ProtocolVersion,
    RecoveryObservationRequest, RecoveryObservationResponse, RunnerId as DomainRunnerId,
};
use tack_runner::{
    EnrollmentCredential,
    client::{
        AttemptId, AttemptLease, AttemptState, CancelObservation, CancellationEvidence,
        CancellationReport, CancellationResponse, ClaimRequest, ClaimRequestId, ClaimResult,
        ClaimedWork, CompletionReport, CompletionResponse, EngineError, EnrollmentRequest,
        EnrollmentResponse, FencingToken, HarnessAdapter, HarnessError, HarnessOutcome,
        HeartbeatRequest, HeartbeatResponse, JournalError, JournalState, LeaseResult,
        LocalRunHandle, OwnerOnlyJournal, ProtocolClientError, PullProtocol, RecoveryObservation,
        RepositorySpec, RunCycle, RunnerCredential, RunnerEngine, RunnerId, RunnerSession,
        StartPhase, StartReport, Timestamp, Workspace, WorkspaceError, WorkspaceId,
        WorkspaceJournal, WorkspaceManager, WorktreeProvisioner,
    },
};

mod common;
use common::temp_dir as root;
use common::usage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailurePoint {
    None,
    PreparingAck,
    ProcessStartAck,
    Heartbeat,
    Completion,
    Cancellation,
}

#[derive(Debug, Default)]
struct ProtocolEvidence {
    events: Vec<String>,
    completion_reports: usize,
    cancellation_reports: usize,
    recovery_reports: Vec<RecoveryObservation>,
}

#[derive(Clone)]
struct FakeProtocol {
    work: Arc<Mutex<Option<ClaimedWork>>>,
    failure: FailurePoint,
    cancellation_requested: bool,
    evidence: Arc<Mutex<ProtocolEvidence>>,
    recovery_failures_remaining: Arc<AtomicUsize>,
}

impl FakeProtocol {
    fn new(work: ClaimedWork, failure: FailurePoint, cancellation_requested: bool) -> Self {
        Self {
            work: Arc::new(Mutex::new(Some(work))),
            failure,
            cancellation_requested,
            evidence: Arc::new(Mutex::new(ProtocolEvidence::default())),
            recovery_failures_remaining: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn event(&self, event: impl Into<String>) {
        self.evidence
            .lock()
            .expect("protocol evidence")
            .events
            .push(event.into());
    }

    fn fail_recovery_reports(&self, count: usize) {
        self.recovery_failures_remaining
            .store(count, Ordering::SeqCst);
    }
}

#[async_trait]
impl PullProtocol for FakeProtocol {
    async fn enroll(
        &self,
        _enrollment_credential: &EnrollmentCredential,
        _request: EnrollmentRequest,
    ) -> Result<EnrollmentResponse, ProtocolClientError> {
        Err(ProtocolClientError::Rejected)
    }

    async fn refresh(
        &self,
        _session: &RunnerSession,
        _request: tack_runner::client::RefreshRequest,
    ) -> Result<tack_runner::client::RefreshResponse, ProtocolClientError> {
        Err(ProtocolClientError::Rejected)
    }

    async fn claim(
        &self,
        _session: &RunnerSession,
        _request: ClaimRequest,
    ) -> Result<ClaimResult, ProtocolClientError> {
        self.event("claim_committed");
        self.work
            .lock()
            .expect("claim lock")
            .take()
            .map(|work| ClaimResult::Work(Box::new(work)))
            .ok_or(ProtocolClientError::Rejected)
    }

    async fn heartbeat(
        &self,
        _session: &RunnerSession,
        request: HeartbeatRequest,
    ) -> Result<HeartbeatResponse, ProtocolClientError> {
        self.event("heartbeat");
        if self.failure == FailurePoint::Heartbeat {
            return Err(ProtocolClientError::Transport);
        }
        let active = request
            .active_attempts
            .into_iter()
            .next()
            .expect("active attempt");
        Ok(HeartbeatResponse {
            protocol_version: ProtocolVersion::v1(),
            heartbeat_id: request.heartbeat_id,
            accepted_at: Timestamp::new("2026-08-07T12:00:01Z"),
            lease_results: vec![LeaseResult {
                attempt_id: active.attempt_id,
                fencing_token: active.fencing_token,
                lease_expires_at: Timestamp::new("2026-08-07T12:01:00Z"),
                cancellation_requested: self.cancellation_requested,
            }],
        })
    }

    async fn report_start(
        &self,
        _session: &RunnerSession,
        report: StartReport,
    ) -> Result<(), ProtocolClientError> {
        let (event, fails) = match report.phase {
            StartPhase::Preparing => (
                "start:preparing",
                self.failure == FailurePoint::PreparingAck,
            ),
            StartPhase::ProcessObservedRunning => (
                "start:process_observed_running",
                self.failure == FailurePoint::ProcessStartAck,
            ),
        };
        self.event(event);
        if fails {
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
        let mut evidence = self.evidence.lock().expect("protocol evidence");
        evidence.events.push("completion".into());
        evidence.completion_reports += 1;
        if self.failure == FailurePoint::Completion {
            Err(ProtocolClientError::Transport)
        } else {
            Ok(CompletionResponse {
                protocol_version: ProtocolVersion::v1(),
                attempt_id: report.attempt_id,
                completion_id: report.completion_id,
                state: report.terminal_state,
                replayed: false,
                committed_at: Timestamp::new("2026-08-07T12:00:01Z"),
            })
        }
    }

    async fn report_cancellation(
        &self,
        _session: &RunnerSession,
        report: CancellationReport,
    ) -> Result<CancellationResponse, ProtocolClientError> {
        let mut evidence = self.evidence.lock().expect("protocol evidence");
        evidence.events.push("cancellation_observation".into());
        evidence.cancellation_reports += 1;
        if self.failure == FailurePoint::Cancellation {
            Err(ProtocolClientError::Transport)
        } else {
            Ok(CancellationResponse {
                protocol_version: ProtocolVersion::v1(),
                attempt_id: report.attempt_id,
                cancellation_request_id: report.cancellation_request_id,
                state: AttemptState::Cancelled,
                replayed: false,
                committed_at: Timestamp::new("2026-08-07T12:00:01Z"),
            })
        }
    }

    async fn observe_recovery(
        &self,
        _session: &RunnerSession,
        report: RecoveryObservationRequest,
    ) -> Result<RecoveryObservationResponse, ProtocolClientError> {
        let mut evidence = self.evidence.lock().expect("protocol evidence");
        evidence
            .events
            .push(format!("recovery:{:?}", report.observation));
        evidence.recovery_reports.push(report.observation);
        drop(evidence);
        if self
            .recovery_failures_remaining
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok()
        {
            Err(ProtocolClientError::Transport)
        } else {
            let mut response: RecoveryObservationResponse = serde_json::from_str(include_str!(
                "../../../docs/contracts/runner-v1/recovery-observation.response.json"
            ))
            .expect("recovery fixture");
            response.attempt_id = report.attempt_id;
            response.recovery_key = report.recovery_key;
            Ok(response)
        }
    }
}

#[derive(Debug, Default)]
struct ProcessEvidence {
    starts: usize,
    cancels: usize,
    reconciles: usize,
}

#[derive(Clone)]
struct FakeAdapter {
    evidence: Arc<Mutex<ProcessEvidence>>,
    recovery: RecoveryObservation,
}

impl FakeAdapter {
    fn new(recovery: RecoveryObservation) -> Self {
        Self {
            evidence: Arc::new(Mutex::new(ProcessEvidence::default())),
            recovery,
        }
    }
}

#[async_trait]
impl HarnessAdapter for FakeAdapter {
    async fn validate(
        &self,
        _spec: &tack_runner::client::engine::ExecutionSpec,
    ) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn start(
        &self,
        _spec: &tack_runner::client::engine::ExecutionSpec,
    ) -> Result<LocalRunHandle, HarnessError> {
        self.evidence.lock().expect("process evidence").starts += 1;
        Ok(LocalRunHandle {
            process_id: "fake-process".into(),
        })
    }

    async fn cancel(&self, _handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        self.evidence.lock().expect("process evidence").cancels += 1;
        Ok(CancellationEvidence {
            observation: CancelObservation::ProcessStopped,
            observed_at: Timestamp::new("2026-08-07T12:00:01Z"),
            details: serde_json::Map::new(),
        })
    }

    async fn wait(&self, _handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        Ok(HarnessOutcome {
            terminal_state: AttemptState::Succeeded,
            terminal_reason: serde_json::json!({"code":"completed"}),
            final_checkpoint: None,
            actual_execution: actual_execution(),
            usage: usage(),
        })
    }

    async fn reconcile(
        &self,
        _journal: &tack_runner::client::AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        self.evidence.lock().expect("process evidence").reconciles += 1;
        Ok(self.recovery)
    }
}

#[derive(Clone)]
struct FakeWorktree {
    fail: bool,
    provisions: Arc<AtomicUsize>,
}

impl FakeWorktree {
    fn succeeds() -> Self {
        Self {
            fail: false,
            provisions: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn fails() -> Self {
        Self {
            fail: true,
            provisions: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl WorktreeProvisioner for FakeWorktree {
    async fn provision(
        &self,
        _workspace: &Workspace,
        _repository: &RepositorySpec,
    ) -> Result<(), WorkspaceError> {
        self.provisions.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(WorkspaceError::Io)
        } else {
            Ok(())
        }
    }
}

fn actual_execution() -> tack_orch::execution::ActualExecution {
    serde_json::from_str(
        r#"{
            "harness_kind":"fake", "harness_version":"1.0.0",
            "model_provider":"test-provider", "model_id":"test-model",
            "model_observation_source":"harness_reported",
            "capability_snapshot":{
                "cancel":{"support":"supported","reason":null},
                "resume":{"support":"unsupported","reason":"no resume"},
                "decisions":{"support":"supported","reason":null},
                "artifacts":{"support":"supported","reason":null},
                "usage":{"support":"advisory","reason":"partial"}
            },
            "workspace_id":"ws_617474656d70742d6372617368",
            "base_revision":"0123456789abcdef",
            "started_at":"2026-08-07T12:00:00Z", "ended_at":"2026-08-07T12:01:00Z"
        }"#,
    )
    .expect("actual execution")
}

fn work() -> ClaimedWork {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture");
    let mut request: ExecutionRequestSnapshot =
        serde_json::from_value(fixture["request"].clone()).expect("request snapshot");
    let mut attempt: AttemptSnapshot =
        serde_json::from_value(fixture["attempt"].clone()).expect("attempt snapshot");
    request.repository.base_revision = "0123456789abcdef".into();
    attempt.attempt_id = DomainAttemptId::new("attempt-crash");
    attempt.runner_id = DomainRunnerId::new("runner-crash");
    attempt.base_revision = request.repository.base_revision.clone();
    ClaimedWork {
        claim_request_id: ClaimRequestId::new("claim-crash"),
        lease: AttemptLease {
            attempt_id: AttemptId::new("attempt-crash"),
            runner_id: RunnerId::new("runner-crash"),
            fencing_token: FencingToken(7),
            attempt_number: 1,
            state: AttemptState::Leased,
            issued_at: Timestamp::new("2026-08-07T12:00:00Z"),
            expires_at: Timestamp::new("2026-08-07T12:01:00Z"),
        },
        request,
        attempt,
    }
}

fn session() -> RunnerSession {
    RunnerSession::new(
        RunnerId::new("runner-crash"),
        RunnerCredential::new("test-secret-never-log"),
        Timestamp::new("2026-08-07T13:00:00Z"),
    )
}

fn claim() -> ClaimRequest {
    ClaimRequest {
        claim_request_id: ClaimRequestId::new("claim-crash"),
        available_capacity: 1,
        wait: Duration::ZERO,
    }
}

#[tokio::test]
async fn after_claim_before_spawn_failure_recovers_as_process_stopped_without_respawn() {
    let root_dir = root("before-spawn");
    let root = root_dir.path();
    let protocol = FakeProtocol::new(work(), FailurePoint::None, false);
    let adapter = FakeAdapter::new(RecoveryObservation::ProcessStopped);
    let failed_worktree = FakeWorktree::fails();
    let journal = OwnerOnlyJournal::new(root);
    let engine = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), failed_worktree.clone()),
    );

    assert!(engine.run_once(&session(), claim()).await.is_err());
    assert_eq!(adapter.evidence.lock().expect("process evidence").starts, 0);
    assert_eq!(failed_worktree.provisions.load(Ordering::SeqCst), 1);
    let unresolved = journal.unresolved().expect("unresolved journal");
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].state, JournalState::Prepared);

    let recovery = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );
    let outcomes = recovery.recover(&session()).await.expect("recover");
    assert!(matches!(outcomes.as_slice(), [RunCycle::Completed { .. }]));
    let process = adapter.evidence.lock().expect("process evidence");
    assert_eq!(process.starts, 0, "recovery cannot respawn the harness");
    assert_eq!(process.reconciles, 1);
    drop(process);
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(
        evidence.recovery_reports,
        vec![RecoveryObservation::ProcessStopped]
    );
    assert!(
        evidence
            .events
            .iter()
            .any(|event| event == "start:preparing")
    );
    assert!(
        evidence
            .events
            .iter()
            .any(|event| event == "recovery:ProcessStopped")
    );
    drop(evidence);
}

#[tokio::test]
async fn after_spawn_before_ack_failure_is_audited_ambiguous_and_never_retried() {
    let root_dir = root("spawn-before-ack");
    let root = root_dir.path();
    let protocol = FakeProtocol::new(work(), FailurePoint::ProcessStartAck, false);
    let adapter = FakeAdapter::new(RecoveryObservation::Ambiguous);
    let journal = OwnerOnlyJournal::new(root);
    let engine = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );

    let result = engine
        .run_once(&session(), claim())
        .await
        .expect("quarantine result");
    assert!(matches!(result, RunCycle::Quarantined { .. }));
    let process = adapter.evidence.lock().expect("process evidence");
    assert_eq!(process.starts, 1);
    assert_eq!(process.cancels, 1);
    drop(process);
    assert!(journal.unresolved().expect("journal scan").is_empty());
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(
        evidence.recovery_reports,
        vec![RecoveryObservation::Ambiguous]
    );
    assert!(
        evidence
            .events
            .iter()
            .any(|event| event == "start:process_observed_running")
    );
    assert!(
        evidence
            .events
            .iter()
            .any(|event| event == "recovery:Ambiguous")
    );
    drop(evidence);
}

#[tokio::test]
async fn completion_response_loss_stays_in_terminal_outbox_without_duplicate_send() {
    let root_dir = root("completion");
    let root = root_dir.path();
    let protocol = FakeProtocol::new(work(), FailurePoint::Completion, false);
    let adapter = FakeAdapter::new(RecoveryObservation::Ambiguous);
    let journal = OwnerOnlyJournal::new(root);
    let engine = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );

    let result = engine
        .run_once(&session(), claim())
        .await
        .expect("terminal outbox result");
    assert!(matches!(result, RunCycle::TerminalReportPending { .. }));
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(
        evidence.completion_reports, 1,
        "completion is never blind-retried"
    );
    assert!(evidence.recovery_reports.is_empty());
    assert!(evidence.events.iter().any(|event| event == "completion"));
    drop(evidence);
    assert_eq!(journal.unresolved().expect("journal scan").len(), 1);
}

#[tokio::test]
async fn cancellation_response_loss_stays_in_terminal_outbox_without_duplicate_send() {
    let root_dir = root("cancellation");
    let root = root_dir.path();
    let protocol = FakeProtocol::new(work(), FailurePoint::Cancellation, true);
    let adapter = FakeAdapter::new(RecoveryObservation::Ambiguous);
    let journal = OwnerOnlyJournal::new(root);
    let engine = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );

    let result = engine
        .run_once(&session(), claim())
        .await
        .expect("terminal outbox result");
    assert!(matches!(result, RunCycle::TerminalReportPending { .. }));
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(evidence.cancellation_reports, 1);
    assert_eq!(evidence.completion_reports, 0);
    assert!(evidence.recovery_reports.is_empty());
    assert!(
        evidence
            .events
            .iter()
            .any(|event| event == "cancellation_observation")
    );
    drop(evidence);
    let process = adapter.evidence.lock().expect("process evidence");
    assert_eq!(process.starts, 1);
    assert_eq!(process.cancels, 1, "only the requested cancellation runs");
    drop(process);
    assert_eq!(journal.unresolved().expect("journal scan").len(), 1);
}

#[tokio::test]
async fn failed_ambiguity_report_stays_pending_then_restart_quarantines_without_respawn() {
    let root_dir = root("ambiguity-report-retry");
    let root = root_dir.path();
    let protocol = FakeProtocol::new(work(), FailurePoint::ProcessStartAck, false);
    protocol.fail_recovery_reports(1);
    let adapter = FakeAdapter::new(RecoveryObservation::Ambiguous);
    let journal = OwnerOnlyJournal::new(root);
    let engine = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );

    assert!(matches!(
        engine
            .run_once(&session(), claim())
            .await
            .expect("pending result"),
        RunCycle::RecoveryPending { .. }
    ));
    assert_eq!(
        journal.unresolved().expect("pending local evidence").len(),
        1,
        "failed report keeps the journal eligible for restart recovery"
    );
    assert_eq!(adapter.evidence.lock().expect("process evidence").starts, 1);

    let restarted = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );
    let outcomes = restarted
        .recover(&session())
        .await
        .expect("restart recovery");
    assert!(matches!(
        outcomes.as_slice(),
        [RunCycle::Quarantined { .. }]
    ));

    let process = adapter.evidence.lock().expect("process evidence");
    assert_eq!(
        process.starts, 1,
        "restart recovery must never launch again"
    );
    assert_eq!(process.reconciles, 1);
    assert_eq!(process.cancels, 1, "only the original post-spawn stop runs");
    drop(process);
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(
        evidence.recovery_reports,
        vec![
            RecoveryObservation::Ambiguous,
            RecoveryObservation::Ambiguous
        ],
        "the ambiguity report is retried exactly once after its failed delivery"
    );
    assert_eq!(
        evidence
            .events
            .iter()
            .filter(|event| event.as_str() == "start:process_observed_running")
            .count(),
        1
    );
    drop(evidence);
    assert!(
        journal
            .unresolved()
            .expect("post-quarantine scan")
            .is_empty()
    );
    assert!(
        root.join("quarantine")
            .read_dir()
            .expect("quarantine")
            .next()
            .is_some(),
        "server-acknowledged ambiguity is preserved as local quarantine evidence"
    );
}

#[tokio::test]
async fn process_running_recovery_observation_reports_ambiguity_and_quarantines_without_spawn() {
    let root_dir = root("process-running-recovery");
    let root = root_dir.path();
    let protocol = FakeProtocol::new(work(), FailurePoint::None, false);
    let adapter = FakeAdapter::new(RecoveryObservation::ProcessRunning);
    let journal = OwnerOnlyJournal::new(root);
    let lease = work().lease;
    journal
        .persist_before_spawn(&tack_runner::client::AttemptJournal::prepared(
            &lease,
            WorkspaceJournal {
                workspace_id: WorkspaceId::new("ws_crash"),
                path: root.join("workspaces/attempt-crash"),
                base_revision: "0123456789abcdef".into(),
            },
        ))
        .expect("persist prior journal");

    let engine = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );
    let outcomes = engine
        .recover(&session())
        .await
        .expect("recover running process");
    assert!(matches!(
        outcomes.as_slice(),
        [RunCycle::Quarantined { .. }]
    ));
    let process = adapter.evidence.lock().expect("process evidence");
    assert_eq!(
        process.starts, 0,
        "recovery must not spawn a second process"
    );
    assert_eq!(process.reconciles, 1);
    assert_eq!(
        process.cancels, 0,
        "no local handle exists to cancel on restart"
    );
    drop(process);
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(
        evidence.recovery_reports,
        vec![RecoveryObservation::ProcessRunning]
    );
    assert!(
        evidence
            .events
            .iter()
            .any(|event| event == "recovery:ProcessRunning"),
        "a running process is preserved as an audited recovery fact"
    );
    drop(evidence);
    assert!(
        journal
            .unresolved()
            .expect("post-quarantine scan")
            .is_empty()
    );
    assert!(
        root.join("quarantine")
            .read_dir()
            .expect("quarantine")
            .next()
            .is_some()
    );
}

#[tokio::test]
async fn reoffered_quarantined_attempt_is_rejected_before_a_second_spawn() {
    let root_dir = root("quarantined-reoffer");
    let root = root_dir.path();
    let protocol = FakeProtocol::new(work(), FailurePoint::ProcessStartAck, false);
    let adapter = FakeAdapter::new(RecoveryObservation::Ambiguous);
    let journal = OwnerOnlyJournal::new(root);
    let first = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );
    assert!(matches!(
        first
            .run_once(&session(), claim())
            .await
            .expect("first quarantine"),
        RunCycle::Quarantined { .. }
    ));

    *protocol.work.lock().expect("claim lock") = Some(work());
    let restarted = RunnerEngine::new(
        protocol.clone(),
        adapter.clone(),
        journal.clone(),
        WorkspaceManager::new(root.join("workspaces"), FakeWorktree::succeeds()),
    );
    assert!(matches!(
        restarted.run_once(&session(), claim()).await,
        Err(EngineError::Journal(JournalError::AlreadyExists))
    ));

    let process = adapter.evidence.lock().expect("process evidence");
    assert_eq!(
        process.starts, 1,
        "reoffered quarantined work cannot relaunch"
    );
    assert_eq!(process.cancels, 1);
    drop(process);
    let evidence = protocol.evidence.lock().expect("protocol evidence");
    assert_eq!(
        evidence
            .events
            .iter()
            .filter(|event| event.as_str() == "claim_committed")
            .count(),
        2,
        "the server reoffer reached the runner but stopped at local evidence"
    );
    assert_eq!(
        evidence
            .events
            .iter()
            .filter(|event| event.as_str() == "start:preparing")
            .count(),
        1,
        "the rejected reoffer never starts preparation or a process"
    );
    assert_eq!(
        evidence.recovery_reports,
        vec![RecoveryObservation::Ambiguous]
    );
    drop(evidence);
    assert!(journal.unresolved().expect("journal scan").is_empty());
    assert!(
        root.join("quarantine")
            .read_dir()
            .expect("quarantine")
            .next()
            .is_some()
    );
}
