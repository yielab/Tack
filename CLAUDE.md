# CLAUDE.md

Guidance for Claude Code in this repository. This file is a **map, not a manual** —
deep reference lives in the docs listed at the bottom and is read on demand.

## Project Overview

**Tack** is two components built to be one product — **the board** (project manager: Rust
backend via Axum + SQLite, SolidJS frontend) and **the runner** (a small worker that executes
near your code and credentials) — shipped as a single `tack` binary where `tack serve
--with-runner` embeds a runner in the board's own process. Multiple workflows
(Scrum/Kanban/phase) with per-project vocabulary; 10 project-type presets; MCP server
(`tack mcp`). Core is complete (backend, frontend, CLI).

**No board has an open card.** Parts IV through VII (Phases 58–61) are all carded out: the
embedded runner, adoption and distribution, agent onboarding and provider choice, and the
desktop app with its background service. What is left is not card work — tagging a release
and the publish list in `docs/LAUNCH-CHECKLIST.md`. **These status lines decay in days —
`TODO.md`'s "Which board is live" table is the authority, not this paragraph.** A new cycle
branches from `develop`; `README.md` and `docs/screenshots/**` are shared files, and the
conflict rules for them are in `TODO.md` §VI.3 and §V.3. The boards are the authority for what
shipped; `docs/book/src/roadmap.md` records only intent.

**Never read `TODO.md` whole — it costs ~199k tokens.** Active boards sit in its first ~2400
lines (Part VII, then VI, then V, then IV — extract one, never all); the archive (Parts I–III) sits below them.
It was kept in-file because hundreds of doc comments cited its section numbers; **no Rust file cites it any
more**, so extracting it is now an open option rather than a blocked one — see `TODO.md`'s own header. Costs and extraction recipes for every big file:
**`.claude/context-budget.md`**. Before designing anything, read
**`.claude/scope-discipline.md`** — this tree's recurring defect is well-built mechanisms
with no caller (`model_profiles`, the `decisions` path, the superseded docket control plane).

**Skills:** `/card` (work a board card), `/feature` (feature work off the board),
`/gate` (scoped verification), `/tokens` (usage measurement vs `.claude/token-baseline.md`), `/status` (where am I / what is next), `/integrate` (merge finished card branches and review them as one change). Prefer them over improvising the workflow.

## Local Domain

Served at **https://tack.test** via the workspace-central Caddy + dnsmasq setup.
`Caddyfile.local` here is auto-imported by `~/Sites/Caddyfile`. No global `{}` block, no
separate Caddy instance, no custom ports; reload with `sudo systemctl reload caddy`.
See `~/Sites/LOCAL-DOMAINS.md`.

## Commands

```bash
cargo build                                # whole workspace compiles
cargo run -p tack-cli -- serve             # server + web UI at http://127.0.0.1:3210
cargo run -p tack-runner -- --help         # the runner is a SEPARATE binary from `tack`

cargo nextest run --workspace              # the test runner (.config/nextest.toml): ~15 s, prints failures + one summary line
cargo nextest run --workspace -E 'binary(wave2_gate)'        # select with a filterset — never `-p`: it builds a second copy of every crate
cargo nextest run --workspace -E 'binary(runner_contract)'   # byte-pins every runner-v1 fixture
cargo nextest run --workspace -E 'binary(openapi_contract)'  # spec drift gate

cd frontend && npm run dev                 # http://localhost:5173, proxies /api (start API first)
cd frontend && npm run type-check && npx vitest run
make e2e                                   # Playwright (make e2e-install once)
make audit                                 # cargo audit + npm audit

.githooks/pre-push                         # THE definition of done — run it before saying a change is finished
```

**`.githooks/pre-push` is the gate that decides whether work can leave the machine**, and
running it directly is the only check that cannot drift from it. It covers more than the
test suite: `scripts/check-comments.sh`, `scripts/check-test-hygiene.sh`, **`cargo fmt --all
--check` for the workspace *and* for `crates/tack-desktop` separately**, `cargo clippy
--workspace --all-targets -- -D warnings`, and a freshness check on the lockfiles and
`schema.gen.ts`. A green test suite is not a finished change. Formatting in particular is
invisible until a push is attempted, so unformatted work accumulates silently across merges
and then blocks the whole branch at once — which is exactly how it has failed here before.

