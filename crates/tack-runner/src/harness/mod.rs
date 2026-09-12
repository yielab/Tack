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
#[path = "tests.rs"]
mod tests;
