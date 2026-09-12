//! Shared fixtures for `runs.rs` and `traces.rs`: both mount the same
//! `/health` and `/status.json` wiremock responses, link a control plane to
//! the seeded project through the same `TestRepoStore`, and poll the same
//! `spawn_reconciler`/`wait_for_tick_after` pair instead of sleeping a
//! guessed duration. `TestRepoStore` is a test-only `ControlPlaneStore`
//! wrapping `Repository` directly, since `tack-orch` must never depend on
//! `tack-api` for the real `orch_store::RepoControlPlaneStore` (same
//! mechanical shape, so a passing test here evidences that trait too).
//! `retention.rs` doesn't use any of this.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use tack_db::Repository;
use tack_db::repo::orch::{
    CreateControlPlane, NewOrchApproval, NewOrchEvent, NewOrchMetric, NewOrchRun, NewOrchTask,
    OrchApproval, OrchRun, UpsertOrchLink,
};
use tack_orch::adapters::docket::DocketAdapter;
use tack_orch::reconciler::{
    ControlPlaneStore, HealthRecord, ReconcilerConfig, RegisteredPlane, spawn_reconcilers,
};
use tack_orch::{ControlPlane, OrchError};

use crate::common::{create_test_workspace, make_item, make_project};

pub(crate) struct TestRepoStore {
    pub(crate) repo: Repository,
}

#[async_trait::async_trait]
impl ControlPlaneStore for TestRepoStore {
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
        let rows = self
            .repo
            .list_control_planes()
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))?;

        let mut planes = Vec::new();
        for row in rows {
            if row.kind != "docket" {
                continue;
            }
            let token = self
                .repo
                .get_control_plane_token(row.id)
                .await
                .map_err(|e| OrchError::Unavailable(e.to_string()))?;
            let adapter = DocketAdapter::new(row.base_url.clone(), token)
                .map_err(|e| OrchError::Unavailable(e.to_string()))?;
            planes.push(RegisteredPlane {
                id: row.id,
                control_plane: Arc::new(adapter) as Arc<dyn ControlPlane>,
            });
        }
        Ok(planes)
    }

    async fn record_health(
        &self,
        control_plane_id: Uuid,
        record: &HealthRecord,
    ) -> Result<(), OrchError> {
        self.repo
            .update_control_plane_health(
                control_plane_id,
                record.health.as_str(),
                record.last_seen_at,
                record.consecutive_failures,
                record.api_version.as_deref(),
            )
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    async fn list_linked_projects(&self, control_plane_id: Uuid) -> Result<Vec<String>, OrchError> {
        let links = self
            .repo
            .list_orch_links_for_plane(control_plane_id)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))?;
        Ok(links.into_iter().map(|l| l.remote_project).collect())
    }

    async fn find_item_for_remote_task(
        &self,
        remote_task_id: &str,
    ) -> Result<Option<Uuid>, OrchError> {
        let task = self
            .repo
            .find_orch_task_by_remote_task_id(remote_task_id)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))?;
        Ok(task.map(|t| t.item_id))
    }

    async fn upsert_runs(
        &self,
        control_plane_id: Uuid,
        runs: &[NewOrchRun],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_runs(control_plane_id, runs)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    async fn upsert_approvals(
        &self,
        control_plane_id: Uuid,
        approvals: &[NewOrchApproval],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_approvals(control_plane_id, approvals)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    async fn upsert_metrics(
        &self,
        control_plane_id: Uuid,
        metrics: &[NewOrchMetric],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_metrics(control_plane_id, metrics)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    async fn list_trace_cursors(
        &self,
        control_plane_id: Uuid,
    ) -> Result<std::collections::HashMap<String, String>, OrchError> {
        let cursors = self
            .repo
            .list_trace_cursors(control_plane_id)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))?;
        Ok(cursors
            .into_iter()
            .map(|c| (c.remote_project, c.cursor))
            .collect())
    }

    async fn set_trace_cursor(
        &self,
        control_plane_id: Uuid,
        remote_project: &str,
        cursor: &str,
    ) -> Result<(), OrchError> {
        self.repo
            .set_trace_cursor(control_plane_id, remote_project, cursor)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }

    async fn upsert_events(
        &self,
        control_plane_id: Uuid,
        events: &[NewOrchEvent],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_events(control_plane_id, events)
            .await
            .map_err(|e| OrchError::Unavailable(e.to_string()))
    }
}

