//! Tests for migrations 019–038: the Agent-Factory Control Center schema
//! (`control_planes`, `orch_links`, `orch_tasks`, `orch_runs`, `orch_events`,
//! `orch_approvals`) plus additive
//! columns — 032 `control_planes.config`, 033 `control_planes.secrets`, 034
//! `items.version`, 035 `orch_links.version`, 036 `control_planes.version` — and
//! two table rebuilds, 037 (`orch_runs`) and 038 (`orch_approvals`).
//! See `docs/plans/agnostic-control-plane.md` §4 Phase 1/2/3 and §10.3 for
//! the design; `crates/tack-db/src/migrations.rs`'s own
//! comments above `MIGRATION_032` and `MIGRATION_037` are authoritative for why
//! 032-036 are single-statement ALTERs while 037/038 are not.
//!
//! Covers, for migrations 019-024:
//!   - a fresh database migrates cleanly and ends up with all six new tables;
//!   - an existing database stopped at "018_github_links" upgrades in place when
//!     `run_all` is called again (simulating an installed `tack.db` picking up a new
//!     Tack release);
//!   - foreign-key enforcement (see
//!     `test_foreign_key_rejects_orphan_item` in `integration.rs`) still holds for
//!     every new table that has an incoming FK. `control_planes` is the root of this
//!     schema's FK graph — it has no FK columns of its own — so there is nothing to
//!     orphan and no test for it here.
//!
//! Plus, for migrations 032-036:
//!   - each new column exists after its migration and does not exist before it;
//!   - a fresh database migrates cleanly all the way through 036;
//!   - an existing database stopped at "031_items_completed_at_index" (the last
//!     migration before this batch) upgrades in place;
//!   - the `NOT NULL DEFAULT` columns (`config`, the three `version` columns)
//!     populate correctly on rows that existed *before* the migration ran, not
//!     just on freshly inserted ones — a `DEFAULT` only fires for a value the
//!     `ALTER` statement backfills into existing rows, not for a value an
//!     application layer would need to supply.
//!
//! Plus, for migrations 037-038 — the table rebuilds. This is
//! the one migration batch that rewrites existing rows
//! rather than only adding columns, so "the table exists afterwards" is not
//! enough; every test below checks one of four things:
//! identical row counts and per-row field equality across the
//! rebuild, an empty `PRAGMA foreign_key_check`, the old primary key's
//! uniqueness still holding under the new composite key, and a deliberately
//! half-applied rebuild refusing to boot with a named error instead of
//! silently retrying `DROP TABLE` against data that might be all that
//! survived a crash.

use crate::common::{self, setup_test_db};
use sqlx::Row;
use tack_db::{init_pool, migrations};
use uuid::Uuid;

const NEW_TABLES: [&str; 6] = [
    "control_planes",
    "orch_links",
    "orch_tasks",
    "orch_runs",
    "orch_events",
    "orch_approvals",
];

async fn table_exists(pool: &sqlx::SqlitePool, table: &str) -> bool {
    sqlx::query("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?")
        .bind(table)
        .fetch_optional(pool)
        .await
        .expect("query sqlite_master")
        .is_some()
}

/// Whether `table` has a column named `column`, via `PRAGMA table_info` — the
/// only reliable way to ask SQLite "does this column exist" without attempting
/// a query against it and pattern-matching the error text.
async fn column_exists(pool: &sqlx::SqlitePool, table: &str, column: &str) -> bool {
    sqlx::query(sqlx::AssertSqlSafe(format!("PRAGMA table_info({table})")))
        .fetch_all(pool)
        .await
        .expect("query table_info")
        .iter()
        .any(|row| row.get::<String, _>("name") == column)
}

/// Inserts a single `control_planes` row directly with raw SQL, independent
/// of `Repository::create_control_plane`, and returns its id, for use as a
/// valid FK target in the orphan tests below.
async fn insert_control_plane(pool: &sqlx::SqlitePool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO control_planes (id, name, kind, base_url) VALUES (?, 'Test Plane', 'docket', 'http://localhost:9999')",
    )
    .bind(id.to_string())
    .execute(pool)
    .await
    .expect("insert control_plane");
    id
}

// ─── Fresh install ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_fresh_db_migrates_all_orch_tables() {
    let repo = setup_test_db().await;

    for table in NEW_TABLES {
        assert!(
            table_exists(repo.pool(), table).await,
            "expected table {table} to exist after a fresh migration run"
        );
    }

    let applied: Vec<String> = sqlx::query("SELECT name FROM _migrations ORDER BY id")
        .fetch_all(repo.pool())
        .await
        .expect("select migrations")
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();

    assert!(
        applied.iter().any(|m| m == "024_orch_approvals"),
        "024_orch_approvals must have been applied on a fresh db"
    );
    // Not asserting 024 is the *last* migration applied: migrations
    // 025-027 landed after this test was written (see orch_metrics.rs for their
    // coverage), and later migrations will add more after those. This test's job is
    // "the six orch tables from 019-024 exist," which the loop above already checks.
}

// ─── Upgrade-in-place from an existing 18-migration database ──────────────

