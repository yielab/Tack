//! `items.version` (migration 034) backs `handlers::items`'s `ETag`/`If-Match`
//! support. `update_item`, `update_item_status_checked` and
//! `check_and_update_parent_status` are the three write paths that must bump
//! it; `claim_item_version` is the atomic compare-and-swap the HTTP layer
//! builds its concurrency guard on.

use crate::common::{self, create_test_workspace, make_item, make_project};
use tack_core::models::{CreateItem, ItemType, Priority, UpdateItem};
use tack_core::workflow::StatusCategory;

/// Scrum's "In Progress" column has `wip_limit: Some(5)` — comfortably above
/// the single item this file moves into it.
const TARGET: &str = "In Progress";

#[tokio::test]
async fn fresh_item_starts_at_version_one() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = make_item(&repo, &project).await;

    let version = repo
        .get_item_version(item.id)
        .await
        .expect("db call")
        .expect("item exists");
    assert_eq!(version, 1, "migration 034's column default is 1");
}

#[tokio::test]
async fn update_item_bumps_version() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = make_item(&repo, &project).await;

    let before = repo.get_item_version(item.id).await.unwrap().unwrap();
    repo.update_item(
        item.id,
        UpdateItem {
            title: Some("Renamed".into()),
            ..Default::default()
        },
    )
    .await
    .expect("db call")
    .expect("item exists");
    let after = repo.get_item_version(item.id).await.unwrap().unwrap();
    assert!(
        after > before,
        "update_item must bump version: {before} -> {after}"
    );
}

#[tokio::test]
async fn update_item_status_checked_bumps_version() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = make_item(&repo, &project).await;

    let before = repo.get_item_version(item.id).await.unwrap().unwrap();
    repo.update_item_status_checked(
        item.id,
        project.id,
        TARGET,
        Some(StatusCategory::InProgress),
        &project.workflow,
    )
    .await
    .expect("db call")
    .expect("item exists");
    let after = repo.get_item_version(item.id).await.unwrap().unwrap();
    assert!(
        after > before,
        "update_item_status_checked must bump version: {before} -> {after}"
    );
}

#[tokio::test]
async fn check_and_update_parent_status_bumps_version() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;

    let parent = repo
        .create_item(
            project.id,
            "Backlog",
            CreateItem {
                title: "Parent".into(),
                description: None,
                item_type: Some(ItemType::Epic),
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
        .unwrap();
    let child = repo
        .create_item(
            project.id,
            "Done",
            CreateItem {
                title: "Only child".into(),
                description: None,
                item_type: Some(ItemType::Task),
                parent_id: Some(parent.id),
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
        .unwrap();
    assert_eq!(child.status, "Done");

    let before = repo.get_item_version(parent.id).await.unwrap().unwrap();
    let completed = repo
        .check_and_update_parent_status(parent.id, "Done")
        .await
        .expect("db call");
    assert!(completed, "the only child is already Done");
    let after = repo.get_item_version(parent.id).await.unwrap().unwrap();
    assert!(
        after > before,
        "check_and_update_parent_status must bump version: {before} -> {after}"
    );
}

#[tokio::test]
async fn claim_item_version_succeeds_once_and_rejects_stale_reuse() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = make_item(&repo, &project).await;

    let v1 = repo.get_item_version(item.id).await.unwrap().unwrap();
    assert_eq!(v1, 1);

    // First claim against the true current version succeeds and bumps it.
    let claimed = repo.claim_item_version(item.id, v1).await.expect("db call");
    assert!(claimed, "claiming the true current version must succeed");
    let v2 = repo.get_item_version(item.id).await.unwrap().unwrap();
    assert_eq!(v2, v1 + 1);

    // A second claim reusing the now-stale `v1` must fail — this is the
    // exact shape of a caller that lost a concurrent race.
    let stale_claim = repo.claim_item_version(item.id, v1).await.expect("db call");
    assert!(
        !stale_claim,
        "claiming an already-superseded version must fail, not silently re-apply"
    );
    // And the failed claim must not itself have moved the counter.
    let v3 = repo.get_item_version(item.id).await.unwrap().unwrap();
    assert_eq!(
        v3, v2,
        "a failed claim must be a no-op, not a partial write"
    );
}

#[tokio::test]
async fn claim_version_on_unknown_item_returns_false() {
    let repo = common::setup_test_db().await;
    let claimed = repo
        .claim_item_version(uuid::Uuid::new_v4(), 1)
        .await
        .expect("db call must not error for a missing row");
    assert!(!claimed);
}
