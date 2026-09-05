//! Anthropic's own API as a [`super::Provider`] (ADR 0063 decisions 1, 2
//! and 4): a vendor's own API is the same key+endpoint shape as a gateway,
//! but neither its auth placement nor its catalog body match Vercel's.
//! Fetched from Anthropic's own current documentation
//! (`https://platform.claude.com/docs/en/api/models-list`):
//! the credential goes in an `x-api-key` header (plus a required, fixed
//! `anthropic-version` header) rather than an `Authorization: Bearer`
//! token, and the catalog publishes a model id, a display name and a
//! context window (`max_input_tokens`) but **no price and no modality** —
//! unlike Vercel, whose catalog prices almost every model and names each
//! one's modality. `price` and `modality` are therefore always `None`
//! here, not a gap in this parser: the vendor's own catalog has nothing to
//! publish for either.

use async_trait::async_trait;

use super::{CATALOG_TIMEOUT, CatalogEntry, CatalogFetchError, KnownEndpoint, Provider, Wire};
use crate::config::{ANTHROPIC_CONFIG_KEY, ANTHROPIC_PROVIDER};
use crate::secrets::SecretValue;

const BASE_URL: &str = "https://api.anthropic.com";
// `limit` accepts up to 1000 per Anthropic's own documented range; a single
// page covers every model this vendor currently lists. If the catalog ever
// exceeds that, this fetch under-reports rather than looping pages — no
// pagination is implemented here.
const CATALOG_URL: &str = "https://api.anthropic.com/v1/models?limit=1000";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub(crate) struct Anthropic;

#[async_trait]
impl Provider for Anthropic {
    fn wire_name(&self) -> &'static str {
        ANTHROPIC_PROVIDER
    }

    fn config_key(&self) -> &'static str {
        ANTHROPIC_CONFIG_KEY
    }

    fn display_name(&self) -> &'static str {
        "Anthropic"
    }

    fn endpoint(&self, wire: Wire) -> Option<KnownEndpoint> {
        match wire {
            Wire::AnthropicMessages => Some(KnownEndpoint {
                base_url: BASE_URL,
                // claude-code's own env-based auth distinguishes a bearer
                // token (`ANTHROPIC_AUTH_TOKEN`, used for a gateway proxy)
                // from a native Anthropic key (`ANTHROPIC_API_KEY`, sent as
                // `x-api-key`). Anthropic's own API needs the latter.
                credential_env_var: "ANTHROPIC_API_KEY",
            }),
            // Anthropic's own API does not speak the OpenAI-Responses wire
            // at all — there is no endpoint to point codex at here.
            Wire::OpenAiResponses => None,
        }
    }

    // A direct vendor API is not a proxy: it serves the requested model or
    // the request fails, never a silent substitute, so a harness's own
    // init line is trustworthy here even though it is written before this
    // endpoint answers. Explicit rather than left to the trait default, the
    // same as the gateway module's own explicit `false`.
    fn confirms_served_model_from_init_line(&self) -> bool {
        true
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
            .get(CATALOG_URL)
            .header("x-api-key", secret.expose())
            .header("anthropic-version", ANTHROPIC_VERSION)
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
    max_input_tokens: Option<u64>,
}

#[derive(serde::Deserialize)]
struct CatalogResponse {
    #[serde(default)]
    data: Vec<CatalogModel>,
}

/// Parses one Anthropic `/v1/models` body into the common [`CatalogEntry`]
/// shape. `price` is always `None`: this endpoint has no pricing field of
/// any kind, documented or otherwise — never a gap this parser invents.
/// `modality` is always `None` for the same reason: the response carries
/// an `id`, a `display_name`, `created_at` and `max_input_tokens`/`max_tokens`,
/// and nothing that names what a model accepts or produces.
fn parse_catalog(body: &[u8]) -> Result<Vec<CatalogEntry>, serde_json::Error> {
    let parsed: CatalogResponse = serde_json::from_slice(body)?;
    Ok(parsed
        .data
        .into_iter()
        .map(|model| CatalogEntry {
            id: model.id,
            context_window: model.max_input_tokens,
            price: None,
            modality: None,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Anthropic's own documented example response
    /// (`https://platform.claude.com/docs/en/api/models-list`), trimmed to
    /// the fields this parser reads, plus a second entry with
    /// `max_input_tokens: null` to prove an unpublished context
    /// window stays `None` rather than becoming `0`.
    const SAMPLE_BODY: &str = r#"{
        "data": [
            {
                "id": "claude-opus-5",
                "type": "model",
                "display_name": "Claude Opus 5",
                "created_at": "2026-07-24T00:00:00Z",
                "max_input_tokens": 500000,
                "max_tokens": 128000
            },
            {
                "id": "claude-legacy-unknown-window",
                "type": "model",
                "display_name": "Claude (context window not documented)",
                "created_at": "2020-01-01T00:00:00Z",
                "max_input_tokens": null,
                "max_tokens": null
            }
        ],
        "first_id": "claude-opus-5",
        "has_more": false,
        "last_id": "claude-legacy-unknown-window"
    }"#;

    #[test]
    fn parses_context_window_and_leaves_price_unset_since_the_vendor_publishes_none() {
        let entries = parse_catalog(SAMPLE_BODY.as_bytes()).expect("valid catalog body");
        assert_eq!(entries.len(), 2);

        let opus = entries.iter().find(|e| e.id == "claude-opus-5").unwrap();
        assert_eq!(opus.context_window, Some(500_000));
        assert!(
            opus.price.is_none(),
            "Anthropic's own catalog publishes no price field at all"
        );
        assert!(
            opus.modality.is_none(),
            "Anthropic's own catalog publishes no modality field at all"
        );

        let legacy = entries
            .iter()
            .find(|e| e.id == "claude-legacy-unknown-window")
            .unwrap();
        assert_eq!(legacy.context_window, None);
    }

    #[test]
    fn a_malformed_body_is_a_parse_error_not_a_panic() {
        assert!(parse_catalog(b"not json").is_err());
    }

    #[test]
    fn serves_only_the_anthropic_messages_wire() {
        let provider = Anthropic;
        assert!(provider.endpoint(Wire::AnthropicMessages).is_some());
        assert!(provider.endpoint(Wire::OpenAiResponses).is_none());
    }
}