#[tokio::test]
async fn test_upgrade_from_018_applies_new_orch_migrations_in_place() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    // Simulate an installed tack.db that has only ever seen migrations 001-018.
    migrations::run_up_to(&pool, "018_github_links")
        .await
        .expect("apply migrations up to 018");

    assert!(
        table_exists(&pool, "github_links").await,
        "018_github_links should have applied"
    );
    for table in NEW_TABLES {
        assert!(
            !table_exists(&pool, table).await,
            "table {table} must not exist before the upgrade runs"
        );
    }

    // Now run the full migration set again, as `tack serve` does on every startup.
    migrations::run_all(&pool).await.expect("upgrade in place");

    for table in NEW_TABLES {
        assert!(
            table_exists(&pool, table).await,
            "table {table} must exist after upgrading an existing db in place"
        );
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations")
        .fetch_one(&pool)
        .await
        .expect("count migrations");
    // >= 24 rather than == 24: migrations 025-027 landed after this
    // test was written (see orch_metrics.rs), and later migrations will add more.
    // The exact count isn't this test's job — "the six orch tables from 019-024 exist
    // after upgrading in place" (asserted above) is.
    assert!(
        count >= 24,
        "at least the 24 Wave-1 migrations should be recorded as applied, got {count}"
    );
}

// ─── FK enforcement on the new tables ──────────────────────────────────────

#[tokio::test]
async fn test_orch_links_rejects_orphan_project() {
    let repo = setup_test_db().await;
    let plane_id = insert_control_plane(repo.pool()).await;
    let bogus_project = Uuid::new_v4();

    let result = sqlx::query(
        "INSERT INTO orch_links (project_id, control_plane_id, remote_project) VALUES (?, ?, 'demo')",
    )
    .bind(bogus_project.to_string())
    .bind(plane_id.to_string())
    .execute(repo.pool())
    .await;

    assert!(
        result.is_err(),
        "inserting an orch_link with a dangling project_id must be rejected by the FK constraint"
    );
}

#[tokio::test]
async fn test_orch_tasks_rejects_orphan_item() {
    let repo = setup_test_db().await;
    let bogus_item = Uuid::new_v4();

    let result =
        sqlx::query("INSERT INTO orch_tasks (item_id, remote_task_id) VALUES (?, 'remote-task-1')")
            .bind(bogus_item.to_string())
            .execute(repo.pool())
            .await;

    assert!(
        result.is_err(),
        "inserting an orch_task with a dangling item_id must be rejected by the FK constraint"
    );
}

#[tokio::test]
async fn test_orch_runs_rejects_orphan_control_plane() {
    let repo = setup_test_db().await;
    let bogus_plane = Uuid::new_v4();
    let run_id = Uuid::new_v4();

    let result = sqlx::query(
        "INSERT INTO orch_runs (run_id, control_plane_id, remote_project) VALUES (?, ?, 'demo')",
    )
    .bind(run_id.to_string())
    .bind(bogus_plane.to_string())
    .execute(repo.pool())
    .await;

    assert!(
        result.is_err(),
        "inserting an orch_run with a dangling control_plane_id must be rejected by the FK constraint"
    );
}

#[tokio::test]
async fn test_orch_events_rejects_orphan_control_plane() {
    let repo = setup_test_db().await;
    let bogus_plane = Uuid::new_v4();
    let event_id = Uuid::new_v4();

    let result = sqlx::query(
        "INSERT INTO orch_events (id, control_plane_id, event_type) VALUES (?, ?, 'tool_call')",
    )
    .bind(event_id.to_string())
    .bind(bogus_plane.to_string())
    .execute(repo.pool())
    .await;

    assert!(
        result.is_err(),
        "inserting an orch_event with a dangling control_plane_id must be rejected by the FK constraint"
    );
}

#[tokio::test]
async fn test_orch_approvals_rejects_orphan_control_plane() {
    let repo = setup_test_db().await;
    let bogus_plane = Uuid::new_v4();

    let result = sqlx::query(
        "INSERT INTO orch_approvals (token, control_plane_id) VALUES ('approval-token-1', ?)",
    )
    .bind(bogus_plane.to_string())
    .execute(repo.pool())
    .await;

    assert!(
        result.is_err(),
        "inserting an orch_approval with a dangling control_plane_id must be rejected by the FK constraint"
    );
}

// ─── Multi-attempt redispatch: composite PK on orch_tasks ─────────────────

#[tokio::test]
async fn test_orch_tasks_composite_pk_allows_redispatch_same_item() {
    let repo = setup_test_db().await;
    let ws_id = common::create_test_workspace(&repo).await;
    let project = common::make_project(&repo, ws_id).await;
    let item = common::make_item(&repo, &project).await;

    // Same item, two different remote task ids (e.g. a retry) — must both succeed
    // because the PK is (item_id, remote_task_id), not item_id alone.
    sqlx::query(
        "INSERT INTO orch_tasks (item_id, remote_task_id, attempt) VALUES (?, 'task-a', 1)",
    )
    .bind(item.id.to_string())
    .execute(repo.pool())
    .await
    .expect("first dispatch");

    sqlx::query(
        "INSERT INTO orch_tasks (item_id, remote_task_id, attempt) VALUES (?, 'task-b', 2)",
    )
    .bind(item.id.to_string())
    .execute(repo.pool())
    .await
    .expect("redispatch with a new remote_task_id must succeed");

    // But the same (item_id, remote_task_id) pair twice must collide on the PK.
    let dup = sqlx::query(
        "INSERT INTO orch_tasks (item_id, remote_task_id, attempt) VALUES (?, 'task-a', 3)",
    )
    .bind(item.id.to_string())
    .execute(repo.pool())
    .await;
    assert!(
        dup.is_err(),
        "duplicate (item_id, remote_task_id) must be rejected by the composite primary key"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_tasks WHERE item_id = ?")
        .bind(item.id.to_string())
        .fetch_one(repo.pool())
        .await
        .expect("count tasks");
    assert_eq!(
        count, 2,
        "two distinct dispatches for the same item should both persist"
    );
}

// ─── Migrations 032-036, additive columns only ──────────────────
//
// Each column-existence test pins the migration *immediately before* the one
// under test as the "column must not exist yet" checkpoint, and the migration
// under test itself as the "column must exist now" checkpoint — the same
// `run_up_to` boundary-simulation pattern the 018-vs-full-set test above uses,
// just one migration name at a time instead of five at once.

#[tokio::test]
async fn test_migration_032_adds_control_planes_config_column() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    migrations::run_up_to(&pool, "031_items_completed_at_index")
        .await
        .expect("apply migrations up to 031");
    assert!(
        !column_exists(&pool, "control_planes", "config").await,
        "control_planes.config must not exist before migration 032"
    );

    migrations::run_up_to(&pool, "032_control_plane_config")
        .await
        .expect("apply migration 032");
    assert!(
        column_exists(&pool, "control_planes", "config").await,
        "control_planes.config must exist after migration 032"
    );
}

