//! Claude Code harness adapter.
//!
//! This file implements [`super::HarnessAdapter`] (`engine::HarnessAdapter`,
//! see the module docs on `super` for why it is not redefined here) and
//! [`super::HarnessProbe`] for the `claude` CLI
//! (Anthropic's Claude Code), version `2.1.223` observed installed on the
//! machine this was verified on.
//!
//! ## Observed vs. assumed — read this before trusting a claim below
//!
//! Every behavioral claim in this file was checked by actually invoking the
//! installed `claude` binary from a disposable fixture directory (never this
//! repository). Where a design choice rests on something *not* independently
//! invoked (e.g. the Bedrock/Vertex/Foundry provider families, confirmed
//! only by `strings` against the installed binary, never by an actual
//! provider switch), the comment at that point calls it out explicitly.
//! Nothing here should be read as "this is how Claude Code definitely
//! behaves on every machine" — only "this is what the one installed copy
//! actually did."
//!
//! Concrete findings that shaped this implementation:
//!
//! - `claude --version` prints `"<version> (Claude Code)"` to stdout, exit 0,
//!   empty stderr, and needs neither `HOME` nor `PATH` — a fast, side-effect
//!   free probe (see [`detect_version`]).
//! - `claude -p` reads the prompt from **stdin** when no positional argument
//!   is given. This adapter always uses stdin for the prompt, never argv —
//!   matching [`super::process::ProcessSpec`]'s own documented preference and
//!   keeping the prompt out of `/proc/<pid>/cmdline`.
//! - `--output-format json` (non-streaming) has **no reliable single "model
//!   used" field** — only an aggregate `modelUsage` map that, even for a
//!   single trivial prompt, included a second, unrequested internal model
//!   (`claude-haiku-4-5-...`) alongside the one actually requested. This
//!   adapter uses `--output-format stream-json --verbose` instead, and reads
//!   the authoritative model from the `{"type":"system","subtype":"init"}`
//!   event's `model` field (cross-checked against `assistant` messages).
//! - `is_error` (boolean) is the only reliable success/failure signal.
//!   `subtype` is *not*: an invalid-model 404 was observed with
//!   `"subtype":"success"` alongside `"is_error":true`. This adapter keys
//!   exclusively off `is_error`, never `subtype`.
//! - A persisted per-user settings file (`~/.claude/settings.json`,
//!   `"effortLevel"`) silently changed default behavior in a way that broke
//!   an otherwise-valid invocation (`effort 'xhigh' is not supported when
//!   thinking is disabled`) even with the process environment fully cleared.
//!   This adapter always passes an explicit `--effort high` (verified
//!   compatible across every model exercised) rather than trusting whatever
//!   default a given machine's settings file happens to carry, and passes
//!   `--setting-sources ""` to reduce ambient configuration influence over a
//!   supposedly deterministic run.
//! - Claude Code's own Bash tool runs its command in a **new session**
//!   (distinct `pgid`/`sid` from the top-level `claude` process, confirmed
//!   twice via `ps`), unlike the shared fake harness's `spawn_child` mode
//!   (whose grandchild deliberately stays in-group). A graceful SIGTERM
//!   appeared to let Claude Code clean up that detached session itself, but
//!   a SIGKILL escalation (uncatchable) cannot give it that chance, and
//!   `kill(-pgid, SIGKILL)` does not reach a different session's group. See
//!   `feature_capabilities` below for how this is reflected honestly
//!   (`cancel: Advisory`, not `Supported`) rather than papered over.
//!
//! ## What this adapter does not attempt
//!
//! - Resolving `secret_reference`-only environment entries: no secret-store
//!   client exists in this crate yet. Such entries are skipped with a
//!   `tracing::warn!` (name only) rather than silently dropped or fabricated.
//! - Enumerating installed/available models ahead of a real invocation: the
//!   CLI has no `list-models`-style command, so [`ClaudeCodeAdapter`]'s
//!   [`super::HarnessProbe::probe`] reports zero `model_combinations` rather
//!   than an unverified static alias list ("report capabilities without
//!   assuming models").
//! - Actually exercising the Bedrock/Vertex/Foundry provider paths: doing so
//!   needs real cloud credentials this adapter does not fabricate or request.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use tack_orch::execution::{
    ActualExecution, ActualModelId, ActualModelProvider, CapabilitySupport, CapabilityValue,
    FeatureCapabilities, HarnessCapability, HarnessKind as DomainHarnessKind, Measurement,
    MeasurementSource, Usage as DomainUsage, WorkspaceId as DomainWorkspaceId,
};

use super::{
    AttemptJournal, CancelObservation, CancellationEvidence, ExecutionSpec, HarnessAdapter,
    HarnessError, HarnessOutcome, HarnessProbe, LocalRunHandle, ModelObservationSource,
    RecoveryObservation,
    process::{CancelOutcome, ProcessExit, ProcessLimits, ProcessResult, ProcessSpec},
    redact::SecretMaterial,
};
// `process_alive` only exists under `#[cfg(unix)]` in `process.rs` (it shells
// out to `kill(pid, 0)`); every call site below is itself already gated the
// same way, so the import must match or a non-unix build (e.g. the Windows
// release target) fails to resolve the name at all, not just at the call.
#[cfg(unix)]
use super::process::process_alive;
use crate::{Clock, SystemClock, client::AttemptState, client::Timestamp};

/// The wire value for this harness, matching
/// `registry::HarnessKind::ClaudeCode.as_str()`.
const HARNESS_KIND: &str = "claude-code";

