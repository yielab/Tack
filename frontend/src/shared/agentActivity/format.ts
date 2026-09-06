// Pure formatting + interpretation helpers for agent activity — kept
// isolated and unit-tested because two of them enforce hard correctness
// rules rather than mere copy: "never present an estimate as spend" (every
// money figure carries the word "estimated" AND its pricing-snapshot date,
// even when that date is unknown) and "an item with no agent activity shows
// no chip" (enforced by callers treating an absent badge row /
// `isOrchDisabled` as "nothing to show", not handled here, but
// `deriveAgentChipState` is what those callers feed once they've already
// decided there IS a row to render).

import type { AgentChipState } from '../ui/AgentStateChip';

/** Human-relative time, or an explicit "never"/"unknown" rather than a blank
 *  string. Mirrors `frontend/src/features/fleet/format.ts#relativeTime`
 *  (duplicated, not imported — see `./api.ts`'s note on why this module
 *  can't reach into `features/fleet/**`). */
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

/** Compact token count — tokens are the primary measure. */
export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

/**
 * Every money figure carries the literal word "estimated" and an explicit
 * statement of the pricing-snapshot date — including when that date is
 * `null`. This is stricter than the Fleet view's
 * `formatEstimatedCost` (which silently omits the snapshot clause when the
 * date is unknown): `pricing_snapshot_at` is currently always `null`
 * because no pricing mechanism exists yet, so the null case is handled
 * honestly here — never a fake date, and never a silently dropped
 * qualifier. A reader must never come away thinking a snapshot date exists
 * just because the qualifier is missing.
 */
export function formatEstimatedCost(usd: number | null, snapshotAt: string | null): string {
  if (usd == null) return 'cost estimate unavailable';
  const amount = usd.toLocaleString(undefined, { style: 'currency', currency: 'USD' });
  const snapshot = snapshotAt
    ? `pricing as of ${new Date(snapshotAt).toLocaleDateString()}`
    : 'pricing snapshot date unknown';
  return `${amount} estimated (${snapshot})`;
}

/**
 * Collapses the wire's `TaskStatus` (pending | running | done | failed |
 * blocked | waiting_approval | an unrecognised `Unknown(String)` value — see
 * `./api.ts`'s header comment) down to the chip's 5 visually distinct
 * states.
 *
 * Two states fold into `failed` rather than getting a 6th slot of their own,
 * both deliberate, both flagged here for whoever next touches this:
 *
 *  - `blocked` → `failed`. It has no distinct slot of its own among the
 *    five, and semantically "the agent cannot proceed without intervention"
 *    reads as "needs attention" — the same signal `failed` already carries.
 *    A future change that wants `blocked` to read differently (e.g. it's
 *    not actually an error, just paused) should give it its own
 *    `AgentChipState` member and a 6th tone.
 *  - Any unrecognised value → `failed`. This is the conservative direction:
 *    showing an unfamiliar status as "queued" or "done" risks hiding a real
 *    problem, whereas flagging an unknown status as "needs a look" costs
 *    nothing when it turns out to be fine. Mirrors the Rust side's own
 *    philosophy of never erroring on an unknown value, just choosing the
 *    safe rendering.
 *
 * `done` earns the fifth slot (rather than giving `blocked` its own) because
 * a completed dispatch is exactly the kind of at-a-glance signal this chip
 * exists for.
 */
export function deriveAgentChipState(remoteStatus: string): AgentChipState {
  switch (remoteStatus) {
    case 'pending':
      return 'queued';
    case 'running':
      return 'running';
    case 'waiting_approval':
      return 'waiting_approval';
    case 'done':
      return 'done';
    case 'failed':
    case 'blocked':
      return 'failed';
    default:
      return 'failed';
  }
}

/** Known docket event types (roadmap.md Task 34.4's list) mapped to a short
 *  display label. An event type not in this map still renders — the raw
 *  string, title-cased — rather than being dropped; see `eventTypeLabel`.
 *  Always exercised against an empty `events` array today (B2 hasn't
 *  shipped), but built now so B2 landing needs zero frontend changes. */
const EVENT_TYPE_LABEL: Record<string, string> = {
  tool_call: 'Tool call',
  approval_requested: 'Approval requested',
  approval_granted: 'Approval granted',
  approval_denied: 'Approval denied',
  cost_charged: 'Cost charged',
  budget_exceeded: 'Budget exceeded',
  verification_failed: 'Verification failed',
  tester_verdict_failed: 'Tester verdict: failed',
  rework_started: 'Rework started',
  review_rejected: 'Review rejected',
  session_end: 'Session ended',
  status_map_rejected: 'Status change rejected',
};

/**
 * Message for the "history may be incomplete" notice the item-detail tab
 * shows when `ItemAgentActivity.events_truncated` is true.
 *
 * Deliberately vague about *what* was lost. `events_truncated` is a
 * heuristic — true when any attempt was dispatched before the current
 * retention cutoff (`now - events_retention_days`) — not a count of
 * anything actually rolled up: the retention sweep's aggregate
 * (`orch_events_daily`) is keyed by `(day, control_plane_id, event_type)`
 * and drops `item_id` entirely, so there is no query that can say how many
 * of *this item's* events were aged out, or even whether any were (an item
 * dispatched before the cutoff may simply never have had many events to
 * begin with).
 * This message says "may have been aged out," never a number — inventing a
 * count would be less honest than the vague-but-true statement, not more
 * useful. Names the retention window so the caveat is actionable (a reader
 * can go raise `TACK_ORCH_EVENT_RETENTION_DAYS` if they need longer
 * history) rather than just mysterious.
 */
export function eventsTruncatedMessage(retentionDays: number): string {
  return (
    `This item has an attempt older than the ${retentionDays}-day event retention window — ` +
    `some of its event history may have been aged out by the retention sweep and is no longer available.`
  );
}

/** Display label for an `orch_events.event_type` value — a recognised type's
 *  short label, or the raw wire string title-cased so an event type this
 *  build doesn't know about is still shown as-is, never blank. */
export function eventTypeLabel(eventType: string): string {
  const known = EVENT_TYPE_LABEL[eventType];
  if (known) return known;
  return eventType
    .split('_')
    .map((w) => (w ? w[0].toUpperCase() + w.slice(1) : w))
    .join(' ');
}
