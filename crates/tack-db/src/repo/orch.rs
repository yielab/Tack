//! Repository layer for the Agent-Factory Control Center schema (migrations 019–024):
//! `control_planes`, `orch_links`, `orch_tasks`, `orch_runs`, `orch_events`,
//! `orch_approvals`. See `crates/tack-db/src/migrations.rs` for the authoritative DDL —
//! column names here match it exactly.
//!
//! Two discipline notes that apply to the whole file:
//!
//! 1. **The stored docket Bearer token never leaves this layer in a read DTO.**
//!    [`ControlPlane`] (the read-side struct) exposes `token_set: bool` only. The one
//!    escape hatch is [`Repository::get_control_plane_token`], an internal-only
//!    accessor for the reconciler/adapter (`tack-orch`) to obtain the real credential
//!    for outbound HTTP calls. This mirrors the discipline the S3 backup secret key
//!    already follows at the API layer (`handlers/settings.rs`'s `secret_key_set`).
//! 2. **Every remote-state string column stores whatever docket sent, unvalidated.**
//!    `remote_status`, `state`, `event_type`, etc. are plain `TEXT` and plain `String`
//!    on the Rust side — this layer never rejects an unrecognised value. Degrading
//!    unknown values to "shown as-is" is a whole-cycle non-negotiable;
//!    validating/parsing into an enum happens above this layer, if at all.
//!
//! Batch upserts (`upsert_orch_tasks`, `upsert_orch_runs`, `upsert_orch_events`,
//! `upsert_orch_approvals`) take a slice and run inside a single transaction — one
//! commit per poll cycle, not one per row (the reconciler polls
//! docket every `TACK_ORCH_POLL_SECS` (default 10s); per-row transactions would
//! contend with the UI's writes and surface as `database is locked`). None of these
//! functions make an HTTP call — callers fetch from docket first, then hand the
//! already-parsed data to these functions for a short, self-contained write.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tracing::{debug, instrument};
use uuid::Uuid;

use super::Repository;

// ════════════════════════════════════════════════════════════════════════════════════
// control_planes
// ════════════════════════════════════════════════════════════════════════════════════

/// Read-side view of a `control_planes` row. Deliberately has **no** `token` field —
/// see the module doc. `health` and the remaining reconciler-owned fields are plain
/// strings/numbers rather than a Rust enum: the reconciler (`tack-orch`) owns the
/// `healthy` → `degraded` → `unreachable` state machine, this layer just persists
/// whatever it decided.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ControlPlane {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub api_version: Option<String>,
    pub health: String,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub consecutive_failures: i64,
    /// Whether a token is currently stored. The token itself is never exposed here.
    pub token_set: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateControlPlane {
    pub name: String,
    /// Defaults to `"docket"` (matches the column default) when `None`.
    pub kind: Option<String>,
    pub base_url: String,
    pub token: Option<String>,
}

/// Partial update. Every field is `Option`; `None` means "leave untouched". `token`
/// additionally distinguishes "leave untouched" (`None`) from "clear the stored
/// token" (`Some(None)`) from "set/replace" (`Some(Some(t))`) — the same tri-state
/// shape the API layer (A4) needs for "absent field preserves, explicit null clears".
#[derive(Debug, Clone, Default)]
pub struct UpdateControlPlane {
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub token: Option<Option<String>>,
}

#[derive(sqlx::FromRow)]
struct ControlPlaneRow {
    id: String,
    name: String,
    kind: String,
    base_url: String,
    token: Option<String>,
    api_version: Option<String>,
    health: String,
    last_seen_at: Option<String>,
    consecutive_failures: i64,
    created_at: String,
    updated_at: String,
}

