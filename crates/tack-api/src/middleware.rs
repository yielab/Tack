use axum::{
    extract::{Request, State},
    http::{HeaderName, HeaderValue, StatusCode, header},
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};

use crate::router::AppState;

fn is_public_route(path: &str) -> bool {
    matches!(
        path,
        "/api/health" | "/api/openapi.json" | "/health" | "/openapi.json"
    )
}

/// Browser WebSockets cannot attach `Authorization`. The token middleware
/// checks the dedicated subprotocol during the upgrade; this narrow route
/// recognizer prevents a suffix lookalike from bypassing auth.
fn is_board_websocket_route(path: &str) -> bool {
    let Some(project) = path
        .strip_prefix("/api/projects/")
        .or_else(|| path.strip_prefix("/projects/"))
        .and_then(|rest| rest.strip_suffix("/boards/live"))
    else {
        return false;
    };
    !project.contains('/') && uuid::Uuid::parse_str(project).is_ok()
}

const AUTH_PROTOCOL_PREFIX: &str = "tack.auth.";

/// True when the browser `Origin` header names a loopback host. A remote
/// page cannot forge this: a browser sets `Origin` from the script's own
/// origin, never from where a URL later resolves, so this only ever matches
/// a page that is itself running on `localhost`/`127.0.0.1`.
fn origin_is_loopback(origin: &str) -> bool {
    reqwest::Url::parse(origin)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .is_some_and(|host| crate::config::is_loopback_host(&host))
}

/// Checks the browser `Origin` header against `TACK_ALLOWED_ORIGINS`. When
/// the server itself is bound to loopback, an `Origin` that also names a
/// loopback host is accepted even if it isn't in the configured list — on a
/// single machine, whoever can open this connection already holds the same
/// access as the bind itself (the posture ADR 0059 states for the rest of
/// this API). This is what lets any local dev port (Vite's default `5173`
/// included) reach the socket with no configuration; it does not change
/// what a non-loopback bind accepts, since that check still requires an
/// exact match against the configured list.
fn board_websocket_is_authorized(req: &Request, state: &AppState) -> bool {
    if let Some(origin) = req.headers().get(header::ORIGIN) {
        let Ok(origin) = origin.to_str() else {
            return false;
        };
        let listed = state
            .config
            .allowed_origins
            .iter()
            .any(|allowed| allowed == origin);
        let same_machine = state.config.binds_loopback() && origin_is_loopback(origin);
        if !listed && !same_machine {
            return false;
        }
    }

    let Some(expected) = state.config.api_token.as_deref() else {
        return true;
    };
    let provided = req
        .headers()
        .get_all(header::SEC_WEBSOCKET_PROTOCOL)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(',').map(str::trim))
        .find_map(|protocol| protocol.strip_prefix(AUTH_PROTOCOL_PREFIX))
        .and_then(decode_base64url);
    provided
        .as_deref()
        .is_some_and(|token| constant_time_eq(token, expected.as_bytes()))
}

