use super::*;
use crate::{
    ApprovalState, Capabilities, DecisionSupport, EventScope, ModelSelection, NewRemoteTask, Rated,
    RemoteApproval, RemoteEvent, RemoteRun, RemoteTask, RunSource, RunState, Support, TaskStatus,
    UsageSupport,
};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// -- Health state machine (pure; no ControlPlane needed) --------------

#[test]
fn health_state_transitions_at_3_and_10_failures() {
    let mut t = HealthTracker::new();
    let now = Utc::now();

    // 1 and 2 failures: still healthy.
    assert_eq!(t.observe(false, false, now).state, HealthState::Healthy);
    assert_eq!(t.observe(false, false, now).state, HealthState::Healthy);
    // 3rd failure: degraded.
    let tr = t.observe(false, false, now);
    assert_eq!(tr.state, HealthState::Degraded);
    assert_eq!(tr.consecutive_failures, 3);

    // 4..=9 stay degraded.
    for _ in 4..=9 {
        assert_eq!(t.observe(false, false, now).state, HealthState::Degraded);
    }
    // 10th failure: unreachable.
    let tr = t.observe(false, false, now);
    assert_eq!(tr.state, HealthState::Unreachable);
    assert_eq!(tr.consecutive_failures, 10);
}

#[test]
fn health_recovers_immediately_on_a_single_success() {
    let mut t = HealthTracker::new();
    let now = Utc::now();
    for _ in 0..15 {
        t.observe(false, false, now);
    }
    assert_eq!(t.state, HealthState::Unreachable);

    let tr = t.observe(true, false, now);
    assert_eq!(tr.state, HealthState::Healthy);
    assert_eq!(tr.consecutive_failures, 0);
    assert_eq!(tr.last_seen_at, Some(now));
}

#[test]
fn last_seen_at_is_none_on_a_failed_poll_so_the_store_leaves_it_untouched() {
    let mut t = HealthTracker::new();
    let tr = t.observe(false, false, Utc::now());
    assert_eq!(tr.last_seen_at, None);
}

#[test]
fn warn_logging_is_suppressed_during_a_sustained_outage() {
    // This is the exact suppression logic spawn_one's loop uses to
    // decide whether to `tracing::warn!` — a real sustained outage
    // (docket down for an hour, say) must not spam the log at every
    // tick. Across a long failure streak, `warn` should only fire on
    // the two severity transitions (entering degraded, entering
    // unreachable), never on every one of the (here) 30 failed polls.
    let mut t = HealthTracker::new();
    let now = Utc::now();
    let mut warns = 0;
    for _ in 0..30 {
        if t.observe(false, false, now).log == Some(LogSeverity::Warn) {
            warns += 1;
        }
    }
    assert_eq!(
        warns, 2,
        "expected exactly 2 warns (healthy->degraded, degraded->unreachable) across a 30-failure streak"
    );

    // Recovery logs once at `info`, not `warn`.
    let tr = t.observe(true, false, now);
    assert_eq!(tr.log, Some(LogSeverity::Info));
}

#[test]
fn backoff_is_capped_at_five_minutes() {
    assert_eq!(backoff_secs(100, 10), MAX_BACKOFF_SECS);
    assert_eq!(backoff_secs(1_000_000, 10), MAX_BACKOFF_SECS);
}

#[test]
fn backoff_grows_with_consecutive_failures_and_resets_when_healthy() {
    let a = backoff_secs(1, 10);
    let b = backoff_secs(2, 10);
    let c = backoff_secs(3, 10);
    assert!(
        a < b && b < c,
        "backoff should strictly increase: {a} < {b} < {c}"
    );
    assert_eq!(backoff_secs(0, 10), 10, "no backoff while healthy");
}

#[test]
fn jitter_stays_within_20_percent_of_base() {
    let id = Uuid::new_v4();
    for tick in 0..200 {
        let j = jittered_secs(&id, tick, 100);
        assert!(
            (80..=120).contains(&j),
            "tick {tick} produced {j}, expected within [80, 120] for base 100"
        );
    }
}

#[test]
fn jitter_varies_so_planes_do_not_stampede_in_lockstep() {
    // Different plane ids polled on the same tick should not all land
    // on the exact same jittered interval.
    let base = 100;
    let values: std::collections::HashSet<u64> = (0..20)
        .map(|_| jittered_secs(&Uuid::new_v4(), 1, base))
        .collect();
    assert!(
        values.len() > 1,
        "expected jitter to differ across plane ids on the same tick, got a single value {values:?}"
    );
}

#[test]
fn jitter_never_produces_a_zero_or_negative_sleep() {
    let id = Uuid::new_v4();
    for tick in 0..50 {
        assert!(jittered_secs(&id, tick, 1) >= 1);
    }
}

// -- apiVersion policy --------------------------------------------------

fn sample_status(api_version: &str) -> FleetStatus {
    FleetStatus {
        api_version: api_version.to_string(),
        timestamp: "2026-08-04T00:00:00Z".to_string(),
        gateway: "active".to_string(),
        channels: vec![],
        agents: vec![],
        total_cost_usd_estimated: 0.0,
    }
}

#[test]
fn evaluate_is_reachable_and_matched_when_health_and_status_succeed() {
    let outcome = FetchOutcome {
        health: Ok(Health {
            status: "ok".into(),
            gateway: 1,
        }),
        status: Ok(sample_status(EXPECTED_API_VERSION)),
        runs: Vec::new(),
        approvals: Ok(Vec::new()),
        metrics: Ok(Vec::new()),
        traces: Vec::new(),
    };
    let eval = evaluate(&outcome);
    assert!(eval.reachable);
    assert!(!eval.version_mismatch);
    assert_eq!(
        eval.observed_api_version.as_deref(),
        Some(EXPECTED_API_VERSION)
    );
}

#[test]
fn evaluate_is_unreachable_when_health_call_fails() {
    let outcome = FetchOutcome {
        health: Err(OrchError::Unavailable("connection refused".into())),
        status: Ok(sample_status(EXPECTED_API_VERSION)),
        runs: Vec::new(),
        approvals: Ok(Vec::new()),
        metrics: Ok(Vec::new()),
        traces: Vec::new(),
    };
    assert!(!evaluate(&outcome).reachable);
}

#[test]
fn evaluate_is_unreachable_when_status_call_fails() {
    let outcome = FetchOutcome {
        health: Ok(Health {
            status: "ok".into(),
            gateway: 1,
        }),
        status: Err(OrchError::Decode("malformed json".into())),
        runs: Vec::new(),
        approvals: Ok(Vec::new()),
        metrics: Ok(Vec::new()),
        traces: Vec::new(),
    };
    let eval = evaluate(&outcome);
    assert!(!eval.reachable);
    assert!(
        !eval.version_mismatch,
        "an unparseable status has no apiVersion to compare"
    );
}

#[test]
fn evaluate_flags_a_major_api_version_mismatch_but_stays_reachable() {
    let outcome = FetchOutcome {
        health: Ok(Health {
            status: "ok".into(),
            gateway: 1,
        }),
        status: Ok(sample_status("3")),
        runs: Vec::new(),
        approvals: Ok(Vec::new()),
        metrics: Ok(Vec::new()),
        traces: Vec::new(),
    };
    let eval = evaluate(&outcome);
    assert!(eval.reachable, "the HTTP calls themselves succeeded");
    assert!(eval.version_mismatch);
    assert_eq!(eval.observed_api_version.as_deref(), Some("3"));
    assert!(eval.detail.contains("apiVersion mismatch"));
}

