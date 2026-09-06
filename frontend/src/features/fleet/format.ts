// Pure formatting helpers for the Fleet view — kept isolated and unit-tested
// because two of them enforce hard correctness rules rather
// than mere copy: "never present an estimate as spend" — every money
// figure carries the word "estimated" — and "never a
// confident-looking zero" for unreachable planes.

import type { ControlPlaneHealth } from './api';

/** Human-relative time, or an explicit "never"/"unknown" rather than a blank
 *  string — callers must not have to guess why nothing rendered. */
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

/** Compact token count — tokens are the primary measure and
 *  this must read at least as prominently as the dollar figure next to it. */
export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

/**
 * Every money figure carries the literal word "estimated" and, when the
 * plane reports one, its pricing-snapshot date. This is a correctness
 * requirement: docket's cost numbers are estimates from
 * labelled pricing tables, not billed spend, and must never be presented as
 * if they were.
 */
export function formatEstimatedCost(usd: number | null, snapshotAt: string | null): string {
  if (usd == null) return 'cost estimate unavailable';
  const amount = usd.toLocaleString(undefined, { style: 'currency', currency: 'USD' });
  const snapshot = snapshotAt ? ` (pricing as of ${new Date(snapshotAt).toLocaleDateString()})` : '';
  return `${amount} estimated${snapshot}`;
}

/** `null` budget means "no cap set" — distinct from a `0` cap (paused). */
export function formatBudget(budgetUsd: number | null): string {
  if (budgetUsd == null) return 'no budget cap set';
  return budgetUsd.toLocaleString(undefined, { style: 'currency', currency: 'USD' });
}

/** A plane's data is trustworthy-as-current only when `healthy`. `degraded`
 *  data is recent but may be a poll or two stale; `unreachable`/`unknown`/
 *  `unconfigured` data must never be shown as if it were a live reading —
 *  see `FleetRow.tsx`'s stale-field treatment. `unconfigured` is at least as
 *  stale as `unknown`: the reconciler never even attempted a poll, so there
 *  is no cached reading of any age to fall back on. */
export function isStale(health: ControlPlaneHealth): boolean {
  return health === 'unreachable' || health === 'unknown' || health === 'unconfigured';
}

export const HEALTH_LABEL: Record<ControlPlaneHealth, string> = {
  healthy: 'Healthy',
  degraded: 'Degraded',
  unreachable: 'Unreachable',
  unknown: 'Unknown',
  unconfigured: 'Missing credentials',
};

export const HEALTH_TONE: Record<ControlPlaneHealth, 'success' | 'warning' | 'danger' | 'neutral'> = {
  healthy: 'success',
  degraded: 'warning',
  unreachable: 'danger',
  unknown: 'neutral',
  // Reuses `degraded`'s tone rather than adding a fifth distinct color: an
  // `unconfigured` plane is a config problem an operator can fix in one
  // step (re-enter the token/secret), not an active outage — the same
  // "needs attention, not yet broken" urgency class as `degraded`. The
  // label text (not color) is what distinguishes them, per this file's own
  // WCAG 1.4.1 discipline (see `HealthChip.tsx`'s doc comment).
  unconfigured: 'warning',
};
