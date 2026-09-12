//! Unit economics: tokens, estimated cost, agent-vs-human lead time, and
//! rework rate, sliced by `project_type` and `item_type`. Read-only
//! aggregate endpoints over `Repository::
//! list_completed_item_economics`/`list_item_ids_with_rework_signal`
//! (`tack_db::repo::economics`) — no live docket call, so a plane outage can
//! never turn into a 500 here, the same discipline `handlers::orch`'s
//!
//! Design notes: docs/dev-notes/tack-api/handlers/economics.md

use axum::Json;
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use uuid::Uuid;

use tack_db::repo::economics::ItemEconomicsRow;

use crate::error::ApiResult;
use crate::router::AppState;

// ════════════════════════════════════════════════════════════════════════════
// Constants — the honesty decisions, in one place so the UI copy and the wire
// data can never drift apart (every string below is also asserted verbatim in
// this module's tests).
// ════════════════════════════════════════════════════════════════════════════

/// Below this many samples, a slice reports raw counts/durations instead of a
/// derived average or rate. Stated as a chosen minimum, not a statistically
/// derived optimum: 5 is small enough that a real board reaches it quickly,
/// and large enough that a single outlier item can't masquerade as a trend —
/// avoiding a "3x faster from 2 items" mistake.
pub const MIN_SAMPLE_SIZE: i64 = 5;

/// Rendered next to every agent-vs-human lead-time comparison, not buried in a doc.
pub const LEAD_TIME_SELECTION_BIAS_NOTE: &str = "Items dispatched to agents are not a random sample of all work — auto-dispatch \
     fires only on specific statuses, and people choose what to hand off. This is not \
     a controlled comparison, and no single \"agents are Nx faster\" figure is \
     computed from it.";

/// The exact rework definition this module computes, rendered verbatim in the UI so
/// the number and the words describing it can never drift apart.
pub const REWORK_RATE_DEFINITION: &str = "Share of dispatched items (completed, with at least one docket dispatch) that \
     have at least one rework_started, verification_failed, or tester_verdict_failed \
     event recorded against them.";

/// Rendered next to the rework-rate figure whenever any attempt was excluded as
/// stale. Never states a count of lost events — only a query-time cutoff comparison.
pub const REWORK_TRUNCATION_NOTE: &str = "Rework signals come from mirrored docket events, which age out after the \
     configured retention window and are rolled into a daily total that no longer \
     names the item. Items whose only dispatch attempt predates that window are \
     excluded from this rate rather than counted as \"no rework\" — their event \
     history may already be gone.";

// ════════════════════════════════════════════════════════════════════════════
// Response DTOs
// ════════════════════════════════════════════════════════════════════════════

/// Average-or-raw duration figure. `avg_hours` and `raw_hours` are mutually
/// exclusive: exactly one is populated (or, at `sample_count == 0`, neither).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct LeadTimeStat {
    pub sample_count: i64,
    pub below_min_sample: bool,
    /// `None` whenever `below_min_sample` is true (including `sample_count == 0`) —
    /// see `raw_hours`.
    pub avg_hours: Option<f64>,
    /// Populated only when `below_min_sample` is true and `sample_count > 0`: the
    /// individual durations, so a small sample is shown honestly rather than averaged
    /// into a number that looks more precise than it is.
    pub raw_hours: Option<Vec<f64>>,
}

impl LeadTimeStat {
    fn from_hours(mut hours: Vec<f64>) -> Self {
        let sample_count = hours.len() as i64;
        if sample_count == 0 {
            return Self {
                sample_count: 0,
                below_min_sample: true,
                avg_hours: None,
                raw_hours: None,
            };
        }
        if sample_count < MIN_SAMPLE_SIZE {
            hours.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            return Self {
                sample_count,
                below_min_sample: true,
                avg_hours: None,
                raw_hours: Some(hours),
            };
        }
        let avg = hours.iter().sum::<f64>() / sample_count as f64;
        Self {
            sample_count,
            below_min_sample: false,
            avg_hours: Some(avg),
            raw_hours: None,
        }
    }
}

/// Rework-rate figure for one slice, plus the exact definition and truncation
/// caveat that produced it.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ReworkStat {
    /// Every dispatched item in scope, including ones excluded from the rate below.
    pub attempts_total: i64,
    /// Excluded because their only dispatch predates the retention cutoff — their
    /// event history may already be gone (see `REWORK_TRUNCATION_NOTE`).
    pub attempts_excluded_stale: i64,
    /// Of the *eligible* (`attempts_total - attempts_excluded_stale`) items, how many
    /// carry at least one qualifying event.
    pub attempts_with_rework_signal: i64,
    pub below_min_sample: bool,
    /// `None` when the eligible sample is `0` or below `MIN_SAMPLE_SIZE`.
    pub rate: Option<f64>,
    pub definition: String,
    pub truncation_note: String,
}

