use sqlx::{AssertSqlSafe, Row, SqlitePool};
use tracing::{info, instrument};

/// A migration is either ordinary, all of whose SQL runs in one transaction, or
/// a table rebuild. Rebuilds are still transactional, but additionally prove the
/// copy before the source table is removed.
#[derive(Clone, Copy)]
enum MigrationKind {
    Ordinary(&'static [&'static str]),
    Rebuild(RebuildMigration),
}

#[derive(Clone, Copy)]
struct Migration {
    name: &'static str,
    kind: MigrationKind,
}

#[derive(Clone, Copy)]
struct RebuildMigration {
    source: &'static str,
    staging: &'static str,
    statements: &'static [&'static str],
    copy_step: usize,
    /// Paired source/new-table projections. The first query is evaluated
    /// against the source and the second against the staging table. The sets
    /// must be identical after the copy; count equality separately preserves
    /// duplicate detection too.
    source_projection: &'static str,
    staging_projection: &'static str,
}

fn ordinary(name: &'static str, statements: &'static [&'static str]) -> Migration {
    Migration {
        name,
        kind: MigrationKind::Ordinary(statements),
    }
}

fn rebuild(name: &'static str, spec: RebuildMigration) -> Migration {
    Migration {
        name,
        kind: MigrationKind::Rebuild(spec),
    }
}

/// The full, ordered migration list. Order matters: e.g. sprints before items
/// (items references sprints), and control_planes before the orch_* tables that
/// reference it. The name plus exact SQL are checksummed before any work begins,
/// so a reordered or edited already-recorded migration cannot silently run.
fn all_migrations() -> Vec<Migration> {
    vec![
        ordinary("001_workspaces", &MIGRATION_001[..]),
        ordinary("002_projects", &MIGRATION_002[..]),
        ordinary("003_sprints", &MIGRATION_003_SPRINTS[..]),
        ordinary("004_items", &MIGRATION_004_ITEMS[..]),
        ordinary("005_dependencies", &MIGRATION_005[..]),
        ordinary("006_roles", &MIGRATION_006[..]),
        ordinary("007_comments", &MIGRATION_007[..]),
        ordinary("008_attachments", &MIGRATION_008[..]),
        ordinary("009_board_views", &MIGRATION_009[..]),
        ordinary("010_fts", &MIGRATION_010[..]),
        ordinary("011_project_templates", &MIGRATION_011[..]),
        ordinary("012_custom_fields", &MIGRATION_012[..]),
        ordinary("013_boards", &MIGRATION_013[..]),
        ordinary("014_consolidate_boards", &MIGRATION_014[..]),
        ordinary("015_item_assignee", &MIGRATION_015[..]),
        ordinary("016_perf_indexes", &MIGRATION_016[..]),
        ordinary("017_app_meta", &MIGRATION_017[..]),
        ordinary("018_github_links", &MIGRATION_018[..]),
        ordinary("019_control_planes", &MIGRATION_019[..]),
        ordinary("020_orch_links", &MIGRATION_020[..]),
        ordinary("021_orch_tasks", &MIGRATION_021[..]),
        ordinary("022_orch_runs", &MIGRATION_022[..]),
        ordinary("023_orch_events", &MIGRATION_023[..]),
        ordinary("024_orch_approvals", &MIGRATION_024[..]),
        ordinary("025_orch_metrics", &MIGRATION_025[..]),
        ordinary("026_orch_events_daily", &MIGRATION_026[..]),
        ordinary("027_orch_metrics_daily", &MIGRATION_027[..]),
        ordinary("028_orch_trace_cursors", &MIGRATION_028[..]),
        ordinary("029_item_source", &MIGRATION_029[..]),
        ordinary("030_template_orchestration", &MIGRATION_030[..]),
        ordinary("031_items_completed_at_index", &MIGRATION_031[..]),
        ordinary("032_control_plane_config", &MIGRATION_032[..]),
        ordinary("033_control_plane_secrets", &MIGRATION_033[..]),
        ordinary("034_items_version", &MIGRATION_034[..]),
        ordinary("035_orch_links_version", &MIGRATION_035[..]),
        ordinary("036_control_planes_version", &MIGRATION_036[..]),
        rebuild("037_orch_runs_rebuild", MIGRATION_037),
        rebuild("038_orch_approvals_rebuild", MIGRATION_038),
        ordinary("039_agent_fleets", &MIGRATION_039[..]),
        ordinary("040_agent_runners", &MIGRATION_040[..]),
        ordinary("041_agent_fleet_members", &MIGRATION_041[..]),
        ordinary("042_agent_profiles", &MIGRATION_042[..]),
        ordinary("043_model_profiles", &MIGRATION_043[..]),
        ordinary("044_execution_requests", &MIGRATION_044[..]),
        ordinary("045_execution_attempts", &MIGRATION_045[..]),
        ordinary("046_execution_events", &MIGRATION_046[..]),
        ordinary("047_execution_artifacts", &MIGRATION_047[..]),
        ordinary("048_execution_decisions", &MIGRATION_048[..]),
        ordinary("049_runner_credentials_and_enrollment", &MIGRATION_049[..]),
        ordinary("050_execution_claim_replays", &MIGRATION_050[..]),
        ordinary("051_execution_recovery_audits", &MIGRATION_051[..]),
        ordinary("052_execution_report_replays", &MIGRATION_052[..]),
        ordinary("053_execution_request_snapshot", &MIGRATION_053[..]),
        ordinary("054_execution_event_batch_replays", &MIGRATION_054[..]),
        ordinary("055_execution_completion_replays", &MIGRATION_055[..]),
        ordinary(
            "056_execution_heartbeat_replay_fingerprint",
            &MIGRATION_056[..],
        ),
        ordinary(
            "057_execution_cancellation_replay_response",
            &MIGRATION_057[..],
        ),
        ordinary("058_execution_recovery_replay_response", &MIGRATION_058[..]),
        ordinary(
            "059_quarantine_legacy_execution_request_snapshots",
            &MIGRATION_059[..],
        ),
        ordinary(
            "060_quarantine_malformed_execution_request_snapshots",
            &MIGRATION_060[..],
        ),
        ordinary("061_execution_attempt_start_facts", &MIGRATION_061[..]),
        ordinary("062_project_default_model", &MIGRATION_062[..]),
    ]
}

/// Run all migrations in order.
#[instrument(skip(pool))]
pub async fn run_all(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    info!("Running database migrations...");
    ensure_migrations_table(pool).await?;
    let migrations = all_migrations();
    verify_applied_migration_invariant(pool, &migrations).await?;
    create_pre_upgrade_backup_if_needed(pool, &migrations).await?;
    apply_migrations(pool, &migrations, None).await?;
    info!("All migrations applied");
    Ok(())
}

/// Apply migrations only up to and including `cutoff` (by name). Exists so tests can
/// simulate an existing database stopped at a known migration boundary (e.g. an
/// installed `tack.db` still on "018_github_links") and then call [`run_all`] again to
/// prove the upgrade-in-place path works, without needing a real on-disk fixture file.
///
/// Panics if `cutoff` does not match a known migration name.
pub async fn run_up_to(pool: &SqlitePool, cutoff: &str) -> Result<(), sqlx::Error> {
    ensure_migrations_table(pool).await?;
    let migrations = all_migrations();
    let idx = migrations
        .iter()
        .position(|migration| migration.name == cutoff)
        .unwrap_or_else(|| panic!("run_up_to: unknown migration name {cutoff:?}"));
    verify_applied_migration_invariant(pool, &migrations).await?;
    apply_migrations(pool, &migrations[..=idx], None).await
}

async fn ensure_migrations_table(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            checksum TEXT,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )
    .execute(pool)
    .await?;
    let has_checksum = sqlx::query("PRAGMA table_info(_migrations)")
        .fetch_all(pool)
        .await?
        .iter()
        .any(|row| row.get::<String, _>("name") == "checksum");
    if !has_checksum {
        // Existing installs predate the invariant. This is an internal metadata
        // upgrade, intentionally completed before the ordered migration stream.
        sqlx::query("ALTER TABLE _migrations ADD COLUMN checksum TEXT")
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// A deliberately tiny deterministic checksum, used as an integrity fingerprint
/// rather than a cryptographic signature. It detects accidental edit/reorder of
/// migration definitions without adding a new dependency to the DB crate.
fn migration_checksum(migration: Migration) -> String {
    let mut hash = 0xcbf29ce484222325_u64; // FNV-1a offset basis
    let mut write = |value: &str| {
        for byte in value.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    };
    write(migration.name);
    match migration.kind {
        MigrationKind::Ordinary(statements) => {
            write("ordinary");
            for statement in statements {
                write(statement);
            }
        }
        MigrationKind::Rebuild(spec) => {
            write("rebuild");
            write(spec.source);
            write(spec.staging);
            for statement in spec.statements {
                write(statement);
            }
            write(&spec.copy_step.to_string());
            write(spec.source_projection);
            write(spec.staging_projection);
        }
    }
    format!("fnv1a64:{hash:016x}")
}

/// Ensures the recorded migrations are an exact prefix of this binary's ordered
/// list. Old databases with NULL checksums are adopted once, then all future
/// boots verify both order and contents before issuing any schema SQL.
async fn verify_applied_migration_invariant(
    pool: &SqlitePool,
    migrations: &[Migration],
) -> Result<(), sqlx::Error> {
    let applied = sqlx::query("SELECT id, name, checksum FROM _migrations ORDER BY id")
        .fetch_all(pool)
        .await?;

    for (index, row) in applied.iter().enumerate() {
        let id: i64 = row.get("id");
        let name: String = row.get("name");
        let stored_checksum: Option<String> = row.get("checksum");
        let expected = migrations.get(index).ok_or_else(|| {
            sqlx::Error::Protocol(format!(
                "migration history contains unexpected entry {name:?} at position {index}"
            ))
        })?;
        if name != expected.name {
            return Err(sqlx::Error::Protocol(format!(
                "migration history is out of order at position {index}: recorded {name:?}, \
                 expected {:?}; restore a database with an intact _migrations history",
                expected.name
            )));
        }
        let checksum = migration_checksum(*expected);
        match stored_checksum {
            Some(stored) if stored != checksum => {
                return Err(sqlx::Error::Protocol(format!(
                    "migration {:?} checksum changed (recorded {stored}, expected {checksum}); \
                     refusing to run edited migration history",
                    expected.name
                )));
            }
            Some(_) => {}
            None => {
                sqlx::query("UPDATE _migrations SET checksum = ? WHERE id = ?")
                    .bind(checksum)
                    .bind(id)
                    .execute(pool)
                    .await?;
            }
        }
    }
    Ok(())
}

async fn create_pre_upgrade_backup_if_needed(
    pool: &SqlitePool,
    migrations: &[Migration],
) -> Result<(), sqlx::Error> {
    let applied: Vec<String> = sqlx::query_scalar("SELECT name FROM _migrations")
        .fetch_all(pool)
        .await?;
    let Some(rebuild) = migrations.iter().find(|migration| {
        matches!(migration.kind, MigrationKind::Rebuild(_))
            && !applied.iter().any(|name| name == migration.name)
    }) else {
        return Ok(());
    };

    let database_list = sqlx::query("PRAGMA database_list").fetch_all(pool).await?;
    let database_file = database_list
        .iter()
        .find(|row| row.get::<String, _>("name") == "main")
        .map(|row| row.get::<String, _>("file"))
        .unwrap_or_default();
    if database_file.is_empty() {
        // In-memory databases have no durable source file and are used by tests;
        // there is nothing useful for SQLite to snapshot.
        return Ok(());
    }

    let backup_path = format!("{database_file}.before-{}.sqlite", rebuild.name);
    // VACUUM INTO creates a transactionally consistent SQLite snapshot. Do not
    // overwrite it: after a failed attempt the first pre-upgrade image is the
    // recovery artifact, not disposable cache. A pre-existing file therefore
    // fulfils the contract on retry.
    match sqlx::query("VACUUM main INTO ?")
        .bind(&backup_path)
        .execute(pool)
        .await
    {
        Ok(_) => info!(
            migration = rebuild.name,
            backup_path, "Created automatic pre-upgrade backup"
        ),
        Err(error) if error.to_string().contains("already exists") => {
            info!(
                migration = rebuild.name,
                backup_path, "Reusing automatic pre-upgrade backup"
            )
        }
        Err(error) => return Err(error),
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct FailureInjection {
    migration: &'static str,
    step: usize,
}

/// Test-only control surface used by integration tests to prove every rebuild
/// boundary rolls back. It deliberately injects before a named rebuild step;
/// production callers must use [`run_all`].
#[doc(hidden)]
pub async fn run_all_with_rebuild_failure(
    pool: &SqlitePool,
    migration: &'static str,
    step: usize,
) -> Result<(), sqlx::Error> {
    ensure_migrations_table(pool).await?;
    let migrations = all_migrations();
    verify_applied_migration_invariant(pool, &migrations).await?;
    apply_migrations(
        pool,
        &migrations,
        Some(FailureInjection { migration, step }),
    )
    .await
}

async fn apply_migrations(
    pool: &SqlitePool,
    migrations: &[Migration],
    failure: Option<FailureInjection>,
) -> Result<(), sqlx::Error> {
    for migration in migrations {
        let already_applied: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM _migrations WHERE name = ?)")
                .bind(migration.name)
                .fetch_one(pool)
                .await?;

        if !already_applied {
            info!(migration = migration.name, "Applying migration");
            match migration.kind {
                MigrationKind::Ordinary(statements) => {
                    apply_ordinary_migration(pool, *migration, statements).await?
                }
                MigrationKind::Rebuild(spec) => {
                    apply_rebuild_migration(pool, *migration, spec, failure).await?
                }
            }
            info!(migration = migration.name, "Migration applied successfully");
        }
    }
    Ok(())
}

async fn apply_ordinary_migration(
    pool: &SqlitePool,
    migration: Migration,
    statements: &'static [&'static str],
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let result = async {
        for statement in statements {
            sqlx::query(*statement).execute(&mut *tx).await.map_err(|error| {
                tracing::error!(migration = migration.name, statement, error = %error, "Migration failed");
                error
            })?;
        }
        record_migration(&mut tx, migration).await
    }
    .await;
    match result {
        Ok(()) => tx.commit().await,
        Err(error) => {
            tx.rollback().await?;
            Err(error)
        }
    }
}

async fn apply_rebuild_migration(
    pool: &SqlitePool,
    migration: Migration,
    spec: RebuildMigration,
    failure: Option<FailureInjection>,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let result = async {
        // Keep FK enforcement enabled, but defer it long enough to inspect every
        // violation ourselves. Otherwise a corrupt historical source can make the
        // INSERT fail before `foreign_key_check` is fetched and asserted.
        sqlx::query("PRAGMA defer_foreign_keys=ON")
            .execute(&mut *tx)
            .await?;
        for (step, statement) in spec.statements.iter().enumerate() {
            inject_failure(failure, migration.name, step)?;
            sqlx::query(*statement).execute(&mut *tx).await.map_err(|error| {
                tracing::error!(migration = migration.name, statement, error = %error, "Rebuild migration failed");
                error
            })?;
            if step == spec.copy_step {
                inject_failure(failure, migration.name, spec.statements.len())?;
                verify_copy(&mut tx, migration.name, spec).await?;
            }
        }
        let fk_step = spec.statements.len() + 1;
        inject_failure(failure, migration.name, fk_step)?;
        assert_foreign_key_check(&mut tx, migration.name).await?;
        record_migration(&mut tx, migration).await
    }
    .await;

    match result {
        Ok(()) => tx.commit().await,
        Err(error) => {
            // A retry may happen immediately. Complete rollback before
            // releasing the connection instead of relying on drop's async
            // cleanup, which can otherwise leave a transient schema lock.
            tx.rollback().await?;
            Err(error)
        }
    }
}

fn inject_failure(
    failure: Option<FailureInjection>,
    migration: &str,
    step: usize,
) -> Result<(), sqlx::Error> {
    if failure.is_some_and(|failure| failure.migration == migration && failure.step == step) {
        return Err(sqlx::Error::Protocol(format!(
            "injected failure before rebuild migration {migration} step {step}"
        )));
    }
    Ok(())
}

async fn verify_copy(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    migration: &str,
    spec: RebuildMigration,
) -> Result<(), sqlx::Error> {
    let source_count: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM {}",
        spec.source
    )))
    .fetch_one(&mut **tx)
    .await?;
    let staging_count: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM {}",
        spec.staging
    )))
    .fetch_one(&mut **tx)
    .await?;
    if source_count != staging_count {
        return Err(sqlx::Error::Protocol(format!(
            "rebuild migration {migration} copy verification failed: {} has {source_count} rows, \
             {} has {staging_count}; source table was not removed",
            spec.source, spec.staging
        )));
    }
    let difference_sql = format!(
        "SELECT COUNT(*) FROM ({} EXCEPT {} UNION ALL {} EXCEPT {})",
        spec.source_projection,
        spec.staging_projection,
        spec.staging_projection,
        spec.source_projection
    );
    let differences: i64 = sqlx::query_scalar(AssertSqlSafe(difference_sql))
        .fetch_one(&mut **tx)
        .await?;
    if differences != 0 {
        return Err(sqlx::Error::Protocol(format!(
            "rebuild migration {migration} copy verification found {differences} differing row(s); \
             source table was not removed"
        )));
    }
    Ok(())
}

