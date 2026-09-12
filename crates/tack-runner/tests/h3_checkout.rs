//! A claimed attempt reaches a real harness process with its own
//! checkout of a **real** git repository.
//!
//! `crash_matrix.rs` proves the engine's lifecycle against a fake
//! provisioner: it answers "does the engine call provisioning at the right
//! moment", not "does a checkout exist when the harness starts". This file
//! answers the second question end to end — real `git`, real repository, real
//! child process — because that gap was invisible to every test
//! that used a fake provisioner (an empty directory satisfied all of them).

use std::{
    path::{Path, PathBuf},
    process::Command as SyncCommand,
    sync::{Arc, Mutex},
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
        ClaimedWork, CompletionReport, CompletionResponse, EnrollmentRequest, EnrollmentResponse,
        FencingToken, HarnessAdapter, HarnessError, HarnessOutcome, HeartbeatRequest,
        HeartbeatResponse, LeaseResult, LocalRunHandle, OwnerOnlyJournal, ProtocolClientError,
        PullProtocol, RecoveryObservation, RunCycle, RunnerCredential, RunnerEngine, RunnerId,
        RunnerSession, StartReport, Timestamp, WorkspaceManager,
        workspace::git::{CHECKOUT_MARKER, GitWorktreeProvisioner},
    },
};

mod common;
use common::temp_dir as temp_root;
use common::usage;

/// Resolves `git` to an absolute path instead of relying on `PATH`.
///
/// Not paranoia: `harness::claude_code`'s discovery test overwrites the
/// process-wide `PATH` for the duration of its assertion, and the Rust test
/// harness runs tests on many threads, so any concurrently-running test that
/// resolves a bare program name can observe that empty `PATH` and fail with a
/// spurious "binary not found". This was found as a one-in-many-runs failure
/// of `a_checkout_of_a_different_revision_is_never_reused` during
/// `cargo test --workspace` — a shared-state hazard from a concurrently
/// running test overwriting `PATH`.
fn git_program() -> PathBuf {
    for candidate in [
        "/usr/bin/git",
        "/bin/git",
        "/usr/local/bin/git",
        "/opt/homebrew/bin/git",
    ] {
        if Path::new(candidate).is_file() {
            return PathBuf::from(candidate);
        }
    }
    PathBuf::from("git")
}