fn parse_rfc3339(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

impl ControlPlaneRow {
    fn into_control_plane(self) -> ControlPlane {
        ControlPlane {
            id: Uuid::parse_str(&self.id).unwrap(),
            name: self.name,
            kind: self.kind,
            base_url: self.base_url,
            api_version: self.api_version,
            health: self.health,
            last_seen_at: self.last_seen_at.as_deref().map(parse_rfc3339),
            consecutive_failures: self.consecutive_failures,
            token_set: self.token.is_some(),
            created_at: parse_rfc3339(&self.created_at),
            updated_at: parse_rfc3339(&self.updated_at),
        }
    }
}

const CONTROL_PLANE_COLUMNS: &str = "id, name, kind, base_url, token, api_version, health, \
     last_seen_at, consecutive_failures, created_at, updated_at";

impl Repository {
    #[instrument(skip(self, input))]
    pub async fn create_control_plane(
        &self,
        input: CreateControlPlane,
    ) -> Result<ControlPlane, sqlx::Error> {
        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        let kind = input.kind.unwrap_or_else(|| "docket".to_string());

        sqlx::query(
            "INSERT INTO control_planes
                (id, name, kind, base_url, token, health, consecutive_failures, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 'unknown', 0, ?, ?)",
        )
        .bind(id.to_string())
        .bind(&input.name)
        .bind(&kind)
        .bind(&input.base_url)
        .bind(&input.token)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await?;

        debug!(control_plane_id = %id, name = %input.name, "Control plane created");

        self.get_control_plane(id).await
    }

    #[instrument(skip(self))]
    pub async fn get_control_plane(&self, id: Uuid) -> Result<ControlPlane, sqlx::Error> {
        let row: ControlPlaneRow = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {CONTROL_PLANE_COLUMNS} FROM control_planes WHERE id = ?"
        )))
        .bind(id.to_string())
        .fetch_one(self.pool())
        .await?;

        Ok(row.into_control_plane())
    }

    #[instrument(skip(self))]
    pub async fn list_control_planes(&self) -> Result<Vec<ControlPlane>, sqlx::Error> {
        let rows: Vec<ControlPlaneRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {CONTROL_PLANE_COLUMNS} FROM control_planes ORDER BY name"
        )))
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_control_plane()).collect())
    }

    /// Internal-only accessor for the reconciler/adapter (`tack-orch`) to obtain the
    /// real Bearer token for an outbound docket call. **Never** wire this into an API
    /// response — use [`ControlPlane::token_set`] there instead.
    #[instrument(skip(self))]
    pub async fn get_control_plane_token(&self, id: Uuid) -> Result<Option<String>, sqlx::Error> {
        let token: Option<Option<String>> =
            sqlx::query_scalar("SELECT token FROM control_planes WHERE id = ?")
                .bind(id.to_string())
                .fetch_optional(self.pool())
                .await?;

        Ok(token.flatten())
    }

    #[instrument(skip(self, input))]
    pub async fn update_control_plane(
        &self,
        id: Uuid,
        input: UpdateControlPlane,
    ) -> Result<ControlPlane, sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        if let Some(name) = &input.name {
            sqlx::query("UPDATE control_planes SET name = ?, updated_at = ? WHERE id = ?")
                .bind(name)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }

        if let Some(base_url) = &input.base_url {
            sqlx::query("UPDATE control_planes SET base_url = ?, updated_at = ? WHERE id = ?")
                .bind(base_url)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }

        if let Some(token) = &input.token {
            sqlx::query("UPDATE control_planes SET token = ?, updated_at = ? WHERE id = ?")
                .bind(token)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }

        self.get_control_plane(id).await
    }

    /// Persists the reconciler's health-check outcome. The reconciler (`tack-orch`)
    /// owns the `healthy`/`degraded`/`unreachable` state machine and the
    /// consecutive-failure counter — this just writes down what it decided.
    /// `last_seen_at` should be `Some(now)` on a successful poll and left as the
    /// previous value (pass `None`) on a failed one.
    #[instrument(skip(self))]
    pub async fn update_control_plane_health(
        &self,
        id: Uuid,
        health: &str,
        last_seen_at: Option<DateTime<Utc>>,
        consecutive_failures: i64,
        api_version: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        if let Some(seen) = last_seen_at {
            sqlx::query(
                "UPDATE control_planes
                 SET health = ?, last_seen_at = ?, consecutive_failures = ?,
                     api_version = COALESCE(?, api_version), updated_at = ?
                 WHERE id = ?",
            )
            .bind(health)
            .bind(seen.to_rfc3339())
            .bind(consecutive_failures)
            .bind(api_version)
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        } else {
            sqlx::query(
                "UPDATE control_planes
                 SET health = ?, consecutive_failures = ?,
                     api_version = COALESCE(?, api_version), updated_at = ?
                 WHERE id = ?",
            )
            .bind(health)
            .bind(consecutive_failures)
            .bind(api_version)
            .bind(&now)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn delete_control_plane(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM control_planes WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_links
// ════════════════════════════════════════════════════════════════════════════════════

/// One control-plane link per project — `project_id` is the primary key (W0-B's design
/// decision; see migration 020's comment).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrchLink {
    pub project_id: Uuid,
    pub control_plane_id: Uuid,
    pub remote_project: String,
    pub pipeline_file: Option<String>,
    pub blueprint: Option<String>,
    pub auto_dispatch: bool,
    /// A user-set budget cap, not a derived spend figure — deliberately **not**
    /// suffixed `_estimated` (see migration 020's comment).
    pub budget_usd: Option<f64>,
    /// `status_map` JSON.3. Save-time validation against the
    /// project's `WorkflowConfig` happens above this layer (tack-api), not here.
    pub status_map: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UpsertOrchLink {
    pub control_plane_id: Uuid,
    pub remote_project: String,
    pub pipeline_file: Option<String>,
    pub blueprint: Option<String>,
    pub auto_dispatch: bool,
    pub budget_usd: Option<f64>,
    pub status_map: serde_json::Value,
}

#[derive(sqlx::FromRow)]
struct OrchLinkRow {
    project_id: String,
    control_plane_id: String,
    remote_project: String,
    pipeline_file: Option<String>,
    blueprint: Option<String>,
    auto_dispatch: i64,
    budget_usd: Option<f64>,
    status_map: String,
    created_at: String,
    updated_at: String,
}

impl OrchLinkRow {
    fn into_orch_link(self) -> OrchLink {
        OrchLink {
            project_id: Uuid::parse_str(&self.project_id).unwrap(),
            control_plane_id: Uuid::parse_str(&self.control_plane_id).unwrap(),
            remote_project: self.remote_project,
            pipeline_file: self.pipeline_file,
            blueprint: self.blueprint,
            auto_dispatch: self.auto_dispatch != 0,
            budget_usd: self.budget_usd,
            status_map: serde_json::from_str(&self.status_map).unwrap_or(serde_json::json!({})),
            created_at: parse_rfc3339(&self.created_at),
            updated_at: parse_rfc3339(&self.updated_at),
        }
    }
}

const ORCH_LINK_COLUMNS: &str = "project_id, control_plane_id, remote_project, pipeline_file, \
     blueprint, auto_dispatch, budget_usd, status_map, created_at, updated_at";

impl Repository {
    /// Create-or-replace the link for a project (`ON CONFLICT(project_id)`).
    #[instrument(skip(self, input))]
    pub async fn upsert_orch_link(
        &self,
        project_id: Uuid,
        input: UpsertOrchLink,
    ) -> Result<OrchLink, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let status_map = input.status_map.to_string();

        sqlx::query(
            "INSERT INTO orch_links
                (project_id, control_plane_id, remote_project, pipeline_file, blueprint,
                 auto_dispatch, budget_usd, status_map, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(project_id) DO UPDATE SET
                control_plane_id = excluded.control_plane_id,
                remote_project = excluded.remote_project,
                pipeline_file = excluded.pipeline_file,
                blueprint = excluded.blueprint,
                auto_dispatch = excluded.auto_dispatch,
                budget_usd = excluded.budget_usd,
                status_map = excluded.status_map,
                updated_at = excluded.updated_at",
        )
        .bind(project_id.to_string())
        .bind(input.control_plane_id.to_string())
        .bind(&input.remote_project)
        .bind(&input.pipeline_file)
        .bind(&input.blueprint)
        .bind(input.auto_dispatch as i32)
        .bind(input.budget_usd)
        .bind(&status_map)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await?;

        self.get_orch_link(project_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    #[instrument(skip(self))]
    pub async fn get_orch_link(&self, project_id: Uuid) -> Result<Option<OrchLink>, sqlx::Error> {
        let row: Option<OrchLinkRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {ORCH_LINK_COLUMNS} FROM orch_links WHERE project_id = ?"
        )))
        .bind(project_id.to_string())
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(|r| r.into_orch_link()))
    }

    /// Every project linked to a given control plane — the reconciler uses this to
    /// know which projects to poll `/runs?project=` for.
    #[instrument(skip(self))]
    pub async fn list_orch_links_for_plane(
        &self,
        control_plane_id: Uuid,
    ) -> Result<Vec<OrchLink>, sqlx::Error> {
        let rows: Vec<OrchLinkRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {ORCH_LINK_COLUMNS} FROM orch_links WHERE control_plane_id = ? \
             ORDER BY remote_project"
        )))
        .bind(control_plane_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_orch_link()).collect())
    }

    /// Total number of project ↔ control-plane links, across every plane.
    /// Used by `GET /api/settings/orchestration`'s `linked_project_count`
    /// — a cheap health signal for the settings UI ("orchestration
    /// is on, but nothing is linked yet" vs. "N projects are linked").
    #[instrument(skip(self))]
    pub async fn count_orch_links(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT COUNT(*) FROM orch_links")
            .fetch_one(self.pool())
            .await
    }

    #[instrument(skip(self))]
    pub async fn delete_orch_link(&self, project_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM orch_links WHERE project_id = ?")
            .bind(project_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_tasks — composite PK (item_id, remote_task_id)
// ════════════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrchTask {
    pub item_id: Uuid,
    pub remote_task_id: String,
    pub remote_run_id: Option<String>,
    /// docket's raw task status string — stored as-is, unvalidated.
    pub remote_status: String,
    pub attempt: i64,
    /// Token counts are the primary measure.
    pub tokens_in: i64,
    pub tokens_out: i64,
    /// Derived, estimated — never presented as actual spend.
    pub cost_usd_estimated: Option<f64>,
    pub dispatched_at: DateTime<Utc>,
    pub trusted: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One row to upsert. `remote_run_id` is intentionally not FK-enforced against
/// `orch_runs` — a task can exist before its run is mirrored (see migration 021's
/// comment); correlation is a query-time join, not a constraint.
#[derive(Debug, Clone)]
pub struct NewOrchTask {
    pub item_id: Uuid,
    pub remote_task_id: String,
    pub remote_run_id: Option<String>,
    pub remote_status: String,
    pub attempt: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cost_usd_estimated: Option<f64>,
    pub dispatched_at: DateTime<Utc>,
    pub trusted: bool,
}

#[derive(sqlx::FromRow)]
struct OrchTaskRow {
    item_id: String,
    remote_task_id: String,
    remote_run_id: Option<String>,
    remote_status: String,
    attempt: i64,
    tokens_in: i64,
    tokens_out: i64,
    cost_usd_estimated: Option<f64>,
    dispatched_at: String,
    trusted: i64,
    created_at: String,
    updated_at: String,
}

impl OrchTaskRow {
    fn into_orch_task(self) -> OrchTask {
        OrchTask {
            item_id: Uuid::parse_str(&self.item_id).unwrap(),
            remote_task_id: self.remote_task_id,
            remote_run_id: self.remote_run_id,
            remote_status: self.remote_status,
            attempt: self.attempt,
            tokens_in: self.tokens_in,
            tokens_out: self.tokens_out,
            cost_usd_estimated: self.cost_usd_estimated,
            dispatched_at: parse_rfc3339(&self.dispatched_at),
            trusted: self.trusted != 0,
            created_at: parse_rfc3339(&self.created_at),
            updated_at: parse_rfc3339(&self.updated_at),
        }
    }
}

const ORCH_TASK_COLUMNS: &str = "item_id, remote_task_id, remote_run_id, remote_status, \
     attempt, tokens_in, tokens_out, cost_usd_estimated, dispatched_at, trusted, \
     created_at, updated_at";

impl Repository {
    /// Batch upsert, one transaction for the whole slice. Idempotent: re-upserting the
    /// same `(item_id, remote_task_id)` pairs updates in place, never duplicates.
    /// No-op (no transaction opened) for an empty slice.
    #[instrument(skip(self, tasks), fields(count = tasks.len()))]
    pub async fn upsert_orch_tasks(&self, tasks: &[NewOrchTask]) -> Result<(), sqlx::Error> {
        if tasks.is_empty() {
            return Ok(());
        }

        let now = Utc::now().to_rfc3339();
        let mut tx = self.pool().begin().await?;

        for t in tasks {
            sqlx::query(
                "INSERT INTO orch_tasks
                    (item_id, remote_task_id, remote_run_id, remote_status, attempt,
                     tokens_in, tokens_out, cost_usd_estimated, dispatched_at, trusted,
                     created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(item_id, remote_task_id) DO UPDATE SET
                    remote_run_id = excluded.remote_run_id,
                    remote_status = excluded.remote_status,
                    attempt = excluded.attempt,
                    tokens_in = excluded.tokens_in,
                    tokens_out = excluded.tokens_out,
                    cost_usd_estimated = excluded.cost_usd_estimated,
                    dispatched_at = excluded.dispatched_at,
                    trusted = excluded.trusted,
                    updated_at = excluded.updated_at",
            )
            .bind(t.item_id.to_string())
            .bind(&t.remote_task_id)
            .bind(&t.remote_run_id)
            .bind(&t.remote_status)
            .bind(t.attempt)
            .bind(t.tokens_in)
            .bind(t.tokens_out)
            .bind(t.cost_usd_estimated)
            .bind(t.dispatched_at.to_rfc3339())
            .bind(t.trusted as i32)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        debug!(count = tasks.len(), "orch_tasks upserted");
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_orch_task(
        &self,
        item_id: Uuid,
        remote_task_id: &str,
    ) -> Result<Option<OrchTask>, sqlx::Error> {
        let row: Option<OrchTaskRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {ORCH_TASK_COLUMNS} FROM orch_tasks WHERE item_id = ? AND remote_task_id = ?"
        )))
        .bind(item_id.to_string())
        .bind(remote_task_id)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(|r| r.into_orch_task()))
    }

    #[instrument(skip(self))]
    pub async fn list_orch_tasks_for_item(
        &self,
        item_id: Uuid,
    ) -> Result<Vec<OrchTask>, sqlx::Error> {
        let rows: Vec<OrchTaskRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {ORCH_TASK_COLUMNS} FROM orch_tasks WHERE item_id = ? ORDER BY attempt DESC"
        )))
        .bind(item_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_orch_task()).collect())
    }

    /// Looks up a task by docket's `remote_task_id` alone (item unknown) — used to
    /// correlate an incoming approval's `context.taskId` to an item.
    /// Multiple items could theoretically share a stale `remote_task_id` only across
    /// different attempts of the *same* item (the PK is per-item), so this returns the
    /// most recently dispatched match.
    #[instrument(skip(self))]
    pub async fn find_orch_task_by_remote_task_id(
        &self,
        remote_task_id: &str,
    ) -> Result<Option<OrchTask>, sqlx::Error> {
        let row: Option<OrchTaskRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {ORCH_TASK_COLUMNS} FROM orch_tasks WHERE remote_task_id = ? \
             ORDER BY dispatched_at DESC LIMIT 1"
        )))
        .bind(remote_task_id)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(|r| r.into_orch_task()))
    }

    /// One row per item in `project_id` that has **at least one** `orch_tasks`
    /// row (an inner join via `JOIN items` — the bulk badge endpoint keeps
    /// this inner-join contract rather
    /// than a left join with an explicit null state), carrying only its
    /// *latest* attempt. "Latest" = highest `attempt` number, ties broken by
    /// `dispatched_at` desc, with a final tie-break on `remote_task_id` desc
    /// purely to make the query deterministic — the PK is `(item_id,
    /// remote_task_id)`, so two rows for the same item can never share every
    /// one of the three columns, and the anti-join below always yields exactly
    /// one row per `item_id`.
    #[instrument(skip(self))]
    pub async fn list_latest_orch_task_status_for_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<LatestOrchTaskStatus>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            item_id: String,
            remote_status: String,
            attempt: i64,
            updated_at: String,
        }

        let rows: Vec<Row> = sqlx::query_as(
            "SELECT t.item_id, t.remote_status, t.attempt, t.updated_at
             FROM orch_tasks t
             JOIN items i ON i.id = t.item_id
             WHERE i.project_id = ?
               AND NOT EXISTS (
                 SELECT 1 FROM orch_tasks t2
                 WHERE t2.item_id = t.item_id
                   AND (t2.attempt > t.attempt
                        OR (t2.attempt = t.attempt AND t2.dispatched_at > t.dispatched_at)
                        OR (t2.attempt = t.attempt AND t2.dispatched_at = t.dispatched_at
                            AND t2.remote_task_id > t.remote_task_id))
               )",
        )
        .bind(project_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| LatestOrchTaskStatus {
                item_id: Uuid::parse_str(&r.item_id).unwrap(),
                remote_status: r.remote_status,
                attempt: r.attempt,
                updated_at: parse_rfc3339(&r.updated_at),
            })
            .collect())
    }
}

