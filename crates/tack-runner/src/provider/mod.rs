//! Runner-side provider endpoints: where a harness sends requests and which
//! credential authenticates them, when that differs from the harness's own
//! ambient login.
//!
//! Every harness this crate drives already has a working, credential-free
//! mode: the harness's own subscription/login (Claude Max, a ChatGPT plan,
//! `codex login`, ...). That mode needs nothing from this module — it is
//! simply the absence of a configured entry for the request's provider, and
//! every function here treats it as `None`/a typed absence, never a second
//! implicit case to branch on.
//!
//! What this module adds is the other mode: `RunnerConfig::providers` names
//! an entry (`[provider.<name>]`), and [`resolve_endpoint`] tells a harness
//! adapter what to inject when a request's provider matches one — a base
//! URL, the name of the environment variable that must carry the
//! credential, and the resolved credential itself. A gateway and a vendor's
//! own direct API are the same shape here — a base URL plus a credential —
//! so a second provider, gateway or direct, is a new [`Provider`]
//! implementation registered in [`registry`], never a new mechanism or a
//! branch inside an adapter, [`resolve_endpoint`] or [`attach_catalog`].
//! Providers do not share a catalog body shape, an auth header placement,
//! or a pricing shape, so each one parses its own catalog into the common
//! [`CatalogEntry`] shape; nothing in this file names a vendor.
//!
//! Two providers exist today: [`vercel_ai_gateway`] and [`anthropic`].

mod anthropic;
mod vercel_ai_gateway;

use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tack_orch::execution::{
    ModelCombination, ModelId, ModelMetadata, ModelProvider, RunnerCapabilities,
};

use crate::Clock;
use crate::config::ProviderConfig;
use crate::secrets::{SecretStore, SecretValue};

const CATALOG_TIMEOUT: Duration = Duration::from_secs(10);

/// Recorded in `ModelCombination::discovery` for a catalog-sourced entry,
/// distinct from `"reported"` — a vendor's published list, not something
/// this runner measured while actually running a task (ADR 0061 decision
/// 3).
pub const CATALOG_DISCOVERY: &str = "catalog_reported";

/// Harness kinds whose adapters apply a [`ProviderEndpoint`]. A harness
/// qualifies only if its adapter can point it at a configured endpoint
/// through per-spawn injection alone — environment variables or invocation
/// flags set for that one process. A harness that instead needs a written,
/// persistent config file (or a package loaded at its own startup) is a
/// materially different mechanism this crate does not implement, and stays
/// off this list.
const CATALOG_ELIGIBLE_HARNESSES: [&str; 2] = ["claude-code", "codex"];

/// The wire shape a harness adapter already speaks. Selects which
/// [`Provider::endpoint`] applies and how the adapter injects it —
/// environment variables for an Anthropic-Messages CLI, invocation flags
/// for an OpenAI-Responses one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wire {
    AnthropicMessages,
    OpenAiResponses,
}

/// Which [`Wire`] a catalog-eligible harness kind speaks, so [`attach_catalog`]
/// never records a model combination for a harness a provider cannot
/// actually reach — a provider need not serve every wire (the Anthropic
/// provider has no OpenAI-Responses endpoint at all).
fn wire_for_harness(harness_kind: &str) -> Option<Wire> {
    match harness_kind {
        "claude-code" => Some(Wire::AnthropicMessages),
        "codex" => Some(Wire::OpenAiResponses),
        _ => None,
    }
}

/// What an adapter must inject to point a spawn at a configured provider
/// endpoint instead of the harness's own default: where to send requests,
/// which environment variable carries the credential, its resolved value,
/// and a display label for a harness that must declare the provider under
/// a name (codex's `-c model_providers.<key>.name`).
#[derive(Debug)]
pub struct ProviderEndpoint {
    pub base_url: String,
    pub credential_env_var: String,
    pub credential: SecretValue,
    pub display_name: String,
}

/// Fixed, non-configurable facts about one provider's endpoint for one
/// wire — vendor data, never user configuration. `enabled`/`secret`
/// ([`ProviderConfig`]) are the only two knobs a `[provider.<name>]` table
/// exposes; a base URL or a credential env-var name is not one of them.
pub struct KnownEndpoint {
    pub base_url: &'static str,
    pub credential_env_var: &'static str,
}

