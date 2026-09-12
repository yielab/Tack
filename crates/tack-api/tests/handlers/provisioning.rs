//! Tests for `POST /api/templates/{id}/provision` — the end-to-end "create
//! a project from a template, provision a docket pod, link the two" flow,
//! and its rollback behavior on partial failure: a bad `status_map` or a
//! docket `400`/`409` each roll the project back (per
//! `core/pod_provisioning.py`'s "either fully created or nothing created"
//! contract), while an `orch_links` write failure *after* a successful
//! `POST /pods` leaves both standing as `pod_created_link_failed` — the one
//! step that is never rolled back, since the pod itself can't be
//! un-provisioned.

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
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ─── Helpers (mirrors orchestration/dispatch/item.rs /
// orchestration/reporting/approvals.rs) ─────────────────────────────────────

fn orch_config() -> AppConfig {
    AppConfig {
        orch_enable: true,
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

/// A template with no explicit `workflow` — falls back to `simple_workflow()`
/// ("To Do" / "Doing" / "Done"), same default
/// `orchestration/fleet_templates/templates.rs` relies on.
async fn create_template(app: &Router) -> Uuid {
    let res = req(
        app,
        Method::POST,
        "/api/templates",
        Some(json!({"name": "Provisioning Test Template", "project_type": "software"})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn create_control_plane(app: &Router, base_url: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        "/api/control-planes",
        Some(json!({"name": "docket-1", "base_url": base_url})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn count_projects(app: &Router) -> usize {
    let res = req(app, Method::GET, "/api/projects", None).await;
    assert_eq!(res.status(), StatusCode::OK);
    body_json(res).await.as_array().unwrap().len()
}

fn provision_body(template_ok: bool, control_plane_id: Uuid, remote_project: &str) -> Value {
    let mut status_map = json!({});
    if !template_ok {
        // References a status name that doesn't exist in simple_workflow().
        status_map = json!({"dispatch_from": ["Nonexistent Status"]});
    }
    json!({
        "name": "Provisioned Project",
        "provision_pod": {
            "control_plane_id": control_plane_id,
            "remote_project": remote_project,
            "blueprint": "software",
            "status_map": status_map,
        }
    })
}

// ─── Gating ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_project_with_pod_409s_when_orch_disabled() {
    let (app, _state) = app_with_state(AppConfig::default()).await;
    let template_id = create_template(&app).await;

    let res = req(
        &app,
        Method::POST,
        &format!("/api/templates/{template_id}/provision"),
        Some(provision_body(true, Uuid::new_v4(), "blog-api")),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");
}

// ─── Happy path ──────────────────────────────────────────────────────────

#[tokio::test]
async fn happy_path_creates_project_provisions_pod_and_links_it() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/pods"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "ok": true,
            "project": "blog-api",
            "blueprint": "software",
            "members": [
                {"id": "blog-api-lead", "role": "lead", "model": "anthropic/claude-opus-4-5"}
            ]
        })))
        .mount(&server)
        .await;

    let (app, _state) = app_with_state(orch_config()).await;
    let template_id = create_template(&app).await;
    let control_plane_id = create_control_plane(&app, &server.uri()).await;

    let res = req(
        &app,
        Method::POST,
        &format!("/api/templates/{template_id}/provision"),
        Some(provision_body(true, control_plane_id, "blog-api")),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let body = body_json(res).await;

    assert_eq!(body["provisioning"]["status"], "linked");
    assert_eq!(body["provisioning"]["remote_project"], "blog-api");
    assert_eq!(body["provisioning"]["members"][0]["role"], "lead");
    let project_id = body["project"]["id"].as_str().unwrap();

    // The project is real.
    let get_res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}"),
        None,
    )
    .await;
    assert_eq!(get_res.status(), StatusCode::OK);

    // The link is real.
    let link_res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/orch-link"),
        None,
    )
    .await;
    assert_eq!(link_res.status(), StatusCode::OK);
    let link_body = body_json(link_res).await;
    assert_eq!(link_body["linked"], true);
    assert_eq!(link_body["link"]["remote_project"], "blog-api");
}

// ─── Validation before anything is created ─────────────────────────────────

#[tokio::test]
async fn unknown_control_plane_404s_before_creating_any_project() {
    let (app, _state) = app_with_state(orch_config()).await;
    let template_id = create_template(&app).await;
    let before = count_projects(&app).await;

    let res = req(
        &app,
        Method::POST,
        &format!("/api/templates/{template_id}/provision"),
        Some(provision_body(true, Uuid::new_v4(), "blog-api")),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        count_projects(&app).await,
        before,
        "no project should be created when the control plane id is unknown"
    );
}

#[tokio::test]
async fn empty_remote_project_400s_before_creating_any_project() {
    let (app, _state) = app_with_state(orch_config()).await;
    let template_id = create_template(&app).await;
    let control_plane_id = create_control_plane(&app, "http://127.0.0.1:1").await;
    let before = count_projects(&app).await;

    let mut body = provision_body(true, control_plane_id, "");
    body["provision_pod"]["remote_project"] = json!("   ");
    let res = req(
        &app,
        Method::POST,
        &format!("/api/templates/{template_id}/provision"),
        Some(body),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(count_projects(&app).await, before);
}

// ─── Rollback: a failure strictly before POST /pods succeeds ──────────────

#[tokio::test]
async fn bad_status_map_rolls_back_without_calling_docket() {
    // Deliberately no `/pods` mock mounted — if the handler incorrectly
    // called docket before validating status_map, it would hit this
    // MockServer's default 404-for-unmatched-route response, and the
    // resulting error message would read "pod provisioning failed: ..."
    // instead of naming the bad status. Asserting the message's shape
    // below is what actually proves docket was never reached.
    let server = MockServer::start().await;

    let (app, _state) = app_with_state(orch_config()).await;
    let template_id = create_template(&app).await;
    let control_plane_id = create_control_plane(&app, &server.uri()).await;
    let before = count_projects(&app).await;

    let res = req(
        &app,
        Method::POST,
        &format!("/api/templates/{template_id}/provision"),
        Some(provision_body(false, control_plane_id, "blog-api")),
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = body_json(res).await;
    let message = body["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("Nonexistent Status"),
        "must name the bad status, got: {message}"
    );
    assert!(
        !message.contains("pod provisioning failed"),
        "must never have reached docket: {message}"
    );
    assert!(
        message.contains("rolled back"),
        "must confirm the rollback in the error message: {message}"
    );
    assert_eq!(
        count_projects(&app).await,
        before,
        "the project created for this attempt must be rolled back"
    );
}

#[tokio::test]
async fn docket_error_rolls_back_the_project() {
    let cases = [
        (
            400,
            StatusCode::BAD_REQUEST,
            json!({"ok": false, "error": "unknown blueprint 'made-up'"}),
            "unknown blueprint",
        ),
        (
            409,
            StatusCode::CONFLICT,
            json!({"ok": false, "error": "'blog-api' already exists"}),
            "already exists",
        ),
    ];
    for (docket_status, expected_status, docket_body, message_contains) in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/pods"))
            .respond_with(ResponseTemplate::new(docket_status).set_body_json(docket_body))
            .mount(&server)
            .await;

        let (app, _state) = app_with_state(orch_config()).await;
        let template_id = create_template(&app).await;
        let control_plane_id = create_control_plane(&app, &server.uri()).await;
        let before = count_projects(&app).await;

        let res = req(
            &app,
            Method::POST,
            &format!("/api/templates/{template_id}/provision"),
            Some(provision_body(true, control_plane_id, "blog-api")),
        )
        .await;
        assert_eq!(res.status(), expected_status, "docket {docket_status}");
        let body = body_json(res).await;
        let message = body["error"]["message"].as_str().unwrap();
        assert!(
            message.contains(message_contains),
            "docket {docket_status}: {message}"
        );
        assert!(
            message.contains("rolled back"),
            "docket {docket_status}: {message}"
        );
        assert_eq!(
            count_projects(&app).await,
            before,
            "docket {docket_status}: project must roll back even when docket itself created nothing"
        );
    }
}

// ─── The one step that is never rolled back ────────────────────────────────

#[tokio::test]
async fn orch_link_write_failure_after_pod_leaves_both_standing() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/pods"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "ok": true, "project": "blog-api", "blueprint": "software", "members": []
        })))
        .mount(&server)
        .await;

    let (app, state) = app_with_state(orch_config()).await;
    let template_id = create_template(&app).await;
    let control_plane_id = create_control_plane(&app, &server.uri()).await;

    // Force the one remaining write to fail deterministically, simulating
    // the rare "docket succeeded, Tack's own DB write then failed" case
    // this test is about — without needing to fault-inject application
    // code.
    sqlx::query("DROP TABLE orch_links")
        .execute(state.pool())
        .await
        .expect("drop orch_links for the test");

    let before = count_projects(&app).await;
    let res = req(
        &app,
        Method::POST,
        &format!("/api/templates/{template_id}/provision"),
        Some(provision_body(true, control_plane_id, "blog-api")),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let body = body_json(res).await;

    assert_eq!(body["provisioning"]["status"], "pod_created_link_failed");
    let warnings = body["provisioning"]["warnings"].as_array().unwrap();
    assert!(
        warnings.iter().any(|w| {
            let s = w.as_str().unwrap();
            s.contains("Settings") && s.contains("blog-api")
        }),
        "must name a concrete manual-link instruction: {warnings:?}"
    );

    // The project is real and was NOT rolled back — the pod is real too
    // and cannot be un-provisioned, so deleting the project now would only
    // make things worse (see the module doc).
    assert_eq!(
        count_projects(&app).await,
        before + 1,
        "the project must not be rolled back once the pod already exists"
    );
    let project_id = body["project"]["id"].as_str().unwrap();
    let get_res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}"),
        None,
    )
    .await;
    assert_eq!(get_res.status(), StatusCode::OK);
}
