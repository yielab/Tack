// Frontend DTO surface.
//
// The backend DTOs below are DERIVED from the machine-generated OpenAPI schema
// (`src/shared/api/schema.gen.ts`, produced by `npm run gen:api` from
// `docs/openapi.json`). They therefore cannot silently drift from the Rust API:
// if a handler's request/response shape changes, `gen:api` regenerates the
// schema and any incompatible usage fails `type-check` in CI.
//
// Only genuinely frontend-internal shapes that have no 1:1 backend schema stay
// hand-written here:
//   - `BoardState` / `BoardColumn` — the *derived* Kanban view built client-side
//     by `deriveBoard()` (the backend has no single "board state" response).
//   - `BoardEvent` — the WebSocket event union (WS frames aren't in the REST spec).
//   - the template-builder shapes (`TemplateCustomField`, `TemplateBoardConfig`,
//     `CreateTemplate`) — the editor's working model, which is looser than the
//     backend's `CreateProjectTemplate` body.

import type { components } from '../api/schema.gen';

type Schemas = components['schemas'];

// ─── Projects ──────────────────────────────────────────────────────────────

export type Project = Schemas['Project'];
export type ProjectType = Schemas['ProjectType'];
export type WorkflowConfig = Schemas['WorkflowConfig'];
export type WorkflowType = Schemas['WorkflowType'];
export type StatusCategory = Schemas['StatusCategory'];
/** A single workflow status column (`StatusDef` in the backend schema). */
export type WorkflowStatus = Schemas['StatusDef'];
export type UpdateProject = Schemas['UpdateProject'];
export type CreateProject = Schemas['CreateProject'];
export type ProjectModelDefault = Schemas['ProjectModelDefault'];

// ─── Items ─────────────────────────────────────────────────────────────────

export type Item = Schemas['Item'];

/**
 * Paginated envelope returned by `GET /api/projects/:id/items` (`PaginatedItems`
 * in the backend schema). Clients page through `total` items using
 * `page`/`per_page` so large projects are never silently truncated.
 */
export type ItemPage = Schemas['PaginatedItems'];

/** Detail envelope for `GET /api/items/:id` (item + roles + dependencies). */
export type ItemDetail = Schemas['ItemDetail'];

export type ItemType = Schemas['ItemType'];
export type Priority = Schemas['Priority'];
export type EstimateUnit = Schemas['EstimateUnit'];

// NB: the backend `CreateItem` has **no `status`** (creates land in the first
// workflow column) and **does** accept `assignee` — both reconciled here by
// deriving from the spec. `UpdateItem` likewise has no `parent_id` (re-parenting
// via PATCH is not supported by the API — see List.tsx).
export type CreateItem = Schemas['CreateItem'];
export type UpdateItem = Schemas['UpdateItem'];

// ─── Sprints ───────────────────────────────────────────────────────────────

export type Sprint = Schemas['Sprint'];
export type SprintStatus = Schemas['SprintStatus'];
export type CreateSprint = Schemas['CreateSprint'];

// ─── Board (frontend-derived, not a backend response) ──────────────────────

export interface BoardState {
  columns: BoardColumn[];
}

export interface BoardColumn {
  status: string;
  items: Item[];
  wip_limit?: number | null;
  wip_exceeded: boolean;
}

// ─── Custom fields (definitions) ───────────────────────────────────────────

export type CustomFieldType = Schemas['CustomFieldType'];
/** Field definition (`CustomFieldDefinition` in the backend schema). */
export type CustomField = Schemas['CustomFieldDefinition'];
export type CreateCustomField = Schemas['CreateCustomField'];
export type UpdateCustomField = Schemas['UpdateCustomField'];
export type CustomFieldValue = Schemas['CustomFieldValue'];

// ─── Project templates ─────────────────────────────────────────────────────

export type ProjectTemplate = Schemas['ProjectTemplate'];
export type CreateProjectFromTemplate = Schemas['CreateProjectFromTemplate'];

// The template *builder* keeps its own looser working shapes. These feed the
// `POST /custom-templates` body (backend `CreateProjectTemplate`); the editor
// carries extra UI-only conveniences (e.g. `collapsed`) not sent to the API.
export interface TemplateCustomField {
  name: string;
  field_type: CustomFieldType;
  description?: string | null;
  required?: boolean;
  options?: string[] | null;
}

export interface TemplateBoardColumn {
  status: string;
  wip_limit?: number | null;
  collapsed?: boolean;
}

export interface TemplateBoardConfig {
  name: string;
  description?: string | null;
  columns: TemplateBoardColumn[];
  is_default?: boolean;
}

export interface CreateTemplate {
  name: string;
  description?: string | null;
  project_type: ProjectType;
  vocabulary?: Record<string, string> | null;
  workflow?: WorkflowConfig | null;
  custom_fields?: TemplateCustomField[] | null;
  default_boards?: TemplateBoardConfig[] | null;
}

// ─── Comments ──────────────────────────────────────────────────────────────

export type CommentType = Schemas['CommentType'];
export type Comment = Schemas['Comment'];
export type CreateComment = Schemas['CreateComment'];

// ─── Dependencies ──────────────────────────────────────────────────────────

export type DependencyType = Schemas['DependencyType'];
export type Dependency = Schemas['Dependency'];
export type CreateDependency = Schemas['CreateDependency'];

// ─── Roles / specialties ───────────────────────────────────────────────────

export type Role = Schemas['Role'];
export type CreateRole = Schemas['CreateRole'];

// ─── Attachments ───────────────────────────────────────────────────────────

export type Attachment = Schemas['Attachment'];

// ─── Realtime board events (WebSocket) ─────────────────────────────────────
//
// Mirrors `BoardEvent` in crates/tack-api/src/handlers/websocket.rs, which is
// serialized with `#[serde(tag = "type", rename_all = "snake_case")]`. The
// WebSocket frames are not part of the REST OpenAPI document, so this union
// stays hand-written.

export type BoardEvent =
  | { type: 'item_created'; project_id: string; item_id: string; status: string }
  | {
      type: 'item_updated';
      project_id: string;
      item_id: string;
      old_status: string | null;
      new_status: string;
    }
  | { type: 'item_deleted'; project_id: string; item_id: string }
  | { type: 'board_config_updated'; project_id: string }
  | { type: 'sprint_updated'; project_id: string; sprint_id: string }
  // Mirrors `BoardEvent::AgentRunUpdated`. Emitted only when a mirrored agent
  // run's state actually changed (or was newly attributed to this item); a
  // `state` value outside the known `RunState` set is possible (docket's
  // `Unknown(String)` fallback) and must be rendered as-is, not assumed to
  // be one of the known strings.
  | { type: 'agent_run_updated'; project_id: string; item_id: string; run_id: string; state: string }
  // Mirrors `BoardEvent::ApprovalPending`. Emitted only on the transition
  // into a `pending` approval state — never on a grant/deny decision or a
  // re-poll of an already-pending approval.
  | {
      type: 'approval_pending';
      project_id: string;
      item_id: string;
      token: string;
      action: string | null;
    }
  | { type: 'ping' };

export type BoardEventType = BoardEvent['type'];
