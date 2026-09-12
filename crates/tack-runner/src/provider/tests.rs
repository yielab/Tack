use super::*;
use crate::config::{VERCEL_AI_GATEWAY_CONFIG_KEY, VERCEL_AI_GATEWAY_PROVIDER};

fn providers(enabled: bool, secret: &str) -> BTreeMap<String, ProviderConfig> {
    BTreeMap::from([(
        VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
        ProviderConfig {
            enabled,
            secret: secret.to_owned(),
        },
    )])
}

#[test]
fn a_direct_vendor_provider_resolves_to_no_endpoint_at_all() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let secrets = SecretStore::file(dir.path().join("secrets.json"));
    let result = resolve_endpoint(
        &providers(true, "demo"),
        &secrets,
        "anthropic-max-subscription",
        Wire::AnthropicMessages,
    );
    assert!(matches!(result, Ok(None)));
}

#[test]
fn a_disabled_provider_is_a_typed_not_configured_error() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let secrets = SecretStore::file(dir.path().join("secrets.json"));
    let result = resolve_endpoint(
        &providers(false, "demo"),
        &secrets,
        VERCEL_AI_GATEWAY_PROVIDER,
        Wire::AnthropicMessages,
    );
    assert!(
        matches!(result, Err(ProviderError::NotConfigured(name)) if name == VERCEL_AI_GATEWAY_PROVIDER)
    );
}

#[test]
fn an_enabled_provider_with_no_such_secret_is_a_typed_secret_error() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let secrets = SecretStore::file(dir.path().join("secrets.json"));
    let result = resolve_endpoint(
        &providers(true, "does-not-exist"),
        &secrets,
        VERCEL_AI_GATEWAY_PROVIDER,
        Wire::AnthropicMessages,
    );
    assert!(
        matches!(result, Err(ProviderError::Secret(name, _)) if name == VERCEL_AI_GATEWAY_PROVIDER)
    );
}

#[test]
fn a_configured_provider_resolves_the_wire_specific_endpoint() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("secrets.json");
    let secrets = SecretStore::file(path.clone());
    secrets
        .set("demo-secret", "the-real-key")
        .expect("seed secret");

    let claude = resolve_endpoint(
        &providers(true, "demo-secret"),
        &secrets,
        VERCEL_AI_GATEWAY_PROVIDER,
        Wire::AnthropicMessages,
    )
    .expect("resolves")
    .expect("endpoint present");
    assert_eq!(claude.base_url, "https://ai-gateway.vercel.sh/claude-code");
    assert_eq!(claude.credential_env_var, "ANTHROPIC_AUTH_TOKEN");
    assert_eq!(claude.credential.expose(), "the-real-key");

    let codex = resolve_endpoint(
        &providers(true, "demo-secret"),
        &secrets,
        VERCEL_AI_GATEWAY_PROVIDER,
        Wire::OpenAiResponses,
    )
    .expect("resolves")
    .expect("endpoint present");
    assert_eq!(codex.base_url, "https://ai-gateway.vercel.sh/codex/v1");
    assert_eq!(codex.credential_env_var, "AI_GATEWAY_API_KEY");
}

#[test]
fn credential_is_never_visible_through_debug() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("secrets.json");
    let store = SecretStore::file(path.clone());
    store
        .set("demo-secret", "must-never-be-printed")
        .expect("seed secret");
    let endpoint = resolve_endpoint(
        &providers(true, "demo-secret"),
        &store,
        VERCEL_AI_GATEWAY_PROVIDER,
        Wire::AnthropicMessages,
    )
    .expect("resolves")
    .expect("endpoint present");

    assert!(!format!("{endpoint:?}").contains("must-never-be-printed"));
}

#[test]
fn the_registry_carries_both_providers_and_anthropic_serves_only_the_anthropic_messages_wire() {
    let names: Vec<&'static str> = registry().iter().map(|p| p.wire_name()).collect();
    assert!(names.contains(&"vercel-ai-gateway"));
    assert!(names.contains(&"anthropic-direct"));

    let anthropic = registry()
        .into_iter()
        .find(|p| p.wire_name() == "anthropic-direct")
        .expect("anthropic is registered");
    assert!(anthropic.endpoint(Wire::AnthropicMessages).is_some());
    assert!(
        anthropic.endpoint(Wire::OpenAiResponses).is_none(),
        "Anthropic's own API has no OpenAI-Responses endpoint"
    );
}

#[test]
fn resolve_endpoint_reaches_the_anthropic_provider_too() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let path = dir.path().join("secrets.json");
    let secrets = SecretStore::file(path);
    secrets
        .set("anthropic-secret", "sk-ant-demo")
        .expect("seed secret");
    let providers = BTreeMap::from([(
        "anthropic".to_owned(),
        ProviderConfig {
            enabled: true,
            secret: "anthropic-secret".to_owned(),
        },
    )]);

    let endpoint = resolve_endpoint(
        &providers,
        &secrets,
        "anthropic-direct",
        Wire::AnthropicMessages,
    )
    .expect("resolves")
    .expect("endpoint present");
    assert_eq!(endpoint.credential_env_var, "ANTHROPIC_API_KEY");
    assert_eq!(endpoint.credential.expose(), "sk-ant-demo");

    assert!(matches!(
        resolve_endpoint(
            &providers,
            &secrets,
            "anthropic-direct",
            Wire::OpenAiResponses
        ),
        Ok(None)
    ));
}

