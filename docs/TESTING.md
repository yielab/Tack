# Tack Testing Guide

Every Rust test runs with one command and needs no external service:

```bash
cargo nextest run --workspace
```

The summary line says how many ran — `N tests run: N passed, M skipped`. The skipped ones
are `#[ignore]`d on purpose: the perf test and the live-harness runner tests, which bill a
real agent account. `cargo nextest` is not part of cargo; install it once with
`cargo install cargo-nextest --locked`, or take the prebuilt from <https://get.nexte.st>.
`tack-desktop` is a workspace of its own:
`cargo nextest run --manifest-path crates/tack-desktop/Cargo.toml`.

Frontend: `cd frontend && npm test` (Vitest). Browser E2E: `make e2e`.

A count appears in this guide only next to the command that produces it. A number that
drops between two runs on the same branch is a finding — a suite silently stopped
running — even when everything passes.

## Quick start

```bash
cargo nextest run --workspace                                   # everything — ~15 s to execute on a warm build
cargo nextest run --workspace -E 'package(tack-db)'             # one crate
cargo nextest run --workspace -E 'binary(wave2_gate)'           # one test binary
cargo nextest run --workspace -E 'test(/fencing/)'              # tests whose name matches a regex
cargo nextest run --workspace --no-capture -E 'test(<name>)'    # see println!/tracing output (runs serially)
cargo nextest run --workspace --run-ignored ignored-only -E 'package(tack-db)'   # the perf test (50k items, p95 < 100 ms)
cargo nextest list --workspace                                  # what would run, without running it
```

Filtersets: `cargo nextest run --help` and <https://nexte.st/docs/filtersets/>.

## Two rules, and the measurements behind them

**Always `--workspace`; select with `-E`, never with `-p`.** `cargo test -p tack-api` and
`cargo test --workspace` resolve dependency features differently, so `target/` keeps two
copies of every tack crate and each source change is compiled once per form you use.
Measured 2026-09-05 on a warm cache: switching from `--workspace` to `-p tack-api` with no
source change recompiled `tack-core`, `tack-db`, `tack-orch` and `tack-api` — 13 s. The `-E`
filter selects what *runs*; the build is the workspace's either way, and that is the point.
Reproduce: `cargo test --workspace --no-run && cargo test -p tack-api --no-run` and count the
`Compiling` lines of the second.

**Never read a green run's output.** `.config/nextest.toml` prints failures and one summary
line — a green run is ~8 lines. `cargo test --workspace` prints one line per passing test:
~2,700 lines, ~84k tokens for a reader that is a language model, to learn the word "ok".
Reproduce: `cargo test --workspace 2>&1 | wc -c` against `cargo nextest run --workspace 2>&1 | wc -c`.

`cargo test` still works — nothing forbids it — but it is not what CI, `make test` or the
`/gate` skill call, and nothing in this repository should tell anyone to run it.

**A test's temporary paths come from a guard.** Ask `tempfile` for the directory and hold
the guard; never build a path with `env::temp_dir().join(...)`:

```rust
let dir = tempfile::tempdir().expect("temporary directory");
let db_path = dir.path().join("subject.db");   // -wal, -shm and the migration
                                               // runner's snapshot land beside it
```

Both failure modes of the hand-built form are gone: the guard removes the directory when it
drops, so a panicking test cleans up too, and it removes *everything* inside, so nothing has
to be named — the reason the old form leaked was that no test knew the migration runner
writes a `.before-037_orch_runs_rebuild.sqlite` next to any file-backed database. Measured
2026-09-05: one green run left **83** entries in `/tmp` before, **0** after; 4,499 had
accumulated. Reproduce: `touch /tmp/mark && cargo nextest run --workspace && find /tmp
-maxdepth 1 -newer /tmp/mark | wc -l`.

