//! Integration tests for the `orch` repository module: CRUD for
//! `control_planes` and `orch_links`, plus batch upsert helpers for
//! `orch_tasks`, `orch_runs`, `orch_events`, `orch_approvals`. Covers: the
//! control-plane read DTO never carries the stored token, including
//! serialized; batch upserts of N rows are idempotent; unrecognised
//! remote-state strings are stored and returned as-is.

use crate::common::{create_test_workspace, make_item, make_project, setup_test_db};
use chrono::Utc;
use tack_db::repo::orch::{
    CreateControlPlane, NewOrchApproval, NewOrchEvent, NewOrchRun, NewOrchTask, UpdateControlPlane,
    UpsertOrchLink,
};
use uuid::Uuid;

// ════════════════════════════════════════════════════════════════════════════════════
// control_planes
// ════════════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_get_list_control_plane() {
    let repo = setup_test_db().await;

    let created = repo
        .create_control_plane(CreateControlPlane {
            name: "Primary Docket".into(),
            kind: None,
            base_url: "http://localhost:9999".into(),
            token: Some("super-secret-token".into()),
        })
        .await
        .expect("create control plane");

    assert_eq!(created.kind, "docket", "kind defaults to docket");
    assert_eq!(created.health, "unknown");
    assert!(
        created.token_set,
        "token_set must be true once a token is stored"
    );

    let fetched = repo
        .get_control_plane(created.id)
        .await
        .expect("get control plane");
    assert_eq!(fetched.name, "Primary Docket");
    assert!(fetched.token_set);

    let listed = repo.list_control_planes().await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, created.id);
}

#[tokio::test]
async fn control_plane_without_token_reports_token_set_false() {
    let repo = setup_test_db().await;

    let created = repo
        .create_control_plane(CreateControlPlane {
            name: "No Token Plane".into(),
            kind: Some("docket".into()),
            base_url: "http://localhost:9999".into(),
            token: None,
        })
        .await
        .expect("create control plane");

    assert!(!created.token_set);
    assert_eq!(
        repo.get_control_plane_token(created.id)
            .await
            .expect("get token"),
        None
    );
}

/// Non-negotiable #2: the read DTO must never carry the token, in Rust or on the wire.
#[tokio::test]
async fn control_plane_read_dto_never_exposes_token() {
    let repo = setup_test_db().await;

    let created = repo
        .create_control_plane(CreateControlPlane {
            name: "Secret Plane".into(),
            kind: None,
            base_url: "http://localhost:9999".into(),
            token: Some("do-not-leak-me".into()),
        })
        .await
        .expect("create control plane");

    let fetched = repo
        .get_control_plane(created.id)
        .await
        .expect("get control plane");

    // The struct itself has no field to hold a token — this is a compile-time
    // guarantee, not just a runtime one (no `.token` field exists on `ControlPlane`).
    // The runtime check below asserts the *serialized* form is equally clean.
    let json = serde_json::to_string(&fetched).expect("serialize control plane");
    assert!(
        !json.contains("do-not-leak-me"),
        "serialized control plane must not contain the raw token value: {json}"
    );
    assert!(
        !json.contains("\"token\""),
        "serialized control plane must not contain a `token` key: {json}"
    );
    assert!(
        json.contains("\"token_set\":true"),
        "serialized control plane must expose token_set: {json}"
    );

    // The internal-only accessor is the sole way to retrieve the real value.
    let real_token = repo
        .get_control_plane_token(created.id)
        .await
        .expect("get_control_plane_token");
    assert_eq!(real_token.as_deref(), Some("do-not-leak-me"));
}

#[tokio::test]
async fn update_control_plane_name_and_base_url() {
    let repo = setup_test_db().await;
    let created = repo
        .create_control_plane(CreateControlPlane {
            name: "Old Name".into(),
            kind: None,
            base_url: "http://old:9999".into(),
            token: None,
        })
        .await
        .expect("create");

    let updated = repo
        .update_control_plane(
            created.id,
            UpdateControlPlane {
                name: Some("New Name".into()),
                base_url: Some("http://new:9999".into()),
                token: None,
            },
        )
        .await
        .expect("update");

    assert_eq!(updated.name, "New Name");
    assert_eq!(updated.base_url, "http://new:9999");
    assert!(!updated.token_set, "token untouched, was never set");
}

