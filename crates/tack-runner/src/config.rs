use std::{collections::BTreeMap, fmt, path::PathBuf};

use serde::Deserialize;

use crate::{ConfigError, RunnerError};

pub const DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:3210/api/runner/v1";
pub const DEFAULT_RUNNER_ID: &str = "local-runner";
pub const DEFAULT_STATE_DIR: &str = ".tack-runner";

/// The provider name recorded everywhere this system records or requests a
/// model's route (`ModelProvider`, `requested_model_provider`, a catalog
/// entry's `discovery`) — ADR 0061 decision 4.
pub const VERCEL_AI_GATEWAY_PROVIDER: &str = "vercel-ai-gateway";

/// The `[provider.<name>]` table name and `RunnerConfig::providers` map key
/// for the provider above. Kept distinct from [`VERCEL_AI_GATEWAY_PROVIDER`]
/// because a config section header and a wire-level provider name follow
/// different spelling conventions in this project (underscore vs. hyphen).
pub const VERCEL_AI_GATEWAY_CONFIG_KEY: &str = "vercel_ai_gateway";

/// Default `secret` entry name for the provider above — the runner-local
/// secret store name a fresh install resolves with no configuration.
/// `SecretStore::resolve` does not append `/default` on its own, so this
/// must spell the full entry name.
pub const DEFAULT_VERCEL_AI_GATEWAY_SECRET: &str = "vercel-ai-gateway/default";

/// The `[provider.<name>]`/`RunnerConfig::providers` key for Anthropic's
/// own API (ADR 0063 decisions 2 and 4: a vendor's own API and a gateway
/// are the same key+endpoint mode).
pub const ANTHROPIC_CONFIG_KEY: &str = "anthropic";

/// The value recorded as `ModelProvider`/`requested_model_provider` when a
/// request wants this provider's *configured, Tack-managed* endpoint.
/// Deliberately not the bare string `"anthropic"`: that string is already
/// claude-code's own native vendor-family identifier (its default when a
/// request names no provider at all — see `claude_code.rs`'s own
/// `KNOWN_PROVIDER_FAMILIES`-style vocabulary), meaning a request can
/// legitimately name "anthropic" as its target vendor while still wanting
/// the harness's own ambient subscription, not this runner's injected key.
/// A distinct wire name keeps those two requests distinguishable, exactly
/// as `VERCEL_AI_GATEWAY_PROVIDER` already is not a vendor family a harness
/// would ever produce on its own.
pub const ANTHROPIC_PROVIDER: &str = "anthropic-direct";

/// Default `secret` entry name for the provider above, mirroring
/// [`DEFAULT_VERCEL_AI_GATEWAY_SECRET`].
pub const DEFAULT_ANTHROPIC_SECRET: &str = "anthropic/default";

/// A credential whose normal formatting is always redacted.
#[derive(Clone, PartialEq, Eq)]
pub struct EnrollmentCredential(String);

impl EnrollmentCredential {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the secret only to the protocol implementation that must use it.
    /// Callers must never put this value into a log, error, or command line.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for EnrollmentCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EnrollmentCredential([REDACTED])")
    }
}

impl fmt::Display for EnrollmentCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigOverrides {
    pub api_base_url: Option<String>,
    pub runner_id: Option<String>,
    pub state_dir: Option<PathBuf>,
    pub enrollment_credential: Option<EnrollmentCredential>,
    /// Keyed the same as [`RunnerConfig::providers`]. Only a field actually
    /// present here is merged — an override never clears a provider's other
    /// field, matching [`ConfigOverrides`]'s own `Option`-means-untouched
    /// rule for every other member.
    pub providers: BTreeMap<String, ProviderOverride>,
}

/// A partial [`ProviderConfig`] update: `None` leaves the field untouched,
/// mirroring every other member of [`ConfigOverrides`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderOverride {
    pub enabled: Option<bool>,
    pub secret: Option<String>,
}

/// Whether a provider is on, and where its credential lives — the only two
/// user-configurable facts about it. Endpoint URLs and credential env-var
/// names are not configuration; they are fixed per-provider data in
/// `crate::provider::known_endpoint`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderConfig {
    pub enabled: bool,
    pub secret: String,
}

#[derive(Default)]
pub struct RunnerConfigSources<'a> {
    pub file_toml: Option<&'a str>,
    pub environment: ConfigOverrides,
    pub command_line: ConfigOverrides,
}

/// Safe runner configuration. `Debug` inherits credential redaction from
/// [`EnrollmentCredential`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerConfig {
    pub api_base_url: String,
    pub runner_id: String,
    pub state_dir: PathBuf,
    pub enrollment_credential: Option<EnrollmentCredential>,
    /// Configured provider endpoints, keyed by `[provider.<name>]` table
    /// name. One entry per known provider is seeded by
    /// [`RunnerConfig::defaults`] ([`VERCEL_AI_GATEWAY_CONFIG_KEY`],
    /// [`ANTHROPIC_CONFIG_KEY`], both disabled); a name absent from this
    /// map has no configured endpoint at all, not merely a disabled one —
    /// the harness's own subscription/login mode applies.
    pub providers: BTreeMap<String, ProviderConfig>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    api_base_url: Option<String>,
    runner_id: Option<String>,
    state_dir: Option<PathBuf>,
    enrollment_credential: Option<String>,
    /// `[provider.<name>]` tables. Free-form key: a name this build does
    /// not recognize simply sits unused rather than failing to parse — the
    /// dedicated `enabled`/`secret` fields inside each table still reject
    /// an unknown field via `deny_unknown_fields` on
    /// [`ProviderFileConfig`].
    provider: Option<BTreeMap<String, ProviderFileConfig>>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderFileConfig {
    enabled: Option<bool>,
    secret: Option<String>,
}

