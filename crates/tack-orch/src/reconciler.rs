//! The orchestration reconciler: one background `tokio` task per registered
//! control plane, polling it on an interval and driving the
//! `healthy` → `degraded` → `unreachable` state machine.
//!
//! # The three-phase shape, and why it is not just discipline
//!
//!
//! Design notes: docs/dev-notes/tack-orch/reconciler.md

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tack_db::repo::orch::{NewOrchApproval, NewOrchEvent, NewOrchMetric, NewOrchRun};
use tokio::sync::{Mutex as AsyncMutex, watch};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    ControlPlane, FleetStatus, Health, MetricSample, OrchError, RemoteApproval, RemoteEvent,
    RemoteRun, TracesPage,
};

// ---------------------------------------------------------------------------
// Health state machine
// ---------------------------------------------------------------------------

/// Consecutive `/health` + `/status.json` failures before a plane is shown
/// `degraded`. Recovery is immediate on a single success — see
/// [`HealthTracker::observe`].
pub const DEGRADED_AFTER_FAILURES: i64 = 3;

/// Consecutive failures before a plane is shown `unreachable`.
pub const UNREACHABLE_AFTER_FAILURES: i64 = 10;

/// Poll backoff never waits longer than this, regardless of how long a plane
/// has been failing.
pub const MAX_BACKOFF_SECS: u64 = 300;

/// docket's `SERVE_API_VERSION` as verified by W0-A against `serve.py`
/// Compared against [`FleetStatus::api_version`]
/// on every poll — see [`evaluate`] and the module doc's apiVersion policy.
pub const EXPECTED_API_VERSION: &str = "2";

/// A control plane's health as the reconciler sees it. Column values in
/// `control_planes.health` are these variants' [`HealthState::as_str`]
/// output verbatim (`"healthy"` / `"degraded"` / `"unreachable"`) — the
/// column also allows `"unknown"` as its pre-first-poll default, which this
/// enum deliberately has no variant for: nothing in this module ever writes
/// `"unknown"`, only a fresh row's DEFAULT does.
///
/// Variant order is significant: `derive(PartialOrd, Ord)` ranks
/// `Healthy < Degraded < Unreachable`, which [`evaluate`] and
/// [`HealthTracker::observe`] rely on to combine two independent signals
/// (reachability and apiVersion match) by taking the more severe one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HealthState {
    Healthy,
    Degraded,
    Unreachable,
}

impl HealthState {
    pub fn as_str(&self) -> &'static str {
        match self {
            HealthState::Healthy => "healthy",
            HealthState::Degraded => "degraded",
            HealthState::Unreachable => "unreachable",
        }
    }
}

impl std::fmt::Display for HealthState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Severity to log a health transition at. Anything that doesn't change
/// state (a poll that fails for the 7th time in a row while already
/// `unreachable`, say) logs at `debug`, not `warn` — this is what makes the
/// "logs backoff at warn without spam" acceptance criterion true: warn-level
/// logging only fires *on a transition*, so a sustained outage produces at
/// most two warns (entering `degraded`, entering `unreachable`) no matter
/// how long it lasts, plus one `info` on recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogSeverity {
    Warn,
    Info,
}

/// What [`HealthTracker::observe`] decided this tick, ready to persist and log.
#[derive(Debug, Clone)]
struct HealthTransition {
    state: HealthState,
    consecutive_failures: i64,
    /// `Some(now)` on a successful poll; `None` on a failed one — mirrors
    /// `update_control_plane_health`'s contract exactly: `None` means
    /// "leave the stored `last_seen_at` untouched", not "clear it".
    last_seen_at: Option<DateTime<Utc>>,
    log: Option<LogSeverity>,
}

/// In-memory per-plane state driving the health state machine. One instance
/// lives for the lifetime of a plane's `tokio` task ([`spawn_one`]) — it is
/// not itself persisted; [`HealthTransition`]/[`HealthRecord`] are the
/// persisted projection of it after each tick.
#[derive(Debug, Clone)]
struct HealthTracker {
    consecutive_failures: i64,
    state: HealthState,
    version_mismatch: bool,
}

impl HealthTracker {
    fn new() -> Self {
        Self {
            consecutive_failures: 0,
            state: HealthState::Healthy,
            version_mismatch: false,
        }
    }

    /// Feed one poll's outcome into the state machine.
    ///
    /// `reachable` is true iff both required Wave-1 calls (`/health`,
    /// `/status.json`) succeeded. `version_mismatch` is an independent
    /// signal (see [`evaluate`]): a plane can be fully reachable and still
    /// show `degraded` because it's running a docket version this Tack
    /// doesn't understand. Recovery is immediate — a single `reachable`
    /// poll resets `consecutive_failures` to zero and re-evaluates state
    /// from scratch, regardless of how deep the prior outage was.
    fn observe(
        &mut self,
        reachable: bool,
        version_mismatch: bool,
        now: DateTime<Utc>,
    ) -> HealthTransition {
        let prev_state = self.state;
        let prev_version_mismatch = self.version_mismatch;

        self.consecutive_failures = if reachable {
            0
        } else {
            self.consecutive_failures.saturating_add(1)
        };

        let reachability_state = if self.consecutive_failures < DEGRADED_AFTER_FAILURES {
            HealthState::Healthy
        } else if self.consecutive_failures < UNREACHABLE_AFTER_FAILURES {
            HealthState::Degraded
        } else {
            HealthState::Unreachable
        };

        // A version mismatch never reports healthy, but never overrides a
        // worse reachability-driven state either — take the more severe of
        // the two independent signals.
        let version_floor = if version_mismatch {
            HealthState::Degraded
        } else {
            HealthState::Healthy
        };
        let new_state = reachability_state.max(version_floor);

        self.state = new_state;
        self.version_mismatch = version_mismatch;

        let last_seen_at = reachable.then_some(now);

        let log = if new_state != prev_state {
            Some(if new_state > prev_state {
                LogSeverity::Warn
            } else {
                LogSeverity::Info
            })
        } else if version_mismatch && !prev_version_mismatch {
            Some(LogSeverity::Warn)
        } else {
            None
        };

        HealthTransition {
            state: new_state,
            consecutive_failures: self.consecutive_failures,
            last_seen_at,
            log,
        }
    }
}

/// Poll interval grows exponentially with consecutive failures (doubling
/// each time), capped at [`MAX_BACKOFF_SECS`]. `consecutive_failures <= 0`
/// (healthy) returns `base_secs` unchanged — backoff only kicks in once a
/// poll has actually failed.
pub fn backoff_secs(consecutive_failures: i64, base_secs: u64) -> u64 {
    let base = base_secs.max(1);
    if consecutive_failures <= 0 {
        return base;
    }
    // Cap the exponent well below 64 so the shift can't overflow; the
    // result saturates to MAX_BACKOFF_SECS long before this matters.
    let exp = consecutive_failures.min(20) as u32;
    let multiplier = 1u64.checked_shl(exp).unwrap_or(u64::MAX);
    base.saturating_mul(multiplier).min(MAX_BACKOFF_SECS)
}

/// Deterministic ±20% jitter, so N planes sharing the same `TACK_ORCH_POLL_SECS`
/// don't all wake up on the same tick and stampede the gateway. Derived from
/// a hash of `(plane_id, tick)` via `DefaultHasher` — `rand` is not a
/// workspace dependency. Deterministic on purpose: it keeps scheduling
/// reproducible in tests and doesn't need a seeded RNG threaded through.
fn jittered_secs(plane_id: &Uuid, tick: u64, base_secs: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    plane_id.hash(&mut hasher);
    tick.hash(&mut hasher);
    let h = hasher.finish();

    // Map the hash to a jitter fraction in [-0.20, 0.20].
    let bucket = (h % 4001) as i64 - 2000; // -2000..=2000
    let jitter_frac = bucket as f64 / 10000.0; // -0.20..=0.20

    let base = base_secs.max(1) as f64;
    let jittered = base * (1.0 + jitter_frac);
    jittered.round().max(1.0) as u64
}

