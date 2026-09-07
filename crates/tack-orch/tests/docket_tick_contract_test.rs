//! The tick-level contract oracle — see `docs/plans/agnostic-control-plane.md`
//! §6 ("How docket is proven not to have regressed") for why this file
//! exists and exactly what it has to survive.
//!
//! # The gap this closes
//!
//! Of `ControlPlane`'s thirteen methods, only four of the 37 tests in
//! `docket_adapter_test.rs` assert what actually LEAVES the process on the
//! wire — the other 33 assert decoding only (plan §1.10). A trait reshape
//! could change what docket receives on nine methods and every existing test
//! would stay green. This file is the primary oracle any such reshape has to
//! be proved against: it drives one full reconciler tick — the fetch phase
//! AND the whole persist phase, not `reconcile_once` in isolation — against
//! a real `wiremock` docket and a real in-memory SQLite, and snapshots two
//! things golden files must stay byte-identical across:
//!
//!   (A) the ORDERED list of HTTP requests the tick issued — method, path,
//!       sorted query, header NAMES only (never values — see "Never leaks
//!       the token" below), body canonicalised;
//!   (B) the resulting rows of `orch_runs`, `orch_approvals`, `orch_events`,
//!       `orch_metrics`, and `orch_trace_cursors`, deterministically sorted,
//!       with Tack-generated volatility normalised away (see
//!       "Normalisation" below).
//!
//! # Why a secondary, per-method wire test is not enough on its own
//!
//! A per-method golden proves "given this input, `DocketAdapter::traces`
//! sends this request" — it says nothing about HOW MANY TIMES
//! `reconcile_once` calls it, or in what order relative to
//! `list_runs`/`list_approvals`. Three refactors defeat a method-level
//! golden and are caught only here (plan §6's table, reproduced by this
//! file's five scenarios):
//!   - re-scoping the poll loop to iterate active runs instead of linked
//!     projects — steady state (N linked projects, 0 active runs) then
//!     issues ZERO trace/run calls where it issued N (see
//!     `three_linked_projects_issues_three_per_project_calls_each`, and its
//!     mirror image `zero_linked_projects_issues_no_per_project_calls`,
//!     which proves the loop legitimately issues nothing when there is
//!     nothing to poll, so the request COUNT is a real signal and not just
//!     "zero because empty is the loop's only mode");
//!   - dropping `persist_events`'s retention-age guard — a rewound cursor
//!     then resurrects a row already rolled into `orch_events_daily` and
//!     purged, double-counting it on the next rollup (see
//!     `rewound_cursor_re_delivers_overlapping_events_without_resurrecting_a_purged_row`);
//!   - changing `derive_event_id`'s separator/field order/namespace — out of
//!     THIS file's scope; see the pinned-literal test in `reconciler.rs`.
//!
//! # Pattern copied, not invented
//!
//! The fetch-plus-persist-through-a-real-`spawn_reconcilers`-loop shape is
//! `tests/ingestion/runs.rs` and `tests/ingestion/traces.rs`'s, copied
//! deliberately rather than reinvented. `TestRepoStore` below is the same
//! mechanical, thin `ControlPlaneStore` impl those two files share via
//! `ingestion/support.rs` — duplicated here rather than imported from there
//! (same reasoning as `ingestion/support.rs`'s own module doc: `tack-orch`
//! must never depend on `tack-api`, so there is no single real
//! implementation to import, and each test binary stays scoped to its own
//! concerns).
//! Unlike those two files, THIS file deliberately never seeds an
//! `orch_tasks` row or an item to correlate against — correlation
//! (task_ids/context.taskId/session_id matching) is already covered there;
//! this file's only job is the wire shape and the five tables' resulting
//! rows, so every run/approval/event below lands uncorrelated (`item_id:
//! null`) on purpose. That also means no `<ITEM_ID>` placeholder is needed
//! in the normalisation scheme below.
//!
//! The `UPDATE_GOLDEN=1` / plain-diff harness shape is
//! `crates/tack-api/tests/openapi_contract.rs`'s `UPDATE_OPENAPI=1` gate,
//! read and copied deliberately (`assert_matches_golden`, below).
//!
//! # Exactly one tick, deterministically
//!
//! `spawn_one`'s loop (`reconciler.rs`) fires tick 1 immediately — there is
//! no up-front sleep — then sleeps `jittered_secs(base, ±20%)` before tick
//! 2. `run_one_tick` (below) configures `poll_secs` at 100_000, so even with
//! jitter a second tick cannot start within this test's wait window. It then
//! waits for the expected request COUNT to land, capped at
//! [`REQUEST_WAIT_CAP`] — capped, not asserted-and-panicked, so a
//! deliberately-broken store (see "Proving the oracle is real" below) lets
//! the test fall through to the golden comparison instead of dying early on
//! an opaque timeout — before a short grace sleep for the persist phase and
//! aborting the task.
//!
//! # Normalisation — what stays literal, what does not
//!
//! Golden determinism requires every value regenerated fresh to collapse to
//! the same text on every run. Two independent sources of non-determinism
//! exist here, both normalised, and nothing else is:
//!   - **Wall-clock timestamps Tack itself writes** (`created_at`,
//!     `updated_at`, `scraped_at` — all `datetime('now')` at insert time,
//!     see `tack-db/src/repo/orch.rs`'s upsert functions) collapse to the
//!     literal `<NOW>`.
//!   - **IDs Tack mints at runtime, never read from a fixture**:
//!     `control_planes.id` (`Uuid::new_v4()` in `create_control_plane`,
//!     freshly generated every test run) collapses to
//!     `<CONTROL_PLANE_ID>` wherever it appears as a foreign key;
//!     `orch_events.id` (`derive_event_id` — content-derived, but its input
//!     INCLUDES that same random `control_plane_id`, so it is exactly as
//!     volatile across runs even though it is deterministic *within* one)
//!     and `orch_metrics.id` (`Uuid::new_v4()` at insert, no natural key —
//!     see `upsert_orch_metrics`'s doc comment) collapse to ordinal
//!     placeholders (`<EVENT_ID_1>`, `<METRIC_ID_1>`, ...) assigned after
//!     sorting, so a row's POSITION and every OTHER column still fully
//!     participate in the diff.
//!
//! Everything else — `run_id`/`token` (docket's own ids, echoed verbatim by
//! the mock), `remote_project`, `state`, `source`, `started_at`/`ended_at`
//! (parsed from the fixture's `startedAt`/`finishedAt`), `requested_at`
//! (parsed from the fixture's `created`), `occurred_at` (parsed from the
//! fixture's `ts`), `payload`/`labels` (fixture content, canonicalised for
//! key order only), `cursor` (docket's own minted `next`, echoed verbatim)
//! — comes FROM the fixture and is asserted literally, unnormalised. Only
//! values Tack itself mints may be normalised; normalising a value that came
//! off the wire would blind this oracle to the exact class of change it
//! exists to catch.
//!
//! # Never leaks the token
//!
//! Every control plane below is created WITH a Bearer token
//! ([`PLANE_TOKEN`]) specifically so the authenticated/unauthenticated route
//! split (`docket.rs`'s `get_authed`/`get_unauthed`) has something real to
//! prove: the header NAME `authorization` appears in the golden for
//! `/runs`, `/approvals`, `/traces/*` and never for `/health`,
//! `/status.json`, `/metrics`. [`assert_never_leaks_token`] is a second,
//! direct check (not just "we only serialise header names, never values")
//! that the literal token string never reaches either golden file.
//!
//! # Proving the oracle is real
//!
//! Not automated here (it would defeat its own point — a permanently broken
//! store would just make this test permanently fail instead of proving
//! anything about a REGRESSION). Done once, by hand: temporarily make
//! `three_linked_projects_issues_three_per_project_calls_each` link zero
//! projects instead of three (comment out the `link_project` call inside
//! its loop) while leaving its golden-file names pointed at the real
//! (three-project) goldens, run just that test, confirm it fails with
//! `assert_eq!` printing the full committed JSON (which names `demo-b`/
//! `demo-c`'s `/runs`/`/traces` requests) against a shorter actual JSON that
//! is missing them, then revert.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::sqlite::SqlitePool;
use sqlx::{Column, Row};
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use tack_core::models::{CreateProject, Project, ProjectType};
use tack_core::vocabulary;
use tack_db::repo::orch::{
    CreateControlPlane, NewOrchApproval, NewOrchEvent, NewOrchMetric, NewOrchRun, UpsertOrchLink,
};
use tack_db::{Repository, init_pool, migrations};
use tack_orch::adapters::docket::DocketAdapter;
use tack_orch::reconciler::{
    ControlPlaneStore, HealthRecord, ReconcilerConfig, RegisteredPlane, spawn_reconcilers,
};
use tack_orch::{ControlPlane, OrchError};

