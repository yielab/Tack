//! End-to-end integration tests for runs + approvals ingestion: a real
//! `tack_db::Repository` (in-memory
//! SQLite, real migrations), a real `DocketAdapter` pointed at a `wiremock`
//! stand-in for docket, and the real `reconciler::spawn_reconcilers` loop —
//! proving the whole chain (fetch → correlate → persist) composes
//! correctly, not just that each piece compiles against the others' types.
//!
//! `TestRepoStore` lives in `support.rs`, shared with `traces.rs` — see that
//! module's doc for why it can't just be the real
//! `tack-api::orch_store::RepoControlPlaneStore`. The pool and
//! workspace/project/item seed come from `crate::common`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::Utc;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use tack_db::repo::orch::NewOrchTask;
use tack_orch::reconciler::{ControlPlaneStore, ReconcilerConfig, spawn_reconcilers};

use crate::common::{create_test_workspace, make_item, make_project, setup_test_db};
use crate::support::{
    EMPTY_APPROVALS_BODY, TestRepoStore, mount_health_and_status as mount_common,
    seed_control_plane_and_link,
};

fn run_json(id: &str, task_ids: &str) -> String {
    format!(
        r#"{{"id":"{id}","source":"cli","project":"demo","state":"succeeded","taskIds":{task_ids},
        "error":"","created":"2026-08-04T19:50:43.129083+00:00",
        "startedAt":"2026-08-04T19:50:43.129674+00:00",
        "finishedAt":"2026-08-04T19:50:43.130194+00:00","pids":[],"variables":{{}}}}"#
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn correlated_and_uncorrelated_runs_and_approvals_mirror_idempotently() {
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

    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"runs":[{},{}]}}"#,
            run_json("run-1", r#"["task-1"]"#),
            run_json("run-cli-only", "[]"),
        )))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"pending":[
                {"token":"apr-1","project":"demo","role":"implementer","action":"gate",
                 "state":"pending","created":"2026-08-04T19:50:50Z",
                 "context":{"taskId":"task-1","pipelineIndex":0}},
                {"token":"apr-uncorrelated","project":"demo","role":"implementer","action":"gate",
                 "state":"pending","created":"2026-08-04T19:50:51Z","context":{}}
            ]}"#,
        ))
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

    // One tick is enough for the first poll's correlation to land.
    tokio::time::sleep(Duration::from_millis(400)).await;

    let run1 = repo
        .get_orch_run("run-1")
        .await
        .expect("query")
        .expect("run-1 must be mirrored");
    assert_eq!(run1.item_id, Some(item.id), "run-1's task_ids correlate");

    let run_cli = repo
        .get_orch_run("run-cli-only")
        .await
        .expect("query")
        .expect("run-cli-only must still be mirrored, unattributed");
    assert_eq!(
        run_cli.item_id, None,
        "an empty task_ids run must land unattributed, not be dropped or error"
    );

    let apr1 = repo
        .get_orch_approval("apr-1")
        .await
        .expect("query")
        .expect("apr-1 must be mirrored");
    assert_eq!(apr1.item_id, Some(item.id));
    assert_eq!(apr1.remote_task_id.as_deref(), Some("task-1"));

    let apr_uncorrelated = repo
        .get_orch_approval("apr-uncorrelated")
        .await
        .expect("query")
        .expect("an uncorrelated approval must still surface, not be dropped");
    assert_eq!(apr_uncorrelated.item_id, None);

    // Let at least one more tick happen (poll_secs=1, jittered 0.8–1.2s) and
    // confirm re-polling the exact same docket state is idempotent: no
    // duplicate rows, same content.
    tokio::time::sleep(Duration::from_millis(1_400)).await;
    for h in handles {
        h.abort();
    }

    let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_runs")
        .fetch_one(repo.pool())
        .await
        .expect("count runs");
    assert_eq!(run_count, 2, "re-polling must not duplicate orch_runs rows");

    let approval_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_approvals")
        .fetch_one(repo.pool())
        .await
        .expect("count approvals");
    assert_eq!(
        approval_count, 2,
        "re-polling must not duplicate orch_approvals rows"
    );

    // Control-plane health must still read healthy — an /approvals or /runs
    // failure never happened here, but this also proves the ingestion
    // machinery didn't somehow interfere with the health persistence path.
    let plane = repo
        .get_control_plane(control_plane_id)
        .await
        .expect("get control plane");
    assert_eq!(plane.health, "healthy");
}

/// A `Respond` impl that returns a different body on each successive call,
/// repeating the last body forever once the list is exhausted — used to
/// simulate a run's `taskIds` becoming known and then (unrealistically, but
/// this is exactly the case the DB-layer COALESCE guards against) reverting
/// to unknown on a later poll.
struct SequentialBody {
    bodies: Vec<String>,
    calls: AtomicUsize,
}

impl Respond for SequentialBody {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let idx = self.calls.fetch_add(1, Ordering::SeqCst);
        let body = self
            .bodies
            .get(idx)
            .or_else(|| self.bodies.last())
            .cloned()
            .unwrap_or_default();
        ResponseTemplate::new(200).set_body_string(body)
    }
}

#[tokio::test]
async fn a_later_poll_does_not_erase_an_earlier_run_attribution() {
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
    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_APPROVALS_BODY))
        .mount(&server)
        .await;

    // First poll: task_ids known, correlates. Every poll after: task_ids
    // empty again — simulating a poll that "forgot" the attribution. The
    // repo's ON CONFLICT ... COALESCE(excluded.item_id, item_id) must keep
    // the first poll's attribution regardless.
    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo"))
        .respond_with(SequentialBody {
            bodies: vec![format!(
                r#"{{"runs":[{}]}}"#,
                run_json("run-1", r#"["task-1"]"#)
            )],
            calls: AtomicUsize::new(0),
        })
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!(r#"{{"runs":[{}]}}"#, run_json("run-1", "[]"))),
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

    // First tick: correlated.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let run_after_first_poll = repo
        .get_orch_run("run-1")
        .await
        .expect("query")
        .expect("run-1 mirrored");
    assert_eq!(run_after_first_poll.item_id, Some(item.id));

    // Two more ticks, each returning an empty task_ids list.
    tokio::time::sleep(Duration::from_millis(2_400)).await;
    for h in handles {
        h.abort();
    }

    let run_after_later_polls = repo
        .get_orch_run("run-1")
        .await
        .expect("query")
        .expect("run-1 still mirrored");
    assert_eq!(
        run_after_later_polls.item_id,
        Some(item.id),
        "a later poll that doesn't know the attribution must never erase one already learned"
    );

    let _ = control_plane_id;
}