#[test]
fn evaluate_ignores_a_minor_version_difference() {
    // "2.1" vs expected "2" (or a future "2.0"): same major, not a
    // mismatch — see major_version's doc comment.
    let outcome = FetchOutcome {
        health: Ok(Health {
            status: "ok".into(),
            gateway: 1,
        }),
        status: Ok(sample_status("2.1")),
        runs: Vec::new(),
        approvals: Ok(Vec::new()),
        metrics: Ok(Vec::new()),
        traces: Vec::new(),
    };
    assert!(!evaluate(&outcome).version_mismatch);
}

#[test]
fn version_mismatch_forces_at_least_degraded_even_while_reachability_is_healthy() {
    let mut t = HealthTracker::new();
    let tr = t.observe(true, true, Utc::now());
    assert_eq!(tr.state, HealthState::Degraded);
    assert_eq!(tr.log, Some(LogSeverity::Warn));
}

#[test]
fn version_mismatch_does_not_downgrade_an_already_unreachable_plane() {
    let mut t = HealthTracker::new();
    let now = Utc::now();
    for _ in 0..10 {
        t.observe(false, false, now);
    }
    assert_eq!(t.state, HealthState::Unreachable);
    // Reachable again but with a version mismatch on this same tick:
    // reachability resets failures to 0 (healthy floor), but the
    // mismatch keeps it at degraded, not unreachable and not healthy.
    let tr = t.observe(true, true, now);
    assert_eq!(tr.state, HealthState::Degraded);
}

// -- Fake ControlPlane, for reconcile_once / spawn tests ---------------

/// `(events, next)` scripted for one `(project, since)` pair — `next` is
/// scripted explicitly (not derived) since the real cursor is opaque and
/// remote-minted.
type ScriptedTracesResponse = (Vec<RemoteEvent>, Option<String>);

/// A `ControlPlane` whose `health`/`status`/`list_runs`/`list_approvals`
/// responses are scripted; every other method (not needed by these
/// tests) returns `Disabled`. Drives `reconcile_once`/`spawn_reconcilers`
/// without a real adapter.
struct FakeControlPlane {
    health_calls: AtomicUsize,
    healthy: bool,
    panic_on_health: bool,
    runs: Vec<RemoteRun>,
    approvals: Vec<RemoteApproval>,
    approvals_should_fail: bool,
    metrics: Vec<MetricSample>,
    metrics_should_fail: bool,
    /// Keyed by the `project`/`since` pair `traces()` was called with —
    /// lets a single test script different responses per project without
    /// needing per-project fakes. Most tests don't care about `next` and
    /// leave it `None` via [`Self::with_traces`]; [`Self::with_traces_next`]
    /// is there for the ones that do.
    traces: std::collections::HashMap<(String, Option<String>), ScriptedTracesResponse>,
    traces_should_fail: bool,
}

impl FakeControlPlane {
    fn healthy() -> Self {
        Self {
            health_calls: AtomicUsize::new(0),
            healthy: true,
            panic_on_health: false,
            runs: Vec::new(),
            approvals: Vec::new(),
            approvals_should_fail: false,
            metrics: Vec::new(),
            metrics_should_fail: false,
            traces: std::collections::HashMap::new(),
            traces_should_fail: false,
        }
    }

    fn panics_on_health() -> Self {
        Self {
            panic_on_health: true,
            ..Self::healthy()
        }
    }

    fn with_runs(runs: Vec<RemoteRun>) -> Self {
        Self {
            runs,
            ..Self::healthy()
        }
    }

    fn with_approvals(approvals: Vec<RemoteApproval>) -> Self {
        Self {
            approvals,
            ..Self::healthy()
        }
    }

    fn healthy_with_failing_approvals() -> Self {
        Self {
            approvals_should_fail: true,
            ..Self::healthy()
        }
    }

    fn with_metrics(metrics: Vec<MetricSample>) -> Self {
        Self {
            metrics,
            ..Self::healthy()
        }
    }

    fn healthy_with_failing_metrics() -> Self {
        Self {
            metrics_should_fail: true,
            ..Self::healthy()
        }
    }

    /// `traces()` for `(project, since)` returns `events` with no `next`
    /// cursor (`None`) — for tests that only care about the events
    /// themselves. Call multiple times to script more than one
    /// project/cursor combination.
    fn with_traces(mut self, project: &str, since: Option<&str>, events: Vec<RemoteEvent>) -> Self {
        self.traces.insert(
            (project.to_string(), since.map(str::to_string)),
            (events, None),
        );
        self
    }

    /// Same as [`Self::with_traces`], but also scripts the exact `next`
    /// cursor the remote "minted" for this response — for tests that
    /// assert on the persisted cursor value (the opaque cursor is
    /// remote-minted and scripted, not computed by the fake).
    fn with_traces_next(
        mut self,
        project: &str,
        since: Option<&str>,
        events: Vec<RemoteEvent>,
        next: Option<&str>,
    ) -> Self {
        self.traces.insert(
            (project.to_string(), since.map(str::to_string)),
            (events, next.map(str::to_string)),
        );
        self
    }

    fn healthy_with_failing_traces() -> Self {
        Self {
            traces_should_fail: true,
            ..Self::healthy()
        }
    }
}

#[async_trait::async_trait]
impl ControlPlane for FakeControlPlane {
    fn kind(&self) -> &'static str {
        "fake"
    }

    /// Not exercised by any test in this module (every test here
    /// scripts `health`/`status`/`list_runs`/`list_approvals`/etc., not
    /// capability negotiation) — a plausible, internally-consistent
    /// value so the trait is satisfied, nothing more.
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
        self.health_calls.fetch_add(1, Ordering::SeqCst);
        if self.panic_on_health {
            panic!("simulated adapter bug");
        }
        if self.healthy {
            Ok(Health {
                status: "ok".into(),
                gateway: 1,
            })
        } else {
            Err(OrchError::Unavailable("connection refused".into()))
        }
    }

    async fn status(&self) -> Result<FleetStatus, OrchError> {
        Ok(sample_status(EXPECTED_API_VERSION))
    }

    async fn metrics(&self) -> Result<Vec<crate::MetricSample>, OrchError> {
        if self.metrics_should_fail {
            Err(OrchError::Unavailable("metrics endpoint down".into()))
        } else {
            Ok(self.metrics.clone())
        }
    }

    async fn list_runs(&self, _project: Option<&str>) -> Result<Vec<RemoteRun>, OrchError> {
        Ok(self.runs.clone())
    }

    async fn get_run(&self, _run_id: &str) -> Result<RemoteRun, OrchError> {
        Err(OrchError::Disabled)
    }

    async fn list_approvals(&self) -> Result<Vec<RemoteApproval>, OrchError> {
        if self.approvals_should_fail {
            Err(OrchError::Unavailable("approvals endpoint down".into()))
        } else {
            Ok(self.approvals.clone())
        }
    }

    async fn list_tasks(&self, _project: &str) -> Result<Vec<RemoteTask>, OrchError> {
        Err(OrchError::Disabled)
    }

    async fn traces(&self, project: &str, since: Option<&str>) -> Result<TracesPage, OrchError> {
        if self.traces_should_fail {
            return Err(OrchError::Unavailable("traces endpoint down".into()));
        }
        let key = (project.to_string(), since.map(str::to_string));
        let (events, next) = self.traces.get(&key).cloned().unwrap_or_default();
        Ok(TracesPage { events, next })
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
    ) -> Result<crate::ApprovalState, OrchError> {
        Err(OrchError::Disabled)
    }

    async fn provision_pod(
        &self,
        _params: crate::ProvisionPodParams,
    ) -> Result<crate::ProvisionedPod, OrchError> {
        Err(OrchError::Disabled)
    }
}

