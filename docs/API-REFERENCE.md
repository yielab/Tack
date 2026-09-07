# Tack API Reference

**Spec of record:** [`docs/openapi.json`](openapi.json), generated from the router — where this page and the spec disagree, the spec is right.
**Base URL:** `http://localhost:3210/api`
**WebSocket URL:** `ws://localhost:3210/api/projects/{id}/boards/live`

> ## The source of truth is the OpenAPI spec, not this file
>
> The authoritative, always-current contract is the machine-generated OpenAPI 3.1
> document — served live at **`GET /api/openapi.json`** and checked into the repo at
> **[`docs/openapi.json`](openapi.json)** (90+ paths, over 120 REST operations, plus 1
> WebSocket — check the file itself rather than trusting a hand-written count here, it
> grows every wave). It is generated from the handler annotations and gated in CI, so
> it cannot silently drift from the code.
>
> **This document is a hand-written companion** — narrative, auth model, and
> copy-paste `curl` examples. Exact field names, status codes, and request/response
> shapes may lag the code; when they disagree, **the spec wins**. Load
> `docs/openapi.json` into any OpenAPI viewer (Redocly / Scalar / Swagger Editor) for
> the exhaustive, verified schemas.

---

## Table of Contents

1. [Authentication](#authentication)
2. [Projects](#projects)
3. [Items](#items)
4. [Dependencies](#dependencies)
5. [Attachments](#attachments)
6. [Sprints](#sprints)
7. [Roles](#roles)
8. [Comments](#comments)
9. [Search](#search)
10. [Export/Import](#exportimport)
11. [Templates](#templates)
12. [Custom Fields](#custom-fields)
13. [Multiple Boards](#multiple-boards)
14. [WebSocket Events](#websocket-events)
15. [Remote Cloud Backup](#remote-cloud-backup)
16. [Local Backup](#local-backup)
17. [Runner, Fleet & Execution](#runner-fleet--execution)
18. [Error Responses](#error-responses)

---

## Authentication

Authentication is **optional and off by default** (solo/small-team focus). When the
server is started with `TACK_API_TOKEN` set, every `/api/*` route except
`GET /api/health` requires a matching bearer token:

```http
Authorization: Bearer <token>
```

Requests with a missing or wrong token receive `401 Unauthorized`. There are no
per-user accounts — all clients share the single token. See
[Administration & Security](book/src/user-guide/administration.md) for setup.

---

## Projects

### List All Projects

```http
GET /api/projects
```

**Response:**
```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "workspace_id": "default",
    "name": "My Software Project",
    "description": "A web application for task management",
    "project_type": "software",
    "vocabulary": {
      "task": "User Story",
      "sprint": "Iteration"
    },
    "workflow": {
      "statuses": [
        {
          "name": "Todo",
          "category": "backlog",
          "wip_limit": null
        },
        {
          "name": "In Progress",
          "category": "in_progress",
          "wip_limit": 3
        },
        {
          "name": "Done",
          "category": "done",
          "wip_limit": null
        }
      ],
      "transitions": null
    },
    "created_at": "2026-03-16T10:00:00Z",
    "updated_at": "2026-03-16T10:00:00Z",
    "archived": false
  }
]
```

### Create Project

```http
POST /api/projects
Content-Type: application/json

{
  "name": "My Project",
  "description": "Optional description",
  "project_type": "software"
}
```

**Project types:** `software`, `web`, `mobile`, `construction`, `personal`, `homework`, `maintenance`, `legal`, `research`, `event`, `custom`

**Response:** `201 Created` + Project object

### Get Project

```http
GET /api/projects/{id}
```

**Response:** Project object

### Update Project

```http
PATCH /api/projects/{id}
Content-Type: application/json

{
  "name": "Updated Name",
  "description": "Updated description"
}
```

**Response:** Updated Project object

### Delete Project

```http
DELETE /api/projects/{id}
```

**Response:** `204 No Content`

---

## Items

### List Items (Scoped to Project)

```http
GET /api/projects/{id}/items?status=Todo&priority=high&limit=50&offset=0
```

**Query Parameters:**
- `status` - Filter by status
- `priority` - Filter by priority (critical, high, medium, low, none)
- `item_type` - Filter by type (epic, feature, task, subtask, bug, requirement)
- `sprint_id` - Filter by sprint
- `parent_id` - Filter by parent (use "null" for top-level items)
- `limit` - Page size (default: 50, max: 100)
- `offset` - Pagination offset

**Response:** Array of Item objects

### Get Item Tree

```http
GET /api/projects/{project_id}/items/tree
```

**Response:** All of the project's items, parents with their children nested inline.

### Create Item

```http
POST /api/projects/{id}/items
Content-Type: application/json

{
  "title": "Implement user authentication",
  "description": "Add JWT-based authentication with refresh tokens",
  "item_type": "task",
  "priority": "high",
  "assignee": "Backend Team",
  "estimate": 5,
  "estimate_unit": "story_points",
  "tags": ["backend", "security"],
  "parent_id": null,
  "sprint_id": null,
  "due_date": "2026-03-20T00:00:00Z"
}
```

**Response:** `201 Created` + Item object + broadcasts `ItemCreated` event. `status` is not settable here — a new item
starts in the workflow's initial state; move it with `PATCH /api/items/{id}`.

### Get Item

```http
GET /api/items/{id}
```

**Response:** Item object

### Update Item

```http
PATCH /api/items/{id}
Content-Type: application/json

{
  "status": "In Progress",
  "priority": "critical",
  "estimate": 8
}
```

**Response:** Updated Item object + broadcasts `ItemUpdated` event

**Note:** Status changes trigger workflow validation. Auto-updates `started_at` when moving to in-progress, `completed_at` when moving to done.

### Delete Item

```http
DELETE /api/items/{id}
```

**Response:** `204 No Content` + broadcasts `ItemDeleted` event

---

## Dependencies

### List Dependencies

```http
GET /api/items/{id}/dependencies
```

**Response:**
```json
{
  "blocks": [
    {
      "id": "770e8400-e29b-41d4-a716-446655440002",
      "title": "Backend API implementation",
      "status": "In Progress"
    }
  ],
  "blocked_by": [
    {
      "id": "880e8400-e29b-41d4-a716-446655440003",
      "title": "Database schema design",
      "status": "Done"
    }
  ]
}
```

### Add Dependency

```http
POST /api/items/{id}/dependencies
Content-Type: application/json

{
  "depends_on_id": "880e8400-e29b-41d4-a716-446655440003"
}
```

**Response:** `201 Created`

**Validation:** Prevents cycles (A depends on B, B depends on A)

### Remove Dependency

```http
DELETE /api/items/{source_id}/dependencies/{target_id}
```

**Response:** `204 No Content`

---

## Attachments

### Upload Attachment

```http
POST /api/items/{id}/attachments
Content-Type: multipart/form-data

file: <binary data>
```

**Limits:** Max 50MB per file

**Response:**
```json
{
  "id": "990e8400-e29b-41d4-a716-446655440004",
  "item_id": "660e8400-e29b-41d4-a716-446655440001",
  "filename": "screenshot.png",
  "mime_type": "image/png",
  "size_bytes": 1024000,
  "storage_path": "storage/660e8400-e29b-41d4-a716-446655440001/screenshot-uuid.png",
  "uploaded_at": "2026-03-16T11:00:00Z"
}
```

### Download Attachment

```http
GET /api/attachments/{id}
```

**Response:** Binary file with proper Content-Type and Content-Disposition headers

### Delete Attachment

```http
DELETE /api/attachments/{id}
```

**Response:** `204 No Content`

---

## Sprints

### List Sprints

```http
GET /api/projects/{id}/sprints?status=active
```

**Query Parameters:**
- `status` - Filter by status (planning, active, review, closed)

**Response:** Array of Sprint objects

### Create Sprint

```http
POST /api/projects/{id}/sprints
Content-Type: application/json

{
  "name": "Sprint 1",
  "goal": "Implement authentication and user management",
  "start_date": "2026-03-16T00:00:00Z",
  "end_date": "2026-03-30T00:00:00Z"
}
```

**Response:** `201 Created` + Sprint object. A new sprint always starts in `planning` —
`status` isn't a `CreateSprint` field; move it with the status route below.

### Get Sprint

```http
GET /api/sprints/{id}
```

**Response:** Sprint object, or `404` if it doesn't exist.

### Update Sprint

```http
PATCH /api/sprints/{id}
Content-Type: application/json

{
  "name": "Sprint 1",
  "goal": "Updated goal",
  "start_date": "2026-03-16T00:00:00Z",
  "end_date": "2026-03-30T00:00:00Z"
}
```

**Response:** Updated Sprint object

This is a full replacement of `name`, `goal`, `start_date` and `end_date` — every
field is written as given, so omitting `goal`, `start_date` or `end_date` clears the
stored value rather than leaving it alone. `name` is required (1-200 characters).
`status` is not part of this route; it moves through its own endpoint below.

### Update Sprint Status

```http
PATCH /api/sprints/{id}/status
Content-Type: application/json

{
  "status": "active"
}
```

**Response:** `200 OK`. Fires the sprint's lifecycle webhooks (`planning` → `active` →
`review` → `closed`).

---

## Roles

### List Roles

```http
GET /api/projects/{id}/roles
```

**Response:** Array of Role objects

### Create Role

```http
POST /api/projects/{id}/roles
Content-Type: application/json

{
  "name": "Backend Developer",
  "description": "Handles API and database development"
}
```

**Response:** `201 Created` + Role object

### Delete Role

```http
DELETE /api/roles/{id}
```

**Response:** `200 OK`, or `404` if the role doesn't exist.

### Assign Role to Item

```http
PUT /api/items/{item_id}/roles/{role_id}
```

**Response:** `200 OK`

### Unassign Role

```http
DELETE /api/items/{item_id}/roles/{role_id}
```

**Response:** `200 OK`

---

## Comments

### List Comments

```http
GET /api/items/{id}/comments
```

**Response:** Array of Comment objects

### Create Comment

```http
POST /api/items/{id}/comments
Content-Type: application/json

{
  "content": "This needs to be prioritized for the next sprint",
  "author": "John Doe"
}
```

**Response:** `201 Created` + Comment object

---

## Search

### Global Search (All Projects)

```http
GET /api/search?q=authentication&workspace_id=default
```

**Query Parameters:**
- `q` - Search query (min 2 characters)
- `workspace_id` - Optional workspace filter

**Response:** Array of matching Item objects

**Note:** Uses SQLite FTS5 full-text search on titles, descriptions, and tags

### Project-Scoped Search

```http
GET /api/projects/{id}/search?q=authentication
```

**Response:** Array of matching Item objects within the project

---

## Export/Import

### Export Project (JSON)

```http
GET /api/projects/{id}/export?format=json
```

**Response:** Complete project snapshot including items, sprints, workflow config

### Export Project (CSV)

```http
GET /api/projects/{id}/export?format=csv
```

**Response:** CSV file with item list (id, title, type, status, priority, parent_id, created_at)

### Import Project (Placeholder)

```http
POST /api/projects/import
Content-Type: application/json

{
  "project": { /* Project data */ },
  "items": [ /* Items array */ ],
  "sprints": [ /* Sprints array */ ]
}
```

**Status:** Basic validation implemented, full import pending

### Import from CSV

```http
POST /api/projects/{id}/import-csv
Content-Type: text/csv

title,description,priority
Fix login bug,Users can't sign in on Safari,high
```

A UTF-8 CSV with a header row; only `title` is required. Recognized columns:
`title`, `description`, `type`/`item_type`, `status`, `priority`, `assignee`,
`estimate`.

**Response `200`:** `{ "created": N, "skipped": N }`

### Import from GitHub

```http
POST /api/projects/{id}/import-github
Content-Type: application/json

{
  "repo": "owner/repo",
  "token": null,
  "import_closed": false,
  "label_filter": []
}
```

Fetches issues from a GitHub repo and creates matching items; pull requests are
skipped. `token` is optional — unauthenticated calls are limited to 60 requests/hour,
a token raises that to 5,000/hour.

**Response `200`:** counts of created/skipped issues plus rate-limit remaining.

### Import from Linear

```http
POST /api/projects/{id}/import-linear
Content-Type: application/json

{
  "api_key": "lin_api_...",
  "team_id": null,
  "project_id": null,
  "import_completed": false,
  "label_filter": []
}
```

Fetches issues from Linear's GraphQL API (cursor-paginated, 50 per page) and creates
matching items. `team_id`/`project_id` narrow the fetch; `project_id` wins if both are
set.

**Response `200`:** `{ "created": N, "skipped": N }`

---

## Templates

**NEW in v1.2** - Reusable project blueprints with workflow, vocabulary, custom fields, and boards

### Create Template

```http
POST /api/templates
Content-Type: application/json

{
  "name": "Software Development (Scrum)",
  "description": "Full-featured scrum template with sprints and roles",
  "project_type": "software",
  "vocabulary": {
    "task": "User Story",
    "sprint": "Sprint",
    "epic": "Epic"
  },
  "workflow": {
    "workflow_type": "scrum",
    "statuses": [
      {"name": "Backlog", "category": "todo", "wip_limit": null, "order": 0},
      {"name": "In Progress", "category": "in_progress", "wip_limit": 5, "order": 1},
      {"name": "Done", "category": "done", "wip_limit": null, "order": 2}
    ]
  },
  "custom_fields": [
    {"name": "Customer", "field_type": "text", "required": false}
  ],
  "default_boards": [
    {"name": "Main Board", "grouping": "status", "is_default": true}
  ],
  "is_builtin": false
}
```

**Response:**
```json
{
  "id": "template-uuid",
  "name": "Software Development (Scrum)",
  "description": "Full-featured scrum template",
  "project_type": "software",
  "is_builtin": false,
  "created_at": "2026-03-17T10:00:00Z",
  "updated_at": "2026-03-17T10:00:00Z"
}
```

### List Templates

```http
GET /api/templates
GET /api/templates?project_type=software
```

**Response:**
```json
[
  {
    "id": "template-uuid",
    "name": "Software Development (Scrum)",
    "project_type": "software",
    "is_builtin": false,
    "created_at": "2026-03-17T10:00:00Z"
  }
]
```

### Get Template

```http
GET /api/templates/{id}
```

**Response:** Full template object with all configuration

### Delete Template

```http
DELETE /api/templates/{id}
```

**Note:** Built-in templates (is_builtin=true) cannot be deleted

**Response:** 204 No Content

### Create Project from Template

```http
POST /api/projects/from-template/{template_id}
Content-Type: application/json

{
  "name": "My New Project",
  "description": "Created from template"
}
```

**Response:** Complete project object with template configuration applied

**What Gets Applied:**
- Workflow configuration (statuses, WIP limits)
- Vocabulary mappings
- Custom field definitions
- Default boards

### Save Project as Template

```http
POST /api/projects/{project_id}/save-as-template
Content-Type: application/json

{
  "name": "Software Development (Scrum)",
  "description": "Snapshot of this project's workflow, vocabulary, custom fields and boards"
}
```

**Response `200`:** the created template object, or `404` if the project doesn't exist.

---

## Custom Fields

**NEW in v1.2** - User-defined metadata fields with 9 types

### Field Types

- `text` - Short text input
- `long_text` - Multi-line text area
- `number` - Numeric input
- `date` - Date picker
- `boolean` - True/false checkbox
- `select` - Single choice dropdown (requires options)
- `multi_select` - Multiple choice (requires options)
- `url` - Website link with validation
- `email` - Email address with validation

### Create Custom Field

```http
POST /api/projects/{project_id}/custom-fields
Content-Type: application/json

{
  "name": "Customer",
  "field_type": "text",
  "description": "Customer name for this work item",
  "required": false,
  "default_value": null,
  "options": null,
  "validation": null
}
```

**For select/multi_select fields:**
```json
{
  "name": "Priority Level",
  "field_type": "select",
  "options": ["Low", "Medium", "High", "Critical"],
  "required": true
}
```

**Response:**
```json
{
  "id": "field-uuid",
  "project_id": "project-uuid",
  "name": "Customer",
  "field_type": "text",
  "description": "Customer name for this work item",
  "required": false,
  "default_value": null,
  "options": null,
  "validation": null,
  "created_at": "2026-03-17T10:00:00Z",
  "updated_at": "2026-03-17T10:00:00Z"
}
```

### List Custom Fields

```http
GET /api/projects/{project_id}/custom-fields
```

**Response:**
```json
[
  {
    "id": "field-uuid",
    "project_id": "project-uuid",
    "name": "Customer",
    "field_type": "text",
    "required": false
  }
]
```

### Get Custom Field

```http
GET /api/custom-fields/{id}
```

**Response:** Full field definition object

### Update Custom Field

```http
PATCH /api/custom-fields/{id}
Content-Type: application/json

{
  "name": "Client Name",
  "description": "Updated description",
  "required": true
}
```

**Response:** Updated field object

### Delete Custom Field

```http
DELETE /api/custom-fields/{id}
```

**Note:** Cascade deletes all field values for items

**Response:** 204 No Content

### Set Field Value (Upsert)

```http
PUT /api/items/{item_id}/custom-fields/{field_id}
Content-Type: application/json

{
  "value": "Acme Corp"
}
```

**For different field types:**
- `text`, `long_text`, `url`, `email`: String value
- `number`: Numeric value
- `date`: ISO 8601 string ("2026-03-17T00:00:00Z")
- `boolean`: true/false
- `select`: Single string from options
- `multi_select`: Array of strings from options

**Response:**
```json
{
  "item_id": "item-uuid",
  "field_id": "field-uuid",
  "value": "Acme Corp",
  "updated_at": "2026-03-17T10:00:00Z"
}
```

### Get Field Value

```http
GET /api/items/{item_id}/custom-fields/{field_id}
```

**Response:** Field value object or 404 if not set

### Get All Field Values for Item

```http
GET /api/items/{item_id}/custom-fields
```

**Response:**
```json
[
  {
    "field_id": "field-uuid",
    "field_name": "Customer",
    "field_type": "text",
    "value": "Acme Corp"
  }
]
```

### Delete Field Value

```http
DELETE /api/items/{item_id}/custom-fields/{field_id}
```

**Response:** 204 No Content

---

## Multiple Boards

**NEW in v1.2** - Create unlimited boards per project with different groupings

### Grouping Options

- `status` - Group by item status (default)
- `priority` - Group by priority level
- `item_type` - Group by item type (epic, task, etc.)
- `sprint` - Group by sprint
- `assignee` - Group by assigned role
- `custom_field` - Group by custom field value (use `{"custom_field": "field-uuid"}`)

### Create Board

```http
POST /api/projects/{project_id}/boards
Content-Type: application/json

{
  "name": "Priority View",
  "description": "Items grouped by priority",
  "grouping": "priority",
  "filters": null,
  "is_default": false
}
```

**For custom field grouping:**
```json
{
  "name": "Customer Board",
  "grouping": {"custom_field": "field-uuid"},
  "is_default": false
}
```

**Response:**
```json
{
  "id": "board-uuid",
  "project_id": "project-uuid",
  "name": "Priority View",
  "description": "Items grouped by priority",
  "grouping": "priority",
  "filters": null,
  "is_default": false,
  "created_at": "2026-03-17T10:00:00Z",
  "updated_at": "2026-03-17T10:00:00Z"
}
```

**Note:** When creating a board with is_default=true, other boards are automatically unmarked

### List Boards

```http
GET /api/projects/{project_id}/boards
```

**Response:**
```json
[
  {
    "id": "board-uuid",
    "name": "Main Board",
    "grouping": "status",
    "is_default": true
  },
  {
    "id": "board-uuid-2",
    "name": "Priority View",
    "grouping": "priority",
    "is_default": false
  }
]
```

### Get Board

```http
GET /api/boards/{id}
```

**Response:** Full board object

### Update Board

```http
PATCH /api/boards/{id}
Content-Type: application/json

{
  "name": "Updated Name",
  "grouping": "sprint",
  "is_default": true
}
```

**Response:** Updated board object

### Delete Board

```http
DELETE /api/boards/{id}
```

**Note:** Cannot delete the last board or a default board without setting another as default first

**Response:** 204 No Content

### Get Board View (with Grouped Items)

```http
GET /api/boards/{id}/view
```

**Response:**
```json
{
  "board": {
    "id": "board-uuid",
    "name": "Priority View",
    "grouping": "priority"
  },
  "columns": [
    {
      "name": "Critical",
      "items": [
        {
          "id": "item-uuid",
          "title": "Fix production bug",
          "priority": "critical",
          "status": "In Progress"
        }
      ]
    },
    {
      "name": "High",
      "items": [...]
    }
  ]
}
```

**Smart Grouping:** Items are automatically grouped based on the board's grouping configuration

---

## WebSocket Events

### Event Types

**ItemCreated**
```json
{
  "event_type": "ItemCreated",
  "project_id": "550e8400-e29b-41d4-a716-446655440000",
  "item_id": "660e8400-e29b-41d4-a716-446655440001",
  "timestamp": "2026-03-16T11:00:00Z"
}
```

**ItemUpdated**
```json
{
  "event_type": "ItemUpdated",
  "project_id": "550e8400-e29b-41d4-a716-446655440000",
  "item_id": "660e8400-e29b-41d4-a716-446655440001",
  "timestamp": "2026-03-16T11:00:00Z"
}
```

**ItemDeleted**
```json
{
  "event_type": "ItemDeleted",
  "project_id": "550e8400-e29b-41d4-a716-446655440000",
  "item_id": "660e8400-e29b-41d4-a716-446655440001",
  "timestamp": "2026-03-16T11:00:00Z"
}
```

**BoardConfigUpdated**
```json
{
  "event_type": "BoardConfigUpdated",
  "project_id": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": "2026-03-16T11:00:00Z"
}
```

**Ping** (Keepalive)
```json
{
  "event_type": "Ping",
  "timestamp": "2026-03-16T11:00:00Z"
}
```

### Client Implementation

```javascript
const projectId = '550e8400-e29b-41d4-a716-446655440000';
const ws = new WebSocket(`ws://localhost:3210/api/projects/${projectId}/boards/live`);

ws.onopen = () => console.log('Connected');
ws.onerror = (error) => console.error('WebSocket error:', error);
ws.onclose = () => console.log('Disconnected');

ws.onmessage = (event) => {
  const boardEvent = JSON.parse(event.data);

  switch (boardEvent.event_type) {
    case 'ItemCreated':
    case 'ItemUpdated':
    case 'ItemDeleted':
    case 'BoardConfigUpdated':
      // Refetch board data
      fetchBoard();
      break;
    case 'Ping':
      // Keepalive, ignore
      break;
  }
};
```

---

## Remote Cloud Backup

Requires a cloud destination to be configured — either via the `TACK_BACKUP_*`
environment variables (see [configuration](../CLAUDE.md)) or at runtime through the
settings endpoint below (UI: **Settings → Cloud Backup**). The backup/list/restore
endpoints return `409 Conflict` when no destination is configured.

### Trigger Remote Backup

```http
POST /api/backup/remote
```

Creates a `.tar.zst` bundle of the SQLite database + attachments directory and uploads
it to the configured S3-compatible bucket. Prunes old backups to keep only
`TACK_BACKUP_RETENTION` (default 10) copies.

**Response `200`:**
```json
{
  "format_version": 1,
  "created_at": "2026-06-12T15:04:05+00:00",
  "migration_version": 18,
  "db_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  "install_id": "a1b2c3d4-...",
  "item_count": 42,
  "object_key": "tack/tack-backup-2026-06-12T15-04-05+00-00Z.tar.zst",
  "bundle_size_bytes": 1048576
}
```

### List Remote Backups

```http
GET /api/backup/remote
```

Returns all backups in the bucket, newest first, by reading lightweight sidecar
manifests (no full bundle download needed).

**Response `200`:** array of manifest objects (same shape as above).

### Stage Remote Restore

```http
POST /api/backup/remote/restore
Content-Type: application/json

{ "key": "tack/tack-backup-2026-06-12T15-04-05+00-00Z.tar.zst" }
```

Downloads the bundle and stages it for the next server restart. Omit `key` (or send
an empty body) to restore the latest backup automatically.

**Rejection:** returns `409 Conflict` when the snapshot's `migration_version` is
higher than the running binary's — upgrade Tack before restoring.

**Response `200`:**
```json
{
  "staged": true,
  "restart_required": true,
  "object_key": "tack/tack-backup-2026-06-12T15-04-05+00-00Z.tar.zst",
  "message": "Restore staged. Restart the server to apply."
}
```

After the server restarts, both the database and the attachments directory are
swapped atomically. The previous DB and attachments are preserved as `<path>.bak`
for manual recovery.

### Verify Remote Backup

```http
POST /api/backup/remote/verify
Content-Type: application/json

{ "key": "tack/tack-backup-2026-06-12T15-04-05+00-00Z.tar.zst" }
```

Downloads the bundle and checks its sha256, format and schema version **without**
staging anything, so the UI can preview whether a restore is safe first.

**Response `200`:** a verdict plus the manifest. `404` if no remote backups exist,
`409` if remote backup isn't configured.

### Cloud Backup Settings

```http
GET /api/settings/backup
```

Returns the effective cloud-backup configuration — env defaults with any
UI-saved overrides applied. The secret key is **never** returned; instead a
`secret_key_set` boolean indicates whether one is stored.

**Response `200`:**
```json
{
  "configured": true,
  "endpoint": "https://<account>.r2.cloudflarestorage.com",
  "bucket": "my-tack-backups",
  "region": "auto",
  "access_key": "AKIA…",
  "secret_key_set": true,
  "prefix": "tack",
  "retention": 10
}
```

```http
PUT /api/settings/backup
Content-Type: application/json

{
  "endpoint": "https://<account>.r2.cloudflarestorage.com",
  "bucket": "my-tack-backups",
  "region": "auto",
  "access_key": "AKIA…",
  "secret_key": "…",
  "prefix": "tack",
  "retention": 10
}
```

Saves the configuration to the `app_meta` table. Leave `secret_key` blank to keep
the currently stored secret (so the masked UI field can be submitted untouched).
Any blank string field clears that override and falls back to the environment
default. Returns the same masked shape as `GET`.

---

## Local Backup

A local counterpart to the remote flow above — no cloud destination required.

### Download Local Backup

```http
GET /api/backup
```

Streams a `VACUUM INTO` snapshot of the SQLite database as `application/octet-stream`,
with secrets scrubbed. `400` if the configured database isn't file-based.

### Stage Local Restore

```http
POST /api/restore
Content-Type: application/octet-stream

<sqlite file bytes>
```

Validates the uploaded file is a SQLite database and writes it as `<db-path>.restore`.
The next server startup applies the staged restore automatically. `400` if the upload
isn't a valid SQLite file, `409` if its schema is newer than the running binary's.

**Response `200`:** restore staged for next restart.

---

## Runner, Fleet & Execution

Everything for handing a board item to a coding-agent runner (Codex or Claude Code),
and for the orchestration layer that supervises those runs, lives under
`/api/executions`, `/api/runner-fleets`, `/api/runners`, `/api/agent-profiles`,
`/api/model-profiles`, `/api/local-runner*` (the runner embedded by `tack serve
--with-runner` and its stored secrets), `/api/control-planes`, `/api/fleet`,
`/api/items/{id}/dispatch`, `/api/sprints/{id}/dispatch` (+ its `dry-run`),
`/api/items/{id}/agent-activity`, `/api/projects/{id}/agent-activity`,
`/api/approvals`, `/api/projects/{id}/orch-budget`, `/api/projects/{id}/orch-policy`,
`/api/projects/{id}/orch-link`, `/api/settings/orchestration`, `/api/economics/items`,
`/api/economics/summary` and `/api/metrics` (operator surface, ordinary
`Authorization: Bearer` auth), plus the separate `/api/runner/v1/*` protocol (14 paths,
per-handler hashed runner-credential auth — deliberately never routed through operator
auth). This document does not hand-duplicate any of that surface: it changes every
wave and the schemas are fully typed in `docs/openapi.json` already. Two narrative
guides cover the execution path instead:

- **[Agent Runners & Fleet Execution](book/src/user-guide/agent-runners.md)** —
  concepts, **running an item with an agent**, **choosing a model and a provider**,
  enrolling/revoking a runner, credentials, workspace/artifact storage, the capability
  matrix, version compatibility, Docket's relationship to this surface, and — read
  before relying on any of this — exactly what runs end-to-end today versus what stops
  at enrollment in this build.
- **[Recovery Runbook](book/src/user-guide/recovery-runbook.md)** — `needs_operator`,
  lease fencing, and how an operator resolves a stuck attempt.

### Worked example: `POST /api/executions`

All thirteen required fields, run against a real `tack serve --with-runner` with a
stand-in `claude` binary standing in for a real, authenticated install — request and
response are both copied verbatim from that run, not constructed from the schema. The
narrative walkthrough (creating the referenced item/agent-profile/runner first, then
polling to completion) is in [Running an item with an
agent](book/src/user-guide/agent-runners.md#running-an-item-with-an-agent).

```http
POST /api/executions
Content-Type: application/json

{
  "item_id": "1bb00bc7-82d0-43fc-8a48-f50e1bf75b7c",
  "idempotency_key": "vi-a1-demo-003",
  "selector_kind": "exact_runner",
  "selector_id": "runr_8fa0dfb9-638f-492d-b278-ee06a789ad04",
  "agent_profile_id": "ap_91b4ea76-9f1a-4725-8a58-21a57d92572c",
  "requested_harness_kind": "claude-code",
  "requested_model_provider": "anthropic",
  "requested_model_id": "claude-sonnet-4-5",
  "agent_profile_snapshot": {
    "name": "demo-profile",
    "instructions": "Print the single word DONE and exit. Do not modify any files.",
    "tool_policy": {},
    "timeout_seconds": 120,
    "budgets": {}
  },
  "repository_snapshot": {
    "kind": "git",
    "remote": "/path/to/local/repo",
    "base_revision": "ce38796ef4f70db35eeb8d6d8b8e86477e1a883c",
    "subdirectory": null
  },
  "permission_policy": { "tools": [], "network": false },
  "budgets": {},
  "environment": {},
  "metadata": {},
  "timeout_seconds": 120
}
```

**Response:** `200 OK`
```json
{
  "protocol_version": 1,
  "request_id": "exec_1ecec9359d33039dedfd637df7fa4af87dca282c23437509516332e0adf243aa",
  "state": "queued",
  "replayed": false
}
```

Polling `GET /api/executions/{request_id}/attempts` after this, on a live embedded
runner, reached `"state": "succeeded"` in the same real run — see the agent-runners
walkthrough linked above for the full attempt body.

`resolve_decision` (`POST /api/attempts/{attempt_id}/decisions/{decision_id}/resolve`)
requires the separate `TACK_EXECUTION_DECISION_TOKEN` secret, fail-closed when unset —
see [Authentication](#authentication) for how this differs from the ordinary API
token.

---

## Error Responses

### Standard Error Format

```json
{
  "error": "Item not found",
  "details": "No item exists with id: 660e8400-e29b-41d4-a716-446655440001"
}
```

### HTTP Status Codes

- `200 OK` - Successful GET/PATCH
- `201 Created` - Successful POST
- `204 No Content` - Successful DELETE
- `400 Bad Request` - Invalid input, validation error
- `404 Not Found` - Resource not found
- `409 Conflict` - Workflow validation error, WIP limit exceeded
- `500 Internal Server Error` - Server error

### Common Errors

**Workflow Validation Error (409 Conflict)**
```json
{
  "error": "Invalid status transition",
  "details": "Cannot move from 'Permit' to 'Handover' in construction workflow"
}
```

**WIP Limit Exceeded (409 Conflict)**
```json
{
  "error": "WIP limit exceeded",
  "details": "Column 'In Progress' has limit of 3, currently has 3 items"
}
```

**Dependency Cycle Detected (409 Conflict)**
```json
{
  "error": "Dependency cycle detected",
  "details": "Adding this dependency would create a cycle: A → B → C → A"
}
```

---

## Rate Limiting

**Current Status:** No rate limiting implemented

For production deployments, consider adding rate limiting at the reverse proxy level (e.g., Caddy, nginx).

---

## CORS

CORS is controlled by `TACK_ALLOWED_ORIGINS` — a comma-separated allow-list of
origins permitted to call the API from a browser. It defaults to
`localhost:8080,127.0.0.1:8080`.

The bundled SPA is served **same-origin** by the same binary, so it does not need
CORS configured at all. `TACK_ALLOWED_ORIGINS` only matters when a *separate-origin*
browser client calls the API. (There is no `TACK_CORS_ORIGIN` variable.)

---

## API Versioning

**Current Status:** No versioning (v1 implicit)

Future API changes will use `/api/v2` prefix for breaking changes.

---

## Health Check

```http
GET /api/health
```

**Response:**
```json
{
  "status": "ok",
  "version": "0.1.0-beta.7",
  "migrations_applied": 18
}
```

`version` is the running binary's `CARGO_PKG_VERSION`; `migrations_applied` is the
number of rows in the `_migrations` table (i.e. how many schema migrations have run).

---

## Debug Endpoints (Development Only)

### System Info

```http
GET /api/debug/info
```

**Response:**
```json
{
  "version": "1.0.0",
  "database": "sqlite:tack.db",
  "log_level": "debug",
  "storage_dir": "./storage"
}
```

### Database Stats

```http
GET /api/debug/db-stats
```

**Response:**
```json
{
  "projects": 5,
  "items": 127,
  "sprints": 12,
  "attachments": 34,
  "database_size_mb": 15.2
}
```

---

## Examples

See [API-EXAMPLES.md](./API-EXAMPLES.md) for complete workflow examples with curl commands.
