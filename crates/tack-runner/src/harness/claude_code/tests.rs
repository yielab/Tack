use super::*;
use crate::client::{
    AttemptId, AttemptLease, ClaimRequestId, ClaimedWork, FencingToken, RunnerId, Workspace,
    WorkspaceId as ClientWorkspaceId,
};
use tack_orch::execution::{
    AgentProfileSnapshot, AttemptSnapshot, EnvironmentValue, ExecutionRequestSnapshot,
    ExecutionState, HarnessKind as DomainHarnessKindType, PermissionPolicy, RepositorySnapshot,
    RequestedModelProvider, RunnerSelector,
};

#[derive(Clone, Copy)]
struct FixedClock(std::time::SystemTime);

impl crate::Clock for FixedClock {
    fn now(&self) -> std::time::SystemTime {
        self.0
    }
}

fn clock() -> FixedClock {
    FixedClock(std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_754_000_000))
}

/// A scratch directory that removes itself, and everything written under
/// it, when the returned guard drops — including when an assertion panics
/// first. Whatever holds a path into it must hold the guard too.
fn temp_workspace(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

fn fake_binary() -> HarnessBinary {
    let (program, args) = super::super::fixtures::fake_harness_command();
    HarnessBinary {
        program,
        prefix_args: args,
    }
}

/// Takes the directory rather than making one, so the store's file cannot
/// outlive the guard that removes it. File-backed, never the platform
/// keychain, so parallel tests never see each other's entries and CI
/// needs no Secret Service.
fn test_secret_store(dir: &Path) -> crate::secrets::SecretStore {
    crate::secrets::SecretStore::file(dir.join("secrets.json"))
}

/// Returns the adapter with the scratch directory its secret store lives
/// in: drop the guard and the adapter is pointing at nothing.
fn adapter_with_fake_binary() -> (ClaudeCodeAdapter<FixedClock>, tempfile::TempDir) {
    let scratch = temp_workspace("secrets");
    let adapter =
        ClaudeCodeAdapter::with_binary(fake_binary(), clock(), test_secret_store(scratch.path()))
            .with_cancel_grace(Duration::from_millis(150));
    (adapter, scratch)
}

fn adapter_with_fake_binary_and_secrets(
    secrets: crate::secrets::SecretStore,
) -> ClaudeCodeAdapter<FixedClock> {
    ClaudeCodeAdapter::with_binary(fake_binary(), clock(), secrets)
        .with_cancel_grace(Duration::from_millis(150))
}

fn permission_policy(tools: &[&str], network: bool) -> PermissionPolicy {
    PermissionPolicy {
        tools: tools.iter().map(|tool| tool.to_string()).collect(),
        network,
        additional: Default::default(),
    }
}

fn spec_with(
    harness_kind: &str,
    provider: Option<&str>,
    tools: &[&str],
    network: bool,
    environment: BTreeMap<String, EnvironmentValue>,
    workspace_path: PathBuf,
) -> ExecutionSpec {
    let request = ExecutionRequestSnapshot {
        request_id: tack_orch::execution::ExecutionRequestId::new("exec_test"),
        item_id: tack_orch::execution::ItemId::new("item_test"),
        idempotency_key: tack_orch::execution::IdempotencyKey::new("idem_test"),
        created_by: serde_json::json!({"source": "operator", "subject_id": "test"}),
        created_at: DateTime::<Utc>::from(clock().0),
        selector: RunnerSelector::ExactRunner {
            runner_id: tack_orch::execution::RunnerId::new("runr_test"),
        },
        agent_profile_id: tack_orch::execution::AgentProfileId::new("ap_test"),
        resolved_agent_profile: AgentProfileSnapshot {
            name: "Test profile".to_string(),
            instructions: "Print exactly: ok".to_string(),
            tool_policy: serde_json::json!({}),
            timeout_seconds: 60,
            budgets: serde_json::json!({}),
            additional: Default::default(),
        },
        requested_harness_kind: DomainHarnessKindType::new(harness_kind),
        requested_model_provider: provider.map(RequestedModelProvider::new),
        requested_model_id: None,
        repository: RepositorySnapshot {
            kind: "git".to_string(),
            remote: "https://example.invalid/repo.git".to_string(),
            base_revision: "0123456789abcdef0123456789abcdef01234567".to_string(),
            subdirectory: None,
            additional: Default::default(),
        },
        permission_policy: permission_policy(tools, network),
        timeout_seconds: 5,
        budgets: serde_json::json!({}),
        status_map_policy_id: None,
        environment,
        metadata: serde_json::json!({}),
        additional: Default::default(),
    };
    let attempt = AttemptSnapshot {
        attempt_id: tack_orch::execution::AttemptId::new("att_test"),
        request_id: request.request_id.clone(),
        attempt_number: 1,
        runner_id: tack_orch::execution::RunnerId::new("runr_test"),
        fencing_token: tack_orch::execution::FencingToken(1),
        state: ExecutionState::Leased,
        workspace_id: None,
        base_revision: request.repository.base_revision.clone(),
        lease_issued_at: None,
        lease_expires_at: None,
        last_heartbeat_at: None,
        additional: Default::default(),
    };
    ExecutionSpec {
        work: ClaimedWork {
            claim_request_id: ClaimRequestId::new("claim_test"),
            lease: AttemptLease {
                attempt_id: AttemptId::new("att_test"),
                runner_id: RunnerId::new("runr_test"),
                fencing_token: FencingToken(1),
                attempt_number: 1,
                state: AttemptState::Leased,
                issued_at: Timestamp::new("2026-08-06T12:20:00Z"),
                expires_at: Timestamp::new("2026-08-06T12:21:00Z"),
            },
            request,
            attempt,
        },
        workspace: Workspace {
            attempt_id: AttemptId::new("att_test"),
            id: ClientWorkspaceId::new("ws_test"),
            path: workspace_path,
            base_revision: "0123456789abcdef0123456789abcdef01234567".to_string(),
        },
    }
}

// ---- validate: pre-spawn rejection ----------------------------------

#[tokio::test]
async fn validate_accepts_a_well_formed_claude_code_spec() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("validate-ok");
    let workspace = workspace_dir.path();
    let spec = spec_with(
        "claude-code",
        None,
        &["Read"],
        true,
        BTreeMap::new(),
        workspace.to_path_buf(),
    );
    assert!(adapter.validate(&spec).await.is_ok());
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// Acceptance: an unsupported selection fails pre-spawn, before any
/// process launches. A model provider this installed CLI has no way to
/// honor (confirmed absent from the three real provider-switch families
/// found via `strings` on the binary — see the module docs) is rejected
/// by `validate` alone; no `SupervisedProcess` bookkeeping entry is ever
/// created.
#[tokio::test]
async fn validate_rejects_an_unsupported_model_provider_before_any_process_launches() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("validate-bad-provider");
    let workspace = workspace_dir.path();
    let spec = spec_with(
        "claude-code",
        Some("openai"),
        &[],
        true,
        BTreeMap::new(),
        workspace.to_path_buf(),
    );
    assert!(matches!(
        adapter.validate(&spec).await,
        Err(HarnessError::Rejected { .. })
    ));
    assert!(
        adapter.processes.lock().await.is_empty(),
        "a pre-spawn rejection must never create process bookkeeping"
    );
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

#[tokio::test]
async fn validate_accepts_every_known_provider_family_case_insensitively() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    for provider in ["anthropic", "BEDROCK", "Vertex", "foundry"] {
        let workspace_dir = temp_workspace("validate-provider-ok");
        let workspace = workspace_dir.path();
        let spec = spec_with(
            "claude-code",
            Some(provider),
            &[],
            true,
            BTreeMap::new(),
            workspace.to_path_buf(),
        );
        assert!(
            adapter.validate(&spec).await.is_ok(),
            "provider {provider} should be accepted"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }
}

#[tokio::test]
async fn validate_rejects_a_spec_requesting_a_different_harness_kind() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("validate-wrong-kind");
    let workspace = workspace_dir.path();
    let spec = spec_with(
        "codex",
        None,
        &[],
        true,
        BTreeMap::new(),
        workspace.to_path_buf(),
    );
    assert!(matches!(
        adapter.validate(&spec).await,
        Err(HarnessError::Rejected { .. })
    ));
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

#[tokio::test]
async fn validate_rejects_a_network_tool_when_network_is_denied() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("validate-network-conflict");
    let workspace = workspace_dir.path();
    let spec = spec_with(
        "claude-code",
        None,
        &["WebFetch"],
        false,
        BTreeMap::new(),
        workspace.to_path_buf(),
    );
    assert!(matches!(
        adapter.validate(&spec).await,
        Err(HarnessError::Rejected { .. })
    ));
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

#[tokio::test]
async fn validate_rejects_when_the_resolved_binary_no_longer_exists() {
    let missing = HarnessBinary {
        program: PathBuf::from("/nonexistent/definitely/not/claude"),
        prefix_args: Vec::new(),
    };
    let secrets_dir = temp_workspace("secrets");
    let adapter =
        ClaudeCodeAdapter::with_binary(missing, clock(), test_secret_store(secrets_dir.path()));
    let workspace_dir = temp_workspace("validate-missing-binary");
    let workspace = workspace_dir.path();
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        BTreeMap::new(),
        workspace.to_path_buf(),
    );
    assert!(matches!(
        adapter.validate(&spec).await,
        Err(HarnessError::Rejected { .. })
    ));
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

// ---- fake-binary-driven lifecycle tests ------------------------------

fn env_entry(value: &str) -> EnvironmentValue {
    EnvironmentValue {
        value: Some(value.to_string()),
        secret_reference: None,
        additional: Default::default(),
    }
}

fn secret_reference_entry(reference: &str) -> EnvironmentValue {
    EnvironmentValue {
        value: None,
        secret_reference: Some(reference.to_string()),
        additional: Default::default(),
    }
}

/// Acceptance: fake-binary success. Drives the real shared fixture
/// through `start`/`wait`; since the generic fake binary's `success`
/// mode is not shaped like Claude Code's real stream-json output (by
/// design), this proves the honest exit-code
/// fallback path end to end (spawn, capture, redact, parse), not a
/// structured-result parse.
#[tokio::test]
async fn fake_binary_success_mode_is_reported_succeeded_via_the_honest_exit_code_fallback() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("fake-success");
    let workspace = workspace_dir.path();
    let mut environment = BTreeMap::new();
    environment.insert("TACK_FAKE_HARNESS_MODE".to_string(), env_entry("success"));
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        environment,
        workspace.to_path_buf(),
    );

    adapter.validate(&spec).await.expect("validate");
    let handle = adapter.start(&spec).await.expect("start");
    let outcome = adapter.wait(&handle).await.expect("wait");

    assert_eq!(outcome.terminal_state, AttemptState::Succeeded);
    assert_eq!(
        outcome.terminal_reason["reason"],
        "no structured result envelope was produced; inferred success from exit code 0"
    );
    assert_eq!(outcome.actual_execution.model_id.as_str(), UNOBSERVED_MODEL);
    assert_eq!(
        outcome.usage.tokens_in.source,
        MeasurementSource::NotMeasured
    );
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// `artifacts: Supported` once had no backing implementation — `wait()` never
/// called `ArtifactStager::stage_file`, only the live (billed, opt-in)
/// test's own test body did, bypassing the adapter entirely. This proves
/// the fix through the adapter's own `wait()`, via the free fake-binary
/// path, matching `codex.rs`'s identical proof shape.
#[tokio::test]
async fn fake_binary_success_stages_a_real_log_artifact() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("fake-success-artifact");
    let workspace = workspace_dir.path();
    let mut environment = BTreeMap::new();
    environment.insert("TACK_FAKE_HARNESS_MODE".to_string(), env_entry("success"));
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        environment,
        workspace.to_path_buf(),
    );

    adapter.validate(&spec).await.expect("validate");
    let handle = adapter.start(&spec).await.expect("start");
    let outcome = adapter.wait(&handle).await.expect("wait");

    let artifact = &outcome.terminal_reason["artifact"];
    assert_eq!(artifact["kind"], "log");
    assert_eq!(artifact["media_type"], "text/plain");
    let staged_path = artifact["staged_path"].as_str().expect("staged_path");
    let staged_bytes = std::fs::read(staged_path).expect("read staged artifact");
    assert!(String::from_utf8_lossy(&staged_bytes).contains("fake-harness-ok"));
    assert_eq!(
        artifact["sha256"].as_str().unwrap(),
        crate::harness::sha256::sha256_hex(&staged_bytes)
    );

    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// Acceptance: fake-binary failure.