/// Provider families the installed `claude` 2.1.223 binary genuinely knows
/// about on its own — facts about the binary, never about how Tack is
/// configured. `"anthropic"` is the first-party default (no flag needed);
/// the other three are switched via environment variables the CLI itself
/// documents only indirectly (`--bare`'s help text names them collectively
/// as "3P providers"). Their exact names were confirmed by `strings` against
/// the installed binary (`ANTHROPIC_BEDROCK_BASE_URL`, `ANTHROPIC_VERTEX_*`,
/// `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`,
/// `CLAUDE_CODE_USE_FOUNDRY` all present) — static inspection of the shipped
/// artifact, not a live provider switch (never attempted: it would need real
/// cloud credentials that would not be fabricated for this). A
/// Tack-configured provider (Vercel's gateway, Anthropic's own API used as a
/// key+endpoint rather than the native mode above) is not one of these —
/// see [`is_known_provider`], which checks both halves without listing the
/// configured one here by name.
const NATIVE_PROVIDER_FAMILIES: &[&str] = &["anthropic", "bedrock", "vertex", "foundry"];

/// Every provider family this adapter accepts: one of the harness's own
/// native families above, or the wire name of a provider
/// `crate::provider::registry` actually knows about. The configured half is
/// never copied into a second, hand-maintained list — it is asked of the
/// registry directly, so a new `Provider` module is reachable from
/// claude-code the moment it is registered, with no second edit site here.
fn is_known_provider(name: &str) -> bool {
    NATIVE_PROVIDER_FAMILIES.contains(&name)
        || crate::provider::registry()
            .iter()
            .any(|provider| provider.wire_name() == name)
}

/// The full list [`is_known_provider`] checks against, built fresh for a
/// rejection reason — never cached, since the registry half can change
/// between builds and this is only ever assembled on the one rejected path.
fn known_provider_families() -> Vec<&'static str> {
    let mut families: Vec<&'static str> = NATIVE_PROVIDER_FAMILIES.to_vec();
    families.extend(crate::provider::registry().iter().map(|p| p.wire_name()));
    families
}

/// Tool names that touch the network, matched case-insensitively against a
/// requested `permission_policy.tools` entry. Used only to reject a
/// self-contradictory request (network denied, but a network tool allowed)
/// before spawning anything.
const NETWORK_TOOLS: &[&str] = &["webfetch", "websearch"];

/// From `docs/contracts/runner-v1/limits.json`'s `request_timeout_seconds_max`
/// (frozen; not re-read from disk here since this file may not depend on
/// contract JSON parsing, but the value itself is copied verbatim).
const MAX_TIMEOUT_SECONDS: u64 = 86_400;

/// Generous but bounded stdout/stderr caps for a real coding-assistant
/// stream-json transcript. Matches the spirit of `process.rs`'s own
/// memory-bounded capture; the exact numbers are this adapter's own choice,
/// not part of the frozen contract.
const MAX_STDOUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 4 * 1024 * 1024;

/// What to execute and any fixed leading arguments, so the same code path
/// drives either the real, absolute-resolved `claude` binary or the shared
/// fake harness (`/bin/sh <script path>`, per
/// `crate::harness::fixtures::fake_harness_command`) without a second
/// branch anywhere else in this file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessBinary {
    pub program: PathBuf,
    pub prefix_args: Vec<String>,
}

impl HarnessBinary {
    fn command_line(&self, extra_args: Vec<String>) -> (PathBuf, Vec<String>) {
        let mut args = self.prefix_args.clone();
        args.extend(extra_args);
        (self.program.clone(), args)
    }
}

/// Searches the *runner process's own* `PATH` (never an attempt-supplied
/// value), then the shared well-known install locations in
/// [`super::locate`], for an executable named `claude`. Resolved once by
/// [`ClaudeCodeAdapter::discover`]; a later uninstall is caught defensively
/// in `validate`/`start`, not by re-searching on every call.
fn discover_installed_binary() -> Result<HarnessBinary, String> {
    let program = super::locate::locate_installed("claude").map_err(|error| error.to_string())?;
    Ok(HarnessBinary {
        program,
        prefix_args: Vec::new(),
    })
}

/// One in-flight (spawned, not yet reaped) attempt process, keyed by its own
/// pid (as a string) in [`ClaudeCodeAdapter::processes`]. Everything `wait`
/// needs that only `start` has access to (the original spec has no second
/// trip through `wait`/`cancel`, which take only an opaque
/// [`LocalRunHandle`]) is captured here.
struct RunningEntry {
    process: super::process::SupervisedProcess,
    secrets: SecretMaterial,
    limits: ProcessLimits,
    requested_provider: Option<String>,
    started_at: DateTime<Utc>,
    /// Needed so `wait()` can actually stage the raw
    /// run log it claims (`artifacts: Advisory`) — `start()` is the only
    /// place these are known; `wait()` only ever sees the opaque
    /// `LocalRunHandle`.
    workspace_path: PathBuf,
    attempt_id: String,
}

/// The Claude Code harness adapter. `C` is the injected [`Clock`] — no
/// adapter method sleeps or reads `SystemTime::now()` directly; every
/// timestamp comes from `self.clock` — matching
/// `RunnerEngine<P, A, W, C = SystemClock>`'s own generic-with-default shape.
pub struct ClaudeCodeAdapter<C = SystemClock> {
    binary: HarnessBinary,
    clock: C,
    /// Grace period between SIGTERM and SIGKILL in `cancel`. A field (not a
    /// constant) so tests can shrink it; defaults to 5s, matching
    /// `process.rs::ProcessLimits`'s own default.
    cancel_grace: Duration,
    processes: tokio::sync::Mutex<BTreeMap<String, RunningEntry>>,
    /// Pids `cancel` explicitly terminated, consulted (and cleared) by
    /// `wait` if it is ever also called for the same handle — the current
    /// engine never does this in one `run_claimed` cycle (it calls either
    /// `cancel` or `wait`, never both, for a given attempt — see
    /// `engine.rs::run_claimed`), but the trait takes `&self`, not `&mut
    /// self`, and does not itself document that exclusion, so this adapter
    /// stays correct defensively rather than assuming a caller convention it
    /// cannot see from its own trait bound.
    cancelled: tokio::sync::Mutex<std::collections::BTreeSet<String>>,
    /// Resolves `secret_reference` environment entries. Shared with every
    /// other adapter the runner constructed at startup — see
    /// `crate::secrets::SecretStore`.
    secrets: crate::secrets::SecretStore,
    /// Configured provider endpoints (`RunnerConfig::providers`), consulted
    /// only when a request's `requested_model_provider` names one — see
    /// `crate::provider::resolve_endpoint`. Empty by default, meaning every
    /// request spawns against the CLI's own ambient login.
    providers: std::collections::BTreeMap<String, crate::config::ProviderConfig>,
}