// Silence "unused" on ApprovalState/RunState/TaskStatus imports, which
// exist only so this module compiles against the exact same import set
// future poll_* fns will need; keep them imported here as a
// living example rather than trimming and re-adding later.
#[allow(dead_code)]
fn _uses_remote_enums(_a: ApprovalState, _b: RunState, _c: TaskStatus) {}

#[tokio::test]
async fn panic_in_a_poll_is_isolated_by_the_task_boundary() {
    let cp: Arc<dyn ControlPlane> = Arc::new(FakeControlPlane::panics_on_health());
    // This is exactly the isolation spawn_one relies on: reconcile_once
    // runs inside its own tokio::spawn, so a panic inside it surfaces as
    // a JoinError to the caller instead of unwinding the caller's stack.
    let result = tokio::spawn(async move { reconcile_once(&cp, &[], &HashMap::new()).await }).await;
    assert!(
        result.is_err(),
        "the panic must surface as a JoinError, not propagate"
    );
}

// -- Persistence-side fake, for spawn_reconcilers tests -----------------

struct FakeStore {
    planes: Vec<RegisteredPlane>,
    list_called: AtomicBool,
    health_records: Mutex<Vec<(Uuid, HealthRecord)>>,
    /// `orch_links.remote_project` stand-in — what `list_linked_projects`
    /// returns for every plane (this fake doesn't scope by
    /// `control_plane_id`; one plane per test is enough for these tests).
    linked_projects: Vec<String>,
    /// `orch_tasks.remote_task_id -> item_id` stand-in for
    /// `find_item_for_remote_task`.
    known_tasks: std::collections::HashMap<String, Uuid>,
    upserted_runs: Mutex<Vec<(Uuid, Vec<NewOrchRun>)>>,
    upserted_approvals: Mutex<Vec<(Uuid, Vec<NewOrchApproval>)>>,
    upserted_metrics: Mutex<Vec<(Uuid, Vec<NewOrchMetric>)>>,
    /// `orch_trace_cursors` stand-in, seeded via [`Self::with_trace_cursor`]
    /// and mutated by `set_trace_cursor` — a `Mutex` (not the read-only
    /// `linked_projects`/`known_tasks` shape) because the reconciler
    /// itself writes to it every tick.
    trace_cursors: Mutex<std::collections::HashMap<String, String>>,
    upserted_events: Mutex<Vec<(Uuid, Vec<NewOrchEvent>)>>,
}

impl FakeStore {
    fn new(planes: Vec<RegisteredPlane>) -> Self {
        Self {
            planes,
            list_called: AtomicBool::new(false),
            health_records: Mutex::new(Vec::new()),
            linked_projects: Vec::new(),
            known_tasks: std::collections::HashMap::new(),
            upserted_runs: Mutex::new(Vec::new()),
            upserted_approvals: Mutex::new(Vec::new()),
            upserted_metrics: Mutex::new(Vec::new()),
            trace_cursors: Mutex::new(std::collections::HashMap::new()),
            upserted_events: Mutex::new(Vec::new()),
        }
    }

    fn with_linked_projects(mut self, projects: Vec<String>) -> Self {
        self.linked_projects = projects;
        self
    }

    fn with_known_task(mut self, remote_task_id: &str, item_id: Uuid) -> Self {
        self.known_tasks.insert(remote_task_id.to_string(), item_id);
        self
    }

    fn with_trace_cursor(self, remote_project: &str, cursor: &str) -> Self {
        self.trace_cursors
            .lock()
            .unwrap()
            .insert(remote_project.to_string(), cursor.to_string());
        self
    }
}

#[async_trait::async_trait]
impl ControlPlaneStore for FakeStore {
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
        self.list_called.store(true, Ordering::SeqCst);
        Ok(self.planes.clone())
    }

    async fn record_health(
        &self,
        control_plane_id: Uuid,
        record: &HealthRecord,
    ) -> Result<(), OrchError> {
        self.health_records
            .lock()
            .unwrap()
            .push((control_plane_id, record.clone()));
        Ok(())
    }

    async fn list_linked_projects(
        &self,
        _control_plane_id: Uuid,
    ) -> Result<Vec<String>, OrchError> {
        Ok(self.linked_projects.clone())
    }

    async fn find_item_for_remote_task(
        &self,
        remote_task_id: &str,
    ) -> Result<Option<Uuid>, OrchError> {
        Ok(self.known_tasks.get(remote_task_id).copied())
    }

    async fn upsert_runs(
        &self,
        control_plane_id: Uuid,
        runs: &[NewOrchRun],
    ) -> Result<(), OrchError> {
        self.upserted_runs
            .lock()
            .unwrap()
            .push((control_plane_id, runs.to_vec()));
        Ok(())
    }

    async fn upsert_approvals(
        &self,
        control_plane_id: Uuid,
        approvals: &[NewOrchApproval],
    ) -> Result<(), OrchError> {
        self.upserted_approvals
            .lock()
            .unwrap()
            .push((control_plane_id, approvals.to_vec()));
        Ok(())
    }

    async fn upsert_metrics(
        &self,
        control_plane_id: Uuid,
        metrics: &[NewOrchMetric],
    ) -> Result<(), OrchError> {
        self.upserted_metrics
            .lock()
            .unwrap()
            .push((control_plane_id, metrics.to_vec()));
        Ok(())
    }

    async fn list_trace_cursors(
        &self,
        _control_plane_id: Uuid,
    ) -> Result<HashMap<String, String>, OrchError> {
        Ok(self.trace_cursors.lock().unwrap().clone())
    }

    async fn set_trace_cursor(
        &self,
        _control_plane_id: Uuid,
        remote_project: &str,
        cursor: &str,
    ) -> Result<(), OrchError> {
        self.trace_cursors
            .lock()
            .unwrap()
            .insert(remote_project.to_string(), cursor.to_string());
        Ok(())
    }

    async fn upsert_events(
        &self,
        control_plane_id: Uuid,
        events: &[NewOrchEvent],
    ) -> Result<(), OrchError> {
        self.upserted_events
            .lock()
            .unwrap()
            .push((control_plane_id, events.to_vec()));
        Ok(())
    }
}

fn healthy_plane(id: Uuid) -> RegisteredPlane {
    RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy()),
    }
}

#[tokio::test]
async fn disabled_orchestration_spawns_no_tasks_and_never_queries_the_store() {
    let store = Arc::new(FakeStore::new(vec![healthy_plane(Uuid::new_v4())]));
    let handles = spawn_reconcilers(false, store.clone(), ReconcilerConfig::default()).await;
    assert!(handles.is_empty());
    assert!(
        !store.list_called.load(Ordering::SeqCst),
        "a disabled reconciler must not even query for registered planes"
    );
}