#[tokio::test]
async fn test_migration_033_adds_control_planes_secrets_column() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    migrations::run_up_to(&pool, "032_control_plane_config")
        .await
        .expect("apply migrations up to 032");
    assert!(
        !column_exists(&pool, "control_planes", "secrets").await,
        "control_planes.secrets must not exist before migration 033"
    );

    migrations::run_up_to(&pool, "033_control_plane_secrets")
        .await
        .expect("apply migration 033");
    assert!(
        column_exists(&pool, "control_planes", "secrets").await,
        "control_planes.secrets must exist after migration 033"
    );
}

#[tokio::test]
async fn test_migration_034_adds_items_version_column() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    migrations::run_up_to(&pool, "033_control_plane_secrets")
        .await
        .expect("apply migrations up to 033");
    assert!(
        !column_exists(&pool, "items", "version").await,
        "items.version must not exist before migration 034"
    );

    migrations::run_up_to(&pool, "034_items_version")
        .await
        .expect("apply migration 034");
    assert!(
        column_exists(&pool, "items", "version").await,
        "items.version must exist after migration 034"
    );
}

#[tokio::test]
async fn test_migration_035_adds_orch_links_version_column() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    migrations::run_up_to(&pool, "034_items_version")
        .await
        .expect("apply migrations up to 034");
    assert!(
        !column_exists(&pool, "orch_links", "version").await,
        "orch_links.version must not exist before migration 035"
    );

    migrations::run_up_to(&pool, "035_orch_links_version")
        .await
        .expect("apply migration 035");
    assert!(
        column_exists(&pool, "orch_links", "version").await,
        "orch_links.version must exist after migration 035"
    );
}

#[tokio::test]
async fn test_migration_036_adds_control_planes_version_column() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    migrations::run_up_to(&pool, "035_orch_links_version")
        .await
        .expect("apply migrations up to 035");
    assert!(
        !column_exists(&pool, "control_planes", "version").await,
        "control_planes.version must not exist before migration 036"
    );

    migrations::run_up_to(&pool, "036_control_planes_version")
        .await
        .expect("apply migration 036");
    assert!(
        column_exists(&pool, "control_planes", "version").await,
        "control_planes.version must exist after migration 036"
    );
}

#[tokio::test]
async fn test_fresh_db_migrates_all_the_way_through_036() {
    let repo = setup_test_db().await;

    for (table, column) in [
        ("control_planes", "config"),
        ("control_planes", "secrets"),
        ("control_planes", "version"),
        ("items", "version"),
        ("orch_links", "version"),
    ] {
        assert!(
            column_exists(repo.pool(), table, column).await,
            "expected {table}.{column} to exist after a fresh migration run"
        );
    }

    let applied: Vec<String> = sqlx::query("SELECT name FROM _migrations ORDER BY id")
        .fetch_all(repo.pool())
        .await
        .expect("select migrations")
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();

    assert!(
        applied.iter().any(|m| m == "036_control_planes_version"),
        "036_control_planes_version must have been applied on a fresh db"
    );
}

#[tokio::test]
async fn test_upgrade_from_031_applies_card_g5a_migrations_in_place() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    // Simulate an installed tack.db that has only ever seen migrations 001-031 —
    // i.e. everything up to but not including migrations 032-036.
    migrations::run_up_to(&pool, "031_items_completed_at_index")
        .await
        .expect("apply migrations up to 031");

    for (table, column) in [
        ("control_planes", "config"),
        ("control_planes", "secrets"),
        ("control_planes", "version"),
        ("items", "version"),
        ("orch_links", "version"),
    ] {
        assert!(
            !column_exists(&pool, table, column).await,
            "{table}.{column} must not exist before the upgrade runs"
        );
    }

    // Now run the full migration set again, as `tack serve` does on every startup.
    migrations::run_all(&pool).await.expect("upgrade in place");

    for (table, column) in [
        ("control_planes", "config"),
        ("control_planes", "secrets"),
        ("control_planes", "version"),
        ("items", "version"),
        ("orch_links", "version"),
    ] {
        assert!(
            column_exists(&pool, table, column).await,
            "{table}.{column} must exist after upgrading an existing db in place"
        );
    }
}

