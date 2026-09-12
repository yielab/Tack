use super::*;
use crate::client::journal::{JournalState, WorkspaceJournal};
use crate::client::{AttemptId, FencingToken, RunnerId, WorkspaceId, engine::HarnessError};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tack_orch::execution::{CapabilityValue, RequestedModelId, RequestedModelProvider};

/// A trivial trait-level fake — deliberately not the fake *binary*: this
/// module tests dispatch/routing logic, which does not need a real
/// subprocess.
struct TaggedFakeAdapter {
    tag: &'static str,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl HarnessAdapter for TaggedFakeAdapter {
    async fn validate(&self, _spec: &ExecutionSpec) -> Result<(), HarnessError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn start(&self, _spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(LocalRunHandle {
            process_id: format!("{}-process", self.tag),
        })
    }

    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(handle.process_id, format!("{}-process", self.tag));
        Ok(CancellationEvidence {
            observation: CancelObservation::ProcessStopped,
            observed_at: crate::client::Timestamp::new("2026-08-06T12:24:00Z"),
            details: serde_json::Map::from_iter([("tag".into(), serde_json::json!(self.tag))]),
        })
    }

    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(handle.process_id, format!("{}-process", self.tag));
        Ok(HarnessOutcome {
            terminal_state: crate::client::AttemptState::Succeeded,
            terminal_reason: serde_json::json!({"tag": self.tag}),
            final_checkpoint: None,
            actual_execution: actual_execution(self.tag),
            usage: usage(),
        })
    }

    async fn reconcile(
        &self,
        journal: &AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(
            journal.process_id.as_deref(),
            Some(format!("{}-process", self.tag)).as_deref()
        );
        Ok(RecoveryObservation::ProcessStopped)
    }
}

fn actual_execution(tag: &str) -> tack_orch::execution::ActualExecution {
    serde_json::from_value(serde_json::json!({
        "harness_kind": tag,
        "harness_version": "1.0.0",
        "model_provider": "test-provider",
        "model_id": "test-model",
        "model_observation_source": "harness_reported",
        "capability_snapshot": {
            "cancel": {"support": "supported", "reason": null},
            "resume": {"support": "unsupported", "reason": "no resume"},
            "decisions": {"support": "supported", "reason": null},
            "artifacts": {"support": "supported", "reason": null},
            "usage": {"support": "advisory", "reason": "partial"}
        },
        "workspace_id": "ws_test",
        "base_revision": "revision",
        "started_at": "2026-08-06T12:20:00Z",
        "ended_at": "2026-08-06T12:25:00Z"
    }))
    .expect("actual execution fixture")
}

fn usage() -> tack_orch::execution::Usage {
    serde_json::from_value(serde_json::json!({
        "tokens_in": {"value": 1, "source": "measured"},
        "tokens_out": {"value": 2, "source": "measured"},
        "duration_ms": {"value": 3, "source": "measured"},
        "cost_usd": {"value": null, "source": "not_measured"}
    }))
    .expect("usage fixture")
}

fn spec_requesting(kind: &str) -> ExecutionSpec {
    let claim: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture");
    let mut request: tack_orch::execution::ExecutionRequestSnapshot =
        serde_json::from_value(claim["request"].clone()).expect("request fixture");
    request.requested_harness_kind = DomainHarnessKind::new(kind);
    let attempt: tack_orch::execution::AttemptSnapshot =
        serde_json::from_value(claim["attempt"].clone()).expect("attempt fixture");
    ExecutionSpec {
        work: crate::client::ClaimedWork {
            claim_request_id: crate::client::ClaimRequestId::new("claim"),
            lease: crate::client::AttemptLease {
                attempt_id: AttemptId::new("attempt"),
                runner_id: RunnerId::new("runner"),
                fencing_token: FencingToken(1),
                attempt_number: 1,
                state: crate::client::AttemptState::Leased,
                issued_at: crate::client::Timestamp::new("2026-08-06T12:20:00Z"),
                expires_at: crate::client::Timestamp::new("2026-08-06T12:21:00Z"),
            },
            request,
            attempt,
        },
        workspace: crate::client::Workspace {
            attempt_id: AttemptId::new("attempt"),
            id: WorkspaceId::new("ws_test"),
            path: PathBuf::from("/tmp/does-not-matter"),
            base_revision: "revision".into(),
        },
    }
}

