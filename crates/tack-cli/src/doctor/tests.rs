use super::*;
use tack_orch::execution::{HarnessKind, ModelCombination, ModelId, ModelProvider};

fn fixed_timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
        .expect("fixture timestamp")
        .into()
}

fn capability(kind: &str, installed_version: &str, probe_error: Option<&str>) -> HarnessCapability {
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
fn claude_code_missing_from_the_harness_list_is_reported_absent_using_its_own_discovery_error() {
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