/// One row of [`Repository::list_latest_orch_task_status_for_project`] — the
/// minimum a Board/List/Table badge needs. See that method's doc comment for
/// the "latest" tie-break rule and the inner-join rationale.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LatestOrchTaskStatus {
    pub item_id: Uuid,
    pub remote_status: String,
    pub attempt: i64,
    pub updated_at: DateTime<Utc>,
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_runs
// ════════════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrchRun {
    pub run_id: String,
    pub control_plane_id: Uuid,
    /// `None` = mirrored, unattributed — the normal case for a docket-CLI-dispatched
    /// run pre-Phase-35 (must not be treated as an error; see migration 022's comment).
    pub item_id: Option<Uuid>,
    pub remote_project: String,
    /// Raw `RunSource` string (`cli` / `webhook` / `schedule` / `sweep` / `mcp` / or an
    /// unrecognised value) — stored as-is.
    pub source: String,
    /// Raw `RunState` string — stored as-is.
    pub state: String,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewOrchRun {
    pub run_id: String,
    pub item_id: Option<Uuid>,
    pub remote_project: String,
    pub source: String,
    pub state: String,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(sqlx::FromRow)]
struct OrchRunRow {
    run_id: String,
    control_plane_id: String,
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

impl OrchRunRow {
    fn into_orch_run(self) -> OrchRun {
        OrchRun {
            run_id: self.run_id,
            control_plane_id: Uuid::parse_str(&self.control_plane_id).unwrap(),
            item_id: self.item_id.as_deref().map(|s| Uuid::parse_str(s).unwrap()),
            remote_project: self.remote_project,
            source: self.source,
            state: self.state,
            started_at: self.started_at.as_deref().map(parse_rfc3339),
            ended_at: self.ended_at.as_deref().map(parse_rfc3339),
            error: self.error,
            created_at: parse_rfc3339(&self.created_at),
            updated_at: parse_rfc3339(&self.updated_at),
        }
    }
}

// `run_id` here is a SELECT alias, not the physical column — migration 037
// renamed the real
// column to `external_run_id` as part of widening the primary key to
// `(control_plane_id, external_run_id, run_attempt)`. Aliasing it back to
// `run_id` in every read keeps [`OrchRun`]/[`NewOrchRun`]'s Rust-level shape
// — and therefore every caller in `tack-api`/`tack-orch` that only ever
// touches `.run_id`, never the raw column name — unchanged. `run_attempt`
// and `correlation_id` are deliberately not read or written here yet: no
// caller has a value for either until the reshape that actually dispatches
// retries and mints correlation ids lands, so this layer keeps
// treating every run as attempt 1 with no correlation id, which is exactly
// what migration 037 backfilled onto every pre-existing row.
const ORCH_RUN_COLUMNS: &str = "external_run_id AS run_id, control_plane_id, item_id, \
     remote_project, source, state, started_at, ended_at, error, created_at, updated_at";

impl Repository {
    /// Batch upsert, one transaction for the whole slice. Idempotent: re-polling the
    /// same run just refreshes `state`/`ended_at`/etc. Conflicts on
    /// `(control_plane_id, external_run_id, run_attempt)` — the widened primary key
    /// from migration 037 — but every row this function writes still pins
    /// `run_attempt` to `1`, so for any run this has ever upserted the dedup key is
    /// still, in effect, `(control_plane_id, external_run_id)`: identical to the old
    /// single-column `run_id` PK for the common case of one `control_plane_id` per
    /// external run id, which is the only case this function's caller (the
    /// reconciler, one batch per plane) has ever produced.
    #[instrument(skip(self, runs), fields(count = runs.len()))]
    pub async fn upsert_orch_runs(
        &self,
        control_plane_id: Uuid,
        runs: &[NewOrchRun],
    ) -> Result<(), sqlx::Error> {
        if runs.is_empty() {
            return Ok(());
        }

        let now = Utc::now().to_rfc3339();
        let mut tx = self.pool().begin().await?;

        for r in runs {
            sqlx::query(
                "INSERT INTO orch_runs
                    (control_plane_id, external_run_id, run_attempt, item_id, remote_project,
                     source, state, started_at, ended_at, error, created_at, updated_at)
                 VALUES (?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(control_plane_id, external_run_id, run_attempt) DO UPDATE SET
                    -- item_id is only ever set going forward (a later poll may learn
                    -- the attribution a CLI-dispatched run didn't have); never clear
                    -- a known attribution back to NULL.
                    item_id = COALESCE(excluded.item_id, orch_runs.item_id),
                    remote_project = excluded.remote_project,
                    source = excluded.source,
                    state = excluded.state,
                    started_at = excluded.started_at,
                    ended_at = excluded.ended_at,
                    error = excluded.error,
                    updated_at = excluded.updated_at",
            )
            .bind(control_plane_id.to_string())
            .bind(&r.run_id)
            .bind(r.item_id.map(|i| i.to_string()))
            .bind(&r.remote_project)
            .bind(&r.source)
            .bind(&r.state)
            .bind(r.started_at.map(|d| d.to_rfc3339()))
            .bind(r.ended_at.map(|d| d.to_rfc3339()))
            .bind(&r.error)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        debug!(count = runs.len(), "orch_runs upserted");
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_orch_run(&self, run_id: &str) -> Result<Option<OrchRun>, sqlx::Error> {
        let row: Option<OrchRunRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {ORCH_RUN_COLUMNS} FROM orch_runs WHERE external_run_id = ?"
        )))
        .bind(run_id)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(|r| r.into_orch_run()))
    }

