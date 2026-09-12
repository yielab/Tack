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

- **`.githooks/pre-push`** runs the comment and test-hygiene checks, `cargo fmt --all --check` for the root workspace *and*, separately, for `crates/tack-desktop` (its own workspace), `cargo clippy --workspace --all-targets -- -D warnings`, and a check that `Cargo.lock` and (when `frontend/node_modules` exists) `schema.gen.ts` are not stale — not the test suite. This mirrors most of what CI's `rust` job checks before the test run, so failures are caught locally before they reach GitHub. See "Pull Request Process" below for the exact command.
- **The `tack-generated` merge driver** for `Cargo.lock`, `frontend/package-lock.json`, `docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts`. Each is a pure function of sources tracked elsewhere, so hand-merging one is always either busywork or a mistake. The driver resolves them without a conflict and `.githooks/post-merge` regenerates them from the merged sources — staged, never committed for you. `scripts/regen-generated.sh` is the same regeneration, runnable by hand.

`rust-toolchain.toml` pins the exact compiler both you and CI use, and rustup installs it with `rustfmt` and `clippy` the first time you run `cargo` here — you do not need to select a toolchain yourself, and you should not override it. Bumping that pin is a deliberate one-line change that Dependabot proposes monthly; it can surface new clippy lints, which is precisely why it is not left to whatever day upstream ships a release.

Skipping the setup leaves you with git's ordinary text merge on those files. That is how the repository behaved before, so a clone without it is degraded, not broken.

---

## Requirements

| Tool | Version | Install |
| --- | --- | --- |
| Rust | 1.94+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
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
│   │   │   ├── migrations.rs   # 62 schema migrations (auto-run on startup; live count is
│   │   │   │                  #   GET /api/health's migrations_applied)
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
│   │       ├── repository/         # CRUD, retention, concurrency
│   │       ├── migrations/         # schema/upgrade tests
│   │       └── perf_test.rs        # 50k-item perf test (#[ignore])
│   ├── tack-orch/            # Agent-fleet ControlPlane client + the neutral runner-v1
│   │   │                     # execution domain. Depends on core+db only — must never
│   │   │                     # depend on tack-api (tack-api depends on this crate).
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── reconciler.rs               # Polls each control plane, drives health state
│   │       ├── adapters/                   # docket adapter + a Prometheus /metrics parser
│   │       ├── execution/                  # Transport-free runner-v1 protocol types
│   │       ├── scheduler/                  # Pure runner-selection decision library
│   │       ├── model_policy/               # Deterministic model-selection precedence
│   │       ├── execution_retention.rs      # Cancellable stale/terminal-row sweep
│   │       ├── execution_observability.rs  # Id-free fleet health snapshot + alerts
│   │       └── usage_provenance.rs         # Requested-vs-actual model + usage economics
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
│   │   │       ├── export.rs           # JSON/YAML/CSV export + JSON/YAML import
│   │   │       ├── import_github.rs    # GitHub Issues import
│   │   │       ├── import_linear.rs    # Linear import
│   │   │       ├── items.rs
│   │   │       ├── projects.rs
│   │   │       ├── roles.rs
│   │   │       ├── spa.rs              # SPA fallback (--features embed-spa)
│   │   │       ├── sprints.rs
│   │   │       ├── templates.rs
│   │   │       ├── websocket.rs
│   │   │       ├── orch.rs, decisions.rs, executions.rs, runner_admin.rs,
│   │   │       │   provisioning.rs, economics.rs, settings.rs, attempt_lists.rs,
│   │   │       │   local_runner.rs   # operator execution/fleet/orchestration surface
│   │   │       └── runner_protocol.rs + runner_protocol/  # /api/runner/v1, its own
│   │   │                              # per-handler hashed-credential auth (runner_auth.rs)
│   │   └── tests/
│   │       ├── common/mod.rs       # test_app(), test_app_with_config()
│   │       ├── handlers/           # CRUD, routes, economics, provisioning (one file per area)
│   │       ├── orchestration/      # dispatch, approvals, reconciler, fleet
│   │       ├── runner_protocol/    # lifecycle, decisions, artifact events
│   │       ├── security/           # auth surfaces, CORS, write races
│   │       ├── wiring/             # proofs that a seam is load-bearing
│   │       ├── openapi_contract.rs # spec-drift gate — regenerates and diffs docs/openapi.json
│   │       └── wave2_gate.rs       # named CI gate, kept its own binary
│   ├── tack-runner/          # Pull-based execution runner — its OWN binary, separate
│   │   │                     # from `tack`. Owns local credentials, workspace, journal,
│   │   │                     # and the harness subprocess `tack-api` must never touch.
│   │   └── src/
│   │       ├── main.rs, lib.rs
│   │       ├── engine.rs        # Per-attempt lifecycle driving one HarnessAdapter
│   │       ├── client.rs        # Polls /api/runner/v1: enroll, claim, heartbeat, …
│   │       ├── journal.rs       # Owner-only TOML journal written before spawn
│   │       ├── workspace.rs     # Isolated per-attempt workspace/worktree
│   │       ├── secrets.rs       # Local vendor credential storage
│   │       └── harness/         # process.rs, event_sink.rs, redact.rs, artifact.rs, and
│   │                            # one module per harness: codex.rs, claude_code.rs
│   └── tack-cli/             # clap CLI (talks to API over HTTP, never opens the DB)
│       └── src/
│           ├── main.rs         # `Commands` enum + dispatch
│           ├── client.rs       # HTTP client wrapper (reqwest)
│           ├── config.rs       # ~/.tackrc reader
│           ├── mcp.rs          # `tack mcp` — MCP server over stdio
│           ├── service.rs      # `tack service` — systemd/launchd background service
│           ├── execution.rs    # `tack execution`/`fleet`/`runner` client commands
│           ├── doctor.rs       # `tack doctor` diagnostics
│           └── vocab.rs        # Vocabulary-aware output
├── frontend/
│   ├── src/
│   │   ├── app/                # Root App/Layout components and routes.tsx
│   │   ├── features/           # One directory per feature: board, list, sprints, fleet,
│   │   │                       # agents, provisioning, economics, settings, …
│   │   ├── shared/              # Cross-feature: api/ (generated client + schema.gen.ts),
│   │   │                       # realtime/, orch/, execution/, ui/, state/, vocab/
│   │   └── test/               # Vitest setup
│   ├── e2e/                    # Playwright specs
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
     +-------------------+
     |                   |