// ---------------------------------------------------------------------------
// Fixture setup — mirrors ingestion/support.rs
// ---------------------------------------------------------------------------

async fn setup_repo() -> Repository {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_all(&pool).await.expect("migrations");
    Repository::new(pool)
}

async fn seed_workspace(repo: &Repository) -> Uuid {
    let id = Uuid::new_v4();
    let vocab = serde_json::to_string(&vocabulary::default_vocabulary()).unwrap();
    sqlx::query(
        "INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'Test Workspace', ?)",
    )
    .bind(id.to_string())
    .bind(&vocab)
    .execute(repo.pool())
    .await
    .expect("insert workspace");
    id
}

async fn seed_project(repo: &Repository, workspace_id: Uuid) -> Project {
    repo.create_project(
        workspace_id,
        CreateProject {
            name: "Test Project".into(),
            description: None,
            project_type: ProjectType::Software,
            template: None,
        },
    )
    .await
    .expect("create project")
}

/// Bearer token every control plane below is configured with — see the
/// module doc's "Never leaks the token".
const PLANE_TOKEN: &str = "docket-secret-do-not-leak-9f3a";

async fn seed_plane(repo: &Repository, base_url: &str) -> Uuid {
    repo.create_control_plane(CreateControlPlane {
        name: "Test Docket".into(),
        kind: None,
        base_url: base_url.to_string(),
        token: Some(PLANE_TOKEN.to_string()),
    })
    .await
    .expect("create control plane")
    .id
}

