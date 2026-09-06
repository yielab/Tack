// Wire-format boundary for the app-level Settings → Orchestration page —
// makes the agent-factory control center discoverable. Every assumption
// about `GET/PUT /api/settings/orchestration`
// and the `/control-planes` admin CRUD lives in this one file —
// `OrchestrationSettingsSection.tsx`, `ControlPlanesManager.tsx`, and
// `ProjectLinker.tsx` only ever import types and functions from here, never
// construct a request body or read a raw wire field themselves, the same
// pattern every other feature's `api.ts` in this tree follows.
//
// ── The `GET/PUT /api/settings/orchestration` contract ──────────────────────
//
// Frozen by the E1/E2 split before either agent started (E1 builds the Rust
// side — making `TACK_ORCH_ENABLE` runtime-toggleable and DB-backed,
// following the existing Cloud Backup settings precedent
// (`app_meta`/`TACK_BACKUP_*`) — E2, this file, builds everything the
// operator sees). Field-for-field, exactly as specified — do not rename or
// "improve" a field name here without re-syncing with E1's DTO:
//
//   GET  /api/settings/orchestration → 200, reachable even when
//        orchestration is OFF (this is the one orchestration route that must
//        never itself be gated behind the flag it reports on — otherwise an
//        operator could never discover the toggle in the first place).
//   PUT  /api/settings/orchestration  body: { enabled: boolean } → 200, same
//        response shape as the GET.
//
// `source` and `env_default` look redundant at first read but answer two
// different questions: `env_default` is what `TACK_ORCH_ENABLE` is actually
// set to in this process's environment (or `false` if unset); `source` says
// which of that env value or a saved database override is the one currently
// in effect for `enabled`. An operator can save `enabled: false` from the UI
// on a server that has `TACK_ORCH_ENABLE=true` in its environment — `source`
// would then read `"database"` and `enabled` would be `false`, with
// `env_default: true` still telling the operator what the environment alone
// would have produced. `OrchestrationSettingsSection.tsx` surfaces both,
// so an operator whose deployment sets
// TACK_ORCH_ENABLE understands where the value came from.
//
// This endpoint's own fetch failing is NOT treated as "orchestration is
// disabled" the way every other orchestration route is (see
// `shared/api/client.ts#isOrchestrationDisabledError`) — by contract it
// always answers 200. A failure here means the request genuinely
// couldn't reach the server, so the section renders a distinct
// "couldn't load" retry state, never the
// "disabled" empty state the rest of the app uses.
//
// ── Control-plane admin (`/control-planes`) ──────────────────────────────
//
// `POST/GET/PATCH/DELETE /api/control-planes(/{id})` already exist —
// `crates/tack-api/src/handlers/orch.rs`'s
// `create_control_plane`/`list_control_planes`/`get_control_plane`/
// `update_control_plane`/`delete_control_plane` — but before this file no
// page ever called anything past `GET /control-planes` (`LinkForm.tsx`
// reads
// the list to populate a picker; its own header note says registering one is
// still a `curl POST /api/control-planes` away). `ControlPlaneDetail` below
// is copied field-for-field from `ControlPlaneResponse`
// (`handlers/orch.rs`) — note there is deliberately no `token` field
// anywhere in a response; the token is write-only, exactly like
// `TACK_BACKUP_SECRET_KEY`/Cloud Backup's `secret_key_set` boolean, mirrored
// here as `token_set`.
//
// Duplicated rather than imported from `features/settings/orchestration/
// api.ts` (D2's per-project panel) even though both live under
// `features/settings/**` (the same top-level `architecture.test.ts` feature,
// so importing across would be legal) — that file's `ControlPlaneOption` is
// deliberately narrow (id/name/kind/health, exactly what a `<select>`
// needs); this page's control-plane *admin* UI needs the fuller
// `ControlPlaneResponse` shape (base_url, token_set, last_seen_at,
// consecutive_failures) to actually manage a plane, not just pick one.

import { request } from '../../../shared/api/client';
import type { Capabilities } from '../../../shared/orch/capabilities';

export type OrchestrationSettingsSource = 'database' | 'env_default';

/** `GET/PUT /api/settings/orchestration` response — see this file's header
 *  comment for what `source` vs. `env_default` each mean. */
