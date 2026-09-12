use super::*;
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tack_orch::reconciler::{ControlPlaneStore, HealthRecord, RegisteredPlane};
use tack_orch::{
    Capabilities, ControlPlane, DecisionSupport, EventScope, FleetStatus, Health, MetricSample,
    ModelSelection, NewRemoteTask, OrchError, Rated, RemoteApproval, RemoteRun, RemoteTask,
    Support, TracesPage, UsageSupport,
};
use uuid::Uuid;

/// Reachable-but-uninteresting fake control plane — enough to keep one
/// reconciler tick from erroring loudly; the tests below only care
/// about task lifecycle, not what a tick actually observes.
struct QuietControlPlane;

#[async_trait]
impl ControlPlane for QuietControlPlane {
    fn kind(&self) -> &'static str {
        "fake"
    }
    /// Not exercised by anything in this module — these tests only
    /// care about task lifecycle (see this struct's own doc comment) —
    /// so any internally-consistent value satisfies the trait.
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            dispatch: true,
            cancel: false,
            pause: Rated::new(Support::Unsupported, "fake plane: no pause mechanism"),
            resume: Rated::new(Support::Unsupported, "fake plane: no resume mechanism"),
            event_scope: Rated::new(EventScope::Project, "fake plane: scripted per project"),
            artifacts: false,
            decisions: Rated::new(DecisionSupport::Poll, "fake plane: scripted approvals"),
            usage: Rated::new(UsageSupport::NotMeasured, "fake plane: no usage source"),
            model_selection: Rated::new(
                ModelSelection::Unsupported,
                "fake plane: no model routing",
            ),
            runtimes: false,
            plane_metrics: true,
            provisioning: false,
        }
    }
    async fn health(&self) -> Result<Health, OrchError> {
        Ok(Health {
            status: "ok".to_string(),
            gateway: 1,
        })
    }
    async fn status(&self) -> Result<FleetStatus, OrchError> {
        Ok(FleetStatus {
            api_version: "1".to_string(),
            timestamp: "2026-08-05T00:00:00Z".to_string(),
            gateway: "active".to_string(),
            channels: Vec::new(),
            agents: Vec::new(),
            total_cost_usd_estimated: 0.0,
        })
    }
    async fn metrics(&self) -> Result<Vec<MetricSample>, OrchError> {
        Ok(Vec::new())
    }
    async fn list_runs(&self, _project: Option<&str>) -> Result<Vec<RemoteRun>, OrchError> {
        Ok(Vec::new())
    }
    async fn get_run(&self, _run_id: &str) -> Result<RemoteRun, OrchError> {
        Err(OrchError::Disabled)
    }
    async fn list_approvals(&self) -> Result<Vec<RemoteApproval>, OrchError> {
        Ok(Vec::new())
    }
    async fn list_tasks(&self, _project: &str) -> Result<Vec<RemoteTask>, OrchError> {
        Err(OrchError::Disabled)
    }
    async fn traces(&self, _project: &str, _since: Option<&str>) -> Result<TracesPage, OrchError> {
        Ok(TracesPage::default())
    }
    async fn enqueue_task(
        &self,
        _project: &str,
        _task: NewRemoteTask,
    ) -> Result<String, OrchError> {
        Err(OrchError::Disabled)
    }
    async fn dispatch(
        &self,
        _project: &str,
        _vars: serde_json::Value,
    ) -> Result<String, OrchError> {
        Err(OrchError::Disabled)
    }
    async fn decide_approval(
        &self,
        _token: &str,
        _grant: bool,
    ) -> Result<tack_orch::ApprovalState, OrchError> {
        Err(OrchError::Disabled)
    }
    async fn provision_pod(
        &self,
        _params: tack_orch::ProvisionPodParams,
    ) -> Result<tack_orch::ProvisionedPod, OrchError> {
        Err(OrchError::Disabled)
    }
}

