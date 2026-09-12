//! Tests for the reconciler-driven counterpart to
//! `orchestration/dispatch/item.rs`'s dispatch-time application. When
//! `RepoControlPlaneStore::upsert_runs` sees a run reach a terminal
//! `RunState`, it applies `status_map.on_succeeded`/`on_failed`/
//! `on_cancelled` through the workflow engine — unless a human has moved
//! the card since dispatch, in which case the human wins and the attempt
//! is recorded as `status_map_skipped_human_override`. Calls `upsert_runs`
//! directly (like `reconciler/broadcast.rs`): the surface under test is
//! `RepoControlPlaneStore`, not an HTTP endpoint.

use chrono::Utc;
use serde_json::{Value, json};
use tack_api::config::AppConfig;
use tack_api::orch_store::RepoControlPlaneStore;
use tack_core::models::{
    CreateItem, CreateProject, Item, ItemType, Priority, Project, ProjectType, UpdateItem,
};
use tack_db::repo::orch::{CreateControlPlane, NewOrchRun, NewOrchTask, UpsertOrchLink};
use tack_db::{Repository, init_pool, migrations};
use tack_orch::reconciler::ControlPlaneStore;
use tokio::sync::broadcast;
use uuid::Uuid;

/// A fresh in-memory-DB-backed `Repository`, migrations applied, with a
/// software project (scrum workflow: Backlog / To Do / In Progress /
/// In Review / Done, no explicit transitions — free-form) and one item.
async fn test_repo_with_item() -> (Repository, Project, Item) {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool);

    let workspace_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'Test Workspace', '{}')",
    )
    .bind(workspace_id.to_string())
    .execute(repo.pool())
    .await
    .expect("insert workspace");

    let project = repo
        .create_project(
            workspace_id,
            CreateProject {
                name: "Test Project".into(),
                description: None,
                project_type: ProjectType::Software,
                template: None,
            },
        )
        .await
        .expect("create project");

    let status = project
        .workflow
        .initial_status()
        .expect("initial status")
        .to_string();
    let item = repo
        .create_item(
            project.id,
            &status,
            CreateItem {
                title: "Test Item".into(),
                description: None,
                item_type: Some(ItemType::Task),
                parent_id: None,
                priority: Some(Priority::Medium),
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .expect("create item");

    (repo, project, item)
}

async fn test_plane_id(repo: &Repository) -> Uuid {
    repo.create_control_plane(CreateControlPlane {
        name: "docket-test".into(),
        kind: Some("docket".into()),
        base_url: "http://127.0.0.1:7331".into(),
        token: None,
    })
    .await
    .expect("create control plane")
    .id
}

async fn link(repo: &Repository, project_id: Uuid, plane_id: Uuid, status_map: Value) {
    repo.upsert_orch_link(
        project_id,
        UpsertOrchLink {
            control_plane_id: plane_id,
            remote_project: "demo".into(),
            pipeline_file: None,
            blueprint: None,
            auto_dispatch: false,
            budget_usd: None,
            status_map,
        },
    )
    .await
    .expect("upsert_orch_link");
}

/// Simulates `dispatch_item` having already run: an `orch_tasks`
/// attempt row exists with the given `remote_status`, and (separately,
/// mirroring what `dispatcher::apply_mapped_status` would have done) the
/// item's status is set directly.
async fn simulate_dispatch(
    repo: &Repository,
    item_id: Uuid,
    remote_status: &str,
    item_status: &str,
) {
    repo.upsert_orch_tasks(&[NewOrchTask {
        item_id,
        remote_task_id: "task-1".into(),
        remote_run_id: None,
        remote_status: remote_status.to_string(),
        attempt: 1,
        tokens_in: 0,
        tokens_out: 0,
        cost_usd_estimated: None,
        dispatched_at: Utc::now(),
        trusted: true,
    }])
    .await
    .expect("upsert_orch_tasks");
    set_status(repo, item_id, item_status).await;
}

/// Simulates a human (or anything else) dragging the card — a direct status
/// write, same as `simulate_dispatch`'s second half, just named for what the
/// test is standing in for.
async fn set_status(repo: &Repository, item_id: Uuid, status: &str) {
    repo.update_item(
        item_id,
        UpdateItem {
            status: Some(status.to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("update_item")
    .expect("item exists");
}

fn new_run(run_id: &str, item_id: Option<Uuid>, state: &str) -> NewOrchRun {
    NewOrchRun {
        run_id: run_id.to_string(),
        item_id,
        remote_project: "demo".into(),
        source: "cli".into(),
        state: state.to_string(),
        started_at: Some(Utc::now()),
        ended_at: None,
        error: None,
    }
}

fn store_with_context(repo: Repository, workspace_id: Uuid) -> RepoControlPlaneStore {
    let (tx, _rx) = broadcast::channel(16);
    RepoControlPlaneStore::new(repo, tx).with_app_context(AppConfig::default(), workspace_id, None)
}

// ─── Undisturbed: docket's terminal state wins ─────────────────────────────

#[tokio::test]
async fn on_succeeded_moves_item_to_done_when_untouched() {
    let (repo, project, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    link(
        &repo,
        project.id,
        plane_id,
        json!({
            "dispatch_from": ["To Do"],
            "on_running": "In Progress",
            "on_succeeded": "Done",
        }),
    )
    .await;
    simulate_dispatch(&repo, item.id, "running", "In Progress").await;

    let store = store_with_context(repo.clone(), Uuid::new_v4());
    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "succeeded")])
        .await
        .expect("upsert_runs");

    let updated = repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(
        updated.status, "Done",
        "an untouched item must move to on_succeeded's target"
    );

    let events = repo.list_orch_events_for_item(item.id, None).await.unwrap();
    assert!(
        events.is_empty(),
        "a clean application records no skip event: {events:?}"
    );
}

#[tokio::test]
async fn on_failed_moves_item_that_reached_waiting_approval() {
    // Proves the "which single key was last used" resolution, not a union:
    // the attempt went through waiting_approval, so the expected marker is
    // on_waiting_approval's value, not on_running's — and since the item is
    // still sitting there, this is *not* a human override.
    let (repo, project, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    link(
        &repo,
        project.id,
        plane_id,
        json!({
            "dispatch_from": ["To Do"],
            "on_running": "In Progress",
            "on_waiting_approval": "In Review",
            "on_failed": "In Review",
        }),
    )
    .await;
    simulate_dispatch(&repo, item.id, "waiting_approval", "In Review").await;

    let store = store_with_context(repo.clone(), Uuid::new_v4());
    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "failed")])
        .await
        .expect("upsert_runs");

    let updated = repo.get_item(item.id).await.unwrap().unwrap();
    // on_failed's target ("In Review") is also where the item already was,
    // so this is a no-op transition, not a rejection or a skip.
    assert_eq!(updated.status, "In Review");
}