async fn link_project(repo: &Repository, project_id: Uuid, plane_id: Uuid, remote_project: &str) {
    repo.upsert_orch_link(
        project_id,
        UpsertOrchLink {
            control_plane_id: plane_id,
            remote_project: remote_project.to_string(),
            pipeline_file: None,
            blueprint: None,
            auto_dispatch: false,
            budget_usd: None,
            status_map: serde_json::json!({}),
        },
    )
    .await
    .expect("create orch link");
}

/// A [`ControlPlaneStore`] backed directly by a real `Repository` — the
/// test-only stand-in for `tack-api::orch_store::RepoControlPlaneStore` (see
/// the module doc for why this can't just import that type, and why it is
/// duplicated here rather than imported from `ingestion/support.rs`'s copy,
/// shared there by `ingestion/runs.rs` and `ingestion/traces.rs`).
struct TestRepoStore {
    repo: Repository,
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

// ---------------------------------------------------------------------------
// docket wire-body builders
// ---------------------------------------------------------------------------

const HEALTH_BODY: &str = r#"{"status":"ok","gateway":0}"#;
const STATUS_BODY: &str = r#"{"apiVersion":"2","timestamp":"2026-08-05T00:00:00Z","gateway":"inactive","channels":[],"agents":[],"totalCostUsd":0.0}"#;

async fn mount_health_status(server: &MockServer) {
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

/// One `GET /runs` list entry. `finished_at: None` renders as JSON `null`,
/// matching a still-running docket run.
fn run_json(
    id: &str,
    source: &str,
    state: &str,
    created: &str,
    finished_at: Option<&str>,
) -> String {
    let finished = match finished_at {
        Some(f) => format!("\"{f}\""),
        None => "null".to_string(),
    };
    format!(
        r#"{{"id":"{id}","source":"{source}","project":"demo","state":"{state}","taskIds":[],
        "error":"","created":"{created}","startedAt":"{created}","finishedAt":{finished},
        "pids":[],"variables":{{}}}}"#
    )
}

fn approval_json(token: &str, action: &str, created: &str) -> String {
    format!(
        r#"{{"token":"{token}","project":"demo","role":"implementer","action":"{action}","state":"pending","created":"{created}","context":{{}}}}"#
    )
}

/// docket's *real* `GET /traces/{{project}}` wire shape, verified against
/// `serve.py`'s `_traces_page`/`do_GET` (see `adapters/docket.rs`'s module
/// doc): `events` is an array of raw JSON **strings**, each independently
/// encoding one event object, not an array of objects. Copied verbatim from
/// `ingestion/traces.rs`'s own helper of the same name — see the module
/// doc's "Pattern copied, not invented".
fn traces_body(events: &[serde_json::Value], next: &str) -> String {
    let encoded: Vec<String> = events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect();
    serde_json::json!({ "events": encoded, "next": next }).to_string()
}

fn trace_event_json(
    session_id: &str,
    ts: &str,
    event_type: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "ts": ts,
        "project": "demo",
        "session_id": session_id,
        "agent_role": "lead",
        "event_type": event_type,
        "payload": payload,
        "cost_usd": 0.0021,
        "duration_ms": 842
    })
}

// ---------------------------------------------------------------------------
// Driving exactly one tick
// ---------------------------------------------------------------------------

/// Base poll interval for every scenario: large enough (even after ±20%
/// jitter — see `reconciler.rs`'s `jittered_secs`) that a second tick cannot
/// start inside this test's wait window. See the module doc's "Exactly one
/// tick, deterministically".
const TICK_POLL_SECS: u64 = 100_000;

/// Upper bound on how long [`run_one_tick`] waits for the expected request
/// count to land before giving up and snapshotting whatever arrived anyway.
/// Deliberately a cap, not a hard-panic timeout — see the module doc's
/// "Proving the oracle is real" for why a deliberately-broken store must be
/// allowed to reach the golden comparison rather than die on a timeout
/// assertion first.
const REQUEST_WAIT_CAP: Duration = Duration::from_millis(1_500);

/// Grace period after the expected request count lands, before snapshotting
/// the database. The persist phase (`record_health`, then
/// `persist_runs`/`persist_approvals`/`persist_metrics`/`persist_events`)
/// runs strictly after the fetch phase's last HTTP `.await` resolves, inside
/// the same spawned tick — see `reconciler.rs`'s module doc, "three-phase
/// shape". SQLite writes against an in-memory pool are effectively
/// instantaneous; this margin is generous, not load-bearing.
const PERSIST_GRACE: Duration = Duration::from_millis(200);

