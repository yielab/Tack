# Architecture & Implementation Notes

Crate responsibilities, design patterns, and implementation details of record.
This file is the authority — CLAUDE.md keeps only the condensed map. The
mdBook's crate-tour predates `tack-orch`/`tack-runner` and has not caught up,
so this file is the more current source on anything the two disagree about.

**Project structure:**
```
crates/
├── tack-core/     Pure business logic (no I/O)
├── tack-db/       SQLite persistence layer
├── tack-orch/     Agent-fleet orchestration client (ControlPlane trait, reconciler)
│                  + the neutral runner-v1 execution domain (execution/), the
│                  deterministic scheduler and model-policy resolver, and the
│                  execution domain's own retention/observability/provenance
│                  background modules
├── tack-api/      Axum HTTP server + WebSocket (library; pub fn serve)
├── tack-runner/   Pull-based execution runner — its own binary; owns local
│                  credentials, workspace, journal and the harness subprocess
└── tack-cli/      The single `tack` binary — runs the server (tack serve) and the CLI client

frontend/
├── src/
│   ├── app/         Root App/Layout components and routes.tsx
│   ├── features/    One directory per feature (board, list, sprints, fleet,
│   │                agents, provisioning, economics, settings, …)
│   ├── shared/      Cross-feature code: api/ (generated client + schema),
│   │                realtime/, orch/, execution/, ui/, state/, vocab/, keyboard/
│   └── test/        Vitest setup
├── e2e/             Playwright specs
├── public/          Static assets
└── package.json     Frontend dependencies (SolidJS, Vite, Tailwind v4)

docs/                Documentation
├── API-REFERENCE.md Complete API documentation
├── TESTING.md       Testing guide
└── *.md            Various guides
```

## Crate Boundaries & Responsibilities

**tack-core** (pure, zero I/O):
- Domain models: `Project`, `Item`, `Sprint`, `Role`, `Comment`, `Dependency`, etc.
- Workflow engine: validates transitions, enforces WIP limits, provides presets
- Vocabulary system: customizable term mapping per project
- Dependency graph: DAG with cycle detection (DFS-based)
- Error types: typed domain errors (`CoreError`)