impl RunnerConfig {
    pub fn defaults() -> Self {
        let mut providers = BTreeMap::new();
        providers.insert(
            VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
            ProviderConfig {
                enabled: false,
                secret: DEFAULT_VERCEL_AI_GATEWAY_SECRET.to_owned(),
            },
        );
        providers.insert(
            ANTHROPIC_CONFIG_KEY.to_owned(),
            ProviderConfig {
                enabled: false,
                secret: DEFAULT_ANTHROPIC_SECRET.to_owned(),
            },
        );
        Self {
            api_base_url: DEFAULT_API_BASE_URL.to_owned(),
            runner_id: DEFAULT_RUNNER_ID.to_owned(),
            state_dir: PathBuf::from(DEFAULT_STATE_DIR),
            enrollment_credential: None,
            providers,
        }
    }

    /// Applies defaults, configuration file, environment, then command-line
    /// values. Supplying source values explicitly keeps this deterministic and
    /// testable without global environment mutation.
    pub fn from_sources(sources: RunnerConfigSources<'_>) -> Result<Self, ConfigError> {
        let mut config = Self::defaults();

        if let Some(contents) = sources.file_toml {
            let file: FileConfig = toml::from_str(contents).map_err(|_| ConfigError::Invalid)?;
            let providers = file
                .provider
                .unwrap_or_default()
                .into_iter()
                .map(|(name, entry)| {
                    (
                        name,
                        ProviderOverride {
                            enabled: entry.enabled,
                            secret: entry.secret,
                        },
                    )
                })
                .collect();
            config.apply(ConfigOverrides {
                api_base_url: file.api_base_url,
                runner_id: file.runner_id,
                state_dir: file.state_dir,
                enrollment_credential: file.enrollment_credential.map(EnrollmentCredential::new),
                providers,
            });
        }
        config.apply(sources.environment);
        config.apply(sources.command_line);

        if config.runner_id.trim().is_empty() {
            return Err(ConfigError::EmptyRunnerId);
        }
        Ok(config)
    }

    pub fn environment_overrides() -> ConfigOverrides {
        let mut providers = BTreeMap::new();
        let enabled = std::env::var("TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_ENABLED")
            .ok()
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        let secret = std::env::var("TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_SECRET").ok();
        if enabled.is_some() || secret.is_some() {
            providers.insert(
                VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
                ProviderOverride { enabled, secret },
            );
        }
        let anthropic_enabled = std::env::var("TACK_RUNNER_PROVIDER_ANTHROPIC_ENABLED")
            .ok()
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        let anthropic_secret = std::env::var("TACK_RUNNER_PROVIDER_ANTHROPIC_SECRET").ok();
        if anthropic_enabled.is_some() || anthropic_secret.is_some() {
            providers.insert(
                ANTHROPIC_CONFIG_KEY.to_owned(),
                ProviderOverride {
                    enabled: anthropic_enabled,
                    secret: anthropic_secret,
                },
            );
        }
        ConfigOverrides {
            api_base_url: std::env::var("TACK_RUNNER_API_URL").ok(),
            runner_id: std::env::var("TACK_RUNNER_ID").ok(),
            state_dir: std::env::var_os("TACK_RUNNER_STATE_DIR").map(PathBuf::from),
            enrollment_credential: std::env::var("TACK_RUNNER_ENROLLMENT_TOKEN")
                .ok()
                .map(EnrollmentCredential::new),
            providers,
        }
    }

    pub fn require_enrollment_credential(&self) -> Result<&EnrollmentCredential, RunnerError> {
        self.enrollment_credential
            .as_ref()
            .ok_or(RunnerError::MissingEnrollmentCredential)
    }

    /// Where the file-backend secret store keeps its owner-only file when no
    /// platform credential store answers. Always under `state_dir`, so it
    /// moves with `--state-dir`/`TACK_RUNNER_STATE_DIR` exactly like the
    /// enrolled session file does.
    pub fn secret_store_path(&self) -> PathBuf {
        self.state_dir.join("secrets.json")
    }

    fn apply(&mut self, overrides: ConfigOverrides) {
        if let Some(value) = overrides.api_base_url {
            self.api_base_url = value;
        }
        if let Some(value) = overrides.runner_id {
            self.runner_id = value;
        }
        if let Some(value) = overrides.state_dir {
            self.state_dir = value;
        }
        if let Some(value) = overrides.enrollment_credential {
            self.enrollment_credential = Some(value);
        }
        for (name, provider_override) in overrides.providers {
            let entry = self
                .providers
                .entry(name)
                .or_insert_with(|| ProviderConfig {
                    enabled: false,
                    secret: String::new(),
                });
            if let Some(enabled) = provider_override.enabled {
                entry.enabled = enabled;
            }
            if let Some(secret) = provider_override.secret {
                entry.secret = secret;
            }
        }
    }
}

#[cfg(test)]
#[path = "config/tests.rs"]
mod tests;
