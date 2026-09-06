// Wire-format boundary for per-item agent activity (the item-detail "Agent
// Activity" tab, and the badge chip shown on Board/List/Table).
//
// Both endpoints below (`crates/tack-api/src/handlers/orch.rs`'s
// `get_item_agent_activity` / `get_project_agent_activity`) are reconciled
// against this file field-for-field. Two assumptions worth stating
// explicitly since nothing in the type shape below makes them obvious:
//
//  1. "Latest attempt" tie-break (bulk badge endpoint): highest `attempt`
//     number wins; ties broken by `dispatched_at` desc.
//  2. Inner join, not a left join with an explicit null state — an item
//     with no `orch_tasks` row has no row in `AgentBadgeResponse.rows`,
//     which is what `useAgentActivityMap` already treats as "no badge."
//
// `ItemAgentActivity.events_truncated` / `.events_retention_days` (see their
// own doc comments below) are additive fields; `AgentActivityTab.tsx`
// doesn't render them yet — that's a follow-up, not a blocker, since
// `events` is empty until trace ingestion lands regardless.
//
// Field provenance — every field here is a snake_case projection of a real
// column, not an invention:
//
//  1. `crates/tack-db/src/migrations.rs`:
//     - migration 021 `orch_tasks` (PK `(item_id, remote_task_id)`):
//       `remote_task_id, remote_run_id, remote_status, attempt, tokens_in,
//       tokens_out, cost_usd_estimated, dispatched_at`. `trusted` (an
//       untrusted auto-dispatch flag) exists on the table but is
//       deliberately NOT surfaced here — there's no UI story for it yet
//       (no auto-dispatch feature has landed), so exposing it now would
//       just be a field nobody reads.
//     - migration 022 `orch_runs`: `run_id, source, state, started_at,
//       ended_at, error` (via `orch_tasks.remote_run_id`, not a hard FK —
//       see the migration's own comment on why).
//     - migration 023 `orch_events`: `id, event_type, payload, occurred_at`
//       — this table is currently ALWAYS EMPTY: ingestion is blocked on a
//       docket endpoint that doesn't exist yet (`GET /traces/{project}`).
//       The `events` field is still modeled and rendered now so that when
//       ingestion ships, populating it is a pure backend change — no
//       frontend diff.
//     - migration 024 `orch_approvals`: `token, remote_task_id, agent,
//       action, state, requested_at, decided_at`.
//  2. `crates/tack-orch/src/lib.rs` — the wire *values* for
//     the state fields come from `TaskStatus` (orch_tasks.remote_status:
//     pending|running|done|failed|blocked|waiting_approval), `RunState`
//     (orch_runs.state: queued|running|succeeded|failed|cancelled), and
//     `ApprovalState` (orch_approvals.state: pending|granted|denied). Every
//     one of these carries an `Unknown(String)` fallback on the Rust side —
//     a docket upgrade that adds a state must degrade to "shown as-is,"
//     never a deserialization error — so every string field below is typed
//     as a plain `string`, not a TS union, and rendering code must handle
//     an unrecognised value by showing it verbatim rather than assuming
//     it's one of the known set. `deriveAgentChipState` in `./format.ts` is
//     where that degradation happens for the chip.
//
// Two endpoints, two shapes:
//
//  - `GET /items/{id}/agent-activity` — full detail for the item-detail tab.
//    One row per `orch_tasks` attempt for this item, each carrying its
//    correlated run (if `remote_run_id` resolves) and events (empty until
//    trace ingestion lands), plus every `orch_approvals` row for this item
//    (both pending and decided — the tab is a history view, not just a
//    pending-inbox).
//  - `GET /projects/{id}/agent-activity` — the bulk shape Board/List/Table
//    need for badges: one row per item *that has at least one `orch_tasks`
//    row* (an inner join, not a left join with nulls) — an item with no
//    agent activity simply has no row, which is how the chip knows to
//    render nothing. Each row carries only the latest attempt's raw
//    `remote_status`, projected down to what a badge needs. "Latest
//    attempt" means the row with the highest `attempt` number (ties broken
//    by `dispatched_at` desc), not e.g. "any non-terminal attempt wins" —
//    confirmed against `list_latest_orch_task_status_for_project`
//    in `crates/tack-db/src/repo/orch.rs`.

import { request, isOrchestrationDisabledError } from '../api/client';

/** One `orch_events` row, scoped to the attempt/run it belongs to. Always an
 *  empty array pre-B2 (trace ingestion) — see the header comment. `payload`
 *  is kept as `unknown` (mirrors the Rust side's `serde_json::Value`) since
 *  its shape varies by `event_type` and isn't specified anywhere yet. */
export interface ItemAgentEvent {
  id: string;
  /** docket's event type verbatim (`tool_call`, `approval_requested`,
   *  `cost_charged`, `budget_exceeded`, `verification_failed`,
   *  `tester_verdict_failed`, `rework_started`, `review_rejected`,
   *  `session_end`, `status_map_rejected`, …) — an unrecognised type is
   *  stored and shown as-is, never dropped (roadmap.md Task 34.4). */
  event_type: string;
  payload: unknown;
  occurred_at: string;
}

