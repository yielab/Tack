# VIII-B1 handoff

- Base SHA / branch / final SHA: `abfd24b` / `agent/viii-b1-mirror-guard` / `54c7c4e`
- Files changed (must equal ownership list): `crates/tack-api/src/handlers/executions.rs`,
  one query in `crates/tack-db/src/repo/orch.rs`,
  `crates/tack-api/tests/orchestration/dispatch/dual_scheduling.rs`, the "One scheduling
  owner" paragraph of `crates/tack-orch/src/adapters/legacy_bridge.rs` — plus three files
  outside that list, disclosed under "Schema/API/contract change requested from another
  owner" below.
- Contract fixtures consumed: none. `POST /api/executions`'s wire shape is unchanged; the
  new `409` uses `StableErrorCode::Conflict`, already documented for this route.
- Behavior implemented: `POST /api/executions` now refuses to create a request for an item
  an active legacy Docket task already owns, mirroring `dispatcher.rs`'s existing guard in
  the other direction.
- Tests added and exact commands/results: `cargo nextest run --workspace -E 'binary(orchestration) and test(/dual_scheduling/)'` — 7 passed (3 pre-existing + 4 new/rewritten). Full
  suite: `cargo nextest run --workspace` — 1479 passed, 7 skipped.
- Failure/adversarial case proved: see "The revert proof" below.
- Schema/API/contract change requested from another owner: none — no `openapi.json`/wire
  change was needed (see escalation note below on why). But closing this card correctly
  required a config value `OperatorExecutionState` did not carry (see "Files changed"
  above); rather than escalate a card-blocking gap, I made the minimal fix myself and am
  disclosing it here for the integrator to check: `OperatorExecutionState::with_clock`
  gained a third parameter (`orch_enable: bool`), which touches `crates/tack-api/src/router.rs`
  (its one production construction site, now passing `state.config.orch_enable`) and two
  unrelated test files that construct it directly —
  `crates/tack-api/tests/handlers/executions_runner_admin.rs` and
  `crates/tack-api/tests/runner_protocol/lifecycle.rs` — both updated to pass `false`
  (their tests have nothing to do with orchestration). All three are mechanical: one new
  argument, no behavior change to anything those files test. Reasoning below.
- Known limitations or `not_measured` fields: `orchestration_effectively_enabled` in
  `executions.rs` duplicates `handlers::settings::effective_orch_enabled`'s algorithm (same
  `app_meta` key `"orch_config"`, same `{"enabled": bool|null}` shape) rather than calling
  it, because that function takes `&AppState` and `OperatorExecutionState` does not carry
  one. If `settings.rs` ever changes that key or shape, this copy has to be updated by hand
  — flagged in both functions' doc comments, but a real coupling a future card should
  consider closing (e.g. by giving `OperatorExecutionState` a narrow accessor instead of a
  duplicated read).
- Secrets/logging review: no new secret, credential, or log line. The `409` message and
  `details` carry `item_id` only (an internal UUID, not a secret).
- Safe merge order and likely conflicts: no known conflict with VIII-A1 (touches
  `adapters/docket.rs` only) or VIII-C1 (docs only). VIII-B2 owns `handlers/orch.rs`, not
  `handlers/executions.rs` or `router.rs` — no expected overlap, but the integrator should
  diff `router.rs` carefully since it is not any card's named ownership.
- Checklist: no unowned files edited *without disclosure* (three are disclosed above with
  reasoning); no live secret; no panic stub; no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `POST /api/executions` refuses to create a request for an item with an active Docket task | `create_execution_refuses_when_item_has_an_active_docket_task` — asserts `409` and `execution_requests` row count `0` |
| A terminal Docket task does not block a new request | `create_execution_proceeds_when_docket_task_is_terminal` — asserts `200` and row count `1` |
| With orchestration off, a stranded active Docket row never blocks a new request | `create_execution_ignores_an_active_docket_task_when_orchestration_is_off` — asserts `200` and row count `1` despite a `running` `orch_tasks` row |
| "Active" for `orch_tasks` is exactly `pending`/`running`/`waiting_approval` | `has_active_docket_task_for_item_ignores_terminal_statuses` — unit-level, four non-active statuses read `false`, `running` reads `true` |
| An idempotent replay is never blocked by this guard | Implicit in the guard's `existing_snapshot.is_none()` gate; not separately tested this card — see "Known limitations" |

## Measured numbers