    #[instrument(skip(self))]
    pub async fn list_orch_runs_for_item(
        &self,
        item_id: Uuid,
    ) -> Result<Vec<OrchRun>, sqlx::Error> {
        let rows: Vec<OrchRunRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {ORCH_RUN_COLUMNS} FROM orch_runs WHERE item_id = ? ORDER BY created_at DESC"
        )))
        .bind(item_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_orch_run()).collect())
    }
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_events — append-only telemetry mirror
// ════════════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrchEvent {
    pub id: Uuid,
    pub control_plane_id: Uuid,
    pub item_id: Option<Uuid>,
    pub run_id: Option<String>,
    /// docket's raw trace event type, stored verbatim — including types Tack doesn't
    /// yet recognise.
    pub event_type: String,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// One event to upsert. `id` is caller-assigned: the ingester must
/// derive a stable id from the source event (e.g. from docket's own event id/offset,
/// or a deterministic hash of `(run_id, occurred_at, event_type)`) so that re-polling
/// the same trace window is idempotent rather than duplicating rows. This layer has no
/// way to detect a "duplicate" event on its own — `orch_events` has no natural key
/// besides the id the caller provides.
#[derive(Debug, Clone)]
pub struct NewOrchEvent {
    pub id: Uuid,
    pub item_id: Option<Uuid>,
    pub run_id: Option<String>,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct OrchEventRow {
    id: String,
    control_plane_id: String,
    item_id: Option<String>,
    run_id: Option<String>,
    event_type: String,
    payload: String,
    occurred_at: String,
    created_at: String,
}

impl OrchEventRow {
    fn into_orch_event(self) -> OrchEvent {
        OrchEvent {
            id: Uuid::parse_str(&self.id).unwrap(),
            control_plane_id: Uuid::parse_str(&self.control_plane_id).unwrap(),
            item_id: self.item_id.as_deref().map(|s| Uuid::parse_str(s).unwrap()),
            run_id: self.run_id,
            event_type: self.event_type,
            payload: serde_json::from_str(&self.payload).unwrap_or(serde_json::json!({})),
            occurred_at: parse_rfc3339(&self.occurred_at),
            created_at: parse_rfc3339(&self.created_at),
        }
    }
}

const ORCH_EVENT_COLUMNS: &str = "id, control_plane_id, item_id, run_id, event_type, payload, \
     occurred_at, created_at";

impl Repository {
    /// Batch upsert (`ON CONFLICT(id)`), one transaction for the whole slice.
    #[instrument(skip(self, events), fields(count = events.len()))]
    pub async fn upsert_orch_events(
        &self,
        control_plane_id: Uuid,
        events: &[NewOrchEvent],
    ) -> Result<(), sqlx::Error> {
        if events.is_empty() {
            return Ok(());
        }

        let now = Utc::now().to_rfc3339();
        let mut tx = self.pool().begin().await?;

        for e in events {
            let payload = e.payload.to_string();
            sqlx::query(
                "INSERT INTO orch_events
                    (id, control_plane_id, item_id, run_id, event_type, payload,
                     occurred_at, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(id) DO UPDATE SET
                    item_id = excluded.item_id,
                    run_id = excluded.run_id,
                    event_type = excluded.event_type,
                    payload = excluded.payload,
                    occurred_at = excluded.occurred_at",
            )
            .bind(e.id.to_string())
            .bind(control_plane_id.to_string())
            .bind(e.item_id.map(|i| i.to_string()))
            .bind(&e.run_id)
            .bind(&e.event_type)
            .bind(&payload)
            .bind(e.occurred_at.to_rfc3339())
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        debug!(count = events.len(), "orch_events upserted");
        Ok(())
    }

    /// An item's event timeline, chronological (oldest first). Pass `limit` to cap
    /// the number of rows (e.g. for a UI page); `None` returns everything.
    #[instrument(skip(self))]
    pub async fn list_orch_events_for_item(
        &self,
        item_id: Uuid,
        limit: Option<i64>,
    ) -> Result<Vec<OrchEvent>, sqlx::Error> {
        let rows: Vec<OrchEventRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {ORCH_EVENT_COLUMNS} FROM orch_events WHERE item_id = ? \
             ORDER BY occurred_at ASC LIMIT ?"
        )))
        .bind(item_id.to_string())
        .bind(limit.unwrap_or(i64::MAX))
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_orch_event()).collect())
    }
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_approvals
// ════════════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrchApproval {
    /// docket's approval token — a correlation id, not a credential.
    pub token: String,
    pub control_plane_id: Uuid,
    /// `None` = uncorrelated; must still surface in the fleet-wide approvals inbox.
    pub item_id: Option<Uuid>,
    pub remote_task_id: Option<String>,
    pub agent: Option<String>,
    pub action: Option<String>,
    /// Raw `ApprovalState` string — stored as-is.
    pub state: String,
    pub requested_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewOrchApproval {
    pub token: String,
    pub item_id: Option<Uuid>,
    pub remote_task_id: Option<String>,
    pub agent: Option<String>,
    pub action: Option<String>,
    pub state: String,
    pub requested_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct OrchApprovalRow {
    token: String,
    control_plane_id: String,
    item_id: Option<String>,
    remote_task_id: Option<String>,
    agent: Option<String>,
    action: Option<String>,
    state: String,
    requested_at: String,
    decided_at: Option<String>,
    created_at: String,
    updated_at: String,
}

impl OrchApprovalRow {
    fn into_orch_approval(self) -> OrchApproval {
        OrchApproval {
            token: self.token,
            control_plane_id: Uuid::parse_str(&self.control_plane_id).unwrap(),
            item_id: self.item_id.as_deref().map(|s| Uuid::parse_str(s).unwrap()),
            remote_task_id: self.remote_task_id,
            agent: self.agent,
            action: self.action,
            state: self.state,
            requested_at: parse_rfc3339(&self.requested_at),
            decided_at: self.decided_at.as_deref().map(parse_rfc3339),
            created_at: parse_rfc3339(&self.created_at),
            updated_at: parse_rfc3339(&self.updated_at),
        }
    }
}

const ORCH_APPROVAL_COLUMNS: &str = "token, control_plane_id, item_id, remote_task_id, agent, \
     action, state, requested_at, decided_at, created_at, updated_at";

impl Repository {
    /// Batch upsert (`ON CONFLICT(token)`), one transaction for the whole slice.
    /// `item_id` follows the same "never clear a known attribution" rule as
    /// `upsert_orch_runs` — a later poll may correlate an approval that arrived
    /// uncorrelated, but a correlation is never un-learned.
    #[instrument(skip(self, approvals), fields(count = approvals.len()))]
    pub async fn upsert_orch_approvals(
        &self,
        control_plane_id: Uuid,
        approvals: &[NewOrchApproval],
    ) -> Result<(), sqlx::Error> {
        if approvals.is_empty() {
            return Ok(());
        }

        let now = Utc::now().to_rfc3339();
        let mut tx = self.pool().begin().await?;

        for a in approvals {
            sqlx::query(
                "INSERT INTO orch_approvals
                    (token, control_plane_id, item_id, remote_task_id, agent, action,
                     state, requested_at, decided_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(token) DO UPDATE SET
                    item_id = COALESCE(excluded.item_id, orch_approvals.item_id),
                    remote_task_id = excluded.remote_task_id,
                    agent = excluded.agent,
                    action = excluded.action,
                    state = excluded.state,
                    decided_at = excluded.decided_at,
                    updated_at = excluded.updated_at",
            )
            .bind(&a.token)
            .bind(control_plane_id.to_string())
            .bind(a.item_id.map(|i| i.to_string()))
            .bind(&a.remote_task_id)
            .bind(&a.agent)
            .bind(&a.action)
            .bind(&a.state)
            .bind(a.requested_at.to_rfc3339())
            .bind(a.decided_at.map(|d| d.to_rfc3339()))
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        debug!(count = approvals.len(), "orch_approvals upserted");
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_orch_approval(
        &self,
        token: &str,
    ) -> Result<Option<OrchApproval>, sqlx::Error> {
        let row: Option<OrchApprovalRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {ORCH_APPROVAL_COLUMNS} FROM orch_approvals WHERE token = ?"
        )))
        .bind(token)
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(|r| r.into_orch_approval()))
    }

    /// Every approval for one item — pending **and** decided, since the item-detail
    /// "Agent Activity" tab is a history view, not just a pending-inbox (see
    /// `frontend/src/shared/agentActivity/api.ts`'s `ItemAgentActivity.approvals`
    /// doc comment). Newest-requested first, matching the tab's overall
    /// newest-first orientation (`list_orch_tasks_for_item`'s `attempt DESC`).
    #[instrument(skip(self))]
    pub async fn list_orch_approvals_for_item(
        &self,
        item_id: Uuid,
    ) -> Result<Vec<OrchApproval>, sqlx::Error> {
        let rows: Vec<OrchApprovalRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {ORCH_APPROVAL_COLUMNS} FROM orch_approvals WHERE item_id = ? \
             ORDER BY requested_at DESC"
        )))
        .bind(item_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_orch_approval()).collect())
    }

    /// Fleet-wide pending-approval inbox, **oldest first** — docket approvals fail
    /// closed on timeout, so surfacing the longest-waiting one first has a real cost.
    /// Includes uncorrelated (`item_id IS NULL`) records.
    #[instrument(skip(self))]
    pub async fn list_pending_orch_approvals(&self) -> Result<Vec<OrchApproval>, sqlx::Error> {
        let rows: Vec<OrchApprovalRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {ORCH_APPROVAL_COLUMNS} FROM orch_approvals WHERE state = 'pending' \
             ORDER BY requested_at ASC"
        )))
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_orch_approval()).collect())
    }

    /// The same fleet-wide, oldest-first pending inbox as
    /// [`Self::list_pending_orch_approvals`], enriched with the correlated
    /// control plane / item / project — everything the inbox UI needs
    /// to show "the requesting agent, the action text, and the correlated
    /// item" in one round trip rather than N+1 follow-up reads. `LEFT JOIN`s
    /// against `items`/`projects` so an uncorrelated approval
    /// (`item_id IS NULL`) still comes back with every `item_*`/`project_*`
    /// field `None` rather than being silently dropped — the whole reason
    /// this query exists is that an uncorrelated approval is the most
    /// likely one to be silently blocking a fleet, and must
    /// still surface. `control_planes` is an inner join: `orch_approvals.
    /// control_plane_id` is `NOT NULL` and cascades on delete, so a
    /// dangling reference here would mean the schema's own invariant broke,
    /// not a case to degrade gracefully.
    #[instrument(skip(self))]
    pub async fn list_pending_orch_approvals_with_context(
        &self,
    ) -> Result<Vec<PendingOrchApproval>, sqlx::Error> {
        let rows: Vec<PendingOrchApprovalRow> = sqlx::query_as(
            "SELECT
                a.token, a.control_plane_id, cp.name AS control_plane_name,
                a.item_id, i.title AS item_title, i.status AS item_status,
                i.project_id AS project_id, p.name AS project_name,
                a.remote_task_id, a.agent, a.action, a.requested_at
             FROM orch_approvals a
             JOIN control_planes cp ON cp.id = a.control_plane_id
             LEFT JOIN items i ON i.id = a.item_id
             LEFT JOIN projects p ON p.id = i.project_id
             WHERE a.state = 'pending'
             ORDER BY a.requested_at ASC",
        )
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| r.into_pending_approval())
            .collect())
    }

    /// Writes back docket's real decision once
    /// [`tack_orch::ControlPlane::decide_approval`] has actually resumed or
    /// killed the gated task on docket's side — this is a **local mirror
    /// update only**, called after that HTTP call already succeeded, never
    /// instead of it (the decision is real the moment docket accepts it;
    /// this just keeps Tack's own `orch_approvals` row from re-appearing in
    /// [`Self::list_pending_orch_approvals_with_context`] on the next
    /// fetch). A no-op, not an error, if `token` isn't in Tack's mirror at
    /// all — the caller only ever has a token that came from this table in
    /// the first place (the inbox lists it), so this is defensive, not an
    /// expected path.
    #[instrument(skip(self))]
    pub async fn mark_orch_approval_decided(
        &self,
        token: &str,
        state: &str,
        decided_at: chrono::DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE orch_approvals SET state = ?, decided_at = ?, updated_at = ? WHERE token = ?",
        )
        .bind(state)
        .bind(decided_at.to_rfc3339())
        .bind(&now)
        .bind(token)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}