#[tokio::test]
async fn enabled_orchestration_spawns_one_task_per_registered_plane() {
    let store = Arc::new(FakeStore::new(vec![
        healthy_plane(Uuid::new_v4()),
        healthy_plane(Uuid::new_v4()),
        healthy_plane(Uuid::new_v4()),
    ]));
    let handles = spawn_reconcilers(
        true,
        store.clone(),
        ReconcilerConfig {
            poll_secs: 1,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(handles.len(), 3);
    for h in handles {
        h.abort();
    }
}

// -- Supervised spawn: self-healing rather than a one-time snapshot —
//    see the module doc above `spawn_reconcilers_supervised`

/// A store whose registered-plane list can change after construction —
/// unlike `FakeStore`'s fixed `Vec`, this lets a test simulate a control
/// plane being registered or deleted *while the supervisor is already
/// running*.
struct MutableStore {
    planes: Mutex<Vec<RegisteredPlane>>,
}

impl MutableStore {
    fn new(planes: Vec<RegisteredPlane>) -> Self {
        Self {
            planes: Mutex::new(planes),
        }
    }

    fn register(&self, plane: RegisteredPlane) {
        self.planes.lock().unwrap().push(plane);
    }

    fn delete(&self, id: Uuid) {
        self.planes.lock().unwrap().retain(|p| p.id != id);
    }
}

#[async_trait::async_trait]
impl ControlPlaneStore for MutableStore {
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
        Ok(self.planes.lock().unwrap().clone())
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
        _runs: &[NewOrchRun],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn upsert_approvals(
        &self,
        _control_plane_id: Uuid,
        _approvals: &[NewOrchApproval],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn upsert_metrics(
        &self,
        _control_plane_id: Uuid,
        _metrics: &[NewOrchMetric],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn list_trace_cursors(
        &self,
        _control_plane_id: Uuid,
    ) -> Result<HashMap<String, String>, OrchError> {
        Ok(HashMap::new())
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
        _events: &[NewOrchEvent],
    ) -> Result<(), OrchError> {
        Ok(())
    }
}

/// A `poll_secs` long enough that, absent cancellation, a test relying
/// on it would time out rather than pass by accident, paired with a
/// `supervisor_scan_secs` fast enough to keep tests quick (the field's
/// unit is whole seconds — 1 is the floor `supervisor_loop` enforces).
fn fast_scan_config() -> ReconcilerConfig {
    ReconcilerConfig {
        poll_secs: 60,
        supervisor_scan_secs: 1,
        ..Default::default()
    }
}

/// Polls `f` every 20ms until it returns `true` or `deadline` passes,
/// panicking with `msg` in the latter case — a bounded wait instead of a
/// fixed sleep, so these tests are fast when the supervisor behaves and
/// don't hang forever when it doesn't.
async fn wait_until(deadline: Duration, msg: &str, mut f: impl FnMut() -> bool) {
    let start = tokio::time::Instant::now();
    loop {
        if f() {
            return;
        }
        assert!(tokio::time::Instant::now() - start < deadline, "{msg}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn supervised_spawn_starts_one_task_per_already_registered_plane() {
    let store = Arc::new(FakeStore::new(vec![
        healthy_plane(Uuid::new_v4()),
        healthy_plane(Uuid::new_v4()),
        healthy_plane(Uuid::new_v4()),
    ]));
    let (_stop_tx, stop_rx) = watch::channel(false);

    let reconciler = spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;

    // The initial reconcile_tick inside spawn_reconcilers_supervised is
    // synchronous, so all 3 are already up by the time `.await` above
    // resolves — no polling wait needed here, unlike the tests below.
    assert_eq!(reconciler.live_task_count().await, 3);
}

#[tokio::test]
async fn supervised_spawn_stops_every_task_after_the_global_stop_signal() {
    let id = Uuid::new_v4();
    let store = Arc::new(FakeStore::new(vec![healthy_plane(id)]));
    let (stop_tx, stop_rx) = watch::channel(false);

    let reconciler = spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;
    assert_eq!(reconciler.live_task_count().await, 1);

    stop_tx.send(true).expect("receiver still alive");

    wait_until(
        Duration::from_secs(3),
        "task should stop on its own within 3s of the global stop signal",
        || {
            // Synchronous best-effort check: `live_task_count` is async,
            // so poll it via try_lock instead of blocking this closure.
            reconciler
                .tasks
                .try_lock()
                .map(|g| g.values().all(|t| t.handle.is_finished()))
                .unwrap_or(false)
        },
    )
    .await;
}

#[tokio::test]
async fn supervised_spawn_already_stopped_never_starts_a_tick() {
    let id = Uuid::new_v4();
    let store = Arc::new(FakeStore::new(vec![healthy_plane(id)]));
    // Signalled *before* spawning — spawn_reconcilers_supervised's own
    // check must skip the initial reconcile_tick entirely, and the
    // supervisor loop's top-of-loop check must never let one through
    // either.
    let (stop_tx, stop_rx) = watch::channel(true);
    drop(stop_tx);

    let reconciler = spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;
    assert_eq!(
        reconciler.live_task_count().await,
        0,
        "a pre-stopped supervisor must never start a poller, even for an \
         already-registered plane"
    );
}

#[tokio::test]
async fn repeated_global_start_stop_cycles_leave_no_task_running() {
    let id = Uuid::new_v4();
    let store = Arc::new(FakeStore::new(vec![healthy_plane(id)]));

    for _ in 0..3 {
        let (stop_tx, stop_rx) = watch::channel(false);
        let reconciler =
            spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;
        assert_eq!(
            reconciler.live_task_count().await,
            1,
            "one task per cycle, never accumulating"
        );

        stop_tx.send(true).expect("receiver still alive");
        wait_until(
            Duration::from_secs(3),
            "each cycle's task must stop before the next starts",
            || {
                reconciler
                    .tasks
                    .try_lock()
                    .map(|g| g.values().all(|t| t.handle.is_finished()))
                    .unwrap_or(false)
            },
        )
        .await;
    }
}

/// **Guards against `list_registered()` being read only once at spawn
/// time.** (`tack-api`'s `orch_runtime.rs` has the same reproduction one
/// layer up, against `OrchRuntime` itself.) A plane registered after spawn
/// must still get polled. This
/// starts the supervisor with zero planes registered, registers one
/// after it's already running, and asserts it gets picked up — the
/// "enable -> register -> link" sequence the setup wizard walks
/// users through.
#[tokio::test]
async fn a_plane_registered_after_the_supervisor_starts_gets_polled() {
    let store = Arc::new(MutableStore::new(Vec::new()));
    let (_stop_tx, stop_rx) = watch::channel(false);

    let reconciler = spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;
    assert_eq!(
        reconciler.live_task_count().await,
        0,
        "nothing registered yet"
    );

    let plane_id = Uuid::new_v4();
    store.register(healthy_plane(plane_id));

    wait_until(
        Duration::from_secs(5),
        "a control plane registered after the supervisor started was never picked up",
        || {
            reconciler
                .tasks
                .try_lock()
                .map(|g| g.len() == 1)
                .unwrap_or(false)
        },
    )
    .await;
}

/// The other half of the diff: a plane removed from `control_planes`
/// (deleted through the API, or vanishing by any other means — this
/// fake doesn't distinguish) must have its poller stopped on the very
/// next scan, with no global stop signal involved at all.
#[tokio::test]
async fn a_deleted_plane_stops_being_polled_without_a_global_stop_signal() {
    let plane_id = Uuid::new_v4();
    let store = Arc::new(MutableStore::new(vec![healthy_plane(plane_id)]));
    let (_stop_tx, stop_rx) = watch::channel(false);

    let reconciler = spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;
    assert_eq!(reconciler.live_task_count().await, 1);

    store.delete(plane_id);

    wait_until(
        Duration::from_secs(5),
        "a plane deleted from the store kept being polled",
        || {
            reconciler
                .tasks
                .try_lock()
                .map(|g| g.is_empty())
                .unwrap_or(false)
        },
    )
    .await;
}

/// No leaked tasks across repeated register/delete churn — the map
/// converges back to empty every time, not just eventually.
#[tokio::test]
async fn repeated_register_delete_cycles_leak_no_tasks() {
    let store = Arc::new(MutableStore::new(Vec::new()));
    let (_stop_tx, stop_rx) = watch::channel(false);
    let reconciler = spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;

    for _ in 0..3 {
        let plane_id = Uuid::new_v4();
        store.register(healthy_plane(plane_id));
        wait_until(
            Duration::from_secs(5),
            "registered plane was never picked up during a churn cycle",
            || {
                reconciler
                    .tasks
                    .try_lock()
                    .map(|g| g.len() == 1)
                    .unwrap_or(false)
            },
        )
        .await;

        store.delete(plane_id);
        wait_until(
            Duration::from_secs(5),
            "deleted plane's poller was never stopped during a churn cycle",
            || {
                reconciler
                    .tasks
                    .try_lock()
                    .map(|g| g.is_empty())
                    .unwrap_or(false)
            },
        )
        .await;
    }
}

#[tokio::test]
async fn a_running_plane_task_persists_health_after_its_first_tick() {
    let id = Uuid::new_v4();
    let store = Arc::new(FakeStore::new(vec![healthy_plane(id)]));
    let handles = spawn_reconcilers(
        true,
        store.clone(),
        ReconcilerConfig {
            poll_secs: 1,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(handles.len(), 1);

    // The loop polls immediately on start (no artificial initial
    // delay — deliberately different from the due-soon/backup
    // schedulers), so a short real-time wait
    // is enough to observe the first persisted record.
    tokio::time::sleep(Duration::from_millis(300)).await;
    for h in handles {
        h.abort();
    }

    let records = store.health_records.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy)
    );
}

#[tokio::test]
async fn store_error_listing_planes_yields_no_tasks_not_a_panic() {
    struct FailingStore;
    #[async_trait::async_trait]
    impl ControlPlaneStore for FailingStore {
        async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
            Err(OrchError::Unavailable("db unreachable".into()))
        }
        async fn record_health(&self, _id: Uuid, _record: &HealthRecord) -> Result<(), OrchError> {
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
            _runs: &[NewOrchRun],
        ) -> Result<(), OrchError> {
            Ok(())
        }
        async fn upsert_approvals(
            &self,
            _control_plane_id: Uuid,
            _approvals: &[NewOrchApproval],
        ) -> Result<(), OrchError> {
            Ok(())
        }
        async fn upsert_metrics(
            &self,
            _control_plane_id: Uuid,
            _metrics: &[NewOrchMetric],
        ) -> Result<(), OrchError> {
            Ok(())
        }
        async fn list_trace_cursors(
            &self,
            _control_plane_id: Uuid,
        ) -> Result<HashMap<String, String>, OrchError> {
            Ok(HashMap::new())
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
            _events: &[NewOrchEvent],
        ) -> Result<(), OrchError> {
            Ok(())
        }
    }

    let handles =
        spawn_reconcilers(true, Arc::new(FailingStore), ReconcilerConfig::default()).await;
    assert!(handles.is_empty());
}

// -- Runs + approvals ingestion -----------------------------------------

fn sample_run(id: &str, project: &str, task_ids: Vec<String>) -> RemoteRun {
    RemoteRun {
        id: id.to_string(),
        source: RunSource::Cli,
        project: project.to_string(),
        state: RunState::Succeeded,
        task_ids,
        error: String::new(),
        created: "2026-08-04T19:50:43.129083+00:00".to_string(),
        started_at: Some("2026-08-04T19:50:43.129674+00:00".to_string()),
        finished_at: Some("2026-08-04T19:50:43.130194+00:00".to_string()),
        pids: Vec::new(),
        variables: serde_json::json!({}),
    }
}

fn sample_approval(token: &str, context: serde_json::Value) -> RemoteApproval {
    RemoteApproval {
        token: token.to_string(),
        project: "demo".to_string(),
        role: "implementer".to_string(),
        action: "pod dispatch — task enqueue".to_string(),
        state: ApprovalState::Pending,
        created: "2026-08-04T19:50:50Z".to_string(),
        context,
    }
}

/// Runs `spawn_one`'s loop (via `spawn_reconcilers`) for one tick against
/// a `FakeStore` (whose plane list is set via [`FakeStore::new`]), then
/// aborts the task and returns the store for assertions. Every test
/// below follows this same "one tick, then inspect what got upserted"
/// shape.
async fn run_one_tick(store: Arc<FakeStore>) -> Arc<FakeStore> {
    let store_dyn: Arc<dyn ControlPlaneStore> = store.clone();
    let handles = spawn_reconcilers(
        true,
        store_dyn,
        ReconcilerConfig {
            poll_secs: 60,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(handles.len(), 1, "expected exactly one plane registered");
    tokio::time::sleep(Duration::from_millis(300)).await;
    for h in handles {
        h.abort();
    }
    store
}

#[tokio::test]
async fn approvals_poll_failure_leaves_plane_health_untouched() {
    let id = Uuid::new_v4();
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy_with_failing_approvals()),
    };
    let store = Arc::new(FakeStore::new(vec![plane]));
    let store = run_one_tick(store).await;

    let records = store.health_records.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy),
        "a /approvals failure must not degrade plane health: {records:?}"
    );
}

#[tokio::test]
async fn a_correlated_run_lands_with_the_right_item_id() {
    let id = Uuid::new_v4();
    let item_id = Uuid::new_v4();
    let run = sample_run("run-1", "demo", vec!["task-1".to_string()]);
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::with_runs(vec![run])),
    };
    let store = Arc::new(
        FakeStore::new(vec![plane])
            .with_linked_projects(vec!["demo".to_string()])
            .with_known_task("task-1", item_id),
    );
    let store = run_one_tick(store).await;

    let upserted = store.upserted_runs.lock().unwrap();
    let (_, runs) = upserted
        .iter()
        .find(|(cp_id, runs)| *cp_id == id && !runs.is_empty())
        .expect("expected at least one upserted run");
    let run = runs.iter().find(|r| r.run_id == "run-1").expect("run-1");
    assert_eq!(run.item_id, Some(item_id));
    assert_eq!(run.remote_project, "demo");
    assert_eq!(run.source, "cli");
    assert_eq!(run.state, "succeeded");
}

#[tokio::test]
async fn an_uncorrelated_run_lands_with_item_id_none_and_does_not_error() {
    let id = Uuid::new_v4();
    // Empty task_ids: the normal shape of a run dispatched from
    // docket's own CLI, not through Tack. Must not error.
    let run = sample_run("run-cli-only", "demo", vec![]);
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::with_runs(vec![run])),
    };
    let store =
        Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
    let store = run_one_tick(store).await;

    let upserted = store.upserted_runs.lock().unwrap();
    let (_, runs) = upserted
        .iter()
        .find(|(cp_id, runs)| *cp_id == id && !runs.is_empty())
        .expect("expected the CLI-dispatched run to still be mirrored");
    let run = runs
        .iter()
        .find(|r| r.run_id == "run-cli-only")
        .expect("run-cli-only");
    assert_eq!(run.item_id, None);

    // And plane health must be entirely unaffected by this.
    let records = store.health_records.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy)
    );
}

