//! End-to-end integration tests for trace ingestion: a real
//! `tack_db::Repository` (in-memory SQLite, real migrations),
//! a real `DocketAdapter` pointed at a `wiremock` stand-in for docket, and
//! the real `reconciler::spawn_reconcilers` loop.
//!
//! This file focuses on exactly the two things `reconciler.rs`'s own
//! `#[cfg(test)]` unit tests (against a `FakeStore`) cannot prove, because
//! that fake is a plain in-memory `Vec` with no `ON CONFLICT` semantics:
//!
//!   1. **Row-count idempotency through the real `orch_events` table** —
//!      re-polling an overlapping cursor window, including a deliberately
//!      rewound cursor, must produce **zero** new rows, which only a real
//!      `ON CONFLICT(id) DO UPDATE` table can actually demonstrate.
//!   2. **Retention composition** — an event re-ingested after its row was
//!      already rolled up into `orch_events_daily` and purged must not
//!      resurrect a raw row that then gets double-counted by a later
//!      rollup. This needs the real repo-layer rollup function
//!      (`Repository::rollup_and_purge_orch_events`) alongside real
//!      ingestion — exercising both together is the whole point.
//!
//! `TestRepoStore` lives in `support.rs`, shared with `runs.rs`. The pool and
//! workspace/project/item seed come from `crate::common`.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use tack_db::repo::orch::NewOrchTask;
use tack_orch::reconciler::{ControlPlaneStore, ReconcilerConfig, spawn_reconcilers};

use crate::common::{create_test_workspace, make_item, make_project, setup_test_db};
use crate::support::{TestRepoStore, mount_health_and_status, seed_control_plane_and_link};

const EMPTY_RUNS_BODY: &str = r#"{"runs":[]}"#;
const EMPTY_APPROVALS_BODY: &str = r#"{"pending":[]}"#;

/// This file doesn't exercise runs/approvals, but a linked project makes
/// `poll_runs` fire too — mock both empty on top of the shared health/status
/// pair so they never error this file's health assertions or logs.
async fn mount_common(server: &MockServer) {
    mount_health_and_status(server).await;
    Mock::given(method("GET"))
        .and(path("/runs"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_RUNS_BODY))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_APPROVALS_BODY))
        .mount(server)
        .await;
}

/// Builds docket's *real* wire shape for `GET /traces/{project}` — verified
/// against `serve.py`'s `_traces_page`/`do_GET` (see
/// `adapters/docket.rs`'s module doc): `events` is an array of raw JSON
/// **strings**, each independently encoding one event object, not an array
/// of objects. Every fixture/mock body in this file goes through this
/// helper specifically so a regression back to the old (wrong) shape would
/// fail this file's tests, not just `docket_adapter_test.rs`'s.
fn traces_body(events: &[serde_json::Value], next: &str) -> String {
    let encoded: Vec<String> = events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect();
    serde_json::json!({ "events": encoded, "next": next }).to_string()
}