/// One row of the fleet-wide approvals inbox, enriched with correlated
/// context — see [`Repository::list_pending_orch_approvals_with_context`].
/// Always `state == "pending"` (the query's own `WHERE`), so unlike
/// [`OrchApproval`] this doesn't carry `state`/`decided_at`/`created_at`/
/// `updated_at` at all — nothing here that a caller would ever read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PendingOrchApproval {
    pub token: String,
    pub control_plane_id: Uuid,
    pub control_plane_name: String,
    /// `None` = uncorrelated (the reconciler could not attribute this
    /// approval to a Tack item) — see the query's own doc comment for why
    /// this must never be filtered out.
    pub item_id: Option<Uuid>,
    pub item_title: Option<String>,
    pub item_status: Option<String>,
    pub project_id: Option<Uuid>,
    pub project_name: Option<String>,
    pub remote_task_id: Option<String>,
    /// `orch_approvals.agent` — populated from docket's `role` field on
    /// ingestion. There is no separate "agent"
    /// concept on docket's wire shape — role is the closest field.
    pub agent: Option<String>,
    pub action: Option<String>,
    pub requested_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct PendingOrchApprovalRow {
    token: String,
    control_plane_id: String,
    control_plane_name: String,
    item_id: Option<String>,
    item_title: Option<String>,
    item_status: Option<String>,
    project_id: Option<String>,
    project_name: Option<String>,
    remote_task_id: Option<String>,
    agent: Option<String>,
    action: Option<String>,
    requested_at: String,
}

