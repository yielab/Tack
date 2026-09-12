//! Tests for the item/project agent-activity endpoints:
//! `GET /api/items/{id}/agent-activity` and
//! `GET /api/projects/{id}/agent-activity`.
//!
//! Covers: both routes 404 with `TACK_ORCH_ENABLE` unset;
//! an unknown item 404s on the detail endpoint; the bulk endpoint is an inner
//! join (items with no `orch_tasks` row are absent, never a null-status row);
//! the "latest attempt" tie-break (highest `attempt`, then `dispatched_at`
//! desc); attempts are newest-first; a task's `remote_run_id` correlates the
//! right `orch_runs` row and `orch_events`; `run.error` is `""` not `null`
//! when absent; `pricing_snapshot_at` is always `null`; approvals include
//! both pending and decided, newest-requested-first; and `events_truncated`
//! reflects whether any attempt predates the retention cutoff.

use crate::common;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use chrono::{Duration, Utc};
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::orch_runtime::OrchRuntime;
use tack_api::router::{AppState, build_router};
use tack_db::repo::orch::{NewOrchApproval, NewOrchEvent, NewOrchRun, NewOrchTask};
use tack_db::{Repository, init_pool, migrations};
use tokio::sync::broadcast;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Helpers (mirrors orchestration/control_plane/resource.rs's app_with_state) ─

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

fn new_task(
    item_id: Uuid,
    remote_task_id: &str,
    attempt: i64,
    dispatched_at: chrono::DateTime<Utc>,
) -> NewOrchTask {
    NewOrchTask {
        item_id,
        remote_task_id: remote_task_id.to_string(),
        remote_run_id: None,
        remote_status: "running".to_string(),
        attempt,
        tokens_in: 100,
        tokens_out: 200,
        cost_usd_estimated: Some(0.01),
        dispatched_at,
        trusted: true,
    }
}

// ─── Disabled-orchestration discipline ─────────────────────────────────────

#[tokio::test]
async fn both_routes_409_when_orch_disabled() {
    let (app, _) = common::test_app().await; // orch_enable defaults to false
    let fake = Uuid::new_v4();

    let res = req(
        &app,
        Method::GET,
        &format!("/api/items/{fake}/agent-activity"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{fake}/agent-activity"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let body = body_json(res).await;
    assert_eq!(body["error"]["code"], "orchestration_disabled");
}

#[tokio::test]
async fn item_agent_activity_404s_for_unknown_item() {
    let (app, _) = app_with_state(orch_config()).await;
    let res = req(
        &app,
        Method::GET,
        &format!("/api/items/{}/agent-activity", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn item_agent_activity_empty_for_item_with_no_dispatches() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Untouched item").await;

    let res = req(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/agent-activity"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["attempts"], json!([]));
    assert_eq!(v["approvals"], json!([]));
    assert_eq!(v["events_truncated"], false);
}

// ─── Bulk badge endpoint: inner join + latest-attempt tie-break ───────────

#[tokio::test]
async fn project_agent_activity_is_inner_join_excludes_items_with_no_tasks() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let dispatched_item = create_item(&app, project_id, "Dispatched").await;
    let _untouched_item = create_item(&app, project_id, "Never dispatched").await;

    state
        .repo
        .upsert_orch_tasks(&[new_task(dispatched_item, "task-1", 1, Utc::now())])
        .await
        .expect("seed task");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/agent-activity"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        1,
        "an item with zero orch_tasks rows must not appear at all (inner join, not a null-status row): {rows:?}"
    );
    assert_eq!(rows[0]["item_id"], dispatched_item.to_string());
}

#[tokio::test]
async fn project_agent_activity_returns_empty_rows_for_project_with_no_activity() {
    let (app, _) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    create_item(&app, project_id, "No activity").await;

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/agent-activity"),
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v["rows"], json!([]));
}

#[tokio::test]
async fn project_agent_activity_latest_attempt_wins_by_highest_attempt_number() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Redispatched item").await;

    let now = Utc::now();
    state
        .repo
        .upsert_orch_tasks(&[
            {
                let mut t = new_task(item_id, "task-attempt-1", 1, now - Duration::hours(2));
                t.remote_status = "failed".to_string();
                t
            },
            {
                let mut t = new_task(item_id, "task-attempt-2", 2, now - Duration::hours(1));
                t.remote_status = "running".to_string();
                t
            },
        ])
        .await
        .expect("seed tasks");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/agent-activity"),
        None,
    )
    .await;
    let v = body_json(res).await;
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "one row per item, not one per attempt");
    assert_eq!(rows[0]["attempt"], 2);
    assert_eq!(
        rows[0]["remote_status"], "running",
        "the higher attempt number must win regardless of dispatched_at ordering"
    );
}

