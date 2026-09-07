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
mod tests {
    use super::*;
    use tack_orch::execution::{HarnessKind, ModelCombination, ModelId, ModelProvider};

    fn fixed_timestamp() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
            .expect("fixture timestamp")
            .into()
    }

    fn capability(
        kind: &str,
        installed_version: &str,
        probe_error: Option<&str>,
    ) -> HarnessCapability {
        HarnessCapability {
            harness_kind: HarnessKind::new(kind),
            installed_version: installed_version.to_owned(),
            probe_error: probe_error.map(str::to_owned),
            probed_at: fixed_timestamp(),
            model_combinations: Vec::new(),
            model_passthrough: None,
            additional: Default::default(),
        }
    }

    #[test]
    fn a_healthy_probe_is_present_with_its_real_version() {
        let harnesses = vec![capability("codex", "0.149.1", None)];
        let status = classify("codex", &harnesses, None);
        assert!(matches!(
            status,
            HarnessStatus::Present { version: "0.149.1" }
        ));
    }

    /// The literal wording `CodexLocator::resolve`'s own
    /// locator produces for "never found on PATH" — this is what doctor must
    /// recognize as absence, not a probe error.
    #[test]
    fn a_binary_never_found_on_path_is_absent_not_a_probe_error() {
        let harnesses = vec![capability(
            "codex",
            "",
            Some("`codex` was not found on PATH"),
        )];
        let status = classify("codex", &harnesses, None);
        assert!(
            matches!(status, HarnessStatus::Absent { reason } if reason.contains("not found on PATH"))
        );
    }

    /// Proves the acceptance-critical distinction: a binary that IS found
    /// and spawned (so the process definitely exists on this machine), but
    /// whose `--version` output doesn't parse, must never be reported as
    /// "absent" — it is a probe error against a present binary. Mirrors
    /// `codex.rs`'s own `probe_reports_an_unrecognized_version_string_as_an_
    /// explicit_probe_error` fixture wording.
    #[test]
    fn a_present_binary_with_unparseable_version_output_is_a_probe_error_not_absent() {
        let harnesses = vec![capability(
            "codex",
            "",
            Some("codex --version output was not a recognizable version string"),
        )];
        let status = classify("codex", &harnesses, None);
        assert!(matches!(
            status,
            HarnessStatus::ProbeError { version: None, .. }
        ));
    }

    /// A second, independent proof of the same distinction: a harness that
    /// confirms a real installed version and still fails a later probe step
    /// must render as "present" with a version, plus a distinct probe
    /// error, never collapse into "absent".
    #[test]
    fn a_present_binary_with_a_later_probe_failure_keeps_its_confirmed_version() {
        let harnesses = vec![capability(
            "future-harness",
            "1.18.0",
            Some(
                "installed_version 1.18.0 confirmed; provider/model enumeration failed \
                 (see additional.model_listing_error)",
            ),
        )];
        let status = classify("future-harness", &harnesses, None);
        assert!(matches!(
            status,
            HarnessStatus::ProbeError {
                version: Some("1.18.0"),
                ..
            }
        ));
    }

    /// Claude Code's own discovery failure never produces a
    /// `HarnessCapability` entry at all (see `classify`'s doc comment) —
    /// this proves doctor still reports it as absent rather than silently
    /// omitting it because the entry doesn't exist.
    #[test]
    fn claude_code_missing_from_the_harness_list_is_reported_absent_using_its_own_discovery_error()
    {
        let harnesses: Vec<HarnessCapability> = Vec::new();
        let status = classify(
            "claude-code",
            &harnesses,
            Some("no executable named `claude` was found on PATH"),
        );
        assert!(matches!(
            status,
            HarnessStatus::Absent { reason } if reason.contains("no executable named")
        ));
    }

    #[test]
    fn claude_code_registered_and_healthy_is_present() {
        let harnesses = vec![capability("claude-code", "2.1.252 (Claude Code)", None)];
        let status = classify(
            "claude-code",
            &harnesses,
            None, // discovery succeeded, so there is no error to carry
        );
        assert!(matches!(status, HarnessStatus::Present { .. }));
    }

    #[test]
    fn every_known_harness_kind_has_a_credential_note() {
        for kind in KNOWN_HARNESS_KINDS {
            assert_ne!(credential_note(kind), "unknown harness kind");
        }
    }

    /// `render`/`render_model_info` are exercised for their side effects
    /// (stdout), not a return value; this only proves they run to
    /// completion for a shape with real model combinations, so a future
    /// field addition to `ModelCombination` fails loudly here instead of
    /// only in a manual `--json` read.
    #[test]
    fn render_does_not_panic_on_a_populated_report() {
        let mut with_models = capability("codex", "1.18.0", None);
        with_models.model_combinations = vec![ModelCombination {
            model_provider: ModelProvider::new("openai"),
            model_ids: vec![ModelId::new("grok-code")],
            discovery: "codex models".to_owned(),
            model_metadata: Default::default(),
            additional: Default::default(),
        }];
        with_models.model_passthrough = Some(CapabilityValue {
            support: CapabilitySupport::Unsupported,
            reason: Some("declaration-based only".to_owned()),
            additional: Default::default(),
        });

        let report = bootstrap::DiscoveryReport {
            capabilities: tack_orch::execution::RunnerCapabilities {
                protocol_version: None,
                runner_version: "0.0.0-test".to_owned(),
                reported_at: fixed_timestamp(),
                labels: Default::default(),
                concurrency: tack_orch::execution::Concurrency {
                    total: 1,
                    available: 1,
                    additional: Default::default(),
                },
                harnesses: vec![with_models],
                features: tack_orch::execution::FeatureCapabilities {
                    cancel: CapabilityValue {
                        support: CapabilitySupport::Advisory,
                        reason: None,
                        additional: Default::default(),
                    },
                    resume: CapabilityValue {
                        support: CapabilitySupport::Unsupported,
                        reason: None,
                        additional: Default::default(),
                    },
                    decisions: CapabilityValue {
                        support: CapabilitySupport::Unsupported,
                        reason: None,
                        additional: Default::default(),
                    },
                    artifacts: CapabilityValue {
                        support: CapabilitySupport::Advisory,
                        reason: None,
                        additional: Default::default(),
                    },
                    usage: CapabilityValue {
                        support: CapabilitySupport::Advisory,
                        reason: None,
                        additional: Default::default(),
                    },
                    additional: Default::default(),
                },
                limits: tack_orch::execution::CapabilityLimits {
                    event_payload_bytes_max: 1,
                    artifact_content_bytes_max: 1,
                    additional: Default::default(),
                },
                additional: Default::default(),
            },
            claude_code_discovery_error: Some(
                "no executable named `claude` was found on PATH".to_owned(),
            ),
            secret_backend: tack_runner::secrets::SecretBackendKind::File,
            provider_catalog: BTreeMap::from([(
                tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
                tack_runner::provider::CatalogStatus::Configured {
                    model_count: 373,
                    priced_model_count: 352,
                    context_window_model_count: 355,
                    checked_at: fixed_timestamp(),
                },
            )]),
        };

        render(&report);
    }

    /// `render_provider` is exercised for its side effects (stdout), not a
    /// return value; this only proves every `CatalogStatus` variant renders
    /// to completion, so a future variant addition fails loudly here
    /// instead of only in a manual `--json` read. Also proves a provider
    /// this build knows about but the map has no entry for (an empty map)
    /// renders as "not configured" instead of panicking on a missing key.
    #[test]
    fn render_provider_does_not_panic_for_any_catalog_status() {
        render_provider(&BTreeMap::new());
        for status in [
            tack_runner::provider::CatalogStatus::NotConfigured,
            tack_runner::provider::CatalogStatus::SecretUnresolved,
            tack_runner::provider::CatalogStatus::Unreachable { status: Some(401) },
            tack_runner::provider::CatalogStatus::Unreachable { status: None },
            tack_runner::provider::CatalogStatus::Configured {
                model_count: 373,
                priced_model_count: 352,
                context_window_model_count: 355,
                checked_at: fixed_timestamp(),
            },
        ] {
            let mut catalog = BTreeMap::new();
            for provider in tack_runner::provider::registry() {
                catalog.insert(provider.config_key().to_owned(), status.clone());
            }
            render_provider(&catalog);
        }
    }
}
