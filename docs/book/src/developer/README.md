# Architecture Overview

> They are separate because they scale and fail differently. **One board, many runners:**
> a board on a small VPS dispatches to runners on ten developers' machines, each with its
> own agent, model and capacity. A runner that dies mid-run cannot corrupt the board — its
> lease expires and its fencing token stops writing. A board that restarts cannot lose a
> run — the runner's journal knows what it started. **One developer runs both in one
> process with one command**, on the same contract, with the same recovery.

You have read the quick-start and can run the server. This document explains *why* the code is structured the way it is — the reasoning behind each layer, how a request actually travels through the system, and where the interesting domain logic lives.

---

## 1. The Layering Rule

Tack enforces a strict one-way dependency graph across the workspace's six crates:

```
tack-core  ←  tack-db  ←  tack-orch  ←  tack-api  ←  tack-cli

tack-runner  (a separate binary, sibling to this graph — talks to tack-api
              over the runner-v1 HTTP contract, not through any Rust dependency)
```

Each arrow means "depends on". No reverse arrows are allowed. A seventh crate,
`tack-desktop`, is deliberately excluded from this workspace (its own `Cargo.toml`
and lockfile) because Tauri drags GTK/WebKit/glib into whatever workspace holds it —
see the [Crate Tour](crate-tour.md#tack-desktop) for how it supervises `tack` instead
of linking against it.

**What this means in practice:**

- `tack-core` has zero I/O. It cannot open a file, touch a database, or make a network call. It only contains pure Rust structs, enums, and functions. You can run every test in it without a database process.
- `tack-db` knows about `tack-core` (it persists those structs), but it knows nothing about HTTP, routing, or config files.
- `tack-orch` knows about `tack-core` and `tack-db` but nothing about HTTP — it is the control-plane client (Docket reconciler) and the neutral runner-v1 execution domain, both usable without Axum. See [Crate Tour](crate-tour.md#tack-orch).
- `tack-api` is the only place where HTTP concerns (status codes, request extraction, CORS) and database concerns meet. It depends on `tack-orch` to spawn the reconciler and expose the orchestration/execution routes.
- `tack-cli` is the single `tack` binary. It depends on `tack-api` so that `tack serve` can start the server in-process, but its **client** commands only talk to a running server over HTTP — they never open the database directly. This means the CLI works whether the server is local or on a remote machine.
- `tack-runner` is a separate binary entirely. It never depends on any of the crates above; it speaks the runner-v1 protocol to `tack-api` over loopback or the network, the same way a remote runner would.

**Why bother?** The layering prevents the kind of "everything knows about everything" entanglement that makes codebases brittle. It also means:

- Workflow rules are always consistent. Because all validation lives in `tack-core`, neither a direct DB call nor an API call nor a CLI command can bypass it.
- Tests are fast. Core tests are pure function calls — no test database, no async runtime, no cleanup.
- The domain model can be reused. If a future project needs the same workflow engine, it can depend on `tack-core` without dragging in SQLite or Axum.

---

## 2. The Universal Item Model

Every piece of work in Tack — whether it is called a task, an epic, a bug, a work order, or an assignment — is stored as an `Item`. There is one table, one struct, one set of repository functions.

The `Item` struct in `tack-core/src/models.rs` carries an `item_type` field that records what kind of thing it is (`Task`, `Epic`, `Bug`, `Feature`, `Subtask`, `Requirement`, or a `Custom(String)` for anything else). The **vocabulary** system then translates that type name into whatever label the project uses.

Think of it like a spreadsheet: every row has the same columns. The header labels change depending on who is looking at the sheet. A construction project relabels `task` as `Work Order` and `sprint` as `Phase`, but the underlying row structure is identical.

**Benefits:**

- One migration path. Adding a new field to items (say, a `story_points` column) means one migration, one struct update, one set of repository functions — not separate migrations per item type.
- Hierarchy is free. Because every item can have a `parent_id` pointing to another item, the tree structure (epic → feature → task → subtask) falls out naturally without a separate parent-type/child-type mapping.
- Filtering is uniform. The same `ItemFilter` struct handles filtering by `item_type`, `status`, `priority`, `assignee`, or tag regardless of project type.

The tradeoff is that the `item_type` field carries less compile-time enforcement than a proper type hierarchy. This is intentional — the flexibility outweighs the constraint for a tool meant to work across very different domains.

---

## 3. How a Request Flows Through the System

Here is a concrete walk-through of `PATCH /api/items/{id}` — the endpoint that moves an item to a new status.

```
HTTP client
    │
    ▼
Axum router (tack-api/src/router.rs)
    │  Extracts: Path(item_id), State(AppState), Json(UpdateItem)
    ▼
Handler: update_item (tack-api/src/handlers/items.rs)
    │  1. Calls repo.get_item(id) to load current item
    │  2. Calls repo.get_project(project_id) to load workflow
    │  3. Calls workflow.validate_transition(old_status, new_status)  ← pure, no I/O
    │  4. Calls workflow.check_wip_limit(new_status, current_count)  ← pure, no I/O
    │  5. Calls repo.update_item(id, patch)  ← writes to DB
    │  6. Calls repo.check_and_update_parent_status(...)  ← best-effort parent cascade
    │  7. Calls broadcast_event(ItemUpdated { ... })  ← fires WebSocket event
    ▼
Repository (tack-db/src/repo/items.rs)
    │  Runs parameterised SQL via sqlx
    ▼
SQLite
    │
    ◀── Result<Item, sqlx::Error>
    │
Handler maps error → ApiError → HTTP response
    │
    ▼
HTTP client receives JSON
```

A few things worth noting:

- Steps 3 and 4 call pure functions in `tack-core`. No database round-trip is needed to validate the transition — the entire workflow config was loaded with the project in step 2, and it lives in memory as a `WorkflowConfig` struct.
- The handler owns the orchestration. It decides the order of validation, persistence, and notification. The repository and the core crate do not know about each other.
- Errors propagate upward via Rust's `?` operator. The `ApiError` type (in `tack-api/src/error.rs`) implements `IntoResponse`, mapping each `CoreError` variant to the appropriate HTTP status code.

---

## 4. How Workflow Validation Works

The workflow for a project is a `WorkflowConfig` struct stored as JSON in the `projects` table. When the server loads a project, it deserialises that JSON back into the struct — no extra tables, no joins.

The `WorkflowConfig` holds:

- A list of `StatusDef` entries (name, category, optional WIP limit, sort order).
- An optional list of `Transition` pairs (`from`, `to`).

`validate_transition(from, to)` does two things:

1. Checks that both `from` and `to` exist in the status list.
2. If an explicit transition list exists, checks that the pair is in it.

If the transition list is absent (`None`), any move between two valid statuses is allowed. This is the Scrum and Kanban default. The construction workflow, by contrast, defines an explicit list — you cannot skip from `Permit` to `Handover`.

**Why is this in `tack-core` and not in the database layer?**

Because it is a business rule, not a storage rule. The database does not know what statuses are valid — that is configuration data stored in a JSON column. Only `tack-core` knows how to interpret that configuration and enforce the rules. Putting the validation in the repository would mean the repository would need to import workflow logic, violating the layer boundary. Putting it in the handler (without core) would scatter the logic and make it untestable without a running server.

By living in `tack-core`, the validation is:

- Testable with zero dependencies (73 unit tests, none of which touch a database).
- Reusable by any layer that loads a `WorkflowConfig`.
- Independent of how the config was persisted.

---

## 5. How Real-Time Updates Work

The real-time board feature uses a Tokio broadcast channel. Here is the architecture:

```
AppState
  └── broadcast_tx: broadcast::Sender<BoardEvent>   (capacity: 100)

On any mutating API call:
  handler calls broadcast_event(&state, BoardEvent::ItemUpdated { ... })
    └── state.broadcast_tx.send(event)  ← non-blocking, drops if no subscribers

On WebSocket connection (GET /api/projects/{id}/boards/live):
  handler upgrades the connection
  spawns two tasks:
    send_task:  rx.recv() in a loop → filter by project_id → send JSON to client
    recv_task:  reads messages from client (handles Close, Ping)
  tokio::select! on both tasks — whichever ends first, the other is aborted
```

The broadcast channel is the pub/sub backbone. Every handler that modifies data calls `broadcast_event()`; every WebSocket connection subscribes to the channel and filters events by `project_id` before forwarding them to the client.

**Important properties:**

- **Fire and forget.** `broadcast_tx.send()` returns immediately whether or not there are subscribers. If nobody is listening, the event is dropped. This keeps the write path fast.
- **Per-project filtering happens in the send task.** The channel carries events for all projects; each WebSocket connection only forwards events that match its `project_id`. This keeps the channel logic simple while allowing one channel to serve many concurrent connections.
- **No persistence.** Events are not stored. A client that disconnects and reconnects will not receive missed events — it will simply see the current board state on the next page load.

---

## 6. Directory Map

```
.
├── crates/
│   ├── tack-core/         Pure domain logic; no I/O
│   │   └── src/
│   │       ├── models.rs    All domain structs and DTOs
│   │       ├── workflow.rs  WorkflowConfig, validation, presets
│   │       ├── vocabulary.rs VocabularyMap, resolve(), presets
│   │       ├── dependency.rs DependencyGraph, cycle detection
│   │       ├── error.rs     CoreError enum
│   │       └── lib.rs       Re-exports
│   │
│   ├── tack-db/           SQLite persistence layer
│   │   └── src/
│   │       ├── lib.rs       init_pool() — WAL mode, foreign keys on
│   │       ├── migrations.rs Ordered migrations as embedded SQL strings
│   │       ├── repo.rs      Repository struct — delegates to submodules
│   │       └── repo/        One file per entity (items, projects, sprints, …)
│   │
│   ├── tack-orch/         Control-plane client + runner-v1 execution domain
│   │   └── src/
│   │       ├── lib.rs       ControlPlane trait, OrchError
│   │       ├── reconciler.rs Docket poll loop, health state machine
│   │       ├── adapters/    Docket adapter
│   │       ├── execution/   Lifecycle validation, fencing/idempotency types
│   │       ├── scheduler/   Deterministic runner selection
│   │       ├── model_policy/ Deterministic model-selection precedence
│   │       ├── execution_retention.rs Cancellable retention sweep
│   │       ├── execution_observability.rs Fleet health snapshots
│   │       └── usage_provenance.rs Requested-vs-actual model + usage economics
│   │
│   ├── tack-api/          Axum HTTP server + WebSocket
│   │   └── src/
│   │       ├── main.rs      Startup: config, pool, migrations, listen
│   │       ├── router.rs    All routes + AppState + middleware wiring
│   │       ├── config.rs    AppConfig — TOML / env var loading
│   │       ├── error.rs     ApiError → HTTP status code mapping
│   │       ├── debug.rs     /api/health, /api/debug/* endpoints
│   │       ├── middleware.rs Bearer token gate
│   │       └── handlers/    One file per entity group + websocket.rs
│   │
│   ├── tack-cli/          CLI (clap), talks to API over HTTP
│   │   └── src/
│   │       ├── main.rs      Command tree + implementation functions
│   │       ├── client.rs    TackClient — thin reqwest wrapper
│   │       ├── config.rs    ~/.tackrc reader/writer
│   │       ├── service.rs   `tack service` — systemd/launchd unit management
│   │       └── vocab.rs     Fetch and cache project vocabulary
│   │
│   ├── tack-runner/       Separate binary; pull-based execution runner
│   │   └── src/
│   │       ├── main.rs      Startup, config, registry
│   │       ├── engine.rs    Claim → prepare → run → report loop
│   │       ├── harness/     Adapters per coding agent (codex, claude_code)
│   │       ├── journal.rs   Local attempt journal for crash recovery
│   │       └── workspace.rs Per-attempt working directory + credentials
│   │
│   └── tack-desktop/      Tauri shell; excluded from the workspace above
│       └── src/            Supervises `tack` as a bundled sidecar process
│
├── frontend/                SolidJS + TypeScript + Tailwind v4
│   └── src/
│       ├── app/             Router, layout, route definitions
│       ├── features/        One directory per feature (board, projects, …)
│       ├── shared/          Reusable UI components, state helpers
│       └── types/           TypeScript type definitions (mirrors API shapes)
│
└── docs/book/               mdBook developer documentation
```