fn journal_with_process(process_id: Option<&str>) -> AttemptJournal {
    AttemptJournal {
        attempt_id: AttemptId::new("attempt"),
        runner_id: RunnerId::new("runner"),
        fencing_token: FencingToken(1),
        workspace: WorkspaceJournal {
            workspace_id: WorkspaceId::new("ws_test"),
            path: PathBuf::from("/tmp/does-not-matter"),
            base_revision: "revision".into(),
        },
        state: JournalState::ProcessObservedRunning,
        process_id: process_id.map(str::to_owned),
        last_event_checkpoint: None,
        pending_terminal_report: None,
    }
}

fn registry_with_two_kinds() -> (AdapterRegistry, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let codex_calls = Arc::new(AtomicUsize::new(0));
    let claude_calls = Arc::new(AtomicUsize::new(0));
    let mut registry = AdapterRegistry::new();
    registry.register_adapter(
        DomainHarnessKind::new("codex"),
        Box::new(TaggedFakeAdapter {
            tag: "codex",
            calls: Arc::clone(&codex_calls),
        }),
    );
    registry.register_adapter(
        DomainHarnessKind::new("claude-code"),
        Box::new(TaggedFakeAdapter {
            tag: "claude-code",
            calls: Arc::clone(&claude_calls),
        }),
    );
    (registry, codex_calls, claude_calls)
}

#[tokio::test]
async fn validate_and_start_dispatch_to_the_requested_kind_only() {
    let (registry, codex_calls, claude_calls) = registry_with_two_kinds();

    registry
        .validate(&spec_requesting("codex"))
        .await
        .expect("codex validate");
    assert_eq!(codex_calls.load(Ordering::SeqCst), 1);
    assert_eq!(claude_calls.load(Ordering::SeqCst), 0);

    let handle = registry
        .start(&spec_requesting("claude-code"))
        .await
        .expect("claude-code start");
    assert_eq!(codex_calls.load(Ordering::SeqCst), 1);
    assert_eq!(claude_calls.load(Ordering::SeqCst), 1);
    // The handle the engine sees is opaque; only this module's own
    // routing depends on its internal shape.
    assert!(handle.process_id.contains("claude-code-process"));
}