#[tokio::test]
async fn test_preexisting_rows_backfill_default_config_and_version() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    // Stop at 031 — one migration short of migrations 032-036 — and insert one
    // row per table the batch touches, via raw SQL (no repository layer call,
    // so nothing here depends on tack-db's Rust API already knowing about
    // columns this same migration run is about to add).
    migrations::run_up_to(&pool, "031_items_completed_at_index")
        .await
        .expect("apply migrations up to 031");

    let workspace_id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, name) VALUES (?, 'Pre-existing Workspace')")
        .bind(workspace_id.to_string())
        .execute(&pool)
        .await
        .expect("insert workspace");

    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (id, workspace_id, name) VALUES (?, ?, 'Pre-existing Project')",
    )
    .bind(project_id.to_string())
    .bind(workspace_id.to_string())
    .execute(&pool)
    .await
    .expect("insert project");

    let item_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO items (id, project_id, title, status) VALUES (?, ?, 'Pre-existing Item', 'todo')",
    )
    .bind(item_id.to_string())
    .bind(project_id.to_string())
    .execute(&pool)
    .await
    .expect("insert item");

    let plane_id = insert_control_plane(&pool).await;

    sqlx::query(
        "INSERT INTO orch_links (project_id, control_plane_id, remote_project) VALUES (?, ?, 'demo')",
    )
    .bind(project_id.to_string())
    .bind(plane_id.to_string())
    .execute(&pool)
    .await
    .expect("insert orch_link");

    // Now bring every pre-existing row through migrations 032-036.
    migrations::run_all(&pool).await.expect("upgrade in place");

    let item_version: i64 = sqlx::query_scalar("SELECT version FROM items WHERE id = ?")
        .bind(item_id.to_string())
        .fetch_one(&pool)
        .await
        .expect("select item version");
    assert_eq!(
        item_version, 1,
        "a pre-existing item must backfill to version 1, not 0 or NULL"
    );

    let link_version: i64 =
        sqlx::query_scalar("SELECT version FROM orch_links WHERE project_id = ?")
            .bind(project_id.to_string())
            .fetch_one(&pool)
            .await
            .expect("select orch_link version");
    assert_eq!(
        link_version, 1,
        "a pre-existing orch_link must backfill to version 1, not 0 or NULL"
    );

    let (plane_version, plane_config, plane_secrets): (i64, String, Option<String>) =
        sqlx::query_as("SELECT version, config, secrets FROM control_planes WHERE id = ?")
            .bind(plane_id.to_string())
            .fetch_one(&pool)
            .await
            .expect("select control_plane row");
    assert_eq!(
        plane_version, 1,
        "a pre-existing control_plane must backfill to version 1, not 0 or NULL"
    );
    assert_eq!(
        plane_config, "{}",
        "a pre-existing control_plane must backfill config to the empty-object default"
    );
    assert_eq!(
        plane_secrets, None,
        "a pre-existing control_plane must backfill secrets to NULL, never an empty value \
         that could be mistaken for 'no secrets configured yet' vs 'checked and empty'"
    );
}

// ─── Migrations 037-038, the two table rebuilds ─────────────────
//
// Unlike 032-036, these rewrite existing rows rather than only adding a
// column, so "the column/table exists afterwards" is not the bar — see the
// module doc comment above for the four things every test group here checks.

/// Inserts a bare workspace → project → item chain via raw SQL (mirroring
/// `test_preexisting_rows_backfill_default_config_and_version` above) and
/// returns the item id, for use as a valid `orch_runs`/`orch_approvals`
/// `item_id` FK target that predates the rebuild migrations.
async fn seed_item(pool: &sqlx::SqlitePool) -> Uuid {
    let workspace_id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, name) VALUES (?, 'G5b Workspace')")
        .bind(workspace_id.to_string())
        .execute(pool)
        .await
        .expect("insert workspace");

    let project_id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, workspace_id, name) VALUES (?, ?, 'G5b Project')")
        .bind(project_id.to_string())
        .bind(workspace_id.to_string())
        .execute(pool)
        .await
        .expect("insert project");

    let item_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO items (id, project_id, title, status) VALUES (?, ?, 'G5b Item', 'todo')",
    )
    .bind(item_id.to_string())
    .bind(project_id.to_string())
    .execute(pool)
    .await
    .expect("insert item");

    item_id
}

// ─── 037: orch_runs ────────────────────────────────────────────────────────

#[tokio::test]
async fn test_migration_037_renames_run_id_to_external_run_id_and_widens_the_pk() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    migrations::run_up_to(&pool, "036_control_planes_version")
        .await
        .expect("apply migrations up to 036");
    assert!(
        column_exists(&pool, "orch_runs", "run_id").await,
        "orch_runs.run_id must still exist before migration 037"
    );
    for column in ["external_run_id", "run_attempt", "correlation_id"] {
        assert!(
            !column_exists(&pool, "orch_runs", column).await,
            "orch_runs.{column} must not exist before migration 037"
        );
    }

    migrations::run_up_to(&pool, "037_orch_runs_rebuild")
        .await
        .expect("apply migration 037");

    assert!(
        !column_exists(&pool, "orch_runs", "run_id").await,
        "orch_runs.run_id must be gone after migration 037 — it is renamed, not duplicated \
         alongside external_run_id"
    );
    for column in ["external_run_id", "run_attempt", "correlation_id"] {
        assert!(
            column_exists(&pool, "orch_runs", column).await,
            "orch_runs.{column} must exist after migration 037"
        );
    }
}

