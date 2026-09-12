# IX-M4-tack-api-runner_vertical_slice handoff

- Base SHA / branch / final SHA: `ddd9f88` / `agent/ix-m4-api-runner_vertical_slice` / `85733a5`
- Files changed (must equal ownership list): `crates/tack-api/tests/runner_vertical_slice/repository_crash.rs`
  (the only file under this card's ownership that needed a change;
  `crates/tack-api/tests/runner_vertical_slice.rs` was read and left untouched — already an
  8-line preamble plus two `mod` declarations, no budget it breaks).
- Contract fixtures consumed: none (`docs/contracts/runner-v1/` not touched).
- Behavior implemented: none — this card prunes tests; no file under `crates/tack-api/src/**`
  was touched.
- Tests added and exact commands/results: none added as a *feature* proof; test count moved
  7 -> 10 purely from splitting three over-budget tests, each split preserving every original
  assertion. `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-api-runner_vertical_slice cargo nextest
  run --workspace -E 'binary(runner_vertical_slice)'` -> 10 tests run, 10 passed, 0 skipped.
- Failure/adversarial case proved: n/a (no new behavior; all seven original crash/fault-
  injection claims are unchanged, see *Claim -> evidence*).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none. Every test in the file is now inside all
  budgets (max body 55, max name 58, preamble 0 — the file has no `//!` block).
- Secrets/logging review: n/a (test-only change, no logging or secret paths touched).
- Safe merge order and likely conflicts: independent of every other IX-M4 sub-card (own file,
  own crate/binary already isolated by the ownership list). No conflicts expected against
  `develop`: only `crates/tack-api/tests/runner_vertical_slice/repository_crash.rs` was
  written.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim -> evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| All seven original crash/fault-injection claims still hold, split rather than weakened | `cargo nextest run --workspace -E 'binary(runner_vertical_slice)'` — 10/10 pass; see *What was removed* for the exact old-name -> new-name(s) mapping and confirmation every assertion moved intact |
| Every test name in this file is <=60 chars, no articles | `python3 -c "..."` length check during authoring (max 58) and `python3 scripts/maintainability.py measure crates/tack-api/tests/runner_vertical_slice/repository_crash.rs` — `name` column reads 58 |
| Every test body is <=60 lines (target 40) | same `measure` command — `max` column reads 55, `avg` reads 34 |
| No fixed waits in this binary | `grep -n "sleep(" crates/tack-api/tests/runner_vertical_slice/repository_crash.rs` — no hits, confirmed before and after |
| `cargo fmt`, clippy, comment and test-hygiene gates all pass on the final tree | see *Budget check* below for the verbatim `check --changed` output; `cargo fmt --all -- --check` (root and `crates/tack-desktop`), `cargo clippy --workspace --all-targets -- -D warnings`, `bash scripts/check-comments.sh`, `bash scripts/check-test-hygiene.sh` all exit 0 |

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree:

```
✓ maintainability budgets hold (1 files checked)
```