/// Acceptance-adjacent: proves the encode/decode workaround actually
/// routes a handle back to the *same* adapter that produced it, and
/// never to the other registered kind — the concrete failure mode the
/// missing `LocalRunHandle.harness_kind` field (documented above) would
/// otherwise risk.
#[tokio::test]
async fn cancel_and_wait_route_the_start_generated_handle_back_to_its_own_adapter() {
    let (registry, codex_calls, claude_calls) = registry_with_two_kinds();

    let handle = registry
        .start(&spec_requesting("claude-code"))
        .await
        .expect("start");
    codex_calls.store(0, Ordering::SeqCst);
    claude_calls.store(0, Ordering::SeqCst);

    registry.cancel(&handle).await.expect("cancel");
    assert_eq!(claude_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        codex_calls.load(Ordering::SeqCst),
        0,
        "must never reach the other kind"
    );

    let codex_handle = registry
        .start(&spec_requesting("codex"))
        .await
        .expect("start codex");
    codex_calls.store(0, Ordering::SeqCst);
    claude_calls.store(0, Ordering::SeqCst);
    registry.wait(&codex_handle).await.expect("wait");
    assert_eq!(codex_calls.load(Ordering::SeqCst), 1);
    assert_eq!(claude_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn an_unregistered_kind_fails_pre_spawn_with_a_typed_rejection() {
    let (registry, _, _) = registry_with_two_kinds();

    assert!(matches!(
        registry
            .validate(&spec_requesting("unregistered-harness"))
            .await,
        Err(HarnessError::Rejected { .. })
    ));
    assert!(matches!(
        registry
            .start(&spec_requesting("unregistered-harness"))
            .await,
        Err(HarnessError::Rejected { .. })
    ));
}

#[tokio::test]
async fn reconcile_with_no_recorded_process_id_needs_no_dispatch() {
    let registry = AdapterRegistry::new(); // no adapters registered at all
    let observation = registry
        .reconcile(&journal_with_process(None))
        .await
        .expect("reconcile with no process id");
    assert_eq!(observation, RecoveryObservation::ProcessStopped);
}

#[tokio::test]
async fn reconcile_decodes_the_kind_and_routes_to_the_right_adapter() {
    let (registry, codex_calls, claude_calls) = registry_with_two_kinds();
    let handle = registry
        .start(&spec_requesting("codex"))
        .await
        .expect("start");
    codex_calls.store(0, Ordering::SeqCst);

    let mut journal = journal_with_process(Some(&handle.process_id));
    journal.process_id = Some(handle.process_id);
    let observation = registry.reconcile(&journal).await.expect("reconcile");
    assert_eq!(observation, RecoveryObservation::ProcessStopped);
    assert_eq!(codex_calls.load(Ordering::SeqCst), 1);
    assert_eq!(claude_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn reconcile_with_an_undecodable_process_id_is_explicitly_unavailable_not_ambiguous_success()
{
    let (registry, _, _) = registry_with_two_kinds();
    let journal = journal_with_process(Some("not-an-encoded-handle-at-all"));

    assert!(matches!(
        registry.reconcile(&journal).await,
        Err(HarnessError::RecoveryUnavailable)
    ));
}

#[tokio::test]
async fn registered_kinds_and_capabilities_are_in_deterministic_sorted_order() {
    let (registry, _, _) = registry_with_two_kinds();
    assert_eq!(registry.registered_kinds(), vec!["claude-code", "codex"]);
}

struct FakeProbe {
    kind: &'static str,
    installed: bool,
    /// Configurable so the same fake can play both an
    /// honest probe (the default every pre-existing test below uses) and
    /// a deliberately lying one, for
    /// `registering_a_probe_that_overclaims_cancel_support_is_rejected_before_any_attempt_exists`.
    cancel_support: CapabilitySupport,
}

impl FakeProbe {
    fn honest(kind: &'static str, installed: bool) -> Self {
        Self {
            kind,
            installed,
            cancel_support: CapabilitySupport::Advisory,
        }
    }
}

#[async_trait]
impl HarnessProbe for FakeProbe {
    fn harness_kind(&self) -> DomainHarnessKind {
        DomainHarnessKind::new(self.kind)
    }

    async fn probe(&self) -> HarnessCapability {
        HarnessCapability {
            harness_kind: DomainHarnessKind::new(self.kind),
            installed_version: if self.installed {
                "1.2.3".into()
            } else {
                String::new()
            },
            probe_error: if self.installed {
                None
            } else {
                Some("not found on PATH".into())
            },
            probed_at: chrono::DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
                .expect("fixture timestamp")
                .into(),
            model_combinations: Vec::new(),
            // Deliberately un-attested: the fake probe stands in
            // for the runner so scheduler tests can cover the
            // "no attestation" path; the real adapters each attest
            // explicitly.
            model_passthrough: None,
            additional: Default::default(),
        }
    }

    fn declared_capabilities(&self) -> FeatureCapabilities {
        fn unsupported(reason: &str) -> CapabilityValue {
            CapabilityValue {
                support: CapabilitySupport::Unsupported,
                reason: Some(reason.to_owned()),
                additional: Default::default(),
            }
        }
        FeatureCapabilities {
            cancel: CapabilityValue {
                support: self.cancel_support,
                reason: Some("fake probe for registry-dispatch tests".to_owned()),
                additional: Default::default(),
            },
            resume: unsupported("fake"),
            decisions: unsupported("fake"),
            artifacts: unsupported("fake"),
            usage: unsupported("fake"),
            additional: Default::default(),
        }
    }
}

#[tokio::test]
async fn capabilities_reports_an_honest_probe_error_for_an_uninstalled_harness_never_a_fake_success()
 {
    let mut registry = AdapterRegistry::new();
    registry
        .register_probe(Box::new(FakeProbe::honest("codex", true)))
        .expect("an honest probe registers cleanly");
    registry
        .register_probe(Box::new(FakeProbe::honest("claude-code", false)))
        .expect("an honest probe registers cleanly");

    let reports = registry.capabilities().await;
    assert_eq!(reports.len(), 2);
    let codex = reports
        .iter()
        .find(|report| report.harness_kind.as_str() == "codex")
        .expect("codex report");
    assert_eq!(codex.probe_error, None);
    let claude = reports
        .iter()
        .find(|report| report.harness_kind.as_str() == "claude-code")
        .expect("claude-code report");
    assert_eq!(claude.probe_error.as_deref(), Some("not found on PATH"));
}

/// Acceptance gate: "a lying capability is caught before invocation."
/// The shared
/// cancellation primitive (`harness::process::SupervisedProcess::cancel`,
/// a process-group SIGTERM/SIGKILL) cannot reliably reach a descendant a
/// harness's own shell-tool spawns into a new OS session — checking
/// with `ps` against real Claude Code found this. A probe that
/// nonetheless claims
/// `cancel: Supported` is rejected here, at registration — never
/// silently accepted only to be discovered wrong the first time a real
/// cancellation against a live attempt fails to reach a detached
/// descendant.
#[tokio::test]
async fn registering_a_probe_that_overclaims_cancel_support_is_rejected_before_any_attempt_exists()
{
    let mut registry = AdapterRegistry::new();
    let lying = FakeProbe {
        kind: "lying-harness",
        installed: true,
        cancel_support: CapabilitySupport::Supported,
    };

    let error = match registry.register_probe(Box::new(lying)) {
        Ok(_) => panic!("a probe claiming Supported cancellation must be rejected"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        HarnessRegistrationError::OverclaimedCancelSupport {
            support: CapabilitySupport::Supported,
            ceiling: CapabilitySupport::Advisory,
            ..
        }
    ));

    // Never inserted: dispatch/capability reporting never sees it.
    assert!(registry.capabilities().await.is_empty());
}

#[test]
fn handle_encoding_round_trips_kinds_and_process_ids_containing_colons() {
    let encoded = encode_handle("open:code", "pid:123:extra");
    let (kind, inner) = decode_handle(&encoded).expect("decode");
    assert_eq!(kind, "open:code");
    assert_eq!(inner, "pid:123:extra");
}

#[test]
fn decode_handle_rejects_input_with_no_recognizable_encoding() {
    assert_eq!(decode_handle("no-colon-here"), None);
    assert_eq!(
        decode_handle("zz:inner"),
        None,
        "non-hex prefix is rejected"
    );
}

// ---- The real, reconciled adapters -------------------------------------
//
// Everything above this point tests dispatch/routing with trait-level
// fakes. These last tests are the two
// acceptance-gate proofs that need the
// *real* adapters (`codex::CodexAdapter`, `claude_code::ClaudeCodeAdapter`),
// not stand-ins: "the same fixture
// completes through both fake adapters" and "registration of both
// is order-independent." Each real adapter's own file already has
// its own exhaustive fixture-driven test suite;
// these two tests are deliberately narrow, cross-cutting proofs that
// only make sense here, where both are in scope together.

/// A scratch directory that removes itself, and everything written under
/// it, when the returned guard drops — including when an assertion panics
/// first.
fn cross_adapter_temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

/// A fresh, hermetic file-backed store per call — never the platform
/// keychain — so parallel `#[test]` functions never see each other's
/// entries and CI needs no Secret Service.
/// Takes the directory rather than making one, so the store's file cannot
/// outlive the guard that removes it. Each call gets its own file inside
/// that directory: two adapters in one test must not share a store.
fn cross_adapter_secret_store(dir: &std::path::Path) -> SecretStore {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    SecretStore::file(dir.join(format!("secrets-{n}.json")))
}

/// One deterministic fixture script, driven identically by both
/// real adapters. Never the shared `fake_harness_command()` — that
/// fixture is env-var-driven and single-purpose per spawn, which cannot
/// honestly answer a harness's own version/model-listing/run calls
/// (multiple purposes) in one adapter instance without also faking a
/// probe-only environment override this cross-cutting test has no
/// business reaching into. Branches on its own argv instead:
/// a literal `--version` token prints a clean version string; a literal
/// `models` token prints one deterministic `provider/model` line; anything
/// else (each adapter's real `run`/`exec`/`-p` invocation) prints a
/// fixed, non-JSON marker and exits 0 — which both
/// adapters' own `wait()` honestly classifies as `Succeeded` from the
/// exit code alone (Codex always does; Claude Code falls
/// back to exit-code classification when stdout does not parse as its
/// own structured output).
/// Returns the guard alongside the command: the script lives inside it,
/// so dropping it before the adapters run would delete the program they
/// are about to spawn.
fn cross_adapter_fixture_command() -> (PathBuf, Vec<String>, tempfile::TempDir) {
    let dir = cross_adapter_temp_dir("script");
    let script_path = dir.path().join("fixture.sh");
    let script = r#"#!/bin/sh
for arg in "$@"; do
  case "$arg" in
--version) echo "1.0.0"; exit 0 ;;
models) printf 'demo/model-a\n'; exit 0 ;;
  esac
done
echo "cross-adapter-fixture-complete"
exit 0
"#;
    std::fs::write(&script_path, script).expect("write cross-adapter fixture script");
    (
        PathBuf::from("/bin/sh"),
        vec![script_path.display().to_string()],
        dir,
    )
}

/// Builds a real `ExecutionSpec` for `kind`, reusing `claim.response.json`
/// exactly as `spec_requesting` above does, but with a real (existing)
/// workspace directory — the real adapters actually spawn a process
/// there, unlike the trait-level fakes above — and an explicit
/// provider/model pair valid for that specific adapter.
fn real_adapter_spec(
    kind: &str,
    provider: &str,
    model: &str,
    workspace_path: PathBuf,
) -> ExecutionSpec {
    let claim: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../docs/contracts/runner-v1/claim.response.json"
    ))
    .expect("claim fixture");
    let mut request: tack_orch::execution::ExecutionRequestSnapshot =
        serde_json::from_value(claim["request"].clone()).expect("request fixture");
    request.requested_harness_kind = DomainHarnessKind::new(kind);
    request.requested_model_provider = Some(RequestedModelProvider::new(provider));
    request.requested_model_id = Some(RequestedModelId::new(model));
    let attempt: tack_orch::execution::AttemptSnapshot =
        serde_json::from_value(claim["attempt"].clone()).expect("attempt fixture");
    ExecutionSpec {
        work: crate::client::ClaimedWork {
            claim_request_id: crate::client::ClaimRequestId::new("claim"),
            lease: crate::client::AttemptLease {
                attempt_id: AttemptId::new("attempt"),
                runner_id: RunnerId::new("runner"),
                fencing_token: FencingToken(1),
                attempt_number: 1,
                state: crate::client::AttemptState::Leased,
                issued_at: crate::client::Timestamp::new("2026-08-09T12:20:00Z"),
                expires_at: crate::client::Timestamp::new("2026-08-09T13:20:00Z"),
            },
            request,
            attempt,
        },
        workspace: crate::client::Workspace {
            attempt_id: AttemptId::new("attempt"),
            id: WorkspaceId::new("ws_cross_adapter"),
            path: workspace_path,
            base_revision: "revision".into(),
        },
    }
}