#[tokio::test]
async fn test_migration_037_rebuild_preserves_every_row_and_field_equality() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_up_to(&pool, "036_control_planes_version")
        .await
        .expect("apply migrations up to 036");

    let plane_id = insert_control_plane(&pool).await;
    let item_id = seed_item(&pool).await;

    // One run correlated to an item, one unattributed (the "CLI
    // dispatch" case migration 022's own comment documents) — both must
    // survive the rebuild with every non-renamed column byte-for-byte intact.
    sqlx::query(
        "INSERT INTO orch_runs
            (run_id, control_plane_id, item_id, remote_project, source, state,
             started_at, ended_at, error, created_at, updated_at)
         VALUES ('run-attributed', ?, ?, 'demo', 'webhook', 'running',
                 '2026-01-01T00:00:00+00:00', NULL, NULL,
                 '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
    )
    .bind(plane_id.to_string())
    .bind(item_id.to_string())
    .execute(&pool)
    .await
    .expect("insert attributed run");

    sqlx::query(
        "INSERT INTO orch_runs
            (run_id, control_plane_id, item_id, remote_project, source, state,
             started_at, ended_at, error, created_at, updated_at)
         VALUES ('run-cli', ?, NULL, 'demo', 'cli', 'succeeded',
                 '2026-01-02T00:00:00+00:00', '2026-01-02T00:10:00+00:00', NULL,
                 '2026-01-02T00:00:00+00:00', '2026-01-02T00:10:00+00:00')",
    )
    .bind(plane_id.to_string())
    .execute(&pool)
    .await
    .expect("insert unattributed run");

    let count_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_runs")
        .fetch_one(&pool)
        .await
        .expect("count before");
    assert_eq!(count_before, 2);

    migrations::run_up_to(&pool, "037_orch_runs_rebuild")
        .await
        .expect("apply migration 037");

    let count_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_runs")
        .fetch_one(&pool)
        .await
        .expect("count after");
    assert_eq!(
        count_after, 2,
        "the rebuild must not drop or duplicate any row"
    );

    #[derive(sqlx::FromRow, Debug)]
    struct Row {
        external_run_id: String,
        control_plane_id: String,
        run_attempt: i64,
        correlation_id: Option<String>,
        item_id: Option<String>,
        remote_project: String,
        source: String,
        state: String,
        started_at: Option<String>,
        ended_at: Option<String>,
        error: Option<String>,
        created_at: String,
        updated_at: String,
    }

    const COLUMNS: &str = "external_run_id, control_plane_id, run_attempt, correlation_id, \
         item_id, remote_project, source, state, started_at, ended_at, error, created_at, \
         updated_at";

    let attributed: Row = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM orch_runs WHERE external_run_id = 'run-attributed'"
    )))
    .fetch_one(&pool)
    .await
    .expect("fetch attributed run");

    assert_eq!(
        attributed.external_run_id, "run-attributed",
        "external_run_id must survive the migration 037 rebuild unchanged, got: {:?}",
        attributed.external_run_id
    );
    assert_eq!(
        attributed.control_plane_id,
        plane_id.to_string(),
        "control_plane_id must survive the migration 037 rebuild unchanged, got: {:?}",
        attributed.control_plane_id
    );
    assert_eq!(
        attributed.run_attempt, 1,
        "a row that predates the attempt concept must backfill to attempt 1, not 0"
    );
    assert_eq!(
        attributed.correlation_id, None,
        "a row that predates Tack minting correlation ids must backfill to NULL, not an empty \
         string that could be mistaken for a minted-but-blank id"
    );
    assert_eq!(
        attributed.item_id.as_deref(),
        Some(item_id.to_string()).as_deref(),
        "item_id must still correlate the run to its item after the migration 037 rebuild, \
         got: {:?}",
        attributed.item_id
    );
    assert_eq!(
        attributed.remote_project, "demo",
        "remote_project must survive the migration 037 rebuild unchanged, got: {:?}",
        attributed.remote_project
    );
    assert_eq!(
        attributed.source, "webhook",
        "source must survive the migration 037 rebuild unchanged, got: {:?}",
        attributed.source
    );
    assert_eq!(
        attributed.state, "running",
        "state must survive the migration 037 rebuild unchanged, got: {:?}",
        attributed.state
    );
    assert_eq!(
        attributed.started_at.as_deref(),
        Some("2026-01-01T00:00:00+00:00"),
        "started_at must survive the migration 037 rebuild unchanged, got: {:?}",
        attributed.started_at
    );
    assert_eq!(
        attributed.ended_at, None,
        "ended_at must stay NULL across the migration 037 rebuild for a still-running row, \
         got: {:?}",
        attributed.ended_at
    );
    assert_eq!(
        attributed.error, None,
        "error must stay NULL across the migration 037 rebuild for a run with no error, \
         got: {:?}",
        attributed.error
    );
    assert_eq!(
        attributed.created_at, "2026-01-01T00:00:00+00:00",
        "created_at must survive the migration 037 rebuild unchanged, got: {:?}",
        attributed.created_at
    );
    assert_eq!(
        attributed.updated_at, "2026-01-01T00:00:00+00:00",
        "updated_at must survive the migration 037 rebuild unchanged, got: {:?}",
        attributed.updated_at
    );

    let cli_run: Row = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM orch_runs WHERE external_run_id = 'run-cli'"
    )))
    .fetch_one(&pool)
    .await
    .expect("fetch cli run");
    assert_eq!(
        cli_run.item_id, None,
        "an unattributed run must stay unattributed across the rebuild"
    );
    assert_eq!(
        cli_run.source, "cli",
        "source must survive the migration 037 rebuild unchanged for the unattributed run, \
         got: {:?}",
        cli_run.source
    );
    assert_eq!(
        cli_run.state, "succeeded",
        "state must survive the migration 037 rebuild unchanged for the unattributed run, \
         got: {:?}",
        cli_run.state
    );
    assert_eq!(
        cli_run.ended_at.as_deref(),
        Some("2026-01-02T00:10:00+00:00"),
        "ended_at must survive the migration 037 rebuild unchanged for the unattributed run, \
         got: {:?}",
        cli_run.ended_at
    );
}