The one thing the compiler will not catch: a helper that builds the directory and returns
only a path deletes it as it returns. Return the guard alongside — `(Repository, TempDir)` —
or take the directory as a parameter. Production code may use the temp directory and is not
scanned; `scripts/check-test-hygiene.sh` (~0.7 s, in `pre-push` and CI) covers everything
under `crates/*/tests/` and every `#[cfg(test)] mod tests`.

## Where the tests live, and how each crate is tested

| Crate | Where | Harness | What belongs here |
|---|---|---|---|
| `tack-core` | `#[cfg(test)]` next to the code | plain `#[test]`; the crate has no I/O | business rules — a rule that can be tested without a database is tested here, not above |
| `tack-db` | `crates/tack-db/tests/` | `common::setup_test_db()`: a fresh `sqlite::memory:` pool with every migration applied, per test | repository round-trips, migrations, FTS, cascades. **Locking claims need a file-backed DB** — the in-memory harness masks races |
| `tack-orch` | `#[cfg(test)]` and `crates/tack-orch/tests/` | `runner_contract` byte-pins `docs/contracts/runner-v1/`; the `docket_*_contract_test` pair regenerates golden files | control-plane logic, reconciler, the neutral execution domain. A fixture edit updates `tests/runner_contract/fixtures.rs` in the same change |
| `tack-api` | `crates/tack-api/tests/` | `common::test_app()`, `test_app_with_config()`, `test_app_with_file_db()`: a wired router over an in-memory DB, driven with `tower::ServiceExt::oneshot` — no port | status codes, response shapes, auth surfaces, wiring that proves a handler is reachable |
| `tack-runner` | mostly `#[cfg(test)]`; `crates/tack-runner/tests/` | fake harness binaries under `src/harness/fixtures/`; the crash matrix | credential handling, journal, subprocess boundary. Live-harness tests are `#[ignore]` and billed |
| `tack-cli` | `crates/tack-cli/tests/` | `wiremock` stubs the API; the scheduler E2E spawns a real `tack serve` on a bind-then-drop port, so `.config/nextest.toml` runs each of its tests with nothing alongside | request shaping, error surfacing, the end-to-end scheduler path |

Each `tests/*.rs` file is its own binary — its own crate, its own full link, seconds of CPU
and tens of megabytes on disk per file. **Add a test to the existing file whose subject
fits; do not add a file per feature.** (ADR 0064 groups the existing files by subject.)

Conventions that hold everywhere: `assert_matches!` for enum variants; `#[tokio::test]` for
async; a test of "writes nothing" or "rejects before X" asserts the absence directly (row
counts, an untouched checkpoint) and proves itself load-bearing by reverting the fix once; a
wait is a bounded poll on a condition, never a fixed `sleep`; a flaky test is recorded, never
retried into green (`retries = 0` in the nextest config).

Example — a handler test with the shared harness:

```rust
#[tokio::test]
async fn health_returns_ok() {
    let (app, _workspace) = common::test_app().await;
    let res = app
        .oneshot(Request::builder().uri("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
```

## Contract and regeneration gates

Three tests guard artifacts that are committed rather than computed. They run inside the
full suite; the first two also *rewrite* the artifact when asked, and CI fails when that
rewrite differs from what is committed:

```bash
UPDATE_OPENAPI=1 cargo nextest run --workspace -E 'binary(openapi_contract)' && git diff --exit-code docs/openapi.json
UPDATE_GOLDEN=1 cargo nextest run --workspace -E 'binary(docket_tick_contract_test) | binary(docket_wire_contract_test)' && git diff --exit-code crates/tack-orch/tests/golden/
cargo nextest run --workspace -E 'binary(runner_contract)'   # never regenerated: the fixtures are the authority
```

`docs/openapi.json` and `frontend/src/shared/api/schema.gen.ts` are generated;
`./scripts/regen-generated.sh` does both plus the lockfiles. Never hand-edit or hand-merge
them.

### With the embedded SPA

```bash
cd frontend && npm run build && cd ..
cargo nextest run -p tack-api --features embed-spa    # -p on purpose: a feature build is its own resolution anyway
```