// ---------------------------------------------------------------------------
// Fetch phase — HTTP calls only, no database access anywhere in this
// section (see the module doc's "three-phase shape").
// ---------------------------------------------------------------------------

async fn poll_health(control_plane: &Arc<dyn ControlPlane>) -> Result<Health, OrchError> {
    control_plane.health().await
}

async fn poll_status(control_plane: &Arc<dyn ControlPlane>) -> Result<FleetStatus, OrchError> {
    control_plane.status().await
}

/// `GET /runs?project=`, one call per linked project (docket has no
/// fleet-wide runs listing). `projects` comes from
/// [`ControlPlaneStore::list_linked_projects`], fetched in [`spawn_one`]
/// before this runs. Each project's own result is kept independent so one
/// project's failure never drops another's runs for this tick.
async fn poll_runs(
    control_plane: &Arc<dyn ControlPlane>,
    projects: &[String],
) -> Vec<(String, Result<Vec<RemoteRun>, OrchError>)> {
    let mut out = Vec::with_capacity(projects.len());
    for project in projects {
        let result = control_plane.list_runs(Some(project)).await;
        out.push((project.clone(), result));
    }
    out
}

/// `GET /approvals` — fleet-wide, not per-project. A failure here
/// must never influence [`evaluate`]'s reachability verdict: a docket that's
/// up but whose `/approvals` route errors is a degraded *feature*, not a
/// degraded *plane*.
async fn poll_approvals(
    control_plane: &Arc<dyn ControlPlane>,
) -> Result<Vec<RemoteApproval>, OrchError> {
    control_plane.list_approvals().await
}

/// `GET /metrics` — fleet-wide, not per-project, exactly like `/health`/
/// `/status.json`. `ControlPlane::metrics()` (the `DocketAdapter` impl)
/// already fetches the raw Prometheus text and
/// parses it via `adapters::prometheus::parse` internally — this function is
/// not a second parsing step, just the same thin HTTP-call wrapper every
/// other `poll_*` fn is. A failure here must never influence [`evaluate`]'s
/// reachability verdict, same as `poll_approvals`.
async fn poll_metrics(
    control_plane: &Arc<dyn ControlPlane>,
) -> Result<Vec<MetricSample>, OrchError> {
    control_plane.metrics().await
}

/// One linked project's `/traces?since=` result for this tick, paired with
/// the exact cursor that was actually sent as `since` — carried alongside
/// the result rather than re-derived at persist time from a separately
/// threaded map, so [`persist_events`] can never pair a result with the
/// wrong "previous cursor" even if a future edit reorders when cursors are
/// read. `since` is `None` for a project that has never been polled before
/// (no stored row yet) — docket treats an absent/empty `since` as "from the
/// beginning", so the very first poll for a newly-linked project mirrors
/// its entire trace history, same as `poll_runs`'s first-poll behavior for
/// CLI-dispatched runs. The `Ok` payload is a [`TracesPage`] — events plus
/// the remote's own opaque `next` cursor, which [`persist_events`] stores
/// verbatim (see the module doc's "Trace cursor" section).
type TracesPollResult = (String, Option<String>, Result<TracesPage, OrchError>);

/// `GET /traces/{project}?since=`, one call per linked project — docket has
/// no fleet-wide trace listing, same shape as [`poll_runs`]. `cursors` is
/// this tick's starting cursor per project, resolved by
/// [`spawn_one`] via `ControlPlaneStore::list_trace_cursors` before the
/// fetch phase begins — the same "DB read outside the panic-isolation
/// boundary" pattern established for `list_linked_projects` (see the
/// module doc). Each project's own result is kept independent so one
/// project's failure never blocks another's traces for this tick.
async fn poll_traces(
    control_plane: &Arc<dyn ControlPlane>,
    projects: &[String],
    cursors: &HashMap<String, String>,
) -> Vec<TracesPollResult> {
    let mut out = Vec::with_capacity(projects.len());
    for project in projects {
        let since = cursors.get(project).cloned();
        let result = control_plane.traces(project, since.as_deref()).await;
        out.push((project.clone(), since, result));
    }
    out
}

/// Every HTTP call one poll tick needs, gathered as a flat struct-of-results
/// so one failing call never blocks the others from being attempted. See the
/// module doc for how to add a new field here.
struct FetchOutcome {
    health: Result<Health, OrchError>,
    status: Result<FleetStatus, OrchError>,
    runs: Vec<(String, Result<Vec<RemoteRun>, OrchError>)>,
    approvals: Result<Vec<RemoteApproval>, OrchError>,
    metrics: Result<Vec<MetricSample>, OrchError>,
    traces: Vec<TracesPollResult>,
}

/// Fetch phase for one poll tick. No database access happens in this
/// function or anything it calls — `projects`/`trace_cursors` are supplied
/// by the caller ([`spawn_one`]), already resolved before this is invoked.
/// Returns both the evaluated verdict (used for health) and the raw fetch
/// (used for runs/approvals/traces persistence) — see the module doc's "one
/// deviation from the recipe" note for why this isn't just
/// `PollEvaluation`.
async fn reconcile_once(
    control_plane: &Arc<dyn ControlPlane>,
    projects: &[String],
    trace_cursors: &HashMap<String, String>,
) -> (PollEvaluation, FetchOutcome) {
    let fetched = FetchOutcome {
        health: poll_health(control_plane).await,
        status: poll_status(control_plane).await,
        runs: poll_runs(control_plane, projects).await,
        approvals: poll_approvals(control_plane).await,
        metrics: poll_metrics(control_plane).await,
        traces: poll_traces(control_plane, projects, trace_cursors).await,
    };
    let evaluation = evaluate(&fetched);
    (evaluation, fetched)
}

/// Decide phase's input-independent half: turns a [`FetchOutcome`] into a
/// plain verdict. Deliberately reads only `.health`/`.status` — a
/// runs/approvals/traces/metrics fetch failure must never affect plane
/// reachability; those are data-ingestion concerns, not health.
struct PollEvaluation {
    reachable: bool,
    version_mismatch: bool,
    observed_api_version: Option<String>,
    detail: String,
}

/// apiVersion policy:
/// a "mismatch" is a difference in the **major** version component — the
/// substring before the first `.`, or the whole string if there is no `.`.
/// docket's version scheme today is a bare incrementing integer (`"2"`), so
/// in practice this is currently an exact-string comparison; the `.`-split
/// is there so a future move to a dotted scheme (`"2.1"` vs `"3.0"`) degrades
/// only on the part that actually signals a breaking contract change, not on
/// every patch bump.
fn major_version(v: &str) -> &str {
    v.split('.').next().unwrap_or(v)
}

fn evaluate(outcome: &FetchOutcome) -> PollEvaluation {
    let health_ok = outcome.health.is_ok();
    let status_ok = outcome.status.is_ok();
    let reachable = health_ok && status_ok;

    let (version_mismatch, observed_api_version) = match &outcome.status {
        Ok(status) => {
            let mismatch =
                major_version(&status.api_version) != major_version(EXPECTED_API_VERSION);
            (mismatch, Some(status.api_version.clone()))
        }
        Err(_) => (false, None),
    };

    let detail = if !reachable {
        let mut parts = Vec::new();
        if let Err(e) = &outcome.health {
            parts.push(format!("health: {e}"));
        }
        if let Err(e) = &outcome.status {
            parts.push(format!("status: {e}"));
        }
        parts.join("; ")
    } else if version_mismatch {
        format!(
            "apiVersion mismatch: Tack expects major version {}, control plane reports {}",
            major_version(EXPECTED_API_VERSION),
            observed_api_version.as_deref().unwrap_or("?")
        )
    } else {
        String::new()
    };

    PollEvaluation {
        reachable,
        version_mismatch,
        observed_api_version,
        detail,
    }
}

