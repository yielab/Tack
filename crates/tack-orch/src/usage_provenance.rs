//! Requested-vs-actual model provenance and honest, provenance-separated
//! usage economics.
//!
//! Two independent pure concerns, neither performing I/O:
//!
//! - [`compare_model_provenance`]: the request's resolved model (or "no
//!   model requested", i.e. auto-select) against the attempt's
//!   `ActualExecution` observation — visible, never silently reconciled.
//! - [`build_usage_economics`]: keeps runner-observed wall-clock time cost
//!   structurally separate from the harness/vendor's own self-reported
//!   token/dollar usage — never summed into one opaque number.
//!
//! Every dollar-valued field here is named `*_usd_estimated`, matching this
//! crate's own module-level convention (`crate::lib`'s "Money is always an
//! estimate"). Absent usage is `Measurement { value: None, source:
//! NotMeasured, .. }`, never a fabricated `0`/`0.0` — see this module's
//! `absent_usage_never_serializes_as_zero` test, which asserts the literal
//! JSON shape rather than trusting the type alone.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::execution::{
    ActualExecution, ActualModelId, ActualModelProvider, Measurement, MeasurementSource,
    RequestedModelId, RequestedModelProvider, Usage,
};

/// The comparison between what an execution request asked for and what an
/// attempt actually ran on. All three variants carry the full observed
/// facts — never coalesced into a bare boolean "matched" flag, so a caller
/// (the frontend's attempt rendering, in particular) can show *both* sides
/// of a mismatch rather than just "something changed."
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelProvenance {
    /// The attempt ran on exactly the requested provider/model.
    Matched { provider: String, model_id: String },
    /// The request allowed auto-selection (no explicit provider/model was
    /// ever resolved for it) and the attempt observed a concrete choice.
    /// Distinct from [`Self::Matched`] — nothing was requested to match
    /// against — and distinct from [`Self::Mismatched`] — nothing was
    /// contradicted, since nothing specific was asked for.
    AutoSelectObserved {
        actual_provider: String,
        actual_model_id: String,
    },
    /// The attempt ran on a provider and/or model different from what was
    /// explicitly requested. Both sides are carried in full.
    Mismatched {
        requested_provider: String,
        requested_model_id: String,
        actual_provider: String,
        actual_model_id: String,
    },
}

/// Compares a resolved request (`None` for auto-select — the request's
/// nullable-pair shape) against an attempt's actually-observed model.
/// Compares via `.as_str()`, never by unwrapping into a shared type — the
/// *requested* namespace ([`RequestedModelProvider`]/[`RequestedModelId`])
/// and the *actual* namespace ([`ActualModelProvider`]/[`ActualModelId`])
/// stay textually distinct types all the way through, exactly as
/// `crate::scheduler::select::evaluate_candidate` already does for
/// requested-vs-declared.
pub fn compare_model_provenance(
    requested: Option<(&RequestedModelProvider, &RequestedModelId)>,
    actual_provider: &ActualModelProvider,
    actual_model_id: &ActualModelId,
) -> ModelProvenance {
    match requested {
        None => ModelProvenance::AutoSelectObserved {
            actual_provider: actual_provider.as_str().to_string(),
            actual_model_id: actual_model_id.as_str().to_string(),
        },
        Some((provider, model_id))
            if provider.as_str() == actual_provider.as_str()
                && model_id.as_str() == actual_model_id.as_str() =>
        {
            ModelProvenance::Matched {
                provider: actual_provider.as_str().to_string(),
                model_id: actual_model_id.as_str().to_string(),
            }
        }
        Some((provider, model_id)) => ModelProvenance::Mismatched {
            requested_provider: provider.as_str().to_string(),
            requested_model_id: model_id.as_str().to_string(),
            actual_provider: actual_provider.as_str().to_string(),
            actual_model_id: actual_model_id.as_str().to_string(),
        },
    }
}

fn not_measured() -> Measurement<f64> {
    Measurement {
        value: None,
        source: MeasurementSource::NotMeasured,
        additional: Default::default(),
    }
}

/// Runner-observed wall-clock time cost — a dimension with entirely
/// different provenance from the harness's own self-reported [`Usage`]:
///
/// - `wall_clock_ms` is a fact the runner/API directly witnesses (attempt
///   `started_at`/`ended_at`, `execution_attempts` columns, migration 045)
///   — always derivable once both are known, never itself wrapped in a
///   [`Measurement`] (there is no "estimated" wall clock; it either is or
///   is not known yet).
/// - `cost_usd_estimated` stays `not_measured` unless a caller supplies an
///   infra rate. **No such rate is stored anywhere in this schema today**,
///   so `runner_rate_usd_per_hour` is always caller-supplied, never
///   invented by this module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunnerTimeCost {
    pub wall_clock_ms: Option<u64>,
    pub cost_usd_estimated: Measurement<f64>,
}