#[tokio::test]
async fn test_migration_037_foreign_key_check_is_empty_after_rebuild() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_up_to(&pool, "036_control_planes_version")
        .await
        .expect("apply migrations up to 036");

    let plane_id = insert_control_plane(&pool).await;
    let item_id = seed_item(&pool).await;
    sqlx::query(
        "INSERT INTO orch_runs (run_id, control_plane_id, item_id, remote_project) \
         VALUES ('run-1', ?, ?, 'demo')",
    )
    .bind(plane_id.to_string())
    .bind(item_id.to_string())
    .execute(&pool)
    .await
    .expect("insert run");

    migrations::run_all(&pool).await.expect("upgrade in place");

    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .expect("run foreign_key_check");
    assert!(
        violations.is_empty(),
        "PRAGMA foreign_key_check must report no violations after the 037/038 rebuilds, got \
         {} row(s)",
        violations.len()
    );
}

#[tokio::test]
async fn test_migration_037_old_pk_uniqueness_still_enforced_for_a_single_attempt() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_up_to(&pool, "037_orch_runs_rebuild")
        .await
        .expect("apply migrations through 037");

    let plane_id = insert_control_plane(&pool).await;

    sqlx::query(
        "INSERT INTO orch_runs (control_plane_id, external_run_id, run_attempt, remote_project) \
         VALUES (?, 'run-dup', 1, 'demo')",
    )
    .bind(plane_id.to_string())
    .execute(&pool)
    .await
    .expect("first insert");

    let dup = sqlx::query(
        "INSERT INTO orch_runs (control_plane_id, external_run_id, run_attempt, remote_project) \
         VALUES (?, 'run-dup', 1, 'demo')",
    )
    .bind(plane_id.to_string())
    .execute(&pool)
    .await;
    assert!(
        dup.is_err(),
        "a second row with the same (control_plane_id, external_run_id, run_attempt) must be \
         rejected — this is exactly what the old single-column run_id PRIMARY KEY rejected for \
         the same (plane, run) pair, just expressed over the widened key"
    );

    // A genuinely different attempt of the same external run id is now
    // representable — proving the key was actually widened, not merely renamed.
    let retry = sqlx::query(
        "INSERT INTO orch_runs (control_plane_id, external_run_id, run_attempt, remote_project) \
         VALUES (?, 'run-dup', 2, 'demo')",
    )
    .bind(plane_id.to_string())
    .execute(&pool)
    .await;
    assert!(
        retry.is_ok(),
        "a second attempt of the same external_run_id must now be representable — that is the \
         entire point of widening the primary key: {retry:?}"
    );
}

// ─── 038: orch_approvals ────────────────────────────────────────────────────

#[tokio::test]
async fn test_migration_038_control_plane_id_becomes_nullable_and_gains_new_columns() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_up_to(&pool, "037_orch_runs_rebuild")
        .await
        .expect("apply migrations through 037");

    for column in ["kind", "external_id", "provider_metadata"] {
        assert!(
            !column_exists(&pool, "orch_approvals", column).await,
            "orch_approvals.{column} must not exist before migration 038"
        );
    }

    migrations::run_up_to(&pool, "038_orch_approvals_rebuild")
        .await
        .expect("apply migration 038");

    for column in ["kind", "external_id", "provider_metadata"] {
        assert!(
            column_exists(&pool, "orch_approvals", column).await,
            "orch_approvals.{column} must exist after migration 038"
        );
    }

    // The entire point of the rebuild: a decision with no control plane
    // behind it yet (a hook-raised decision from a never-dispatched run) must
    // now be insertable.
    let result = sqlx::query(
        "INSERT INTO orch_approvals (token, control_plane_id) VALUES ('tok-no-plane', NULL)",
    )
    .execute(&pool)
    .await;
    assert!(
        result.is_ok(),
        "control_plane_id must be nullable after migration 038: {result:?}"
    );
}

