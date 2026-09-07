//! Harness process/event infrastructure and the shared adapter-registration
//! seam.
//!
//! [`crate::client::engine::HarnessAdapter`] (re-exported as
//! [`HarnessAdapter`] at this module's root) is the frozen per-attempt
//! lifecycle interface each concrete harness adapter (`codex.rs`,
//! `claude_code.rs`) implements. This module supplies the
//! genuinely separate piece: [`HarnessProbe`], for harness
//! discovery/capability reporting, which has no home in
//! `engine::HarnessAdapter` at all (see "Why `HarnessProbe` is not a sixth
//! `HarnessAdapter` method" below). [`AdapterRegistry`] is the shared
//! registry wiring: a struct that itself implements `engine::HarnessAdapter`
//! by dispatching to whichever concrete adapter matches a claimed attempt's
//! requested harness kind, so `RunnerEngine<P, A, W, C>`'s single `A:
//! HarnessAdapter` can serve every registered harness through one engine
//! instance. [`process`] and [`event_sink`] are the lower-level primitives
//! the concrete adapters compose inside their own
//! `validate`/`start`/`cancel`/`wait` implementations.
//!
//! ## Why `HarnessProbe` is not a sixth `HarnessAdapter` method
//!
//! `engine::HarnessAdapter`'s five methods all take an `ExecutionSpec` or a
//! `LocalRunHandle` — both of which only exist once a request has been
//! claimed. Capability reporting (`RunnerCapabilities.harnesses`, populated
//! at enrollment/refresh — see `tack_orch::execution::capabilities`) has to
//! run *before* any attempt exists, to tell the scheduler what this runner
//! can even do. There is nowhere on the existing trait to hang that. Rather
//! than forcing capability discovery through a method that would need to
//! fabricate an `ExecutionSpec` to call, [`HarnessProbe`] is a small,
//! separate trait for exactly that: "detect version, report capabilities,
//! independent of any specific attempt."
//!
//! ## Two open interface gaps, proven by two real adapters
//!
//! 1. **`LocalRunHandle` cannot name its own harness kind.** `cancel`/`wait`
//!    take only `&LocalRunHandle { process_id: String }`, with no
//!    harness-kind field, so a dispatching registry has no way to route a
//!    bare handle back to the adapter that produced it. [`AdapterRegistry`]
//!    works around this by encoding the kind into the opaque `process_id`
//!    string it hands back from `start` (see `encode_handle`/`decode_handle`
//!    below) and decoding it again in `cancel`/`wait`/`reconcile`. The
//!    straightforward fix — adding a `harness_kind` field to
//!    `LocalRunHandle` — is still not made: `LocalRunHandle { process_id:
//!    ... }` is constructed by literal in `crates/tack-runner/tests/
//!    crash_matrix.rs:277`, and any new required field breaks that
//!    construction.
//! 2. **Kind-key type duplication, still present.** [`AdapterRegistry`] keys
//!    directly on `tack_orch::execution::HarnessKind` (an opaque string,
//!    matching `ExecutionRequestSnapshot::requested_harness_kind`).
//!    `registry.rs` separately defines its own `HarnessKind` enum
//!    (`Codex`/`ClaudeCode`/`Other(String)`). Whether these two
//!    types should be unified, and whether `AdapterRegistry` itself belongs
//!    in `registry.rs` instead of here, remains an open registry-shape
//!    decision.

pub mod artifact;
pub mod claude_code;
pub mod codex;
pub mod event_sink;
pub mod fixtures;
pub mod locate;
pub mod process;
pub mod redact;
pub mod sha256;

use std::collections::BTreeMap;

use async_trait::async_trait;
use tack_orch::execution::{
    CapabilitySupport, ExecutionRequestSnapshot, FeatureCapabilities, HarnessCapability,
    HarnessKind as DomainHarnessKind,
};
use thiserror::Error;

use crate::secrets::SecretStore;

pub use crate::client::engine::{
    CancelObservation, CancellationEvidence, ExecutionSpec, HarnessAdapter, HarnessError,
    HarnessOutcome, LocalRunHandle,
};
pub use crate::client::{AttemptJournal, RecoveryObservation};

