//! The operator bearer-token boundary: a path that only looks like an
//! unauthenticated route (a suffix appended to `/api/projects`) still lands
//! behind `require_token`, `/api/health` stays open, the response carries a
//! CSP that disallows inline/executable content, and a WebSocket handshake
//! driven over a raw TCP connection authorizes via the subprotocol-embedded
//! credential with no token in the query string.

use crate::common;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use tack_api::config::AppConfig;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::{Duration, timeout},
};
use tower::ServiceExt;

const API_TOKEN: &str = "secret-token";
const SPLIT_ORIGIN: &str = "https://app.example.test";

fn protected_config() -> AppConfig {
    AppConfig {
        api_token: Some(API_TOKEN.into()),
        allowed_origins: vec![SPLIT_ORIGIN.into()],
        ..AppConfig::default()
    }
}

#[tokio::test]
async fn suffix_lookalikes_stay_behind_the_bearer_gate() {
    let (app, _) = common::test_app_with_config(protected_config()).await;
    for uri in ["/api/projects/health", "/api/projects/openapi.json"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }

    let health = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
}

#[tokio::test]
async fn csp_disallows_executable_content() {
    let (app, _) = common::test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let csp = response
        .headers()
        .get(header::CONTENT_SECURITY_POLICY)
        .and_then(|value| value.to_str().ok())
        .expect("CSP response header");
    assert!(csp.contains("script-src 'self'"));
    assert!(csp.contains("object-src 'none'"));
}

#[tokio::test]
async fn split_origin_websocket_handshake_accepts_subprotocol_credential_without_query_token() {
    let (app, _) = common::test_app_with_config(protected_config()).await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let address = listener.local_addr().expect("test listener address");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let project = reqwest::Client::new()
        .post(format!("http://{address}/api/projects"))
        .bearer_auth(API_TOKEN)
        .json(&serde_json::json!({ "name": "WebSocket test", "project_type": "software" }))
        .send()
        .await
        .expect("create project over the real listener")
        .error_for_status()
        .expect("project creation must succeed")
        .json::<serde_json::Value>()
        .await
        .expect("project JSON");
    let project_id = project["id"].as_str().expect("project ID");

    let request_target = format!("/api/projects/{project_id}/boards/live");
    let request = format!(
        "GET {request_target} HTTP/1.1\r\n\
         Host: {address}\r\n\
         Origin: {SPLIT_ORIGIN}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Protocol: tack.v1, tack.auth.c2VjcmV0LXRva2Vu\r\n\r\n"
    );
    assert!(!request_target.contains('?'));
    assert!(!request.contains("Authorization:"));

    let mut stream = TcpStream::connect(address)
        .await
        .expect("connect WebSocket client");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("send browser-style WebSocket handshake");
    let mut buffer = [0_u8; 4096];
    let count = timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .expect("WebSocket handshake timed out")
        .expect("read WebSocket handshake");
    let response = String::from_utf8_lossy(&buffer[..count]).into_owned();
    server.abort();

    assert!(
        response.starts_with("HTTP/1.1 101"),
        "subprotocol credential should authorize the upgrade, got: {response}"
    );
}

/// A raw handshake reader (this test, or `curl`) only ever checks the status
/// line — see the assertion above, which is the shape of every other
/// WebSocket check in this file. That shape cannot tell a spec-compliant
/// handshake apart from one a real browser refuses to use: RFC 6455 §4.1
/// requires a client that offered a subprotocol to fail the connection when
/// the response omits `Sec-WebSocket-Protocol`, and nothing about the status
/// line changes either way. This test closes that gap on the wire level by
/// parsing the response headers and asserting the exact selected value; the
/// browser side of the same gap (proving a real client actually stays
/// connected) is proved separately in
/// `frontend/e2e/board-websocket-subprotocol.spec.ts`, which no Rust-only
/// test can stand in for.
#[tokio::test]
async fn board_live_handshake_selects_the_tack_v1_subprotocol() {
    let (app, _) = common::test_app().await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let address = listener.local_addr().expect("test listener address");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let project = reqwest::Client::new()
        .post(format!("http://{address}/api/projects"))
        .json(&serde_json::json!({ "name": "WS subprotocol test", "project_type": "software" }))
        .send()
        .await
        .expect("create project over the real listener")
        .error_for_status()
        .expect("project creation must succeed")
        .json::<serde_json::Value>()
        .await
        .expect("project JSON");
    let project_id = project["id"].as_str().expect("project ID");

    let request_target = format!("/api/projects/{project_id}/boards/live");
    let request = format!(
        "GET {request_target} HTTP/1.1\r\n\
         Host: {address}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Protocol: tack.v1\r\n\r\n"
    );

    let mut stream = TcpStream::connect(address)
        .await
        .expect("connect WebSocket client");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("send browser-style WebSocket handshake");
    let mut buffer = [0_u8; 4096];
    let count = timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .expect("WebSocket handshake timed out")
        .expect("read WebSocket handshake");
    let response = String::from_utf8_lossy(&buffer[..count]).into_owned();
    server.abort();

    assert!(
        response.starts_with("HTTP/1.1 101"),
        "handshake did not upgrade: {response}"
    );
    let header_line = response
        .lines()
        .find(|line| {
            line.to_ascii_lowercase()
                .starts_with("sec-websocket-protocol:")
        })
        .unwrap_or_else(|| panic!("response carries no Sec-WebSocket-Protocol header: {response}"));
    let value = header_line
        .split_once(':')
        .expect("header has a colon")
        .1
        .trim();
    assert_eq!(
        value, "tack.v1",
        "response selected the wrong subprotocol: {header_line}"
    );
}