async fn assert_foreign_key_check(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    migration: &str,
) -> Result<(), sqlx::Error> {
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut **tx)
        .await?;
    if violations.is_empty() {
        Ok(())
    } else {
        Err(sqlx::Error::Protocol(format!(
            "rebuild migration {migration} failed foreign_key_check with {} violation(s)",
            violations.len()
        )))
    }
}

async fn record_migration(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    migration: Migration,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO _migrations (name, checksum) VALUES (?, ?)")
        .bind(migration.name)
        .bind(migration_checksum(migration))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

// Each migration is an array of individual SQL statements.

const MIGRATION_001: [&str; 1] = ["CREATE TABLE IF NOT EXISTS workspaces (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        description TEXT,
        default_vocabulary TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )"];

const MIGRATION_002: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS projects (
        id TEXT PRIMARY KEY NOT NULL,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        description TEXT,
        project_type TEXT NOT NULL DEFAULT 'software',
        vocabulary TEXT NOT NULL DEFAULT '{}',
        workflow TEXT NOT NULL DEFAULT '{}',
        archived INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_projects_workspace ON projects(workspace_id)",
    "CREATE INDEX IF NOT EXISTS idx_projects_archived ON projects(archived)",
];

const MIGRATION_003_SPRINTS: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS sprints (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        goal TEXT,
        start_date TEXT,
        end_date TEXT,
        status TEXT NOT NULL DEFAULT 'planning',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_sprints_project ON sprints(project_id)",
    "CREATE INDEX IF NOT EXISTS idx_sprints_status ON sprints(project_id, status)",
];

const MIGRATION_004_ITEMS: [&str; 7] = [
    "CREATE TABLE IF NOT EXISTS items (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        parent_id TEXT REFERENCES items(id) ON DELETE SET NULL,
        title TEXT NOT NULL,
        description TEXT,
        item_type TEXT NOT NULL DEFAULT 'task',
        status TEXT NOT NULL,
        priority TEXT NOT NULL DEFAULT 'medium',
        estimate REAL,
        estimate_unit TEXT NOT NULL DEFAULT 'story_points',
        tags TEXT NOT NULL DEFAULT '[]',
        sort_order INTEGER NOT NULL DEFAULT 0,
        sprint_id TEXT REFERENCES sprints(id) ON DELETE SET NULL,
        due_date TEXT,
        started_at TEXT,
        completed_at TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_items_project ON items(project_id)",
    "CREATE INDEX IF NOT EXISTS idx_items_parent ON items(parent_id)",
    "CREATE INDEX IF NOT EXISTS idx_items_status ON items(project_id, status)",
    "CREATE INDEX IF NOT EXISTS idx_items_type ON items(project_id, item_type)",
    "CREATE INDEX IF NOT EXISTS idx_items_priority ON items(project_id, priority)",
    "CREATE INDEX IF NOT EXISTS idx_items_sort ON items(project_id, sort_order)",
];

const MIGRATION_005: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS dependencies (
        id TEXT PRIMARY KEY NOT NULL,
        source_item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        target_item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        dependency_type TEXT NOT NULL DEFAULT 'blocks',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(source_item_id, target_item_id, dependency_type)
    )",
    "CREATE INDEX IF NOT EXISTS idx_deps_source ON dependencies(source_item_id)",
    "CREATE INDEX IF NOT EXISTS idx_deps_target ON dependencies(target_item_id)",
];

