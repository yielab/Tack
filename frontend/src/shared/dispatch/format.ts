// Pure interpretation helpers for dispatch outcomes. Isolated and
// unit-tested because "queued" / "policy-blocked" / "waiting-approval" are
// three distinct, never-conflated outcomes — this is the one place that
// distinction is decided, so every call site (item detail, the board card
// menu, the sprint dispatch modal) renders it identically rather than each
// inventing its own wording.

import type { BadgeTone } from '../ui/Badge';
import type { DispatchItemResponse, SprintDispatchItemResponse, SprintDispatchSummary } from './api';

export interface DispatchOutcomeDescription {
  tone: BadgeTone;
  label: string;
}

/**
 * Every known `outcome` (single-item, `DispatchItemResponse`) / `decision`
 * (sprint, `SprintDispatchItemResponse`) value, each with a tone that is
 * visually and semantically distinct. `waiting_approval` is deliberately
 * `warning`, never `success`: rendering waiting-approval as success is a
 * correctness bug, not a wording nit. `already_in_flight` gets its
 * own `info` tone rather than folding into `dispatched` — the task WAS
 * already running before this click, a meaningfully different fact from
 * "this click just started it."
 *
 * The last three exist only on `SprintDispatchItemResponse.decision` —
 * see `./api.ts`'s header comment:
 * `waiting_on_dependencies` (a DAG blocker, not a policy/eligibility gate —
 * `neutral`, the same calm tone as the other "not yet, nothing wrong" states,
 * since most items in a sprint plan legitimately start here), `would_dispatch`
 * (dry-run only — a *prediction*, so it gets its own `info` tone, never the
 * `success` tone reserved for an outcome that actually happened), and `error`
 * (real run only — this item's own dispatch failed or its worker task
 * panicked; `danger`, but a distinct label from `blocked` so an operator can
 * tell "docket refused this on purpose" apart from "something broke").
 */
const OUTCOME: Record<string, DispatchOutcomeDescription> = {
  dispatched: { tone: 'success', label: 'Dispatched' },
  waiting_approval: { tone: 'warning', label: 'Waiting on approval' },
  blocked: { tone: 'danger', label: 'Blocked by policy' },
  already_in_flight: { tone: 'info', label: 'Already running' },
  no_dispatch_policy: { tone: 'neutral', label: 'No dispatch policy configured' },
  not_eligible: { tone: 'neutral', label: 'Not eligible right now' },
  waiting_on_dependencies: { tone: 'neutral', label: 'Waiting on dependencies' },
  would_dispatch: { tone: 'info', label: 'Would dispatch' },
  error: { tone: 'danger', label: 'Dispatch error' },
};

/** Title-cases an unrecognised raw value rather than dropping it — the same
 *  "shown as-is, never a hard failure" discipline the Rust side applies to
 *  every wire enum, extended to this frontend's own outcome
 *  taxonomy in case a future backend version adds another value. */
function titleCase(raw: string): string {
  return raw
    .split('_')
    .map((w) => (w ? w[0].toUpperCase() + w.slice(1) : w))
    .join(' ');
}

export function describeDispatchOutcome(outcome: string): DispatchOutcomeDescription {
  return OUTCOME[outcome] ?? { tone: 'neutral', label: titleCase(outcome) };
}

/**
 * The "why" behind a single-item dispatch outcome — the specific thing that
 * makes "dispatch failed" useless and 'blocked by guardrail policy
 * "prompt-injection"' useful. Returns `null`
 * when there's nothing more informative to say than the outcome label itself
 * already conveys.
 */
export function dispatchOutcomeDetail(res: DispatchItemResponse): string | null {
  if (res.outcome === 'blocked') {
    const policy = res.policy_id ? `guardrail policy "${res.policy_id}"` : 'a guardrail policy';
    return res.message ? `${policy}: ${res.message}` : policy;
  }
  if (res.outcome === 'not_eligible') {
    const from = res.dispatch_from?.length ? res.dispatch_from.join(', ') : 'a dispatchable status';
    return res.current_status ? `item is in "${res.current_status}" — needs to be in ${from}` : null;
  }
  if (res.status_map_rejected) {
    return `board status unchanged — the workflow rejected the automatic move: ${res.status_map_rejected}`;
  }
  if (res.status_applied) {
    return `board moved to "${res.status_applied}"`;
  }
  return null;
}

