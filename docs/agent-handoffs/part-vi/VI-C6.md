# VI-C6 handoff

- Base SHA / branch / final SHA: `a84a089` (develop) / `agent/vi-c6-unknown-attempt` / not committed — working tree only, per instruction not to commit.
- Files changed (must equal ownership list): `crates/tack-api/tests/handlers/attempt_lists.rs` (extended — 4 new tests plus one helper and one signature change to an existing helper), `docs/agent-handoffs/part-vi/VI-C6.md` (this file). No production file touched (`git diff --stat` is exactly one line, the test file — confirmed after every temporary revert was restored).
- Contract fixtures consumed: none — same rationale as VI-C5: these two routes carry no `docs/contracts/runner-v1/` fixture of their own.
- Behavior implemented: no new behavior. Closed VI-C5's own declared gap: what `GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts` and `.../decisions` do for (a) an attempt number nobody ever claimed, and (b) an attempt number that is real but belongs to a *different* execution request.
- Tests added and exact commands/results:
  - `attempt_artifacts_unknown_attempt_number_is_404`
  - `attempt_artifacts_from_a_different_execution_is_404`
  - `attempt_decisions_unknown_attempt_number_is_404`
  - `attempt_decisions_from_a_different_execution_is_404`
  - Also changed `create_agent_profile`'s signature to take a `label: &str` (the cross-execution tests stand up two executions in one test, each needing its own agent profile; profile names are unique, so the previous hardcoded name collided on the second call — fixed at both existing call sites too).
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C6 cargo nextest run --workspace` → `1424 tests run: 1424 passed, 7 skipped`.
  - `cargo nextest run --workspace -E 'binary(runner_contract)'` → `18 tests run: 18 passed, 0 skipped`.
  - `cargo nextest run --workspace -E 'binary(openapi_contract)'` → `5 tests run: 5 passed, 0 skipped`.
  - `cargo clippy --workspace --all-targets -- -D warnings` → clean. `./scripts/check-comments.sh` and `./scripts/check-test-hygiene.sh` → both clean.
- Failure/adversarial case proved: see the Claim → evidence table — two distinct reverts, each restored immediately after capturing the failure.
- Schema/API/contract change requested from another owner: none. **The finding the card asked me to watch for did not occur** — neither route answers `200`/empty for an attempt that was never created; both correctly answer `404` with `{"resource": "execution_attempt"}`. See "What was found" below for the mechanism and the one real bug this pass exposed (as a synthetic revert, not live in `develop`).
- Known limitations or `not_measured` fields: none opened by this card. The remaining declared gap from VI-C5 (unknown `request_id`, as opposed to unknown `attempt_number`) is *not* addressed here — the card's own two questions are both about `attempt_number`, and the `request_id`-level `404` is already exercised by the twin `/attempts` route's own test (`operator_read_routes.rs::attempts_for_an_unknown_request_id_is_404`) via the shared `execution_request_exists` guard both handlers call first.
- Secrets/logging review: no new logging added; no secret-bearing fields touched.
- Safe merge order and likely conflicts: touches only `crates/tack-api/tests/handlers/attempt_lists.rs`, the same file VI-C5 introduced. No other Wave 17 card owns this file per VI-C5's own handoff; merge after (or before) VI-C5 cleanly either way since this is a pure extension of that file's tail, not an edit to VI-C5's existing lines.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry. Confirmed.

## What was found

Both handlers (`crates/tack-api/src/handlers/attempt_lists.rs::list_execution_attempt_artifacts`
and `::list_execution_attempt_decisions`) share one mechanism for both open questions. Each
calls a repo method (`tack-db/src/repo/execution.rs::list_execution_artifacts_for_attempt_number`
/ `::list_execution_decisions_for_attempt_number`) whose first statement is:

```sql
SELECT id FROM execution_attempts WHERE request_id = ? AND attempt_number = ?
```

If that returns no row, the repo method returns `Ok(None)`; the handler turns `None` into
`404 {"error":{"code":"not_found","details":{"resource":"execution_attempt"}}}` (lines 130-137
and 243-250 of the handler file — the `let Some(x) = x else { return Err(...) }` guard right
after the repo call). If it returns a row, the handler proceeds to a real `data` list (which may
itself be empty — that is VI-C5's territory, not this card's).

Because the query binds **both** `request_id` and `attempt_number`, the two open questions
resolve identically and correctly:

- **An attempt number nobody ever claimed** (e.g. attempt 99 on a request that only ever
  claimed attempt 1): zero rows match on `attempt_number` alone → `None` → `404`.
- **An attempt number that is real but belongs to a different execution** (e.g. execution X
  claimed attempt 1 and has a real manifested artifact/decision on it; execution Y is a real,
  separate execution request that never claimed anything; the caller asks for Y's attempt 1):
  zero rows match because `request_id = Y` excludes X's row → `None` → `404`. X's real artifact
  never reaches the response.

**A caller cannot wrongly conclude "no artifacts/decisions yet" from a `404` here** — the
routes already distinguish that from the true empty-but-real-attempt case (`200`/`data: []`,
VI-C5's `attempt_artifacts_is_empty_before_any_manifest` /
`attempt_decisions_is_empty_before_any_decision_raised`). The finding the card asked me to
watch for — a route answering `200` with an empty list for an attempt that was never created —
**does not occur**. Both routes are pinned as `404` for both scenarios, by the four new tests.

The one real bug this pass exposed was synthetic, produced only to prove test B's specificity
(see the Claim → evidence table's second row per route): if the `request_id` predicate were
ever dropped from that `SELECT` — e.g. a future refactor that "simplifies" the lookup to just
`attempt_number` — the cross-execution case stops being a `404` and starts **leaking another
execution's real artifact/decision content** (`200` with X's row, when the caller asked about
Y). That never happens in `develop` today; it is exactly what
`attempt_artifacts_from_a_different_execution_is_404` and
`attempt_decisions_from_a_different_execution_is_404` now guard against by asserting the
seeded cross-execution id is absent from the raw response body, not just the status code.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| An attempt number that was never claimed for a real execution request returns `404` naming `execution_attempt`, not `200`/empty | `attempt_artifacts_unknown_attempt_number_is_404`. Reverted `tack-db/src/repo/execution.rs::list_execution_artifacts_for_attempt_number`'s not-found guard (`let Some(attempt_id) = attempt_id else { return Ok(None) }` → `let attempt_id = attempt_id.unwrap_or_default()`, falling through to a query that matches nothing); rerun failed with `left: 200 right: 404`, body `{"data":[],"protocol_version":1}`. Reverted back. |
| An attempt number that is real but belongs to a different execution returns `404` naming `execution_attempt`, and leaks nothing from the other execution's real artifact | `attempt_artifacts_from_a_different_execution_is_404`. Two reverts, each restored before the next: (1) same guard-bypass as above — also fails this test (`left: 200 right: 404`), proving it shares the same not-found mechanism; (2) a sharper, isolated revert — dropped the `request_id = ?` predicate and its bind from the same repo method's `SELECT`, leaving only `WHERE attempt_number = ?`. Rerun: **only** this test failed (the unknown-number test stayed green), with `left: 200 right: 404` and the response body containing the other execution's real seeded row: `{"data":[{"artifact_id":"cross-execution-artifact",...}],"protocol_version":1}`. Reverted back. |
| An attempt number that was never claimed for a real execution request returns `404` naming `execution_attempt` (decisions route) | `attempt_decisions_unknown_attempt_number_is_404`. Same guard-bypass technique applied to `list_execution_decisions_for_attempt_number`; rerun failed with `left: 200 right: 404`, body `{"data":[],"protocol_version":1}`. Reverted back. |
| An attempt number that is real but belongs to a different execution returns `404` naming `execution_attempt`, and leaks nothing from the other execution's real decision (decisions route) | `attempt_decisions_from_a_different_execution_is_404`. Same two-revert pair as the artifacts case, on `list_execution_decisions_for_attempt_number`: guard-bypass fails this test too (`left: 200 right: 404`); the isolated `request_id`-predicate drop fails **only** this test, leaking `{"data":[{"decision_id":"cross-execution-decision",...}],"protocol_version":1}`. Reverted back. |

Both reverts were run once each against the full `attempt_lists` test module (109 tests) to
confirm exactly which tests moved: the guard-bypass revert (applied to both repo methods at
once) failed exactly the 4 new tests and none of VI-C5's existing 6; the predicate-drop revert
(applied to both repo methods at once) failed exactly the 2 cross-execution tests and left the
other 107 (including the 2 unknown-number tests) green.

## Measured numbers

- Full workspace suite: `1424 tests run: 1424 passed, 7 skipped` (`cargo nextest run --workspace`).
- `attempt_lists` module alone: `109 tests run: 109 passed` (VI-C5's 6 + this card's 4, on a
  clean tree; drops to 105/109 or 107/109 under the two respective temporary reverts above).
- `runner_contract`: `18 tests run: 18 passed, 0 skipped`.
- `openapi_contract`: `5 tests run: 5 passed, 0 skipped`.
- `git diff --stat`: `1 file changed, 135 insertions(+), 3 deletions(-)` — `crates/tack-api/tests/handlers/attempt_lists.rs` only, confirmed after both temporary reverts were restored.

## What a stranger still cannot do

Nothing changes for a user of the product — both routes behaved this way before this card;
VI-C4 shipped the handlers, and they already scope by `(request_id, attempt_number)` together.
What a stranger arriving at this crate's test suite could not previously do is find, at the
Rust level, a test that fails if a future change collapses "attempt never existed" or "attempt
belongs to someone else" into a false `200`/empty, or — more sharply — into a real cross-tenant
data leak. Both gaps VI-C5 declared open are now closed for these two routes.

## Surface-map delta

None. This card touches no console-vs-UI step in §VI.0's surface map — it is a test-only
proof card against handlers VI-C4 already shipped and VI-C5 already exercised for auth, empty
and ordering.

## Context spent

- Tokens read before the first edit (cold start): the card + dispatch block (~1.4k), the
  cold-start capsule §VI.0 (~2.5k, per the dispatch's own reading list), `attempt_lists.rs`
  whole (~2.9k), the handler source `handlers/attempt_lists.rs` (~2.2k), the two repo methods
  in `tack-db/src/repo/execution.rs` (~1.6k), the VI-C5 handoff's gap section (~0.3k), and a
  peek at `operator_read_routes.rs`'s own unknown-request-id/unknown-attempt-number tests for
  the established pattern (~1.5k) — roughly 12.5k tokens, in line with the dispatch's own ~9k
  estimate for the required reading plus the extra sibling-file peek (see below).
- Context size at handoff: well under the 150k stop threshold.
- Files opened and not used: none beyond the dispatch's own reading list — the one addition
  (`operator_read_routes.rs`'s `attempts_for_an_unknown_request_id_is_404` and
  `events_reflect_a_real_reported_batch_and_unknown_attempt_number_is_404`) was read
  deliberately to confirm the established `404`-body shape (`error.details.resource`) this
  card's assertions reuse, not a detour.
- Read-list lines that were wrong: none. One environment gap not caused by the dispatch: the
  worktree's initial `HEAD` (before this session created the card branch) was `e5206c7`, an
  older commit than the `develop` tip (`a84a089`) the card specifies — the branch had to be
  recreated from `develop` explicitly rather than from the worktree's pre-existing checkout.
  Worth a note for whoever writes the next dispatch: confirm the worktree's starting `HEAD`
  matches the named base SHA before assuming `git checkout -b <branch>` alone is correct.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
