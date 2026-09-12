//! `tack runner doctor`: reports which harness binaries this machine has,
//! what each declares it can do, and where its credentials come from —
//! without enrolling a runner or requiring a running server.
//!
//! Everything printed here comes from [`tack_runner::bootstrap::probe`], the
//! same discovery/capability-probing step a real enrollment or refresh
//! performs. There is no second, independently derived guess at what a
//! runner would report: this command and a live runner share one source of
//! truth.

use std::collections::BTreeMap;
use std::time::Duration;

use tack_orch::execution::{CapabilitySupport, CapabilityValue, HarnessCapability};
use tack_runner::{
    RunnerConfig, RunnerConfigSources, SecretStore, bootstrap, harness::process::ProcessLimits,
};

/// Irrelevant to a probe (each adapter's own `--version` call is bounded by
/// a separate, shorter, internal timeout — see e.g.
/// `harness::codex::DEFAULT_PROBE_TIMEOUT`), but `build_adapter_registry`
/// requires a value and this command never constructs a real `RunnerConfig`
/// to source one from. Mirrors `local_runner.rs`'s own
/// `HARNESS_PROCESS_LIMITS` so the two callers of the same composition root
/// stay visibly consistent.
const PROCESS_LIMITS: ProcessLimits =
    ProcessLimits::new(4 * 1024 * 1024, 1024 * 1024, Duration::from_secs(3_600));

/// The harnesses this build knows how to probe, in the fixed order the
/// report displays them — never the registry's own `BTreeMap` iteration
/// order, which is keyed by wire string and would silently reorder if a
/// kind's spelling changed.
const KNOWN_HARNESS_KINDS: [&str; 2] = ["codex", "claude-code"];

/// Runs the probe and prints the report; `as_json` switches to the raw
/// [`tack_orch::execution::RunnerCapabilities`] snapshot instead of the
/// human-readable rendering, so it can be diffed byte-for-byte against
/// whatever a real enrollment or refresh sent a server.
pub fn run(as_json: bool) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    let report = runtime.block_on(probe());

    if as_json {
        println!("{}", serde_json::to_string_pretty(&report.capabilities)?);
        return Ok(());
    }

    render(&report);
    Ok(())
}

async fn probe() -> bootstrap::DiscoveryReport {
    // A doctor run never claims or executes an attempt, so the only thing
    // this path feeds — `wait()`'s artifact-staging directory — is never
    // reached; nothing is created or written under it.
    let staging_root = std::env::temp_dir().join("tack-runner-doctor-unused-staging");
    // The real state dir a `runner start` in this same environment would
    // use (honors `--state-dir`/`TACK_RUNNER_STATE_DIR`), so the backend
    // this reports is the one a live runner would actually pick — never a
    // second, independently derived guess.
    let config = RunnerConfig::from_sources(RunnerConfigSources {
        environment: RunnerConfig::environment_overrides(),
        ..RunnerConfigSources::default()
    })
    .unwrap_or_else(|_| RunnerConfig::defaults());
    let secrets = SecretStore::open(&config.secret_store_path());
    bootstrap::probe(&staging_root, &PROCESS_LIMITS, &secrets, &config.providers).await
}

/// What this machine's probe found for one harness kind.
enum HarnessStatus<'a> {
    Present {
        version: &'a str,
    },
    /// The binary was found (and, for Codex, actually spawned) but
    /// a probe step failed — an unparseable version string, a nonzero exit,
    /// or a timed-out process. Distinct from `Absent`:
    /// this machine can find the harness, something about probing it went
    /// wrong.
    ProbeError {
        version: Option<&'a str>,
        reason: &'a str,
    },
    Absent {
        reason: &'a str,
    },
}