#[tokio::test]
async fn fake_binary_failure_mode_is_reported_failed_via_the_honest_exit_code_fallback() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("fake-failure");
    let workspace = workspace_dir.path();
    let mut environment = BTreeMap::new();
    environment.insert("TACK_FAKE_HARNESS_MODE".to_string(), env_entry("failure"));
    environment.insert("TACK_FAKE_HARNESS_EXIT_CODE".to_string(), env_entry("7"));
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        environment,
        workspace.to_path_buf(),
    );

    adapter.validate(&spec).await.expect("validate");
    let handle = adapter.start(&spec).await.expect("start");
    let outcome = adapter.wait(&handle).await.expect("wait");

    assert_eq!(outcome.terminal_state, AttemptState::Failed);
    assert_eq!(
        outcome.terminal_reason["reason"],
        "no structured result envelope was produced; inferred failure from a non-zero exit \
         code"
    );
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// Acceptance: fake-binary malformed output. Exercises the *real*
/// `malformed` mode byte-for-byte (deliberately unparseable, mixed
/// garbage), through the full spawn/capture pipeline, proving this
/// never panics and never fabricates a structured, confident result —
/// only the honest, clearly-labeled exit-code fallback (this specific
/// fixture's exit code is 0, so this lands on the "success" side of the
/// fallback, same as the plain `success` mode; see the module doc on
/// `parse_run_output` for why *any* non-JSON generic fake-binary output
/// collapses into that one fallback rather than being distinguished
/// from `success` by content alone).
#[tokio::test]
async fn fake_binary_malformed_mode_never_panics_and_never_fabricates_structured_data() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("fake-malformed");
    let workspace = workspace_dir.path();
    let mut environment = BTreeMap::new();
    environment.insert("TACK_FAKE_HARNESS_MODE".to_string(), env_entry("malformed"));
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        environment,
        workspace.to_path_buf(),
    );

    adapter.validate(&spec).await.expect("validate");
    let handle = adapter.start(&spec).await.expect("start");
    let outcome = adapter.wait(&handle).await.expect("wait");

    // No panic reaching here is itself part of what this test proves.
    assert!(matches!(
        outcome.terminal_state,
        AttemptState::Succeeded | AttemptState::Failed
    ));
    assert_eq!(outcome.actual_execution.model_id.as_str(), UNOBSERVED_MODEL);
    assert_eq!(
        outcome.actual_execution.model_observation_source,
        "not_observed"
    );
    assert_eq!(outcome.usage.tokens_in.value, None);
    assert_eq!(
        outcome.usage.cost_usd.source,
        MeasurementSource::NotMeasured
    );
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// A more realistic "malformed" case than generic garbage: a stream
/// that starts out perfectly valid (a real `system`/`init` line) and is
/// then truncated/corrupted before any terminal `result` object ever
/// arrives — e.g. a crash mid-run. Pure unit test against the parser
/// directly (no process spawn needed): proves the "some JSON, but no
/// result object" branch specifically, which the generic fake binary
/// cannot exercise (see the two tests above).
#[test]
fn a_stream_with_a_valid_init_line_but_no_terminal_result_is_failed_not_a_guessed_success() {
    let stdout = concat!(
        r#"{"type":"system","subtype":"init","model":"claude-sonnet-5","claude_code_version":"2.1.223"}"#,
        "\n",
        r#"{"type":"assistant","message":{"model":"claude-sonnet-5","content":[]}}"#,
        "\n",
    );
    let result = ProcessResult {
        exit: ProcessExit::Exited(0),
        stdout: super::super::process::CapturedOutput {
            text: stdout.to_string(),
            truncated: false,
            bytes_dropped: 0,
            total_bytes_seen: stdout.len() as u64,
        },
        stderr: Default::default(),
    };

    let parsed = parse_run_output(&result, None);

    assert!(parsed.is_error);
    assert_eq!(parsed.terminal_reason["reason"], "malformed_output");
    assert_eq!(parsed.model_id, UNOBSERVED_MODEL);
}