/// Decode the unpadded base64url form permitted by a WebSocket subprotocol.
/// `=` padding is intentionally rejected because browser WebSocket APIs only
/// permit HTTP-token characters in a subprotocol name.
fn decode_base64url(value: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(value.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in value.bytes() {
        let six_bits = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(six_bits);
        bits += 6;
        while bits >= 8 {
            bits -= 8;
            output.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    if bits == 6 { None } else { Some(output) }
}

/// Middleware: if `TACK_API_TOKEN` is configured, require a matching
/// `Authorization: Bearer <token>` header on every request except `/api/health`.
///
/// Uses a constant-time comparison to prevent timing-oracle attacks. This is
/// the only credential a request can hold — there is no per-user account to
/// authenticate as instead — a deliberate single-operator scope recorded in
/// `docs/adr/0059-single-operator-identity-posture.md`, not an omission.
pub async fn require_token(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();
    if is_public_route(path) {
        return Ok(next.run(req).await);
    }

    if is_board_websocket_route(path) {
        return if board_websocket_is_authorized(&req, &state) {
            Ok(next.run(req).await)
        } else {
            Err(StatusCode::UNAUTHORIZED)
        };
    }

    let Some(ref expected) = state.config.api_token else {
        // No token configured: pure-local mode, allow everything. A
        // non-loopback bind reaching this branch is refused earlier, at
        // startup, by `AppConfig::validate_security` — this function never
        // has to tell "no token, loopback" apart from "no token, exposed".
        return Ok(next.run(req).await);
    };

    let provided = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match provided {
        Some(tok) if constant_time_eq(tok.as_bytes(), expected.as_bytes()) => {
            Ok(next.run(req).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Header the operator execution/fleet handlers
/// (`crate::handlers::executions`, `crate::handlers::runner_admin`) read to
/// scope idempotency and audit `actor` fields. Those handlers trust it
/// completely, so it must never be attacker-controlled — see the strip-and-
/// replace below, which is what makes that true.
pub const OPERATOR_PRINCIPAL_HEADER: &str = "x-tack-principal";

/// Derives the operator principal from the request's already-authenticated
/// context rather than anything the client sent. Tack's operator-auth model
/// (`docs/contracts/runner-v1/protocol.json`'s `operator_session_or_api_token`)
/// is a single shared bearer token, not per-user sessions, so every request
/// that clears `require_token` with the same configured token is
/// structurally the same principal; a stable, non-secret identifier derived
/// from the configured secret (or a fixed local identifier when no token is
/// configured, i.e. `require_token` itself allows every caller through) is
/// therefore both correct today and forward-compatible if the operator-auth
/// model later grows real per-caller sessions — the injection point does not
/// change, only what this function returns.
fn operator_principal_value(config: &crate::config::AppConfig) -> String {
    match config.api_token.as_deref() {
        Some(token) if !token.is_empty() => {
            let digest = Sha256::digest(token.as_bytes());
            format!("operator:token:{}", hex::encode(&digest[..8]))
        }
        _ => "operator:local".to_string(),
    }
}

/// Strips any client-supplied [`OPERATOR_PRINCIPAL_HEADER`] and replaces it
/// with the server-derived value. Layered directly on the operator
/// execution/fleet sub-router in `router.rs`, *inside* the `require_token`
/// gate, so it only ever runs for a request that already cleared operator
/// authentication (or ran in unauthenticated local-only mode, where every
/// caller is equally trusted). A request arriving here with its own
/// `x-tack-principal` set is not an error — it is simply overwritten, so a
/// client cannot read or collide with another principal's idempotency scope
/// by guessing or copying a header value.
pub async fn inject_operator_principal(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let principal = operator_principal_value(&state.config);
    let value = HeaderValue::from_str(&principal)
        .expect("operator principal is hex/ASCII and always a valid header value");
    req.headers_mut()
        .insert(HeaderName::from_static(OPERATOR_PRINCIPAL_HEADER), value);
    next.run(req).await
}

/// Byte-wise constant-time equality. Processes all bytes without early exit
/// so the comparison time does not reveal how many bytes matched.
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    // std::hint::black_box prevents the compiler from optimising away the fold.
    std::hint::black_box(
        a.iter()
            .zip(b.iter())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        constant_time_eq, decode_base64url, is_board_websocket_route, is_public_route,
        operator_principal_value,
    };
    use crate::config::AppConfig;

    #[test]
    fn equal_bytes_match() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn different_bytes_no_match() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn different_lengths_no_match() {
        assert!(!constant_time_eq(b"short", b"longer"));
    }

    #[test]
    fn empty_bytes_match() {
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn only_exact_public_routes_bypass_bearer_auth() {
        assert!(is_public_route("/api/health"));
        assert!(!is_public_route("/api/projects/health"));
    }

    #[test]
    fn only_the_exact_board_websocket_shape_defers_to_upgrade_auth() {
        assert!(is_board_websocket_route(
            "/api/projects/00000000-0000-0000-0000-000000000000/boards/live"
        ));
        assert!(!is_board_websocket_route(
            "/api/projects/not-a-uuid/boards/live"
        ));
        assert!(!is_board_websocket_route(
            "/api/projects/health/boards/live"
        ));
        assert!(!is_board_websocket_route(
            "/api/projects/00000000-0000-0000-0000-000000000000/boards/live-evil"
        ));
    }

    #[test]
    fn websocket_subprotocol_credentials_decode_without_query_parameters() {
        assert_eq!(
            decode_base64url("b3BlcmF0b3ItdG9rZW4"),
            Some(b"operator-token".to_vec())
        );
        assert_eq!(decode_base64url("not=valid"), None);
    }

    /// The runner-v1 router and the operator execution/fleet
    /// router each authenticate with their own distinct
    /// credential type and must never be listed here — `require_token`'s
    /// exemption check must stay exact-match, and neither router is mounted
    /// inside the `require_token`-gated sub-app in `router.rs` in the first
    /// place (see `build_router`'s `runner_router` nest, which sits outside
    /// the `api` router's `require_token` layer entirely). This test pins
    /// the narrower, directly-checkable half of that guarantee: even if a
    /// future edit accidentally added one of these paths to this list, a
    /// suffix/prefix lookalike must still not slip through.
    #[test]
    fn no_runner_or_execution_path_is_publicly_exempt() {
        for path in [
            "/api/runner/v1/enroll",
            "/api/runner/v1/refresh",
            "/api/runner/v1/claim",
            "/api/runner/v1/heartbeat",
            "/api/runner/v1/attempts/att_1/events",
            "/api/runner/v1/attempts/att_1/recovery-observation",
            "/runner/v1/enroll",
            "/api/executions",
            "/api/executions/exec_1",
            "/api/executions/exec_1/cancel",
            "/api/executions/exec_1/requeue",
            "/api/runner-fleets",
            "/api/runners/enrollment",
            "/api/runners/runr_1/revoke",
            "/api/agent-profiles",
            "/api/model-profiles",
            "/executions",
            "/runner-fleets",
        ] {
            assert!(!is_public_route(path), "unexpectedly exempt: {path}");
        }
        // Suffix/prefix lookalikes of the routes that genuinely *are* public
        // must still be rejected — the existing behavior this test also
        // guards against regressing while adding the rows above.
        for lookalike in [
            "/api/healthy",
            "/api/health/",
            "/api/openapi.json.evil",
            "/api/health2",
        ] {
            assert!(
                !is_public_route(lookalike),
                "unexpectedly exempt: {lookalike}"
            );
        }
    }

    #[test]
    fn operator_principal_is_stable_and_never_the_raw_token() {
        let no_token = AppConfig::default();
        let first = operator_principal_value(&no_token);
        let second = operator_principal_value(&no_token);
        assert_eq!(first, second);
        assert_eq!(first, "operator:local");

        let with_token = AppConfig {
            api_token: Some("super-secret-value".into()),
            ..AppConfig::default()
        };
        let derived = operator_principal_value(&with_token);
        assert_ne!(derived, "operator:local");
        assert!(!derived.contains("super-secret-value"));
        assert_eq!(derived, operator_principal_value(&with_token));

        let different_token = AppConfig {
            api_token: Some("another-secret".into()),
            ..AppConfig::default()
        };
        assert_ne!(derived, operator_principal_value(&different_token));
    }
}