/// Computes [`RunnerTimeCost`] from an attempt's observed start/end times
/// and an optional infra rate. Never fabricates a value: missing timestamps
/// leave `wall_clock_ms: None`; a missing rate leaves `cost_usd_estimated`
/// `not_measured` — it is never assumed to be `0.0`.
pub fn compute_runner_time_cost(
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    runner_rate_usd_per_hour: Option<f64>,
) -> RunnerTimeCost {
    let wall_clock_ms = match (started_at, ended_at) {
        (Some(start), Some(end)) => {
            let millis = end.signed_duration_since(start).num_milliseconds();
            // Clamp rather than panic/underflow on a clock that somehow
            // reports `ended_at` before `started_at` — that is a data
            // anomaly for whoever ingests these facts to investigate, not
            // grounds for this pure function to produce nonsense or crash.
            Some(u64::try_from(millis).unwrap_or(0))
        }
        _ => None,
    };
    let cost_usd_estimated = match (wall_clock_ms, runner_rate_usd_per_hour) {
        (Some(ms), Some(rate)) => Measurement {
            value: Some((ms as f64 / 3_600_000.0) * rate),
            source: MeasurementSource::Estimated,
            additional: Default::default(),
        },
        _ => not_measured(),
    };
    RunnerTimeCost {
        wall_clock_ms,
        cost_usd_estimated,
    }
}

/// Two independently-provenanced dollar dimensions, deliberately never
/// summed into one figure — see this module's doc comment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageEconomics {
    /// Pass-through of `Usage.cost_usd` — the harness/vendor's own
    /// self-reported dollar figure, provenance (`measured` / `estimated` /
    /// `not_measured`) preserved verbatim, never reinterpreted.
    pub model_token_cost_usd_estimated: Measurement<f64>,
    /// This module's own derived dimension — see [`RunnerTimeCost`].
    pub runner_time_cost: RunnerTimeCost,
}

/// Builds [`UsageEconomics`] from a completion's optional [`Usage`] report
/// and an attempt's observed start/end times. `usage: None` (the attempt
/// has not completed, or the harness reported none at all) yields
/// `model_token_cost_usd_estimated`'s `not_measured` — never a fabricated
/// zero.
pub fn build_usage_economics(
    usage: Option<&Usage>,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    runner_rate_usd_per_hour: Option<f64>,
) -> UsageEconomics {
    let model_token_cost_usd_estimated = usage
        .map(|usage| usage.cost_usd.clone())
        .unwrap_or_else(not_measured);
    UsageEconomics {
        model_token_cost_usd_estimated,
        runner_time_cost: compute_runner_time_cost(started_at, ended_at, runner_rate_usd_per_hour),
    }
}

/// Every derived fact this module produces for one attempt, in one call —
/// a repository/service-handler convenience. Takes the
/// same raw column shapes `tack_db::repo::execution::AttemptListingRow`
/// already carries (`actual_execution`/`usage` as raw JSON text, possibly
/// absent) plus the request's resolved requested provider/model, so a
/// caller (`tack-api`'s executions handler) can pass real row data straight
/// through without this module depending on `tack-db`'s row type directly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptFacts {
    /// `None` only when the attempt has not yet reported `actual_execution`
    /// (still in flight) — distinct from a comparison result, which always
    /// has a matched/auto/mismatched answer once actual data exists.
    pub model_provenance: Option<ModelProvenance>,
    pub usage_economics: UsageEconomics,
}

/// Parses raw `execution_attempts` columns and produces [`AttemptFacts`].
/// Malformed JSON in `actual_execution_json`/`usage_json` (should not
/// happen — both are written by this codebase's own completion handler —
/// but a raw `TEXT` column has no schema enforcement) is treated the same
/// as "not yet reported," never a panic.
#[allow(clippy::too_many_arguments)]
pub fn derive_attempt_facts(
    requested_provider: Option<&str>,
    requested_model_id: Option<&str>,
    actual_execution_json: Option<&str>,
    usage_json: Option<&str>,
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    runner_rate_usd_per_hour: Option<f64>,
) -> AttemptFacts {
    let actual: Option<ActualExecution> =
        actual_execution_json.and_then(|raw| serde_json::from_str(raw).ok());
    let usage: Option<Usage> = usage_json.and_then(|raw| serde_json::from_str(raw).ok());

    let model_provenance = actual.as_ref().map(|actual| {
        let requested = match (requested_provider, requested_model_id) {
            (Some(provider), Some(model_id)) => Some((
                RequestedModelProvider::new(provider),
                RequestedModelId::new(model_id),
            )),
            _ => None,
        };
        compare_model_provenance(
            requested
                .as_ref()
                .map(|(provider, model_id)| (provider, model_id)),
            &actual.model_provider,
            &actual.model_id,
        )
    });

    AttemptFacts {
        model_provenance,
        usage_economics: build_usage_economics(
            usage.as_ref(),
            started_at,
            ended_at,
            runner_rate_usd_per_hour,
        ),
    }
}

#[cfg(test)]
#[path = "usage_provenance/tests.rs"]
mod tests;