#[tokio::test]
async fn test_migration_038_rebuild_preserves_every_row_and_field_equality() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_up_to(&pool, "037_orch_runs_rebuild")
        .await
        .expect("apply migrations through 037");

    let plane_id = insert_control_plane(&pool).await;
    let item_id = seed_item(&pool).await;

    sqlx::query(
        "INSERT INTO orch_approvals
            (token, control_plane_id, item_id, remote_task_id, agent, action, state,
             requested_at, decided_at, created_at, updated_at)
         VALUES ('apr-1', ?, ?, 'task-1', 'implementer', 'delete prod table', 'pending',
                 '2026-01-01T00:00:00+00:00', NULL,
                 '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
    )
    .bind(plane_id.to_string())
    .bind(item_id.to_string())
    .execute(&pool)
    .await
    .expect("insert approval");

    migrations::run_up_to(&pool, "038_orch_approvals_rebuild")
        .await
        .expect("apply migration 038");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_approvals")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(count, 1, "the rebuild must not drop or duplicate any row");

    #[derive(sqlx::FromRow, Debug)]
    struct Row {
        token: String,
        control_plane_id: Option<String>,
        kind: String,
        external_id: Option<String>,
        provider_metadata: String,
        item_id: Option<String>,
        remote_task_id: Option<String>,
        agent: Option<String>,
        action: Option<String>,
        state: String,
        requested_at: String,
        decided_at: Option<String>,
    }

    let row: Row = sqlx::query_as(
        "SELECT token, control_plane_id, kind, external_id, provider_metadata, item_id, \
                remote_task_id, agent, action, state, requested_at, decided_at \
         FROM orch_approvals WHERE token = 'apr-1'",
    )
    .fetch_one(&pool)
    .await
    .expect("fetch approval");

    assert_eq!(row.token, "apr-1");
    assert_eq!(
        row.control_plane_id.as_deref(),
        Some(plane_id.to_string()).as_deref()
    );
    assert_eq!(
        row.kind, "approval",
        "every pre-existing approval predates any other kind and must backfill to 'approval'"
    );
    assert_eq!(row.external_id, None);
    assert_eq!(row.provider_metadata, "{}");
    assert_eq!(row.item_id.as_deref(), Some(item_id.to_string()).as_deref());
    assert_eq!(row.remote_task_id.as_deref(), Some("task-1"));
    assert_eq!(row.agent.as_deref(), Some("implementer"));
    assert_eq!(row.action.as_deref(), Some("delete prod table"));
    assert_eq!(row.state, "pending");
    assert_eq!(row.requested_at, "2026-01-01T00:00:00+00:00");
    assert_eq!(row.decided_at, None);
}

// ─── Upgrade-in-place and fresh-install coverage for both rebuilds ────────

#[tokio::test]
async fn test_upgrade_from_036_applies_card_g5b_rebuilds_in_place() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    migrations::run_up_to(&pool, "036_control_planes_version")
        .await
        .expect("apply migrations up to 036");

    assert!(column_exists(&pool, "orch_runs", "run_id").await);
    assert!(!column_exists(&pool, "orch_runs", "external_run_id").await);
    assert!(!column_exists(&pool, "orch_approvals", "kind").await);

    migrations::run_all(&pool).await.expect("upgrade in place");

    assert!(!column_exists(&pool, "orch_runs", "run_id").await);
    for column in ["external_run_id", "run_attempt", "correlation_id"] {
        assert!(column_exists(&pool, "orch_runs", column).await);
    }
    for column in ["kind", "external_id", "provider_metadata"] {
        assert!(column_exists(&pool, "orch_approvals", column).await);
    }
}

#[tokio::test]
async fn test_fresh_db_migrates_all_the_way_through_038() {
    let repo = setup_test_db().await;

    for (table, column) in [
        ("orch_runs", "external_run_id"),
        ("orch_runs", "run_attempt"),
        ("orch_runs", "correlation_id"),
        ("orch_approvals", "kind"),
        ("orch_approvals", "external_id"),
        ("orch_approvals", "provider_metadata"),
    ] {
        assert!(
            column_exists(repo.pool(), table, column).await,
            "expected {table}.{column} to exist after a fresh migration run"
        );
    }

    let applied: Vec<String> = sqlx::query("SELECT name FROM _migrations ORDER BY id")
        .fetch_all(repo.pool())
        .await
        .expect("select migrations")
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();

    assert!(
        applied.iter().any(|m| m == "038_orch_approvals_rebuild"),
        "038_orch_approvals_rebuild must have been applied on a fresh db"
    );
}

// ─── Rebuild recovery: stale staging and every statement failure ───────────

#[tokio::test]
async fn test_a_stale_orch_runs_staging_table_is_recovered_without_a_boot_loop() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_up_to(&pool, "036_control_planes_version")
        .await
        .expect("apply migrations up to 036");

    // A staging table left behind by a half-applied rebuild must not
    // brick the first repaired boot. The original remains authoritative.
    sqlx::query("CREATE TABLE orch_runs_new (external_run_id TEXT)")
        .execute(&pool)
        .await
        .expect("simulate the half-applied intermediate table");

    migrations::run_all(&pool)
        .await
        .expect("the repaired transactional rebuild must recover the stale staging table");

    let recorded: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = '037_orch_runs_rebuild')",
    )
    .fetch_one(&pool)
    .await
    .expect("check _migrations");
    assert!(
        recorded,
        "the rebuild record appears only once its recovery transaction commits"
    );

    assert!(
        table_exists(&pool, "orch_runs").await,
        "orch_runs must exist after a recovered rebuild"
    );
    assert!(!table_exists(&pool, "orch_runs_new").await);
}

#[tokio::test]
async fn test_a_stale_orch_approvals_staging_table_is_recovered_without_a_boot_loop() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_up_to(&pool, "037_orch_runs_rebuild")
        .await
        .expect("apply migrations through 037");

    sqlx::query("CREATE TABLE orch_approvals_new (token TEXT)")
        .execute(&pool)
        .await
        .expect("simulate the half-applied intermediate table");

    migrations::run_all(&pool)
        .await
        .expect("the repaired transactional rebuild must recover stale approval staging");

    let recorded: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = '038_orch_approvals_rebuild')",
    )
    .fetch_one(&pool)
    .await
    .expect("check _migrations");
    assert!(recorded);

    // 037 already landed cleanly before this test manufactured the stale 038
    // staging table; recovery must leave its committed record untouched.
    let migration_037_recorded: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = '037_orch_runs_rebuild')",
    )
    .fetch_one(&pool)
    .await
    .expect("check _migrations");
    assert!(
        migration_037_recorded,
        "037 was already applied before this test created the half-applied 038 state, and \
         must stay recorded"
    );

    assert!(
        table_exists(&pool, "orch_approvals").await,
        "orch_approvals must exist after recovery"
    );
    assert!(!table_exists(&pool, "orch_approvals_new").await);
}