/// Registers the plane behind `repo` with `spawn_reconcilers`, lets exactly
/// one tick run, then aborts and returns every request the mock server
/// observed (matched or not — an unmatched request still belongs in the
/// wire golden, since it proves the code tried to call a route this
/// scenario didn't expect).
async fn run_one_tick(
    server: &MockServer,
    repo: Repository,
    expected_requests: usize,
) -> Vec<Request> {
    let store: Arc<dyn ControlPlaneStore> = Arc::new(TestRepoStore { repo });
    let handles = spawn_reconcilers(
        true,
        store,
        ReconcilerConfig {
            poll_secs: TICK_POLL_SECS,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        handles.len(),
        1,
        "exactly one control plane must be registered for a single observed tick"
    );

    let start = tokio::time::Instant::now();
    loop {
        let seen = server.received_requests().await.unwrap_or_default().len();
        if seen >= expected_requests || start.elapsed() >= REQUEST_WAIT_CAP {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(PERSIST_GRACE).await;

    for h in handles {
        h.abort();
    }

    server.received_requests().await.unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Golden artifact (A): the ordered request list
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct GoldenRequest {
    method: String,
    path: String,
    /// Sorted by key via `BTreeMap`'s own iteration order — deterministic
    /// regardless of whether some future dependency enables `serde_json`'s
    /// `preserve_order` feature elsewhere in the build (this crate itself
    /// never does — see `reconciler.rs`'s `derive_event_id` doc comment).
    query: BTreeMap<String, String>,
    /// Header NAMES only, lower-cased, sorted, deduplicated — never values.
    /// See the module doc's "Never leaks the token".
    headers: Vec<String>,
    body: Option<serde_json::Value>,
}

/// Recursively rebuilds a `serde_json::Value` so every object's keys are
/// sorted, independent of whatever `Map` implementation `serde_json` is
/// compiled with — see [`GoldenRequest::query`]'s doc comment for why this
/// crate does not rely on that being invariant forever.
fn canonical_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let sorted: BTreeMap<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), canonical_json(v)))
                .collect();
            serde_json::to_value(sorted).expect("re-serialize a sorted object")
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(canonical_json).collect())
        }
        other => other.clone(),
    }
}

/// The body canonicalised: parse JSON and re-serialise with sorted keys;
/// non-JSON bodies as-is. Every request this
/// file's scenarios capture is a bodyless `GET`, so this always returns
/// `None` in practice; the JSON/non-JSON branches exist so the golden format
/// does not have to change the day a future scenario captures a `POST`.
fn canonicalize_body(bytes: &[u8]) -> Option<serde_json::Value> {
    if bytes.is_empty() {
        return None;
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) => Some(canonical_json(&value)),
            Err(_) => Some(serde_json::Value::String(text.to_string())),
        },
        Err(_) => Some(serde_json::Value::String(format!(
            "<{} non-utf8 bytes>",
            bytes.len()
        ))),
    }
}

fn to_golden_request(req: &Request) -> GoldenRequest {
    let query: BTreeMap<String, String> = req
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let mut headers: Vec<String> = req
        .headers
        .keys()
        .map(|k| k.as_str().to_ascii_lowercase())
        .collect();
    headers.sort();
    headers.dedup();
    GoldenRequest {
        method: req.method.to_string(),
        path: req.url.path().to_string(),
        query,
        headers,
        body: canonicalize_body(&req.body),
    }
}

fn to_golden_requests(reqs: &[Request]) -> Vec<GoldenRequest> {
    reqs.iter().map(to_golden_request).collect()
}

// ---------------------------------------------------------------------------
// Golden artifact (B): the resulting rows
// ---------------------------------------------------------------------------

const NOW_PLACEHOLDER: &str = "<NOW>";
const CONTROL_PLANE_PLACEHOLDER: &str = "<CONTROL_PLANE_ID>";
const WALL_CLOCK_COLUMNS: [&str; 3] = ["created_at", "updated_at", "scraped_at"];

#[derive(Serialize)]
struct GoldenRows {
    orch_runs: Vec<BTreeMap<String, serde_json::Value>>,
    orch_approvals: Vec<BTreeMap<String, serde_json::Value>>,
    orch_events: Vec<BTreeMap<String, serde_json::Value>>,
    orch_metrics: Vec<BTreeMap<String, serde_json::Value>>,
    orch_trace_cursors: Vec<BTreeMap<String, serde_json::Value>>,
}

