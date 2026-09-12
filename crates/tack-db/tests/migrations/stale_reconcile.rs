//! Stale `orch_tasks`/`orch_approvals` reconciliation.
//!
//! Nothing updates `orch_tasks.remote_status`/`orch_approvals.state` once
//! dispatched except a poll of a *reachable* control plane. A plane that goes
//! `unreachable` and never recovers leaves any "active" row active forever —
//! which also permanently blocks legacy redispatch for that item (via
//! `dispatcher::is_active_task_status`). `Repository::reconcile_stale_orch_tasks`/
//! `reconcile_stale_orch_approvals` are the fix: a local-only sweep (no HTTP
//! call, so it cannot perturb `docket_tick_contract_test.rs`'s pinned
//! per-tick request sequence).

use crate::common::{create_test_workspace, make_item, make_project, setup_test_db};
use chrono::{Duration, Utc};
use tack_db::repo::orch::{CreateControlPlane, UpsertOrchLink};
use uuid::Uuid;

/// Creates a control plane under `name` and links it to `project_id` (the
/// `orch_tasks` sweep routes through a project's link); returns its id.
async fn make_linked_control_plane(
    repo: &tack_db::Repository,
    project_id: Uuid,
    name: &str,
) -> Uuid {
    let plane_id = repo
        .create_control_plane(CreateControlPlane {
            name: name.into(),
            kind: None,
            base_url: "http://127.0.0.1:1".into(),
            token: None,
        })
        .await
        .unwrap()
        .id;
    repo.upsert_orch_link(
        project_id,
        UpsertOrchLink {
            control_plane_id: plane_id,
            remote_project: "demo".into(),
            pipeline_file: None,
            blueprint: None,
            auto_dispatch: false,
            budget_usd: None,
            status_map: serde_json::json!({}),
        },
    )
    .await
    .unwrap();
    plane_id
}

/// Creates a bare control plane under `name`, unlinked — the `orch_approvals`
/// sweep keys off `orch_approvals.control_plane_id` directly, no link needed.
async fn make_control_plane(repo: &tack_db::Repository, name: &str) -> Uuid {
    repo.create_control_plane(CreateControlPlane {
        name: name.into(),
        kind: None,
        base_url: "http://127.0.0.1:1".into(),
        token: None,
    })
    .await
    .unwrap()
    .id
}

async fn insert_orch_task(
    repo: &tack_db::Repository,
    item_id: Uuid,
    remote_task_id: &str,
    remote_status: &str,
    dispatched_at: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO orch_tasks (
            item_id, remote_task_id, remote_status, attempt, dispatched_at,
            trusted, created_at, updated_at
         ) VALUES (?, ?, ?, 1, ?, 1, ?, ?)",
    )
    .bind(item_id.to_string())
    .bind(remote_task_id)
    .bind(remote_status)
    .bind(dispatched_at.to_rfc3339())
    .bind(dispatched_at.to_rfc3339())
    .bind(dispatched_at.to_rfc3339())
    .execute(repo.pool())
    .await
    .expect("insert orch_tasks fixture row");
}

async fn task_status(repo: &tack_db::Repository, item_id: Uuid, remote_task_id: &str) -> String {
    let row: (String,) = sqlx::query_as(
        "SELECT remote_status FROM orch_tasks WHERE item_id = ? AND remote_task_id = ?",
    )
    .bind(item_id.to_string())
    .bind(remote_task_id)
    .fetch_one(repo.pool())
    .await
    .unwrap();
    row.0
}

async fn insert_orch_approval(
    repo: &tack_db::Repository,
    control_plane_id: Uuid,
    item_id: Uuid,
    token: &str,
    state: &str,
    requested_at: chrono::DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO orch_approvals (
            token, control_plane_id, item_id, remote_task_id, agent, action, state,
            requested_at, created_at, updated_at
         ) VALUES (?, ?, ?, 'rt', 'coder', 'do-thing', ?, ?, ?, ?)",
    )
    .bind(token)
    .bind(control_plane_id.to_string())
    .bind(item_id.to_string())
    .bind(state)
    .bind(requested_at.to_rfc3339())
    .bind(requested_at.to_rfc3339())
    .bind(requested_at.to_rfc3339())
    .execute(repo.pool())
    .await
    .expect("insert orch_approvals fixture row");
}

async fn approval_state(repo: &tack_db::Repository, token: &str) -> String {
    let row: (String,) = sqlx::query_as("SELECT state FROM orch_approvals WHERE token = ?")
        .bind(token)
        .fetch_one(repo.pool())
        .await
        .unwrap();
    row.0
}

// ─── orch_tasks ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn stale_task_on_long_unreachable_plane_is_marked_stale() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_linked_control_plane(&repo, project.id, "dead-plane").await;

    let long_ago = Utc::now() - Duration::days(30);
    repo.update_control_plane_health(plane_id, "unreachable", Some(long_ago), 50, None)
        .await
        .unwrap();
    insert_orch_task(&repo, item.id, "task-stale", "running", long_ago).await;

    let cutoff = Utc::now() - Duration::days(7);
    let affected = repo.reconcile_stale_orch_tasks(cutoff).await.unwrap();

    assert_eq!(affected, 1);
    assert_eq!(task_status(&repo, item.id, "task-stale").await, "stale");
}