struct OneRunFakeStore {
    plane_id: Uuid,
    list_calls: AtomicUsize,
}

#[async_trait]
impl ControlPlaneStore for OneRunFakeStore {
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
        self.list_calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![RegisteredPlane {
            id: self.plane_id,
            control_plane: Arc::new(QuietControlPlane),
        }])
    }
    async fn record_health(
        &self,
        _control_plane_id: Uuid,
        _record: &HealthRecord,
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn list_linked_projects(
        &self,
        _control_plane_id: Uuid,
    ) -> Result<Vec<String>, OrchError> {
        Ok(Vec::new())
    }
    async fn find_item_for_remote_task(
        &self,
        _remote_task_id: &str,
    ) -> Result<Option<Uuid>, OrchError> {
        Ok(None)
    }
    async fn upsert_runs(
        &self,
        _control_plane_id: Uuid,
        _runs: &[tack_db::repo::orch::NewOrchRun],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn upsert_approvals(
        &self,
        _control_plane_id: Uuid,
        _approvals: &[tack_db::repo::orch::NewOrchApproval],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn upsert_metrics(
        &self,
        _control_plane_id: Uuid,
        _metrics: &[tack_db::repo::orch::NewOrchMetric],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn list_trace_cursors(
        &self,
        _control_plane_id: Uuid,
    ) -> Result<std::collections::HashMap<String, String>, OrchError> {
        Ok(std::collections::HashMap::new())
    }
    async fn set_trace_cursor(
        &self,
        _control_plane_id: Uuid,
        _remote_project: &str,
        _cursor: &str,
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn upsert_events(
        &self,
        _control_plane_id: Uuid,
        _events: &[tack_db::repo::orch::NewOrchEvent],
    ) -> Result<(), OrchError> {
        Ok(())
    }
}

fn fast_config() -> ReconcilerConfig {
    ReconcilerConfig {
        poll_secs: 60,
        ..Default::default()
    }
}

/// A store whose registered-plane list can grow after construction —
/// unlike `OneRunFakeStore`'s fixed list, this lets a test simulate a
/// control plane being registered *after* `OrchRuntime::start()` has
/// already been called, the exact sequence the setup wizard walks users
/// through (enable orchestration -> register a control plane -> link a
/// project). See `a_plane_registered_after_start_gets_polled` below,
/// which reproduces exactly that case.
struct MutableFakeStore {
    planes: std::sync::Mutex<Vec<Uuid>>,
}

impl MutableFakeStore {
    fn empty() -> Self {
        Self {
            planes: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn register(&self, id: Uuid) {
        self.planes.lock().unwrap().push(id);
    }
}

#[async_trait]
impl ControlPlaneStore for MutableFakeStore {
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
        Ok(self
            .planes
            .lock()
            .unwrap()
            .iter()
            .map(|id| RegisteredPlane {
                id: *id,
                control_plane: Arc::new(QuietControlPlane),
            })
            .collect())
    }
    async fn record_health(
        &self,
        _control_plane_id: Uuid,
        _record: &HealthRecord,
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn list_linked_projects(
        &self,
        _control_plane_id: Uuid,
    ) -> Result<Vec<String>, OrchError> {
        Ok(Vec::new())
    }
    async fn find_item_for_remote_task(
        &self,
        _remote_task_id: &str,
    ) -> Result<Option<Uuid>, OrchError> {
        Ok(None)
    }
    async fn upsert_runs(
        &self,
        _control_plane_id: Uuid,
        _runs: &[tack_db::repo::orch::NewOrchRun],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn upsert_approvals(
        &self,
        _control_plane_id: Uuid,
        _approvals: &[tack_db::repo::orch::NewOrchApproval],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn upsert_metrics(
        &self,
        _control_plane_id: Uuid,
        _metrics: &[tack_db::repo::orch::NewOrchMetric],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn list_trace_cursors(
        &self,
        _control_plane_id: Uuid,
    ) -> Result<std::collections::HashMap<String, String>, OrchError> {
        Ok(std::collections::HashMap::new())
    }
    async fn set_trace_cursor(
        &self,
        _control_plane_id: Uuid,
        _remote_project: &str,
        _cursor: &str,
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn upsert_events(
        &self,
        _control_plane_id: Uuid,
        _events: &[tack_db::repo::orch::NewOrchEvent],
    ) -> Result<(), OrchError> {
        Ok(())
    }
}

/// **The bug this test guards against.** `OrchRuntime::start` must
/// never regress to something like `spawn_reconcilers_cancellable`,
/// which reads `store.list_registered()` exactly once and spawns one
/// task per plane found at that instant, never re-reading the list. A
/// control plane registered after `start()` (enable -> register -> link,
/// the natural setup order a guided wizard walks users through) would
/// then never be polled: no task, no health updates, no error
/// anywhere. This test enables orchestration with zero planes
/// registered, registers one *after* `start()` has already returned,
/// and asserts it gets picked up and polled without a restart or a
/// second `start()` call.
#[tokio::test]
async fn a_plane_registered_after_start_gets_polled() {
    let fake = Arc::new(MutableFakeStore::empty());
    let store: Arc<dyn ControlPlaneStore> = fake.clone();
    let runtime = OrchRuntime::new();

    runtime.start(store.clone(), fast_config()).await;
    assert_eq!(runtime.live_task_count().await, 0, "nothing registered yet");

    let plane_id = Uuid::new_v4();
    fake.register(plane_id);

    // Bounded poll rather than a fixed sleep: fast when the fix works,
    // and doesn't hang forever if it doesn't.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if runtime.live_task_count().await == 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "a control plane registered after start() was never picked up for polling"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    runtime.stop().await;
}

#[tokio::test]
async fn start_then_stop_leaves_no_live_task() {
    let store: Arc<dyn ControlPlaneStore> = Arc::new(OneRunFakeStore {
        plane_id: Uuid::new_v4(),
        list_calls: AtomicUsize::new(0),
    });
    let runtime = OrchRuntime::new();

    runtime.start(store.clone(), fast_config()).await;
    // Give the spawned task a moment to reach its first sleep/select.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(runtime.live_task_count().await, 1);

    runtime.stop().await;
    // Cooperative shutdown observes the signal via select! promptly.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        runtime.live_task_count().await,
        0,
        "task must have exited after stop()"
    );
}

#[tokio::test]
async fn start_is_idempotent_while_already_running() {
    let store: Arc<dyn ControlPlaneStore> = Arc::new(OneRunFakeStore {
        plane_id: Uuid::new_v4(),
        list_calls: AtomicUsize::new(0),
    });
    let runtime = OrchRuntime::new();

    runtime.start(store.clone(), fast_config()).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(runtime.live_task_count().await, 1);

    // A second start() while running must not spawn a duplicate task.
    runtime.start(store.clone(), fast_config()).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        runtime.live_task_count().await,
        1,
        "a second start() must be a no-op, not a second task"
    );

    runtime.stop().await;
}

#[tokio::test]
async fn stop_without_a_prior_start_is_a_harmless_no_op() {
    let runtime = OrchRuntime::new();
    runtime.stop().await; // must not panic
    assert_eq!(runtime.live_task_count().await, 0);
}

#[tokio::test]
async fn repeated_toggles_never_leave_more_than_one_task_alive() {
    let store: Arc<dyn ControlPlaneStore> = Arc::new(OneRunFakeStore {
        plane_id: Uuid::new_v4(),
        list_calls: AtomicUsize::new(0),
    });
    let runtime = OrchRuntime::new();

    for _ in 0..3 {
        runtime.start(store.clone(), fast_config()).await;
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(runtime.live_task_count().await, 1);

        runtime.stop().await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(runtime.live_task_count().await, 0);
    }
}
