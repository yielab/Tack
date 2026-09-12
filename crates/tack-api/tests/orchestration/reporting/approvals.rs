//! Tests for `GET /api/approvals` and `POST /api/approvals/{token}`.
//!
//! Covers: the off/token-gate guards; the fleet-wide inbox is oldest-first
//! and includes uncorrelated (`item_id: null`) approvals, enriched with
//! control-plane/item/project context; `grant_available` reflects whether
//! `TACK_ORCH_APPROVAL_TOKEN` is configured; `channel: "tack"` reaching
//! docket on the wire for both grant and deny, removing the row from the
//! pending inbox; an already-decided token (docket 409) surfacing as 409,
//! not 500; and an unknown token 404ing without ever calling docket.

use crate::common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use chrono::Utc;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::router::{AppState, build_router};
use tack_db::repo::orch::NewOrchApproval;
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use uuid::Uuid;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const APPROVAL_HEADER: &str = "x-tack-approval-token";

// ─── Helpers (mirrors orchestration/dispatch/item.rs) ──────────────────────

fn orch_config() -> AppConfig {
    AppConfig {
        orch_enable: true,
        ..AppConfig::default()
    }
}

fn orch_config_with_approval_token(token: &str) -> AppConfig {
    AppConfig {
        orch_enable: true,
        orch_approval_token: Some(token.to_string()),
        ..AppConfig::default()
    }
}

async fn app_with_state(config: AppConfig) -> (Router, AppState) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");

    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'CI Workspace', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert workspace");

    let (tx, _rx) = broadcast::channel(16);
    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        ..config
    };
    let state = AppState {
        repo: Repository::new(pool),
        config,
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        orch_runtime: OrchRuntime::new(),
        local_runner: None,
    };

    (build_router(state.clone()), state)
}