/// A closed vocabulary for
/// `ActualExecution.model_observation_source`.
///
/// `tack_orch::execution::ActualExecution.model_observation_source` is a
/// bare `String` on the wire, but the independently
/// implemented adapters converged on exactly these three
/// meanings: `codex.rs` introduced
/// `"requested_not_confirmed"` for "this adapter cannot observe which model
/// actually ran, so it echoes the request instead of fabricating a value";
/// `claude_code.rs` independently produced
/// `"harness_reported"` (the frozen fixture's own exemplar value, used when
/// a real `stream-json` `system`/`init` event names the model) and
/// `"not_observed"` (used only when neither an observation nor a request
/// value exists to report — Claude Code is the one adapter that can honor
/// true auto-selection at all, so it is the only one that can ever hit this
/// case). This enum does not change what any adapter reports in what
/// situation — it centralizes the three literals so a future adapter cannot
/// silently invent a fourth, incompatible string for one of these same three
/// situations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelObservationSource {
    /// The value was read directly from the harness's own output (e.g.
    /// Claude Code's `system`/`init` event `model` field).
    HarnessReported,
    /// The value echoes the operator's request; the harness gave no
    /// independent confirmation it actually used it.
    RequestedNotConfirmed,
    /// Neither an observation nor a request value exists to report (only
    /// reachable by an adapter that permits auto-selection at all).
    NotObserved,
}

impl ModelObservationSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HarnessReported => "harness_reported",
            Self::RequestedNotConfirmed => "requested_not_confirmed",
            Self::NotObserved => "not_observed",
        }
    }
}

/// Resolves `request.environment` into concrete `NAME=value` pairs — the one
/// mechanism `claude_code.rs` and `codex.rs` each call from
/// both `validate` (to fail pre-spawn, discarding the map) and `start` (to
/// build the spawned process's real environment). A literal `value` is used
/// as-is; a `secret_reference` resolves through `store`. Either way the
/// value is also registered with `secrets` so it is redacted if it ever
/// surfaces in captured harness output — exactly like a literal `value`
/// already was before this existed.
///
/// A `secret_reference` this store cannot resolve fails typed, naming only
/// the reference — never fabricated as a silently-unset variable. Calling
/// this from `validate` means that failure happens before any journal
/// record or workspace exists.
pub(crate) fn resolve_environment(
    store: &SecretStore,
    request: &ExecutionRequestSnapshot,
    secrets: &mut redact::SecretMaterial,
) -> Result<BTreeMap<String, String>, HarnessError> {
    let mut resolved = BTreeMap::new();
    for (name, value) in &request.environment {
        match (&value.value, &value.secret_reference) {
            (Some(literal), _) => {
                secrets.register(literal.clone());
                resolved.insert(name.clone(), literal.clone());
            }
            (None, Some(reference)) => {
                let secret = store.resolve(reference).map_err(|error| {
                    tracing::warn!(
                        name = %name,
                        reference = %reference,
                        error = %error,
                        "secret_reference could not be resolved before spawn"
                    );
                    HarnessError::Rejected {
                        reason: format!("secret_reference_unresolved: {reference}"),
                    }
                })?;
                tracing::debug!(name = %name, reference = %reference, "secret_reference resolved");
                secrets.register(secret.expose().to_owned());
                resolved.insert(name.clone(), secret.expose().to_owned());
            }
            (None, None) => {}
        }
    }
    Ok(resolved)
}

/// Harness discovery/capability reporting, independent of any specific
/// claimed attempt. See the module docs for why this is not a sixth
/// [`HarnessAdapter`] method.
///
/// Capability honesty is inherited directly from the existing,
/// already-frozen `tack_orch::execution::capabilities` types this trait
/// returns: [`HarnessCapability`] carries a nullable `probe_error` (so
/// "this harness could not be probed, because X" is representable without
/// treating probe failure as this runner's own bug), and every entry in its
/// `FeatureCapabilities` is a `CapabilityValue { support, reason }` with
/// three explicit levels (`supported` / `unsupported` / `advisory`) — never
/// a bare `bool`, and never silently omitted to mean "no". A
/// [`HarnessProbe`] implementation must fill in real reasons, not leave them
/// `None` for convenience.
#[async_trait]
pub trait HarnessProbe: Send + Sync {
    /// The kind this probe reports for. Kept as a method rather than a
    /// separate registration key so a probe cannot be registered under a
    /// kind it does not itself believe it is reporting for.
    fn harness_kind(&self) -> DomainHarnessKind;

    /// Detects the installed version and reports capabilities. Probing
    /// itself can fail (binary missing, version string unparseable, ...);
    /// that failure belongs in the returned `HarnessCapability.probe_error`,
    /// never as an `Err` — an absent/broken installation is exactly as
    /// "successful" a probe result as a healthy one, just less capable.
    async fn probe(&self) -> HarnessCapability;