## Continuous integration

`.github/workflows/ci.yml` runs on every push to `main`, `develop` and `claude/**`, on every
pull request, and by hand (`workflow_dispatch`).

| Job | What it runs | When |
|---|---|---|
| `rust` | `scripts/check-comments.sh` → `scripts/check-test-hygiene.sh` → `cargo fmt --check` → `cargo clippy --workspace --all-targets -- -D warnings` → **`cargo nextest run --workspace --profile ci`** (one run; JUnit uploaded as `junit-rust`) → the OpenAPI and golden regenerate-and-diff gates | every push and PR |
| `frontend` | schema drift, type-check, token lint, build, entry-bundle budget | every push and PR |
| `docs` | `mdbook build` + link check | every push and PR |
| `msrv` | `cargo build --workspace --locked` on the pinned dependency floor | every push and PR |
| `desktop` | fmt, clippy, `cargo test` in the `tack-desktop` workspace | every push and PR |
| `deny`, `security` | licenses and duplicate versions; `cargo audit` + `npm audit` | every push and PR |
| `e2e` | Playwright in three browsers, a11y scan, API contract | every push and PR |
| `coverage` | `cargo llvm-cov` floors per crate + Vitest thresholds | **pull requests, `main`, manual** — five instrumented builds that share nothing with the normal one |
| `embed-spa` | release build with the SPA embedded, binary-size budget | **pull requests, `main`, manual** — the size-optimised release profile is the slowest build in the repository |

The full suite runs exactly once, in the `rust` job's `cargo nextest run --workspace
--profile ci` step — each test's own pass/fail is in the uploaded JUnit report, which is why
no step re-runs a subset "to see its status". That same job's last two steps *do* run two
tests a second time, deliberately: the OpenAPI contract test and tack-orch's golden-drift
tests are re-invoked with `UPDATE_OPENAPI=1`/`UPDATE_GOLDEN=1` through a targeted `-E`
filter, which makes them regenerate `docs/openapi.json` / `crates/tack-orch/tests/golden/`
from the current code, and the step then diffs that output against what's committed. This is
a second pass over the same test in generate mode to catch drift, not a second verdict from
the first run — nothing here contradicts "the suite runs once." `CARGO_INCREMENTAL=0`
throughout: CI never reuses incremental state, and keeping it only inflates the cache.

### Pre-push hook

`git config core.hooksPath .githooks` activates it. It runs the comment and test-hygiene
checks, `cargo fmt --all --check` for the root workspace **and, separately, for
`crates/tack-desktop`** (its own workspace, excluded from the root one — nothing else local
sees that crate at all), `cargo clippy --workspace --all-targets -- -D warnings`, and the
lockfile freshness check — **not the test suite**, on purpose: a hook that takes a minute is
a hook people bypass, and the suite is CI's job. The `schema.gen.ts` staleness check only
runs when `frontend/node_modules` exists, so a checkout that has never run `npm install` gets
no local protection against schema drift there — CI's `frontend` job still catches it. Run
`cargo nextest run --workspace` yourself before pushing anything you claim is green.

## Coverage

```bash
make coverage   # reproduces CI's `coverage` job locally: per-crate llvm-cov floors + Vitest thresholds
```

Floors `make coverage` (and CI's `coverage` job) enforce, per `Makefile`'s `coverage` target:
`tack-core` and `tack-runner` ≥ 85 % lines; `tack-db`, `tack-api` and `tack-orch` ≥ 70 %
lines; frontend Vitest ≥ 70 % lines/functions/statements and ≥ 60 % branches.

For an HTML report instead of the pass/fail gate:

```bash
cargo install cargo-llvm-cov
cargo llvm-cov nextest --workspace --html --output-dir coverage/
```

---

## Manual smoke test

With the server running (`cargo run -p tack-cli -- serve`):

```bash
BASE=http://localhost:3210/api

# Health
curl -s $BASE/health | jq