const MIGRATION_006: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS roles (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        color TEXT NOT NULL DEFAULT '#6366f1',
        icon TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_roles_project ON roles(project_id)",
    "CREATE TABLE IF NOT EXISTS item_roles (
        item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
        PRIMARY KEY (item_id, role_id)
    )",
];

const MIGRATION_007: [&str; 2] = [
    "CREATE TABLE IF NOT EXISTS comments (
        id TEXT PRIMARY KEY NOT NULL,
        item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        author TEXT,
        content TEXT NOT NULL,
        comment_type TEXT NOT NULL DEFAULT 'comment',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_comments_item ON comments(item_id)",
];

const MIGRATION_008: [&str; 2] = [
    "CREATE TABLE IF NOT EXISTS attachments (
        id TEXT PRIMARY KEY NOT NULL,
        item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        filename TEXT NOT NULL,
        mime_type TEXT NOT NULL,
        storage_path TEXT NOT NULL,
        size_bytes INTEGER NOT NULL DEFAULT 0,
        uploaded_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_attachments_item ON attachments(item_id)",
];

const MIGRATION_009: [&str; 2] = [
    "CREATE TABLE IF NOT EXISTS board_views (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        columns TEXT NOT NULL DEFAULT '[]',
        filters TEXT,
        grouping TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_board_views_project ON board_views(project_id)",
];

const MIGRATION_010: [&str; 4] = [
    "CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
        title,
        description,
        tags,
        content='items',
        content_rowid='rowid'
    )",
    "CREATE TRIGGER IF NOT EXISTS items_fts_insert AFTER INSERT ON items BEGIN
        INSERT INTO items_fts(rowid, title, description, tags)
        VALUES (new.rowid, new.title, COALESCE(new.description, ''), new.tags);
    END",
    "CREATE TRIGGER IF NOT EXISTS items_fts_delete AFTER DELETE ON items BEGIN
        INSERT INTO items_fts(items_fts, rowid, title, description, tags)
        VALUES ('delete', old.rowid, old.title, COALESCE(old.description, ''), old.tags);
    END",
    "CREATE TRIGGER IF NOT EXISTS items_fts_update AFTER UPDATE ON items BEGIN
        INSERT INTO items_fts(items_fts, rowid, title, description, tags)
        VALUES ('delete', old.rowid, old.title, COALESCE(old.description, ''), old.tags);
        INSERT INTO items_fts(rowid, title, description, tags)
        VALUES (new.rowid, new.title, COALESCE(new.description, ''), new.tags);
    END",
];

const MIGRATION_011: [&str; 2] = [
    "CREATE TABLE IF NOT EXISTS project_templates (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        description TEXT,
        project_type TEXT NOT NULL DEFAULT 'software',
        vocabulary TEXT NOT NULL DEFAULT '{}',
        workflow TEXT NOT NULL DEFAULT '{}',
        custom_fields TEXT NOT NULL DEFAULT '[]',
        default_boards TEXT NOT NULL DEFAULT '[]',
        is_builtin INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_templates_type ON project_templates(project_type)",
];