// ---------------------------------------------------------------------------
// Persistence interface
// ---------------------------------------------------------------------------

/// A control plane the reconciler should be polling, with its live adapter
/// already constructed. Building this from a `control_planes` DB row (kind
/// dispatch → concrete adapter, e.g. `DocketAdapter`) is `ControlPlaneStore`
/// implementors' job, not this module's — see the module doc's persistence
/// section.
#[derive(Clone)]
pub struct RegisteredPlane {
    pub id: Uuid,
    pub control_plane: Arc<dyn ControlPlane>,
}

/// What to persist after one poll tick. Field shapes mirror
/// `tack_db::Repository::update_control_plane_health`'s parameters exactly
/// (`i64` failure count, `Option<DateTime<Utc>>` with `None` = "don't
/// touch") so a `ControlPlaneStore` impl backed by the repo is a direct
/// pass-through.
#[derive(Debug, Clone)]
pub struct HealthRecord {
    pub health: HealthState,
    pub consecutive_failures: i64,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub api_version: Option<String>,
}

/// The narrow persistence interface the reconciler needs. Deliberately not
/// `tack_db::Repository` directly — see the module doc's "Persistence
/// interface" section for why.
///
/// The four methods below `record_health` are each a
/// thin, mechanical pass-through to a single `tack_db::repo::orch` function
/// (`list_orch_links_for_plane`, `find_orch_task_by_remote_task_id`,
/// `upsert_orch_runs`, `upsert_orch_approvals`) — the same shape
/// `record_health` already has against `update_control_plane_health`. No
/// correlation or business logic belongs in an implementor of this trait;
/// that lives in [`spawn_one`]'s persistence phase (`persist_runs`/
/// `persist_approvals`), which is the whole reason this trait stays narrow
/// rather than growing into `tack_db::Repository` by another name.
#[async_trait::async_trait]
pub trait ControlPlaneStore: Send + Sync {
    /// Every control plane currently registered, each with a live adapter
    /// ready to poll.
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError>;

    /// Persist one poll tick's outcome for a single plane.
    async fn record_health(
        &self,
        control_plane_id: Uuid,
        record: &HealthRecord,
    ) -> Result<(), OrchError>;

    /// Distinct `remote_project` names linked to this control plane
    /// (`orch_links.remote_project`) — what `poll_runs` needs to build its
    /// per-project `/runs?project=` calls. Order is not significant.
    async fn list_linked_projects(&self, control_plane_id: Uuid) -> Result<Vec<String>, OrchError>;

    /// Look up the Tack item a docket `remote_task_id` was dispatched for
    /// (`orch_tasks.remote_task_id` → `orch_tasks.item_id`), if any. `Ok(None)`
    /// means "no such task is known to Tack" — not an error; a run or
    /// approval correlating against it stays unattributed for this tick;
    /// CLI-dispatched work must not error here.
    async fn find_item_for_remote_task(
        &self,
        remote_task_id: &str,
    ) -> Result<Option<Uuid>, OrchError>;

    /// Batch upsert into `orch_runs` (`ON CONFLICT(run_id)`, idempotent —
    /// see `tack_db::repo::orch::upsert_orch_runs`). A `None` `item_id` on a
    /// `NewOrchRun` never clobbers a previously-learned attribution; the
    /// repo layer's `COALESCE` guarantees that, not this trait.
    async fn upsert_runs(
        &self,
        control_plane_id: Uuid,
        runs: &[NewOrchRun],
    ) -> Result<(), OrchError>;

    /// Batch upsert into `orch_approvals` (`ON CONFLICT(token)`, idempotent
    /// — see `tack_db::repo::orch::upsert_orch_approvals`). Same
    /// never-unlearn-an-attribution guarantee as [`Self::upsert_runs`].
    async fn upsert_approvals(
        &self,
        control_plane_id: Uuid,
        approvals: &[NewOrchApproval],
    ) -> Result<(), OrchError>;

    /// Batch insert into `orch_metrics` (append-only — see
    /// `tack_db::repo::orch::upsert_orch_metrics`'s doc comment for why a
    /// metric sample has no natural key to conflict on, unlike every other
    /// method on this trait).
    async fn upsert_metrics(
        &self,
        control_plane_id: Uuid,
        metrics: &[NewOrchMetric],
    ) -> Result<(), OrchError>;

    // ── Trace ingestion ──
    //
    // Same thin-pass-through discipline as every method above: no cursor
    // arithmetic (the cursor is opaque — see the module doc's "Trace
    // cursor" section), no event-id derivation, no retention-age filtering
    // here — all of that lives in `derive_event_id`/`persist_events` in
    // this module. An implementor's job is exactly "read/write these
    // rows", nothing more.

    /// Every stored resume cursor for this plane's linked projects, keyed by
    /// `remote_project` (`tack_db::repo::orch::list_trace_cursors`). A
    /// project absent from the map has never been polled (or its cursor was
    /// never advanced) — [`poll_traces`] treats that as `since: None`
    /// ("from the beginning"), not an error.
    async fn list_trace_cursors(
        &self,
        control_plane_id: Uuid,
    ) -> Result<HashMap<String, String>, OrchError>;

    /// Persist the resume cursor for one `(control_plane_id, remote_project)`
    /// pair after a poll (`tack_db::repo::orch::set_trace_cursor`) — see the
    /// module doc's "Trace cursor" section for why the cursor lives keyed
    /// this way rather than as a column on `orch_links`.
    async fn set_trace_cursor(
        &self,
        control_plane_id: Uuid,
        remote_project: &str,
        cursor: &str,
    ) -> Result<(), OrchError>;

    /// Batch upsert into `orch_events` (`ON CONFLICT(id)`, idempotent — see
    /// `tack_db::repo::orch::upsert_orch_events` and [`derive_event_id`],
    /// which is what makes the same source event always produce the same
    /// `id`).
    async fn upsert_events(
        &self,
        control_plane_id: Uuid,
        events: &[NewOrchEvent],
    ) -> Result<(), OrchError>;
}

// ---------------------------------------------------------------------------
// Persist phase — runs/approvals ingestion and item correlation
// ---------------------------------------------------------------------------
//
// Everything below is called from spawn_one's persistence phase, strictly
// after the fetch phase (reconcile_once) has already completed — see the
// module doc. Correlation itself (mapping a docket-side id to a Tack item)
// lives here, not in the ControlPlaneStore trait or its implementors: the
// store's job is mechanical CRUD, this module's job is deciding what to CRUD.

/// A run or approval attributes to whichever task id, out of a candidate
/// list, is the first one Tack actually knows about. `None` means none of
/// the candidates correlate — the normal, expected state for a
/// docket-CLI-dispatched run (empty `task_ids`) and must not be treated as
/// an error.
async fn correlate_remote_task(
    store: &dyn ControlPlaneStore,
    candidates: impl IntoIterator<Item = &str>,
) -> Option<Uuid> {
    for remote_task_id in candidates {
        match store.find_item_for_remote_task(remote_task_id).await {
            Ok(Some(item_id)) => return Some(item_id),
            Ok(None) => continue,
            Err(e) => {
                warn!(
                    remote_task_id = %remote_task_id,
                    error = %e,
                    "failed to correlate remote task id to an item; treating as uncorrelated for this candidate"
                );
                continue;
            }
        }
    }
    None
}

/// `RemoteApproval::context`'s one documented shape is
/// `{"taskId": "...", "pipelineIndex": 0}`, but `context` is
/// an open dict on docket's side — a missing or non-string `taskId` is an
/// uncorrelated approval, not a parse error.
fn extract_task_id(context: &serde_json::Value) -> Option<String> {
    context.get("taskId")?.as_str().map(str::to_string)
}

