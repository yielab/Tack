use super::*;
use crate::client::journal::{JournalState, WorkspaceJournal};
use crate::client::{
    AttemptId, AttemptLease, ClaimRequestId, ClaimedWork, FencingToken, RunnerId,
    Workspace as ClientWorkspace, WorkspaceId,
};
use crate::harness::fixtures::fake_harness_command;
use std::time::SystemTime;
use tack_orch::execution::{
    AttemptSnapshot, ExecutionRequestSnapshot, HarnessKind as DomainHarnessKind, RequestedModelId,
    RequestedModelProvider,
};

#[derive(Clone, Copy)]
struct FixedClock(SystemTime);

impl crate::Clock for FixedClock {
    fn now(&self) -> SystemTime {
        self.0
    }
}

fn clock_at(rfc3339_at: &str) -> FixedClock {
    FixedClock(
        chrono::DateTime::parse_from_rfc3339(rfc3339_at)
            .expect("fixture timestamp")
            .into(),
    )
}

fn generous_limits() -> ProcessLimits {
    ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(10))
}

/// A scratch directory that removes itself, and everything written under
/// it, when the returned guard drops — including when an assertion panics
/// first. Whatever holds a path into it must hold the guard too.
fn temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

/// A minimal, deterministic "fixture repo" workspace: a couple of known
/// files with fixed content, created fresh per test rather than checked
/// into the tree — giving every test a real, workspace-confined,
/// reproducible directory to run the fake harness against (mirroring
/// the
/// `each_workspace_confined_process_only_ever_sees_its_own_canary_file`
/// pattern).
fn deterministic_fixture_repo(label: &str) -> tempfile::TempDir {
    let root = temp_dir(label);
    std::fs::write(root.path().join("README.md"), b"# fixture repo\n").expect("write README");
    std::fs::write(root.path().join("main.rs"), b"fn main() {}\n").expect("write main.rs");
    root
}

fn fixed_command() -> CodexLocator {
    let (program, prefix_args) = fake_harness_command();
    CodexLocator::Fixed {
        program,
        prefix_args,
    }
}

/// A fresh, hermetic file-backed store per call — never the platform
/// keychain — so parallel `#[test]` functions never see each other's
/// entries and CI needs no Secret Service.
/// Takes the directory rather than making one, so the store's file cannot
/// outlive the guard that removes it. File-backed, never the platform
/// keychain, so parallel tests never see each other's entries and CI
/// needs no Secret Service.
fn test_secret_store(dir: &std::path::Path) -> crate::secrets::SecretStore {
    crate::secrets::SecretStore::file(dir.join("secrets.json"))
}

/// Returns the adapter with the scratch directory its artifact staging
/// root and secret store both live in: drop the guard and the adapter is
/// pointing at nothing.
fn adapter_with_env(
    probe_env: BTreeMap<String, String>,
) -> (CodexAdapter<FixedClock>, tempfile::TempDir) {
    let scratch = temp_dir("artifacts");
    let adapter = CodexAdapter::with_clock(
        fixed_command(),
        generous_limits(),
        Duration::from_secs(5),
        probe_env,
        scratch.path().to_path_buf(),
        clock_at("2026-08-09T12:00:00Z"),
        test_secret_store(scratch.path()),
    );
    (adapter, scratch)
}

fn adapter() -> (CodexAdapter<FixedClock>, tempfile::TempDir) {
    adapter_with_env(BTreeMap::new())
}

fn env_map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn spec_with(
    workspace_path: PathBuf,
    model: Option<(&str, &str)>,
    extra_env: &[(&str, &str)],
) -> ExecutionSpec {
    let claim: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture");
    let mut request: ExecutionRequestSnapshot =
        serde_json::from_value(claim["request"].clone()).expect("request fixture");
    request.requested_harness_kind = DomainHarnessKind::new(CODEX_HARNESS_KIND);
    request.requested_model_provider =
        model.map(|(provider, _)| RequestedModelProvider::new(provider));
    request.requested_model_id = model.map(|(_, id)| RequestedModelId::new(id));
    request.timeout_seconds = 3600;
    for (key, value) in extra_env {
        request.environment.insert(
            (*key).to_owned(),
            tack_orch::execution::EnvironmentValue {
                value: Some((*value).to_owned()),
                secret_reference: None,
                additional: BTreeMap::new(),
            },
        );
    }
    let attempt: AttemptSnapshot =
        serde_json::from_value(claim["attempt"].clone()).expect("attempt fixture");

    ExecutionSpec {
        work: ClaimedWork {
            claim_request_id: ClaimRequestId::new("claim"),
            lease: AttemptLease {
                attempt_id: AttemptId::new("attempt"),
                runner_id: RunnerId::new("runner"),
                fencing_token: FencingToken(1),
                attempt_number: 1,
                state: crate::client::AttemptState::Leased,
                issued_at: Timestamp::new("2026-08-09T11:59:00Z"),
                expires_at: Timestamp::new("2026-08-09T12:59:00Z"),
            },
            request,
            attempt,
        },
        workspace: ClientWorkspace {
            attempt_id: AttemptId::new("attempt"),
            id: WorkspaceId::new("ws_codex_test"),
            path: workspace_path,
            base_revision: "revision".into(),
        },
    }
}

// ---- validate() / start() pre-spawn rejection --------------------