impl PendingOrchApprovalRow {
    fn into_pending_approval(self) -> PendingOrchApproval {
        PendingOrchApproval {
            token: self.token,
            control_plane_id: Uuid::parse_str(&self.control_plane_id).unwrap(),
            control_plane_name: self.control_plane_name,
            item_id: self.item_id.as_deref().map(|s| Uuid::parse_str(s).unwrap()),
            item_title: self.item_title,
            item_status: self.item_status,
            project_id: self
                .project_id
                .as_deref()
                .map(|s| Uuid::parse_str(s).unwrap()),
            project_name: self.project_name,
            remote_task_id: self.remote_task_id,
            agent: self.agent,
            action: self.action,
            requested_at: parse_rfc3339(&self.requested_at),
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_metrics — append-only Prometheus scrape mirror
// ════════════════════════════════════════════════════════════════════════════════════
//
// See migration 025's
// comment in `migrations.rs`. Unlike every other table in this file, there is no
// `ON CONFLICT` upsert path here: a metric sample has no natural key across scrapes,
// so `upsert_orch_metrics` (named for consistency with this file's other batch
// functions) is a plain
// batch `INSERT`, one transaction for the whole slice.

fn labels_to_json(labels: &BTreeMap<String, String>) -> String {
    // BTreeMap serializes with keys in sorted order — this is what makes `labels`
    // a stable, comparable string: the same logical label set always produces the
    // same JSON text, which both the retention rollup's GROUP BY and
    // list_latest_orch_metrics's equality-based correlated subquery depend on.
    serde_json::to_string(labels).unwrap_or_else(|_| "{}".to_string())
}

fn labels_from_json(s: &str) -> BTreeMap<String, String> {
    serde_json::from_str(s).unwrap_or_default()
}

/// One metric sample to persist. `labels` uses the same shape as
/// `tack_orch::MetricSample` (the crate this repo layer must not depend on) —
/// callers (the reconciler) convert field-for-field.
#[derive(Debug, Clone)]
pub struct NewOrchMetric {
    pub name: String,
    pub labels: BTreeMap<String, String>,
    pub value: f64,
}

/// The latest known sample for one `(control_plane_id, name, labels)` triple —
/// what `GET /api/metrics` (task 34.7) merges with Tack's own work-tracking
/// metrics. Deliberately narrower than a full `orch_metrics` row (no `id`,
/// no `created_at`): those are meaningless for a "current value" projection.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrchMetricLatest {
    pub control_plane_id: Uuid,
    pub control_plane_name: String,
    pub name: String,
    pub labels: BTreeMap<String, String>,
    /// `f64::NAN` if the stored value is SQL NULL — SQLite has no native NaN
    /// representation, so a `NaN` sample and a genuine NULL are indistinguishable
    /// once persisted; see migration 025's comment in `migrations.rs`.
    pub value: f64,
    pub scraped_at: DateTime<Utc>,
}

impl Repository {
    /// Batch insert, one transaction for the whole slice. **Not** an upsert in the
    /// usual sense — see the section doc comment above for why a metric sample has
    /// no natural key to conflict on. No-op (no transaction opened) for an empty
    /// slice. All samples in `metrics` share one `scraped_at`/`created_at` (the
    /// moment this function was called), matching the fact that they came from a
    /// single `/metrics` scrape.
    #[instrument(skip(self, metrics), fields(count = metrics.len()))]
    pub async fn upsert_orch_metrics(
        &self,
        control_plane_id: Uuid,
        metrics: &[NewOrchMetric],
    ) -> Result<(), sqlx::Error> {
        if metrics.is_empty() {
            return Ok(());
        }

        let now = Utc::now().to_rfc3339();
        let mut tx = self.pool().begin().await?;

        for m in metrics {
            sqlx::query(
                "INSERT INTO orch_metrics
                    (id, control_plane_id, name, labels, value, scraped_at, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(control_plane_id.to_string())
            .bind(&m.name)
            .bind(labels_to_json(&m.labels))
            .bind(m.value)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        debug!(count = metrics.len(), "orch_metrics inserted");
        Ok(())
    }

    /// The most recent sample for every `(control_plane_id, name, labels)` triple
    /// across every registered plane — what `GET /api/metrics` renders. A
    /// correlated subquery, not a window function, to keep this working on older
    /// SQLite builds; call volume here is "once per human/Prometheus scrape of
    /// Tack's own `/api/metrics`," not a hot path.
    #[instrument(skip(self))]
    pub async fn list_latest_orch_metrics(&self) -> Result<Vec<OrchMetricLatest>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            control_plane_id: String,
            control_plane_name: String,
            name: String,
            labels: String,
            // NULL means the original sample was NaN (see NewOrchMetric's doc
            // comment) — mapped back to f64::NAN below.
            value: Option<f64>,
            scraped_at: String,
        }

        let rows: Vec<Row> = sqlx::query_as(
            "SELECT m.control_plane_id, cp.name AS control_plane_name, m.name, m.labels, \
                    m.value, m.scraped_at \
             FROM orch_metrics m \
             JOIN control_planes cp ON cp.id = m.control_plane_id \
             WHERE m.scraped_at = ( \
                 SELECT MAX(m2.scraped_at) FROM orch_metrics m2 \
                 WHERE m2.control_plane_id = m.control_plane_id \
                   AND m2.name = m.name AND m2.labels = m.labels \
             ) \
             ORDER BY cp.name, m.name",
        )
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| OrchMetricLatest {
                control_plane_id: Uuid::parse_str(&r.control_plane_id).unwrap(),
                control_plane_name: r.control_plane_name,
                name: r.name,
                labels: labels_from_json(&r.labels),
                value: r.value.unwrap_or(f64::NAN),
                scraped_at: parse_rfc3339(&r.scraped_at),
            })
            .collect())
    }
}

// ════════════════════════════════════════════════════════════════════════════════════
// Retention: roll orch_events / orch_metrics into per-day aggregates, then purge
// ════════════════════════════════════════════════════════════════════════════════════

fn day_bucket(rfc3339: &str) -> String {
    // occurred_at/scraped_at are always written via `DateTime<Utc>::to_rfc3339()`
    // (see upsert_orch_events/upsert_orch_metrics above) — "YYYY-MM-DDT...+00:00" —
    // so the first 10 bytes are always the UTC calendar day, ASCII, safe to slice.
    rfc3339.chars().take(10).collect()
}

/// Outcome of one `rollup_and_purge_orch_*` call (which may run several bounded
/// batches internally — see the doc comment on
/// [`Repository::rollup_and_purge_orch_events`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct RollupStats {
    /// Total raw rows folded into a daily aggregate and deleted.
    pub rows_purged: i64,
    /// Number of batch transactions it took (for observability/tuning only).
    pub batches_run: i64,
}