/** The `orch_runs` row correlated to an attempt via `remote_run_id`, or
 *  `null` when the task hasn't been picked up by a run yet (queued) or the
 *  correlation hasn't resolved (`remote_run_id` isn't a hard FK — see
 *  migration 021's comment). */
export interface ItemAgentRun {
  run_id: string;
  /** `RunSource`: cli | webhook | schedule | sweep | mcp | (unknown, verbatim). */
  source: string;
  /** `RunState`: queued | running | succeeded | failed | cancelled | (unknown, verbatim). */
  state: string;
  started_at: string | null;
  ended_at: string | null;
  /** Non-empty only when `state === 'failed'`. */
  error: string;
}

/** One `orch_tasks` row (one dispatch attempt) for the item. */
export interface ItemAgentAttempt {
  remote_task_id: string;
  remote_run_id: string | null;
  /** `TaskStatus`: pending | running | done | failed | blocked |
   *  waiting_approval | (unknown, verbatim — see header comment). */
  remote_status: string;
  /** 1-based; an item can be redispatched, each redispatch is a new row
   *  (migration 021's PK note — this is why it's `(item_id, remote_task_id)`
   *  and not a single-column key). */
  attempt: number;
  dispatched_at: string;
  /** Primary measure — always a number, never absent. */
  tokens_in: number;
  tokens_out: number;
  /** Derived estimate, never billed spend. `null` when the task hasn't been
   *  costed yet (e.g. still queued). */
  cost_usd_estimated: number | null;
  /** Pricing-table snapshot date backing `cost_usd_estimated`. Currently
   *  ALWAYS `null` — no pricing-snapshot mechanism exists anywhere in the
   *  system yet (confirmed against A4's Wave-1 handoff, which found the same
   *  for the Fleet view's identical field). Rendering code must say so
   *  honestly rather than dropping the qualifier — see `format.ts`. */
  pricing_snapshot_at: string | null;
  run: ItemAgentRun | null;
  events: ItemAgentEvent[];
}

/** One `orch_approvals` row for the item (pending or already decided — the
 *  tab shows history, not just the live inbox). */
export interface ItemAgentApproval {
  token: string;
  remote_task_id: string | null;
  /** `orch_approvals.agent` — populated from docket's `role` field on
   *  ingestion: there's no separate "agent" concept on docket's wire
   *  shape — role is the closest field. Kept as a plain string, may be
   *  `null` if docket ever omits it. */
  agent: string | null;
  action: string | null;
  /** `ApprovalState`: pending | granted | denied | (unknown, verbatim). */
  state: string;
  requested_at: string;
  /** `null` until `decide_approval` writes a decision back — ingestion
   *  alone never populates this. */
  decided_at: string | null;
}

export interface ItemAgentActivity {
  /** Newest attempt first — the tab's own display order. */
  attempts: ItemAgentAttempt[];
  approvals: ItemAgentApproval[];
  /** Not yet rendered by
   *  `AgentActivityTab.tsx` (a follow-up, not a blocker: `events` is empty
   *  until trace ingestion lands regardless, so there's nothing to
   *  qualify yet in practice). `orch_events_daily` (the retention rollup)
   *  aggregates by day/control_plane/event_type only — it drops `item_id`
   *  entirely, so the backend can't say "this item's events were rolled
   *  up," only "this item has an attempt old enough that some of its
   *  history might have been." True when any attempt in `attempts` was
   *  dispatched before the current retention cutoff (`now -
   *  events_retention_days`) — the honest signal for "don't treat `events`
   *  as necessarily the complete history." */
  events_truncated: boolean;
  /** The retention window (`TACK_ORCH_EVENT_RETENTION_DAYS`) backing
   *  `events_truncated`, echoed for context. */
  events_retention_days: number;
}

/** One row of the bulk `GET /projects/{id}/agent-activity` response — the
 *  minimum a badge needs. Only items with at least one `orch_tasks` row
 *  appear (see header comment on the inner-join assumption). */
export interface AgentBadgeRow {
  item_id: string;
  /** Latest attempt's raw `remote_status` — see `deriveAgentChipState` in
   *  `./format.ts` for how this collapses to the chip's 5 visual states. */
  remote_status: string;
  attempt: number;
  updated_at: string;
}

export interface AgentBadgeResponse {
  rows: AgentBadgeRow[];
}

/** True when the request failed because orchestration is disabled
 *  server-side — the default for every existing install. Callers treat this
 *  the same as "no agent activity" (fail open to a quiet UI, not an error
 *  state) since it's the overwhelmingly common case, not a bug. Delegates to
 *  `shared/api/client.ts#isOrchestrationDisabledError`,
 *  the one canonical place this check lives; kept as its own export so every
 *  existing caller keeps working unchanged. */
export function isOrchDisabled(err: unknown): boolean {
  return isOrchestrationDisabledError(err);
}

export const agentActivityApi = {
  getForItem: (itemId: string) =>
    request<ItemAgentActivity>(`/items/${itemId}/agent-activity`),
  listForProject: (projectId: string) =>
    request<AgentBadgeResponse>(`/projects/${projectId}/agent-activity`),
};
