//! Tests for `GET /api/projects/{id}/orch-budget` and
//! `GET /api/projects/{id}/orch-policy`.
//!
//! Covers: 404 with `TACK_ORCH_ENABLE` unset (both routes); an unlinked
//! project reports `linked: false` while still surfacing any real historical
//! token/cost totals (never inventing a link); the unreachable-vs-zero
//! staleness distinction for `cost_usd_estimated` (same rule `GET /api/fleet`
//! established); real token/cost sums from `orch_tasks`; the policy
//! endpoint's scoping to exactly the linked control plane's own
//! `orch_metrics` samples (never leaking a second plane's numbers); denial
//! rate computed from `docket_tool_calls_total`, `None` (not `0.0`) when no
//! tool-call data exists at all; and policy-hit / approval-channel grouping.
//!
//! Deliberately does **not** test for any "paused" field on either response —
//! see `handlers/orch.rs`'s module doc above `OrchBudgetResponse` for why that
//! isn't reachable and isn't built.

use crate::common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use chrono::Utc;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::router::{AppState, build_router};
use tack_db::repo::orch::NewOrchMetric;
use tack_db::repo::orch::NewOrchTask;
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Helpers (mirrors orchestration/control_plane/resource.rs /
// orchestration/reporting/agent_activity.rs) ────────────────────────────────

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

async fn create_item(app: &Router, project_id: Uuid, title: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/items"),
        Some(json!({"title": title})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn create_control_plane(app: &Router, name: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        "/api/control-planes",
        Some(json!({"name": name, "base_url": "http://docket.local:9999"})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn link_project(app: &Router, project_id: Uuid, plane_id: Uuid, budget_usd: Option<f64>) {
    let res = req(
        app,
        Method::PUT,
        &format!("/api/projects/{project_id}/orch-link"),
        Some(json!({
            "control_plane_id": plane_id,
            "remote_project": "my-remote-project",
            "budget_usd": budget_usd,
            "status_map": {}
        })),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
}

fn new_task(
    item_id: Uuid,
    remote_task_id: &str,
    tokens_in: i64,
    tokens_out: i64,
    cost: f64,
) -> NewOrchTask {
    NewOrchTask {
        item_id,
        remote_task_id: remote_task_id.to_string(),
        remote_run_id: None,
        remote_status: "done".to_string(),
        attempt: 1,
        tokens_in,
        tokens_out,
        cost_usd_estimated: Some(cost),
        dispatched_at: Utc::now(),
        trusted: true,
    }
}

fn metric(name: &str, labels: &[(&str, &str)], value: f64) -> NewOrchMetric {
    NewOrchMetric {
        name: name.to_string(),
        labels: labels
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        value,
    }
}

// ─── Off by default ────────────────────────────────────

#[tokio::test]
async fn both_new_routes_409_when_orch_disabled() {
    let (app, _) = common::test_app().await; // orch_enable defaults to false
    let fake = Uuid::new_v4();

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{fake}/orch-budget"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{fake}/orch-policy"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");
}

// ─── Budget ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn orch_budget_unlinked_project_reports_linked_false_and_null_cost() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id =
        common::create_project(&app, "Orch Budget/Policy Test Project", "software").await;

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/orch-budget"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["linked"], false);
    assert!(v["control_plane_id"].is_null());
    assert!(v["budget_usd"].is_null());
    assert_eq!(v["tokens_in"], json!(0));
    assert_eq!(v["tokens_out"], json!(0));
    assert!(
        v["cost_usd_estimated"].is_null(),
        "an unlinked project must never report a confident cost figure: {v:?}"
    );
}

#[tokio::test]
async fn orch_budget_reports_zero_cost_distinctly_from_unreachable() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id =
        common::create_project(&app, "Orch Budget/Policy Test Project", "software").await;
    let plane_id = create_control_plane(&app, "docket-1").await;
    link_project(&app, project_id, plane_id, Some(50.0)).await;

    // Freshly registered plane: health defaults to "unknown" (not yet
    // polled), which is reachable-enough to report a real, current zero.
    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/orch-budget"),
        None,
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(v["linked"], true);
    assert_eq!(v["health"], "unknown");
    assert_eq!(v["budget_usd"], json!(50.0));
    assert_eq!(
        v["cost_usd_estimated"],
        json!(0.0),
        "a reachable plane with nothing dispatched yet must report a real Some(0.0), not null"
    );
    assert_eq!(v["tokens_in"], json!(0));
    assert_eq!(v["tokens_out"], json!(0));
    assert!(v["pricing_snapshot_at"].is_null());

    state
        .repo
        .update_control_plane_health(plane_id, "unreachable", None, 10, None)
        .await
        .expect("record health");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/orch-budget"),
        None,
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(v["health"], "unreachable");
    assert!(
        v["cost_usd_estimated"].is_null(),
        "an unreachable plane's cost must be None/null (stale), never a confident zero: {v:?}"
    );
    // Budget cap and token totals stay honest/real even while cost is stale.
    assert_eq!(v["budget_usd"], json!(50.0));
    assert_eq!(v["tokens_in"], json!(0));
}

#[tokio::test]
async fn orch_budget_reflects_real_token_and_cost_sums_from_orch_tasks() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id =
        common::create_project(&app, "Orch Budget/Policy Test Project", "software").await;
    let item_a = create_item(&app, project_id, "Item A").await;
    let item_b = create_item(&app, project_id, "Item B").await;
    let plane_id = create_control_plane(&app, "docket-1").await;
    link_project(&app, project_id, plane_id, Some(10.0)).await;

    state
        .repo
        .upsert_orch_tasks(&[
            new_task(item_a, "task-a1", 1_000, 500, 0.05),
            new_task(item_b, "task-b1", 2_000, 1_000, 0.10),
        ])
        .await
        .expect("seed tasks");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/orch-budget"),
        None,
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(v["tokens_in"], json!(3_000));
    assert_eq!(v["tokens_out"], json!(1_500));
    // Floating point sum of 0.05 + 0.10 — allow for representation noise.
    let cost = v["cost_usd_estimated"].as_f64().unwrap();
    assert!((cost - 0.15).abs() < 1e-9, "expected ~0.15, got {cost}");
}

// ─── Policy ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn orch_policy_unlinked_project_returns_empty_with_linked_false() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id =
        common::create_project(&app, "Orch Budget/Policy Test Project", "software").await;

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/orch-policy"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["linked"], false);
    assert_eq!(v["scoped_to_control_plane_only"], true);
    assert_eq!(v["tool_calls"], json!([]));
    assert!(v["denial_rate"].is_null());
    assert_eq!(v["policy_hits"], json!([]));
    assert_eq!(v["approvals_by_channel"], json!([]));
}

#[tokio::test]
async fn orch_policy_scopes_metrics_to_the_linked_control_plane_only() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id =
        common::create_project(&app, "Orch Budget/Policy Test Project", "software").await;
    let my_plane = create_control_plane(&app, "my-plane").await;
    let other_plane = create_control_plane(&app, "other-plane").await;
    link_project(&app, project_id, my_plane, None).await;

    state
        .repo
        .upsert_orch_metrics(
            my_plane,
            &[metric(
                "docket_tool_calls_total",
                &[("decision", "allow")],
                7.0,
            )],
        )
        .await
        .expect("seed my plane's metrics");
    state
        .repo
        .upsert_orch_metrics(
            other_plane,
            &[metric(
                "docket_tool_calls_total",
                &[("decision", "deny")],
                99.0,
            )],
        )
        .await
        .expect("seed other plane's metrics");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/orch-policy"),
        None,
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(v["linked"], true);
    assert_eq!(v["control_plane_id"], my_plane.to_string());
    let tool_calls = v["tool_calls"].as_array().unwrap();
    assert_eq!(
        tool_calls.len(),
        1,
        "must include only the linked plane's own samples, never another plane's: {tool_calls:?}"
    );
    assert_eq!(tool_calls[0]["decision"], "allow");
    assert_eq!(tool_calls[0]["count"], json!(7.0));
    // The other plane's deny=99 must not leak into this project's denial rate.
    assert_eq!(v["denial_rate"], json!(0.0));
}

#[tokio::test]
async fn orch_policy_computes_denial_rate_from_tool_calls() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id =
        common::create_project(&app, "Orch Budget/Policy Test Project", "software").await;
    let plane_id = create_control_plane(&app, "docket-1").await;
    link_project(&app, project_id, plane_id, None).await;

    state
        .repo
        .upsert_orch_metrics(
            plane_id,
            &[
                metric("docket_tool_calls_total", &[("decision", "allow")], 6.0),
                metric("docket_tool_calls_total", &[("decision", "ask")], 2.0),
                metric("docket_tool_calls_total", &[("decision", "deny")], 2.0),
            ],
        )
        .await
        .expect("seed metrics");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/orch-policy"),
        None,
    )
    .await;
    let v = body_json(res).await;
    let rate = v["denial_rate"].as_f64().unwrap();
    assert!((rate - 0.2).abs() < 1e-9, "expected 2/10 = 0.2, got {rate}");
    assert!(v["scraped_at"].is_string());
}