const MIGRATION_012: [&str; 4] = [
    "CREATE TABLE IF NOT EXISTS custom_field_definitions (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        field_type TEXT NOT NULL,
        description TEXT,
        required INTEGER NOT NULL DEFAULT 0,
        default_value TEXT,
        options TEXT,
        validation TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_custom_fields_project ON custom_field_definitions(project_id)",
    "CREATE TABLE IF NOT EXISTS custom_field_values (
        id TEXT PRIMARY KEY NOT NULL,
        item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        field_id TEXT NOT NULL REFERENCES custom_field_definitions(id) ON DELETE CASCADE,
        value TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(item_id, field_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_custom_values_item ON custom_field_values(item_id)",
];

const MIGRATION_013: [&str; 3] = [
    "CREATE TABLE IF NOT EXISTS boards (
        id TEXT PRIMARY KEY NOT NULL,
        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        description TEXT,
        filters TEXT,
        grouping TEXT,
        is_default INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_boards_project ON boards(project_id)",
    "CREATE INDEX IF NOT EXISTS idx_boards_default ON boards(project_id, is_default)",
];

// Backfill a Default Board for every project that has no boards yet, then retire board_views.
// The UUID-like id is generated from randomblob(16) — unique and compatible with how the app
// stores UUIDs as TEXT.
const MIGRATION_014: [&str; 2] = [
    "INSERT INTO boards (id, project_id, name, is_default, created_at, updated_at)
     SELECT
       lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' || substr(lower(hex(randomblob(2))),2) || '-' || lower(hex(randomblob(2))) || '-' || lower(hex(randomblob(6))),
       id,
       'Default Board',
       1,
       datetime('now'),
       datetime('now')
     FROM projects
     WHERE id NOT IN (SELECT DISTINCT project_id FROM boards)",
    "DROP TABLE IF EXISTS board_views",
];

const MIGRATION_015: [&str; 2] = [
    "ALTER TABLE items ADD COLUMN assignee TEXT",
    "CREATE INDEX IF NOT EXISTS idx_items_assignee ON items(project_id, assignee)",
];

const MIGRATION_016: [&str; 1] = [
    // Sprint-grouping board view filters items by (project_id, sprint_id); no composite index existed.
    "CREATE INDEX IF NOT EXISTS idx_items_sprint ON items(project_id, sprint_id)",
];

const MIGRATION_017: [&str; 1] = [
    // Generic key/value store for app-level settings (e.g. install_id, cloud-backup
    // config edited from the UI). Created IF NOT EXISTS because install_id may have
    // lazily created it on an older install.
    "CREATE TABLE IF NOT EXISTS app_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL)",
];

const MIGRATION_018: [&str; 1] = [
    // Links a Tack item to a GitHub issue for push-only status sync.
    // One issue per item; removed automatically when the item is deleted.
    "CREATE TABLE IF NOT EXISTS github_links (
        item_id TEXT PRIMARY KEY NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        repo TEXT NOT NULL,
        issue_number INTEGER NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
];

// ─── Agent-Factory Control Center ──────────────────────────
//
// Schema for the tack-orch integration: Tack holds desired state, an external
// "control plane" (docket) executes agent work, and a reconciler mirrors progress
// back through these tables. See docs/book/src/roadmap.md's Agent-Factory Control
// Center section for the full design; that section's schema table is authoritative
// for these six migrations.
//
// Every FK to items/projects/control_planes is ON DELETE CASCADE, matching the
// github_links precedent: deleting the Tack-side row must not leave orphaned
// orchestration rows behind. Timestamps are TEXT RFC3339 (via datetime('now') at
// insert time, same as the rest of this file); all IDs are TEXT UUIDs.

const MIGRATION_019: [&str; 1] = [
    // One row per registered control plane (currently always "docket", but the
    // ControlPlane trait is written to allow other kinds later). `token` is the
    // docket Bearer credential: nullable, and write-only over the API (never
    // returned to clients — same discipline as the S3 backup secret key).
    // `health` is the reconciler's state machine: healthy | degraded | unreachable.
    "CREATE TABLE IF NOT EXISTS control_planes (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL,
        kind TEXT NOT NULL DEFAULT 'docket',
        base_url TEXT NOT NULL,
        token TEXT,
        api_version TEXT,
        health TEXT NOT NULL DEFAULT 'unknown',
        last_seen_at TEXT,
        consecutive_failures INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
];

const MIGRATION_020: [&str; 2] = [
    // Links a Tack project to a pod on a control plane — one link per project
    // (mirrors the one-issue-per-item shape of github_links). `status_map` is a JSON
    // object (validated against the project's WorkflowConfig at save time, in the API
    // layer, not here) mapping remote run states to Tack statuses. `budget_usd` is a
    // user-set cap, not a computed spend figure, so it does not need the
    // "_estimated" suffix reserved for derived cost numbers.
    "CREATE TABLE IF NOT EXISTS orch_links (
        project_id TEXT PRIMARY KEY NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        remote_project TEXT NOT NULL,
        pipeline_file TEXT,
        blueprint TEXT,
        auto_dispatch INTEGER NOT NULL DEFAULT 0,
        budget_usd REAL,
        status_map TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_links_control_plane ON orch_links(control_plane_id)",
];

const MIGRATION_021: [&str; 2] = [
    // item ↔ docket task. PK is (item_id, remote_task_id) rather than a single-column
    // key: an item can be redispatched (retries, reruns), and each dispatch gets its
    // own task row rather than overwriting the last one. `remote_run_id` correlates to
    // orch_runs.run_id but is deliberately not a hard FK — orch_runs is populated by a
    // separate poll step and a task may briefly (or, for data predating write-side
    // dispatch, permanently) have no known run. Token counts are the primary measure;
    // cost_usd_estimated is derived and named accordingly per the "never present an
    // estimate as spend" rule.
    "CREATE TABLE IF NOT EXISTS orch_tasks (
        item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        remote_task_id TEXT NOT NULL,
        remote_run_id TEXT,
        remote_status TEXT NOT NULL DEFAULT 'pending',
        attempt INTEGER NOT NULL DEFAULT 1,
        tokens_in INTEGER NOT NULL DEFAULT 0,
        tokens_out INTEGER NOT NULL DEFAULT 0,
        cost_usd_estimated REAL,
        dispatched_at TEXT NOT NULL DEFAULT (datetime('now')),
        trusted INTEGER NOT NULL DEFAULT 1,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (item_id, remote_task_id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_tasks_remote_run ON orch_tasks(remote_run_id)",
];

const MIGRATION_022: [&str; 2] = [
    // Mirror of docket's GET /runs. `item_id` is nullable: a run dispatched from the
    // docket CLI (not through Tack) mirrors "unattributed" until a matching orch_tasks
    // row correlates it — a case this schema must not error on.
    "CREATE TABLE IF NOT EXISTS orch_runs (
        run_id TEXT PRIMARY KEY NOT NULL,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        item_id TEXT REFERENCES items(id) ON DELETE CASCADE,
        remote_project TEXT NOT NULL,
        source TEXT NOT NULL DEFAULT 'cli',
        state TEXT NOT NULL DEFAULT 'queued',
        started_at TEXT,
        ended_at TEXT,
        error TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_runs_plane_state ON orch_runs(control_plane_id, state)",
];

const MIGRATION_023: [&str; 3] = [
    // Append-only telemetry: hops, tool calls, verdicts, rework, status_map_rejected,
    // etc. `event_type` stores docket's event type verbatim (including types Tack does
    // not yet recognise) so a docket upgrade degrades to "shown as-is" rather than a
    // dropped or errored row. Two indexes: one for an item's timeline, one for the
    // retention sweep (delete-by-age scans occurred_at across all items).
    "CREATE TABLE IF NOT EXISTS orch_events (
        id TEXT PRIMARY KEY NOT NULL,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        item_id TEXT REFERENCES items(id) ON DELETE CASCADE,
        run_id TEXT,
        event_type TEXT NOT NULL,
        payload TEXT NOT NULL DEFAULT '{}',
        occurred_at TEXT NOT NULL DEFAULT (datetime('now')),
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_events_item_occurred ON orch_events(item_id, occurred_at)",
    "CREATE INDEX IF NOT EXISTS idx_orch_events_occurred ON orch_events(occurred_at)",
];

const MIGRATION_024: [&str; 3] = [
    // Mirror of docket's GET /approvals. `token` (docket's approval token, not a
    // credential) is the natural PK. Correlated to an item via
    // record.context.taskId → orch_tasks.remote_task_id at ingest time in the
    // repository layer; `remote_task_id` is stored for that correlation but, like
    // orch_tasks.remote_run_id, is not a hard FK. Uncorrelated records keep
    // item_id = NULL and must still surface in the fleet-wide approvals inbox.
    "CREATE TABLE IF NOT EXISTS orch_approvals (
        token TEXT PRIMARY KEY NOT NULL,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        item_id TEXT REFERENCES items(id) ON DELETE CASCADE,
        remote_task_id TEXT,
        agent TEXT,
        action TEXT,
        state TEXT NOT NULL DEFAULT 'pending',
        requested_at TEXT NOT NULL DEFAULT (datetime('now')),
        decided_at TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_approvals_item ON orch_approvals(item_id)",
    "CREATE INDEX IF NOT EXISTS idx_orch_approvals_state ON orch_approvals(state)",
];

// ─── Metrics ingestion + retention ─────
//
// `orch_metrics` gets its own migration here, 025, plus two rollup tables (026, 027)
// for the retention sweep.

const MIGRATION_025: [&str; 4] = [
    // One row per scrape per metric per label set. Append-only time series: unlike
    // orch_tasks/orch_runs/orch_events/orch_approvals, a sample has no natural key
    // across scrapes — the same name+labels recorded at two different scrape times are
    // two distinct data points, not a correction of one another — so this table has no
    // ON CONFLICT upsert path (see repo/orch.rs's upsert_orch_metrics doc comment).
    // `labels` is a canonical (BTreeMap-key-sorted) JSON object so the same logical
    // label set always serializes identically; the retention rollup (026/027) and the
    // "latest sample per metric" query (GET /api/metrics) both depend on that.
    // `value` is nullable, not `REAL NOT NULL`: SQLite has no native NaN representation,
    // and sqlx's SQLite driver silently binds an `f64::NAN` value as SQL NULL (verified
    // directly — an `f64::NAN` bind against a `NOT NULL` column fails with a constraint
    // violation, while `+Inf`/`-Inf` bind and round-trip fine as ordinary REAL values).
    // The Prometheus parser (`adapters::prometheus::parse_value`) deliberately preserves
    // `NaN`/`Inf` rather than dropping them, so this column must accept whatever it hands
    // back rather than erroring the whole scrape over one exotic sample.
    "CREATE TABLE IF NOT EXISTS orch_metrics (
        id TEXT PRIMARY KEY NOT NULL,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        name TEXT NOT NULL,
        labels TEXT NOT NULL DEFAULT '{}',
        value REAL,
        scraped_at TEXT NOT NULL DEFAULT (datetime('now')),
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    // Retention sweep scan: oldest-first, unscoped by plane.
    "CREATE INDEX IF NOT EXISTS idx_orch_metrics_scraped ON orch_metrics(scraped_at)",
    // Per-plane timeline / bounded-retention-batch queries.
    "CREATE INDEX IF NOT EXISTS idx_orch_metrics_plane_scraped ON orch_metrics(control_plane_id, scraped_at)",
    // "Latest sample per (plane, name, labels)" — GET /api/metrics's merge query.
    "CREATE INDEX IF NOT EXISTS idx_orch_metrics_plane_name_labels ON orch_metrics(control_plane_id, name, labels)",
];

const MIGRATION_026: [&str; 2] = [
    // Per-day rollup of orch_events, written by the retention sweep before the
    // corresponding raw rows are deleted. Grouped at (day, control_plane_id,
    // event_type) — deliberately NOT per-item: SQLite treats NULL as distinct from
    // every other NULL in a UNIQUE constraint, so a nullable item_id in the uniqueness
    // key would silently allow duplicate aggregate rows for uncorrelated events.
    // Per-item event history is expected to age out along with the raw rows once
    // retention passes; the day/plane/type total (what the unit-economics queries
    // need) is what survives.
    "CREATE TABLE IF NOT EXISTS orch_events_daily (
        id TEXT PRIMARY KEY NOT NULL,
        day TEXT NOT NULL,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        event_type TEXT NOT NULL,
        event_count INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(day, control_plane_id, event_type)
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_events_daily_plane_day ON orch_events_daily(control_plane_id, day)",
];

const MIGRATION_027: [&str; 2] = [
    // Per-day rollup of orch_metrics: count/sum/min/max per (day, control_plane_id,
    // metric_name, labels), written before the corresponding raw scrape rows are
    // deleted. `labels` is non-nullable (default '{}'), so — unlike the item_id
    // concern on orch_events_daily above — including it in the UNIQUE key is safe:
    // SQLite's NULL-distinctness quirk only bites nullable columns.
    "CREATE TABLE IF NOT EXISTS orch_metrics_daily (
        id TEXT PRIMARY KEY NOT NULL,
        day TEXT NOT NULL,
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        metric_name TEXT NOT NULL,
        labels TEXT NOT NULL DEFAULT '{}',
        sample_count INTEGER NOT NULL DEFAULT 0,
        value_sum REAL NOT NULL DEFAULT 0,
        value_min REAL,
        value_max REAL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE(day, control_plane_id, metric_name, labels)
    )",
    "CREATE INDEX IF NOT EXISTS idx_orch_metrics_daily_plane_day ON orch_metrics_daily(control_plane_id, day)",
];

// ─── Trace ingestion ──────────────────────────────
//
// docket's `/traces/{project}?since=` cursor (`serve.py`'s `_traces_page`) is a
// compound `"<ts>Z:<n>"` token, not a bare timestamp or offset — see
// `crates/tack-orch/src/reconciler.rs`'s module doc for the full mechanics. It must
// resume correctly per *docket* project, independent of which (if any) Tack project
// is currently linked to it, so this is its own table keyed on
// `(control_plane_id, remote_project)` rather than a column bolted onto `orch_links`
// (whose PK is `project_id` — Tack's side of the link, which can be unlinked/relinked
// without docket's own trace history caring).

const MIGRATION_028: [&str; 1] = ["CREATE TABLE IF NOT EXISTS orch_trace_cursors (
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        remote_project TEXT NOT NULL,
        cursor TEXT NOT NULL,
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (control_plane_id, remote_project)
    )"];

// ─── Item provenance / trust boundary ────────────────────
//
// Tack imports items from GitHub Issues and Linear — text written by anyone who
// can file an issue on a linked repo. Once the dispatcher sends
// an item's title/description to docket as agent input, that text becomes
// instructions to an autonomous agent. `source` is a sticky, creation-time-only
// marker of where an item's text came from, read by the dispatcher
// (`tack-api::dispatcher::resolve_default_trust`, and the auto-dispatch hook in
// `handlers::items`) to decide whether to pass `trusted: true` or `trusted: false`
// to docket's `pre_input` policy gate. See `tack_core::models::ItemSource` for the
// full enum and the trust rule (`is_trusted()`).
//
// Default is deliberately `'unknown'`, not `'manual'`: every *existing* row in
// every existing install — including items imported via GitHub before this trust
// mechanism existed (migration 018 predates it) — backfills to `'unknown'`,
// which `ItemSource::is_trusted()` treats as untrusted. `'manual'` is a value only
// ever written explicitly, by the ordinary create-item code path, never assumed.
// This is the "unsafe state is never the accidental default" rule, applied
// literally: a NULL/legacy/unrecognised source resolves to "do not trust this
// text with operator privileges," not the reverse.
const MIGRATION_029: [&str; 1] =
    ["ALTER TABLE items ADD COLUMN source TEXT NOT NULL DEFAULT 'unknown'"];

// Holds a serialized `TemplateOrchestration` (tack-core). A
// nullable column, not `NOT NULL DEFAULT '{}'` — `NULL` means "this template
// has no orchestration block," distinct from `'{}'` ("an orchestration block
// with every field at its default"). Every row that predates this migration
// backfills to `NULL`, so `repo::templates::get_template`/`list_templates`
// deserialize it to `orchestration: None`, matching `ProjectTemplate`'s own
// `#[serde(default)]` — the same "absent means nothing, existing templates
// are untouched" rule migration 029 established for `items.source`, applied
// here to a column instead of a JSON payload key.
const MIGRATION_030: [&str; 1] = ["ALTER TABLE project_templates ADD COLUMN orchestration TEXT"];

// Unit economics: `GET /api/economics/summary` and
// `GET /api/economics/items` both start from "every item where completed_at IS NOT
// NULL" — a query the existing indexes don't cover: `idx_items_status` and friends are
// all `(project_id, ...)` composites, but this scan is deliberately NOT scoped to one
// project (it slices by `project_type`/`item_type` across the whole instance), so
// there is no leading `project_id` predicate for those indexes to serve. Partial
// (`WHERE completed_at IS NOT NULL`) because most items in a healthy board are not yet
// done, so the index only needs to cover the minority that are.
const MIGRATION_031: [&str; 1] = [
    "CREATE INDEX IF NOT EXISTS idx_items_completed_at ON items(completed_at) \
     WHERE completed_at IS NOT NULL",
];

// ─── Agnostic Control Plane additive columns ─
//
// Five single-statement ALTERs, one per migration name: `apply_migrations` above
// runs every statement in a migration with no wrapping transaction and only
// records the name once every statement in it has succeeded. A migration carrying
// more than one ALTER that fails partway records nothing, so the next boot replays
// the statements that already applied, hits SQLite's "duplicate column name", and
// the server never boots again — there is no down-migration to fall back to. 029
// and 030 already established the one-ALTER precedent; these five follow it
// without exception.
//
// The two table rebuilds this same design needs (037 `orch_runs`, 038
// `orch_approvals`) are deliberately NOT in this batch. A `CREATE …_new` / copy /
// `DROP` / `RENAME` sequence is exactly the kind of multi-statement, non-atomic
// operation the one-ALTER rule warns about, just with higher stakes (existing
// rows, not just an empty column) — it ships as its own migration-runner release,
// with its own half-applied-boot guard, so a partial rebuild is never silently
// retried.

// `control_planes.config` is never a secret, so it is intentionally absent from
// `remote_backup.rs::SENSITIVE_META_KEYS`/`scrub_snapshot_secrets`. A GitHub
// Actions plane needs `{owner, repo, workflow_file, ref, api_base}`; today
// `control_planes` (migration 019) has only `base_url` and one `token`, with
// nowhere to put provider-shaped configuration that isn't a credential. `config`
// is opaque JSON at this layer — the registry (`tack-orch::adapters::registry`)
// is what gives it a per-`kind` shape; this migration only makes room.
// `NOT NULL DEFAULT '{}'` so every row that predates this column — including
// every docket plane already registered — reads back as "no extra config," which
// is exactly what a docket plane has today.
const MIGRATION_032: [&str; 1] =
    ["ALTER TABLE control_planes ADD COLUMN config TEXT NOT NULL DEFAULT '{}'"];

// Write-only credentials JSON, alongside the existing single `token` column
// (migration 019). One column, not two: a GitHub Actions plane needs *two*
// secrets — a PAT/App credential to call the Actions API, and a webhook signing
// secret for `POST /api/webhooks/github/{id}` — and a provider-shaped
// JSON blob is cheaper to extend than a new ALTER every time a future provider
// needs a third. Nullable, not `DEFAULT '{}'`: `NULL` means "no secrets stored,"
// distinct from `'{}'` ("a secrets block whose provider-specific keys are all
// absent") — same discipline migration 030 used for `orchestration TEXT`.
//
// `remote_backup.rs::scrub_snapshot_secrets` already nulls this column before
// every downloadable/uploadable backup snapshot, alongside `control_planes.token`
// (that function's own doc comment states the rule: every secret-bearing column
// gets its own null-before-VACUUM block there, run before the trailing `VACUUM`).
// The column is still unused in practice: `tack_db::repo::orch::ControlPlane`
// (the read struct the real `registry::build` callers loop over) does not yet
// surface `config`/`secrets`, so nothing writes a real value into it — but
// backup scrubbing already covers it whenever a caller does.
const MIGRATION_033: [&str; 1] = ["ALTER TABLE control_planes ADD COLUMN secrets TEXT"];

// ─── Optimistic concurrency columns ──────────────────────────────────────
//
// `version INTEGER NOT NULL DEFAULT 1` on the three tables an HTTP client can
// read an ETag from and later PATCH: items, orch_links, control_planes.
// Bumping it on write and wiring `ETag`/`If-Match` lives in
// `tack-api::handlers::items`/`handlers::orch`. `DEFAULT 1` (not
// 0): a row that predates this column has been written exactly once (its
// `INSERT`), so its version number under the new scheme is 1, matching what a
// freshly created row gets going forward — an `If-Match: 1` sent by a client
// that fetched the row *before* this migration ran is still correct against the
// row *after* the migration ran.

// items is the highest-traffic write target in the schema and the one most
// exposed to a lost-update race (concurrent HTTP PATCH vs. an MCP write vs. the
// reconciler's own `apply_mapped_status` call).
const MIGRATION_034: [&str; 1] =
    ["ALTER TABLE items ADD COLUMN version INTEGER NOT NULL DEFAULT 1"];

// orch_links (migration 020): one row per project-to-plane link, edited from the
// Settings UI. Same lost-update risk as items — two browser tabs editing the same
// project's link — just lower traffic.
const MIGRATION_035: [&str; 1] =
    ["ALTER TABLE orch_links ADD COLUMN version INTEGER NOT NULL DEFAULT 1"];

// control_planes (migration 019): edited from the same Settings UI, and now also
// the row `secrets` (033) lands on — a stale-read-then-write race here is a
// credential clobber, not just a config clobber.
const MIGRATION_036: [&str; 1] =
    ["ALTER TABLE control_planes ADD COLUMN version INTEGER NOT NULL DEFAULT 1"];

// ─── Table rebuilds — THE IRREVERSIBLE MIGRATIONS ────
//
// 037 and 038 are the one place in this file that breaks the "every migration
// is a single-statement ALTER" rule on purpose, because the
// change each needs cannot be expressed as an ALTER at all:
//
// `orch_runs.run_id` (migration 022) is a *global* primary key with
// `control_plane_id` sitting outside that key. A future adapter (docket today,
// a second provider later) needs to mint a Tack-side correlation id *before*
// it knows the provider's own run id — the whole point of the nonce-exchange
// handshake this schema exists to support — and later attach the provider id
// once the run reports in. Doing that against a single-column PK means
// inserting a placeholder row keyed on the correlation id and then
// "backfilling" the provider's real id once it's known — which is a *second*
// row under a different primary key, because `ON CONFLICT(run_id)`
// (`repo/orch.rs::upsert_orch_runs`) has no way to notice the two rows are the
// same run. Every run dispatched this way would double from that point on.
// Rebuilding the primary key around `(control_plane_id, external_run_id,
// run_attempt)` with a separate, nullable `correlation_id` column sidesteps
// this entirely: the correlation id and the provider id are different columns
// from the start, never competing for the same uniqueness slot.
//
// `orch_approvals.control_plane_id` (migration 024) is `NOT NULL REFERENCES
// control_planes(id)`. A decision raised by a `PreToolUse` hook inside a run
// that was never dispatched through a *registered* plane has no control-plane
// row to reference at the moment it's raised. `NOT NULL` makes that insert
// fail outright; only a column-level rebuild can widen it to nullable.
//
// Both rebuilds use SQLite's documented copy/swap procedure inside one
// transaction. Foreign keys remain enabled: neither table is a parent of a
// foreign key, so the old OFF/ON toggle was unnecessary and made a crash
// recoverable only by an operator. The runner creates the `_new` table, copies
// with explicit columns (never `SELECT *`), proves count and field equality
// before DROP, swaps, recreates indexes, fetches `PRAGMA foreign_key_check`
// and commits the migration record last. A statement failure rolls the whole
// transaction back, leaving a retryable original table and no staging residue.
//
// **This is the only step in the whole cycle that rewrites existing rows.**
// Before a file-backed database enters its first pending rebuild, `run_all`
// creates a consistent `VACUUM INTO` snapshot beside it. That snapshot is an
// additional recovery artifact, not a substitute for the atomic copy/swap.

// Every existing column is carried across unchanged; `run_id` is copied into
// the new `external_run_id` (positionally, not by name, in the INSERT below)
// and every pre-existing row gets `run_attempt = 1` (it has only ever had one
// attempt under the old schema, which had no attempt concept at all) and
// `correlation_id = NULL` (it predates Tack minting one). `run_attempt`
// defaults to 1 for the same reason `items.version` (034) defaults to 1, not
// 0: a row that predates the column has implicitly already had exactly one
// "version" of whatever the column now counts.
const MIGRATION_037_STATEMENTS: [&str; 6] = [
    // No released version of Tack has ever depended on partial state from an
    // earlier draft of this rebuild, so it's always safe to unconditionally
    // drop any pre-existing staging table before rebuilding — and this is
    // inside the surrounding transaction, so an interrupted retry cannot
    // leave an orphan or lose rows.
    "DROP TABLE IF EXISTS orch_runs_new",
    "CREATE TABLE IF NOT EXISTS orch_runs_new (
        control_plane_id TEXT NOT NULL REFERENCES control_planes(id) ON DELETE CASCADE,
        external_run_id TEXT NOT NULL,
        run_attempt INTEGER NOT NULL DEFAULT 1,
        correlation_id TEXT UNIQUE,
        item_id TEXT REFERENCES items(id) ON DELETE CASCADE,
        remote_project TEXT NOT NULL,
        source TEXT NOT NULL DEFAULT 'cli',
        state TEXT NOT NULL DEFAULT 'queued',
        started_at TEXT,
        ended_at TEXT,
        error TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (control_plane_id, external_run_id, run_attempt)
    )",
    "INSERT INTO orch_runs_new
        (control_plane_id, external_run_id, run_attempt, correlation_id, item_id,
         remote_project, source, state, started_at, ended_at, error, created_at, updated_at)
     SELECT control_plane_id, run_id, 1, NULL, item_id, remote_project, source, state,
            started_at, ended_at, error, created_at, updated_at
     FROM orch_runs",
    "DROP TABLE orch_runs",
    "ALTER TABLE orch_runs_new RENAME TO orch_runs",
    "CREATE INDEX IF NOT EXISTS idx_orch_runs_plane_state ON orch_runs(control_plane_id, state)",
];