/// Dynamic `SELECT *` row -> sorted-key map, generic across all five tables.
/// Two columns need type-aware decoding rather than the `TEXT` every other
/// column in these five tables uses: `orch_metrics.value` (`REAL`, nullable
/// — see migration 025's comment on why it can't be `NOT NULL`) and
/// `orch_runs.run_attempt` (`INTEGER NOT NULL DEFAULT 1`, added by migration
/// 037's rebuild — sqlx's SQLite driver rejects decoding a
/// declared-`INTEGER` column as `Option<String>` outright rather than
/// coercing, so without this branch every scenario that touches `orch_runs`
/// panics on the first row).
async fn fetch_table(
    pool: &SqlitePool,
    sql: &'static str,
) -> Vec<BTreeMap<String, serde_json::Value>> {
    let rows = sqlx::query(sql)
        .fetch_all(pool)
        .await
        .expect("query golden snapshot table");
    rows.into_iter()
        .map(|row| {
            let mut map = BTreeMap::new();
            for col in row.columns() {
                let name = col.name();
                let value = if name == "value" {
                    row.try_get::<Option<f64>, _>(name)
                        .expect("decode REAL column")
                        .map(|v| serde_json::json!(v))
                        .unwrap_or(serde_json::Value::Null)
                } else if name == "run_attempt" {
                    row.try_get::<Option<i64>, _>(name)
                        .expect("decode INTEGER column")
                        .map(|v| serde_json::json!(v))
                        .unwrap_or(serde_json::Value::Null)
                } else {
                    row.try_get::<Option<String>, _>(name)
                        .expect("decode TEXT column")
                        .map(serde_json::Value::String)
                        .unwrap_or(serde_json::Value::Null)
                };
                map.insert(name.to_string(), value);
            }
            map
        })
        .collect()
}

fn normalize_common(row: &mut BTreeMap<String, serde_json::Value>, control_plane_id: &str) {
    for col in WALL_CLOCK_COLUMNS {
        if row.contains_key(col) {
            row.insert(
                col.to_string(),
                serde_json::Value::String(NOW_PLACEHOLDER.to_string()),
            );
        }
    }
    for value in row.values_mut() {
        if let serde_json::Value::String(s) = value
            && s.as_str() == control_plane_id
        {
            *s = CONTROL_PLANE_PLACEHOLDER.to_string();
        }
    }
}

/// Re-parses a `TEXT`-stored JSON column (`payload`, `labels`) into a nested
/// `Value`, canonicalised — for readability, and so the "body canonicalised"
/// discipline applies here too, not only to request bodies.
fn parse_json_field(row: &mut BTreeMap<String, serde_json::Value>, field: &str) {
    let parsed = match row.get(field) {
        Some(serde_json::Value::String(raw)) => serde_json::from_str::<serde_json::Value>(raw).ok(),
        _ => None,
    };
    if let Some(value) = parsed {
        row.insert(field.to_string(), canonical_json(&value));
    }
}

/// Replaces a Tack-generated id column with an ordinal placeholder — see the
/// module doc's "Normalisation". `ordinal` is the row's position after
/// sorting, so two runs of the same scenario always assign the same
/// placeholder to the same (by every OTHER column) row.
fn normalize_generated_id(
    row: &mut BTreeMap<String, serde_json::Value>,
    field: &str,
    label: &str,
    ordinal: usize,
) {
    row.insert(
        field.to_string(),
        serde_json::Value::String(format!("<{label}_{}>", ordinal + 1)),
    );
}

async fn snapshot_rows(pool: &SqlitePool, control_plane_id: Uuid) -> GoldenRows {
    let cp = control_plane_id.to_string();

    // `external_run_id`, not `run_id` — migration 037 rebuilt
    // `orch_runs` around the widened primary key `(control_plane_id,
    // external_run_id, run_attempt)` and renamed the physical column; the
    // repo layer (`tack-db/src/repo/orch.rs`) aliases it back to `run_id` in
    // its own `SELECT`s so `OrchRun`'s Rust-level shape is unchanged, but
    // this is a raw `SELECT *` against the table itself, which sees the
    // physical name. Ordering by it (rather than the full new PK) is still
    // sufficient here: every scenario in this file uses a single
    // `control_plane_id`, and every run it seeds has `run_attempt` 1 (no
    // scenario exercises retries).
    let mut orch_runs = fetch_table(pool, "SELECT * FROM orch_runs ORDER BY external_run_id").await;
    let mut orch_approvals = fetch_table(pool, "SELECT * FROM orch_approvals ORDER BY token").await;
    let mut orch_events = fetch_table(
        pool,
        "SELECT * FROM orch_events ORDER BY occurred_at, event_type",
    )
    .await;
    let mut orch_metrics =
        fetch_table(pool, "SELECT * FROM orch_metrics ORDER BY name, labels").await;
    let mut orch_trace_cursors = fetch_table(
        pool,
        "SELECT * FROM orch_trace_cursors ORDER BY remote_project",
    )
    .await;

    for row in orch_runs
        .iter_mut()
        .chain(orch_approvals.iter_mut())
        .chain(orch_events.iter_mut())
        .chain(orch_metrics.iter_mut())
        .chain(orch_trace_cursors.iter_mut())
    {
        normalize_common(row, &cp);
    }

    for (i, row) in orch_events.iter_mut().enumerate() {
        parse_json_field(row, "payload");
        normalize_generated_id(row, "id", "EVENT_ID", i);
    }
    for (i, row) in orch_metrics.iter_mut().enumerate() {
        parse_json_field(row, "labels");
        normalize_generated_id(row, "id", "METRIC_ID", i);
    }

    GoldenRows {
        orch_runs,
        orch_approvals,
        orch_events,
        orch_metrics,
        orch_trace_cursors,
    }
}

