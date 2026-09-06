# Crate Tour

This chapter walks through each of the six Rust crates in the main workspace in depth: what it owns, what it deliberately does not own, the key files, and the patterns worth understanding. `tack-core`, `tack-db`, `tack-api`, and `tack-cli` predate the Part III runner-fleet cycle; `tack-orch` and `tack-runner` were added by it. A seventh crate, `tack-desktop`, sits outside that workspace by design — its own section at the end of this chapter says why.

> The SolidJS web UI in `frontend/` is covered separately in
> [Frontend & Design System](frontend.md) — structure, the design-token system,
> and the `shared/ui` component kit.

---

## `tack-core`

**Lives in:** `crates/tack-core/src/`

**Owns:** domain models, workflow engine, vocabulary system, dependency DAG, typed error enum.

**Does not own:** anything that performs I/O. No `sqlx`, no `reqwest`, no file operations, no `tokio`. This is enforced by the `Cargo.toml` — the crate has no async runtime dependency at all.

---

### `models.rs`

The single source of truth for every domain struct. Notable types:

- `Item` — the universal work unit. Fields include `item_type: ItemType`, `status: String`, `parent_id: Option<Uuid>`, `tags: Vec<String>`. The `status` field is a plain string rather than an enum because valid statuses are project-specific configuration, not compile-time constants.
- `Project` — carries the `workflow: WorkflowConfig` and `vocabulary: VocabularyMap` inline. Both are serialised to JSON when stored in SQLite.
- `ItemType` — an enum with `Epic`, `Feature`, `Task`, `Subtask`, `Bug`, `Requirement`, and `Custom(String)`. The `Custom` variant allows ad-hoc item types without a code change.
- `CreateItem`, `UpdateItem`, `CreateProject`, `UpdateProject`, etc. — all DTOs used for both API deserialization and repository function parameters. Keeping them in `tack-core` means the API and CLI reference the same validated shapes.

Validation constraints (length, range) are expressed via the `validator` crate's derive macros directly on the DTO structs. The API handlers call `.validate()?` before doing anything with the data.

---

### `workflow.rs`

Defines `WorkflowConfig`, which is what gets stored as JSON per project.

```rust
pub struct WorkflowConfig {
    pub workflow_type: WorkflowType,
    pub statuses: Vec<StatusDef>,
    pub transitions: Option<Vec<Transition>>,
}
```

Each `StatusDef` has a `name`, a `category` (`Todo`, `InProgress`, or `Done`), an optional `wip_limit`, and an `order` integer for display sorting.

**`validate_transition(from, to)`** is the central enforcement function. It:
1. Checks that both names exist in the status list.
2. If `transitions` is `Some(list)`, checks that the pair appears in the list.
3. Returns `Ok(())` or `Err(CoreError::InvalidTransition { from, to })`.

If `transitions` is `None`, any move between two known statuses is allowed. This is the default for Scrum and Kanban workflows.

**`check_wip_limit(status, current_count)`** looks up the `StatusDef` for the target column and returns `Err(CoreError::WipLimitExceeded { ... })` if `current_count >= limit`.

**Preset functions** produce ready-made configs for each domain:

| Function | Type | Statuses | Transitions |
|---|---|---|---|
| `scrum_workflow()` | `Scrum` | Backlog, To Do, In Progress (WIP 5), In Review (WIP 3), Done | None (open) |
| `kanban_workflow()` | `Kanban` | Queue, In Progress (WIP 3), Review (WIP 2), Done | None (open) |
| `simple_workflow()` | `Simple` | To Do, Doing, Done | None (open) |
| `construction_workflow()` | `Construction` | Permit, Procurement, Build, Inspect, Handover | Explicit linear list |

`workflow_for_type(project_type)` maps each `ProjectType` to the right preset. Adding a new project type means adding a variant to the `ProjectType` enum, a preset function, and a match arm here.

The test suite in this file (29 tests) covers initial status selection, transition validation for open and constrained workflows, WIP limit edge cases, done-status detection, and parent-completion logic — all without any database or async runtime.

---

### `vocabulary.rs`

`VocabularyMap` is a type alias for `HashMap<String, String>`. It maps canonical keys (like `"task"`, `"sprint"`, `"epic"`) to their display labels for a given project.