#[tokio::test]
async fn update_control_plane_token_set_then_clear() {
    let repo = setup_test_db().await;
    let created = repo
        .create_control_plane(CreateControlPlane {
            name: "Plane".into(),
            kind: None,
            base_url: "http://localhost:9999".into(),
            token: None,
        })
        .await
        .expect("create");
    assert!(!created.token_set);

    // Absent `token` field (None) preserves the stored value (still unset).
    let unchanged = repo
        .update_control_plane(created.id, UpdateControlPlane::default())
        .await
        .expect("no-op update");
    assert!(!unchanged.token_set);

    // Some(Some(t)) sets it.
    let with_token = repo
        .update_control_plane(
            created.id,
            UpdateControlPlane {
                token: Some(Some("fresh-token".into())),
                ..Default::default()
            },
        )
        .await
        .expect("set token");
    assert!(with_token.token_set);
    assert_eq!(
        repo.get_control_plane_token(created.id).await.unwrap(),
        Some("fresh-token".to_string())
    );

    // Some(None) explicitly clears it.
    let cleared = repo
        .update_control_plane(
            created.id,
            UpdateControlPlane {
                token: Some(None),
                ..Default::default()
            },
        )
        .await
        .expect("clear token");
    assert!(!cleared.token_set);
    assert_eq!(
        repo.get_control_plane_token(created.id).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn update_control_plane_health_state_machine_persists() {
    let repo = setup_test_db().await;
    let created = repo
        .create_control_plane(CreateControlPlane {
            name: "Plane".into(),
            kind: None,
            base_url: "http://localhost:9999".into(),
            token: None,
        })
        .await
        .expect("create");

    let seen_at = Utc::now();
    repo.update_control_plane_health(created.id, "degraded", None, 3, None)
        .await
        .expect("record failed poll");

    let after_failure = repo.get_control_plane(created.id).await.unwrap();
    assert_eq!(after_failure.health, "degraded");
    assert_eq!(after_failure.consecutive_failures, 3);
    // last_seen_at untouched by a failed poll (we passed None).
    assert_eq!(after_failure.last_seen_at, None);

    repo.update_control_plane_health(created.id, "healthy", Some(seen_at), 0, Some("2"))
        .await
        .expect("record recovery");

    let after_recovery = repo.get_control_plane(created.id).await.unwrap();
    assert_eq!(after_recovery.health, "healthy");
    assert_eq!(after_recovery.consecutive_failures, 0);
    assert!(after_recovery.last_seen_at.is_some());
    assert_eq!(after_recovery.api_version.as_deref(), Some("2"));
}

#[tokio::test]
async fn delete_control_plane() {
    let repo = setup_test_db().await;
    let created = repo
        .create_control_plane(CreateControlPlane {
            name: "Plane".into(),
            kind: None,
            base_url: "http://localhost:9999".into(),
            token: None,
        })
        .await
        .expect("create");

    assert!(repo.delete_control_plane(created.id).await.expect("delete"));
    assert!(
        !repo
            .delete_control_plane(created.id)
            .await
            .expect("delete again")
    );
    assert!(repo.get_control_plane(created.id).await.is_err());
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_links
// ════════════════════════════════════════════════════════════════════════════════════

async fn make_control_plane(repo: &tack_db::Repository) -> Uuid {
    repo.create_control_plane(CreateControlPlane {
        name: "Plane".into(),
        kind: None,
        base_url: "http://localhost:9999".into(),
        token: None,
    })
    .await
    .expect("create control plane")
    .id
}

#[tokio::test]
async fn upsert_get_delete_orch_link() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let plane_id = make_control_plane(&repo).await;

    assert_eq!(repo.get_orch_link(project.id).await.unwrap(), None);

    let link = repo
        .upsert_orch_link(
            project.id,
            UpsertOrchLink {
                control_plane_id: plane_id,
                remote_project: "demo-remote".into(),
                pipeline_file: Some("pipeline.yaml".into()),
                blueprint: Some("software".into()),
                auto_dispatch: false,
                budget_usd: Some(50.0),
                status_map: serde_json::json!({"dispatch_from": ["Ready"]}),
            },
        )
        .await
        .expect("upsert link");

    assert_eq!(link.project_id, project.id);
    assert_eq!(link.remote_project, "demo-remote");
    assert_eq!(link.budget_usd, Some(50.0));

    // Re-upsert (ON CONFLICT(project_id)) replaces the row, not duplicates it.
    let updated = repo
        .upsert_orch_link(
            project.id,
            UpsertOrchLink {
                control_plane_id: plane_id,
                remote_project: "demo-remote-renamed".into(),
                pipeline_file: None,
                blueprint: None,
                auto_dispatch: true,
                budget_usd: None,
                status_map: serde_json::json!({}),
            },
        )
        .await
        .expect("re-upsert link");
    assert_eq!(updated.remote_project, "demo-remote-renamed");
    assert!(updated.auto_dispatch);
    assert_eq!(updated.budget_usd, None);

    let for_plane = repo.list_orch_links_for_plane(plane_id).await.unwrap();
    assert_eq!(for_plane.len(), 1, "still exactly one link, not two");

    assert!(repo.delete_orch_link(project.id).await.unwrap());
    assert_eq!(repo.get_orch_link(project.id).await.unwrap(), None);
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_tasks — batch upsert idempotency + composite PK
// ════════════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn upsert_orch_tasks_batch_is_idempotent() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;

    let make_batch = |status: &str| {
        vec![
            NewOrchTask {
                item_id: item.id,
                remote_task_id: "task-1".into(),
                remote_run_id: Some("run-1".into()),
                remote_status: status.to_string(),
                attempt: 1,
                tokens_in: 100,
                tokens_out: 50,
                cost_usd_estimated: Some(0.01),
                dispatched_at: Utc::now(),
                trusted: true,
            },
            NewOrchTask {
                item_id: item.id,
                remote_task_id: "task-2".into(),
                remote_run_id: Some("run-1".into()),
                remote_status: status.to_string(),
                attempt: 1,
                tokens_in: 10,
                tokens_out: 5,
                cost_usd_estimated: None,
                dispatched_at: Utc::now(),
                trusted: false,
            },
        ]
    };

    repo.upsert_orch_tasks(&make_batch("running"))
        .await
        .expect("first upsert");
    let after_first = repo.list_orch_tasks_for_item(item.id).await.unwrap();
    assert_eq!(after_first.len(), 2);

    // Re-upsert the identical (item_id, remote_task_id) pairs with different content —
    // must update in place, not duplicate, and must not error.
    repo.upsert_orch_tasks(&make_batch("done"))
        .await
        .expect("second upsert must be idempotent");
    let after_second = repo.list_orch_tasks_for_item(item.id).await.unwrap();
    assert_eq!(after_second.len(), 2, "no duplicate rows from re-upserting");
    assert!(
        after_second.iter().all(|t| t.remote_status == "done"),
        "re-upsert should have refreshed remote_status in place"
    );

    // Composite PK: a *different* remote_task_id for the same item is a new row (a
    // redispatch), not an update of an existing one.
    repo.upsert_orch_tasks(&[NewOrchTask {
        item_id: item.id,
        remote_task_id: "task-3-retry".into(),
        remote_run_id: None,
        remote_status: "pending".into(),
        attempt: 2,
        tokens_in: 0,
        tokens_out: 0,
        cost_usd_estimated: None,
        dispatched_at: Utc::now(),
        trusted: true,
    }])
    .await
    .expect("third task, different remote_task_id");
    assert_eq!(
        repo.list_orch_tasks_for_item(item.id).await.unwrap().len(),
        3
    );

    // Empty batch is a documented no-op, not an error.
    repo.upsert_orch_tasks(&[])
        .await
        .expect("empty batch is a no-op");
}

#[tokio::test]
async fn get_and_find_orch_task() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;

    assert_eq!(
        repo.get_orch_task(item.id, "nonexistent").await.unwrap(),
        None
    );

    repo.upsert_orch_tasks(&[NewOrchTask {
        item_id: item.id,
        remote_task_id: "task-abc".into(),
        remote_run_id: None,
        // An unrecognised docket status string must round-trip untouched — this layer
        // never validates/rejects it.
        remote_status: "some_future_status_tack_has_never_seen".into(),
        attempt: 1,
        tokens_in: 0,
        tokens_out: 0,
        cost_usd_estimated: None,
        dispatched_at: Utc::now(),
        trusted: true,
    }])
    .await
    .expect("upsert");

    let fetched = repo
        .get_orch_task(item.id, "task-abc")
        .await
        .unwrap()
        .expect("task exists");
    assert_eq!(
        fetched.remote_status,
        "some_future_status_tack_has_never_seen"
    );

    let found = repo
        .find_orch_task_by_remote_task_id("task-abc")
        .await
        .unwrap()
        .expect("found by remote_task_id alone");
    assert_eq!(found.item_id, item.id);
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_runs — batch upsert idempotency, unattributed runs
// ════════════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn orch_runs_upsert_idempotent_keeps_unattributed_runs() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;

    let batch = vec![
        NewOrchRun {
            run_id: "run-cli-1".into(),
            item_id: None, // dispatched from the docket CLI, not via Tack — normal case
            remote_project: "demo".into(),
            source: "cli".into(),
            state: "queued".into(),
            started_at: None,
            ended_at: None,
            error: None,
        },
        NewOrchRun {
            run_id: "run-cli-2".into(),
            item_id: None,
            remote_project: "demo".into(),
            source: "totally_new_source_docket_invented".into(),
            state: "running".into(),
            started_at: Some(Utc::now()),
            ended_at: None,
            error: None,
        },
    ];

    repo.upsert_orch_runs(plane_id, &batch)
        .await
        .expect("first upsert");
    assert!(repo.get_orch_run("run-cli-1").await.unwrap().is_some());

    // Re-upload the same batch with an updated state — idempotent, no duplicate rows
    // (run_id is the PK), no error.
    let mut updated_batch = batch;
    updated_batch[0].state = "succeeded".into();
    updated_batch[0].ended_at = Some(Utc::now());
    repo.upsert_orch_runs(plane_id, &updated_batch)
        .await
        .expect("second upsert must be idempotent");

    let run = repo.get_orch_run("run-cli-1").await.unwrap().unwrap();
    assert_eq!(run.state, "succeeded");
    assert!(run.item_id.is_none(), "still unattributed");

    let run2 = repo.get_orch_run("run-cli-2").await.unwrap().unwrap();
    assert_eq!(
        run2.source, "totally_new_source_docket_invented",
        "unrecognised source string stored as-is"
    );

    repo.upsert_orch_runs(plane_id, &[])
        .await
        .expect("empty batch is a no-op");
}

#[tokio::test]
async fn orch_run_attribution_is_never_unlearned() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_control_plane(&repo).await;

    // First poll: no item_id known yet.
    repo.upsert_orch_runs(
        plane_id,
        &[NewOrchRun {
            run_id: "run-x".into(),
            item_id: None,
            remote_project: "demo".into(),
            source: "cli".into(),
            state: "running".into(),
            started_at: None,
            ended_at: None,
            error: None,
        }],
    )
    .await
    .unwrap();

    // A later poll learns the attribution.
    repo.upsert_orch_runs(
        plane_id,
        &[NewOrchRun {
            run_id: "run-x".into(),
            item_id: Some(item.id),
            remote_project: "demo".into(),
            source: "cli".into(),
            state: "running".into(),
            started_at: None,
            ended_at: None,
            error: None,
        }],
    )
    .await
    .unwrap();
    assert_eq!(
        repo.get_orch_run("run-x").await.unwrap().unwrap().item_id,
        Some(item.id)
    );

    // A subsequent poll that (correctly) omits item_id must not clear it back to NULL.
    repo.upsert_orch_runs(
        plane_id,
        &[NewOrchRun {
            run_id: "run-x".into(),
            item_id: None,
            remote_project: "demo".into(),
            source: "cli".into(),
            state: "succeeded".into(),
            started_at: None,
            ended_at: Some(Utc::now()),
            error: None,
        }],
    )
    .await
    .unwrap();
    let run = repo.get_orch_run("run-x").await.unwrap().unwrap();
    assert_eq!(run.state, "succeeded");
    assert_eq!(run.item_id, Some(item.id), "known attribution must persist");

    assert_eq!(
        repo.list_orch_runs_for_item(item.id).await.unwrap().len(),
        1
    );
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_events — batch upsert idempotency
// ════════════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn upsert_orch_events_batch_is_idempotent() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_control_plane(&repo).await;

    let event_id = Uuid::new_v4();
    let batch = vec![NewOrchEvent {
        id: event_id,
        item_id: Some(item.id),
        run_id: Some("run-1".into()),
        event_type: "an_event_type_from_the_future".into(),
        payload: serde_json::json!({"hop": 1}),
        occurred_at: Utc::now(),
    }];

    repo.upsert_orch_events(plane_id, &batch)
        .await
        .expect("first upsert");
    assert_eq!(
        repo.list_orch_events_for_item(item.id, None)
            .await
            .unwrap()
            .len(),
        1
    );

    // Re-upserting the same event id (simulating a re-poll of an overlapping cursor
    // window) must not duplicate the row.
    repo.upsert_orch_events(plane_id, &batch)
        .await
        .expect("second upsert must be idempotent");
    let events = repo.list_orch_events_for_item(item.id, None).await.unwrap();
    assert_eq!(events.len(), 1, "no duplicate rows from re-upserting");
    assert_eq!(events[0].event_type, "an_event_type_from_the_future");
    assert_eq!(events[0].payload, serde_json::json!({"hop": 1}));

    repo.upsert_orch_events(plane_id, &[])
        .await
        .expect("empty batch is a no-op");
}

