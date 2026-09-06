# Roadmap

> **Where this project actually stands, as of 2026-08-30.** Phases 0–56 are delivered and
> merged on `develop`. **Nothing since `v0.1.0-beta.6` (2026-06-22) has ever been released**,
> so the entire runner fleet — the thing that distinguishes Tack — is not downloadable by
> anyone. Phase 57's tag is refused: the live smoke fails, and `codex` has never completed a
> live attempt. Two cycles are active in parallel: **Phase 58** (`tack serve --with-runner`,
> packaging and first run) and **Phase 59** (adoption and the first real public release,
> opened by the audit of 2026-08-30). Both are carded in [TODO.md](../../../TODO.md), Parts
> IV and V. Sections below this banner describe **intent**; the boards describe what shipped.
>
> The rest of this file is long and mostly closed-phase history. If you are picking work up
> cold, read the two _Next_ sections at the end — _Standalone Single-Binary Operation_ and
> _Adoption & First Public Release_ — and skip everything between.

**Current version:** 0.1.0-beta.6 (a large body of unreleased work is pending → `v0.1.0-beta.7`)  
**Status:** All thirteen engineering phases complete, plus competitive/growth phases
20 (MCP server), 22 (dev-native CLI), 23 (Table view), 24 (positioning & presets),
and 25 (local-first). A full-repo audit (July 2026) produced the **audit-driven
Phases 26–32**, which are now **implemented and verified green** (244 Rust tests,
169 Vitest, clippy clean, frontend builds) — see the status board below. The work
is staged for release as `v0.1.0-beta.7`.

