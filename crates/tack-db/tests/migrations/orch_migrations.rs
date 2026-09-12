//! Tests for migrations 019-038: the Agent-Factory Control Center schema
//! (`control_planes`, `orch_links`, `orch_tasks`, `orch_runs`, `orch_events`,
//! `orch_approvals`), the additive columns from 032-036, and the two table
//! rebuilds 037 (`orch_runs`) and 038 (`orch_approvals`). Design:
//! `docs/plans/agnostic-control-plane.md` §4/§10.3; `migrations.rs` above
//! `MIGRATION_032`/`MIGRATION_037` explains why 032-036 are single ALTERs
//! while 037/038 rebuild the table.
//! Covers fresh install, upgrade-in-place per checkpoint, FK enforcement,
//! and for the rebuilds: row/field preservation, an empty
//! `foreign_key_check`, old-PK uniqueness, and crash-interrupted recovery.

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

/// Inserts a bare workspace -> project -> item chain via raw SQL and returns
/// the project and item ids, for use as valid FK targets that predate a
/// given migration.
async fn seed_item(pool: &sqlx::SqlitePool) -> (Uuid, Uuid) {
    let workspace_id = Uuid::new_v4();
    sqlx::query("INSERT INTO workspaces (id, name) VALUES (?, 'Rebuild Fixture Workspace')")
        .bind(workspace_id.to_string())
        .execute(pool)
        .await
        .expect("insert workspace");

    let project_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO projects (id, workspace_id, name) VALUES (?, ?, 'Rebuild Fixture Project')",
    )
    .bind(project_id.to_string())
    .bind(workspace_id.to_string())
    .execute(pool)
    .await
    .expect("insert project");

    let item_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO items (id, project_id, title, status) VALUES (?, ?, 'Rebuild Fixture Item', 'todo')",
    )
    .bind(item_id.to_string())
    .bind(project_id.to_string())
    .execute(pool)
    .await
    .expect("insert item");

    (project_id, item_id)
}

// ─── Fresh install ─────────────────────────────────────────────────────────

#[tokio::test]
async fn fresh_db_migrates_all_orch_tables() {
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
async fn upgrade_from_018_applies_new_orch_migrations_in_place() {
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
        "at least the first 24 migrations should be recorded as applied, got {count}"
    );
}

// ─── FK enforcement on the new tables ──────────────────────────────────────

#[tokio::test]
async fn orphan_fk_insert_is_rejected() {
    let repo = setup_test_db().await;
    let plane_id = insert_control_plane(repo.pool()).await;
    let bogus = Uuid::new_v4();

    let cases = [
        (
            "orch_links.project_id",
            format!(
                "INSERT INTO orch_links (project_id, control_plane_id, remote_project) \
                 VALUES ('{bogus}', '{plane_id}', 'demo')"
            ),
        ),
        (
            "orch_tasks.item_id",
            format!(
                "INSERT INTO orch_tasks (item_id, remote_task_id) \
                 VALUES ('{bogus}', 'remote-task-1')"
            ),
        ),
        (
            "orch_runs.control_plane_id",
            format!(
                "INSERT INTO orch_runs (run_id, control_plane_id, remote_project) \
                 VALUES ('{}', '{bogus}', 'demo')",
                Uuid::new_v4()
            ),
        ),
        (
            "orch_events.control_plane_id",
            format!(
                "INSERT INTO orch_events (id, control_plane_id, event_type) \
                 VALUES ('{}', '{bogus}', 'tool_call')",
                Uuid::new_v4()
            ),
        ),
        (
            "orch_approvals.control_plane_id",
            format!(
                "INSERT INTO orch_approvals (token, control_plane_id) \
                 VALUES ('approval-token-1', '{bogus}')"
            ),
        ),
    ];

    for (label, sql) in cases {
        let result = sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(repo.pool())
            .await;
        assert!(result.is_err(), "{label}: dangling FK must be rejected");
    }
}

// ─── Multi-attempt redispatch: composite PK on orch_tasks ─────────────────