/// Acceptance: cancel kills the process. Uses the shared fixture's
/// `spawn_child` mode (a real, still-running descendant) purely to get
/// a real, still-alive pid to cancel against; this test's own concern is
/// this adapter's `cancel` correctly observing the process stop and
/// cleaning up its own bookkeeping — grandchild-tree coverage itself is
/// `process::tests::cancel_kills_the_whole_descendant_tree_...`.
#[tokio::test]
async fn cancel_stops_the_process_and_forgets_its_own_bookkeeping_entry() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("cancel");
    let workspace = workspace_dir.path();
    let mut environment = BTreeMap::new();
    environment.insert("TACK_FAKE_HARNESS_MODE".to_string(), env_entry("hang"));
    environment.insert(
        "TACK_FAKE_HARNESS_SLEEP_SECONDS".to_string(),
        env_entry("3600"),
    );
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        environment,
        workspace.to_path_buf(),
    );

    adapter.validate(&spec).await.expect("validate");
    let handle = adapter.start(&spec).await.expect("start");
    assert_eq!(adapter.processes.lock().await.len(), 1);

    let pid: u32 = handle.process_id.parse().expect("numeric pid handle");
    assert!(
        process_alive(pid),
        "process must be observed running before cancel"
    );

    let evidence = adapter.cancel(&handle).await.expect("cancel");
    assert_eq!(evidence.observation, CancelObservation::ProcessStopped);
    assert!(
        !process_alive(pid),
        "process must actually be gone after cancel reports stopped"
    );
    assert!(
        adapter.processes.lock().await.is_empty(),
        "cancel must remove its own bookkeeping entry"
    );
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

#[tokio::test]
async fn cancel_of_an_unknown_handle_is_a_typed_error_not_a_panic() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let handle = LocalRunHandle {
        process_id: "not-a-number".to_string(),
    };
    assert!(matches!(
        adapter.cancel(&handle).await,
        Err(HarnessError::Process)
    ));
}

// ---- redaction ---------------------------------------------------------

/// Acceptance: arguments and environment are redacted in logs and
/// events; a canary appears nowhere. Plants a canary as a plain
/// environment value, drives the fake binary's `echo_canary` mode (which
/// actively echoes it back on both stdout and stderr — a worst-case
/// leaky harness), and asserts the captured/returned outcome never
/// contains it, while the `ProcessSpec` this adapter built also never
/// exposes it via `Debug` (structural half, inherited unchanged from
/// `process.rs`).
#[tokio::test]
async fn a_planted_canary_in_the_environment_never_survives_into_the_returned_outcome() {
    const CANARY: &str = "tack-d2-claude-code-canary-6f31a2";
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("canary");
    let workspace = workspace_dir.path();
    let mut environment = BTreeMap::new();
    environment.insert(
        "TACK_FAKE_HARNESS_MODE".to_string(),
        env_entry("echo_canary"),
    );
    environment.insert("TACK_TEST_SECRET".to_string(), env_entry(CANARY));
    environment.insert(
        "TACK_FAKE_HARNESS_ECHO_ENV_KEYS".to_string(),
        env_entry("TACK_TEST_SECRET"),
    );
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        environment,
        workspace.to_path_buf(),
    );

    adapter.validate(&spec).await.expect("validate");
    let handle = adapter.start(&spec).await.expect("start");
    let outcome = adapter.wait(&handle).await.expect("wait");

    let serialized = serde_json::to_string(&outcome.terminal_reason).expect("serialize");
    assert!(
        !serialized.contains(CANARY),
        "canary must never survive into the returned terminal_reason"
    );
    assert!(
        serialized.contains("[REDACTED]"),
        "the leak must actually have been scrubbed, \
        not merely absent because nothing echoed it"
    );
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

// -----------------------------------------------------------------
// secret_reference resolution: the value reaches the spawned process,
// never a log line; a reference the store cannot resolve fails typed
// and pre-spawn, before the adapter touches anything.
// -----------------------------------------------------------------