impl ClaudeCodeAdapter<SystemClock> {
    /// Discovers the installed `claude` binary via the runner process's own
    /// `PATH` and constructs an adapter around it with the real system
    /// clock. The primary, non-test constructor.
    pub fn discover(secrets: crate::secrets::SecretStore) -> Result<Self, String> {
        Ok(Self::with_binary(
            discover_installed_binary()?,
            SystemClock,
            secrets,
        ))
    }

    /// Test-only: thin wrapper over the already-`pub`
    /// [`Self::with_binary`], named to match `codex.rs`'s
    /// identical `for_fixture` so `harness::mod::tests`'s "same fixture
    /// completes through both real adapters" acceptance proof can
    /// construct both adapters through one uniform call shape.
    #[cfg(test)]
    pub(crate) fn for_fixture(
        program: PathBuf,
        prefix_args: Vec<String>,
        secrets: crate::secrets::SecretStore,
    ) -> Self {
        Self::with_binary(
            HarnessBinary {
                program,
                prefix_args,
            },
            SystemClock,
            secrets,
        )
    }
}

impl<C: Clock> ClaudeCodeAdapter<C> {
    /// Constructs an adapter around an explicit [`HarnessBinary`] and clock.
    /// Used directly by tests to point at the shared fake harness fixture
    /// (`crate::harness::fixtures::fake_harness_command`) instead of a real
    /// `claude` install.
    pub fn with_binary(
        binary: HarnessBinary,
        clock: C,
        secrets: crate::secrets::SecretStore,
    ) -> Self {
        Self {
            binary,
            clock,
            cancel_grace: Duration::from_secs(5),
            processes: tokio::sync::Mutex::new(BTreeMap::new()),
            cancelled: tokio::sync::Mutex::new(std::collections::BTreeSet::new()),
            secrets,
            providers: std::collections::BTreeMap::new(),
        }
    }

    /// Overrides the SIGTERM→SIGKILL grace period used by `cancel`. Tests
    /// use a small value so a cancellation test never depends on a
    /// multi-second real sleep to pass.
    pub fn with_cancel_grace(mut self, grace: Duration) -> Self {
        self.cancel_grace = grace;
        self
    }

    /// Configures the provider endpoints this adapter may point a spawn at
    /// — see `crate::provider::resolve_endpoint`. Not part of `with_binary`
    /// itself so every existing call site (fixtures, tests) keeps
    /// constructing an adapter with no configured endpoint at all, exactly
    /// today's behavior, without editing each one.
    pub fn with_providers(
        mut self,
        providers: std::collections::BTreeMap<String, crate::config::ProviderConfig>,
    ) -> Self {
        self.providers = providers;
        self
    }

    /// Exactly `HOME` and `PATH`, read from the *runner process's own*
    /// environment (never from attempt-supplied data) — not blanket
    /// ambient-environment inheritance (which `process.rs`'s own docs flag
    /// as a rule-12 leak), but two specific, non-secret, operationally
    /// required values: `claude` needs `HOME` to find its OAuth
    /// session/config, and `PATH` if it shells out internally (observed:
    /// its Bash tool invokes a real shell). Everything else the harness
    /// needs must come through the frozen `environment` field on the
    /// request.
    fn base_environment(&self) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        if let Ok(home) = std::env::var("HOME") {
            env.insert("HOME".to_string(), home);
        }
        if let Ok(path) = std::env::var("PATH") {
            env.insert("PATH".to_string(), path);
        }
        env
    }

    /// Runs `<binary> --version` from a neutral, non-attempt directory (no
    /// workspace exists yet at probe time) with a bounded timeout, since a
    /// probe must never hang the caller forever on a broken installation.
    async fn detect_version(&self) -> (String, Option<String>) {
        let neutral_dir = std::env::temp_dir();
        let (program, args) = self.binary.command_line(vec!["--version".to_string()]);
        let spec = ProcessSpec {
            program,
            args,
            env: self.base_environment(),
            stdin: None,
            working_directory: neutral_dir.clone(),
            workspace_root: neutral_dir,
        };
        let process = match spec.spawn().await {
            Ok(process) => process,
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "claude-code adapter failed to spawn a version probe"
                );
                return (
                    String::new(),
                    Some("failed to spawn the harness binary for a version probe".to_string()),
                );
            }
        };
        let limits = ProcessLimits::new(4096, 4096, Duration::from_secs(10));
        let result = match process
            .wait_with_capture(&limits, &SecretMaterial::new())
            .await
        {
            Ok(result) => result,
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "claude-code adapter's version probe failed to complete"
                );
                return (
                    String::new(),
                    Some("version probe failed while capturing output".to_string()),
                );
            }
        };
        match result.exit {
            ProcessExit::Exited(0) => parse_version_text(&result.stdout.text),
            other => (
                String::new(),
                Some(format!("version probe exited abnormally: {other:?}")),
            ),
        }
    }

    /// Best-effort process identity check for `reconcile`: does the still-
    /// alive pid's own `argv[0]` resolve to the same program this adapter
    /// would have spawned? A bare `kill(pid, 0)` liveness check alone cannot
    /// rule out pid reuse (an unrelated process started later at the same
    /// pid); this narrows that risk without claiming certainty. Linux-only
    /// (`/proc/<pid>/cmdline`); `None` (not `Some(false)`) on every other
    /// platform, or if `/proc` cannot be read, meaning "alive, but identity
    /// unverifiable" — never conflated with "confirmed a different process."
    #[cfg(target_os = "linux")]
    fn process_program_matches(&self, pid: u32) -> Option<bool> {
        let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let mut parts = raw.split(|byte| *byte == 0).filter(|part| !part.is_empty());
        let argv0 = parts.next()?;
        let argv0_path = Path::new(std::str::from_utf8(argv0).ok()?);
        let resolved = argv0_path
            .canonicalize()
            .unwrap_or_else(|_| argv0_path.to_path_buf());
        let expected = self
            .binary
            .program
            .canonicalize()
            .unwrap_or_else(|_| self.binary.program.clone());
        Some(resolved == expected)
    }

    #[cfg(not(target_os = "linux"))]
    fn process_program_matches(&self, _pid: u32) -> Option<bool> {
        None
    }

    /// Stages the (already-scrubbed) combined stdout/stderr as a `log`
    /// artifact inside the attempt's own workspace, via
    /// [`super::artifact::ArtifactStager`] — the exact pattern
    /// `codex.rs` already proves out. `artifacts: Supported`
    /// once had no backing implementation: `wait()` never called this before.
    /// Stages under the workspace's own `.artifacts` directory, matching
    /// this adapter's live test's own choice (`ArtifactStager::new(workspace.join(".artifacts"))`)
    /// rather than a separate external staging root — `discover()` has no
    /// such root to give it. Best-effort: a staging failure only omits the
    /// `artifact` key from `terminal_reason`, never fails the attempt.
    fn stage_run_log(
        workspace_path: &std::path::Path,
        attempt_id: &str,
        stdout: &str,
        stderr: &str,
    ) -> Option<Value> {
        let relative = PathBuf::from(".tack-runner").join("claude-code-run.log");
        let absolute = workspace_path.join(&relative);
        if let Some(parent) = absolute.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            return None;
        }
        let mut combined = String::new();
        combined.push_str("=== stdout ===\n");
        combined.push_str(stdout);
        combined.push_str("\n=== stderr ===\n");
        combined.push_str(stderr);
        if std::fs::write(&absolute, combined.as_bytes()).is_err() {
            return None;
        }

        let stager = super::artifact::ArtifactStager::new(workspace_path.join(".artifacts"));
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
                tracing::warn!(?error, "claude-code wait: artifact staging failed");
                None
            }
        }
    }
}

