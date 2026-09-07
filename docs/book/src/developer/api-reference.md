# API Reference

**Base URL:** `http://127.0.0.1:3210/api`  
**WebSocket:** `ws://127.0.0.1:3210/api/projects/{id}/boards/live`

The full endpoint reference lives in [docs/API-REFERENCE.md](../../../API-REFERENCE.md). This
page summarizes the endpoint surface for quick orientation.

---

## Authentication

When `TACK_API_TOKEN` is set, all requests need:

```
Authorization: Bearer <token>
```

`GET /api/health` is always public. Without a token configured, no auth is required.

---

## Endpoints — the OpenAPI spec is the source of truth

The complete, always-current endpoint surface (paths, methods, parameters,
request/response schemas, and error shapes) is the machine-generated OpenAPI 3.1
contract, **not a hand-maintained list**:

- **Live:** `GET /api/openapi.json` from a running server.
- **In-repo:** [`docs/openapi.json`](../../../openapi.json), regenerated from the
  handler annotations and checked in.

Both are gated in CI: the Rust *OpenAPI contract drift gate* fails the build if the
committed spec falls out of sync with the code, and the frontend *OpenAPI TS types
drift gate* fails if the generated client types (`schema.gen.ts`) drift from the
spec. That chain — handlers → `docs/openapi.json` → `schema.gen.ts` — is why this
page no longer enumerates endpoints by hand: any such list would silently rot, which
is exactly the failure this contract eliminates.

To browse the spec interactively, load `docs/openapi.json` into any OpenAPI viewer
(Redocly, Scalar, Swagger Editor, or `npx @redocly/cli preview-docs docs/openapi.json`),
or count the surface yourself:

```sh
python3 -c "
import json
spec = json.load(open('docs/openapi.json'))
methods = {'get','put','post','delete','options','head','patch','trace'}
ops = sum(1 for item in spec['paths'].values() for k in item if k in methods)
print(f\"{len(spec['paths'])} paths, {ops} operations\")
"
```

There is also **1 WebSocket** endpoint (`/api/projects/{id}/boards/live`), which —
like the multipart upload — is documented in prose below rather than in the spec.
No fixed operation count is given here: it changes with every endpoint added, and
a hand-copied number is exactly the kind of drift this page exists to avoid — trust
the command above, or the spec, over anything written here.

---


## WebSocket Events

Connect to `ws://127.0.0.1:3210/api/projects/{id}/boards/live` with a standard WebSocket client. Events are JSON objects:

| Type | Payload |
|---|---|
| `ItemCreated` | Full item object |
| `ItemUpdated` | Full item object |
| `ItemDeleted` | `{"id":"…"}` |
| `BoardConfigUpdated` | Updated board config |
| `SprintUpdated` | Full sprint object |
| `AgentRunUpdated` | `{project_id, item_id, run_id, state}` — an orchestrated agent run mirrored from a control plane (docket) changed state, or was newly attributed to this item |
| `ApprovalPending` | `{project_id, item_id, token, action}` — a mirrored approval transitioned into `pending`; not emitted on every re-poll or on a grant/deny decision |
| `Ping` | `{}` — keepalive, sent periodically |

---

## Error Responses

All errors return JSON:

```json
{"error": "Item not found"}
{"error": "WIP limit exceeded for column 'In Progress'"}
{"error": "Transition from 'Permit' to 'Handover' is not allowed"}
```

| Status | Cause |
|---|---|
| `400` | Bad request (validation failure) |
| `401` | Missing or invalid API token |
| `404` | Resource not found |
| `409` | Conflict (e.g., dependency cycle detected) |
| `422` | Workflow transition rejected |
| `500` | Internal server error |

---

## Pagination and Filtering

`GET /projects/{id}/items` supports:

| Param | Type | Description |
|---|---|---|
| `status` | string | Filter by column name |
| `priority` | string | `high`, `medium`, `low` |
| `item_type` | string | `task`, `epic`, `bug`, etc. |
| `assignee` | string | Assignee string match |
| `sprint_id` | UUID | Items in a specific sprint |
| `parent_id` | UUID | Children of a specific item |
| `limit` | int | Page size (default 50) |
| `offset` | int | Pagination offset |