#[tokio::test]
async fn a_correlated_approval_lands_with_the_right_item_id() {
    let id = Uuid::new_v4();
    let item_id = Uuid::new_v4();
    let approval = sample_approval(
        "apr-1",
        serde_json::json!({"taskId": "task-1", "pipelineIndex": 2}),
    );
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::with_approvals(vec![approval])),
    };
    let store = Arc::new(FakeStore::new(vec![plane]).with_known_task("task-1", item_id));
    let store = run_one_tick(store).await;

    let upserted = store.upserted_approvals.lock().unwrap();
    let (_, approvals) = upserted
        .iter()
        .find(|(cp_id, approvals)| *cp_id == id && !approvals.is_empty())
        .expect("expected at least one upserted approval");
    let approval = approvals
        .iter()
        .find(|a| a.token == "apr-1")
        .expect("apr-1");
    assert_eq!(approval.item_id, Some(item_id));
    assert_eq!(approval.remote_task_id.as_deref(), Some("task-1"));
    assert_eq!(approval.agent.as_deref(), Some("implementer"));
    assert_eq!(approval.state, "pending");
}

#[tokio::test]
async fn an_uncorrelated_approval_lands_with_item_id_none_and_still_surfaces() {
    let id = Uuid::new_v4();
    // No "taskId" in context at all — an approval Tack cannot attribute
    // to any item. This must still persist (item_id: NULL),
    // not be dropped, since it's exactly the kind of approval most
    // likely to silently block a fleet.
    let approval = sample_approval("apr-uncorrelated", serde_json::json!({}));
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::with_approvals(vec![approval])),
    };
    let store = Arc::new(FakeStore::new(vec![plane]));
    let store = run_one_tick(store).await;

    let upserted = store.upserted_approvals.lock().unwrap();
    let (_, approvals) = upserted
        .iter()
        .find(|(cp_id, approvals)| *cp_id == id && !approvals.is_empty())
        .expect("expected the uncorrelated approval to still be mirrored");
    let approval = approvals
        .iter()
        .find(|a| a.token == "apr-uncorrelated")
        .expect("apr-uncorrelated");
    assert_eq!(approval.item_id, None);
    assert_eq!(approval.remote_task_id, None);
}