impl ReworkStat {
    fn compute(total: i64, excluded_stale: i64, with_signal: i64) -> Self {
        let eligible = total - excluded_stale;
        let below_min_sample = eligible < MIN_SAMPLE_SIZE;
        let rate = if eligible > 0 && !below_min_sample {
            Some(with_signal as f64 / eligible as f64)
        } else {
            None
        };
        Self {
            attempts_total: total,
            attempts_excluded_stale: excluded_stale,
            attempts_with_rework_signal: with_signal,
            below_min_sample,
            rate,
            definition: REWORK_RATE_DEFINITION.to_string(),
            truncation_note: REWORK_TRUNCATION_NOTE.to_string(),
        }
    }
}

/// One row of the summary: "overall", one `project_type`, or one `item_type`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EconomicsSlice {
    /// `"overall"`, a `project_type` value, or an `item_type` value — see the
    /// containing response's `by_project_type`/`by_item_type` field it came from.
    pub key: String,
    pub completed_item_count: i64,
    /// Completed items with at least one `orch_tasks` row (dispatched to an agent at
    /// least once — regardless of who ultimately finished it).
    pub agent_completed_count: i64,
    /// Completed items with zero `orch_tasks` rows — never dispatched.
    pub human_completed_count: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    /// Summed `orch_tasks.cost_usd_estimated` for this slice's agent-dispatched
    /// items. `None` only when `agent_completed_count == 0` (nothing to sum);
    /// `Some(0.0)` means agent items exist but none report a cost yet.
    pub cost_usd_estimated: Option<f64>,
    /// Always `None` today — no pricing-snapshot mechanism exists yet.
    pub pricing_snapshot_at: Option<String>,
    /// `cost_usd_estimated / agent_completed_count`. `None` whenever
    /// `agent_completed_count < MIN_SAMPLE_SIZE` — the headline "cost per shipped
    /// item" figure is exactly the kind of small-sample-noise ratio this module
    /// guards against elsewhere, so it is withheld below the stated minimum
    /// rather than shown from a handful of items.
    pub cost_usd_estimated_per_item: Option<f64>,
    pub agent_lead_time: LeadTimeStat,
    pub human_lead_time: LeadTimeStat,
    pub lead_time_selection_bias_note: String,
    pub rework: ReworkStat,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EconomicsSummaryResponse {
    pub generated_at: DateTime<Utc>,
    pub min_sample_size: i64,
    pub events_retention_days: u32,
    pub overall: EconomicsSlice,
    pub by_project_type: Vec<EconomicsSlice>,
    pub by_item_type: Vec<EconomicsSlice>,
}

/// Which population a `GET /api/economics/items` row belongs to (see the module doc's
/// definition: "dispatched at least once" vs. "never dispatched").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EconomicsPopulation {
    Agent,
    Human,
}

/// One completed item's economics — the row shape behind both the dashboard's
/// drill-down list and the CSV/JSON export (task 38.4).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EconomicsItemResponse {
    pub item_id: Uuid,
    pub project_id: Uuid,
    pub project_type: String,
    pub item_type: String,
    pub title: String,
    pub status: String,
    pub population: EconomicsPopulation,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub first_dispatched_at: Option<DateTime<Utc>>,
    pub attempt_count: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub cost_usd_estimated: Option<f64>,
    pub pricing_snapshot_at: Option<String>,
    /// `dispatched_at → completed_at` for an agent item, `started_at → completed_at`
    /// for a human item; `None` if the required timestamp is missing or the computed
    /// duration is negative (a data anomaly, e.g. a redispatch after completion —
    /// excluded rather than shown as a nonsensical negative duration).
    pub lead_time_hours: Option<f64>,
    /// Only meaningful when `population == Agent`; always `false` for a human item.
    pub rework_applicable: bool,
    /// Whether this item's rework-signal data is trustworthy — `false` when its only
    /// dispatch predates the retention cutoff (see the module doc).
    pub rework_data_reliable: bool,
    /// Raw signal presence; only trust this when `rework_applicable &&
    /// rework_data_reliable` both hold.
    pub rework_signal: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EconomicsItemsResponse {
    pub rows: Vec<EconomicsItemResponse>,
    /// Total matching rows before `limit`/`offset` — never truncated silently (see
    /// the query docs below).
    pub total: i64,
}

