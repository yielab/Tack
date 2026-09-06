// Wire-format boundary for the Fleet view.
//
// `GET /api/fleet` (`crates/tack-api/src/router.rs`'s `orch::get_fleet`) is
// real and confirmed field-for-field against the handler. Every assumption
// about the response shape is isolated to this one file: the types below plus
// the single `fleetApi.list()` fetch function. If the endpoint's shape ever
// changes, reconciling the frontend means editing THIS FILE ONLY — never
// `FleetRow.tsx`, `FleetPage.tsx`, `HealthChip.tsx`, or `format.ts`, which all
// consume the `FleetRow` type, not the wire response directly.
//
// Derived from two sources:
//
//  1. `crates/tack-orch/src/lib.rs` — `FleetStatus`,
//     `FleetAgent`, `Health`, mirroring docket's own `/status.json`. A
//     `FleetAgent` carries `id, name, kind, scope, model, registered,
//     bindings, last_activity, cost_usd_estimated ("costUsd" on the wire),
//     budget_usd`.
//  2. `crates/tack-db/src/migrations.rs` migrations 019 (`control_planes`:
//     name, kind, base_url, health, last_seen_at, consecutive_failures,
//     api_version) and 020 (`orch_links`: project_id, control_plane_id,
//     blueprint, auto_dispatch, budget_usd, status_map).
//
// `GET /api/fleet` is the Fleet view's aggregate: one row per Tack project
// that has an `orch_links` row, merging the link, its linked
// `control_planes` row (health, last_seen_at, ...), and a snapshot of that
// plane's live `FleetStatus` (roster, cost, budget). `FleetRow` below is
// the confirmed snake_case projection of that response.

import { request, isOrchestrationDisabledError } from '../../shared/api/client';
import type { Capabilities } from '../../shared/orch/capabilities';

/** Mirrors the reconciler's health state machine:
 *  `healthy` → `degraded` (3 consecutive poll failures) → `unreachable` (10).
 *  Recovery is immediate on a single success. `unknown` covers a plane that
 *  has been registered but has not completed a first poll yet — it is
 *  visually treated the same as `unreachable` (no trustworthy data), just
 *  with different copy ("not yet connected" vs "last seen …"). `unconfigured`
 *  is the fifth state: this build of Tack could not build a live
 *  adapter for the plane's `kind` at all — most commonly a restored backup,
 *  whose `secrets` column comes back `NULL` — so the reconciler never even
 *  attempted a poll. Distinct from `unknown` (registered, poll pending) and
 *  from `unreachable` (polled, failing): here nothing was ever tried. */
export type ControlPlaneHealth = 'healthy' | 'degraded' | 'unreachable' | 'unknown' | 'unconfigured';

/** docket's `/status.json` → `FleetStatus.gateway` ("active"/"inactive"),
 *  mirrored through the reconciled `control_planes` row. `unknown` is the
 *  frontend's own addition for "we have no fresh reading" (stale planes). */
export type FleetGatewayState = 'active' | 'inactive' | 'unknown';

/** One roster member, projected from tack-orch's `FleetAgent` DTO down to the
 *  fields the row needs: who, what role, which model. */
export interface FleetRosterAgent {
  id: string;
  name: string;
  /** `FleetAgent.kind` — the agent's role/specialty (e.g. "backend-dev",
   *  "reviewer"), not a Tack `item_type`. */
  role: string;
  model: string;
}

/** One row of `GET /api/fleet` — a Tack project's `orch_links` entry, its
 *  linked `control_planes` row, and a snapshot of that plane's FleetStatus. */
export interface FleetRow {
  project_id: string;
  project_name: string;
  control_plane_id: string;
  control_plane_name: string;
  /** `control_planes.kind`, e.g. `"docket"`. */
  control_plane_kind: string;
  health: ControlPlaneHealth;
  /** `control_planes.last_seen_at` — RFC3339, or `null` if the plane has
   *  never completed a poll (fresh registration). */
  last_seen_at: string | null;
  consecutive_failures: number;
  /** What this row's control plane can actually do — `null` only in the
   *  `'unconfigured'` health case (`ControlPlaneResponse.capabilities` /
   *  `FleetEntry.capabilities` on the wire): this build of Tack has no
   *  adapter for the plane's `kind` at all, so there was nothing to ask.
   *  Every gated control in this row reads THIS
   *  field, never the `control_plane_kind` string above it. */
  capabilities: Capabilities | null;
  gateway: FleetGatewayState;
  roster: FleetRosterAgent[];
  /** Most recent `FleetAgent.last_activity` across the roster, RFC3339, or
   *  `null` if no agent has ever run. */
  last_activity_at: string | null;
  /** Token counts are the primary measure — always a
   *  number, never absent. A `0` is only meaningful when `health ===
   *  'healthy'`; the row component must not render it as fact otherwise. */
  tokens_in: number;
  tokens_out: number;
  /** Named `cost_usd_estimated` to match tack-orch's field exactly (never
   *  "cost" or "spend") — an estimate from labelled pricing, never billed
   *  spend. `null` when the plane has never reported a figure. */
  cost_usd_estimated: number | null;
  /** Pricing-table snapshot date backing `cost_usd_estimated`, when the plane
   *  reports one. RFC3339 or a plain date string. */
  pricing_snapshot_at: string | null;
  /** `orch_links.budget_usd` — a user-set cap, deliberately NOT suffixed
   *  `_estimated`: it's a config value, not a
   *  derived spend figure. */
  budget_usd: number | null;
  pending_approval_count: number;
}

export interface FleetResponse {
  rows: FleetRow[];
}

/** True when the request failed because orchestration is disabled
 *  server-side — distinct from a 200 with an empty `rows` array (enabled,
 *  nothing registered yet) and from any other failure (network error, 500,
 *  ...). The Fleet page renders a different empty state for each case.
 *  Delegates to `shared/api/client.ts#isOrchestrationDisabledError`, the one
 *  place this check is actually defined — this export
 *  stays so every existing caller (`FleetPage.tsx`) keeps working unchanged. */
export function isOrchDisabled(err: unknown): boolean {
  return isOrchestrationDisabledError(err);
}

export const fleetApi = {
  list: () => request<FleetResponse>('/fleet'),
};
