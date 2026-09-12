//! Tests for `GET /api/economics/summary` and `GET /api/economics/items`.
//!
//! The min-sample/staleness/negative-duration branches of the aggregation math
//! itself are exhaustively unit-tested in `crates/tack-api/src/handlers/
//! economics.rs`'s own `#[cfg(test)]` module (pure functions, no DB needed). This
//! file instead proves the HTTP plumbing: the off-by-default gate, that a real
//! `orch_tasks` row makes an item "agent" population and a real `items.status
//! /Done` transition with no dispatch makes it "human", that tokens/cost sums come
//! from real seeded data, project_type/item_type slicing, the rework-signal item-
//! level correlation via `orch_events`, and the CSV/JSON export shapes.

use crate::common;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::router::{AppState, build_router};
use tack_db::repo::orch::{NewOrchEvent, NewOrchTask};
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;

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

async fn body_text(res: axum::response::Response) -> (StatusCode, axum::http::HeaderMap, String) {
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = to_bytes(res.into_body(), 4 * 1024 * 1024).await.unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
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

async fn create_item(app: &Router, project_id: Uuid, title: &str, item_type: &str) -> Uuid {
    let res = req(
        app,
        Method::POST,
        &format!("/api/projects/{project_id}/items"),
        Some(json!({"title": title, "item_type": item_type})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let v = body_json(res).await;
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

async fn patch_status(app: &Router, item_id: Uuid, status: &str) {
    let res = req(
        app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"status": status})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
}

/// Completes an item with no dispatch history — the "human" population. Goes
/// straight to "Done" (every default workflow preset names its terminal status
/// "Done") rather than via an in-progress step first, since the in-progress status
/// name differs per preset ("In Progress" for scrum/kanban, "Doing" for simple,
/// etc.) and this helper is used across multiple project types.
async fn complete_as_human(app: &Router, item_id: Uuid) {
    patch_status(app, item_id, "Done").await;
}

/// Completes an item after seeding an `orch_tasks` row for it directly (bypassing
/// a real docket call, same technique `orchestration/reporting/budget_policy.rs`
/// uses) — the "agent" population. `dispatched_at` is caller-controlled so
/// lead-time/staleness scenarios can be constructed precisely.
#[allow(clippy::too_many_arguments)]
async fn complete_as_agent(
    state: &AppState,
    app: &Router,
    item_id: Uuid,
    remote_task_id: &str,
    tokens_in: i64,
    tokens_out: i64,
    cost: f64,
    dispatched_at: chrono::DateTime<Utc>,
) {
    state
        .repo
        .upsert_orch_tasks(&[NewOrchTask {
            item_id,
            remote_task_id: remote_task_id.to_string(),
            remote_run_id: None,
            remote_status: "done".to_string(),
            attempt: 1,
            tokens_in,
            tokens_out,
            cost_usd_estimated: Some(cost),
            dispatched_at,
            trusted: true,
        }])
        .await
        .expect("seed orch_tasks");
    patch_status(app, item_id, "Done").await;
}

async fn seed_rework_event(
    state: &AppState,
    control_plane_id: Uuid,
    item_id: Uuid,
    event_type: &str,
) {
    state
        .repo
        .upsert_orch_events(
            control_plane_id,
            &[NewOrchEvent {
                id: Uuid::new_v4(),
                item_id: Some(item_id),
                run_id: None,
                event_type: event_type.to_string(),
                payload: json!({}),
                occurred_at: Utc::now(),
            }],
        )
        .await
        .expect("seed orch_events");
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

// ─── Off by default ────────────────────────────────────

#[tokio::test]
async fn both_routes_409_when_orch_disabled() {
    let (app, _) = common::test_app().await; // orch_enable defaults to false

    let res = req(&app, Method::GET, "/api/economics/summary", None).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");

    let res = req(&app, Method::GET, "/api/economics/items", None).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");
}

// ─── Summary ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn summary_is_empty_and_well_formed_with_no_completed_items() {
    let (app, _) = app_with_state(orch_config()).await;

    let res = req(&app, Method::GET, "/api/economics/summary", None).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["overall"]["completed_item_count"], json!(0));
    assert_eq!(v["overall"]["agent_completed_count"], json!(0));
    assert_eq!(v["overall"]["cost_usd_estimated"], Value::Null);
    assert_eq!(v["min_sample_size"], json!(5));
    assert!(v["by_project_type"].as_array().unwrap().is_empty());
    assert!(v["by_item_type"].as_array().unwrap().is_empty());
    // Rule 6: pricing_snapshot_at is always null — no pricing mechanism exists.
    assert_eq!(v["overall"]["pricing_snapshot_at"], Value::Null);
}

#[tokio::test]
async fn summary_splits_agent_and_human_populations_with_real_token_and_cost_sums() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Econ Project", "software").await;

    let agent_item = create_item(&app, project_id, "Agent-done item", "task").await;
    complete_as_agent(
        &state,
        &app,
        agent_item,
        "task-1",
        1_000,
        500,
        0.25,
        Utc::now() - Duration::hours(3),
    )
    .await;

    let human_item = create_item(&app, project_id, "Human-done item", "task").await;
    complete_as_human(&app, human_item).await;

    let res = req(&app, Method::GET, "/api/economics/summary", None).await;
    let v = body_json(res).await;

    assert_eq!(v["overall"]["completed_item_count"], json!(2));
    assert_eq!(v["overall"]["agent_completed_count"], json!(1));
    assert_eq!(v["overall"]["human_completed_count"], json!(1));
    assert_eq!(v["overall"]["tokens_in"], json!(1_000));
    assert_eq!(v["overall"]["tokens_out"], json!(500));
    let cost = v["overall"]["cost_usd_estimated"].as_f64().unwrap();
    assert!((cost - 0.25).abs() < 1e-9);
    // 1 agent sample, 1 human sample — both below MIN_SAMPLE_SIZE (5): raw, not avg.
    assert_eq!(
        v["overall"]["agent_lead_time"]["below_min_sample"],
        json!(true)
    );
    assert_eq!(
        v["overall"]["human_lead_time"]["below_min_sample"],
        json!(true)
    );
    // Selection-bias caveat travels with the comparison, not just in a doc.
    let note = v["overall"]["lead_time_selection_bias_note"]
        .as_str()
        .unwrap();
    assert!(note.contains("not a random sample"));
}

#[tokio::test]
async fn summary_slices_by_project_type_and_item_type() {
    let (app, state) = app_with_state(orch_config()).await;

    let sw_project = common::create_project(&app, "Software Line", "software").await;
    let sw_item = create_item(&app, sw_project, "Bug fix", "bug").await;
    complete_as_agent(
        &state,
        &app,
        sw_item,
        "task-sw",
        100,
        50,
        0.01,
        Utc::now() - Duration::hours(1),
    )
    .await;

    let construction_project =
        common::create_project(&app, "Construction Line", "construction").await;
    let construction_item =
        create_item(&app, construction_project, "Pour foundation", "task").await;
    // Construction's workflow is linear/strict — walk it forward one step at a
    // time (Permit → Procurement → Build → Inspect → Handover) rather than
    // asserting a fixed name shortcut.
    for status in ["Procurement", "Build", "Inspect", "Handover"] {
        patch_status(&app, construction_item, status).await;
    }

    let res = req(&app, Method::GET, "/api/economics/summary", None).await;
    let v = body_json(res).await;

    let by_project_type = v["by_project_type"].as_array().unwrap();
    let keys: Vec<&str> = by_project_type
        .iter()
        .map(|s| s["key"].as_str().unwrap())
        .collect();
    assert!(keys.contains(&"software"));
    assert!(keys.contains(&"construction"));

    let software_slice = by_project_type
        .iter()
        .find(|s| s["key"] == "software")
        .unwrap();
    assert_eq!(software_slice["agent_completed_count"], json!(1));

    let construction_slice = by_project_type
        .iter()
        .find(|s| s["key"] == "construction")
        .unwrap();
    assert_eq!(construction_slice["human_completed_count"], json!(1));

    let by_item_type = v["by_item_type"].as_array().unwrap();
    let bug_slice = by_item_type.iter().find(|s| s["key"] == "bug").unwrap();
    assert_eq!(bug_slice["completed_item_count"], json!(1));
}

#[tokio::test]
async fn summary_rework_rate_correlates_via_item_id_and_names_its_definition() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Rework Project", "software").await;
    let plane_id = create_control_plane(&app, "docket-rework").await;

    let reworked_item = create_item(&app, project_id, "Needed rework", "task").await;
    complete_as_agent(
        &state,
        &app,
        reworked_item,
        "task-r1",
        10,
        10,
        0.01,
        Utc::now() - Duration::hours(1),
    )
    .await;
    seed_rework_event(&state, plane_id, reworked_item, "verification_failed").await;

    let clean_item = create_item(&app, project_id, "Clean run", "task").await;
    complete_as_agent(
        &state,
        &app,
        clean_item,
        "task-r2",
        10,
        10,
        0.01,
        Utc::now() - Duration::hours(1),
    )
    .await;

    let res = req(&app, Method::GET, "/api/economics/summary", None).await;
    let v = body_json(res).await;

    let rework = &v["overall"]["rework"];
    assert_eq!(rework["attempts_total"], json!(2));
    assert_eq!(rework["attempts_excluded_stale"], json!(0));
    assert_eq!(rework["attempts_with_rework_signal"], json!(1));
    // 2 eligible attempts — below MIN_SAMPLE_SIZE (5), so no rate is asserted, but
    // the definition/truncation copy must still be present and correct.
    assert_eq!(rework["below_min_sample"], json!(true));
    assert_eq!(rework["rate"], Value::Null);
    let definition = rework["definition"].as_str().unwrap();
    assert!(definition.contains("rework_started"));
    assert!(definition.contains("verification_failed"));
    assert!(definition.contains("tester_verdict_failed"));
}