#[test]
fn extract_task_id_handles_missing_and_non_string_taskid() {
    assert_eq!(
        extract_task_id(&serde_json::json!({"taskId": "task-1", "pipelineIndex": 0})),
        Some("task-1".to_string())
    );
    assert_eq!(extract_task_id(&serde_json::json!({})), None);
    assert_eq!(
        extract_task_id(&serde_json::json!({"taskId": 42})),
        None,
        "a non-string taskId is treated as uncorrelated, not a parse error"
    );
    assert_eq!(extract_task_id(&serde_json::json!(null)), None);
}

#[test]
fn parse_optional_rfc3339_accepts_both_docket_timestamp_conventions() {
    // core/runs.py's `+00:00` offset form.
    assert!(parse_optional_rfc3339(Some("2026-08-04T19:50:43.129083+00:00")).is_some());
    // core/approval.py's `Z` form.
    assert!(parse_optional_rfc3339(Some("2026-08-04T19:50:50Z")).is_some());
    // Malformed input degrades to None rather than panicking/erroring.
    assert_eq!(parse_optional_rfc3339(Some("not-a-timestamp")), None);
    assert_eq!(parse_optional_rfc3339(None), None);
}

// -- Metrics ingestion ----------------------------------------------------

fn sample_metric(name: &str, value: f64) -> MetricSample {
    let mut labels = std::collections::BTreeMap::new();
    labels.insert("agent".to_string(), "demo-lead".to_string());
    MetricSample {
        name: name.to_string(),
        labels,
        value,
    }
}

#[tokio::test]
async fn metrics_land_via_upsert_metrics_on_a_successful_poll() {
    let id = Uuid::new_v4();
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::with_metrics(vec![
            sample_metric("docket_agents_total", 3.0),
            sample_metric("docket_agent_cost_usd", 1.5),
        ])),
    };
    let store = Arc::new(FakeStore::new(vec![plane]));
    let store = run_one_tick(store).await;

    let upserted = store.upserted_metrics.lock().unwrap();
    let (_, metrics) = upserted
        .iter()
        .find(|(cp_id, metrics)| *cp_id == id && !metrics.is_empty())
        .expect("expected at least one upserted metric batch");
    assert_eq!(metrics.len(), 2);
    assert!(
        metrics
            .iter()
            .any(|m| m.name == "docket_agents_total" && m.value == 3.0)
    );
    assert!(
        metrics
            .iter()
            .any(|m| m.name == "docket_agent_cost_usd" && m.value == 1.5)
    );
}

#[tokio::test]
async fn metrics_poll_failure_leaves_plane_health_untouched_and_persists_nothing() {
    let id = Uuid::new_v4();
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy_with_failing_metrics()),
    };
    let store = Arc::new(FakeStore::new(vec![plane]));
    let store = run_one_tick(store).await;

    let records = store.health_records.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy),
        "a /metrics failure must not degrade plane health: {records:?}"
    );

    let upserted = store.upserted_metrics.lock().unwrap();
    assert!(
        upserted.iter().all(|(_, m)| m.is_empty()),
        "a failed metrics poll must not persist anything: {upserted:?}"
    );
}

// -- Trace ingestion --------------------------------------------------------

fn sample_event(session_id: &str, ts: &str, event_type: &str) -> RemoteEvent {
    RemoteEvent {
        ts: ts.to_string(),
        project: "demo".to_string(),
        session_id: session_id.to_string(),
        agent_role: "lead".to_string(),
        event_type: event_type.to_string(),
        payload: serde_json::json!({"tool": "bash", "command": "cargo test"}),
        cost_usd_estimated: Some(0.0021),
        duration_ms: Some(842),
    }
}

#[test]
fn derive_event_id_is_deterministic_for_the_same_source_event() {
    let cp_id = Uuid::new_v4();
    let event = sample_event("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call");
    let id1 = derive_event_id(cp_id, "demo", &event);
    let id2 = derive_event_id(cp_id, "demo", &event);
    assert_eq!(
        id1, id2,
        "the same source event must always derive the same id"
    );
}

#[test]
fn derive_event_id_differs_when_any_field_differs() {
    let cp_id = Uuid::new_v4();
    let base = sample_event("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call");
    let base_id = derive_event_id(cp_id, "demo", &base);

    let mut different_ts = base.clone();
    different_ts.ts = "2026-08-04T19:52:28Z".to_string();
    assert_ne!(base_id, derive_event_id(cp_id, "demo", &different_ts));

    let mut different_payload = base.clone();
    different_payload.payload = serde_json::json!({"tool": "bash", "command": "cargo build"});
    assert_ne!(base_id, derive_event_id(cp_id, "demo", &different_payload));

    assert_ne!(
        base_id,
        derive_event_id(cp_id, "other-project", &base),
        "the same event on a different remote_project must derive a different id"
    );
    assert_ne!(
        base_id,
        derive_event_id(Uuid::new_v4(), "demo", &base),
        "the same event on a different control plane must derive a different id"
    );
}