#[tokio::test]
async fn orch_tasks_composite_pk_allows_redispatch_same_item() {
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
// Each case pins the migration *immediately before* the one under test as
// the "column must not exist yet" checkpoint, and the migration under test
// itself as the "column must exist now" checkpoint.

struct ColumnAddCase {
    before: &'static str,
    after: &'static str,
    table: &'static str,
    column: &'static str,
}

const COLUMN_ADD_CASES: &[ColumnAddCase] = &[
    ColumnAddCase {
        before: "031_items_completed_at_index",
        after: "032_control_plane_config",
        table: "control_planes",
        column: "config",
    },
    ColumnAddCase {
        before: "032_control_plane_config",
        after: "033_control_plane_secrets",
        table: "control_planes",
        column: "secrets",
    },
    ColumnAddCase {
        before: "033_control_plane_secrets",
        after: "034_items_version",
        table: "items",
        column: "version",
    },
    ColumnAddCase {
        before: "034_items_version",
        after: "035_orch_links_version",
        table: "orch_links",
        column: "version",
    },
    ColumnAddCase {
        before: "035_orch_links_version",
        after: "036_control_planes_version",
        table: "control_planes",
        column: "version",
    },
];

#[tokio::test]
async fn migrations_032_to_036_each_add_one_column() {
    for case in COLUMN_ADD_CASES {
        let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
        migrations::run_up_to(&pool, case.before)
            .await
            .expect("apply migrations before the target");
        assert!(
            !column_exists(&pool, case.table, case.column).await,
            "{}.{} must not exist before {}",
            case.table,
            case.column,
            case.after
        );

        migrations::run_up_to(&pool, case.after)
            .await
            .expect("apply the target migration");
        assert!(
            column_exists(&pool, case.table, case.column).await,
            "{}.{} must exist after {}",
            case.table,
            case.column,
            case.after
        );
    }
}

#[tokio::test]
async fn fresh_db_migrates_through_036() {
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
async fn upgrade_from_031_applies_032_through_036_in_place() {
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

/// Asserts the `version` column selected by `sql` (bound to `id`) backfilled
/// to `1`, never `0` or `NULL` — used for both `items.version` and
/// `orch_links.version` in the migration 032-036 backfill test.
async fn assert_version_backfilled_to_one(
    pool: &sqlx::SqlitePool,
    sql: &'static str,
    id: &str,
    label: &str,
) {
    let version: i64 = sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("select version");
    assert_eq!(
        version, 1,
        "a pre-existing {label} must backfill to version 1, not 0 or NULL"
    );
}

#[tokio::test]
async fn preexisting_rows_backfill_default_config_and_version() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");

    // Stop at 031 — one migration short of migrations 032-036 — and insert one
    // row per table the batch touches, via raw SQL (no repository layer call,
    // so nothing here depends on tack-db's Rust API already knowing about
    // columns this same migration run is about to add).
    migrations::run_up_to(&pool, "031_items_completed_at_index")
        .await
        .expect("apply migrations up to 031");

    let (project_id, item_id) = seed_item(&pool).await;
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

    assert_version_backfilled_to_one(
        &pool,
        "SELECT version FROM items WHERE id = ?",
        &item_id.to_string(),
        "item",
    )
    .await;
    assert_version_backfilled_to_one(
        &pool,
        "SELECT version FROM orch_links WHERE project_id = ?",
        &project_id.to_string(),
        "orch_link",
    )
    .await;

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

// ─── 037: orch_runs ────────────────────────────────────────────────────────

#[derive(sqlx::FromRow, Debug, PartialEq)]
struct OrchRun037 {
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

async fn count_orch_runs(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM orch_runs")
        .fetch_one(pool)
        .await
        .expect("count orch_runs")
}

/// Fetches one post-037 `orch_runs` row by its new `external_run_id`, for the
/// rebuild's row/field-preservation test.
async fn fetch_037_run(pool: &sqlx::SqlitePool, external_run_id: &str) -> OrchRun037 {
    const COLUMNS: &str = "external_run_id, control_plane_id, run_attempt, correlation_id, \
         item_id, remote_project, source, state, started_at, ended_at, error, created_at, \
         updated_at";
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM orch_runs WHERE external_run_id = '{external_run_id}'"
    )))
    .fetch_one(pool)
    .await
    .expect("fetch orch_runs row")
}

struct RunFixture037 {
    run_id: &'static str,
    item_id: Option<Uuid>,
    source: &'static str,
    state: &'static str,
    started_at: &'static str,
    ended_at: Option<&'static str>,
}

impl RunFixture037 {
    /// The row migration 037 must produce from this fixture: `created_at`
    /// always equals `started_at`; `updated_at` equals `ended_at` once the
    /// run has finished, `started_at` otherwise; `run_attempt` and
    /// `correlation_id` backfill to `1` and `NULL` for every pre-037 row.
    fn expected(&self, plane_id: Uuid) -> OrchRun037 {
        OrchRun037 {
            external_run_id: self.run_id.into(),
            control_plane_id: plane_id.to_string(),
            run_attempt: 1,
            correlation_id: None,
            item_id: self.item_id.map(|id| id.to_string()),
            remote_project: "demo".into(),
            source: self.source.into(),
            state: self.state.into(),
            started_at: Some(self.started_at.into()),
            ended_at: self.ended_at.map(Into::into),
            error: None,
            created_at: self.started_at.into(),
            updated_at: self.ended_at.unwrap_or(self.started_at).into(),
        }
    }
}

/// Inserts one pre-037 `orch_runs` row per [`RunFixture037::expected`].
async fn insert_pre_037_run(pool: &sqlx::SqlitePool, plane_id: Uuid, fx: &RunFixture037) {
    sqlx::query(
        "INSERT INTO orch_runs
            (run_id, control_plane_id, item_id, remote_project, source, state,
             started_at, ended_at, error, created_at, updated_at)
         VALUES (?, ?, ?, 'demo', ?, ?, ?, ?, NULL, ?, ?)",
    )
    .bind(fx.run_id)
    .bind(plane_id.to_string())
    .bind(fx.item_id.map(|id| id.to_string()))
    .bind(fx.source)
    .bind(fx.state)
    .bind(fx.started_at)
    .bind(fx.ended_at)
    .bind(fx.started_at)
    .bind(fx.ended_at.unwrap_or(fx.started_at))
    .execute(pool)
    .await
    .expect("insert orch_runs fixture row");
}

#[tokio::test]
async fn migration_037_renames_run_id_widens_primary_key() {
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
async fn migration_037_rebuild_preserves_rows_and_fields() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_up_to(&pool, "036_control_planes_version")
        .await
        .expect("apply migrations up to 036");

    let plane_id = insert_control_plane(&pool).await;
    let (_, item_id) = seed_item(&pool).await;

    // One run correlated to an item, one unattributed (the "CLI dispatch"
    // case migration 022's own comment documents) — both must survive the
    // rebuild with every non-renamed column byte-for-byte intact.
    let fixtures = [
        RunFixture037 {
            run_id: "run-attributed",
            item_id: Some(item_id),
            source: "webhook",
            state: "running",
            started_at: "2026-01-01T00:00:00+00:00",
            ended_at: None,
        },
        RunFixture037 {
            run_id: "run-cli",
            item_id: None,
            source: "cli",
            state: "succeeded",
            started_at: "2026-01-02T00:00:00+00:00",
            ended_at: Some("2026-01-02T00:10:00+00:00"),
        },
    ];
    for fx in &fixtures {
        insert_pre_037_run(&pool, plane_id, fx).await;
    }
    let seeded = count_orch_runs(&pool).await;
    assert_eq!(seeded, 2, "both fixture rows must be seeded");

    migrations::run_up_to(&pool, "037_orch_runs_rebuild")
        .await
        .expect("apply migration 037");

    let after_rebuild = count_orch_runs(&pool).await;
    assert_eq!(
        after_rebuild, 2,
        "the rebuild must not drop or duplicate any row"
    );

    // Every field compared at once via derived `PartialEq` against the
    // fixture's own backfill rule — a mismatch on any one field prints both
    // full rows so the diff is still readable.
    for fx in &fixtures {
        let actual = fetch_037_run(&pool, fx.run_id).await;
        assert_eq!(
            actual,
            fx.expected(plane_id),
            "{} must survive the migration 037 rebuild field-for-field",
            fx.run_id
        );
    }
}

#[tokio::test]
async fn migration_037_foreign_key_check_is_empty_after_rebuild() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_up_to(&pool, "036_control_planes_version")
        .await
        .expect("apply migrations up to 036");

    let plane_id = insert_control_plane(&pool).await;
    let (_, item_id) = seed_item(&pool).await;
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
async fn migration_037_pk_uniqueness_enforced_per_attempt() {
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

#[derive(sqlx::FromRow, Debug)]
struct OrchApproval038 {
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

/// Fetches one post-038 `orch_approvals` row by its token, for the rebuild's
/// row/field-preservation test.
async fn fetch_038_approval(pool: &sqlx::SqlitePool, token: &str) -> OrchApproval038 {
    sqlx::query_as(
        "SELECT token, control_plane_id, kind, external_id, provider_metadata, item_id, \
                remote_task_id, agent, action, state, requested_at, decided_at \
         FROM orch_approvals WHERE token = ?",
    )
    .bind(token)
    .fetch_one(pool)
    .await
    .expect("fetch orch_approvals row")
}

#[tokio::test]
async fn migration_038_control_plane_id_nullable_and_gains_columns() {
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
async fn migration_038_rebuild_preserves_rows_and_fields() {
    let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
    migrations::run_up_to(&pool, "037_orch_runs_rebuild")
        .await
        .expect("apply migrations through 037");

    let plane_id = insert_control_plane(&pool).await;
    let (_, item_id) = seed_item(&pool).await;

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

    let row = fetch_038_approval(&pool, "apr-1").await;
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
async fn upgrade_from_036_applies_037_and_038_rebuilds_in_place() {
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
async fn fresh_db_migrates_through_038() {
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

struct StaleStagingCase {
    run_up_to: &'static str,
    create_staging_sql: &'static str,
    staging_table: &'static str,
    migration: &'static str,
    surviving_table: &'static str,
    prior_migrations_stay_recorded: &'static [&'static str],
}

const STALE_STAGING_CASES: &[StaleStagingCase] = &[
    StaleStagingCase {
        run_up_to: "036_control_planes_version",
        create_staging_sql: "CREATE TABLE orch_runs_new (external_run_id TEXT)",
        staging_table: "orch_runs_new",
        migration: "037_orch_runs_rebuild",
        surviving_table: "orch_runs",
        prior_migrations_stay_recorded: &[],
    },
    StaleStagingCase {
        run_up_to: "037_orch_runs_rebuild",
        create_staging_sql: "CREATE TABLE orch_approvals_new (token TEXT)",
        staging_table: "orch_approvals_new",
        migration: "038_orch_approvals_rebuild",
        surviving_table: "orch_approvals",
        prior_migrations_stay_recorded: &["037_orch_runs_rebuild"],
    },
];

#[tokio::test]
async fn stale_rebuild_staging_table_recovers_without_boot_loop() {
    for case in STALE_STAGING_CASES {
        let pool = init_pool("sqlite::memory:").await.expect("in-memory pool");
        migrations::run_up_to(&pool, case.run_up_to)
            .await
            .expect("apply pre-rebuild schema");

        // A staging table left behind by a half-applied rebuild must not
        // brick the first repaired boot. The original remains authoritative.
        sqlx::query(case.create_staging_sql)
            .execute(&pool)
            .await
            .expect("simulate the half-applied intermediate table");

        migrations::run_all(&pool)
            .await
            .expect("the repaired transactional rebuild must recover the stale staging table");

        let mut migrations_to_check = vec![case.migration];
        migrations_to_check.extend(case.prior_migrations_stay_recorded);
        for migration in migrations_to_check {
            let recorded: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = ?)")
                    .bind(migration)
                    .fetch_one(&pool)
                    .await
                    .expect("check _migrations");
            assert!(recorded, "{migration} must be recorded after recovery");
        }

        assert!(table_exists(&pool, case.surviving_table).await);
        assert!(!table_exists(&pool, case.staging_table).await);
    }
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
async fn rebuild_failure_at_every_step_rolls_back_and_retries() {
    // 037: six SQL statements, copy verification, and fetched FK assertion.
    // 038: seven SQL statements, copy verification, and fetched FK assertion.
    for (migration, steps) in [
        ("037_orch_runs_rebuild", 8),
        ("038_orch_approvals_rebuild", 9),
    ] {
        assert_injected_rebuild_failure_rolls_back(migration, steps).await;
    }
}

#[tokio::test]
async fn rebuild_refuses_foreign_key_violation_before_deletion() {
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
async fn migration_history_checksum_tampering_refuses_to_run() {
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
async fn file_backed_rebuild_creates_pre_upgrade_snapshot() {
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
