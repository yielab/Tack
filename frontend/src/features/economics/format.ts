// Formatting + interpretation helpers for the unit-economics dashboard. Every money
// figure goes through `formatEstimatedCost` from `shared/agentActivity/format.ts` —
// reused verbatim, matching the Fleet view's own precedent for the identical
// field, not reimplemented here.

import { formatEstimatedCost, formatTokens } from '../../shared/agentActivity/format';
import type { LeadTimeStat, ReworkStat } from './api';

export { formatEstimatedCost, formatTokens };

/** Compact hour duration — under a day in hours, otherwise days (one decimal). */
export function formatHours(hours: number): string {
  if (hours < 48) return `${hours.toFixed(1)}h`;
  return `${(hours / 24).toFixed(1)}d`;
}

/** A percentage string, or an explicit "n/a" — never a bare `NaN`/`0%` standing in
 *  for "no data". */
export function formatRate(rate: number | null): string {
  if (rate == null) return 'n/a';
  return `${(rate * 100).toFixed(1)}%`;
}

/**
 * The lead-time summary line for one population ("agent" or "human"), honoring the
 * min-sample rule: below `MIN_SAMPLE_SIZE` this shows the raw durations instead of
 * an average that would overstate precision from a handful of items. Never
 * returns a bare number with no sample-size context.
 */
export function describeLeadTime(stat: LeadTimeStat): string {
  if (stat.sample_count === 0) return 'no completed items yet';
  if (stat.below_min_sample) {
    const raw = (stat.raw_hours ?? []).map(formatHours).join(', ');
    return `too few samples (${stat.sample_count}) for a stable average — raw: ${raw}`;
  }
  return `${formatHours(stat.avg_hours ?? 0)} average (${stat.sample_count} items)`;
}

/**
 * The rework-rate summary line. Mirrors `describeLeadTime`'s min-sample discipline:
 * below the stated minimum this names the raw counts rather than a percentage that
 * would read as more stable than a handful of items supports.
 */
export function describeRework(stat: ReworkStat): string {
  const eligible = stat.attempts_total - stat.attempts_excluded_stale;
  if (eligible === 0) {
    return 'no eligible dispatch history yet';
  }
  if (stat.below_min_sample) {
    return `too few eligible attempts (${eligible}) for a stable rate — ${stat.attempts_with_rework_signal} of ${eligible} showed a rework signal`;
  }
  return `${formatRate(stat.rate)} (${stat.attempts_with_rework_signal} of ${eligible} eligible attempts)`;
}

/**
 * "Estimated cost per shipped item" — the headline number: cost-per-completed-item
 * by product line — withheld below the minimum sample size rather than shown as a
 * precise-looking figure from a handful of items. Always routes through
 * `formatEstimatedCost` so the "estimated" qualifier and pricing-snapshot caveat
 * survive even in this per-item form.
 */
export function describeCostPerItem(
  perItem: number | null,
  pricingSnapshotAt: string | null,
  agentCompletedCount: number,
  minSampleSize: number,
): string {
  if (agentCompletedCount === 0) return 'no agent-dispatched items yet';
  if (perItem == null) {
    return `too few agent-dispatched items (${agentCompletedCount}, minimum ${minSampleSize}) for a stable per-item figure`;
  }
  return `${formatEstimatedCost(perItem, pricingSnapshotAt)} per item`;
}