// ---------------------------------------------------------------------------
// Golden-file harness — mirrors crates/tack-api/tests/openapi_contract.rs's
// UPDATE_OPENAPI=1 gate exactly, renamed here to UPDATE_GOLDEN=1.
// ---------------------------------------------------------------------------

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/tick")
}

fn to_golden_json<T: Serialize>(value: &T) -> String {
    let mut s = serde_json::to_string_pretty(value).expect("serialize golden json");
    s.push('\n');
    s
}

fn assert_matches_golden(file_name: &str, actual: &str) {
    let path = golden_dir().join(file_name);

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_dir()).expect("create tests/golden/tick");
        std::fs::write(&path, actual).expect("write golden file");
        eprintln!("Regenerated {}", path.display());
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "could not read {} ({e}).\nGenerate it with: UPDATE_GOLDEN=1 cargo nextest run --workspace -E 'binary(docket_tick_contract_test)'",
            path.display()
        )
    });

    assert_eq!(
        committed,
        actual,
        "\n\n{} is out of date with the reconciler's observed tick behaviour.\n\
         Regenerate it with:\n    UPDATE_GOLDEN=1 cargo nextest run --workspace -E 'binary(docket_tick_contract_test)'\n",
        path.display()
    );
}

fn assert_never_leaks_token(golden_text: &str) {
    assert!(
        !golden_text.contains(PLANE_TOKEN),
        "a golden file must never contain the raw bearer token"
    );
}

// ---------------------------------------------------------------------------
// Scenario 1 — cold start: no orch_trace_cursors row exists yet
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cold_start_no_cursor_row() {
    let repo = setup_repo().await;
    let workspace_id = seed_workspace(&repo).await;
    let project = seed_project(&repo, workspace_id).await;

    let server = MockServer::start().await;
    mount_health_status(&server).await;

    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"runs":[{}]}}"#,
            run_json(
                "run-cold-1",
                "cli",
                "running",
                "2026-08-01T00:00:00+00:00",
                None
            )
        )))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"pending":[{}]}}"#,
            approval_json(
                "apr-cold-1",
                "Confirm irreversible deploy",
                "2026-08-01T00:05:00Z"
            )
        )))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/metrics"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("docket_pod_cost_usd{pod=\"demo\"} 4.5\n"),
        )
        .mount(&server)
        .await;

    // No `since` query param is expected here — that is the whole point of
    // this scenario: no orch_trace_cursors row exists yet, so poll_traces
    // must pass `since: None`, and DocketAdapter::traces must not append a
    // `since` pair to the URL at all.
    Mock::given(method("GET"))
        .and(path("/traces/demo"))
        .respond_with(ResponseTemplate::new(200).set_body_string(traces_body(
            &[trace_event_json(
                "agent:demo:dispatch",
                "2026-08-01T00:10:00Z",
                "session_start",
                serde_json::json!({"note": "cold-start"}),
            )],
            "2026-08-01T00:10:00Z:1",
        )))
        .mount(&server)
        .await;

    let plane_id = seed_plane(&repo, &server.uri()).await;
    link_project(&repo, project.id, plane_id, "demo").await;

    let requests = run_one_tick(&server, repo.clone(), 6).await;

    let requests_golden = to_golden_json(&to_golden_requests(&requests));
    assert_never_leaks_token(&requests_golden);
    assert_matches_golden("cold_start.requests.json", &requests_golden);

    let rows = snapshot_rows(repo.pool(), plane_id).await;
    let rows_golden = to_golden_json(&rows);
    assert_never_leaks_token(&rows_golden);
    assert_matches_golden("cold_start.rows.json", &rows_golden);
}

// ---------------------------------------------------------------------------
// Scenario 2 — warm cursor: a stored cursor is resumed from
// ---------------------------------------------------------------------------

#[tokio::test]
async fn warm_cursor_resumes_from_the_stored_position() {
    let repo = setup_repo().await;
    let workspace_id = seed_workspace(&repo).await;
    let project = seed_project(&repo, workspace_id).await;

    let server = MockServer::start().await;
    mount_health_status(&server).await;

    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"runs":[{}]}}"#,
            run_json(
                "run-warm-1",
                "webhook",
                "succeeded",
                "2026-08-02T00:00:00+00:00",
                Some("2026-08-02T00:00:02+00:00")
            )
        )))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"pending":[]}"#))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/metrics"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("docket_pod_cost_usd{pod=\"demo\"} 9.75\n"),
        )
        .mount(&server)
        .await;

    const STORED_CURSOR: &str = "2026-08-01T12:00:00Z:0";
    Mock::given(method("GET"))
        .and(path("/traces/demo"))
        .and(query_param("since", STORED_CURSOR))
        .respond_with(ResponseTemplate::new(200).set_body_string(traces_body(
            &[trace_event_json(
                "agent:demo:dispatch",
                "2026-08-01T12:05:00Z",
                "tool_call",
                serde_json::json!({"tool": "bash", "command": "cargo build"}),
            )],
            "2026-08-01T12:05:00Z:1",
        )))
        .mount(&server)
        .await;

    let plane_id = seed_plane(&repo, &server.uri()).await;
    link_project(&repo, project.id, plane_id, "demo").await;
    repo.set_trace_cursor(plane_id, "demo", STORED_CURSOR)
        .await
        .expect("seed a warm cursor before the observed tick");

    let requests = run_one_tick(&server, repo.clone(), 6).await;

    let requests_golden = to_golden_json(&to_golden_requests(&requests));
    assert_never_leaks_token(&requests_golden);
    assert_matches_golden("warm_cursor.requests.json", &requests_golden);

    let rows = snapshot_rows(repo.pool(), plane_id).await;
    let rows_golden = to_golden_json(&rows);
    assert_never_leaks_token(&rows_golden);
    assert_matches_golden("warm_cursor.rows.json", &rows_golden);
}