fn now_rfc3339<C: Clock>(clock: &C) -> String {
    DateTime::<Utc>::from(clock.now()).to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_version_text(raw: &str) -> (String, Option<String>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (
            String::new(),
            Some("version probe produced no output".to_string()),
        );
    }
    let first_token = trimmed.split_whitespace().next().unwrap_or("");
    if looks_like_a_version_token(first_token) {
        (first_token.to_string(), None)
    } else {
        // Bounded: this text came from the harness's own stdout, which this
        // code path has already decided it cannot fully trust the shape of.
        let bounded: String = trimmed.chars().take(200).collect();
        (
            bounded,
            Some("installed harness reported an unrecognized version string format".to_string()),
        )
    }
}

fn looks_like_a_version_token(token: &str) -> bool {
    if !token.starts_with(|ch: char| ch.is_ascii_digit()) {
        return false;
    }
    token
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-')
}

/// A sentinel used whenever the actual model genuinely could not be
/// observed (malformed/absent structured output). Distinct from any real
/// model id Claude Code could report, and always paired with
/// `model_observation_source: "not_observed"` so a reader never mistakes it
/// for a harness-reported value.
const UNOBSERVED_MODEL: &str = "unknown";

struct ParsedRun {
    is_error: bool,
    terminal_reason: Value,
    harness_version: Option<String>,
    model_provider: String,
    model_id: String,
    model_observation_source: String,
    usage: DomainUsage,
}

fn not_measured_usage() -> DomainUsage {
    let not_measured = |value_is_none: bool| {
        let _ = value_is_none;
        MeasurementSource::NotMeasured
    };
    DomainUsage {
        tokens_in: Measurement {
            value: None,
            source: not_measured(true),
            additional: Default::default(),
        },
        tokens_out: Measurement {
            value: None,
            source: MeasurementSource::NotMeasured,
            additional: Default::default(),
        },
        duration_ms: Measurement {
            value: None,
            source: MeasurementSource::NotMeasured,
            additional: Default::default(),
        },
        cost_usd: Measurement {
            value: None,
            source: MeasurementSource::NotMeasured,
            additional: Default::default(),
        },
        additional: Default::default(),
    }
}

fn build_usage(result_value: &Value) -> DomainUsage {
    let tokens_in = result_value
        .pointer("/usage/input_tokens")
        .and_then(Value::as_u64);
    let tokens_out = result_value
        .pointer("/usage/output_tokens")
        .and_then(Value::as_u64);
    let duration_ms = result_value.get("duration_ms").and_then(Value::as_u64);
    let cost_usd = result_value.get("total_cost_usd").and_then(Value::as_f64);

    let source_for = |present: bool| {
        if present {
            MeasurementSource::Measured
        } else {
            MeasurementSource::NotMeasured
        }
    };

    DomainUsage {
        tokens_in: Measurement {
            value: tokens_in,
            source: source_for(tokens_in.is_some()),
            additional: Default::default(),
        },
        tokens_out: Measurement {
            value: tokens_out,
            source: source_for(tokens_out.is_some()),
            additional: Default::default(),
        },
        duration_ms: Measurement {
            value: duration_ms,
            source: source_for(duration_ms.is_some()),
            additional: Default::default(),
        },
        cost_usd: Measurement {
            value: cost_usd,
            source: source_for(cost_usd.is_some()),
            additional: Default::default(),
        },
        additional: Default::default(),
    }
}