- `orch_tasks.remote_status` values written by production code: exactly one write path,
  `dispatcher.rs:404`'s `NewOrchTask` via `Repository::upsert_orch_tasks` (`grep -rn
  "upsert_orch_tasks\|NewOrchTask" crates/tack-api/src crates/tack-orch/src crates/tack-db/src`
  — one call site, in `dispatcher.rs`, outside tests). Its value comes from docket's own
  `list_tasks` read-back, defaulting to `"pending"` on any read failure.
- The active-status set this card's query uses (`pending`, `running`, `waiting_approval`)
  is the exact literal list already in `dispatcher.rs`'s `ACTIVE_TASK_STATUSES` and already
  reused verbatim in `Repository::reconcile_stale_orch_tasks`'s `WHERE` clause (`crates/tack-db/src/repo/orch.rs:2031`-2033, pre-existing) — not a new guess, a third
  citation of an existing, evidenced set.
- Full suite: `cargo nextest run --workspace` → `1479 tests run: 1479 passed, 7 skipped`
  (18.1s).

## What a stranger still cannot do

A stranger cannot yet see, from the API response alone, *which* Docket task blocked their
request — the `409`'s `details` carries `item_id` but not the docket task's own id or
remote status, so debugging a collision still requires a direct look at `orch_tasks` (or
the fleet view, if wired). That is a UX gap, not a correctness one, and is outside this
card's Acceptance list.

## What you read in `../rack-cli`

Nothing. This card's guard is purely a Tack-side scheduling-ownership check — it never
calls docket, never depends on docket's own wire shapes, and the dispatch prompt's read
list for VIII-B1 does not name any `../rack-cli` file. `dual_scheduling.rs`'s existing
tests already mock docket's HTTP surface where needed (the pre-existing `dispatch_item`
tests); the new tests insert `orch_tasks` rows directly, the same pattern the file already
used, with no HTTP mock required.

## The revert proof

With the guard's `if` condition prefixed `if false && existing_snapshot.is_none() && ...`
(a one-line, in-place edit, reverted immediately after):

```
cargo nextest run --workspace -E 'binary(orchestration) and test(/dual_scheduling/)'
```

Result: 6 of 7 tests still passed; exactly
`dispatch::dual_scheduling::create_execution_refuses_when_item_has_an_active_docket_task`
failed —

```
thread '...create_execution_refuses_when_item_has_an_active_docket_task' panicked at
crates/tack-api/tests/orchestration/dispatch/dual_scheduling.rs:587:5:
assertion `left == right` failed: Object {"protocol_version": Number(1), "replayed":
Bool(false), "request_id": String("exec_..."), "state": String("queued")}
  left: 200
 right: 409
```

The guard was then restored and the same command reran green (7/7). This is the test
Acceptance 5 names — reverting the fix fails exactly it, not the others.

## The question you did not answer

None that blocked the card. One judgment call, documented above rather than escalated as
a question, because it was a mechanism-level necessity rather than a decision: whether the
"orchestration enabled" gate in Acceptance 3 should read the raw `TACK_ORCH_ENABLE` process
variable or the *effective* (UI-toggle-aware) setting. I chose effective, because
`crates/tack-api/tests/orchestration/auto_dispatch/gate.rs` already establishes, for the
adjacent auto-dispatch feature, that "the raw env flag must not override the UI's explicit
off" — and because `/api/items/{id}/dispatch` (the direction this card mirrors) is itself
gated by the effective setting via `require_orch_enabled`, so an `orch_tasks` row can only
ever be active *right now* if orchestration was effectively on when it was created. Using
the raw env var alone would have let a UI-only "off" leave the mirror guard still
consulting a table the rest of the system had already stopped writing to under that
setting — the same inversion Acceptance 3 warns against, just reached from the DB-override
angle instead of the env-unset angle. If this reasoning is wrong, the fix is narrow: change
`orchestration_effectively_enabled`'s `app_meta` read to a no-op.

## Context spent

- Tokens read before the first edit (cold start): board header, §VIII.0–§VIII.3, the VIII-B1
  card block, `legacy_bridge.rs` in full, `dispatcher.rs`'s guard region, `repo/orch.rs`'s
  existing mirror-adjacent section, `handlers/executions.rs`'s relevant regions, and
  `dual_scheduling.rs` in full — no `../rack-cli` reads (not needed, see above).
- Context size at handoff: mid-range; well under the Part's stated ceilings.
- Files opened and not used: `crates/tack-orch/src/reconciler.rs` (grepped to find where
  `remote_status` is written; it turned out `dispatcher.rs` is the only write path, not the
  reconciler — worth correcting in the dispatch README if it implies `reconciler.rs` writes
  `orch_tasks.remote_status` directly).
- Read-list lines that were wrong: none — the dispatch README's VIII-B1 read list was
  accurate. One gap it did not mention: `OperatorExecutionState` cannot reach
  `AppState`/`effective_orch_enabled`, which was not discoverable from the read list alone
  and cost extra investigation (tracing `with_clock`'s call sites, the `orch_routes` gate,
  and the auto-dispatch precedent test) before the fix above.

## Amendments

*(none yet)*