/// Classifies one harness kind from the raw probe output.
///
/// Codex is always registered regardless of whether its
/// binary exists (`bootstrap::build_adapter_registry`'s doc comment), so
/// its absence surfaces as a `probe_error` on its own
/// [`HarnessCapability`] entry — specifically the literal `"<name> was not
/// found on PATH"` every `*Locator::resolve` in this tree produces (see
/// `codex.rs`), which is what distinguishes it here from every
/// other probe failure (malformed output, nonzero exit, timeout) that same
/// field also carries. Claude Code is different: a
/// missing binary means `ClaudeCodeAdapter::discover` never runs at all, so
/// there is no entry to inspect — `claude_code_discovery_error` is the only
/// place that failure is recorded.
fn classify<'a>(
    kind: &str,
    harnesses: &'a [HarnessCapability],
    claude_code_discovery_error: Option<&'a str>,
) -> HarnessStatus<'a> {
    let Some(capability) = harnesses.iter().find(|h| h.harness_kind.as_str() == kind) else {
        let reason = claude_code_discovery_error.unwrap_or("harness not registered");
        return HarnessStatus::Absent { reason };
    };

    match &capability.probe_error {
        None => HarnessStatus::Present {
            version: &capability.installed_version,
        },
        Some(reason) if reason.contains("not found on PATH") => HarnessStatus::Absent { reason },
        Some(reason) => HarnessStatus::ProbeError {
            version: (!capability.installed_version.is_empty())
                .then_some(capability.installed_version.as_str()),
            reason,
        },
    }
}

fn render(report: &bootstrap::DiscoveryReport) {
    let capabilities = &report.capabilities;
    println!("Tack runner doctor — harness discovery for this machine");
    println!();

    for kind in KNOWN_HARNESS_KINDS {
        let status = classify(
            kind,
            &capabilities.harnesses,
            report.claude_code_discovery_error.as_deref(),
        );

        println!("{kind}");
        match status {
            HarnessStatus::Present { version } => {
                println!("  status:      present");
                println!("  version:     {version}");
            }
            HarnessStatus::ProbeError { version, reason } => {
                println!("  status:      present, probe error");
                if let Some(version) = version {
                    println!("  version:     {version} (partially confirmed)");
                }
                println!("  probe_error: {reason}");
            }
            HarnessStatus::Absent { reason } => {
                println!("  status:      absent");
                println!("  reason:      {reason}");
            }
        }
        println!("  credentials: {}", credential_note(kind));

        if let Some(capability) = capabilities
            .harnesses
            .iter()
            .find(|h| h.harness_kind.as_str() == kind)
        {
            render_model_info(capability);
        }
        println!();
    }

    println!("Runner-wide capabilities (apply identically to every harness above):");
    render_feature("cancel", &capabilities.features.cancel);
    render_feature("resume", &capabilities.features.resume);
    render_feature("decisions", &capabilities.features.decisions);
    render_feature("artifacts", &capabilities.features.artifacts);
    render_feature("usage", &capabilities.features.usage);
    println!();

    println!("Secret store (resolves `secret_reference` environment entries):");
    println!("  backend: {}", report.secret_backend);
    println!(
        "  note: the platform credential store gets {:?} to answer before this falls back to \
         an owner-only file; a store that is slow to start up (a Secret Service activating \
         over D-Bus, for example) can miss that window on one boot and clear it on the next, \
         so which backend answers for the same secret name is not guaranteed to stay the same \
         across restarts.",
        tack_runner::secrets::PLATFORM_STORE_TIMEOUT
    );
    println!();

    render_provider(&report.provider_catalog);

    println!(
        "Every harness above can also authenticate itself directly, using its own \
         login/credential mechanism — the server never touches that credential either way. \
         The runner is allowed to hold a provider key of its own (above) and point a harness \
         at a configured endpoint instead; see docs/adr/0050-runner-control-plane.md, \
         docs/adr/0058-standalone-single-binary-runner.md and \
         docs/adr/0061-provider-credentials-at-the-runner-boundary.md."
    );
}