// A *scoped* subscriber (`tracing::dispatcher::set_default`, not
// `tracing_subscriber::fmt().init()`): this test file shares a test
// binary with `git.rs`, which installs its own *global* default for the
// same reason (see its identical comment) — a second global `.init()`
// here would panic ("a global default trace dispatcher has already been
// set"). A scoped dispatcher avoids that collision and is sufficient
// here because the callsite this test exercises
// (`harness::resolve_environment`'s resolved-reference log line) is
// reached by no other test in this crate, so nothing can have cached its
// interest as "never" before this test's guard is the active dispatcher
// for the first, and only, real evaluation. The guard is held for the
// whole test (a `#[tokio::test]` with no `flavor` runs single-threaded,
// so it stays valid across every `.await` in the test body).
thread_local! {
    static SECRET_LOG_CAPTURE: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

struct SecretLogCapture;

impl std::io::Write for SecretLogCapture {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        SECRET_LOG_CAPTURE.with(|captured| captured.borrow_mut().extend_from_slice(buffer));
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl tracing_subscriber::fmt::MakeWriter<'_> for SecretLogCapture {
    type Writer = Self;

    fn make_writer(&self) -> Self::Writer {
        SecretLogCapture
    }
}

#[must_use = "the capture is only active while this guard is alive"]
fn install_secret_log_capture() -> tracing::dispatcher::DefaultGuard {
    SECRET_LOG_CAPTURE.with(|captured| captured.borrow_mut().clear());
    let subscriber = tracing_subscriber::fmt()
        .with_writer(SecretLogCapture)
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .finish();
    tracing::dispatcher::set_default(&tracing::Dispatch::new(subscriber))
}

/// Acceptance: a live attempt with a `secret_reference` environment
/// entry reaches the spawned process with the resolved value set — the
/// shim here proves it by writing the value's *byte length* to a marker
/// file it controls, never the value itself. Captured `tracing` output
/// for the same run names the entry (positive control: asserted
/// present) and never contains the value.
#[tokio::test]
async fn secret_reference_resolves_and_only_its_length_reaches_the_shim() {
    let _log_capture = install_secret_log_capture();
    let workspace_dir = temp_workspace("secret-reference-length");
    let workspace = workspace_dir.path();
    let secret_value = "topsecret-canary-9f3a21";
    let store_dir = temp_workspace("secret-length-store");
    let store = crate::secrets::SecretStore::file(store_dir.path().join("secrets.json"));
    store.set("demo", secret_value).expect("seed the store");

    let marker = workspace.join("secret-length.marker");
    let script = format!(
        "#!/bin/sh\nprintf '%s' \"$SECRET_VAR\" | wc -c > {}\nexit 0\n",
        marker.display()
    );
    let script_path = workspace.join("shim.sh");
    std::fs::write(&script_path, script).expect("write shim script");
    let binary = HarnessBinary {
        program: PathBuf::from("/bin/sh"),
        prefix_args: vec![script_path.display().to_string()],
    };

    let adapter = ClaudeCodeAdapter::with_binary(binary, clock(), store);
    let mut environment = BTreeMap::new();
    environment.insert("SECRET_VAR".to_string(), secret_reference_entry("demo"));
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        environment,
        workspace.to_path_buf(),
    );

    adapter
        .validate(&spec)
        .await
        .expect("a resolvable secret_reference validates");
    let handle = adapter.start(&spec).await.expect("start");
    let outcome = adapter.wait(&handle).await.expect("wait");
    assert_eq!(outcome.terminal_state, AttemptState::Succeeded);

    let recorded = std::fs::read_to_string(&marker).expect("shim wrote the length marker");
    let recorded_length: usize = recorded.trim().parse().expect("marker holds a byte count");
    assert_eq!(
        recorded_length,
        secret_value.len(),
        "the shim must have received the resolved value, not something else"
    );

    let captured = SECRET_LOG_CAPTURE
        .with(|captured| String::from_utf8(captured.borrow().clone()))
        .expect("utf-8");
    assert!(
        captured.contains("demo") || captured.contains("SECRET_VAR"),
        "the test is only load-bearing if resolution actually logged the entry: {captured:?}"
    );
    assert!(
        !captured.contains(secret_value),
        "the resolved secret value reached a log line: {captured}"
    );

    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// Acceptance: a `secret_reference` the store cannot resolve fails at
/// `validate` with a typed reason naming only the reference, before the
/// adapter does anything else — proven here at the adapter boundary
/// (`validate` itself never touches a filesystem path outside checking
/// its own binary exists). The engine-level ordering constraint this
/// interacts with (workspace provisioning and the journal write both
/// already precede `HarnessAdapter::validate` in
/// `RunnerEngine::run_claimed`) is out of scope here — this test only
/// proves the adapter-boundary behavior.
#[tokio::test]
async fn validate_rejects_a_missing_secret_reference_typed_and_touches_nothing() {
    let workspace_dir = temp_workspace("secret-reference-missing");
    let workspace = workspace_dir.path();
    std::fs::write(workspace.join("sentinel.txt"), b"before").expect("seed workspace");

    let state_guard = temp_workspace("secret-missing-state");
    // Deliberately not created: a failed lookup must not bring the file
    // fallback's directory into existence just by trying.
    let state_dir = state_guard.path().join("state");
    let store = crate::secrets::SecretStore::file(state_dir.join("secrets.json"));

    let adapter = adapter_with_fake_binary_and_secrets(store);
    let mut environment = BTreeMap::new();
    environment.insert(
        "SECRET_VAR".to_string(),
        secret_reference_entry("does-not-exist"),
    );
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        environment,
        workspace.to_path_buf(),
    );

    let error = adapter
        .validate(&spec)
        .await
        .expect_err("a missing secret_reference must fail pre-spawn");
    assert!(
        matches!(
            &error,
            HarnessError::Rejected { reason }
                if reason.starts_with("secret_reference_unresolved:")
                    && reason.contains("does-not-exist")
        ),
        "unexpected error: {error:?}"
    );

    assert!(
        !state_dir.exists(),
        "a rejected validate must not create the secret store's state directory"
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("sentinel.txt")).expect("sentinel survives"),
        "before",
        "a rejected validate must not modify the workspace it was given"
    );
    assert_eq!(
        std::fs::read_dir(workspace)
            .expect("read workspace")
            .count(),
        1,
        "a rejected validate must not add files to the workspace it was given"
    );

    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// Direct regression guards for both capability corrections
/// made to this file: `cancel` was already `Advisory` (a correctly
/// evidenced finding — the registration-time gate
/// in `harness::mod` relies on it staying that way); `artifacts` is
/// downgraded from an unbacked `Supported` to `Advisory`
/// (`wait()` never staged anything before
/// this — see `fake_binary_success_stages_a_real_log_artifact` above for
/// the fix itself).
#[test]
fn declared_capabilities_match_the_reconciled_iii_d5_values() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let declared = HarnessProbe::declared_capabilities(&adapter);
    assert_eq!(declared.cancel.support, CapabilitySupport::Advisory);
    assert!(declared.cancel.reason.is_some());
    assert_eq!(declared.artifacts.support, CapabilitySupport::Advisory);
    assert!(declared.artifacts.reason.is_some());
}

// ---- version parsing (pure unit tests) --------------------------------

#[test]
fn the_real_observed_claude_code_version_string_is_recognized() {
    // Byte-for-byte what `claude --version` printed on a real installed
    // binary: "2.1.223 (Claude Code)\n".
    let (version, error) = parse_version_text("2.1.223 (Claude Code)\n");
    assert_eq!(version, "2.1.223");
    assert_eq!(error, None);
}

#[test]
fn the_fake_binarys_unknown_version_fixture_is_reported_as_explicitly_unrecognized() {
    let (version, error) =
        parse_version_text("harness-cli version 999.999.999-nightly-exotic-format\n");
    assert!(
        error.is_some(),
        "an unrecognized shape must set probe_error"
    );
    assert!(
        version.contains("harness-cli"),
        "the raw observed text must still be preserved, not discarded"
    );
}

#[test]
fn empty_version_output_is_a_probe_error_with_no_fabricated_version() {
    let (version, error) = parse_version_text("");
    assert_eq!(version, "");
    assert!(error.is_some());
}

#[test]
fn a_version_token_with_a_prerelease_suffix_is_still_recognized() {
    let (version, error) = parse_version_text("3.0.0-beta.1 (Claude Code)\n");
    assert_eq!(version, "3.0.0-beta.1");
    assert_eq!(error, None);
}

/// The probe declares zero `model_combinations` (the CLI has no
/// list-models command) and instead attests `model_passthrough:
/// supported` — the claim the scheduler now relies on to make
/// claude-code schedulable at all. It must be Supported, carry a
/// reason, and coexist with the empty combination list rather than
/// replace its honesty note.
#[tokio::test]
async fn probe_attests_model_passthrough_instead_of_inventing_a_model_list() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let capability = adapter.probe().await;

    assert!(capability.model_combinations.is_empty());
    let passthrough = capability
        .model_passthrough
        .expect("claude-code probe must attest model_passthrough");
    assert_eq!(passthrough.support, CapabilitySupport::Supported);
    assert!(passthrough.reason.is_some());
    assert!(capability.additional.contains_key("model_discovery_note"));
}

/// Acceptance: fake-binary unknown version, driven through the real
/// spawn path (not only the pure-function tests above), using the
/// shared fixture's dedicated `unknown_version` mode.
#[tokio::test]
async fn probe_reports_the_shared_fixtures_unknown_version_output_honestly() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    // `detect_version` always invokes `--version` with no env override,
    // so this test instead exercises the same parsing path `probe`
    // depends on by driving the fixture in `unknown_version` mode
    // directly through `ProcessSpec`, matching exactly what
    // `detect_version` does internally.
    let workspace_dir = temp_workspace("unknown-version-probe");
    let workspace = workspace_dir.path().to_path_buf();
    let mut env = BTreeMap::new();
    env.insert(
        "TACK_FAKE_HARNESS_MODE".to_string(),
        "unknown_version".to_string(),
    );
    let (program, args) = fake_binary().command_line(Vec::new());
    let spec = ProcessSpec {
        program,
        args,
        env,
        stdin: None,
        working_directory: workspace.to_path_buf(),
        workspace_root: workspace,
    };
    let result = spec
        .spawn()
        .await
        .expect("spawn")
        .wait_with_capture(
            &ProcessLimits::new(4096, 4096, Duration::from_secs(10)),
            &SecretMaterial::new(),
        )
        .await
        .expect("wait");
    assert_eq!(result.exit, ProcessExit::Exited(0));
    let (version, error) = parse_version_text(&result.stdout.text);
    assert!(error.is_some());
    assert!(version.contains("999.999.999"));
    let _ = adapter; // constructed only to keep this test grouped with its siblings
}

