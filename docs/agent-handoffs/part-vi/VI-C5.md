# VI-C5 handoff

- Base SHA / branch / final SHA: `605b649` (develop) / `agent/vi-c5-attempt-list-tests` / not committed — working tree only, per instruction not to commit.
- Files changed (must equal ownership list): `crates/tack-api/tests/handlers/attempt_lists.rs` (new), `crates/tack-api/tests/handlers.rs` (wired the new module + updated its module doc comment), `docs/agent-handoffs/part-vi/VI-C5.md` (this file). Matches the card's `Owns` line exactly — no production file touched (`git diff --stat` outside `crates/tack-api/tests/` and this handoff is empty).
- Contract fixtures consumed: none. These two routes carry no `docs/contracts/runner-v1/` fixture of their own (they are operator-facing reads, not runner-v1 protocol traffic); `runner_contract` and `openapi_contract` were run to confirm this card's changes leave both byte-identical, not because either route is fixture-pinned.
- Behavior implemented: no new behavior — six handler tests for the two routes VI-C4 shipped without one (`GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts` and `.../decisions`, both in `crates/tack-api/src/handlers/attempt_lists.rs`).
- Tests added and exact commands/results:
  - `attempt_artifacts_requires_operator_auth_and_leaks_nothing_without_it`
  - `attempt_artifacts_is_empty_before_any_manifest`
  - `attempt_artifacts_are_returned_oldest_first`
  - `attempt_decisions_requires_operator_auth_and_leaks_nothing_without_it`
  - `attempt_decisions_is_empty_before_any_decision_raised`
  - `attempt_decisions_are_returned_oldest_first`
  - `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C5 cargo nextest run --workspace` → `1420 tests run: 1420 passed, 7 skipped`.
  - `cargo nextest run --workspace -E 'binary(runner_contract)'` → `18 tests run: 18 passed, 0 skipped`.
  - `cargo nextest run --workspace -E 'binary(openapi_contract)'` → `5 tests run: 5 passed, 0 skipped`.
  - `cargo clippy --workspace --all-targets -- -D warnings` → clean. `cargo fmt --all --check` → clean. `./scripts/check-comments.sh` and `./scripts/check-test-hygiene.sh` → both clean.
- Failure/adversarial case proved: see the Claim → evidence table below — each of the six tests was proven load-bearing by reverting the exact behavior it asserts, in isolation, then restoring the file.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: the two routes' `404` behavior for an unknown `request_id`/`attempt_number` is not covered here — the card's Acceptance names three properties only (auth rejection, empty case, ordering), and the sibling file `operator_read_routes.rs` already establishes the `404` pattern for the twin `/attempts` and `/attempts/{n}/events` routes, so adding it here would be scope the card did not ask for.
- Secrets/logging review: no new logging added; the two handlers already redact `content_reference` (`content_verified: bool` only) and these tests don't touch that path.
- Safe merge order and likely conflicts: touches only `crates/tack-api/tests/handlers.rs` and a new file under `crates/tack-api/tests/handlers/` — no other Wave 17 card owns this directory, so no conflict expected with VI-D1 or VI-D2.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry. Confirmed.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| An unauthenticated caller cannot read one attempt's artifact manifests, and no real artifact data leaks into the rejection response | `attempt_artifacts_requires_operator_auth_and_leaks_nothing_without_it`. Reverted `crates/tack-api/src/middleware.rs::require_token`'s `_ => Err(StatusCode::UNAUTHORIZED)` arm to `_ => Ok(next.run(req).await)`; rerun showed `left: 200 right: 401` with the seeded artifact (`auth-check-artifact`) present in the body. Reverted back. |
| An unauthenticated caller cannot read one attempt's decisions, and no real decision data leaks into the rejection response | `attempt_decisions_requires_operator_auth_and_leaks_nothing_without_it`. Same revert as above (shared `require_token` gate), rerun in the same pass: `left: 200 right: 401` with `auth-check-decision` present in the body. Reverted back. |
| A claimed attempt that has manifested no artifacts yet returns `200` with `data: []`, not a `404` | `attempt_artifacts_is_empty_before_any_manifest`. Reverted `handlers/attempt_lists.rs::list_execution_attempt_artifacts` to `artifacts.filter(\|a\| !a.is_empty())` before the `Some`/`None` split (collapsing "empty" into "not found"); rerun failed with `left: 404 right: 200`. Reverted back. |
| A claimed attempt that has raised no decisions yet returns `200` with `data: []`, not a `404` | `attempt_decisions_is_empty_before_any_decision_raised`. Same technique applied to `list_execution_attempt_decisions`'s `decisions` binding; rerun failed with `left: 404 right: 200`. Reverted back. |
| Artifacts for one attempt come back oldest-first (by `created_at`), not by insertion/rowid order | `attempt_artifacts_are_returned_oldest_first`. Reverted `tack-db/src/repo/execution.rs::list_execution_artifacts_for_attempt_number`'s `ORDER BY created_at` to `ORDER BY created_at DESC`; rerun failed with `left: "art-newer" right: "art-older"`. Reverted back. |
| Decisions for one attempt come back oldest-first (by `created_at`) | `attempt_decisions_are_returned_oldest_first`. Same technique applied to `list_execution_decisions_for_attempt_number`; rerun failed with `left: "dec-newer" right: "dec-older"`. Reverted back. |