impl Repository {
    /// Rolls every `orch_events` row with `occurred_at < cutoff` into
    /// `orch_events_daily` (grouped by day/control_plane_id/event_type) and deletes
    /// the raw rows — **in the same transaction**, one bounded batch of up to
    /// `batch_size` rows at a time, looping until nothing older than `cutoff`
    /// remains.
    ///
    /// **Why batched, not one transaction for the whole sweep:**
    /// SQLite allows exactly one writer at a
    /// time, so a single transaction spanning a large backlog (e.g. the first
    /// sweep after upgrading a long-running install with years of history) would
    /// hold that lock for its entire duration, blocking every other write in the
    /// process (item moves, the reconciler's own health writes, ...) for as long
    /// as it takes. Bounding each transaction to `batch_size` rows keeps any single
    /// write short, at the cost of the overall sweep taking several round trips —
    /// an entirely acceptable trade for a background job with no user waiting on
    /// it.
    ///
    /// **Why the aggregate write and the delete are in the *same* transaction, not
    /// two separately-committed steps ordered "aggregate first, delete second":**
    /// the requirement is that a crash between them loses nothing. Two independently
    /// committed steps cannot fully satisfy that: a crash after the aggregate
    /// commits but before the delete commits leaves the raw rows still present,
    /// and a naive retry would re-aggregate and double-count them. Putting both in
    /// one transaction removes the in-between state entirely — either neither
    /// effect is visible (a retry re-reads the same untouched raw rows: safe, just
    /// redone) or both are (the raw rows are gone, so a retry can never see them
    /// again to double-count). That is strictly stronger than "ordered so a crash
    /// loses nothing": it makes the lossy window impossible rather than merely
    /// bounding what could be lost in it.
    #[instrument(skip(self))]
    pub async fn rollup_and_purge_orch_events(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<RollupStats, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Candidate {
            id: String,
            control_plane_id: String,
            event_type: String,
            occurred_at: String,
        }

        let cutoff_str = cutoff.to_rfc3339();
        let mut stats = RollupStats::default();

        loop {
            let mut tx = self.pool().begin().await?;

            let candidates: Vec<Candidate> = sqlx::query_as(
                "SELECT id, control_plane_id, event_type, occurred_at FROM orch_events \
                 WHERE occurred_at < ? ORDER BY occurred_at ASC LIMIT ?",
            )
            .bind(&cutoff_str)
            .bind(batch_size)
            .fetch_all(&mut *tx)
            .await?;

            if candidates.is_empty() {
                tx.commit().await?;
                break;
            }

            let batch_len = candidates.len();
            let mut groups: BTreeMap<(String, String, String), i64> = BTreeMap::new();
            let mut ids = Vec::with_capacity(batch_len);
            for c in &candidates {
                let key = (
                    day_bucket(&c.occurred_at),
                    c.control_plane_id.clone(),
                    c.event_type.clone(),
                );
                *groups.entry(key).or_insert(0) += 1;
                ids.push(c.id.clone());
            }

            let now = Utc::now().to_rfc3339();
            for ((day, control_plane_id, event_type), count) in &groups {
                sqlx::query(
                    "INSERT INTO orch_events_daily
                        (id, day, control_plane_id, event_type, event_count, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(day, control_plane_id, event_type) DO UPDATE SET
                        event_count = event_count + excluded.event_count,
                        updated_at = excluded.updated_at",
                )
                .bind(Uuid::new_v4().to_string())
                .bind(day)
                .bind(control_plane_id)
                .bind(event_type)
                .bind(*count)
                .bind(&now)
                .bind(&now)
                .execute(&mut *tx)
                .await?;
            }

            // The delete happens after the aggregate write above, in the same
            // transaction — see the doc comment for why that ordering-within-one-
            // commit is the load-bearing property, not just statement order.
            let placeholders = vec!["?"; ids.len()].join(",");
            let sql = format!("DELETE FROM orch_events WHERE id IN ({placeholders})");
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
            for id in &ids {
                q = q.bind(id);
            }
            q.execute(&mut *tx).await?;

            tx.commit().await?;

            stats.rows_purged += batch_len as i64;
            stats.batches_run += 1;

            if (batch_len as i64) < batch_size {
                break;
            }
        }

        debug!(
            rows_purged = stats.rows_purged,
            batches = stats.batches_run,
            "orch_events retention sweep complete"
        );
        Ok(stats)
    }

    /// Same contract as [`Self::rollup_and_purge_orch_events`] (batched, atomic
    /// aggregate-then-delete per batch), for `orch_metrics` / `orch_metrics_daily`.
    /// Non-finite sample values (`NaN`/`Inf`, which the Prometheus parser
    /// deliberately preserves rather than dropping — see
    /// `adapters::prometheus::parse_value`) are still counted in
    /// `sample_count` but excluded from `value_sum`/`value_min`/`value_max`, so one
    /// `NaN` sample can't poison an entire day's aggregate.
    #[instrument(skip(self))]
    pub async fn rollup_and_purge_orch_metrics(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<RollupStats, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Candidate {
            id: String,
            control_plane_id: String,
            name: String,
            labels: String,
            // NULL means the original sample was NaN — see NewOrchMetric's doc
            // comment. Treated identically to a non-finite value below: counted
            // in sample_count, excluded from sum/min/max.
            value: Option<f64>,
            scraped_at: String,
        }

        #[derive(Default, Clone, Copy)]
        struct Agg {
            count: i64,
            sum: f64,
            min: Option<f64>,
            max: Option<f64>,
        }

        let cutoff_str = cutoff.to_rfc3339();
        let mut stats = RollupStats::default();

        loop {
            let mut tx = self.pool().begin().await?;

            let candidates: Vec<Candidate> = sqlx::query_as(
                "SELECT id, control_plane_id, name, labels, value, scraped_at FROM orch_metrics \
                 WHERE scraped_at < ? ORDER BY scraped_at ASC LIMIT ?",
            )
            .bind(&cutoff_str)
            .bind(batch_size)
            .fetch_all(&mut *tx)
            .await?;

            if candidates.is_empty() {
                tx.commit().await?;
                break;
            }

            let batch_len = candidates.len();
            let mut groups: BTreeMap<(String, String, String, String), Agg> = BTreeMap::new();
            let mut ids = Vec::with_capacity(batch_len);
            for c in &candidates {
                let key = (
                    day_bucket(&c.scraped_at),
                    c.control_plane_id.clone(),
                    c.name.clone(),
                    c.labels.clone(),
                );
                let agg = groups.entry(key).or_default();
                agg.count += 1;
                if let Some(v) = c.value
                    && v.is_finite()
                {
                    agg.sum += v;
                    agg.min = Some(agg.min.map_or(v, |m| m.min(v)));
                    agg.max = Some(agg.max.map_or(v, |m| m.max(v)));
                }
                ids.push(c.id.clone());
            }

            let now = Utc::now().to_rfc3339();
            for ((day, control_plane_id, name, labels), agg) in &groups {
                sqlx::query(
                    "INSERT INTO orch_metrics_daily
                        (id, day, control_plane_id, metric_name, labels, sample_count,
                         value_sum, value_min, value_max, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(day, control_plane_id, metric_name, labels) DO UPDATE SET
                        sample_count = sample_count + excluded.sample_count,
                        value_sum = value_sum + excluded.value_sum,
                        value_min = CASE
                            WHEN excluded.value_min IS NULL THEN value_min
                            WHEN value_min IS NULL THEN excluded.value_min
                            ELSE min(value_min, excluded.value_min)
                        END,
                        value_max = CASE
                            WHEN excluded.value_max IS NULL THEN value_max
                            WHEN value_max IS NULL THEN excluded.value_max
                            ELSE max(value_max, excluded.value_max)
                        END,
                        updated_at = excluded.updated_at",
                )
                .bind(Uuid::new_v4().to_string())
                .bind(day)
                .bind(control_plane_id)
                .bind(name)
                .bind(labels)
                .bind(agg.count)
                .bind(agg.sum)
                .bind(agg.min)
                .bind(agg.max)
                .bind(&now)
                .bind(&now)
                .execute(&mut *tx)
                .await?;
            }

            // Same atomicity guarantee as rollup_and_purge_orch_events: this
            // delete is part of the same transaction as the aggregate writes
            // above, not a separately-committed follow-up.
            let placeholders = vec!["?"; ids.len()].join(",");
            let sql = format!("DELETE FROM orch_metrics WHERE id IN ({placeholders})");
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql));
            for id in &ids {
                q = q.bind(id);
            }
            q.execute(&mut *tx).await?;

            tx.commit().await?;

            stats.rows_purged += batch_len as i64;
            stats.batches_run += 1;

            if (batch_len as i64) < batch_size {
                break;
            }
        }

        debug!(
            rows_purged = stats.rows_purged,
            batches = stats.batches_run,
            "orch_metrics retention sweep complete"
        );
        Ok(stats)
    }
}

/// One day's rolled-up event-type count for one control plane.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrchEventDailyAggregate {
    pub day: String,
    pub control_plane_id: Uuid,
    pub event_type: String,
    pub event_count: i64,
}

/// One day's rolled-up metric aggregate for one control plane.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrchMetricDailyAggregate {
    pub day: String,
    pub control_plane_id: Uuid,
    pub metric_name: String,
    pub labels: BTreeMap<String, String>,
    pub sample_count: i64,
    pub value_sum: f64,
    pub value_min: Option<f64>,
    pub value_max: Option<f64>,
}

impl Repository {
    /// A control plane's rolled-up event-type totals, most recent day first. Read
    /// path for the retention sweep's output — exercised directly by tests, and
    /// available to a future unit-economics view.
    #[instrument(skip(self))]
    pub async fn list_orch_events_daily(
        &self,
        control_plane_id: Uuid,
    ) -> Result<Vec<OrchEventDailyAggregate>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            day: String,
            control_plane_id: String,
            event_type: String,
            event_count: i64,
        }

        let rows: Vec<Row> = sqlx::query_as(
            "SELECT day, control_plane_id, event_type, event_count FROM orch_events_daily \
             WHERE control_plane_id = ? ORDER BY day DESC, event_type ASC",
        )
        .bind(control_plane_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| OrchEventDailyAggregate {
                day: r.day,
                control_plane_id: Uuid::parse_str(&r.control_plane_id).unwrap(),
                event_type: r.event_type,
                event_count: r.event_count,
            })
            .collect())
    }

    /// A control plane's rolled-up metric aggregates, most recent day first.
    #[instrument(skip(self))]
    pub async fn list_orch_metrics_daily(
        &self,
        control_plane_id: Uuid,
    ) -> Result<Vec<OrchMetricDailyAggregate>, sqlx::Error> {
        #[derive(sqlx::FromRow)]
        struct Row {
            day: String,
            control_plane_id: String,
            metric_name: String,
            labels: String,
            sample_count: i64,
            value_sum: f64,
            value_min: Option<f64>,
            value_max: Option<f64>,
        }

        let rows: Vec<Row> = sqlx::query_as(
            "SELECT day, control_plane_id, metric_name, labels, sample_count, value_sum, \
                    value_min, value_max \
             FROM orch_metrics_daily WHERE control_plane_id = ? \
             ORDER BY day DESC, metric_name ASC",
        )
        .bind(control_plane_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| OrchMetricDailyAggregate {
                day: r.day,
                control_plane_id: Uuid::parse_str(&r.control_plane_id).unwrap(),
                metric_name: r.metric_name,
                labels: labels_from_json(&r.labels),
                sample_count: r.sample_count,
                value_sum: r.value_sum,
                value_min: r.value_min,
                value_max: r.value_max,
            })
            .collect())
    }
}

// ════════════════════════════════════════════════════════════════════════════════════
// orch_trace_cursors (migration 028 — trace ingestion)
// ════════════════════════════════════════════════════════════════════════════════════
//
// Resume state for `GET /traces/{project}?since=` (docket P22-3), keyed by
// `(control_plane_id, remote_project)` — deliberately its own table rather than a
// column on `orch_links` (whose PK is the *Tack* project id, not this pair; see
// migration 028's comment). The stored value is docket's own compound cursor token
// verbatim (`"<ts>Z:<n>"`, or a bare timestamp/empty string) — this layer treats it as
// an opaque string, never parses or validates it. See
// `crates/tack-orch/src/reconciler.rs`'s module doc for the cursor's real semantics
// and why losing/rewinding it must never duplicate ingested rows (that guarantee comes
// from `orch_events.id` being content-derived, not from this table).

/// One project's trace-poll resume state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TraceCursor {
    pub control_plane_id: Uuid,
    pub remote_project: String,
    pub cursor: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct TraceCursorRow {
    control_plane_id: String,
    remote_project: String,
    cursor: String,
    updated_at: String,
}

