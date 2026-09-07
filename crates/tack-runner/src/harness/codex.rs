//! Codex harness adapter/probe.
//!
//! Implements [`crate::harness::HarnessAdapter`] and
//! [`crate::harness::HarnessProbe`] for
//! `harness_kind = "codex"`, composing the shared process/redaction/artifact
//! infrastructure (`crate::harness::{process, redact, artifact}`).
//!
//! ## Unverified against a real binary
//!
//! Every fake-binary test below drives `crate::harness::fixtures::fake_harness_command`,
//! never a real `codex` process. Three opt-in live tests (bottom of the
//! test module) resolve a real `codex` binary from `PATH` at run time and
//! cleanly skip (print and return, no panic) when it or a further
//! precondition (an opt-in env var, a configured provider secret) is
//! absent; each is additionally `#[ignore]`d so a plain `cargo test` never
//! attempts it. Two of the three (the provider-endpoint tests) have been
//! run against the real binary and their finding is recorded at point (3)
//! below; everything else in this file specific to Codex's real CLI
//! remains a documented **guess**, not a verified fact, called out here and
//! in code comments at its point of use.
//!
//! **Unverified assumptions about Codex's real CLI contract:**
//!
//! 1. The installed binary is literally named `codex` and is found via a
//!    `PATH` search (never a hardcoded path). See [`CodexLocator`].
//! 2. Version detection invokes `codex --version` with exit code 0; the real
//!    CLI (observed: `codex-cli 0.149.1`) prefixes the version with a
//!    program-name token rather than printing it bare, so detection scans
//!    for a strict `X.Y[.Z]` numeric token anywhere in the output rather
//!    than requiring the whole line to be one. See
//!    [`CodexAdapter::detect_version`]/[`find_strict_version_token`].
//! 3. **Measured, not a guess:** non-interactive execution is
//!    `codex exec --json --model <requested model id>` with the agent
//!    profile's instructions piped over **stdin** (never argv, for the same
//!    `ps`/`/proc` exposure reason `process.rs` documents) — confirmed
//!    against the real installed binary (0.149.1), including with a
//!    provider pointed at a different endpoint via per-invocation `-c`
//!    overrides (`model_provider=...`, `model_providers.<key>.*`): the
//!    request genuinely reached that endpoint rather than Codex's built-in
//!    OpenAI provider, and no `~/.codex/config.toml` was written. See
//!    [`CodexAdapter::start`].
//! 4. Because (3) is unverified, **this adapter never attempts to parse
//!    Codex's real stdout/stderr shape.** `terminal_state` is derived solely
//!    from the child's process exit code (`0` → succeeded, nonzero/signalled
//!    → failed, killed-by-timeout → failed). It deliberately does not
//!    special-case the shared fixture's `malformed` mode into a different
//!    outcome — see [`classify_exit`] and the `malformed` test below for why
//!    that would itself be inventing a contract that cannot be verified.
//! 5. Whether/how Codex reports which model it actually used is unverified.
//!    Rather than fabricate an "observed" model, [`ActualExecution`]'s
//!    `model_provider`/`model_id` echo the **requested** selection with
//!    `model_observation_source = "requested_not_confirmed"` — a new value,
//!    not previously present in any frozen fixture (which only exemplifies
//!    `"harness_reported"`).
//! 6. Session resume, a decision/approval protocol, and parseable usage
//!    (token/cost) output are all unverified for Codex. Each is reported
//!    honestly (`unsupported`/`advisory` with a reason) rather than assumed
//!    — see [`CodexAdapter::feature_capabilities`].
//! 7. Codex's real model-discovery mechanism (if any) is unverified, so
//!    [`HarnessCapability::model_combinations`] is always empty — this
//!    adapter never hardcodes a model list: it reports capabilities
//!    without assuming models.
//!
//! ## Why `ActualExecution.model_provider`/`model_id` are non-nullable but
//! this adapter cannot always fill them honestly
//!
//! `ExecutionRequestSnapshot.requested_model_provider`/`requested_model_id`
//! are `Option<...>` — nullable when auto-selection is allowed.
//! `ActualExecution.model_provider`/`model_id` are **not** `Option`. Because
//! this adapter has no verified way to observe which model an auto-selected
//! Codex run actually used, it cannot honestly fill a non-nullable field for
//! that case without fabricating a value, which would be exactly the kind
//! of hidden fake success this codebase forbids. Rather than guess, `validate`
//! rejects a spec with no explicit `requested_model_provider`/`requested_model_id`
//! **pre-spawn** (see [`CodexAdapter::check_selection`]). This is a real,
//! falsifying observation about the frozen contract — non-nullable fields
//! this adapter cannot always honestly fill — not something this adapter
//! resolves unilaterally.

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
mod tests {
    use super::*;
    use crate::client::journal::{JournalState, WorkspaceJournal};
    use crate::client::{
        AttemptId, AttemptLease, ClaimRequestId, ClaimedWork, FencingToken, RunnerId,
        Workspace as ClientWorkspace, WorkspaceId,
    };
    use crate::harness::fixtures::fake_harness_command;
    use std::time::SystemTime;
    use tack_orch::execution::{
        AttemptSnapshot, ExecutionRequestSnapshot, HarnessKind as DomainHarnessKind,
        RequestedModelId, RequestedModelProvider,
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
            "../../../../docs/contracts/runner-v1/claim.response.json"
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
    async fn fake_binary_success_completes_succeeded_with_normalized_output_and_a_staged_artifact()
    {
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
        let (adapter, _scratch) =
            adapter_with_env(env_map(&[("TACK_FAKE_HARNESS_MODE", "malformed")]));
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

    fn enabled_gateway_providers(
        secret_name: &str,
    ) -> BTreeMap<String, crate::config::ProviderConfig> {
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
    fn env_name_dump_locator(
        workspace: &std::path::Path,
        marker: &std::path::Path,
    ) -> CodexLocator {
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
}
