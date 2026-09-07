# Contributing to Tack

Thanks for your interest in Tack. This guide covers how to report bugs, propose
features, submit pull requests, and set up your dev environment. All
participation is governed by our [Code of Conduct](CODE_OF_CONDUCT.md); by
contributing you agree to uphold it. Project decisions follow the
[governance model](GOVERNANCE.md) (single-maintainer / BDFL).

**Jump to:** [Reporting bugs & requesting features](#reporting-bugs--requesting-features)
· [Pull request process](#pull-request-process) · [Branching model](#branching-model)

## Quick Start

```bash
git clone https://github.com/yielab/tack.git
cd tack

# Hooks and the merge driver for generated files (per-clone git config,
# so it cannot be committed — this is the one-time setup)
./scripts/setup-git.sh

# Verify the build and tests pass (nextest is the test runner: cargo install cargo-nextest --locked)
cargo build
cargo nextest run --workspace
```

`scripts/setup-git.sh` wires up two things:

- **`.githooks/pre-push`** runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and a check that `Cargo.lock` and `schema.gen.ts` are not stale. This mirrors CI, so failures are caught locally before they reach GitHub.
- **The `tack-generated` merge driver** for `Cargo.lock`, `frontend/package-lock.json`, `docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts`. Each is a pure function of sources tracked elsewhere, so hand-merging one is always either busywork or a mistake. The driver resolves them without a conflict and `.githooks/post-merge` regenerates them from the merged sources — staged, never committed for you. `scripts/regen-generated.sh` is the same regeneration, runnable by hand.

`rust-toolchain.toml` pins the exact compiler both you and CI use, and rustup installs it with `rustfmt` and `clippy` the first time you run `cargo` here — you do not need to select a toolchain yourself, and you should not override it. Bumping that pin is a deliberate one-line change that Dependabot proposes monthly; it can surface new clippy lints, which is precisely why it is not left to whatever day upstream ships a release.

Skipping the setup leaves you with git's ordinary text merge on those files. That is how the repository behaved before, so a clone without it is degraded, not broken.

---

## Requirements

| Tool | Version | Install |
| --- | --- | --- |
| Rust | 1.89+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 22+ | [nodejs.org](https://nodejs.org/) |
| Git | 2.x | system package manager |
| curl | any | pre-installed on most systems |
| jq | any | `apt install jq` / `brew install jq` (optional, for pretty JSON) |

No external database, Docker, or services needed. SQLite is embedded.

---

## Project Structure

```text
Tack/
├── Cargo.toml                  # Workspace root (shared dependencies)
├── Cargo.lock                  # Pinned dependency versions
├── Makefile                    # Common dev commands
├── .githooks/
│   ├── pre-push                # fmt + clippy + generated-file staleness gate
│   └── post-merge              # regenerates what the tack-generated merge driver resolved
├── crates/
│   ├── tack-desktop/         # Tauri desktop shell — its OWN workspace, not a member of
│   │                         # the one below (Tauri needs GTK/WebKit; the server must not).
│   │                         # Build with `make desktop`; `cargo --workspace` never sees it.
│   ├── tack-core/            # Pure domain logic (no I/O, no DB)
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── models.rs       # All data structures, DTOs, and custom-field validation
│   │       ├── workflow.rs     # Workflow engine (transitions, WIP, parent-auto-complete)
│   │       ├── vocabulary.rs   # Term customization system
│   │       ├── dependency.rs   # Dependency graph (DAG with cycle detection)
│   │       └── error.rs        # Domain error types
│   ├── tack-db/              # Database layer
│   │   ├── src/
│   │   │   ├── lib.rs          # Pool initialization, WAL mode
│   │   │   ├── migrations.rs   # 18 schema migrations (auto-run on startup)
│   │   │   ├── repo.rs         # Repository struct
│   │   │   └── repo/           # One file per entity
│   │   │       ├── projects.rs
│   │   │       ├── items.rs
│   │   │       ├── sprints.rs
│   │   │       ├── roles.rs
│   │   │       ├── comments.rs
│   │   │       ├── dependencies.rs
│   │   │       ├── attachments.rs
│   │   │       ├── boards.rs
│   │   │       ├── custom_fields.rs
│   │   │       └── templates.rs
│   │   └── tests/
│   │       ├── repository.rs       # CRUD, retention, concurrency (one binary)
│   │       ├── migrations.rs       # schema/upgrade tests (one binary)
│   │       └── perf_test.rs        # 50k-item perf test (#[ignore])
│   ├── tack-api/             # Axum HTTP server + WebSocket
│   │   ├── src/
│   │   │   ├── main.rs         # Server entry point + staged restore
│   │   │   ├── lib.rs
│   │   │   ├── router.rs       # All routes, AppState, middleware
│   │   │   ├── config.rs       # TOML/env config
│   │   │   ├── error.rs        # ApiError → HTTP status mapping
│   │   │   ├── debug.rs        # /api/health, /api/debug/*
│   │   │   ├── middleware.rs   # Bearer token auth
│   │   │   ├── webhook.rs      # Outbound webhook delivery
│   │   │   └── handlers/
│   │   │       ├── attachments.rs
│   │   │       ├── backup.rs           # GET /api/backup, POST /api/restore
│   │   │       ├── boards_multi.rs     # Multiple boards per project
│   │   │       ├── comments.rs
│   │   │       ├── custom_fields.rs
│   │   │       ├── dependencies.rs
│   │   │       ├── export.rs           # JSON/CSV export + import
│   │   │       ├── import_github.rs    # GitHub Issues import
│   │   │       ├── import_linear.rs    # Linear import
│   │   │       ├── items.rs
│   │   │       ├── projects.rs
│   │   │       ├── roles.rs
│   │   │       ├── spa.rs              # SPA fallback (--features embed-spa)
│   │   │       ├── sprints.rs
│   │   │       ├── templates.rs
│   │   │       └── websocket.rs
│   │   └── tests/
│   │       ├── common/mod.rs       # test_app(), test_app_with_config()
│   │       ├── handlers.rs         # CRUD, routes, economics, provisioning
│   │       ├── orchestration.rs    # dispatch, approvals, reconciler, fleet
│   │       ├── runner_protocol.rs  # lifecycle, decisions, artifact events
│   │       ├── security.rs         # auth surfaces, CORS, write races
│   │       ├── wiring.rs           # proofs that a seam is load-bearing
│   │       └── wave2_gate.rs       # named CI gate, kept its own binary
│   └── tack-cli/             # clap CLI (talks to API over HTTP)
│       └── src/
│           ├── main.rs         # All commands
│           ├── client.rs       # HTTP client wrapper (reqwest)
│           ├── config.rs       # ~/.tackrc reader
│           └── vocab.rs        # Vocabulary-aware output
├── frontend/
│   ├── src/
│   │   ├── components/         # Reusable UI components
│   │   ├── pages/              # Board, List, Dashboard, Sprints, Calendar, Timeline, Settings, Templates
│   │   ├── lib/                # api.ts, vocab.ts, websocket, optimistic UI
│   │   └── types/              # TypeScript types
│   └── dist/                   # Built SPA (gitignored; embedded via --features embed-spa)
└── docs/                       # Documentation
```

## Dependency Flow

```text
tack-core  (pure logic, no I/O)
     ^
     |
tack-db    (depends on core, adds SQLite)
     ^
     |
tack-api   (depends on core + db, adds HTTP)

tack-cli   (depends on core only — talks to tack-api over HTTP, no DB)
```

**Rule:** `tack-core` must never import `tack-db` or any I/O crate.
Keep business logic testable without a database. `tack-cli` must never import
`tack-db` — all data access goes through the HTTP API.

---

## Development Workflow

### Common Commands

```bash
# ─── Building ────────────────────────────────────
cargo build                    # Debug build (fast compile)
cargo build --release          # Release build (optimized, ~10 MB binary)
cargo build -p tack-core     # Build only one crate

# ─── Testing (nextest — see docs/TESTING.md) ─────
cargo nextest run --workspace                                 # Everything; the summary line carries the count
cargo nextest run --workspace -E 'package(tack-core)'         # One crate — always --workspace, select with -E, never -p
cargo nextest run --workspace -E 'test(workflow)'             # Tests matching a name
cargo nextest run --workspace --no-capture -E 'test(<name>)'  # Show println! output (runs serially)

# Frontend tests
cd frontend && npm test         # Vitest

# ─── Running ─────────────────────────────────────
cargo run -p tack-cli -- serve              # Start the API server
cargo run --bin tack-cli -- --help    # CLI help

# ─── Code Quality ────────────────────────────────
cargo fmt --all                                       # Format all code
cargo fmt --all --check                               # Check formatting (same as CI)
cargo clippy --workspace --all-targets -- -D warnings # Lint (same as CI — --all-targets covers tests too)
cargo check                                           # Type-check without building
make coverage                           # Rust + frontend coverage against CI's thresholds
make deny                               # License + duplicate-dependency check (same policy as CI)

# ─── Debugging ───────────────────────────────────
RUST_LOG=debug cargo run -p tack-cli -- serve           # Debug logging
RUST_LOG=tack_db=trace cargo run -p tack-cli -- serve # Trace SQL queries
TACK_LOG_JSON=true cargo run -p tack-cli -- serve     # JSON log output
```

### Manual API Testing

Once the server is running (`cargo run -p tack-cli -- serve`):

```bash
# 1. Check health
curl -s localhost:3210/api/health | jq

# 2. Create a project
curl -s -X POST localhost:3210/api/projects \
  -H "Content-Type: application/json" \
  -d '{"name":"Test Project","project_type":"software"}' | jq

# 3. Copy the project ID, then create an item
curl -s -X POST localhost:3210/api/projects/<PROJECT_ID>/items \
  -H "Content-Type: application/json" \
  -d '{"title":"My first task","item_type":"task","priority":"high"}' | jq

# 4. List all items
curl -s localhost:3210/api/projects/<PROJECT_ID>/items | jq

# 5. Check DB stats
curl -s localhost:3210/api/debug/db-stats | jq
```

### Writing Tests

**Unit tests** go in the same file as the code, inside a `#[cfg(test)]` module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_feature() {
        let wf = scrum_workflow();
        assert!(wf.validate_transition("Backlog", "In Progress").is_ok());
    }
}
```

**API handler tests** go under `crates/tack-api/tests/handlers/`, in the module whose subject fits, using `axum::Router::oneshot()`:

```rust
#[tokio::test]
async fn my_handler_returns_200() {
    let (app, _) = common::test_app().await;
    let res = app.oneshot(
        Request::builder()
            .method(Method::GET)
            .uri("/api/health")
            .body(Body::empty())
            .unwrap(),
    ).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
```

**DB integration tests** go under `crates/tack-db/tests/repository/` (or `migrations/` for schema work):

```rust
#[tokio::test]
async fn test_my_db_feature() {
    let repo = setup_test_db().await;   // In-memory SQLite
    let ws_id = create_test_workspace(&repo).await;
    let project = repo.create_project(ws_id, CreateProject {
        name: "Test".into(),
        ..Default::default()
    }).await.unwrap();
    assert_eq!(project.name, "Test");
}
```

Each test gets its own isolated in-memory database — no cleanup needed.

---

## Good First Contributions

If you're looking for a focused starting point, these areas are self-contained and well-defined:

| Area | What to do | Files to touch |
| --- | --- | --- |
| New project-type preset | Add a workflow + vocabulary pair for a new domain (e.g. `education`, `events`, `research`) | `workflow.rs`, `vocabulary.rs`, `models.rs` |
| New custom field type | Add a new type variant with validation logic | `tack-core/src/models.rs` (CustomFieldType + validate_value) |
| Vocabulary translation | Add a non-English vocabulary pack for an existing project type | `tack-core/src/vocabulary.rs` |
| CLI output polish | Improve table formatting or add a `--format table\|csv\|json` flag to a command | `tack-cli/src/main.rs` |
| Frontend view polish | Fix a visual edge case, improve empty-state UX, or add keyboard shortcuts | `frontend/src/pages/` or `frontend/src/components/` |
| Test coverage | Add handler tests for an endpoint that only has a smoke test | `crates/tack-api/tests/handlers/` |

The crate layering rule is the main constraint: keep `tack-core` free of I/O and `tack-cli` free of direct DB access (all data goes through the HTTP API). See the Dependency Flow section above.

---

## How To Add a New Feature

### Adding a New Entity (e.g., "TimeEntry")

1. **Define the model** in `crates/tack-core/src/models.rs`
2. **Add a migration** in `crates/tack-db/src/migrations.rs` and add it to `run_all()`
3. **Add a repository module** at `crates/tack-db/src/repo/time_entries.rs`; add `pub mod time_entries;` to `repo.rs`
4. **Add a handler module** at `crates/tack-api/src/handlers/time_entries.rs`; add it to the `use crate::handlers::{...}` import in `router.rs`
5. **Add routes** in `crates/tack-api/src/router.rs`
6. **Write tests** under `crates/tack-api/tests/handlers/`

### Adding a New Workflow Preset

Edit `crates/tack-core/src/workflow.rs`, add a new `pub fn my_workflow() -> WorkflowConfig` function and wire it into `workflow_for_type()`.

### Adding a New Vocabulary Pack

Edit `crates/tack-core/src/vocabulary.rs`, add a new match arm in `vocabulary_for_type()`.

---

## Database

### Schema

SQLite with WAL mode enabled. Tables:

| Table | Purpose |
| --- | --- |
| `workspaces` | Top-level container (one per installation) |
| `projects` | Projects with vocabulary + workflow JSON |
| `sprints` | Sprints/iterations with status lifecycle |
| `items` | Universal work items (tasks, epics, etc.) with hierarchy |
| `dependencies` | Item-to-item relationships (blocks, relates_to, etc.) |
| `roles` | Role/specialty definitions per project |
| `item_roles` | Many-to-many junction between items and roles |
| `comments` | Comments and activity log per item |
| `attachments` | File metadata per item |
| `boards` | Multiple boards per project (grouping, filters) |
| `project_templates` | Reusable project blueprints |
| `custom_field_definitions` | User-defined field types per project |
| `custom_field_values` | Field values per item |
| `items_fts` | FTS5 virtual table — full-text search (auto-synced via triggers) |

### Resetting the Database

```bash
rm tack.db tack.db-shm tack.db-wal
cargo run -p tack-cli -- serve   # migrations re-run automatically
```

### Inspecting the Database

```bash
sqlite3 tack.db
.tables
SELECT * FROM _migrations;
PRAGMA journal_mode;   -- should show "wal"
```

---

## Error Handling

- **`tack-core`** — `CoreError` (thiserror)
- **`tack-db`** — `sqlx::Error` + `DependencyError`
- **`tack-api`** — `ApiError` maps all errors to HTTP status codes:

| Domain Error | HTTP Status |
| --- | --- |
| `ItemNotFound`, `ProjectNotFound` | 404 |
| `InvalidTransition`, `WipLimitExceeded` | 400 |
| `DependencyCycle` | 400 |
| Validation error (validator crate) | 422 |
| `sqlx::Error` (internal) | 500 |

Internal errors log the full cause but return only "Internal server error" to the client.

---

## Logging

All logging uses the `tracing` crate with structured spans.

- **Handlers** — `#[instrument(skip(state))]` auto-creates spans
- **Repository methods** — same instrumentation, logs at `debug` level
- **HTTP middleware** — `TraceLayer` logs every request with method, URI, and duration

```bash
RUST_LOG=error cargo run -p tack-cli -- serve                # errors only
RUST_LOG=tack_db=debug cargo run -p tack-cli -- serve      # debug the DB layer
RUST_LOG=trace cargo run -p tack-cli -- serve                # everything (very verbose)
```

---

## Reporting Bugs & Requesting Features

Issues are tracked on [GitHub Issues](https://github.com/yielab/tack/issues).

- **Bugs:** open a [bug report](https://github.com/yielab/tack/issues/new?template=bug_report.yml).
  Include the Tack version or commit, your OS and Rust version, exact steps to
  reproduce, what you expected, and what happened (with any log output). A minimal
  reproduction is the single most helpful thing you can provide.
- **Features / ideas:** open a
  [feature request](https://github.com/yielab/tack/issues/new?template=feature_request.yml),
  or start a thread in [Discussions](https://github.com/yielab/tack/discussions)
  if it is more open-ended. Describe the problem you are trying to solve, not just
  the solution — it helps us keep Tack small and focused (see [GOVERNANCE.md](GOVERNANCE.md)).
- **Security vulnerabilities:** **do not** open a public issue. Report privately
  via [GitHub Security Advisories](https://github.com/yielab/tack/security/advisories/new)
  or email <info@yielab.com>. See [SECURITY.md](SECURITY.md).

For anything beyond a trivial fix, please open (or find) an issue before writing
code, so the approach can be agreed first and you don't invest effort in a change
that may not fit the roadmap.

---

## Pull Request Process

1. **Discuss first for non-trivial work.** Link your PR to an issue. Docs, tests,
   and small self-contained fixes can go straight to a PR.
2. **Branch** off `develop` (see the branching model below). Keep one logical
   change per PR — smaller PRs are reviewed and merged faster.
3. **Write tests** for any new business logic or bug fix (a regression test that
   fails before your change and passes after). See "Writing Tests" above.
4. **Run the full local gate before pushing:**

   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo nextest run --workspace
   ```

   That is the same single run CI does — every Rust test runs exactly once.
   CI's only extra steps are the two regenerate-and-diff gates (the OpenAPI
   spec and tack-orch's golden files); `./scripts/regen-generated.sh` covers
   the first, and docs/TESTING.md lists both.

   Activating the pre-push hook (`git config core.hooksPath .githooks`) runs the
   fmt + clippy portion automatically.

   If you touched the frontend, CI's `frontend` job also runs three checks
   this doesn't:

   ```bash
   cd frontend
   npm run type-check && npm test
   npm run gen:api && git diff --exit-code src/shared/api/schema.gen.ts  # OpenAPI types drift
   npm run lint:tokens                                                   # no raw color literals
   npm run build                                                         # entry bundle stays < 30 KB gzipped
   ```

5. **Update docs and the changelog.** If you changed behavior, config, or the API,
   update the relevant docs in the same PR and add an entry to the `[Unreleased]`
   section of [CHANGELOG.md](CHANGELOG.md). If you changed an API response shape,
   update the Rust handler **and** the matching frontend types / test mocks.
6. **Write a clear PR description** — what changed and why, how you tested it, and
   any follow-ups. Fill out the [PR template](.github/PULL_REQUEST_TEMPLATE.md).
7. **Keep commit messages clean.** Conventional-commit prefixes (`feat:`, `fix:`,
   `docs:`, `refactor:`, `chore:`, `test:`) are appreciated. **No AI-attribution
   lines** (no `Co-Authored-By` bot trailers) in commit messages.
8. **Review.** The maintainer reviews, may request changes, and merges once CI is
   green and the change is approved. Green CI is required — all jobs
   (`rust`, `frontend`, `docs`, `embed-spa`, `security`, `e2e`) must pass.

By submitting a pull request, you agree that your contribution is licensed under
the project's [MIT License](LICENSE).

---

## Branching Model

Tack uses a simple two-long-lived-branch model:

| Branch | Role |
| --- | --- |
| `main` | Release branch. Tagged releases (`vX.Y.Z`) are cut from here. Kept stable. |
| `develop` | Integration branch. Day-to-day work lands here first. |
| `feat/…`, `fix/…`, `docs/…` | Short-lived topic branches for a single change. |

- **Branch topic branches off `develop`** and open your PR **against `develop`**.
- `main` receives changes from `develop` when a release is prepared; the release
  tag triggers `.github/workflows/release.yml` to build and publish artifacts.
- CI runs on pushes to `main`, `develop`, and `claude/**` branches, and on every
  pull request.
- `tack branch <item-id>` (the CLI) can generate a conventional topic-branch name
  from a Tack work item if you track your work in Tack itself.

---

## Code Style

- Run `cargo fmt --all` before committing (the pre-push hook will catch it anyway)
- Fix all `cargo clippy --workspace --all-targets -- -D warnings` before pushing
- Keep `tack-core` free of I/O dependencies
- Use `#[instrument]` on public async functions for tracing
- Prefer returning `Result` over panicking
- Write tests for any new business logic
- No AI attribution lines in commit messages