async fn body_json(res: axum::response::Response) -> Value {
    let bytes = to_bytes(res.into_body(), 4 * 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn req(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
    approval_header: Option<&str>,
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(tok) = approval_header {
        builder = builder.header(APPROVAL_HEADER, tok);
    }
    let body = match body {
        Some(v) => {
            builder = builder.header("Content-Type", "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    use tower::ServiceExt;
    app.clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap()
}

async fn create_control_plane(app: &Router, base_url: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        "/api/control-planes",
        Some(json!({"name": "docket-1", "base_url": base_url})),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn create_item(app: &Router, project_id: Uuid, title: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/items"),
        Some(json!({"title": title})),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn list_approvals(app: &Router) -> axum::response::Response {
    req(app, Method::GET, "/api/approvals", None, None).await
}

async fn decide(
    app: &Router,
    token: &str,
    action: &str,
    approval_header: Option<&str>,
) -> axum::response::Response {
    req(
        app,
        Method::POST,
        &format!("/api/approvals/{token}"),
        Some(json!({"action": action})),
        approval_header,
    )
    .await
}

async fn seed_pending_approval(state: &AppState, control_plane_id: Uuid, token: &str) {
    state
        .repo
        .upsert_orch_approvals(
            control_plane_id,
            &[NewOrchApproval {
                token: token.into(),
                item_id: None,
                remote_task_id: None,
                agent: Some("builder".into()),
                action: Some("git push".into()),
                state: "pending".into(),
                requested_at: Utc::now(),
                decided_at: None,
            }],
        )
        .await
        .expect("seed pending approval");
}

// ─── Off by default / actionable refusal ───────────────────────────────────

#[tokio::test]
async fn list_approvals_409s_when_orch_disabled() {
    let (app, _) = common::test_app().await; // orch_enable defaults to false
    let res = list_approvals(&app).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");
}

#[tokio::test]
async fn decide_approval_409s_when_orch_disabled() {
    let (app, _) = common::test_app().await;
    let res = decide(&app, "apr-1", "grant", Some("whatever")).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");
}

// ─── GET /api/approvals — inbox contents ───────────────────────────────────

#[tokio::test]
async fn inbox_is_oldest_first_includes_uncorrelated_with_context() {
    let (app, state) = app_with_state(orch_config()).await;
    let control_plane_id = create_control_plane(&app, "http://docket.local:9999").await;
    let project_id = common::create_project(&app, "Approvals Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Deploy service").await;

    state
        .repo
        .upsert_orch_approvals(
            control_plane_id,
            &[
                NewOrchApproval {
                    token: "apr-uncorrelated".into(),
                    item_id: None,
                    remote_task_id: None,
                    agent: Some("cli-agent".into()),
                    action: Some("rm -rf /tmp/build".into()),
                    state: "pending".into(),
                    requested_at: Utc::now() - chrono::Duration::seconds(60),
                    decided_at: None,
                },
                NewOrchApproval {
                    token: "apr-correlated".into(),
                    item_id: Some(item_id),
                    remote_task_id: Some("task-1".into()),
                    agent: Some("builder".into()),
                    action: Some("git push origin main".into()),
                    state: "pending".into(),
                    requested_at: Utc::now(),
                    decided_at: None,
                },
            ],
        )
        .await
        .expect("seed approvals");

    let res = list_approvals(&app).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);

    // Oldest first.
    assert_eq!(rows[0]["token"], "apr-uncorrelated");
    assert!(
        rows[0]["item_id"].is_null(),
        "uncorrelated approval must still appear"
    );
    assert!(rows[0]["item_title"].is_null());
    assert!(rows[0]["project_name"].is_null());
    assert_eq!(rows[0]["agent"], "cli-agent");

    assert_eq!(rows[1]["token"], "apr-correlated");
    assert_eq!(rows[1]["item_id"], item_id.to_string());
    assert_eq!(rows[1]["item_title"], "Deploy service");
    assert_eq!(rows[1]["action"], "git push origin main");
    assert!(rows[1]["control_plane_name"].is_string());

    // grant_available reflects config (no TACK_ORCH_APPROVAL_TOKEN here).
    assert_eq!(v["grant_available"], false);
}

#[tokio::test]
async fn inbox_grant_available_is_true_when_approval_token_configured() {
    let (app, _) = app_with_state(orch_config_with_approval_token("secret-1")).await;
    let res = list_approvals(&app).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["grant_available"], true);
    assert_eq!(v["rows"].as_array().unwrap().len(), 0);
}

// ─── POST /api/approvals/{token} — the approval-token gate ────────────────

/// Every way the approval-token gate fails closed: with the token unset in
/// config at all (the safe default — no client-supplied header value can
/// ever satisfy an unset secret), and with a token configured but the
/// header missing or wrong.
#[tokio::test]
async fn decide_approval_403s_when_token_unset_missing_or_wrong() {
    struct Case {
        config_token: Option<&'static str>,
        request_token: Option<&'static str>,
    }
    let cases = [
        Case {
            config_token: None,
            request_token: Some("anything-at-all"),
        },
        Case {
            config_token: None,
            request_token: None,
        },
        Case {
            config_token: Some("correct-secret"),
            request_token: None,
        },
        Case {
            config_token: Some("correct-secret"),
            request_token: Some("wrong-secret"),
        },
    ];

    for case in cases {
        let config = match case.config_token {
            Some(t) => orch_config_with_approval_token(t),
            None => orch_config(),
        };
        let (app, state) = app_with_state(config).await;
        let cp = create_control_plane(&app, "http://docket.local:9999").await;
        seed_pending_approval(&state, cp, "apr-1").await;

        let res = decide(&app, "apr-1", "grant", case.request_token).await;
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "config_token={:?} request_token={:?}",
            case.config_token,
            case.request_token
        );
    }
}

#[tokio::test]
async fn decide_unknown_token_404s_before_calling_docket() {
    let server = MockServer::start().await;
    // No mocks registered — if the handler called docket before checking its
    // own mirror, this test would fail on the unmatched request.
    let (app, _) = app_with_state(orch_config_with_approval_token("secret")).await;
    let _ = &server;

    let res = decide(&app, "apr-does-not-exist", "grant", Some("secret")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

// ─── POST /api/approvals/{token} — the real decision, proxied to docket ───

/// Both grant and deny send `channel: "tack"` on the wire alongside the
/// chosen action, and both remove the decided row from the pending inbox.
#[tokio::test]
async fn decide_approval_sends_action_and_channel_removes_from_inbox() {
    struct Case {
        token: &'static str,
        action: &'static str,
        expect_state: &'static str,
    }
    let cases = [
        Case {
            token: "apr-grant",
            action: "grant",
            expect_state: "granted",
        },
        Case {
            token: "apr-deny",
            action: "deny",
            expect_state: "denied",
        },
    ];

    for case in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(format!("/approvals/{}", case.token)))
            .and(body_partial_json(
                json!({"action": case.action, "channel": "tack"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "ok": true, "token": case.token, "state": case.expect_state
            })))
            .mount(&server)
            .await;

        let (app, state) = app_with_state(orch_config_with_approval_token("secret")).await;
        let cp = create_control_plane(&app, &server.uri()).await;
        seed_pending_approval(&state, cp, case.token).await;

        let res = decide(&app, case.token, case.action, Some("secret")).await;
        assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);

        let row = state
            .repo
            .get_orch_approval(case.token)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.state, case.expect_state);
        assert!(row.decided_at.is_some());

        // No longer in the pending inbox.
        let list_res = list_approvals(&app).await;
        let v = body_json(list_res).await;
        assert_eq!(v["rows"].as_array().unwrap().len(), 0);
    }
}

#[tokio::test]
async fn decide_approval_already_decided_surfaces_as_409() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/approvals/apr-stale"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "ok": false, "error": "Already granted: apr-stale"
        })))
        .mount(&server)
        .await;

    let (app, state) = app_with_state(orch_config_with_approval_token("secret")).await;
    let cp = create_control_plane(&app, &server.uri()).await;
    seed_pending_approval(&state, cp, "apr-stale").await;

    let res = decide(&app, "apr-stale", "grant", Some("secret")).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let v = body_json(res).await;
    let message = v["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("Already granted"), "{message}");
}

#[tokio::test]
async fn decide_approval_unknown_token_on_docket_side_surfaces_as_404() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/approvals/apr-ghost"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "ok": false, "error": "Approval not found: apr-ghost"
        })))
        .mount(&server)
        .await;

    let (app, state) = app_with_state(orch_config_with_approval_token("secret")).await;
    let cp = create_control_plane(&app, &server.uri()).await;
    // Tack's own mirror has the row (otherwise the handler 404s before ever
    // calling docket, per the test above) — this covers docket itself
    // reporting the token unknown on its side.
    seed_pending_approval(&state, cp, "apr-ghost").await;

    let res = decide(&app, "apr-ghost", "grant", Some("secret")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