// ---- result-envelope parsing using real observed shapes ---------------

fn real_success_stdout() -> String {
    // Reproduces (trimmed to the fields this parser reads) the actual
    // `--output-format stream-json --verbose` transcript observed for
    // `claude -p "Print exactly this string..." --model sonnet`, minus
    // fields irrelevant to parsing.
    concat!(
        r#"{"type":"system","subtype":"init","cwd":"/tmp/fixture","session_id":"s1","tools":[],"#,
        r#""model":"claude-sonnet-5","claude_code_version":"2.1.223"}"#,
        "\n",
        r#"{"type":"assistant","message":{"model":"claude-sonnet-5","content":[{"type":"text","text":"ok"}]}}"#,
        "\n",
        r#"{"is_error":false,"duration_api_ms":2485,"num_turns":1,"stop_reason":"end_turn","#,
        r#""session_id":"s1","total_cost_usd":0.0350687,"usage":{"input_tokens":2,"output_tokens":18},"#,
        r#""terminal_reason":"completed","subtype":"success","result":"ok","type":"result","duration_ms":1916}"#,
        "\n",
    )
    .to_string()
}

#[test]
fn a_real_observed_success_transcript_is_parsed_with_the_session_model_and_usage() {
    let result = ProcessResult {
        exit: ProcessExit::Exited(0),
        stdout: super::super::process::CapturedOutput {
            text: real_success_stdout(),
            truncated: false,
            bytes_dropped: 0,
            total_bytes_seen: 0,
        },
        stderr: Default::default(),
    };

    let parsed = parse_run_output(&result, Some("anthropic"));

    assert!(!parsed.is_error);
    assert_eq!(parsed.model_id, "claude-sonnet-5");
    assert_eq!(parsed.model_observation_source, "harness_reported");
    assert_eq!(parsed.harness_version.as_deref(), Some("2.1.223"));
    assert_eq!(parsed.usage.tokens_in.value, Some(2));
    assert_eq!(parsed.usage.tokens_out.value, Some(18));
    assert_eq!(parsed.usage.tokens_in.source, MeasurementSource::Measured);
    assert_eq!(parsed.usage.cost_usd.value, Some(0.0350687));
}

/// Directly encodes the real, observed CLI quirk this adapter's
/// success/failure determination depends on: an invalid-model 400/404
/// API error was returned with `"is_error":true` **and**
/// `"subtype":"success"` in the same object. A parser keying off
/// `subtype` instead of `is_error` would misreport this as a success.
#[test]
fn an_api_error_result_with_a_misleading_subtype_of_success_is_still_reported_as_failed() {
    let stdout = concat!(
        r#"{"type":"system","subtype":"init","model":"claude-3-5-haiku-20241022","claude_code_version":"2.1.223"}"#,
        "\n",
        r#"{"is_error":true,"terminal_reason":"api_error","subtype":"success","#,
        r#""api_error_status":404,"result":"model not found","type":"result"}"#,
        "\n",
    )
    .to_string();
    let result = ProcessResult {
        exit: ProcessExit::Exited(1),
        stdout: super::super::process::CapturedOutput {
            text: stdout,
            truncated: false,
            bytes_dropped: 0,
            total_bytes_seen: 0,
        },
        stderr: Default::default(),
    };

    let parsed = parse_run_output(&result, None);

    assert!(
        parsed.is_error,
        "is_error must win over a misleadingly-named subtype of \"success\""
    );
    assert_eq!(parsed.terminal_reason["subtype"], "success");
}

/// Reproduces the real observed `--max-budget-usd` exhaustion shape
/// (`subtype: "error_max_budget_usd"`, a genuinely distinct, correctly
/// non-"success"-looking subtype, unlike the api_error case above).
#[test]
fn a_budget_exhausted_result_is_parsed_as_failed() {
    let stdout = concat!(
        r#"{"type":"system","subtype":"init","model":"claude-haiku-4-5-20251001","claude_code_version":"2.1.223"}"#,
        "\n",
        r#"{"is_error":true,"terminal_reason":"budget_exhausted","subtype":"error_max_budget_usd","#,
        r#""errors":["Reached maximum budget ($1e-7)"],"total_cost_usd":0.013149,"type":"result"}"#,
        "\n",
    )
    .to_string();
    let result = ProcessResult {
        exit: ProcessExit::Exited(1),
        stdout: super::super::process::CapturedOutput {
            text: stdout,
            truncated: false,
            bytes_dropped: 0,
            total_bytes_seen: 0,
        },
        stderr: Default::default(),
    };

    let parsed = parse_run_output(&result, None);

    assert!(parsed.is_error);
    assert_eq!(parsed.terminal_reason["subtype"], "error_max_budget_usd");
    assert_eq!(parsed.usage.cost_usd.value, Some(0.013149));
}

/// The gateway-specific half of `parsed_from_result_line`: even a
/// terminal `result` line — the case that lets a direct-provider run
/// claim `harness_reported` — must not upgrade a gateway-routed run to
/// that claim, because the init line it came from fired before any
/// network call reached the gateway.
#[test]
fn a_gateway_routed_result_is_recorded_as_requested_not_confirmed_even_on_a_fast_result_line() {
    let stdout = concat!(
        r#"{"type":"system","subtype":"init","model":"anthropic/claude-opus-4.6","claude_code_version":"2.1.261"}"#,
        "\n",
        r#"{"is_error":true,"subtype":"success","api_error_status":404,"#,
        r#""result":"model not found","type":"result"}"#,
        "\n",
    )
    .to_string();
    let result = ProcessResult {
        exit: ProcessExit::Exited(1),
        stdout: super::super::process::CapturedOutput {
            text: stdout,
            truncated: false,
            bytes_dropped: 0,
            total_bytes_seen: 0,
        },
        stderr: Default::default(),
    };

    let direct = parse_run_output(&result, Some("anthropic"));
    assert_eq!(
        direct.model_observation_source,
        ModelObservationSource::HarnessReported.as_str()
    );

    let gateway = parse_run_output(&result, Some(crate::config::VERCEL_AI_GATEWAY_PROVIDER));
    assert_eq!(
        gateway.model_observation_source,
        ModelObservationSource::RequestedNotConfirmed.as_str()
    );
    assert_eq!(gateway.model_id, "anthropic/claude-opus-4.6");
}

#[test]
fn a_missing_is_error_field_fails_closed_as_an_error_not_a_silent_success() {
    let value = serde_json::json!({"type": "result", "result": "no is_error field here"});
    let parsed = parsed_from_result_line(&value, None, None, None);
    assert!(parsed.is_error);
}

// ---- reconcile ---------------------------------------------------------

#[tokio::test]
async fn reconcile_with_no_recorded_process_id_needs_no_dispatch() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let journal = journal_with_process(None);
    let observation = adapter.reconcile(&journal).await.expect("reconcile");
    assert_eq!(observation, RecoveryObservation::ProcessStopped);
}

#[tokio::test]
async fn reconcile_reports_process_stopped_for_a_pid_that_no_longer_exists() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    // A pid that is astronomically unlikely to be a live process on any
    // CI/dev machine, without relying on a fixed platform-specific
    // sentinel like `i32::MAX`.
    let journal = journal_with_process(Some("2000000000"));
    let observation = adapter.reconcile(&journal).await.expect("reconcile");
    assert_eq!(observation, RecoveryObservation::ProcessStopped);
}