// ─── Human wins ─────────────────────────────────────────────────────────

#[tokio::test]
async fn human_move_blocks_on_succeeded_despite_value_collision() {
    // on_running parks the item at "In Progress"; a human drags it to "In
    // Review" — which, deliberately, is also this status_map's own
    // on_waiting_approval *and* on_failed value (both naming the same
    // status is a real configuration shape, not just a test artifact). A
    // naive "is the current status any status_map value" check would
    // misread this as untouched; the real
    // check must compare against only the one key this attempt actually
    // used (on_running, since remote_status is "running", not
    // "waiting_approval") and correctly see the divergence.
    let (repo, project, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    link(
        &repo,
        project.id,
        plane_id,
        json!({
            "dispatch_from": ["To Do"],
            "on_running": "In Progress",
            "on_waiting_approval": "In Review",
            "on_succeeded": "Done",
            "on_failed": "In Review",
        }),
    )
    .await;
    simulate_dispatch(&repo, item.id, "running", "In Progress").await;

    // The human's intervention.
    set_status(&repo, item.id, "In Review").await;

    let store = store_with_context(repo.clone(), Uuid::new_v4());
    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "succeeded")])
        .await
        .expect("upsert_runs");

    let updated = repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(
        updated.status, "In Review",
        "the human's decision must not be silently reverted to Done"
    );

    let events = repo.list_orch_events_for_item(item.id, None).await.unwrap();
    assert_eq!(events.len(), 1, "the skip must be recorded: {events:?}");
    assert_eq!(events[0].event_type, "status_map_skipped_human_override");
    assert_eq!(events[0].payload["trigger"], "on_succeeded");
    assert_eq!(events[0].payload["target_status"], "Done");
    assert_eq!(events[0].payload["current_status"], "In Review");
}

