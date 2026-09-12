use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::config::Config;

fn client_for(base_url: &str) -> TackClient {
    TackClient::new(&Config {
        base_url: base_url.to_string(),
        token: None,
    })
    .unwrap()
}

// Run a blocking closure in a thread that is allowed to block — the
// client is a blocking `reqwest` client, and `wiremock`'s server runs on
// the same tokio runtime the test does, so calling it directly from the
// `#[tokio::test]` body would block that runtime's own worker thread.
async fn run_blocking<F, T>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .expect("blocking task panicked")
}

#[tokio::test]
async fn get_with_etag_reads_the_response_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/items/x"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"3\"")
                .set_body_json(serde_json::json!({ "id": "x" })),
        )
        .mount(&server)
        .await;

    let uri = server.uri();
    let (value, etag) = run_blocking(move || client_for(&uri).get_with_etag("/items/x"))
        .await
        .unwrap();

    assert_eq!(value["id"], "x");
    assert_eq!(etag.as_deref(), Some("\"3\""));
}

#[tokio::test]
async fn get_with_etag_tolerates_a_response_with_no_etag() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/items/x"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    let uri = server.uri();
    let (_, etag) = run_blocking(move || client_for(&uri).get_with_etag("/items/x"))
        .await
        .unwrap();

    assert_eq!(etag, None);
}

#[tokio::test]
async fn patch_if_match_sends_the_if_match_header() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/items/x"))
        .and(header("If-Match", "\"7\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    let uri = server.uri();
    let result = run_blocking(move || {
        client_for(&uri).patch_if_match("/items/x", &serde_json::json!({}), Some("\"7\""))
    })
    .await;

    assert!(result.is_ok(), "{result:?}");
}

/// `None` must round-trip to plain `patch` — an absent precondition
/// preserves today's unconditional-write behavior exactly. The
/// mock rejects any request that *does* carry `If-Match`, so this fails
/// loudly (500, no matching mock) rather than passing vacuously if a
/// future edit starts sending an empty-string header instead of
/// omitting it.
#[tokio::test]
async fn patch_if_match_sends_no_header_when_none_is_given() {
    struct NoIfMatch;
    impl wiremock::Match for NoIfMatch {
        fn matches(&self, request: &wiremock::Request) -> bool {
            !request.headers.contains_key("if-match")
        }
    }

    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/items/x"))
        .and(NoIfMatch)
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    let uri = server.uri();
    let result = run_blocking(move || {
        client_for(&uri).patch_if_match("/items/x", &serde_json::json!({}), None)
    })
    .await;

    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn patch_if_match_maps_412_to_a_conflict_specific_message() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/items/x"))
        .respond_with(ResponseTemplate::new(412).set_body_json(serde_json::json!({
            "error": { "status": 412, "message": "version mismatch" }
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let result = run_blocking(move || {
        client_for(&uri).patch_if_match("/items/x", &serde_json::json!({}), Some("\"1\""))
    })
    .await;

    let err = result
        .expect_err("412 must surface as an error")
        .to_string();
    assert!(err.contains("412"), "unexpected message: {err}");
    assert!(
        err.to_lowercase().contains("re-read"),
        "message must tell the caller to re-read, not just fail: {err}"
    );
}

#[test]
fn status_label_covers_412() {
    assert_eq!(
        status_label(StatusCode::PRECONDITION_FAILED),
        "Precondition failed (item changed — re-read and retry)"
    );
}

// ── error_msg: runner-v1 protocol error envelope ──────────────
//
// The execution/fleet/runner/profile operator routes answer errors with
// `{"error": {"code": ..., "message": ..., ...}}` — `error` is an
// *object*, unlike every pre-existing route's `{"error": "text"}`. These
// pin that both shapes are handled, that the object shape surfaces the
// stable `code` (not just prose), and that the legacy string shape is
// completely unaffected.

#[test]
fn error_msg_reads_the_plain_string_shape_unchanged() {
    let body = serde_json::json!({ "error": "not found" });
    assert_eq!(error_msg(&body), "not found");
}

#[test]
fn error_msg_reads_the_message_key_shape_unchanged() {
    let body = serde_json::json!({ "message": "bad request" });
    assert_eq!(error_msg(&body), "bad request");
}

#[test]
fn error_msg_surfaces_code_and_message_from_the_protocol_envelope() {
    let body = serde_json::json!({
        "error": {
            "code": "idempotency_conflict",
            "message": "The idempotency key was used with a different request",
            "request_id": "req_operator",
            "retryable": false,
            "details": { "idempotency_key": "k1" }
        }
    });
    assert_eq!(
        error_msg(&body),
        "idempotency_conflict: The idempotency key was used with a different request"
    );
}

#[test]
fn error_msg_distinguishes_conflict_codes_from_each_other() {
    let conflict = serde_json::json!({
        "error": { "code": "conflict", "message": "Fleet name already exists" }
    });
    let stale = serde_json::json!({
        "error": { "code": "stale_lease", "message": "The fencing token is stale" }
    });
    let transition = serde_json::json!({
        "error": { "code": "invalid_transition", "message": "Only authoritatively recovered needs_operator attempts may be requeued" }
    });
    let a = error_msg(&conflict);
    let b = error_msg(&stale);
    let c = error_msg(&transition);
    assert_ne!(a, b);
    assert_ne!(b, c);
    assert_ne!(a, c);
    assert!(a.starts_with("conflict:"));
    assert!(b.starts_with("stale_lease:"));
    assert!(c.starts_with("invalid_transition:"));
}

#[test]
fn error_msg_falls_back_to_generic_when_nothing_recognizable_is_present() {
    let body = serde_json::json!({ "whatever": true });
    assert_eq!(error_msg(&body), "server error");
}

#[tokio::test]
async fn post_surfaces_the_protocol_envelope_code_through_extract() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/executions"))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
            "error": {
                "code": "idempotency_conflict",
                "message": "The idempotency key was used with a different request",
                "request_id": "req_operator",
                "retryable": false,
                "details": {}
            }
        })))
        .mount(&server)
        .await;

    let uri = server.uri();
    let result =
        run_blocking(move || client_for(&uri).post("/executions", &serde_json::json!({}))).await;

    let err = result
        .expect_err("409 must surface as an error")
        .to_string();
    assert!(err.contains("409"), "unexpected: {err}");
    assert!(
        err.contains("idempotency_conflict"),
        "must carry the stable code, not just a generic message: {err}"
    );
}