**`resolve(vocab, key)`** looks up a key in the project's vocabulary, falls back to the default vocabulary, and finally falls back to the key itself. This means partial vocabularies work fine — a construction project only needs to override the terms it cares about.

**`vocabulary_for_type(project_type)`** provides preset vocabularies. The construction preset, for example, maps:

- `"task"` → `"Work Order"`
- `"sprint"` → `"Phase"`
- `"epic"` → `"Building"`
- `"bug"` → `"Defect"`

**`validate(vocab)`** ensures that all keys in a user-supplied vocabulary are from the recognised list (`VOCABULARY_KEYS`). Unknown keys return `Err(CoreError::InvalidVocabularyKey(...))` — this prevents typos from silently creating orphaned entries.

---

### `dependency.rs`

`DependencyGraph` is an adjacency-list representation of item dependencies:

```rust
pub struct DependencyGraph {
    edges: HashMap<Uuid, Vec<(Uuid, DependencyType)>>,   // item → items it blocks
    reverse_edges: HashMap<Uuid, Vec<(Uuid, DependencyType)>>,  // item → items that block it
}
```

`DependencyGraph::from_edges(edges)` builds the graph from a slice of `DependencyEdge` values. The API handler loads all existing dependencies for the involved items, builds the graph, then calls `validate_new_edge(source, target)` before inserting.

**`would_create_cycle(source, target)`** runs a depth-first search starting from `target`, following the `edges` adjacency list. If it ever reaches `source`, adding `source → target` would close a cycle and the function returns `true`. The check is O(V + E) over the existing graph.

`validate_new_edge` wraps the check in a `Result`, also catching the self-reference case (`source == target`).

---

### `error.rs`

`CoreError` is a `thiserror`-derived enum that covers every domain-level failure:

- `ItemNotFound(Uuid)`, `ProjectNotFound(Uuid)`, `SprintNotFound(Uuid)`, `RoleNotFound(Uuid)` — map to HTTP 404.
- `InvalidTransition { from, to }`, `WipLimitExceeded { column, limit, current }`, `DependencyCycle(Uuid)`, `DuplicateDependency { ... }`, `InvalidVocabularyKey(String)`, `EmptyWorkflow`, `HasChildren(Uuid, usize)`, `Validation(String)` — map to HTTP 400.

The mapping from `CoreError` to HTTP status codes lives in `tack-api/src/error.rs`, keeping the core crate free of HTTP knowledge.

---

## `tack-db`

**Lives in:** `crates/tack-db/src/`

**Owns:** SQLite connection pool initialisation, migration runner, repository pattern over all entities.

**Does not own:** HTTP concerns, config loading, or business rule enforcement. The repository functions are thin: they translate between Rust structs and SQL rows.

---

### `lib.rs`

`init_pool(database_url)` creates a `SqlitePool` using `SqlitePoolOptions` with a max of 5 connections, then immediately runs two `PRAGMA` statements:

- `PRAGMA journal_mode=WAL` — enables Write-Ahead Logging for better concurrent read performance.
- `PRAGMA foreign_keys=ON` — SQLite does not enforce foreign keys by default; this enables them.

---

### `migrations.rs`

Contains every migration (61 as of this build — check `GET /api/health`'s
`migrations_applied` field rather than trusting a hand-written count here) as `const`
arrays of SQL strings. Each entry is `(&str name, &[&str] statements)`. The runner:

1. Creates `_migrations` table if absent.
2. For each migration, checks if the name is already recorded.
3. Executes each SQL statement in order.
4. Records the migration name on success.

Migrations are idempotent — running them on an existing database is safe. Notable migrations:

- `004_items` — creates the `items` table with indexes on `project_id`, `status`, `priority`, `parent_id`, and `sprint_id`.
- `010_fts` — creates the FTS5 virtual table `items_fts` and three triggers (`after_item_insert`, `after_item_update`, `after_item_delete`) that keep the FTS index in sync with the `items` table.
- `012_custom_fields` — `custom_field_definitions` and `custom_field_values` tables.
- `016_perf_indexes` — additional composite indexes added after profiling.
- `039`–`048` — the ten neutral runner-v1 execution-domain tables added in Part III
  Wave 1 (execution requests/attempts/events/decisions/artifacts, agent profiles,
  runner fleets/members, model profiles). `049`+ refine execution replay, recovery
  and attempt-start facts across later waves.