#[tokio::test]
async fn list_orch_events_for_item_respects_limit_and_order() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_control_plane(&repo).await;

    let base = Utc::now();
    let batch: Vec<NewOrchEvent> = (0..5)
        .map(|i| NewOrchEvent {
            id: Uuid::new_v4(),
            item_id: Some(item.id),
            run_id: None,
            event_type: format!("hop_{i}"),
            payload: serde_json::json!({}),
            occurred_at: base + chrono::Duration::seconds(i),
        })
        .collect();

    repo.upsert_orch_events(plane_id, &batch).await.unwrap();

    let all = repo.list_orch_events_for_item(item.id, None).await.unwrap();
    assert_eq!(all.len(), 5);
    assert_eq!(all[0].event_type, "hop_0", "chronological, oldest first");
    assert_eq!(all[4].event_type, "hop_4");

    let limited = repo
        .list_orch_events_for_item(item.id, Some(2))
        .await
        .unwrap();
    assert_eq!(limited.len(), 2);
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_approvals — batch upsert idempotency, uncorrelated records
// ════════════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn upsert_orch_approvals_batch_is_idempotent() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;

    let batch = vec![NewOrchApproval {
        token: "approval-token-1".into(),
        item_id: None,
        remote_task_id: Some("task-1".into()),
        agent: Some("builder".into()),
        action: Some("git push".into()),
        state: "pending".into(),
        requested_at: Utc::now(),
        decided_at: None,
    }];

    repo.upsert_orch_approvals(plane_id, &batch)
        .await
        .expect("first upsert");
    assert!(
        repo.get_orch_approval("approval-token-1")
            .await
            .unwrap()
            .is_some()
    );

    repo.upsert_orch_approvals(plane_id, &batch)
        .await
        .expect("second upsert must be idempotent");
    let pending = repo.list_pending_orch_approvals().await.unwrap();
    assert_eq!(pending.len(), 1, "no duplicate rows from re-upserting");

    repo.upsert_orch_approvals(plane_id, &[])
        .await
        .expect("empty batch is a no-op");
}