#[tokio::test]
async fn summary_excludes_stale_attempts_from_the_rework_denominator() {
    // Retention window set short so "stale" is easy to construct.
    let config = AppConfig {
        orch_event_retention_days: 7,
        ..orch_config()
    };
    let (app, state) = app_with_state(config).await;

    let project_id = common::create_project(&app, "Stale Rework Project", "software").await;

    let fresh_item = create_item(&app, project_id, "Fresh dispatch", "task").await;
    complete_as_agent(
        &state,
        &app,
        fresh_item,
        "task-fresh",
        10,
        10,
        0.01,
        Utc::now() - Duration::hours(1),
    )
    .await;

    let stale_item = create_item(&app, project_id, "Stale dispatch", "task").await;
    complete_as_agent(
        &state,
        &app,
        stale_item,
        "task-stale",
        10,
        10,
        0.01,
        Utc::now() - Duration::days(30),
    )
    .await;

    let res = req(&app, Method::GET, "/api/economics/summary", None).await;
    let v = body_json(res).await;
    let rework = &v["overall"]["rework"];
    assert_eq!(rework["attempts_total"], json!(2));
    assert_eq!(rework["attempts_excluded_stale"], json!(1));
    assert_eq!(v["events_retention_days"], json!(7));
    let note = rework["truncation_note"].as_str().unwrap();
    assert!(note.contains("retention window"));
}

