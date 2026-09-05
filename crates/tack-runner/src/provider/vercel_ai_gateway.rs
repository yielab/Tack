//! The Vercel AI Gateway [`super::Provider`]: one catalog serving both
//! wires this crate speaks, a bearer credential, and a catalog body that
//! publishes pricing, a context window and a modality per model (ADR 0061
//! decision 4; ADR 0063 decisions 1, 2 and 4).

use async_trait::async_trait;

use super::{CATALOG_TIMEOUT, CatalogEntry, CatalogFetchError, KnownEndpoint, Provider, Wire};
use crate::config::{VERCEL_AI_GATEWAY_CONFIG_KEY, VERCEL_AI_GATEWAY_PROVIDER};
use crate::secrets::SecretValue;

const CATALOG_URL: &str = "https://ai-gateway.vercel.sh/v1/models";

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
            .get(CATALOG_URL)
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
mod tests {
    use super::*;

    /// Three real entries captured from a live
    /// `https://ai-gateway.vercel.sh/v1/models` fetch, chosen to cover the
    /// three shapes that matter: full pricing and a real
    /// context window; `"pricing": {}` with a literal `0` context window;
    /// and no `context_window` key at all.
    const SAMPLE_BODY: &str = r#"{
        "object": "list",
        "data": [
            {
                "id": "alibaba/qwen-3-14b",
                "context_window": 40960,
                "max_tokens": 16384,
                "type": "language",
                "pricing": {"input": "0.00000012", "output": "0.00000024"}
            },
            {
                "id": "bfl/flux-2-flex",
                "context_window": 0,
                "max_tokens": 0,
                "type": "image",
                "pricing": {}
            },
            {
                "id": "openai/whisper-1",
                "type": "transcription",
                "pricing": {"input": "0.0000000001", "transcription_duration_cost_per_second": "0.0001"}
            }
        ]
    }"#;

    #[test]
    fn parses_priced_context_windowed_empty_pricing_and_absent_context_window_entries() {
        let entries = parse_catalog(SAMPLE_BODY.as_bytes()).expect("valid catalog body");
        assert_eq!(entries.len(), 3);

        let qwen = entries
            .iter()
            .find(|e| e.id == "alibaba/qwen-3-14b")
            .unwrap();
        assert_eq!(qwen.context_window, Some(40960));
        assert!(qwen.price.is_some());
        assert!(
            qwen.modality.is_none(),
            "this real captured entry has no modalities key, so it must stay unset, never inferred"
        );

        let flux = entries.iter().find(|e| e.id == "bfl/flux-2-flex").unwrap();
        assert_eq!(
            flux.context_window,
            Some(0),
            "the vendor's own body publishes a literal 0, passed through as published"
        );
        assert!(
            flux.price.is_none(),
            "an empty pricing object means no price published, not Some({{}})"
        );

        let whisper = entries.iter().find(|e| e.id == "openai/whisper-1").unwrap();
        assert_eq!(
            whisper.context_window, None,
            "no context_window key at all for this non-text model"
        );
        assert!(whisper.price.is_some());
    }

    #[test]
    fn a_malformed_body_is_a_parse_error_not_a_panic() {
        assert!(parse_catalog(b"not json").is_err());
    }

    /// The three real entries above happen not to carry a `modalities` key.
    /// This entry is not a live capture: it proves the parser passes the
    /// key through opaquely, exactly as it does for `pricing`, whatever
    /// shape it holds — not that this is the vendor's actual shape for it.
    #[test]
    fn modality_passes_through_opaquely_when_the_catalog_publishes_it() {
        const BODY_WITH_MODALITY: &str = r#"{
            "object": "list",
            "data": [
                {
                    "id": "openai/gpt-5.6-sol",
                    "context_window": 400000,
                    "pricing": {"input": "0.000002", "output": "0.000008"},
                    "modalities": {"input": ["text", "image"], "output": ["text"]}
                }
            ]
        }"#;
        let entries = parse_catalog(BODY_WITH_MODALITY.as_bytes()).expect("valid catalog body");
        let sol = entries
            .iter()
            .find(|e| e.id == "openai/gpt-5.6-sol")
            .unwrap();
        assert_eq!(
            sol.modality,
            Some(serde_json::json!({"input": ["text", "image"], "output": ["text"]})),
            "whatever shape the vendor publishes under modalities is stored as-is"
        );
    }

    /// Opt-in, matching the harness adapters' own `#[ignore]`-gated live
    /// tests: never runs under a plain `cargo test`, never required in CI.
    /// Proves the fetch reaches the real gateway host with the real
    /// request shape and still parses its current body — never a
    /// fabricated substitute for the two unit tests above, which exercise
    /// the parser but not the network call itself. Reads the key directly
    /// from an environment variable rather than a machine's own secret
    /// store, since a bare fetch needs nothing else.
    #[tokio::test]
    #[ignore = "opt-in: requires TACK_RUN_LIVE_VERCEL_CATALOG_TEST=1 and \
                TACK_LIVE_VERCEL_AI_GATEWAY_KEY set to a real key; run with \
                TACK_RUN_LIVE_VERCEL_CATALOG_TEST=1 TACK_LIVE_VERCEL_AI_GATEWAY_KEY=... \
                cargo nextest run --workspace --run-ignored ignored-only \
                -E 'test(vercel_ai_gateway::tests::live_)'"]
    async fn live_fetch_catalog_reaches_the_real_gateway_when_opted_in() {
        if std::env::var("TACK_RUN_LIVE_VERCEL_CATALOG_TEST").as_deref() != Ok("1") {
            eprintln!(
                "skipping live Vercel AI Gateway catalog test: set \
                 TACK_RUN_LIVE_VERCEL_CATALOG_TEST=1 and TACK_LIVE_VERCEL_AI_GATEWAY_KEY to opt in"
            );
            return;
        }
        let Ok(key) = std::env::var("TACK_LIVE_VERCEL_AI_GATEWAY_KEY") else {
            eprintln!(
                "skipping live Vercel AI Gateway catalog test: TACK_LIVE_VERCEL_AI_GATEWAY_KEY is \
                 not set"
            );
            return;
        };
        let dir = tempfile::tempdir().expect("temporary directory");
        let secrets = crate::secrets::SecretStore::file(dir.path().join("secrets.json"));
        secrets.set("live-key", &key).expect("seed secret");
        let secret = secrets.resolve("live-key").expect("resolve secret");

        let entries = VercelAiGateway
            .fetch_catalog(&secret)
            .await
            .expect("the real gateway answers a real key with a parseable catalog");
        assert!(
            !entries.is_empty(),
            "the real gateway's catalog is never empty when the key is valid"
        );
        let priced = entries.iter().filter(|e| e.price.is_some()).count();
        let windowed = entries
            .iter()
            .filter(|e| e.context_window.is_some())
            .count();
        eprintln!(
            "live Vercel AI Gateway catalog: {} models ({} priced, {} publish a context window)",
            entries.len(),
            priced,
            windowed
        );
    }
}