/// Test-only [`Provider`] with no network involved — the fixed catalog
/// (or fetch failure) it is built with is simply returned. Proves
/// [`attach_catalog_to`]'s own orchestration guarantee independent of
/// either real provider's HTTP behaviour, which is covered separately
/// by each provider module's own body-parsing tests and, for Vercel,
/// a live fetch.
struct FakeProvider {
    config_key: &'static str,
    wire_name: &'static str,
    result: FakeResult,
}

enum FakeResult {
    Entries(Vec<CatalogEntry>),
    SecretNeverResolves,
}

#[async_trait]
impl Provider for FakeProvider {
    fn wire_name(&self) -> &'static str {
        self.wire_name
    }

    fn config_key(&self) -> &'static str {
        self.config_key
    }

    fn display_name(&self) -> &'static str {
        self.config_key
    }

    fn endpoint(&self, _wire: Wire) -> Option<KnownEndpoint> {
        Some(KnownEndpoint {
            base_url: "https://example.invalid",
            credential_env_var: "FAKE_TOKEN",
        })
    }

    async fn fetch_catalog(
        &self,
        _secret: &SecretValue,
    ) -> Result<Vec<CatalogEntry>, CatalogFetchError> {
        match &self.result {
            FakeResult::Entries(entries) => Ok(entries.clone()),
            // `attach_one_catalog` never calls `fetch_catalog` for a
            // secret that does not resolve — reaching this would be
            // the test's own bug, not the mechanism's.
            FakeResult::SecretNeverResolves => {
                panic!("fetch_catalog must not be called when the secret does not resolve")
            }
        }
    }
}

fn empty_capabilities() -> RunnerCapabilities {
    let advisory_off = tack_orch::execution::CapabilityValue {
        support: tack_orch::execution::CapabilitySupport::Unsupported,
        reason: None,
        additional: Default::default(),
    };
    RunnerCapabilities {
        protocol_version: None,
        runner_version: "0.0.0-test".to_owned(),
        reported_at: Utc::now(),
        labels: Default::default(),
        concurrency: tack_orch::execution::Concurrency {
            total: 1,
            available: 1,
            additional: Default::default(),
        },
        harnesses: vec![tack_orch::execution::HarnessCapability {
            harness_kind: tack_orch::execution::HarnessKind::new("claude-code"),
            installed_version: "1.0.0".to_owned(),
            probe_error: None,
            probed_at: Utc::now(),
            model_combinations: Vec::new(),
            model_passthrough: None,
            additional: Default::default(),
        }],
        features: tack_orch::execution::FeatureCapabilities {
            cancel: advisory_off.clone(),
            resume: advisory_off.clone(),
            decisions: advisory_off.clone(),
            artifacts: advisory_off.clone(),
            usage: advisory_off,
            additional: Default::default(),
        },
        limits: tack_orch::execution::CapabilityLimits {
            event_payload_bytes_max: 1,
            artifact_content_bytes_max: 1,
            additional: Default::default(),
        },
        additional: Default::default(),
    }
}

#[tokio::test]
async fn one_providers_unresolvable_secret_never_suppresses_the_others_catalog() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let secrets = SecretStore::file(dir.path().join("secrets.json"));
    secrets.set("working-secret", "token").expect("seed secret");

    let providers = BTreeMap::from([
        (
            "broken".to_owned(),
            ProviderConfig {
                enabled: true,
                secret: "no-such-secret".to_owned(),
            },
        ),
        (
            "working".to_owned(),
            ProviderConfig {
                enabled: true,
                secret: "working-secret".to_owned(),
            },
        ),
    ]);
    let registry: Vec<Box<dyn Provider>> = vec![
        Box::new(FakeProvider {
            config_key: "broken",
            wire_name: "broken-vendor",
            result: FakeResult::SecretNeverResolves,
        }),
        Box::new(FakeProvider {
            config_key: "working",
            wire_name: "working-vendor",
            result: FakeResult::Entries(vec![CatalogEntry {
                id: "working-vendor/model-1".to_owned(),
                context_window: Some(128_000),
                price: Some(serde_json::json!({"input": "0.000001"})),
                modality: Some(serde_json::json!({"input": ["text"], "output": ["text"]})),
            }]),
        }),
    ];

    let mut capabilities = empty_capabilities();
    let statuses = attach_catalog_to(
        registry,
        &mut capabilities,
        &providers,
        &secrets,
        &crate::clock::SystemClock,
    )
    .await;

    assert!(matches!(
        statuses.get("broken"),
        Some(CatalogStatus::SecretUnresolved)
    ));
    match statuses.get("working") {
        Some(CatalogStatus::Configured {
            model_count,
            priced_model_count,
            context_window_model_count,
            ..
        }) => {
            assert_eq!(*model_count, 1);
            assert_eq!(*priced_model_count, 1);
            assert_eq!(*context_window_model_count, 1);
        }
        other => panic!("expected the working provider's catalog to arrive, got {other:?}"),
    }

    let claude_code = capabilities
        .harnesses
        .iter()
        .find(|h| h.harness_kind.as_str() == "claude-code")
        .expect("claude-code harness present");
    assert_eq!(claude_code.model_combinations.len(), 1);
    assert_eq!(
        claude_code.model_combinations[0].model_provider.as_str(),
        "working-vendor"
    );
    let metadata = claude_code.model_combinations[0]
        .model_metadata
        .get(&tack_orch::execution::ModelId::new(
            "working-vendor/model-1",
        ))
        .expect("the one fetched model's metadata must be attached to the combination");
    assert_eq!(metadata.context_window, Some(128_000));
    assert!(metadata.price.is_some());
    assert!(metadata.modality.is_some());
}