async fn assert_injected_rebuild_failure_rolls_back(migration: &'static str, steps: usize) {
    for step in 0..steps {
        let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
        let cutoff = if migration == "037_orch_runs_rebuild" {
            "036_control_planes_version"
        } else {
            "037_orch_runs_rebuild"
        };
        migrations::run_up_to(&pool, cutoff)
            .await
            .expect("apply pre-rebuild schema");

        let source_table = if migration == "037_orch_runs_rebuild" {
            "orch_runs"
        } else {
            "orch_approvals"
        };
        let staging_table = if migration == "037_orch_runs_rebuild" {
            "orch_runs_new"
        } else {
            "orch_approvals_new"
        };

        let result = migrations::run_all_with_rebuild_failure(&pool, migration, step).await;
        assert!(
            result.is_err(),
            "failure injection at {migration} step {step} must fail"
        );
        assert!(
            table_exists(&pool, source_table).await,
            "source table must survive injected {migration} step {step}"
        );
        assert!(
            !table_exists(&pool, staging_table).await,
            "transaction rollback must remove staging after injected {migration} step {step}"
        );
        let recorded: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = ?)")
                .bind(migration)
                .fetch_one(&pool)
                .await
                .expect("check migration record");
        assert!(
            !recorded,
            "migration record must not exist before commit after injected {migration} step {step}"
        );

        migrations::run_all(&pool)
            .await
            .expect("a clean retry after every injected failure must recover");
    }
}

#[tokio::test]
async fn test_every_orch_runs_rebuild_statement_rolls_back_and_retries() {
    // Six SQL statements, copy verification, and fetched FK assertion.
    assert_injected_rebuild_failure_rolls_back("037_orch_runs_rebuild", 8).await;
}

#[tokio::test]
async fn test_every_orch_approvals_rebuild_statement_rolls_back_and_retries() {
    // Seven SQL statements, copy verification, and fetched FK assertion.
    assert_injected_rebuild_failure_rolls_back("038_orch_approvals_rebuild", 9).await;
}

#[tokio::test]
async fn test_rebuild_refuses_a_foreign_key_violation_before_source_deletion() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_up_to(&pool, "036_control_planes_version")
        .await
        .expect("apply pre-rebuild schema");

    // Manufacture legacy corruption on one connection. This is intentionally
    // outside normal repository behavior: the point is to prove the rebuild
    // fetches and asserts foreign_key_check rather than merely executing its
    // PRAGMA and ignoring the returned rows.
    let mut connection = pool.acquire().await.expect("acquire connection");
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&mut *connection)
        .await
        .expect("disable FK enforcement for corruption fixture");
    sqlx::query(
        "INSERT INTO orch_runs (run_id, control_plane_id, remote_project) \
         VALUES ('orphan-run', 'missing-plane', 'demo')",
    )
    .execute(&mut *connection)
    .await
    .expect("insert intentionally orphaned legacy row");
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&mut *connection)
        .await
        .expect("restore FK enforcement");
    drop(connection);

    let error = migrations::run_all(&pool)
        .await
        .expect_err("foreign_key_check must reject a corrupt rebuild source");
    assert!(error.to_string().contains("foreign_key_check"));
    assert!(table_exists(&pool, "orch_runs").await);
    assert!(!table_exists(&pool, "orch_runs_new").await);
    let recorded: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = '037_orch_runs_rebuild')",
    )
    .fetch_one(&pool)
    .await
    .expect("check migration record");
    assert!(!recorded, "a rejected rebuild must not be recorded");
}

#[tokio::test]
async fn test_migration_history_checksum_tampering_refuses_to_run() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_up_to(&pool, "036_control_planes_version")
        .await
        .expect("apply known prefix");
    sqlx::query("UPDATE _migrations SET checksum = 'tampered' WHERE name = '001_workspaces'")
        .execute(&pool)
        .await
        .expect("tamper checksum fixture");

    let error = migrations::run_all(&pool)
        .await
        .expect_err("an edited recorded migration must fail closed");
    assert!(error.to_string().contains("checksum changed"));
    assert!(
        !table_exists(&pool, "orch_runs_new").await,
        "the invariant is checked before a rebuild can make staging state"
    );
}

#[tokio::test]
async fn test_file_backed_rebuild_creates_an_automatic_pre_upgrade_snapshot() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let db_path = dir.path().join("pre-upgrade-snapshot.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    let pool = init_pool(&db_url).await.expect("file-backed pool");
    migrations::run_up_to(&pool, "036_control_planes_version")
        .await
        .expect("apply pre-rebuild schema");

    migrations::run_all(&pool)
        .await
        .expect("file-backed rebuild with snapshot");
    let backup_path = format!("{}.before-037_orch_runs_rebuild.sqlite", db_path.display());
    assert!(
        std::path::Path::new(&backup_path).is_file(),
        "the first pending rebuild must create a durable pre-upgrade SQLite snapshot"
    );
}