/// Renders one block per provider this build knows how to configure —
/// which harnesses it reaches and, when it is on, the catalog state. Every
/// provider is walked in registry order regardless of whether it is
/// enabled, mirroring how a disabled entry was always shown before a
/// second provider existed.
fn render_provider(catalog: &BTreeMap<String, tack_runner::provider::CatalogStatus>) {
    for provider in tack_runner::provider::registry() {
        let reaches = tack_runner::provider::reaches(provider.as_ref());
        println!("Provider endpoint ({}):", provider.config_key());
        println!("  reaches: {}", reaches.join(", "));
        match catalog.get(provider.config_key()) {
            None | Some(tack_runner::provider::CatalogStatus::NotConfigured) => {
                println!("  status:  not configured");
            }
            Some(tack_runner::provider::CatalogStatus::SecretUnresolved) => {
                println!("  status:  configured, but its secret does not resolve");
            }
            Some(tack_runner::provider::CatalogStatus::Unreachable { status }) => match status {
                Some(status) => println!("  status:  catalog error (HTTP {status})"),
                None => println!("  status:  catalog error (request failed)"),
            },
            Some(tack_runner::provider::CatalogStatus::Configured {
                model_count,
                priced_model_count,
                context_window_model_count,
                checked_at,
            }) => {
                println!("  status:  configured");
                println!("  catalog: {model_count} models, checked at {checked_at}");
                println!(
                    "  price:   {priced_model_count} of {model_count} models published a price ({} Not measured)",
                    model_count - priced_model_count
                );
                println!(
                    "  limit:   {context_window_model_count} of {model_count} models published a context window ({} Not measured)",
                    model_count - context_window_model_count
                );
            }
        }
        println!();
    }
}

fn render_feature(name: &str, value: &CapabilityValue) {
    let support = support_label(value.support);
    match &value.reason {
        Some(reason) => println!("  {name:<10} {support:<11} — {reason}"),
        None => println!("  {name:<10} {support}"),
    }
}

fn render_model_info(capability: &HarnessCapability) {
    if capability.model_combinations.is_empty() {
        println!("  model_combinations: (none reported)");
    } else {
        println!("  model_combinations:");
        for combination in &capability.model_combinations {
            let models = combination
                .model_ids
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "    {} ({}): {models}",
                combination.model_provider.as_str(),
                combination.discovery
            );
        }
    }

    match &capability.model_passthrough {
        Some(value) => {
            let support = support_label(value.support);
            match &value.reason {
                Some(reason) => println!("  model_passthrough: {support} — {reason}"),
                None => println!("  model_passthrough: {support}"),
            }
        }
        None => println!("  model_passthrough: (not attested)"),
    }
}

fn support_label(support: CapabilitySupport) -> &'static str {
    match support {
        CapabilitySupport::Supported => "supported",
        CapabilitySupport::Unsupported => "unsupported",
        CapabilitySupport::Advisory => "advisory",
    }
}

/// Where each harness's provider credential actually lives, grounded in
/// that adapter's own environment-forwarding code
/// (`crates/tack-runner/src/harness/{codex,claude_code}.rs`) —
/// never a guess about which environment variable a CLI reads internally.
fn credential_note(kind: &str) -> &'static str {
    match kind {
        "codex" => {
            "Codex authenticates itself (its own CLI login flow or an API key it reads from \
             its own environment/config — see `codex --help`). Tack never reads, stores, or \
             forwards it. This adapter forwards no ambient host environment into an actual \
             run: only entries explicitly set on the execution request's own `environment` \
             field ever reach the codex process."
        }
        "claude-code" => {
            "Claude Code authenticates itself: typically an OAuth session under $HOME/.claude \
             established by its own login flow, or an API key it reads from its own \
             environment. Tack never reads, stores, or forwards it. This adapter forwards \
             only HOME and PATH from the runner process's own environment, so the installed \
             CLI can find its existing session; anything else must come through the \
             execution request's own `environment` field."
        }
        other => {
            debug_assert!(false, "unhandled harness kind {other:?}");
            "unknown harness kind"
        }
    }
}

#[cfg(test)]
#[path = "doctor/tests.rs"]
mod tests;