/**
 * The sprint-item equivalent of `dispatchOutcomeDetail` — a separate
 * function, not an overload, because `SprintDispatchItemResponse` names its
 * decision field `decision` (not `outcome`) and adds two cases
 * `DispatchItemResponse` has no equivalent for: `waiting_on_dependencies`
 * (names how many direct dependencies are still open — item ids only, the
 * API doesn't resolve titles, so this deliberately reports a count rather
 * than fabricating names) and `error` (real run only — docket's own error
 * text, or "the dispatch task itself failed," verbatim).
 */
export function sprintItemDetail(item: SprintDispatchItemResponse): string | null {
  if (item.decision === 'blocked') {
    const policy = item.policy_id ? `guardrail policy "${item.policy_id}"` : 'a guardrail policy';
    return item.message ? `${policy}: ${item.message}` : policy;
  }
  if (item.decision === 'error') {
    return item.error ? `dispatch error: ${item.error}` : 'dispatch error';
  }
  if (item.decision === 'waiting_on_dependencies') {
    const n = item.blocked_by?.length ?? 0;
    return n > 0 ? `waiting on ${n} direct ${n === 1 ? 'dependency' : 'dependencies'} to finish` : null;
  }
  if (item.decision === 'not_eligible') {
    const from = item.dispatch_from?.length ? item.dispatch_from.join(', ') : 'a dispatchable status';
    return item.current_status ? `item is in "${item.current_status}" — needs to be in ${from}` : null;
  }
  if (item.status_map_rejected) {
    return `board status unchanged — the workflow rejected the automatic move: ${item.status_map_rejected}`;
  }
  if (item.status_applied) {
    return `board moved to "${item.status_applied}"`;
  }
  return null;
}

/** Every key of `SprintDispatchSummary` that names a per-item decision
 *  bucket, in a sensible reading order — `total` excluded (an aggregate, not
 *  a decision itself). `errored` is the summary's own field name for the
 *  per-item `"error"` decision (plural noun vs. the item-level value's verb
 *  form — a naming quirk of the generated schema, reconciled here so
 *  `describeDispatchOutcome` still gets the right key). */
const SUMMARY_ORDER: { key: keyof SprintDispatchSummary; decision: string }[] = [
  { key: 'dispatched', decision: 'dispatched' },
  { key: 'waiting_approval', decision: 'waiting_approval' },
  { key: 'would_dispatch', decision: 'would_dispatch' },
  { key: 'blocked', decision: 'blocked' },
  { key: 'errored', decision: 'error' },
  { key: 'already_in_flight', decision: 'already_in_flight' },
  { key: 'waiting_on_dependencies', decision: 'waiting_on_dependencies' },
  { key: 'not_eligible', decision: 'not_eligible' },
  { key: 'no_dispatch_policy', decision: 'no_dispatch_policy' },
];

/**
 * Turns the server's own pre-computed `SprintDispatchSummary` into a display
 * list — never re-derived by counting `items` client-side (C3 built the
 * summary specifically so nobody has to, and re-deriving it independently
 * would risk the two silently drifting apart). Zero-count buckets are
 * omitted so the summary row only shows what's actually present. Every
 * bucket stays separate — never merged into a single total — so a sentence
 * like "8 dispatched" can never be shown when three of those are actually
 * waiting on a human or a dependency.
 */
export function summarizeSprintDispatchCounts(
  summary: SprintDispatchSummary,
): { decision: string; count: number }[] {
  return SUMMARY_ORDER.map(({ key, decision }) => ({ decision, count: summary[key] })).filter(
    (entry) => entry.count > 0,
  );
}