#[tokio::test]
async fn uncorrelated_approvals_still_appear_in_pending_inbox() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_control_plane(&repo).await;

    repo.upsert_orch_approvals(
        plane_id,
        &[
            NewOrchApproval {
                token: "uncorrelated".into(),
                item_id: None,
                remote_task_id: None,
                agent: None,
                action: Some("rm -rf /tmp/build".into()),
                state: "pending".into(),
                requested_at: Utc::now() - chrono::Duration::seconds(10),
                decided_at: None,
            },
            NewOrchApproval {
                token: "correlated".into(),
                item_id: Some(item.id),
                remote_task_id: Some("task-1".into()),
                agent: Some("builder".into()),
                action: Some("git push".into()),
                state: "pending".into(),
                requested_at: Utc::now(),
                decided_at: None,
            },
        ],
    )
    .await
    .expect("upsert both");

    let pending = repo.list_pending_orch_approvals().await.unwrap();
    assert_eq!(pending.len(), 2);
    // Oldest first.
    assert_eq!(pending[0].token, "uncorrelated");
    assert!(pending[0].item_id.is_none());
    assert_eq!(pending[1].token, "correlated");
    assert_eq!(pending[1].item_id, Some(item.id));

    // A decision (granted/denied) removes it from the *pending* inbox but the row
    // (and its final state) survives.
    repo.upsert_orch_approvals(
        plane_id,
        &[NewOrchApproval {
            token: "correlated".into(),
            item_id: Some(item.id),
            remote_task_id: Some("task-1".into()),
            agent: Some("builder".into()),
            action: Some("git push".into()),
            state: "granted".into(),
            requested_at: Utc::now(),
            decided_at: Some(Utc::now()),
        }],
    )
    .await
    .unwrap();

    let pending_after = repo.list_pending_orch_approvals().await.unwrap();
    assert_eq!(pending_after.len(), 1);
    let decided = repo.get_orch_approval("correlated").await.unwrap().unwrap();
    assert_eq!(decided.state, "granted");
    assert!(decided.decided_at.is_some());
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_metrics
// ════════════════════════════════════════════════════════════════════════════════════