/// One entry from a provider's own model catalog, parsed into the shape
/// every provider fills regardless of its vendor's own body shape (ADR
/// 0063 decision 5). A field the vendor's catalog does not publish is
/// `None`, never a default or a zero (decision 7). `price` holds the
/// catalog's own quoted price exactly as published, not a normalized
/// `{input, output}` pair — vendor catalogs use dozens of mutually
/// incompatible pricing shapes (tiered rates, regional variants, a literal
/// `"varies_by_provider"`), so the raw published value is the only
/// representation that does not silently falsify most of them. `modality`
/// is kept just as raw for the same reason: not every provider publishes
/// it, and the ones that do do not agree on a shape either.
#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub id: String,
    pub context_window: Option<u64>,
    pub price: Option<serde_json::Value>,
    pub modality: Option<serde_json::Value>,
}

/// Why `requested_provider` named a known endpoint but it could not be
/// resolved into a working [`ProviderEndpoint`]. Every variant names a
/// fact, never a secret value — safe inside a `HarnessError::Rejected`
/// reason or a log line.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider {0:?} has no enabled [provider.<name>] entry")]
    NotConfigured(String),
    #[error("provider {0:?} secret: {1}")]
    Secret(String, crate::secrets::SecretError),
}

/// Why a catalog request did not produce a parsed body — a status the
/// vendor's own endpoint returned, or a request that never got a response
/// at all.
#[derive(Debug)]
pub enum CatalogFetchError {
    Transport,
    Status(u16),
}

/// One provider this runner knows how to talk to in key+endpoint mode (ADR
/// 0063 decisions 1, 2 and 4). Every vendor difference — the endpoint per
/// [`Wire`], where the auth credential goes, the catalog's own body shape —
/// lives inside one implementation; [`resolve_endpoint`] and
/// [`attach_catalog`] call only these methods and never a vendor's name.
/// Adding a provider is one more [`registry`] entry and its own module.
#[async_trait]
pub trait Provider: Send + Sync {
    /// The value recorded as `ModelProvider`/`requested_model_provider` —
    /// the wire-level provider name a harness adapter's request carries.
    fn wire_name(&self) -> &'static str;

    /// The `[provider.<name>]` table name and `RunnerConfig::providers` map
    /// key for this provider.
    fn config_key(&self) -> &'static str;

    /// A human-readable label for `tack runner doctor` and
    /// [`ProviderEndpoint::display_name`].
    fn display_name(&self) -> &'static str;

    /// This provider's endpoint for `wire`, or `None` when it does not
    /// serve that wire at all.
    fn endpoint(&self, wire: Wire) -> Option<KnownEndpoint>;

    /// Whether a harness's own init/result line, once spawned against this
    /// provider's endpoint, states which model actually served the request —
    /// as opposed to only which model was requested. A harness's init line
    /// is emitted before any network call reaches this provider's endpoint,
    /// so it can only ever state what the harness was configured to
    /// request; whether that is also what answered depends on whether
    /// anything between the harness and the model can substitute one model
    /// for another. A gateway can (routing, fallback, aliasing); a vendor's
    /// own direct API cannot (it serves the requested model or the request
    /// fails, never a silent substitute). Defaults to the safe answer —
    /// `false`, unconfirmed — so a provider module that does not override
    /// this is never credited with a capability it has not proven; every
    /// provider in [`registry`] states its own value explicitly rather than
    /// inheriting the default silently.
    fn confirms_served_model_from_init_line(&self) -> bool {
        false
    }

    /// Fetches and parses this provider's own model catalog, using
    /// whatever auth placement and body shape its vendor requires.
    async fn fetch_catalog(
        &self,
        secret: &SecretValue,
    ) -> Result<Vec<CatalogEntry>, CatalogFetchError>;
}

/// Every provider this build knows about, in the fixed order `tack runner
/// doctor` displays them. Nothing outside this module constructs a
/// [`Provider`]; a caller only ever walks this list.
pub fn registry() -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(vercel_ai_gateway::VercelAiGateway),
        Box::new(anthropic::Anthropic),
    ]
}