Live-harness runner tests are `#[ignore]` (Claude Code's is billed — run deliberately with
`--run-ignored ignored-only`). Never `cargo test --workspace`: it prints ~84k tokens for a
green run where nextest prints ~150.
Release profile optimizes for size (`lto`, `opt-level="z"`) and compiles slowly — use
`--release` sparingly. Full testing guide: `docs/TESTING.md`.

## Configuration

All `TACK_*` variables (server, runner, backup, orchestration, execution domain) are
documented in **`docs/CONFIG.md`** — the single authority for those tables. Posture
rules that bite:

- Anything that **deletes data or reaches the network** is off-by-default behind a
  `TACK_*_ENABLE` gate (`TACK_ORCH_ENABLE`, `TACK_EXECUTION_RETENTION_ENABLE`);
  read/log-only watchers may default on.
- Privileged actions carry their **own token**, distinct from `TACK_API_TOKEN`,
  fail-closed when unset (`TACK_ORCH_APPROVAL_TOKEN`, `TACK_EXECUTION_DECISION_TOKEN`).
- Secrets are write-only over the API, never logged, and every new secret column is
  added to `remote_backup.rs::scrub_snapshot_secrets` in the same commit.

## Architecture (map)

```
crates/
├── tack-core/     Pure business logic, zero I/O (models, workflow, vocabulary, DAG)
├── tack-db/       SQLite via sqlx; 62 migrations; FTS5; repository pattern in repo/
├── tack-orch/     ControlPlane trait + reconciler + neutral runner-v1 execution domain.
│                  Depends on core+db only — must NEVER depend on tack-api
├── tack-api/      Axum server (library; tack_api::serve). 97 documented paths + WebSocket
├── tack-runner/   Pull-based execution runner — separate binary; owns credentials,
│                  workspace, journal, and the harness subprocess (codex/claude_code)
└── tack-cli/      The single `tack` binary: serve + CLI client (HTTP only, never opens the DB)

frontend/          SolidJS + Tailwind v4; two-axis design tokens (mode × palette);
                   types generated from the OpenAPI spec

crates/tack-desktop/   Tauri shell that supervises `tack` as a bundled sidecar. NOT a
                   member of the workspace above — its own, via `exclude` in the root
                   Cargo.toml, because Tauri drags GTK/WebKit/glib into whatever workspace
                   holds it and the server must keep building where none of that exists.
                   `make desktop` builds it; `cargo --workspace` never sees it. Its own
                   Cargo.lock, CI job and Dependabot entry.
```

Boundary rules: **two auth surfaces, structurally separated** — operator routes under
`/api` behind `require_token`; runner routes (`/api/runner/v1`) are a sibling of `/api`
with per-handler hashed-credential auth; never route one through the other. Every
attempt-scoped mutation validates runner id + attempt id + fencing token; a stale fence
returns `stale_lease` and writes nothing (16 call sites in
`handlers/runner_protocol.rs`). **Known inconsistency (III-G2 audit, 2026-08-19,
finding F1, non-blocking):** when an attempt was superseded by a pre-spawn *recovery*,
the retried old fence returns `409 conflict` instead of `stale_lease` on heartbeat,
decisions, artifacts and the observation routes — an earlier guard fires first. Nothing
is written either way; the code is inconsistent with itself, not unsafe. III-G5 routes it. Full crate detail, design patterns, and implementation
notes (workflow validation, auto-status propagation, WebSocket events, attachments…):
**`docs/ARCHITECTURE.md`**.

## Rules that bite (learned here, enforced everywhere)

- **`BEGIN IMMEDIATE` is mandatory for read-then-write transactions** — deferred ones
  deadlock under concurrency; prove concurrency tests against a file-backed DB, not the
  shared in-memory harness.
- **One `ALTER` per migration name.** The migration runner executes statements
  individually with no wrapping transaction; a multi-`ALTER` migration failing midway
  bricks the install.
- **`docs/contracts/runner-v1/` fixtures outrank any Rust/TS type.** Fixture edits update
  the pin table in `crates/tack-orch/tests/runner_contract.rs` in the same change.
