//! Tests for `POST /api/projects/{id}/orch-dispatch` (ADR 0065) — the
//! project-level docket pipeline trigger. Distinct from the item/sprint
//! dispatch routes covered by `item.rs`: this route claims no Tack item,
//! so its tests assert directly that it never writes `orch_tasks` or
//! `execution_requests`, never that a status code alone implies it.
//!
//! Covers: `409 orchestration_disabled` with `TACK_ORCH_ENABLE` off;
//! `403` with `TACK_ORCH_DISPATCH_TOKEN` unset (the safe default) or a
//! wrong header value, regardless of the ordinary Bearer token; `404` for
//! an unknown project and for a project with no docket link; the happy
//! path (`run_id`/`remote_project` in the response, `variables` reaching
//! docket verbatim, zero `orch_tasks`/`execution_requests` rows written);
//! and that the `variables` body never reaches the logs.

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
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const DISPATCH_TOKEN_HEADER: &str = "x-tack-dispatch-token";

// ─── Harness (mirrors orchestration/dispatch/item.rs's own helpers,
// deliberately not shared — a private copy avoids coupling this file's
// tests to that file's helper signatures changing later) ──────────────────

fn orch_config() -> AppConfig {
    AppConfig {
        orch_enable: true,
        ..AppConfig::default()
    }
}