/// Acceptance: "the same fixture completes through both fake
/// adapters — one deterministic fixture, two adapters, same
/// observable outcome." Drives the real `CodexAdapter` and `ClaudeCodeAdapter`
/// — through the frozen `HarnessAdapter` trait only,
/// exactly as `AdapterRegistry` would dispatch to them — against the one
/// fixture script above, and asserts both reach
/// `AttemptState::Succeeded` from the identical input.
#[tokio::test]
async fn the_same_fixture_completes_through_both_real_adapters() {
    let secrets_dir = cross_adapter_temp_dir("secrets");
    let secrets = secrets_dir.path();
    let (program, args, _script_dir) = cross_adapter_fixture_command();
    let staging_root_dir = cross_adapter_temp_dir("artifacts");
    let staging_root = staging_root_dir.path();

    let codex_workspace_dir = cross_adapter_temp_dir("codex-ws");
    let codex_workspace = codex_workspace_dir.path();
    let codex = crate::harness::codex::CodexAdapter::for_fixture(
        program.clone(),
        args.clone(),
        staging_root.to_path_buf(),
        cross_adapter_secret_store(secrets),
    );
    let codex_spec = real_adapter_spec(
        "codex",
        "openai",
        "opaque/model-alpha",
        codex_workspace.to_path_buf(),
    );
    codex.validate(&codex_spec).await.expect("codex validate");
    let codex_handle = codex.start(&codex_spec).await.expect("codex start");
    let codex_outcome = codex.wait(&codex_handle).await.expect("codex wait");
    assert_eq!(
        codex_outcome.terminal_state,
        crate::client::AttemptState::Succeeded
    );

    let claude_workspace_dir = cross_adapter_temp_dir("claude-ws");
    let claude_workspace = claude_workspace_dir.path();
    let claude = crate::harness::claude_code::ClaudeCodeAdapter::for_fixture(
        program.clone(),
        args.clone(),
        cross_adapter_secret_store(secrets),
    );
    let claude_spec = real_adapter_spec(
        "claude-code",
        "anthropic",
        "claude-fixture-model",
        claude_workspace.to_path_buf(),
    );
    claude
        .validate(&claude_spec)
        .await
        .expect("claude-code validate");
    let claude_handle = claude.start(&claude_spec).await.expect("claude-code start");
    let claude_outcome = claude.wait(&claude_handle).await.expect("claude-code wait");
    assert_eq!(
        claude_outcome.terminal_state,
        crate::client::AttemptState::Succeeded
    );

    for workspace in [codex_workspace, claude_workspace] {
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }
}

