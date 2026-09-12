//! Unit economics — read-only aggregate queries over
//! `items` + `orch_tasks` (+ `orch_events` for the rework signal). Deliberately its own
//! module rather than an extension of `repo/orch.rs`: every query here is
//! additive/read-only against tables `repo/orch.rs` already owns, so a separate file
//! avoids colliding with unrelated edits to that file.
//!
//!
//! Design notes: docs/dev-notes/tack-db/repo/economics.md

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tracing::instrument;
use uuid::Uuid;

use super::Repository;

fn parse_rfc3339(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

/// One completed item (`items.completed_at IS NOT NULL`), joined to its project's
/// `project_type` and aggregated against every `orch_tasks` row dispatched for it (a
/// `LEFT JOIN`, so a never-dispatched item still gets a row — with `attempt_count = 0`
/// and every `orch_tasks`-derived field `None`/zero — rather than being silently
/// excluded, which is what makes the agent-vs-human population split possible
/// downstream).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemEconomicsRow {
    pub item_id: Uuid,
    pub project_id: Uuid,
    pub project_type: String,
    pub item_type: String,
    pub title: String,
    pub status: String,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    /// Count of `orch_tasks` rows for this item — 0 for an item never dispatched.
    pub attempt_count: i64,
    /// Earliest `orch_tasks.dispatched_at` for this item — the anchor for "agent lead
    /// time" (`first_dispatched_at → completed_at`). `None` iff `attempt_count == 0`.
    pub first_dispatched_at: Option<DateTime<Utc>>,
    /// Latest `orch_tasks.dispatched_at` — used only to judge whether this item's
    /// rework-signal correlation is trustworthy (see the module doc above), never
    /// shown as a metric on its own.
    pub last_dispatched_at: Option<DateTime<Utc>>,
    pub tokens_in: i64,
    pub tokens_out: i64,
    /// `SUM(orch_tasks.cost_usd_estimated)` — `None` when `attempt_count == 0`
    /// (nothing to sum, distinct from a confident `0.0`).
    pub cost_usd_estimated: Option<f64>,
}

#[derive(sqlx::FromRow)]
struct ItemEconomicsSqlRow {
    item_id: String,
    project_id: String,
    project_type: String,
    item_type: String,
    title: String,
    status: String,
    started_at: Option<String>,
    completed_at: Option<String>,
    attempt_count: i64,
    first_dispatched_at: Option<String>,
    last_dispatched_at: Option<String>,
    tokens_in: i64,
    tokens_out: i64,
    cost_usd_estimated: Option<f64>,
}

impl ItemEconomicsSqlRow {
    fn into_row(self) -> ItemEconomicsRow {
        ItemEconomicsRow {
            item_id: Uuid::parse_str(&self.item_id).unwrap(),
            project_id: Uuid::parse_str(&self.project_id).unwrap(),
            project_type: self.project_type,
            item_type: self.item_type,
            title: self.title,
            status: self.status,
            started_at: self.started_at.as_deref().map(parse_rfc3339),
            completed_at: self.completed_at.as_deref().map(parse_rfc3339),
            attempt_count: self.attempt_count,
            first_dispatched_at: self.first_dispatched_at.as_deref().map(parse_rfc3339),
            last_dispatched_at: self.last_dispatched_at.as_deref().map(parse_rfc3339),
            tokens_in: self.tokens_in,
            tokens_out: self.tokens_out,
            cost_usd_estimated: self.cost_usd_estimated,
        }
    }
}

impl Repository {
    /// Every completed item (`completed_at IS NOT NULL`), with its project's
    /// `project_type` and its `orch_tasks` totals folded in via one `LEFT JOIN` +
    /// `GROUP BY` — a single query rather than an N+1 per item. Unpaginated by
    /// design (like `list_items_for_sprint`): a unit-economics dashboard that
    /// silently truncated at some page size would misreport the very totals it exists
    /// to get right. See `idx_items_completed_at` (migration 031) — without it this is
    /// a full scan of `items` on an instance with many projects, since the query has
    /// no `project_id` predicate to use any of the existing `(project_id, ...)`
    /// composite indexes.
    #[instrument(skip(self))]
    pub async fn list_completed_item_economics(
        &self,
    ) -> Result<Vec<ItemEconomicsRow>, sqlx::Error> {
        let rows: Vec<ItemEconomicsSqlRow> = sqlx::query_as(
            "SELECT
                i.id AS item_id,
                i.project_id AS project_id,
                p.project_type AS project_type,
                i.item_type AS item_type,
                i.title AS title,
                i.status AS status,
                i.started_at AS started_at,
                i.completed_at AS completed_at,
                COUNT(t.remote_task_id) AS attempt_count,
                MIN(t.dispatched_at) AS first_dispatched_at,
                MAX(t.dispatched_at) AS last_dispatched_at,
                COALESCE(SUM(t.tokens_in), 0) AS tokens_in,
                COALESCE(SUM(t.tokens_out), 0) AS tokens_out,
                SUM(t.cost_usd_estimated) AS cost_usd_estimated
             FROM items i
             JOIN projects p ON p.id = i.project_id
             LEFT JOIN orch_tasks t ON t.item_id = i.id
             WHERE i.completed_at IS NOT NULL
             GROUP BY i.id, i.project_id, p.project_type, i.item_type, i.title, \
                      i.status, i.started_at, i.completed_at",
        )
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_row()).collect())
    }

    /// Distinct `item_id`s that have at least one `orch_events` row of type
    /// `rework_started`, `verification_failed`, or `tester_verdict_failed` — this
    /// module's rework-signal definition. Item-level, not
    /// attempt-level — see this module's doc comment on why `run_id` correlation
    /// isn't usable today. Only reflects events still in the raw table: an item whose
    /// only qualifying event aged past `TACK_ORCH_EVENT_RETENTION_DAYS` and was rolled
    /// into `orch_events_daily` (which drops `item_id`) will not appear here even
    /// though rework genuinely happened — callers must cross-check
    /// `ItemEconomicsRow::last_dispatched_at` against their own retention cutoff
    /// before treating absence from this set as "confirmed no rework."
    #[instrument(skip(self))]
    pub async fn list_item_ids_with_rework_signal(&self) -> Result<HashSet<Uuid>, sqlx::Error> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT item_id FROM orch_events \
             WHERE item_id IS NOT NULL \
               AND event_type IN ('rework_started', 'verification_failed', 'tester_verdict_failed')",
        )
        .fetch_all(self.pool())
        .await?;

        Ok(ids
            .into_iter()
            .filter_map(|s| Uuid::parse_str(&s).ok())
            .collect())
    }
}
