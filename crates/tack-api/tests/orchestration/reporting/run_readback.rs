//! Tests for `GET /api/orch-runs/{run_id}` — a pipeline run's mirrored
//! state, read back by its own id rather than through a Tack item.
//!
//! Covers: `409 orchestration_disabled` with `TACK_ORCH_ENABLE` off; a run
//! id with no `orch_runs` row reports `mirrored: false` with every other
//! field `null` — `200`, never `404`, since Tack cannot tell an un-polled
//! run apart from an unknown one; a mirrored row (with and without an
//! attributed item) round-trips its fields, including a `null` `item_id`
//! for the CLI-pipeline-dispatch case ADR 0065 decision 5 describes; and
//! the route reuses `Repository::get_orch_run` rather than adding a second
//! `orch_runs` query — proved by reverting the handler to always answer
//! `mirrored: false` and watching the round-trip test go red.

use crate::common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use chrono::Utc;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::router::{AppState, build_router};
use tack_db::repo::orch::NewOrchRun;
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Harness (mirrors reporting/agent_activity.rs's own helpers, kept as a
// private copy so this file's tests don't couple to that file's helper
// signatures changing later) ────────────────────────────────────────────────

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

async fn get_run(app: &Router, run_id: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/orch-runs/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

// ─── Off by default ─────────────────────────────────────────────────────────

#[tokio::test]
async fn run_readback_409s_when_orch_disabled() {
    let (app, _) = common::test_app().await; // orch_enable defaults to false
    let res = get_run(&app, "run-whatever").await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");
}

// ─── Unmirrored run: a legitimate 200, never a 404 ─────────────────────────

#[tokio::test]
async fn run_readback_reports_unmirrored_run_as_200_not_404() {
    let (app, _) = app_with_state(orch_config()).await;

    let res = get_run(&app, "run-never-polled").await;
    assert_eq!(
        res.status(),
        StatusCode::OK,
        "an un-polled run id is a legitimate answer, not a 404"
    );
    let v = body_json(res).await;
    assert_eq!(v["run_id"], "run-never-polled");
    assert_eq!(v["mirrored"], false);
    // Unmeasured is nullable — never a fabricated state, never 0/success
    // standing in for "not yet known".
    for field in [
        "item_id",
        "remote_project",
        "source",
        "state",
        "started_at",
        "ended_at",
        "error",
        "mirrored_at",
    ] {
        assert!(
            v[field].is_null(),
            "field {field} must be null for an unmirrored run, got {:?}",
            v[field]
        );
    }
}

// ─── Mirrored run: round-trips what the reconciler last wrote ─────────────

#[tokio::test]
async fn run_readback_round_trips_a_mirrored_run_with_no_item() {
    let (app, state) = app_with_state(orch_config()).await;
    let plane = state
        .repo
        .create_control_plane(tack_db::repo::orch::CreateControlPlane {
            name: "docket-1".to_string(),
            kind: None,
            base_url: "http://docket.local".to_string(),
            token: None,
        })
        .await
        .expect("create plane");

    state
        .repo
        .upsert_orch_runs(
            plane.id,
            &[NewOrchRun {
                run_id: "run-cli-1".to_string(),
                // The CLI-dispatched-pipeline case (ADR 0065 decision 5):
                // no item is ever supplied.
                item_id: None,
                remote_project: "demo-pipeline".to_string(),
                source: "cli".to_string(),
                state: "running".to_string(),
                started_at: Some(Utc::now()),
                ended_at: None,
                error: None,
            }],
        )
        .await
        .expect("seed run");

    let res = get_run(&app, "run-cli-1").await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["run_id"], "run-cli-1");
    assert_eq!(v["mirrored"], true);
    assert!(
        v["item_id"].is_null(),
        "a CLI-dispatched pipeline run claims no item"
    );
    assert_eq!(v["remote_project"], "demo-pipeline");
    assert_eq!(v["source"], "cli");
    assert_eq!(v["state"], "running");
    assert!(v["started_at"].is_string());
    assert!(v["ended_at"].is_null());
    assert!(v["error"].is_null());
    assert!(v["mirrored_at"].is_string());
}

#[tokio::test]
async fn run_readback_carries_an_attributed_item_id_when_one_exists() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Run Readback Test Project", "software").await;
    let item_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{project_id}/items"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({"title": "Attributed item"})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(item_res.status(), StatusCode::OK);
    let item_id = Uuid::parse_str(body_json(item_res).await["id"].as_str().unwrap()).unwrap();

    let plane = state
        .repo
        .create_control_plane(tack_db::repo::orch::CreateControlPlane {
            name: "docket-1".to_string(),
            kind: None,
            base_url: "http://docket.local".to_string(),
            token: None,
        })
        .await
        .expect("create plane");

    state
        .repo
        .upsert_orch_runs(
            plane.id,
            &[NewOrchRun {
                run_id: "run-attributed-1".to_string(),
                item_id: Some(item_id),
                remote_project: "demo-pipeline".to_string(),
                source: "webhook".to_string(),
                state: "succeeded".to_string(),
                started_at: Some(Utc::now()),
                ended_at: Some(Utc::now()),
                error: None,
            }],
        )
        .await
        .expect("seed run");

    let res = get_run(&app, "run-attributed-1").await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["item_id"], item_id.to_string());
    assert_eq!(v["state"], "succeeded");
}

#[tokio::test]
async fn run_readback_reports_a_failed_run_state_without_calling_it_a_permission_verdict() {
    let (app, state) = app_with_state(orch_config()).await;
    let plane = state
        .repo
        .create_control_plane(tack_db::repo::orch::CreateControlPlane {
            name: "docket-1".to_string(),
            kind: None,
            base_url: "http://docket.local".to_string(),
            token: None,
        })
        .await
        .expect("create plane");

    state
        .repo
        .upsert_orch_runs(
            plane.id,
            &[NewOrchRun {
                run_id: "run-blocked-1".to_string(),
                item_id: None,
                remote_project: "demo-pipeline".to_string(),
                source: "cli".to_string(),
                state: "failed".to_string(),
                started_at: Some(Utc::now()),
                ended_at: Some(Utc::now()),
                error: Some("guardrail policy denied the request".to_string()),
            }],
        )
        .await
        .expect("seed run");

    let res = get_run(&app, "run-blocked-1").await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["state"], "failed");
    assert_eq!(v["error"], "guardrail policy denied the request");

    // No field name anywhere in the response claims permission — the wire
    // shape is exactly OrchRunReadbackResponse's declared fields.
    let keys: std::collections::BTreeSet<&str> =
        v.as_object().unwrap().keys().map(String::as_str).collect();
    let allowed: std::collections::BTreeSet<&str> = [
        "run_id",
        "mirrored",
        "item_id",
        "remote_project",
        "source",
        "state",
        "started_at",
        "ended_at",
        "error",
        "mirrored_at",
    ]
    .into_iter()
    .collect();
    assert_eq!(keys, allowed, "response carries an undocumented field");
    for forbidden in ["permitted", "allowed", "approved"] {
        assert!(
            !keys.contains(forbidden),
            "response must never claim permission via field {forbidden}"
        );
    }
}
