import { describe, it, expect } from 'vitest';
import {
  describeDispatchOutcome,
  dispatchOutcomeDetail,
  sprintItemDetail,
  summarizeSprintDispatchCounts,
} from './format';
import type { DispatchItemResponse, SprintDispatchItemResponse, SprintDispatchSummary } from './api';

const BASE: DispatchItemResponse = {
  outcome: 'dispatched',
  task: null,
  approval_token: null,
  current_status: null,
  dispatch_from: null,
  message: null,
  policy_id: null,
  status_applied: null,
  status_map_rejected: null,
};

const BASE_ITEM: SprintDispatchItemResponse = {
  item_id: 'i1',
  title: 'Item 1',
  status: 'Ready',
  order: 0,
  decision: 'would_dispatch',
  blocked_by: null,
  policy_id: null,
  message: null,
  status_applied: null,
  status_map_rejected: null,
  approval_token: null,
  current_status: null,
  dispatch_from: null,
  error: null,
  task: null,
};

describe('describeDispatchOutcome', () => {
  it('gives each of the nine known values its own tone+label pair — never the same pair for dispatched and waiting_approval', () => {
    const dispatched = describeDispatchOutcome('dispatched');
    const waiting = describeDispatchOutcome('waiting_approval');
    const blocked = describeDispatchOutcome('blocked');
    const wouldDispatch = describeDispatchOutcome('would_dispatch');

    // The headline correctness rule this whole card exists to enforce:
    // waiting-approval must never read as a plain success, and a dry-run
    // PREDICTION ("would_dispatch") must never read as an outcome that
    // actually happened ("dispatched").
    expect(waiting.tone).not.toBe('success');
    expect(waiting.tone).not.toBe(dispatched.tone);
    expect(blocked.tone).not.toBe(dispatched.tone);
    expect(blocked.tone).not.toBe(waiting.tone);
    expect(wouldDispatch.tone).not.toBe('success');
    expect(wouldDispatch.label).not.toBe(dispatched.label);

    expect(dispatched).toEqual({ tone: 'success', label: 'Dispatched' });
    expect(waiting.label.toLowerCase()).toContain('approval');
    expect(blocked.label.toLowerCase()).toContain('block');
  });

  it('every one of the nine documented values is visually distinct by tone+label pair', () => {
    const known = [
      'dispatched',
      'waiting_approval',
      'blocked',
      'already_in_flight',
      'no_dispatch_policy',
      'not_eligible',
      'waiting_on_dependencies',
      'would_dispatch',
      'error',
    ];
    const pairs = known.map((o) => JSON.stringify(describeDispatchOutcome(o)));
    expect(new Set(pairs).size).toBe(known.length);
  });

  it('"error" (sprint-only: this item\'s own dispatch failed) is distinct from "blocked" (docket refused it on purpose)', () => {
    const error = describeDispatchOutcome('error');
    const blocked = describeDispatchOutcome('blocked');
    expect(error.label).not.toBe(blocked.label);
  });

  it('degrades an unrecognised value to a visible, title-cased label rather than throwing', () => {
    const desc = describeDispatchOutcome('some_future_value');
    expect(desc.tone).toBe('neutral');
    expect(desc.label).toBe('Some Future Value');
  });
});

describe('dispatchOutcomeDetail (single-item DispatchItemResponse)', () => {
  it('names the specific guardrail policy for a blocked outcome — "dispatch failed" is not enough', () => {
    const res: DispatchItemResponse = {
      ...BASE,
      outcome: 'blocked',
      policy_id: 'prompt-injection',
      message: 'destructive shell command in task description',
    };
    const detail = dispatchOutcomeDetail(res);
    expect(detail).toContain('prompt-injection');
    expect(detail).toContain('destructive shell command');
  });

  it('explains a not_eligible outcome by current status and the required dispatch_from set', () => {
    const res: DispatchItemResponse = {
      ...BASE,
      outcome: 'not_eligible',
      current_status: 'Done',
      dispatch_from: ['Ready', 'Backlog'],
    };
    const detail = dispatchOutcomeDetail(res);
    expect(detail).toContain('Done');
    expect(detail).toContain('Ready');
  });

  it('surfaces a status_map_rejected reason even when the outcome itself is "dispatched"', () => {
    const res: DispatchItemResponse = {
      ...BASE,
      outcome: 'dispatched',
      status_map_rejected: 'invalid transition: Permit -> Handover',
    };
    expect(dispatchOutcomeDetail(res)).toContain('invalid transition');
  });

  it('names the applied status when one was actually applied', () => {
    const res: DispatchItemResponse = { ...BASE, outcome: 'dispatched', status_applied: 'In Progress' };
    expect(dispatchOutcomeDetail(res)).toContain('In Progress');
  });

  it('returns null when there is nothing more to say than the outcome label', () => {
    expect(dispatchOutcomeDetail(BASE)).toBeNull();
  });
});

