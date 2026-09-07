# VI-C39 handoff

- Base SHA / branch / final SHA: `9b251bd` (develop) / `agent/vi-c39-msrv-sqlx` / `fb9332d`
- Files changed (must equal ownership list): `Cargo.toml`, `Cargo.lock`,
  `crates/tack-desktop/Cargo.toml`, `.github/workflows/ci.yml`, `.github/dependabot.yml`,
  `README.md`, `CONTRIBUTING.md`, `CHANGELOG.md`, plus every `tack-db`/`tack-api`/`tack-orch`
  source and test file sqlx 0.9's new `SqlSafeStr` bound touched (listed in full below).
- Contract fixtures consumed: none — `runner_contract` and `openapi_contract` both stay green
  with zero fixture/spec edits (this is a dependency bump with no API-shape change; confirmed
  by `./scripts/regen-generated.sh` producing zero diff to `docs/openapi.json` or
  `frontend/src/shared/api/schema.gen.ts`).
- Behavior implemented: none a user can observe. The dependency floor moves and `sqlx` lands on
  0.9.0; every call site sqlx's new compile-time SQL-injection lint touched was already safe
  (see "Why every `AssertSqlSafe` here is safe" below) and no query's SQL text or bind order
  changed.
- Tests added: none — this is a floor/dependency card, not a behavior change. Proved the floor
  itself is real by building under both toolchains (see "Claim → evidence").
- Failure/adversarial case proved: the 1.89 build failure below **is** the adversarial case this
  card exists to prove — see "Claim → evidence" row 2.
- Schema/API/contract change requested from another owner: none.
- Known limitations: none new. `crates/tack-desktop`'s own advisories remain out of scope (owned
  by VI-C37, per VI-C35's handoff) — untouched here beyond its `rust-version` bump.
- Secrets/logging review: n/a — no code path touching secrets or logs was changed.
- Safe merge order and likely conflicts: `Cargo.lock` is the only file likely to conflict with
  another in-flight dependency card; regenerate once after integrating alongside any other Wave
  17 card that also touches it, per `CLAUDE.md`'s "regenerate once at the end" rule. No other
  file here is owned by another Wave 17 card.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## The decision this card executes

`sqlx` 0.9.0 (stable, 2026-05-21) declares `rust-version = 1.94.0`. The floor here was 1.89
(`docs/agent-handoffs/deps/cargo-major-2026-09.md` recorded the same finding and deliberately
left `sqlx` at 0.8, calling the floor rise "the user's call"). The user made that call today: the
floor rises to 1.94 — the lowest Rust the newest **stable** `sqlx` line needs, not the newest
Rust available — and `sqlx` 0.9 lands in the same change.

## `sqlx` 0.9.0 changelog: what applies here, what does not