// ════════════════════════════════════════════════════════════════════════════
// Pure aggregation — no I/O, exercised directly by this module's unit tests with
// hand-built `ItemEconomicsRow`s rather than only through a DB-backed integration
// test (the min-sample/staleness branches are easiest to prove exhaustively here).
// ════════════════════════════════════════════════════════════════════════════

struct SliceBuilder {
    completed_item_count: i64,
    agent_completed_count: i64,
    human_completed_count: i64,
    tokens_in: i64,
    tokens_out: i64,
    cost_sum: f64,
    agent_lead_hours: Vec<f64>,
    human_lead_hours: Vec<f64>,
    rework_total: i64,
    rework_excluded_stale: i64,
    rework_with_signal: i64,
}

impl SliceBuilder {
    fn new() -> Self {
        Self {
            completed_item_count: 0,
            agent_completed_count: 0,
            human_completed_count: 0,
            tokens_in: 0,
            tokens_out: 0,
            cost_sum: 0.0,
            agent_lead_hours: Vec::new(),
            human_lead_hours: Vec::new(),
            rework_total: 0,
            rework_excluded_stale: 0,
            rework_with_signal: 0,
        }
    }

    fn add(
        &mut self,
        item: &ItemEconomicsRow,
        has_rework_signal: bool,
        retention_cutoff: DateTime<Utc>,
    ) {
        self.completed_item_count += 1;
        let is_agent = item.attempt_count > 0;

        if is_agent {
            self.agent_completed_count += 1;
            self.tokens_in += item.tokens_in;
            self.tokens_out += item.tokens_out;
            self.cost_sum += item.cost_usd_estimated.unwrap_or(0.0);

            if let (Some(dispatched), Some(completed)) =
                (item.first_dispatched_at, item.completed_at)
                && let Some(hours) = positive_hours(dispatched, completed)
            {
                self.agent_lead_hours.push(hours);
            }

            self.rework_total += 1;
            let reliable = item
                .last_dispatched_at
                .is_some_and(|d| d >= retention_cutoff);
            if reliable {
                if has_rework_signal {
                    self.rework_with_signal += 1;
                }
            } else {
                self.rework_excluded_stale += 1;
            }
        } else {
            self.human_completed_count += 1;
            if let (Some(started), Some(completed)) = (item.started_at, item.completed_at)
                && let Some(hours) = positive_hours(started, completed)
            {
                self.human_lead_hours.push(hours);
            }
        }
    }

    fn finish(self, key: String) -> EconomicsSlice {
        let cost_usd_estimated = if self.agent_completed_count > 0 {
            Some(self.cost_sum)
        } else {
            None
        };
        let cost_usd_estimated_per_item = match (cost_usd_estimated, self.agent_completed_count) {
            (Some(sum), n) if n >= MIN_SAMPLE_SIZE => Some(sum / n as f64),
            _ => None,
        };

        EconomicsSlice {
            key,
            completed_item_count: self.completed_item_count,
            agent_completed_count: self.agent_completed_count,
            human_completed_count: self.human_completed_count,
            tokens_in: self.tokens_in,
            tokens_out: self.tokens_out,
            cost_usd_estimated,
            pricing_snapshot_at: None,
            cost_usd_estimated_per_item,
            agent_lead_time: LeadTimeStat::from_hours(self.agent_lead_hours),
            human_lead_time: LeadTimeStat::from_hours(self.human_lead_hours),
            lead_time_selection_bias_note: LEAD_TIME_SELECTION_BIAS_NOTE.to_string(),
            rework: ReworkStat::compute(
                self.rework_total,
                self.rework_excluded_stale,
                self.rework_with_signal,
            ),
        }
    }
}

/// Hours between two timestamps, or `None` if the duration is zero-or-negative — a
/// data anomaly (e.g. a redispatch recorded after completion) rather than a real
/// lead time, excluded instead of shown as nonsensical.
fn positive_hours(from: DateTime<Utc>, to: DateTime<Utc>) -> Option<f64> {
    let seconds = (to - from).num_seconds();
    if seconds <= 0 {
        return None;
    }
    Some(seconds as f64 / 3600.0)
}

