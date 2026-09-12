//! The Vercel AI Gateway [`super::Provider`]: one catalog serving both
//! wires this crate speaks, a bearer credential, and a catalog body that
//! publishes pricing, a context window and a modality per model (ADR 0061
//! decision 4; ADR 0063 decisions 1, 2 and 4).

use async_trait::async_trait;

use super::{CATALOG_TIMEOUT, CatalogEntry, CatalogFetchError, KnownEndpoint, Provider, Wire};
use crate::config::{VERCEL_AI_GATEWAY_CONFIG_KEY, VERCEL_AI_GATEWAY_PROVIDER};
use crate::secrets::SecretValue;

const CATALOG_URL: &str = "https://ai-gateway.vercel.sh/v1/models";

/// Test-only escape hatch: `scripts/smoke.sh` step 13 is this variable's
/// only intended setter, documented as such in `docs/CONFIG.md`. **What it
/// actually does, stated plainly rather than left implicit:** when present,
/// every URL this provider would otherwise send a request to — including
/// [`fetch_catalog`]'s `bearer_auth(secret.expose())` call, which carries
/// whatever credential this runner has stored for this provider — is
/// rebased under it instead of the real gateway host. This is not a
/// harmless flag: setting it to an attacker-controlled host redirects a
/// real stored credential there. It is not a new privilege, though —
/// whoever can set an environment variable on this process can already
/// read the same credential straight out of the secret store this process
/// already has open, so this adds no attack surface beyond "control this
/// process's environment," which is already full compromise. The loopback
/// restriction below exists anyway, cheaply, to catch a *mistake* (a
/// non-loopback value reached this process by accident — a copied env
/// file, a misconfigured deployment) rather than a deliberate attacker, who
/// gains nothing from this check that they didn't already have. Unset in
/// every other path — including every other test in this module — so this
/// costs one environment lookup and changes nothing for the real gateway.
const TEST_BASE_URL_OVERRIDE_VAR: &str = "TACK_RUNNER_VERCEL_AI_GATEWAY_TEST_BASE_URL";

/// Accepts only a loopback base — see the credential-exposure note above.
/// A non-loopback value is treated exactly like the variable being unset,
/// never as a hard error: this is a smoke-test convenience, not a
/// configuration surface with its own validation contract, and refusing to
/// start would be a worse failure mode for a test harness than silently
/// falling back to the real gateway (which then fails loudly on a fake key,
/// rather than this process failing to start at all).
fn test_base_url_override() -> Option<String> {
    let raw = std::env::var(TEST_BASE_URL_OVERRIDE_VAR).ok()?;
    let base = raw.trim_end_matches('/');
    is_loopback_base(base).then(|| base.to_owned())
}

/// Whether `base` addresses this machine's own loopback interface.
///
/// The host is compared as a whole, never as a prefix: `localhost` and
/// `localhost.example.com` share the same first nine characters, and a
/// prefix test would accept the second — handing the stored credential to
/// whoever owns that domain, which is the exact outcome the check exists to
/// prevent. `127.` is likewise only loopback when what follows it is the
/// rest of a dotted-quad address, not an arbitrary label.
fn is_loopback_base(base: &str) -> bool {
    let Some(rest) = base.strip_prefix("http://") else {
        return false;
    };
    let host = match rest.strip_prefix("[") {
        // An IPv6 literal keeps its brackets, and only `::1` is loopback.
        Some(after) => match after.split_once(']') {
            Some((inside, _)) => return inside == "::1",
            None => return false,
        },
        None => rest.split_once([':', '/']).map_or(rest, |(host, _)| host),
    };
    host == "localhost"
        || host
            .strip_prefix("127.")
            .is_some_and(|octets| !octets.is_empty() && octets.split('.').all(is_octet))
}

fn is_octet(part: &str) -> bool {
    !part.is_empty()
        && part.len() <= 3
        && part.bytes().all(|b| b.is_ascii_digit())
        && part.parse::<u16>().is_ok_and(|n| n <= 255)
}

fn catalog_url() -> String {
    match test_base_url_override() {
        Some(base) => format!("{base}/v1/models"),
        None => CATALOG_URL.to_owned(),
    }
}

pub(crate) struct VercelAiGateway;