#[test]
fn derive_event_id_ignores_field_boundaries_not_field_content() {
    // Naive delimiter-free concatenation would hash "a" + "bc" the same
    // as "ab" + "c" — the \u{1} separator in derive_event_id must
    // prevent that. Two events differing only in where a boundary falls
    // between session_id and agent_role must derive different ids.
    let cp_id = Uuid::new_v4();
    let mut a = sample_event("agent:demo:ab", "2026-08-04T19:52:27Z", "tool_call");
    a.agent_role = "c".to_string();
    let mut b = sample_event("agent:demo:a", "2026-08-04T19:52:27Z", "tool_call");
    b.agent_role = "bc".to_string();
    assert_ne!(
        derive_event_id(cp_id, "demo", &a),
        derive_event_id(cp_id, "demo", &b)
    );
}

/// Pins `derive_event_id`'s output to a literal UUID — the determinism
/// tests just above this one only
/// prove the function returns the same id for the same input *within
/// one build*. They would not notice a changed field separator, a
/// reordered field in the `format!` (`:1058-1066`), or a changed
/// [`ORCH_EVENT_ID_NAMESPACE`] byte constant, because both the "before"
/// and "after" id in that comparison would move together and still
/// match each other.
///
/// That distinction matters because `ORCH_EVENT_ID_NAMESPACE`'s own doc
/// comment states the real stake: this id is `orch_events.id`, and
/// `upsert_orch_events`'s `ON CONFLICT(id) DO UPDATE` is what makes
/// re-ingesting an already-seen docket trace event a no-op. Change the
/// derivation and every event a deployment already ingested gets a
/// *different* id computed for it on the next poll after the upgrade —
/// not rejected as a duplicate, but inserted again as if new. Every
/// user's event timeline doubles its history and every cost rollup
/// built from `orch_events` counts the same spend twice, silently,
/// with a fully green test suite (see
/// `docs/plans/agnostic-control-plane.md` §6's regression table, third
/// row, for the exact refactor this catches).
///
/// If this test fails, the correct response is almost always to
/// **revert whatever changed `derive_event_id`'s output**, not to
/// update the literal below to match the new value — updating the
/// literal is only correct if every already-deployed instance's
/// `orch_events` table is being intentionally, knowingly re-keyed (a
/// decision far above what a code change should make silently).
#[test]
fn derive_event_id_matches_the_pinned_literal() {
    let control_plane_id =
        Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("valid fixed uuid");
    let event = RemoteEvent {
        ts: "2026-08-04T19:52:27Z".to_string(),
        project: "proj".to_string(),
        session_id: "agent:proj:task-1".to_string(),
        agent_role: "lead".to_string(),
        event_type: "tool_call".to_string(),
        payload: serde_json::json!({"tool": "bash", "command": "cargo test"}),
        cost_usd_estimated: Some(0.0021),
        duration_ms: Some(842),
    };
    assert_eq!(
        derive_event_id(control_plane_id, "proj", &event).to_string(),
        "4808170d-9797-561e-8fbb-dd8e9b94a9fe",
        "derive_event_id's output for this exact fixed input must never move — see this test's doc comment"
    );
}

#[test]
fn session_id_task_id_parses_the_agent_project_suffix_convention() {
    assert_eq!(
        session_id_task_id("agent:demo:task-90e465a8"),
        Some("task-90e465a8".to_string())
    );
    // docket also mints this convention for non-task sessions
    // (core/dispatch.py's bare dispatch session, core/pod.py's project
    // key) — parsing still succeeds, correlation against orch_tasks is
    // just expected to miss, which is not this function's concern.
    assert_eq!(
        session_id_task_id("agent:demo:dispatch"),
        Some("dispatch".to_string())
    );
    assert_eq!(session_id_task_id("not-the-agent-convention"), None);
    assert_eq!(session_id_task_id(""), None);
}

#[tokio::test]
async fn a_correlated_trace_event_lands_with_the_right_item_id() {
    let id = Uuid::new_v4();
    let item_id = Uuid::new_v4();
    let event = sample_event("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call");
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy().with_traces("demo", None, vec![event])),
    };
    let store = Arc::new(
        FakeStore::new(vec![plane])
            .with_linked_projects(vec!["demo".to_string()])
            .with_known_task("task-1", item_id),
    );
    let store = run_one_tick(store).await;

    let upserted = store.upserted_events.lock().unwrap();
    let (_, events) = upserted
        .iter()
        .find(|(cp_id, events)| *cp_id == id && !events.is_empty())
        .expect("expected at least one upserted event");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].item_id, Some(item_id));
    assert_eq!(events[0].event_type, "tool_call");
}

#[tokio::test]
async fn an_uncorrelated_trace_event_lands_with_item_id_none_and_does_not_error() {
    let id = Uuid::new_v4();
    // "dispatch" is docket's own non-task session suffix (see
    // session_id_task_id's doc) — never correlates to any orch_tasks
    // row, and must not be treated as an error.
    let event = sample_event(
        "agent:demo:dispatch",
        "2026-08-04T19:52:27Z",
        "session_start",
    );
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy().with_traces("demo", None, vec![event])),
    };
    let store =
        Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
    let store = run_one_tick(store).await;

    let upserted = store.upserted_events.lock().unwrap();
    let (_, events) = upserted
        .iter()
        .find(|(cp_id, events)| *cp_id == id && !events.is_empty())
        .expect("expected the uncorrelated event to still be mirrored");
    assert_eq!(events[0].item_id, None);

    let records = store.health_records.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy)
    );
}

#[tokio::test]
async fn an_unrecognised_event_type_is_stored_verbatim() {
    let id = Uuid::new_v4();
    let event = sample_event(
        "agent:demo:task-1",
        "2026-08-04T19:52:40Z",
        "some_future_event_type_v3",
    );
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy().with_traces("demo", None, vec![event])),
    };
    let store =
        Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
    let store = run_one_tick(store).await;

    let upserted = store.upserted_events.lock().unwrap();
    let (_, events) = upserted
        .iter()
        .find(|(cp_id, events)| *cp_id == id && !events.is_empty())
        .expect("expected the event to still be mirrored");
    assert_eq!(events[0].event_type, "some_future_event_type_v3");
}

#[tokio::test]
async fn traces_poll_failure_leaves_plane_health_untouched_and_persists_nothing() {
    let id = Uuid::new_v4();
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy_with_failing_traces()),
    };
    let store =
        Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
    let store = run_one_tick(store).await;

    let records = store.health_records.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy),
        "a /traces failure must not degrade plane health: {records:?}"
    );

    let upserted = store.upserted_events.lock().unwrap();
    assert!(
        upserted.iter().all(|(_, e)| e.is_empty()),
        "a failed traces poll must not persist anything: {upserted:?}"
    );
}

