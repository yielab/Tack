// Wire-format boundary for the Provisioning wizard. Every assumption about
// `POST /api/templates/{id}/provision`, `GET /api/control-planes`, and
// `GET /api/templates` lives in this one file — `ProvisioningWizard.tsx`
// and `format.ts` only ever import types and functions from here, never
// construct a request body or read a raw wire field themselves, the same
// pattern every other feature's `api.ts` in this tree follows.
//
// Field shapes are copied field-for-field from the real Rust handler
// (`crates/tack-api/src/handlers/provisioning.rs` —
// `CreateProjectWithPodRequest`/`ProvisionPodRequest`/
// `CreateProjectWithPodResponse`/`ProvisioningOutcome`), not a guess, and
// cross-checked against the regenerated `shared/api/schema.gen.ts`.

import { request, isOrchestrationDisabledError } from '../../shared/api/client';

/** `"unknown"` | `"healthy"` | `"degraded"` | `"unreachable"` — the
 *  reconciler's health state machine, persisted verbatim. Duplicated from
 *  `features/fleet/api.ts`/`features/settings/orchestration/api.ts` rather
 *  than imported — same cross-feature-boundary precedent those two files
 *  already set for each other. */
export type ControlPlaneHealth = 'healthy' | 'degraded' | 'unreachable' | 'unknown';

/** One row of `GET /api/control-planes` — only the fields the wizard's
 *  picker needs. */
export interface ControlPlaneOption {
  id: string;
  name: string;
  kind: string;
  health: ControlPlaneHealth;
}

/** One row of `GET /api/templates` — only the fields the wizard's picker
 *  needs. `orchestration`, when present, seeds the pod-shape step's
 *  defaults (blueprint, budget cap, verify command, status map) so picking
 *  a template that already declares an `orchestration` block pre-fills
 *  step 2 rather than leaving the operator to re-type what the template
 *  author already decided. */
export interface TemplateOption {
  id: string;
  name: string;
  project_type: string;
  orchestration: TemplateOrchestration | null;
}

/** `docket` pod blueprint — the five real values, from `core/blueprints.py`. */
export type OrchBlueprint = 'software' | 'research' | 'content' | 'ops' | 'agentic-product';

export const BLUEPRINT_OPTIONS: { value: OrchBlueprint; label: string }[] = [
  { value: 'software', label: 'Software (codebase pod)' },
  { value: 'research', label: 'Research (shared work dir)' },
  { value: 'content', label: 'Content (shared work dir)' },
  { value: 'ops', label: 'Ops (shared work dir)' },
  { value: 'agentic-product', label: 'Agentic product (shared work dir)' },
];

export interface TemplateStatusMap {
  dispatch_from: string[];
  on_running: string | null;
  on_waiting_approval: string | null;
  on_succeeded: string | null;
  on_failed: string | null;
  on_cancelled: string | null;
}

export interface TemplateOrchestration {
  blueprint: OrchBlueprint;
  pipeline_yaml: string | null;
  pipeline_file: string | null;
  verify_cmd: string | null;
  budget_usd: number | null;
  status_map: TemplateStatusMap;
  auto_dispatch: boolean;
  pod_shape: string | null;
}

/** `provision_pod` half of `POST /api/templates/{id}/provision`'s body. */
export interface ProvisionPodRequest {
  control_plane_id: string;
  remote_project: string;
  blueprint?: OrchBlueprint | null;
  path?: string | null;
  pod_shape?: string | null;
  budget_usd?: number | null;
  verify_cmd?: string | null;
  auto_dispatch?: boolean | null;
  pipeline_file?: string | null;
}

export interface CreateProjectWithPodRequest {
  name: string;
  description?: string | null;
  provision_pod: ProvisionPodRequest;
}

export interface ProvisionedPodMember {
  id: string;
  role: string;
  model: string;
}

/** Discriminated on `status` — mirrors the Rust `#[serde(tag = "status")]`
 *  enum exactly. Both variants mean the project is real; only `"linked"`
 *  means the pod is also wired up to it. See `ProvisioningWizard.tsx`'s
 *  `ResultPanel` for how each is rendered. */
export type ProvisioningOutcome =
  | {
      status: 'linked';
      control_plane_id: string;
      remote_project: string;
      blueprint: string;
      members: ProvisionedPodMember[];
      warnings: string[];
    }
  | {
      status: 'pod_created_link_failed';
      control_plane_id: string;
      remote_project: string;
      blueprint: string;
      members: ProvisionedPodMember[];
      warnings: string[];
    };

export interface ProvisionedProject {
  id: string;
  name: string;
  description: string | null;
}

export interface CreateProjectWithPodResponse {
  project: ProvisionedProject;
  provisioning: ProvisioningOutcome;
}

export const provisioningApi = {
  /** Doubles as the wizard's `orchAvailable()` probe — a 404 here means
   *  `TACK_ORCH_ENABLE` is unset server-side (the whole `/templates/{id}/
   *  provision` route lives in the same gated sub-router), so the wizard
   *  never needs a second "is orchestration on" request. */
  listControlPlanes: () => request<ControlPlaneOption[]>('/control-planes'),
  listTemplates: () => request<TemplateOption[]>('/templates'),
  provision: (templateId: string, body: CreateProjectWithPodRequest) =>
    request<CreateProjectWithPodResponse>(`/templates/${templateId}/provision`, {
      method: 'POST',
      body: JSON.stringify(body),
    }),
};

/** True when a request failed because orchestration is disabled
 *  server-side — distinct from any other failure. Delegates to
 *  `shared/api/client.ts#isOrchestrationDisabledError`;
 *  kept as its own export so every existing caller (`ProvisioningWizard.tsx`)
 *  keeps working unchanged. */
export function isOrchDisabled(err: unknown): boolean {
  return isOrchestrationDisabledError(err);
}