// ---------------------------------------------------------------------------
// Scenario 3 — rewound cursor: catches a dropped retention/dedup guard
// ---------------------------------------------------------------------------

/// Pre-tick state: an event already ingested, then rolled up into
/// `orch_events_daily` and purged from the raw table by a retention sweep —
/// exactly what a real deployment looks like long after the event happened.
/// The stored cursor is then set to a REWOUND value (pointing at/before that
/// already-purged event, not to wherever a well-behaved cursor would have
/// advanced to after ingesting it), and the observed tick's mock genuinely
/// re-delivers that same event content alongside a fresh one — proving the
/// scenario's overlap is real, not just a cursor number that happens to be
/// small. See `persist_events`/`derive_event_id`'s doc comments in
/// `reconciler.rs` for why a content-derived id makes resurrection possible
/// in the first place: purge deletes the row, and re-ingesting identical
/// content later derives the identical id and would insert a fresh row
/// unless the retention-age guard at ingest time stops it.
#[tokio::test]
async fn rewound_cursor_re_delivers_overlapping_events_without_resurrecting_a_purged_row() {
    let repo = setup_repo().await;
    let workspace_id = seed_workspace(&repo).await;
    let project = seed_project(&repo, workspace_id).await;

    let server = MockServer::start().await;
    let plane_id = seed_plane(&repo, &server.uri()).await;
    link_project(&repo, project.id, plane_id, "demo").await;

    const ANCIENT_TS: &str = "2020-01-01T00:00:05Z";
    let ancient_payload = serde_json::json!({"tool": "bash", "command": "echo hi"});
    repo.upsert_orch_events(
        plane_id,
        &[NewOrchEvent {
            id: Uuid::new_v4(),
            item_id: None,
            run_id: None,
            event_type: "tool_call".into(),
            payload: ancient_payload.clone(),
            occurred_at: DateTime::parse_from_rfc3339(ANCIENT_TS)
                .unwrap()
                .with_timezone(&Utc),
        }],
    )
    .await
    .expect("seed the ancient event");

    let purge_stats = repo
        .rollup_and_purge_orch_events(Utc::now(), 500)
        .await
        .expect("roll up and purge the ancient event before the observed tick");
    assert_eq!(
        purge_stats.rows_purged, 1,
        "setup bug: the ancient event must actually be purged before the tick runs"
    );

    const REWOUND_CURSOR: &str = "2020-01-01T00:00:00Z:0";
    repo.set_trace_cursor(plane_id, "demo", REWOUND_CURSOR)
        .await
        .expect("seed the rewound cursor");

    mount_health_status(&server).await;
    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"runs":[]}"#))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"pending":[]}"#))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/metrics"))
        .respond_with(ResponseTemplate::new(200).set_body_string(""))
        .mount(&server)
        .await;

    const FRESH_TS: &str = "2026-08-04T12:00:00Z";
    Mock::given(method("GET"))
        .and(path("/traces/demo"))
        .and(query_param("since", REWOUND_CURSOR))
        .respond_with(ResponseTemplate::new(200).set_body_string(traces_body(
            &[
                // The overlapping re-delivery: byte-identical to the event
                // that was already purged above.
                trace_event_json(
                    "agent:demo:dispatch",
                    ANCIENT_TS,
                    "tool_call",
                    ancient_payload.clone(),
                ),
                // A genuinely new event in the same page, proving the guard
                // drops only the stale one, not the whole batch.
                trace_event_json(
                    "agent:demo:dispatch",
                    FRESH_TS,
                    "tool_call",
                    serde_json::json!({"tool": "bash", "command": "cargo test"}),
                ),
            ],
            "2026-08-04T12:00:00Z:1",
        )))
        .mount(&server)
        .await;

    let requests = run_one_tick(&server, repo.clone(), 6).await;

    let requests_golden = to_golden_json(&to_golden_requests(&requests));
    assert_never_leaks_token(&requests_golden);
    assert_matches_golden("rewound_cursor.requests.json", &requests_golden);

    // The regression this scenario exists to catch, asserted directly and
    // not only through the golden diff: dropping persist_events's
    // retention-age guard would resurrect the purged row, and this table
    // would show 2 rows instead of 1.
    let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_events")
        .fetch_one(repo.pool())
        .await
        .expect("count orch_events rows");
    assert_eq!(
        event_count, 1,
        "the already-purged event must not be resurrected; only the fresh one may land"
    );
    let daily_count: i64 =
        sqlx::query_scalar("SELECT event_count FROM orch_events_daily WHERE control_plane_id = ?")
            .bind(plane_id.to_string())
            .fetch_one(repo.pool())
            .await
            .expect("read the daily rollup aggregate");
    assert_eq!(
        daily_count, 1,
        "the daily aggregate must not be double-counted by a resurrected raw row"
    );

    let rows = snapshot_rows(repo.pool(), plane_id).await;
    let rows_golden = to_golden_json(&rows);
    assert_never_leaks_token(&rows_golden);
    assert_matches_golden("rewound_cursor.rows.json", &rows_golden);
}

