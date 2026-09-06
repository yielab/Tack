// Formatting + interpretation helpers for the Orchestration settings panel.
// Kept isolated and unit-tested for the same reason `shared/agentActivity/
// format.ts` is: two of these enforce "never present an
// estimate as spend" rather than mere copy — `formatBudgetCap` and
// `budgetProgress`'s caveat text. Reuses `formatEstimatedCost`/`formatTokens`
// from `shared/agentActivity/format.ts` verbatim rather than writing a
// second cost formatter.

import type { ControlPlaneHealth } from './api';

export type HealthTone = 'success' | 'warning' | 'danger' | 'neutral';

/** Duplicated from `features/fleet/format.ts` rather than imported — see
 *  `./api.ts`'s note on why this module doesn't reach into
 *  `features/fleet/**`. */
export const HEALTH_LABEL: Record<ControlPlaneHealth, string> = {
  healthy: 'Healthy',
  degraded: 'Degraded',
  unreachable: 'Unreachable',
  unknown: 'Unknown',
};

export const HEALTH_TONE: Record<ControlPlaneHealth, HealthTone> = {
  healthy: 'success',
  degraded: 'warning',
  unreachable: 'danger',
  unknown: 'neutral',
};

/** The user-set budget cap on its own — never a derived figure, so it never
 *  carries "estimated". */
export function formatBudgetCap(budgetUsd: number | null): string {
  if (budgetUsd == null) return 'no budget cap set';
  return `${budgetUsd.toLocaleString(undefined, { style: 'currency', currency: 'USD' })} cap`;
}

export interface BudgetProgress {
  /** 0..N (can exceed 1 — over cap). */
  fraction: number;
  tone: 'success' | 'warning' | 'danger';
}

/**
 * `cost / budget`, clamped at 0 but deliberately NOT clamped above 1 (a
 * project can genuinely exceed its cap — that's exactly the state an
 * operator most needs to see, not one this function should hide by capping
 * the fraction at 100%). Returns `null` whenever the fraction wouldn't mean
 * anything: no budget cap set, cap is zero-or-negative, or `costUsd` is
 * `null` (stale/unlinked — see `OrchBudget.cost_usd_estimated`'s doc
 * comment).
 *
 * **This is an estimate of a fraction of an estimate**:
 * `costUsd` is Tack's own token-based estimate, unverified
 * against a real bill, and the fraction compounds that uncertainty with a
 * user-typed cap. Every caller must render `BUDGET_PROGRESS_CAVEAT` beside
 * this value — never the bare percentage alone.
 */
export function budgetProgress(costUsd: number | null, budgetUsd: number | null): BudgetProgress | null {
  if (costUsd == null || budgetUsd == null || budgetUsd <= 0) return null;
  const fraction = Math.max(0, costUsd / budgetUsd);
  const tone: BudgetProgress['tone'] = fraction >= 1 ? 'danger' : fraction >= 0.7 ? 'warning' : 'success';
  return { fraction, tone };
}

export function formatPercent(fraction: number): string {
  return `${(fraction * 100).toLocaleString(undefined, { maximumFractionDigits: 1 })}%`;
}

/** Mandatory companion copy for any rendered `budgetProgress` fraction — see
 *  that function's own doc comment. */
export const BUDGET_PROGRESS_CAVEAT =
  'This is an estimate of a fraction of an estimate: Tack’s own token-based cost model, ' +
  'against a manually set budget cap — not verified spend. Treat it as directional, not exact.';

/** Static, honest copy for the "pause" gap this panel deliberately does not
 *  build a control for. See `api.ts`'s header comment and
 *  `crates/tack-api/src/handlers/orch.rs`'s `OrchBudgetResponse` doc comment
 *  for the full reasoning: no HTTP route exists to detect or clear a
 *  docket budget pause, only the CLI remedy named below. */
export const BUDGET_PAUSE_NOTE =
  'If a pod stops accepting new work after this budget is reached, docket has auto-paused it. ' +
  'Tack cannot detect or clear this over HTTP today — from the docket CLI, run ' +
  '"docket profile <pod-id> --resume" to clear the pause.';

/** Static copy explaining why every number on the Policy panel is
 *  control-plane-wide, not project-specific. See `OrchPolicy`'s doc comment
 *  in `api.ts`. */
export const POLICY_SCOPE_CAVEAT =
  'These figures cover the whole control plane this project is linked to, not just this ' +
  'project’s own agents — docket does not break guardrail/approval/tool-call counters ' +
  'down per project. If more than one project shares this control plane, they all show the same numbers.';

export function formatDenialRate(rate: number | null): string {
  if (rate == null) return 'no tool-call data observed yet';
  return `${formatPercent(rate)} of tool calls denied`;
}

/** Human-relative time, or an explicit "never"/"unknown" rather than a blank
 *  string. Duplicated from `shared/agentActivity/format.ts` rather than
 *  imported for scrape-time display, matching that file's own precedent of
 *  small formatting helpers being fine to duplicate across a feature
 *  boundary (see its own header comment re: `features/fleet/format.ts`). */
export function relativeTime(iso: string | null): string {
  if (!iso) return 'never';
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return 'unknown';
  const diffSec = Math.round((Date.now() - then) / 1000);
  if (diffSec < 5) return 'just now';
  if (diffSec < 60) return `${diffSec}s ago`;
  const min = Math.round(diffSec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.round(hr / 24);
  if (day < 30) return `${day}d ago`;
  return new Date(iso).toLocaleDateString();
}