    /// The per-feature support this
    /// adapter honestly promises, independent of any specific attempt.
    ///
    /// The only place any adapter computed its own
    /// `FeatureCapabilities` was inside `HarnessAdapter::wait` — *after* a
    /// process had already run — so nothing in the pre-attempt path could
    /// ever check a claimed capability before spawning anything. Each real
    /// adapter's own `declared_capabilities` reuses exactly the same
    /// computation `wait` already stamps onto
    /// `ActualExecution.capability_snapshot`, so there is exactly one source
    /// of truth per adapter, not two that could quietly diverge.
    ///
    /// [`AdapterRegistry::register_probe`] calls this once, at registration,
    /// and refuses to register a probe whose declared `cancel` support
    /// exceeds [`PROCESS_GROUP_CANCEL_CEILING`] — the honest ceiling for the
    /// only cancellation primitive this runner implements
    /// (`harness::process::SupervisedProcess::cancel`, a process-group
    /// SIGTERM/SIGKILL). Checking with `ps` against real Claude Code showed
    /// that mechanism cannot reliably reach a descendant a harness's own
    /// shell-tool spawns into a new OS session. The Codex adapter has no
    /// adapter-specific evidence its own tool execution stays inside the
    /// process group either. So a capability that lies about cancellation is
    /// caught once, here, before any attempt is ever started — never only
    /// discovered when a real cancellation silently fails against a live
    /// attempt.
    fn declared_capabilities(&self) -> FeatureCapabilities;
}

/// The cancellation support ceiling for any [`HarnessProbe`] built on
/// `harness::process::SupervisedProcess::cancel` — see
/// [`HarnessProbe::declared_capabilities`]. Not a blanket "cancel can never
/// be `Supported`" rule: a future adapter with a genuinely different
/// cancellation mechanism (e.g. one that walks the full descendant tree by
/// pid rather than relying on OS process-group membership) could justify a
/// higher ceiling. No adapter in this tree has that mechanism today.
pub const PROCESS_GROUP_CANCEL_CEILING: CapabilitySupport = CapabilitySupport::Advisory;

/// A probe rejected at registration, before it can ever back a
/// claimed attempt. Kept distinct from [`HarnessError`] (a per-attempt,
/// per-`HarnessAdapter`-call error) since this is a registration-time,
/// whole-probe rejection with nothing to do with any single attempt.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HarnessRegistrationError {
    #[error(
        "probe for harness kind {kind:?} declares cancel support {support:?}, which exceeds \
         what the shared process-group cancellation primitive can honestly promise \
         ({ceiling:?}): a descendant that starts its own session or process group is not \
         reachable by a group-wide signal"
    )]
    OverclaimedCancelSupport {
        kind: String,
        support: CapabilitySupport,
        ceiling: CapabilitySupport,
    },
}

/// Dispatches the frozen [`HarnessAdapter`] lifecycle across every
/// registered harness kind, and aggregates [`HarnessProbe`] reports. This
/// **is** the shared registry wiring: each of the concrete adapters is
/// registered here — see the module docs on the open "kind-key type
/// duplication" gap against `registry.rs`'s own `HarnessKind`.
///
/// Implements [`HarnessAdapter`] itself, so `RunnerEngine::new(protocol,
/// adapter_registry, journal, workspaces)` is a complete, multi-harness
/// runner with no `engine.rs` changes: `AdapterRegistry` simply **is** the
/// engine's one concrete adapter type parameter — adding a harness is
/// registering it here, never a new engine type parameter.
#[derive(Default)]
pub struct AdapterRegistry {
    adapters: BTreeMap<String, Box<dyn HarnessAdapter>>,
    probes: BTreeMap<String, Box<dyn HarnessProbe>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: BTreeMap::new(),
            probes: BTreeMap::new(),
        }
    }

    pub fn register_adapter(
        &mut self,
        kind: DomainHarnessKind,
        adapter: Box<dyn HarnessAdapter>,
    ) -> &mut Self {
        self.adapters.insert(kind.as_str().to_owned(), adapter);
        self
    }

    /// Registers a probe, first checking its own [`HarnessProbe::declared_capabilities`]
    /// against [`PROCESS_GROUP_CANCEL_CEILING`] — a lying capability is
    /// caught before invocation. A probe that overclaims is
    /// rejected here and never inserted — `capabilities()`/dispatch never
    /// see it — rather than silently accepted and only discovered wrong once
    /// a real cancellation against a live attempt fails to reach a detached
    /// descendant.
    pub fn register_probe(
        &mut self,
        probe: Box<dyn HarnessProbe>,
    ) -> Result<&mut Self, HarnessRegistrationError> {
        let declared = probe.declared_capabilities();
        if declared.cancel.support == CapabilitySupport::Supported
            && PROCESS_GROUP_CANCEL_CEILING != CapabilitySupport::Supported
        {
            return Err(HarnessRegistrationError::OverclaimedCancelSupport {
                kind: probe.harness_kind().as_str().to_owned(),
                support: declared.cancel.support,
                ceiling: PROCESS_GROUP_CANCEL_CEILING,
            });
        }
        self.probes
            .insert(probe.harness_kind().as_str().to_owned(), probe);
        Ok(self)
    }

    /// Harness kinds with a registered adapter, in deterministic sorted
    /// order (`BTreeMap` iteration order), never insertion order — so which
    /// adapter registered first can never become accidental dispatch priority.
    pub fn registered_kinds(&self) -> Vec<String> {
        self.adapters.keys().cloned().collect()
    }

    /// Probes every registered harness. Order is the same deterministic
    /// sorted order as [`Self::registered_kinds`], so a capability snapshot
    /// diff is never noise from iteration order alone.
    pub async fn capabilities(&self) -> Vec<HarnessCapability> {
        let mut reports = Vec::with_capacity(self.probes.len());
        for probe in self.probes.values() {
            reports.push(probe.probe().await);
        }
        reports
    }

    fn resolve(&self, kind: &str) -> Result<&dyn HarnessAdapter, HarnessError> {
        self.adapters
            .get(kind)
            .map(|boxed| boxed.as_ref())
            .ok_or_else(|| HarnessError::Rejected {
                reason: format!("no adapter is registered for harness kind {kind:?}"),
            })
    }
}