#[tokio::test]
async fn a_successful_traces_poll_advances_the_stored_cursor() {
    // The cursor is opaque and remote-minted — this fake
    // scripts docket's "minted" next value explicitly via
    // `with_traces_next` rather than computing one, and this test just
    // proves that value is what actually gets persisted, verbatim.
    let id = Uuid::new_v4();
    let events = vec![
        sample_event("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call"),
        sample_event("agent:demo:task-1", "2026-08-04T19:52:40Z", "session_start"),
    ];
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy().with_traces_next(
            "demo",
            None,
            events,
            Some("2026-08-04T19:52:40Z:1"),
        )),
    };
    let store =
        Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
    let store = run_one_tick(store).await;

    let cursors = store.trace_cursors.lock().unwrap();
    assert_eq!(
        cursors.get("demo").map(String::as_str),
        Some("2026-08-04T19:52:40Z:1")
    );
}

#[tokio::test]
async fn a_stored_cursor_is_used_as_since_on_the_next_poll() {
    let id = Uuid::new_v4();
    // FakeControlPlane's traces() is keyed by the exact (project, since)
    // pair it was called with — seeding it *only* for
    // since = Some("2026-08-04T19:52:27Z:1") proves poll_traces reads
    // the stored cursor and sends it, rather than always polling with
    // since = None.
    let event = sample_event("agent:demo:task-1", "2026-08-04T19:52:40Z", "tool_result");
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy().with_traces(
            "demo",
            Some("2026-08-04T19:52:27Z:1"),
            vec![event],
        )),
    };
    let store = Arc::new(
        FakeStore::new(vec![plane])
            .with_linked_projects(vec!["demo".to_string()])
            .with_trace_cursor("demo", "2026-08-04T19:52:27Z:1"),
    );
    let store = run_one_tick(store).await;

    let upserted = store.upserted_events.lock().unwrap();
    let (_, events) = upserted
        .iter()
        .find(|(cp_id, events)| *cp_id == id && !events.is_empty())
        .expect(
            "expected an event — only present if poll_traces actually sent \
             the stored cursor as `since`",
        );
    assert_eq!(events[0].event_type, "tool_result");
}

#[tokio::test]
async fn an_event_older_than_the_retention_cutoff_is_not_persisted() {
    let id = Uuid::new_v4();
    // Well outside even a 1-day retention window — simulates a rewound
    // cursor re-delivering an event that was already rolled up and
    // purged by the retention sweep (see the module doc's "Trace
    // cursor" / retention-composition section).
    let stale_event = sample_event("agent:demo:task-1", "2020-01-01T00:00:00Z", "tool_call");
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy().with_traces(
            "demo",
            None,
            vec![stale_event],
        )),
    };
    let store =
        Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));

    let store_dyn: Arc<dyn ControlPlaneStore> = store.clone();
    let handles = spawn_reconcilers(
        true,
        store_dyn,
        ReconcilerConfig {
            poll_secs: 60,
            event_retention_days: 1,
            ..Default::default()
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    for h in handles {
        h.abort();
    }

    let upserted = store.upserted_events.lock().unwrap();
    assert!(
        upserted.iter().all(|(_, e)| e.is_empty()),
        "an event older than the retention cutoff must never be (re-)inserted: {upserted:?}"
    );
}

// -- Retention sweep -------------------------------------------------------

/// A [`RetentionStore`] whose two rollup methods are scripted and every
/// call recorded, for driving [`spawn_retention_sweep`] without a real
/// database.
struct FakeRetentionStore {
    events_outcome: RollupOutcome,
    metrics_outcome: RollupOutcome,
    events_calls: Mutex<Vec<DateTime<Utc>>>,
    metrics_calls: Mutex<Vec<DateTime<Utc>>>,
}

impl FakeRetentionStore {
    fn new() -> Self {
        Self {
            events_outcome: RollupOutcome::default(),
            metrics_outcome: RollupOutcome::default(),
            events_calls: Mutex::new(Vec::new()),
            metrics_calls: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl RetentionStore for FakeRetentionStore {
    async fn rollup_and_purge_events(
        &self,
        cutoff: DateTime<Utc>,
        _batch_size: i64,
    ) -> Result<RollupOutcome, OrchError> {
        self.events_calls.lock().unwrap().push(cutoff);
        Ok(self.events_outcome)
    }

    async fn rollup_and_purge_metrics(
        &self,
        cutoff: DateTime<Utc>,
        _batch_size: i64,
    ) -> Result<RollupOutcome, OrchError> {
        self.metrics_calls.lock().unwrap().push(cutoff);
        Ok(self.metrics_outcome)
    }
}

#[tokio::test]
async fn disabled_retention_sweep_spawns_nothing_and_never_calls_the_store() {
    let store = Arc::new(FakeRetentionStore::new());
    let handle = spawn_retention_sweep(false, store.clone(), 90, 1);
    assert!(handle.is_none());

    // Give a would-be sweep a chance to run, if it wrongly had.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(store.events_calls.lock().unwrap().is_empty());
    assert!(store.metrics_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn enabled_retention_sweep_calls_both_rollups_with_a_cutoff_derived_from_retention_days() {
    let store = Arc::new(FakeRetentionStore::new());
    let retention_days = 90u32;
    let before_spawn = Utc::now();
    let handle = spawn_retention_sweep(true, store.clone(), retention_days, 1);
    assert!(handle.is_some());

    tokio::time::sleep(Duration::from_millis(200)).await;
    handle.unwrap().abort();

    let events_calls = store.events_calls.lock().unwrap();
    let metrics_calls = store.metrics_calls.lock().unwrap();
    assert!(
        !events_calls.is_empty(),
        "expected at least one events rollup call"
    );
    assert!(
        !metrics_calls.is_empty(),
        "expected at least one metrics rollup call"
    );

    // The cutoff passed must be ~ (now - retention_days), not some other
    // arbitrary value — bounds-check against the window this test ran in.
    let expected_floor = before_spawn - chrono::Duration::days(retention_days as i64 + 1);
    let expected_ceiling = Utc::now() - chrono::Duration::days(retention_days as i64 - 1);
    for cutoff in events_calls.iter().chain(metrics_calls.iter()) {
        assert!(
            *cutoff > expected_floor && *cutoff < expected_ceiling,
            "cutoff {cutoff} not within the expected ~{retention_days}-day-ago window"
        );
    }
}

#[tokio::test]
async fn a_rollup_failure_is_logged_and_does_not_stop_the_sweep_ticker() {
    struct AlwaysFailingRetentionStore {
        calls: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl RetentionStore for AlwaysFailingRetentionStore {
        async fn rollup_and_purge_events(
            &self,
            _cutoff: DateTime<Utc>,
            _batch_size: i64,
        ) -> Result<RollupOutcome, OrchError> {
            *self.calls.lock().unwrap() += 1;
            Err(OrchError::Unavailable("db unreachable".into()))
        }

        async fn rollup_and_purge_metrics(
            &self,
            _cutoff: DateTime<Utc>,
            _batch_size: i64,
        ) -> Result<RollupOutcome, OrchError> {
            Err(OrchError::Unavailable("db unreachable".into()))
        }
    }

    let store = Arc::new(AlwaysFailingRetentionStore {
        calls: Mutex::new(0),
    });
    // A short sweep interval so several ticks happen inside the test's wait.
    let handle = spawn_retention_sweep(true, store.clone(), 90, 1).unwrap();
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    handle.abort();

    assert!(
        *store.calls.lock().unwrap() >= 2,
        "the ticker must keep retrying after a failed sweep, not stop"
    );
}