// ─── Items (per-item list + CSV/JSON export) ───────────────────────────────

#[tokio::test]
async fn items_endpoint_lists_completed_items_with_population_and_filters_by_project_type() {
    let (app, state) = app_with_state(orch_config()).await;
    let sw_project = common::create_project(&app, "SW", "software").await;
    let sw_item = create_item(&app, sw_project, "SW item", "task").await;
    complete_as_agent(
        &state,
        &app,
        sw_item,
        "task-x",
        42,
        24,
        0.02,
        Utc::now() - Duration::hours(1),
    )
    .await;

    let personal_project = common::create_project(&app, "Personal", "personal").await;
    let personal_item = create_item(&app, personal_project, "Personal item", "task").await;
    complete_as_human(&app, personal_item).await;

    let res = req(&app, Method::GET, "/api/economics/items", None).await;
    let v = body_json(res).await;
    assert_eq!(v["total"], json!(2));
    let rows = v["rows"].as_array().unwrap();
    let sw_row = rows
        .iter()
        .find(|r| r["item_id"] == json!(sw_item))
        .unwrap();
    assert_eq!(sw_row["population"], json!("agent"));
    assert_eq!(sw_row["tokens_in"], json!(42));
    let personal_row = rows
        .iter()
        .find(|r| r["item_id"] == json!(personal_item))
        .unwrap();
    assert_eq!(personal_row["population"], json!("human"));

    let res = req(
        &app,
        Method::GET,
        "/api/economics/items?project_type=software",
        None,
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(v["total"], json!(1));
    assert_eq!(v["rows"][0]["item_id"], json!(sw_item));
}

#[tokio::test]
async fn items_endpoint_paginates_without_silently_truncating_the_total() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Pagination Project", "software").await;
    for i in 0..7 {
        let item_id = create_item(&app, project_id, &format!("Item {i}"), "task").await;
        complete_as_agent(
            &state,
            &app,
            item_id,
            &format!("task-{i}"),
            1,
            1,
            0.001,
            Utc::now() - Duration::hours(1),
        )
        .await;
    }

    let res = req(
        &app,
        Method::GET,
        "/api/economics/items?limit=3&offset=0",
        None,
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(
        v["total"],
        json!(7),
        "total must reflect the full match count, not the page size"
    );
    assert_eq!(v["rows"].as_array().unwrap().len(), 3);

    let res = req(
        &app,
        Method::GET,
        "/api/economics/items?limit=3&offset=6",
        None,
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(v["rows"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn items_endpoint_csv_export_is_an_attachment_with_a_header_row() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "CSV Project", "software").await;
    let item_id = create_item(&app, project_id, "CSV item", "task").await;
    complete_as_agent(
        &state,
        &app,
        item_id,
        "task-csv",
        5,
        5,
        0.001,
        Utc::now() - Duration::hours(1),
    )
    .await;

    let res = req(&app, Method::GET, "/api/economics/items?format=csv", None).await;
    let (status, headers, text) = body_text(res).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.get("content-type").unwrap(), "text/csv");
    assert!(
        headers
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("attachment")
    );
    let mut lines = text.lines();
    let header_row = lines.next().unwrap();
    assert!(header_row.starts_with("item_id,project_id,project_type"));
    assert_eq!(lines.count(), 1, "one data row for the one completed item");
}

#[tokio::test]
async fn items_endpoint_reports_rework_not_reliable_for_a_stale_only_dispatch() {
    let (app, state) = app_with_state(AppConfig {
        orch_event_retention_days: 7,
        ..orch_config()
    })
    .await;
    let project_id = common::create_project(&app, "Stale Item Project", "software").await;
    let item_id = create_item(&app, project_id, "Stale item", "task").await;
    complete_as_agent(
        &state,
        &app,
        item_id,
        "task-stale-1",
        1,
        1,
        0.001,
        Utc::now() - Duration::days(30),
    )
    .await;

    let res = req(&app, Method::GET, "/api/economics/items", None).await;
    let v = body_json(res).await;
    let row = v["rows"].as_array().unwrap().first().unwrap();
    assert_eq!(row["rework_applicable"], json!(true));
    assert_eq!(row["rework_data_reliable"], json!(false));
}