Read via the raw changelog (`raw.githubusercontent.com/launchbadge/sqlx/main/CHANGELOG.md`,
since GitHub's rendered blob view truncates to navigation chrome for a WebFetch). Full summary
is in the branch's tool output; the load-bearing parts for this repo:

- **`SqlSafeStr` / `AssertSqlSafe`** — every `query`/`query_as`/`query_scalar` call now requires
  `impl SqlSafeStr`, satisfied only by `&'static str` or the explicit `AssertSqlSafe(..)`
  wrapper around a `String`/`&str`. This is the change that broke the build (38 call sites, all
  below).
- **`Migrate` trait significantly changed, `MigrateError` restructured, `Migration` struct
  gained a no-transaction field.** **Does not apply**: `grep -rn "sqlx::migrate\|sqlx::Migrator\|MigrateError"
  crates/*/src` returns nothing. `crates/tack-db/src/migrations.rs` is a fully hand-rolled
  runner (`SqlitePool` + raw `sqlx::query`/transactions) — it has never used sqlx's `Migrate`
  trait, `sqlx::migrate!`, or `sqlx-cli`. The **stop clause** in this card ("say what and stop"
  if 0.9 needs a migration-runner change) does not fire.
- **SQLite-specific changes** (extension loading now `unsafe`, new opt-in
  `sqlite-deserialize`/`sqlite-load-extension`/`sqlite-unlock-notify` features, stricter
  `SqliteValue`/`SqliteValueRef` `Send`/`Sync` bounds, `libsqlite3-sys` version-range widening).
  **Does not apply**: this repo never loads a SQLite extension, and WAL mode is set by an
  explicit `PRAGMA journal_mode=WAL` in `crates/tack-db/src/lib.rs`, not through any sqlx
  journal-mode config the 0.9 changelog touches. No journal/WAL behavior changed.
- **`fetch_optional()` return type**, **`Arguments`/`SqliteArguments`/`AnyArguments` lifetime
  parameters removed**, **`Encode` now returns `Result`**: none of these needed a source change
  — nothing in this tree implements `Encode`/`Decode`/`Arguments` by hand, and every
  `fetch_optional()` call site already matched on `Option<Row>`.
- **Postgres/MySQL-specific breaking changes**: not applicable — this repo's only sqlx feature
  is `sqlite` (see `Cargo.toml`'s workspace dependency), Postgres and MySQL drivers are not
  compiled in.

## Why every `AssertSqlSafe` here is safe

sqlx 0.9's lint exists to catch string-interpolated *values* landing in SQL text (real
injection). Every site this bump touched builds its dynamic SQL from one of three closed,
non-attacker-controlled shapes, never from a bound value re-inserted as text:

1. **A compile-time `const`/local column-list string** (`CONTROL_PLANE_COLUMNS`,
   `ORCH_TASK_COLUMNS`, `ORCH_RUN_COLUMNS`, `ORCH_EVENT_COLUMNS`, `ORCH_APPROVAL_COLUMNS`,
   `ORCH_LINK_COLUMNS` in `orch.rs`; a local `const COLUMNS` in the migration test) —
   interpolated into the SQL text, but the string is fixed in the binary, never derived from a
   request.
2. **A `?`-placeholder count** built from `.map(|_| "?").collect().join(",")` or a manual
   `push('?')` loop, sized to `Vec<T>::len()` — the text itself carries no data, every real
   value is still bound separately with `.bind(..)`.
3. **A server-generated temp-file path** (`backup.rs`, `remote_backup.rs`) — `std::env::temp_dir().join(format!("tack-{snap,backup}-{}.db", Uuid::new_v4()))`,
   with the one meta-character SQLite's `VACUUM INTO '...'` string literal cares about
   (`'`) already escaped (`.replace('\'', "''")`) before this bump, unchanged by it.

`crates/tack-db/src/repo/items.rs::item_filter_clause` is the one site that looked closest to
real user input (`ItemFilter`'s `status`/`priority`/`assignee` etc.) — confirmed every one of
those fields is pushed through `binds: Vec<String>` and bound with `.bind(..)`, never spliced
into `where_clause`'s text; only the `?` placeholders and static ` AND col = ?` fragments are
in the string. No behavior or safety property changed at any of the 38 sites — this is
satisfying a new compile-time proof obligation for code that was already correct, not fixing a
bug.

## Every sqlx API this bump touched, and its replacement

| Old (0.8) | New (0.9) | Where |
|---|---|---|
| `sqlx::query(statement)` where `statement: &&str` from `for s in &[&str]` | needs `*statement` (deref to `&'static str`) *and* the enclosing fn signature widened from `statements: &[&str]` to `statements: &'static [&'static str]` — the elided lifetime on the old signature does not preserve `'static` through a reference, even though every real caller only ever passes `'static` data | `migrations.rs::apply_ordinary_migration`, `apply_rebuild_migration` |
| `sqlx::query_scalar(&format!(..))` / `sqlx::query(&format!(..))` / `sqlx::query_as(&format!(..))` | `sqlx::query_scalar(AssertSqlSafe(format!(..)))` / `sqlx::query(AssertSqlSafe(sql))` / `sqlx::query_as(AssertSqlSafe(format!(..)))` — `AssertSqlSafe<String>` and `AssertSqlSafe<&str>` both implement `SqlSafeStr`; `&format!(..)` (a `&String`) implemented nothing | all 38 sites below |
| `fn fetch_table(pool: &SqlitePool, sql: &str)` | `sql: &'static str` — same elided-lifetime problem as the migration runner, for a test helper always called with string literals | `tack-orch/tests/docket_tick_contract_test.rs::fetch_table` |

Full list of the 38 call sites `AssertSqlSafe`/lifetime-widening was applied to:

- `crates/tack-db/src/migrations.rs` — 384, 417 (`*statement` deref + signature widen), 463,
  466, 483 (`AssertSqlSafe(format!(..))`)
- `crates/tack-db/src/repo/dependencies.rs` — 134
- `crates/tack-db/src/repo/execution.rs` — 2520, 3146, 3206, 3358, 3371, 3448
- `crates/tack-db/src/repo/items.rs` — 264, 284
- `crates/tack-db/src/repo/orch.rs` — 156, 168, 404, 421, 590, 606, 626, 849, 864, 993, 1134,
  1154, 1170, 1582, 1724
- `crates/tack-api/src/debug.rs` — 98
- `crates/tack-api/src/handlers/backup.rs` — 42
- `crates/tack-api/src/remote_backup.rs` — 379
- `crates/tack-db/tests/migrations/orch_migrations.rs` — 72, 754, 832
- `crates/tack-db/tests/repository/execution_retention.rs` — 262
- `crates/tack-api/tests/handlers/executions_runner_admin.rs` — 674 (the doc comment on this
  helper already stated why hand-interpolation is deliberate and safe here, predating this
  bump — only the wrapper is new)
- `crates/tack-orch/tests/docket_tick_contract_test.rs` — 645 (signature widen to `&'static
  str`, not `AssertSqlSafe`)

`cargo build --workspace` alone surfaced the first 32 (29 in `tack-db`'s lib, 3 in `tack-api`'s
lib); `cargo clippy --workspace --all-targets` (which also builds every test binary) surfaced
the remaining 6, all in test crates the plain library build never compiles.

## Claim → evidence

| Claim | Evidence — command, output |
|---|---|
| The 1.94 toolchain installs to an exact patch, used verbatim in CI's pin | `rustup toolchain install 1.94 --profile minimal` → `1.94-x86_64-unknown-linux-gnu installed - rustc 1.94.1 (e408947bf 2026-03-25)` |
| The floor is real, not nominal: 1.94 builds, 1.89 fails | `RUSTUP_TOOLCHAIN=1.94.1 cargo build --workspace --locked -j 4` → `Finished` (dev profile) in 1m 15s. `RUSTUP_TOOLCHAIN=1.89.0 cargo build --workspace --locked -j 4` → `error: rustc 1.89.0 is not supported by the following packages: sqlx@0.9.0 requires rustc 1.94.0` (and five more lines: `sqlx-core`, `sqlx-macros`, `sqlx-macros-core`, `sqlx-sqlite`×2, `tack-runner`×2, all naming 1.94) |
| `sqlx` resolved to exactly 0.9.0, not a later patch | `grep -A2 'name = "sqlx"' Cargo.lock` → `version = "0.9.0"` |
| No migration-runner or SQLite journal/WAL behavior changed (stop-clause check) | `grep -rn "sqlx::migrate\|sqlx::Migrator\|MigrateError" crates/*/src` → no matches; `grep -n "journal_mode" crates/tack-db/src/lib.rs` → the one explicit `PRAGMA journal_mode=WAL`, unchanged by this diff |
| The full suite is unaffected under the pinned 1.98.1 toolchain | `cargo nextest run --workspace --build-jobs 4 --test-threads 4` → `1463 tests run: 1463 passed, 7 skipped` |
| Clippy is clean workspace-wide, including every test target | `cargo clippy --workspace --all-targets -- -D warnings` → `Finished` dev profile, zero warnings |
| No API response shape changed | `./scripts/regen-generated.sh` → clean; `git status` on `docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts` both unchanged before and after |
| `crates/tack-desktop`'s own workspace still resolves after its `rust-version` bump | `cd crates/tack-desktop && cargo metadata --locked -q >/dev/null` → exit 0, no output; its own `Cargo.lock` untouched (`git status --porcelain -- crates/tack-desktop/` shows only `Cargo.toml` modified) |
| No `1.89` reference remains outside the three files the card excepts | `grep -rln "1\.89" --exclude-dir=target --exclude-dir=node_modules --exclude-dir=.git .` → `CHANGELOG.md`, `TODO.md`, four files under `docs/agent-handoffs/`, and `frontend/public/icons.svg` (a false-positive decimal in an SVG path's coordinate data, not a version string) |
| `.githooks/pre-push` is green on the final tree | run directly from the worktree root → `✓ pre-push checks passed` (comment gate, test-hygiene gate, `cargo fmt --all --check` for both workspaces, clippy, generated-files freshness all passed); re-ran automatically by `git push` itself with the same result |

## Measured numbers

- `rustc 1.94.1` (`e408947bf`, 2026-03-25) — the exact patch `rustup toolchain install 1.94
  --profile minimal` resolved, pinned verbatim in CI (`dtolnay/rust-toolchain@1.94.1`,
  `RUSTUP_TOOLCHAIN: 1.94.1`).
- `cargo build --workspace --locked` under `RUSTUP_TOOLCHAIN=1.94.1`: clean, 1m 15s (cold, this
  worktree's target dir).
- `cargo build --workspace --locked` under `RUSTUP_TOOLCHAIN=1.89.0`: fails at the dependency
  resolution / MSRV-check stage before any compilation, citing `sqlx`, `sqlx-core`,
  `sqlx-macros`, `sqlx-macros-core`, `sqlx-sqlite` (×2 features) and `tack-runner` (×2), each
  naming `requires rustc 1.94` / `requires rustc 1.94.0`.
- `cargo nextest run --workspace --build-jobs 4 --test-threads 4`: 1463 passed, 7 skipped, 0
  failed (identical numbers to the last recorded baseline in VI-C35's handoff — this bump added
  no tests and broke none).
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 warnings.
- `Cargo.lock` diff: 69 insertions, 358 deletions (`git diff --stat HEAD~1 HEAD -- Cargo.lock`)
  — sqlx's own dependency tree shrank (0.9 dropped several crypto crates — `rsa`, `pkcs1`,
  `pkcs8`, `der`, `spki`, `signature`, `hkdf`/`hmac` v0.12, `rand` 0.8, several `windows-*`
  crates — from its default feature set) while a handful of others bumped in place
  (`hashbrown`, `hashlink`, `flume`, `whoami`, `etcetera`).
- 38 call sites changed across 12 files to satisfy `SqlSafeStr`.

## What a stranger still cannot do

Nothing changes for anyone using the `tack` binary — this is a floor-and-dependency card with
no observable behavior change (see "Behavior implemented" above). What changes is for a
*contributor*: building Tack from source now needs Rust 1.94+, not 1.89+ (`README.md`,
`CONTRIBUTING.md`), and a contributor who runs `sqlx` directly in new code will hit the
`SqlSafeStr` bound on any dynamic query they write — the pattern to reach for is
`AssertSqlSafe(..)` around a string built from *no* attacker-controlled substring, exactly as
documented above, never around a value that should have been a bind parameter instead.

## Escalation / out-of-scope notes

None. This card's scope (floor + `sqlx` bump + every reference move) was fully bounded by its
own Acceptance list; no adjacent defect or second card surfaced while working it.

## Proposed board row

Suggested text for `TODO.md`'s VI-C39 entry (not applied — `TODO.md` is not edited by this
card, per instructions):

> **VI-C39 integrated `<date>`** — the dependency floor moved to Rust 1.94 and `sqlx` landed on
> 0.9.0. The only break was `sqlx` 0.9's new compile-time `SqlSafeStr` proof obligation: 38 call
> sites across `tack-db`/`tack-api`/`tack-orch` build dynamic SQL from a fixed column-list
> constant or a `?`-placeholder count (never from a bound value re-inserted as text) and now
> carry `AssertSqlSafe(..)`; two helpers that iterated `&[&str]` needed their signature widened
> to `&'static [&'static str]` since the elided lifetime did not preserve `'static` through a
> reference. Neither of 0.9's `Migrate`-trait nor SQLite-driver breaking changes applied — this
> repo's migration runner has never used sqlx's `Migrate` trait, and WAL mode is set by an
> explicit `PRAGMA`, not sqlx's own journal-mode config. `cargo build --workspace --locked`
> proved green under `RUSTUP_TOOLCHAIN=1.94.1` and red under `1.89.0` (naming `sqlx` and its
> subcrates as the reason), so the new floor is real, not nominal. 1463/1463 tests, clippy
> clean, `pre-push` green, no observable API change.

## Context spent

- Read before the first edit: this card's `TODO.md` slice, `CLAUDE.md` (root + repo), the MSRV
  job in `ci.yml`, root `Cargo.toml`, `crates/tack-desktop/Cargo.toml`'s header, `README.md`'s
  badge/prose lines, `CONTRIBUTING.md`'s requirements table, `.github/dependabot.yml`'s `sqlx`
  ignore block, `docs/agent-handoffs/deps/cargo-major-2026-09.md` (the prior bump that
  deliberately left `sqlx` at 0.8 and recorded why), `.claude/scope-discipline.md`, and sqlx's
  0.9.0 changelog (fetched raw, since the rendered GitHub blob view returns only navigation
  chrome to a fetch tool).
- Files opened and not used for an edit: none of significance — every file read above either
  needed an edit or was read specifically to confirm the stop-clause (`Migrate` trait, WAL
  pragma) did not apply.
- Read-list lines that were wrong: none — the prior handoff's finding (`sqlx` 0.9.0 needs
  `rust-version = 1.94.0`) reproduced exactly.

## CI run

`gh workflow run ci.yml --ref agent/vi-c39-msrv-sqlx` →
[run 34124195564](https://github.com/yielab/tack/actions/runs/34124195564), all 10 jobs green:

| Job | Result | Duration |
|---|---|---|
| Rust (fmt + clippy + test) | ✓ | 4m34s |
| Frontend (typecheck + build) | ✓ | 34s |
| Coverage (llvm-cov + Vitest thresholds) | ✓ | 6m51s |
| Security (dependency audit) | ✓ | 36s |
| E2E (Playwright, cross-browser) | ✓ | 7m57s |
| Docs (mdBook build) | ✓ | 1m28s |
| Desktop app (tack-desktop workspace) | ✓ | 3m35s |
| **MSRV (Rust 1.94 — dependency floor)** | ✓ | 2m28s |
| cargo-deny (dependency policy) | ✓ | 39s |
| Embed SPA (single-binary packaging) | ✓ | 10m19s |

The MSRV job's "Run dtolnay/rust-toolchain@1.94.1" and "Build on MSRV" steps both passed —
CI now measures the new floor, not the old one.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*