#[tokio::test]
async fn project_agent_activity_ties_on_attempt_break_by_dispatched_at_desc() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Two same-attempt rows").await;

    let now = Utc::now();
    // Same attempt number, different remote_task_id (the PK requires that) and
    // different dispatched_at — the later dispatched_at must win.
    state
        .repo
        .upsert_orch_tasks(&[
            {
                let mut t = new_task(item_id, "task-older", 1, now - Duration::hours(2));
                t.remote_status = "failed".to_string();
                t
            },
            {
                let mut t = new_task(item_id, "task-newer", 1, now - Duration::minutes(1));
                t.remote_status = "done".to_string();
                t
            },
        ])
        .await
        .expect("seed tasks");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/projects/{project_id}/agent-activity"),
        None,
    )
    .await;
    let v = body_json(res).await;
    let rows = v["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]["remote_status"], "done",
        "on an attempt-number tie, the row with the later dispatched_at must win"
    );
}

// ─── Item detail endpoint: ordering, run/event correlation, honesty fields ─

#[tokio::test]
async fn item_agent_activity_attempts_are_newest_first() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Multi-attempt item").await;

    let now = Utc::now();
    state
        .repo
        .upsert_orch_tasks(&[
            new_task(item_id, "task-1", 1, now - Duration::hours(2)),
            new_task(item_id, "task-2", 2, now - Duration::hours(1)),
            new_task(item_id, "task-3", 3, now),
        ])
        .await
        .expect("seed tasks");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/agent-activity"),
        None,
    )
    .await;
    let v = body_json(res).await;
    let attempts = v["attempts"].as_array().unwrap();
    let attempt_numbers: Vec<i64> = attempts
        .iter()
        .map(|a| a["attempt"].as_i64().unwrap())
        .collect();
    assert_eq!(
        attempt_numbers,
        vec![3, 2, 1],
        "attempts must be newest (highest attempt number) first"
    );
    for a in attempts {
        assert!(
            a["pricing_snapshot_at"].is_null(),
            "no pricing-snapshot mechanism exists yet — must always be null"
        );
    }
}

#[tokio::test]
async fn item_agent_activity_correlates_run_and_events_via_remote_run_id() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Correlated item").await;
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

    let mut task = new_task(item_id, "task-with-run", 1, Utc::now());
    task.remote_run_id = Some("run-abc".to_string());
    state
        .repo
        .upsert_orch_tasks(&[task])
        .await
        .expect("seed task");

    state
        .repo
        .upsert_orch_runs(
            plane.id,
            &[NewOrchRun {
                run_id: "run-abc".to_string(),
                item_id: Some(item_id),
                remote_project: "my-remote-project".to_string(),
                source: "webhook".to_string(),
                state: "running".to_string(),
                started_at: Some(Utc::now()),
                ended_at: None,
                error: None,
            }],
        )
        .await
        .expect("seed run");

    state
        .repo
        .upsert_orch_events(
            plane.id,
            &[NewOrchEvent {
                id: Uuid::new_v4(),
                item_id: Some(item_id),
                run_id: Some("run-abc".to_string()),
                event_type: "tool_call".to_string(),
                payload: json!({"tool": "git"}),
                occurred_at: Utc::now(),
            }],
        )
        .await
        .expect("seed event");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/agent-activity"),
        None,
    )
    .await;
    let v = body_json(res).await;
    let attempts = v["attempts"].as_array().unwrap();
    assert_eq!(attempts.len(), 1);
    let run = &attempts[0]["run"];
    assert_eq!(run["run_id"], "run-abc");
    assert_eq!(run["source"], "webhook");
    assert_eq!(
        run["error"], "",
        "error must be an empty string, never null, when the run has no error"
    );
    let events = attempts[0]["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event_type"], "tool_call");
}

