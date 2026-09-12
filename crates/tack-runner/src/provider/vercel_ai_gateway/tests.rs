use super::*;

/// The override's guard compares the whole host, never a prefix. Each
/// rejected case below is a host that shares a loopback host's opening
/// characters while belonging to somebody else — accepting one would
/// send the stored gateway credential to whoever owns that name, which
/// is the entire reason the guard exists.
#[test]
fn only_a_whole_loopback_host_is_accepted_as_a_base() {
    for accepted in [
        "http://127.0.0.1:9",
        "http://127.0.0.1",
        "http://localhost:3500",
        "http://localhost",
        "http://[::1]:8080",
    ] {
        assert!(is_loopback_base(accepted), "must accept {accepted}");
    }
    for rejected in [
        "http://localhost.attacker.example",
        "http://localhostile.example",
        "http://127.evil.example",
        "http://127.0.0.1.attacker.example",
        "http://[::2]:8080",
        "https://localhost",
        "http://10.0.0.1",
    ] {
        assert!(!is_loopback_base(rejected), "must reject {rejected}");
    }
}

/// Proves the smoke-only override actually rebases every URL this
/// provider would otherwise hit, and that its absence changes nothing —
/// the property `scripts/smoke.sh` step 13 depends on. Mutates a
/// process-wide env var, safe under `cargo nextest` (one process per
/// test); do not run this test under a bare `cargo test` alongside
/// others in this module in the same process.
#[test]
fn the_test_only_base_url_override_rebases_catalog_and_both_wires_and_is_a_no_op_when_unset() {
    assert_eq!(
        catalog_url(),
        CATALOG_URL,
        "unset: the real host, unchanged"
    );
    let claude = VercelAiGateway
        .endpoint(Wire::AnthropicMessages)
        .expect("endpoint present");
    assert_eq!(claude.base_url, "https://ai-gateway.vercel.sh/claude-code");

    unsafe {
        std::env::set_var(TEST_BASE_URL_OVERRIDE_VAR, "http://127.0.0.1:9/smoke-gw");
    }
    assert_eq!(catalog_url(), "http://127.0.0.1:9/smoke-gw/v1/models");
    let claude = VercelAiGateway
        .endpoint(Wire::AnthropicMessages)
        .expect("endpoint present");
    assert_eq!(claude.base_url, "http://127.0.0.1:9/smoke-gw/claude-code");
    assert_eq!(claude.credential_env_var, "ANTHROPIC_AUTH_TOKEN");
    let codex = VercelAiGateway
        .endpoint(Wire::OpenAiResponses)
        .expect("endpoint present");
    assert_eq!(codex.base_url, "http://127.0.0.1:9/smoke-gw/codex/v1");
    assert_eq!(codex.credential_env_var, "AI_GATEWAY_API_KEY");
    unsafe {
        std::env::remove_var(TEST_BASE_URL_OVERRIDE_VAR);
    }
    assert_eq!(catalog_url(), CATALOG_URL, "removed: back to the real host");

    // A non-loopback value is a mistake, not a valid override target —
    // treated exactly like unset, never honored, so a credential can
    // never be silently redirected to a non-loopback host through this
    // variable.
    unsafe {
        std::env::set_var(TEST_BASE_URL_OVERRIDE_VAR, "http://example.invalid");
    }
    assert_eq!(
        catalog_url(),
        CATALOG_URL,
        "a non-loopback override is ignored, not honored"
    );
    let claude = VercelAiGateway
        .endpoint(Wire::AnthropicMessages)
        .expect("endpoint present");
    assert_eq!(
        claude.base_url, "https://ai-gateway.vercel.sh/claude-code",
        "a non-loopback override must never reach the endpoint either"
    );
    unsafe {
        std::env::remove_var(TEST_BASE_URL_OVERRIDE_VAR);
    }
}

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