pub(crate) const HEALTH_BODY: &str = r#"{"status":"ok","gateway":0}"#;
pub(crate) const STATUS_BODY: &str = r#"{"apiVersion":"2","timestamp":"2026-08-04T00:00:00Z","gateway":"inactive","channels":[],"agents":[],"totalCostUsd":0.0}"#;
pub(crate) const EMPTY_APPROVALS_BODY: &str = r#"{"pending":[]}"#;

pub(crate) async fn mount_health_and_status(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HEALTH_BODY))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/status.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(STATUS_BODY))
        .mount(server)
        .await;
}

pub(crate) async fn seed_control_plane_and_link(
    repo: &Repository,
    project_id: Uuid,
    base_url: &str,
) -> Uuid {
    let plane = repo
        .create_control_plane(CreateControlPlane {
            name: "Test Docket".into(),
            kind: None,
            base_url: base_url.to_string(),
            token: None,
        })
        .await
        .expect("create control plane");

    repo.upsert_orch_link(
        project_id,
        UpsertOrchLink {
            control_plane_id: plane.id,
            remote_project: "demo".into(),
            pipeline_file: None,
            blueprint: None,
            auto_dispatch: false,
            budget_usd: None,
            status_map: serde_json::json!({}),
        },
    )
    .await
    .expect("create orch link");

    plane.id
}

pub(crate) struct ProjectFixture {
    pub(crate) project: tack_core::models::Project,
    pub(crate) item: tack_core::models::Item,
}

/// A workspace/project/item plus one pending, dispatched `orch_task` for
/// `"task-1"` — the correlation target every ingestion fixture's mocked
/// docket response attributes an event, run or approval back to.
pub(crate) async fn seed_project_with_pending_task(repo: &Repository) -> ProjectFixture {
    let workspace_id = create_test_workspace(repo).await;
    let project = make_project(repo, workspace_id).await;
    let item = make_item(repo, &project).await;
    repo.upsert_orch_tasks(&[NewOrchTask {
        item_id: item.id,
        remote_task_id: "task-1".into(),
        remote_run_id: None,
        remote_status: "pending".into(),
        attempt: 1,
        tokens_in: 0,
        tokens_out: 0,
        cost_usd_estimated: None,
        dispatched_at: Utc::now(),
        trusted: true,
    }])
    .await
    .expect("seed orch task");
    ProjectFixture { project, item }
}

/// Starts the reconciler against `repo` through the same `TestRepoStore`
/// every ingestion test uses. Every fixture here registers exactly one
/// control plane, so a caller never needs to re-check `handles.len()`.
pub(crate) async fn spawn_reconciler(
    repo: &Repository,
    config: ReconcilerConfig,
) -> Vec<JoinHandle<()>> {
    let store: Arc<dyn ControlPlaneStore> = Arc::new(TestRepoStore { repo: repo.clone() });
    let handles = spawn_reconcilers(true, store, config).await;
    assert_eq!(handles.len(), 1, "exactly one control plane is registered");
    handles
}

pub(crate) async fn stop_reconciler(handles: Vec<JoinHandle<()>>) {
    for h in handles {
        h.abort();
    }
}

pub(crate) async fn last_seen_at(
    repo: &Repository,
    control_plane_id: Uuid,
) -> Option<DateTime<Utc>> {
    repo.get_control_plane(control_plane_id)
        .await
        .expect("get control plane")
        .last_seen_at
}

/// Polls `probe` every 25ms until it returns `Some`, returning that value;
/// panics with `description` if 5s pass first. Stands in for a fixed sleep
/// that guesses how long a background reconciler tick takes.
pub(crate) async fn poll_for<F, Fut, T>(description: &str, mut probe: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut ticker = tokio::time::interval(Duration::from_millis(25));
    loop {
        ticker.tick().await;
        if let Some(value) = probe().await {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "condition not met within 5s: {description}"
        );
    }
}