fn orch_config_with_dispatch_token(token: &str) -> AppConfig {
    AppConfig {
        orch_enable: true,
        orch_dispatch_token: Some(token.to_string()),
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

async fn body_json_val(res: axum::response::Response) -> Value {
    let bytes = to_bytes(res.into_body(), 4 * 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn req(
    app: &Router,
    method: Method,
    uri: &str,
    headers: &[(&str, &str)],
    body: Option<Value>,
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
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

async fn create_control_plane(app: &Router, base_url: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        "/api/control-planes",
        &[],
        Some(json!({"name": "docket-1", "base_url": base_url})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json_val(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn link_project(
    app: &Router,
    project_id: Uuid,
    control_plane_id: Uuid,
    remote_project: &str,
) {
    let res = req(
        app,
        Method::PUT,
        &format!("/api/projects/{project_id}/orch-link"),
        &[],
        Some(json!({
            "control_plane_id": control_plane_id,
            "remote_project": remote_project,
            "status_map": {},
        })),
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "{:?}",
        body_json_val(res).await
    );
}

async fn dispatch_pipeline(
    app: &Router,
    project_id: Uuid,
    token: Option<&str>,
    variables: Option<Value>,
) -> axum::response::Response {
    let headers: Vec<(&str, &str)> = token
        .map(|t| vec![(DISPATCH_TOKEN_HEADER, t)])
        .unwrap_or_default();
    let body = Some(json!({ "variables": variables.unwrap_or_else(|| json!({})) }));
    req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/orch-dispatch"),
        &headers,
        body,
    )
    .await
}

async fn count_orch_tasks(state: &AppState) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM orch_tasks")
        .fetch_one(state.repo.pool())
        .await
        .unwrap();
    row.0
}

async fn count_execution_requests(state: &AppState) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM execution_requests")
        .fetch_one(state.repo.pool())
        .await
        .unwrap();
    row.0
}

/// `POST /dispatch/{project}` returning docket's real "accepted" shape.
async fn mock_dispatch_allow(server: &MockServer, project: &str, run_id: &str) {
    Mock::given(method("POST"))
        .and(path(format!("/dispatch/{project}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "run": run_id, "project": project, "status": "dispatched"
        })))
        .mount(server)
        .await;
}

// ─── Off by default / actionable refusal ───────────────────────────────────

#[tokio::test]
async fn dispatch_409s_when_orch_disabled() {
    let (app, _) = common::test_app().await; // orch_enable defaults to false
    let res = dispatch_pipeline(&app, Uuid::new_v4(), Some("whatever"), None).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json_val(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");
}

// ─── Fail-closed dispatch token — the acceptance-2 / acceptance-10 case ────

#[tokio::test]
async fn dispatch_403s_when_dispatch_token_unset() {
    // orch_dispatch_token: None — the safe default this test pins.
    let (app, _) = app_with_state(orch_config()).await;
    let project_id =
        common::create_project(&app, "Pipeline Dispatch Test Project", "software").await;

    let res = dispatch_pipeline(&app, project_id, Some("anything"), None).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body = body_json_val(res).await;
    let message = body["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("TACK_ORCH_DISPATCH_TOKEN"),
        "must name the missing gate, not just refuse silently: {body}"
    );
}

#[tokio::test]
async fn dispatch_403s_when_dispatch_token_wrong() {
    let (app, _) = app_with_state(orch_config_with_dispatch_token("correct-token")).await;
    let project_id =
        common::create_project(&app, "Pipeline Dispatch Test Project", "software").await;

    let res = dispatch_pipeline(&app, project_id, Some("wrong-token"), None).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn dispatch_403s_when_dispatch_token_header_missing() {
    let (app, _) = app_with_state(orch_config_with_dispatch_token("correct-token")).await;
    let project_id =
        common::create_project(&app, "Pipeline Dispatch Test Project", "software").await;

    let res = dispatch_pipeline(&app, project_id, None, None).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

// ─── Project resolution: no second way to name a docket project ───────────

#[tokio::test]
async fn dispatch_404s_for_unknown_project() {
    let (app, _) = app_with_state(orch_config_with_dispatch_token("tok")).await;
    let res = dispatch_pipeline(&app, Uuid::new_v4(), Some("tok"), None).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn dispatch_404s_when_project_not_linked() {
    let (app, _) = app_with_state(orch_config_with_dispatch_token("tok")).await;
    let project_id =
        common::create_project(&app, "Pipeline Dispatch Test Project", "software").await;

    let res = dispatch_pipeline(&app, project_id, Some("tok"), None).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

// ─── Happy path: run started, nothing item-scoped written ─────────────────

#[tokio::test]
async fn dispatch_success_returns_run_id_and_writes_no_item_scoped_row() {
    let server = MockServer::start().await;
    mock_dispatch_allow(&server, "demo-pipeline", "run-happy-1").await;

    let (app, state) = app_with_state(orch_config_with_dispatch_token("tok")).await;
    let project_id =
        common::create_project(&app, "Pipeline Dispatch Test Project", "software").await;
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(&app, project_id, cp, "demo-pipeline").await;

    let before_tasks = count_orch_tasks(&state).await;
    let before_executions = count_execution_requests(&state).await;

    let res = dispatch_pipeline(
        &app,
        project_id,
        Some("tok"),
        Some(json!({"branch": "main"})),
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "{:?}",
        body_json_val(res).await
    );
    let v = body_json_val(res).await;
    assert_eq!(v["run_id"], "run-happy-1");
    assert_eq!(v["remote_project"], "demo-pipeline");
    // No "status"/"outcome" field promising anything about the run beyond
    // having started — see the handler's own doc comment for why.
    assert_eq!(v.get("outcome"), None);
    assert_eq!(v.get("status"), None);

    // Assert the absence directly, not just a 200 — a project-level pipeline
    // trigger must never claim a Tack item (ADR 0065 decision 5).
    assert_eq!(
        count_orch_tasks(&state).await,
        before_tasks,
        "must write no orch_tasks row"
    );
    assert_eq!(
        count_execution_requests(&state).await,
        before_executions,
        "must write no execution_requests row"
    );
}

#[tokio::test]
async fn dispatch_sends_variables_to_docket_verbatim() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/dispatch/demo-pipeline"))
        .and(body_json(json!({"branch": "main", "retries": 2})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "run": "run-vars", "project": "demo-pipeline", "status": "dispatched"
        })))
        .mount(&server)
        .await;

    let (app, _) = app_with_state(orch_config_with_dispatch_token("tok")).await;
    let project_id =
        common::create_project(&app, "Pipeline Dispatch Test Project", "software").await;
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(&app, project_id, cp, "demo-pipeline").await;

    let res = dispatch_pipeline(
        &app,
        project_id,
        Some("tok"),
        Some(json!({"branch": "main", "retries": 2})),
    )
    .await;
    // wiremock's `body_json` match already proves the pass-through; a
    // non-200 here would mean the mock never matched, i.e. the body diverged.
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "{:?}",
        body_json_val(res).await
    );
}

#[tokio::test]
async fn dispatch_omitted_variables_default_to_empty_object() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/dispatch/demo-pipeline"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true, "run": "run-empty", "project": "demo-pipeline", "status": "dispatched"
        })))
        .mount(&server)
        .await;

    let (app, _) = app_with_state(orch_config_with_dispatch_token("tok")).await;
    let project_id =
        common::create_project(&app, "Pipeline Dispatch Test Project", "software").await;
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(&app, project_id, cp, "demo-pipeline").await;

    // A bare `{}` request body — no `variables` field at all.
    let res = req(
        &app,
        Method::POST,
        &format!("/api/projects/{project_id}/orch-dispatch"),
        &[(DISPATCH_TOKEN_HEADER, "tok")],
        Some(json!({})),
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "{:?}",
        body_json_val(res).await
    );
}