fn trace_event_json(session_id: &str, ts: &str, event_type: &str) -> serde_json::Value {
    serde_json::json!({
        "ts": ts,
        "project": "demo",
        "session_id": session_id,
        "agent_role": "lead",
        "event_type": event_type,
        "payload": {"tool": "bash", "command": "cargo test -p tack-orch"},
        "cost_usd": 0.0021,
        "duration_ms": 842
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_correlated_and_uncorrelated_trace_event_mirror_and_re_polling_is_idempotent() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;

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

    let server = MockServer::start().await;
    mount_common(&server).await;

    // Recent timestamps (within the default 90-day retention window) so
    // persist_events' age filter never interferes with this test — that's
    // covered separately below.
    let events = vec![
        trace_event_json("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call"),
        trace_event_json(
            "agent:demo:dispatch",
            "2026-08-04T19:52:40Z",
            "session_start",
        ),
    ];
    // Matches *any* /traces/demo request regardless of `since` — every poll
    // (including a rewound one) sees the identical overlapping window,
    // which is exactly the scenario this test is proving is safe.
    Mock::given(method("GET"))
        .and(path("/traces/demo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(traces_body(&events, "2026-08-04T19:52:40Z:1")),
        )
        .mount(&server)
        .await;

    let control_plane_id = seed_control_plane_and_link(&repo, project.id, &server.uri()).await;

    let store: Arc<dyn ControlPlaneStore> = Arc::new(TestRepoStore { repo: repo.clone() });
    let handles = spawn_reconcilers(
        true,
        store,
        ReconcilerConfig {
            poll_secs: 1,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(handles.len(), 1);

    // Several ticks (poll_secs=1, jittered 0.8-1.2s) — each one re-fetches
    // the same overlapping window from the mock above.
    tokio::time::sleep(Duration::from_millis(2_600)).await;
    for h in handles {
        h.abort();
    }

    let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_events")
        .fetch_one(repo.pool())
        .await
        .expect("count events");
    assert_eq!(
        event_count, 2,
        "repeated overlapping polls must not duplicate orch_events rows"
    );

    let events_for_item = repo
        .list_orch_events_for_item(item.id, None)
        .await
        .expect("list events for item");
    assert_eq!(
        events_for_item.len(),
        1,
        "only the correlated event attributes to the item"
    );
    assert_eq!(events_for_item[0].event_type, "tool_call");

    // -- Now deliberately rewind the cursor and re-poll. -----------------
    repo.set_trace_cursor(control_plane_id, "demo", "")
        .await
        .expect("rewind cursor");

    let store: Arc<dyn ControlPlaneStore> = Arc::new(TestRepoStore { repo: repo.clone() });
    let handles = spawn_reconcilers(
        true,
        store,
        ReconcilerConfig {
            poll_secs: 1,
            ..Default::default()
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(1_400)).await;
    for h in handles {
        h.abort();
    }

    let event_count_after_rewind: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_events")
        .fetch_one(repo.pool())
        .await
        .expect("count events after rewind");
    assert_eq!(
        event_count_after_rewind, 2,
        "a deliberately rewound cursor re-ingesting an overlapping window must \
         produce zero new rows"
    );

    let plane = repo
        .get_control_plane(control_plane_id)
        .await
        .expect("get control plane");
    assert_eq!(plane.health, "healthy");
}

#[tokio::test]
async fn retention_composition_re_ingesting_a_purged_event_does_not_double_count() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;

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

    let server = MockServer::start().await;
    mount_common(&server).await;

    // Deliberately ancient — this is the event a real, badly-rewound cursor
    // would re-deliver long after a retention sweep has already rolled it
    // up and purged it.
    let stale_ts = "2020-01-01T00:00:05Z";
    let events = vec![trace_event_json("agent:demo:task-1", stale_ts, "tool_call")];
    Mock::given(method("GET"))
        .and(path("/traces/demo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(traces_body(&events, &format!("{stale_ts}:1"))),
        )
        .mount(&server)
        .await;

    let control_plane_id = seed_control_plane_and_link(&repo, project.id, &server.uri()).await;

    // Phase 1: ingest with a retention window wide enough that the 2020
    // timestamp is not filtered at ingest time.
    let store: Arc<dyn ControlPlaneStore> = Arc::new(TestRepoStore { repo: repo.clone() });
    let handles = spawn_reconcilers(
        true,
        store,
        ReconcilerConfig {
            poll_secs: 1,
            event_retention_days: 36_500, // ~100 years — never filters this fixture
            ..Default::default()
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    for h in handles {
        h.abort();
    }

    let count_after_ingest: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_events")
        .fetch_one(repo.pool())
        .await
        .expect("count events");
    assert_eq!(
        count_after_ingest, 1,
        "the stale-but-not-yet-retained event must land"
    );

    // Phase 2: roll it up and purge it — simulating the retention sweep
    // having already run past this event's age.
    let cutoff = Utc::now();
    let stats = repo
        .rollup_and_purge_orch_events(cutoff, 500)
        .await
        .expect("rollup and purge");
    assert_eq!(stats.rows_purged, 1);

    let count_after_purge: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_events")
        .fetch_one(repo.pool())
        .await
        .expect("count events after purge");
    assert_eq!(count_after_purge, 0);

    let daily = repo
        .list_orch_events_daily(control_plane_id)
        .await
        .expect("list daily aggregate");
    assert_eq!(daily.len(), 1);
    assert_eq!(daily[0].event_count, 1, "rolled up exactly once");

    // Phase 3: re-poll with a *real* (small) retention window — the same
    // stale event comes back from the mock (the cursor was advanced past
    // it, but this mock ignores `since` entirely, standing in for a
    // rewound/lost cursor exactly as the previous test does explicitly).
    // persist_events' retention-age guard must refuse to resurrect it.
    repo.set_trace_cursor(control_plane_id, "demo", "")
        .await
        .expect("rewind cursor");

    let store: Arc<dyn ControlPlaneStore> = Arc::new(TestRepoStore { repo: repo.clone() });
    let handles = spawn_reconcilers(
        true,
        store,
        ReconcilerConfig {
            poll_secs: 1,
            event_retention_days: 90, // realistic default — the 2020 event is well past this
            ..Default::default()
        },
    )
    .await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    for h in handles {
        h.abort();
    }

    let count_after_reingest: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_events")
        .fetch_one(repo.pool())
        .await
        .expect("count events after re-ingest attempt");
    assert_eq!(
        count_after_reingest, 0,
        "an already-purged, now-stale event must not be resurrected as a raw row"
    );

    // And critically: re-running the rollup must not find (and thus not
    // double-count) anything — the daily total stays at 1.
    let stats_second_sweep = repo
        .rollup_and_purge_orch_events(Utc::now(), 500)
        .await
        .expect("second rollup");
    assert_eq!(stats_second_sweep.rows_purged, 0);

    let daily_after = repo
        .list_orch_events_daily(control_plane_id)
        .await
        .expect("list daily aggregate again");
    assert_eq!(daily_after.len(), 1);
    assert_eq!(
        daily_after[0].event_count, 1,
        "re-ingesting a purged event must never double-count its daily aggregate"
    );
}