#[tokio::test]
async fn item_agent_activity_run_is_null_when_remote_run_id_unresolved() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Queued item").await;

    let mut task = new_task(item_id, "task-no-run-yet", 1, Utc::now());
    task.remote_run_id = None;
    state
        .repo
        .upsert_orch_tasks(&[task])
        .await
        .expect("seed task");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/agent-activity"),
        None,
    )
    .await;
    let v = body_json(res).await;
    let attempts = v["attempts"].as_array().unwrap();
    assert!(attempts[0]["run"].is_null());
    assert_eq!(attempts[0]["events"], json!([]));
}

#[tokio::test]
async fn item_agent_activity_includes_pending_and_decided_approvals_newest_first() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Approvals item").await;
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

    let now = Utc::now();
    state
        .repo
        .upsert_orch_approvals(
            plane.id,
            &[
                NewOrchApproval {
                    token: "tok-older".to_string(),
                    item_id: Some(item_id),
                    remote_task_id: None,
                    agent: Some("builder".to_string()),
                    action: Some("git push".to_string()),
                    state: "granted".to_string(),
                    requested_at: now - Duration::hours(2),
                    decided_at: Some(now - Duration::hours(1)),
                },
                NewOrchApproval {
                    token: "tok-newer".to_string(),
                    item_id: Some(item_id),
                    remote_task_id: None,
                    agent: Some("builder".to_string()),
                    action: Some("rm -rf".to_string()),
                    state: "pending".to_string(),
                    requested_at: now,
                    decided_at: None,
                },
            ],
        )
        .await
        .expect("seed approvals");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/agent-activity"),
        None,
    )
    .await;
    let v = body_json(res).await;
    let approvals = v["approvals"].as_array().unwrap();
    assert_eq!(approvals.len(), 2, "both pending and decided must appear");
    assert_eq!(approvals[0]["token"], "tok-newer", "newest-requested first");
    assert_eq!(approvals[1]["token"], "tok-older");
    assert!(approvals[1]["decided_at"].is_string());
}

// ─── events_truncated honesty signal ───────────────────────────────────────

#[tokio::test]
async fn events_truncated_is_false_when_nothing_predates_the_retention_cutoff() {
    let (app, state) = app_with_state(orch_config()).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Fresh item").await;

    state
        .repo
        .upsert_orch_tasks(&[new_task(item_id, "task-fresh", 1, Utc::now())])
        .await
        .expect("seed task");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/agent-activity"),
        None,
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(v["events_truncated"], false);
    assert_eq!(
        v["events_retention_days"],
        json!(AppConfig::default().orch_event_retention_days)
    );
}

#[tokio::test]
async fn events_truncated_is_true_when_an_attempt_predates_the_retention_cutoff() {
    let config = AppConfig {
        orch_event_retention_days: 1,
        ..orch_config()
    };
    let (app, state) = app_with_state(config).await;
    let project_id = common::create_project(&app, "Agent Activity Test Project", "software").await;
    let item_id = create_item(&app, project_id, "Old item").await;

    // Dispatched well before the 1-day retention cutoff.
    state
        .repo
        .upsert_orch_tasks(&[new_task(
            item_id,
            "task-ancient",
            1,
            Utc::now() - Duration::days(30),
        )])
        .await
        .expect("seed task");

    let res = req(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/agent-activity"),
        None,
    )
    .await;
    let v = body_json(res).await;
    assert_eq!(
        v["events_truncated"], true,
        "an attempt older than the retention window must flag that its events may have been rolled up"
    );
    assert_eq!(v["events_retention_days"], json!(1));
}
