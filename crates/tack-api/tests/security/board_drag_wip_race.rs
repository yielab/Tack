//! Tests `handlers::items::update_item` — the ordinary board-drag / API
//! PATCH path — must not read a WIP-limited column's item count and then
//! write the new status as two separate, unlocked steps
//! (`Repository::count_items_by_status` followed by a plain
//! `Repository::update_item`), the same race
//! `crates/tack-api/tests/security/wip_limit_race.rs` guards against on the
//! dispatch path, but only on that one call site. This is the everyday
//! path: a human dragging cards on the board, or any API client calling
//! `PATCH /api/items/{id}` directly.
//!
//! This drives `N` genuinely concurrent `PATCH /api/items/{id}` requests —
//! `N` distinct items, all eligible, all targeting the same WIP-limited
//! column — through the real HTTP path and asserts the column's final count
//! never exceeds its configured limit.
//!
//! Unlike the dispatch race test, there is no outbound HTTP round trip on
//! this path to hold open with a mock delay — the whole request is
//! in-process. The race window here is the handful of `.await` points
//! between the count read and the status write, plus contention over the
//! pool's five connections when twelve requests arrive at once on a
//! multi-thread runtime; that's enough to reproduce the race reliably.

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

// ─── Helpers (mirrors wip_limit_race.rs) ───────────────────────────────────

async fn app_with_state() -> (Router, AppState) {
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
        ..AppConfig::default()
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

// ─── The race ───────────────────────────────────────────────────────────

/// `N` distinct items, all sitting in "Backlog", all `PATCH`ed to "In
/// Progress" (scrum's WIP-limited column, limit 5) at the same moment.
/// However many of the `N` requests win the race, the column must never end
/// up holding more than its configured limit. "software" -> scrum_workflow():
/// "In Progress" has wip_limit = Some(5) and `transitions: None` (no
/// explicit-transition restriction), so every item created here can go
/// straight from the initial status ("Backlog") to "In Progress".
#[tokio::test(flavor = "multi_thread", worker_threads = 16)]
async fn concurrent_board_drags_into_the_same_wip_limited_column_never_exceed_the_limit() {
    const N: usize = 12;
    const WIP_LIMIT: i64 = 5;

    let (app, state) = app_with_state().await;
    let project_id = common::create_project(&app, "Board Drag WIP Race Test", "software").await;

    let mut item_ids = Vec::with_capacity(N);
    for i in 0..N {
        item_ids.push(create_item(&app, project_id, &format!("Item {i}")).await);
    }

    // Fire all N PATCHes at once — real concurrency (multi-thread runtime +
    // tokio::spawn), not just interleaved polling on one task.
    let handles: Vec<_> = item_ids
        .iter()
        .copied()
        .map(|item_id| {
            let app = app.clone();
            tokio::spawn(async move {
                req(
                    &app,
                    Method::PATCH,
                    &format!("/api/items/{item_id}"),
                    Some(json!({"status": "In Progress"})),
                )
                .await
                .status()
            })
        })
        .collect();

    let mut statuses = Vec::with_capacity(N);
    for h in handles {
        statuses.push(h.await.expect("PATCH task panicked"));
    }
    // Every request reaches the handler: either 200 (applied) or 400 (WIP
    // limit rejected) — never a 500, and never anything else.
    for s in &statuses {
        assert!(
            *s == StatusCode::OK || *s == StatusCode::BAD_REQUEST,
            "unexpected status {s}"
        );
    }

    let final_count = state
        .repo
        .count_items_by_status(project_id, "In Progress")
        .await
        .expect("count_items_by_status");

    assert!(
        final_count <= WIP_LIMIT,
        "WIP limit for 'In Progress' is {WIP_LIMIT}, but {final_count} of {N} concurrently \
         PATCHed items ended up there — handlers::items::update_item's WIP-limit check and its \
         status write are not atomic against concurrent writers into the same column"
    );

    // Sanity: nothing vanished, and every item that didn't make it into "In
    // Progress" is still sitting in "Backlog" — a rejected status change
    // must leave the item untouched.
    let backlog_count = state
        .repo
        .count_items_by_status(project_id, "Backlog")
        .await
        .expect("count_items_by_status");
    assert_eq!(
        final_count + backlog_count,
        N as i64,
        "every item must be in exactly one of Backlog/In Progress — none lost, none duplicated"
    );
}

/// A `PATCH` that only touches non-status fields (title here) must not pay
/// for the WIP-checked transaction path at all — it should behave exactly
/// as it did before, going straight through the ordinary field-by-field
/// `Repository::update_item`.
#[tokio::test]
async fn patch_without_a_status_change_is_unaffected() {
    let (app, _state) = app_with_state().await;
    let project_id = common::create_project(&app, "Board Drag WIP Race Test", "software").await;
    let item_id = create_item(&app, project_id, "Untouched status").await;

    let res = req(
        &app,
        Method::PATCH,
        &format!("/api/items/{item_id}"),
        Some(json!({"title": "Renamed, status untouched"})),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{:?}", body_json(res).await);
    let v = body_json(res).await;
    assert_eq!(v["title"], "Renamed, status untouched");
    // Status must be exactly what it was at creation (the project's initial
    // status, "Backlog" for scrum) — untouched by the title-only PATCH.
    assert_eq!(v["status"], "Backlog");
}
