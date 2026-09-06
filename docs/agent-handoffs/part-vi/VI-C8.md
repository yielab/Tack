# VI-C8 handoff

- Base SHA / branch / final SHA: given base `502a71d` (`develop`) — the worktree's own
  starting `HEAD` was `e5206c7`, an unrelated later commit not descended from `502a71d`, so
  the branch was recreated explicitly with `git checkout -b agent/vi-c8-attempt-scoping
  develop` (confirmed `502a71d` is `develop`'s exact tip at the time). Same environment gap
  VI-C6's own handoff already flagged for the next dispatch. Branch: `agent/vi-c8-attempt-scoping`
  / final SHA: not committed — working tree only, per instruction not to commit.
- Files changed (must equal ownership list): `crates/tack-api/tests/handlers/attempt_scoping.rs`
  (new, 594 lines), `crates/tack-api/tests/handlers.rs` (module registration + one line added
  to the file's own top doc comment), `docs/agent-handoffs/part-vi/VI-C8.md` (this file). No
  production file touched — confirmed by `git diff --stat` and `git status --porcelain` after
  every temporary revert below was restored.
- Contract fixtures consumed: none — same rationale as VI-C5/VI-C6: neither the events nor the
  artifact-download route carries a `docs/contracts/runner-v1/` fixture of its own.
- Behavior implemented: no new behavior. Closed the two gaps VI-C6's own audit named: no test
  anywhere previously noticed if `list_events_for_attempt_number` or
  `get_execution_artifact_by_attempt_number` (`crates/tack-db/src/repo/execution.rs`) stopped
  scoping by `request_id`. VI-C6 covered the other two sites at the same natural key
  (`list_execution_artifacts_for_attempt_number`, `list_execution_decisions_for_attempt_number`)
  in `attempt_lists.rs`; this card covers the remaining two, backing
  `GET /api/executions/{request_id}/attempts/{attempt_number}/events`
  (`crates/tack-api/src/handlers/executions.rs`) and
  `GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content`
  (`crates/tack-api/src/handlers/runner_protocol/artifact_download.rs`, mounted on the operator
  surface by `router.rs#operator_execution_routes`, never under the runner-only protocol
  router).
- Tests added and exact commands/results:
  - `attempt_events_from_a_different_execution_is_404`
  - `attempt_events_unknown_attempt_number_is_404`
  - `attempt_artifact_download_from_a_different_execution_is_404` (the serious one — a real
    file download, not a JSON listing)
  - `attempt_artifact_download_unknown_attempt_number_is_404`
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C8 cargo nextest run --workspace` →
    `1428 tests run: 1428 passed, 7 skipped`.
  - `cargo nextest run --workspace -E 'binary(handlers)'` → `113 tests run: 113 passed, 0
    skipped` (VI-C5/VI-C6's 109 in `attempt_lists` + this card's 4 in the new `attempt_scoping`
    module).
  - `cargo clippy --workspace --all-targets -- -D warnings` → clean.
    `./scripts/check-comments.sh` → `no board archaeology in crates/`.
    `./scripts/check-test-hygiene.sh` → `tests take their temporary paths from a guard`.
- Failure/adversarial case proved: see the Claim → evidence table. One revert, applied to all
  four sites at once (mirroring exactly how the integrator's own revert was described to me),
  restored immediately after capturing the failing output.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none opened by this card. The artifact-download
  route's own auth path (`x-tack-principal`, injected by `inject_operator_principal` — distinct
  from the JSON-listing routes' own `require_token` gate, though both sit behind the same
  middleware stack in `operator_execution_routes`) is exercised implicitly by every test here
  succeeding with a plain operator bearer token; a dedicated "download route rejects an
  unauthenticated caller" test already exists (`wiring/artifact.rs`,
  `runner_protocol/artifact_events.rs`) and was not duplicated here — this card's scope is
  cross-execution scoping, not auth.
- Secrets/logging review: no new logging added; no secret-bearing fields touched. The uploaded
  artifact bytes in the leak-proof test are synthetic ASCII fixture content
  (`cross-execution-artifact-bytes`), never a real credential or secret.
- Safe merge order and likely conflicts: touches only a new file plus one small, additive edit
  to `handlers.rs`'s module list and top doc comment (a line VI-C6 also touched, but at a
  different position — the module-registration block; expect a trivial adjacent-line conflict
  if merged in the same pass as another card that also registers a new `handlers/` module,
  resolved by keeping both new `#[path]`/`mod` pairs).
- Checklist: no unowned files, no live secret, no panic stub, no blind retry. Confirmed.

## What was found

Both routes resolve an attempt the same way VI-C6's two routes do: a repo method first looks
up the internal `attempt_id` from the operator-facing `(request_id, attempt_number)` pair, then
proceeds only if that lookup returns `Some`:

```sql
SELECT id FROM execution_attempts WHERE request_id = ? AND attempt_number = ?
```

`list_events_for_attempt_number` and `get_execution_artifact_by_attempt_number` both open with
exactly this statement (lines 2500-2502 and 2855-2857 of
`crates/tack-db/src/repo/execution.rs` on the base commit) — the same natural key, same
`Option`-collapsing `Ok(None)` on a miss, as VI-C6's two sites. Because the query binds **both**
`request_id` and `attempt_number`, an attempt number that is real but belongs to a different
execution request correctly resolves to `None` → `404 {"resource":"execution_attempt"}` on both
routes, and an attempt number nobody ever claimed resolves the same way. Neither route needed a
production fix; both were already correctly scoped. No stop-condition finding — production is
correct, this closes the missing-test gap only.

**The artifact-download route is the one worth spelling out.** Unlike every JSON-listing route,
a successful response here is not a `data` array — it is the artifact's actual bytes, streamed
back verbatim with a `Content-Disposition: attachment` header naming the file
(`crates/tack-api/src/handlers/runner_protocol/artifact_download.rs::download_artifact_content`).
Proving the leak for this route meant manifesting and uploading a real artifact through the
real runner-protocol write path (claim → accept → start → manifest → `PUT .../content`, all
required — the manifest handler rejects a merely `leased` attempt with a named `409 conflict`,
"Artifacts can only be recorded while the attempt is running or awaiting a decision", stricter
than the repository's own eligibility check, which was the first thing this card's draft got
wrong and had to fix) and then showing those exact bytes come back through a **different**,
real, never-claimed execution's request id.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| An attempt number nobody ever claimed returns `404` naming `execution_attempt` on the events route | `attempt_events_unknown_attempt_number_is_404`. Passes on the base tree. |
| An attempt number that is real but belongs to a different execution returns `404` on the events route, and leaks nothing from the other execution's real event | `attempt_events_from_a_different_execution_is_404`. Dropped the `request_id = ?` predicate and its `.bind(request_id)` from `list_events_for_attempt_number`'s `SELECT` (leaving only `WHERE attempt_number = ?`), across all four of the file's sites at once (see below). Rerun: **failed** — `left: 200 right: 404`, response body `{"data":[{"created_at":"...","event_id":"cross-execution-event","kind":"log","occurred_at":"...","payload":{"message":"cross-execution-event-payload"},"sequence":1,"source":"runner"}],"protocol_version":1}` — the other execution's real reported event, verbatim, in the body a different caller received. Reverted; passes again. |
| An attempt number nobody ever claimed returns `404` naming `artifact_id` on the download route | `attempt_artifact_download_unknown_attempt_number_is_404`. Passes on the base tree. |
| An attempt number that is real but belongs to a different execution returns `404` on the download route, and **hands over no file content** from the other execution's real artifact | `attempt_artifact_download_from_a_different_execution_is_404`. Same predicate-drop revert (this route's site, `get_execution_artifact_by_attempt_number`, edited in the same pass). Rerun: **failed** — `left: 200 right: 404`, response body (the raw streamed bytes, not JSON): `cross-execution-artifact-bytes` — the other execution's real uploaded file, byte for byte, downloaded by a caller who supplied a different execution's request id. Reverted; passes again. |

The revert was applied to **all four** sites in `crates/tack-db/src/repo/execution.rs` at once
(`list_events_for_attempt_number`, `get_execution_artifact_by_attempt_number`, and VI-C6's own
two sites, `list_execution_artifacts_for_attempt_number` /
`list_execution_decisions_for_attempt_number`) — the same shape of change the card describes as
"the integrator's own revert" — then the whole `handlers` test binary (113 tests) was rerun
once. **Exactly four tests failed and nothing else**: VI-C6's own
`attempt_artifacts_from_a_different_execution_is_404` and
`attempt_decisions_from_a_different_execution_is_404`, plus this card's
`attempt_events_from_a_different_execution_is_404` and
`attempt_artifact_download_from_a_different_execution_is_404`. The other 109 — including both
of this card's own "unknown attempt number" tests — stayed green, confirming the revert's blast
radius is exactly the four routes' cross-execution behavior and nothing structural. The file
was restored immediately after capturing this output; `git diff crates/tack-db/` is empty on
the tree as delivered.

## Measured numbers

- Full workspace suite: `1428 tests run: 1428 passed, 7 skipped` (`cargo nextest run --workspace`).
- `handlers` test binary alone: `113 tests run: 113 passed, 0 skipped`
  (`cargo nextest run --workspace -E 'binary(handlers)'`).
- Single-revert run (all four sites, at once): `113 tests run: 109 passed, 4 failed` — the
  four named above.
- `git status --porcelain`: two lines — one modified (`crates/tack-api/tests/handlers.rs`), one
  untracked (`crates/tack-api/tests/handlers/attempt_scoping.rs`) — after the revert was
  restored. `git diff crates/tack-db/src/repo/execution.rs`: empty.

## What a stranger still cannot do

Nothing changes for a user of the product — both routes behaved this way before this card; the
scoping was already correct. What a stranger arriving at this crate's test suite could not
previously do is find, at the Rust level, a test that fails if a future change collapses the
events route's or the artifact-download route's `(request_id, attempt_number)` lookup into
`attempt_number` alone. For the download route specifically, that gap was the sharper one:
before this card, nothing in the Rust test suite would have caught a regression that hands one
execution's uploaded file to a caller who only ever proved they control a *different*
execution's request id — a genuine cross-tenant file leak, not metadata exposure. That gap is
now closed.

## Surface-map delta

None. This card touches no console-vs-UI step in §VI.0's surface map — it is a test-only proof
card against routes VI-C4 already shipped.

## Context spent

- Tokens read before the first edit (cold start): the card + dispatch block (~1.3k), the
  cold-start capsule §VI.0 (~2.5k), `attempt_lists.rs` whole (~2.9k), the four repo-method
  sites in `tack-db/src/repo/execution.rs` (~1.8k, read in two passes — the surrounding
  `get_execution_artifact`/`list_execution_artifacts_for_attempt_number`/
  `list_execution_decisions_for_attempt_number` context was read too, to distinguish this
  card's two sites from VI-C6's two), the download handler
  `runner_protocol/artifact_download.rs` whole (~1.1k, needed for its auth mechanism and
  response shape — not on the dispatch's own read-list, but load-bearing: the route's
  success case is a byte stream, not JSON, which drove the whole test design), the events
  handler's mount in `executions.rs` (~0.2k, grep only), `operator_read_routes.rs`'s existing
  events-route test (~1.8k, to confirm claim-only — no accept/start — is sufficient for
  reporting events) and `wiring/artifact.rs` whole (~2.2k, the closest existing precedent for
  driving the download route through the real production router with a configured
  `storage_dir`) — roughly 14k tokens, above the dispatch's ~9k estimate; the download route's
  own file and its closest precedent were the overrun, both necessary since the dispatch's
  "do not read" list excluded the frontend/runner-protocol/other-handoff surfaces but the
  download handler itself is neither.
- Context size at handoff: well under the 150k stop threshold.
- Files opened and not used: none — every file read fed directly into either the test design
  or this handoff.
- Read-list lines that were wrong: the dispatch's own grep recipe for finding "the two repo
  methods... and the handlers that call them" undercounted by one hop for the download route —
  `grep -n "attempt_number}/artifacts/{artifact_id}/content" router.rs` finds nothing, because
  `artifact_download::routes(...)` is mounted through `operator_execution_routes` in
  `router.rs`, not matched by path string (the route path itself lives in
  `artifact_download.rs::routes`, not `router.rs`); the working recipe was
  `grep -rn "get_execution_artifact_by_attempt_number" crates/tack-api/src/` directly. Worth
  noting for whoever writes the next dispatch touching this route. Also confirmed: the
  worktree's starting `HEAD` mismatch VI-C6 already flagged recurred for this card too — still
  worth fixing at the dispatch/worktree-provisioning level rather than re-discovering per card.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