**Next cycle (added August 2026): Phases 33–38 — the Agent-Factory Control Center.**
Tack becomes the control panel for a factory of products built by
[docket](https://github.com/yielab/docket) agent fleets: a new `tack-orch` crate with
a `ControlPlane` trait and a pull-based reconciler, six new tables, dispatch from the
board, a fleet-wide approvals inbox, one-click product+pod provisioning, and
per-product unit economics. Executable task cards for parallel agents are in
[TODO.md](../../../TODO.md); the reciprocal docket-side work is Phase 22 of that
project's `ROADMAP.md`. **That cycle is complete** — all six phases shipped 2026-08-05.

**Historical cycle (partially implemented August 2026): Phases 39–49 — the Agnostic
Control Plane.**
Phases 33–38 built a control center against exactly one backend. This cycle makes it true
of any backend: capability negotiation so the UI disables what a provider cannot do and
names why, a `ControlPlane` trait with no docket nouns left in it, a **GitHub Actions
adapter** chosen because it shares none of docket's shape, an inbound telemetry channel
pushed from inside a run, per-item model choice owned by Tack, and the GitHub pipeline
finished in both directions (which closes Phase 21). Full plan with per-item verification
commands in [docs/plans/agnostic-control-plane.md](../../plans/agnostic-control-plane.md);
task cards in [TODO.md](../../../TODO.md), Part II.

**Status correction (2026-08-06):** Phases 39–42 exist in the current unreleased working
tree, but Phase 41's atomic-write acceptance and Phase 42's provider-scoped identity are
reopened. Phases 43–49 are frozen/superseded by the harness-agnostic runner plan appended
at the bottom of this roadmap. Their text is intentionally retained as design history.

## Audit-driven cycle (Phases 26–32) — status board

| Phase | Title | Status | Deferred within phase |
|---|---|---|---|
| 26 | Correctness Hotfix | ✅ Done | — (release cut/tag itself pending) |
| 27 | Security Hardening | ✅ Done | 27.1 shipped a shared-secret Alexa gate instead of full X.509 cert-chain validation (size-budget trade-off, documented) |
| 28 | Backup → Sync v2 | ✅ Done (28.1–28.5) | 28.6 client-side bundle encryption — deliberately deferred (crypto-dep size cost) |
| 29 | Contract-First API | ✅ Done (29.1, 29.3–29.6) | 29.2 partial: pagination + error-envelope shipped; full handler response-typing + 201/204 normalization still pending (~20 endpoints still return ad-hoc JSON, modeled as `Object` in the spec) |
| 30 | Vocabulary & Nav | ✅ Done | — |
| 31 | Construction Verticals | ✅ Done | — |
| 32 | Enterprise OSS Standards | ✅ Done (32.1–32.5) | 32.6 binary-size guard (tokio `full` trim, feature-gate `object_store`, CI size gate) not started; retroactive git tags for beta.1–5 pending |

**Still open after this cycle:** Phase 21 inbound GitHub sync (webhook/poll + comment mirroring) — now **scheduled as Phases 47 and 49**; the four deferred sub-tasks above (28.6, 29.2 typing, 32.6, historical tags). Everything else in 26–32 is code-complete and tested.

---

## Completed

### Phase 0 — Repo Hygiene
Dead code removed. Docs consolidated. Status and architecture accurately documented.

### Phase 1 — CI & Security Baseline
- GitHub Actions: fmt + clippy + test + frontend typecheck + build + bundle size gate
- CORS allow-list (`TACK_ALLOWED_ORIGINS`)
- Global body-size limit + 50 MB upload cap
- Optional Bearer token auth (`TACK_API_TOKEN`)
- Input validation via `validator` on all Create/Update DTOs

### Phase 2 — Architecture Correctness
- Workflow validation moved into `tack-core` (was in DB layer)
- Dual-board system removed — one `boards` table
- CLI rewritten to use the HTTP API (no direct DB access)
- Import implemented (JSON round-trip with ID remapping)
- `assignee` field added to Item model

### Phase 3 — Product Depth
- CLI: sprint commands, shell completions, vocabulary-aware output, `--json` on all commands
- Settings UI: live vocabulary editor, workflow column editor
- Performance: sprint index, lazy-loaded routes, 22 KB entry bundle

### Phase 4 — Release Readiness
- Backup/restore: `GET /api/backup` (VACUUM INTO), `POST /api/restore` (staged), CLI commands
- Observability: `/api/health` with migration count, all handlers instrumented with `#[instrument]`
- Single-binary: `--features embed-spa` embeds SPA into the release binary (~10 MB)

### Phase 5 — Frontend View Consolidation
- "Group By" removed; Board always shows columns by workflow status — no setting to configure
- Board derives columns from the project workflow directly; no saved board object required
- Settings → Boards CRUD panel and `BoardSelector` removed
- Board drag = status change with WIP limit enforcement on drop
- Tree view deleted; List gains a Hierarchy toggle (indent by `parent_id`, expand/collapse all)
- Calendar and Timeline made interactive: drag to reschedule items / resize to set date range
- Sprint rebuilt as a two-pane planning surface: Backlog ↔ sprint lanes with capacity/burndown
- Sprint promoted to a work tab alongside Board / List / Timeline / Calendar
- All five views share one data source via `ProjectItemsContext` — no redundant fetches

### Phase 6 — Frontend Tests
- 106 Vitest unit tests across 17 test files — all passing
- Coverage: API client contracts, `deriveBoard` pure function, context providers, settings panels, CSV import UI
- Playwright end-to-end tests: deferred (golden-path covered by Rust handler tests)

### Phase 7 — Template Management Depth
- Template creator: vocabulary, workflow, custom-field, and board sections — "Coming Soon" placeholder gone
- `POST /api/projects/{id}/save-as-template` snapshots vocabulary, workflow, custom-field definitions, and boards
- "Save as template" dialog in project Settings routes to the Templates gallery on completion
- Built-in templates seeded for every `ProjectType` on first run (`is_builtin = true`, not deletable)
- Templates gallery groups built-in vs user templates; each card shows status count, vocab overrides, field count, board count
- CLI: `template list`, `template show <id>`, `template create-from <id>` with `--json`
- Template payload validation: at least one status per category, no duplicate names, Select/MultiSelect require options

### Phase 8 — Custom Field Validation + Alexa Voice Integration
- `CustomFieldDefinition::validate_value()` in `tack-core`: enforces type (string, number, boolean, date, URL, select option membership, multi-select array), plus JSON-configured rules (`pattern`, `min_length`, `max_length`, `min`, `max`, `max_items`)
- `set_field_value` handler returns `422 Unprocessable Entity` on validation failure instead of silently storing bad data; 28 new unit tests in `tack-core`
- `POST /api/alexa` custom-skill endpoint: maps voice intents onto existing item/workflow logic
  - **AddTaskIntent** — creates item at the initial workflow status
  - **ListTasksIntent** — speaks open-item count and first few titles
  - **CompleteTaskIntent** — moves matching open item to first Done status; enforces transition rules and WIP limits; propagates parent auto-completion
- Alexa endpoint authentication: constant-time skill-ID comparison + ±150 s timestamp replay guard
- Vocabulary-aware spoken responses (construction projects say "Work Order", not "task")
- Endpoint is exempt from the Bearer-token gate; disabled (404) when `TACK_ALEXA_SKILL_ID` is unset
- Board view applies per-board item filters on fetch (replaces TODO no-op)
- 13 new Alexa handler tests covering verification, all intents, and edge cases

### Phase 9 — Full Integration Test Coverage

- API integration tests expanded from 16 → 36 (sprints, roles, comments, dependencies, search, JSON/CSV export, item update/delete)
- Frontend utility tests: vocab resolution, lens persistence, keyboard manager, optimistic-update rollback — total 144 Vitest tests across 21 files
- Vitest config cleaned up for vite 8 + OXC pipeline (removed stale esbuild shim)

### Phase 10 — Webhook Notifications

- `TACK_WEBHOOK_URL` — when set, POSTs JSON events for every item create/update/delete, sprint status transition, and items due within the next hour
- `TACK_WEBHOOK_SECRET` — optional HMAC-SHA256 signing; adds `X-Tack-Signature: sha256=<hex>` so receivers can verify authenticity
- Background task fires `item.due_soon` once per hour for incomplete items whose `due_date` falls in the next 60 minutes
- Delivery is fire-and-forget (tokio::spawn); errors are logged but never fail the originating request
- All event payloads include `event`, `timestamp`, and `project_id` fields

### Phase 11 — GitHub Issues Import

- `POST /api/projects/{id}/import-github` — fetches issues from any public (or token-accessible private) GitHub repository and creates Tack items
- Request body: `repo` (owner/repo or full URL), `token` (optional PAT), `import_closed` (default false), `label_filter` (optional label allow-list)
- Pull requests are silently skipped; closed issues map to the first Done-category status
- Pagination handled automatically (100 issues/request until the last page)
- Response includes `created`, `skipped`, and `rate_limit_remaining` counts
- 7 unit tests for URL parsing covering all input forms

### Phase 12 — Linear Import

- `POST /api/projects/{id}/import-linear` — fetches issues from Linear's GraphQL API and creates Tack items
- Request body: `api_key` (Linear personal API key), `team_id` (optional slug or ID), `project_id` (optional; overrides `team_id`), `import_completed` (default false), `label_filter` (optional label allow-list)
- Priority mapping: Urgent→Critical, High→High, Medium→Medium, Low→Low; No priority→unset
- Cursor-based pagination (50 issues per request)
- Response includes `created` and `skipped` counts
- 6 unit tests covering filter generation, cursor inclusion, project-vs-team precedence, priority mapping, and cursor injection sanitisation

### Phase 13 — Remote Cloud Backup (S3-Compatible)

**Goal:** Let a user back up their entire Tack instance to any S3-compatible object
store (Cloudflare R2 / Backblaze B2 free tiers, AWS S3, or a self-hosted MinIO) and
restore it on a different machine — so the same data is reusable across local
installations. This is **snapshot replication**, not a live shared database: semantics
are "one active writer at a time, last upload wins." It reuses the existing
`VACUUM INTO` backup flow ([backup.rs](../../../crates/tack-api/src/handlers/backup.rs))
and the existing staged-restore-on-startup mechanism
([main.rs](../../../crates/tack-api/src/main.rs)).

**Design decisions (locked):**

- Provider-agnostic via the [`object_store`](https://docs.rs/object_store) crate — a
  free provider and a custom server are the same thing (an endpoint + access keys),
  so there is exactly one code path. No per-provider SDKs.
- The backup bundle **includes uploaded attachments**, not just the database, because
  attachments live on disk in `TACK_STORAGE_DIR` (not in SQLite) and would otherwise
  be lost on cross-machine restore.
- Keep it simple: no incremental/delta backups, no encryption in v1 (documented as a
  follow-up), no multi-writer conflict resolution.

#### Task 1 — Dependencies

Add to the workspace `Cargo.toml` `[workspace.dependencies]` and wire into
`crates/tack-api/Cargo.toml`:

- `object_store = { version = "0.11", features = ["aws"] }` — the `aws` feature speaks
  the S3 API and supports custom endpoints (R2/B2/MinIO).
- `tar = "0.4"` and `zstd = "0.13"` — to bundle DB + attachments into one compressed archive.
- `bytes` is already transitively available via axum; add it explicitly if needed.

#### Task 2 — Bundle format

A single artifact `tack-backup-<UTC-RFC3339>.tar.zst` containing:

- `database.db` — the `VACUUM INTO` snapshot (reuse the existing helper in
  `handlers/backup.rs`; factor the snapshot-to-bytes logic into a reusable function).
- `attachments/…` — a recursive copy of `TACK_STORAGE_DIR` (skip if the dir is empty
  or unset).
- `manifest.json` — `{ "format_version": 1, "created_at": <rfc3339>, "migration_version": <u32>, "db_sha256": <hex>, "install_id": <uuid>, "item_count": <u64> }`.
  `migration_version` = the count of applied rows in the `_migrations` table.
  `install_id` = a UUID persisted once in a new `app_meta` row (or a sidecar file next
  to the DB); generate on first run if absent.

#### Task 3 — New module `crates/tack-api/src/remote_backup.rs`

Pure-ish module, unit-testable. Public surface:

```rust
pub struct RemoteBackupConfig { /* from AppConfig, Task 4 */ }
pub fn store_from_config(cfg: &AppConfig) -> Result<Arc<dyn ObjectStore>, BackupError>;
pub async fn create_bundle(pool: &SqlitePool, storage_dir: &str) -> Result<Vec<u8>, BackupError>;
pub async fn upload(store: &dyn ObjectStore, prefix: &str, bytes: Vec<u8>) -> Result<BackupManifest, BackupError>;
pub async fn list(store: &dyn ObjectStore, prefix: &str) -> Result<Vec<BackupManifest>, BackupError>;
pub async fn download(store: &dyn ObjectStore, key: &str) -> Result<Vec<u8>, BackupError>;
pub async fn prune(store: &dyn ObjectStore, prefix: &str, keep: usize) -> Result<usize, BackupError>;
```

- Build the `AmazonS3` store with `AmazonS3Builder` — set `endpoint`, `region`, bucket,
  access key, secret key from config; call `.with_allow_http(true)` only when the
  endpoint is plain `http://` (local MinIO).
- `list` reads each object's key + the small `manifest.json` it stores **alongside** the
  archive (`<archive>.manifest.json`) so listing never downloads the full bundle.
- Define a `BackupError` (`thiserror`) mapped to HTTP 5xx, plus a 400 for "remote backup
  not configured."

#### Task 4 — Config ([config.rs](../../../crates/tack-api/src/config.rs))

Add fields to `AppConfig`, loaded from `tack.toml` and `TACK_BACKUP_*` env vars
(same precedence pattern as existing config):

| Env var | TOML key | Type | Default | Notes |
| --- | --- | --- | --- | --- |
| `TACK_BACKUP_ENDPOINT` | `backup.endpoint` | `Option<String>` | none | e.g. `https://<acct>.r2.cloudflarestorage.com` |
| `TACK_BACKUP_BUCKET` | `backup.bucket` | `Option<String>` | none | required to enable |
| `TACK_BACKUP_REGION` | `backup.region` | `String` | `auto` | R2 uses `auto` |
| `TACK_BACKUP_ACCESS_KEY` | `backup.access_key` | `Option<String>` | none | |
| `TACK_BACKUP_SECRET_KEY` | `backup.secret_key` | `Option<String>` | none | never log |
| `TACK_BACKUP_PREFIX` | `backup.prefix` | `String` | `tack` | object key prefix |
| `TACK_BACKUP_INTERVAL_SECS` | `backup.interval_secs` | `Option<u64>` | none | omit = manual only |
| `TACK_BACKUP_RETENTION` | `backup.retention` | `usize` | `10` | keep newest N |

Add a helper `AppConfig::remote_backup_enabled() -> bool` (true when bucket + access
key + secret key are all set). Document every key in `CLAUDE.md` and the deployment guide.

#### Task 5 — Endpoints ([router.rs](../../../crates/tack-api/src/router.rs) + `handlers/backup.rs`)

All gated behind `remote_backup_enabled()` (return `409 Conflict` with a clear message
when disabled). All subject to the existing Bearer-token middleware.

- `POST /api/backup/remote` → `create_bundle` + `upload` + `prune`; returns the new
  `BackupManifest` as JSON.
- `GET  /api/backup/remote` → `list`; returns `[BackupManifest]` newest-first.
- `POST /api/backup/remote/restore` → body `{ "key": "<optional; defaults to latest>" }`;
  download, **validate `migration_version` ≤ the running binary's migration count**
  (reject newer snapshots with `409` to prevent corruption), then stage (Task 6).
  Returns `{ "staged": true, "restart_required": true }`.

#### Task 6 — Atomic staged restore (extend [main.rs](../../../crates/tack-api/src/main.rs))

The current code stages a `.restore` DB file applied on next startup. Extend it so the
**DB and attachments swap together**:

- On restore request: unpack the bundle to `<db>.restore` (database) and
  `<storage_dir>.restore/` (attachments).
- Extend `apply_staged_restore()` (startup) to, when `.restore` artifacts exist:
  move current DB → `<db>.bak`, current storage dir → `<storage_dir>.bak`, then promote
  the `.restore` artifacts atomically. Log every move at `warn` for auditability.
- Because migrations auto-run forward on startup, restoring an **older** snapshot is
  safe; the Task 5 version guard blocks the unsafe **newer** direction.

#### Task 7 — Scheduler (in `main.rs`, mirror the webhook due-soon loop)

When `interval_secs` is set, `tokio::spawn` a loop that sleeps the interval, calls the
backup path, prunes to `retention`, and logs success/failure (never panics the server).
Model it on the existing hourly `item.due_soon` background task.

#### Task 8 — CLI ([tack-cli/src/main.rs](../../../crates/tack-cli/src/main.rs))

Thin wrappers over the new endpoints (CLI talks HTTP, never the DB directly):

- `tack backup --remote` → `POST /api/backup/remote`, print manifest.
- `tack backups` → `GET /api/backup/remote`, print a table (date, size, item count, key).
- `tack restore --remote [--key <key>]` → `POST /api/backup/remote/restore`; print the
  "restart the server to apply" notice. Support `--json` like every other command.

#### Task 9 — Tests

- `remote_backup.rs` unit tests: bundle round-trips (create → unpack → identical DB bytes +
  attachment tree), manifest serialization, prune keeps newest N, version-guard logic.
- Handler tests: `409` when disabled; restore rejects a manifest whose `migration_version`
  exceeds the local count. Use a mock/in-memory `ObjectStore` (object_store ships an
  in-memory backend) so no network is required.
- CLI test: arg parsing for the new `--remote`/`--key` flags.

#### Acceptance criteria

- With env vars pointing at any S3-compatible bucket, `tack backup --remote` uploads a
  `.tar.zst` bundle + sidecar manifest; `tack backups` lists it.
- On a second, empty install pointed at the same bucket, `tack restore --remote`
  followed by a server restart reproduces the **items, sprints, and attachments** of the
  source install.
- Restoring a snapshot created by a newer schema is rejected, not silently applied.
- `cargo test --workspace` and `cargo clippy` pass; no secrets are logged.

#### Out of scope (explicit follow-ups, do not implement here)

- Client-side encryption of bundles (`age`/AES) before upload.
- Incremental/delta backups and deduplication.
- Live multi-writer sync (would require Turso/libSQL — a separate, larger effort).

---

## Planned

The product is feature-complete for the solo-dev / small-team use case. Phases 20–25
are **competitive / growth** work, driven by a deep competitive analysis of Tack
against the Rust and self-hosted PM ecosystem.

**Why these phases (the research, in brief):** No mature Rust-native, web-based,
self-hostable PM tool exists among the leading Jira alternatives — Tack is alone in
its niche. Competitors run other stacks (Plane/Django, Vikunja/Go, Huly/TypeScript,
OpenProject/Rails); the only _actually Rust_ rivals are hobby-scale terminal tools
(taskwarrior-tui, rust_kanban, kanbanban, fulsomenko/kanban). The verified gaps Tack
should close are: a **Table view** (Vikunja), **bi-directional GitHub sync** (Huly),
**AI-agent / MCP** support (Plane, Vibe Kanban), and **git-from-card** dev
conveniences (fulsomenko/kanban). Tack's differentiators to strengthen: the **single
~10 MB binary** (no Postgres/Docker-compose), **per-project vocabulary + domain
presets**, the **web + API + CLI triad**, and the **MIT license** (vs the AGPL/EPL
field). The Rust/self-host community values keyboard-first, offline/local-first,
plaintext, single-binary, low-memory tools — and criticizes heavy Docker-compose
deployments and bloated SPAs. The dominant 2025–26 trend is **AI agents as
first-class PM actors**, which makes Phase 20 the highest-leverage work.

Each phase below carries file paths and acceptance criteria so it can be picked up
cold. Three product decisions gate the dependent tasks — call them before dispatching:

1. **MCP transport** — stdio sidecar (`tack mcp`) vs an HTTP/SSE endpoint in `tack serve`? _(blocks Phase 20)_ — recommend **stdio sidecar** for v1.
2. **GitHub sync scope** — mirror status + comments + close-state, or status only for v1? _(scopes Phase 21)_
3. **Local-first** — is offline/CRDT sync in scope this cycle, or parked? _(gates Phase 25)_

> **Suggested parallelization:** Phase 22 (T1), Phase 24 (T1, T2), and the Phase 20
> decision have no dependencies and can start immediately. Phases 20 and 21 are each
> internally sequential; Phase 23 is a self-contained frontend track.

### Phase 20 — MCP Server (AI-agent-controllable board) ✅ _done_

**Goal:** Expose Tack's board to AI coding agents (Claude Code, Codex, etc.) via the
Model Context Protocol so an agent can read, search, and update items. No self-hosted
PM competitor ships a first-class MCP server, and the architecture fit is strong: it's
a thin protocol layer over the existing REST handlers. Writes go **through the REST
API**, never the DB directly (per the CLI contract), so workflow validation, WIP
limits, and parent-auto-complete all still apply.

> Shipped: `tack mcp` ([mcp.rs](../../../crates/tack-cli/src/mcp.rs)) — a hand-rolled
> JSON-RPC 2.0 stdio server (stdio sidecar transport, decision recorded in
> [docs/MCP.md](../../MCP.md)). Eight tools: `list_projects`, `list_items`,
> `get_item`, `search_items`, `create_item`, `update_item`, `move_item`,
> `add_comment`. 10 unit tests + a live end-to-end session (handshake → tools/list →
> read → create/move/comment, with invalid transitions correctly rejected as tool
> errors). Documented in [docs/MCP.md](../../MCP.md), the README, and the CLI reference.

#### Task 1 — Decide & document transport

Resolve decision #1. Write a `docs/MCP.md` stub comparing a stdio sidecar subcommand
(`tack mcp`, spawned per-agent, talks to `tack serve` over HTTP) vs an HTTP/SSE MCP
endpoint mounted in `tack serve`. Recommend **stdio sidecar** for v1 (simplest for
Claude Code's MCP config; no new auth surface). Get sign-off. **Blocks all other Phase 20 tasks.**

#### Task 2 — Scaffold the `tack mcp` subcommand

Add an `Mcp` subcommand in [tack-cli/src/main.rs](../../../crates/tack-cli/src/main.rs)
and a new `crates/tack-cli/src/mcp.rs`. Prefer the official `rmcp` Rust SDK in
`Cargo.toml`; fall back to a minimal JSON-RPC-over-stdio loop with `serde_json`.
Implement the MCP `initialize` + `tools/list` handshake over stdio. Reuse
[tack-cli/src/client.rs](../../../crates/tack-cli/src/client.rs) to reach `tack serve`,
reading server URL + optional `TACK_API_TOKEN` from
[tack-cli/src/config.rs](../../../crates/tack-cli/src/config.rs).

#### Task 3 — Read tools

`list_projects`, `get_board`, `search_items`, `get_item`. Map each to an existing REST
call (mirror DTOs from `handlers/{projects,boards_multi,items}.rs`; `search_items` uses
the global `/api/search` and per-project search). Return compact JSON (id, title, type,
status, assignee) to keep agent context small.

#### Task 4 — Write tools

`create_item`, `update_item`, `move_item` (status transition), `add_comment`. Route all
writes through the REST API in
[handlers/items.rs](../../../crates/tack-api/src/handlers/items.rs); surface validation
errors back as MCP tool errors. WebSocket broadcast happens server-side automatically.

#### Task 5 — Document & ship config

Expand `docs/MCP.md` with a copy-paste Claude Code `.mcp.json` snippet, a tool reference
table, and a security note (the MCP server inherits the API token — document scoping).
Add a README feature bullet and a `CHANGELOG.md` entry.

#### Acceptance criteria

- `tack mcp` completes an MCP handshake and lists tools (test with an MCP inspector or
  scripted stdin/stdout).
- An agent can enumerate projects, read a board, full-text search, fetch an item, create
  an item, move it through a **valid** transition, get a clear error on an **invalid**
  transition / WIP breach, and add a comment — with the live board updating over the
  existing WebSocket.
- Unit test the JSON-RPC tool dispatch; integration-test writes against an in-memory server.
- A new user can wire Tack into Claude Code in under 5 minutes from `docs/MCP.md`.

### Phase 21 — Bi-Directional GitHub Sync ⏳ _v1 push-only shipped; inbound + comments pending_

**Goal:** Upgrade the existing one-way GitHub _import_ into two-way _sync_ so item
status/comments/close-state mirror back to GitHub — closing the biggest "real work" gap
vs Huly. Scope per decision #2.

> **v1 shipped (push-only, status):** imported items are linked in a `github_links`
> table; completing a linked item closes its GitHub issue (reopening on the way back
> out) when `TACK_GITHUB_TOKEN` is set. Best-effort/fire-and-forget. Pieces:
> migration 018, `repo/github_links.rs`, `github_sync.rs` (`push_issue_state` +
> `state_change`), the `maybe_sync_github` hook in `handlers/items.rs`, and import
> linking. `TACK_GITHUB_API_BASE` makes import+push testable/Enterprise-ready. Tests:
> wiremock push (3), link round-trip, and a full import→complete→close integration
> test — all against a **mocked** GitHub (no real repo touched). Decision recorded in
> [docs/GITHUB-SYNC.md](../../GITHUB-SYNC.md).
>
> **Still pending (future slices):** Task 1 comments-mirroring, Task 4 inbound sync
> (webhook/poll), per-project tokens + manual item linking, and a real-repo live test.
> The task notes below describe the _full_ bi-directional vision.
>
> **Scheduled:** the remaining slices are **Phases 47 and 49** of the Agnostic
> Control Plane cycle. Phase 47 builds the inbound webhook receiver (signature
> verification, delivery dedupe, echo suppression) that Task 4 describes; Phase 49 adds
> comment/label mirroring, per-project credentials, PR and check-run state, and merge
> evidence. Task 2's "backfill links for previously imported items" is covered by
> migration 049's reverse index.

#### Task 1 — Spec the sync model

Write `docs/GITHUB-SYNC.md`: link storage (item ↔ GitHub issue number + repo), conflict
policy (last-write-wins vs GitHub-authoritative), v1 scope (recommend: status +
close-state + comments outbound; inbound via webhook or poll), and push trigger (reuse
the webhook dispatch path in [webhook.rs](../../../crates/tack-api/src/webhook.rs) vs a
periodic reconciler). **Blocks the rest of Phase 21.**

#### Task 2 — Persist the issue↔item link

Add migration #18 in
[tack-db/src/migrations.rs](../../../crates/tack-db/src/migrations.rs) — an
`external_links` table (or `gh_repo` / `gh_issue_number` / `gh_synced_at` columns on
items) — plus a repo module under `crates/tack-db/src/repo/`, following the "Adding a
New Entity" pattern. Backfill links for previously imported items.

#### Task 3 — Outbound push

On a linked item's update, PATCH the GitHub issue (state, optionally labels) and POST
mirrored comments via the GitHub REST API using the stored PAT. Hook into the item
update path in `handlers/items.rs`; new logic in `handlers/import_github.rs` (or a new
`github_sync.rs`). Rate-limit aware; best-effort with logged failures.

#### Task 4 — Inbound sync

Implement either `POST /api/projects/{id}/github/webhook` (verify
`X-Hub-Signature-256`) or a poll-on-interval reconciler, per Task 1. Apply remote
changes through `tack-core` so workflow rules hold. Register the route in
[router.rs](../../../crates/tack-api/src/router.rs).

#### Acceptance criteria

- Moving a linked item to a Done status closes the GitHub issue; adding a comment mirrors
  it (test with a mocked GitHub endpoint).
- Closing an issue on GitHub moves the linked Tack item to Done (test with a captured
  webhook payload fixture).
- Migration runs clean on startup; `cargo test --workspace` and `cargo clippy` pass.

### Phase 22 — Dev-Native CLI (git-from-card)

**Goal:** Make the CLI the surface Rust devs love — cheap, high-signal, and a praised
feature in the Rust kanban field (mirrors fulsomenko/kanban).

#### Task 1 — `tack branch <item-id>` ✅ _done_

Fetch the item via `client.rs`; slugify `type/id-short-title` (e.g.
`feat/a1b2-add-table-view`). Print `git checkout -b <branch>` by default; run it with
`--checkout`; `--json` outputs the name. New `crates/tack-cli/src/git.rs` +
subcommand in `main.rs`. Respect a configurable prefix template. Unit-test the slug
function.

> Shipped: [git.rs](../../../crates/tack-cli/src/git.rs) (`slugify`, `type_prefix`,
> `branch_name`, 8 unit tests) + the `Branch` subcommand and `cmd_branch` in
> `main.rs`. `--prefix` overrides the type-derived prefix (feature→feat, bug→fix,
> …). Documented in the [CLI reference](user-guide/cli.md).

#### Task 2 — `tack open` / smart start

`tack open <id>` opens the item's web URL in `$BROWSER`. Optional `tack start <id>` =
move to the first in-progress status + `tack branch --checkout` in one step. Cover with
CLI help + a smoke test.

#### Acceptance criteria

- `tack branch <id>` prints a sane branch name; `--checkout` creates+switches; `--json`
  works. `cargo test` green.

### Phase 23 — Table View (editable grid) ✅ _Task 1 done_

**Goal:** Add a sortable, column-configurable, inline-editable grid — Vikunja's 4th view
and a common expectation. Parity, not differentiation.

#### Task 1 — Table view component ✅ _done_

New `frontend/src/features/table/` mirroring the structure of
`frontend/src/features/list/`; register the route and nav entry. Reuse the List data
layer and API client. Columns: title, type, status, priority, assignee, custom fields,
due date — headers respect per-project vocabulary. Inline edit → existing item-update
endpoint with optimistic UI. Sort + filter + column show/hide.

> Shipped: [Table.tsx](../../../frontend/src/features/table/Table.tsx) — sixth work
> lens (tab + `/projects/:id/table` route + command palette). Columns title, type,
> status, priority, assignee, due; inline edit of all but type (read-only, since the
> frontend `UpdateItem` doesn't carry `item_type`); sort (priority-rank aware),
> filter, and `localStorage`-persisted column show/hide. Threaded `assignee` through
> the TS `Item`/`UpdateItem` types. 11 Vitest tests for the pure `sortItems`/
> `filterItems`/`typeKey` helpers + 2 Playwright journeys (list/filter, inline-edit
> persistence). **Custom-field columns deferred** — they need per-item value fetches
> (a separate, larger change); the rest of the column set shipped.

#### Task 2 — Density toggle ✅ _done_

Wire the table to the `--density-*` tokens from the design roadmap's Phase 18 if they've
landed; otherwise ship comfortable-only.

> Shipped: a self-contained comfortable/compact toggle on the Table (row padding),
> persisted to `localStorage` — independent of the design roadmap's Phase 18 tokens,
> which can later supersede the local padding values.

#### Acceptance criteria

- Grid renders project items; inline edits persist and broadcast over WebSocket;
  sorting/filtering work. Vitest unit tests + a Playwright journey in `frontend/e2e/`.

### Phase 24 — Positioning & Proof ✅ _done_

**Goal:** Convert real strengths into evidence and messaging. Mostly docs/CI — low cost,
high marketing leverage.

> Shipped: [docs/BENCHMARKS.md](../../BENCHMARKS.md) (measured 10.3 MiB binary,
> ~113 ms cold start, ~12 MiB idle RSS, sub-3 ms p99 reads, with repro steps); a
> [one-line installer](../../../install.sh) verified end-to-end against the real
> v0.1.0-beta.6 release (resolves the platform asset via the GitHub API); README
> install options (`curl | sh`, `cargo install`) + a "Why Tack" comparison table vs
> Plane/Vikunja/Huly + an AI-agent lede; and three new vertical presets (Task 3,
> shipped earlier). Docs cross-linked in the README index.

#### Task 1 — Footprint benchmark ✅ _done_

Extend the existing k6 baseline in `tests/load/`; publish `docs/BENCHMARKS.md` with
cold-start, idle RSS, p50/p99 latency, binary size (embed-spa vs not), and a
deploy-step-count comparison vs a named competitor. Reproducible methodology — replaces
the inferential footprint claims with numbers.

#### Task 2 — One-line install + README positioning

Add a prebuilt-binary install path (`install.sh` curl-pipe-sh and/or `cargo binstall`)
with GitHub Release artifacts in `.github/workflows/`. Rewrite the README lede around
"one binary, one SQLite file, MIT-licensed, AI-agent-ready," with a comparison table
(Tack vs Plane/Vikunja/Huly: deploy complexity, deps, license, CLI, MCP).

#### Task 3 — Vertical workflow presets ✅ _done_

Add ≥2 new `ProjectType` presets (e.g. legal/case, research/lab, editorial/content,
event planning) across
[tack-core/src/workflow.rs](../../../crates/tack-core/src/workflow.rs), `vocabulary.rs`,
and `models.rs`, wiring `workflow_for_type()`, per the "Adding a New Project Type"
guide. Unit-test in `workflow.rs`.

> Shipped: three new types — **`legal`** (Intake → Discovery → Drafting → Review →
> Closed), **`research`** (Hypothesis → Design → Experiment → Analysis → Published),
> **`event`** (Ideas → Booked → In Progress → Confirmed → Done) — each with a
> workflow + 16-key vocabulary preset, a seeded built-in template, and CLI/UI
> selectors. The three duplicated `ProjectType`→string matches in
> `tack-db/repo/templates.rs` were collapsed to `.to_string()` (Display) while
> here. New unit tests in `workflow.rs` and `vocabulary.rs`; verified live (a
> `legal` project resolves task→Filing, board→Docket, assignee→Counsel).

#### Acceptance criteria

- `docs/BENCHMARKS.md` published with reproducible numbers; a user can install Tack with
  one command; README leads with the differentiators; new project types are selectable.

### Phase 25 — Plaintext / Local-First ✅ _done_

**Goal:** Court the plaintext + local-first crowd.

#### Task 1 — YAML/TOML project round-trip ✅ _done_

Extend [handlers/export.rs](../../../crates/tack-api/src/handlers/export.rs) with a
`format=yaml` export and a matching import; stable key ordering for clean git diffs;
reuse the existing round-trip ID remapping. Golden-file test for lossless round-trip.

> Shipped: `export?format=yaml` plus content-negotiated import (YAML when
> `Content-Type` mentions YAML, else JSON — both decode to the same intermediate
> `Value`, so a YAML export round-trips unchanged). serde_json's sorted keys keep
> diffs stable. Exposed in Settings → Data (Export YAML; import accepts `.yaml`/
> `.yml`). Integration test `export_yaml_round_trips_through_import`. Chose YAML over
> TOML because items' many null/Option fields can't serialize to TOML.

#### Task 2 — Offline-capable PWA (spike) ✅ _done_

Time-boxed spike: evaluate a service-worker + IndexedDB cache over the SolidJS SPA, and
whether CRDT sync is worth it vs the WebSocket model. Produce a go/no-go recommendation —
not an implementation.

> Spike complete. Recommendation:
> **NO-GO on CRDT sync** (misaligned with the single-writer/small-team model);
> **conditional-go, low-priority on a read-only offline PWA**; the YAML round-trip
> (Task 1) already captures the high-value, low-cost part of the trend.

#### Acceptance criteria

- YAML export→edit→import round-trips losslessly; the offline-PWA spike reached a
  clear go/no-go.

## Next — Audit-Driven Phases 26–32 (July 2026)

A four-track parallel audit (backend/security, frontend/UX, docs/OSS standards,
backup-sync/API contracts) of the full repo produced these phases. They are ordered
by risk: **Phase 26 is a release blocker** (two shipped UI features silently lose
data); 27 closes security holes; 28–32 are quality/growth. Every task carries file
paths and acceptance criteria so it can be dispatched cold to an agent.

**Headline audit findings (evidence, not opinion):**

| # | Finding | Where | Severity |
|---|---------|-------|----------|
| 1 | `update_item` has **no UPDATE branch for `sprint_id`, `due_date`, `estimate_unit`** — drag-to-sprint (`Sprints.tsx:191`) and due-date edits (`ItemHeader.tsx:133`) are silent no-ops that vanish on refresh | `crates/tack-db/src/repo/items.rs:179-262` | Critical |
| 2 | `started_at`/`completed_at` are **never set on ordinary status moves** (only in parent auto-propagation) — due-soon webhooks keep firing for completed items; cycle-time data is never populated; contradicts CLAUDE.md | `crates/tack-db/src/repo/items.rs:202-208` vs `:401` | Critical |
| 3 | Alexa endpoint authenticates by **skill-ID + timestamp only — no Amazon cert-chain/signature validation**; skill IDs are not secret, so the endpoint is forgeable when enabled | `crates/tack-api/src/handlers/alexa.rs:368-384` | High |
| 4 | No warning when binding non-loopback with no `TACK_API_TOKEN` — unauthenticated read/write API + DB download exposed | `crates/tack-api/src/server.rs:104-108`, `middleware.rs:30-33` | High |
| 5 | `PRAGMA foreign_keys=ON` runs on **one pooled connection**; the other 4 enforce nothing — advertised referential integrity is best-effort | `crates/tack-db/src/lib.rs:24` | High |
| 6 | Backup restore: **tar path traversal** (`attachments/../../…` escapes staging), `db_sha256` **never verified**, stale `-wal`/`-shm` not removed (corruption risk at recovery time), swap can strand an empty DB | `crates/tack-api/src/remote_backup.rs:461-471`, `server.rs:183-218` | High |
| 7 | **S3 secret key + install_id ride inside every backup bundle** (they live in `app_meta`, which `VACUUM INTO` snapshots) — exfiltratable via `GET /api/backup`; restore clones another install's identity | `crates/tack-api/src/handlers/settings.rs` + backup flow | High |
| 8 | Backup **scheduler reads env config only** — UI-saved settings (Settings → Cloud Backup) are ignored by scheduled runs; split-brain destinations | `crates/tack-api/src/server.rs:229-239` vs `effective_backup_config()` | High |
| 9 | Linear import interpolates `team_id`/`project_id` **unescaped into GraphQL** (cursor is sanitized; these aren't) | `crates/tack-api/src/handlers/import_linear.rs:303-312` | Medium |
| 10 | Vocabulary promise half-kept: global "+ New" modal ignores project vocabulary; Sprints view, WorkTabs, command palette, first-run guide hardcode "sprint"/"Story Points" | `frontend/src/app/Layout.tsx:181`, `features/sprints/Sprints.tsx`, `shared/ui/WorkTabs.tsx:11` | High (product) |
| 11 | Breadcrumb broken for Table + Sprints views (stale `tree` key, `sprints` vs `/sprint` mismatch); Table view missing from the sidebar entirely | `frontend/src/shared/ui/Breadcrumb.tsx:5-10`, `Sidebar.tsx:165-170` | Medium |
| 12 | `docs/API-REFERENCE.md` ("canonical") drifts from `router.rs` in 8+ places; `docs/DEPLOYMENT-GUIDE.md` documents a **Docker/compose setup that does not exist in the repo** | docs | High (trust) |
| 13 | No API contract artifact (no OpenAPI/schema); DTOs triple-maintained by hand (Rust ⇄ TS types ⇄ test mocks); two incompatible error JSON shapes; item lists silently truncate at 100 in the UI | `crates/tack-api/src/handlers/*`, `frontend/src/types/` | High (SDD) |
| 14 | Releases ship unsigned (no checksums/SBOM/provenance); SECURITY.md routes disclosure through **public** issues; no CoC; MSRV claim (1.75+) not enforced anywhere | `.github/workflows/release.yml`, `SECURITY.md` | Medium |

**What the audit confirmed is genuinely strong (keep, don't churn):** pure
`tack-core` reused identically by REST/CLI/MCP/Alexa; centralized `CoreError→HTTP`
mapping; disciplined parameterized SQL; constant-time token compare + write-only
secrets; ~82 KB gzip total frontend JS with per-route lazy loading; aggressive
release profile (10.3 MiB binary); 6-job CI with bundle-size, design-token, and
axe gates. The architecture is **not overengineered** — the two size wins available
are trimming `tokio = "full"` features and feature-gating `object_store`.

### Phase 26 — Correctness Hotfix ✅ _done_ (release blocker → cut `v0.1.0-beta.7`)

**Goal:** No shipped surface silently loses user data. Everything here is small,
sequential-free, and must land before any other phase ships.

#### Task 26.1 — Persist the missing `update_item` fields

In [repo/items.rs](../../../crates/tack-db/src/repo/items.rs) add UPDATE branches for
`sprint_id`, `due_date`, `estimate_unit`. Support **null-clears** (the frontend
already sends `null` to clear): switch the Rust `UpdateItem` fields in
[models.rs](../../../crates/tack-core/src/models.rs) to double-`Option`
(`#[serde(default, with = "::serde_with::rust::double_option")]`) so "absent" ≠
"set to null", and thread through the handler. Regression tests: PATCH sprint
assignment persists across a re-fetch; PATCH `due_date: null` clears it.

#### Task 26.2 — Set `started_at` / `completed_at` on status transitions

In the item-update path, when the target status category becomes in-progress and
`started_at` is NULL → set it; when it becomes done → set `completed_at`; when it
leaves done → clear `completed_at`. This fixes `list_items_due_soon`
([items.rs:157](../../../crates/tack-db/src/repo/items.rs)) firing webhooks for
completed items. Assert both timestamps in tests (none exist today — that's why
this shipped broken).

#### Task 26.3 — Enforce foreign keys on every pooled connection

Replace the one-off `PRAGMA foreign_keys=ON` ([lib.rs:24](../../../crates/tack-db/src/lib.rs))
with `SqliteConnectOptions::foreign_keys(true)` (or `after_connect`). Add a test that
an orphaning insert actually rejects.

#### Task 26.4 — Fix broken frontend navigation

[Breadcrumb.tsx](../../../frontend/src/shared/ui/Breadcrumb.tsx): drop stale `tree`,
add `table` + `sprint` keys, fix the `sprints`/`/sprint` mismatch. Add the Table
lens to [Sidebar.tsx](../../../frontend/src/shared/ui/Sidebar.tsx). Pick one label
("Sprints") across WorkTabs / Sidebar / Breadcrumb / document title.

#### Task 26.5 — Alexa create path must validate

`add_task` in [alexa.rs](../../../crates/tack-api/src/handlers/alexa.rs) builds
`CreateItem` and skips `.validate()` — the only mutation path that does. Add it.

#### Task 26.6 — Cut the release

Add the pending UI-redesign + WCAG entries to `CHANGELOG.md [Unreleased]`
(commits `536d959`, `bef2771` are undocumented), fold in 26.1–26.5, tag
`v0.1.0-beta.7`. The Unreleased section currently holds ~10 shipped features —
that backlog is itself a cadence bug.

**Acceptance:** drag an item into a sprint, refresh — it's still there. Set and
clear a due date — persists. Complete an item — `completed_at` set, no due-soon
webhook. `cargo test --workspace` includes new regressions for every fix.

### Phase 27 — Security Hardening ✅ _done_

**Goal:** Safe-by-default for a tool that invites `TACK_HOST=0.0.0.0` + a public
S3 bucket. Independent tasks; parallelize freely.

> Shipped. Note: 27.1 landed a **mandatory shared-secret gate**
> (`TACK_ALEXA_SHARED_SECRET`, constant-time compared, passed via the skill
> endpoint URL) rather than full Amazon `SignatureCertChainUrl` X.509/RSA
> verification — the pure-Rust cert stack was judged too heavy for the ~10 MB
> binary budget. Documented in [docs/ALEXA.md](../../ALEXA.md). All other tasks
> (exposed-bind warning, tar-slip rejection, sha256 + format_version restore
> verification, secret-scrubbed bundles, Linear GraphQL escaping, validation
> stragglers) shipped as specified.

#### Task 27.1 — Alexa request signature validation

Implement Amazon's required `SignatureCertChainUrl` + `Signature` validation
(cert-chain URL allow-list `https://s3.amazonaws.com/echo.api/…`, chain verification,
SAN check for `echo.api`, body-hash compare) in
[alexa.rs](../../../crates/tack-api/src/handlers/alexa.rs), keeping the existing
skill-ID + timestamp checks. If the cert dependency is deemed too heavy for the
binary budget, the fallback is a mandatory shared-secret query param documented in
`docs/ALEXA.md` — but say so explicitly; today's check is forgeable by anyone who
knows the (non-secret) skill ID.

#### Task 27.2 — Refuse/warn on exposed unauthenticated bind

In [server.rs](../../../crates/tack-api/src/server.rs): if the bind host is
non-loopback and `api_token` is `None`, log a prominent warning at startup (and
consider requiring `TACK_INSECURE_NO_AUTH=1` to proceed). Also stop returning
`database_url` from `/api/debug/info` ([debug.rs:40](../../../crates/tack-api/src/handlers/debug.rs)).

#### Task 27.3 — Sanitize tar extraction

In `parse_bundle` ([remote_backup.rs:461-471](../../../crates/tack-api/src/remote_backup.rs)):
reject any entry whose path contains `..`/absolute components (mirror
`tar::Entry::unpack_in` semantics). Test with a malicious bundle fixture.

#### Task 27.4 — Verify backup integrity on restore

`stage_restore` must verify `db_sha256` and `format_version` from the manifest
before staging (both are computed at backup time and ignored at restore time).
Reject mismatches with a clear 409. ~15 lines + tests.

#### Task 27.5 — Escape Linear GraphQL filter fields

Apply the cursor-style sanitization to `team_id`/`project_id` in
[import_linear.rs:303-312](../../../crates/tack-api/src/handlers/import_linear.rs);
add the injection test that exists for cursors but not for these.

#### Task 27.6 — Keep secrets out of backup bundles

Strip (or re-key) `app_meta.backup_config` and `install_id` from the `VACUUM INTO`
snapshot — the S3 secret currently ships inside every local download and remote
bundle, and restoring clones the source install's identity. Simplest: after
snapshotting to the temp file, open it and `DELETE FROM app_meta WHERE key IN (…)`;
on restore, regenerate `install_id`.

#### Task 27.7 — Validation stragglers

Add `validator` derives + `.validate()` calls to `GitHubImportRequest`,
`LinearImportRequest`, board create/update, `UpdateSprintStatus`,
`CreateProjectFromTemplate`, and `UpdateBackupSettings` (retention ≥ 1,
interval ≥ 60 — today `retention: 0` deletes the backup you just made and
`interval_secs: 0` panics the scheduler task).

**Acceptance:** a forged Alexa POST with a known skill ID is rejected; a bundle
with `attachments/../../x` fails restore; a tampered bundle fails the sha check;
`retention=0` returns 422; startup on `0.0.0.0` without a token screams.

### Phase 28 — Backup → Sync v2 (safe personal multi-device) ✅ _done (28.1–28.5); 28.6 deferred_

**Goal:** Upgrade snapshot replication into a **trustworthy** "one active writer,
last upload wins" sync across a user's machines — without building CRDTs (Phase 25
spike already ruled that out). Depends on 27.3/27.4/27.6.

> Shipped: generation-counter conflict detection (persisted in `app_meta`, no
> migration), scheduler honoring runtime UI settings, fail-safe restore swap
> (stale `-wal`/`-shm` cleanup, rollback-on-failure, backup-before-restore),
> sidecar-first + `put_multipart` uploads with orphan reconciliation, a
> `POST /api/backup/remote/verify` preview endpoint wired into the Settings panel,
> and migration-version parity on the local restore path. **28.6 (client-side
> bundle encryption) is deferred** — it would add a crypto dependency against the
> binary-size budget; a `// TODO(phase-28.6)` marks the hook and
> [DEPLOYMENT-GUIDE.md](../../DEPLOYMENT-GUIDE.md) documents the interim guidance
> (private bucket + provider-side encryption-at-rest).

#### Task 28.1 — Scheduler honors runtime (UI) settings

`run_scheduled_backup` ([server.rs:229-239](../../../crates/tack-api/src/server.rs))
must call `effective_backup_config()` each tick instead of captured env config —
today UI-configured backups never schedule, and UI-overridden buckets are silently
ignored by the scheduler. Set `MissedTickBehavior::Delay` while there.

#### Task 28.2 — Generation counter + conflict detection

Add a monotonically increasing `generation` in `app_meta`, bumped on every write
transaction batch (or per-backup), recorded in the manifest. On **upload**: if the
remote head manifest has `generation` ≥ local and a different `install_id`, return
a 409 with both manifests ("another device has uploaded newer work — restore first
or force"). On **restore**: if local `generation` > snapshot's, require
`{"force": true}`. This is the single biggest missing piece for sync semantics.

#### Task 28.3 — Fail-safe restore swap

Make `apply_staged_restore` transactional-ish: delete stale `-wal`/`-shm` files,
roll back to `.bak` if any rename in the sequence fails (today a half-failed swap
boots a brand-new empty DB), keep one timestamped `.bak` generation, and clear
pre-existing `.restore` staging dirs before unpacking (they currently merge across
attempts). Auto-snapshot to the remote (if configured) before staging — "backup
before restore".

#### Task 28.4 — Upload robustness

Upload the sidecar manifest **before** the bundle (or reconcile orphans in
`prune`) — today a failed sidecar PUT leaks an invisible, unprunable bundle
forever. Use `object_store::put_multipart` for bundles over ~32 MB instead of
building everything in RAM (bundle creation currently holds DB + all attachments +
tar + zstd output simultaneously).

#### Task 28.5 — Restore preview + local-path parity

`POST /api/backup/remote/verify` (download + sha + version check, no staging) and
surface it in Settings → Cloud Backup as "Verify". Give the **local** `POST
/api/restore` the same migration-version guard the remote path has (today a local
restore of a newer-schema DB bricks startup).

#### Task 28.6 — Optional bundle encryption (v2 follow-up, keep scoped)

`age`-style symmetric encryption of bundles with a user passphrase stored only in
memory/env. Explicitly out of scope if it threatens the size budget; document
either way in `docs/DEPLOYMENT-GUIDE.md`.

**Acceptance:** two installs pointed at one bucket cannot silently overwrite each
other's newer snapshot; a mid-restore failure leaves the original DB bootable; a
UI-only cloud config schedules backups; 500 MB of attachments backs up without
holding it all in RAM.

### Phase 29 — Contract-First API (the SDD workflow) ✅ _done (29.1, 29.3–29.6); 29.2 partial_

**Goal:** One machine-readable contract, generated from the code, gating CI, and
feeding the frontend types and the docs — making the API-REFERENCE drift class of
bug structurally impossible. Sequence: 29.1 → 29.2 → 29.3 → (29.4 ∥ 29.5 ∥ 29.6).

> Shipped: the error envelope is unified across all handlers and parsed by the
> frontend; item lists return a `{data,total,page,per_page}` envelope (fixing the
> silent truncation at 100) with the CLI, MCP, and SPA consumers updated;
> **`utoipa` generates an OpenAPI 3.1 spec** (68 operations / 43 paths) served at
> `GET /api/openapi.json` and committed to [docs/openapi.json](../../openapi.json)
> — no bundled Swagger UI, and `ToSchema` is feature-gated so `tack-core` stays
> pure. Two CI drift gates lock the chain **handlers → `docs/openapi.json` →
> `frontend/src/shared/api/schema.gen.ts`** (Rust `git diff` gate + frontend
> `gen:api` gate), so neither the spec nor the generated TS client can silently
> diverge. Both API-reference docs now point at the spec as the source of truth
> instead of listing endpoints by hand. **29.2 is partial:** the pagination and
> error-envelope fixes shipped, but the broader refactor of ~59 `Json<Value>`
> handlers to typed response DTOs and the create→201 / delete→204 normalization
> are **not done** — ~20 endpoints are modeled as free-form `Object` in the spec
> until their handlers return named types. That typed-response pass + a `/api/v1`
> prefix is the batched pre-1.0 breaking change.

#### Task 29.1 — Unify the error contract

Migrate `handlers/backup.rs` and `handlers/settings.rs` ad-hoc `{"error": "…"}`
tuples to the structured `ApiError` envelope
([error.rs:85-92](../../../crates/tack-api/src/error.rs)); make the frontend's
`toApiError` ([client.ts:62-70](../../../frontend/src/shared/api/client.ts)) parse
it (users currently see raw JSON in error toasts).

#### Task 29.2 — Type the responses

Replace `Json<serde_json::Value>` returns with typed DTOs across
`handlers/` (~59 REST handlers; the DTOs mostly already exist in `tack-core`).
Standardize: creates → `201` + entity, deletes → `204` (they're currently split
between `{"deleted": true}`/200 and 204 by module). Add a pagination envelope
`{data, total, page, per_page}` to item lists — the UI silently truncates at 100
items today because no total is returned — and thread paging through
[api/items.ts](../../../frontend/src/shared/api/items.ts).

#### Task 29.3 — Generate `openapi.json` with `utoipa`

`utoipa` + `utoipa-axum` derives on DTOs/handlers (behind a feature flag in
`tack-core` so the pure crate stays dependency-light); serve at
`/api/openapi.json`; CI drift gate: regenerate and `git diff --exit-code` a
committed `docs/openapi.json`. Known manual spots: multipart upload, WS endpoint,
`ItemType::Custom` untagged enum.

#### Task 29.4 — Generate the frontend types

`openapi-typescript` in `frontend/scripts/`, replacing the hand-written
`frontend/src/types/` mirrors (the audit found `CreateItem.status` sent but
ignored, `assignee` supported but omitted). Type-check gate stays; hand-written
types remain only for frontend-internal shapes.

#### Task 29.5 — Regenerate the API docs from the spec

Replace the endpoint tables in `docs/API-REFERENCE.md` and
`docs/book/src/developer/api-reference.md` with spec-generated sections (keep the
prose). The audit found 8+ documented endpoints that don't exist (old board routes,
`PATCH/DELETE /sprints/{id}`, wrong role-assignment verbs, `GET /items/{id}/history`)
and shipped endpoints that are undocumented (import-github/linear/csv, YAML export,
save-as-template, backup/restore).

#### Task 29.6 — Contract tests

Point `schemathesis` (or a Rust equivalent) at the spec against the in-memory test
app in CI; retire the hand-rolled envelope checks in `frontend/e2e/api.spec.ts`
once redundant. `/api/v1` prefixing is **deliberately deferred** until the first
post-1.0 breaking change; batch the 201/204 normalization (29.2) as the last
pre-contract break.

**Acceptance:** `git diff --exit-code docs/openapi.json` gates CI; frontend types
are generated, not hand-copied; both API reference docs match `router.rs` because
they're derived from it; error toasts show human messages.

### Phase 30 — Vocabulary Integrity & Navigation Polish ✅ _done_

**Goal:** The core differentiator — per-project vocabulary — holds on **every**
surface. A construction project must never say "sprint". All frontend; parallelize
by file.

> Shipped: the global "+ New" modal, Sprints view, WorkTabs, command palette,
> document titles, and the first-run guide all resolve per-project vocabulary;
> breadcrumb + sidebar navigation fixed (Table lens now reachable); ~28 raw
> priority-hex literals replaced with tokens (dark-mode/palette-correct); dead
> deps removed; axe scans extended to five more pages; `check-tokens.sh` now also
> catches hex in inline styles.

#### Task 30.1 — Fix the global "+ New" modal

[Layout.tsx:181-187](../../../frontend/src/app/Layout.tsx) renders
`CreateItemModal` without the `vocabulary` prop (Board passes it; the global
entry doesn't) — the type picker shows Epic/Feature/Task regardless of project.
Resolve vocabulary from the active project context.

#### Task 30.2 — Vocab-ify the Sprints view

[Sprints.tsx](../../../frontend/src/features/sprints/Sprints.tsx): "Sprint
Planning" h1, "Backlog" pane (vocab key exists: "Pending Work"), drag hints,
toasts, "Sprint 1" placeholder — `t()` is already in scope; this is mechanical.
Same pass: `CreateItemModal.tsx:332` "Story Points" → `t('story_points')`,
`Dashboard.tsx:234`, `ItemHeader.tsx:138`.

#### Task 30.3 — Vocab-ify the chrome

WorkTabs "Sprint" tab, Sidebar "Sprints", command palette entries
([Layout.tsx:85-103](../../../frontend/src/app/Layout.tsx)), document titles, and
[EmptyProjectGuide.tsx:88-96](../../../frontend/src/shared/ui/EmptyProjectGuide.tsx)
step 3 ("Plan a sprint") — the **first-run screen** a construction user sees.
Point the guide's vocabulary link at the vocabulary tab, not `/settings`.

#### Task 30.4 — Token + hygiene sweep

Replace the 28 raw hex literals in inline styles (priority colors duplicated in
`Sprints.tsx:33-38`, `Timeline.tsx:260-265,457-460`, `Calendar.tsx:49-52`) with the
existing tokenized `priorityColor()` from
[PriorityDot.tsx](../../../frontend/src/shared/ui/PriorityDot.tsx) — they currently
ignore dark mode and the Clay/Graphite palettes. Extend `scripts/check-tokens.sh`
to catch hex in `style` props. Remove dead deps `@kobalte/core` and
`@solid-primitives/keyboard` (zero imports). Extend axe E2E scans beyond the
current 3 pages to Table, Timeline, Calendar, Sprints, and the item drawer.

**Acceptance:** `grep -ri "sprint\|story points" frontend/src --include='*.tsx'`
returns only vocab-resolved or teaching-copy hits; a `construction` project reads
Phase/Work Order/Effort Hours on every surface including first-run; all palettes
render priority colors correctly in dark mode.

### Phase 31 — Construction Verticals & Preset Onboarding ✅ _done_

**Goal:** Make "works for building projects" concretely true for wood-frame,
steel-frame, and SIP-panel builds — and make presets discoverable at project
creation. The template system already carries vocabulary + workflow +
custom-fields + boards as JSON; **no schema change, no new enum variants.**

> Shipped: three seeded `BuiltinSpec` construction verticals (Wood Frame, Steel
> Frame, SIP Panel), each with a tailored linear-plus-rework workflow and
> build-system custom fields (stud spacing, steel grade, panel count, …); the
> built-in seed dedup key moved from project-type to template-name so multiple
> Construction templates coexist (backward-compatible). The New Project modal is
> now template-first (selectable cards with a workflow + vocabulary preview and a
> "start blank" fallback), and the Templates page type filter covers all 11 types.

#### Task 31.1 — Three construction sub-preset templates

Add `BuiltinSpec` entries (all `ProjectType::Construction`) in
[templates.rs:201-260](../../../crates/tack-db/src/repo/templates.rs):

- **Wood Frame Build** — Permit → Foundation → Framing → Rough-In (MEP) →
  Insulation & Drywall → Finish → Inspect → Handover; custom fields: stud spacing
  (select: 16"/24" o.c.), lumber grade, sheathing type, shear-wall schedule ref.
- **Steel Frame Build** — Permit → Engineering → Fabrication → Erection →
  Decking/MEP → Fireproofing → Inspect → Handover; custom fields: steel grade,
  bolt spec, weld inspection class, torque log ref.
- **SIP Panel Build** — Design/Shop Drawings → Panel Fabrication → Delivery →
  Foundation → Panel Set → Seal & Penetrations → MEP Chases → Inspect → Handover;
  custom fields: panel count, panel thickness, spline type, sealant spec.

Each keeps the construction vocabulary base (Work Order/Phase/Inspection Point)
with per-template workflow transitions (linear + rework loops back from Inspect).
Unit-test seeding and workflow transition rules.

#### Task 31.2 — Templates in the New Project modal

[CreateProjectModal.tsx](../../../frontend/src/shared/ui/CreateProjectModal.tsx)
currently offers a bare 11-way `<select>`; the richer Templates gallery is a
disconnected page. Replace the type select with template cards (the
`templateSummaryChips` preview component already exists) grouped by domain, with
"start blank" fallback. Fix the Templates page type filter which is missing
legal/research/event ([Templates.tsx:38-47](../../../frontend/src/pages/Templates.tsx)).

#### Task 31.3 — Preset preview

On template-card focus, show the workflow columns and 3 sample vocabulary mappings
("task → Work Order") before the user commits — today the only hint is one line of
help text.

**Acceptance:** a user can create a "SIP Panel Build" project from the New Project
dialog in two clicks, and its board shows SIP phases with SIP custom fields;
`tack init --template` can instantiate the same from the CLI.

### Phase 32 — Enterprise OSS Standards ✅ _done (32.1–32.5); 32.6 not started_

**Goal:** Close the governance/integrity gaps between "excellent solo hygiene" and
"enterprise-grade open source". All independent; ideal for parallel dispatch.

> Shipped: truthful single-binary deployment docs **plus** a real `Dockerfile` +
> `docker-compose.yml` (the old guide documented a compose setup that didn't
> exist); release integrity in `release.yml` (SHA256SUMS, build provenance
> attestation, CycloneDX SBOMs, `cargo auditable`); private disclosure via GitHub
> Security Advisories; `CODE_OF_CONDUCT.md`, `GOVERNANCE.md`, PR-process +
> branching sections in CONTRIBUTING; CI gates for MSRV (**Rust 1.85**, the
> edition-2024 floor — the old "1.75+" claim was impossible), `cargo-llvm-cov` +
> Vitest coverage thresholds, `cargo-deny`, and a weekly scheduled audit; and the
> enumerated doc-staleness fixes. **32.6 (binary-size guard — trim `tokio` `full`
> features, feature-gate `object_store`, add a CI binary-size gate) is not
> started.** Retroactive git tags for beta.1–5 remain a manual follow-up.

#### Task 32.1 — Truthful deployment docs

`docs/DEPLOYMENT-GUIDE.md` leads with Docker Compose instructions **for a
Dockerfile and compose file that do not exist in the repo**, describing the retired
two-service architecture. Either ship a minimal `Dockerfile` (scratch/distroless +
the static musl binary — trivially small) + compose file, or rewrite the guide
around the single-binary + systemd + Caddy reality. Decide and do one.

#### Task 32.2 — Release integrity

In `.github/workflows/release.yml`: emit `SHA256SUMS`, add
`actions/attest-build-provenance`, publish an SBOM (`cargo auditable` +
CycloneDX; npm SBOM for the SPA). Tag every future CHANGELOG release (beta.1–5
have entries but no tags; only beta.6 is tagged).

#### Task 32.3 — Security policy & community files

SECURITY.md currently tells reporters to open a **public** issue — switch to GitHub
Security Advisories (keep email fallback `info@yielab.com`). Add
CODE_OF_CONDUCT.md (Contributor Covenant), a PR-process/branching section in
CONTRIBUTING.md (README promises it's there; it isn't), `.github/ISSUE_TEMPLATE/config.yml`
with a security-contact link, and a one-paragraph GOVERNANCE.md (BDFL statement).

#### Task 32.4 — MSRV + coverage gates

README claims "Rust 1.75+" but nothing enforces it: set `rust-version` in the
workspace `Cargo.toml`, add an MSRV CI job, state the bump policy. Add
`cargo-llvm-cov` + Vitest coverage thresholds to CI (TESTING.md documents targets
core ≥85% / db+api ≥70% with zero enforcement). Add `cargo-deny`
(license + duplicate-dep policy) and a weekly scheduled audit run.

#### Task 32.5 — Doc staleness sweep

Fix the enumerated drift: introduction.md's false "`mdbook test` in CI" claim,
TESTING.md's stale a11y-debt note + "three jobs" (there are six),
developer/README.md "67 unit tests" → 73, API-REFERENCE CORS section
(`TACK_CORS_ORIGIN` doesn't exist → `TACK_ALLOWED_ORIGINS`), health-response
example shape. (The endpoint tables themselves are superseded by Phase 29.5.)

#### Task 32.6 — Binary size guard

Trim `tokio = { features = ["full"] }` to the used set
(`rt-multi-thread, net, macros, signal, time, fs`); feature-gate `object_store`
(`remote-backup` feature, on by default in releases) so minimal builds drop the
S3 machinery; verify `regex` is actually needed at the workspace level. Add a
binary-size gate to CI next to the existing 30 KB frontend bundle gate.

**Acceptance:** release assets ship with checksums + provenance + SBOM; a security
report can be filed privately; CI enforces MSRV + coverage + licenses; every doc
claim spot-checked in the audit is now true.

### Multi-Agent Dispatch Plan (Phases 26–32)

Designed for parallel agent execution. Rules of engagement: every agent writes
regression tests for its own fixes, runs `cargo test --workspace`, `cargo clippy`,
and `npm run type-check` before finishing, and never touches another track's files.

**Wave 1 — no interdependencies, dispatch simultaneously:**

| Agent | Scope | Tasks | Primary files |
|-------|-------|-------|---------------|
| A1 (Rust, DB) | Item persistence hotfix | 26.1, 26.2, 26.3 | `tack-db/src/repo/items.rs`, `tack-core/src/models.rs`, `tack-db/src/lib.rs` |
| A2 (Rust, security) | Restore integrity | 27.3, 27.4, 28.3 | `tack-api/src/remote_backup.rs`, `tack-api/src/server.rs` |
| A3 (Rust, security) | Endpoint hardening | 27.1, 27.2, 27.5, 26.5 | `handlers/alexa.rs`, `server.rs`, `handlers/import_linear.rs` |
| A4 (Rust, backup) | Sync correctness | 28.1, 28.4, 27.6, 27.7 | `server.rs`, `remote_backup.rs`, `handlers/settings.rs` |
| A5 (frontend) | Nav + vocab integrity | 26.4, 30.1, 30.2, 30.3 | `Breadcrumb.tsx`, `Sidebar.tsx`, `Layout.tsx`, `Sprints.tsx` |
| A6 (frontend) | Hygiene sweep | 30.4 | inline-style hex, dead deps, axe coverage |
| A7 (docs/CI) | OSS standards | 32.1–32.5 | `SECURITY.md`, `CONTRIBUTING.md`, `.github/`, `docs/` |

**Wave 2 — after Wave 1 merges:**

| Agent | Scope | Tasks | Depends on |
|-------|-------|-------|-----------|
| B1 (Rust) | Error + response typing | 29.1, 29.2 | A1 (touches `handlers/`) |
| B2 (Rust+data) | Generation-counter sync | 28.2, 28.5 | A2, A4 |
| B3 (Rust, templates) | Construction presets | 31.1 | none (waits only to avoid `templates.rs` churn) |
| B4 (frontend) | Template-first onboarding | 31.2, 31.3 | B3 (template data) |
| B5 (build) | Size guard | 32.6 | A4 (feature-gating `object_store`) |
| B6 (release) | Cut `v0.1.0-beta.7` | 26.6 | A1–A5 merged |

**Wave 3 — the SDD backbone (sequential inside the track):**

| Agent | Scope | Tasks | Depends on |
|-------|-------|-------|-----------|
| C1 (Rust) | utoipa spec + CI drift gate | 29.3 | B1 |
| C2 (frontend) | Generated TS types | 29.4 | C1 |
| C3 (docs) | Spec-generated API reference | 29.5 | C1 |
| C4 (QA) | Contract tests in CI | 29.6 | C1 |

**Definition of done for the whole cycle:** `v0.1.0-beta.7` tagged with all Wave 1
fixes; a SIP-panel construction project creatable in two clicks with correct
vocabulary end-to-end; two devices sharing a bucket cannot clobber each other;
`openapi.json` gates CI and generates the frontend types and the API docs; release
assets are signed and SBOM'd.

---

## Next — Agent-Factory Control Center (Phases 33–38, August 2026)

**Thesis:** Tack becomes the **control center for a factory of products** built by
governed agent fleets. [docket](https://github.com/yielab/docket) (folder
`~/Sites/rack-cli`) already runs the fleets — pods of Lead/Implementer/Reviewer/Tester
agents, per-project isolation, a policy chokepoint on every tool call, budgets, and a
hash-chained audit log. What it does not have is a plan of record, a roadmap, a board,
or a place to see cost-per-outcome. Tack is exactly that, and already carries the
primitives: a universal item model, a DAG, per-project workflows and vocabulary,
real-time `BoardEvent` push, and an external-link precedent (`github_links` +
`maybe_sync_github`) that this cycle copies wholesale.

**The architecture in one line:** _Tack holds desired state, docket executes, and a
reconciler in a new `tack-orch` crate closes the loop._ Intent flows **push** (Tack →
docket, synchronous, returns a run id); progress flows **pull** (a jittered poll loop,
Kubernetes-style). Pull is not a compromise — docket has no outbound webhook, and a
reconciler survives docket restarts, missed deliveries, and Tack downtime with no
queue and no replay logic.

```text
┌──────────────────────── Tack (control center) ───────────────────────┐
│  Fleet · Approvals inbox · Board/Timeline · Item agent-activity      │
│  tack-api    POST /api/items/{id}/dispatch                           │
│              POST /api/sprints/{id}/dispatch      (DAG-ordered)      │
│              POST /api/approvals/{token}                             │
│  tack-orch   ControlPlane trait ──► DocketAdapter                    │
│    dispatcher   item → task → run       (intent, synchronous)        │
│    reconciler   poll: /health /status.json /runs /approvals /metrics │
│                 → orch_* tables → workflow engine → BoardEvent       │
└────────────────────────────────┬─────────────────────────────────────┘
                                 │ HTTP + Bearer
┌────────────────────────────────▼─────────────────────────────────────┐
│  docket serve --dispatch    Lead → Implementer → Reviewer → Tester   │
│  gated turn loop · approval store · budgets · audit chain            │
└──────────────────────────────────────────────────────────────────────┘
```

### What docket exposes today (verified against `~/Sites/rack-cli/src/docket/serve.py`)

| Surface | Auth | Payload |
|---|---|---|
| `GET /status.json` | none | `{apiVersion, gateway, channels, agents[{id,name,kind,scope,model,lastActivity,costUsd,budgetUsd}], totalCostUsd}` |
| `GET /metrics` | none | Prometheus: `docket_agent_turns_total`, `docket_tool_calls_total`, `docket_approvals_total`, `docket_policy_hits_total`, `docket_turn_duration_seconds`, `docket_gateway_up` |
| `GET /health` | none | liveness |
| `GET /runs?project=`, `GET /runs/{id}` | Bearer | run registry — `source` (cli/webhook/schedule/sweep/mcp), `state` (queued/running/succeeded/failed/cancelled), hops, pids |
| `GET /approvals` | Bearer | pending records **including `context: {taskId, pipelineIndex}`** — this is what correlates an approval back to a Tack item |
| `POST /dispatch/{project}` | Bearer | binds pipeline `variables` from the JSON body, creates the run record **before** work starts, returns `{ok, run, project}` |
| `POST /approvals/{token}` | Bearer | `{action: grant\|deny}`, audit-logged with `channel="http"` |

### Two blocking gaps found in docket (fix upstream first)

1. **There is no HTTP endpoint to enqueue a task.** `POST /dispatch/{project}` dispatches
   an _existing_ queue (`effective_pipeline` → `resolve_variables` → `dispatch_pod`).
   Task creation lives only in `core/dispatch.py::enqueue_task`, reachable via the CLI
   or the `delegate` MCP tool. **Phase 35 cannot start until docket ships
   `POST /tasks/{project}`.** Likewise `docket add` (pod provisioning) is CLI-only,
   which blocks Phase 37.
2. **docket's turn loop builds its tool registry from built-ins only** — its agents
   _cannot_ call Tack's MCP server from inside a turn. So agents cannot self-report
   progress, and this cycle must not depend on it: reporting comes from the dispatch
   lifecycle (runs + traces), not from the agents. The `orch_tasks` link table makes
   agent self-reporting a drop-in addition the day docket closes that wire.

Both are tracked as a docket-side work package in `~/Sites/rack-cli/ROADMAP.md`
(Phase 20 — "Tack control-plane integration").

### Non-negotiable design rules for this cycle

- **Status transitions go through the workflow engine, never raw SQL.** The reconciler
  calls the same update path as a user drag, so WIP limits, explicit transitions
  (construction is linear!), `started_at`/`completed_at`, and parent auto-propagation
  all still fire. A rejected transition records an `orch_events` row of type
  `status_map_rejected` and **leaves the item alone** — it never forces the move.
- **Never hold a SQLite write transaction across an HTTP call to docket.** Tack is
  single-writer; "database locked" is already in the troubleshooting guide. Fetch →
  parse → short write txn.
- **Preserve docket's cost honesty.** docket reports _measured tokens_ and a
  _clearly labelled estimate_, and refuses to call an estimate spend. Tokens are the
  primary stored metric; the money column is named `cost_usd_estimated`, carries the
  pricing-snapshot date, and renders as "estimated" in the UI. Silently relabelling it
  as spend would be this cycle's worst failure mode.
- **A control-plane failure never fails a user request.** The reconciler is a background
  task with its own backoff, exactly like the existing due-soon and backup schedulers
  in [server.rs](../../../crates/tack-api/src/server.rs).

### Status of this cycle as of 2026-08-05

Built by a multi-agent run tracked in [`TODO.md`](../../../TODO.md); every card's
handoff note is in that file's §6.

| Phase | Status | Notes |
|---|---|---|
| 33 — Control-Plane Link (read-only) | ✅ **done** | Reconciler polls a live docket; Fleet view ships |
| 34 — Run Mirroring & Telemetry | ✅ **done** | Runs, approvals, traces, metrics, retention, realtime, Agent Activity UI |
| 35 — Dispatch | ✅ **done** | Item + DAG-ordered sprint dispatch, trust boundary, dispatch UI |
| 36 — Governance Surface | ✅ **done** | Approvals inbox + budget/policy panels. **No pause control** — docket exposes no pause/resume over HTTP, in either direction |
| 37 — Factory Provisioning | ✅ **done** | `POST /pods` had already shipped; the block was wrong. Provisioning wizard, rollback before the pod exists, never after |
| 38 — Unit Economics | ✅ **done** | Tokens, estimated cost, lead time, rework rate — with the comparisons the data cannot support deliberately not computed |

**Every card in this cycle is built.** Nothing is blocked.

**Corrections made during the run, worth carrying forward:**

- Phases 34 and 35 were marked blocked on docket endpoints that **had already
  shipped**. docket's own `ROADMAP.md` still lists them as `TODO` and is stale against
  its source; `src/docket/serve.py` is the authority. Re-verify before trusting any
  "blocked" marker.
- The `POST /tasks/{project}` response shape documented in `TODO.md` §1.4 was wrong;
  corrected after live verification.
- Migrations run to **029**, not 024 — see the corrected table below.

### Schema added this cycle (migrations 019–029)

| Migration | Table | Purpose |
|---|---|---|
| 019 | `control_planes` | `id, name, kind, base_url, token` (write-only over the API), `api_version, health, last_seen_at, consecutive_failures` |
| 020 | `orch_links` | Tack project ↔ docket pod: `project_id, control_plane_id, remote_project, pipeline_file, blueprint, auto_dispatch, budget_usd, status_map` (JSON) |
| 021 | `orch_tasks` | item ↔ docket task, **PK `(item_id, remote_task_id)`** — 1:N, an item can be redispatched. `remote_run_id, remote_status, attempt, tokens_in, tokens_out, cost_usd_estimated, dispatched_at, trusted` |
| 022 | `orch_runs` | mirror of `/runs` — `run_id` PK, `control_plane_id, remote_project, source, state, started_at, ended_at, error, item_id` |
| 023 | `orch_events` | append-only telemetry (hops, verdicts, rework, tool calls, `status_map_rejected`); index `(item_id, occurred_at)` |
| 024 | `orch_approvals` | mirror of `/approvals`, correlated to items via `context.taskId`; `token` PK |
| 025 | `orch_metrics` | mirror of docket's Prometheus `/metrics` scrape |
| 026 | `orch_events_daily` | per-day aggregate of purged `orch_events`. Keyed `(day, control_plane_id, event_type)` — **drops `item_id`**, so per-item history truncation is not recoverable from the aggregate |
| 027 | `orch_metrics_daily` | per-day aggregate of purged `orch_metrics`; non-finite samples counted but excluded from sum/min/max |
| 028 | `orch_trace_cursors` | resumption cursor per `(control_plane_id, remote_project)`, stored opaque |
| 029 | `items.source` | sticky provenance for the prompt-injection trust boundary; defaults to `unknown`, which resolves to untrusted |

Agent state is **not** denormalized onto `items` — the board query LEFT JOINs the latest
`orch_tasks` row. Revisit only if profiling says so.

### Phase 33 — Control-Plane Link (read-only) ✅ _done 2026-08-05_

**Goal:** See the whole agent fleet from inside Tack, with **zero write path** to
docket. Independently shippable and safe to run against a live fleet.

#### Task 33.1 — `crates/tack-orch` crate skeleton

New workspace member. Defines the `ControlPlane` **trait** (`health`, `status`,
`metrics`, `list_runs`, `get_run`, `list_approvals`, `list_tasks`) plus `OrchError`
(`thiserror`) and the shared DTOs every other task consumes. The trait is what makes
this a factory control center rather than a docket-specific dashboard — a future
GitHub-Actions or Temporal adapter drops in beside `DocketAdapter`. Register in the
workspace `Cargo.toml`; `tack-orch` may depend on `tack-core` and `tack-db`, never on
`tack-api`.

#### Task 33.2 — Migrations 019–024

Land all six tables in [migrations.rs](../../../crates/tack-db/src/migrations.rs) in one
change (single owner — this file is the cycle's biggest merge chokepoint). Follow the
`018_github_links` precedent. Every FK `ON DELETE CASCADE` from `items`/`projects`.

#### Task 33.3 — `DocketAdapter`

`crates/tack-orch/src/adapters/docket.rs`: async `reqwest` client implementing
`ControlPlane` against the table above. Bearer token on the authenticated routes only.
Per-request timeout (default 5s), typed deserialization, and a **Prometheus text
parser** for `/metrics` (no new dependency — the format is trivial; parse
`name{labels} value`). Tests run against a `wiremock` fixture server built from real
captured payloads.

#### Task 33.4 — `orch` repository module

`crates/tack-db/src/repo/orch.rs`: CRUD for `control_planes` and `orch_links`, plus
upsert helpers for the mirror tables (used from Phase 34). Register in
[repo.rs](../../../crates/tack-db/src/repo.rs).

#### Task 33.5 — Config + control-plane CRUD API

`TACK_ORCH_ENABLE` (default **false** — nothing in this cycle runs until it is on),
`TACK_ORCH_POLL_SECS` (default 10), `TACK_ORCH_EVENT_RETENTION_DAYS` (default 90) in
[config.rs](../../../crates/tack-api/src/config.rs) and `tack.toml`. New handler
`handlers/orch.rs` + routes `GET/POST /api/control-planes`, `GET/PATCH/DELETE
/api/control-planes/{id}`, `GET/PUT /api/projects/{id}/orch-link`. The docket token is
**write-only** over the API — returned as a `token_set: bool`, exactly like the S3
secret key in [settings.rs](../../../crates/tack-api/src/handlers/settings.rs).

#### Task 33.6 — Reconciler skeleton

`crates/tack-orch/src/reconciler.rs`: a `tokio` task per registered control plane,
jittered interval, exponential backoff, and a health state machine
(`healthy → degraded` after 3 consecutive failures `→ unreachable` after 10). Spawned
from [server.rs](../../../crates/tack-api/src/server.rs) alongside the existing
schedulers, and **only** when `TACK_ORCH_ENABLE=true`. This task polls `/health` and
`/status.json` only; Phase 34 adds the rest.

#### Task 33.7 — Fleet view (frontend, read-only)

`frontend/src/features/fleet/`: one row per product — Tack project, pod health, roster
(roles + models), last activity, burn vs budget, gateway state. Sourced from
`GET /api/control-planes` + `GET /api/fleet`. Empty state explains how to register a
control plane. Tokens only, WCAG AA, axe-clean — the design-system rules in
[frontend.md](developer/frontend.md) apply unchanged.

#### Task 33.8 — docket-side read endpoints

Upstream in `~/Sites/rack-cli`: `GET /tasks/{project}` (the pod's `TASK_LIST.json` as
JSON) and `GET /traces/{project}?since=` (trace events). See that repo's ROADMAP
Phase 20. Tack can ship 33.1–33.7 without these; 34.4 needs them.

#### Task 33.9 — Docs + OpenAPI

New `docs/book/src/user-guide/orchestration.md` and
`docs/book/src/developer/orchestration.md`; register both in `SUMMARY.md`. All new
endpoints annotated with `utoipa` so the Phase 29.3 CI drift gate covers them.

**Acceptance:** with `TACK_ORCH_ENABLE=true` and a live `docket serve`, the Fleet view
lists every pod with correct roles, models, and budget state; killing `docket serve`
flips the control plane to `degraded` then `unreachable` without a single failed Tack
request or a `database is locked` error; with the flag off, no reconciler task is
spawned and no new route accepts traffic.

### Phase 34 — Run Mirroring & Telemetry ✅ _done 2026-08-05_

**Goal:** Every run, hop, verdict, approval, and token is visible in Tack, live, and
attributable to an item.

#### Task 34.1 — `/runs` ingestion

Extend the reconciler: for each linked project, `GET /runs?project=` → upsert
`orch_runs`. State transitions emit an `orch_events` row and a `BoardEvent`. Runs are
correlated to items through `orch_tasks.remote_run_id` (populated in Phase 35; until
then runs mirror unattributed, which is fine and must not error).

#### Task 34.2 — `/approvals` ingestion

`GET /approvals` → upsert `orch_approvals`. Correlate to an item by reading
`record.context.taskId` and joining `orch_tasks.remote_task_id`. Records that don't
correlate are stored with `item_id = NULL` and still shown in the fleet-level inbox.

#### Task 34.3 — `/metrics` ingestion + rollup

Parse the Prometheus text into `orch_metrics` (one row per scrape per metric per label
set). Reuse the parser from 33.3 — do not write a second one.

#### Task 34.4 — Trace ingestion

`GET /traces/{project}?since=<cursor>` → `orch_events`. Map docket's event types
(`tool_call`, `approval_*`, `cost_charged`, `budget_exceeded`, `verification_failed`,
`tester_verdict_failed`, `rework_started`, `review_rejected`, `session_end`, …) to a
Tack-side taxonomy; store unknown types verbatim rather than dropping them, so a docket
upgrade that adds an event type degrades to "shown as-is".

#### Task 34.5 — Broadcast agent state

Add `BoardEvent::AgentRunUpdated { project_id, item_id, run_id, state }` and
`BoardEvent::ApprovalPending { … }` in
[websocket.rs](../../../crates/tack-api/src/handlers/websocket.rs). Frontend
`shared/realtime` handles both. This is what makes the board move by itself while a
fleet works.

#### Task 34.6 — Retention + daily rollup

`orch_events` and `orch_metrics` grow unbounded (docket has the identical open gap).
A daily task rolls raw rows into per-day aggregates **before** deleting anything older
than `TACK_ORCH_EVENT_RETENTION_DAYS`, so the Phase 38 unit-economics history outlives
the raw events. Retention is not optional and not a follow-up.

#### Task 34.7 — Tack's own `GET /api/metrics`

Prometheus text merging Tack's work-tracking metrics (items by status, cycle time,
throughput) with the mirrored docket ones, so one Grafana scrape covers the factory.
Unauthenticated like docket's, but bound by the same CORS/bind warnings as Phase 27.2.

#### Task 34.8 — Item "Agent Activity" tab

In `frontend/src/features/item-detail/`: a timeline of hops, tool calls, verdicts,
rework cycles, approvals, and tokens/estimated cost for the item's `orch_tasks`. This
is where "running / finished / any progress" actually lives for a single work item.

#### Task 34.9 — Agent badges on Board, List, Table

A compact state chip (queued / running / waiting-approval / failed) driven by the
`orch_tasks` LEFT JOIN. One shared component; no per-view reimplementation.

**Acceptance:** dispatching a pod from the docket CLI (no Tack involvement) surfaces
the run in Tack within one poll interval; the item timeline shows every hop with token
counts; every money figure in the UI reads "estimated"; a 90-day-old event is gone but
its day's aggregate survives.

### Phase 35 — Dispatch: Tack drives the factory ✅ _done 2026-08-05 — the `POST /tasks` block was incorrect; docket had already shipped it_

**Goal:** Moving a card dispatches a governed agent pipeline.

#### Task 35.1 — docket-side `POST /tasks/{project}`

Upstream. Body `{description, priority, trusted}` → `dispatch.enqueue_task` → returns
`{taskId}`. Same Bearer auth, same audit logging, and it must honour the `pre_input`
policy gate (a `block` verdict returns 4xx with the policy id).

#### Task 35.2 — `status_map` schema + validation

`orch_links.status_map` shape:

```json
{ "dispatch_from": ["Ready"], "on_running": "In Progress",
  "on_waiting_approval": "Blocked", "on_succeeded": "Done",
  "on_failed": "Blocked", "on_cancelled": "Ready" }
```

Validated **at save time** against the project's `WorkflowConfig` — every named status
must exist. Hardcoding "running → In Progress" would break the construction, personal,
and homework presets, which is the whole point of Tack's vocabulary system.

#### Task 35.3 — `POST /api/items/{id}/dispatch`

Resolve the link → enqueue the task on docket → `POST /dispatch/{project}` with the
item's fields bound as pipeline `variables` → persist `orch_tasks` (task id + run id)
→ apply `on_running`. Idempotent per `(item_id, attempt)`.

#### Task 35.4 — `POST /api/sprints/{id}/dispatch` (DAG-ordered)

**The highest-value primitive in this cycle** — this is what "run a product line" means,
and it has no precedent in either codebase, so it gets the most design attention.
Topologically sort the sprint's items using
[dependency.rs](../../../crates/tack-core/src/dependency.rs), enqueue in order, and hold
an item until its dependencies reach a `done`-category status. Respect the pod's WIP by
capping in-flight dispatches per project. A cycle in the graph is already impossible
(the DAG validator prevents it) — assert it anyway and fail loudly.

#### Task 35.5 — Auto-dispatch hook

In [items.rs](../../../crates/tack-api/src/handlers/items.rs), beside the existing
`maybe_sync_github` call: when an item enters a `dispatch_from` status and
`orch_links.auto_dispatch` is true, fire the dispatcher. Best-effort like the GitHub
hook — a dispatch failure logs and records an event, never fails the user's PATCH.

#### Task 35.6 — Reconciler applies `status_map`

Terminal run states drive the item's status **through the workflow engine**. On
rejection, record `status_map_rejected` with the workflow's reason and surface it in
the UI. See the non-negotiables above.

#### Task 35.7 — Untrusted-source flag

GitHub/Linear-imported item titles and descriptions are **attacker-authored text that
becomes agent input**. Mark imported items `source: imported` at import time and pass
`trusted: false` on enqueue so docket's `pre_input` policy hook treats them as
untrusted. This is a real prompt-injection boundary, not a formality.

#### Task 35.8 — Dispatch UI

"Dispatch to agents" on the item detail and the board card menu; "Run sprint" on the
Sprints view with a dependency-order preview and an in-flight cap control.

#### Task 35.9 — Security gating + backup exclusion

Every dispatch route is gated behind `TACK_ORCH_ENABLE` **and** a configured control
plane token; off by default. The docket token must be **excluded from backup bundles**
— the S3 secret key already leaked this way once (finding #7 of the July audit). Ship
the exclusion and its regression test in the same commit.

**Acceptance:** dragging a card to Ready enqueues a docket task, the pipeline runs
Lead → Implementer → Reviewer → Tester, and the card lands in Done by itself; a
construction project with a linear workflow refuses an illegal auto-move and shows why;
`GET /api/backup` contains no control-plane token; with `TACK_ORCH_ENABLE` unset, every
dispatch route 404s.

### Phase 36 — Governance Surface ✅ _done 2026-08-05 — no pause control: docket exposes none over HTTP_

**Goal:** Approve, pause, and audit the fleet from the board.

#### Task 36.1 — Approvals proxy

`POST /api/approvals/{token}` `{action}` → docket's `POST /approvals/{token}`. Requires
a **separate** `TACK_ORCH_APPROVAL_TOKEN`; with only `TACK_API_TOKEN` set, the
approvals surface is read-only. Rationale: the Tack token is a single shared secret,
and granting a docket approval is a genuinely higher-privilege action than editing a
card.

#### Task 36.2 — Approvals inbox

A fleet-wide page: every pending approval across every pod, with the requesting agent,
the action text, the correlated item, and grant/deny. Ordered oldest-first — docket's
approvals **fail closed on timeout**, so latency here has a real cost.

#### Task 36.3 — Budget & pause visibility

Per-pod burn vs cap from `/status.json`, a warning band, and a clear "this pod is
budget-paused; its queue is refusing tasks" state with the `docket profile <id>
--resume` remedy spelled out. Tack does not clear the pause itself in this phase.

#### Task 36.4 — Policy & audit panel

Denial rate, policy hits by id, approvals by channel, tool-call volume — all from
`/metrics`. Link out to `docket audit verify` rather than reimplementing chain
verification in Rust.

#### Task 36.5 — docket-side `channel="tack"`

Upstream: accept and record a `tack` approval channel so the audit chain's provenance
stays honest instead of every board decision masquerading as `http`.

**Acceptance:** a gated `git push` from an Implementer appears in the Tack inbox within
one poll interval, granting it from Tack resumes the pipeline, and
`docket audit verify` shows the entry tagged `channel="tack"`.

### Phase 37 — Factory Provisioning ✅ _done 2026-08-05 — the block was incorrect; `POST /pods` had shipped_

**Goal:** One click creates a product: a Tack project **and** its governed pod,
pipeline, verify command, and budget. This is what makes it a factory rather than a
very good dashboard.

#### Task 37.1 — `orchestration` block on templates

Extend the template payload
([templates.rs](../../../crates/tack-db/src/repo/templates.rs)) with an optional
`orchestration` object: docket blueprint (`software` / `research` / `content` / `ops` /
`agentic-product`), pipeline YAML reference, `verifyCmd`, budget cap, default
`status_map`, pod shape (`--pod full` / `--with`). Backwards compatible — templates
without it behave exactly as today.

#### Task 37.2 — Provisioning flow

`POST /api/projects/from-template/{id}` gains `provision_pod: true` → create the Tack
project, then the docket pod, pipeline, budget, and `orch_link`, in that order, with
**rollback on partial failure** (a Tack project with a half-created pod is worse than
no project). Requires the upstream provisioning endpoint from 37.5.

#### Task 37.3 — Pipeline library

Store validated docket pipeline YAML in Tack (`pipelines` table or a template field),
validated by round-tripping through docket's own `pipeline validate` rather than
reimplementing the schema in Rust — a second validator would drift.

#### Task 37.4 — New-product wizard

Product type → template → pod shape → budget → verify command → create. Reuses the
Phase 31 template-first onboarding surface.

#### Task 37.5 — docket-side pod provisioning over HTTP

Upstream: `docket add` is CLI-only. Needs `POST /pods` `{project, path, blueprint,
pod, budget, verifyCmd}` returning the pod roster.

**Acceptance:** from an empty install, one wizard pass yields a board with the right
vocabulary **and** a `docket list` entry with the right roles, models, budget, and
verify command; a forced failure mid-provision leaves neither a stray project nor a
stray pod.

### Phase 38 — Unit Economics & Optimization ✅ _done 2026-08-05_

**Goal:** Answer "which product lines are economical to build with agents?" — the
question a factory operator actually has, and one nobody can currently answer.

#### Task 38.1 — Core metrics

Per completed item: tokens in/out, estimated cost, **agent lead time**
(`dispatched_at → completed_at`) versus Tack's existing human `started_at →
completed_at`, and **rework rate** (`rework_started` + `verification_failed` +
`tester_verdict_failed` events per task). Rework rate is the agent-fleet equivalent of
a defect escape rate and is the earliest regression signal available.

#### Task 38.2 — Product-line comparison

Slice every metric by `project_type` and `item_type`. Cost-per-completed-item by
product line is the headline number of the whole cycle.

#### Task 38.3 — Model right-sizing feedback

Export per-role outcome quality (rework rate, verdict pass rate) against the model
docket used, in a shape docket's role→model policy can consume. This closes the
optimization loop: Tack observes outcomes, docket adjusts models. Export only in this
phase — Tack does not mutate docket's model policy.

#### Task 38.4 — Analytics export

CSV/JSON export of the factory analytics, reusing the existing export machinery in
[export.rs](../../../crates/tack-api/src/handlers/export.rs).

**Acceptance:** a dashboard answers "what did each product line cost, in tokens and
estimated dollars, per shipped item, and how often did agents need rework?" — with
every dollar figure labelled an estimate and carrying its pricing-snapshot date.

### Multi-Agent Dispatch Plan (Phases 33–38)

The full wave plan, per-agent task cards, file-ownership map, and rules of engagement
live in **[TODO.md](../../../TODO.md)** at the repo root — written to be picked up cold
by parallel Sonnet agents.

**Sequencing at a glance:** Task 33.1 + 33.2 are a blocking Wave 0 (they define the
crate, the trait, and every table the rest of the cycle writes to). Phase 33's
remaining tasks fan out in Wave 1. Phase 34 is the widest parallel wave. Phase 35 is
blocked on the upstream docket endpoint and is the only phase with real cross-task
coupling. Phases 36–38 are largely independent tracks.

**Definition of done for the whole cycle:** from a single Tack install, an operator can
provision a new product with its governed agent pod in one action, watch its roadmap
execute itself on the board in real time, approve gated actions from the approvals
inbox, and answer what the product line cost per shipped item — with every cost figure
honestly labelled an estimate.

## Next — Agnostic Control Plane (Phases 39–49, August 2026)

**Thesis:** competitors start from the fleet and bolt on a board. Tack starts from the
work item and adds the fleet. That is why Tack can say _"this feature cost 4.2M tokens
across 3 runs and 2 reworks"_ and a fleet dashboard cannot — it has no concept of a
feature. Phases 33–38 built that control center against exactly one backend. This cycle
makes it true of any backend, proves it with a second adapter that shares none of
docket's shape, closes the GitHub pipeline in both directions, and gives the operator
per-item model choice.

**The test this cycle has to pass:** _can an adapter be written for a provider with no
pods, no roles, no hops, no approval store and no policy engine, without touching the
trait?_ If not, the trait is contaminated. Today it is: of the thirteen methods on
`ControlPlane`, only `kind()` and `get_run()` survive that question unchanged, three
(`status()`, `list_tasks()`, `provision_pod()`) are pure docket, and eight carry
docket-shaped DTOs.

The full plan, with per-item verification commands and the reasoning behind every
decision, is **[docs/plans/agnostic-control-plane.md](../../plans/agnostic-control-plane.md)**.
Executable task cards for parallel agents are in [TODO.md](../../../TODO.md), Part II.

### The load-bearing idea: capability negotiation

A control plane over N providers that does not model capabilities will either lie or
degrade to the intersection of all providers. Today _"docket exposes no pause/resume over
HTTP"_ is prose in `developer/orchestration.md` and a note in a UI panel. It becomes
`capabilities().pause == Unsupported`, with the UI disabling the control and naming the
reason from that value — never from a hard-coded `kind === 'docket'` check.

Model selection is the acceptance test for it. docket owns its own routing and may ignore
a model passed from outside; a GitHub Actions adapter forwards it verbatim. So
`capabilities().model_selection` has three live values — `Honoured`, `Advisory`,
`Unsupported` — and the UI must render three different things rather than show a picker
that silently does nothing.

### Verified external facts this cycle is built on

Checked against the vendor documentation during planning, not assumed.

| Fact | Value | Consequence |
|---|---|---|
| `POST /repos/{o}/{r}/actions/workflows/{id}/dispatches` on github.com | `200` with `{workflow_run_id, run_url, html_url}` | usable as a fast path only |
| The same endpoint on **GitHub Enterprise Server 3.17** | `204 No Content`, empty body | **correlation must not depend on the dispatch response** |
| Cancel a run | `POST .../runs/{id}/cancel` -> `202` | `capabilities().cancel == true` |
| Pause / suspend / hold a run | **no endpoint exists** | `capabilities().pause == Unsupported` — a capability, not a gap to work around |
| Run logs | `GET .../runs/{id}/logs` -> `302`, link expires in **1 minute** | not an event stream; events derive from `/jobs` steps instead |
| Log and artifact retention | default 90 days; public 1–90, private 1–400 | bounded history for the derived event channel |
| Job limits | GitHub-hosted **6 h**, self-hosted **5 days**, workflow run 35 days including waiting | the ceiling for a blocking decision |
| Pending deployments | `GET`/`POST .../runs/{id}/pending_deployments`, body `{environment_ids, state, comment}` | Actions' decision store |
| Webhook headers | `X-GitHub-Event`, `X-GitHub-Delivery` (GUID), `X-Hub-Signature-256` | dedupe key and HMAC verification |
| Claude Code hooks | `PreToolUse`/`PostToolUse` are **synchronous and block the tool call**; default `command` hook timeout **600 s**, per-hook `timeout` field; exit 2 blocks | the HITL mechanism and its ceiling |

### Decisions taken before planning

| # | Decision | Chosen | Cost accepted |
|---|---|---|---|
| D1 | Final shape of `ControlPlane` | one breaking reshape | one wide, risky commit |
| D2 | Inbound telemetry ingest | add it, behind a third secret | a new public and auth surface |
| D3 | Published route shapes | **break with notice** | 21 operations change; external clients must update |
| D4 | Concurrency control | `version` column + `ETag`/`If-Match` | three additive migrations |
| D5 | Credential a run holds | one **per-run scoped** credential | new minting and expiry machinery |
| D6 | Non-additive migrations | **rebuild `orch_runs` and `orch_approvals`** | the only irreversible step in the cycle |
| D7 | Plane-wide metrics | keep `plane_metrics()` on the trait | one method most providers will not implement |

### Non-negotiable design rules for this cycle

The four rules from Phases 33–38 stand unchanged. Five more apply here:

- **Tack never runs agents, and never proxies model traffic.** It is always a client of a
  control plane, it never holds a vendor key on the request path, and it never implements
  routing or fallback. Tack configures and reads a gateway; it never becomes one.
- **Model identifiers are opaque strings.** Tack stores the identifier plus the id of the
  gateway that understands it, and **never parses, maps, normalises or classifies it.**
  Tack classifies work items; the gateway classifies models. No tier abstraction under any
  name — docket removed `economy`/`standard`/`premium` in 0.2.0 and accepts them nowhere.
- **No docket noun crosses the anti-corruption layer.** If the UI says "pod," the ACL has
  leaked. `OrchBlueprint` and `TemplateOrchestration` in `tack-core/src/models.rs` are an
  existing violation this cycle resolves.
- **One `ALTER` per migration name.** `migrations.rs` runs each statement individually with
  **no wrapping transaction** and records the name only after all of them succeed. A
  multi-`ALTER` migration failing partway records nothing, re-runs statement one on the
  next boot, hits `duplicate column name`, and the server never boots again — with no
  down-migration. Both existing `ALTER` migrations are deliberately single statements.
- **The harness is a config string, not an abstraction.** Which agent CLI runs is one plain
  field on the project, beside the workflow name to trigger. No harness trait, no
  capability matrix, no plugin layer.

### Schema this cycle (migrations 032–049)

| Migration | Change | Purpose |
|---|---|---|
| 032 | `control_planes.config` | provider configuration JSON (owner/repo, workflow file, ref, API base) |
| 033 | `control_planes.secrets` | write-only credentials JSON; **must be added to `remote_backup.rs::scrub_snapshot_secrets` in the same commit** |
| 034–036 | `items.version`, `orch_links.version`, `control_planes.version` | optimistic concurrency (D4) |
| 037 | **`orch_runs` rebuild** | PK becomes `(control_plane_id, external_run_id, run_attempt)`; adds `correlation_id TEXT UNIQUE`. A global `run_id` PK makes a Tack-minted id and a provider id two rows `ON CONFLICT` can never merge |
| 038 | **`orch_approvals` rebuild** | `control_plane_id` becomes nullable (a hook-originated decision has no plane); adds `kind`, `external_id`, `provider_metadata`. `token` stays the PK and the URL segment |
| 039 | `orch_events.source` | `'poll' \| 'push' \| 'local'` — three provenances already exist in the code, so a two-value vocabulary would mislabel every locally-minted row |
| 040–041 | `orch_events.external_id` + partial unique index | ingest idempotency |
| 042 | `orch_model_policy` | per-item / per-item-type / per-project model choice |
| 043 | `orch_links.harness` | the harness config field |
| 044–049 | `github_links.{host,node_id,last_synced_at,remote_updated_at,state_hash}` + reverse index | bi-directional sync; the index is **non-unique** because a unique one could fail on a user's existing duplicates and brick the boot loop |

### Status of this cycle

| Phase | Title | Status |
|---|---|---|
| 39 | Regression Oracle | **implemented in current working tree; unreleased** |
| 40 | Capability Model & Adapter Registry | **implemented in current working tree; unreleased** |
| 41 | Optimistic Concurrency | **partial — CAS/version scaffolding implemented; atomic mutation and browser adoption reopened** |
| 42 | Run Identity & Decision Store Rebuild | **transitional implementation in current working tree; provider identity acceptance reopened** |
| 43 | Agnostic `ControlPlane` Trait (breaking) | **superseded by Phase 50+ runner boundary** |
| 44 | Unified Decision Inbox | **superseded; decision capability re-scoped into Phase 55** |
| 45 | GitHub Actions Adapter & Telemetry Ingest | **superseded; compile-only adapter is not a harness proof** |
| 46 | Model Policy & Gateway | **superseded; model profiles and measured usage re-scoped into Phase 56** |
| 47 | Inbound GitHub Webhooks | **frozen; remains optional future GitHub integration work** |
| 48 | Intervention Without Pause | **superseded; runner decisions re-scoped into Phase 55** |
| 49 | Bi-Directional GitHub Pipeline | **frozen; remains optional future GitHub integration work** |

**Historical sequencing (superseded):** this plan expected every phase to ship alone,
Phases 39–43 to form one coherent release, and Phase 45 to falsify or prove the trait. The
Phase 50+ cycle now supplies the active sequencing and acceptance gates.

**Two findings from the planning read that change what "done" means:**

1. **Nine of thirteen trait methods have no test asserting what leaves the process.** Only
   four of the 37 `DocketAdapter` tests check an outgoing request. A rewrite could change
   what docket receives on nine methods and CI would stay green. Phase 39 exists solely to
   fix that before anything else runs.
2. **`orch_tasks.tokens_in`/`tokens_out` are written as literal `0` by
   `dispatcher.rs:382` and updated by nothing.** Every token figure in the Fleet view, the
   Budget panel and the whole Economics page currently reads a structural zero — for
   docket too. Phase 46 builds the first real measurement path.

### Phase 39 — Regression Oracle **implemented in working tree; unreleased**

**Goal:** make the docket adapter's _and the reconciler's_ observable behaviour a
committed artifact, so the reshape has something to be proved against.

#### Task 39.1 — Tick-level contract test (the primary oracle)

`crates/tack-orch/tests/docket_tick_contract_test.rs`. Drives a full `reconcile_once` plus
the whole persist phase against a `wiremock` docket **and** an in-memory SQLite — both
patterns already exist in `tests/ingestion_test.rs` and `tests/traces_ingestion_test.rs`.
Snapshots two artifacts to `tests/golden/`: the **ordered** list of HTTP requests the tick
issued (method, path, sorted query, headers present, canonicalised body) and the resulting
rows of `orch_runs`, `orch_approvals`, `orch_events`, `orch_metrics`,
`orch_trace_cursors`, deterministically sorted. Five scenarios: cold start, warm cursor,
**rewound cursor**, zero linked projects, three linked projects.

#### Task 39.2 — Per-method wire contract test (secondary)

`crates/tack-orch/tests/docket_wire_contract_test.rs` — request transcript plus decoded
result for all thirteen current methods.

#### Task 39.3 — Pinned-literal event id

`derive_event_id`'s namespace constant carries a _"must never change once any deployment
has ingested a single event"_ warning, and the existing determinism test only proves
determinism **within one build**. Add an assertion against a committed literal UUID.

#### Task 39.4 — CI gates

A golden-drift step in the `rust` job mirroring the existing OpenAPI gate, plus
`cargo llvm-cov -p tack-orch --fail-under-lines 70` — there is **no `tack-orch` coverage
floor today**, which makes the adapter the least-guarded code in the workspace.

**Acceptance:** `UPDATE_GOLDEN=1 cargo test -p tack-orch --test docket_tick_contract_test
&& git diff --exit-code crates/tack-orch/tests/golden/` exits 0 on an unmodified tree; and
each of the three wrong refactors documented in the plan (delegate-and-re-scope, dropping
the retention guard, changing the id derivation) fails at least one committed test.

### Phase 40 — Capability Model & Adapter Registry **implemented in working tree; unreleased**

**Goal:** make "what can this provider do" a typed value the UI reads, and give a plane
somewhere to keep provider configuration and credentials.

#### Task 40.1 — `Capabilities`

`dispatch`, `cancel`, `pause: Support{Unsupported,Advisory,Supported}`, `resume`,
`event_scope: EventScope{None,Run,Project,Plane}`, `artifacts`,
`decisions: DecisionSupport{None,Poll,Push}`,
`usage: UsageSupport{NotMeasured,FromProvider,FromGateway}`,
`model_selection: ModelSelection{Unsupported,Advisory,Honoured}`, `runtimes`,
`plane_metrics`, `provisioning`. Each carries a `reason` for the disabled case.

#### Task 40.2 — Migrations 032 and 033

`control_planes.config` then `control_planes.secrets`, **one `ALTER` each**. The `secrets`
commit also adds its block to `remote_backup.rs::scrub_snapshot_secrets`, before the
trailing `VACUUM`.

#### Task 40.3 — `health = 'unconfigured'`

A restored backup has `secrets IS NULL`, and `orch_store.rs` currently `continue`s past a
failed adapter construction with only a `warn!` — the plane **vanishes from polling
invisibly**. Benign for docket (its token is optional), fatal and silent for any plane
whose credentials are required.

#### Task 40.4 — One adapter registry

`tack_orch::adapters::registry::build(kind, base_url, config, secrets)` replaces four
copy-pasted `match kind` sites. It must live in `tack-orch` — `crates/tack-orch/Cargo.toml`
forbids depending on `tack-api`. The four callers keep their **different** failure
behaviour: one warns and continues, three error.

#### Task 40.5 — GitHub Actions compile-only stub

`kind()` and `capabilities()` truthfully filled in, every other method `unimplemented!()`,
never registered. Its only job is to make _"both adapters compile against the trait"_ a
Phase 43 gate rather than a Phase 45 discovery.

#### Task 40.6 — Fix the auto-dispatch gate

`handlers/items.rs` gates auto-dispatch on `state.config.orch_enable`, not
`effective_orch_enabled` — so it **ignores the UI toggle today**. With a GitHub Actions
plane that means a workflow dispatched automatically while the UI reports orchestration
off. A behaviour change to a shipped feature; note it in `CHANGELOG.md`.

#### Task 40.7 — Surface capabilities in the API and the UI

`GET /api/control-planes/{id}` and `GET /api/fleet` carry them; every gated control reads
them. Retire `grant_available` and `useAgentActivityMap`'s `orchAvailable()`-as-dispatch-gate
— the latter really means _"orchestration is on"_, not _"this provider can dispatch"_.

**Acceptance:** `rg -c "match .*kind\.as_str\(\)" crates/tack-api/src/` returns 0;
`rg -n "kind === 'docket'|grant_available" frontend/src` returns 0; a disabled control
renders a reason string **sourced from the capability**, asserted by a Vitest test.

### Phase 41 — Optimistic Concurrency ~~**partial; acceptance reopened**~~ **acceptance closed by Part III Wave 0**

**Goal:** make a lost update detectable, and be honest about the writers that never go
through HTTP.

> **Acceptance closed 2026-08-14** (verified against the tree at `5c6842f`, not taken from a
> handoff). The reopened half was atomic write plus browser ETag; both now hold.
> `Repository::update_item_atomically` (`crates/tack-db/src/repo/items.rs`) performs the WIP
> count and the conditional update inside one `BEGIN IMMEDIATE` transaction, and the browser
> sends `If-Match` and treats `412` as concurrency feedback rather than a network error
> (`frontend/src/shared/api/items.ts`). Proven by the 9 tests in
> `crates/tack-api/tests/item_concurrency_test.rs` — including
> `multi_field_wip_rejection_writes_nothing_and_does_not_bump_version` and
> `before_update_failure_cannot_partially_apply_a_multi_field_patch`, which assert the
> absence of a partial write rather than only a status code. Delivered by III-A1/III-A2;
> **unreleased**. The original status is struck through, not deleted.

#### Task 41.1 — Migrations 034–036

`version INTEGER NOT NULL DEFAULT 1` on `items`, `orch_links`, `control_planes`, **one
`ALTER` each**. The repo layer bumps it on every `UPDATE`.

#### Task 41.2 — `ETag` and `If-Match`

`ETag` on `GET`, `If-Match` on `PATCH`/`PUT`, `412` on mismatch. **An absent `If-Match`
preserves today's behaviour exactly**, so nothing breaks.

#### Task 41.3 — CORS

`router.rs` has **no `expose_headers` call at all**, so a browser can read no
non-safelisted response header today. Add `expose_headers([ETAG])`, and add `if-match` and
**`x-tack-approval-token`** to the allow-list. The latter is a pre-existing bug: the decide
call works only because production is same-origin via `embed-spa`, and is already broken on
any cross-origin `TACK_ALLOWED_ORIGINS` path. This ships the repo's first CORS test.

#### Task 41.4 — MCP writes send `If-Match`

`tack-cli`'s client `request()` **cannot set a header at all**, so every MCP write is
unconditionally last-write-wins. The agent-versus-human race is precisely the one this
phase exists for, and the agent path is the unprotected one.

#### Task 41.5 — Document the writers that bypass HTTP

The reconciler calls `dispatcher::apply_mapped_status` directly, with no request and no
`If-Match` — **the largest single mutator of `items.status` is outside this control by
design**. And `propagate_parent_completion` mutates a _parent_ item on a child's PATCH, so
a parent's ETag changes with no caller having touched it. Both are correct; both must be
written down so a client does not conclude `412` is a total ordering.

**Acceptance:** replaying a stale `ETag` after a write has landed, and presenting an `ETag`
belonging to a different item, each produce `412` — sequentially, so the check is
deterministic; a `PATCH` with no `If-Match` still succeeds; and a preflight response allows
`if-match` and `x-tack-approval-token` and exposes `etag`.

> **Corrected 2026-08-06.** This phase originally accepted on "two concurrent `PATCH`es
> carrying the same `ETag` produce exactly one `200` and one `412`". An adversarial pass
> that made the `If-Match` comparison always succeed — so a stale `ETag` is accepted and
> `412` never returned — was caught by that test only 5 times in 15 runs, because the
> underlying atomic `UPDATE ... WHERE version = ?` reproduces the same shape by coincidence
> when two racers share one still-valid version. Observing a race is not the same as
> proving a precondition was checked. The concurrent test remains in the suite as a
> property test of the compare-and-swap layer.

### Phase 42 — Run Identity & Decision Store Rebuild ~~**transitional; acceptance reopened**~~ **acceptance closed by Part III Wave 0**

**Goal:** give a run an identity two providers can share, and let a decision exist without
a control plane.

> **Acceptance closed 2026-08-14** (verified against the tree at `5c6842f`). Every clause of
> the acceptance below now holds in `crates/tack-db/src/migrations.rs`: migration 037 rebuilds
> `orch_runs` with `PRIMARY KEY (control_plane_id, external_run_id, run_attempt)` and a
> separate `correlation_id TEXT UNIQUE`; migration 038 adds `kind`, `external_id` and
> `provider_metadata` to `orch_approvals` while `token` stays the primary key; and the
> half-applied-boot guard refuses to re-run a partial rebuild rather than silently retrying
> `DROP TABLE`. Both rebuilds run as a transactional copy/verify/swap behind a `VACUUM INTO`
> snapshot. Proven by the 31 tests in `crates/tack-db/tests/orch_migrations_test.rs`, which
> inject failure at every boundary. Delivered by III-A3; **unreleased**. The original status
> is struck through, not deleted.

#### Task 42.1 — Migration 037, rebuild `orch_runs`

SQLite's 12-step procedure, **this table only**: `PRAGMA foreign_keys=OFF`, create with
`PRIMARY KEY (control_plane_id, external_run_id, run_attempt)` and `correlation_id TEXT
UNIQUE`, `INSERT ... SELECT` copying `run_id` into `external_run_id` with `run_attempt = 1`,
drop, rename, recreate indexes, `PRAGMA foreign_key_check`.

#### Task 42.2 — Migration 038, rebuild `orch_approvals`

`control_plane_id` becomes nullable; add `kind`, `external_id`, `provider_metadata`.
`token` stays the primary key and the URL segment — renaming a column that is in a user's
database buys nothing.

#### Task 42.3 — Half-applied-rebuild guard

`run_all` refuses to boot if both `orch_runs` and `orch_runs_new` exist, with an error
naming the backup endpoint, rather than re-running `DROP TABLE`.

#### Task 42.4 — Release note

This upgrade rewrites two tables. Take a backup first.

**Acceptance:** a seeded database at migration 036 upgrades with identical row counts and
per-row field equality, an empty `PRAGMA foreign_key_check`, and the old PK's uniqueness
still enforced; a deliberately half-applied state refuses to boot with a named error.

### Phase 43 — Agnostic `ControlPlane` Trait **superseded; do not start** (breaking)

**Goal:** replace the docket-shaped trait with the agnostic one, and prove docket's
behaviour did not move.

Sixteen methods, every one capability-gated. Four corrections against the obvious design,
each forced by real code:

- **`events` is scoped, not per-run.** `RemoteEvent` carries no run id and `persist_events`
  says so outright. A per-run `events()` is _unimplementable_ for docket — every event
  currently ingested would be dropped. `EventScope::{Run, Project, Plane}` plus
  `capabilities().event_scope` declares which shape an adapter serves.
- **`dispatch` returns a rich `DispatchAck`.** Today `dispatcher.rs` makes a **second**
  call to `list_tasks` purely to recover `remote_status` and `approval_token`. Deleting
  `list_tasks` without widening the ack makes `approval_token` permanently `null` and sends
  every approval-gated dispatch down the `on_running` branch. **The OpenAPI drift gate
  cannot catch that** — the field still exists, only its value dies.
- **`RunState` stays a normalized closed enum on the trait**, never in `provider_metadata`.
  `orch_store.rs` is the only place a finishing agent moves a card and it matches three
  literals; GitHub has nine conclusions, seven of which would fall through with no error,
  no log and no event, leaving cards permanently in "In Progress".
- **`plane_metrics()` stays** (D7). docket's `/metrics` is plane-wide with no run or project
  dimension, and `GET /api/projects/{id}/orch-policy` is built entirely from it including a
  server-computed `denial_rate` — a per-adapter UI fragment cannot produce a number the
  server already committed to in the spec.

#### Task 43.1 — Normalized DTOs and the `RunState` mapping table

`PlaneHealth`, `Runtime`, `DispatchTarget`, `DispatchRequest`, `DispatchAck`, `RunHandle`,
`RunStatus`, `RunState`, `EventScope`, `EventPage`, `RunEvent`, `Artifact`, `Decision`,
`DecisionKind`, `DecisionAnswer`, `DecisionState`, `Usage`, `CorrelatableRecord`. `RunState`
becomes `Queued | Running | Blocked | Succeeded | Failed | Cancelled | TimedOut |
Unknown(String)`, keeping the existing `remote_string_enum!` round-trip discipline. GitHub
`waiting` and `action_required` map to `Blocked` **and raise a `Decision`** — a deployment
gate is a human waiting, not a terminal state.

#### Task 43.2 — The trait, and `DocketAdapter` rewritten

`provision_pod` leaves the trait for a provider-specific route.

#### Task 43.3 — Reconciler restructure

`evaluate` consumes **only** `health()`. `EXPECTED_API_VERSION` moves out of the reconciler
into the docket adapter, and `PlaneHealth` carries `api_version` and `version_ok` — the
adapter decides what "reachable" means, so docket keeps requiring both `/health` and
`/status.json` while a GitHub plane does not go unreachable for lacking a runner-admin
scope. Correlation moves to a first-class `correlation_keys()` rather than reaching into
`RemoteRun.task_ids` and `RemoteApproval.context.taskId`.

#### Task 43.4 — `status_map` gains `on_blocked` and `on_timed_out`

Both optional; absent means fall back to `on_waiting_approval` and `on_failed`, so **every
`status_map` already saved in a user's database behaves exactly as it does today**.

#### Task 43.5 — `tack-api` onto the new trait

The read-back call in `dispatcher.rs` disappears because `DispatchAck` carries `state` and
`pending_decision_id`.

#### Task 43.6 — Resolve the `tack-core` layer violation

Move `OrchBlueprint` and `TemplateOrchestration` out of the pure-domain crate into
`tack-api::handlers::provisioning`. `project_templates.orchestration` is already `TEXT`, so
`tack-core` keeps only an opaque `serde_json::Value`. No migration.

#### Task 43.7 — Regenerate the contract, write the breaking-change notes

#### Task 43.8 — Frontend: neutral shapes, lazy provider fragment

`shared/orch/providers/docket/` loaded with `lazy(() => import(...))`, the pattern already
used for every route, so the 30 KB gzipped entry-bundle gate is unaffected. `Pod health`
becomes `Health`; `Roster` becomes `Runtimes`.

**Acceptance:** the Phase 39 tick golden is **byte-identical**; both adapters compile
against the new trait; a `status_map` saved before this release behaves identically;
`rg -n "blueprint|Blueprint|\bpod\b|docket" crates/tack-core/src/` returns 0;
`rg -n "Pod health|Roster|Burn vs budget" frontend/src` returns 0; and
`dispatch_ack_carries_the_approval_token_without_a_second_call` asserts both a non-null
token and exactly one docket request.

### Phase 44 — Unified Decision Inbox **superseded by Phase 55**

**Goal:** turn the approvals inbox into a four-kind decision inbox any provider, or an
uninstrumented hook, can feed.

"Blocked waiting on a human" exists in every provider in a different shape: an approval
token in docket, a hook exiting 2 in Claude Code, an environment protection rule in
Actions. The inbox is already the most active surface in the app — it is the **only**
polling loop in the entire frontend, while the Fleet page does not poll at all.

#### Task 44.1 — Four kinds

`ApprovalOfIrreversibleAction`, `PlanAwaitingReview`, `OpenQuestion`,
`WorkOrderAmbiguity`, end to end.

#### Task 44.2 — Routes move to `/api/decisions`

Per D3, broken with notice, no alias. `TACK_ORCH_APPROVAL_TOKEN` keeps its exact meaning —
resolving a decision stays higher-privilege than editing a card.

#### Task 44.3 — Per-kind answer controls, axe-clean

#### Task 44.4 — Decision provenance without an actor

With one shared secret there is no per-user identity, so the audit row records the
**surface** a decision was resolved through and the UI never renders a name it does not
have.

**Acceptance:** the four kinds render distinctly; the existing approval-token gating tests
pass verbatim in behaviour; the UI never implies attribution.

### Phase 45 — GitHub Actions Adapter & Telemetry Ingest **superseded; do not start**

**Goal:** the falsification test. A second adapter, end to end, including the only way its
agent telemetry can exist.

**Where it runs:** **GitHub-hosted runners** first. Zero infrastructure, honours "one
binary, no runtime dependencies", and the adapter's job is to falsify the trait, not to
reach local resources. Self-hosted costs Tack **zero code** — `runs-on` lives in the target
repo's workflow file, which Tack neither writes nor reads — and is the only option that
reaches a local GPU, database or `.env`. "Under docket" is rejected: it would make adapter
2 a docket variant and prove nothing.

**How progress reaches the panel:** two independent channels. Run lifecycle is _pulled_
from the provider API (and later pushed by webhooks). Agent telemetry is **pushed from
inside the run** by a `PostToolUse` hook, because no provider API reports tool-level detail
— GitHub knows "job 2 of 3, in progress" and nothing more, because GitHub is not the agent.
That is what forces an inbound endpoint Tack does not have today.

The ingest endpoints are **in this phase, not before it**: their only consumer is this
adapter's handshake, and shipping a separately-credentialed write endpoint with zero
callers is pure attack surface.

#### Task 45.1 — The adapter

`health` uses a cheap authenticated `GET /repos/{o}/{r}`, **never** the runner list — that
needs repo-admin and a 403 would pin every plane at `unreachable` through the backoff.
`events` is `EventScope::Run`, derived from `GET .../runs/{id}/jobs` steps with the cursor
as the highest `(job_id, step_number)`; logs are not used at all. `pending_decisions` and
`resolve_decision` map to `pending_deployments`. `pause`/`resume` are `Unsupported`. Raw
`reqwest` — no `octocrab`, per the crate's own rule against a second HTTP client.

#### Task 45.2 — Correlation, with a single-use nonce

Tack mints `tack_run_id` and passes it as a **non-secret** workflow input. Because that
input is caller-supplied, anyone with `actions:write` could forge one, so
`POST /api/fleet/runs/bind` verifies it was minted **by Tack, for that plane, and is still
unbound**, and consumes it. That one call is simultaneously the handshake, the correlation
binding, and the exchange that returns the **per-run credential** (D5) — which therefore
never appears anywhere loggable. **Workflow inputs are visible in the run's UI and logs**;
no vendor key, gateway key or API token ever travels as one.

#### Task 45.3 — `POST /api/fleet/runs/{correlation_id}/events`

Authenticated by the run credential only, idempotent on a caller-supplied `event_id`,
rejects events for a run in a terminal state.

#### Task 45.4 — Migrations 039–041

`orch_events.source` with the **three-value** vocabulary plus a backfill of existing
locally-minted rows to `'local'`; `external_id`; a partial unique index over the new column,
which cannot fail on existing data.

#### Task 45.5 — Auth wiring, outside both existing gates

A run must not need `TACK_API_TOKEN`, and toggling orchestration off in the UI must not
`409` an in-flight handshake and re-label every live run "not instrumented". These routes
get their own sub-router with their own guard — **not** a fourth entry in the
`path().ends_with(...)` exemption list, which would silently exempt any future path ending
in the same string.

#### Task 45.6 — Reference workflow and hook

Under `docs/examples/github-actions/`, with the operator guide. Documents honestly that
fork-PR runs receive no secrets, cannot bind, and will correctly render "not instrumented".

#### Task 45.7 — "Not instrumented" versus "waiting on a human"

A missing bind is **not** enough to declare a run uninstrumented: a run parked on a
required-reviewer environment sits in `waiting` for up to 35 days and is exactly what the
decision inbox exists to surface. The timer is suppressed whenever the run is `waiting` or
a decision is open against it. Separately, a run that goes `queued -> cancelled` without
ever starting never arms an `in_progress`-based timer at all, so the reaper runs off a
**dispatch-time deadline** and leaves an event explaining why the card stopped.

#### Task 45.8 — Frontend: a second Kind, with its own config form and lazy fragment

**Acceptance:** a forged, a reused, and a wrong-plane nonce each `403` and create no run
row; replaying an event batch leaves the row count unchanged; a `waiting` run is never
labelled "not instrumented"; a run cancelled before starting is reaped and leaves an event;
and a test enumerating the router asserts the token-exemption set is exactly the intended
paths.

### Phase 46 — Model Policy & Gateway **superseded by Phase 56**

**Goal:** the operator picks the model for a piece of work from the UI, without editing
YAML or restarting anything, and can see where the choice came from.

Resolution order: **item override -> item-type default -> project default -> control-plane
default.** The UI shows the **resolved** value and its provenance — "sonnet, from project
default". A policy whose provenance is invisible is a policy nobody trusts.

#### Task 46.1 — Migration 042, `orch_model_policy`

#### Task 46.2 — Pure resolution in `tack-core`

It **never parses, maps, normalises or classifies the identifier**. This also removes the
staleness problem permanently: a new model needs no Tack release.

#### Task 46.3 — API returns the resolved value with its provenance

#### Task 46.4 — `capabilities().model_selection` respected, all three values live

docket `Unsupported` (it owns its routing), GitHub Actions `Honoured` (forwarded verbatim).
**This is the capability-negotiation acceptance test for the whole cycle.**

#### Task 46.5 — Per-project gateway config

Base URL, an optional **server-side read-only spend-query credential**, and nothing else.
The run gets its own key from a repo secret the operator sets; Tack never sends one into a
run. When a gateway is configured and a pre-dispatch probe fails, dispatch returns a new
`gateway_unreachable` outcome and **no run starts** — a run that silently bypasses the
gateway is unmeasured and uncapped.

#### Task 46.6 — The first real token measurement

Roll pushed telemetry and docket's own `cost_charged` events into `orch_tasks`. Where no
source exists, render **"not measured"**, never `0`.

#### Task 46.7 — `orch_links.harness`

One plain config field. No trait, no capability matrix, no plugin layer.

**Acceptance:** all sixteen presence combinations resolve correctly; a nonsense identifier
round-trips unchanged; three capability values render three different controls; a project
with no measurement source renders "not measured" and never `$0.00` or `0 tokens`.

### Phase 47 — Inbound GitHub Webhooks **frozen; future optional integration**

**Goal:** replace polling latency with push for run lifecycle, and add the inbound half of
the pipeline.

#### Task 47.1 — `POST /api/webhooks/github/{control_plane_id}`

`X-Hub-Signature-256` verified with the `hmac`/`sha2`/`hex` crates already present —
`webhook.rs` has `sign` and no `verify` today — using the existing `constant_time_eq`.

#### Task 47.2 — Delivery dedupe on `X-GitHub-Delivery`

#### Task 47.3 — `workflow_run`, `workflow_job`, `deployment_review`

Polling stays as the reconciliation backstop, so a missed delivery self-heals.

#### Task 47.4 — Echo suppression, three layers

A `ChangeOrigin` tag so a webhook-driven write never re-fires `maybe_sync_github`; a
`github_links.state_hash` backstop for when the tag is lost across a process boundary; and
dropping deliveries whose `sender.id` is the identity Tack pushes as. `ItemSource` cannot
serve here — it is written once at creation and `update_item` never touches it.

**Acceptance:** a bad, missing, or wrong-plane signature each `401` with zero writes; a
replayed delivery GUID changes no row; a dropped delivery is recovered by the next poll;
and a webhook-driven status change produces **zero** outbound requests.

### Phase 48 — Intervention Without Pause **superseded by Phase 55**

**Goal:** fail-closed human-in-the-loop inside a run, on a provider with no pause API.

A `PreToolUse` hook runs **synchronously and blocks the tool call until it returns**. So it
can post a decision request to Tack and poll for the answer with a bounded wait, returning
allow or deny. That makes the decision inbox the mechanism that supplies **intervention**,
not just visibility.

#### Task 48.1 — Raise and await, on the run credential

The run credential can **raise** a decision and can **never answer one** — resolution stays
behind `TACK_ORCH_APPROVAL_TOKEN`.

#### Task 48.2 — The ceiling, stated and enforced

The reference hook sets `timeout: 600` **explicitly** rather than relying on the default,
and requests `wait=540`, leaving headroom for the round trip. 540 s is far under the 6 h
GitHub-hosted job cap, so the wait can never be what kills a job. **Expiry is fail-closed:**
deny, hook exits 2, tool call blocked, and an event records the expiry.

#### Task 48.3 — Where the item lands on expiry

`on_blocked` if set, otherwise the item does not move and the decision is recorded as
expired. Never silently "done".

#### Task 48.4 — Reference hook and the cost note

The wait is **paid idle runner time**, so decisions belong at genuine gates, not per tool
call.

**Acceptance:** a run credential cannot resolve its own decision; an expired decision
returns deny, writes an audit row, and never moves an item to a done status.

### Phase 49 — Bi-Directional GitHub Pipeline **frozen; future optional integration** — closes Phase 21

**Goal:** finish the bridge whose first foot shipped as Phase 21 v1.

| Event | Effect |
|---|---|
| Item created in Tack | Issue created on GitHub (optional, per project) |
| Issue created/edited on GitHub | Item created/updated in Tack |
| Dispatch from Tack | Run started, visible both sides |
| PR opened by the agent | Item moves state, PR linked |
| CI checks running | Item in verifying |
| Check failed | Item to failed, with the run link |
| PR merged | Item to done, with evidence: SHA, run URL, artifacts |
| Agent hits a blocking decision | In Tack's inbox **and** as a comment/label on the issue |
| Human resolves on either side | Reflected on the other |

#### Task 49.1 — Migrations 044–049

**One `ALTER` each**, plus a **non-unique** reverse index — a unique one could fail on a
user's existing duplicates, and a failed statement in this migration runner bricks the boot
loop. Uniqueness is enforced in the repo layer, which logs when it finds more than one.

#### Task 49.2 — Credential precedence, decided and written down

`TACK_GITHUB_TOKEN`/`TACK_GITHUB_API_BASE` remain the fallback; a control plane's own
credentials win where a plane is involved. Two token sources with different scopes exist
today and the rule was never stated.

#### Task 49.3 — Inbound issues and comments

Applied **through `tack-core`** so workflow rules hold, with `ItemSource::Github` preserved
so the trust boundary is not laundered.

#### Task 49.4 — Outbound item to issue, per project, opt-in, off by default

#### Task 49.5 — PR, check suite, and merge evidence

#### Task 49.6 — Decision mirroring both ways

#### Task 49.7 — Retry and rate limits for the outbound path

Today the push is `tokio::spawn`'d and never awaited, with zero retry and no persisted
failure record — unlike the auto-dispatch hook beside it, which writes an event. Add a
bounded retry honouring `Retry-After` and `x-ratelimit-reset`. No new dependency: `tower`
is already a workspace dep with `features = ["full"]`.

#### Task 49.8 — Rewrite `docs/GITHUB-SYNC.md` for v2

**Acceptance:** an issue edited on GitHub updates the item without bypassing the workflow
engine (an illegal transition is rejected, not forced); a merged PR completes the item with
SHA, run URL and artifacts all non-empty; resolving on either side reflects on the other
with no echo; and a rate-limited push retries and records a failure event.

### Multi-Agent Dispatch Plan (Phases 39–49)

Per-agent task cards, the file-ownership map, and the rules of engagement live in
**[TODO.md](../../../TODO.md)**, Part II. The full plan with per-item verification commands
is [docs/plans/agnostic-control-plane.md](../../plans/agnostic-control-plane.md).

**Sequencing at a glance:** Phase 39 is blocking and must land before anything is touched —
it is the only thing that makes the reshape safe rather than blind. Phases 40–42 are
independent of each other. Phase 43 is the widest single change in the cycle and is
deliberately one phase rather than two coexisting trait surfaces. Phase 45 is the phase that
either falsifies the trait or proves it; if it forces a trait change, that is **evidence**,
and Phase 39's golden re-runs to prove docket still did not move. Phases 46–49 are largely
independent tracks.

**Definition of done for the whole cycle:** an operator registers a GitHub Actions control
plane beside a docket one, dispatches the same sprint to either, sees per-item token cost
from both, resolves a blocking decision raised from inside a running workflow, and watches a
merged PR complete the card with its SHA and run URL — with every control the provider
cannot support visibly disabled and its reason named, and with docket's behaviour provably
unchanged throughout.

### Future / Optional

#### Multi-User / Auth
The current design is explicitly local-only and single-user (one shared token, no per-user accounts or identities). The API token (`TACK_API_TOKEN`)
covers the "shared on a LAN" use case. Full multi-user would require a proper auth layer (session
or JWT), per-user access control, and an audit log. The Phase 60 section "Identity and a
second person — later, with a trigger" records when this becomes necessary and the shape
it takes; it supersedes this paragraph in detail.

#### Notifications / Reminders
- Due-date notifications (OS native, email, or webhook)
- Recurring items

#### Import Formats
- ~~GitHub Issues import~~ — shipped in Phase 11
- ~~Linear export import~~ — shipped in Phase 12

#### Mobile / Offline
No current plans. The SPA is responsive on mobile browsers; no native app and no offline-first sync.

---

## Known Gaps

> **This table is the Phases 26–32 audit snapshot, not a current gap list.** Each row records
> what that audit found; the "Tracked in" column names the phase that took the work on, and
> those phases are marked done above. Rows verified closed as of 2026-08-14 are annotated
> inline. Read the owning phase's own section for what actually shipped versus what it
> deferred — several closed only partially. Current Part III gaps live in the Part III section
> below and on `TODO.md`'s board, which is the authority.

| Area | Gap | Tracked in |
|---|---|---|
| Item updates | `sprint_id` / `due_date` / `estimate_unit` not persisted by `update_item`; `started_at`/`completed_at` never set on ordinary status moves | Phase 26 (blocker) |
| Security | Alexa endpoint lacks Amazon cert-chain validation; no warning on unauthenticated non-loopback bind; tar-slip + unverified sha256 in backup restore; S3 secret embedded in backup bundles | Phase 27 |
| Backup as sync | No conflict detection between installs; scheduler ignores UI-saved settings; non-atomic restore swap | Phase 28 |
| API contract | ~~No OpenAPI spec; hand-maintained TS types and API docs have drifted from the router~~ **closed** — `docs/openapi.json` (90 paths) is generated from the code, `frontend/src/shared/api/schema.gen.ts` is generated from it, and CI fails on drift. "Two error JSON shapes" **still holds and is now deliberate**: `ErrorEnvelope` for operator routes, `RunnerV1ErrorEnvelope` for the runner protocol — two separate auth surfaces, not an oversight | Phase 29 |
| Vocabulary | Global "+ New" modal, Sprints view, tabs/palette/first-run guide hardcode "sprint"/"Story Points" | Phase 30 |
| Coverage reporting | ~~168 Vitest unit tests and a Playwright E2E suite ship; automated coverage thresholds in CI are not yet enforced~~ **closed** — CI's `coverage` job enforces `cargo llvm-cov --fail-under-lines` per crate (tack-core 85, tack-db/tack-api/tack-orch 70) alongside Vitest thresholds; the frontend suite is now 724 tests across 85 files | Phase 32 |
| Release integrity | No checksums/SBOM/provenance on release assets; SECURITY.md routes disclosure through public issues | Phase 32 |
| Custom field validation | `validation` rules enforced (pattern, min/max, min/max_length, max_items); full JSON Schema not supported | Future |
| Auth | No multi-user auth (by design for v1) | Future |

---

## Contributing

See [CONTRIBUTING.md](../../../CONTRIBUTING.md) for code style, PR process, and how to add new
features. The [Adding Features](developer/adding-features.md) guide walks through the
three most common extension patterns.

---

# Next — Harness-Agnostic Runner Fleet (Phases 50–57)

**Status:** feature-complete, **release still refused.** Phases 50–56 delivered; Phase 57 (the
Docket bridge, recovery and release phase) is the only phase remaining. Updated 2026-08-26
against `TODO.md`'s Part III board (integration SHA `6252f52`/`c193a77` on `develop`), the
authority for wave status and accepted integration SHAs.

**Corrected 2026-08-26 — the previous reading of this line was wrong.** It said the only
remaining blocker was that `codex` was not installed. `codex` has since been installed
(`codex-cli 0.149.1`; the smoke now reports 3 of 3 harnesses present for the first time in
this cycle) and `./scripts/smoke.sh --live` was run in full. It **failed**: the live
`opencode` attempt never reached a terminal state within its 300 s budget, and all three
harness kinds then failed step 8 — plausibly as a cascade of that hang saturating the
capacity-1 runner, though that has explicitly **not** been verified. Installing `codex` moved
the blocker; it did not clear it, and `codex` has still never completed a live attempt. The
Wave 9 amendment on the Part III board carries the full observation and the four genuinely
open items. Do not treat the release as one smoke run away.
This section supersedes the unstarted implementation work in Phases 43–49. Phases 39–42 and
every earlier section remain in this document as implementation and decision history.

Execution is tracked card-by-card on the **Part III board** in `TODO.md`, which is the
authority for wave status, card ownership and accepted integration SHAs. Per-card evidence
lives in `docs/agent-handoffs/part-iii/`. This section records the architectural intent; the
board records what actually shipped.

## Product outcome

Tack remains a project-management application and the source of truth for work. A user can
open any item, create an **execution request**, select an agent profile, fleet, harness,
model provider and model, and observe the resulting attempts without turning the item itself
into a provider-specific run record.

The first supported harness families are:

- Codex;
- Claude Code;
- OpenCode.

They are executed by a new pull-based `tack-runner`, not by the Tack API process. Docket is
no longer the architectural center. It may survive as an optional legacy bridge if that
provides value, but Tack must not maintain two competing schedulers for the same execution.
GitHub Actions remains a CI/integration target, not the proof of a coding-agent harness
abstraction.

## Why the boundary changes

The old `ControlPlane` boundary assumes another project already owns tasks, runtimes,
approvals, metrics and provisioning. That is appropriate for Docket, but not for Codex,
Claude Code or OpenCode: those are execution harnesses, not remote project-management
control planes. Generalising Docket's thirteen-method interface would preserve the wrong
ownership model and force harness adapters to invent unsupported fleet-wide APIs.

The new boundary is:

```text
Tack item
   │ creates
   ▼
Execution request ── durable queue / scheduler ──► Fleet
                                                   │ grants fenced lease
                                                   ▼
                                             tack-runner
                                                   │ local adapter
                         ┌─────────────────────────┼─────────────────────────┐
                         ▼                         ▼                         ▼
                       Codex                  Claude Code                OpenCode
                         │                         │                         │
                         └──── events / decisions / artifacts / usage ─────┘
                                                   │
                                                   ▼
                                immutable attempt history in Tack
```

## Vocabulary and ownership

These are different concepts and must never be collapsed into one `provider` field:

| Concept | Meaning | Example |
|---|---|---|
| PM item | Human intent and workflow state | “Implement export validation” |
| Execution request | Durable request to work on an item | Run the item with a selected policy |
| Agent profile | Instructions, role, tools, permissions and budget | “Rust reviewer, read/write repo” |
| Fleet | Scheduling group | “Local trusted development machines” |
| Runner | One registered worker process/machine | `runner-laptop-01` |
| Harness | Program that performs the coding-agent session | Codex, Claude Code, OpenCode |
| Model provider | Service that serves a model | OpenAI, Anthropic, a gateway, local runtime |
| Model | Opaque model identifier | Stored exactly as selected/reported |
| Attempt | One immutable execution of a request | Attempt 2 on `runner-laptop-01` |
| Decision | Human input required by a running attempt | Allow/deny a tool action |

Tack owns items, requests, scheduling state, leases and the normalized history. The runner
owns local workspace preparation, harness invocation, local credentials, process lifecycle
and conversion from harness output to the runner protocol. A harness adapter does not get
to mutate Tack item state directly.

## Non-negotiable architecture rules

1. **The API server never launches a coding harness.** Only `tack-runner` starts local
   processes. The server may schedule, lease, cancel and record them.
2. **Pull, do not push.** Runners authenticate, register capabilities and claim work. Tack
   does not connect to arbitrary runner URLs or send stored model credentials to a changed
   host.
3. **One item may have many requests and attempts.** PM workflow state and execution state
   remain separate. Status mapping is explicit, optional and passes through the workflow
   engine.
4. **Requested and actual execution facts are both stored.** A request records the desired
   fleet/harness/provider/model; the attempt records what the runner actually used, including
   harness version and capability snapshot.
5. **Capability values drive every choice.** The UI offers only combinations reported by
   eligible runners. Harness/provider/model combinations are not assumed to form a Cartesian
   product.
6. **Model identifiers are opaque.** Tack may compare identity and display provenance, but
   does not infer quality tiers or silently substitute a model.
7. **Secrets stay at the narrowest boundary.** Harness/model credentials remain local to a
   runner where possible. Tack stores hashed runner credentials and secret references, not
   raw vendor credentials inside execution payloads.
8. **No exactly-once claim.** The system guarantees at most one valid active lease through
   fencing. A crash after a local process starts can be ambiguous unless the harness supports
   resume/idempotency; ambiguous attempts become `needs_operator` and are never blindly
   retried.
9. **Every attempt uses an isolated workspace/worktree.** A lost or cancelled attempt must
   not leave another attempt writing the same checkout.
10. **Usage is nullable and sourced.** Missing measurement is `not_measured`, never zero.
    Estimated and provider-reported amounts remain distinct.
11. **The runner protocol is versioned and bounded.** Events, artifact metadata, heartbeat
    frequency, payload size and retention all have explicit limits.
12. **Docket is a legacy adapter, not a dependency.** New runner work must start and pass CI
    without Docket running or its repository being present.

## Execution and failure semantics

The normalized request/attempt lifecycle is:

```text
queued → leased → preparing → running → waiting_decision
   │        │          │          │              │
   └────────┴──────────┴──────────┴──────────────┼─► succeeded
                                                 ├─► failed
                                                 ├─► cancelled
                                                 ├─► lost
                                                 └─► needs_operator
```

- Claiming a request is one SQLite transaction that creates an attempt, installs a lease
  owner, lease expiry and monotonically increasing fencing token, and changes the request
  from `queued` to `leased`.
- Heartbeats and all writes carry the attempt id and fencing token. Writes from an expired
  owner are rejected.
- A runner keeps a small local journal before spawning the harness. On restart it reports
  the journal and either resumes/reconciles or marks ownership ambiguous.
- Lease expiry alone does not automatically start another process. The scheduler first
  classifies the previous attempt as safely recoverable, terminal, or `needs_operator`.
- Completion and the final event commit are idempotent. Replaying an event batch or terminal
  report changes no row and emits no duplicate WebSocket notification.
- Cancellation has requested and observed states. A request is not “cancelled” merely
  because Tack sent a signal; the runner must report termination or become lost.

## Additive data model

New execution work uses an additive namespace rather than stretching the existing
`orch_*` tables. Exact migration numbers are allocated by the single migration owner only
after Phase 50 decides the fate of unreleased migrations 037/038.

| Table | Required purpose/invariants |
|---|---|
| `agent_fleets` | Named scheduling groups and optional concurrency/default policy |
| `agent_runners` | Identity, hashed credential, state, last heartbeat, labels, capacity, capability snapshot and protocol version |
| `agent_fleet_members` | Many-to-many fleet membership; unique `(fleet_id, runner_id)` |
| `agent_profiles` | Reusable instructions, tool/permission policy, limits; no vendor secret values |
| `model_profiles` | Opaque provider/model/config reference and display name; optional runner-local secret reference |
| `execution_requests` | Item, requested selections, priority, idempotency key, state, cancellation request and immutable request snapshot |
| `execution_attempts` | Request attempt number, runner, fencing token, lease timestamps, actual harness/provider/model/version, terminal reason and nullable usage |
| `execution_events` | Idempotent ordered event stream; unique `(attempt_id, event_id)` with source and bounded payload |
| `execution_artifacts` | Metadata and content reference/checksum; content storage policy is explicit |
| `execution_decisions` | Pending/resolved/expired human decisions scoped to one attempt, with separate resolver authorization |

Repository APIs expose typed keys containing attempt and runner scope. No lookup may
correlate provider-local ids globally. All row conversion is fallible; malformed persisted
identifiers/timestamps surface corruption rather than panicking or fabricating `Utc::now()`.

## Runner protocol v1

Runner routes live under a separately authenticated, versioned `/api/runner/v1` router.
They are not added to the existing suffix-based bearer-token exemption list.

Minimum operations:

| Operation | Contract |
|---|---|
| Register/refresh | Exchange a one-time enrollment token, publish runner identity, protocol version and capabilities |
| Heartbeat | Idempotently refresh runner health/capacity and active-attempt leases |
| Claim | Long-poll or bounded poll; atomically grants one eligible request and fencing token |
| Accept/start | Records preparation/start facts before the local harness process is launched |
| Event batch | Caller-supplied event ids; the batch and checkpoint commit atomically |
| Decision poll | Returns unresolved decisions visible to that attempt only |
| Artifact manifest | Registers bounded metadata/checksum before optional content upload |
| Complete | Idempotent terminal report carrying actual execution snapshot and usage provenance |
| Cancel observation | Runner acknowledges whether the process stopped, was already terminal, or is ambiguous |

The operator-facing API separately creates/cancels execution requests, lists attempts and
events, manages fleets/runners/profiles, and issues/revokes enrollment credentials. Runner
credentials cannot create PM items or resolve their own privileged decisions.

## Harness adapter boundary

Adapters live in `tack-runner`, not in `tack-api`. The interface stays small and is proven
against real harness behavior before it is frozen:

```rust
pub trait HarnessAdapter: Send + Sync {
    fn kind(&self) -> HarnessKind;
    async fn probe(&self) -> Result<HarnessCapabilities, HarnessError>;
    async fn validate(&self, spec: &ExecutionSpec) -> Result<(), HarnessError>;
    async fn start(&self, spec: &ExecutionSpec, sink: &dyn EventSink)
        -> Result<LocalRunHandle, HarnessError>;
    async fn cancel(&self, handle: &LocalRunHandle) -> Result<CancelObservation, HarnessError>;
    async fn reconcile(&self, journal: &LocalRunJournal)
        -> Result<RecoveryObservation, HarnessError>;
}
```

No method may use `unimplemented!()`. Unsupported behavior is a typed capability/result.
Harness-specific configuration and raw events stay in adapter-owned metadata; normalized
lifecycle, decisions, artifacts and usage are first-class protocol values.

## Phase status

| Phase | Title | Status |
|---|---|---|
| 50 | Boundary, Safety & Contract Freeze | ✅ Done — Wave 0, integration SHA `f042085` |
| 51 | Durable Execution Domain & Schema | ✅ Done — Wave 1, integration SHA `f14019b` |
| 52 | Pull Runner Protocol & `tack-runner` | ✅ Done — Wave 2, integration SHA `f931fc0` |
| 53 | Codex / Claude Code / OpenCode Harness Proof | ✅ Done — Wave 3, integration SHA `6a53a18`; live-proof caveats below |
| 54 | Fleet Scheduler & Item Assignment UX | ✅ Done — Wave 4, integration SHA `8a6e613` |
| 55 | Decisions, Artifacts & Realtime Activity | ✅ Done — Wave 5, integration SHA `073aa4d` |
| 56 | Model Profiles, Policy & Honest Usage | ✅ Done — Wave 5, integration SHA `073aa4d` |
| 57 | Docket Bridge, Recovery & Release | **open** — feature work delivered through Wave 9; the tag is refused. `codex` is installed as of 2026-08-26 and the live smoke still fails; see the status correction above |
| 58 | Standalone Single-Binary Operation | **active** — see _Next — Standalone Single-Binary Operation_ below; does not depend on Phase 57's release |
| 59 | Adoption & First Public Release | **active** — see _Next — Adoption & First Public Release_ below; opened by the adoption audit of 2026-08-30 and independent of Phase 58 except for the demo card |

### Phase 50 — Boundary, Safety & Contract Freeze

**Goal:** enter implementation with one owner for each chokepoint, a green baseline, and no
ambiguity about which project schedules work.

- Add an ADR declaring Tack the scheduler/plan-of-record, `tack-runner` the process owner,
  and Docket an optional legacy bridge.
- Freeze/delete from production registration the compile-only GitHub Actions adapter; no
  panicking implementation may ship.
- Decide whether unreleased migrations 037/038 are retained, replaced or squashed before
  any new migration is numbered. Include an operator backup/recovery path.
- Fix the release-blocking trust boundaries: stored rich-text XSS/CSP, control-plane URL
  credential retention/SSRF, query-string secret logging, exact auth route matching,
  configuration precedence and unauthenticated non-loopback startup.
- Replace split version claims/field writes with atomic item/control-plane/link mutations;
  fix nullable PATCH semantics and browser ETag adoption.
- Restore a green frontend unit/E2E baseline and correct unresolved font packaging.
- Commit runner protocol fixtures, lifecycle transition tables, payload limits and error
  envelopes before API or runner implementation begins.

**Exit:** the full existing CI matrix is green; the contract fixtures are reviewed; every
migration and router/OpenAPI file has one named integration owner; no Phase 51+ card is
allowed to invent a field or lifecycle state independently.

**Delivered** (Wave 0, `f042085`): 42 frozen fixtures under `docs/contracts/runner-v1/`, ADR
0050, and the trust-boundary/atomicity/migration-recovery repairs. Later grown to 46 fixtures
when Wave 3 evidence showed `accept`/`start` had been omitted.

### Phase 51 — Durable Execution Domain & Schema

**Goal:** represent requests, attempts, fleets and runners without any Docket or harness
noun in the neutral domain.

- Add pure domain types and transition validation.
- Add the tables above through the repaired migration runner, with upgrade, rollback/recovery,
  FK, uniqueness and crash-boundary tests.
- Implement repositories for atomic enqueue, claim/fence, heartbeat, event batch commit,
  terminal completion, cancellation request and lease recovery.
- Add retention and bounded purge for execution events/artifact metadata.
- Add test builders and a fake clock so expiry tests contain no sleeps.

**Exit:** two concurrent claimers produce one lease; an expired fencing token cannot write;
replayed events/completion are no-ops; an ambiguous attempt is not automatically retried;
and a database opened at every supported prior migration upgrades without data loss.

**Delivered** (Wave 1, `f14019b`): the I/O-free execution domain in `tack-orch`, migrations
039–048 for the ten neutral tables, the `tack-runner` crate skeleton, and a contract-
conformance harness that byte-pins every fixture. Wave 2 later found that ten of this
schema's transaction sites deadlocked under concurrency — see Phase 52.

### Phase 52 — Pull Runner Protocol & `tack-runner`

**Goal:** a mock runner can enroll, claim and complete an execution without any real coding
harness installed.

- Create a workspace `tack-runner` binary with local journal, cancellation, graceful
  shutdown and isolated worktree/workspace management.
- Implement the separately authenticated runner router and operator runner/fleet APIs.
- Hash credentials at rest, support enrollment/revocation/rotation, and never return a
  stored credential after enrollment.
- Implement heartbeat health, capacity, fencing and recovery observations.
- Stream bounded events and artifacts using backpressure; do not buffer an unbounded run in
  memory.

**Exit:** an end-to-end mock execution survives API restart; runner restart either safely
resumes or becomes `needs_operator`; revoked credentials fail closed; duplicate reports do
not duplicate rows or notifications.

**Delivered** (Wave 2, `f931fc0`): 13 `/api/runner/v1` endpoints plus the operator execution
and fleet surface, mounted with structurally separate authentication — runner routes sit
outside the operator auth layer entirely rather than in an exemption list, and
`x-tack-principal` is overwritten from server config so a client cannot spoof the identity
that scopes idempotency. The gate is proven by `crates/tack-api/tests/wave2_gate.rs`, which
drives the real router through enroll → claim → accept → start → stream → **restart** →
complete and asserts persisted SQL state at each step.

Three defects worth recording, none of which the cards' own tests caught:

- **Ten transaction sites deadlocked** under concurrent access — deferred SQLite transactions
  that read then write, where two callers both upgrade from reader to writer and one gets
  `SQLITE_LOCKED`. Claim failed 25/25 under stress. All now use `BEGIN IMMEDIATE`; two
  write-first sites were stress-tested and deliberately left alone. The acceptance test that
  should have caught this was swallowing the error and retrying sequentially.
- **A fingerprint mismatch was reported as retryable `conflict`**, so a runner reusing an
  idempotency key with different content was told to retry a request that can never succeed.
  Now split into a non-retryable `idempotency_conflict`.
- **`retryable` was hardcoded false** on every operator error, against fixtures marking four
  codes retryable. The classification now lives in one contract-derived place.

### Phase 53 — Codex / Claude Code / OpenCode Harness Proof

**Goal:** prove the adapter boundary against actual coding harnesses rather than a second
remote scheduler.

- Start with three isolated probe adapters in parallel. Each detects installed version,
  reports capabilities, validates requested model/provider/config, executes a deterministic
  fixture repository and records the actual invocation/result contract.
- Reconcile discoveries once, then freeze `HarnessAdapter` v1. A harness limitation changes
  a capability, not another adapter's behavior.
- Implement production adapters with process-tree cancellation, output parsing, local
  journal/recovery and worktree isolation.
- Add opt-in live contract tests plus deterministic fake-binary tests for CI.

**Exit:** the same fixture item can be completed independently through all three harnesses;
an unsupported provider/model combination is rejected before leasing/spawn; cancelling one
attempt cannot kill another; and no adapter contains a panic placeholder.

**Delivered** (Wave 3, `6a53a18`): shared process/event infrastructure with bounded streaming
and explicit truncation, three adapters built independently against their real CLIs, and one
reconciliation pass. The adapter boundary held — the contract needed four small changes and
no fixture edit.

**Read this before Phase 54.** Probing real binaries falsified two assumptions the design had
been carrying:

- **No harness supports cancellation better than `Advisory`.** Claude Code's and OpenCode's
  shell tools each spawn their execution shell in a **new session**, outside the runner's
  process group — observed with `ps` against both real binaries, independently. Group
  signalling cannot reach those descendants. The registry now _refuses to register_ a probe
  claiming `Supported` cancellation, so an adapter cannot re-introduce the lie. Phase 54's
  scheduler must read the capability snapshot rather than assume cancellation works.
- **Only Claude Code can confirm which model actually ran** (from its `stream-json` `init`
  event; its `--output-format json` has no reliable field — an unrequested internal model
  appeared in `modelUsage` for a trivial prompt). Codex and OpenCode reject auto-selection
  pre-spawn rather than fabricate a value, so a request with no explicit model is
  unschedulable on two of three harnesses.

Also observed and acted on: OpenCode does **not** reject a valid model paired with the wrong
provider — it starts a session and fails afterwards — which is why the adapter validates
pairings itself and keeps provider/model combinations paired rather than flattened. And
`codex` was never installed on the development machine, so that adapter is proven only
against the shared fake binary; its documented assumptions about the real CLI remain
unverified until its opt-in live test runs before release.

### Phase 54 — Fleet Scheduler & Item Assignment UX

**Goal:** users can assign an item to an exact runner or a fleet and select only supported
harness/provider/model combinations.

- Scheduler filters by health, fleet membership, labels, capacity, harness capability and
  model compatibility, then orders deterministically by explicit policy.
- Add project defaults and per-request overrides without storing execution fields on the PM
  item.
- Add “Run with agent” from Board, item detail and Sprint, all using one shared capability
  and execution-request store.
- Add Fleet views for runner health, capacity, current attempts and reasons a selection is
  unavailable.
- Add CLI/MCP execution commands using the same API and optimistic-concurrency behavior.

**Exit:** manual exact-runner and automatic fleet assignment both work; saturated/unhealthy
runners are never leased; all dispatch surfaces make the same capability decision; and a
new request appears consistently in every work lens without duplicate WebSockets.

**Delivered** (Wave 4, `8a6e613`): the pure scheduler (III-E1) wired to live
`agent_runners`/`agent_fleet_members`/`agent_fleets`/`execution_requests` data, replacing a
naive `ORDER BY created_at LIMIT 1` claim match; new `GET /api/runners` and
`GET /api/executions/{id}/attempts[/{n}/events]` routes; and typed OpenAPI schemas for the
whole operator execution/fleet/runner/profile domain, replacing the `{}` placeholder schemas
flagged earlier as the biggest spec-drift item. "Run with agent" ships from Board, item detail
and Sprint against one shared capability/execution-request store, plus a CLI/MCP path and
Fleet views for health, capacity and unavailability reasons. The integration also found and
fixed a deadlock between two individually-correct Wave 4 designs: the modal could only ever
submit `Auto`-model requests (no live capability data existed to unblock a specific choice),
and the scheduler unconditionally rejects `Auto` — so nothing submitted through the landed UI
could ever be claimed by any runner. Healthy selection, saturation, exact-runner exclusivity,
unsupported-model rejection and realtime updates are each proven at the Rust, CLI and
Playwright layers.

**Genuinely open:** `agent_fleet_members` still has no write route on any API surface, so every
proof above uses exact-runner selection — fleet-membership eligibility itself is proven only
directly against the database, not through a live API; `execution_requests` still has no real
`priority` column (a `metadata`-convention stopgap stands in, documented as non-binding);
model-resolution/provenance (III-F3) was deliberately left untouched here, for Wave 5.

### Phase 55 — Decisions, Artifacts & Realtime Activity

**Goal:** one normalized attempt timeline contains agent output, human gates and deliverables.

- Add append-only event/activity UI with explicit source and truncation/retention state.
- Add decisions whose runner credential may raise/read but never resolve; resolution remains
  behind a separately scoped operator credential.
- Add artifact manifests/checksums and a bounded content-storage policy for patches, logs
  and generated files.
- Broadcast state changes after the database transaction commits, with event-id dedupe.
- Map terminal run state to an item only through an optional project status policy and the
  workflow engine.

**Exit:** an approval survives restarts, cannot be self-approved by its run, expiry is
fail-closed, artifacts verify against their checksum, and replay produces no duplicate
timeline or item transition.

**Delivered** (Wave 5, `073aa4d`): scoped decisions (III-F1) that a runner credential may raise
and poll but never resolve — resolution lives behind a second, independent
`TACK_EXECUTION_DECISION_TOKEN` gate, fail-closed when unset, mirroring
`TACK_ORCH_APPROVAL_TOKEN` exactly; verified artifacts with checksummed manifests and a real
content-upload/download path (III-F2, proven wired to the production router by integrator
sub-card III-F6a); and the execution-domain retention sweep plus health watch (III-F5), with
retention defaulted to **off** by the integrator — F5 had shipped it `true`, and deleting rows
and (since F6d) their on-disk blobs must be an explicit operator opt-in, matching
`TACK_ORCH_ENABLE`'s own posture. Integrator sub-card III-F6d found that F1's decision-expiry
sweep and F2's artifact/event sweeps had **zero callers anywhere in the tree** — F5 was
authored before F2's tables existed — and wired all three into `ExecutionRuntime` as one
joined, cancellable task. The frontend timeline renders `not measured` as that exact literal,
never `$0.00`, an em dash, or a blank cell.

**Genuinely open:** no decision-discovery or artifact-discovery/list endpoint exists anywhere
in the codebase (confirmed by reading every handler) — both UI list views stay honestly empty
by design, scoped to one attempt's own event stream; concrete route shapes are requested in
`docs/agent-handoffs/part-iii/III-F4.md`. Webkit Playwright coverage remains unverified —
`libwoff2dec.so.1.0.2` is missing from the build sandbox, first hit in Wave 4 and confirmed to
fail identically on untouched specs in Wave 5, so it is not a regression but genuinely
unmeasured.

### Phase 56 — Model Profiles, Policy & Honest Usage

**Goal:** choose and audit models without turning Tack into a model gateway or inventing
measurements.

- Add opaque model profiles and resolution provenance: request override → agent-profile
  default → project default → fleet default.
- Intersect the resolved choice with the selected runner/harness capabilities before lease.
- Persist requested and actual provider/model plus the source of the actual observation.
- Normalize nullable token/time/cost usage with `measured`, `estimated` and `not_measured`
  provenance. Never aggregate runner cost and model cost into one unlabeled number.
- Rebuild Economics on measured attempts; hide or label structural legacy zeros.

**Exit:** every presence/precedence combination resolves deterministically; opaque unknown
model ids round-trip unchanged; actual selection differences are visible; and absent usage
renders `not measured`, never `0` or `$0.00`.

**Delivered** (Wave 5, `073aa4d`): opaque model profiles and resolution provenance (III-F3) —
request override → agent-profile default → project default → fleet default → auto-select —
wired onto the live `POST /api/executions` path by integrator sub-card III-F6b (F3 shipped the
pure resolver but left it unwired from any HTTP path; F6b is the wiring). `AttemptSummary` now
carries `model_provenance` and `usage_economics`. Nullable usage is normalized as `measured`,
`estimated` or `not_measured`, never a fabricated zero.

**Genuinely open:** `projects` still has no default-model-policy storage —
`ModelPolicySources.project_default` is fully modeled in the pure type but always resolves to
`None`; III-F3's own handoff requests either a real `projects.default_model_policy` column or
an explicitly documented reuse of an existing column, decided by whoever owns the next
migration batch. No runner infra cost-rate is stored anywhere, so
`runner_time_cost.cost_usd_estimated` can never read anything but `not_measured` regardless of
harness — distinct from `model_token_cost_usd_estimated`, which OpenCode reports as a real
measured figure. `model_profiles` (migration 043) — the CRUD table for named provider/model
references — is consulted by nothing in the resolution path above, which reads its per-tier
defaults out of `agent_profiles.limits`/`agent_fleets.default_policy` JSON conventions instead.

**Read this before Phase 57.** Codex was not installed on the machine Wave 3 was implemented on
(III-D1), and nothing in the Wave 4 or Wave 5 handoffs reports installing it since — that
adapter is still proven only against the shared fake binary, so Phase 57's "one live workflow
completes through each supported harness" exit criterion is not yet met for Codex specifically.
Cancellation also remains `Advisory` on every harness, unchanged since Phase 53: no shell tool
spawns inside the runner's process group, and `AdapterRegistry::register_probe` still refuses
to register a probe that claims otherwise.

### Phase 57 — Docket Bridge, Recovery & Release

**Goal:** release the runner path without requiring Docket and without silently losing
useful legacy history.

- Decide whether Docket is maintained as `legacy-docket`, exported/imported into normalized
  attempts, or deprecated. Document one owner for scheduling; never dual-dispatch one
  request.
- Add compatibility views/migration tooling only where users have real legacy data.
- Run kill/restart/fencing tests at every remote/local side-effect boundary, a large-event
  soak, multi-runner capacity tests and a backup/restore drill including artifacts.
- Add protocol compatibility tests across one previous runner version, release notes,
  operator recovery procedures and an explicit rollback path.
- Remove/quarantine obsolete GHA/Docket-generalisation stubs and update public feature claims.

**Exit:** Tack starts and executes with no Docket configured; one live workflow completes
through each supported harness; ambiguous crashes require explicit reconciliation rather
than duplicate work; all CI/cross-browser/security/migration gates pass; the tree is clean
and the release is tagged.

## Explicitly deferred

- Multi-agent fan-out, supervisor/sub-agent graphs and automatic task decomposition. The v1
  scheduler grants one active lease per execution request.
- A model proxy/gateway inside Tack. Existing external gateways may be referenced by an
  opaque model profile, but model traffic never crosses the Tack API process.
- Inbound/bidirectional GitHub synchronization and GitHub Actions execution.
- Multi-user identity/RBAC beyond scoped operator/runner credentials. This becomes required
  before exposing the service to mutually untrusted users.
- A generic plugin ABI. Three in-tree adapters must first prove a stable boundary.

## Definition of done for the cycle

From the same Tack item, an operator can create separate attempts using Codex, Claude Code
and OpenCode; select an exact healthy runner or fleet; choose only a supported provider and
opaque model; see the requested and actual execution facts; resolve a bounded human gate;
inspect verified artifacts and an idempotent event timeline; and recover from API/runner
restart without silent loss or blind duplicate execution. Docket is optional, token/cost
values are measured or explicitly `not measured`, and no historical PM item or legacy
orchestration row is erased to achieve the cutover.

**Progress against that definition, as of III-H9 (`6252f52`/`c193a77` on `develop`):**

| Capability | State |
|---|---|
| Separate attempts through Codex / Claude Code / OpenCode | adapters exist for all three; claude-code and opencode proven live against their real CLIs. **Updated 2026-08-26:** `codex` is now installed (`codex-cli 0.149.1`) and the smoke reports 3 of 3 harnesses present for the first time — but codex has still never _completed_ a live attempt, so this stays two of three, never rounded up. The live run that followed the install failed for an unrelated reason (the opencode attempt hung, then all three failed step 8, plausibly as a saturation cascade — unverified). Scheduling itself is not the blocker (see next row) |
| Only a supported provider and opaque model | enforced pre-spawn, including wrong-provider pairings; resolution provenance (request → agent-profile → project → fleet → auto-select) wired onto the live create path (project tier still always resolves `None` — no storage exists). III-H5 closed the schedulability P0: claude-code/codex are schedulable via a pass-through capability attestation (`HarnessCapability.model_passthrough`), proven live with the real `claude` binary |
| Exact runner or fleet selection | scheduler live-wired to real data (Wave 4) and proven end-to-end at the Rust/CLI/UI layers. `agent_fleet_members` now has a write route (III-H8) — an operator can populate a fleet over the API and a fleet-targeted request schedules onto a member, proven against real DB rows |
| Requested vs. actual execution facts | recorded; actual model confirmable on Claude Code only (unchanged since Phase 53) |
| Bounded human gate | delivered (III-F1) — a runner may raise/poll a decision but never resolve it; resolution is fail-closed behind `TACK_EXECUTION_DECISION_TOKEN`, expiry is swept automatically; no decision-discovery/list endpoint exists, so the UI can only see decisions already attached to an attempt it has loaded; no harness in this tree yet asks a mid-run question, so the decision path is unexercised end-to-end (accepted scope limit, not a defect) |
| Verified artifacts and idempotent event timeline | **fully demonstrable live, both halves.** III-H6 wired the runner engine to submit a real terminal/cancellation event and attempt an artifact upload on every completed attempt. III-H9 then fixed the artifact **content** PUT, which had 500'd on every real upload — a runner-generated `artifact_id` (~220 bytes) was being hex-doubled past Linux's 255-byte filename limit; fixed by hashing the id instead. `./scripts/smoke.sh` now shows every artifact content upload as `200` with bytes confirmed on disk. Still no artifact-discovery/list endpoint — only per-id download |
| Recover from API/runner restart | proven end-to-end by the Wave 2 gate; III-H3 added crash-recovery cleanup for per-attempt checkouts; III-H2 proved restart recovery live by SIGKILLing the runner mid-attempt |
| Measured or explicitly unmeasured token/cost | OpenCode reports real token and cost figures; others report unmeasured; `runner_time_cost.cost_usd_estimated` can never read anything but `not_measured` for any harness — no runner cost-rate is stored anywhere |
| Docket optional, no historical row erased | holds — legacy orchestration untouched, frozen until Phase 57 |

Two release-blocking bugs found and closed since Phase 56: a losing credential-rotation race
answered a healthy runner with a false-fatal `401` instead of retryable `409` (III-H4), and a
second same-named runner enrolling on one host 500'd instead of succeeding (III-H7). Neither is a
capability gap in the table above, but both would have failed a real multi-runner deployment.

---

# Next — Standalone Single-Binary Operation (Phase 58)

**Status:** not started. Decided in
[`docs/adr/0058-standalone-single-binary-runner.md`](../../adr/0058-standalone-single-binary-runner.md).
Execution is tracked card-by-card on the **Part IV board** in `TODO.md` (§IV.0–§IV.6), which is
the authority for wave status, card ownership and accepted integration SHAs. This section
records architectural intent; the board records what actually shipped.

**Does not depend on Phase 57's release.** The two are independent: Phase 57 is about proving
and tagging the fleet, Phase 58 is about how the product is packaged and first run.

## Why this phase exists

Tack's stated principle is a single `tack` binary. The runner fleet, correctly, ships as a
second one. For a fleet of remote runners that is the right shape — but for the most common
case, one developer on one machine, it means: build or install a second binary, create a
pending runner, copy a one-time token out of terminal output, export three environment
variables, and keep a second process alive. Four manual steps and two artifacts before the
product does the thing it exists to do.

The feedback that opened this phase was blunt: the split is "totalmente antiintuitivo" and
"muy fraccionado para la mayoría de usos" against "su principio final de un solo binario".

## The observation that resolves it

**ADR 0050 separates roles, not binaries.** Its rules are about who schedules work, who owns
processes, who holds credentials, and which direction connections open. None of them say
anything about how many executables ship. The `tack` binary already hosts two distinct roles —
HTTP server and API client — chosen by subcommand. A third is consistent with that design
rather than a departure from it.

## Product outcome

One command, `tack serve --with-runner`, takes a machine with one harness installed and no
prior Tack state to a completed agent attempt visible in the UI. No second binary. No pending
runner to create. No one-time token to copy.

Distributed operation is unchanged: a fleet of remote runners works exactly as before, through
the same protocol and the same client, and `tack-runner` remains shippable on a machine that
has no server.

## Non-negotiable rules for this phase

- **The embedded runner speaks runner-v1 over loopback HTTP, like any remote runner.** No
  in-process handler calls, no shared `AppState`, no privileged path. There stays exactly one
  protocol-client implementation and one server-side path serving it. An in-process shortcut
  would be a second path free to drift from `docs/contracts/runner-v1/` — which the fixtures
  exist to prevent — and a path `scripts/smoke.sh` does not exercise, making the mode most
  users run the mode least proven.
- **`tack-api` never depends on `tack-runner`.** The composition root is `tack-cli`, which
  already depends on `tack-api`. ADR 0050's "the Tack API never starts a coding harness" stays
  literally true of the API crate.
- **Off by default; loud on failure.** An embedded runner executes arbitrary coding-agent
  processes on the host serving the UI — strictly more dangerous than anything Tack currently
  gates. It is opt-in (`--with-runner` / `TACK_LOCAL_RUNNER_ENABLE`), and a server that keeps
  running after its runner failed to start is indistinguishable from a scheduler bug, so that
  is an error rather than a log line.
- **Auto-enrollment is refused on any non-loopback bind.** Tack served to a team on `0.0.0.0`
  plus a self-enrolling agent executor is a remote-code-execution surface, not a convenience.
  Refusal is a startup error, never a silent downgrade.
- **Vendor credentials stay outside Tack.** Provider keys (Anthropic, OpenAI, OpenRouter, a
  local endpoint) live in each harness's own environment on the machine running the runner
  role. A Tack model gateway remains an explicit non-goal — see *Explicitly deferred* above,
  unchanged.
- **Packaging only.** No change to the runner-v1 contract, the scheduler, fleets, migrations,
  the operator API or the frontend.

## Scope

- Extract the runner's composition root into a reusable library entry point so the standalone
  binary and any embedder share one wiring rather than two that drift.
- Add a readiness and bound-address signal to the server so an embedder can start a runner
  against the real address once the listener accepts, without polling a guessed URL.
- `tack runner start` — the runner role in the `tack` binary.
- `tack serve --with-runner` — both roles in one supervised process, gated and loopback-checked.
- Zero-touch local enrollment: self-provision a pending runner in-process, then redeem the
  one-time token over loopback HTTP through the ordinary protocol path, storing the durable
  credential owner-only. Hashes-only storage server-side is unchanged.
- `tack runner doctor` — what this machine can run, what each harness declares, and where its
  model credentials come from. This closes a real reported gap: today that information exists
  only inside a capability snapshot posted to a server, so "how do I configure Claude, Codex,
  OpenRouter or a local model" has no local answer.
- A standalone smoke step that can genuinely fail, plus the configuration and provider-
  credential documentation `docs/CONFIG.md` currently lacks entirely.

## Explicitly deferred within this phase

- `tack.toml` support for the enable gate. That file is `tack-api`'s configuration surface and
  the gate belongs to `tack-cli`; a second reader of one file is not needed to deliver this.
- Any change to how models are selected, resolved or validated. `model_profiles` (migration
  043) remains consulted by nothing — a standing finding since Phase 56, untouched here.

## Exit

On a machine with one harness installed and no prior state, `tack serve --with-runner` is the
only command needed to reach a completed attempt. The embedded runner is off unless enabled,
refuses to auto-enroll on a non-loopback bind, and shares one code path with remote runners.
Remote fleets are unaffected. `runner_contract`, `wave2_gate` and `openapi_contract` are
unchanged and drift-free, because nothing in this phase may touch what they pin. Binary-size
growth is measured and recorded as a real number, never estimated.

---

# Next — Adoption & First Public Release (Phase 59)

**Status:** active, not started. Opened by the **adoption audit of 2026-08-30**, whose
findings are consolidated below. Execution is tracked card-by-card on the **Part V board**
in `TODO.md` (§V.0–§V.6), which is the authority for wave status, card ownership and
accepted integration SHAs. This section records the audit and the intent; the board records
what actually shipped.

**Independent of Phase 58** except for one card: the demo (V-C2) needs `tack serve
--with-runner` so the recording is one command rather than four steps and a copied token.

## Why this phase exists

Tack has been a public repository since 2026-03-15. On 2026-08-30 it had **zero stars, zero
forks, zero human-filed issues and one human contributor**. Every open pull request was from
Dependabot.

That is not a verdict on the code. The same audit measured ~122k lines of Rust across six
crates, ~45k lines of SolidJS, 1380 passing workspace tests, 57 migrations, a 92-path
documented API, a ~113 ms cold start at ~11.7 MiB resident, and durable execution semantics
— leases, fencing tokens, replay tables, recovery audits — that no competitor in the
category has. It is a verdict on the fact that **the project has never been released,
published, positioned, or shown to anybody.**

The preceding eight phases asked "does it work?". This one asks "can a stranger get it, run
it, and understand what it is?" — and today the answer is no, for reasons that are mostly
not code.

## What the audit found

### The install path advertised to the public does not work

`README.md` prints, as the headline install method:

```bash
curl -fsSL https://raw.githubusercontent.com/yielab/tack/main/install.sh | sh
```

**There is no `main` branch.** The repository's default branch is `develop`;
`git ls-remote --heads origin main` returns empty, and the URL has returned **404** since the
repository was made public. The script itself is fine — served from `develop` it returns 200.
Only the path is wrong, and `cargo install --git` masked it locally because cargo follows the
default branch. Card **V-A1**.

### The differentiator has never been downloadable

The only release is `v0.1.0-beta.6`, cut **2026-06-22**, whose four assets are `tack-*`
archives only. Everything that makes Tack distinct — the durable execution domain, the
`tack-runner` binary, harness adapters, fleet scheduling, decisions, artifacts, model
profiles: Phases 50 through 58 — postdates it entirely. A visitor who follows the README
today downloads a Tack with no runner fleet at all.

**A correction the audit made to itself, recorded so it is not repeated:** the first reading
was that CI does not package the runner. That is **false**. `release.yml` has built
`tack-runner` with `cargo auditable` and packaged it into its own per-platform archive, with
the systemd unit, since `7d78de3` on 2026-08-19. The gap is that **no tag has been cut
since**. Card **V-A3** cuts one; it does not fix CI.

### The live smoke fails, and Phase 57's blocker was misdiagnosed

Phase 57's tag was believed to be one `codex` installation away from green. It was installed
on 2026-08-26 — three of three harnesses present for the first time — and
`./scripts/smoke.sh --live` returned **`SMOKE FAILED`** for a different reason: a live
attempt that never reached a terminal state, and a step-8 failure message that had been
recorded as stale six days earlier and never reworded, which then misled two readers into
seeing a regression that was not there. Card **V-A2** owns the four open questions the Wave 9
amendment left. Codex has still never completed a live attempt.

### The positioning names the category where Tack loses

The repository description reads *"Self-hosted project manager in a single binary — no
Docker, no database server."* It does not mention agents. That sentence enters Tack in the
category of Plane (~54.6k ★), Huly (~26.9k ★), Focalboard (~26.3k ★), Leantime (~9.4k ★) and
Vikunja (~3.8–5k ★) — mature tools with large communities — while omitting the one capability
none of them has. The README's opening line then describes two products at once and assumes
the reader knows what a harness is. Meanwhile the mdBook under `docs/book/` is good and
**unpublished**: `yielab.github.io/tack` is a 404 and `homepageUrl` is empty. Card **V-A4**.

### The identity model is undeclared rather than decided

There is no users table, no sessions and no per-user permissions. `assignee` is a free-text
column; `roles` is a per-project colour label attached to items, not an identity.
Authorization is one shared bearer token, and when no token is configured `require_token`
allows everything by design, for pure-local mode.

That is a defensible design for a single operator. It is not "self-hosted for a team", and
the documentation does not distinguish them — a reader who sees "self-hosted" reasonably
assumes accounts exist. The posture also aged badly: Phase 27.2 made a non-loopback bind with
no token a **warning**, which was proportionate when the product stored task text and is not
now that the same server schedules agents that execute code. ADR 0058 already chose a startup
**error** for that shape of risk. Card **V-B1** decides and records the posture; it does not
build identity.

### Two execution models coexist in the schema and the UI

Parts I and II built a complete control plane against exactly one backend (docket): a
`ControlPlane` trait, a reconciler, an adapter, 11 `orch_*` tables — including
`orch_runs_new` and `orch_approvals_new`, leftovers of migration 037's rebuild — a
`control_planes` table, an Approvals inbox and a ControlPlanesManager. Part III replaced that
model with the native pull-based runner and kept docket as an optional legacy bridge. Both
now sit side by side, and a reader cannot tell which "fleet" is current.

The cost of removing it is not trivial and the audit will not pretend otherwise: **234 Rust
doc comments cite `TODO.md` section numbers** from those cycles, and the docket half of
`tack-orch` is interleaved with the runner-v1 execution domain that Part III depends on. Card
**V-B2** measures the surface, writes the ADR, and gates — deletion, if chosen, is a later
card that ADR authorizes.

## The competitive situation, and why the timing is the argument

The nearest thing to Tack that ever existed — **Vibe Kanban**, a kanban board orchestrating
Claude Code, Codex and Gemini agents — **shut down on 2026-04-10** when its company (Bloop)
closed. Its farewell states it had thousands of engineers using it daily and never found a
business model. **Crystal**, the other popular open-source orchestrator, was **deprecated in
February 2026**.

What remains above Tack in that category is closed-source (Conductor — macOS-only, $22M
Series A; Sculptor — Claude-only) or is not a board at all (OpenHands, Emdash). What remains
in the project-management category executes nothing.

**Tack is the only thing that is credibly both**, and there is an identifiable,
currently-orphaned audience for exactly that. Emdash (YC W26) is collecting it now. The
category's history carries one warning worth stating plainly: it killed its leader through
failure to monetize, not failure to be useful — which for a project with no company behind it
is an advantage rather than a risk.

## What this phase deliberately does not do

No card in Phase 59 adds a product feature. Three cards remove or gate; the rest are
packaging, proof and truthfulness. The real feature gaps the audit found are recorded here
and scheduled after adoption, not before it:

- **Outbound notifications / SMTP** — zero references in the tree today; the only webhooks
  are inbound from GitHub.
- **Internationalization** — the UI is English-only with no locale infrastructure at all.
- **Time tracking** — `estimate` exists; `time_spent` does not.
- **In-UI diff review of agent artifacts** — the step where a human actually decides.
- **A harness that asks a mid-run question** — the `decisions` path is built, contract-pinned
  and has never been exercised, because no harness in this tree asks anything. It remains a
  documented scope limit rather than a defect.
- **Multi-user accounts** — gated behind V-B1's posture decision.

The ten project-type presets (construction, legal, homework, events) are a live positioning
question: they pull the story toward generic project management, which is the losing
category. V-A4 records the trade-off for a human decision and changes no preset code.

## Exit

A stranger with no prior knowledge can find Tack, understand in fifteen lines what it is and
what it does not do, install it with the command the README prints, run one command to reach
a completed agent attempt, and watch a sixty-second recording of durable recovery that no
competitor can produce. Every capability claim on the first screen traces to a proof. The
identity posture is written down rather than inferred. Nothing is published without explicit
human approval.

---

# Next — Agent Onboarding & Provider UX (Phase 60)

**Status:** active, not started. Opened by the **agent-UX audit of 2026-09-03**, whose
findings are consolidated below. Execution is tracked card-by-card on the **Part VI board**
in `TODO.md` (§VI.0–§VI.6), which is the authority for wave status, card ownership and
accepted integration SHAs. This section records the audit and the intent; the board records
what actually shipped. The dispatch plan — per-card read lists sized for a 200k-context
agent, gates, stop conditions, and the per-wave integrator checklist — is
`docs/agent-handoffs/part-vi/README.md`.

**Runs alongside Phase 59's last wave.** It needs Phase 58 (done — the embedded runner is
what makes a UI-only path possible at all) and ADR 0060 (docket stays and is not touched).
It shares `README.md` and `docs/screenshots/` with Phase 59's V-C2/V-C3; the rule is in
`TODO.md` §VI.3. VI-A3 restructures the README that V-A4 wrote; V-A4's four questions and
its claim → evidence rule carry over unchanged.

## Why this phase exists

Phase 58 made one command reach a completed attempt. Phase 59 makes the project findable and
its claims honest. Neither answers the question a user asks in the first minute — *which
model, from which provider, and where do I put the key?* — and today the product answers it
with a negation: `docs/CONFIG.md` says there is *no* `TACK_*` variable for a model provider,
by design. That is true of credentials, and every reader takes it to mean they cannot choose
a model, while the request body, the CLI and the modal all route exactly that choice.

The deeper defect is structural. The steps between an installed binary and a completed
attempt are spread across **three surfaces** — a harness's own vendor login in a terminal,
the `tack` CLI, and the web UI — and no screen or page shows the whole path. The web UI can
run an item but asks for a runner id, a git remote, a commit and a timeout by hand on every
run. The CLI has the full command tree and the CLI reference documents none of it. The
model-selection precedence that the scheduler implements exists only as a Rust doc comment.
Someone who did not build this cannot use it, and a stranger who installs beta.7 — the
release Phase 59 cut precisely so a stranger *could* — will find that out in the first ten
minutes.

The product's posture — Tack never proxies a model and never holds a vendor login — is
right and stays. This phase keeps it and moves the **guidance** into the product: one
screen that owns the path from an installed binary to a completed attempt, a project-level
answer to "which model", and one provider — Vercel AI Gateway — through which a user who
only wants the UI can authenticate with a single pasted key and choose among every model
the gateway serves, from a list the runner actually measured.

## What the audit found

### Three surfaces and no path

`docs/API-REFERENCE.md` delegates the execution surface to the agent-runners guide; the
guide has 433 lines on enrolling runners, credentials, capability matrices and recovery, and
**zero** examples of creating an execution. The only enumeration of the request's thirteen
required fields is a table row in `docs/MCP.md`. The Quick Start and *Working with Items*
pages do not contain the word "agent". The CLI reference omits `tack execution`, `tack
runner` (including `doctor`, which `docs/CONFIG.md` tells the reader to run), `tack fleet`,
`tack agent-profile` and `tack model-profile`. Two pages each claim to be the configuration
reference and list different variables. Cards **VI-A1** (now, against the product as it is)
and **VI-D1** (after the product changes).

### The provider answer is a negation

ADR 0050 keeps the API server out of the model-calling business; ADR 0058 says "vendor
credentials remain outside Tack". Both are statements about the API server and both are
correct. Both are cited — in `docs/CONFIG.md`, in `tack runner doctor`'s own output — as if
they meant "Tack cannot help you configure a provider". The runner side of the boundary was
never decided: the runner already owns its own credential, owns the harness subprocess and
its environment, and accepts `secret_reference` environment entries that every adapter warns
about and skips because "no secret-store client exists in tack-runner yet". Card **VI-A2**
writes ADR 0061 (see `docs/adr/0061-provider-credentials-at-the-runner-boundary.md`,
accepted 2026-09-03) and decides that side; cards **VI-B1** through **VI-B3** implement it.

### The docs contradict the code in four places

The harness id the guide prints (`claude_code`) is not the wire id (`claude-code`); a request
built from the page fails. The guide says model profiles have "no runtime effect"; the modal
copies the chosen profile into the request's highest-precedence tier. The guide says fleet
membership has no write route; the route exists and the Fleet panel says nothing calls it.
Two of the three "known gaps" listed are no longer gaps, which costs the third — the real one,
no list route for artifacts or decisions — its credibility. **VI-A1** fixes the first three;
**VI-C4** closes the fourth.

### The modal asks for what the product should already know

Five hand-typed fields per run, no memory between runs, a free-text runner id when
`GET /api/runners` exists, a fleet selector with no way to add a member from the UI, and no
project-level storage for a repository at all — the only repository link in the schema is
per-item `github_links`. **VI-C3** gives the project the three facts an attempt needs;
**VI-C2** makes the modal read them.

### The tiers exist; the storage and the UI do not

`resolve_model_policy` walks four tiers — request override, agent-profile default, project
default, fleet default — with an exhaustive test over every presence combination. The
project tier has no storage and always resolves to nothing; the agent-profile and fleet
tiers are JSON conventions inside `limits` and `default_policy` with no field in any panel.
The mechanism is finished and nobody can reach it. **VI-C3**.

### The README shows a project manager

The hero asset's own alt text is *"Board, Timeline, and vocabulary editor"*. The *Features*
section lists project management first and agent execution second. All five screenshots —
board, timeline, dashboard, list, vocabulary editor — are project-management views, and
**none** shows an agent doing anything. The two-component architecture is explained at line
169, after *Status*. The book's introduction lists four core concepts — item, workflow,
project type, vocabulary — with neither *runner* nor *run* among them. V-A4's first sentence
was right about the *what*; the shape of the page still says the opposite, and a reader who
arrives from the category Tack is trying to leave sees exactly that category. Card **VI-A3**
restructures the README, the introduction and the developer overview around one statement
and one diagram; card **VI-D2** makes the assets that show the execution plane, once there
is an honest screen to record.

## The surface map

The design authority for the phase, reproduced from the board. Every step from an installed
binary to a completed attempt, where it happens today, where it happens after this phase,
and — when the answer is not "the UI" — the structural reason, so the question is settled
once.

| Step | Today | Target | Why not fully UI |
|---|---|---|---|
| Turn on agent execution | console flag | **UI** — one switch on a loopback bind; the command only where the switch cannot exist | — starting Tack itself is the one command left |
| Install a harness binary | outside Tack | console, rendered in the UI per harness | external binaries |
| Authenticate a harness with its vendor login | outside Tack | console, rendered in the UI with a re-check | OAuth device flows need a TTY; Tack never holds them |
| Authenticate through Vercel AI Gateway | impossible | **UI** with the embedded runner; console for a remote runner, rendered in the UI | none in the embedded case |
| Enroll a runner | zero-touch / token → console | unchanged | — |
| Create a fleet, add a member | UI / API only | UI | — |
| Create an agent profile | UI | UI, default created on first use | — |
| Choose a default model | impossible | UI, from a measured catalog | — |
| Run an item | UI, five typed fields | UI, zero typed identifiers | — |
| See what an attempt produced, answer a decision | UI with a typed id | UI lists | — |

The last column is closed. A step that cannot move to the UI for a reason not in this table
is an amendment to ADR 0061, not a card's judgement.

## Vercel AI Gateway, and why it is the one provider this phase adds

The harnesses' own logins are the first case and already work. The gateway is the second
case, and the only one worth building now, because it is simultaneously: a **single API
key** rather than a vendor OAuth flow; documented by its vendor with a **dedicated endpoint
for each harness** in this tree (`/claude-code`, `/codex/v1` — fetched 2026-09-03, to be
re-fetched before any card relies on it); and a **catalog endpoint**, so the model picker
shows what a runner measured rather than a list someone typed. That combination is the
only route to "a UI-only user authenticates without a console".

It is added as a **provider configuration at the runner** — the key lives in the runner's
owner-only state directory next to its own credential, the runner's probe fetches the
catalog into the capability snapshot the scheduler already intersects against, and the
adapters inject the vendor-documented environment only for attempts whose resolved model
names the gateway. The Tack API still makes zero model-provider calls. The single exception
— a loopback-only, embedded-runner-only, write-once route that hands a pasted key to the
co-located runner without persisting it — is what ADR 0061 exists to bound. OpenRouter,
direct vendor keys and local endpoints stay in each harness's own configuration until the
gateway path is proven live and a second gateway measurably differs from it.

## Credentials: who runs what, where, and how a key is kept

The credential design follows from where each component runs, so that is settled first.
Tack is one binary with three roles chosen by subcommand — server (`tack serve`), client
(every other subcommand, `tack mcp` included) and runner (`tack serve --with-runner`, or
the separate `tack-runner` binary when the runner is on another machine). The roles
combine into three deployment shapes, and only the first is the target of this phase:

| Shape | Who | Board runs | Runner runs | Status |
|---|---|---|---|---|
| **One person, one machine** | a developer with several projects, their own harness logins and their own API keys | on the laptop, `tack serve --with-runner` | inside the same process, loopback only | **the normal case — what every card in this Part is built for** |
| One person, an always-on board | the same developer, wanting the board reachable when the laptop is closed | on a home server, NAS or small VPS, `tack serve` | on the laptop, `tack-runner`, pulling work over HTTPS | possible today (deployment guide), **not a target**: it needs a reachable host, TLS and a tunnel or overlay network that the normal case never has |
| Several people, one board | a team; each person's runner holds that person's keys | on a shared host | one per person, on their own machines | **deferred** — needs identity; see below |

The split is not a bet that the board will live elsewhere. What the normal case needs
from it is narrower and more important: **the work must outlive the window.** Closing
the browser tab, or the whole UI, must not stop a running attempt or lose a queued one,
and reopening it must show the current state. That is exactly the line the split draws:
the server process (board state plus the embedded runner) keeps running, and the UI is a
view that attaches to it over loopback and fetches the current state when it comes
back. The runner is also the component that holds credentials and touches code, so it
must be able to live wherever those are; in the normal case that is the same machine,
the same process. The split costs the normal case one flag (or the UI switch decision 6
of ADR 0061 adds) and is what makes the other two shapes possible without a redesign.

### Three kinds of secret, three different owners

| Secret | Example | Held by | Standard |
|---|---|---|---|
| A harness's own login | Claude Code's OAuth session, `codex login` | **the harness CLI** — Tack never reads or copies it | OAuth 2.0 device flow (RFC 8628); each CLI keeps its own session (the OS keychain on macOS, an owner-only file on Linux). Every orchestrator worth copying delegates here. |
| A provider API key injected into the harness's environment | a Vercel AI Gateway key | **the runner**, on the machine that launches the harness | OS keychain first, owner-only file second — what `gh` and `docker` do. Never the shared board database. |
| Tack's own secrets | operator token, runner enrollment credential, S3 backup key | already settled | hashed on the server; owner-only and `[REDACTED]` on the runner |

Only the second row was undecided. ADR 0061 decides where it lives; this section fixes
*how*.

### How the runner keeps a provider key

The store the runner gets (VI-B1) has two backends, chosen at runtime, in this order:

1. **The operating system's credential store** — macOS Keychain, Windows Credential
   Manager, Linux Secret Service (what GNOME Keyring and recent KWallet expose) —
   through the `keyring` crate. Encrypted at rest by the OS, bound to the logged-in
   user, invisible to other accounts on the machine, absent from every backup and sync
   folder. This is the desktop standard, and the one a developer on their own machine
   gets without configuring anything.
2. **An owner-only file** in the runner's state directory, mode `0600`, when no
   platform store answers — a headless Linux box, a container. This is the level the
   harnesses themselves use on Linux and what `gh` falls back to. The runner says which
   backend it is using — in `tack runner doctor` and in the UI's response to a pasted
   key — so nobody believes a key is in a keychain when it is in a file.

Entries are named, never positional: `<provider>/<label>`, with `default` the only label
this phase writes. A second key for the same provider (a client's gateway key next to
your own) fits the naming without a migration; letting a project pin a label is a later
card, once a second key exists.

A work request's `secret_reference` — already in the wire contract, never resolved until
now — gains one optional scheme: `store:<name>` (the default when no scheme is given, so
the frozen fixture stays valid) and `env:<VARIABLE>`, which reads the value from the
runner's own environment at spawn time. The second is the twelve-factor path for a
runner started by systemd with `Environment=` or `LoadCredential=`, and needs no store
at all. A store encrypted with a key-encryption key taken from the environment (the
n8n / Gitea pattern) is the *server* standard, and is deliberately not built until a
headless deployment that wants the UI paste route actually exists.

Rejected outright, because they look convenient and are not standard: a provider key in
`tack.db` or `app_meta`, encrypted or not (it turns the S3 backup into a container of
vendor credentials and the server into the holder ADR 0050 forbids); reading or copying
the harnesses' own auth files (no contract, formats change, and it breaks the user's
login); any home-grown encryption.

### Identity and a second person — later, with a trigger

The user model this Part serves is one person with several projects, their own harness
logins and their own keys, on their own machine. Under that model the runner already *is*
the per-person credential boundary: whoever runs the runner owns the keys it holds, and
the board never sees one. Accounts, sessions, roles and per-user tokens would add nothing
that model can use.

They become necessary the day a second person shares a board — to say who created a
request, which runners they may dispatch to, and to give the audit trail a real actor
instead of a hash of the one shared token. That is the demand ADR 0059 asked to see
before being reopened. When it appears, the shape is the ordinary one for a self-hosted
service, recorded here so it is not re-derived: a users table (Argon2id), opaque
sessions in an `httpOnly` cookie, per-user hashed API tokens, two roles (owner, member),
`runners.owner_user_id` and `execution_requests.created_by`, then OIDC. A runner holding
several people's keys (a shared CI box) is a further step — per-user envelope encryption
at the runner — and is not designed until such a runner exists. That work is a Part of
its own, with a new ADR that supersedes 0059; nothing in Part VI depends on it, and
nothing in Part VI is built in a way it would have to undo.

## The two-component story

Every page that introduces agents — the README, the book's introduction, the agent-runners
guide, the developer overview — opens with this statement or links to it. It is written once,
on the Part VI board (§VI.0), and applied verbatim by VI-A3:

> **Tack is two components, built to be one product.**
>
> **The board** is the project manager: workflows, timelines, dependencies, per-project
> vocabulary — one binary, one SQLite file, no accounts, no cloud. It is the plan, the
> policy and the record. It decides *what* runs, *when*, under *which* limits, and it keeps
> the durable history of every run: events, decisions, artifacts, and what it measurably
> cost. **It never executes code and never holds a model credential.**
>
> **The runner** is a small worker that lives where the code and the credentials already
> are — a laptop, a CI box, a machine with a GPU. It pulls work from the board, checks out
> an isolated workspace, launches the coding agent you already use — Claude Code or
> Codex — and reports back. **It holds the keys; the board never sees them.**
>
> They are separate because they scale and fail differently. **One board, many runners:**
> a board on a small VPS dispatches to runners on ten developers' machines, each with its
> own agent, model and capacity. A runner that dies mid-run cannot corrupt the board — its
> lease expires and its fencing token stops writing. A board that restarts cannot lose a
> run — the runner's journal knows what it started. **One developer runs both in one
> process with one command**, on the same contract, with the same recovery.

Two consequences follow. **The runner is named in the story, never on a default screen** —
the README and the docs explain two components because that is the product, while the UI's
default screens say *agent*, *model*, *provider*, *run* and keep "runner", "fleet",
"enroll", "heartbeat", "lease" and "harness" under *Advanced*. And **the recovery demo
is the visual proof of the third paragraph**: V-C2's recording — kill the runner mid-run,
see `needs_operator` and no duplicate, requeue, succeed — is not a competing hero asset; it
is the picture of "they fail differently", and the README gives it a named slot beside
that paragraph. The everyday picture — an item on the board, one click, a run streaming on
the item, done — is the hero, and VI-D2 records it from a release build with a real agent.

## What this phase deliberately does not do

- **Add a second provider**, or let Tack call a model API for any reason, including key
  validation — the runner's probe does that.
- **Broker a vendor OAuth login.** Those stay in the terminal and are rendered, step by
  step, in the UI.
- **Touch docket** (ADR 0060), **build identity** (ADR 0059; the trigger and the shape it will take are
  recorded above, under "Identity and a second person"), or pick up the features Phase
  59 deferred — notifications, i18n, time tracking, in-UI diff review. Listing an attempt's
  artifacts (VI-C4) is not reviewing a diff; that stays a later card.
- **Exercise the `decisions` path with a real harness.** No harness in this tree asks a
  mid-run question; VI-C4 lists decisions, it does not manufacture one.

## Exit

A person who has never seen Tack starts it with `tack serve`, opens the board, and follows
one screen: they turn agent execution on with a switch, paste a gateway key, or copy the one console command it shows
them for the harness they already use; they choose a model from a list the runner actually
measured; they press *Run with agent* on an item without typing an identifier; and they
watch the attempt complete with the model they asked for recorded as the model they got.
Every step they could not do in the UI was shown to them in the UI, with a check that it
worked. The key they pasted sits in the operating system's keychain — or, where there
is none, in one owner-only file the runner names as such — and nowhere else, and the
documentation says so in the same paragraph that says Tack never proxies a model. And a
stranger who reads the README's first screen reports two components — a board that plans
and records, runners that execute where the code lives — before they see a single Kanban
column.

# Next — Desktop app and background service (Phase 61)

**Tack runs as a background service, and the window is a view of it.** Closing the window
never stops the work; only Quit does. The normal install is a desktop application with its
own window and icon, like Docker Desktop, that starts and supervises the Tack server. The
`tack` binary stays what it is for servers and the terminal, and gets the same daemon
through `tack service install`.

The decision record is [ADR 0062](https://github.com/yielab/tack/blob/develop/docs/adr/0062-desktop-app-and-background-service.md)
— eight decisions in one table, **accepted 2026-09-03**. The board is Part VII in
`TODO.md` (top of the file) and the dispatch plan is
`docs/agent-handoffs/part-vii/README.md`; both were created from this section. **Wave 18
(VII-A2, VII-B1) was dispatched 2026-09-03.**

## Why this phase exists

Today a person starts Tack in a terminal and opens a browser tab. The board and runner
already survive a closed tab — the UI reconnects and re-fetches — but nothing survives
the terminal: closing it kills a running agent attempt. Part VI removes every console
step but one, and that one is "start Tack". This phase removes it, and makes the daemon
promise visible instead of implied.

## The shape

| Piece | What it is | Why |
|---|---|---|
| `tack-desktop` | A separate Tauri 2 program: window + tray + supervisor | The server binary must never carry a webview or GTK |
| The sidecar | The platform's `tack` binary, bundled, run as `tack serve --with-runner` | One server, tested once; the app attaches to one that is already running |
| The window | The same web UI, loaded from the local server | No second frontend, no app-only API |
| The tray | Open · runner status · launch at login · Quit (warns on in-flight attempts) | The daemon promise, stated where the user can see it |
| Data | OS per-user app folders, passed as the existing `TACK_*` variables | Apps do not write next to where they were launched |
| `tack service install` | systemd (user) / launchd unit for the terminal path | "Outlives the terminal" without the app |

## Cards — created as the Part VII board on acceptance

| Card | Scope | Needs |
|---|---|---|
| VII-A1 | ADR 0062 accepted (decision card; the user accepts) | — |
| VII-A2 | `tack service install \| uninstall \| status`: systemd user unit, launchd plist, typed unsupported on Windows; uses the OS data folders; docs | A1 |
| VII-B1 | `crates/tack-desktop` skeleton: Tauri 2, sidecar `tack`, supervisor start / attach / stop with version check, window loads the local URL, single instance; `.deb` and `.AppImage` built on the dev machine | A1 |
| VII-B2 | Tray and lifecycle: close hides, tray menu, Quit warns on in-flight attempts, launch-at-login toggle on by default | B1 |
| VII-B3 | Data folders and first run: the four `TACK_*` variables under the OS data dir; first-run screen shows the location and accepts an existing `tack.db`; runner switch state shown, never flipped by the app | B1 |
| VII-C1 | Release pipeline: bundles for Linux, macOS, Windows in the release workflow; CI prerequisites; icon set; unsigned, with the one-time warnings documented in the release notes | B2 · B3 |
| VII-C2 | README "Run it" leads with the app; install page; book; screenshots of the window and tray under Part V's asset rules | C1 · VI-C1 (the first-run screen must be the real Agents page) |
| VII-D1 | Stranger proof: install from the release artifact on a clean user account, open, run an agent on an item, close the window, reopen, see the attempt finished, Quit warns. Linux measured on this machine; macOS and Windows `not_measured` until a machine exists, stated as such | C2 |

Waves continue Part VI's numbering: **18** A1 → A2 ∥ B1 · **19** B2 ∥ B3 · **20** C1 → C2 ·
**21** D1. Cross-Part: B2 reads the runner switch VI-B3 persists; C2 waits for VI-C1;
README and `docs/screenshots/**` follow the conflict rules in `TODO.md` §V.3 and §VI.3.

## What this phase deliberately does not do

- **Mobile or remote access.** A phone on the LAN or a board on a VPS needs infrastructure
  the normal case does not have; both stay out.
- **Decide code signing.** Certificates cost money; the app ships unsigned with the
  warnings documented until that decision is made separately.
- **A second frontend, or an app-only API.** The window shows the served UI, full stop.
- **Turn the runner on by itself.** Installing the app is not consent to run agents; the
  UI switch from ADR 0061 stays the only way.
- **A Windows service.** `tack service` returns a typed unsupported there; the app is the
  Windows path.

## Exit

A person downloads the app from the release page, opens it, sees the board in its own
window with Tack's icon in the dock or taskbar and in the tray, turns agent execution on
from the Agents page, runs an agent on an item, closes the window, comes back later and
finds the attempt finished with its artifacts listed. Quit warns them if something is still
running. `tack service install` gives a terminal user the same guarantee without the app.
Every platform's result is either measured or marked `not_measured`; none is assumed.