impl TraceCursorRow {
    fn into_trace_cursor(self) -> TraceCursor {
        TraceCursor {
            control_plane_id: Uuid::parse_str(&self.control_plane_id).unwrap(),
            remote_project: self.remote_project,
            cursor: self.cursor,
            updated_at: parse_rfc3339(&self.updated_at),
        }
    }
}

impl Repository {
    /// Create-or-replace the resume cursor for one `(control_plane_id,
    /// remote_project)` pair (`ON CONFLICT` on the composite PK).
    #[instrument(skip(self))]
    pub async fn set_trace_cursor(
        &self,
        control_plane_id: Uuid,
        remote_project: &str,
        cursor: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO orch_trace_cursors
                (control_plane_id, remote_project, cursor, updated_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(control_plane_id, remote_project) DO UPDATE SET
                cursor = excluded.cursor,
                updated_at = excluded.updated_at",
        )
        .bind(control_plane_id.to_string())
        .bind(remote_project)
        .bind(cursor)
        .bind(&now)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Every stored cursor for a control plane's linked projects — what the
    /// reconciler resolves per project before calling `/traces?since=` each tick. A
    /// project with no row yet (never polled) simply doesn't appear; callers treat
    /// that as "start from the beginning", not an error.
    #[instrument(skip(self))]
    pub async fn list_trace_cursors(
        &self,
        control_plane_id: Uuid,
    ) -> Result<Vec<TraceCursor>, sqlx::Error> {
        let rows: Vec<TraceCursorRow> = sqlx::query_as(
            "SELECT control_plane_id, remote_project, cursor, updated_at \
             FROM orch_trace_cursors WHERE control_plane_id = ? ORDER BY remote_project",
        )
        .bind(control_plane_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_trace_cursor()).collect())
    }
}

// ════════════════════════════════════════════════════════════════════════════════════
// Legacy Docket bridge: dual-dispatch prevention and stale-row reconciliation
// ════════════════════════════════════════════════════════════════════════════════════
//
// Two problems, both scoped to
// this file even though the first one reads a table this file has never touched before:
//
// 1. **Dual dispatch.** `execution_requests`/`execution_attempts` (migrations 044/045,
//    the neutral runner-v1 domain) and `orch_tasks` (migration 021, this file, the
//    legacy Docket bridge) are two fully independent write paths that can both target
//    the same `item_id` with no coordination whatsoever. [`Repository::
//    has_active_execution_request_for_item`] is a **read-only** query against the
//    neutral domain's table so `tack-api::dispatcher::dispatch_item` can enforce "one
//    scheduling owner": if the item already has a live runner-v1 request, legacy Docket
//    dispatch defers rather than racing it. This is the only direction this file's
//    ownership (`existing orch_*, Docket adapter/reconciler`) can enforce — the mirror
//    guard on the `execution_requests` creation path belongs to `handlers/executions.rs`,
//    and is a request rather than added here. No schema
//    changes; this reads a table `execution_requests` already exposes with no
//    modification to `migrations.rs`.
//
// 2. **Stale rows.** Nothing has ever updated `orch_tasks.remote_status` /
//    `orch_approvals.state` after the initial dispatch/poll except a fresh poll of a
//    *reachable* plane (`dispatcher.rs`'s write, `reconciler.rs`'s `persist_approvals`).
//    A plane that goes `unreachable` (already tracked by `control_planes.health`/
//    `last_seen_at`) and never recovers leaves any row that was "active" at
//    that moment (`pending`/`running`/`waiting_approval` on `orch_tasks`, `pending` on
//    `orch_approvals`) active forever — which also permanently blocks legacy
//    redispatch, since `dispatcher.rs::is_active_task_status` treats those exact values
//    as "still in flight." [`Repository::reconcile_stale_orch_tasks`] and
//    [`Repository::reconcile_stale_orch_approvals`] are **local-only** sweeps: no HTTP
//    call to docket, so they cannot perturb `docket_tick_contract_test.rs`'s pinned
//    per-tick request sequence. **Not wired to any scheduled task** —
//    mirrors `spawn_retention_sweep`'s own "not yet spawned" gap.

impl Repository {
    /// `true` iff `item_id` has an `execution_requests` row whose `state` is not one
    /// of the three terminal strings runner-v1 uses (`succeeded`/`failed`/`cancelled`
    /// — mirrors `tack_orch::execution::types::ExecutionState::is_terminal` exactly,
    /// which is the single authority for that set; this function does not redefine
    /// it, only reuses the same three literals). `queued`/`leased`/`preparing`/
    /// `running`/`waiting_decision`/`lost`/`needs_operator` all count as "active" —
    /// including `lost`/`needs_operator`, because both still require an operator or a
    /// recovery service to resolve before the item is safe to hand to a second
    /// scheduler; neither is a safe redispatch target on its own.
    ///
    /// Read-only against a table this file has never written — see this section's
    /// module doc for why.
    #[instrument(skip(self))]
    pub async fn has_active_execution_request_for_item(
        &self,
        item_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(
            "SELECT EXISTS(
                SELECT 1 FROM execution_requests
                WHERE item_id = ?
                  AND state NOT IN ('succeeded', 'failed', 'cancelled')
             )",
        )
        .bind(item_id.to_string())
        .fetch_one(self.pool())
        .await?;
        Ok(row.0 != 0)
    }

    /// Marks `orch_tasks` rows `remote_status = 'stale'` when all three hold: the row
    /// is currently "active" (`pending`/`running`/`waiting_approval`,
    /// `dispatcher::ACTIVE_TASK_STATUSES`'s exact set), its `dispatched_at` predates
    /// `stale_before`, and the item's project is linked (`orch_links`) to a
    /// `control_planes` row whose `health = 'unreachable'` and whose `last_seen_at`
    /// also predates `stale_before` — a plane that has never once answered `/health`
    /// successfully since before the cutoff, not a plane mid-outage that might still
    /// recover this tick. `'stale'` is deliberately outside
    /// `dispatcher::ACTIVE_TASK_STATUSES`, so a reconciled row immediately becomes
    /// redispatchable (the existing "anything not in the active set is terminal,
    /// redispatch is safe" rule in `dispatcher.rs` already covers it — no separate
    /// unblock logic needed). Returns the number of rows updated.
    ///
    /// No HTTP call — see this section's module doc for why that matters for the
    /// golden tick-contract test.
    #[instrument(skip(self))]
    pub async fn reconcile_stale_orch_tasks(
        &self,
        stale_before: DateTime<Utc>,
    ) -> Result<u64, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let cutoff = stale_before.to_rfc3339();
        let result = sqlx::query(
            "UPDATE orch_tasks
             SET remote_status = 'stale', updated_at = ?
             WHERE remote_status IN ('pending', 'running', 'waiting_approval')
               AND dispatched_at < ?
               AND item_id IN (
                   SELECT i.id FROM items i
                   JOIN orch_links l ON l.project_id = i.project_id
                   JOIN control_planes cp ON cp.id = l.control_plane_id
                   WHERE cp.health = 'unreachable' AND cp.last_seen_at < ?
               )",
        )
        .bind(&now)
        .bind(&cutoff)
        .bind(&cutoff)
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }

    /// Same reconciliation as [`Self::reconcile_stale_orch_tasks`], for
    /// `orch_approvals`: a `pending` approval whose `requested_at` predates
    /// `stale_before`, correlated to a control plane (`orch_approvals.control_plane_id`
    /// directly, no join needed — migration 038's rebuild added the column) that has
    /// been `unreachable` since before the same cutoff, moves to `'expired'` with
    /// `decided_at` set to now. An approval with a `NULL control_plane_id` (possible
    /// post-038-rebuild; see this table's own doc comment) is never touched — there is
    /// no plane to judge unreachable, so "stale" cannot be determined from this signal
    /// alone. Returns the number of rows updated.
    #[instrument(skip(self))]
    pub async fn reconcile_stale_orch_approvals(
        &self,
        stale_before: DateTime<Utc>,
    ) -> Result<u64, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let cutoff = stale_before.to_rfc3339();
        let result = sqlx::query(
            "UPDATE orch_approvals
             SET state = 'expired', decided_at = ?, updated_at = ?
             WHERE state = 'pending'
               AND requested_at < ?
               AND control_plane_id IS NOT NULL
               AND control_plane_id IN (
                   SELECT id FROM control_planes
                   WHERE health = 'unreachable' AND last_seen_at < ?
               )",
        )
        .bind(&now)
        .bind(&now)
        .bind(&cutoff)
        .bind(&cutoff)
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected())
    }
}