const MIGRATION_037: RebuildMigration = RebuildMigration {
    source: "orch_runs",
    staging: "orch_runs_new",
    statements: &MIGRATION_037_STATEMENTS,
    copy_step: 2,
    source_projection: "SELECT control_plane_id, run_id AS external_run_id, 1 AS run_attempt, \
        NULL AS correlation_id, item_id, remote_project, source, state, started_at, ended_at, \
        error, created_at, updated_at FROM orch_runs",
    staging_projection: "SELECT control_plane_id, external_run_id, run_attempt, correlation_id, \
        item_id, remote_project, source, state, started_at, ended_at, error, created_at, \
        updated_at FROM orch_runs_new",
};

// `control_plane_id` drops its `NOT NULL` (a hook-raised decision may have no
// registered plane behind it yet — see the section doc comment above).
// `token` is untouched as the primary key and stays the URL path segment of
// `POST /api/approvals/{token}`: renaming a column that already lives in a
// user's database, and in a URL clients already call, buys nothing — the
// value doesn't change, only what can now point at NULL. `kind` defaults to
// `'approval'` so every pre-existing row — every one of them was, definitionally,
// docket's approval-of-an-irreversible-action shape, the only kind this
// schema could represent before today — keeps reading as exactly that.
// `external_id`/`provider_metadata` are additive, empty on every existing row.
const MIGRATION_038_STATEMENTS: [&str; 7] = [
    "DROP TABLE IF EXISTS orch_approvals_new",
    "CREATE TABLE IF NOT EXISTS orch_approvals_new (
        token TEXT PRIMARY KEY NOT NULL,
        control_plane_id TEXT REFERENCES control_planes(id) ON DELETE CASCADE,
        kind TEXT NOT NULL DEFAULT 'approval',
        external_id TEXT,
        provider_metadata TEXT NOT NULL DEFAULT '{}',
        item_id TEXT REFERENCES items(id) ON DELETE CASCADE,
        remote_task_id TEXT,
        agent TEXT,
        action TEXT,
        state TEXT NOT NULL DEFAULT 'pending',
        requested_at TEXT NOT NULL DEFAULT (datetime('now')),
        decided_at TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    )",
    "INSERT INTO orch_approvals_new
        (token, control_plane_id, kind, external_id, provider_metadata, item_id,
         remote_task_id, agent, action, state, requested_at, decided_at, created_at, updated_at)
     SELECT token, control_plane_id, 'approval', NULL, '{}', item_id, remote_task_id, agent,
            action, state, requested_at, decided_at, created_at, updated_at
     FROM orch_approvals",
    "DROP TABLE orch_approvals",
    "ALTER TABLE orch_approvals_new RENAME TO orch_approvals",
    "CREATE INDEX IF NOT EXISTS idx_orch_approvals_item ON orch_approvals(item_id)",
    "CREATE INDEX IF NOT EXISTS idx_orch_approvals_state ON orch_approvals(state)",
];

