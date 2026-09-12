//! Trace ingestion against a real `Repository`, a real `DocketAdapter`, and
//! the real reconciler loop — proving two things the reconciler's own
//! fake-store unit tests cannot: idempotent re-polling through the real
//! `orch_events` table, and that a re-ingested, already-purged event is
//! never resurrected or double-counted by a later rollup.
//!
//! `TestRepoStore` and the polling helpers live in `support.rs`, shared
//! with `runs.rs`.

use chrono::Utc;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use tack_db::Repository;
use tack_db::repo::orch::{OrchEventDailyAggregate, RollupStats};
use tack_orch::reconciler::DEFAULT_RETENTION_DAYS;

use crate::common::setup_test_db;
use crate::support::{
    fast_poll_config, last_seen_at, mount_health_and_status, orch_event_count, plane_health,
    poll_until, run_one_more_tick, seed_control_plane_and_link, seed_project_with_pending_task,
    spawn_reconciler, stop_reconciler, wait_and_stop,
};

async fn rollup_and_purge(repo: &Repository) -> RollupStats {
    repo.rollup_and_purge_orch_events(Utc::now(), 500)
        .await
        .expect("rollup and purge")
}

async fn daily_events(repo: &Repository, control_plane_id: Uuid) -> Vec<OrchEventDailyAggregate> {
    repo.list_orch_events_daily(control_plane_id)
        .await
        .expect("list daily aggregate")
}

/// Fetches the events attributed to `item_id`, asserting there's exactly
/// one, and returns its `event_type` — every caller here goes on to check
/// that type, and this is the only test asserting correlation narrows to a
/// single event rather than mirroring every fetched event onto the item.
async fn expect_one_event_type(repo: &Repository, item_id: Uuid) -> String {
    let events = repo
        .list_orch_events_for_item(item_id, None)
        .await
        .expect("list events for item");
    assert_eq!(
        events.len(),
        1,
        "only the correlated event attributes to the item"
    );
    events[0].event_type.clone()
}

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

/// Builds docket's real wire shape for `GET /traces/{project}` (see
/// `adapters/docket.rs`'s module doc): `events` is an array of raw JSON
/// **strings**, each independently encoding one event object, not an array
/// of objects. Every mock body in this file goes through this helper
/// specifically so a regression to the old (wrong) shape fails here too.
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
async fn overlapping_polls_correlate_once_and_never_duplicate() {
    let repo = setup_test_db().await;
    let fixture = seed_project_with_pending_task(&repo).await;

    let server = MockServer::start().await;
    mount_common(&server).await;
    let events = vec![
        trace_event_json("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call"),
        trace_event_json(
            "agent:demo:dispatch",
            "2026-08-04T19:52:40Z",
            "session_start",
        ),
    ];
    // Ignores `since` entirely, so the rewound cursor below re-fetches the
    // identical overlapping window.
    Mock::given(method("GET"))
        .and(path("/traces/demo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(traces_body(&events, "2026-08-04T19:52:40Z:1")),
        )
        .mount(&server)
        .await;

    let control_plane_id =
        seed_control_plane_and_link(&repo, fixture.project.id, &server.uri()).await;
    let config = fast_poll_config(DEFAULT_RETENTION_DAYS);
    let handles = spawn_reconciler(&repo, config).await;
    poll_until("both trace events land", || async {
        orch_event_count(&repo).await == 2
    })
    .await;
    let landed_at = last_seen_at(&repo, control_plane_id).await;
    wait_and_stop(&repo, control_plane_id, landed_at, handles).await;

    assert_eq!(
        orch_event_count(&repo).await,
        2,
        "repeated overlapping polls must not duplicate orch_events rows"
    );
    assert_eq!(
        expect_one_event_type(&repo, fixture.item.id).await,
        "tool_call"
    );

    // Deliberately rewind the cursor and re-poll.
    repo.set_trace_cursor(control_plane_id, "demo", "")
        .await
        .expect("rewind cursor");
    let before_repoll = last_seen_at(&repo, control_plane_id).await;
    run_one_more_tick(&repo, control_plane_id, before_repoll, config).await;

    assert_eq!(
        orch_event_count(&repo).await,
        2,
        "a rewound cursor re-ingesting an overlapping window must add zero rows"
    );
    assert_eq!(plane_health(&repo, control_plane_id).await, "healthy");
}

#[tokio::test]
async fn purged_trace_events_are_never_resurrected_or_recounted() {
    let repo = setup_test_db().await;
    let fixture = seed_project_with_pending_task(&repo).await;

    let server = MockServer::start().await;
    mount_common(&server).await;
    // Deliberately ancient: what a badly-rewound cursor would re-deliver
    // long after a retention sweep already rolled it up and purged it.
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

    let control_plane_id =
        seed_control_plane_and_link(&repo, fixture.project.id, &server.uri()).await;

    // Phase 1: ingest with a retention window wide enough that the 2020
    // timestamp isn't filtered at ingest time.
    let handles = spawn_reconciler(&repo, fast_poll_config(36_500)).await;
    poll_until("the stale event lands", || async {
        orch_event_count(&repo).await == 1
    })
    .await;
    stop_reconciler(handles).await;

    // Phase 2: roll it up and purge it, simulating a retention sweep that
    // already ran past this event's age.
    assert_eq!(rollup_and_purge(&repo).await.rows_purged, 1);
    assert_eq!(orch_event_count(&repo).await, 0);
    let daily = daily_events(&repo, control_plane_id).await;
    assert_eq!(daily[0].event_count, 1, "rolled up exactly once");

    // Phase 3: re-poll with a realistic retention window. The mock ignores
    // `since`, standing in for a rewound/lost cursor; the same stale event
    // comes back and must not be resurrected as a raw row.
    repo.set_trace_cursor(control_plane_id, "demo", "")
        .await
        .expect("rewind cursor");
    let since = last_seen_at(&repo, control_plane_id).await;
    run_one_more_tick(&repo, control_plane_id, since, fast_poll_config(90)).await;

    assert_eq!(
        orch_event_count(&repo).await,
        0,
        "an already-purged, now-stale event must not be resurrected"
    );
    assert_eq!(rollup_and_purge(&repo).await.rows_purged, 0);
    let daily_after = daily_events(&repo, control_plane_id).await;
    assert_eq!(
        daily_after[0].event_count, 1,
        "re-ingesting a purged event must never double-count its daily aggregate"
    );
}