#[tokio::test]
async fn validate_rejects_a_mismatched_harness_kind() {
    let (adapter, _scratch) = adapter();
    let workspace_dir = deterministic_fixture_repo("kind-mismatch");
    let workspace = workspace_dir.path();
    let mut spec = spec_with(
        workspace.to_path_buf(),
        Some(("openai", "opaque/model-alpha")),
        &[],
    );
    spec.work.request.requested_harness_kind = DomainHarnessKind::new("claude-code");

    assert!(matches!(
        adapter.validate(&spec).await,
        Err(HarnessError::Rejected { .. })
    ));
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

#[tokio::test]
async fn validate_rejects_an_auto_selected_model_pre_spawn() {
    let (adapter, _scratch) = adapter();
    let workspace_dir = deterministic_fixture_repo("auto-select");
    let workspace = workspace_dir.path();
    let spec = spec_with(workspace.to_path_buf(), None, &[]);

    assert!(matches!(
        adapter.validate(&spec).await,
        Err(HarnessError::Rejected { .. })
    ));
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

#[tokio::test]
async fn validate_rejects_an_unresolvable_binary() {
    let empty_dir_dir = temp_dir("empty-path");
    let empty_dir = empty_dir_dir.path();
    let scratch = temp_dir("artifacts-unresolvable");
    let adapter = CodexAdapter::with_clock(
        CodexLocator::Search {
            // Not the real `codex` program name: the well-known fallback
            // list includes fixed system directories (Homebrew's
            // `/usr/local/bin`) this test cannot isolate the way it
            // isolates `PATH`, so a name no real installer would ever
            // use keeps this test deterministic regardless of what is
            // actually installed on the machine running it.
            program_name: "tack-test-fixture-nonexistent-codex".to_owned(),
            path: Some(std::env::join_paths([empty_dir]).expect("join paths")),
            home: None,
        },
        generous_limits(),
        Duration::from_secs(1),
        BTreeMap::new(),
        scratch.path().to_path_buf(),
        clock_at("2026-08-09T12:00:00Z"),
        test_secret_store(scratch.path()),
    );
    let workspace_dir = deterministic_fixture_repo("unresolvable");
    let workspace = workspace_dir.path();
    let spec = spec_with(
        workspace.to_path_buf(),
        Some(("openai", "opaque/model-alpha")),
        &[],
    );

    assert!(matches!(
        adapter.validate(&spec).await,
        Err(HarnessError::Rejected { .. })
    ));
    std::fs::remove_dir_all(workspace).expect("cleanup");
    std::fs::remove_dir_all(empty_dir).expect("cleanup");
}

/// Acceptance: "an unsupported selection fails pre-spawn — validation
/// rejects it before any process is launched, not after." Proved
/// empirically, not just by code inspection: the spec is configured so
/// the underlying fake process would `hang` for an hour if it were ever
/// actually spawned. If `start()`'s pre-spawn guard were broken, this
/// test would hang (bounded here by an explicit timeout that turns that
/// hang into a fast, loud failure rather than a stuck CI job).
#[tokio::test]
async fn unsupported_selection_fails_pre_spawn_even_when_the_process_would_otherwise_hang_forever()
{
    let (adapter, _scratch) = adapter();
    let workspace_dir = deterministic_fixture_repo("pre-spawn-hang-guard");
    let workspace = workspace_dir.path();
    let spec = spec_with(
        workspace.to_path_buf(),
        None, // auto-select: rejected by check_selection before spawn
        &[
            ("TACK_FAKE_HARNESS_MODE", "hang"),
            ("TACK_FAKE_HARNESS_SLEEP_SECONDS", "3600"),
        ],
    );

    let result = tokio::time::timeout(Duration::from_secs(5), adapter.start(&spec)).await;
    assert!(
        result.is_ok(),
        "start() must reject pre-spawn, not hang waiting on a process it never launched"
    );
    assert!(matches!(
        result.unwrap(),
        Err(HarnessError::Rejected { .. })
    ));
    assert!(
        adapter.running.lock().await.is_empty(),
        "a pre-spawn rejection must never create process bookkeeping (verifier nit 4)"
    );
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

// ---- fake-binary exec-path tests ----------------------------------

#[tokio::test]
async fn fake_binary_success_completes_succeeded_with_normalized_output_and_a_staged_artifact() {
    let (adapter, _scratch) = adapter();
    let workspace_dir = deterministic_fixture_repo("exec-success");
    let workspace = workspace_dir.path();
    let spec = spec_with(
        workspace.to_path_buf(),
        Some(("openai", "opaque/model-alpha")),
        &[("TACK_FAKE_HARNESS_MODE", "success")],
    );

    adapter.validate(&spec).await.expect("validate");
    let handle = adapter.start(&spec).await.expect("start");
    let outcome = adapter.wait(&handle).await.expect("wait");

    assert_eq!(outcome.terminal_state, AttemptState::Succeeded);
    assert_eq!(outcome.terminal_reason["code"], "completed");
    assert!(
        outcome.terminal_reason["stdout"]["text_preview"]
            .as_str()
            .unwrap()
            .contains("fake-harness-ok")
    );
    assert_eq!(outcome.actual_execution.model_provider.as_str(), "openai");
    assert_eq!(
        outcome.actual_execution.model_id.as_str(),
        "opaque/model-alpha"
    );
    assert_eq!(
        outcome.actual_execution.model_observation_source,
        MODEL_OBSERVATION_SOURCE
    );
    assert_eq!(
        outcome.usage.duration_ms.source,
        MeasurementSource::Measured
    );
    assert!(outcome.usage.duration_ms.value.is_some());
    assert_eq!(
        outcome.usage.tokens_in.source,
        MeasurementSource::NotMeasured
    );
    assert!(outcome.usage.tokens_in.value.is_none());

    let artifact = &outcome.terminal_reason["artifact"];
    assert_eq!(artifact["kind"], "log");
    let staged_path = artifact["staged_path"].as_str().expect("staged_path");
    let staged_bytes = std::fs::read(staged_path).expect("read staged artifact");
    assert!(String::from_utf8_lossy(&staged_bytes).contains("fake-harness-ok"));
    assert_eq!(
        artifact["sha256"].as_str().unwrap(),
        crate::harness::sha256::sha256_hex(&staged_bytes)
    );

    std::fs::remove_dir_all(workspace).expect("cleanup");
}

#[tokio::test]
async fn fake_binary_failure_completes_failed_with_the_exit_code_in_terminal_reason() {
    let (adapter, _scratch) = adapter();
    let workspace_dir = deterministic_fixture_repo("exec-failure");
    let workspace = workspace_dir.path();
    let spec = spec_with(
        workspace.to_path_buf(),
        Some(("openai", "opaque/model-alpha")),
        &[
            ("TACK_FAKE_HARNESS_MODE", "failure"),
            ("TACK_FAKE_HARNESS_EXIT_CODE", "17"),
        ],
    );

    let handle = adapter.start(&spec).await.expect("start");
    let outcome = adapter.wait(&handle).await.expect("wait");

    assert_eq!(outcome.terminal_state, AttemptState::Failed);
    assert_eq!(outcome.terminal_reason["code"], "exit_code");
    assert!(
        outcome.terminal_reason["message"]
            .as_str()
            .unwrap()
            .contains("17")
    );
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// Acceptance: malformed output. See module docs assumption (4) for why
/// this proves *robustness* (no panic, bounded/redacted capture, a
/// well-typed result either way) rather than "malformed output causes
/// failure" — the fake fixture's `malformed` mode still exits 0, and
/// this adapter deliberately never parses Codex's unverified real output
/// shape to second-guess an exit code.
#[tokio::test]
async fn fake_binary_malformed_output_does_not_panic_and_still_produces_a_typed_result() {
    let (adapter, _scratch) = adapter();
    let workspace_dir = deterministic_fixture_repo("exec-malformed");
    let workspace = workspace_dir.path();
    let spec = spec_with(
        workspace.to_path_buf(),
        Some(("openai", "opaque/model-alpha")),
        &[("TACK_FAKE_HARNESS_MODE", "malformed")],
    );

    let handle = adapter.start(&spec).await.expect("start");
    let outcome = adapter.wait(&handle).await.expect("wait");

    // The fixture's `malformed` mode exits 0; this adapter classifies
    // purely on exit code (assumption (4)), so this is `Succeeded`, not
    // a fabricated `Failed`.
    assert_eq!(outcome.terminal_state, AttemptState::Succeeded);
    let preview = outcome.terminal_reason["stdout"]["text_preview"]
        .as_str()
        .expect("stdout preview is present and well-formed JSON");
    assert!(!preview.is_empty());
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// Acceptance: cancel kills descendants, proved through the adapter's
/// own `start`/`cancel`, not raw `ProcessSpec` (that is `process.rs`'s
/// own test).
#[tokio::test]
async fn cancel_kills_the_whole_descendant_tree_via_the_adapter() {
    let (adapter, _scratch) = adapter();
    let workspace_dir = deterministic_fixture_repo("exec-cancel");
    let workspace = workspace_dir.path();
    let pidfile = workspace.join("grandchild.pid");
    let spec = spec_with(
        workspace.to_path_buf(),
        Some(("openai", "opaque/model-alpha")),
        &[
            ("TACK_FAKE_HARNESS_MODE", "spawn_child"),
            (
                "TACK_FAKE_HARNESS_PIDFILE",
                pidfile.to_str().expect("utf8 pidfile path"),
            ),
            ("TACK_FAKE_HARNESS_SLEEP_SECONDS", "3600"),
        ],
    );

    let handle = adapter.start(&spec).await.expect("start");
    let grandchild_pid = wait_for_pidfile(&pidfile).await;
    assert!(
        crate::harness::process::process_alive(grandchild_pid),
        "grandchild must be observed running before cancellation"
    );

    let evidence = adapter.cancel(&handle).await.expect("cancel");
    assert_eq!(evidence.observation, CancelObservation::ProcessStopped);

    assert!(
        wait_until_dead(grandchild_pid, Duration::from_secs(5)).await,
        "grandchild must be gone after the adapter cancels its parent"
    );
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// A cancel/wait on a handle this adapter instance never produced (e.g.
/// stale after a restart) is a typed rejection, never a silent success.
#[tokio::test]
async fn cancel_and_wait_on_an_untracked_handle_are_typed_rejections() {
    let (adapter, _scratch) = adapter();
    let handle = LocalRunHandle {
        process_id: "codex:999999:0".to_owned(),
    };
    assert!(matches!(
        adapter.cancel(&handle).await,
        Err(HarnessError::Process)
    ));
    assert!(matches!(
        adapter.wait(&handle).await,
        Err(HarnessError::Process)
    ));
}

// ---- redaction (rule 12) -------------------------------------------

/// Acceptance: arguments/environment are redacted in logs and events.
/// Plants a canary in both the requested environment and (indirectly,
/// since the agent profile instructions become the prompt) stdin, drives
/// the fake harness's `echo_canary` mode so it actively echoes the
/// canary back on stdout *and* stderr, and asserts it appears nowhere in
/// the adapter's own output surface (`HarnessOutcome.terminal_reason`)
/// nor in the staged log artifact.
#[tokio::test]
async fn secret_canaries_never_survive_into_terminal_reason_or_the_staged_artifact() {
    const CANARY_ENV: &str = "tack-test-codex-canary-env-58d1";
    let (adapter, _scratch) = adapter();
    let workspace_dir = deterministic_fixture_repo("redaction");
    let workspace = workspace_dir.path();
    let mut spec = spec_with(
        workspace.to_path_buf(),
        Some(("openai", "opaque/model-alpha")),
        &[
            ("TACK_FAKE_HARNESS_MODE", "echo_canary"),
            ("TACK_TEST_SECRET", CANARY_ENV),
            ("TACK_FAKE_HARNESS_ECHO_ENV_KEYS", "TACK_TEST_SECRET"),
        ],
    );
    // The agent profile's instructions become the prompt piped over
    // stdin; the fake harness's `echo_canary` mode also echoes stdin
    // back, so folding a second canary into the prompt exercises that
    // path too.
    spec.work.request.resolved_agent_profile.instructions =
        "do the tack-test-codex-canary-stdin-a341 thing".to_owned();
    const CANARY_STDIN: &str = "tack-test-codex-canary-stdin-a341";

    let handle = adapter.start(&spec).await.expect("start");
    let outcome = adapter.wait(&handle).await.expect("wait");

    let serialized = outcome.terminal_reason.to_string();
    assert!(
        serialized.contains("[REDACTED]"),
        "the fake harness must actually have echoed something for this test to be meaningful"
    );
    assert!(!serialized.contains(CANARY_ENV));
    assert!(!serialized.contains(CANARY_STDIN));

    let artifact_path = outcome.terminal_reason["artifact"]["staged_path"]
        .as_str()
        .expect("artifact staged");
    let staged_text = std::fs::read_to_string(artifact_path).expect("read staged artifact");
    assert!(!staged_text.contains(CANARY_ENV));
    assert!(!staged_text.contains(CANARY_STDIN));

    std::fs::remove_dir_all(workspace).expect("cleanup");
}

// ---- probe() / HarnessProbe ----------------------------------------

#[tokio::test]
async fn probe_reports_a_recognized_version_with_no_error() {
    let (adapter, _scratch) = adapter_with_env(env_map(&[
        ("TACK_FAKE_HARNESS_MODE", "version"),
        ("TACK_FAKE_HARNESS_VERSION", "9.9.9"),
    ]));
    let capability = adapter.probe().await;

    assert_eq!(capability.harness_kind.as_str(), CODEX_HARNESS_KIND);
    assert_eq!(capability.installed_version, "9.9.9");
    assert_eq!(capability.probe_error, None);
    assert!(capability.model_combinations.is_empty());
    // With no enumerable models, schedulability rests on the
    // pass-through attestation — it must be Supported and carry a reason.
    let passthrough = capability
        .model_passthrough
        .expect("codex probe must attest model_passthrough");
    assert_eq!(passthrough.support, CapabilitySupport::Supported);
    assert!(passthrough.reason.is_some());
}

/// Acceptance: the real `codex` CLI prefixes its version with a
/// program-name token (`codex-cli 0.149.1`) instead of printing it bare.
/// A whole-string check would misclassify this as unrecognized and
/// permanently block scheduling via `HarnessProbeError` regardless of
/// `model_passthrough`; the version must be extracted from among the
/// output's tokens instead.
#[tokio::test]
async fn probe_recognizes_a_program_name_prefixed_version_string() {
    let (adapter, _scratch) = adapter_with_env(env_map(&[
        ("TACK_FAKE_HARNESS_MODE", "version"),
        ("TACK_FAKE_HARNESS_VERSION", "codex-cli 0.149.1"),
    ]));
    let capability = adapter.probe().await;

    assert_eq!(capability.installed_version, "0.149.1");
    assert_eq!(capability.probe_error, None);
}

/// Acceptance: unknown version. The fixture's `unknown_version` mode
/// exits 0 with a string that is not a clean version line; this is an
/// explicit `probe_error`, never a fabricated clean version (rule 7).
#[tokio::test]
async fn probe_reports_an_unrecognized_version_string_as_an_explicit_probe_error() {
    let (adapter, _scratch) =
        adapter_with_env(env_map(&[("TACK_FAKE_HARNESS_MODE", "unknown_version")]));
    let capability = adapter.probe().await;

    assert_eq!(capability.installed_version, "");
    assert!(capability.probe_error.is_some());
    let raw = capability
        .additional
        .get("raw_version_output")
        .and_then(|value| value.as_str())
        .expect("raw output preserved for diagnosis");
    assert!(raw.contains("999.999.999"));
}

/// Acceptance: malformed (probe-level companion to the exec-level
/// malformed test above).
#[tokio::test]
async fn probe_reports_malformed_version_output_as_an_explicit_probe_error() {
    let (adapter, _scratch) = adapter_with_env(env_map(&[("TACK_FAKE_HARNESS_MODE", "malformed")]));
    let capability = adapter.probe().await;

    assert_eq!(capability.installed_version, "");
    assert!(capability.probe_error.is_some());
}

#[tokio::test]
async fn probe_reports_a_nonzero_exit_as_an_explicit_probe_error() {
    let (adapter, _scratch) = adapter_with_env(env_map(&[
        ("TACK_FAKE_HARNESS_MODE", "failure"),
        ("TACK_FAKE_HARNESS_EXIT_CODE", "3"),
    ]));
    let capability = adapter.probe().await;

    assert_eq!(capability.installed_version, "");
    assert!(capability.probe_error.unwrap().contains('3'));
}

#[tokio::test]
async fn probe_reports_an_absent_binary_as_an_explicit_probe_error_never_a_fake_success() {
    let empty_dir_dir = temp_dir("probe-empty-path");
    let empty_dir = empty_dir_dir.path();
    let scratch = temp_dir("artifacts-absent");
    let adapter = CodexAdapter::with_clock(
        CodexLocator::Search {
            // Not the real `codex` program name: the well-known fallback
            // list includes fixed system directories (Homebrew's
            // `/usr/local/bin`) this test cannot isolate the way it
            // isolates `PATH`, so a name no real installer would ever
            // use keeps this test deterministic regardless of what is
            // actually installed on the machine running it.
            program_name: "tack-test-fixture-nonexistent-codex".to_owned(),
            path: Some(std::env::join_paths([empty_dir]).expect("join paths")),
            home: None,
        },
        generous_limits(),
        Duration::from_secs(1),
        BTreeMap::new(),
        scratch.path().to_path_buf(),
        clock_at("2026-08-09T12:00:00Z"),
        test_secret_store(scratch.path()),
    );

    let capability = adapter.probe().await;
    assert_eq!(capability.installed_version, "");
    assert!(capability.probe_error.unwrap().contains("not found"));
    std::fs::remove_dir_all(empty_dir).expect("cleanup");
}

#[tokio::test]
async fn probe_never_hangs_past_its_own_timeout() {
    let scratch = temp_dir("artifacts-hang");
    let adapter = CodexAdapter::with_clock(
        fixed_command(),
        generous_limits(),
        Duration::from_millis(50),
        env_map(&[
            ("TACK_FAKE_HARNESS_MODE", "hang"),
            ("TACK_FAKE_HARNESS_SLEEP_SECONDS", "3600"),
        ]),
        scratch.path().to_path_buf(),
        clock_at("2026-08-09T12:00:00Z"),
        test_secret_store(scratch.path()),
    );

    let capability = tokio::time::timeout(Duration::from_secs(5), adapter.probe())
        .await
        .expect("probe must respect its own timeout rather than hanging the caller");
    assert_eq!(capability.installed_version, "");
    assert!(capability.probe_error.unwrap().contains("timed out"));
}

#[tokio::test]
async fn harness_kind_matches_what_probe_itself_reports() {
    let (adapter, _scratch) = adapter();
    let capability = adapter.probe().await;
    assert_eq!(
        HarnessProbe::harness_kind(&adapter).as_str(),
        capability.harness_kind.as_str()
    );
}

/// Direct regression guard: this adapter's only
/// cancellation primitive is `harness::process::SupervisedProcess::cancel`
/// (a process-group SIGTERM/SIGKILL), which cannot reliably
/// reach a descendant a harness's own shell-tool spawns into a new OS
/// session — and the registration-time gate
/// (`AdapterRegistry::register_probe`) refuses to register any probe
/// still claiming `Supported`. This pins the value directly, not only
/// through the registration side effect.
#[test]
fn declared_cancel_capability_is_advisory_not_supported() {
    let (adapter, _scratch) = adapter();
    let declared = HarnessProbe::declared_capabilities(&adapter);
    assert_eq!(declared.cancel.support, CapabilitySupport::Advisory);
    assert!(declared.cancel.reason.is_some());
}

// ---- reconcile() -----------------------------------------------------

fn journal_with_process(process_id: Option<&str>) -> AttemptJournal {
    AttemptJournal {
        attempt_id: AttemptId::new("attempt"),
        runner_id: RunnerId::new("runner"),
        fencing_token: FencingToken(1),
        workspace: WorkspaceJournal {
            workspace_id: WorkspaceId::new("ws_codex_test"),
            path: PathBuf::from("/tmp/does-not-matter"),
            base_revision: "revision".into(),
        },
        state: JournalState::ProcessObservedRunning,
        process_id: process_id.map(str::to_owned),
        last_event_checkpoint: None,
        pending_terminal_report: None,
    }
}

#[tokio::test]
async fn reconcile_with_no_recorded_process_id_reports_stopped_without_dispatch() {
    let (adapter, _scratch) = adapter();
    let observation = adapter
        .reconcile(&journal_with_process(None))
        .await
        .expect("reconcile");
    assert_eq!(observation, RecoveryObservation::ProcessStopped);
}

#[tokio::test]
async fn reconcile_rejects_an_unrecognized_handle_encoding_as_explicitly_unavailable() {
    let (adapter, _scratch) = adapter();
    let journal = journal_with_process(Some("not-a-codex-handle"));
    assert!(matches!(
        adapter.reconcile(&journal).await,
        Err(HarnessError::RecoveryUnavailable)
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn reconcile_observes_a_real_alive_process_as_running() {
    let (adapter, _scratch) = adapter();
    // A real, independently-alive process this test controls directly
    // (not spawned via the adapter, but a genuine live pid either way).
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep");
    let journal = journal_with_process(Some(&encode_handle(child.id(), 0)));

    let observation = adapter.reconcile(&journal).await.expect("reconcile");
    assert_eq!(observation, RecoveryObservation::ProcessRunning);

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
#[tokio::test]
async fn reconcile_observes_a_dead_pid_as_stopped() {
    let (adapter, _scratch) = adapter();
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawn true");
    let pid = child.id();
    let _ = child.wait(); // reaped: pid is now dead (short-lived `true`)

    let observation = adapter
        .reconcile(&journal_with_process(Some(&encode_handle(pid, 0))))
        .await
        .expect("reconcile");
    assert_eq!(observation, RecoveryObservation::ProcessStopped);
}

// ---- helpers -----------------------------------------------------

async fn wait_for_pidfile(path: &std::path::Path) -> u32 {
    for _ in 0..200 {
        if let Ok(contents) = std::fs::read_to_string(path)
            && let Ok(pid) = contents.trim().parse::<u32>()
        {
            return pid;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("grandchild pidfile was never written: {}", path.display());
}

async fn wait_until_dead(pid: u32, budget: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + budget;
    while tokio::time::Instant::now() < deadline {
        if !crate::harness::process::process_alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    !crate::harness::process::process_alive(pid)
}

// -----------------------------------------------------------------
// Provider endpoint injection: a configured entry reaches a spawned
// process only when the request actually names it; a direct request
// must receive none of it.
// -----------------------------------------------------------------

fn enabled_gateway_providers(secret_name: &str) -> BTreeMap<String, crate::config::ProviderConfig> {
    BTreeMap::from([(
        crate::config::VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        crate::config::ProviderConfig {
            enabled: true,
            secret: secret_name.to_owned(),
        },
    )])
}

/// A shim that records the *names* only of the environment variables it
/// was spawned with — never a value.
fn env_name_dump_locator(workspace: &std::path::Path, marker: &std::path::Path) -> CodexLocator {
    // A single external process (`env`), no pipe to a second one: the
    // name/value split happens in `recorded_env_names` instead, purely
    // to keep this shim's own process footprint minimal under a
    // heavily parallel test run.
    // Only the run itself records its environment. The adapter also
    // invokes this shim as `<shim> --version` for its version probe,
    // with the probe's own empty environment, and that probe can finish
    // after the run — an unconditional `env > marker` then holds
    // whichever process wrote last. `exec` is the run's subcommand; the
    // probe never passes it.
    let script = format!(
        "#!/bin/sh\nfor arg in \"$@\"; do [ \"$arg\" = exec ] && env > {}; done\nexit 0\n",
        marker.display()
    );
    let script_path = workspace.join("dump-env-names.sh");
    std::fs::write(&script_path, script).expect("write shim script");
    CodexLocator::Fixed {
        program: PathBuf::from("/bin/sh"),
        prefix_args: vec![script_path.display().to_string()],
    }
}

/// The *names* only of the `KEY=VALUE` lines `env`'s output wrote to
/// `marker` — this helper is what actually discards every value, so no
/// caller ever inspects one, even a dummy one seeded for a test.
fn recorded_env_names(marker: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(marker)
        .expect("shim wrote the env-names marker")
        .lines()
        .filter_map(|line| line.split('=').next())
        .map(str::to_owned)
        .collect()
}

/// Acceptance: a request naming a direct model provider must spawn
/// with neither the provider endpoint's `-c model_provider` flag nor
/// its credential variable present — even though a gateway entry is
/// configured and enabled on this same adapter.
#[tokio::test]
async fn a_direct_model_request_spawns_with_no_provider_endpoint_variable_present() {
    let workspace_dir = deterministic_fixture_repo("provider-guard-direct");
    let workspace = workspace_dir.path();
    let marker = workspace.join("env-names.marker");
    let scratch = temp_dir("secrets");
    let secrets = test_secret_store(scratch.path());
    secrets
        .set("demo-secret", "unused-by-a-direct-request")
        .expect("seed store");
    let scratch = temp_dir("artifacts");
    let adapter = CodexAdapter::with_clock(
        env_name_dump_locator(workspace, &marker),
        generous_limits(),
        Duration::from_secs(5),
        BTreeMap::new(),
        scratch.path().to_path_buf(),
        clock_at("2026-08-09T12:00:00Z"),
        secrets,
    )
    .with_providers(enabled_gateway_providers("demo-secret"));

    let spec = spec_with(workspace.to_path_buf(), Some(("openai", "gpt-5")), &[]);
    adapter.validate(&spec).await.expect("validate");
    let handle = adapter.start(&spec).await.expect("start");
    let _ = adapter.wait(&handle).await.expect("wait");

    let names = recorded_env_names(&marker);
    assert!(
        !names.iter().any(|name| name == "AI_GATEWAY_API_KEY"),
        "a direct request must never receive the provider endpoint's credential: {names:?}"
    );
}

/// The positive half of the same proof: a request naming the
/// configured provider does receive its credential variable name.
#[tokio::test]
async fn a_configured_provider_request_spawns_with_its_endpoint_variable_present() {
    let workspace_dir = deterministic_fixture_repo("provider-guard-configured");
    let workspace = workspace_dir.path();
    let marker = workspace.join("env-names.marker");
    let scratch = temp_dir("secrets");
    let secrets = test_secret_store(scratch.path());
    secrets
        .set("demo-secret", "a-resolvable-value")
        .expect("seed store");
    let adapter = CodexAdapter::with_clock(
        env_name_dump_locator(workspace, &marker),
        generous_limits(),
        Duration::from_secs(5),
        BTreeMap::new(),
        scratch.path().to_path_buf(),
        clock_at("2026-08-09T12:00:00Z"),
        secrets,
    )
    .with_providers(enabled_gateway_providers("demo-secret"));

    let spec = spec_with(
        workspace.to_path_buf(),
        Some((crate::config::VERCEL_AI_GATEWAY_PROVIDER, "openai/gpt-5.1")),
        &[],
    );
    adapter
        .validate(&spec)
        .await
        .expect("validate a configured-provider request");
    let handle = adapter
        .start(&spec)
        .await
        .expect("start a configured-provider request");
    let _ = adapter.wait(&handle).await.expect("wait");

    let names = recorded_env_names(&marker);
    assert!(
        names.iter().any(|name| name == "AI_GATEWAY_API_KEY"),
        "a gateway-routed request must receive the provider endpoint's credential: {names:?}"
    );
}

/// A configured-but-disabled provider must reject the request
/// pre-spawn with a typed reason, not silently fall back to Codex's
/// own built-in provider.
#[tokio::test]
async fn a_disabled_provider_rejects_the_request_before_any_process_spawns() {
    let scratch = temp_dir("secrets");
    let secrets = test_secret_store(scratch.path());
    secrets
        .set("demo-secret", "irrelevant")
        .expect("seed store");
    let providers = BTreeMap::from([(
        crate::config::VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        crate::config::ProviderConfig {
            enabled: false,
            secret: "demo-secret".to_owned(),
        },
    )]);
    let (adapter, _scratch) = adapter_with_env(BTreeMap::new());
    let adapter = adapter.with_providers(providers);
    let workspace_dir = deterministic_fixture_repo("provider-guard-disabled");
    let workspace = workspace_dir.path();
    let spec = spec_with(
        workspace.to_path_buf(),
        Some((crate::config::VERCEL_AI_GATEWAY_PROVIDER, "openai/gpt-5.1")),
        &[],
    );

    let error = adapter
        .validate(&spec)
        .await
        .expect_err("a disabled provider must reject at validate, before any spawn");
    assert!(matches!(error, HarnessError::Rejected { .. }));
}

// ---- opt-in live test ------------------------------------------------

/// Acceptance: "an opt-in live test records version and artifact."
///
/// Deliberately does **not** attempt a real, non-interactive `codex
/// exec` run: whether that requires network
/// access and provider credentials, and rule 8 ("live harness tests ...
/// never require secrets in CI") makes that an unacceptable risk to take
/// on a guess. Instead this test performs two things that are safe
/// without any credential:
///
/// 1. Real version probing against whatever `codex` is actually on
///    `PATH` (the operation most CLIs support without authentication).
/// 2. Staging a real artifact (a fixed local file, not one produced by
///    running a task) through the exact same [`ArtifactStager`] path
///    `wait()` uses, proving the local mechanism end-to-end.
///
/// Both `#[ignore]`d (so a plain `cargo test` never runs this) and
/// self-skipping at runtime if `codex` is absent, so it can never fail
/// CI and is never the only proof of either behavior (the fake-binary
/// tests above already cover both independently).
#[tokio::test]
#[ignore = "opt-in: requires a real `codex` binary on PATH; run with \
            `cargo nextest run --workspace --run-ignored ignored-only -E 'test(/codex::tests::live_/)'`"]
async fn live_probe_and_artifact_staging_against_a_real_codex_binary_when_present() {
    if super::super::locate::locate_installed(CODEX_PROGRAM_NAME).is_err() {
        eprintln!("skipping live codex test: `codex` not found on PATH");
        return;
    }

    let scratch = temp_dir("live-artifacts");
    let adapter = CodexAdapter::discover(
        ProcessLimits::new(1_048_576, 1_048_576, Duration::from_secs(30)),
        scratch.path().to_path_buf(),
        test_secret_store(scratch.path()),
    );

    let capability = adapter.probe().await;
    eprintln!(
        "live codex probe: installed_version={:?} probe_error={:?}",
        capability.installed_version, capability.probe_error
    );
    // The observed real CLI prints a program-name-prefixed version
    // (`codex-cli 0.149.1`); a probe error here means either the
    // installed binary changed its output shape again or the token
    // scan regressed — either way this is the signal to look again,
    // not an assertion to weaken back to "ran without panicking".
    assert_eq!(
        capability.probe_error, None,
        "codex version probe must recognize the installed binary's real output"
    );

    let workspace_dir = deterministic_fixture_repo("live-artifact");
    let workspace = workspace_dir.path();
    let staging = temp_dir("live-artifact-staging");
    let stager = ArtifactStager::new(staging.path().to_path_buf());
    let staged = stager
        .stage_file(
            "live-attempt",
            workspace,
            std::path::Path::new("README.md"),
            "log",
            "text/plain",
        )
        .expect("stage a real local artifact");
    assert!(staged.size_bytes > 0);
    assert_eq!(
        staged.sha256,
        crate::harness::sha256::sha256_hex(b"# fixture repo\n")
    );
    eprintln!(
        "live codex artifact staged at {}",
        staged.staged_path.display()
    );

    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// Live proof of the provider endpoint path: resolves the real
/// runner-local secret store for a `vercel_ai_gateway` entry, points a
/// real `codex` binary at it via the per-invocation `-c` overrides
/// (never `~/.codex/config.toml`), and records what the CLI reported.
/// Requires an explicit opt-in even under `--ignored`, matching
/// `claude_code.rs`'s identical gateway test and unlike the credential-
/// free live test above: this one does attempt a real, billed `exec`.
#[tokio::test]
#[ignore = "opt-in: requires a real `codex` binary on PATH, a `vercel_ai_gateway` entry in \
            this machine's secret store, *and* TACK_RUN_LIVE_CODEX_GATEWAY_TEST=1 (a real \
            invocation is billed); run with TACK_RUN_LIVE_CODEX_GATEWAY_TEST=1 cargo nextest \
            run --workspace --run-ignored ignored-only -E 'test(/codex::tests::live_/)'"]
async fn live_codex_through_the_configured_provider_when_opted_in() {
    if std::env::var("TACK_RUN_LIVE_CODEX_GATEWAY_TEST").as_deref() != Ok("1") {
        eprintln!(
            "skipping live codex gateway test: set TACK_RUN_LIVE_CODEX_GATEWAY_TEST=1 to opt \
             in (a real invocation is billed)"
        );
        return;
    }
    if super::super::locate::locate_installed(CODEX_PROGRAM_NAME).is_err() {
        eprintln!("skipping live codex gateway test: `codex` not found on PATH");
        return;
    }
    let state_dir = std::env::var_os("TACK_RUNNER_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").expect("HOME is set")).join(".tack-runner")
        });
    let secrets = crate::secrets::SecretStore::open(&state_dir.join("secrets.json"));
    let providers = BTreeMap::from([(
        crate::config::VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        crate::config::ProviderConfig {
            enabled: true,
            secret: crate::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET.to_owned(),
        },
    )]);

    let scratch = temp_dir("live-gateway-artifacts");
    let adapter = CodexAdapter::discover(
        ProcessLimits::new(1_048_576, 1_048_576, Duration::from_secs(60)),
        scratch.path().to_path_buf(),
        secrets,
    )
    .with_providers(providers);

    let workspace_dir = deterministic_fixture_repo("live-gateway");
    let workspace = workspace_dir.path();
    // Codex refuses to run outside a git repository; a real workspace
    // is always a git checkout (`WorkspaceManager`), so this makes the
    // fixture structurally match production rather than special-casing
    // the check away.
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "tack-live-test@example.invalid"],
        vec!["config", "user.name", "tack-live-test"],
    ] {
        std::process::Command::new("git")
            .args(&args)
            .current_dir(workspace)
            .status()
            .expect("git init the fixture repo");
    }
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(workspace)
        .status()
        .expect("git add");
    std::process::Command::new("git")
        .args(["commit", "-q", "-m", "fixture"])
        .current_dir(workspace)
        .status()
        .expect("git commit");
    // `openai/gpt-5.1` and `openai/gpt-5.1-codex` were both measured
    // live to fail here on a codex-side tool the resolved model
    // doesn't support ("Tool 'tool_search' is not supported with
    // ...") — a model-compatibility rejection, not an auth or routing
    // failure (the gateway's own routing metadata confirmed both
    // requests reached and were resolved by the real gateway).
    // `openai/gpt-5.6-sol` is Vercel's own documented default model
    // for Codex through the gateway.
    let mut spec = spec_with(
        workspace.to_path_buf(),
        Some((
            crate::config::VERCEL_AI_GATEWAY_PROVIDER,
            "openai/gpt-5.6-sol",
        )),
        &[],
    );
    spec.work.request.timeout_seconds = 60;
    spec.work.request.resolved_agent_profile.instructions = "Say exactly: ok".to_string();

    if let Err(error) = adapter.validate(&spec).await {
        eprintln!(
            "skipping live codex gateway test: no configured provider entry to validate \
             against ({error})"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
        return;
    }
    let handle = adapter
        .start(&spec)
        .await
        .expect("start a live gateway-routed process");
    let outcome = adapter
        .wait(&handle)
        .await
        .expect("wait for a live gateway-routed process");

    eprintln!(
        "live codex (gateway) outcome: terminal_state={:?} model_provider={} model_id={} \
         model_observation_source={} terminal_reason={}",
        outcome.terminal_state,
        outcome.actual_execution.model_provider.as_str(),
        outcome.actual_execution.model_id.as_str(),
        outcome.actual_execution.model_observation_source,
        outcome.terminal_reason
    );

    // Codex always echoes the requested provider/model rather than
    // attempting to observe one (module docs, assumption 5) — this
    // holds regardless of whether the configured credential is valid.
    assert_eq!(
        outcome.actual_execution.model_provider.as_str(),
        crate::config::VERCEL_AI_GATEWAY_PROVIDER
    );
    assert_eq!(
        outcome.actual_execution.model_id.as_str(),
        "openai/gpt-5.6-sol"
    );
    assert_eq!(
        outcome.actual_execution.model_observation_source,
        MODEL_OBSERVATION_SOURCE
    );

    std::fs::remove_dir_all(workspace).expect("cleanup");
}