/// Parses one of docket's two observed ISO 8601 timestamp conventions
/// (`...+00:00` from `core/runs.py`, `...Z` from `core/approval.py`) into a
/// `DateTime<Utc>`. `None` in, `None`
/// out; a malformed string in also degrades to `None` rather than failing
/// the whole poll — a run/approval with an unparseable timestamp still gets
/// its other fields mirrored.
fn parse_optional_rfc3339(s: Option<&str>) -> Option<DateTime<Utc>> {
    s.and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

/// Batch-correlates and upserts one tick's runs. `runs` is
/// `FetchOutcome::runs` verbatim: one `(remote_project, Result<..>)` pair per
/// linked project polled this tick. A project whose poll failed is logged
/// and skipped — it does not block the other projects' runs from landing,
/// and it never touches plane health (only `.health`/`.status` do that; see
/// `evaluate`).
async fn persist_runs(
    store: &dyn ControlPlaneStore,
    control_plane_id: Uuid,
    runs: &[(String, Result<Vec<RemoteRun>, OrchError>)],
) {
    let mut new_runs = Vec::new();

    for (project, result) in runs {
        match result {
            Ok(remote_runs) => {
                for run in remote_runs {
                    let item_id =
                        correlate_remote_task(store, run.task_ids.iter().map(String::as_str)).await;
                    new_runs.push(NewOrchRun {
                        run_id: run.id.clone(),
                        item_id,
                        // `run.project` (docket's own field on the record) rather
                        // than the queried `project` string — they should always
                        // agree since we always pass `Some(project)`, but the
                        // record's own field is the more authoritative source.
                        remote_project: run.project.clone(),
                        source: run.source.as_str().to_string(),
                        state: run.state.as_str().to_string(),
                        started_at: parse_optional_rfc3339(run.started_at.as_deref()),
                        ended_at: parse_optional_rfc3339(run.finished_at.as_deref()),
                        error: (!run.error.is_empty()).then(|| run.error.clone()),
                    });
                }
            }
            Err(e) => {
                debug!(
                    control_plane_id = %control_plane_id,
                    project = %project,
                    error = %e,
                    "failed to poll runs for a linked project this tick; will retry next tick"
                );
            }
        }
    }

    if !new_runs.is_empty() {
        let count = new_runs.len();
        if let Err(e) = store.upsert_runs(control_plane_id, &new_runs).await {
            warn!(control_plane_id = %control_plane_id, error = %e, "failed to persist mirrored runs");
        } else {
            debug!(control_plane_id = %control_plane_id, count, "mirrored runs upserted");
        }
    }
}

/// Batch-correlates and upserts one tick's approvals. `approvals` is
/// `FetchOutcome::approvals` verbatim — a fleet-wide poll, not per-project.
/// A record whose `created` timestamp doesn't parse is skipped with a
/// warning (`requested_at` is a required column) rather than aborting the
/// whole batch; every other approval in the same poll still lands.
async fn persist_approvals(
    store: &dyn ControlPlaneStore,
    control_plane_id: Uuid,
    approvals: &Result<Vec<RemoteApproval>, OrchError>,
) {
    let remote_approvals = match approvals {
        Ok(a) => a,
        Err(e) => {
            debug!(
                control_plane_id = %control_plane_id,
                error = %e,
                "failed to poll approvals this tick; will retry next tick"
            );
            return;
        }
    };

    let mut new_approvals = Vec::with_capacity(remote_approvals.len());

    for approval in remote_approvals {
        let task_id = extract_task_id(&approval.context);
        let item_id = match &task_id {
            Some(task_id) => correlate_remote_task(store, std::iter::once(task_id.as_str())).await,
            None => None,
        };

        let Some(requested_at) = parse_optional_rfc3339(Some(&approval.created)) else {
            warn!(
                token = %approval.token,
                created = %approval.created,
                "skipping approval with an unparseable created timestamp"
            );
            continue;
        };

        new_approvals.push(NewOrchApproval {
            token: approval.token.clone(),
            item_id,
            remote_task_id: task_id,
            // `role` is docket's name for who the gate is asking — mapped
            // onto the `agent` column, which is what the fleet-wide
            // approvals inbox actually displays.
            agent: Some(approval.role.clone()),
            action: Some(approval.action.clone()),
            state: approval.state.as_str().to_string(),
            requested_at,
            // RemoteApproval carries no decided_at — /approvals only ever
            // returns the still-`pending` set (see docket_adapter's
            // `ApprovalsResponse` doc comment).
            decided_at: None,
        });
    }

    if !new_approvals.is_empty() {
        let count = new_approvals.len();
        if let Err(e) = store
            .upsert_approvals(control_plane_id, &new_approvals)
            .await
        {
            warn!(control_plane_id = %control_plane_id, error = %e, "failed to persist mirrored approvals");
        } else {
            debug!(control_plane_id = %control_plane_id, count, "mirrored approvals upserted");
        }
    }
}

/// Persists one tick's `/metrics` scrape. No correlation is needed
/// — metrics aren't attributed to an item — so, unlike `persist_runs`/
/// `persist_approvals`, this is a straight translation from
/// `FetchOutcome::metrics` to a batch of `NewOrchMetric`. A poll failure is
/// logged and skipped, never propagated into the health verdict already
/// decided in the fetch phase (the same rule every other `persist_*` fn
/// here follows).
async fn persist_metrics(
    store: &dyn ControlPlaneStore,
    control_plane_id: Uuid,
    metrics: &Result<Vec<MetricSample>, OrchError>,
) {
    let samples = match metrics {
        Ok(s) => s,
        Err(e) => {
            debug!(
                control_plane_id = %control_plane_id,
                error = %e,
                "failed to poll metrics this tick; will retry next tick"
            );
            return;
        }
    };

    if samples.is_empty() {
        return;
    }

    let new_metrics: Vec<NewOrchMetric> = samples
        .iter()
        .map(|s| NewOrchMetric {
            name: s.name.clone(),
            labels: s.labels.clone(),
            value: s.value,
        })
        .collect();

    let count = new_metrics.len();
    if let Err(e) = store.upsert_metrics(control_plane_id, &new_metrics).await {
        warn!(control_plane_id = %control_plane_id, error = %e, "failed to persist mirrored metrics");
    } else {
        debug!(control_plane_id = %control_plane_id, count, "mirrored metrics upserted");
    }
}

// ---------------------------------------------------------------------------
// Trace ingestion — event-id derivation, cursor
// reconstruction, and persistence. See the module doc's "Trace cursor"
// section for the full argument; the functions below are its implementation.
// ---------------------------------------------------------------------------

/// Fixed namespace for [`derive_event_id`]'s UUIDv5 derivation. The exact
/// bytes are an arbitrary (but permanently fixed) 16-byte constant — ASCII
/// spelling "tack-orch-events" is a mnemonic, not a meaningful namespace
/// URL. Changing these bytes would silently re-mint a different id for
/// every previously-ingested event, defeating the entire point of a
/// deterministic id, so this must never change once any real deployment has
/// ingested a single trace event.
const ORCH_EVENT_ID_NAMESPACE: Uuid = Uuid::from_bytes(*b"tack-orch-events");

/// Derives `orch_events.id` as a pure function of the source docket trace
/// event, so the *same* event ingested on two different polls — an
/// overlapping cursor window, a rewound/lost cursor, a restart — always
/// produces the *same* row. `upsert_orch_events`'s `ON CONFLICT(id) DO
/// UPDATE` then makes re-ingestion a no-op row-count-wise.
///
/// docket's trace records (`core/trace.py::trace_event`, confirmed by
/// reading the writer directly, not guessed) carry no monotonic sequence
/// number or byte offset — every field written is `ts`, `project`,
/// `session_id`, `agent_role`, `event_type`, `payload`, and two optional
/// fields (`cost_usd`, `duration_ms`). A monotonic-sequence key
/// (`(control_plane_id, remote_project, seq)`) is therefore unavailable;
/// this uses UUIDv5 (namespace + name, deterministic, no randomness)
/// over `control_plane_id`, `remote_project`, and every field of the event.
///
/// `payload` is a `serde_json::Value`; this crate never enables
/// `serde_json`'s `preserve_order` feature (see `Cargo.toml` — no
/// `indexmap` in the dependency tree), so `Value::Object` is backed by a
/// `BTreeMap` and always serializes with its keys in sorted order — a
/// canonicalised form with stable field order falls out of the
/// workspace's existing `serde_json` configuration for free, not a bespoke
/// canonicalizer.
///
/// Every field is joined with `\u{1}` (a control character no real docket
/// field is going to contain) so that, e.g., an empty `session_id` followed
/// by `"x"` can never hash the same as a non-empty `session_id` of `"x"`
/// preceded by nothing — naive string concatenation without a delimiter
/// would not have that property.
///
/// **A caveat worth naming, not hiding:** two *genuinely distinct* docket
/// events that happen to be byte-for-byte identical across every field this
/// function reads (same second-granularity `ts`, same session, same role,
/// same type, same payload, same cost/duration) hash to the same id and
/// collapse into one row. Given a payload usually carries something
/// turn-specific (a tool command, a token count), this is vanishingly
/// unlikely in practice — and if it ever happened, the two events were
/// already indistinguishable to any consumer of this table.
fn derive_event_id(control_plane_id: Uuid, remote_project: &str, event: &RemoteEvent) -> Uuid {
    const SEP: char = '\u{1}';
    let payload = serde_json::to_string(&event.payload).unwrap_or_default();
    let canonical = format!(
        "{control_plane_id}{SEP}{remote_project}{SEP}{}{SEP}{}{SEP}{}{SEP}{}{SEP}{payload}{SEP}{:?}{SEP}{:?}",
        event.ts,
        event.session_id,
        event.agent_role,
        event.event_type,
        event.cost_usd_estimated,
        event.duration_ms,
    );
    Uuid::new_v5(&ORCH_EVENT_ID_NAMESPACE, canonical.as_bytes())
}

/// Extracts the trailing `<suffix>` from docket's `session_id` convention
/// `"agent:<project>:<suffix>"` (`core/dispatch.py`'s `enqueue_task`/hop
/// execution, confirmed by reading the writer directly) as a candidate
/// `orch_tasks.remote_task_id` to correlate against — the same "try, and
/// treat a miss as normal" shape [`persist_approvals`] already uses for
/// `context.taskId`. `<suffix>` is the real task id for a task-dispatched
/// session, but docket also uses this convention for non-task sessions
/// (`"agent:<project>:dispatch"` for a bare project-level dispatch,
/// `core/pod.py`'s own project-key session) — [`correlate_remote_task`] on
/// the result simply won't find a matching `orch_tasks` row for those,
/// which is not an error, so no special-casing is needed here beyond
/// parsing the string.
fn session_id_task_id(session_id: &str) -> Option<String> {
    session_id
        .strip_prefix("agent:")
        .and_then(|rest| rest.split_once(':'))
        .map(|(_project, suffix)| suffix.to_string())
}

/// Batch-derives, correlates, and upserts one tick's trace events, then
/// advances (or leaves untouched) each project's cursor. `traces` is
/// `FetchOutcome::traces` verbatim. A project whose poll failed is logged
/// and skipped — same as [`persist_runs`]/[`persist_approvals`] — and never
/// touches plane health (only `.health`/`.status` do that; see
/// [`evaluate`]).
///
/// **Retention composition** (see the module doc's "Trace cursor" section
/// for the full argument): an event whose `occurred_at` already predates
/// `now - retention_days` — the same cutoff formula
/// [`spawn_retention_sweep`] uses — is dropped, not inserted. Without this,
/// a lost/rewound cursor could resurrect a raw row for an event that was
/// already rolled into `orch_events_daily` and purged; because
/// `orch_events.id` is content-derived rather than server-generated, that
/// resurrection is indistinguishable from a brand-new event to the next
/// sweep, which would then roll its count in a *second* time. Dropping it
/// here instead means the only cost is not (re-)counting a handful of
/// events at the extreme edge of a pathological rewind — never a corrupted
/// total.
async fn persist_events(
    store: &dyn ControlPlaneStore,
    control_plane_id: Uuid,
    traces: &[TracesPollResult],
    retention_days: u32,
) {
    let retention_cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);

    for (project, since, result) in traces {
        let page = match result {
            Ok(page) => page,
            Err(e) => {
                debug!(
                    control_plane_id = %control_plane_id,
                    project = %project,
                    error = %e,
                    "failed to poll traces for a linked project this tick; will retry next tick"
                );
                continue;
            }
        };
        let events = &page.events;

        // The remote's own cursor, forwarded verbatim — never recomputed
        // here (see the module doc's "Trace cursor" section). `None` means
        // the remote didn't mint one this poll; treated as "unchanged",
        // same as the old reconstruction's "no usable anchor" case.
        let next_cursor = page.next.clone();

        let mut new_events = Vec::with_capacity(events.len());
        let mut dropped_stale = 0u32;
        for event in events {
            let Some(occurred_at) = parse_optional_rfc3339(Some(&event.ts)) else {
                warn!(
                    control_plane_id = %control_plane_id,
                    project = %project,
                    ts = %event.ts,
                    "skipping trace event with an unparseable ts"
                );
                continue;
            };
            if occurred_at < retention_cutoff {
                dropped_stale += 1;
                continue;
            }

            let id = derive_event_id(control_plane_id, project, event);
            let item_id = match session_id_task_id(&event.session_id) {
                Some(task_id) => {
                    correlate_remote_task(store, std::iter::once(task_id.as_str())).await
                }
                None => None,
            };

            new_events.push(NewOrchEvent {
                id,
                item_id,
                // docket's trace payload carries no run_id, only session_id
                // (itself derived from task_id, not run_id — see
                // `session_id_task_id`'s doc). Left unset rather than
                // guessing at a session_id → run_id lookup this store
                // doesn't expose.
                run_id: None,
                event_type: event.event_type.clone(),
                payload: event.payload.clone(),
                occurred_at,
            });
        }

        if dropped_stale > 0 {
            warn!(
                control_plane_id = %control_plane_id,
                project = %project,
                dropped_stale,
                retention_days,
                "dropped trace events older than the retention cutoff instead of \
                 resurrecting an already-purged row"
            );
        }

        if !new_events.is_empty() {
            let count = new_events.len();
            if let Err(e) = store.upsert_events(control_plane_id, &new_events).await {
                warn!(
                    control_plane_id = %control_plane_id,
                    project = %project,
                    error = %e,
                    "failed to persist mirrored trace events"
                );
            } else {
                debug!(
                    control_plane_id = %control_plane_id,
                    project = %project,
                    count,
                    "mirrored trace events upserted"
                );
            }
        }

        if let Some(next) = &next_cursor
            && Some(next.as_str()) != since.as_deref()
            && let Err(e) = store
                .set_trace_cursor(control_plane_id, project, next)
                .await
        {
            warn!(
                control_plane_id = %control_plane_id,
                project = %project,
                error = %e,
                "failed to persist trace cursor"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Retention sweep — a separate background task,
// not per-plane. See the module doc's "Retention sweep" section for how this
// relates to the per-plane poll/persist machinery above.
// ---------------------------------------------------------------------------

/// Default retention window, matching `AppConfig::orch_event_retention_days`'s
/// own default (`crates/tack-api/src/config.rs`). Only a fallback for callers
/// that don't have the configured value handy (e.g. a quick manual sweep) —
/// [`spawn_retention_sweep`] takes `retention_days` as an explicit parameter
/// so the real configured value can be threaded in without this crate
/// depending on `tack-api`'s config type.
pub const DEFAULT_RETENTION_DAYS: u32 = 90;

/// Rows processed per sweep transaction — see
/// `tack_db::Repository::rollup_and_purge_orch_events`'s doc comment for why
/// this is bounded rather than one transaction for the whole backlog.
pub const RETENTION_BATCH_SIZE: i64 = 500;

/// Outcome of rolling up and purging one table's stale rows. Mirrors
/// `tack_db::repo::orch::RollupStats` field-for-field so a [`RetentionStore`]
/// impl backed by the real repo layer is a direct pass-through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RollupOutcome {
    pub rows_purged: i64,
    pub batches_run: i64,
}

/// The narrow persistence interface the retention sweep needs. Deliberately
/// separate from [`ControlPlaneStore`]: retention operates fleet-wide across
/// `orch_events`/`orch_metrics`, independent of which planes are currently
/// registered, and needs none of `ControlPlaneStore`'s
/// adapter-construction/per-plane machinery.
#[async_trait::async_trait]
pub trait RetentionStore: Send + Sync {
    /// Roll every `orch_events` row older than `cutoff` into `orch_events_daily`
    /// and delete the raw rows, batched. See
    /// `tack_db::Repository::rollup_and_purge_orch_events`'s doc comment for
    /// the atomicity argument this method's implementors must preserve: the
    /// aggregate write and the delete for a given batch must commit together,
    /// not as two independently-committed steps.
    async fn rollup_and_purge_events(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<RollupOutcome, OrchError>;

    /// Same contract as [`Self::rollup_and_purge_events`], for `orch_metrics` /
    /// `orch_metrics_daily`.
    async fn rollup_and_purge_metrics(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<RollupOutcome, OrchError>;
}

/// Spawn the retention sweep, or don't — the same off-by-default contract as
/// [`spawn_reconcilers`]: `enabled = false` returns `None`
/// immediately without calling `store` at all.
///
/// **Not yet wired into `server.rs`.** Nothing currently calls this at boot,
/// so orchestration event/metric retention does not actually run. Wiring it
/// in mirrors the existing reconciler spawn block there.
///
/// Runs both tables' sweeps back-to-back on one ticker, `sweep_interval_secs`
/// apart, starting immediately (no initial delay, same as
/// [`spawn_one`]'s poll loop). A failure in either sweep is logged and
/// retried next cycle — it never panics the task or stops the ticker.
pub fn spawn_retention_sweep(
    enabled: bool,
    store: Arc<dyn RetentionStore>,
    retention_days: u32,
    sweep_interval_secs: u64,
) -> Option<tokio::task::JoinHandle<()>> {
    if !enabled {
        return None;
    }

    Some(tokio::spawn(async move {
        let interval_secs = sweep_interval_secs.max(1);
        loop {
            let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);

            match store
                .rollup_and_purge_events(cutoff, RETENTION_BATCH_SIZE)
                .await
            {
                Ok(outcome) if outcome.rows_purged > 0 => info!(
                    rows_purged = outcome.rows_purged,
                    batches = outcome.batches_run,
                    "orch_events retention sweep rolled up and purged stale rows"
                ),
                Ok(_) => debug!("orch_events retention sweep: nothing stale to purge"),
                Err(e) => {
                    warn!(error = %e, "orch_events retention sweep failed; will retry next cycle")
                }
            }

            match store
                .rollup_and_purge_metrics(cutoff, RETENTION_BATCH_SIZE)
                .await
            {
                Ok(outcome) if outcome.rows_purged > 0 => info!(
                    rows_purged = outcome.rows_purged,
                    batches = outcome.batches_run,
                    "orch_metrics retention sweep rolled up and purged stale rows"
                ),
                Ok(_) => debug!("orch_metrics retention sweep: nothing stale to purge"),
                Err(e) => {
                    warn!(error = %e, "orch_metrics retention sweep failed; will retry next cycle")
                }
            }

            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
        }
    }))
}

// ---------------------------------------------------------------------------
// Spawn
// ---------------------------------------------------------------------------

/// Reconciler configuration. `poll_secs` is the base interval before backoff
/// and jitter are applied. `event_retention_days` must match
/// `TACK_ORCH_EVENT_RETENTION_DAYS` — it feeds [`persist_events`]'s
/// retention-composition guard, which needs the *same* cutoff
/// [`spawn_retention_sweep`] uses, not an independently-configured one (a
/// mismatch here would either resurrect purged rows or drop events the
/// sweep hasn't purged yet). `supervisor_scan_secs` is unrelated
/// to any single plane's poll cadence — see [`spawn_reconcilers_supervised`].
#[derive(Debug, Clone, Copy)]
pub struct ReconcilerConfig {
    pub poll_secs: u64,
    pub event_retention_days: u32,
    pub supervisor_scan_secs: u64,
}

impl Default for ReconcilerConfig {
    fn default() -> Self {
        Self {
            poll_secs: 10,
            event_retention_days: DEFAULT_RETENTION_DAYS,
            supervisor_scan_secs: DEFAULT_SUPERVISOR_SCAN_SECS,
        }
    }
}

/// How often [`spawn_reconcilers_supervised`]'s background loop re-reads
/// `store.list_registered()` and starts/stops per-plane pollers to match.
/// Deliberately small and decoupled from `poll_secs` (a plane's own poll
/// cadence, which can be much larger): the setup wizard's "enable ->
/// register -> link" flow needs a newly-registered
/// plane to start showing health within a couple of seconds, not wait for
/// whatever poll interval an operator configured.
pub const DEFAULT_SUPERVISOR_SCAN_SECS: u64 = 2;

/// Spawn one reconciler task per registered control plane, or none at all.
///
/// This is the single gate the "off by default" contract depends on: when
/// `enabled` is
/// `false`, this returns immediately with an empty `Vec` **without calling
/// `store.list_registered()` at all** — not just "spawns nothing", but "does
/// not even query for what it would have spawned". See the unit test
/// `disabled_orchestration_spawns_no_tasks_and_never_queries_the_store` for
/// the assertion.
///
/// `enabled` is read from `TACK_ORCH_ENABLE` by the caller (`tack-api`'s
/// `server.rs`) — this function takes a plain `bool` rather than reading the
/// environment itself so it stays testable without env-var mutation.
pub async fn spawn_reconcilers(
    enabled: bool,
    store: Arc<dyn ControlPlaneStore>,
    config: ReconcilerConfig,
) -> Vec<tokio::task::JoinHandle<()>> {
    if !enabled {
        return Vec::new();
    }

    let planes = match store.list_registered().await {
        Ok(planes) => planes,
        Err(e) => {
            warn!(error = %e, "failed to list registered control planes; orchestration reconciler not started");
            return Vec::new();
        }
    };

    planes
        .into_iter()
        .map(|plane| spawn_one(plane, Arc::clone(&store), config, None))
        .collect()
}

/// Resolves once `stop_rx` carries `true` — either because it already did
/// when called, or because a later `send(true)` changes it. Resolves
/// (rather than hanging forever) if the sender is dropped without ever
/// sending `true`, so a task can never be stranded by a stop channel whose
/// other end went away.
async fn wait_until_stopped(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

/// One `tokio` task per plane, looping: fetch → decide → persist → sleep.
/// See the module doc for the panic-isolation and phase-separation
/// rationale.
///
/// `stop_rx`, when present (the runtime enable/disable toggle, driven
/// per-plane by the supervisor — see [`spawn_reconcilers_supervised`]), is
/// checked at
/// the top of every loop iteration and raced against the end-of-tick sleep
/// via `tokio::select!`.
/// Both are safe points: nothing here ever awaits an HTTP call or holds a
/// SQLite write transaction across a check, so a task can only ever stop
/// between ticks, never mid-fetch or mid-persist.
/// `None` (the plain [`spawn_reconcilers`] path) preserves the original,
/// uncancellable infinite loop exactly — existing callers/tests are
/// unaffected.
fn spawn_one(
    plane: RegisteredPlane,
    store: Arc<dyn ControlPlaneStore>,
    config: ReconcilerConfig,
    mut stop_rx: Option<watch::Receiver<bool>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let RegisteredPlane { id, control_plane } = plane;
        let mut tracker = HealthTracker::new();
        let mut tick: u64 = 0;

        loop {
            if let Some(rx) = stop_rx.as_ref()
                && *rx.borrow()
            {
                info!(control_plane_id = %id, "reconciler stopping (orchestration disabled)");
                return;
            }

            tick += 1;

            // Which projects to poll /runs?project= for this tick. A single
            // short DB read, not held open across any HTTP .await below —
            // see the module doc's note on why this lives here rather than
            // inside the panic-isolated fetch phase. A failure here just
            // means this tick mirrors no runs (not an error, not a health
            // signal); it never blocks health/status/approvals polling.
            let projects = match store.list_linked_projects(id).await {
                Ok(projects) => projects,
                Err(e) => {
                    warn!(
                        control_plane_id = %id,
                        error = %e,
                        "failed to list linked projects; skipping run ingestion this tick"
                    );
                    Vec::new()
                }
            };

            // Which cursor to start each linked project's /traces poll from
            // this tick. Same pattern and same accepted staleness
            // window as the `projects` read just above — a single short DB
            // read, not held open across any HTTP .await below. A failure
            // here just means every project starts this tick's traces poll
            // fresh (`since: None`, i.e. "from the beginning") rather than
            // resuming — safe (content-derived event ids make re-ingestion
            // idempotent) if wasteful, and never a health signal.
            let trace_cursors = match store.list_trace_cursors(id).await {
                Ok(cursors) => cursors,
                Err(e) => {
                    warn!(
                        control_plane_id = %id,
                        error = %e,
                        "failed to list trace cursors; polling traces from the beginning this tick"
                    );
                    HashMap::new()
                }
            };

            // Fetch phase, isolated: a panic anywhere inside reconcile_once
            // (this poll or a future poll_* a Wave-2 card adds) surfaces as
            // a JoinError here rather than unwinding this loop — this task
            // keeps ticking, and no other plane's task is affected either.
            let cp = Arc::clone(&control_plane);
            let poll_result =
                tokio::spawn(async move { reconcile_once(&cp, &projects, &trace_cursors).await })
                    .await;

            let now = Utc::now();
            let (evaluation, fetched) = match poll_result {
                Ok((eval, fetched)) => (eval, fetched),
                Err(join_err) => {
                    error!(
                        control_plane_id = %id,
                        error = %join_err,
                        "control-plane poll panicked; treating this tick as a failed poll"
                    );
                    let panic_err = || OrchError::Unavailable("poll task panicked".to_string());
                    (
                        PollEvaluation {
                            reachable: false,
                            version_mismatch: false,
                            observed_api_version: None,
                            detail: format!("poll task panicked: {join_err}"),
                        },
                        FetchOutcome {
                            health: Err(panic_err()),
                            status: Err(panic_err()),
                            runs: Vec::new(),
                            approvals: Err(panic_err()),
                            metrics: Err(panic_err()),
                            traces: Vec::new(),
                        },
                    )
                }
            };

            // Decide phase: pure, synchronous.
            let transition =
                tracker.observe(evaluation.reachable, evaluation.version_mismatch, now);

            // Persist phase: one short call, strictly after the fetch above
            // has already completed.
            let record = HealthRecord {
                health: transition.state,
                consecutive_failures: transition.consecutive_failures,
                last_seen_at: transition.last_seen_at,
                api_version: evaluation.observed_api_version.clone(),
            };
            if let Err(e) = store.record_health(id, &record).await {
                warn!(control_plane_id = %id, error = %e, "failed to persist control-plane health");
            }

            // Persist phase: runs/approvals ingestion. Strictly
            // after record_health, strictly after the fetch phase above has
            // already completed — no HTTP call is in flight during either
            // of these. Failures here are logged and skipped, never
            // propagated into the health verdict already decided above.
            persist_runs(store.as_ref(), id, &fetched.runs).await;
            persist_approvals(store.as_ref(), id, &fetched.approvals).await;
            persist_metrics(store.as_ref(), id, &fetched.metrics).await;
            persist_events(
                store.as_ref(),
                id,
                &fetched.traces,
                config.event_retention_days,
            )
            .await;

            match transition.log {
                Some(LogSeverity::Warn) => warn!(
                    control_plane_id = %id,
                    state = transition.state.as_str(),
                    consecutive_failures = transition.consecutive_failures,
                    detail = %evaluation.detail,
                    "control plane health degraded"
                ),
                Some(LogSeverity::Info) => info!(
                    control_plane_id = %id,
                    state = transition.state.as_str(),
                    "control plane recovered"
                ),
                None => debug!(
                    control_plane_id = %id,
                    state = transition.state.as_str(),
                    consecutive_failures = transition.consecutive_failures,
                    "control plane poll (no state change)"
                ),
            }

            let base = if transition.consecutive_failures > 0 {
                backoff_secs(transition.consecutive_failures, config.poll_secs)
            } else {
                config.poll_secs
            };
            let sleep_for = jittered_secs(&id, tick, base);
            match stop_rx.as_mut() {
                Some(rx) => {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(sleep_for)) => {}
                        _ = wait_until_stopped(rx) => {
                            info!(control_plane_id = %id, "reconciler stopping (orchestration disabled)");
                            return;
                        }
                    }
                }
                None => tokio::time::sleep(Duration::from_secs(sleep_for)).await,
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Supervisor — keeps the running set of per-plane
// pollers in sync with `control_planes`, rather than reading it once.
// ---------------------------------------------------------------------------
//
// **The bug this replaces.** `spawn_reconcilers`/the old
// `spawn_reconcilers_cancellable` each called `store.list_registered()`
// exactly once and spawned one `spawn_one` task per plane found at that
// instant — the list was never re-read. A control plane registered *after*
// the reconciler started was therefore never polled: no task, no health
// updates, no run/approval/trace/metric mirroring, and no error anywhere,
// because nothing failed — the snapshot was simply stale forever. This
// mattered in practice because "enable orchestration -> register a control
// plane -> link a project" is the natural setup order (and exactly what the
// guided setup wizard walks a user through), so the bug landed squarely in
// the first-run path.
//
// **Why a supervisor loop, not an event from the create/delete handlers.**
// Two shapes were on the table: (a) a background loop that periodically
// re-reads `list_registered()` and diffs it against the currently-running
// task set, or (b) the control-plane create/delete handlers notifying the
// runtime directly. (b) is lower-latency in the common case, but every
// future write path that can change `control_planes` (a bulk import, a
// direct DB edit, a restore from backup) has to remember to signal it, and
// any path that doesn't is a silent repeat of this exact bug. (a)
// self-heals regardless of *how* the table changed — including a row
// deleted directly in the database, which no handler-notification scheme
// can observe by construction — at the cost of a bounded polling delay
// ([`DEFAULT_SUPERVISOR_SCAN_SECS`], deliberately small: a few seconds, not
// the per-plane `poll_secs`). The goal is self-healing regardless of how the
// table changed, and the delay is small enough not to hurt the wizard's
// "register -> see it come alive" moment, so (a) is what's implemented.
// Nothing here rules out adding an event-driven nudge later
// (e.g. the create-control-plane handler could shrink the *next* scan's
// wait by writing to a `Notify`) if the scan interval ever needs to be
// larger than a few seconds; it isn't needed today and would be a second
// cancellation-adjacent mechanism for no observable benefit yet.
//
// **What's reused, what's new.** Every per-plane poller is still exactly
// [`spawn_one`], with exactly the same fetch -> decide -> persist -> sleep
// shape and the same `watch`-channel stop signal each poller already uses —
// the supervisor just gives each plane its *own* channel and sender instead
// of one shared broadcast for the whole fleet, so it can stop a single
// plane's poller (deleted) without touching the others. This is the same
// primitive multiplied per-plane, not a second cancellation mechanism.

/// One currently-running per-plane poller, as tracked by the supervisor:
/// its `spawn_one` handle, plus the sender half of *that plane's own* stop
/// channel (not shared with any other plane — see the module doc above).
struct PlaneTask {
    handle: tokio::task::JoinHandle<()>,
    stop_tx: watch::Sender<bool>,
}

/// Supervisor state shared with [`SupervisedReconciler`]'s `live_task_count`
/// — written only by [`supervisor_loop`]/[`reconcile_tick`], read (for a
/// live count, filtering out anything that already exited on its own) by
/// anything holding a clone.
type PlaneTasks = Arc<AsyncMutex<HashMap<Uuid, PlaneTask>>>;

/// Handle to a live supervised reconciler run. Returned by
/// [`spawn_reconcilers_supervised`]; the caller (`tack-api`'s
/// `orch_runtime.rs`) keeps this around only to query
/// [`Self::live_task_count`] — stopping the whole run is done via the
/// `stop_rx` passed into `spawn_reconcilers_supervised`, not through this
/// handle (mirrors `OrchRuntime::stop`'s existing non-blocking-stop
/// discipline: nothing here needs to be awaited to shut down cleanly).
pub struct SupervisedReconciler {
    tasks: PlaneTasks,
}

impl SupervisedReconciler {
    /// Count of per-plane pollers currently alive (spawned and not yet
    /// observed to have exited). Same semantics as `OrchRuntime::
    /// live_task_count`: `0` both when nothing
    /// is registered and when the whole run has been stopped.
    pub async fn live_task_count(&self) -> usize {
        let guard = self.tasks.lock().await;
        guard.values().filter(|t| !t.handle.is_finished()).count()
    }
}

/// Send a stop signal to every currently-tracked plane poller and forget
/// them. Does not await their actual exit — same non-blocking-stop
/// discipline `OrchRuntime::stop` already established: a toggle-
/// off must not hang on however long a plane's in-flight poll takes.
async fn stop_all_plane_tasks(tasks: &PlaneTasks) {
    let mut guard = tasks.lock().await;
    for (_, task) in guard.drain() {
        let _ = task.stop_tx.send(true);
    }
}

/// One diff-and-converge pass: list currently-registered planes, start a
/// poller (a fresh [`spawn_one`] with its own stop channel) for any that
/// don't have one yet, and stop the poller for any tracked plane that no
/// longer appears in the list — deleted through the API, or a row that
/// vanished by any other means (a direct DB edit, a restore). A poller
/// found already finished on its own (defensive: [`spawn_one`]'s loop only
/// ever exits via its own stop signal today, but this keeps the map/count
/// honest even if that ever changes) is pruned the same way.
///
/// A `list_registered` failure (e.g. a transient DB error) is logged and
/// this pass is skipped entirely, leaving every currently-running poller
/// untouched — the next scan retries. This mirrors [`spawn_reconcilers`]'s
/// own handling of the same error, and means a blip in listing planes never
/// tears down pollers that were working fine.
async fn reconcile_tick(
    store: &Arc<dyn ControlPlaneStore>,
    config: &ReconcilerConfig,
    tasks: &PlaneTasks,
) {
    let planes = match store.list_registered().await {
        Ok(planes) => planes,
        Err(e) => {
            warn!(error = %e, "supervisor: failed to list registered control planes this scan; leaving currently-running pollers as-is");
            return;
        }
    };

    let current_ids: HashSet<Uuid> = planes.iter().map(|p| p.id).collect();
    let mut guard = tasks.lock().await;

    guard.retain(|id, task| {
        if task.handle.is_finished() {
            return false;
        }
        if !current_ids.contains(id) {
            info!(control_plane_id = %id, "control plane no longer registered; stopping its poller");
            let _ = task.stop_tx.send(true);
            return false;
        }
        true
    });

    for plane in planes {
        if let std::collections::hash_map::Entry::Vacant(entry) = guard.entry(plane.id) {
            let id = plane.id;
            let (stop_tx, stop_rx) = watch::channel(false);
            let handle = spawn_one(plane, Arc::clone(store), *config, Some(stop_rx));
            info!(control_plane_id = %id, "control plane registered; starting its poller");
            entry.insert(PlaneTask { handle, stop_tx });
        }
    }
}

/// The supervisor's own loop: reconcile immediately, then sleep
/// `scan_secs` (racing the global `stop_rx`) and reconcile again, forever —
/// until `stop_rx` fires, at which point every currently-tracked plane
/// poller is stopped ([`stop_all_plane_tasks`]) and this task exits. Spawned
/// detached by [`spawn_reconcilers_supervised`] (its `JoinHandle` isn't kept
/// anywhere): correctness is fully observable through
/// [`SupervisedReconciler::live_task_count`] converging to `0`, so nothing
/// needs to join this task to prove a clean shutdown, the same reasoning
/// `OrchRuntime::stop` already relies on for the per-plane tasks themselves.
async fn supervisor_loop(
    store: Arc<dyn ControlPlaneStore>,
    config: ReconcilerConfig,
    mut stop_rx: watch::Receiver<bool>,
    tasks: PlaneTasks,
) {
    let scan_secs = config.supervisor_scan_secs.max(1);
    loop {
        if *stop_rx.borrow() {
            stop_all_plane_tasks(&tasks).await;
            return;
        }

        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(scan_secs)) => {}
            _ = wait_until_stopped(&mut stop_rx) => {
                stop_all_plane_tasks(&tasks).await;
                return;
            }
        }

        if *stop_rx.borrow() {
            stop_all_plane_tasks(&tasks).await;
            return;
        }

        reconcile_tick(&store, &config, &tasks).await;
    }
}

/// Start a self-healing reconciler run: one poller per currently-registered
/// control plane, kept in sync with `control_planes` for as long as
/// `stop_rx` stays `false`.
///
/// Does an initial [`reconcile_tick`] synchronously, before returning, so a
/// caller that checks
/// [`SupervisedReconciler::live_task_count`] immediately after this
/// `.await` resolves already sees a poller for every plane registered *as
/// of now*. Everything registered *later* is the supervisor loop's job,
/// picked up within `config.supervisor_scan_secs`.
///
/// Unlike [`spawn_reconcilers`] this has no `enabled` gate of its own: the
/// caller only calls this function when it has already decided to run, so
/// "off" is simply "never call this" rather than a second flag that could
/// disagree with `stop_rx`.
pub async fn spawn_reconcilers_supervised(
    store: Arc<dyn ControlPlaneStore>,
    config: ReconcilerConfig,
    stop_rx: watch::Receiver<bool>,
) -> SupervisedReconciler {
    let tasks: PlaneTasks = Arc::new(AsyncMutex::new(HashMap::new()));

    // Mirrors spawn_one's own top-of-loop check: a `stop_rx` that's already
    // `true` when this is called (defensive — `OrchRuntime` never actually
    // does this, it always hands over a fresh `watch::channel(false)`) means
    // no poller should ever start, not even for an already-registered
    // plane.
    if !*stop_rx.borrow() {
        reconcile_tick(&store, &config, &tasks).await;
    }

    tokio::spawn(supervisor_loop(store, config, stop_rx, Arc::clone(&tasks)));

    SupervisedReconciler { tasks }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "reconciler/tests.rs"]
mod tests;