#[tokio::test]
async fn human_move_to_unlisted_status_is_also_caught() {
    let (repo, project, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    link(
        &repo,
        project.id,
        plane_id,
        json!({
            "dispatch_from": ["To Do"],
            "on_running": "In Progress",
            "on_succeeded": "Done",
        }),
    )
    .await;
    simulate_dispatch(&repo, item.id, "running", "In Progress").await;
    set_status(&repo, item.id, "Backlog").await; // a human sent it back to the backlog

    let store = store_with_context(repo.clone(), Uuid::new_v4());
    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "succeeded")])
        .await
        .expect("upsert_runs");

    let updated = repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(updated.status, "Backlog");
}

// ─── Absent key: "do not touch the item's status" ──────────────────────────

#[tokio::test]
async fn an_absent_on_succeeded_key_leaves_the_item_untouched() {
    let (repo, project, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    link(
        &repo,
        project.id,
        plane_id,
        json!({
            "dispatch_from": ["To Do"],
            "on_running": "In Progress",
            // no on_succeeded
        }),
    )
    .await;
    simulate_dispatch(&repo, item.id, "running", "In Progress").await;

    let store = store_with_context(repo.clone(), Uuid::new_v4());
    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "succeeded")])
        .await
        .expect("upsert_runs");

    let updated = repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(updated.status, "In Progress");
    let events = repo.list_orch_events_for_item(item.id, None).await.unwrap();
    assert!(
        events.is_empty(),
        "an absent key is a deliberate no-op, not something to log: {events:?}"
    );
}

// ─── Non-terminal states never trigger anything ────────────────────────────

#[tokio::test]
async fn a_run_becoming_running_or_queued_never_touches_status_map() {
    let (repo, project, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    link(
        &repo,
        project.id,
        plane_id,
        json!({
            "dispatch_from": ["To Do"],
            "on_running": "In Progress",
            "on_succeeded": "Done",
        }),
    )
    .await;
    simulate_dispatch(&repo, item.id, "running", "In Progress").await;

    let store = store_with_context(repo.clone(), Uuid::new_v4());
    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "queued")])
        .await
        .expect("upsert_runs (queued)");
    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "running")])
        .await
        .expect("upsert_runs (running)");

    let updated = repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(
        updated.status, "In Progress",
        "queued/running have no reconciler-driven status_map trigger of their own"
    );
}

// ─── The workflow engine still governs the move ────────────────────────────

#[tokio::test]
async fn workflow_rejection_records_status_map_rejected_not_override() {
    let (repo, project, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    link(
        &repo,
        project.id,
        plane_id,
        json!({
            "dispatch_from": ["To Do"],
            "on_running": "In Progress",
            // "Nonexistent" is not a real scrum-workflow status; the engine
            // must refuse it exactly like it would a human-driven PATCH.
            "on_succeeded": "Nonexistent",
        }),
    )
    .await;
    simulate_dispatch(&repo, item.id, "running", "In Progress").await;

    let store = store_with_context(repo.clone(), Uuid::new_v4());
    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "succeeded")])
        .await
        .expect("upsert_runs");

    let updated = repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(
        updated.status, "In Progress",
        "a workflow-engine rejection must leave the item untouched"
    );

    let events = repo.list_orch_events_for_item(item.id, None).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].event_type, "status_map_rejected",
        "an engine rejection must use the existing status_map_rejected event type, not the \
         human-override one"
    );
}

// ─── No app context: the feature is inert, not broken ─────────────────────

#[tokio::test]
async fn without_app_context_a_terminal_run_is_a_silent_no_op() {
    let (repo, project, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    link(
        &repo,
        project.id,
        plane_id,
        json!({
            "dispatch_from": ["To Do"],
            "on_running": "In Progress",
            "on_succeeded": "Done",
        }),
    )
    .await;
    simulate_dispatch(&repo, item.id, "running", "In Progress").await;

    let (tx, _rx) = broadcast::channel(16);
    let store = RepoControlPlaneStore::new(repo.clone(), tx); // no with_app_context

    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "succeeded")])
        .await
        .expect("upsert_runs must not error just because status_map application is unavailable");

    let updated = repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(updated.status, "In Progress");
}