fn run_git(directory: &Path, args: &[&str]) -> String {
    let output = SyncCommand::new(git_program())
        .current_dir(directory)
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A real repository with a distinctive file, so "the harness saw the
/// checkout" can be asserted on content rather than on the mere existence of
/// a directory.
fn source_repository() -> (tempfile::TempDir, String) {
    let path_dir = temp_root("source");
    let path = path_dir.path();
    run_git(path, &["-c", "init.defaultBranch=main", "init", "--quiet"]);
    run_git(path, &["config", "user.email", "runner@example.invalid"]);
    run_git(path, &["config", "user.name", "Tack Runner Test"]);
    std::fs::write(path.join("PLAN.md"), "the item to work on\n").expect("write");
    run_git(path, &["add", "."]);
    run_git(path, &["commit", "--quiet", "-m", "plan"]);
    let commit = run_git(path, &["rev-parse", "HEAD"]);
    (path_dir, commit)
}

/// What the harness process observed from inside its own working directory.
#[derive(Debug, Default, Clone)]
struct HarnessObservation {
    process_id: String,
    head: String,
    file: String,
    working_directory: String,
}

/// A minimal but genuinely real harness: it spawns an actual child process in
/// the attempt's workspace and records what that process could see. Nothing
/// here is mocked — if the workspace is not a checkout, the child fails and
/// the recorded observation is empty.
#[derive(Clone, Default)]
struct RealProcessAdapter {
    observed: Arc<Mutex<HarnessObservation>>,
}

#[async_trait]
impl HarnessAdapter for RealProcessAdapter {
    async fn validate(
        &self,
        _spec: &tack_runner::client::engine::ExecutionSpec,
    ) -> Result<(), HarnessError> {
        Ok(())
    }

    async fn start(
        &self,
        spec: &tack_runner::client::engine::ExecutionSpec,
    ) -> Result<LocalRunHandle, HarnessError> {
        let output = SyncCommand::new("/bin/sh")
            .current_dir(&spec.workspace.path)
            .arg("-c")
            .arg("printf '%s\\n%s\\n%s' \"$(git rev-parse HEAD)\" \"$(cat PLAN.md)\" \"$(pwd)\"")
            .output()
            .map_err(|_| HarnessError::Process)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut lines = stdout.lines();
        let process_id = format!("h3-{}", std::process::id());
        *self.observed.lock().expect("observation") = HarnessObservation {
            process_id: process_id.clone(),
            head: lines.next().unwrap_or_default().to_owned(),
            file: lines.next().unwrap_or_default().to_owned(),
            working_directory: lines.next().unwrap_or_default().to_owned(),
        };
        Ok(LocalRunHandle { process_id })
    }

    async fn cancel(&self, _handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        Ok(CancellationEvidence {
            observation: CancelObservation::ProcessStopped,
            observed_at: Timestamp::new("2026-08-19T12:00:01Z"),
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
        Ok(RecoveryObservation::ProcessStopped)
    }
}

#[derive(Clone)]
struct FakeProtocol {
    work: Arc<Mutex<Option<ClaimedWork>>>,
    cancellation_requested: bool,
}

impl FakeProtocol {
    fn new(work: ClaimedWork, cancellation_requested: bool) -> Self {
        Self {
            work: Arc::new(Mutex::new(Some(work))),
            cancellation_requested,
        }
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
        let active = request
            .active_attempts
            .into_iter()
            .next()
            .expect("active attempt");
        Ok(HeartbeatResponse {
            protocol_version: ProtocolVersion::v1(),
            heartbeat_id: request.heartbeat_id,
            accepted_at: Timestamp::new("2026-08-19T12:00:01Z"),
            lease_results: vec![LeaseResult {
                attempt_id: active.attempt_id,
                fencing_token: active.fencing_token,
                lease_expires_at: Timestamp::new("2026-08-19T12:01:00Z"),
                cancellation_requested: self.cancellation_requested,
            }],
        })
    }

    async fn report_start(
        &self,
        _session: &RunnerSession,
        _report: StartReport,
    ) -> Result<(), ProtocolClientError> {
        Ok(())
    }

    async fn report_completion(
        &self,
        _session: &RunnerSession,
        report: CompletionReport,
    ) -> Result<CompletionResponse, ProtocolClientError> {
        Ok(CompletionResponse {
            protocol_version: ProtocolVersion::v1(),
            attempt_id: report.attempt_id,
            completion_id: report.completion_id,
            state: report.terminal_state,
            replayed: false,
            committed_at: Timestamp::new("2026-08-19T12:00:02Z"),
        })
    }

    async fn report_cancellation(
        &self,
        _session: &RunnerSession,
        report: CancellationReport,
    ) -> Result<CancellationResponse, ProtocolClientError> {
        Ok(CancellationResponse {
            protocol_version: ProtocolVersion::v1(),
            attempt_id: report.attempt_id,
            cancellation_request_id: report.cancellation_request_id,
            state: AttemptState::Cancelled,
            replayed: false,
            committed_at: Timestamp::new("2026-08-19T12:00:02Z"),
        })
    }

    async fn observe_recovery(
        &self,
        _session: &RunnerSession,
        report: RecoveryObservationRequest,
    ) -> Result<RecoveryObservationResponse, ProtocolClientError> {
        let mut response: RecoveryObservationResponse = serde_json::from_str(include_str!(
            "../../../docs/contracts/runner-v1/recovery-observation.response.json"
        ))
        .expect("recovery fixture");
        response.attempt_id = report.attempt_id;
        response.recovery_key = report.recovery_key;
        Ok(response)
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
                "decisions":{"support":"unsupported","reason":"no decisions"},
                "artifacts":{"support":"unsupported","reason":"no artifacts"},
                "usage":{"support":"advisory","reason":"partial"}
            },
            "workspace_id":"ws_placeholder",
            "base_revision":"placeholder",
            "started_at":"2026-08-19T12:00:00Z", "ended_at":"2026-08-19T12:01:00Z"
        }"#,
    )
    .expect("actual execution")
}

/// The frozen claim fixture, retargeted at a repository that actually exists
/// on this machine. Only the repository coordinates and the ids change — the
/// snapshot shape stays the contract's.
fn work(attempt: &str, remote: &Path, revision: &str) -> ClaimedWork {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture");
    let mut request: ExecutionRequestSnapshot =
        serde_json::from_value(fixture["request"].clone()).expect("request snapshot");
    let mut snapshot: AttemptSnapshot =
        serde_json::from_value(fixture["attempt"].clone()).expect("attempt snapshot");
    request.repository.remote = remote.to_string_lossy().into_owned();
    request.repository.base_revision = revision.to_owned();
    snapshot.attempt_id = DomainAttemptId::new(attempt);
    snapshot.runner_id = DomainRunnerId::new("runner-h3");
    snapshot.base_revision = revision.to_owned();
    ClaimedWork {
        claim_request_id: ClaimRequestId::new(format!("claim-{attempt}")),
        lease: AttemptLease {
            attempt_id: AttemptId::new(attempt),
            runner_id: RunnerId::new("runner-h3"),
            fencing_token: FencingToken(7),
            attempt_number: 1,
            state: AttemptState::Leased,
            issued_at: Timestamp::new("2026-08-19T12:00:00Z"),
            expires_at: Timestamp::new("2026-08-19T12:01:00Z"),
        },
        request,
        attempt: snapshot,
    }
}

fn session() -> RunnerSession {
    RunnerSession::new(
        RunnerId::new("runner-h3"),
        RunnerCredential::new("test-secret-never-log"),
        Timestamp::new("2026-08-19T13:00:00Z"),
    )
}

fn claim(attempt: &str) -> ClaimRequest {
    ClaimRequest {
        claim_request_id: ClaimRequestId::new(format!("claim-{attempt}")),
        available_capacity: 1,
        wait: Duration::ZERO,
    }
}

#[tokio::test]
async fn a_claimed_attempt_reaches_the_harness_in_its_own_checkout() {
    let (source_dir, commit) = source_repository();
    let source = source_dir.path();
    let root_dir = temp_root("run");
    let root = root_dir.path();
    let adapter = RealProcessAdapter::default();
    let engine = RunnerEngine::new(
        FakeProtocol::new(work("attempt-h3", source, &commit), false),
        adapter.clone(),
        OwnerOnlyJournal::new(root.join("journal")),
        WorkspaceManager::new(
            root.join("workspaces"),
            GitWorktreeProvisioner::new(git_program(), Duration::from_secs(600)),
        ),
    );

    let cycle = engine
        .run_once(&session(), claim("attempt-h3"))
        .await
        .expect("the claimed attempt runs");
    assert!(matches!(cycle, RunCycle::Completed { .. }));

    let observed = adapter.observed.lock().expect("observation").clone();
    assert_eq!(
        observed.head, commit,
        "the harness process must start inside a checkout of the claimed revision"
    );
    assert_eq!(
        observed.file, "the item to work on",
        "the harness process must be able to read the repository's files"
    );
    assert!(!observed.process_id.is_empty());
    assert!(
        observed.working_directory.contains("workspaces"),
        "the harness ran outside the runner's workspace root: {}",
        observed.working_directory
    );
}

/// The complement of the test above: without a real provisioner, the
/// identical run cannot reach the harness at all. Reverting the fix is
/// therefore proven to break the claim, rather than assumed to.
#[tokio::test]
async fn without_a_provisioner_the_attempt_never_reaches_the_harness() {
    let (source_dir, commit) = source_repository();
    let source = source_dir.path();
    let root_dir = temp_root("unavailable");
    let root = root_dir.path();
    let adapter = RealProcessAdapter::default();
    let engine = RunnerEngine::new(
        FakeProtocol::new(work("attempt-h3-gap", source, &commit), false),
        adapter.clone(),
        OwnerOnlyJournal::new(root.join("journal")),
        WorkspaceManager::new(
            root.join("workspaces"),
            tack_runner::client::UnavailableWorktreeProvisioner,
        ),
    );

    let error = engine
        .run_once(&session(), claim("attempt-h3-gap"))
        .await
        .expect_err("provisioning is unavailable, so the attempt cannot run");
    assert!(format!("{error}").contains("worktree provisioning is not configured"));
    assert!(
        adapter
            .observed
            .lock()
            .expect("observation")
            .head
            .is_empty(),
        "no harness process may start without a checkout"
    );
}

#[tokio::test]
async fn a_completed_attempt_leaves_no_checkout_behind() {
    let (source_dir, commit) = source_repository();
    let source = source_dir.path();
    let root_dir = temp_root("cleanup");
    let root = root_dir.path();
    let workspaces = root.join("workspaces");
    let engine = RunnerEngine::new(
        FakeProtocol::new(work("attempt-h3-done", source, &commit), false),
        RealProcessAdapter::default(),
        OwnerOnlyJournal::new(root.join("journal")),
        WorkspaceManager::new(
            &workspaces,
            GitWorktreeProvisioner::new(git_program(), Duration::from_secs(600)),
        ),
    );

    engine
        .run_once(&session(), claim("attempt-h3-done"))
        .await
        .expect("the claimed attempt runs");

    let remaining: Vec<_> = std::fs::read_dir(&workspaces)
        .expect("workspace root")
        .map(|entry| entry.expect("entry").path())
        .collect();
    assert!(
        remaining.is_empty(),
        "a completed attempt must leave no checkout behind: {remaining:?}"
    );
}

#[tokio::test]
async fn a_cancelled_attempt_leaves_no_checkout_behind() {
    let (source_dir, commit) = source_repository();
    let source = source_dir.path();
    let root_dir = temp_root("cancelled");
    let root = root_dir.path();
    let workspaces = root.join("workspaces");
    let engine = RunnerEngine::new(
        FakeProtocol::new(work("attempt-h3-cancel", source, &commit), true),
        RealProcessAdapter::default(),
        OwnerOnlyJournal::new(root.join("journal")),
        WorkspaceManager::new(
            &workspaces,
            GitWorktreeProvisioner::new(git_program(), Duration::from_secs(600)),
        ),
    );

    let cycle = engine
        .run_once(&session(), claim("attempt-h3-cancel"))
        .await
        .expect("the claimed attempt is cancelled");
    assert!(matches!(cycle, RunCycle::Cancelled { .. }));

    let remaining: Vec<_> = std::fs::read_dir(&workspaces)
        .expect("workspace root")
        .map(|entry| entry.expect("entry").path())
        .collect();
    assert!(
        remaining.is_empty(),
        "a cancelled attempt must leave no checkout behind: {remaining:?}"
    );
}

/// Crash recovery: a runner killed after provisioning leaves a checkout on
/// disk with an unresolved journal record. The restart must resolve the
/// attempt and remove that checkout — the "no unusable worktree survives a
/// kill" invariant, proven at the engine level rather than inside the
/// provisioner.
#[tokio::test]
async fn a_checkout_left_by_a_killed_runner_is_removed_by_the_restart() {
    let (source_dir, commit) = source_repository();
    let source = source_dir.path();
    let root_dir = temp_root("recovery");
    let root = root_dir.path();
    let workspaces = root.join("workspaces");
    let journal = OwnerOnlyJournal::new(root.join("journal"));
    let killed = RunnerEngine::new(
        FakeProtocol::new(work("attempt-h3-killed", source, &commit), false),
        // The adapter fails to start, which is where a `kill -9` most often
        // lands: after the checkout exists, before anything terminal is
        // reported. The journal record survives; so does the checkout.
        FailingStartAdapter {
            reconcile_is_ambiguous: false,
        },
        journal.clone(),
        WorkspaceManager::new(
            &workspaces,
            GitWorktreeProvisioner::new(git_program(), Duration::from_secs(600)),
        ),
    );
    assert!(
        killed
            .run_once(&session(), claim("attempt-h3-killed"))
            .await
            .is_err()
    );
    let orphan = std::fs::read_dir(&workspaces)
        .expect("workspace root")
        .map(|entry| entry.expect("entry").path())
        .next()
        .expect("the interrupted attempt left a checkout");
    assert!(
        orphan.join(CHECKOUT_MARKER).exists(),
        "the interrupted attempt had already completed its checkout"
    );
    assert_eq!(journal.unresolved().expect("journal").len(), 1);

    let restarted = RunnerEngine::new(
        FakeProtocol::new(work("attempt-h3-killed", source, &commit), false),
        RealProcessAdapter::default(),
        journal.clone(),
        WorkspaceManager::new(
            &workspaces,
            GitWorktreeProvisioner::new(git_program(), Duration::from_secs(600)),
        ),
    );
    let outcomes = restarted.recover(&session()).await.expect("recovery");
    assert_eq!(outcomes.len(), 1);
    assert!(
        !orphan.exists(),
        "the restart must not leave the killed attempt's checkout behind"
    );
    assert!(journal.unresolved().expect("journal").is_empty());
}

/// The deliberate other half of the rule above: an attempt the runner could
/// not settle is *quarantined*, and its checkout is the evidence an operator
/// will be asked to look at. Deleting it would destroy that evidence, so the
/// cleanup on recovery must be conditional, not unconditional.
#[tokio::test]
async fn a_quarantined_attempt_keeps_its_checkout_as_evidence() {
    let (source_dir, commit) = source_repository();
    let source = source_dir.path();
    let root_dir = temp_root("quarantine");
    let root = root_dir.path();
    let workspaces = root.join("workspaces");
    let journal = OwnerOnlyJournal::new(root.join("journal"));
    let killed = RunnerEngine::new(
        FakeProtocol::new(work("attempt-h3-quarantine", source, &commit), false),
        FailingStartAdapter {
            reconcile_is_ambiguous: false,
        },
        journal.clone(),
        WorkspaceManager::new(
            &workspaces,
            GitWorktreeProvisioner::new(git_program(), Duration::from_secs(600)),
        ),
    );
    assert!(
        killed
            .run_once(&session(), claim("attempt-h3-quarantine"))
            .await
            .is_err()
    );
    let orphan = std::fs::read_dir(&workspaces)
        .expect("workspace root")
        .map(|entry| entry.expect("entry").path())
        .next()
        .expect("the interrupted attempt left a checkout");

    let restarted = RunnerEngine::new(
        FakeProtocol::new(work("attempt-h3-quarantine", source, &commit), false),
        FailingStartAdapter {
            reconcile_is_ambiguous: true,
        },
        journal.clone(),
        WorkspaceManager::new(
            &workspaces,
            GitWorktreeProvisioner::new(git_program(), Duration::from_secs(600)),
        ),
    );
    let outcomes = restarted.recover(&session()).await.expect("recovery");
    assert!(matches!(
        outcomes.as_slice(),
        [RunCycle::Quarantined { .. }]
    ));
    assert!(
        orphan.join(CHECKOUT_MARKER).exists(),
        "a quarantined attempt's checkout must survive for the operator"
    );
}

/// Fails to start, and optionally cannot say what happened afterwards. The
/// second mode is how a quarantine is reached: the engine treats a reconcile
/// error as `Ambiguous`, which no disposition can settle.
struct FailingStartAdapter {
    reconcile_is_ambiguous: bool,
}

#[async_trait]
impl HarnessAdapter for FailingStartAdapter {
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
        Err(HarnessError::Process)
    }

    async fn cancel(&self, _handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        Err(HarnessError::Process)
    }

    async fn wait(&self, _handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        Err(HarnessError::Process)
    }

    async fn reconcile(
        &self,
        _journal: &tack_runner::client::AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        if self.reconcile_is_ambiguous {
            Err(HarnessError::RecoveryUnavailable)
        } else {
            Ok(RecoveryObservation::ProcessStopped)
        }
    }
}
