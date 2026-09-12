//! Sequential correctness tests for `Repository::update_item_status_checked`
//! — the atomic WIP-limit-check-then-write that
//! replaced `dispatcher::apply_mapped_status`'s old, racy
//! `count_items_by_status` + `update_item` pair. These are deterministic,
//! single-writer checks of the method's own behaviour (applied vs.
//! rejected, timestamp bookkeeping, the `None`-on-vanished-item case); the
//! genuinely concurrent reproduction of the race this method fixes lives in
//! `crates/tack-api/tests/security/wip_limit_race.rs`, since it needs the full
//! dispatch flow to drive real concurrent HTTP requests.

use crate::common::{self, create_test_workspace, make_project};
use tack_core::CoreError;
use tack_core::models::{CreateItem, ItemType, Priority};
use tack_db::repo::items::StatusUpdateOutcome;
use uuid::Uuid;

/// Scrum's "In Progress" column (`wip_limit: Some(5)`) is used throughout —
/// see `tack_core::workflow::scrum_workflow`.
const TARGET: &str = "In Progress";
const WIP_LIMIT: usize = 5;

#[tokio::test]
async fn applies_transition_under_limit() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = common::make_item(&repo, &project).await;
    assert_eq!(item.status, "Backlog");

    let outcome = repo
        .update_item_status_checked(item.id, project.id, TARGET, None, &project.workflow)
        .await
        .expect("db call")
        .expect("item exists");

    match outcome {
        StatusUpdateOutcome::Applied(updated) => {
            assert_eq!(updated.status, TARGET);
        }
        StatusUpdateOutcome::Rejected(e) => panic!("expected Applied, got Rejected({e})"),
    }

    let reloaded = repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(reloaded.status, TARGET);
}

#[tokio::test]
async fn rejects_at_limit_leaves_item_and_count_untouched() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;

    // Fill the column to exactly its limit with WIP_LIMIT other items.
    for i in 0..WIP_LIMIT {
        let filler = repo
            .create_item(
                project.id,
                TARGET,
                CreateItem {
                    title: format!("Filler {i}"),
                    description: None,
                    item_type: Some(ItemType::Task),
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
        assert_eq!(filler.status, TARGET);
    }
    assert_eq!(
        repo.count_items_by_status(project.id, TARGET)
            .await
            .unwrap(),
        WIP_LIMIT as i64
    );

    let item = common::make_item(&repo, &project).await;
    let outcome = repo
        .update_item_status_checked(item.id, project.id, TARGET, None, &project.workflow)
        .await
        .expect("db call")
        .expect("item exists");

    match outcome {
        StatusUpdateOutcome::Rejected(CoreError::WipLimitExceeded {
            column,
            limit,
            current,
        }) => {
            assert_eq!(column, TARGET);
            assert_eq!(limit, WIP_LIMIT);
            assert_eq!(current, WIP_LIMIT);
        }
        StatusUpdateOutcome::Rejected(other) => {
            panic!("expected WipLimitExceeded, got {other}")
        }
        StatusUpdateOutcome::Applied(_) => panic!("expected Rejected, the column is already full"),
    }

    // The item itself was left exactly where it started — no partial write.
    let reloaded = repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(reloaded.status, "Backlog");
    // And the column's count didn't move either.
    assert_eq!(
        repo.count_items_by_status(project.id, TARGET)
            .await
            .unwrap(),
        WIP_LIMIT as i64
    );
}

#[tokio::test]
async fn status_with_no_limit_always_applies() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    // "Done" has no wip_limit in scrum_workflow().
    for i in 0..50 {
        let item = repo
            .create_item(
                project.id,
                "Backlog",
                CreateItem {
                    title: format!("Item {i}"),
                    description: None,
                    item_type: Some(ItemType::Task),
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
        let outcome = repo
            .update_item_status_checked(item.id, project.id, "Done", None, &project.workflow)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(outcome, StatusUpdateOutcome::Applied(_)));
    }
    assert_eq!(
        repo.count_items_by_status(project.id, "Done")
            .await
            .unwrap(),
        50
    );
}

#[tokio::test]
async fn status_category_stamps_started_and_completed_at() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = common::make_item(&repo, &project).await;
    assert!(item.started_at.is_none());
    assert!(item.completed_at.is_none());

    // Entering an InProgress-category status stamps started_at.
    let outcome = repo
        .update_item_status_checked(
            item.id,
            project.id,
            TARGET,
            Some(tack_core::workflow::StatusCategory::InProgress),
            &project.workflow,
        )
        .await
        .unwrap()
        .unwrap();
    let updated = match outcome {
        StatusUpdateOutcome::Applied(i) => *i,
        StatusUpdateOutcome::Rejected(e) => panic!("unexpected rejection: {e}"),
    };
    assert!(updated.started_at.is_some());
    assert!(updated.completed_at.is_none());

    // Entering a Done-category status stamps completed_at, keeps started_at.
    let outcome = repo
        .update_item_status_checked(
            item.id,
            project.id,
            "Done",
            Some(tack_core::workflow::StatusCategory::Done),
            &project.workflow,
        )
        .await
        .unwrap()
        .unwrap();
    let done = match outcome {
        StatusUpdateOutcome::Applied(i) => *i,
        StatusUpdateOutcome::Rejected(e) => panic!("unexpected rejection: {e}"),
    };
    assert!(done.started_at.is_some());
    assert!(done.completed_at.is_some());
}

#[tokio::test]
async fn unknown_item_id_returns_none_not_error() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;

    // The count+limit check has nothing to do with a specific item id, so
    // this only fails at the final reload — proving a vanished item (e.g.
    // deleted concurrently) surfaces as `None`, not a decode/row error.
    let outcome = repo
        .update_item_status_checked(Uuid::new_v4(), project.id, TARGET, None, &project.workflow)
        .await
        .expect("db call itself must not fail");
    assert!(outcome.is_none());
}
