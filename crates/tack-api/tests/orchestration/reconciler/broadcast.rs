//! Tests whether `RepoControlPlaneStore::upsert_runs`/`upsert_approvals`
//! (`crates/tack-api/src/orch_store.rs`) emit `BoardEvent::AgentRunUpdated`/
//! `ApprovalPending` when — and only when — a poll actually changes
//! something (a second identical poll broadcasts nothing). Every test
//! subscribes to the same broadcast channel the store was constructed with
//! and asserts on what does or doesn't arrive, rather than on the DB state
//! (covered by `crates/tack-db/tests/repository/orch_repo.rs`/
//! `crates/tack-orch/tests/ingestion/runs.rs`).

use chrono::Utc;
use tack_api::handlers::websocket::BoardEvent;
use tack_api::orch_store::RepoControlPlaneStore;
use tack_core::models::{CreateItem, CreateProject, ItemType, Priority, ProjectType};
use tack_db::repo::orch::{CreateControlPlane, NewOrchApproval, NewOrchRun};
use tack_db::{Repository, init_pool, migrations};
use tack_orch::reconciler::ControlPlaneStore;
use tokio::sync::broadcast;
use uuid::Uuid;

/// A fresh in-memory-DB-backed `Repository`, migrations applied, with a
/// workspace/project/item ready to correlate runs and approvals against.
async fn test_repo_with_item() -> (Repository, Uuid, tack_core::models::Item) {
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

    (repo, project.id, item)
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

fn new_approval(token: &str, item_id: Option<Uuid>, state: &str) -> NewOrchApproval {
    NewOrchApproval {
        token: token.to_string(),
        item_id,
        remote_task_id: Some("task-1".into()),
        agent: Some("reviewer".into()),
        action: Some("merge".into()),
        state: state.to_string(),
        requested_at: Utc::now(),
        decided_at: None,
    }
}

// ─── AgentRunUpdated ────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_runs_broadcasts_for_a_new_correlated_run() {
    let (repo, project_id, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    let (tx, mut rx) = broadcast::channel::<BoardEvent>(16);
    let store = RepoControlPlaneStore::new(repo, tx);

    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "running")])
        .await
        .expect("upsert_runs");

    let event = rx.try_recv().expect("a new correlated run must broadcast");
    match event {
        BoardEvent::AgentRunUpdated {
            project_id: pid,
            item_id,
            run_id,
            state,
        } => {
            assert_eq!(pid, project_id);
            assert_eq!(item_id, item.id);
            assert_eq!(run_id, "run-1");
            assert_eq!(state, "running");
        }
        other => panic!("expected AgentRunUpdated, got {other:?}"),
    }
    assert!(
        rx.try_recv().is_err(),
        "exactly one event should have been broadcast"
    );
}

/// The requirement: re-polling the same run with
/// byte-identical data must not broadcast anything the second time — the
/// reconciler polls every `TACK_ORCH_POLL_SECS` forever, and a naive
/// broadcast-on-every-upsert would flood every connected client.
#[tokio::test]
async fn a_second_identical_poll_of_the_same_run_broadcasts_nothing() {
    let (repo, _project_id, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    let (tx, mut rx) = broadcast::channel::<BoardEvent>(16);
    let store = RepoControlPlaneStore::new(repo, tx);

    let run = new_run("run-1", Some(item.id), "running");
    store
        .upsert_runs(plane_id, std::slice::from_ref(&run))
        .await
        .expect("first upsert_runs");
    rx.try_recv().expect("first poll broadcasts once");

    // Second poll: identical run_id, item_id, and state.
    store
        .upsert_runs(plane_id, std::slice::from_ref(&run))
        .await
        .expect("second upsert_runs");

    assert!(
        rx.try_recv().is_err(),
        "a byte-identical re-poll must not broadcast a second event"
    );
}

#[tokio::test]
async fn upsert_runs_broadcasts_again_when_state_changes() {
    let (repo, project_id, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    let (tx, mut rx) = broadcast::channel::<BoardEvent>(16);
    let store = RepoControlPlaneStore::new(repo, tx);

    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "queued")])
        .await
        .expect("first upsert_runs");
    rx.try_recv().expect("queued state broadcasts");

    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "running")])
        .await
        .expect("second upsert_runs");
    let event = rx
        .try_recv()
        .expect("a real state transition must broadcast again");
    match event {
        BoardEvent::AgentRunUpdated {
            project_id: pid,
            state,
            ..
        } => {
            assert_eq!(pid, project_id);
            assert_eq!(state, "running");
        }
        other => panic!("expected AgentRunUpdated, got {other:?}"),
    }
}