fn bounded_prefix(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// Parses a terminal `{"type":"result", ...}` line (already located by
/// `parse_run_output`) into a [`ParsedRun`]. `is_error` is the sole
/// success/failure signal used — **not** `subtype`, which was directly
/// observed reporting `"success"` alongside `"is_error":true` for an
/// invalid-model API error. See the module docs.
fn parsed_from_result_line(
    result_value: &Value,
    init_model: Option<String>,
    harness_version: Option<String>,
    requested_provider: Option<&str>,
) -> ParsedRun {
    // A missing `is_error` field (never observed, but not contractually
    // guaranteed either) fails closed as an error rather than a silent
    // success.
    let is_error = result_value
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let model_provider = requested_provider
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "anthropic".to_string());
    let (model_id, model_observation_source) = match init_model {
        // This line is emitted before any network call reaches whatever
        // endpoint `model_provider` names, so it states what the CLI was
        // configured to request, not necessarily what answered. Whether
        // that distinction matters is a property of the endpoint, not of
        // any one vendor — `requires_unconfirmed_model_recording` asks the
        // matching registered provider (a name matching none, including
        // every native family above, is never unconfirmed here: that
        // question only applies to a Tack-configured endpoint).
        Some(model) if crate::provider::requires_unconfirmed_model_recording(&model_provider) => (
            model,
            ModelObservationSource::RequestedNotConfirmed
                .as_str()
                .to_string(),
        ),
        Some(model) => (
            model,
            ModelObservationSource::HarnessReported.as_str().to_string(),
        ),
        None => (
            UNOBSERVED_MODEL.to_string(),
            ModelObservationSource::NotObserved.as_str().to_string(),
        ),
    };

    ParsedRun {
        is_error,
        terminal_reason: result_value.clone(),
        harness_version,
        model_provider,
        model_id,
        model_observation_source,
        usage: build_usage(result_value),
    }
}

/// Used when the process produced *some* JSON-shaped lines (so this is not
/// the "produced nothing at all, judge purely by exit code" case) but never
/// a terminal `{"type":"result"}` object — a truncated/corrupted stream, or
/// the shared fake harness's deliberately-garbage `malformed` mode. Always
/// `Failed`, and every field that cannot honestly be known is the explicit
/// unobserved sentinel, never a fabricated value.
fn malformed_outcome(result: &ProcessResult, note: &str) -> ParsedRun {
    ParsedRun {
        is_error: true,
        terminal_reason: serde_json::json!({
            "reason": "malformed_output",
            "detail": note,
            "exit": format!("{:?}", result.exit),
            "stdout_prefix": bounded_prefix(&result.stdout.text, 500),
        }),
        harness_version: None,
        model_provider: "anthropic".to_string(),
        model_id: UNOBSERVED_MODEL.to_string(),
        model_observation_source: ModelObservationSource::NotObserved.as_str().to_string(),
        usage: not_measured_usage(),
    }
}

/// Used when the process produced **no** JSON-shaped stdout at all (empty,
/// or text that never once parsed as a JSON value — e.g. the shared fake
/// harness's generic `success`/`failure` modes, which are not shaped like
/// Claude Code's real output at all by design). The only
/// honest signal left is the raw exit code, and the resulting
/// `terminal_reason` says so explicitly rather than presenting this as a
/// fully-observed result.
fn fallback_from_exit_code(result: &ProcessResult) -> ParsedRun {
    let (is_error, note): (bool, &str) = match result.exit {
        ProcessExit::Exited(0) => (
            false,
            "no structured result envelope was produced; inferred success from exit code 0",
        ),
        ProcessExit::Exited(_) => (
            true,
            "no structured result envelope was produced; inferred failure from a non-zero exit code",
        ),
        ProcessExit::TimedOut => (
            true,
            "process exceeded its timeout with no structured result envelope",
        ),
        #[cfg(unix)]
        ProcessExit::Signaled(_) => (
            true,
            "process terminated by signal with no structured result envelope",
        ),
    };
    ParsedRun {
        is_error,
        terminal_reason: serde_json::json!({
            "reason": note,
            "exit": format!("{:?}", result.exit),
            "stderr_prefix": bounded_prefix(&result.stderr.text, 500),
        }),
        harness_version: None,
        model_provider: "anthropic".to_string(),
        model_id: UNOBSERVED_MODEL.to_string(),
        model_observation_source: ModelObservationSource::NotObserved.as_str().to_string(),
        usage: not_measured_usage(),
    }
}

/// Scans every line of captured stdout for the two `stream-json` lines this
/// adapter actually needs (`system`/`init` for the session's real model and
/// `claude_code_version`, and the terminal `result` object), tolerating and
/// simply skipping any line that fails to parse — a single corrupted line
/// must never abort parsing of an otherwise-good stream.
fn parse_run_output(result: &ProcessResult, requested_provider: Option<&str>) -> ParsedRun {
    let mut init_model: Option<String> = None;
    let mut harness_version: Option<String> = None;
    let mut result_line: Option<Value> = None;
    let mut any_json_line = false;

    for line in result.stdout.text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        any_json_line = true;
        match value.get("type").and_then(Value::as_str) {
            Some("system") if value.get("subtype").and_then(Value::as_str) == Some("init") => {
                init_model = value
                    .get("model")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                harness_version = value
                    .get("claude_code_version")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
            Some("result") => result_line = Some(value),
            _ => {}
        }
    }

    if let Some(result_value) = result_line {
        return parsed_from_result_line(
            &result_value,
            init_model,
            harness_version,
            requested_provider,
        );
    }

    if any_json_line {
        return malformed_outcome(
            result,
            "harness produced JSON output but no parseable terminal `result` object was found",
        );
    }

    fallback_from_exit_code(result)
}

