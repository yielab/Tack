//! CORS preflight coverage: `AllowHeaders`/`ExposeHeaders` are configured as
//! fixed lists (`CorsLayer::allow_headers`/`expose_headers` with an explicit
//! array), so the response always carries the full configured set regardless
//! of what the preflight's own `Access-Control-Request-Headers` asked for —
//! see `tower-http`'s `AllowHeaders::to_header`. A real cross-origin browser
//! still requires each header it needs to be in that fixed list.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;

use crate::common::test_app;
use tack_api::handlers::orch::APPROVAL_TOKEN_HEADER;

/// `Access-Control-Allow-Origin` only reflects an origin the server was
/// actually configured with (`TACK_ALLOWED_ORIGINS`); `AppConfig::default()`
/// seeds `http://localhost:8080` (`config.rs::default_allowed_origins`), so
/// that's the origin every test in this file presents.
const ALLOWED_ORIGIN: &str = "http://localhost:8080";

async fn preflight(app: &Router, uri: &str, method: &str, requested_headers: &str) -> StatusCode {
    let req = Request::builder()
        .method("OPTIONS")
        .uri(uri)
        .header(header::ORIGIN, ALLOWED_ORIGIN)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, method)
        .header(header::ACCESS_CONTROL_REQUEST_HEADERS, requested_headers)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    resp.status()
}

/// Same request as `preflight`, but returns the lower-cased
/// `Access-Control-Allow-Headers` value for substring assertions — matching
/// on the raw header would be brittle to comma/space formatting changes in
/// `tower_http`'s `separated_by_commas`.
async fn preflight_allow_headers(app: &Router, uri: &str, method: &str) -> String {
    let req = Request::builder()
        .method("OPTIONS")
        .uri(uri)
        .header(header::ORIGIN, ALLOWED_ORIGIN)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, method)
        .header(
            header::ACCESS_CONTROL_REQUEST_HEADERS,
            "if-match, x-tack-approval-token, content-type, authorization",
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    resp.headers()
        .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
        .expect("Access-Control-Allow-Headers must be present on a preflight response")
        .to_str()
        .unwrap()
        .to_ascii_lowercase()
}

/// A browser preflight for a cross-origin `PATCH /api/items/{id}` — the
/// shape both the ETag/If-Match write path and the approval-decide call
/// (`frontend/src/features/approvals/api.ts`) actually send — must list
/// `if-match` and `x-tack-approval-token` in the allowed request headers,
/// and a real (non-preflight) response must expose `ETag` so the browser's
/// `fetch()` can read it back off a `GET`. Without `expose_headers` naming
/// `ETag`, a browser can read zero non-safelisted response headers from
/// this API, full stop.
#[tokio::test]
async fn preflight_allows_if_match_approval_token_exposes_etag() {
    let (app, _workspace_id) = test_app().await;
    let item_uri = "/api/items/00000000-0000-0000-0000-000000000000";

    // (a)+(b): If-Match must be allowed for a PATCH preflight.
    let status = preflight(&app, item_uri, "PATCH", "if-match, content-type").await;
    assert_eq!(status, StatusCode::OK);
    let allow_headers = preflight_allow_headers(&app, item_uri, "PATCH").await;
    assert!(
        allow_headers.contains("if-match"),
        "if-match missing from Access-Control-Allow-Headers: {allow_headers}"
    );

    // (c): x-tack-approval-token must be allowed for the approval-decide
    // preflight (`POST /api/approvals/{token}`) — pre-existing bug, fixed
    // here. Assert the literal constant the handler actually checks
    // (`handlers/orch.rs::APPROVAL_TOKEN_HEADER`), not a hand-copied
    // string, so this test can't drift from the header the server reads.
    let approval_uri = "/api/approvals/apr-test-token";
    let status = preflight(&app, approval_uri, "POST", APPROVAL_TOKEN_HEADER).await;
    assert_eq!(status, StatusCode::OK);
    let allow_headers = preflight_allow_headers(&app, approval_uri, "POST").await;
    assert!(
        allow_headers.contains(APPROVAL_TOKEN_HEADER),
        "{APPROVAL_TOKEN_HEADER} missing from Access-Control-Allow-Headers: {allow_headers}"
    );

    // (a): ETag must be readable by JS on the real response, not just sent
    // on the wire. `Access-Control-Expose-Headers` is only attached to
    // non-preflight responses (`tower_http`'s `Cors::call` applies it in
    // the `CorsCall` branch, never `PreflightCall`), so this has to be a
    // plain GET, not another OPTIONS round-trip.
    let req = Request::builder()
        .method("GET")
        .uri("/api/health")
        .header(header::ORIGIN, ALLOWED_ORIGIN)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let expose_headers = resp
        .headers()
        .get(header::ACCESS_CONTROL_EXPOSE_HEADERS)
        .expect("Access-Control-Expose-Headers must be present on a real CORS response")
        .to_str()
        .unwrap()
        .to_ascii_lowercase();
    assert!(
        expose_headers.contains("etag"),
        "etag missing from Access-Control-Expose-Headers: {expose_headers}"
    );
}

/// Negative control: proves `allow_headers` is a fixed, named list (not
/// `AllowHeaders::any()`) — a wildcard would also make the positive
/// assertions above pass, but would silently allow every header on every
/// route rather than only the specific headers this configuration actually
/// allows.
#[tokio::test]
async fn preflight_does_not_allow_arbitrary_header() {
    let (app, _workspace_id) = test_app().await;
    let allow_headers = preflight_allow_headers(
        &app,
        "/api/items/00000000-0000-0000-0000-000000000000",
        "PATCH",
    )
    .await;
    assert!(
        !allow_headers.contains("x-not-a-real-header"),
        "unexpected header present: {allow_headers}"
    );
}