## Measured numbers

- Full workspace suite: `1420 tests run: 1420 passed, 7 skipped` (`cargo nextest run --workspace`, ~15–30s depending on cache warmth).
- `runner_contract`: `18 tests run: 18 passed, 0 skipped`.
- `openapi_contract`: `5 tests run: 5 passed, 0 skipped`.
- `git diff --stat` outside `crates/tack-api/tests/` and this handoff: empty (confirmed twice — once before the revert-proof pass, once after restoring every temporary revert).

## What a stranger still cannot do

Nothing changes for a user of the product itself — both routes already worked before this
card (VI-C4 shipped them). What a stranger arriving at this crate's test suite could not
previously do is find, at the Rust level, a test that fails if either route's auth gate,
empty-vs-not-found split, or ordering guarantee regresses. That gap is now closed for these
two routes specifically; it was never a gap in the frontend or OpenAPI contract coverage.

## Context spent

- Tokens read before the first edit (cold start): read TODO.md header (~1k), the VI-C4
  amendment (~0.3k), `handlers/attempt_lists.rs` (~2.9k), the two repo methods in
  `tack-db/src/repo/execution.rs` (~1.5k), `lifecycle-transitions.json` grep (~0.3k),
  `operator_read_routes.rs` in full as the pattern to follow (~3.5k), `handlers.rs` (~0.2k),
  and `wiring/artifact.rs` in part for the claim/accept/start request-body shapes (~2k) —
  roughly 12k tokens against the dispatch instruction's own reading list, plus one detour
  (see below).
- Context size at handoff: well under the 150k stop threshold.
- Files opened and not used: `crates/tack-api/tests/runner_protocol/artifact_events.rs` and
  `crates/tack-api/src/handlers/runner_protocol.rs`'s `submit_artifacts`/`create_decision`
  bodies were read while scoping how to seed fixture rows, then not used directly — the
  chosen approach (direct `sqlx::query` insert, matching `operator_read_routes.rs`'s own
  "insert directly to keep the fixture focused" precedent) needed only the two tables'
  column lists from `tack-db/src/migrations.rs`, not the write handlers' validation logic.
  Not wasted exactly (it ruled out calling the real write handlers, since decisions require
  the attempt to be `running`/`waiting_decision`, which would have meant an unnecessary
  `accept`+`start` round trip for a read-only test), but worth flagging for a future
  cold-start estimate.
- Read-list lines that were wrong: none — the dispatch instruction's read list was accurate
  and sufficient; the extra reads above were this agent's own choice while designing the
  fixture-seeding approach, not a gap in the instruction.
- One environment note for whoever resumes this worktree: the shared checkout at
  `/home/ox/Sites/objetivosMios` and this worktree currently hold byte-identical source
  under `crates/`, confirmed by direct diff — but that is a snapshot-in-time fact, not a
  standing guarantee. All edits in this session were made through the worktree path.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