/// Starts `app` on a real loopback listener, creates a project through it,
/// then drives a raw WebSocket handshake against that project's `boards/live`
/// route carrying `origin` (when given). Returns the raw HTTP response text
/// so callers can assert on the status line. Shared by the pair of tests
/// below and `board_live_handshake_selects_the_tack_v1_subprotocol` above.
async fn board_live_handshake_response(app: axum::Router, origin: Option<&str>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test listener");
    let address = listener.local_addr().expect("test listener address");
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let project = reqwest::Client::new()
        .post(format!("http://{address}/api/projects"))
        .json(&serde_json::json!({ "name": "WS origin test", "project_type": "software" }))
        .send()
        .await
        .expect("create project over the real listener")
        .error_for_status()
        .expect("project creation must succeed")
        .json::<serde_json::Value>()
        .await
        .expect("project JSON");
    let project_id = project["id"].as_str().expect("project ID");

    let origin_line = origin
        .map(|o| format!("Origin: {o}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "GET /api/projects/{project_id}/boards/live HTTP/1.1\r\n\
         Host: {address}\r\n\
         {origin_line}\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\
         Sec-WebSocket-Protocol: tack.v1\r\n\r\n"
    );

    let mut stream = TcpStream::connect(address)
        .await
        .expect("connect WebSocket client");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("send browser-style WebSocket handshake");
    let mut buffer = [0_u8; 4096];
    let count = timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .expect("WebSocket handshake timed out")
        .expect("read WebSocket handshake");
    let response = String::from_utf8_lossy(&buffer[..count]).into_owned();
    server.abort();
    response
}

/// Following the documented developer recipe (`tack serve` bound to its
/// loopback default, then `npm run dev`, which opens `http://localhost:5173`)
/// sends this exact `Origin` on the board's live handshake, and
/// `default_allowed_origins()` has never listed `5173`. A loopback-bound
/// server now recognizes any loopback-hosted `Origin` as same-machine and
/// authorizes it without needing it in `TACK_ALLOWED_ORIGINS`.
#[tokio::test]
async fn board_live_handshake_from_the_vite_dev_origin_is_authorized_on_a_loopback_bind() {
    let (app, _) = common::test_app().await;
    let response = board_live_handshake_response(app, Some("http://localhost:5173")).await;
    assert!(
        response.starts_with("HTTP/1.1 101"),
        "loopback bind should authorize a loopback browser origin: {response}"
    );
}

/// The same loopback `Origin` gets no special treatment once the server
/// itself is not loopback-bound — proving the fix widens only what a
/// loopback bind accepts, never what `TACK_ALLOWED_ORIGINS` means for a
/// bind reachable beyond this machine.
#[tokio::test]
async fn board_live_handshake_from_a_loopback_origin_is_still_refused_on_a_non_loopback_bind() {
    let (app, _) = common::test_app_with_config(AppConfig {
        host: "0.0.0.0".into(),
        ..AppConfig::default()
    })
    .await;
    let response = board_live_handshake_response(app, Some("http://localhost:5173")).await;
    assert!(
        !response.starts_with("HTTP/1.1 101"),
        "a non-loopback bind must still require an explicitly listed origin: {response}"
    );
}