// Covered by `orch_metrics.rs`, not here.

// ════════════════════════════════════════════════════════════════════════════════════
// orch_trace_cursors (migration 028)
// ════════════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn set_and_list_trace_cursors_scoped_per_plane() {
    let repo = setup_test_db().await;
    let plane_a = make_control_plane(&repo).await;
    let plane_b = make_control_plane(&repo).await;

    assert!(repo.list_trace_cursors(plane_a).await.unwrap().is_empty());

    repo.set_trace_cursor(plane_a, "demo", "2026-08-04T19:52:27Z:1")
        .await
        .expect("set cursor for plane_a/demo");
    repo.set_trace_cursor(plane_a, "other-project", "2026-08-04T00:00:00Z:0")
        .await
        .expect("set cursor for plane_a/other-project");
    // A different plane's cursor for the *same* remote_project name must not
    // collide — the PK is (control_plane_id, remote_project), not remote_project
    // alone (two docket instances can both happen to have a "demo" project).
    repo.set_trace_cursor(plane_b, "demo", "2026-08-03T00:00:00Z:5")
        .await
        .expect("set cursor for plane_b/demo");

    let for_a = repo.list_trace_cursors(plane_a).await.unwrap();
    assert_eq!(for_a.len(), 2, "both of plane_a's projects, not plane_b's");
    let demo_a = for_a.iter().find(|c| c.remote_project == "demo").unwrap();
    assert_eq!(demo_a.cursor, "2026-08-04T19:52:27Z:1");

    let for_b = repo.list_trace_cursors(plane_b).await.unwrap();
    assert_eq!(for_b.len(), 1);
    assert_eq!(for_b[0].cursor, "2026-08-03T00:00:00Z:5");
}

