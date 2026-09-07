# VI-C36 handoff

- Base SHA / branch / final SHA: base `e27c2c0` (develop tip at start) / branch
  `agent/vi-c36-enqueue` / final SHA recorded with the commit below.
- Files changed (must equal ownership list): `crates/tack-api/src/handlers/executions.rs`,
  `crates/tack-api/tests/wiring/model.rs`, this handoff.
- Contract fixtures consumed: none — no wire-shape change landed (see "Stop, not fix" below).
- Behavior implemented: `create_execution`'s `enqueue_execution` catch-all now logs the real
  underlying `sqlx::Error` (`item_id`, `request_id`, the error's `Display`) before mapping it
  to the client-facing `internal_error` envelope, in place of the previous `.map_err(|_| ...)`
  that discarded it unconditionally. No wire-contract change: the response the client sees is
  byte-identical to before this card.
- Tests added and exact commands/results: one new permanent regression test,
  `create_execution_succeeds_again_for_an_item_with_a_finished_attempt`
  (`crates/tack-api/tests/wiring/model.rs`) — runs a real request through claim → accept →
  start → completion against the production router, then re-enqueues the same item with a
  fresh idempotency key, same runner, same agent profile, and asserts `200`/`"state":"queued"`.
  `cargo nextest run --workspace --build-jobs 4 --test-threads 4 -E 'binary(wiring)'` → 15
  passed. Full suite: `cargo nextest run --workspace --build-jobs 4 --test-threads 4` → 1464
  passed, 7 skipped (pre-existing `#[ignore]`s, untouched by this card).
- Failure/adversarial case proved: see "Reproduction attempts" below — this card could not
  reproduce the reported failure, so there is no fix to prove load-bearing by reverting. The
  logging change was verified additively (it changes nothing observable to a client; verified
  by the identical response shape before/after in the test above and by `cargo build -p
  tack-api` compiling clean).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: the reported condition remains unreproduced.
  Whether it is a rare SQLite contention event under real embedded-runner background traffic
  (heartbeats/claims/event-batches racing an operator's `create_execution` call) or something
  specific to the Tauri webview's HTTP client is `not_measured` — this card's tools (curl-
  equivalent HTTP replay, no GUI) cannot drive that traffic mix. See "What is still unknown."
- Secrets/logging review: the new `tracing::warn!` logs `item_id`, `request_id` (both opaque
  ids, already logged elsewhere in this codebase), and the `sqlx::Error`'s `Display`. That
  `Display` is always either a fixed string from this file's own `snapshot_error()` helper
  (e.g. `"invalid execution request snapshot: request_id contradicts normalized request"` —
  no interpolated request content) or a SQLite driver message (a constraint/table name or a
  status string, e.g. `"database is locked"`) — never a bound parameter value, a credential,
  or a request body. Reviewed against every `snapshot_error(...)` call site in
  `crates/tack-db/src/repo/execution.rs` to confirm none embed request content.
- Safe merge order and likely conflicts: touches only `executions.rs`'s `create_execution` and
  a new test at the end of `wiring/model.rs`; no shared-file conflict expected with other Wave
  17 cards.
- Checklist: no unowned files touched; no live secret; no panic stub; no blind retry (this
  card explicitly avoided adding an untested "fix").

## Stop, not fix — why

The card's own escape hatch applies: **"Stop if: the condition turns out to be transient
after all — show the timing, and stop."** This card could not reproduce a persistent,
per-item enqueue failure through any means available to it, despite replaying every
observable detail the original report gave. Before writing this off, every concrete
hypothesis the report's language suggested was tested and falsified:

1. **"One live request per item."** `execution_requests` has no `UNIQUE`/`CHECK` constraint
   and no trigger on `item_id` — confirmed by reading migration 044
   (`crates/tack-db/src/migrations.rs:1397-1426`) in full. A production test
   (`crates/tack-api/tests/handlers/attempt_scoping.rs`,
   `crates/tack-api/tests/handlers/attempt_lists.rs`, both already in the suite) already
   creates two separate execution requests for one item and passes in CI, independent of
   this card.
2. **"A row a previous attempt left."** Replayed against the real production router
   (`tack_api::router::build_router`) with a finished (`succeeded`) first attempt, then again
   with an *unfinished* (`running`, never completed — simulating a hard-killed app) first
   attempt, both times re-enqueuing the same item with a fresh idempotency key, the same
   runner, and the same agent profile. Both succeeded (`200`, `"state":"queued"`) every time.
3. **A real, file-backed database** (not the in-memory harness `CLAUDE.md` warns can hide
   concurrency bugs): the same sequence was replayed against a real `tack serve` process
   (`TACK_DATABASE_URL=sqlite:.../tack.db?mode=rwc`) over HTTP, including the report's own
   timing — a second attempt at the same moment and a third six seconds later, matching the
   original transcript's gap. All three enqueue calls succeeded.
4. **Concurrent write contention** (`crates/tack-orch`'s own history has a precedent here —
   see below): 40 concurrent `create_execution` calls against the *same* item, same runner,
   same profile, fresh keys, fired via a thread pool against the real file-backed server —
   `0/40` failures.

None of these reproduce anything resembling `{"code":"internal_error","message":"Could not
enqueue execution","retryable":true}`. The scripts used for the file-backed and concurrent
reproduction attempts are not part of this diff (scratch tooling, not committed); the
in-process regression test above is the one that is kept, because it directly falsifies the
report's leading hypothesis ("a previous attempt leaves state that blocks the next request")
as a permanent guard.

## What is still unknown

`enqueue_execution` (`crates/tack-db/src/repo/execution.rs:2671`) is not new territory for
this exact failure shape: `docs/agent-handoffs/part-iii/III-B2.md` records that this same
function, before its `BEGIN IMMEDIATE` fix, deadlocked in `6/20` runs under concurrent
duplicate-key retries (SQLite error code 6, `"database is deadlocked"`), one of five sites in
this file fixed for the same class of bug. That fix is in place today (confirmed at
`execution.rs:2684`, `self.pool().begin_with("BEGIN IMMEDIATE")`) and this card's own 40-way
concurrent stress test found no residual deadlock or busy-timeout failure. But that audit
only stress-tested *one* function at a time against its own duplicate-caller pattern — never
the mix of simultaneous traffic a live embedded runner actually produces (its own poll/claim
loop, heartbeats, and event-batch reports, all independently opening `BEGIN IMMEDIATE`
transactions, running concurrently with an operator's `create_execution` call). This card's
tools cannot drive that traffic mix without a GUI or a real coding-agent harness, both out of
reach here. If the condition recurs, the logging added by this card
(`crates/tack-api/src/handlers/executions.rs:844-858`) will show whether it is
`"database is locked"`/deadlocked (supporting the contention theory) or one of
`execution.rs`'s own `snapshot_error` strings (which would point at a real logic bug in
`validate_execution_request_snapshot`, not contention) — that capture is the recommended next
step, not a further blind reproduction attempt.

## Frontend — already adequate, unchanged

The card also asked for the Run-with-agent modal to show the real reason instead of "try
again." Checked and left unchanged: `frontend/src/shared/api/client.ts`'s `toApiError`
already extracts `error.message` verbatim from the envelope, and
`frontend/src/shared/runWithAgent/RunWithAgentModal.tsx`'s `submit()` already surfaces
`err.message` directly in its toast — there is no hardcoded "try again" text in this path.
The modal was never the problem; it passes through whatever the server says. If a future
occurrence lets the server name the real condition, this path already carries that message to
the user with no further frontend change needed.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A second `create_execution` for an item whose first attempt already finished succeeds, not `internal_error` | `create_execution_succeeds_again_for_an_item_with_a_finished_attempt`, `crates/tack-api/tests/wiring/model.rs` |
| The reported failure is not reproducible via the documented protocol, sequentially, timing-matched, or under 40-way concurrent load | See "Stop, not fix" above; commands run interactively this session, not committed as test code |
| `enqueue_execution`'s error is no longer silently discarded | `crates/tack-api/src/handlers/executions.rs:844-858`; verified by reading the diff and confirming `cargo build -p tack-api` compiles |

## Measured numbers

- `cargo nextest run --workspace --build-jobs 4 --test-threads 4 -E 'binary(wiring)'`: 15
  passed, 0 failed.
- `cargo nextest run --workspace --build-jobs 4 --test-threads 4` (full suite): 1464 passed, 7
  skipped.
- Concurrent-load reproduction attempt: 40/40 concurrent `POST /api/executions` calls against
  one item succeeded (`0` failures) against a real file-backed `tack serve` process.
- Sequential reproduction attempt against a real file-backed server, timing-matched to the
  original report's six-second gap: 3/3 enqueue calls succeeded (immediate retry, +6s retry,
  fresh item).

## What a stranger still cannot do

Unchanged by this card. A stranger who hits `"Could not enqueue execution"` today still sees
an opaque `internal_error` with no named cause — this card added server-side logging to make
the *next* occurrence diagnosable, but did not change what the client sees, because no
specific condition could be confirmed to name.

## Context spent

- Tokens read before the first edit (cold start): this card's investigation (reading
  `executions.rs`, `execution.rs`, migrations, the frontend error path, and
  `docs/agent-handoffs/part-iii/III-B2.md`) ran well past a typical cold start because the
  reported condition could not be pinned down from source alone and required live
  reproduction attempts against a real server.
- Files opened and not used: `crates/tack-api/src/dispatcher.rs` and
  `crates/tack-db/src/repo/orch.rs` (the legacy docket "one scheduling owner" invariant) were
  read in full while chasing the "per-item invariant" hypothesis and ruled out — that
  invariant guards the *legacy* dispatch path, not `POST /api/executions`.
- Read-list lines that were wrong: none — this card had no dispatch-plan block to follow (Wave
  17 cards read directly from `TODO.md`).

## Amendments

None yet.