fn build_summary(
    rows: &[ItemEconomicsRow],
    rework_signal_items: &std::collections::HashSet<Uuid>,
    retention_cutoff: DateTime<Utc>,
    retention_days: u32,
) -> EconomicsSummaryResponse {
    let mut overall = SliceBuilder::new();
    let mut by_project_type: std::collections::BTreeMap<String, SliceBuilder> =
        std::collections::BTreeMap::new();
    let mut by_item_type: std::collections::BTreeMap<String, SliceBuilder> =
        std::collections::BTreeMap::new();

    for row in rows {
        let has_signal = rework_signal_items.contains(&row.item_id);
        overall.add(row, has_signal, retention_cutoff);
        by_project_type
            .entry(row.project_type.clone())
            .or_insert_with(SliceBuilder::new)
            .add(row, has_signal, retention_cutoff);
        by_item_type
            .entry(row.item_type.clone())
            .or_insert_with(SliceBuilder::new)
            .add(row, has_signal, retention_cutoff);
    }

    EconomicsSummaryResponse {
        generated_at: Utc::now(),
        min_sample_size: MIN_SAMPLE_SIZE,
        events_retention_days: retention_days,
        overall: overall.finish("overall".to_string()),
        by_project_type: by_project_type
            .into_iter()
            .map(|(k, b)| b.finish(k))
            .collect(),
        by_item_type: by_item_type.into_iter().map(|(k, b)| b.finish(k)).collect(),
    }
}

fn to_item_response(
    row: &ItemEconomicsRow,
    rework_signal_items: &std::collections::HashSet<Uuid>,
    retention_cutoff: DateTime<Utc>,
) -> EconomicsItemResponse {
    let is_agent = row.attempt_count > 0;
    let population = if is_agent {
        EconomicsPopulation::Agent
    } else {
        EconomicsPopulation::Human
    };

    let lead_time_hours = if is_agent {
        row.first_dispatched_at
            .zip(row.completed_at)
            .and_then(|(from, to)| positive_hours(from, to))
    } else {
        row.started_at
            .zip(row.completed_at)
            .and_then(|(from, to)| positive_hours(from, to))
    };

    let rework_data_reliable = is_agent
        && row
            .last_dispatched_at
            .is_some_and(|d| d >= retention_cutoff);
    let rework_signal = is_agent && rework_signal_items.contains(&row.item_id);

    EconomicsItemResponse {
        item_id: row.item_id,
        project_id: row.project_id,
        project_type: row.project_type.clone(),
        item_type: row.item_type.clone(),
        title: row.title.clone(),
        status: row.status.clone(),
        population,
        started_at: row.started_at,
        completed_at: row.completed_at,
        first_dispatched_at: row.first_dispatched_at,
        attempt_count: row.attempt_count,
        tokens_in: row.tokens_in,
        tokens_out: row.tokens_out,
        cost_usd_estimated: row.cost_usd_estimated,
        pricing_snapshot_at: None,
        lead_time_hours,
        rework_applicable: is_agent,
        rework_data_reliable,
        rework_signal,
    }
}

fn build_csv(
    rows: &[&ItemEconomicsRow],
    rework_signal_items: &std::collections::HashSet<Uuid>,
    retention_cutoff: DateTime<Utc>,
) -> String {
    let mut out = String::from(
        "item_id,project_id,project_type,item_type,title,status,population,started_at,\
         completed_at,first_dispatched_at,attempt_count,tokens_in,tokens_out,\
         cost_usd_estimated,lead_time_hours,rework_applicable,rework_data_reliable,\
         rework_signal\n",
    );
    for row in rows {
        let item = to_item_response(row, rework_signal_items, retention_cutoff);
        out.push_str(&format!(
            "{},{},{},{},{},{},{:?},{},{},{},{},{},{},{},{},{},{},{}\n",
            item.item_id,
            item.project_id,
            item.project_type,
            item.item_type,
            item.title.replace(',', " "),
            item.status,
            item.population,
            item.started_at.map(|d| d.to_rfc3339()).unwrap_or_default(),
            item.completed_at
                .map(|d| d.to_rfc3339())
                .unwrap_or_default(),
            item.first_dispatched_at
                .map(|d| d.to_rfc3339())
                .unwrap_or_default(),
            item.attempt_count,
            item.tokens_in,
            item.tokens_out,
            item.cost_usd_estimated
                .map(|c| c.to_string())
                .unwrap_or_default(),
            item.lead_time_hours
                .map(|h| h.to_string())
                .unwrap_or_default(),
            item.rework_applicable,
            item.rework_data_reliable,
            item.rework_signal,
        ));
    }
    out
}

// ════════════════════════════════════════════════════════════════════════════
// Handlers
// ════════════════════════════════════════════════════════════════════════════