const MIGRATION_038: RebuildMigration = RebuildMigration {
    source: "orch_approvals",
    staging: "orch_approvals_new",
    statements: &MIGRATION_038_STATEMENTS,
    copy_step: 2,
    source_projection: "SELECT token, control_plane_id, 'approval' AS kind, NULL AS external_id, \
        '{}' AS provider_metadata, item_id, remote_task_id, agent, action, state, requested_at, \
        decided_at, created_at, updated_at FROM orch_approvals",
    staging_projection: "SELECT token, control_plane_id, kind, external_id, provider_metadata, \
        item_id, remote_task_id, agent, action, state, requested_at, decided_at, created_at, \
        updated_at FROM orch_approvals_new",
};

// ─── Harness-agnostic runner fleet ─────────────────────
//
// These tables are deliberately additive. The existing `orch_*` history is a
// legacy Docket bridge and is neither renamed nor reused for neutral execution
// state. Each named migration remains an ordinary transaction under the runner
// above; `_migrations` is written in the same transaction only at commit.

const MIGRATION_039: [&str; 2] = [
    "CREATE TABLE agent_fleets (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL UNIQUE,
        concurrency_limit INTEGER,
        default_policy TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )",
    "CREATE INDEX idx_agent_fleets_name ON agent_fleets(name)",
];

