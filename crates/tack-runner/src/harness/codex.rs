//! Codex harness adapter/probe.
//!
//! Implements [`crate::harness::HarnessAdapter`] and [`crate::harness::HarnessProbe`] for
//! `harness_kind = "codex"`, composing the shared process/redaction/artifact infrastructure
//! (`crate::harness::{process, redact, artifact}`).
//!
//! Vendor findings — what is measured, what is a documented guess, and at what observed
//! version: `fixtures/codex/README.md`, next to the transcripts that prove them.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tack_orch::execution::{
    ActualExecution, ActualModelId, ActualModelProvider, CapabilitySupport, CapabilityValue,
    FeatureCapabilities, HarnessCapability, HarnessKind, Measurement, MeasurementSource, Usage,
    WorkspaceId as DomainWorkspaceId,
};

use crate::client::{AttemptState, Timestamp};
use crate::harness::{
    AttemptJournal, CancelObservation, CancellationEvidence, ExecutionSpec, HarnessAdapter,
    HarnessError, HarnessOutcome, HarnessProbe, LocalRunHandle, RecoveryObservation,
    artifact::ArtifactStager,
    process::{
        CancelOutcome, CapturedOutput, ProcessExit, ProcessLimits, ProcessSpec, SupervisedProcess,
    },
    redact::SecretMaterial,
};

const CODEX_HARNESS_KIND: &str = "codex";
const CODEX_PROGRAM_NAME: &str = "codex";
const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(15);
/// Not part of any frozen vocabulary today — see the module docs' assumption
/// (5) for why this is a new, adapter-chosen value rather
/// than the fixture-exemplified `"harness_reported"`.
const MODEL_OBSERVATION_SOURCE: &str =
    crate::harness::ModelObservationSource::RequestedNotConfirmed.as_str();
/// Local key this adapter names an injected provider endpoint under, for
/// Codex's own `-c model_providers.<key>.*` overrides — an adapter-chosen
/// label, not a vendor name. Codex only ever sees it for the lifetime of
/// one invocation; nothing persists it.
const CODEX_PROVIDER_KEY: &str = "tack_provider";

/// Quotes `value` the way `-c key="value"` expects: a double-quoted TOML
/// string. Safe for the plain ASCII values this adapter ever passes (a URL,
/// an environment variable name, a display label) — never used on
/// operator- or attempt-supplied text.
fn toml_quoted(value: &str) -> String {
    format!("{value:?}")
}

/// Where to find the `codex` executable.
#[derive(Clone)]
enum CodexLocator {
    /// A snapshot of the runner process's own `PATH` and home directory,
    /// taken once at construction (see `crate::harness::locate::snapshot`)
    /// and re-searched — `PATH` first, then the shared well-known install
    /// locations — on every [`CodexLocator::resolve`] call. Production
    /// default via [`CodexAdapter::discover`].
    Search {
        program_name: String,
        path: Option<std::ffi::OsString>,
        home: Option<PathBuf>,
    },
    /// A fixed program plus prefix args — how every fake-binary test in this
    /// file points the adapter at `crate::harness::fixtures::fake_harness_command`
    /// instead of a real `codex` binary. Never constructed by production
    /// code (only [`CodexLocator::Search`] is, via [`CodexAdapter::discover`]),
    /// so this variant is `#[cfg(test)]`-only, matching the same pattern
    /// `journal.rs`'s `OwnerOnlyJournal::fail_next_update` already uses for a
    /// field that exists purely to make a test possible.
    #[cfg(test)]
    Fixed {
        program: PathBuf,
        prefix_args: Vec<String>,
    },
}

impl CodexLocator {
    fn resolve(&self) -> Result<(PathBuf, Vec<String>), String> {
        match self {
            #[cfg(test)]
            Self::Fixed {
                program,
                prefix_args,
            } => Ok((program.clone(), prefix_args.clone())),
            Self::Search {
                program_name,
                path,
                home,
            } => super::locate::locate(program_name, path.as_deref(), home.as_deref())
                .map(|program| (program, Vec::new()))
                .map_err(|error| error.to_string()),
        }
    }
}

