//! Tests for the orchestration control-plane API.
//!
//! Covers: every route returns a 409 with `error.code: "orchestration_disabled"`
//! while the effective orchestration setting is off, and
//! `/api/settings/orchestration` itself stays reachable throughout; the
//! docket token never appears in a response body, in any shape (create,
//! list, get, patch); the tri-state PATCH semantics for `token`
//! (absent/null/value); `orch-link` save-time validation against the
//! project's workflow; and the `/api/fleet` unreachable-vs-zero distinction.

use crate::common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::router::{AppState, build_router};
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn orch_config() -> AppConfig {
    AppConfig {
        orch_enable: true,
        ..AppConfig::default()
    }
}

/// Like `common::test_app_with_config`, but also hands back the `AppState` so
/// tests can reach into the repo directly (e.g. to simulate the reconciler
/// writing a health outcome) without going through HTTP.
async fn app_with_state(config: AppConfig) -> (Router, AppState, Uuid) {
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

    (build_router(state.clone()), state, workspace_id)
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
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(v) => {
            builder = builder.header("Content-Type", "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    app.clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap()
}

async fn create_project(app: &Router) -> Uuid {
    let res = req(
        app,
        Method::POST,
        "/api/projects",
        Some(json!({"name": "Orch Test Project", "project_type": "software"})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn create_control_plane(app: &Router, token: Option<&str>) -> Value {
    let mut body = json!({"name": "docket-1", "base_url": "http://docket.local:9999"});
    if let Some(t) = token {
        body["token"] = json!(t);
    }
    let res = req(app, Method::POST, "/api/control-planes", Some(body)).await;
    assert_eq!(res.status(), StatusCode::OK);
    body_json(res).await
}

/// A response body must not contain the literal secret anywhere, and must not
/// have a `token` key at all — guards against both a leaked value and a
/// leaked-but-null field shape.
fn assert_no_token_leak(v: &Value, secret: &str) {
    let raw = v.to_string();
    assert!(
        !raw.contains(secret),
        "response leaked the control-plane token: {raw}"
    );
    assert!(
        v.get("token").is_none(),
        "response must not have a `token` key at all: {raw}"
    );
}

// ─── Off by default ────────────────────────────────────

#[tokio::test]
async fn every_orch_route_409s_with_a_stable_code_when_disabled() {
    let (app, _) = common::test_app().await; // orch_enable defaults to false
    let project_id = create_project(&app).await;
    let fake_id = Uuid::new_v4();

    let cases: Vec<(Method, String)> = vec![
        (Method::GET, "/api/control-planes".into()),
        (Method::POST, "/api/control-planes".into()),
        (Method::GET, format!("/api/control-planes/{fake_id}")),
        (Method::PATCH, format!("/api/control-planes/{fake_id}")),
        (Method::DELETE, format!("/api/control-planes/{fake_id}")),
        (Method::GET, format!("/api/projects/{project_id}/orch-link")),
        (Method::PUT, format!("/api/projects/{project_id}/orch-link")),
        (Method::GET, "/api/fleet".into()),
    ];

    for (method, uri) in cases {
        let res = req(&app, method.clone(), &uri, None).await;
        assert_eq!(
            res.status(),
            StatusCode::CONFLICT,
            "{method} {uri} should 409 (actionable, not a bare 404) while orchestration is disabled"
        );
        let body = body_json(res).await;
        assert_eq!(
            body["error"]["code"], "orchestration_disabled",
            "{method} {uri} response must carry the stable machine-readable code"
        );
        let message = body["error"]["message"].as_str().unwrap_or_default();
        assert!(
            message.contains("/api/settings/orchestration"),
            "{method} {uri} message should name where to enable orchestration: {message}"
        );
    }
}

/// `/api/settings/orchestration` itself is the one orchestration-adjacent
/// route that must stay reachable while the feature is off — see
/// `router.rs`'s comment on that route registration.
#[tokio::test]
async fn settings_orchestration_is_reachable_when_disabled() {
    let (app, _) = common::test_app().await; // orch_enable defaults to false
    let res = req(&app, Method::GET, "/api/settings/orchestration", None).await;
    assert_eq!(res.status(), StatusCode::OK);
    let body = body_json(res).await;
    assert_eq!(body["enabled"], false);
    assert_eq!(body["source"], "env_default");
}

#[tokio::test]
async fn orch_routes_are_reachable_when_enabled() {
    let (app, _) = common::test_app_with_config(orch_config()).await;
    let res = req(&app, Method::GET, "/api/control-planes", None).await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = req(&app, Method::GET, "/api/fleet", None).await;
    assert_eq!(res.status(), StatusCode::OK);
}

// ─── Token discipline ───────────────────────────────────────────────────────

#[tokio::test]
async fn token_never_appears_in_create_response() {
    let (app, _) = common::test_app_with_config(orch_config()).await;
    let secret = "docket-super-secret-token";
    let created = create_control_plane(&app, Some(secret)).await;
    assert_no_token_leak(&created, secret);
    assert_eq!(created["token_set"], true);
}

#[tokio::test]
async fn token_never_appears_in_list_or_get_response() {
    let (app, _) = common::test_app_with_config(orch_config()).await;
    let secret = "another-docket-secret";
    let created = create_control_plane(&app, Some(secret)).await;
    let id = created["id"].as_str().unwrap();

    let list_res = req(&app, Method::GET, "/api/control-planes", None).await;
    let list = body_json(list_res).await;
    assert_no_token_leak(&list, secret);

    let get_res = req(
        &app,
        Method::GET,
        &format!("/api/control-planes/{id}"),
        None,
    )
    .await;
    assert_eq!(get_res.status(), StatusCode::OK);
    let got = body_json(get_res).await;
    assert_no_token_leak(&got, secret);
    assert_eq!(got["token_set"], true);
}

#[tokio::test]
async fn patch_with_absent_token_field_preserves_stored_token() {
    let (app, _) = common::test_app_with_config(orch_config()).await;
    let created = create_control_plane(&app, Some("preserve-me")).await;
    let id = created["id"].as_str().unwrap();

    // Patch only the name — no `token` key in the body at all.
    let res = req(
        &app,
        Method::PATCH,
        &format!("/api/control-planes/{id}"),
        Some(json!({"name": "docket-renamed"})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let updated = body_json(res).await;
    assert_eq!(updated["name"], "docket-renamed");
    assert_eq!(
        updated["token_set"], true,
        "token must survive a patch that never mentions it"
    );
    assert_no_token_leak(&updated, "preserve-me");
}

#[tokio::test]
async fn patch_with_explicit_null_token_clears_it() {
    let (app, _) = common::test_app_with_config(orch_config()).await;
    let created = create_control_plane(&app, Some("clear-me")).await;
    let id = created["id"].as_str().unwrap();

    let res = req(
        &app,
        Method::PATCH,
        &format!("/api/control-planes/{id}"),
        Some(json!({"token": null})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let updated = body_json(res).await;
    assert_eq!(
        updated["token_set"], false,
        "an explicit null must clear the stored token"
    );
}

#[tokio::test]
async fn patch_with_token_value_replaces_it() {
    let (app, _) = common::test_app_with_config(orch_config()).await;
    let created = create_control_plane(&app, Some("old-token")).await;
    let id = created["id"].as_str().unwrap();

    let res = req(
        &app,
        Method::PATCH,
        &format!("/api/control-planes/{id}"),
        Some(json!({"token": "new-token"})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let updated = body_json(res).await;
    assert_eq!(updated["token_set"], true);
    assert_no_token_leak(&updated, "old-token");
    assert_no_token_leak(&updated, "new-token");
}

#[tokio::test]
async fn delete_removes_control_plane() {
    let (app, _) = common::test_app_with_config(orch_config()).await;
    let created = create_control_plane(&app, None).await;
    let id = created["id"].as_str().unwrap();

    let res = req(
        &app,
        Method::DELETE,
        &format!("/api/control-planes/{id}"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);

    let res = req(
        &app,
        Method::GET,
        &format!("/api/control-planes/{id}"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_unknown_control_plane_is_404() {
    let (app, _) = common::test_app_with_config(orch_config()).await;
    let res = req(
        &app,
        Method::GET,
        &format!("/api/control-planes/{}", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

// ─── orch-link ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn orch_link_absent_by_default() {
    let (app, _) = common::test_app_with_config(orch_config()).await;
    let project_id = create_project(&app).await;

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/orch-link"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["linked"], false);
    assert!(v["link"].is_null());
}

#[tokio::test]
async fn orch_link_round_trips_with_valid_status_map() {
    let (app, _) = common::test_app_with_config(orch_config()).await;
    let project_id = create_project(&app).await;
    let plane = create_control_plane(&app, None).await;
    let plane_id = plane["id"].as_str().unwrap();

    // The default "software" project type uses the scrum workflow, whose
    // statuses include "To Do" and "Done" (see tack-core's workflow presets).
    let res = req(
        &app,
        Method::PUT,
        &format!("/api/projects/{project_id}/orch-link"),
        Some(json!({
            "control_plane_id": plane_id,
            "remote_project": "my-remote-project",
            "auto_dispatch": true,
            "budget_usd": 50.0,
            "status_map": {
                "dispatch_from": ["To Do"],
                "on_succeeded": "Done"
            }
        })),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
}

#[tokio::test]
async fn orch_link_rejects_unknown_status_name() {
    let (app, _) = common::test_app_with_config(orch_config()).await;
    let project_id = create_project(&app).await;
    let plane = create_control_plane(&app, None).await;
    let plane_id = plane["id"].as_str().unwrap();

    let res = req(
        &app,
        Method::PUT,
        &format!("/api/projects/{project_id}/orch-link"),
        Some(json!({
            "control_plane_id": plane_id,
            "remote_project": "my-remote-project",
            "status_map": { "dispatch_from": ["Not A Real Status"] }
        })),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn orch_link_get_reflects_saved_link() {
    let (app, _) = common::test_app_with_config(orch_config()).await;
    let project_id = create_project(&app).await;
    let plane = create_control_plane(&app, None).await;
    let plane_id = plane["id"].as_str().unwrap();

    let put_res = req(
        &app,
        Method::PUT,
        &format!("/api/projects/{project_id}/orch-link"),
        Some(json!({
            "control_plane_id": plane_id,
            "remote_project": "my-remote-project",
            "status_map": {}
        })),
    )
    .await;
    assert_eq!(put_res.status(), StatusCode::OK);

    let get_res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/orch-link"),
        None,
    )
    .await;
    let v = body_json(get_res).await;
    assert_eq!(v["linked"], true);
    assert_eq!(v["link"]["remote_project"], "my-remote-project");
    assert_eq!(v["link"]["control_plane_id"], plane_id);
}

/// The wire label must come from `legacy_bridge`'s constant, never a
/// re-typed literal — a copied string would silently stop tracking the
/// decision if the constant ever changed.
#[tokio::test]
async fn orch_link_carries_the_legacy_docket_compatibility_constants() {
    let (app, _) = common::test_app_with_config(orch_config()).await;
    let project_id = create_project(&app).await;
    let plane = create_control_plane(&app, None).await;
    let plane_id = plane["id"].as_str().unwrap();

    let put_res = req(
        &app,
        Method::PUT,
        &format!("/api/projects/{project_id}/orch-link"),
        Some(json!({
            "control_plane_id": plane_id,
            "remote_project": "my-remote-project",
            "status_map": {}
        })),
    )
    .await;
    assert_eq!(put_res.status(), StatusCode::OK);
    let put_body = body_json(put_res).await;
    assert_eq!(
        put_body["compatibility_label"],
        tack_orch::adapters::legacy_bridge::LEGACY_DOCKET_COMPATIBILITY_LABEL
    );
    assert_eq!(
        put_body["compatibility_policy"],
        tack_orch::adapters::legacy_bridge::LEGACY_DOCKET_COMPATIBILITY_POLICY
    );

    let get_res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/orch-link"),
        None,
    )
    .await;
    let v = body_json(get_res).await;
    assert_eq!(
        v["link"]["compatibility_label"],
        tack_orch::adapters::legacy_bridge::LEGACY_DOCKET_COMPATIBILITY_LABEL
    );
    assert_eq!(
        v["link"]["compatibility_policy"],
        tack_orch::adapters::legacy_bridge::LEGACY_DOCKET_COMPATIBILITY_POLICY
    );
}

// ─── Fleet aggregate ────────────────────────────────────────────────────────

#[tokio::test]
async fn fleet_is_empty_with_no_links() {
    let (app, _, _) = app_with_state(orch_config()).await;
    let res = req(&app, Method::GET, "/api/fleet", None).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["rows"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn fleet_reports_zero_cost_distinctly_from_unreachable() {
    let (app, state, _) = app_with_state(orch_config()).await;
    let project_id = create_project(&app).await;
    let plane = create_control_plane(&app, None).await;
    let plane_id = Uuid::parse_str(plane["id"].as_str().unwrap()).unwrap();

    req(
        &app,
        Method::PUT,
        &format!("/api/projects/{project_id}/orch-link"),
        Some(json!({
            "control_plane_id": plane_id,
            "remote_project": "my-remote-project",
            "status_map": {}
        })),
    )
    .await;

    // Freshly created plane: health defaults to "unknown" (not yet polled),
    // which is reachable-enough to report a real, current zero.
    let res = req(&app, Method::GET, "/api/fleet", None).await;
    let v = body_json(res).await;
    let entries = v["rows"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["health"], "unknown");
    assert_eq!(
        entries[0]["cost_usd_estimated"],
        json!(0.0),
        "a reachable plane with nothing dispatched yet must report a real Some(0.0), not null"
    );
    assert_eq!(entries[0]["tokens_in"], json!(0));
    assert_eq!(entries[0]["tokens_out"], json!(0));
    assert_eq!(entries[0]["pending_approval_count"], json!(0));
    assert!(entries[0]["last_activity_at"].is_null());
    assert_eq!(entries[0]["gateway"], "unknown");
    assert_eq!(entries[0]["roster"], json!([]));

    // Now simulate the reconciler marking the plane unreachable.
    state
        .repo
        .update_control_plane_health(plane_id, "unreachable", None, 10, None)
        .await
        .expect("record health");

    let res = req(&app, Method::GET, "/api/fleet", None).await;
    let v = body_json(res).await;
    let entries = v["rows"].as_array().unwrap();
    assert_eq!(entries[0]["health"], "unreachable");
    assert!(
        entries[0]["cost_usd_estimated"].is_null(),
        "an unreachable plane's cost must be None/null (stale), never a confident zero: {:?}",
        entries[0]
    );
}