const MIGRATION_040: [&str; 2] = [
    "CREATE TABLE agent_runners (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL UNIQUE,
        credential_hash TEXT NOT NULL,
        state TEXT NOT NULL DEFAULT 'active',
        labels TEXT NOT NULL DEFAULT '{}',
        total_capacity INTEGER NOT NULL DEFAULT 1,
        available_capacity INTEGER NOT NULL DEFAULT 0,
        capability_snapshot TEXT NOT NULL DEFAULT '{}',
        protocol_version INTEGER NOT NULL,
        last_heartbeat_at TEXT,
        revoked_at TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )",
    "CREATE INDEX idx_agent_runners_state_capacity ON agent_runners(state, available_capacity)",
];

const MIGRATION_041: [&str; 2] = [
    "CREATE TABLE agent_fleet_members (
        fleet_id TEXT NOT NULL REFERENCES agent_fleets(id) ON DELETE CASCADE,
        runner_id TEXT NOT NULL REFERENCES agent_runners(id) ON DELETE CASCADE,
        created_at TEXT NOT NULL,
        PRIMARY KEY (fleet_id, runner_id)
    )",
    "CREATE INDEX idx_agent_fleet_members_runner ON agent_fleet_members(runner_id)",
];

const MIGRATION_042: [&str; 2] = [
    "CREATE TABLE agent_profiles (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL UNIQUE,
        instructions TEXT NOT NULL,
        tool_policy TEXT NOT NULL DEFAULT '{}',
        limits TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )",
    "CREATE INDEX idx_agent_profiles_name ON agent_profiles(name)",
];

const MIGRATION_043: [&str; 2] = [
    "CREATE TABLE model_profiles (
        id TEXT PRIMARY KEY NOT NULL,
        name TEXT NOT NULL UNIQUE,
        model_provider TEXT NOT NULL,
        model_id TEXT NOT NULL,
        config_reference TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )",
    "CREATE INDEX idx_model_profiles_provider_model ON model_profiles(model_provider, model_id)",
];

const MIGRATION_044: [&str; 3] = [
    "CREATE TABLE execution_requests (
        id TEXT PRIMARY KEY NOT NULL,
        item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
        idempotency_scope TEXT NOT NULL,
        idempotency_key TEXT NOT NULL,
        request_fingerprint TEXT NOT NULL,
        state TEXT NOT NULL DEFAULT 'queued',
        selector_kind TEXT NOT NULL,
        selector_id TEXT NOT NULL,
        agent_profile_id TEXT REFERENCES agent_profiles(id) ON DELETE SET NULL,
        agent_profile_snapshot TEXT NOT NULL,
        requested_harness_kind TEXT,
        requested_model_provider TEXT,
        requested_model_id TEXT,
        repository_snapshot TEXT NOT NULL,
        permission_policy TEXT NOT NULL,
        timeout_seconds INTEGER,
        budgets TEXT NOT NULL DEFAULT '{}',
        status_map_policy_id TEXT,
        environment TEXT NOT NULL DEFAULT '{}',
        metadata TEXT NOT NULL DEFAULT '{}',
        cancellation_requested_at TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        UNIQUE(idempotency_scope, idempotency_key)
    )",
    "CREATE INDEX idx_execution_requests_queue ON execution_requests(state, created_at)",
    "CREATE INDEX idx_execution_requests_item ON execution_requests(item_id, created_at)",
];

const MIGRATION_045: [&str; 3] = [
    "CREATE TABLE execution_attempts (
        id TEXT PRIMARY KEY NOT NULL,
        request_id TEXT NOT NULL REFERENCES execution_requests(id) ON DELETE CASCADE,
        attempt_number INTEGER NOT NULL,
        runner_id TEXT NOT NULL REFERENCES agent_runners(id) ON DELETE RESTRICT,
        fencing_token INTEGER NOT NULL,
        state TEXT NOT NULL DEFAULT 'leased',
        lease_issued_at TEXT NOT NULL,
        lease_expires_at TEXT NOT NULL,
        last_heartbeat_at TEXT,
        event_checkpoint TEXT,
        completion_id TEXT,
        workspace_id TEXT,
        base_revision TEXT,
        actual_execution TEXT,
        terminal_reason TEXT,
        usage TEXT,
        started_at TEXT,
        ended_at TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        UNIQUE(request_id, attempt_number),
        UNIQUE(request_id, fencing_token)
    )",
    "CREATE INDEX idx_execution_attempts_runner_lease ON execution_attempts(runner_id, lease_expires_at)",
    "CREATE INDEX idx_execution_attempts_request_state ON execution_attempts(request_id, state)",
];

const MIGRATION_046: [&str; 3] = [
    "CREATE TABLE execution_events (
        id TEXT PRIMARY KEY NOT NULL,
        attempt_id TEXT NOT NULL REFERENCES execution_attempts(id) ON DELETE CASCADE,
        event_id TEXT NOT NULL,
        sequence INTEGER NOT NULL,
        source TEXT NOT NULL,
        kind TEXT NOT NULL,
        payload TEXT NOT NULL,
        occurred_at TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(attempt_id, event_id),
        UNIQUE(attempt_id, sequence)
    )",
    "CREATE INDEX idx_execution_events_timeline ON execution_events(attempt_id, sequence)",
    "CREATE INDEX idx_execution_events_occurred ON execution_events(occurred_at)",
];

const MIGRATION_047: [&str; 3] = [
    "CREATE TABLE execution_artifacts (
        id TEXT PRIMARY KEY NOT NULL,
        attempt_id TEXT NOT NULL REFERENCES execution_attempts(id) ON DELETE CASCADE,
        artifact_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        name TEXT NOT NULL,
        media_type TEXT,
        size_bytes INTEGER NOT NULL,
        sha256 TEXT NOT NULL,
        content_disposition TEXT,
        content_reference TEXT,
        metadata TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL,
        UNIQUE(attempt_id, artifact_id)
    )",
    "CREATE INDEX idx_execution_artifacts_attempt ON execution_artifacts(attempt_id)",
    "CREATE INDEX idx_execution_artifacts_sha256 ON execution_artifacts(sha256)",
];

const MIGRATION_048: [&str; 3] = [
    "CREATE TABLE execution_decisions (
        id TEXT PRIMARY KEY NOT NULL,
        attempt_id TEXT NOT NULL REFERENCES execution_attempts(id) ON DELETE CASCADE,
        decision_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        state TEXT NOT NULL DEFAULT 'pending',
        prompt TEXT NOT NULL,
        options TEXT NOT NULL DEFAULT '[]',
        metadata TEXT NOT NULL DEFAULT '{}',
        answer TEXT,
        expires_at TEXT,
        resolved_at TEXT,
        resolved_by TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        UNIQUE(attempt_id, decision_id)
    )",
    "CREATE INDEX idx_execution_decisions_pending ON execution_decisions(state, expires_at)",
    "CREATE INDEX idx_execution_decisions_attempt ON execution_decisions(attempt_id)",
];

// Durable credential/token and protocol idempotency seams.
const MIGRATION_049: [&str; 5] = [
    "ALTER TABLE agent_runners ADD COLUMN runner_version TEXT",
    "ALTER TABLE agent_runners ADD COLUMN credential_expires_at TEXT",
    "ALTER TABLE agent_runners ADD COLUMN credential_rotated_at TEXT",
    "CREATE TABLE agent_enrollment_tokens (id TEXT PRIMARY KEY NOT NULL, runner_id TEXT REFERENCES agent_runners(id) ON DELETE CASCADE, token_hash TEXT NOT NULL UNIQUE, expires_at TEXT NOT NULL, consumed_at TEXT, revoked_at TEXT, created_at TEXT NOT NULL)",
    "CREATE INDEX idx_agent_enrollment_tokens_redeem ON agent_enrollment_tokens(token_hash, expires_at)",
];

const MIGRATION_050: [&str; 2] = [
    "CREATE TABLE execution_claim_replays (runner_id TEXT NOT NULL REFERENCES agent_runners(id) ON DELETE CASCADE, claim_request_id TEXT NOT NULL, attempt_id TEXT NOT NULL REFERENCES execution_attempts(id) ON DELETE CASCADE, created_at TEXT NOT NULL, PRIMARY KEY (runner_id, claim_request_id))",
    "CREATE INDEX idx_execution_claim_replays_attempt ON execution_claim_replays(attempt_id)",
];

const MIGRATION_051: [&str; 2] = [
    "CREATE TABLE execution_recovery_audits (attempt_id TEXT NOT NULL REFERENCES execution_attempts(id) ON DELETE CASCADE, recovery_key TEXT NOT NULL, classification TEXT NOT NULL, details TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL, PRIMARY KEY (attempt_id, recovery_key))",
    "CREATE INDEX idx_execution_recovery_audits_attempt ON execution_recovery_audits(attempt_id)",
];