/// Strict `X.Y[.Z]` numeric-only check against one whitespace-delimited
/// token. Deliberately whole-token, not substring: the shared fixture's
/// `unknown_version` mode (`"harness-cli version
/// 999.999.999-nightly-exotic-format"`) genuinely *contains* a dot-separated
/// numeric run, but neither that token nor any other in the line is a clean
/// version, and none must be reported as one.
fn is_strict_version(candidate: &str) -> bool {
    let parts: Vec<&str> = candidate.split('.').collect();
    (2..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

/// Finds the first whitespace-delimited token in `text` that is itself a
/// strict `X.Y[.Z]` version ([`is_strict_version`]). The real binary prints
/// `codex-cli 0.149.1` — a program-name token ahead of the version, not a
/// bare version string on its own — so the check must scan tokens rather
/// than require the whole trimmed line to be one.
fn find_strict_version_token(text: &str) -> Option<&str> {
    text.split_whitespace()
        .find(|token| is_strict_version(token))
}

fn bounded_preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_owned()
    } else {
        let truncated: String = text.chars().take(max_chars).collect();
        format!("{truncated}\u{2026} (truncated)")
    }
}

fn rfc3339(time: std::time::SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn not_measured<T>() -> Measurement<T> {
    Measurement {
        value: None,
        source: MeasurementSource::NotMeasured,
        additional: BTreeMap::new(),
    }
}

fn describe_capture(output: &CapturedOutput) -> serde_json::Value {
    serde_json::json!({
        "truncated": output.truncated,
        "bytes_dropped": output.bytes_dropped,
        "total_bytes_seen": output.total_bytes_seen,
        // Already scrubbed by `SecretMaterial` before this text was ever
        // retained (see `process.rs::finalize_capture`); bounding it again
        // here is about payload size, not redaction.
        "text_preview": bounded_preview(&output.text, 2000),
    })
}

/// Terminal-state classification from the process exit alone — see module
/// docs, assumption (4), for why content is deliberately never consulted.
fn classify_exit(exit: &ProcessExit) -> (AttemptState, &'static str, String) {
    match exit {
        ProcessExit::Exited(0) => (
            AttemptState::Succeeded,
            "completed",
            "codex exited successfully".to_owned(),
        ),
        ProcessExit::Exited(code) => (
            AttemptState::Failed,
            "exit_code",
            format!("codex exited with status {code}"),
        ),
        #[cfg(unix)]
        ProcessExit::Signaled(signal) => (
            AttemptState::Failed,
            "signaled",
            format!("codex was terminated by signal {signal}"),
        ),
        ProcessExit::TimedOut => (
            AttemptState::Failed,
            "timed_out",
            "codex exceeded its configured timeout and was killed".to_owned(),
        ),
    }
}

/// The opaque handle format this adapter hands back from `start` and expects
/// from `cancel`/`wait`/`reconcile`: `codex:<pid>:<monotonic counter>`. The
/// counter exists only to guarantee uniqueness within one adapter instance's
/// lifetime (pids can, in principle, be reused); it carries no other meaning.
fn encode_handle(pid: u32, counter: u64) -> String {
    format!("codex:{pid}:{counter}")
}

fn parse_handle_pid(process_id: &str) -> Option<u32> {
    let mut parts = process_id.split(':');
    if parts.next()? != "codex" {
        return None;
    }
    let pid = parts.next()?.parse::<u32>().ok()?;
    parts.next()?; // counter, required but not itself inspected
    if parts.next().is_some() {
        return None; // exactly three colon-separated parts, no more
    }
    Some(pid)
}

/// State for one in-flight `start()` → (`cancel()` | `wait()`) pair. Not
/// `Debug`: several fields (`secrets`, captured process handles) must never
/// be printable by accident (rule 12) — omitting the derive entirely is
/// simpler than auditing a hand-written impl every time a field is added.
struct RunningCodexProcess {
    process: SupervisedProcess,
    secrets: SecretMaterial,
    limits: ProcessLimits,
    started_at: DateTime<Utc>,
    workspace_path: PathBuf,
    workspace_id: String,
    base_revision: String,
    attempt_id: String,
    harness_version: String,
    model_provider: String,
    model_id: String,
}

/// The Codex harness adapter/probe. Implements both
/// [`crate::harness::HarnessAdapter`] (the frozen per-attempt lifecycle) and
/// [`crate::harness::HarnessProbe`] (capability discovery).
///
/// Time is injected via `C: crate::Clock` (never `SystemTime::now()`
/// directly) so tests can assert exact `started_at`/`ended_at`/`probed_at`
/// values without a real sleep (rule 9).
pub struct CodexAdapter<C = crate::SystemClock> {
    command: CodexLocator,
    process_limits: ProcessLimits,
    probe_timeout: Duration,
    /// Extra environment merged into every version-detection invocation
    /// only. Always empty in production ([`Self::discover`]); fake-binary
    /// tests use this to steer the shared fixture's `TACK_FAKE_HARNESS_MODE`
    /// during probing specifically, independent of whatever mode a given
    /// test's `start()`/`wait()` call drives via the execution request's own
    /// `environment` map (the exec path's env is never influenced by this
    /// field, only the probe path's is).
    probe_env: BTreeMap<String, String>,
    artifact_staging_root: PathBuf,
    clock: C,
    next_handle: AtomicU64,
    running: tokio::sync::Mutex<BTreeMap<String, RunningCodexProcess>>,
    /// The most recently probed `(installed_version, probe_error)`, used to
    /// stamp `ActualExecution.harness_version` at `wait()` time without a
    /// redundant `--version` invocation on every single attempt. `None`
    /// until the first successful [`HarnessProbe::probe`] call; `start()`
    /// falls back to a one-off detection in that case rather than reporting
    /// a silently fabricated version.
    last_probe: tokio::sync::Mutex<Option<(String, Option<String>)>>,
    /// Resolves `secret_reference` environment entries. Shared with every
    /// other adapter the runner constructed at startup — see
    /// `crate::secrets::SecretStore`.
    secrets: crate::secrets::SecretStore,
    /// Configured provider endpoints (`RunnerConfig::providers`), consulted
    /// only when a request's `requested_model_provider` names one — see
    /// `crate::provider::resolve_endpoint`. Empty by default, meaning every
    /// request spawns against Codex's own built-in provider.
    providers: BTreeMap<String, crate::config::ProviderConfig>,
}

impl CodexAdapter<crate::SystemClock> {
    /// Production constructor: resolves `codex` from the current process's
    /// `PATH` (snapshotted once, here) rather than a hardcoded path.
    /// `artifact_staging_root` is required explicitly, matching
    /// [`ArtifactStager::new`]'s own no-hidden-default style.
    pub fn discover(
        process_limits: ProcessLimits,
        artifact_staging_root: PathBuf,
        secrets: crate::secrets::SecretStore,
    ) -> Self {
        let (path, home) = super::locate::snapshot();
        Self::with_clock(
            CodexLocator::Search {
                program_name: CODEX_PROGRAM_NAME.to_owned(),
                path,
                home,
            },
            process_limits,
            DEFAULT_PROBE_TIMEOUT,
            BTreeMap::new(),
            artifact_staging_root,
            crate::SystemClock,
            secrets,
        )
    }

    /// `pub(crate)` and test-only: points this adapter at an
    /// arbitrary fixture command instead of a real `codex` binary, for the
    /// "same fixture completes through all three fake adapters" acceptance
    /// proof in `harness::mod::tests` (which needs to construct a real
    /// `CodexAdapter` from outside this module). Not part of the public API
    /// — `AdapterRegistry` only ever stores `Box<dyn HarnessAdapter>`, which
    /// never needs to know how a concrete adapter was constructed.
    #[cfg(test)]
    pub(crate) fn for_fixture(
        program: PathBuf,
        prefix_args: Vec<String>,
        artifact_staging_root: PathBuf,
        secrets: crate::secrets::SecretStore,
    ) -> Self {
        Self::with_clock(
            CodexLocator::Fixed {
                program,
                prefix_args,
            },
            ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(10)),
            Duration::from_secs(5),
            BTreeMap::new(),
            artifact_staging_root,
            crate::SystemClock,
            secrets,
        )
    }
}