/// The capability statement this adapter reports both from `probe` and
/// (unchanged, per attempt) as `ActualExecution.capability_snapshot`.
/// `cancel` is deliberately `Advisory`, not `Supported` — see the module
/// docs for the observed session-detachment finding that justifies it.
fn feature_capabilities() -> FeatureCapabilities {
    FeatureCapabilities {
        cancel: CapabilityValue {
            support: CapabilitySupport::Advisory,
            reason: Some(
                "The top-level `claude` process is always signalled reliably (it is always its \
                 own process-group leader). A Bash-tool-spawned subprocess was observed \
                 (via `ps`) running in its own session, distinct from that group; it is only \
                 guaranteed to be cleaned up if Claude Code exits gracefully within the SIGTERM \
                 grace period, since an escalation to SIGKILL is uncatchable and cannot reach a \
                 different session's process group."
                    .to_string(),
            ),
            additional: Default::default(),
        },
        resume: CapabilityValue {
            support: CapabilitySupport::Unsupported,
            reason: Some(
                "Headless (--print) invocation is a single ephemeral process with no daemon or \
                 reattachment interface. `--resume <session-id>` starts a *new* process that \
                 continues stored conversation history; that is a different guarantee than \
                 reattaching to this exact in-flight execution after a runner restart."
                    .to_string(),
            ),
            additional: Default::default(),
        },
        decisions: CapabilityValue {
            support: CapabilitySupport::Unsupported,
            reason: Some(
                "No observed mechanism for pausing headless execution to await an out-of-band \
                 decision through the runner protocol; permission prompts are resolved locally \
                 per --permission-mode, and the non-interactive trust dialog is documented as \
                 skipped entirely in --print mode."
                    .to_string(),
            ),
            additional: Default::default(),
        },
        // Downgraded from
        // `Supported`. Real `Write`/`Edit` tool output genuinely lands in
        // the workspace, but `wait()` used not to actually
        // staged anything — `stage_run_log` below closes that gap by
        // staging the raw, already-redacted stdout/stderr transcript, the
        // same thing the Codex adapter honestly calls
        // `Advisory` rather than `Supported`. No Claude-Code-specific
        // per-file artifact discovery (e.g. a real git diff of files it
        // changed) is implemented, so this adapter now reports the same
        // honest ceiling it does.
        artifacts: CapabilityValue {
            support: CapabilitySupport::Advisory,
            reason: Some(
                "Real Write/Edit tool output lands in the workspace, but only the raw, \
                 already-redacted stdout/stderr transcript is staged as a log artifact today \
                 (matching what the Codex adapter reports); no Claude-Code-specific \
                 per-file artifact discovery is implemented."
                    .to_string(),
            ),
            additional: Default::default(),
        },
        usage: CapabilityValue {
            support: CapabilitySupport::Advisory,
            reason: Some(
                "The harness reports token/cost totals, but an internal auxiliary model's \
                 usage is folded into `total_cost_usd` while the top-level `usage.input_tokens` \
                 / `output_tokens` fields appeared (directly observed) to reflect only the \
                 primary visible turn, so tokens_in/out may undercount true consumption \
                 relative to cost_usd."
                    .to_string(),
            ),
            additional: Default::default(),
        },
        additional: Default::default(),
    }
}

#[async_trait]
impl<C: Clock + Send + Sync> HarnessProbe for ClaudeCodeAdapter<C> {
    fn harness_kind(&self) -> DomainHarnessKind {
        DomainHarnessKind::new(HARNESS_KIND)
    }

    async fn probe(&self) -> HarnessCapability {
        let probed_at = DateTime::<Utc>::from(self.clock.now());
        let (installed_version, probe_error) = self.detect_version().await;
        let mut additional = BTreeMap::new();
        additional.insert(
            "model_discovery_note".to_string(),
            Value::String(
                "Claude Code's CLI has no list-models command; model availability is only \
                 observable via a live, billed invocation, so this probe reports zero \
                 model_combinations rather than an unverified static alias list."
                    .to_string(),
            ),
        );
        HarnessCapability {
            harness_kind: DomainHarnessKind::new(HARNESS_KIND),
            installed_version,
            probe_error,
            probed_at,
            model_combinations: Vec::new(),
            // A pass-through attestation: a claim about THIS adapter's
            // invocation contract (`run_arguments` appends `--model
            // <requested_model_id>` verbatim, asserted by unit test), not
            // about which models exist — the CLI validates the model itself
            // at run time and an invalid one fails the attempt with the
            // CLI's own error envelope (observed live, module docs). This is
            // what makes claude-code schedulable without inventing a model
            // list.
            model_passthrough: Some(CapabilityValue {
                support: CapabilitySupport::Supported,
                reason: Some(
                    "the adapter forwards requested_model_id verbatim via --model; the CLI \
                     validates it at run time (an invalid model returns is_error:true), so \
                     operator-specified opaque models are accepted without the probe claiming \
                     any model list"
                        .to_string(),
                ),
                additional: Default::default(),
            }),
            additional,
        }
    }

    fn declared_capabilities(&self) -> FeatureCapabilities {
        feature_capabilities()
    }
}