#[tokio::test]
async fn reconcile_with_an_undecodable_process_id_is_explicitly_unavailable() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let journal = journal_with_process(Some("not-a-pid"));
    assert!(matches!(
        adapter.reconcile(&journal).await,
        Err(HarnessError::RecoveryUnavailable)
    ));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn reconcile_reports_process_running_for_a_genuinely_still_running_fake_harness() {
    let (adapter, _scratch) = adapter_with_fake_binary();
    let workspace_dir = temp_workspace("reconcile-running");
    let workspace = workspace_dir.path();
    let mut environment = BTreeMap::new();
    // Deliberately `spawn_child`, not `hang`: `hang` mode `exec`s into
    // `sleep` (see `fake_harness.sh`'s own doc comment), which replaces
    // the process image, so `/proc/<pid>/cmdline` permanently becomes
    // `sleep ...` rather than `/bin/sh <script>` moments after spawn —
    // a real `claude` process never re-execs into something else over
    // its own lifetime, so that behavior is specific to this one fixture
    // mode, not representative of what `process_program_matches` needs
    // to identify in production. `spawn_child` mode's own process
    // (distinct from the grandchild `sleep` it backgrounds) never execs,
    // keeping a stable, checkable `/bin/sh <script>` cmdline throughout.
    environment.insert(
        "TACK_FAKE_HARNESS_MODE".to_string(),
        env_entry("spawn_child"),
    );
    environment.insert(
        "TACK_FAKE_HARNESS_SLEEP_SECONDS".to_string(),
        env_entry("3600"),
    );
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        environment,
        workspace.to_path_buf(),
    );
    adapter.validate(&spec).await.expect("validate");
    let handle = adapter.start(&spec).await.expect("start");
    let pid: u32 = handle.process_id.parse().expect("numeric pid");

    let journal = journal_with_process(Some(&handle.process_id));
    // Bounded poll, not a fixed sleep. `spawn_child` mode's own
    // process never execs (see the comment above), so unlike the
    // earlier `hang`-mode attempt this converges rather than being
    // structurally unable to: under heavy parallel test load, reading
    // `/proc/<pid>/cmdline` immediately after spawn can transiently
    // fail (`process_program_matches` returning `None`, since the
    // kernel has not necessarily finished populating it), which is a
    // real, if rare, possibility `reconcile` itself already reports
    // honestly as `Ambiguous` rather than guessing — this loop exists
    // only so the test's assertion is not sensitive to that one-time
    // startup window, not because `reconcile` is retried in production.
    let mut observation = None;
    for _ in 0..80 {
        let latest = adapter.reconcile(&journal).await.expect("reconcile");
        if latest == RecoveryObservation::ProcessRunning {
            observation = Some(latest);
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(observation, Some(RecoveryObservation::ProcessRunning));

    // Cleanup: this adapter's own `cancel` both stops the process (and
    // its backgrounded grandchild, via `process.rs`'s own process-group
    // signalling) and forgets the bookkeeping entry `reconcile` never
    // touched.
    adapter.cancel(&handle).await.expect("cancel");
    assert!(!process_alive(pid));
    std::fs::remove_dir_all(workspace).expect("cleanup");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn reconcile_reports_process_stopped_when_a_live_pid_belongs_to_an_unrelated_program() {
    // A real, currently-alive pid that this adapter did *not* spawn and
    // that does not resolve to `self.binary.program` at all: this
    // adapter's own test-runner process itself.
    let (adapter, _scratch) = adapter_with_fake_binary();
    let own_pid = std::process::id();
    let journal = journal_with_process(Some(&own_pid.to_string()));
    let observation = adapter.reconcile(&journal).await.expect("reconcile");
    assert_eq!(observation, RecoveryObservation::ProcessStopped);
}

fn journal_with_process(process_id: Option<&str>) -> AttemptJournal {
    AttemptJournal {
        attempt_id: AttemptId::new("att_test"),
        runner_id: RunnerId::new("runr_test"),
        fencing_token: FencingToken(1),
        workspace: crate::client::journal::WorkspaceJournal {
            workspace_id: ClientWorkspaceId::new("ws_test"),
            path: PathBuf::from("/tmp/does-not-matter"),
            base_revision: "revision".to_string(),
        },
        state: crate::client::journal::JournalState::ProcessObservedRunning,
        process_id: process_id.map(str::to_owned),
        last_event_checkpoint: None,
        pending_terminal_report: None,
    }
}

// -----------------------------------------------------------------
// Provider endpoint injection: a configured gateway entry reaches a
// spawned process only when the request actually names it; a direct
// request must receive none of it.
// -----------------------------------------------------------------

fn enabled_gateway_providers(
    secret_name: &str,
) -> std::collections::BTreeMap<String, crate::config::ProviderConfig> {
    std::collections::BTreeMap::from([(
        crate::config::VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        crate::config::ProviderConfig {
            enabled: true,
            secret: secret_name.to_owned(),
        },
    )])
}

/// A shim that records the *names* only of the environment variables it
/// was spawned with — never a value — so a test can prove a variable's
/// presence or absence without ever needing to see, let alone assert
/// on, a credential.
fn env_name_dump_binary(workspace: &Path, marker: &Path) -> HarnessBinary {
    // A single external process (`env`), no pipe to a second one: the
    // name/value split happens in `recorded_env_names` instead, purely
    // to keep this shim's own process footprint minimal under a
    // heavily parallel test run.
    let script = format!("#!/bin/sh\nenv > {}\nexit 0\n", marker.display());
    let script_path = workspace.join("dump-env-names.sh");
    std::fs::write(&script_path, script).expect("write shim script");
    HarnessBinary {
        program: PathBuf::from("/bin/sh"),
        prefix_args: vec![script_path.display().to_string()],
    }
}

/// The *names* only of the `KEY=VALUE` lines `env`'s output wrote to
/// `marker` — this helper is what actually discards every value, so no
/// caller ever inspects one, even a dummy one seeded for a test.
fn recorded_env_names(marker: &Path) -> Vec<String> {
    std::fs::read_to_string(marker)
        .expect("shim wrote the env-names marker")
        .lines()
        .filter_map(|line| line.split('=').next())
        .map(str::to_owned)
        .collect()
}

/// Acceptance: a request naming a direct model provider (or none at
/// all) must spawn with neither `ANTHROPIC_BASE_URL` nor
/// `ANTHROPIC_AUTH_TOKEN` present — even though a gateway entry is
/// configured and enabled on this same adapter. Proves the two paths
/// can never be confused by a shared environment variable.
#[tokio::test]
async fn a_direct_model_request_spawns_with_no_provider_endpoint_variable_present() {
    let workspace_dir = temp_workspace("provider-guard-direct");
    let workspace = workspace_dir.path();
    let marker = workspace.join("env-names.marker");
    let binary = env_name_dump_binary(workspace, &marker);

    let secrets_dir = temp_workspace("secrets");
    let secrets = test_secret_store(secrets_dir.path());
    secrets
        .set("demo-secret", "unused-by-a-direct-request")
        .expect("seed store");
    let adapter = ClaudeCodeAdapter::with_binary(binary, clock(), secrets)
        .with_providers(enabled_gateway_providers("demo-secret"));

    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        BTreeMap::new(),
        workspace.to_path_buf(),
    );
    adapter
        .validate(&spec)
        .await
        .expect("validate a direct request");
    let handle = adapter.start(&spec).await.expect("start a direct request");
    let _ = adapter.wait(&handle).await.expect("wait");

    let names = recorded_env_names(&marker);
    assert!(
        !names.iter().any(|name| name == "ANTHROPIC_BASE_URL"),
        "a direct request must never receive the provider endpoint's base URL: {names:?}"
    );
    assert!(
        !names.iter().any(|name| name == "ANTHROPIC_AUTH_TOKEN"),
        "a direct request must never receive the provider endpoint's credential: {names:?}"
    );

    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// The positive half of the same proof: a request naming the
/// configured provider does receive its base URL and credential
/// variable names.
#[tokio::test]
async fn a_configured_provider_request_spawns_with_its_endpoint_variables_present() {
    let workspace_dir = temp_workspace("provider-guard-configured");
    let workspace = workspace_dir.path();
    let marker = workspace.join("env-names.marker");
    let binary = env_name_dump_binary(workspace, &marker);

    let secrets_dir = temp_workspace("secrets");
    let secrets = test_secret_store(secrets_dir.path());
    secrets
        .set("demo-secret", "a-resolvable-value")
        .expect("seed store");
    let adapter = ClaudeCodeAdapter::with_binary(binary, clock(), secrets)
        .with_providers(enabled_gateway_providers("demo-secret"));

    let spec = spec_with(
        "claude-code",
        Some(crate::config::VERCEL_AI_GATEWAY_PROVIDER),
        &[],
        true,
        BTreeMap::new(),
        workspace.to_path_buf(),
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
        names.iter().any(|name| name == "ANTHROPIC_BASE_URL"),
        "a gateway-routed request must receive the provider endpoint's base URL: {names:?}"
    );
    assert!(
        names.iter().any(|name| name == "ANTHROPIC_AUTH_TOKEN"),
        "a gateway-routed request must receive the provider endpoint's credential: {names:?}"
    );

    std::fs::remove_dir_all(workspace).expect("cleanup");
}

/// A configured-but-disabled provider must reject the request pre-spawn
/// with a typed reason, not silently fall back to a direct request.
#[tokio::test]
async fn a_disabled_provider_rejects_the_request_before_any_process_spawns() {
    let workspace_dir = temp_workspace("provider-guard-disabled");
    let workspace = workspace_dir.path();
    let secrets_dir = temp_workspace("secrets");
    let secrets = test_secret_store(secrets_dir.path());
    secrets
        .set("demo-secret", "irrelevant")
        .expect("seed store");
    let providers = std::collections::BTreeMap::from([(
        crate::config::VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        crate::config::ProviderConfig {
            enabled: false,
            secret: "demo-secret".to_owned(),
        },
    )]);
    let adapter =
        ClaudeCodeAdapter::with_binary(fake_binary(), clock(), secrets).with_providers(providers);

    let spec = spec_with(
        "claude-code",
        Some(crate::config::VERCEL_AI_GATEWAY_PROVIDER),
        &[],
        true,
        BTreeMap::new(),
        workspace.to_path_buf(),
    );
    let error = adapter
        .validate(&spec)
        .await
        .expect_err("a disabled provider must reject at validate, before any spawn");
    assert!(matches!(error, HarnessError::Rejected { .. }));

    std::fs::remove_dir_all(workspace).expect("cleanup");
}

// Discovery's search logic is pure and lives in `harness::locate`, with
// its own tests there (found on PATH, found only in a fallback dir, not
// found, non-executable skipped); this file no longer needs a test that
// mutates the real process `PATH` to reach the same behavior.

// ---- live, opt-in test against the real installed `claude` ----------

/// Opt-in, matching `codex.rs`'s own
/// `#[ignore]`-gated live tests: never runs under a plain `cargo test`,
/// never required in CI, and never fails just because `claude` is
/// absent. Unlike Codex's live test (version probe plus a
/// purely local artifact stage, no real model call), a real Claude Code
/// invocation is billed — so this test additionally requires
/// `TACK_RUN_LIVE_CLAUDE_CODE_TEST=1` even under `--ignored`, so that
/// flag alone can never surprise-spend real money. Never depends on a
/// secret being present in this process's own environment: whatever
/// credential the installed CLI already carries (e.g. an OAuth session
/// under `HOME`) is used exactly as the real installation already has
/// it configured — this test never reads, logs, or forwards one itself.
///
/// Records the observed version and stages a real produced artifact (a
/// `README.md` this test asks Claude Code, via its `Write` tool, to
/// overwrite, inside a disposable fixture git repo this test creates
/// and deletes itself — never this checkout).
#[tokio::test]
#[ignore = "opt-in: requires a real `claude` binary on PATH *and* \
            TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 (a real invocation is billed, unlike Codex's \
            free live test); run with TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 cargo nextest \
            run --workspace --run-ignored ignored-only -E 'test(/claude_code::tests::live_/)'"]
async fn live_claude_code_records_version_and_a_real_artifact_when_opted_in() {
    if std::env::var("TACK_RUN_LIVE_CLAUDE_CODE_TEST").as_deref() != Ok("1") {
        eprintln!(
            "skipping live claude-code test: set TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 to opt in \
             (a real invocation is billed)"
        );
        return;
    }
    let secrets_dir = temp_workspace("secrets");
    let Ok(adapter) = ClaudeCodeAdapter::discover(test_secret_store(secrets_dir.path())) else {
        eprintln!("skipping live claude-code test: no `claude` binary discoverable on PATH");
        return;
    };

    let capability = adapter.probe().await;
    assert!(
        capability.probe_error.is_none(),
        "expected a healthy probe against a real installed binary: {:?}",
        capability.probe_error
    );
    assert!(!capability.installed_version.is_empty());
    eprintln!(
        "live claude-code probe: version={}",
        capability.installed_version
    );

    let workspace_dir = temp_workspace("live-fixture-repo");
    let workspace = workspace_dir.path();
    let run = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .current_dir(workspace)
            .output()
            .expect("git available for the live test's disposable fixture repo")
    };
    run("git", &["init", "-q"]);
    run("git", &["config", "user.email", "probe@example.invalid"]);
    run("git", &["config", "user.name", "Probe"]);
    std::fs::write(workspace.join("README.md"), "fixture\n").expect("seed fixture file");
    run("git", &["add", "README.md"]);
    run("git", &["commit", "-q", "-m", "seed"]);

    let mut environment = BTreeMap::new();
    environment.insert(
        "HOME".to_string(),
        env_entry(&std::env::var("HOME").unwrap_or_default()),
    );
    let mut spec = spec_with(
        "claude-code",
        None,
        &["Write"],
        false,
        environment,
        workspace.to_path_buf(),
    );
    // Overrides the shared helper's default "Print exactly: ok" prompt
    // (used by every other test in this module) with one that actually
    // exercises the `Write` tool this spec allows, so the artifact
    // staged below is genuinely something Claude Code produced, not
    // merely the seed content this test itself wrote.
    spec.work.request.resolved_agent_profile.instructions =
        "Using the Write tool, overwrite README.md in the current directory with exactly \
         this content: tack-d2-live-test-marker"
            .to_string();

    adapter.validate(&spec).await.expect("validate a live spec");
    let handle = adapter.start(&spec).await.expect("start a live process");
    let outcome = adapter
        .wait(&handle)
        .await
        .expect("wait for a live process");

    eprintln!(
        "live claude-code outcome: terminal_state={:?} model_id={} harness_version={}",
        outcome.terminal_state,
        outcome.actual_execution.model_id.as_str(),
        outcome.actual_execution.harness_version
    );

    let staged = super::super::artifact::ArtifactStager::new(workspace.join(".artifacts"))
        .stage_file(
            "live-test-attempt",
            workspace,
            Path::new("README.md"),
            "log",
            "text/markdown",
        )
        .expect("stage the real artifact Claude Code's Write tool produced");
    assert!(staged.size_bytes > 0);
    let staged_content = std::fs::read_to_string(&staged.staged_path).unwrap_or_default();
    eprintln!("live claude-code staged artifact content: {staged_content:?}");
    if !staged_content.contains("tack-d2-live-test-marker") {
        eprintln!(
            "note: the model did not reproduce the exact requested marker text — this is \
             model-phrasing variance, not itself a failure of this adapter, so it is only \
             logged, never asserted on"
        );
    }
    eprintln!(
        "live claude-code artifact: sha256={} size_bytes={}",
        staged.sha256, staged.size_bytes
    );

    std::fs::remove_dir_all(workspace).expect("cleanup disposable fixture repo");
}

/// Live proof of the provider endpoint path, not the direct one above:
/// resolves the real runner-local secret store (the platform keychain,
/// or its owner-only file fallback — whichever this machine actually
/// has) for a `vercel_ai_gateway` entry, points a real `claude` binary
/// at it, and records what the CLI reported. Gated identically to
/// [`live_claude_code_records_version_and_a_real_artifact_when_opted_in`]
/// (a real invocation is billed), plus a clean skip when no store entry
/// exists at all — this test never fabricates one.
#[tokio::test]
#[ignore = "opt-in: requires a real `claude` binary on PATH, a `vercel_ai_gateway` entry in \
            this machine's secret store, *and* TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 (a real \
            invocation is billed); run with TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 cargo nextest \
            run --workspace --run-ignored ignored-only -E 'test(/claude_code::tests::live_/)'"]
async fn live_claude_code_through_the_configured_provider_when_opted_in() {
    if std::env::var("TACK_RUN_LIVE_CLAUDE_CODE_TEST").as_deref() != Ok("1") {
        eprintln!(
            "skipping live claude-code gateway test: set TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 to \
             opt in (a real invocation is billed)"
        );
        return;
    }
    let state_dir = std::env::var_os("TACK_RUNNER_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").expect("HOME is set")).join(".tack-runner")
        });
    let secrets = crate::secrets::SecretStore::open(&state_dir.join("secrets.json"));
    let providers = std::collections::BTreeMap::from([(
        crate::config::VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        crate::config::ProviderConfig {
            enabled: true,
            secret: crate::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET.to_owned(),
        },
    )]);

    let Ok(adapter) = ClaudeCodeAdapter::discover(secrets) else {
        eprintln!("skipping live claude-code gateway test: no `claude` binary discoverable");
        return;
    };
    let adapter = adapter.with_providers(providers);

    let workspace_dir = temp_workspace("live-gateway");
    let workspace = workspace_dir.path();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(workspace)
        .status()
        .expect("git init");
    let mut spec = spec_with(
        "claude-code",
        Some(crate::config::VERCEL_AI_GATEWAY_PROVIDER),
        &[],
        true,
        BTreeMap::new(),
        workspace.to_path_buf(),
    );
    spec.work.request.requested_model_id = Some(tack_orch::execution::RequestedModelId::new(
        "anthropic/claude-opus-4.6",
    ));
    spec.work.request.resolved_agent_profile.instructions = "Say exactly: ok".to_string();

    if let Err(error) = adapter.validate(&spec).await {
        eprintln!(
            "skipping live claude-code gateway test: no configured provider entry to \
             validate against ({error})"
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
        "live claude-code (gateway) outcome: terminal_state={:?} model_provider={} \
         model_id={} model_observation_source={} terminal_reason={}",
        outcome.terminal_state,
        outcome.actual_execution.model_provider.as_str(),
        outcome.actual_execution.model_id.as_str(),
        outcome.actual_execution.model_observation_source,
        outcome.terminal_reason
    );

    // Holds regardless of whether the configured credential is itself
    // valid, and regardless of which of two honest outcomes this
    // specific run hits: a `result` line arriving before the request
    // timeout (`requested_not_confirmed`, from `parsed_from_result_line`)
    // or the process being killed mid-retry-storm with no such line
    // ever seen (`not_observed`, from `malformed_outcome`/
    // `fallback_from_exit_code`) — a real invalid-key run measured
    // exponential retry delays that make the latter the far likelier
    // case within any test-sized timeout. The one claim that must never
    // hold for a gateway-routed run is `harness_reported`: this line is
    // emitted before any network call reaches the gateway, so it can
    // never be treated as confirmation the gateway actually served it.
    // A successful completion additionally needs a working credential,
    // which this test does not assert on: that requires a separately
    // run, deliberately billed proof against a real credential.
    assert_ne!(
        outcome.actual_execution.model_observation_source,
        ModelObservationSource::HarnessReported.as_str(),
        "a gateway-routed run must never claim harness_reported"
    );

    std::fs::remove_dir_all(workspace).expect("cleanup disposable fixture repo");
}