/// The catalog-eligible harness kinds this provider's endpoints actually
/// reach, in [`CATALOG_ELIGIBLE_HARNESSES`] order — for `tack runner
/// doctor`'s own rendering.
pub fn reaches(provider: &dyn Provider) -> Vec<&'static str> {
    CATALOG_ELIGIBLE_HARNESSES
        .iter()
        .filter(|harness_kind| {
            wire_for_harness(harness_kind).is_some_and(|wire| provider.endpoint(wire).is_some())
        })
        .copied()
        .collect()
}

/// Whether a harness adapter parsing its own init/result line for a request
/// naming `requested_provider` must record the model as
/// `requested_not_confirmed` rather than `harness_reported` — `true` only
/// when `requested_provider` names a registered provider whose own
/// [`Provider::confirms_served_model_from_init_line`] is `false`. A name
/// matching no registered provider (a harness's own native vendor family,
/// or an unconfigured/unknown string) is never in question here: those
/// paths already record `harness_reported` unconditionally, unaffected by
/// this function.
pub fn requires_unconfirmed_model_recording(requested_provider: &str) -> bool {
    registry()
        .into_iter()
        .find(|provider| provider.wire_name() == requested_provider)
        .is_some_and(|provider| !provider.confirms_served_model_from_init_line())
}

/// Resolves what `requested_provider` needs injected for `wire`, or `None`
/// when `requested_provider` names no known endpoint for that wire at all —
/// the harness's own subscription/login mode, where an adapter must inject
/// nothing.
pub fn resolve_endpoint(
    providers: &BTreeMap<String, ProviderConfig>,
    secrets: &SecretStore,
    requested_provider: &str,
    wire: Wire,
) -> Result<Option<ProviderEndpoint>, ProviderError> {
    let Some(provider) = registry()
        .into_iter()
        .find(|candidate| candidate.wire_name() == requested_provider)
    else {
        return Ok(None);
    };
    let Some(known) = provider.endpoint(wire) else {
        return Ok(None);
    };
    let config = providers
        .get(provider.config_key())
        .filter(|config| config.enabled)
        .ok_or_else(|| ProviderError::NotConfigured(requested_provider.to_owned()))?;
    let credential = secrets
        .resolve(&config.secret)
        .map_err(|error| ProviderError::Secret(requested_provider.to_owned(), error))?;
    tracing::debug!(
        provider = requested_provider,
        secret = %config.secret,
        "provider endpoint resolved for spawn injection"
    );
    Ok(Some(ProviderEndpoint {
        base_url: known.base_url.to_owned(),
        credential_env_var: known.credential_env_var.to_owned(),
        credential,
        display_name: provider.display_name().to_owned(),
    }))
}

/// What asking one configured provider for its model catalog produced — a
/// typed absence for every non-catalog outcome, matching
/// `bootstrap::build_adapter_registry`'s own posture toward a harness that
/// cannot be discovered: never a stale or placeholder list.
#[derive(Debug, Clone)]
pub enum CatalogStatus {
    /// No enabled `[provider.<name>]` entry exists for this provider.
    NotConfigured,
    /// The entry is enabled but its secret does not resolve.
    SecretUnresolved,
    /// The catalog endpoint answered a non-success status, or could not be
    /// reached at all (`status: None`).
    Unreachable { status: Option<u16> },
    /// The catalog request succeeded. `priced_model_count` and
    /// `context_window_model_count` count only entries that published that
    /// field (ADR 0063 decision 7) — never inferred, never a default for
    /// the remainder.
    Configured {
        model_count: usize,
        priced_model_count: usize,
        context_window_model_count: usize,
        checked_at: DateTime<Utc>,
    },
}

/// Fetches every enabled provider's model catalog and, on success, records
/// one [`ModelCombination`] per catalog-eligible harness that provider's
/// endpoint actually reaches — never inventing an entry for a harness this
/// machine did not probe, or for a wire this provider does not serve.
/// Returns one [`CatalogStatus`] per provider, keyed by
/// [`Provider::config_key`]; a provider whose secret does not resolve must
/// not suppress another provider's catalog. Called from both
/// `bootstrap::build_runtime` and `bootstrap::probe`, so a real
/// enrollment/refresh snapshot and a `tack runner doctor --json` run share
/// this one code path rather than two that could quietly diverge.
pub async fn attach_catalog<C: Clock>(
    capabilities: &mut RunnerCapabilities,
    providers: &BTreeMap<String, ProviderConfig>,
    secrets: &SecretStore,
    clock: &C,
) -> BTreeMap<String, CatalogStatus> {
    attach_catalog_to(registry(), capabilities, providers, secrets, clock).await
}