# Create → add → move
PID=$(curl -s -X POST $BASE/projects \
  -H "Content-Type: application/json" \
  -d '{"name":"Smoke","project_type":"software"}' | jq -r '.id')

IID=$(curl -s -X POST $BASE/projects/$PID/items \
  -H "Content-Type: application/json" \
  -d '{"title":"Test task","item_type":"task"}' | jq -r '.id')

curl -s -X PATCH $BASE/items/$IID \
  -H "Content-Type: application/json" \
  -d '{"status":"In Progress"}' | jq .status

# WebSocket (requires websocat)
websocat "ws://localhost:3210/api/projects/$PID/boards/live"

# Search
curl -s "$BASE/projects/$PID/search?q=test" | jq

# GitHub import (requires a valid token for private repos)
curl -s -X POST $BASE/projects/$PID/import-github \
  -H "Content-Type: application/json" \
  -d '{"repo":"owner/repo","label_filter":["bug"]}' | jq

# Backup
curl -s $BASE/backup -o smoke-backup.db
file smoke-backup.db   # should say "SQLite 3.x database"

# Cleanup
curl -s -X DELETE $BASE/projects/$PID
```

---

## End-to-end, accessibility & API-contract tests (Playwright)

Browser-level tests that drive the **real** app — the `tack-api` server plus
the Vite-served SPA — in Chromium, Firefox and WebKit. Playwright owns both
server lifecycles, so a single command is all that's needed; the API runs
against a throwaway `e2e.db` so your working database is never touched.

```bash
make e2e-install     # one-time: download the browser engines
make e2e             # run the whole suite (chromium + firefox + webkit)
make e2e-ui          # interactive runner for debugging
```

### The database is reset every run, not just named "throwaway"

`frontend/playwright.config.ts`'s API `webServer` entry deletes `e2e.db*` and
`storage-e2e/` (in that order — see the config's own comment for why the
order matters) before it runs `cargo run -p tack-cli -- serve`, so every
invocation starts from an empty database and an empty storage dir. Before
this reset existed, nothing ever threw the file away: across one real
session it reached 383 enrolled runners, 100 projects, 277 execution
requests and 723 items, and the suite's own failure count tracked that
growth — different tests failing at each level, all passing when run alone.
**A flake rate measured against this database means nothing unless the
database's starting state is stated with it.**

The reset itself is not the expensive part. Measured directly against the
command the `webServer` entry runs (`time (rm -rf storage-e2e && rm -f
e2e.db*)`): **2ms** against a single run's leftovers (~900KB database, 40KB
storage dir) and **8ms** against ~284 accumulated `agent_runners` rows
(~8.2MB database, 240KB storage dir) — `rm` unlinks, it doesn't read, so the
cost does not grow with what's being thrown away.

The expensive part, if you skip the reset, is everything downstream. Three
consecutive full chromium runs from a clean state (`time npx playwright test
--project=chromium --workers=2`, `CARGO_TARGET_DIR` pointed at a warm
target) measured **58s, 33s, 34s** (the first pays a one-time compile-check
cost the other two don't) with row counts identical at the end of every run:
`agent_runners` 15, `projects` 4, `items` 21, `execution_requests` 11. Left
to accumulate instead — a manually-run server reused across repeated
invocations, never reset — the same class of run slowed as `agent_runners`
climbed, on an otherwise idle machine: 17s at 0, 24s at 225. A later run
against a further-accumulated database (~285 `agent_runners`) took nearly
two minutes and failed 16 tests that pass at every other level measured
here, none of them the same test — but the machine was no longer idle by
then (an unrelated CPU load spike, not from this suite, was independently
confirmed via `uptime` and `ps`), so that number is directional corroboration,
not a clean measurement. It is nonetheless consistent with this cycle's
earlier report of 383 runners producing 9 failures where a clean database
produces none. Resetting every time is faster than not, not merely more
correct.

If you need to inspect what a run left behind — debugging a failure,
checking a migration — copy `e2e.db`/`storage-e2e` aside before the *next*
run reclaims them; there is no flag to skip the reset.

### Running this suite while another instance is also running it

The API server binds a **fixed** port (3399) and the SPA a fixed port
(5199), the same for every checkout — there is nothing per-worktree or
per-process about them. Locally (never in CI), Playwright's
`reuseExistingServer` means a second invocation that finds something
already answering the health check on that port **reuses it** instead of
starting its own — and the reset above only ever runs in the codepath that
starts a fresh server. Reuse is silent: nothing reports that the server
answering your requests belongs to a different checkout, with a different
database, possibly mid-run itself.

This is not hypothetical — it is the concrete explanation for several
irreconcilable flake-rate measurements produced across this codebase's
history, each taken without realizing another process on the same machine
was answering the same port. If a run reports failures that don't reproduce
solo and don't match anything you changed, check for another instance
before trusting the number:

```bash
pgrep -af "playwright|tack serve"   # any other run or leftover server
ss -ltnp | grep -E '3399|5199'      # who actually holds this suite's ports
```

A `tack serve` on a *different* port (3210 is the plain `tack serve`
default; an installed release build or another tool may sit there) is
unrelated and safe to ignore. One already on 3399 or 5199 is not — either
wait for it to finish or coordinate with whoever's running it; there is no
per-worktree isolation for these ports today.

Layout (`frontend/e2e/`):

| File | Covers |
| --- | --- |
| `smoke.spec.ts` | Every primary surface renders without a blank screen or page error — **all 3 browsers** |
| `journey.spec.ts` | A created item flows to the board and opens with the correct title (regression guard for the two QA bugs) |
| `a11y.spec.ts` | WCAG 2.0/2.1 A & AA scans via axe-core (chromium) — new violations fail CI |
| `api.spec.ts` | Wire-contract checks: health shape, hardening headers, response envelopes, 404s |
| `helpers.ts` | Single source of truth for API response shapes (`getOrCreateProject`, etc.) |

Config: `frontend/playwright.config.ts`. Cross-browser coverage is the `projects`
list; engine-independent specs (`a11y`, `api`) self-skip to chromium only.

**Triaging existing a11y debt:** add the axe rule id to `KNOWN_ISSUES` in
`a11y.spec.ts` with a tracking note instead of deleting the assertion, so the
gate keeps blocking *new* regressions.

---

## Dependency vulnerability scanning

```bash
make audit           # cargo audit (Rust) + npm audit --audit-level=high (frontend)
```

Runs in CI as the **security** job (`cargo-audit` via the RustSec advisory DB +
`npm audit`). [Dependabot](../.github/dependabot.yml) opens weekly grouped
update PRs for cargo, npm and GitHub Actions.

Known, justified Rust advisory exceptions live in
[`.cargo/audit.toml`](../.cargo/audit.toml) with a documented reason each — the
gate still fails on any **new** advisory. Re-review that list on every dep bump.

> **Known a11y debt:** none currently. The `KNOWN_ISSUES` list in
> `e2e/a11y.spec.ts` is empty — the earlier `color-contrast` and `select-name`
> suppressions have been fixed and removed, so the axe scan gates on a fully
> clean baseline. If a justified, hard-to-fix violation ever needs suppressing,
> add its axe rule id to `KNOWN_ISSUES` with a tracking note rather than deleting
> the assertion, so the suite keeps blocking *new* classes of regression.

---

## Load / performance testing (k6)

HTTP-level load test establishing the performance baseline. Not part of default
CI (needs a running server, time-consuming) — run on demand.

```bash
# terminal 1: a server with a throwaway DB
TACK_DATABASE_URL='sqlite:load.db?mode=rwc' cargo run -p tack-cli --release -- serve
# terminal 2:
make load
```

Ramps to 50 VUs on the read hot path + a write path, asserting p95 latency and
error-rate thresholds. The write p95 threshold is where SQLite's single-writer
model shows up first. See [`tests/load/README.md`](../tests/load/README.md).

---

