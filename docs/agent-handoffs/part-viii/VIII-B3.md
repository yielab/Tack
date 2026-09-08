# VIII-B3 handoff

- Base SHA / branch / final SHA: `884b217` / `agent/viii-b3-replay-collision` / `9e80704`
- Files changed (must equal ownership list): `crates/tack-api/src/handlers/executions.rs`
  (the conflict payload), `crates/tack-api/tests/orchestration/dispatch/dual_scheduling.rs`
  (the idempotent-replay case and a payload-naming test), `crates/tack-db/src/repo/orch.rs`
  (one new read-only query, `active_docket_task_for_item`).
- Contract fixtures consumed: none. `docs/contracts/runner-v1/errors/conflict.json` pins a
  generic `{"details":{}}` example unrelated to this route's specific payload; not touched.
- Behavior implemented: two additions to the mirror guard in `create_execution`, neither
  touching its `if` condition:
  1. A client replaying an idempotency key it already used successfully now genuinely
     succeeds even if a legacy Docket task went active on the same item after the original
     create — proven end to end, not just asserted from reading the code (see "Claim →
     evidence").
  2. The `409` a fresh (non-replay) create gets while an active Docket task blocks it now
     names that task's id and status in `details`, alongside the existing `item_id`.
- Tests added and exact commands/results:
  `cargo nextest run --workspace -E 'binary(orchestration) and test(/dual_scheduling/)'` —
  9 passed (7 pre-existing + 2 new: `create_execution_replay_succeeds_despite_an_active_docket_task`,
  `create_execution_conflict_names_the_colliding_docket_task`). Full suite:
  `cargo nextest run --workspace` — 1487 passed, 7 skipped.
- Failure/adversarial case proved: see "The revert proof" below.
- Schema/API/contract change requested from another owner: none. `RunnerV1ErrorEnvelope::
  details` (and the real runtime type it mirrors, `tack_orch::execution::ProtocolErrorEnvelope`)
  is typed `serde_json::Value` — free-form JSON, already covering any shape — so the two new
  payload keys need no OpenAPI schema change. Verified, not assumed: `cargo nextest run
  --workspace -E 'binary(openapi_contract)'` (5/5 passed, no diff) both before and after this
  change, and `docs/openapi.json`'s `RunnerV1ErrorEnvelope`/`RunnerV1Error` schemas inspected
  directly (`details` has no `properties` — an open object). No regeneration was run or
  needed. The route's documented `409` description in the `#[utoipa::path(...)]` block
  (`"conflict / idempotency_conflict / runner_revoked"`) is unchanged and still accurate —
  only the free-form body grew two keys.
- Known limitations or `not_measured` fields: none new. The existing "stranger" gap this
  card closes (see VIII-B1's handoff) is now closed for `create_execution`'s own `409`; a
  stranger can still not see this from `GET /api/executions/{id}` or any other read route —
  only from the `409` itself, at the moment of collision.
- Secrets/logging review: no new log line was added — the guard's error branch builds a
  response payload, not a `tracing` call. The two new payload fields are the row's
  `remote_task_id` and `remote_status` only (both ids/status tags, per the rule); nothing
  else from `orch_tasks` (no `cost_usd_estimated`, `tokens_in`/`tokens_out`, `trusted`,
  `dispatched_at`) reaches the response.
- Safe merge order and likely conflicts: no expected conflict with VIII-A2 (owns
  `handlers/orch.rs`, `router.rs`, `config.rs`, `tack-cli`, `docs/CONFIG.md`,
  `docs/API-REFERENCE.md`, `docs/openapi.json`, `schema.gen.ts` — none of which this card
  touches). No conflict with VIII-A1 (`adapters/docket.rs`) or VIII-C1 (docs only). Within
  `crates/tack-db/src/repo/orch.rs`, the new function is appended immediately after
  `has_active_docket_task_for_item` (VIII-B1's, unmodified) and before
  `reconcile_stale_orch_tasks` — a pure addition, no existing lines moved.
- Checklist: no unowned files edited; no live secret; no panic stub (`active_docket_task_for_item`
  returns `Result`/`Option`, never unwraps); no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A client replaying an idempotency key it already used successfully still gets `200`/`replayed: true`, even if a legacy Docket task went active on the same item after the original create | `create_execution_replay_succeeds_despite_an_active_docket_task` — same `agent_profile_id`/`runner_id` submitted twice with the same `idempotency_key`; a `running` `orch_tasks` row is inserted between the two calls; asserts `200`, `replayed: true`, and `execution_requests` row count stays `1` |
| A fresh (non-replay) `409` for this guard names which Docket task collided, and its status, not only `item_id` | `create_execution_conflict_names_the_colliding_docket_task` — inserts a `waiting_approval` task named `remote-task-named`, asserts `error.details.docket_task_id == "remote-task-named"` and `error.details.docket_task_status == "waiting_approval"` |
| The `existing_snapshot.is_none()` arm is load-bearing for the replay claim, and for nothing else in this file | See "The revert proof" — removing it fails exactly the replay test; the other 8 tests in this file (including the payload-naming test) stay green |
| Adding the two new payload keys required no OpenAPI schema change | `cargo nextest run --workspace -E 'binary(openapi_contract)'` — 5/5 passed, no diff, both before and after the payload edit |

## Measured numbers

- `dual_scheduling.rs` test count: 7 pre-existing + 2 new = 9
  (`cargo nextest run --workspace -E 'binary(orchestration) and test(/dual_scheduling/)'`).
- Full workspace suite: `cargo nextest run --workspace` → `1487 tests run: 1487 passed, 7
  skipped` (~18s).
- `openapi_contract`: `cargo nextest run --workspace -E 'binary(openapi_contract)'` →
  `5 tests run: 5 passed, 0 skipped`.
- `runner_contract`: `cargo nextest run --workspace -E 'binary(runner_contract)'` →
  `18 tests run: 18 passed, 0 skipped` — unaffected, since this card's payload change is
  outside the runner-v1 wire fixtures it pins.
- `.githooks/pre-push`: green (comments, test-hygiene, `cargo fmt --all --check` for the
  workspace and separately for `crates/tack-desktop`, `cargo clippy --workspace
  --all-targets -- -D warnings`, generated-file freshness).

## What a stranger still cannot do

A stranger still cannot see *why* the guard fired from anywhere except the `409` response
itself at the moment of the collision — there is still no persisted record connecting a
refused `create_execution` call to the Docket task that blocked it (no audit row, no event).
This card only enriches the synchronous response; it does not add a durable trail. That is a
separate, larger feature (an audit/event log for refused requests), well outside this card's
Acceptance list and not one this Part scopes.

## What you read in `../rack-cli`

Nothing. This card's changes are entirely Tack-side: a read-only `orch_tasks` query and an
error-payload enrichment. Neither the guard's condition nor the active-status set is being
re-decided, and no docket wire shape is involved — the `remote_task_id`/`remote_status`
columns this card reads were already being written by `dispatcher.rs` before this card
existed (see VIII-B1's own "What you read in `../rack-cli`": none, for the same reason).
`../rack-cli` was not opened.

## The revert proof

With the guard's `if` condition's `existing_snapshot.is_none() &&` arm removed (the
condition becomes `if (state.orchestration_enabled)(...).await && state.repo
.has_active_docket_task_for_item(...).await?`, a one-line, in-place edit, reverted
immediately after):

```
cargo nextest run --workspace -E 'binary(orchestration) and test(/dual_scheduling/)'
```

Result: 8 of 9 tests still passed; exactly
`dispatch::dual_scheduling::create_execution_replay_succeeds_despite_an_active_docket_task`
failed —

```
thread '...create_execution_replay_succeeds_despite_an_active_docket_task' panicked at
crates/tack-api/tests/orchestration/dispatch/dual_scheduling.rs:796:5:
assertion `left == right` failed: a replay of an existing request must never be blocked by
a Docket task that went active after the original create: Object {"error": Object {"code":
String("conflict"), "details": Object {"docket_task_id":
String("remote-task-after-create"), "docket_task_status": String("running"), "item_id":
String("c40c6ce0-0d6a-4f0e-8f8c-2ac3c6cdbd17")}, "message": String("Item has an active
legacy Docket task; refusing to create a runner-v1 execution request to preserve one
scheduling owner"), "request_id": String("req_operator"), "retryable": Bool(true)}}
  left: 409
 right: 200
```

The guard was then restored and the same command reran green (9/9). This is the test
Acceptance 1 names — reverting the fix fails exactly it, not the payload-naming test or any
of the seven VIII-B1 tests in the same file.

## The question you did not answer

None. This card's Acceptance was fully answerable from ADR 0060/0065, VIII-B1's handoff,
and the code — no new decision was needed. The one judgment call worth recording (not an
unanswered question, since a clear default existed): when the collision-naming query races
the boolean guard and the row resolves in between (an extremely narrow window — both reads
happen within the same request, milliseconds apart), `active_docket_task_for_item` returns
`None` and the payload renders `docket_task_id`/`docket_task_status` as JSON `null` rather
than a fabricated placeholder like `"unknown"` — matching this repository's "unmeasured is
nullable" rule. This path is not covered by a test (it would require injecting a delete
between two queries inside one handler invocation, which the test harness has no seam for);
flagged here as `not_measured` rather than silently assumed correct.

## Context spent

- Tokens read before the first edit (cold start): Part VIII README header + the VIII-B3
  card block, `TODO.md` §VIII.1–§VIII.2 and the VIII-B3 card block in §VIII.4,
  `docs/agent-handoffs/part-viii/VIII-B1.md` in full, the mirror guard region and
  `OperatorExecutionState` in `handlers/executions.rs`, `has_active_docket_task_for_item`
  and its module doc in `repo/orch.rs`, the `orch_tasks` table definition in
  `migrations.rs`, and `dual_scheduling.rs` in full.
- Context size at handoff: low-to-mid range; well under the Part's stated ceilings.
- Files opened and not used: none beyond the read list — the dispatch README's VIII-B3
  block was accurate and complete for this card.
- Read-list lines that were wrong: none.

## Amendments

*(none yet)*