/// Acceptance: "register both adapters without introducing
/// ordering-dependent behavior." Registers the two real adapters (and
/// their probes) into two separate `AdapterRegistry` instances in
/// opposite orders and proves both the registered-kind set and dispatch
/// itself are identical either way — `BTreeMap`-keyed registration
/// structurally cannot let "who registered first" become dispatch
/// priority, and this proves it empirically, not just by code
/// inspection. Also proves each real probe's declared cancellation
/// capability (both now `Advisory`) passes
/// the registration-time ceiling check.
#[tokio::test]
async fn registering_both_real_adapters_is_order_independent() {
    let secrets_dir = cross_adapter_temp_dir("order-secrets");
    let secrets = secrets_dir.path();
    let staging_root_dir = cross_adapter_temp_dir("order-artifacts");
    let staging_root = staging_root_dir.path();
    // codex has no fallible `discover()` to `unwrap_or_else` around like
    // claude-code below, so it needs an explicit fixture: a real `codex`
    // binary is a local-dev-only assumption (CI runners don't install
    // it), and `for_fixture` resolves unconditionally, which is exactly
    // what the assertions below need — see the doc comment further down.
    let (codex_program, codex_args, _codex_script_dir) = cross_adapter_fixture_command();

    let mut forward = AdapterRegistry::new();
    forward.register_adapter(
        DomainHarnessKind::new("codex"),
        Box::new(crate::harness::codex::CodexAdapter::for_fixture(
            codex_program.clone(),
            codex_args.clone(),
            staging_root.to_path_buf(),
            cross_adapter_secret_store(secrets),
        )),
    );
    forward.register_adapter(
        DomainHarnessKind::new("claude-code"),
        Box::new(
            crate::harness::claude_code::ClaudeCodeAdapter::discover(cross_adapter_secret_store(
                secrets,
            ))
            .unwrap_or_else(|_| {
                crate::harness::claude_code::ClaudeCodeAdapter::for_fixture(
                    PathBuf::from("/bin/sh"),
                    vec!["-c".to_owned(), "exit 0".to_owned()],
                    cross_adapter_secret_store(secrets),
                )
            }),
        ),
    );

    let mut backward = AdapterRegistry::new();
    backward.register_adapter(
        DomainHarnessKind::new("claude-code"),
        Box::new(
            crate::harness::claude_code::ClaudeCodeAdapter::discover(cross_adapter_secret_store(
                secrets,
            ))
            .unwrap_or_else(|_| {
                crate::harness::claude_code::ClaudeCodeAdapter::for_fixture(
                    PathBuf::from("/bin/sh"),
                    vec!["-c".to_owned(), "exit 0".to_owned()],
                    cross_adapter_secret_store(secrets),
                )
            }),
        ),
    );
    backward.register_adapter(
        DomainHarnessKind::new("codex"),
        Box::new(crate::harness::codex::CodexAdapter::for_fixture(
            codex_program.clone(),
            codex_args.clone(),
            staging_root.to_path_buf(),
            cross_adapter_secret_store(secrets),
        )),
    );

    assert_eq!(forward.registered_kinds(), backward.registered_kinds());
    assert_eq!(forward.registered_kinds(), vec!["claude-code", "codex"]);

    // Dispatch itself, not only the registered-kind set, is order
    // independent. The fixture's requested model (provider "openai",
    // id "opaque/model-alpha") is unsupported by claude-code (unknown
    // provider family), so it rejects it pre-spawn regardless of
    // registry order. codex is a pass-through harness: it cannot
    // independently verify a model's identity, so it accepts any
    // *explicit* provider/id pre-spawn and defers the real check to the
    // harness at run time — an accepted dispatch, not a rejection, for
    // any locator that resolves, which the fixture above always does.
    for kind in ["codex", "claude-code"] {
        let spec = spec_requesting(kind);
        let forward_result = forward.validate(&spec).await;
        let backward_result = backward.validate(&spec).await;
        let expect_ok = kind == "codex";
        assert_eq!(
            forward_result.is_ok(),
            expect_ok,
            "kind {kind}: forward registry"
        );
        assert_eq!(
            backward_result.is_ok(),
            expect_ok,
            "kind {kind}: backward registry"
        );
    }

    // The registration-time gate: both real, now-reconciled probes
    // register cleanly (neither still claims `cancel: Supported`).
    let mut probe_registry = AdapterRegistry::new();
    probe_registry
        .register_probe(Box::new(crate::harness::codex::CodexAdapter::for_fixture(
            codex_program.clone(),
            codex_args.clone(),
            staging_root.to_path_buf(),
            cross_adapter_secret_store(secrets),
        )))
        .expect("codex probe registers cleanly");
    probe_registry
        .register_probe(Box::new(
            crate::harness::claude_code::ClaudeCodeAdapter::discover(cross_adapter_secret_store(
                secrets,
            ))
            .unwrap_or_else(|_| {
                crate::harness::claude_code::ClaudeCodeAdapter::for_fixture(
                    PathBuf::from("/bin/sh"),
                    vec!["-c".to_owned(), "exit 0".to_owned()],
                    cross_adapter_secret_store(secrets),
                )
            }),
        ))
        .expect("claude-code probe registers cleanly");

    let reports = probe_registry.capabilities().await;
    assert_eq!(reports.len(), 2, "both real probes registered");
    let mut kinds: Vec<&str> = reports.iter().map(|r| r.harness_kind.as_str()).collect();
    kinds.sort_unstable();
    assert_eq!(kinds, vec!["claude-code", "codex"]);
}