#[async_trait]
impl<C: Clock + Send + Sync> HarnessAdapter for ClaudeCodeAdapter<C> {
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError> {
        let request = &spec.work.request;

        if request.requested_harness_kind.as_str() != HARNESS_KIND {
            let reason = format!(
                "requested harness kind {:?} does not match this adapter's kind {HARNESS_KIND:?}",
                request.requested_harness_kind.as_str()
            );
            tracing::warn!(
                reason,
                "claude-code adapter received a spec requesting a different harness kind"
            );
            return Err(HarnessError::Rejected { reason });
        }

        if let Some(provider) = &request.requested_model_provider {
            let normalized = provider.as_str().trim().to_ascii_lowercase();
            if !is_known_provider(&normalized) {
                let known = known_provider_families();
                let reason = format!(
                    "requested model provider {:?} is not one of this adapter's known provider \
                     families {known:?}",
                    provider.as_str()
                );
                tracing::warn!(
                    reason,
                    "claude-code adapter rejected an unsupported model provider before spawn"
                );
                return Err(HarnessError::Rejected { reason });
            }
        }

        if !request.permission_policy.network {
            let requests_network_tool = request.permission_policy.tools.iter().any(|tool| {
                let lower = tool.to_ascii_lowercase();
                NETWORK_TOOLS.contains(&lower.as_str())
            });
            if requests_network_tool {
                let reason = "permission_policy denies network but names a network tool \
                               (WebFetch/WebSearch), a self-contradictory request this adapter \
                               cannot honor consistently"
                    .to_owned();
                tracing::warn!(
                    reason,
                    "claude-code adapter rejected a policy allowing a network tool while network \
                     is denied"
                );
                return Err(HarnessError::Rejected { reason });
            }
        }

        if !self.binary.program.exists() {
            let reason = format!(
                "resolved claude binary at {} no longer exists",
                self.binary.program.display()
            );
            tracing::warn!(
                reason,
                "claude-code adapter's resolved binary no longer exists"
            );
            return Err(HarnessError::Rejected { reason });
        }

        // Every `secret_reference` entry must resolve before the harness
        // process exists. This discards the resolved values — `start`
        // resolves again for real — so a rejection here never leaves a
        // running process behind. The engine has already journaled and
        // announced the attempt by now, and turns a refusal here into a
        // reported failure rather than an abandoned lease.
        super::resolve_environment(&self.secrets, request, &mut SecretMaterial::new())?;

        // Same discard-and-recheck discipline as above, for a configured
        // provider endpoint: a disabled or misconfigured provider must
        // reject here, before any process is spawned, not partway through
        // `start`.
        if let Err(error) = crate::provider::resolve_endpoint(
            &self.providers,
            &self.secrets,
            request
                .requested_model_provider
                .as_ref()
                .map(|provider| provider.as_str())
                .unwrap_or(""),
            crate::provider::Wire::AnthropicMessages,
        ) {
            let reason = error.to_string();
            tracing::warn!(
                reason,
                "claude-code adapter rejected a request whose provider endpoint could not be \
                 resolved"
            );
            return Err(HarnessError::Rejected { reason });
        }

        Ok(())
    }