`python3 scripts/maintainability.py measure --totals` before and after this card (workspace-wide;
"before" measured by swapping the pre-edit file back in via `git show
ddd9f88:crates/tack-api/tests/runner_vertical_slice/repository_crash.rs`, measuring, then
`git checkout --` to restore the committed file — this card touches one file, so the entire
totals delta below is this file's):

Before:
```
totals: prod=56223 (comments 11931) test=71141 ratio=1.265 tests=1440 sleeps_in_tests=40 env_gated=0
```

After:
```
totals: prod=56223 (comments 11931) test=71163 ratio=1.266 tests=1443 sleeps_in_tests=40 env_gated=0
```

`python3 scripts/maintainability.py measure crates/tack-api/tests/runner_vertical_slice*` before and after:

Before:
```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-api/tests/runner_vertical_slice/repository_crash.rs      0     0   0%    719    7    73  110   80    0
crates/tack-api/tests/runner_vertical_slice.rs                  0     0   0%     14    0     0    0    0    8
totals: prod=0 (comments 0) test=733 ratio=0.0 tests=7 sleeps_in_tests=0 env_gated=0
```

After:
```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-api/tests/runner_vertical_slice/repository_crash.rs      0     0   0%    741   10    34   55   58    0
crates/tack-api/tests/runner_vertical_slice.rs                  0     0   0%     14    0     0    0    0    8
totals: prod=0 (comments 0) test=755 ratio=0.0 tests=10 sleeps_in_tests=0 env_gated=0
```

Net: 733 -> 755 test lines (+22, +3.0%, from the eight new file-local helper functions, which
are not test bodies themselves and so are not counted by `avg`/`max`); 7 -> 10 tests; file max
body 110 -> 55 (was over the 60-line hard cap by 50 lines, twice: also 66 in the completion test
and 66 in the enrollment test, not visible in the file-level `max` column but confirmed
individually before editing via a small script against `maintainability.tests_in`); file max
name 80 -> 58 (six of seven names were over 60 chars); `sleeps_in_tests` 0 -> 0 throughout
(confirmed by direct `grep` both before and after, per §2.2 rule 8 — nothing to remove).
This card's per-card budget (15 new tests / 600 new test lines, §2.3) is not exceeded: net +3
tests, +22 lines.

## What was removed

Per §2.2 rule number (`1: variants -> rows`, `2: name`, `3: body`, `6: preamble`,
`8: fixed wait`):

- **`3` + `2`**: `crash_during_event_batch_rolls_back_rows_and_checkpoint_then_replays_once`
  (108-line body, 73-char name; actually two unrelated claims — "a crash at either of two
  injection points leaves no partial write" and "a successful batch is replayed idempotently"
  — plus two duplicated crash-injection blocks that differed only in which trigger was
  installed) split into `event_batch_fault_leaves_no_partial_write` (41 chars; itself
  table-driven per rule `1` over the two crash-point cases — event-row insert and checkpoint
  update — via a small `EventBatchFault` table and one `assert_event_batch_fault_rolls_back`
  helper, so the two near-identical crash blocks became one loop body instead of two copies)
  and `event_batch_replay_writes_events_exactly_once` (45 chars, the untouched normal-path
  apply-then-replay assertions). Extracted `crash_events()` and `crash_event_batch()` (each
  used by both new tests) and `event_checkpoint_state()` (used by both crash-point cases).
- **`3` + `2`**: `crash_during_completion_rolls_back_attempt_and_request_then_replays_once`
  (66-line body, 72-char name; over the hard cap and itself two claims — rollback-on-crash and
  replay-idempotency — that did not need to share a fixture, since each gets its own in-memory
  db from `fixture()`) split into `completion_crash_rolls_back_attempt_and_request` (47 chars)
  and `completion_replay_is_idempotent_and_terminal_once` (49 chars); the second no longer
  installs and drops a trigger it never needed. Extracted `completion_crash()` (used by both).
- **`3` + `2`**: `heartbeat_fault_rolls_back_then_replays_the_authoritative_response_once`
  (93-line body, 71-char name; three claims — crash rolls back and writes no replay row,
  successful heartbeat replay is idempotent, and a heartbeat with a new observed timestamp both
  conflicts and never double-writes capacity) split into
  `heartbeat_crash_rolls_back_and_writes_no_replay_row` (51 chars) and
  `heartbeat_replay_is_idempotent_then_conflicts_on_new_state` (58 chars, the latter two claims
  kept together since the capacity assertion is only meaningful against the same replay/conflict
  sequence it currently follows). Extracted `crash_heartbeat()` and `crash_heartbeat_lease()`
  (used by both new tests); `crash_heartbeat` also collapses the underlying 7-argument
  `heartbeat_batch` call to a 2-argument call at every test call site, which is what kept the
  split bodies from re-exploding under rustfmt's one-argument-per-line rule for 7+-argument
  calls (the sibling `ix-m4-db-repository` card's known gotcha, confirmed here too during a
  scratch attempt with the call written out inline).
- **`2` only** (name, no structural change — body already under the hard cap):
  `crash_before_claim_commit_rolls_back_request_capacity_and_fence` (63 chars) ->
  `claim_crash_rolls_back_then_retry_keeps_fence_at_one` (52 chars);
  `post_spawn_recovery_audits_needs_operator_and_never_grants_a_second_fence` (73 chars) ->
  `ambiguous_recovery_needs_operator_and_blocks_reclaim` (52 chars);
  `crash_during_cancellation_request_is_retryable_without_false_terminal_state` (75 chars) ->
  `cancellation_crash_retries_without_false_terminal_state` (55 chars);
  `enrollment_redemption_has_one_concurrent_winner_and_consumes_the_hash_only_token` (80 chars)
  -> `enrollment_redeem_has_one_winner_and_keeps_hash_only_token` (58 chars). The first and last
  of these four also gained a small helper (`try_claim()` for the claim test, reused by the
  ambiguous-recovery test's second claim attempt too; `pending_enroll_runner()` /
  `pending_enroll_token()` for the enrollment test) purely to offset the line cost of the
  longer descriptive names and keep bodies well clear of the cap, not because either body was
  over budget before renaming.
- No `6` (preamble): the file has no `//!` block to begin with, and none was added.
- No `8` (fixed wait): confirmed via `grep -n "sleep("` before starting, and again on the
  final file — zero hits both times.
- No `5` (third-layer duplicate): `python3 scripts/maintainability.py duplicate-tests` was run
  before editing and none of this file's seven original names appeared in its `tack-api`
  output; the file's own preamble already documents where the router-layer counterpart lives
  (`handlers/production_router.rs`, `wave2_gate.rs`), so this file stays the one repository-
  layer pinning and nothing here was a candidate for cutting.

## Re-baselined?

`no`. This card brought one file further under budget than its baseline entry required (the
ratchet only demands "not worse"); `scripts/maintainability-baseline.json` is untouched —
`git diff scripts/maintainability-baseline.json` is empty.

## Context spent

- Tokens read before the first edit (cold start): read the card's own two files in full
  (719 + 14 lines), `docs/plans/human-maintainability.md` §2.1-2.2 (~70 lines), and
  `crates/tack-api/tests/common/mod.rs` in full (356 lines) to confirm no existing helper
  duplicated what this card needed to add. No estimate was given in the dispatch to compare
  against.
- Context size at handoff: single-file card, no sub-agents spawned, well under half the
  session's budget.
- Files opened and not used: none — every file read (`common/mod.rs`, the plan doc, the
  sibling `IX-M4-tack-db-repository.md` handoff for precedent on rename/split style) informed
  a decision actually made.
- Read-list lines that were wrong: none; the card's own line/test/name counts for
  `repository_crash.rs` (719 lines, 7 tests, max body 110, max name 80) matched
  `scripts/maintainability.py measure`'s output exactly.

## Amendments

*(none yet)*
