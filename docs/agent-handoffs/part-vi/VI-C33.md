# VI-C33 handoff

- Base SHA / branch / final SHA: dispatched against `develop`'s tip `cc7db7e` (`Merge deps:
  @solidjs/router 1.0 and @types/node 26; typescript 7 and jsdom 30 held back with reasons`)
  on branch `agent/vi-c33-tracing-init-once`, one commit
  (`fix(api): make tracing init idempotent so llvm-cov's one-process test run stops
  panicking`) — `git log -1 --format='%H' agent/vi-c33-tracing-init-once` for the exact
  final SHA; recording it here is self-referential (this line is part of that commit's own
  content) so it is named by its message instead of chased through further amends.
- Files changed (equals the card's ownership list, plus one comment correction it made
  stale):
  - `crates/tack-api/src/server.rs` — `init_tracing` now calls `try_init` instead of
    `init` on both branches (JSON and plain formatting).
  - `crates/tack-cli/src/local_runner.rs` — a test-module comment that described the old
    `.init()` panic-on-second-call behavior; corrected to describe `try_init`'s silent
    no-op instead, since the fix makes the old wording false.
  - `CHANGELOG.md` — one `[Unreleased] / Fixed` bullet.
  - `docs/agent-handoffs/part-vi/VI-C33.md` — this file.
- Contract fixtures consumed: none.
- Behavior implemented: process-wide tracing initialization is now idempotent. `tack
  serve` (and every in-process caller of `serve_inner`) still gets its subscriber
  configured exactly once, honoring `TACK_LOG_LEVEL` and `TACK_LOG_JSON` as before; a
  second call in the same process — which never happens in a real `tack` binary, only
  under `cargo llvm-cov`'s one-process-for-all-tests model — now silently keeps the
  first subscriber instead of panicking.
- Tests added and exact commands/results: none added — the existing test
  (`server::tests::serve_with_ready_signals_the_real_bound_address`) already exercises
  `init_tracing` through `serve_with_ready`; no new assertion was needed because the
  fix's proof is the coverage job itself going from panic to pass. See "Claim → evidence"
  for the exact commands and numbers.
- Failure/adversarial case proved: reverted the fix (`git stash`) and reran
  `cargo llvm-cov -p tack-api --fail-under-lines 70` against `develop`'s tip unchanged —
  panics identically to the card's cited evidence (same panic site, same message, same
  downstream `RecvError`). Restored the fix and reran the same command clean. See
  "Claim → evidence".
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none.
- Secrets/logging review: no new log line, no new secret surface. `try_init`'s discarded
  `Err` carries no data worth logging — it is the expected outcome of a second call, not
  a failure — and the comment says so.
- Safe merge order and likely conflicts: touches only `server.rs`'s `init_tracing` body
  and one test-module comment in a different crate; no overlap with any other Part VI or
  Part VII card's owned files. No conflicts expected.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| The Coverage job's `tack-api` step panics on `develop`'s tip, unfixed | `git stash` (removed the fix), `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C33 nice -n 19 cargo llvm-cov -p tack-api --fail-under-lines 70` → `thread 'server::tests::serve_with_ready_signals_the_real_bound_address' panicked at .../tracing-subscriber-0.3.23/src/util.rs:94:14: failed to set global default subscriber: SetGlobalDefaultError("a global default trace dispatcher has already been set")`, then `panicked at crates/tack-api/src/server.rs:773:35: readiness signal never arrived: RecvError(())`; `test result: FAILED. 108 passed; 1 failed` |
| The same command is green after the fix | `git stash pop` (fix restored), same command → `TOTAL 15747 4339 72.45% ... 12036 3353 72.14%` lines coverage, exit 0, no panic |
| The fix does not change `tack serve`'s logging behavior | `init_tracing`'s only change is `.init()` → `.try_init()` with the `Err` discarded; the `EnvFilter`/`fmt::layer` construction, `TACK_LOG_LEVEL`/`TACK_LOG_JSON` handling, and the single real call site (`server.rs:74`, once per process start) are all unchanged |
| The workspace test suite is unaffected | `cargo nextest run --workspace --build-jobs 4 --test-threads 4` → `1463 tests run: 1463 passed, 7 skipped`, exit 0 |
| `.githooks/pre-push` passes from the worktree root | `nice -n 19 ./.githooks/pre-push` → `✓ pre-push checks passed`, exit 0 |
| The other four Coverage-job Rust steps are unaffected and green | see Measured numbers below |

## Measured numbers

All under `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C33`, `nice -n 19`, run from the
worktree root, each with the Coverage job's exact per-crate command from `.github/workflows/ci.yml`:

| Crate | Command | Lines coverage | Floor | Result |
|---|---|---|---|---|
| tack-core | `cargo llvm-cov -p tack-core --fail-under-lines 85` | 89.34% | 85% | pass |
| tack-db | `cargo llvm-cov -p tack-db --fail-under-lines 70` | 71.52% | 70% | pass |
| tack-api | `cargo llvm-cov -p tack-api --fail-under-lines 70` | 72.14% | 70% | pass |
| tack-orch | `cargo llvm-cov -p tack-orch --fail-under-lines 70` | 94.75% | 70% | pass |
| tack-runner | `cargo llvm-cov -p tack-runner --fail-under-lines 85` | 93.35% | 85% | pass |

Frontend Vitest coverage (`npx vitest run --coverage ...`) was not run — this card owns
only `init_tracing` and its Rust callers; the frontend step shares no code with it and the
card's Acceptance is scoped to the Coverage job's `tack-api` failure.

`cargo nextest run --workspace --build-jobs 4 --test-threads 4`: 1463 tests run, 1463
passed, 7 skipped, 69.456s.

## What a stranger still cannot do

Nothing changes for anyone using `tack`. This was a CI-only defect: `cargo llvm-cov`'s
one-process-per-crate test execution collided with a subscriber-install call that assumed
one process per test (true under the normal `nextest` runner, false under `llvm-cov`). No
runtime behavior, endpoint, or CLI output changes; `tack serve`'s own logging is byte-for-byte
the same as before this card.

## Context spent

- Tokens read before the first edit (cold start): read the card's own `TODO.md` section
  (~1.1k tokens), the reporting contract and scope-discipline docs (~2k tokens), the
  existing (previous agent's) diff, `init_tracing` and its call sites, the colliding
  test, `ci.yml`'s Coverage job, and the Coverage section of
  `docs/agent-handoffs/deps/actions-major.md` — all told, well under the card's own
  budget; no dispatch-plan block exists for this card (it is a standalone Wave 17 fix,
  not part of the dispatch README).
- Context size at handoff: moderate — the largest single read was the nextest/llvm-cov
  command output, kept to `tail`-truncated slices rather than full logs.
- Files opened and not used: `docs/agent-handoffs/part-vi/TEMPLATE.md` and `VI-C32.md`
  were read only as a formatting reference for this file, not for their content — expected
  use, not waste.
- Read-list lines that were wrong: none noted — the card's read list matched what was
  actually needed.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*