export interface OrchestrationSettings {
  enabled: boolean;
  source: OrchestrationSettingsSource;
  reconciler_running: boolean;
  control_plane_count: number;
  linked_project_count: number;
  poll_secs: number;
  /** Whether `TACK_ORCH_APPROVAL_TOKEN` is configured — never the value
   *  itself, the same "boolean only, secret never round-trips" rule as
   *  `token_set` below. (Card G1 retired the one place this pattern used to
   *  leak into a response as a one-off "is this feature usable" flag — the
   *  approvals inbox's old grant-availability boolean — in favour of the
   *  server enforcing the real check on every write regardless; see
   *  `features/approvals/api.ts`'s header for why. This field is a
   *  different, legitimate case: a plain server-config boolean an operator
   *  is here specifically to read, not a per-request capability guess.) */
  approval_token_set: boolean;
  env_default: boolean;
}

export interface UpdateOrchestrationSettingsBody {
  enabled: boolean;
}

/** `"unknown"` | `"healthy"` | `"degraded"` | `"unreachable"` |
 *  `"unconfigured"` — the reconciler's health state machine, persisted
 *  verbatim. `unconfigured` (card G1) means this build of Tack could not
 *  build a live adapter for the plane's `kind` at all — most commonly a
 *  restored backup, whose `secrets` column comes back `NULL` — so the
 *  reconciler never attempted a poll. Duplicated from
 *  `features/fleet/api.ts`/`features/settings/orchestration/api.ts` per
 *  their own established precedent (each feature directory's wire boundary
 *  owns its own copy of this tiny union rather than reaching across). */
export type ControlPlaneHealth = 'healthy' | 'degraded' | 'unreachable' | 'unknown' | 'unconfigured';

/** `ControlPlaneResponse` (`handlers/orch.rs`) in full — every field a
 *  management UI needs, as opposed to `features/settings/orchestration/
 *  api.ts#ControlPlaneOption`'s picker-only projection. */
export interface ControlPlaneDetail {
  id: string;
  name: string;
  kind: string;
  base_url: string;
  api_version: string | null;
  health: ControlPlaneHealth;
  last_seen_at: string | null;
  consecutive_failures: number;
  /** True when a docket Bearer token is currently stored for this plane.
   *  The token itself is write-only over this API — never returned. */
  token_set: boolean;
  /** What this plane can actually do — `null` only in the `'unconfigured'`
   *  health case: this build of Tack has no adapter for `kind` at all, so
   *  there was nothing to ask. `ControlPlanesManager.tsx` reads this,
   *  never `kind`, to decide what to show. */
  capabilities: Capabilities | null;
  created_at: string;
  updated_at: string;
}

export interface CreateControlPlaneBody {
  name: string;
  /** Defaults to `"docket"` server-side when omitted. */
  kind?: string;
  base_url: string;
  /** docket Bearer token. Write-only — never sent back by the server. */
  token?: string;
}

export interface UpdateControlPlaneBody {
  name?: string;
  base_url?: string;
  /** Absent = leave the stored token untouched. `null` = clear it. A string
   *  = set/replace it — mirrors `UpdateControlPlaneRequest`'s tri-state
   *  field exactly (`handlers/orch.rs`). */
  token?: string | null;
}

export const orchestrationSettingsApi = {
  get: () => request<OrchestrationSettings>('/settings/orchestration'),

  update: (enabled: boolean) =>
    request<OrchestrationSettings>('/settings/orchestration', {
      method: 'PUT',
      body: JSON.stringify({ enabled } satisfies UpdateOrchestrationSettingsBody),
    }),

  listControlPlanes: () => request<ControlPlaneDetail[]>('/control-planes'),

  getControlPlane: (id: string) => request<ControlPlaneDetail>(`/control-planes/${id}`),

  createControlPlane: (body: CreateControlPlaneBody) =>
    request<ControlPlaneDetail>('/control-planes', {
      method: 'POST',
      body: JSON.stringify(body),
    }),

  updateControlPlane: (id: string, body: UpdateControlPlaneBody) =>
    request<ControlPlaneDetail>(`/control-planes/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(body),
    }),

  deleteControlPlane: (id: string) =>
    request<void>(`/control-planes/${id}`, { method: 'DELETE' }),
};