#[tokio::test]
async fn set_trace_cursor_upserts_in_place() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;

    repo.set_trace_cursor(plane_id, "demo", "2026-08-04T19:52:27Z:1")
        .await
        .unwrap();
    repo.set_trace_cursor(plane_id, "demo", "2026-08-04T19:52:40Z:2")
        .await
        .unwrap();

    let cursors = repo.list_trace_cursors(plane_id).await.unwrap();
    assert_eq!(
        cursors.len(),
        1,
        "re-setting the same pair updates, not duplicates"
    );
    assert_eq!(cursors[0].cursor, "2026-08-04T19:52:40Z:2");
}

#[tokio::test]
async fn orch_trace_cursor_cascades_on_control_plane_delete() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;
    repo.set_trace_cursor(plane_id, "demo", "2026-08-04T19:52:27Z:1")
        .await
        .unwrap();

    assert!(repo.delete_control_plane(plane_id).await.unwrap());
    assert!(
        repo.list_trace_cursors(plane_id).await.unwrap().is_empty(),
        "ON DELETE CASCADE from control_planes must remove its trace cursors too"
    );
}

// ════════════════════════════════════════════════════════════════════════════════════
// list_pending_orch_approvals_with_context / mark_orch_approval_decided
// ════════════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn pending_approvals_with_context_include_uncorrelated_rows() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item = make_item(&repo, &project).await;
    let plane_id = make_control_plane(&repo).await;

    repo.upsert_orch_approvals(
        plane_id,
        &[
            NewOrchApproval {
                token: "uncorrelated".into(),
                item_id: None,
                remote_task_id: None,
                agent: Some("cli-agent".into()),
                action: Some("rm -rf /tmp/build".into()),
                state: "pending".into(),
                requested_at: Utc::now() - chrono::Duration::seconds(30),
                decided_at: None,
            },
            NewOrchApproval {
                token: "correlated".into(),
                item_id: Some(item.id),
                remote_task_id: Some("task-1".into()),
                agent: Some("builder".into()),
                action: Some("git push".into()),
                state: "pending".into(),
                requested_at: Utc::now(),
                decided_at: None,
            },
        ],
    )
    .await
    .expect("upsert both");

    let rows = repo
        .list_pending_orch_approvals_with_context()
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);

    // Oldest first, same ordering guarantee as list_pending_orch_approvals.
    let uncorrelated = &rows[0];
    assert_eq!(uncorrelated.token, "uncorrelated");
    assert!(
        uncorrelated.item_id.is_none(),
        "an uncorrelated approval must still surface here — it's the whole point of this query"
    );
    assert!(uncorrelated.item_title.is_none());
    assert!(uncorrelated.project_id.is_none());
    assert!(uncorrelated.project_name.is_none());
    assert_eq!(uncorrelated.agent.as_deref(), Some("cli-agent"));
    assert_eq!(uncorrelated.action.as_deref(), Some("rm -rf /tmp/build"));
    // control_plane_name is still populated even when the item is unknown.
    assert_eq!(uncorrelated.control_plane_name, "Plane");

    let correlated = &rows[1];
    assert_eq!(correlated.token, "correlated");
    assert_eq!(correlated.item_id, Some(item.id));
    assert_eq!(correlated.item_title.as_deref(), Some(item.title.as_str()));
    assert_eq!(
        correlated.item_status.as_deref(),
        Some(item.status.as_str())
    );
    assert_eq!(correlated.project_id, Some(project.id));
    assert_eq!(
        correlated.project_name.as_deref(),
        Some(project.name.as_str())
    );
}