Each ordinary migration runs in its own transaction with the `_migrations` record
inserted at commit; a failing statement rolls the whole migration back. Applied
migrations are checked at every startup against the binary's own ordered list by name
**and** a deterministic checksum — an edited or reordered history refuses to boot
rather than running silently. A small subset (037/038, a table
copy/verify/swap rebuild predating Part III) additionally takes an automatic
`VACUUM INTO` snapshot before its first attempt. See `docs/MIGRATION-GUIDE.md` for the
operator-facing version of this and `docs/adr/0008-transactional-migration-rebuild-recovery.md`
for the design rationale.

---

### `repo.rs` and `repo/`

`repo.rs` declares the `Repository` struct, which holds a `SqlitePool`, and re-exports a method for each database operation by delegating to the appropriate submodule:

```rust
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub async fn create_item(&self, ...) -> Result<Item, sqlx::Error> {
        items::create_item(self.pool(), ...).await
    }
    // ...
}
```

This design gives callers a single `repo` value to pass around while keeping each entity's SQL in its own file.

**Per-entity submodules** (`items.rs`, `projects.rs`, `sprints.rs`, `roles.rs`, `comments.rs`, `dependencies.rs`, `attachments.rs`, `boards.rs`, `templates.rs`, `custom_fields.rs`):

- Functions take `&SqlitePool` (or `&self` for the struct-based submodules) and return `Result<T, sqlx::Error>` or `Result<T, DependencyError>`.
- Queries use `sqlx::query` / `sqlx::query_as` with positional `?` parameters.
- UUIDs are stored as `TEXT` — bound as `.bind(id.to_string())` and parsed back from the row.
- JSON fields (`workflow`, `vocabulary`, `tags`) are serialised to/from strings with `serde_json`.
- Timestamps are stored as RFC 3339 strings and parsed via `chrono::DateTime<Utc>`.

**Notable function: `check_and_update_parent_status`** (in `items.rs`). After an item is moved to a `Done`-category status, the handler calls this function with the item's `parent_id`. It queries whether all sibling items are also in a done status, and if so, updates the parent. The `WorkflowConfig::should_complete_parent(all_siblings_done)` call in `tack-core` provides the decision logic — the repository only handles the data queries.

---

## `tack-orch`

**Lives in:** `crates/tack-orch/src/`

**Owns:** two things that share a crate because both sit between `tack-db` and
`tack-api` without depending on `tack-api`: the Docket agent-fleet orchestration
client (`ControlPlane` trait, reconciler, adapters), and — added in Part III — the
neutral runner-v1 **execution domain** (`execution/`): lifecycle validation,
fencing/idempotency types, and the pure state-machine rules that both `tack-api`'s
handlers and `tack-runner`'s protocol implementation must agree on.

**Does not own:** HTTP handling or SQL. Depends only on `tack-core` and `tack-db`; the
dependency points inward deliberately — `tack-api` depends on this crate (to spawn the
reconciler and expose the orchestration/execution routes), never the reverse. This
boundary is load-bearing: it's what lets `tack-runner`'s tests exercise the same
lifecycle rules as the server without linking Axum.

---

### `execution/lifecycle.rs`

`validate_transition(from, to, actor)` is the single authority for every legal
execution-attempt state change. States: `queued | leased | preparing | running |
waiting_decision | succeeded | failed | cancelled | lost | needs_operator`. Each
transition is validated against **both** the state pair and which `TransitionActor`
(`Scheduler`, `Operator`, `LeaseOwner`, `RecoveryService`) is allowed to request it —
for example, only `RecoveryService` may move an attempt into `lost` or
`needs_operator`; a lease owner reporting the same crash cannot self-authorize it. See
the [Recovery Runbook](../user-guide/recovery-runbook.md) for what drives those
transitions operationally.

### `execution/types.rs`

Wire-shape-adjacent types shared by both the server and (once wired) the runner:
`AgentProfileSnapshot { name, instructions, tool_policy, timeout_seconds, budgets }`,
`RepositorySnapshot { kind, remote, base_revision }`, `PermissionPolicy { tools,
network }`, and `EnvironmentValue { value, secret_reference }` — the last one is why
execution requests never store a raw secret: every environment entry is either a
literal non-secret value or an opaque reference the runner resolves locally.

### `reconciler.rs` and `adapters::docket`