**tack-db**:
- SQLite via `sqlx` (async)
- 62 migrations (`grep -oE '"[0-9]{3}_[a-zA-Z0-9_]+"' crates/tack-db/src/migrations.rs | sort -u | wc -l`; the live count is `GET /api/health`'s `migrations_applied`) with FTS5 full-text search on items
- Repository pattern: CRUD for all entities in `repo/` submodules
- Auto-runs migrations on startup
- Database is created automatically if missing

**tack-orch** (agent-fleet orchestration client + the neutral runner-v1 execution domain — depends only on `tack-core` and `tack-db`; must never depend on `tack-api`, the dependency points inward, `tack-api` depends on this crate to spawn the reconciler, run the scheduler/retention/observability tasks, and expose the orchestration and execution routes):

*Agent-fleet orchestration (off by default, gated behind `TACK_ORCH_ENABLE`):*
- Defines the `ControlPlane` trait (`health`, `status`, `metrics`, `list_runs`, `list_approvals`, `list_tasks`, `traces`, plus gated write/dispatch methods) — the seam that makes Tack a factory control center rather than a docket-specific dashboard. `docket` (`adapters::docket::DocketAdapter`) is the only implementor today.
- `reconciler.rs`: one `tokio` task per registered control plane, polling `/health` + `/status.json` on a jittered interval and driving a `healthy` → `degraded` (3 consecutive failures) → `unreachable` (10) health state machine, persisted to `control_planes`
- Remote enums (`RunState`, `RunSource`, `TaskStatus`, `ApprovalState`) all carry an `Unknown(String)` fallback so a docket upgrade degrades gracefully instead of failing a poll
- `adapters::prometheus`: dependency-free `/metrics` text-exposition parser, reused by any future metrics ingestion

*The runner-v1 execution domain and its supporting modules:*
- `execution/`: the neutral, transport-free runner-v1 protocol domain (request/attempt/event/artifact/decision types) that both `tack-api`'s handlers and `tack-runner` build on
- `scheduler/`: a pure, I/O-free decision library — given a candidate pool of runners (health/capacity/labels/declared harness and model support) and a request (exact runner or fleet selector, required harness, optional provider/model, priority), it returns a selected runner or a typed reason none qualify; it never grants the authoritative lease, only the repository's fenced claim does that. `select::select_runner` decides one request; `batch::schedule` orders several by priority then FIFO fairness; `wiring` is the live `tack-db`-backed caller
- `model_policy/`: deterministic model-selection precedence — request override → agent-profile default → project default → fleet default → auto-select when nothing is configured — pure resolution plus a `wiring` module that fetches each tier's configured default
- `execution_retention.rs`: a cancellable background sweep that purges stale/terminal execution rows, mirroring the reconciler's own retention sweep but with an injectable clock and a real stop signal
- `execution_observability.rs`: a periodic, **id-free** fleet health snapshot (runner/queue/lease/event state counts, bounded to the domain's small closed vocabularies) plus stuck/ambiguous alerts — never labeled by attempt, request or runner id
- `usage_provenance.rs`: compares an execution request's resolved model against what the attempt actually ran on, and keeps runner-observed wall-clock cost structurally separate from harness/vendor-reported token and dollar usage
- Every dollar-valued field across this crate is named `*_usd_estimated` — token counts are the primary, trustworthy measure; docket reports no real spend, so any cost figure downstream is a derived estimate; absent usage is a typed "not measured", never a fabricated `0`. See `docs/book/src/developer/orchestration.md`

**tack-api** (library — does not build its own binary):
- Axum HTTP server with 97 documented paths (`python3 -c "import json; print(len(json.load(open('docs/openapi.json'))['paths']))"`) + 1 WebSocket not in the spec — includes the orchestration endpoints gated behind `TACK_ORCH_ENABLE`, the operator execution/fleet surface, and the 14 `/api/runner/v1` runner-protocol paths. `docs/openapi.json` is generated and authoritative; re-run the count above rather than trusting this number after the next handler change.
- **Two authentication surfaces, separated structurally.** Operator routes live under `/api` behind `require_token`. Runner routes are nested as a _sibling_ of `/api` on the outer router, so they never traverse the operator auth layer at all — deliberately not an exemption-list entry, which a later edit could quietly widen. Each runner handler authenticates its own hashed bearer credential via `handlers/runner_protocol/runner_auth.rs`.
- `x-tack-principal` is **overwritten from server config** by `middleware::inject_operator_principal` and never read from the request. Operator idempotency is scoped by principal, so a trusted header would let one caller collide with another's requests.
- Server entry point exposed as `tack_api::serve()` (in `server.rs`)
- WebSocket support for real-time board updates
- Request handlers in `handlers/` (per entity)
- Config loading: TOML + env vars
- Error mapping: `CoreError` → HTTP status codes
- Debug endpoints: `/api/health`, `/api/debug/info`, `/api/debug/db-stats`
- File upload support: multipart/form-data (max 50MB)
- Export functionality: JSON and CSV formats
- `orch_store.rs`: wires `tack-orch`'s `ControlPlaneStore` trait to the real `Repository` + a `kind`-dispatched adapter constructor; spawns the reconciler from `server.rs` behind `config.orch_enable`

**tack-runner** (the pull-based execution runner — its own binary, separate from `tack`):
- Polls the Tack API's `/api/runner/v1` surface: enroll, refresh capabilities, claim, heartbeat, accept, start, stream events, poll decisions, submit artifacts, complete, and report cancellation/recovery observations
- Owns everything the API must not: local vendor credentials, the isolated per-attempt workspace/worktree, the owner-only TOML journal written **before** spawn, and the harness subprocess itself
- `harness/` holds the adapter layer — `process.rs` (bounded output capture, timeouts, process-group cancellation), `event_sink.rs` (backpressure), `redact.rs`, `artifact.rs`, and one module per harness: `codex.rs`, `claude_code.rs`. The harness vocabulary itself is open (`HarnessKind::Other(String)`) — this crate ships adapters for these two, a runner may still report any kind string
- Two traits: `client::engine::HarnessAdapter` (per-attempt lifecycle — `validate`/`start`/`cancel`/`wait`/`reconcile`) and `harness::HarnessProbe` (version/capability discovery, which needs no claimed attempt). `AdapterRegistry` implements `HarnessAdapter` by dispatching on the requested harness kind, so the engine takes exactly one adapter type
- **Capabilities are honest or the adapter is rejected.** `AdapterRegistry::register_probe` refuses any probe claiming `Supported` cancellation, because every harness's shell tool spawns its subprocess in a new session outside the runner's process group — verified with `ps` against real `claude`. Cancellation is `Advisory` everywhere; the scheduler must read the capability snapshot, never assume
- Live harness tests are opt-in (`#[ignore]` + a PATH check) and never required in CI; the shared fake binary at `harness/fixtures/fake_harness.sh` is the always-runnable path, driven by `TACK_FAKE_HARNESS_MODE`

**tack-cli** (the single `tack` binary):
- `tack` with no subcommand (or `tack serve`) starts the server + web UI via `tack_api::serve()` — the primary, UI-first entry point
- CLI client using `clap` (`Commands` enum in `main.rs`): board/item basics — `init`, `projects`, `add`, `list`, `move`, `board`, `branch`, `search`, `config`, `completions`; entity subcommand groups — `sprint`, `template`, `role`, `comment`, `field`; backup — `backup`, `backups`, `restore`; and the execution/runner surface, each its own subcommand group — `execution` (create/list/get/cancel/reconcile requests), `fleet` (runner fleets), `runner` (enroll/revoke/start), `service` (manage `tack` as a background service — a systemd user unit on Linux, a launchd agent on macOS), `agent-profile`, `model-profile`
- `tack mcp` — Model Context Protocol server over stdio (hand-rolled JSON-RPC 2.0 in `mcp.rs`); proxies tool calls to a running server over HTTP so workflow rules apply. See `docs/MCP.md`
- `tack branch <item-id>` — derives/creates a git branch from an item (`git.rs`)
- Client commands talk to the server over HTTP (blocking `reqwest`); never open the DB directly

**frontend** (SolidJS + TypeScript):
- Responsive SPA on a **two-axis design-token system**: mode (`.dark` class) × palette (`data-palette` attr) → Teal/Clay/Graphite × light/dark, switched from the sidebar footer. All colors come from `--color-*` tokens in `index.css`; components never use raw hex. WCAG AA, axe-gated in CI. Fonts: Hanken Grotesk + JetBrains Mono (self-hosted via `@fontsource`). See `docs/book/src/developer/frontend.md`.
- **Board view** with HTML5 drag-and-drop (visual Kanban-style)
- **List view** with sortable table, filtering, and bulk operations
- WebSocket integration for real-time updates
- Optimistic UI updates (instant feedback)
- Keyboard shortcuts + command palette (Ctrl+K)
- Global search (Ctrl+/)
- Toast notifications
- Skeleton loading screens
- Status: 100% complete (all core features working)

## Key Design Patterns

**Universal Item Model**: All work units (epics, features, tasks, bugs, etc.) share the same `Item` struct. The `item_type` field and project vocabulary determine how they're labeled and displayed.

**Workflow Engine**: Each project has a `WorkflowConfig` defining:
- Status columns (name, category, WIP limit, order)
- Optional explicit transitions (e.g., construction workflow enforces linear progression)
- Presets: `scrum_workflow()`, `kanban_workflow()`, `simple_workflow()`, `construction_workflow()`

**Vocabulary Mapping**: Projects store a `VocabularyMap` to rename terms:
```rust
{
  "task": "Work Order",
  "sprint": "Phase",
  "epic": "Building"
}
```

**Dependency Graph**: DAG-based system prevents cycles. Uses adjacency lists for both forward (`edges`) and reverse (`reverse_edges`) lookups. Validates new edges before insertion.

**Project Types**: Each `ProjectType` (software, construction, personal, etc.) auto-selects default workflow and vocabulary. Users can fully customize after creation.

## Database Schema Highlights

- **62 migrations** tracked in the `_migrations` table — see `GET /api/health`'s `migrations_applied` for the live count rather than trusting this number (039–048 added the ten neutral execution tables; 049–061 refine execution replay, recovery and attempt-start facts; 062 adds project-level default-model selection)
- Migrations are transactional with ordered-prefix and checksum enforcement; 037/038 do a copy/verify/swap rebuild guarded by a `VACUUM INTO` snapshot
- **`BEGIN IMMEDIATE` is mandatory for read-then-write transactions.** A deferred transaction that reads then writes deadlocks under concurrency — two callers both upgrade from reader to writer and SQLite returns `SQLITE_LOCKED`. 16 sites in `repo/execution.rs` hit this (`grep -c 'begin_with("BEGIN IMMEDIATE")' crates/tack-db/src/repo/execution.rs`); each was stress-tested before and after the fix. Write-first methods are fine as-is and were deliberately left deferred. Note the shared in-memory test harness can _mask_ these races — prove any new concurrency test load-bearing against a file-backed DB by reverting the fix and watching it fail
- **FTS5 virtual table** (`items_fts`) for full-text search across titles, descriptions, tags
- **Triggers** maintain FTS index on INSERT/UPDATE/DELETE
- **Foreign keys** enforce referential integrity (e.g., items → projects, items → sprints)
- **Indexes** on common queries: project_id, status, priority, parent_id
- **Attachments table** with file metadata (filename, mime_type, storage_path, size)

## API Endpoint Structure

All routes follow RESTful conventions:

- `/api/projects` — CRUD for projects (5 endpoints)
- `/api/projects/{id}/boards` — Multiple boards per project (CRUD + view)
  - `GET /api/projects/{id}/boards/live` — **WebSocket** for real-time updates
- `/api/projects/{id}/export` — Export to JSON/YAML/CSV (1 endpoint)
- `/api/projects/import` — Import from JSON or YAML (1 endpoint)
- `/api/projects/{id}/items` — Items scoped to project (3 endpoints)
- `/api/items/{id}` — Individual item operations (3 endpoints with WebSocket broadcasting)
- `/api/items/{id}/dependencies` — Dependency management (3 endpoints)
- `/api/items/{id}/attachments` — File attachments (2 endpoints)
- `/api/attachments/{id}` — Download/delete attachments (2 endpoints)
- `/api/projects/{id}/sprints` — Sprint management (4 endpoints)
- `/api/projects/{id}/roles` — Role/specialty management (5 endpoints)
- `/api/items/{id}/comments` — Comments on items (2 endpoints)
- `/api/projects/{id}/search` — Full-text search within project (1 endpoint)
- `/api/search` — **Global search** across all projects (1 endpoint)
- `/api/projects/{id}/import-github` — GitHub Issues import (1 endpoint; `owner/repo` or full URL, optional PAT, label filter, PR-skipping, cursor pagination). Imported items are linked in the `github_links` table so completing them pushes a close back to GitHub when `TACK_GITHUB_TOKEN` is set (push-only). See `docs/GITHUB-SYNC.md`
- `/api/projects/{id}/import-linear` — Linear import (1 endpoint; Linear API key, optional team/project filter, label filter, priority mapping, cursor pagination)
- `/api/backup`, `/api/restore` — Local DB backup download / staged restore (2 endpoints)
- `/api/backup/remote` (POST/GET), `/api/backup/remote/restore` — Cloud (S3-compatible) backup, list, and staged restore (3 endpoints)
- `/api/settings/backup` (GET/PUT) — Read/update the UI-editable cloud-backup config; secret key is write-only (returned as a `secret_key_set` boolean)
- `/api/control-planes` (GET/POST), `/api/control-planes/{id}` (GET/PATCH/DELETE), `/api/projects/{id}/orch-link` (GET/PUT), `/api/fleet` (GET) — Agent-fleet orchestration (8 endpoints; all gated behind `TACK_ORCH_ENABLE`, 404 when unset). Control-plane token is write-only (`token_set` boolean). See `docs/book/src/developer/orchestration.md` and `docs/book/src/user-guide/orchestration.md`
- `/api/executions`, `/api/runner-fleets`, `/api/runners/*`, `/api/agent-profiles`, `/api/model-profiles` — **Operator** execution surface (create/list/get/cancel/requeue, fleet and profile management, runner enrollment and revocation). Under operator auth. Raw enrollment tokens are returned exactly once at issue time and only their SHA-256 hash is stored
- `/api/runner/v1/*` — **Runner protocol**, 14 paths under a separate credential: `enroll`, `refresh`, `claim`, `heartbeat`, and per-attempt `accept`, `start`, `events`, `decisions`, `decisions/poll`, `artifacts`, `artifacts/{artifact_id}/content` (PUT — the content upload), `completion`, `cancellation-observation`, `recovery-observation`. Every attempt-scoped mutation validates runner identity + attempt id + current fencing token; a stale fence returns the stable `stale_lease` error and writes nothing

Query parameters support filtering, pagination, and search.

## Important Implementation Details

### Workflow Validation

When updating an item's status:
1. Check if both statuses exist in project workflow
2. If explicit transitions defined, validate the move is allowed
3. Check WIP limit for target column (before adding)
4. Update `started_at` when moving to in-progress category
5. Update `completed_at` when moving to done category
6. **Auto-propagate parent status** if item has a parent and all siblings are complete

**Example:** Construction projects have strict transitions (Permit → Procurement → Build → Inspect → Handover). Jumping from Permit to Handover is rejected.

### Auto-Status Propagation

When a child item is moved to a "done" status, the system automatically checks if all siblings are also complete. If so, the parent item is automatically updated to "done" as well. This cascades up the hierarchy.

**Implementation:**
- Repository method: `check_and_update_parent_status(parent_id, completed_status)`
- Triggered in `update_item` handler after status change
- Only activates when moving to a status with `StatusCategory::Done`
- Errors are silently ignored (best-effort feature)

**Example workflow:**
1. Epic "User Auth" has 3 child tasks
2. Complete task 1 → no parent update (still 2 incomplete)
3. Complete task 2 → no parent update (still 1 incomplete)
4. Complete task 3 → **parent epic auto-completes** ✓

### Dependency Cycle Detection

Before creating a dependency:
1. Check for self-reference (`source == target`)
2. Build adjacency graph from existing dependencies
3. Run DFS from target to see if it can reach source
4. Reject if cycle detected

The graph is reconstructed on each validation. For large dependency sets, consider caching.

### Sprint Status Lifecycle

Sprints have four states:
- `Planning` → `Active` → `Review` → `Closed`

Items can only be assigned to active or planning sprints (enforced in handlers).

### Tags & Search

- Tags are stored as JSON arrays in SQLite
- FTS5 index includes tags, titles, descriptions
- Search via `/api/projects/{id}/search?q=term` uses FTS5 `MATCH`

### File Attachments

- Upload via multipart/form-data to `/api/items/{id}/attachments`
- Max file size: 50MB (configurable in handler)
- Files stored in `TACK_STORAGE_DIR` organized by item ID
- Unique filenames prevent collisions (UUID-based)
- Download includes proper Content-Type and Content-Disposition headers
- Metadata stored in database: filename, mime_type, storage_path, size_bytes

### Export/Import

- **JSON Export**: Complete project snapshot — project, items, sprints, dependencies, metadata
  - `GET /api/projects/{id}/export?format=json`
- **YAML Export**: The same snapshot, serialized as YAML. YAML is a JSON superset, so both formats decode into the same intermediate value on import
  - `GET /api/projects/{id}/export?format=yaml`
- **CSV Export**: Simplified item list for spreadsheet import
  - `GET /api/projects/{id}/export?format=csv`
  - Includes: id, title, type, status, priority, parent_id, created_at
- **Import**: `POST /api/projects/import` (`handlers/export.rs::import_project`) accepts the same JSON-or-YAML snapshot shape produced by export, dispatched on `Content-Type`. Creates a new project, imports sprints then items (two passes — create, then wire up `parent_id`) then dependencies, preserves each item's original `source` trust marker, and restores the source project's workflow and vocabulary. If any step fails, the created project is deleted and the error is returned — nothing is left half-imported.

### WebSocket Real-Time Updates

- **Endpoint**: `GET /api/projects/{id}/boards/live` (WebSocket upgrade)
- **Purpose**: Real-time board state updates for live collaboration
- **Implementation**:
  - Uses Tokio broadcast channel (100 message capacity)
  - AppState contains `broadcast_tx: broadcast::Sender<BoardEvent>`
  - Each WebSocket connection subscribes to the broadcast channel
  - Events are filtered by project_id before sending to client
- **Event Types**:
  - `ItemCreated` - New item added to board
  - `ItemUpdated` - Item status/details changed
  - `ItemDeleted` - Item removed
  - `BoardConfigUpdated` - WIP limits or columns changed
  - `SprintUpdated` - Sprint status changed
  - `Ping` - Keepalive (sent to all)
- **Usage**: Frontend connects via WebSocket, receives JSON events, updates UI reactively
- **Broadcasting**: Other handlers call `websocket::broadcast_event()` to notify all subscribers

### Testing Strategy

- **Unit tests** in `tack-core` for business logic (workflow, dependencies, vocabulary)
- **Integration tests** in `tack-db` for repository operations (require SQLite)
- **Handler tests** in `tack-api` use in-memory databases
- **Frontend unit tests** in `frontend/src/**/*.test.tsx` (Vitest + jsdom)
- **End-to-end tests** in `frontend/e2e/` (Playwright): cross-browser smoke, user
  journeys, axe accessibility scans, and API wire-contract checks. Test setup
  talks to the API directly; the browser exercises the SPA via the proxy.
- **Security**: `cargo audit` + `npm audit` in CI; justified advisory exceptions
  in `.cargo/audit.toml`. **Performance**: k6 baseline in `tests/load/`.
- **Integration gates** in `crates/tack-api/tests/wave2_gate.rs` — deliberately import no
  test infrastructure from any other test module and drive the real `build_router`,
  because one area's own green tests are not evidence that the integrated system works.

**A test that asserts a status code has usually not proved the claim.** The recurring failure
in this codebase has been tests that pass while proving something weaker than their name says:
a 413 asserted without checking the DB was unwritten; a concurrency test that caught `Err(_)`
and retried sequentially, so it silently stopped being concurrent; a capability reported
`Supported` with no implementation behind it. When adding a test for a "writes nothing" or
"rejects before X" claim, assert the absence directly — row counts, an untouched checkpoint,
empty bookkeeping — and prove the test is load-bearing by reverting the fix and watching it
fail.

Use `assert_matches!` macro for enum matching in tests. When changing an API
response shape, update both the Rust handler and the matching mock in the
frontend unit/E2E tests (e.g. the `GET /items/{id}` detail envelope).


## Common Patterns When Adding Features

### Adding a New Entity

1. Define model in `tack-core/src/models.rs`
2. Add migration in `tack-db/src/migrations.rs`
3. Create repository module in `tack-db/src/repo/`
4. Add handler in `tack-api/src/handlers/`
5. Register routes in `tack-api/src/router.rs`
6. Add DTOs for create/update operations

### Extending Workflow Logic

1. Add logic to `tack-core/src/workflow.rs`
2. Write unit tests in the same file
3. Update handlers to call new validation/logic
4. No database changes needed (workflow is JSON in DB)

### Adding a New Project Type

1. Add variant to `ProjectType` enum in `models.rs`
2. Create workflow preset in `workflow.rs` (e.g., `education_workflow()`)
3. Create vocabulary preset in `vocabulary.rs`
4. Update `workflow_for_type()` match statement

## Troubleshooting

**Migration errors on startup:**
- Check `_migrations` table: `SELECT * FROM _migrations`
- Manually delete failed migration record if needed
- Restart server to retry

**Database locked errors:**
- SQLite only supports one writer at a time
- Check if another process has the DB open
- Use `?mode=rwc` in connection string (already default)

**FTS5 not found:**
- SQLite must be compiled with FTS5 support
- Check with: `sqlite3 tack.db "PRAGMA compile_options;"`

