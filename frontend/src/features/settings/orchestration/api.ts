// Wire-format boundary for the project-settings "Orchestration" panel
// (budget + policy panels). Every assumption
// about `GET/PUT /api/projects/{id}/orch-link`, `GET /api/control-planes`,
// `GET /api/projects/{id}/orch-budget`, and `GET /api/projects/{id}/orch-policy`
// lives in this one file — `BudgetPanel.tsx`, `PolicyPanel.tsx`,
// `LinkForm.tsx`, and `OrchestrationPanel.tsx` only ever import types and
// functions from here, never construct a request body or read a raw wire
// field themselves, the same pattern every other feature's `api.ts` in
// this tree follows.
//
// Every field below is copied field-for-field from the real Rust handler
// (`crates/tack-api/src/handlers/orch.rs` — `OrchLinkView`/`OrchLinkResponse`,
// `ControlPlaneResponse`, `OrchBudgetResponse`, `OrchPolicyResponse`), not
// a guess.
//
// **No "paused" field anywhere in here, deliberately.** `OrchBudgetResponse`'s
// own doc comment (`handlers/orch.rs`) explains why: docket has no HTTP route
// to clear a budget pause, and the one read-side proxy that exists (a
// `paused_refused` trace event) can't be attributed to a single linked
// project with Tack's current schema. Do not add a `paused`/`is_paused`
// field here without first closing that gap server-side — a client-invented
// pause indicator would silently do nothing.

import { request, isOrchestrationDisabledError } from '../../../shared/api/client';

/** `"unknown"` | `"healthy"` | `"degraded"` | `"unreachable"` — the
 *  reconciler's health state machine, persisted verbatim. `null` only when
 *  the project (or, for policy, no metrics have ever been scraped) has
 *  nothing to report health for. */
export type ControlPlaneHealth = 'healthy' | 'degraded' | 'unreachable' | 'unknown';

/** One row of `GET /api/control-planes` — only the fields the link form's
 *  picker needs. */
export interface ControlPlaneOption {
  id: string;
  name: string;
  kind: string;
  health: ControlPlaneHealth;
}

export interface OrchLink {
  project_id: string;
  control_plane_id: string;
  remote_project: string;
  pipeline_file: string | null;
  blueprint: string | null;
  auto_dispatch: boolean;
  budget_usd: number | null;
  /** `tack_orch::adapters::legacy_bridge::LEGACY_DOCKET_COMPATIBILITY_LABEL`,
   *  verbatim — never a locally re-typed string. */
  compatibility_label: string;
  /** `tack_orch::adapters::legacy_bridge::LEGACY_DOCKET_COMPATIBILITY_POLICY`,
   *  verbatim. */
  compatibility_policy: string;
  created_at: string;
  updated_at: string;
}

export interface OrchLinkView {
  linked: boolean;
  link: OrchLink | null;
}

export interface UpsertOrchLinkBody {
  control_plane_id: string;
  remote_project: string;
  budget_usd?: number | null;
}

/** `GET /api/projects/{id}/orch-budget` response. */
export interface OrchBudget {
  linked: boolean;
  control_plane_id: string | null;
  control_plane_name: string | null;
  health: ControlPlaneHealth | null;
  /** User-set cap — `null` if unlinked or never set. */
  budget_usd: number | null;
  /** Always a real, present number — never null, even when unlinked (a
   *  project can carry real dispatch history after being unlinked). */
  tokens_in: number;
  tokens_out: number;
  /** `null` = stale/unknown (unlinked, or the linked plane is unreachable).
   *  `0` = a reachable plane that genuinely has nothing mirrored yet. */
  cost_usd_estimated: number | null;
  /** Always `null` today — no pricing-snapshot mechanism exists yet. */
  pricing_snapshot_at: string | null;
}

export interface ToolCallEntry {
  /** `"allow"` | `"ask"` | `"deny"` — shown verbatim for any other value. */
  decision: string;
  count: number;
}

export interface PolicyHitEntry {
  policy_id: string;
  hook: string;
  action: string;
  count: number;
}

export interface ApprovalChannelEntry {
  channel: string;
  outcome: string;
  count: number;
}

/** `GET /api/projects/{id}/orch-policy` response. Every figure here is
 *  **control-plane-wide**, not scoped to just this project — see
 *  `scoped_to_control_plane_only` (always `true` when present) and this
 *  file's exported `POLICY_SCOPE_CAVEAT` copy. docket's own `/metrics` has
 *  no per-project breakdown at all (confirmed by reading `serve.py` — every
 *  guardrail/tool-call/approval counter folds every linked project's trace
 *  files together), so Tack's mirrored `orch_metrics` inherits the same
 *  fleet-wide shape. */
export interface OrchPolicy {
  linked: boolean;
  control_plane_id: string | null;
  control_plane_name: string | null;
  health: ControlPlaneHealth | null;
  scoped_to_control_plane_only: boolean;
  scraped_at: string | null;
  tool_calls: ToolCallEntry[];
  /** `deny / (allow + ask + deny)`. `null` — never `0` — when no tool-gate
   *  decision has been observed at all. */
  denial_rate: number | null;
  policy_hits: PolicyHitEntry[];
  approvals_by_channel: ApprovalChannelEntry[];
}

export const orchestrationApi = {
  getLink: (projectId: string) => request<OrchLinkView>(`/projects/${projectId}/orch-link`),
  putLink: (projectId: string, body: UpsertOrchLinkBody) =>
    request<OrchLink>(`/projects/${projectId}/orch-link`, {
      method: 'PUT',
      body: JSON.stringify({ ...body, status_map: {} }),
    }),
  listControlPlanes: () => request<ControlPlaneOption[]>('/control-planes'),
  getBudget: (projectId: string) => request<OrchBudget>(`/projects/${projectId}/orch-budget`),
  getPolicy: (projectId: string) => request<OrchPolicy>(`/projects/${projectId}/orch-policy`),
};

/** True when a request failed because orchestration is disabled server-side —
 *  distinct from any other failure. Delegates to
 *  `shared/api/client.ts#isOrchestrationDisabledError` (the
 *  same "409/403 + machine-readable code, 404 kept only as a legacy
 *  fallback" contract `features/settings/orchestrationSettings/api.ts`
 *  documents in full); kept as its own export so every existing caller
 *  (`OrchestrationPanel.tsx`) keeps working unchanged. */
export function isOrchDisabled(err: unknown): boolean {
  return isOrchestrationDisabledError(err);
}