const MIGRATION_052: [&str; 2] = [
    "CREATE TABLE execution_heartbeat_replays (runner_id TEXT NOT NULL REFERENCES agent_runners(id) ON DELETE CASCADE, heartbeat_id TEXT NOT NULL, response TEXT NOT NULL, created_at TEXT NOT NULL, PRIMARY KEY (runner_id, heartbeat_id))",
    "CREATE TABLE execution_cancellation_replays (attempt_id TEXT NOT NULL REFERENCES execution_attempts(id) ON DELETE CASCADE, cancellation_request_id TEXT NOT NULL, state TEXT NOT NULL, created_at TEXT NOT NULL, PRIMARY KEY (attempt_id, cancellation_request_id))",
];
const MIGRATION_053: [&str; 1] =
    ["ALTER TABLE execution_requests ADD COLUMN request_snapshot TEXT NOT NULL DEFAULT '{}'"];
const MIGRATION_054: [&str; 1] = [
    "CREATE TABLE execution_event_batch_replays (attempt_id TEXT NOT NULL REFERENCES execution_attempts(id) ON DELETE CASCADE, checkpoint TEXT NOT NULL, fingerprint TEXT NOT NULL, response TEXT NOT NULL, created_at TEXT NOT NULL, PRIMARY KEY (attempt_id, checkpoint))",
];
const MIGRATION_055: [&str; 1] = [
    "CREATE TABLE execution_completion_replays (attempt_id TEXT NOT NULL REFERENCES execution_attempts(id) ON DELETE CASCADE, completion_id TEXT NOT NULL, fingerprint TEXT NOT NULL, response TEXT NOT NULL, committed_at TEXT NOT NULL, PRIMARY KEY(attempt_id, completion_id))",
];
const MIGRATION_056: [&str; 1] =
    ["ALTER TABLE execution_heartbeat_replays ADD COLUMN fingerprint TEXT NOT NULL DEFAULT ''"];
const MIGRATION_057: [&str; 2] = [
    "ALTER TABLE execution_cancellation_replays ADD COLUMN fingerprint TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE execution_cancellation_replays ADD COLUMN response TEXT NOT NULL DEFAULT ''",
];
const MIGRATION_058: [&str; 2] = [
    "ALTER TABLE execution_recovery_audits ADD COLUMN fingerprint TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE execution_recovery_audits ADD COLUMN response TEXT NOT NULL DEFAULT ''",
];
const MIGRATION_059: [&str; 1] = [
    "UPDATE execution_requests SET state = 'needs_operator', updated_at = created_at WHERE state = 'queued' AND request_snapshot = '{}'",
];
const MIGRATION_060: [&str; 1] = [
    "WITH safe_snapshot AS MATERIALIZED (SELECT id, state, CASE WHEN json_valid(request_snapshot) THEN request_snapshot ELSE '{}' END AS request_snapshot, json_valid(request_snapshot) AS snapshot_valid FROM execution_requests)
        UPDATE execution_requests SET state = 'needs_operator', updated_at = created_at WHERE id IN (
        SELECT id FROM safe_snapshot WHERE state NOT IN ('succeeded','failed','cancelled') AND CASE
        WHEN snapshot_valid = 0 THEN 1
        WHEN json_type(request_snapshot) IS NOT 'object' THEN 1
        WHEN json_type(request_snapshot, '$.request_id') IS NOT 'text' THEN 1
        WHEN json_type(request_snapshot, '$.item_id') IS NOT 'text' THEN 1
        WHEN json_type(request_snapshot, '$.idempotency_key') IS NOT 'text' THEN 1
        WHEN json_type(request_snapshot, '$.created_by') IS NOT 'object' THEN 1
        WHEN json_type(request_snapshot, '$.created_by.source') IS NOT 'text' THEN 1
        WHEN json_type(request_snapshot, '$.created_by.subject_id') IS NOT 'text' THEN 1
        WHEN json_type(request_snapshot, '$.created_at') IS NOT 'text' THEN 1
        WHEN datetime(json_extract(request_snapshot, '$.created_at')) IS NULL THEN 1
        WHEN json_extract(request_snapshot, '$.created_at') NOT GLOB '????-??-??T??:??:??*' THEN 1
        WHEN substr(json_extract(request_snapshot, '$.created_at'), -1) IS NOT 'Z' AND substr(json_extract(request_snapshot, '$.created_at'), -6, 1) NOT IN ('+', '-') THEN 1
        WHEN json_type(request_snapshot, '$.selector') IS NOT 'object' THEN 1
        WHEN json_type(request_snapshot, '$.selector.kind') IS NOT 'text' THEN 1
        WHEN json_extract(request_snapshot, '$.selector.kind') NOT IN ('exact_runner','fleet','any') THEN 1
        WHEN json_extract(request_snapshot, '$.selector.kind') = 'exact_runner' AND json_type(request_snapshot, '$.selector.runner_id') IS NOT 'text' THEN 1
        WHEN json_extract(request_snapshot, '$.selector.kind') = 'fleet' AND json_type(request_snapshot, '$.selector.fleet_id') IS NOT 'text' THEN 1
        WHEN json_type(request_snapshot, '$.agent_profile_id') IS NOT 'text' THEN 1
        WHEN json_type(request_snapshot, '$.requested_harness_kind') IS NOT 'text' THEN 1
        WHEN json_type(request_snapshot, '$.requested_model_provider') IS NOT 'text' AND json_type(request_snapshot, '$.requested_model_provider') IS NOT 'null' THEN 1
        WHEN json_type(request_snapshot, '$.requested_model_id') IS NOT 'text' AND json_type(request_snapshot, '$.requested_model_id') IS NOT 'null' THEN 1
        WHEN json_type(request_snapshot, '$.resolved_agent_profile') IS NOT 'object' THEN 1
        WHEN json_type(request_snapshot, '$.resolved_agent_profile.name') IS NOT 'text' THEN 1
        WHEN json_type(request_snapshot, '$.resolved_agent_profile.instructions') IS NOT 'text' THEN 1
        WHEN json_type(request_snapshot, '$.resolved_agent_profile.tool_policy') IS NOT 'object' THEN 1
        WHEN json_type(request_snapshot, '$.resolved_agent_profile.timeout_seconds') IS NOT 'integer' OR json_extract(request_snapshot, '$.resolved_agent_profile.timeout_seconds') < 0 THEN 1
        WHEN json_type(request_snapshot, '$.resolved_agent_profile.budgets') IS NOT 'object' THEN 1
        WHEN json_type(request_snapshot, '$.repository') IS NOT 'object' THEN 1
        WHEN json_type(request_snapshot, '$.repository.kind') IS NOT 'text' THEN 1
        WHEN json_type(request_snapshot, '$.repository.remote') IS NOT 'text' THEN 1
        WHEN json_type(request_snapshot, '$.repository.base_revision') IS NOT 'text' THEN 1
        WHEN json_type(request_snapshot, '$.repository.subdirectory') IS NOT 'text' AND json_type(request_snapshot, '$.repository.subdirectory') IS NOT 'null' THEN 1
        WHEN json_type(request_snapshot, '$.permission_policy') IS NOT 'object' THEN 1
        WHEN json_type(request_snapshot, '$.permission_policy.tools') IS NOT 'array' THEN 1
        WHEN json_type(request_snapshot, '$.permission_policy.network') IS NOT 'true' AND json_type(request_snapshot, '$.permission_policy.network') IS NOT 'false' THEN 1
        WHEN json_type(request_snapshot, '$.timeout_seconds') IS NOT 'integer' OR json_extract(request_snapshot, '$.timeout_seconds') < 0 THEN 1
        WHEN json_type(request_snapshot, '$.budgets') IS NOT 'object' THEN 1
        WHEN json_type(request_snapshot, '$.status_map_policy_id') IS NOT 'text' AND json_type(request_snapshot, '$.status_map_policy_id') IS NOT 'null' THEN 1
        WHEN json_type(request_snapshot, '$.environment') IS NOT 'object' THEN 1
        WHEN json_type(request_snapshot, '$.metadata') IS NOT 'object' THEN 1
        WHEN EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(request_snapshot) THEN request_snapshot ELSE '{}' END, '$.permission_policy.tools') AS tool WHERE tool.type IS NOT 'text') THEN 1
        WHEN EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(request_snapshot) THEN request_snapshot ELSE '{}' END, '$.environment') AS environment WHERE environment.type IS NOT 'object' OR ((json_type(environment.value, '$.value') = 'text') + (json_type(environment.value, '$.secret_reference') = 'text')) != 1) THEN 1
        ELSE 0 END = 1)",
];
const MIGRATION_061: [&str; 2] = [
    "ALTER TABLE execution_attempts ADD COLUMN prepared_at TEXT",
    "ALTER TABLE execution_attempts ADD COLUMN process_id TEXT",
];
const MIGRATION_062: [&str; 1] = ["ALTER TABLE projects ADD COLUMN default_model TEXT"];