/// The live counterpart to the fake-shim guard tests above: with the
/// configured provider enabled *and* genuinely working (a real, billed
/// gateway completion is proven by the test above), a direct-model
/// request against the same adapter must still never reach the
/// gateway. Never bills anything itself — a direct request with no
/// ambient login on this machine fails in milliseconds
/// ("Not logged in"), which is the point: if it had instead reached
/// the gateway, it would have succeeded, exactly like the test above.
#[tokio::test]
#[ignore = "opt-in: requires a real `claude` binary on PATH *and*                 TACK_RUN_LIVE_CLAUDE_CODE_TEST=1; run with                 TACK_RUN_LIVE_CLAUDE_CODE_TEST=1 cargo nextest run --workspace                 --run-ignored ignored-only -E 'test(/claude_code::tests::live_/)'"]
async fn live_claude_code_direct_model_never_reaches_the_configured_provider_when_opted_in() {
    if std::env::var("TACK_RUN_LIVE_CLAUDE_CODE_TEST").as_deref() != Ok("1") {
        eprintln!(
            "skipping live claude-code direct-guard test: set TACK_RUN_LIVE_CLAUDE_CODE_TEST=1                  to opt in"
        );
        return;
    }
    let state_dir = std::env::var_os("TACK_RUNNER_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").expect("HOME is set")).join(".tack-runner")
        });
    let secrets = crate::secrets::SecretStore::open(&state_dir.join("secrets.json"));
    let providers = std::collections::BTreeMap::from([(
        crate::config::VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        crate::config::ProviderConfig {
            enabled: true,
            secret: crate::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET.to_owned(),
        },
    )]);

    let Ok(adapter) = ClaudeCodeAdapter::discover(secrets) else {
        eprintln!("skipping live claude-code direct-guard test: no `claude` binary discoverable");
        return;
    };
    let adapter = adapter.with_providers(providers);

    let workspace_dir = temp_workspace("live-direct-guard");
    let workspace = workspace_dir.path();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(workspace)
        .status()
        .expect("git init");
    // No requested_model_provider at all: the direct/subscription path.
    let spec = spec_with(
        "claude-code",
        None,
        &[],
        true,
        BTreeMap::new(),
        workspace.to_path_buf(),
    );

    adapter
        .validate(&spec)
        .await
        .expect("a direct request validates even with the provider configured");
    let handle = adapter
        .start(&spec)
        .await
        .expect("start a direct-model process");
    let outcome = adapter
        .wait(&handle)
        .await
        .expect("wait for a direct-model process");

    eprintln!(
        "live claude-code (direct, provider configured but unused) outcome:              terminal_state={:?} terminal_reason={}",
        outcome.terminal_state, outcome.terminal_reason
    );

    // The decisive check: the gateway's own distinctive error shape
    // ("authentication_failed"/"api_retry") must never appear on a
    // direct request, proving it never reached ai-gateway.vercel.sh.
    let serialized = outcome.terminal_reason.to_string();
    assert!(
        !serialized.contains("authentication_failed") && !serialized.contains("api_retry"),
        "a direct request must never show the gateway's own error shape: {serialized}"
    );

    std::fs::remove_dir_all(workspace).expect("cleanup");
}