/// An uncorrelated run (e.g. dispatched from docket's own CLI) has no Tack
/// project to filter a `BoardEvent` into, so it's persisted but not
/// broadcast — until a later poll learns its attribution, at which point it
/// broadcasts once, even though the `state` itself didn't change.
#[tokio::test]
async fn uncorrelated_run_waits_for_attribution_before_broadcast() {
    let (repo, project_id, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    let (tx, mut rx) = broadcast::channel::<BoardEvent>(16);
    let store = RepoControlPlaneStore::new(repo, tx);

    store
        .upsert_runs(plane_id, &[new_run("run-1", None, "running")])
        .await
        .expect("first upsert_runs (uncorrelated)");
    assert!(
        rx.try_recv().is_err(),
        "an uncorrelated run has no project to broadcast into"
    );

    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "running")])
        .await
        .expect("second upsert_runs (now correlated)");
    let event = rx
        .try_recv()
        .expect("newly-learned attribution is a real change worth broadcasting");
    match event {
        BoardEvent::AgentRunUpdated {
            project_id: pid,
            item_id,
            state,
            ..
        } => {
            assert_eq!(pid, project_id);
            assert_eq!(item_id, item.id);
            assert_eq!(state, "running", "state itself did not change");
        }
        other => panic!("expected AgentRunUpdated, got {other:?}"),
    }

    // And a third, now-identical poll broadcasts nothing again.
    store
        .upsert_runs(plane_id, &[new_run("run-1", Some(item.id), "running")])
        .await
        .expect("third upsert_runs (identical)");
    assert!(
        rx.try_recv().is_err(),
        "identical after attribution is learned must still broadcast nothing"
    );
}

// ─── ApprovalPending ────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_approvals_broadcasts_for_new_pending_correlated_row() {
    let (repo, project_id, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    let (tx, mut rx) = broadcast::channel::<BoardEvent>(16);
    let store = RepoControlPlaneStore::new(repo, tx);

    store
        .upsert_approvals(plane_id, &[new_approval("tok-1", Some(item.id), "pending")])
        .await
        .expect("upsert_approvals");

    let event = rx
        .try_recv()
        .expect("a new pending correlated approval must broadcast");
    match event {
        BoardEvent::ApprovalPending {
            project_id: pid,
            item_id,
            token,
            action,
        } => {
            assert_eq!(pid, project_id);
            assert_eq!(item_id, item.id);
            assert_eq!(token, "tok-1");
            assert_eq!(action.as_deref(), Some("merge"));
        }
        other => panic!("expected ApprovalPending, got {other:?}"),
    }
    assert!(rx.try_recv().is_err());
}

/// Same requirement as for runs, for approvals.
#[tokio::test]
async fn a_second_identical_pending_poll_broadcasts_nothing() {
    let (repo, _project_id, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    let (tx, mut rx) = broadcast::channel::<BoardEvent>(16);
    let store = RepoControlPlaneStore::new(repo, tx);

    let approval = new_approval("tok-1", Some(item.id), "pending");
    store
        .upsert_approvals(plane_id, std::slice::from_ref(&approval))
        .await
        .expect("first upsert_approvals");
    rx.try_recv().expect("first poll broadcasts once");

    store
        .upsert_approvals(plane_id, std::slice::from_ref(&approval))
        .await
        .expect("second upsert_approvals");

    assert!(
        rx.try_recv().is_err(),
        "a byte-identical re-poll must not broadcast a second event"
    );
}

/// `ApprovalPending` only fires on the transition into `pending` — a
/// granted/denied approval is no longer actionable, so
/// it must not re-trigger a "pending" board nudge.
#[tokio::test]
async fn upsert_approvals_does_not_broadcast_for_a_non_pending_state() {
    let (repo, _project_id, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    let (tx, mut rx) = broadcast::channel::<BoardEvent>(16);
    let store = RepoControlPlaneStore::new(repo, tx);

    store
        .upsert_approvals(plane_id, &[new_approval("tok-1", Some(item.id), "granted")])
        .await
        .expect("upsert_approvals");

    assert!(
        rx.try_recv().is_err(),
        "a non-pending approval must never emit ApprovalPending"
    );
}

/// Mirrors the run case: an uncorrelated approval is persisted (the
/// fleet-wide inbox still surfaces it) but not broadcast until a later poll
/// learns which item it belongs to.
#[tokio::test]
async fn uncorrelated_approval_waits_for_attribution_before_broadcast() {
    let (repo, project_id, item) = test_repo_with_item().await;
    let plane_id = test_plane_id(&repo).await;
    let (tx, mut rx) = broadcast::channel::<BoardEvent>(16);
    let store = RepoControlPlaneStore::new(repo, tx);

    store
        .upsert_approvals(plane_id, &[new_approval("tok-1", None, "pending")])
        .await
        .expect("first upsert_approvals (uncorrelated)");
    assert!(
        rx.try_recv().is_err(),
        "an uncorrelated approval has no project to broadcast into"
    );

    store
        .upsert_approvals(plane_id, &[new_approval("tok-1", Some(item.id), "pending")])
        .await
        .expect("second upsert_approvals (now correlated)");
    let event = rx
        .try_recv()
        .expect("newly-learned attribution is a real change worth broadcasting");
    match event {
        BoardEvent::ApprovalPending {
            project_id: pid,
            item_id,
            ..
        } => {
            assert_eq!(pid, project_id);
            assert_eq!(item_id, item.id);
        }
        other => panic!("expected ApprovalPending, got {other:?}"),
    }
}