#[tokio::test]
async fn orch_policy_denial_rate_is_none_with_no_tool_call_data() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id =
        common::create_project(&app, "Orch Budget/Policy Test Project", "software").await;
    let plane_id = create_control_plane(&app, "docket-1").await;
    link_project(&app, project_id, plane_id, None).await;

    // Only policy-hit data, no tool-call samples at all.
    state
        .repo
        .upsert_orch_metrics(
            plane_id,
            &[metric(
                "docket_policy_hits_total",
                &[
                    ("policy_id", "no-prod-secrets"),
                    ("hook", "pre_tool_call"),
                    ("action", "deny"),
                ],
                3.0,
            )],
        )
        .await
        .expect("seed metrics");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/orch-policy"),
        None,
    )
    .await;
    let v = body_json(res).await;
    assert!(
        v["denial_rate"].is_null(),
        "no tool-call data observed at all must be None, never a fabricated 0.0: {v:?}"
    );
    let hits = v["policy_hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["policy_id"], "no-prod-secrets");
    assert_eq!(hits[0]["hook"], "pre_tool_call");
    assert_eq!(hits[0]["action"], "deny");
    assert_eq!(hits[0]["count"], json!(3.0));
}

#[tokio::test]
async fn orch_policy_groups_approvals_by_channel_and_outcome() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id =
        common::create_project(&app, "Orch Budget/Policy Test Project", "software").await;
    let plane_id = create_control_plane(&app, "docket-1").await;
    link_project(&app, project_id, plane_id, None).await;

    state
        .repo
        .upsert_orch_metrics(
            plane_id,
            &[
                metric(
                    "docket_approvals_total",
                    &[("channel", "tack"), ("outcome", "granted")],
                    4.0,
                ),
                metric(
                    "docket_approvals_total",
                    &[("channel", "timeout"), ("outcome", "denied")],
                    1.0,
                ),
            ],
        )
        .await
        .expect("seed metrics");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/orch-policy"),
        None,
    )
    .await;
    let v = body_json(res).await;
    let approvals = v["approvals_by_channel"].as_array().unwrap();
    assert_eq!(approvals.len(), 2);
    assert!(
        approvals
            .iter()
            .any(|a| a["channel"] == "tack" && a["outcome"] == "granted" && a["count"] == 4.0)
    );
    assert!(
        approvals
            .iter()
            .any(|a| a["channel"] == "timeout" && a["outcome"] == "denied" && a["count"] == 1.0)
    );
}