/// `GET /api/economics/summary`.
#[utoipa::path(
    get,
    path = "/api/economics/summary",
    tag = "orchestration",
    responses(
        (status = 200, description = "Unit economics (tokens, estimated cost, agent-vs-human lead time, rework rate) sliced by project_type and item_type", body = EconomicsSummaryResponse),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn get_economics_summary(
    State(state): State<AppState>,
) -> ApiResult<Json<EconomicsSummaryResponse>> {
    let rows = state.repo.list_completed_item_economics().await?;
    let rework_signal_items = state.repo.list_item_ids_with_rework_signal().await?;
    let retention_days = state.config.orch_event_retention_days;
    let retention_cutoff = Utc::now() - Duration::days(retention_days as i64);

    Ok(Json(build_summary(
        &rows,
        &rework_signal_items,
        retention_cutoff,
        retention_days,
    )))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EconomicsItemsQuery {
    /// Filter to one `project_type` (e.g. `"software"`). Omit for all.
    pub project_type: Option<String>,
    /// Filter to one `item_type` (e.g. `"bug"`). Omit for all.
    pub item_type: Option<String>,
    /// `"json"` (default, paginated) or `"csv"` (an attachment; ignores
    /// `limit`/`offset`, capped at `EXPORT_MAX_ROWS`).
    #[serde(default = "default_items_format")]
    pub format: String,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

fn default_items_format() -> String {
    "json".to_string()
}

const ITEMS_DEFAULT_LIMIT: u32 = 200;
const ITEMS_MAX_LIMIT: u32 = 2000;
/// CSV export ignores pagination and returns up to this many rows — a safety cap,
/// not real streaming (task 38.4 asked for CSV/JSON export "reusing the existing
/// export machinery in export.rs", which itself loads a whole project into memory the
/// same way; this matches that same "good enough for one operator's own instance"
/// scope rather than building a new streaming path).
const EXPORT_MAX_ROWS: usize = 20_000;

/// `GET /api/economics/items` — per-completed-item economics (task 38.1's raw
/// population) plus CSV/JSON export (task 38.4), reusing `export.rs`'s
/// `?format=` + `Content-Disposition: attachment` convention rather than a second
/// export route.
#[utoipa::path(
    get,
    path = "/api/economics/items",
    tag = "orchestration",
    params(EconomicsItemsQuery),
    responses(
        (status = 200, description = "Per-completed-item economics (JSON, paginated) or a CSV export attachment, per the format query", body = EconomicsItemsResponse),
        (status = 404, description = "Orchestration disabled (TACK_ORCH_ENABLE unset)"),
    ),
)]
#[instrument(skip(state))]
pub async fn get_economics_items(
    State(state): State<AppState>,
    Query(query): Query<EconomicsItemsQuery>,
) -> ApiResult<Response> {
    let rows = state.repo.list_completed_item_economics().await?;
    let rework_signal_items = state.repo.list_item_ids_with_rework_signal().await?;
    let retention_days = state.config.orch_event_retention_days;
    let retention_cutoff = Utc::now() - Duration::days(retention_days as i64);

    let mut filtered: Vec<&ItemEconomicsRow> = rows
        .iter()
        .filter(|r| {
            query
                .project_type
                .as_deref()
                .map(|pt| pt == r.project_type)
                .unwrap_or(true)
        })
        .filter(|r| {
            query
                .item_type
                .as_deref()
                .map(|it| it == r.item_type)
                .unwrap_or(true)
        })
        .collect();
    filtered.sort_by_key(|r| std::cmp::Reverse(r.completed_at));

    if query.format == "csv" {
        let capped_len = filtered.len().min(EXPORT_MAX_ROWS);
        let csv = build_csv(
            &filtered[..capped_len],
            &rework_signal_items,
            retention_cutoff,
        );
        return Ok((
            [
                (header::CONTENT_TYPE, "text/csv"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"unit-economics.csv\"",
                ),
            ],
            csv,
        )
            .into_response());
    }

    let total = filtered.len() as i64;
    let limit = query
        .limit
        .unwrap_or(ITEMS_DEFAULT_LIMIT)
        .min(ITEMS_MAX_LIMIT) as usize;
    let offset = query.offset.unwrap_or(0) as usize;
    let page: Vec<EconomicsItemResponse> = filtered
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|r| to_item_response(r, &rework_signal_items, retention_cutoff))
        .collect();

    Ok(Json(EconomicsItemsResponse { rows: page, total }).into_response())
}

/// Mounted with one line into `router.rs`'s `orch_routes()`
/// (`.merge(economics::economics_routes())`) so it inherits `require_orch_enabled`
/// and the ordinary Bearer-token gate without a second layer here.
pub fn economics_routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route(
            "/economics/summary",
            axum::routing::get(get_economics_summary),
        )
        .route("/economics/items", axum::routing::get(get_economics_items))
}

#[cfg(test)]
#[path = "economics/tests.rs"]
mod tests;