- **`docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts` are generated** —
  never hand-edit; regenerate via `UPDATE_OPENAPI=1 cargo nextest run --workspace -E
  'binary(openapi_contract)'` then `cd frontend && npm run gen:api`, or `./scripts/regen-generated.sh`
  for those plus the lockfiles. Never hand-**merge** them either: `.gitattributes` routes
  them through the `tack-generated` driver (`./scripts/setup-git.sh` registers it) and
  `post-merge` regenerates. After merging several branches, regenerate once at the end
  rather than trusting any branch's copy.
- **Unsupported is typed, unknown is explicit, unmeasured is nullable** — no
  `unimplemented!()`, no zero standing in for "unknown" (never render `$0.00` for
  unmeasured money; the literal is `Not measured`). Capability claims are load-bearing.
- **Logs carry ids only** — never credentials, prompt bodies, query strings or env
  values; tests assert the redaction.
- **A status-code assertion alone proves little.** For "writes nothing / rejects before
  X" claims, assert the absence directly (row counts, untouched checkpoint) and prove
  the test load-bearing by reverting the fix once.
- **A load-bearing number carries the command that produces it, and gets re-measured before
  it is quoted.** A claim like "234 doc comments cite this" decides things — it justified
  freezing 9k lines in place, and it was 13× wrong within two weeks of being written while
  four documents kept repeating it. If you are about to rely on a count someone else wrote,
  run its command first; if it has no command, that is the finding.
- **A test's temporary paths come from a `tempfile` guard, never from
  `env::temp_dir().join(...)`.** A hand-built path is removed by a statement at the end of
  the test, which a failing assertion skips, and the removal has to name each file, so
  whatever the code under test writes *beside* it — SQLite's `-wal`/`-shm`, the migration
  runner's pre-upgrade snapshot — is never named. Hold the guard as long as anything reads
  the path: a helper returning only the path deletes the directory as it returns, and the
  compiler will not tell you. `scripts/check-test-hygiene.sh` enforces this (~0.7s, in
  `pre-push` and CI); production code may still use the temp directory and is not scanned.
- **Each board card writes one handoff** in `docs/agent-handoffs/`; corrections are
  appended as amendments, never rewritten.
- Changing an API response shape updates the matching frontend unit/E2E mocks in the
  same change. Frontend colors come from `--color-*` tokens only, never raw hex.

## Code style

`tracing` macros for logging; `#[instrument(skip(pool))]` on async fns; `thiserror` in
core/db, `anyhow` in CLI; `chrono::DateTime<Utc>`; UUIDv4 stored as TEXT;
`assert_matches!` in tests.

**Comments explain the code, never the project's history.** Write what the code does when
the name doesn't say it, why a non-obvious choice was made, what breaks if you change it,
and what isn't true yet (an unwired column, a mechanism with no caller). Never write card,
wave, phase or `TODO.md §` references, narratives of how the code got here, instructions
aimed at a finished cycle, dates, attributions, or commented-out code — `git log` and the
handoffs already hold all of that, and a reader with the code but not the board can't use
it. Full rule, with the examples it was derived from: `.claude/scope-discipline.md`.
**`scripts/check-comments.sh` enforces this** (~0.2s, runs in `pre-push` and CI). It exists
because this rule decayed silently once already: card citations returned to doc comments and
reached operator log lines and API error responses before anyone noticed. When it fires,
rewriting is almost always right and deleting is almost always wrong — the comment usually
wraps something real in board scaffolding. Keep the knowledge, drop the pointer.

## Where everything else lives

| Topic | Doc |
|---|---|
| All configuration tables + debugging | `docs/CONFIG.md` |
| Crate detail, patterns, implementation notes, troubleshooting | `docs/ARCHITECTURE.md` |
| Endpoint reference / examples | `docs/API-REFERENCE.md` |
| Testing guide (unit → E2E → load) | `docs/TESTING.md` |
| Active board, wave rules, card ownership | `TODO.md` (extract, don't read whole) |
| Per-card decision history | `docs/agent-handoffs/` |
| Wire contract of record | `docs/contracts/runner-v1/` |
| MCP, GitHub sync, deployment | `docs/MCP.md`, `docs/GITHUB-SYNC.md`, `docs/DEPLOYMENT-GUIDE.md` |
| User/developer book (mdBook) | `docs/book/src/` |