/// Same as [`poll_for`] for a plain boolean condition.
pub(crate) async fn poll_until<F, Fut>(description: &str, mut condition: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    poll_for(description, || {
        let outcome = condition();
        async move { outcome.await.then_some(()) }
    })
    .await;
}

async fn poll_for_tick_after(
    repo: &Repository,
    control_plane_id: Uuid,
    since: Option<DateTime<Utc>>,
) -> DateTime<Utc> {
    poll_for("a reconciler tick completed", || async {
        last_seen_at(repo, control_plane_id)
            .await
            .filter(|seen| since.is_none_or(|s| *seen > s))
    })
    .await
}

/// Waits for the reconciler to complete a full tick strictly after `since`,
/// including that tick's persist phase — not just its health update. The
/// loop writes `last_seen_at` before persisting runs/approvals/events, so
/// a *second* bump can only happen once the first tick's persist phase has
/// completed (the loop is one sequential task); waiting for two bumps is
/// what makes this race-free, unlike waiting for a single one.
pub(crate) async fn wait_for_tick_after(
    repo: &Repository,
    control_plane_id: Uuid,
    since: Option<DateTime<Utc>>,
) {
    let first = poll_for_tick_after(repo, control_plane_id, since).await;
    poll_for_tick_after(repo, control_plane_id, Some(first)).await;
}

/// Spawns a fresh reconciler, lets it complete one full tick, then stops
/// it — the shape every "re-poll and check nothing changed" test needs.
pub(crate) async fn run_one_more_tick(
    repo: &Repository,
    control_plane_id: Uuid,
    since: Option<DateTime<Utc>>,
    config: ReconcilerConfig,
) {
    let handles = spawn_reconciler(repo, config).await;
    wait_and_stop(repo, control_plane_id, since, handles).await;
}

/// Waits for an already-running reconciler to complete one more tick past
/// `since`, then stops it — for a test that keeps its first spawn running
/// rather than restarting it.
pub(crate) async fn wait_and_stop(
    repo: &Repository,
    control_plane_id: Uuid,
    since: Option<DateTime<Utc>>,
    handles: Vec<JoinHandle<()>>,
) {
    wait_for_tick_after(repo, control_plane_id, since).await;
    stop_reconciler(handles).await;
}

/// A one-second poll interval, fast enough for a test to observe several
/// ticks without a long wait; `retention_days` overrides the default when a
/// test needs events outside (or, deliberately, still inside) that window.
pub(crate) fn fast_poll_config(retention_days: u32) -> ReconcilerConfig {
    ReconcilerConfig {
        poll_secs: 1,
        event_retention_days: retention_days,
        ..Default::default()
    }
}

pub(crate) async fn plane_health(repo: &Repository, control_plane_id: Uuid) -> String {
    repo.get_control_plane(control_plane_id)
        .await
        .expect("get control plane")
        .health
}

pub(crate) async fn expect_run(repo: &Repository, remote_run_id: &str) -> OrchRun {
    repo.get_orch_run(remote_run_id)
        .await
        .expect("query run")
        .unwrap_or_else(|| panic!("run {remote_run_id} must be mirrored"))
}

pub(crate) async fn expect_approval(repo: &Repository, token: &str) -> OrchApproval {
    repo.get_orch_approval(token)
        .await
        .expect("query approval")
        .unwrap_or_else(|| panic!("approval {token} must be mirrored"))
}

pub(crate) async fn orch_event_count(repo: &Repository) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM orch_events")
        .fetch_one(repo.pool())
        .await
        .expect("count events")
}

pub(crate) async fn orch_run_count(repo: &Repository) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM orch_runs")
        .fetch_one(repo.pool())
        .await
        .expect("count runs")
}

pub(crate) async fn orch_approval_count(repo: &Repository) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM orch_approvals")
        .fetch_one(repo.pool())
        .await
        .expect("count approvals")
}