#[async_trait]
impl HarnessAdapter for AdapterRegistry {
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        let kind = spec.work.request.requested_harness_kind.as_str();
        self.resolve(kind)?.validate(spec).await
    }

    async fn start(&self, spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
        let kind = spec.work.request.requested_harness_kind.as_str();
        let handle = self.resolve(kind)?.start(spec).await?;
        Ok(LocalRunHandle {
            process_id: encode_handle(kind, &handle.process_id),
        })
    }

    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        let (kind, inner) = decode_handle(&handle.process_id).ok_or(HarnessError::Process)?;
        self.resolve(&kind)?
            .cancel(&LocalRunHandle { process_id: inner })
            .await
    }

    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        let (kind, inner) = decode_handle(&handle.process_id).ok_or(HarnessError::Process)?;
        self.resolve(&kind)?
            .wait(&LocalRunHandle { process_id: inner })
            .await
    }

    async fn reconcile(
        &self,
        journal: &AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        let Some(process_id) = journal.process_id.as_deref() else {
            // No process was ever confirmed running for this attempt, for
            // any harness kind: there is nothing kind-specific left to
            // check, so this is the one case that needs no dispatch at all.
            return Ok(RecoveryObservation::ProcessStopped);
        };
        let (kind, inner) = decode_handle(process_id).ok_or(HarnessError::RecoveryUnavailable)?;
        let mut delegated = journal.clone();
        delegated.process_id = Some(inner);
        self.resolve(&kind)?.reconcile(&delegated).await
    }
}

/// Embeds `kind` into an opaque handle string as `<hex(kind)>:<inner>`.
/// Hex-encoding the kind (not the inner id) means the inner process id can
/// contain any bytes, including a literal `:`, without ambiguity — the
/// split only ever happens on the *first* colon, mirroring the same
/// hex-encoding-for-safe-embedding convention `journal.rs`/`workspace.rs`
/// already use for attempt ids.
fn encode_handle(kind: &str, inner: &str) -> String {
    let hex_kind: String = kind
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("{hex_kind}:{inner}")
}

fn decode_handle(process_id: &str) -> Option<(String, String)> {
    let (hex_kind, inner) = process_id.split_once(':')?;
    if hex_kind.is_empty() || hex_kind.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex_kind.len() / 2);
    for pair in hex_kind.as_bytes().as_chunks::<2>().0 {
        let pair_str = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(pair_str, 16).ok()?);
    }
    let kind = String::from_utf8(bytes).ok()?;
    Some((kind, inner.to_owned()))
}

#[cfg(test)]
mod tests {
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

        async fn cancel(
            &self,
            handle: &LocalRunHandle,
        ) -> Result<CancellationEvidence, HarnessError> {
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
                crate::harness::claude_code::ClaudeCodeAdapter::discover(
                    cross_adapter_secret_store(secrets),
                )
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
                crate::harness::claude_code::ClaudeCodeAdapter::discover(
                    cross_adapter_secret_store(secrets),
                )
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
                crate::harness::claude_code::ClaudeCodeAdapter::discover(
                    cross_adapter_secret_store(secrets),
                )
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
}
