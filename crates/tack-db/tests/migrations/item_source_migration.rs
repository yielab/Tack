//! Tests for migration 029 (`items.source`) — the sticky provenance/trust
//! marker behind the prompt-injection trust boundary (`ItemSource::is_trusted`).
//!
//! Covers:
//!   - every creation path writes an explicit, correct `source`;
//!   - `update_item` never mutates it once set (the "sticky" requirement —
//!     untrusted text must stay untrusted for the item's whole lifetime);
//!   - an item that existed before this migration ran resolves to the
//!     *safe* value (untrusted), never to `manual` by accident — the
//!     "unsafe state is never the accidental default" rule.

use crate::common::{create_test_workspace, make_project, setup_test_db};
use tack_core::models::{CreateItem, ItemSource, ItemType, Priority, UpdateItem};
use tack_db::{Repository, init_pool, migrations};
use uuid::Uuid;

fn minimal_create_item(title: &str) -> CreateItem {
    CreateItem {
        title: title.to_string(),
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
    }
}

// ─── Fresh install: every creation path writes an explicit, correct source ─

#[tokio::test]
async fn fresh_manual_create_item_is_trusted() {
    let repo = setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let status = project.workflow.initial_status().unwrap().to_string();

    let item = repo
        .create_item(project.id, &status, minimal_create_item("Manual item"))
        .await
        .expect("create item");

    assert_eq!(item.source, ItemSource::Manual);
    assert!(item.source.is_trusted());

    // Round-trip through a fresh read, not just the create-time return value.
    let reloaded = repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(reloaded.source, ItemSource::Manual);
}

#[tokio::test]
async fn create_item_with_source_persists_and_only_manual_is_trusted() {
    let repo = setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let status = project.workflow.initial_status().unwrap().to_string();

    for source in [
        ItemSource::Github,
        ItemSource::Linear,
        ItemSource::JsonImport,
        ItemSource::CsvImport,
        ItemSource::Unknown,
    ] {
        let item = repo
            .create_item_with_source(
                project.id,
                &status,
                minimal_create_item(&format!("{source} item")),
                source.clone(),
            )
            .await
            .expect("create item with source");

        assert_eq!(item.source, source);
        assert!(!item.source.is_trusted(), "{source} must not be trusted");

        let reloaded = repo.get_item(item.id).await.unwrap().unwrap();
        assert_eq!(
            reloaded.source, source,
            "source must round-trip through a DB read"
        );
    }
}

// ─── Sticky: update_item never mutates source ──────────────────────────────

#[tokio::test]
async fn update_item_never_changes_source() {
    let repo = setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let status = project.workflow.initial_status().unwrap().to_string();

    let item = repo
        .create_item_with_source(
            project.id,
            &status,
            minimal_create_item("Untrusted item"),
            ItemSource::Github,
        )
        .await
        .expect("create item");

    // Edit several unrelated fields, including the very fields whose text is
    // the actual injection surface (title/description) — if anything were
    // ever to accidentally "launder" trust on edit, this is where it would
    // show up.
    let updated = repo
        .update_item(
            item.id,
            UpdateItem {
                title: Some("Edited title".into()),
                description: Some(Some(
                    "Edited description, possibly attacker-authored".into(),
                )),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        updated.source,
        ItemSource::Github,
        "editing an item must never change its provenance/trust marker"
    );
    assert!(!updated.source.is_trusted());
}

// ─── Upgrade-in-place: a pre-migration item resolves to untrusted ──────────

/// Inserts a workspace/project/item row directly with raw SQL, matching the
/// pre-029 schema exactly (no `source` column) — what a real row on an
/// existing install looks like at this point in migration history, including
/// one imported from GitHub before the column existed (migration
/// 018/github_links predates it).
async fn seed_pre_029_item(pool: &sqlx::SqlitePool) -> Uuid {
    let ws_id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, name) VALUES (?, 'Legacy Workspace')")
        .bind(ws_id.to_string())
        .execute(pool)
        .await
        .expect("insert workspace");

    let project_id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, workspace_id, name) VALUES (?, ?, 'Legacy Project')")
        .bind(project_id.to_string())
        .bind(ws_id.to_string())
        .execute(pool)
        .await
        .expect("insert project");

    let item_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO items (id, project_id, title, status) \
         VALUES (?, ?, 'Legacy item, possibly imported from GitHub before this column existed', 'To Do')",
    )
    .bind(item_id.to_string())
    .bind(project_id.to_string())
    .execute(pool)
    .await
    .expect("insert legacy item");

    item_id
}

#[tokio::test]
async fn upgrade_in_place_backfills_pre_migration_items_to_untrusted() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    // Simulate an installed tack.db stopped at "028_orch_trace_cursors" — every
    // migration before 029 (and its `items.source` column) ever existed.
    migrations::run_up_to(&pool, "028_orch_trace_cursors")
        .await
        .expect("apply migrations up to 028");
    let item_id = seed_pre_029_item(&pool).await;

    // Now run the full migration set again, as `tack serve` does on every
    // startup — this is the actual upgrade-in-place path.
    migrations::run_all(&pool).await.expect("upgrade in place");

    // Assert at the raw SQL level first: the column exists and backfilled
    // to the literal 'unknown' value migration 029 writes, not NULL and not
    // 'manual'.
    let raw_source: String = sqlx::query_scalar("SELECT source FROM items WHERE id = ?")
        .bind(item_id.to_string())
        .fetch_one(&pool)
        .await
        .expect("select raw source column");
    assert_eq!(raw_source, "unknown");

    // And through the repository layer, which is what every real caller
    // (including the dispatcher's trust check) actually uses.
    let repo = Repository::new(pool);
    let item = repo
        .get_item(item_id)
        .await
        .expect("get item")
        .expect("item must still exist after the upgrade");

    assert_eq!(
        item.source,
        ItemSource::Unknown,
        "a pre-migration item must backfill to the 'unknown' source, not 'manual'"
    );
    assert!(
        !item.source.is_trusted(),
        "an item whose provenance predates this column must resolve to untrusted, \
         never to trusted-by-default — unsafe state must not be the accidental \
         default"
    );
}

#[tokio::test]
async fn migration_029_is_applied_on_fresh_db() {
    let repo = setup_test_db().await;
    let applied: Vec<String> = sqlx::query_scalar("SELECT name FROM _migrations ORDER BY id")
        .fetch_all(repo.pool())
        .await
        .expect("select migrations");
    assert!(
        applied.iter().any(|m| m == "029_item_source"),
        "029_item_source must have been applied on a fresh db"
    );
}