impl<C> CodexAdapter<C>
where
    C: crate::Clock,
{
    fn with_clock(
        command: CodexLocator,
        process_limits: ProcessLimits,
        probe_timeout: Duration,
        probe_env: BTreeMap<String, String>,
        artifact_staging_root: PathBuf,
        clock: C,
        secrets: crate::secrets::SecretStore,
    ) -> Self {
        Self {
            command,
            process_limits,
            probe_timeout,
            probe_env,
            artifact_staging_root,
            clock,
            next_handle: AtomicU64::new(0),
            running: tokio::sync::Mutex::new(BTreeMap::new()),
            last_probe: tokio::sync::Mutex::new(None),
            secrets,
            providers: BTreeMap::new(),
        }
    }

    /// Configures the provider endpoints this adapter may point a spawn at
    /// — see `crate::provider::resolve_endpoint`. Not part of `with_clock`
    /// itself so every existing call site (fixtures, tests) keeps
    /// constructing an adapter with no configured endpoint at all, exactly
    /// today's behavior, without editing each one.
    pub fn with_providers(
        mut self,
        providers: BTreeMap<String, crate::config::ProviderConfig>,
    ) -> Self {
        self.providers = providers;
        self
    }

    /// `harness_kind` self-check plus the "no auto-selected model" rejection
    /// documented in the module docs. Shared by `validate` and `start` so
    /// the two can never disagree about what counts as an unsupported
    /// selection.
    fn check_selection(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        if spec.work.request.requested_harness_kind.as_str() != CODEX_HARNESS_KIND {
            let reason = format!(
                "requested harness kind {:?} does not match this adapter's kind {CODEX_HARNESS_KIND:?}",
                spec.work.request.requested_harness_kind.as_str()
            );
            tracing::warn!(
                reason,
                "codex: rejecting a spec requesting a different harness kind"
            );
            return Err(HarnessError::Rejected { reason });
        }
        if spec.work.request.requested_model_provider.is_none()
            || spec.work.request.requested_model_id.is_none()
        {
            let reason = "codex cannot independently confirm which model an auto-selected run \
                           actually used, so ActualExecution.model_provider/model_id (non-nullable) \
                           cannot be honestly filled; an explicit requested_model_provider and \
                           requested_model_id are both required"
                .to_owned();
            tracing::warn!(reason, "codex: rejecting an auto-selected model pre-spawn");
            return Err(HarnessError::Rejected { reason });
        }
        Ok(())
    }

    /// Runs `codex --version` (assumption (2), see module docs) with
    /// `self.probe_env` merged in, bounded by `self.probe_timeout`. Never
    /// returns an `Err`: every failure mode (binary missing, spawn failure,
    /// nonzero exit, timeout, unparseable output) is folded into the
    /// `Option<String>` (probe-error reason) return slot, matching
    /// `HarnessProbe::probe`'s own contract that probing itself cannot fail.
    async fn detect_version(
        &self,
    ) -> (String, Option<String>, BTreeMap<String, serde_json::Value>) {
        let (program, mut args) = match self.command.resolve() {
            Ok(resolved) => resolved,
            Err(reason) => return (String::new(), Some(reason), BTreeMap::new()),
        };
        args.push("--version".to_owned());

        let probe_workspace = std::env::temp_dir();
        let process_spec = ProcessSpec {
            program,
            args,
            env: self.probe_env.clone(),
            stdin: None,
            working_directory: probe_workspace.clone(),
            workspace_root: probe_workspace,
        };

        let limits = ProcessLimits::new(8192, 8192, self.probe_timeout);
        let spawned = match process_spec.spawn().await {
            Ok(child) => child,
            Err(error) => {
                return (
                    String::new(),
                    Some(format!("codex --version could not be spawned: {error}")),
                    BTreeMap::new(),
                );
            }
        };
        let result = match spawned
            .wait_with_capture(&limits, &SecretMaterial::new())
            .await
        {
            Ok(result) => result,
            Err(error) => {
                return (
                    String::new(),
                    Some(format!(
                        "codex --version failed while capturing output: {error}"
                    )),
                    BTreeMap::new(),
                );
            }
        };

        match result.exit {
            ProcessExit::Exited(0) => {
                let trimmed = result.stdout.text.trim();
                if trimmed.is_empty() {
                    (
                        String::new(),
                        Some("codex --version produced no output".to_owned()),
                        BTreeMap::new(),
                    )
                } else if let Some(version) = find_strict_version_token(trimmed) {
                    (version.to_owned(), None, BTreeMap::new())
                } else {
                    let mut additional = BTreeMap::new();
                    additional.insert(
                        "raw_version_output".to_owned(),
                        serde_json::json!(bounded_preview(trimmed, 200)),
                    );
                    (
                        String::new(),
                        Some(
                            "codex --version output was not a recognizable version string"
                                .to_owned(),
                        ),
                        additional,
                    )
                }
            }
            ProcessExit::Exited(code) => (
                String::new(),
                Some(format!("codex --version exited with status {code}")),
                BTreeMap::new(),
            ),
            #[cfg(unix)]
            ProcessExit::Signaled(signal) => (
                String::new(),
                Some(format!("codex --version was terminated by signal {signal}")),
                BTreeMap::new(),
            ),
            ProcessExit::TimedOut => (
                String::new(),
                Some("codex --version timed out".to_owned()),
                BTreeMap::new(),
            ),
        }
    }

    /// Honest, harness-agnostic-where-possible feature support. See module
    /// docs assumption (6) for why `resume`/`decisions`/`usage` are
    /// `unsupported` rather than guessed, and why `artifacts` is `advisory`.
    fn feature_capabilities(&self) -> FeatureCapabilities {
        FeatureCapabilities {
            // Downgraded from `Supported`. This
            // adapter's only cancellation primitive is
            // `harness::process::SupervisedProcess::cancel` (a process-group
            // SIGTERM/SIGKILL), the exact same mechanism proved (via `ps`
            // against real Claude Code) cannot reliably reach a
            // descendant a harness's own shell-tool spawns into a new OS
            // session. `codex` is not installed
            // on any machine this adapter has been built against, so there
            // is no adapter-specific evidence its own tool execution stays
            // inside the process group either; claiming `Supported` on that
            // silence would be exactly the "hidden fake success" rule 7
            // forbids.
            cancel: CapabilityValue {
                support: CapabilitySupport::Advisory,
                reason: Some(
                    "the top-level codex process is always signalled reliably (it is always \
                     its own process-group leader), but a shell-tool-spawned descendant that \
                     detaches into its own OS session (observed for Claude Code; \
                     never independently verified for codex, since codex is not installed) \
                     would only be reached if it exits gracefully within the SIGTERM grace \
                     period — a SIGKILL escalation cannot reach a different session's process \
                     group"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            resume: CapabilityValue {
                support: CapabilitySupport::Unsupported,
                reason: Some(
                    "codex session/resume behavior has not been observed and is not \
                     implemented by this adapter"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            decisions: CapabilityValue {
                support: CapabilitySupport::Unsupported,
                reason: Some(
                    "the runner protocol has no wired decision transport yet, and codex's own \
                     approval/decision behavior has not been observed"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            artifacts: CapabilityValue {
                support: CapabilitySupport::Advisory,
                reason: Some(
                    "only raw captured stdout/stderr is staged as a log artifact; no \
                     codex-specific artifact discovery (e.g. a git diff) has been implemented \
                     or verified"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            usage: CapabilityValue {
                support: CapabilitySupport::Unsupported,
                reason: Some(
                    "token/cost usage has not been observed in codex output on this machine; \
                     only wall-clock duration is measured"
                        .to_owned(),
                ),
                additional: BTreeMap::new(),
            },
            additional: BTreeMap::new(),
        }
    }

    async fn take_running(&self, process_id: &str) -> Result<RunningCodexProcess, HarnessError> {
        self.running.lock().await.remove(process_id).ok_or_else(|| {
            tracing::warn!(
                process_id,
                "codex: handle not tracked by this adapter instance"
            );
            HarnessError::Process
        })
    }

    /// Stages the (already-scrubbed) combined stdout/stderr as a `log`
    /// artifact inside the attempt's own workspace, via
    /// [`ArtifactStager`]. Best-effort: staging failure never fails the
    /// attempt itself, matching the "auto-status propagation" best-effort
    /// pattern already established elsewhere in this codebase — it only
    /// omits the `artifact` key from `terminal_reason`.
    fn stage_run_log(
        &self,
        workspace_path: &std::path::Path,
        attempt_id: &str,
        stdout: &CapturedOutput,
        stderr: &CapturedOutput,
    ) -> Option<serde_json::Value> {
        let relative = PathBuf::from(".tack-runner").join("codex-run.log");
        let absolute = workspace_path.join(&relative);
        if let Some(parent) = absolute.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            return None;
        }
        let mut combined = String::new();
        combined.push_str("=== stdout ===\n");
        combined.push_str(&stdout.text);
        combined.push_str("\n=== stderr ===\n");
        combined.push_str(&stderr.text);
        if std::fs::write(&absolute, combined.as_bytes()).is_err() {
            return None;
        }

        let stager = ArtifactStager::new(&self.artifact_staging_root);
        match stager.stage_file(attempt_id, workspace_path, &relative, "log", "text/plain") {
            Ok(staged) => Some(serde_json::json!({
                "kind": staged.kind,
                "name": staged.name,
                "media_type": staged.media_type,
                "size_bytes": staged.size_bytes,
                "sha256": staged.sha256,
                "staged_path": staged.staged_path.display().to_string(),
            })),
            Err(error) => {
                tracing::warn!(?error, "codex wait: artifact staging failed");
                None
            }
        }
    }
}

#[async_trait]
impl<C> HarnessProbe for CodexAdapter<C>
where
    C: crate::Clock,
{
    fn harness_kind(&self) -> HarnessKind {
        HarnessKind::new(CODEX_HARNESS_KIND)
    }

    async fn probe(&self) -> HarnessCapability {
        let (installed_version, probe_error, additional) = self.detect_version().await;
        *self.last_probe.lock().await = Some((installed_version.clone(), probe_error.clone()));
        HarnessCapability {
            harness_kind: HarnessKind::new(CODEX_HARNESS_KIND),
            installed_version,
            probe_error,
            probed_at: DateTime::<Utc>::from(self.clock.now()),
            // Deliberately always empty — see module docs assumption (7).
            model_combinations: Vec::new(),
            // A pass-through attestation: a claim about THIS adapter's
            // invocation contract (`--model <requested_model_id>` is passed
            // verbatim, and a spec without an explicit model is rejected
            // pre-spawn — see the module docs), not about which models
            // exist. Assumption (7) stands: no model list is invented.
            model_passthrough: Some(CapabilityValue {
                support: CapabilitySupport::Supported,
                reason: Some(
                    "the adapter forwards requested_model_id verbatim via --model and rejects \
                     specs without an explicit model pre-spawn; model validity is established \
                     by the Codex CLI at run time, so operator-specified opaque models are \
                     accepted without the probe claiming any model list"
                        .to_string(),
                ),
                additional: Default::default(),
            }),
            additional,
        }
    }

    fn declared_capabilities(&self) -> FeatureCapabilities {
        self.feature_capabilities()
    }
}

#[async_trait]
impl<C> HarnessAdapter for CodexAdapter<C>
where
    C: crate::Clock,
{
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        self.check_selection(spec)?;
        self.command.resolve().map_err(|reason| {
            tracing::warn!(reason, "codex validate: binary unresolvable");
            HarnessError::Rejected { reason }
        })?;
        // Every `secret_reference` entry must resolve before the harness
        // process exists. This discards the resolved values — `start`
        // resolves again for real. The engine has already journaled and
        // announced the attempt by now, and turns a refusal here into a
        // reported failure rather than an abandoned lease.
        super::resolve_environment(
            &self.secrets,
            &spec.work.request,
            &mut SecretMaterial::new(),
        )?;

        // Same discard-and-recheck discipline, for a configured provider
        // endpoint. `check_selection` above already guarantees a provider
        // is present.
        let provider = spec
            .work
            .request
            .requested_model_provider
            .as_ref()
            .expect("check_selection rejects a missing model provider before this point")
            .as_str();
        if let Err(error) = crate::provider::resolve_endpoint(
            &self.providers,
            &self.secrets,
            provider,
            crate::provider::Wire::OpenAiResponses,
        ) {
            let reason = error.to_string();
            tracing::warn!(
                reason,
                "codex: rejecting a request whose provider endpoint could not be resolved"
            );
            return Err(HarnessError::Rejected { reason });
        }
        Ok(())
    }

    async fn start(&self, spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
        self.check_selection(spec)?;
        let (program, mut args) = self.command.resolve().map_err(|reason| {
            tracing::warn!(reason, "codex start: binary unresolvable");
            HarnessError::Rejected { reason }
        })?;

        let model_provider = spec
            .work
            .request
            .requested_model_provider
            .as_ref()
            .expect("check_selection rejects a missing model provider before this point")
            .as_str()
            .to_owned();
        let model_id = spec
            .work
            .request
            .requested_model_id
            .as_ref()
            .expect("check_selection rejects a missing model id before this point")
            .as_str()
            .to_owned();

        // A configured provider endpoint applies only when this request's
        // provider names one (e.g. a gateway) — a direct-vendor request
        // (Codex's own built-in provider) resolves to `None` and this
        // adapter injects nothing, so the two paths can never be confused
        // by a shared environment variable. `-c` overrides are per-
        // invocation only: this adapter never writes `~/.codex/config.toml`.
        // They are global flags, so they must precede the `exec` subcommand
        // pushed below.
        let endpoint = match crate::provider::resolve_endpoint(
            &self.providers,
            &self.secrets,
            &model_provider,
            crate::provider::Wire::OpenAiResponses,
        ) {
            Ok(endpoint) => endpoint,
            Err(error) => {
                return Err(HarnessError::Rejected {
                    reason: error.to_string(),
                });
            }
        };
        if let Some(endpoint) = &endpoint {
            args.push("-c".to_owned());
            args.push(format!("model_provider={CODEX_PROVIDER_KEY}"));
            args.push("-c".to_owned());
            args.push(format!(
                "model_providers.{CODEX_PROVIDER_KEY}.name={}",
                toml_quoted(&endpoint.display_name)
            ));
            args.push("-c".to_owned());
            args.push(format!(
                "model_providers.{CODEX_PROVIDER_KEY}.base_url={}",
                toml_quoted(&endpoint.base_url)
            ));
            args.push("-c".to_owned());
            args.push(format!(
                "model_providers.{CODEX_PROVIDER_KEY}.env_key={}",
                toml_quoted(&endpoint.credential_env_var)
            ));
            // Measured non-load-bearing in the installed binary (0.149.1)
            // — "responses" already applies as the effective default — but
            // set explicitly anyway, defensively, matching the vendor's own
            // documented shape.
            args.push("-c".to_owned());
            args.push(format!(
                "model_providers.{CODEX_PROVIDER_KEY}.wire_api={}",
                toml_quoted("responses")
            ));
        }

        // Measured against the real binary: `exec --json --model <id>` with
        // the prompt on stdin is the actual non-interactive invocation
        // shape, not a guess — see the module docs' corrected assumption
        // (3).
        args.push("exec".to_owned());
        args.push("--json".to_owned());
        args.push("--model".to_owned());
        args.push(model_id.clone());

        let prompt = spec
            .work
            .request
            .resolved_agent_profile
            .instructions
            .clone();

        let mut secrets = SecretMaterial::new();
        secrets.register(prompt.clone());

        let mut env = super::resolve_environment(&self.secrets, &spec.work.request, &mut secrets)?;
        if let Some(endpoint) = endpoint {
            env.insert(
                endpoint.credential_env_var,
                endpoint.credential.expose().to_string(),
            );
        }

        let timeout = if spec.work.request.timeout_seconds > 0 {
            Duration::from_secs(spec.work.request.timeout_seconds)
        } else {
            self.process_limits.timeout
        };
        let limits = ProcessLimits {
            timeout,
            ..self.process_limits.clone()
        };

        let process_spec = ProcessSpec {
            program,
            args,
            env,
            stdin: Some(prompt.into_bytes()),
            working_directory: spec.workspace.path.clone(),
            workspace_root: spec.workspace.path.clone(),
        };

        let supervised = process_spec.spawn().await.map_err(|error| {
            tracing::warn!(?error, "codex start: spawn failed");
            HarnessError::Process
        })?;
        let pid = supervised.pid();

        let harness_version = match self.last_probe.lock().await.clone() {
            Some((version, _)) if !version.is_empty() => version,
            _ => self.detect_version().await.0,
        };

        let handle_id = encode_handle(pid, self.next_handle.fetch_add(1, Ordering::SeqCst));
        let running = RunningCodexProcess {
            process: supervised,
            secrets,
            limits,
            started_at: DateTime::<Utc>::from(self.clock.now()),
            workspace_path: spec.workspace.path.clone(),
            workspace_id: spec.workspace.id.as_str().to_owned(),
            base_revision: spec.workspace.base_revision.clone(),
            attempt_id: spec.work.lease.attempt_id.as_str().to_owned(),
            harness_version,
            model_provider,
            model_id,
        };
        self.running.lock().await.insert(handle_id.clone(), running);

        Ok(LocalRunHandle {
            process_id: handle_id,
        })
    }

    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        let running = self.take_running(&handle.process_id).await?;
        let outcome = running
            .process
            .cancel(running.limits.termination_grace)
            .await
            .map_err(|error| {
                tracing::warn!(?error, "codex cancel: signal delivery failed");
                HarnessError::Process
            })?;

        let mut details = serde_json::Map::new();
        details.insert(
            "outcome".to_owned(),
            serde_json::json!(match outcome {
                CancelOutcome::Stopped => "stopped_after_sigterm",
                CancelOutcome::Killed => "killed_after_sigkill",
            }),
        );

        Ok(CancellationEvidence {
            observation: CancelObservation::ProcessStopped,
            observed_at: Timestamp::new(rfc3339(self.clock.now())),
            details,
        })
    }

    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        let RunningCodexProcess {
            process,
            secrets,
            limits,
            started_at,
            workspace_path,
            workspace_id,
            base_revision,
            attempt_id,
            harness_version,
            model_provider,
            model_id,
        } = self.take_running(&handle.process_id).await?;

        let result = process
            .wait_with_capture(&limits, &secrets)
            .await
            .map_err(|error| {
                tracing::warn!(?error, "codex wait: capture failed");
                HarnessError::Process
            })?;

        let ended_at = DateTime::<Utc>::from(self.clock.now());
        let elapsed_ms = ended_at
            .signed_duration_since(started_at)
            .num_milliseconds()
            .max(0) as u64;

        let (terminal_state, code, message) = classify_exit(&result.exit);
        let mut terminal_reason = serde_json::json!({
            "code": code,
            "message": message,
            "stdout": describe_capture(&result.stdout),
            "stderr": describe_capture(&result.stderr),
        });
        if let Some(artifact) =
            self.stage_run_log(&workspace_path, &attempt_id, &result.stdout, &result.stderr)
        {
            terminal_reason["artifact"] = artifact;
        }

        let usage = Usage {
            tokens_in: not_measured(),
            tokens_out: not_measured(),
            duration_ms: Measurement {
                value: Some(elapsed_ms),
                source: MeasurementSource::Measured,
                additional: BTreeMap::new(),
            },
            cost_usd: not_measured(),
            additional: BTreeMap::new(),
        };

        let actual_execution = ActualExecution {
            harness_kind: HarnessKind::new(CODEX_HARNESS_KIND),
            harness_version,
            model_provider: ActualModelProvider::new(model_provider),
            model_id: ActualModelId::new(model_id),
            model_observation_source: MODEL_OBSERVATION_SOURCE.to_owned(),
            capability_snapshot: self.feature_capabilities(),
            workspace_id: DomainWorkspaceId::new(workspace_id),
            base_revision,
            started_at,
            ended_at,
            additional: BTreeMap::new(),
        };

        Ok(HarnessOutcome {
            terminal_state,
            terminal_reason,
            final_checkpoint: None,
            actual_execution,
            usage,
        })
    }

    async fn reconcile(
        &self,
        journal: &AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        let Some(process_id) = journal.process_id.as_deref() else {
            // Nothing was ever confirmed running for this attempt; there is
            // no process-liveness question left to answer.
            return Ok(RecoveryObservation::ProcessStopped);
        };
        let Some(pid) = parse_handle_pid(process_id) else {
            tracing::warn!(process_id, "codex reconcile: unrecognized handle encoding");
            return Err(HarnessError::RecoveryUnavailable);
        };

        #[cfg(unix)]
        {
            if crate::harness::process::process_alive(pid) {
                Ok(RecoveryObservation::ProcessRunning)
            } else {
                Ok(RecoveryObservation::ProcessStopped)
            }
        }
        #[cfg(not(unix))]
        {
            // Reconcile the journal only when reconciliation is genuinely
            // supported: non-Unix has no portable liveness
            // primitive here (matches `harness/process.rs`'s own documented
            // non-Unix cancellation fallback), so this is honestly reported
            // as unavailable rather than guessed.
            let _ = pid;
            Err(HarnessError::RecoveryUnavailable)
        }
    }
}

#[cfg(test)]
#[path = "codex/tests.rs"]
mod tests;