One `tokio` task per registered Docket control plane, polling `/health` +
`/status.json` on a jittered interval and driving a `healthy → degraded (3 failures) →
unreachable (10)` state machine. Remote enums all carry an `Unknown(String)` fallback
so a Docket version mismatch degrades gracefully instead of failing a poll. This half
of the crate is entirely independent of the execution domain above — see
[Docket compatibility](../user-guide/agent-runners.md#docket-compatibility) for how
(and why) the two never share a code path.

---

## `tack-api`

**Lives in:** `crates/tack-api/src/`

**Owns:** HTTP server startup, route registration, request/response handling, configuration, WebSocket management, error mapping.

**Does not own:** SQL queries (those are in `tack-db`) or business rules (those are in `tack-core`). Handlers orchestrate calls to both.

This crate is a **library only** — it does not produce its own binary. The single
`tack` binary (in `tack-cli`) calls `tack_api::serve()` to start the server.

---

### `server.rs`

Exposes `pub async fn serve()`, the server entry point. It does these things in order:

1. Loads `AppConfig` (TOML file or environment variables).
2. Initialises the `tracing` subscriber (plain text or JSON depending on config).
3. Applies any staged database restore (rename `.restore` file into place).
4. Calls `init_pool()` and `migrations::run_all()`.
5. Ensures a default workspace row exists (creates one if the table is empty).
6. Builds `AppState`, calls `build_router(state)`, and starts `axum::serve` with graceful shutdown on `CTRL+C`.

`tack-cli` builds a Tokio runtime and calls `serve()` when you run `tack` with no
subcommand (or `tack serve`).

---

### `router.rs`

`AppState` is the shared state cloned into every handler:

```rust
pub struct AppState {
    pub repo: Repository,
    pub config: AppConfig,
    pub workspace_id: Uuid,
    pub broadcast_tx: broadcast::Sender<BoardEvent>,
}
```

`build_router(state)` assembles the Axum `Router`. Routes are grouped by entity and nested under `/api`. The file also wires up:

- **CORS** — reads `config.allowed_origins`, constructs a `tower_http::cors::CorsLayer`.
- **Body limit** — `DefaultBodyLimit::max(config.max_body_size_bytes)` globally; the attachment upload route overrides this to 50 MB.
- **Security headers** — `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin`, `X-Frame-Options: DENY` via `SetResponseHeaderLayer`.
- **Request tracing** — `TraceLayer` logs every request with method and URI.
- **Token gate** — `middleware::from_fn_with_state(state, require_token)` wraps all `/api` routes.
- **embed-spa feature** — when compiled with `--features embed-spa`, a fallback handler serves the bundled SPA.

---

### `handlers/`

One file per entity group. A typical handler follows this shape:

```rust
pub async fn update_item(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateItem>,
) -> ApiResult<Json<Item>> {
    input.validate().map_err(|e| ApiError::BadRequest(e.to_string()))?;
    // load, validate, persist, broadcast
}
```

Handlers return `ApiResult<Json<T>>`, which is `Result<Json<T>, ApiError>`. The `ApiError` type implements `IntoResponse`, so Axum converts errors to JSON automatically.

**`websocket.rs`** is somewhat different from other handler files — see the Architecture Overview for a full walkthrough of the connection lifecycle. The key public API it exposes to other handlers is:

```rust
pub fn broadcast_event(state: &AppState, event: BoardEvent) { ... }
```

Any handler that mutates data calls this after a successful write.

---

### `config.rs`

`AppConfig::load()` tries to read `tack.toml` from the current directory. If that fails, it reads environment variables (`TACK_HOST`, `TACK_PORT`, `TACK_DATABASE_URL`, etc.) over a `Default::default()` base. There is no `figment` or other config framework — the logic is a straightforward chain of `if let Ok(v) = std::env::var(...)` assignments.

The API token is never logged. The only place it appears in logs is a boolean "token configured: true/false" in the startup message.

---

### `error.rs`

`ApiError` is the unified error type for all handlers. It implements `IntoResponse` with this mapping:

| `ApiError` variant | HTTP status |
|---|---|
| `NotFound` | 404 |
| `BadRequest` | 400 |
| `Conflict` | 409 |
| `Core(CoreError::*NotFound*)` | 404 |
| `Core(CoreError::InvalidTransition \| WipLimitExceeded \| …)` | 400 |
| `Database` | 500 |
| `Internal` | 500 |

The response body is always `{ "error": { "status": <code>, "message": "<text>" } }`.

---

## `tack-runner`

**Lives in:** `crates/tack-runner/src/`

**Owns:** its own binary (`tack-runner`, entirely separate from `tack`) — local
enrollment/credential handling, the isolated per-attempt workspace, the owner-only
attempt journal, and the harness adapter layer. **Does not own** anything the API
must not touch: vendor credentials, workspace contents, and the harness subprocess
never leave this crate. See [Agent Runners & Fleet Execution](../user-guide/agent-runners.md)
for the operator-facing view of everything below.

### `config.rs`

`RunnerConfig::from_sources` layers defaults → TOML file → environment
(`TACK_RUNNER_API_URL`, `TACK_RUNNER_ID`, `TACK_RUNNER_STATE_DIR`,
`TACK_RUNNER_ENROLLMENT_TOKEN`) → CLI flags. `EnrollmentCredential`'s `Debug`/
`Display` are hardcoded to print `[REDACTED]` — redaction here is structural, not a
convention a future `println!` could accidentally bypass.

### `journal.rs` and `workspace.rs`

A `WorkspaceJournal` record is written to `TACK_RUNNER_STATE_DIR` **before** any
harness process is allowed to spawn — this ordering is what makes crash recovery
possible (see the [Recovery Runbook](../user-guide/recovery-runbook.md)).
`JournalState` tracks `Prepared → ProcessObservedRunning → ... → Reported`; a restart
finds this file and reports what it actually observed rather than guessing.
`WorktreeProvisioner` is the sole trait allowed to create a workspace's git worktree —
tests inject a fake so unit tests never touch a real checkout.

### `client.rs` — `RunnerProtocolClient`

Defines the trait the runner's runtime loop drives: `enroll`, `refresh`, `claim`,
`heartbeat`, and per-attempt `accept`/`start`/`events`/`decisions`/`artifacts`/
`completion`/`cancellation`/`recovery-observation`. **The only implementation of this
trait in the tree today is `UnavailableProtocolClient`**, which fails immediately with
a typed `RunnerError::ProtocolUnavailable` — there is no HTTP-backed implementation
wired into `main.rs` yet, and the crate does not depend on `reqwest`. This is a
genuine, tested gap (`runtime::tests::unavailable_protocol_is_a_typed_failure_not_success`
pins the failure as deliberate), not an oversight papered over with a fake success —
see [What actually runs today](../user-guide/agent-runners.md#what-actually-runs-today).

### `harness/`

The adapter layer: `process.rs` (bounded output capture, timeouts, process-group
cancellation), `event_sink.rs` (backpressure), `redact.rs`, `artifact.rs`, and one
module per harness (`codex.rs`, `claude_code.rs`) — the harness vocabulary itself stays
open (`HarnessKind::Other(String)`); this crate just ships adapters for these two. Two
traits:
`HarnessAdapter` (per-attempt lifecycle: `validate`/`start`/`cancel`/`wait`/
`reconcile`) and `HarnessProbe` (version/capability discovery). `AdapterRegistry`
implements `HarnessAdapter` by dispatching on harness kind and **refuses to register
any probe claiming `cancel: supported`** — every harness's own shell tool spawns its
subprocess in a new session outside the runner's process group, confirmed against real
binaries in Wave 3 (`docs/agent-handoffs/part-iii/III-D{1,2,3,4,5}.md`). Live harness
tests are `#[ignore]`d and never required in CI; `harness/fixtures/fake_harness.sh`,
driven by `TACK_FAKE_HARNESS_MODE`, is the always-runnable path every required test
uses instead.

---

## `tack-cli`

**Lives in:** `crates/tack-cli/src/`

**Owns:** the single `tack` binary — both starting the server and the command-line client (parsing, human-readable output, HTTP calls to the API).

**Does not own:** any `tack-core` or `tack-db` types directly. The client commands work entirely through the HTTP API — they serialise to JSON for requests and deserialise from `serde_json::Value` for responses (no strongly-typed response structs). This keeps the CLI decoupled from internal model changes that do not affect the API contract. (To *run* the server it depends on `tack-api` and calls `tack_api::serve()`.)

---

### `main.rs`

Uses `clap`'s derive API. The top-level `Cli` struct has two global flags (`--api-url`, `--token`) and an **optional** `Commands` enum. Commands include `serve`, `init`, `projects`, `add`, `list`, `move`, `board`, `search`, `sprint`, `config`, `completions`, `backup`, and `restore`.

Running `tack` with **no subcommand** — or `tack serve` — starts the server + web UI: `run_server()` builds a Tokio runtime and calls `tack_api::serve()`. This is the primary, UI-first entry point. Everything else is the CLI client.

Three cases (`serve`, `config`, and `completions`) are handled before the `TackClient` is constructed, since they do not need a live connection.

All other commands instantiate a `TackClient`, call the appropriate method, and then either print raw JSON (with `--json`) or format a human-readable table. The table formatter (`print_table_row`) pads and truncates columns to fixed widths, which keeps the output readable in standard terminals.

The `add` and `list` commands fetch the project's vocabulary via `vocab::fetch()` and translate `item_type` strings through it before printing — so a construction project shows `Work Order` rather than `task` in the output.

---

### `client.rs`

`TackClient` wraps `reqwest::blocking::Client`. All methods prepend `/api` to the supplied path and attach the `Authorization: Bearer <token>` header when a token is configured.

Public methods:

- `get(path)` → `serde_json::Value`
- `post(path, body)` → `serde_json::Value`
- `patch(path, body)` → `serde_json::Value`
- `delete(path)` → `()`
- `get_bytes(path)` → `Vec<u8>` — used for backup download
- `post_bytes(path, data)` → `serde_json::Value` — used for restore upload

Error handling: the `extract()` helper parses the response body regardless of status code, then returns the body on success or extracts the `error.message` field and surfaces it as an `anyhow::Error` on failure.

---

### `config.rs`

`Config::load(base_url_override, token_override)` applies a precedence chain:

1. CLI flag value (passed as `Option<String>`)
2. Environment variable (`TACK_API_URL`, `TACK_API_TOKEN`)
3. `~/.tackrc` — a TOML file with `base_url` and optional `token` fields
4. Default: `http://127.0.0.1:3210`

`config::save(base_url, token)` writes `~/.tackrc`. This is what `tack config --url <url>` does.

---

## `tack-desktop`

**Lives in:** `crates/tack-desktop/src/`

**Owns:** the Tauri shell — a window, a system tray icon, and the supervisor that either attaches to a `tack serve` already answering on the configured port or starts one itself as a bundled sidecar. Built with `make desktop`; excluded from the root workspace (see the crate map in the top-level `CLAUDE.md`) so a contributor without Tauri's system dependencies (GTK, WebKit) still builds every other crate with a plain `cargo build --workspace`. Its own `Cargo.lock`, CI job, and Dependabot entry follow from that same exclusion.

**Does not own:** the server. It never opens the database or reimplements anything `tack-api` already does — only starts, attaches to, and supervises the `tack` binary as a child process, and only ever stops a server it started itself.

---

### `main.rs`

Builds the Tauri app and calls `attach_or_start` before creating the window — the window exists only on `Ok`. Of the ways that call can fail, two render a dialog naming the reason before exiting (a port already held by something that isn't Tack; an attached server older than the bundle), and a catch-all arm covers everything else. The window opens at `1200x800` (`WebviewWindowBuilder::inner_size`).

### `supervisor.rs`

`attach_or_start` probes the configured port for a Tack server already answering at a compatible version and attaches to it instead of spawning a second one; otherwise it spawns the bundled sidecar binary and waits up to a fixed health timeout for `/api/health` to answer. A server this process did not start is never signalled to stop, on any exit path.

### `first_run.rs`

`ensure_settings` runs before the supervisor does: on an empty data root it shows a first-run dialog (database path, port) and writes the answer to `settings.json` before anything tries to reach a server. The dialog call itself runs off the main thread — Tauri's dialog plugin deadlocks the event loop if it doesn't.

### `tray.rs`

Builds the tray icon and its menu: **Open Tack** (show or refocus the window), a permanently disabled entry reading **Agent execution: unknown — the switch arrives with the Agents page** (`AGENT_EXECUTION_LABEL`, a hard-coded constant passed `enabled: false` — nothing in this file makes an HTTP request; it does not read `GET /api/local-runner` or any other live state), a checked **Launch at login** toggle (on by default the first time), and **Quit**. Closing the window only hides it — the server keeps running; **Quit** is the action that actually stops it.