// ─── Redaction: `variables` never reaches the logs ─────────────────────────
//
// Self-contained log-capture rig — see
// `runner_protocol/log_capture.rs`'s doc comment for why a *global*
// `tracing` default subscriber, gated by a thread-local flag, is what
// actually closes the cross-test interest-cache race under `cargo test`;
// this binary's tests are all plain `#[tokio::test]` (current-thread), the
// same precondition that makes the pattern safe there. Not shared with that
// module — it lives in a different compiled test binary.
mod log_capture {
    use std::cell::RefCell;
    use std::sync::{Arc, Mutex, Once};

    thread_local! {
        static LOG_CAPTURE: RefCell<Option<Arc<Mutex<Vec<u8>>>>> = const { RefCell::new(None) };
    }

    static GLOBAL_LOG_CAPTURE_INIT: Once = Once::new();

    fn ensure_global_log_capture_installed() {
        GLOBAL_LOG_CAPTURE_INIT.call_once(|| {
            let subscriber = tracing_subscriber::fmt()
                .with_writer(GlobalLogWriter)
                .with_max_level(tracing::Level::DEBUG)
                .finish();
            tracing::subscriber::set_global_default(subscriber).expect(
                "GLOBAL_LOG_CAPTURE_INIT guards the only global tracing subscriber \
                 this binary ever installs",
            );
        });
    }

    struct GlobalLogWriter;

    impl std::io::Write for GlobalLogWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            LOG_CAPTURE.with(|cell| {
                if let Some(buffer) = cell.borrow().as_ref() {
                    buffer.lock().unwrap().extend_from_slice(buf);
                }
            });
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for GlobalLogWriter {
        type Writer = GlobalLogWriter;
        fn make_writer(&'a self) -> Self::Writer {
            GlobalLogWriter
        }
    }

    pub(crate) struct CaptureGuard;

    impl CaptureGuard {
        pub(crate) fn start() -> (Self, Arc<Mutex<Vec<u8>>>) {
            ensure_global_log_capture_installed();
            let buffer = Arc::new(Mutex::new(Vec::new()));
            LOG_CAPTURE.with(|cell| *cell.borrow_mut() = Some(buffer.clone()));
            (Self, buffer)
        }
    }

    impl Drop for CaptureGuard {
        fn drop(&mut self) {
            LOG_CAPTURE.with(|cell| *cell.borrow_mut() = None);
        }
    }
}

const SECRET_VARIABLE_MARKER: &str = "SECRET_DISPATCH_VARIABLE_MARKER_7c1e9";

#[tokio::test]
async fn dispatch_variables_never_reach_the_logs() {
    let server = MockServer::start().await;
    mock_dispatch_allow(&server, "demo-pipeline", "run-redaction").await;

    let (app, _) = app_with_state(orch_config_with_dispatch_token("tok")).await;
    let project_id =
        common::create_project(&app, "Pipeline Dispatch Test Project", "software").await;
    let cp = create_control_plane(&app, &server.uri()).await;
    link_project(&app, project_id, cp, "demo-pipeline").await;

    let (guard, captured) = log_capture::CaptureGuard::start();

    let res = dispatch_pipeline(
        &app,
        project_id,
        Some("tok"),
        Some(json!({"secret_field": SECRET_VARIABLE_MARKER})),
    )
    .await;
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "{:?}",
        body_json_val(res).await
    );

    drop(guard);
    let text = String::from_utf8_lossy(&captured.lock().unwrap()).into_owned();

    assert!(
        !text.is_empty(),
        "capture rig must have observed real log output"
    );
    assert!(
        !text.contains(SECRET_VARIABLE_MARKER),
        "the variables body leaked into logs:\n{text}"
    );
    // Non-vacuous: the project id (an id, not a secret) is expected to
    // appear somewhere in `#[instrument]`'s own span fields, confirming the
    // capture rig is observing genuine production log lines, not an empty
    // or unreached subscriber.
    assert!(
        text.contains(&project_id.to_string()),
        "capture rig did not observe the real handler's own instrumentation:\n{text}"
    );
}