#[async_trait]
impl Provider for VercelAiGateway {
    fn wire_name(&self) -> &'static str {
        VERCEL_AI_GATEWAY_PROVIDER
    }

    fn config_key(&self) -> &'static str {
        VERCEL_AI_GATEWAY_CONFIG_KEY
    }

    fn display_name(&self) -> &'static str {
        "Vercel AI Gateway"
    }

    fn endpoint(&self, wire: Wire) -> Option<KnownEndpoint> {
        if let Some(base) = test_base_url_override() {
            let (suffix, credential_env_var) = match wire {
                Wire::AnthropicMessages => ("/claude-code", "ANTHROPIC_AUTH_TOKEN"),
                Wire::OpenAiResponses => ("/codex/v1", "AI_GATEWAY_API_KEY"),
            };
            // Leaked deliberately: this arm only ever runs under the smoke
            // test's own opt-in env var, at most twice per process (one
            // leak per `Wire`), never in a production runner.
            let base_url: &'static str = Box::leak(format!("{base}{suffix}").into_boxed_str());
            return Some(KnownEndpoint {
                base_url,
                credential_env_var,
            });
        }
        match wire {
            // No `/v1` suffix: the CLI appends `/v1/messages` itself, and a
            // double suffix 404s.
            Wire::AnthropicMessages => Some(KnownEndpoint {
                base_url: "https://ai-gateway.vercel.sh/claude-code",
                credential_env_var: "ANTHROPIC_AUTH_TOKEN",
            }),
            Wire::OpenAiResponses => Some(KnownEndpoint {
                base_url: "https://ai-gateway.vercel.sh/codex/v1",
                credential_env_var: "AI_GATEWAY_API_KEY",
            }),
        }
    }

    // A gateway can route, fall back, or alias a request to a different
    // model than the one requested, so a harness's own init line — written
    // before any call reaches this endpoint — cannot be trusted as what was
    // actually served. Explicit rather than left to the trait default: a
    // reader should not have to check the default to know this was decided,
    // not overlooked.
    fn confirms_served_model_from_init_line(&self) -> bool {
        false
    }

    async fn fetch_catalog(
        &self,
        secret: &SecretValue,
    ) -> Result<Vec<CatalogEntry>, CatalogFetchError> {
        let client = reqwest::Client::builder()
            .timeout(CATALOG_TIMEOUT)
            .user_agent(concat!("tack-runner/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| CatalogFetchError::Transport)?;
        let response = client
            .get(catalog_url())
            .bearer_auth(secret.expose())
            .send()
            .await
            .map_err(|_| CatalogFetchError::Transport)?;
        let status = response.status();
        if !status.is_success() {
            return Err(CatalogFetchError::Status(status.as_u16()));
        }
        let body = response
            .bytes()
            .await
            .map_err(|_| CatalogFetchError::Status(status.as_u16()))?;
        parse_catalog(&body).map_err(|_| CatalogFetchError::Status(status.as_u16()))
    }
}

#[derive(serde::Deserialize)]
struct CatalogModel {
    id: String,
    #[serde(default)]
    context_window: Option<u64>,
    #[serde(default)]
    pricing: Option<serde_json::Value>,
    #[serde(default)]
    modalities: Option<serde_json::Value>,
}

#[derive(serde::Deserialize)]
struct CatalogResponse {
    #[serde(default)]
    data: Vec<CatalogModel>,
}

/// Parses one Vercel AI Gateway catalog body into the common
/// [`CatalogEntry`] shape. Measured against the real gateway
/// (`https://ai-gateway.vercel.sh/v1/models`, 373 models at measurement
/// time): 21 entries publish `"pricing": {}` — an explicitly *empty*
/// object, not a missing key or a `null` — for a model this vendor does not
/// price (rerank, some audio); folded into `None` here rather than kept as
/// `Some({})`, which would print as a literal empty object instead of the
/// project's own `Not measured` convention. 18 entries omit `context_window`
/// entirely (non-text models: transcription, text-to-speech); at least one
/// entry (`bfl/flux-2-flex`, an image model) instead publishes a literal
/// `0` for it — the vendor's own catalog is not internally consistent about
/// omission vs. zero for "not applicable," and this parser passes that
/// value through as published (`Some(0)`) rather than guessing which
/// non-text model types should be reinterpreted as `None`. Every model in
/// the same live fetch also carries a `modalities` key; this parser stores
/// it exactly as published, the same treatment as `pricing`, since the
/// live fetch was not re-run to confirm every value that key takes across
/// all 373 models.
fn parse_catalog(body: &[u8]) -> Result<Vec<CatalogEntry>, serde_json::Error> {
    let parsed: CatalogResponse = serde_json::from_slice(body)?;
    Ok(parsed
        .data
        .into_iter()
        .map(|model| CatalogEntry {
            id: model.id,
            context_window: model.context_window,
            price: model
                .pricing
                .filter(|value| value != &serde_json::json!({})),
            modality: model.modalities,
        })
        .collect())
}

#[cfg(test)]
#[path = "vercel_ai_gateway/tests.rs"]
mod tests;