// ---------------------------------------------------------------------------
// Scenario 4 — a plane with zero linked projects
// ---------------------------------------------------------------------------

#[tokio::test]
async fn zero_linked_projects_issues_no_per_project_calls() {
    let repo = setup_repo().await;

    let server = MockServer::start().await;
    mount_health_status(&server).await;
    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"pending":[]}"#))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/metrics"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("docket_pod_cost_usd{pod=\"unlinked\"} 1.0\n"),
        )
        .mount(&server)
        .await;
    // Deliberately no /runs or /traces/* mock: a plane with zero linked
    // projects must never call either — poll_runs/poll_traces both loop
    // over `projects` and issue zero calls for an empty slice. If a future
    // refactor iterates something other than "this plane's linked
    // projects", an unmounted request here surfaces as an unmatched
    // (still-recorded) request in the captured requests, and the golden
    // diff catches it immediately.

    let plane_id = seed_plane(&repo, &server.uri()).await;
    // No project is linked to this plane at all.

    let requests = run_one_tick(&server, repo.clone(), 4).await;

    let requests_golden = to_golden_json(&to_golden_requests(&requests));
    assert_never_leaks_token(&requests_golden);
    assert_matches_golden("zero_projects.requests.json", &requests_golden);

    let rows = snapshot_rows(repo.pool(), plane_id).await;
    let rows_golden = to_golden_json(&rows);
    assert_never_leaks_token(&rows_golden);
    assert_matches_golden("zero_projects.rows.json", &rows_golden);
}

// ---------------------------------------------------------------------------
// Scenario 5 — a plane with three linked projects
// ---------------------------------------------------------------------------

/// The scenario that catches a re-scoped poll loop: with 3 linked projects
/// and 0 active runs (the steady state), the tick must issue three
/// `/runs?project=` calls and three `/traces/{project}` calls. A refactor
/// that iterates active runs instead of linked projects would issue zero of
/// each — see the module doc's "Why a secondary, per-method wire test is not
/// enough on its own".
#[tokio::test]
async fn three_linked_projects_issues_three_per_project_calls_each() {
    let repo = setup_repo().await;
    let workspace_id = seed_workspace(&repo).await;

    let server = MockServer::start().await;
    mount_health_status(&server).await;
    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"pending":[]}"#))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/metrics"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("docket_pod_cost_usd{pod=\"fleet\"} 3.0\n"),
        )
        .mount(&server)
        .await;

    let plane_id = seed_plane(&repo, &server.uri()).await;

    // Alphabetical so this test's expected request order matches
    // `list_orch_links_for_plane`'s own `ORDER BY remote_project` — see that
    // function's doc comment in tack-db/src/repo/orch.rs.
    let remote_projects = ["demo-a", "demo-b", "demo-c"];
    for (i, remote) in remote_projects.into_iter().enumerate() {
        let project = seed_project(&repo, workspace_id).await;
        link_project(&repo, project.id, plane_id, remote).await;

        Mock::given(method("GET"))
            .and(path("/runs"))
            .and(query_param("project", remote))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"runs":[]}"#))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/traces/{remote}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(traces_body(&[], &format!("cursor-{}", i + 1))),
            )
            .mount(&server)
            .await;
    }

    let requests = run_one_tick(&server, repo.clone(), 10).await;

    // Asserted directly, not only through the golden diff — see this
    // function's doc comment for the exact regression this guards.
    let runs_calls = requests.iter().filter(|r| r.url.path() == "/runs").count();
    let traces_calls = requests
        .iter()
        .filter(|r| r.url.path().starts_with("/traces/"))
        .count();
    assert_eq!(runs_calls, 3, "one /runs call per linked project");
    assert_eq!(traces_calls, 3, "one /traces call per linked project");

    let requests_golden = to_golden_json(&to_golden_requests(&requests));
    assert_never_leaks_token(&requests_golden);
    assert_matches_golden("three_projects.requests.json", &requests_golden);

    let rows = snapshot_rows(repo.pool(), plane_id).await;
    let rows_golden = to_golden_json(&rows);
    assert_never_leaks_token(&rows_golden);
    assert_matches_golden("three_projects.rows.json", &rows_golden);
}
