// Pure, framework-agnostic display logic for one attempt's model provenance
// and usage economics. Kept separate from `AttemptList.tsx` for the same
// reason `shared.ts` is separate from `RunWithAgentModal.tsx` — unit
// testable without mounting anything, and a single place every consumer
// (`AttemptList.tsx`, its tests) reads through so "Not measured" can never
// silently drift into "$0.00" at a second call site.
//
// **The load-bearing rule this whole file exists to enforce (CLAUDE.md
// rule 1, "unsupported is typed, unknown is explicit, unmeasured is
// nullable"):** `usage_economics.runner_time_cost.cost_usd_estimated`
// is `{value: null, source: "not_measured"}` in *every real response this
// API returns today* — no runner infra cost-rate is stored anywhere in this
// schema. `formatUsdMeasurement` below renders that literal, unmistakable
// text `"Not measured"` — never `$0.00`, `—`, `0`, or a blank cell, which
// would each be a lie about real money. `absent_usage_never_serializes_as_zero`
// (`crates/tack-orch/src/usage_provenance.rs`) is the backend half of this
// same guarantee; this file is the frontend half.

import type { ModelProvenance, RunnerTimeCost, UsageEconomics } from '../execution/attempts';
import type { Measurement } from '../execution/types';
import type { StateTone } from './shared';

/** The exact literal every unmeasured usage figure must render. Never
 *  interpolated or abbreviated differently at a second call site — every
 *  caller that needs this text imports this constant rather than retyping
 *  the string, so a future edit can never accidentally introduce a second,
 *  slightly different spelling. */
export const NOT_MEASURED_TEXT = 'Not measured';

/**
 * Renders a `Measurement<number>` dollar figure honestly. `source ===
 * 'not_measured'` is checked FIRST and unconditionally returns
 * {@link NOT_MEASURED_TEXT} regardless of `value` (defensive: a
 * `not_measured` source should always carry `value: null` per the backend's
 * own guarantee, but this function does not trust that pairing blindly — a
 * `null` value on its own, whatever the `source` says, is also treated as
 * "not measured" rather than risking a `NaN`/blank render). A real `0` with
 * a `measured`/`estimated` source is a genuine, distinct fact and renders as
 * a real `$0.00...`, never collapsed into the same text as "unmeasured".
 */
export function formatUsdMeasurement(measurement: Measurement<number>): string {
  if (measurement.source === 'not_measured' || measurement.value === null) {
    return NOT_MEASURED_TEXT;
  }
  const decimals = Math.abs(measurement.value) > 0 && Math.abs(measurement.value) < 0.01 ? 4 : 2;
  const dollars = `$${measurement.value.toFixed(decimals)}`;
  const provenanceLabel = measurement.source === 'measured' ? 'measured' : 'estimated';
  return `${dollars} (${provenanceLabel})`;
}

/**
 * `wall_clock_ms` is a plain derivable fact, never itself a `Measurement`
 * (there is no "estimated" wall clock — `usage_provenance.rs`'s own doc
 * comment). `null` means "not yet known" (the attempt hasn't reported both
 * `started_at`/`ended_at`), a genuinely different reason for absence than
 * "not measured" — this function's wording is deliberately distinct from
 * {@link NOT_MEASURED_TEXT} so the two absence reasons are never visually
 * conflated.
 */
export function formatWallClock(wallClockMs: number | null): string {
  if (wallClockMs === null) return 'Unknown — attempt has not finished yet';
  const totalSeconds = Math.round(wallClockMs / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const parts: string[] = [];
  if (hours > 0) parts.push(`${hours}h`);
  if (hours > 0 || minutes > 0) parts.push(`${minutes}m`);
  parts.push(`${seconds}s`);
  return parts.join(' ');
}

export interface RunnerTimeCostDisplay {
  wallClock: string;
  costUsd: string;
}

export function formatRunnerTimeCost(cost: RunnerTimeCost): RunnerTimeCostDisplay {
  return {
    wallClock: formatWallClock(cost.wall_clock_ms),
    costUsd: formatUsdMeasurement(cost.cost_usd_estimated),
  };
}

export interface UsageEconomicsDisplay {
  modelTokenCostUsd: string;
  runnerTime: RunnerTimeCostDisplay;
}

/** Never sums the two dollar dimensions — `UsageEconomics`'s own doc
 *  comment: they are independently provenanced and must stay visibly
 *  separate line items, not folded into one "total cost" that would imply a
 *  precision neither figure actually has. */
export function formatUsageEconomics(usage: UsageEconomics): UsageEconomicsDisplay {
  return {
    modelTokenCostUsd: formatUsdMeasurement(usage.model_token_cost_usd_estimated),
    runnerTime: formatRunnerTimeCost(usage.runner_time_cost),
  };
}

// ─── Model provenance ───────────────────────────────────────────────────────

export interface ModelProvenanceDisplay {
  label: string;
  detail: string;
  tone: StateTone;
}

/**
 * `null` means the attempt has not yet reported `actual_execution` — a
 * "hasn't happened yet" state, deliberately worded differently from
 * {@link NOT_MEASURED_TEXT} ("won't be measured"), since the two are not the
 * same fact: this one may still resolve to a real value once the attempt
 * completes.
 */
export function describeModelProvenance(provenance: ModelProvenance | null): ModelProvenanceDisplay {
  if (provenance === null) {
    return { label: 'Not yet reported', detail: 'This attempt has not reported its actual execution yet.', tone: 'neutral' };
  }
  switch (provenance.kind) {
    case 'matched':
      return {
        label: 'Matched request',
        detail: `Ran on ${provenance.provider} / ${provenance.model_id}, as requested.`,
        tone: 'success',
      };
    case 'auto_select_observed':
      return {
        label: 'Auto-selected',
        detail: `The request allowed auto-selection; the runner chose ${provenance.actual_provider} / ${provenance.actual_model_id}.`,
        tone: 'info',
      };
    case 'mismatched':
      return {
        label: 'Mismatched request',
        detail:
          `Requested ${provenance.requested_provider} / ${provenance.requested_model_id}, ` +
          `but ran on ${provenance.actual_provider} / ${provenance.actual_model_id}.`,
        tone: 'warning',
      };
    /* istanbul ignore next -- defensive: ModelProvenance is a closed union
       today; an unrecognised `kind` still renders instead of throwing. */
    default: {
      const unknown = provenance as { kind: string };
      return { label: 'Unrecognised provenance', detail: `Unknown kind: ${unknown.kind}`, tone: 'neutral' };
    }
  }
}