describe('sprintItemDetail (SprintDispatchItemResponse — card C3\'s real decision vocabulary)', () => {
  it('names the guardrail policy for a blocked decision', () => {
    const item: SprintDispatchItemResponse = { ...BASE_ITEM, decision: 'blocked', policy_id: 'block-cmd', message: 'rm -rf' };
    expect(sprintItemDetail(item)).toContain('block-cmd');
  });

  it('reports the dispatch error text for an "error" decision (real run only)', () => {
    const item: SprintDispatchItemResponse = { ...BASE_ITEM, decision: 'error', error: 'connection reset' };
    expect(sprintItemDetail(item)).toContain('connection reset');
  });

  it('counts direct dependencies for waiting_on_dependencies rather than fabricating names — the API only gives ids', () => {
    const item: SprintDispatchItemResponse = {
      ...BASE_ITEM,
      decision: 'waiting_on_dependencies',
      blocked_by: ['11111111-1111-1111-1111-111111111111', '22222222-2222-2222-2222-222222222222'],
    };
    const detail = sprintItemDetail(item)!;
    expect(detail).toContain('2');
    expect(detail).not.toContain('1111');
  });

  it('explains a not_eligible decision by current status and dispatch_from, same as the single-item case', () => {
    const item: SprintDispatchItemResponse = {
      ...BASE_ITEM,
      decision: 'not_eligible',
      current_status: 'Done',
      dispatch_from: ['Ready'],
    };
    expect(sprintItemDetail(item)).toContain('Done');
  });

  it('returns null for a plain would_dispatch prediction with nothing more to explain', () => {
    expect(sprintItemDetail(BASE_ITEM)).toBeNull();
  });
});

describe('summarizeSprintDispatchCounts', () => {
  const SUMMARY: SprintDispatchSummary = {
    total: 4,
    dispatched: 2,
    waiting_approval: 1,
    blocked: 1,
    already_in_flight: 0,
    waiting_on_dependencies: 0,
    not_eligible: 0,
    no_dispatch_policy: 0,
    would_dispatch: 0,
    errored: 0,
  };

  it('reads the server-provided counts directly — never re-derives them, and keeps every decision in its own bucket', () => {
    const counts = summarizeSprintDispatchCounts(SUMMARY);
    expect(counts).toEqual([
      { decision: 'dispatched', count: 2 },
      { decision: 'waiting_approval', count: 1 },
      { decision: 'blocked', count: 1 },
    ]);
    // Never a single merged "4 dispatched" bucket.
    const dispatchedEntry = counts.find((c) => c.decision === 'dispatched');
    expect(dispatchedEntry?.count).not.toBe(SUMMARY.total);
  });

  it('omits zero-count buckets', () => {
    const counts = summarizeSprintDispatchCounts(SUMMARY);
    expect(counts.some((c) => c.count === 0)).toBe(false);
  });

  it('maps the summary\'s "errored" key to the "error" decision so its tone/label are found', () => {
    const counts = summarizeSprintDispatchCounts({ ...SUMMARY, errored: 3, total: 7 });
    const errorEntry = counts.find((c) => c.decision === 'error');
    expect(errorEntry).toEqual({ decision: 'error', count: 3 });
  });

  it('returns an empty array when every bucket is zero', () => {
    const empty: SprintDispatchSummary = {
      total: 0,
      dispatched: 0,
      waiting_approval: 0,
      blocked: 0,
      already_in_flight: 0,
      waiting_on_dependencies: 0,
      not_eligible: 0,
      no_dispatch_policy: 0,
      would_dispatch: 0,
      errored: 0,
    };
    expect(summarizeSprintDispatchCounts(empty)).toEqual([]);
  });
});