#[tokio::test]
async fn task_on_healthy_plane_is_never_marked_stale() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_linked_control_plane(&repo, project.id, "healthy-plane").await;

    let long_ago = Utc::now() - Duration::days(30);
    // Healthy, with an old `last_seen_at` — a synthetic combination the reconciler
    // itself would never produce (a healthy tick always sets `last_seen_at` to
    // "now"), constructed directly to isolate the `health` column's own necessity in
    // the sweep's WHERE clause from `last_seen_at`'s. If the sweep only checked
    // `last_seen_at` and not `health`, this row would be wrongly swept despite being
    // healthy.
    sqlx::query("UPDATE control_planes SET health = 'healthy', last_seen_at = ? WHERE id = ?")
        .bind(long_ago.to_rfc3339())
        .bind(plane_id.to_string())
        .execute(repo.pool())
        .await
        .unwrap();
    insert_orch_task(&repo, item.id, "task-healthy", "running", long_ago).await;

    let cutoff = Utc::now() - Duration::days(7);
    let affected = repo.reconcile_stale_orch_tasks(cutoff).await.unwrap();

    assert_eq!(affected, 0);
    assert_eq!(
        task_status(&repo, item.id, "task-healthy").await,
        "running",
        "a task on a healthy plane must be left untouched, load-bearing proof: this \
         is the same query the previous test proves does mark a row stale when the \
         plane is unreachable"
    );
}

#[tokio::test]
async fn recently_unreachable_plane_does_not_yet_stale_its_tasks() {
    // A plane that just went unreachable might still recover this tick — only a
    // sustained outage (last_seen_at itself predates the cutoff) counts as stale.
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_linked_control_plane(&repo, project.id, "recently-down").await;

    // last_seen_at is recent even though health is already "unreachable" — a plane
    // that just flipped state, not one down for a long time.
    repo.update_control_plane_health(plane_id, "unreachable", Some(Utc::now()), 3, None)
        .await
        .unwrap();

    let long_ago = Utc::now() - Duration::days(30);
    insert_orch_task(&repo, item.id, "task-recent", "running", long_ago).await;

    let cutoff = Utc::now() - Duration::days(7);
    let affected = repo.reconcile_stale_orch_tasks(cutoff).await.unwrap();

    assert_eq!(affected, 0);
    assert_eq!(task_status(&repo, item.id, "task-recent").await, "running");
}

#[tokio::test]
async fn terminal_task_status_is_never_touched_by_sweep() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_linked_control_plane(&repo, project.id, "dead-plane-2").await;

    let long_ago = Utc::now() - Duration::days(30);
    repo.update_control_plane_health(plane_id, "unreachable", Some(long_ago), 50, None)
        .await
        .unwrap();
    insert_orch_task(&repo, item.id, "task-done", "succeeded", long_ago).await;

    let cutoff = Utc::now() - Duration::days(7);
    let affected = repo.reconcile_stale_orch_tasks(cutoff).await.unwrap();

    assert_eq!(affected, 0);
    assert_eq!(
        task_status(&repo, item.id, "task-done").await,
        "succeeded",
        "an already-terminal remote_status must never be overwritten"
    );
}

// ─── orch_approvals ─────────────────────────────────────────────────────────

#[tokio::test]
async fn pending_approval_on_long_unreachable_plane_expires() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_control_plane(&repo, "dead-plane-3").await;

    let long_ago = Utc::now() - Duration::days(30);
    repo.update_control_plane_health(plane_id, "unreachable", Some(long_ago), 50, None)
        .await
        .unwrap();
    insert_orch_approval(&repo, plane_id, item.id, "tok-stale", "pending", long_ago).await;

    let cutoff = Utc::now() - Duration::days(7);
    let affected = repo.reconcile_stale_orch_approvals(cutoff).await.unwrap();

    assert_eq!(affected, 1);
    assert_eq!(approval_state(&repo, "tok-stale").await, "expired");
}

#[tokio::test]
async fn decided_approval_is_never_touched_by_sweep() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_control_plane(&repo, "dead-plane-4").await;

    let long_ago = Utc::now() - Duration::days(30);
    repo.update_control_plane_health(plane_id, "unreachable", Some(long_ago), 50, None)
        .await
        .unwrap();
    insert_orch_approval(&repo, plane_id, item.id, "tok-granted", "granted", long_ago).await;

    let cutoff = Utc::now() - Duration::days(7);
    let affected = repo.reconcile_stale_orch_approvals(cutoff).await.unwrap();

    assert_eq!(affected, 0);
    assert_eq!(approval_state(&repo, "tok-granted").await, "granted");
}