/// The body of [`attach_catalog`], parameterized over the provider list so
/// tests can exercise the orchestration guarantee (one provider's failure
/// never suppresses another's) against fakes, with no network involved.
async fn attach_catalog_to<C: Clock>(
    providers_registry: Vec<Box<dyn Provider>>,
    capabilities: &mut RunnerCapabilities,
    providers: &BTreeMap<String, ProviderConfig>,
    secrets: &SecretStore,
    clock: &C,
) -> BTreeMap<String, CatalogStatus> {
    let mut statuses = BTreeMap::new();
    for provider in providers_registry {
        let status =
            attach_one_catalog(provider.as_ref(), capabilities, providers, secrets, clock).await;
        statuses.insert(provider.config_key().to_owned(), status);
    }
    statuses
}

async fn attach_one_catalog<C: Clock>(
    provider: &dyn Provider,
    capabilities: &mut RunnerCapabilities,
    providers: &BTreeMap<String, ProviderConfig>,
    secrets: &SecretStore,
    clock: &C,
) -> CatalogStatus {
    let Some(config) = providers
        .get(provider.config_key())
        .filter(|config| config.enabled)
    else {
        return CatalogStatus::NotConfigured;
    };
    let secret = match secrets.resolve(&config.secret) {
        Ok(secret) => secret,
        Err(_) => return CatalogStatus::SecretUnresolved,
    };
    tracing::debug!(secret = %config.secret, "provider secret resolved for catalog fetch");
    let entries = match provider.fetch_catalog(&secret).await {
        Ok(entries) => entries,
        Err(CatalogFetchError::Status(status)) => {
            tracing::warn!(status, "provider catalog fetch rejected");
            return CatalogStatus::Unreachable {
                status: Some(status),
            };
        }
        Err(CatalogFetchError::Transport) => {
            tracing::warn!("provider catalog fetch failed: transport error");
            return CatalogStatus::Unreachable { status: None };
        }
    };
    tracing::debug!(model_count = entries.len(), "provider catalog fetched");

    let checked_at = DateTime::<Utc>::from(clock.now());
    let model_ids: Vec<ModelId> = entries
        .iter()
        .map(|entry| ModelId::new(entry.id.clone()))
        .collect();
    // Only models the catalog actually said something about get an entry —
    // a model absent from this map is one the provider published nothing
    // for, never a claim of zero (ADR 0063 decision 7).
    let model_metadata: BTreeMap<ModelId, ModelMetadata> = entries
        .iter()
        .filter(|entry| {
            entry.context_window.is_some() || entry.price.is_some() || entry.modality.is_some()
        })
        .map(|entry| {
            (
                ModelId::new(entry.id.clone()),
                ModelMetadata {
                    context_window: entry.context_window,
                    price: entry.price.clone(),
                    modality: entry.modality.clone(),
                    additional: Default::default(),
                },
            )
        })
        .collect();
    for harness in capabilities.harnesses.iter_mut() {
        if !CATALOG_ELIGIBLE_HARNESSES.contains(&harness.harness_kind.as_str()) {
            continue;
        }
        let Some(wire) = wire_for_harness(harness.harness_kind.as_str()) else {
            continue;
        };
        if provider.endpoint(wire).is_none() {
            continue;
        }
        harness.model_combinations.push(ModelCombination {
            model_provider: ModelProvider::new(provider.wire_name()),
            model_ids: model_ids.clone(),
            discovery: CATALOG_DISCOVERY.to_owned(),
            model_metadata: model_metadata.clone(),
            additional: Default::default(),
        });
    }
    CatalogStatus::Configured {
        model_count: entries.len(),
        priced_model_count: entries.iter().filter(|entry| entry.price.is_some()).count(),
        context_window_model_count: entries
            .iter()
            .filter(|entry| entry.context_window.is_some())
            .count(),
        checked_at,
    }
}

#[cfg(test)]
mod tests {
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
}
