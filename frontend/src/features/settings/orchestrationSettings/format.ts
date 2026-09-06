// Pure formatting/interpretation helpers for the Settings → Orchestration
// page — kept isolated and unit-tested like every other feature's
// `format.ts` (`features/fleet/format.ts`, `features/approvals/format.ts`).

import type { ControlPlaneHealth } from './api';
import type { BadgeTone } from '../../../shared/ui';

/** Human-relative elapsed time since `iso`, with no "just now"/"ago" suffix
 *  duplication in call sites. Falls back to "unknown" for an unparseable
 *  timestamp rather than blank/NaN. Duplicated from
 *  `features/approvals/format.ts#elapsedSince`'s algorithm — not imported;
 *  `features/approvals/**` and `features/settings/**` are different
 *  top-level features under `architecture.test.ts`'s cross-feature-import
 *  rule. */
export function elapsedSince(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return 'unknown';
  const diffSec = Math.max(0, Math.round((Date.now() - then) / 1000));
  if (diffSec < 5) return 'just now';
  if (diffSec < 60) return `${diffSec}s`;
  const min = Math.round(diffSec / 60);
  if (min < 60) return `${min}m`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h`;
  const day = Math.round(hr / 24);
  return `${day}d`;
}

export const HEALTH_LABEL: Record<ControlPlaneHealth, string> = {
  healthy: 'Healthy',
  degraded: 'Degraded',
  unreachable: 'Unreachable',
  unknown: 'Not yet connected',
  unconfigured: 'Missing credentials',
};

export const HEALTH_TONE: Record<ControlPlaneHealth, BadgeTone> = {
  healthy: 'success',
  degraded: 'warning',
  unreachable: 'danger',
  unknown: 'neutral',
  // Shares degraded's tone — see `features/fleet/format.ts#HEALTH_TONE`'s
  // identical choice and its doc comment for the reasoning.
  unconfigured: 'warning',
};

/** Copy explaining what "not yet connected" actually means — the state
 *  every freshly-registered control plane starts in until the reconciler's
 *  next poll (the health state machine: healthy → degraded
 *  after 3 consecutive failures → unreachable after 10; recovery is
 *  immediate on one success). There is no synchronous "test connection"
 *  endpoint — this IS the test, playing out over the next poll cycle. */
export function healthExplanation(health: ControlPlaneHealth, pollSecs: number): string {
  switch (health) {
    case 'unknown':
      return `Waiting for Tack's reconciler to poll this plane for the first time — usually within ${pollSecs}s.`;
    case 'healthy':
      return 'Reachable as of the most recent poll.';
    case 'degraded':
      return 'The last few polls failed, but Tack hasn’t given up yet.';
    case 'unreachable':
      return 'Ten or more consecutive polls have failed. Check the base URL and token.';
    case 'unconfigured':
      return 'Tack has no usable credentials for this plane — often a restored backup, ' +
        'whose secrets are cleared on export. Re-enter the token below to resume polling.';
    default:
      return '';
  }
}