#[tokio::test]
async fn mark_orch_approval_decided_removes_from_pending_inbox() {
    let repo = setup_test_db().await;
    let plane_id = make_control_plane(&repo).await;

    repo.upsert_orch_approvals(
        plane_id,
        &[NewOrchApproval {
            token: "apr-decide-1".into(),
            item_id: None,
            remote_task_id: None,
            agent: Some("builder".into()),
            action: Some("deploy".into()),
            state: "pending".into(),
            requested_at: Utc::now(),
            decided_at: None,
        }],
    )
    .await
    .unwrap();

    assert_eq!(
        repo.list_pending_orch_approvals_with_context()
            .await
            .unwrap()
            .len(),
        1
    );

    let decided_at = Utc::now();
    repo.mark_orch_approval_decided("apr-decide-1", "granted", decided_at)
        .await
        .expect("mark decided");

    assert!(
        repo.list_pending_orch_approvals_with_context()
            .await
            .unwrap()
            .is_empty(),
        "a decided approval must disappear from the pending inbox"
    );

    let row = repo
        .get_orch_approval("apr-decide-1")
        .await
        .unwrap()
        .expect("row still exists");
    assert_eq!(row.state, "granted");
    assert!(row.decided_at.is_some());
}

#[tokio::test]
async fn mark_orch_approval_decided_on_unknown_token_is_a_no_op() {
    let repo = setup_test_db().await;
    // No row exists for this token at all — must not error (defensive path,
    // see the function's own doc comment).
    repo.mark_orch_approval_decided("does-not-exist", "granted", Utc::now())
        .await
        .expect("unknown token must be a no-op, not an error");
}