    async fn start(&self, spec: &ExecutionSpec) -> Result<LocalRunHandle, HarnessError> {
        let request = &spec.work.request;
        let workspace_root = spec.workspace.path.clone();
        let working_directory = match request.repository.subdirectory.as_deref() {
            Some(subdirectory) if !subdirectory.is_empty() => workspace_root.join(subdirectory),
            _ => workspace_root.clone(),
        };

        let mut secrets = SecretMaterial::new();
        let resolved_environment =
            super::resolve_environment(&self.secrets, request, &mut secrets)?;
        let mut env = self.base_environment();
        env.extend(resolved_environment);

        // A configured provider endpoint applies only when this request's
        // provider names one (e.g. a gateway) — a direct-vendor request
        // (the harness's own subscription/login mode) resolves to `None`
        // and this adapter injects nothing, so the two paths can never be
        // confused by a shared environment variable.
        match crate::provider::resolve_endpoint(
            &self.providers,
            &self.secrets,
            request
                .requested_model_provider
                .as_ref()
                .map(|provider| provider.as_str())
                .unwrap_or(""),
            crate::provider::Wire::AnthropicMessages,
        ) {
            Ok(Some(endpoint)) => {
                env.insert("ANTHROPIC_BASE_URL".to_string(), endpoint.base_url);
                env.insert(
                    endpoint.credential_env_var,
                    endpoint.credential.expose().to_string(),
                );
                // Measured against the installed CLI (2.1.260): empty,
                // unset and non-empty all produced byte-identical outgoing
                // requests, with ANTHROPIC_AUTH_TOKEN winning regardless —
                // this contradicts the vendor's own documented claim that a
                // non-empty value wins. Set empty anyway, at zero cost,
                // rather than trusted to already be absent.
                env.insert("ANTHROPIC_API_KEY".to_string(), String::new());
            }
            Ok(None) => {}
            Err(error) => {
                return Err(HarnessError::Rejected {
                    reason: error.to_string(),
                });
            }
        }

        let tools_value = request.permission_policy.tools.join(",");

        let mut args = vec![
            "-p".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--no-session-persistence".to_string(),
            "--permission-mode".to_string(),
            "bypassPermissions".to_string(),
            "--effort".to_string(),
            "high".to_string(),
            "--setting-sources".to_string(),
            String::new(),
            "--tools".to_string(),
            tools_value,
        ];
        if let Some(model_id) = &request.requested_model_id {
            args.push("--model".to_string());
            args.push(model_id.as_str().to_string());
        }
        if let Some(budget) = request
            .budgets
            .get("cost_usd")
            .and_then(Value::as_f64)
            .filter(|value| *value > 0.0)
        {
            args.push("--max-budget-usd".to_string());
            args.push(budget.to_string());
        }

        let prompt = request.resolved_agent_profile.instructions.clone();
        let (program, args) = self.binary.command_line(args);

        let process_spec = ProcessSpec {
            program,
            args,
            env,
            stdin: Some(prompt.into_bytes()),
            working_directory,
            workspace_root,
        };

        let process = process_spec.spawn().await.map_err(|error| {
            tracing::warn!(
                ?error,
                "claude-code adapter failed to spawn the harness process"
            );
            HarnessError::Process
        })?;
        let pid = process.pid();
        let process_id = pid.to_string();

        let timeout = Duration::from_secs(request.timeout_seconds.clamp(1, MAX_TIMEOUT_SECONDS));
        let limits = ProcessLimits::new(MAX_STDOUT_BYTES, MAX_STDERR_BYTES, timeout);
        let requested_provider = request
            .requested_model_provider
            .as_ref()
            .map(|provider| provider.as_str().to_string());

        let entry = RunningEntry {
            process,
            secrets,
            limits,
            requested_provider,
            started_at: DateTime::<Utc>::from(self.clock.now()),
            workspace_path: spec.workspace.path.clone(),
            attempt_id: spec.work.lease.attempt_id.as_str().to_owned(),
        };
        self.processes
            .lock()
            .await
            .insert(process_id.clone(), entry);

        Ok(LocalRunHandle { process_id })
    }

    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancellationEvidence, HarnessError> {
        // Takes ownership of the entry rather than signalling by a raw pid
        // this adapter looks up separately: `SupervisedProcess::cancel`
        // (`process.rs`) is the only way to *reap* the process as part of
        // confirming it stopped. A pid-only `kill(pid, 0)` liveness poll
        // cannot distinguish "still running" from "exited but not yet
        // reaped" (a zombie still answers `kill(pid, 0)` successfully until
        // something calls `waitpid` on it) — an earlier version of this
        // method signalled by pid without ever reaping, and its own test
        // caught it hanging at `Ambiguous` forever because the killed
        // process was never actually reaped. Left as a documented lesson,
        // not silently fixed.
        let entry = self
            .processes
            .lock()
            .await
            .remove(&handle.process_id)
            .ok_or(HarnessError::Process)?;
        self.cancelled
            .lock()
            .await
            .insert(handle.process_id.clone());

        let pid = entry.process.pid();
        let (observation, process_outcome) = match entry.process.cancel(self.cancel_grace).await {
            Ok(CancelOutcome::Stopped) => (CancelObservation::ProcessStopped, "stopped"),
            Ok(CancelOutcome::Killed) => (CancelObservation::ProcessStopped, "killed"),
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "claude-code adapter failed to deliver a cancellation signal"
                );
                (CancelObservation::Ambiguous, "signal_failed")
            }
        };

        Ok(CancellationEvidence {
            observation,
            observed_at: Timestamp::new(now_rfc3339(&self.clock)),
            details: serde_json::Map::from_iter([
                ("pid".to_string(), Value::from(pid)),
                ("process_outcome".to_string(), Value::from(process_outcome)),
            ]),
        })
    }

    async fn wait(&self, handle: &LocalRunHandle) -> Result<HarnessOutcome, HarnessError> {
        let entry = self
            .processes
            .lock()
            .await
            .remove(&handle.process_id)
            .ok_or(HarnessError::Process)?;
        let was_cancelled = self.cancelled.lock().await.remove(&handle.process_id);

        let result = entry
            .process
            .wait_with_capture(&entry.limits, &entry.secrets)
            .await
            .map_err(|error| {
                tracing::warn!(
                    ?error,
                    "claude-code adapter failed while capturing process output"
                );
                HarnessError::Process
            })?;

        let parsed = parse_run_output(&result, entry.requested_provider.as_deref());
        let terminal_state = if was_cancelled {
            AttemptState::Cancelled
        } else if parsed.is_error {
            AttemptState::Failed
        } else {
            AttemptState::Succeeded
        };
        let ended_at = DateTime::<Utc>::from(self.clock.now());

        // `artifacts: Advisory` (downgraded from an
        // unbacked `Supported` — see `feature_capabilities`) is only honest
        // if `wait()` actually stages something. Best-effort, exactly like
        // `codex.rs`'s identical `stage_run_log`: a staging
        // failure only omits the `artifact` key, never fails the attempt.
        let mut terminal_reason = parsed.terminal_reason;
        if let Some(artifact) = Self::stage_run_log(
            &entry.workspace_path,
            &entry.attempt_id,
            &result.stdout.text,
            &result.stderr.text,
        ) && let Some(object) = terminal_reason.as_object_mut()
        {
            object.insert("artifact".to_string(), artifact);
        }

        Ok(HarnessOutcome {
            terminal_state,
            terminal_reason,
            final_checkpoint: None,
            actual_execution: ActualExecution {
                harness_kind: DomainHarnessKind::new(HARNESS_KIND),
                harness_version: parsed.harness_version.unwrap_or_default(),
                model_provider: ActualModelProvider::new(parsed.model_provider),
                model_id: ActualModelId::new(parsed.model_id),
                model_observation_source: parsed.model_observation_source,
                capability_snapshot: feature_capabilities(),
                // The engine overwrites `workspace_id`/`base_revision` from
                // the real `Workspace` via `HarnessOutcome::
                // normalize_workspace_facts` after `wait` returns
                // (`engine.rs`); these are placeholders, never reported
                // onward as-is.
                workspace_id: DomainWorkspaceId::new(""),
                base_revision: String::new(),
                started_at: entry.started_at,
                ended_at,
                additional: Default::default(),
            },
            usage: parsed.usage,
        })
    }

    async fn reconcile(
        &self,
        journal: &AttemptJournal,
    ) -> Result<RecoveryObservation, HarnessError> {
        let Some(process_id) = &journal.process_id else {
            return Ok(RecoveryObservation::ProcessStopped);
        };
        let Ok(pid) = process_id.parse::<u32>() else {
            return Err(HarnessError::RecoveryUnavailable);
        };

        #[cfg(unix)]
        {
            if !process_alive(pid) {
                return Ok(RecoveryObservation::ProcessStopped);
            }
            match self.process_program_matches(pid) {
                Some(true) => Ok(RecoveryObservation::ProcessRunning),
                // The pid is alive, but resolves to a different program: the
                // original attempt process is confirmed gone, its pid has
                // simply been recycled by the OS to something unrelated.
                Some(false) => Ok(RecoveryObservation::ProcessStopped),
                // Alive, but identity is unverifiable on this platform
                // (non-Linux Unix, or `/proc` unreadable): a bare liveness
                // check alone is not proof this is genuinely the same
                // attempt, given pid reuse. Honest uncertainty, not a
                // confident guess either way.
                None => Ok(RecoveryObservation::Ambiguous),
            }
        }
        #[cfg(not(unix))]
        {
            // No portable liveness primitive at all on this platform (see
            // `process.rs`'s own non-Unix cancellation fallback for the same
            // documented limitation). Reconciliation is not genuinely
            // supported here.
            Ok(RecoveryObservation::Ambiguous)
        }
    }
}

#[cfg(test)]
#[path = "claude_code/tests.rs"]
mod tests;