tack-orch (depends    tack-api   (depends on core + db + orch, adds HTTP;
  on core + db only;      spawns orch's reconciler and mounts its routes)
  must never depend
  on tack-api)

tack-cli     (depends on core only — talks to tack-api over HTTP, no DB)
tack-runner  (its own binary; talks to tack-api's /api/runner/v1 over HTTP,
             no DB — owns local credentials and the harness subprocess instead)
```

**Rule:** `tack-core` must never import `tack-db` or any I/O crate.
Keep business logic testable without a database. `tack-orch` depends inward on
`tack-core`/`tack-db` only and must never depend on `tack-api` — `tack-api` depends
on `tack-orch`, not the reverse. `tack-cli` and `tack-runner` must never import
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

Every test follows the size rules in `docs/TESTING.md` ("Where a test lives, and how big
it may be"): one claim per test, a name of at most 60 characters that states it, a body of
at most 60 lines, helpers in `tests/common`, no fixed waits.
`python3 scripts/maintainability.py check --changed` tells you before the push does.

**Unit tests** go in the same file as the code, inside a `#[cfg(test)]` module, while that
module is under 150 lines; past that they move to `<module>/tests.rs`
(`#[cfg(test)] mod tests;` in the source file, `use super::*;` at the top of the new one):

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
| Frontend view polish | Fix a visual edge case, improve empty-state UX, or add keyboard shortcuts | `frontend/src/features/` (the relevant feature) or `frontend/src/shared/ui/` |
| Test coverage | Add handler tests for an endpoint that only has a smoke test | `crates/tack-api/tests/handlers/` |

The crate layering rule is the main constraint: keep `tack-core` free of I/O and `tack-cli` free of direct DB access (all data goes through the HTTP API). See the Dependency Flow section above.

---

## How To Add a New Feature

### Adding a New Entity (e.g., "TimeEntry")

1. **Define the model** in `crates/tack-core/src/models.rs`
2. **Add a migration** in `crates/tack-db/src/migrations.rs` and add it to `all_migrations()`
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
4. **Run the full local gate before pushing.** Running the hook script directly is the
   most reliable way — it is the definition CI's `rust` job checks against, so unlike a
   hand-typed command list it cannot quietly drift from it:

   ```bash
   ./.githooks/pre-push
   ```

   That runs, in order: the comment and test-hygiene checks, `cargo fmt --all --check`
   for the root workspace *and*, separately, for `crates/tack-desktop` (its own
   workspace, excluded from the root one), `cargo clippy --workspace --all-targets --
   -D warnings`, and the lockfile / `schema.gen.ts` freshness checks (the latter only
   when `frontend/node_modules` exists) — **not the test suite**, on purpose. Run that
   yourself:

   ```bash
   cargo nextest run --workspace
   ```

   Together these reproduce everything CI's `rust` job checks except two steps that
   regenerate a committed artifact from the code and diff it, rather than testing
   something new — the OpenAPI spec and tack-orch's golden files, each rerun through a
   targeted filter. `./scripts/regen-generated.sh` covers the first; docs/TESTING.md's
   Continuous Integration section describes both.

   Activating the hook (`git config core.hooksPath .githooks` — done once by
   `./scripts/setup-git.sh`) runs it automatically on every `git push`.

   If you touched the frontend, CI's `frontend` job also runs checks the hook doesn't:

   ```bash
   cd frontend
   npm run type-check && npm test
   npm run gen:api && git diff --exit-code src/shared/api/schema.gen.ts  # OpenAPI types drift
   npm run lint:tokens                                                   # no raw color literals
   npm run build                                                         # entry bundle stays < 30 KB gzipped
   ```

5. **Update docs.** If you changed behavior, config, or the API, update the relevant docs
   in the same PR. Do **not** edit [CHANGELOG.md](CHANGELOG.md): its release sections are
   generated from commit messages by [git-cliff](https://git-cliff.org) (`cliff.toml`,
   `make changelog` to preview). If you changed an API response shape, update the Rust
   handler **and** the matching frontend types / test mocks.
6. **Write a clear PR description** — what changed and why, how you tested it, and
   any follow-ups. Fill out the [PR template](.github/PULL_REQUEST_TEMPLATE.md).
7. **Write conventional commit messages — they are the changelog.** `type(scope): what
   changed`, where the type picks the section a user reads:

   | Type | CHANGELOG section |
   |---|---|
   | `feat` | Added |
   | `fix` | Fixed |
   | `refactor`, `perf`, `api` | Changed |
   | `revert` | Removed |
   | `security` | Security |
   | `docs(book\|readme\|config\|api\|mcp\|deploy\|cli\|install\|desktop)` | Documentation |
   | `test`, `build`, `ci`, `chore`, `style`, other `docs` scopes | not in the changelog |

   A `!` after the type (`feat(api)!:`) marks a breaking change and is listed first. The
   subject should read as a changelog line on its own — say what a user can now do, not
   which file moved. A commit that does not follow the form never reaches the changelog.
   **No AI-attribution lines** (no `Co-Authored-By` bot trailers) in commit messages.
8. **Review.** The maintainer reviews, may request changes, and merges once CI is
   green and the change is approved. Green CI is required — all ten jobs in
   `.github/workflows/ci.yml` (`rust`, `msrv`, `desktop`, `coverage`, `deny`,
   `frontend`, `docs`, `embed-spa`, `security`, `e2e`) must pass.

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
- `main` receives changes from `develop` when a release is prepared; pushing a
  version tag (`git tag v0.1.0-beta.N && git push origin v0.1.0-beta.N`) triggers
  `.github/workflows/release.yml`, which builds single-binary distributions for
  Linux/macOS/Windows and attaches them to a GitHub Release with checksums, SBOMs,
  and build-provenance attestations. This is a maintainer action, not something a
  contributor's PR does.
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
