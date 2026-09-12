# IX-M4-tack-db-repository handoff

- Base SHA / branch / final SHA: `9433ef9` / `agent/ix-m4-db-repository` / `b672a9f`
- Files changed (must equal ownership list): `crates/tack-db/tests/repository.rs` (untouched,
  only read), `crates/tack-db/tests/repository/version_concurrency.rs`,
  `status_update_checked.rs`, `execution_retention.rs`, `event_artifact_retention.rs`,
  `orch_repo.rs`, `integration.rs`, `execution_repo.rs` — exactly this card's one test binary.
- Contract fixtures consumed: none (`docs/contracts/runner-v1/` not touched).
- Behavior implemented: none — this card prunes tests, no production file under
  `crates/tack-db/src/**` was touched.
- Tests added and exact commands/results: none added net-new as a *feature* proof; test count
  moved 136 -> 137 (see *Measured numbers*) purely from splitting/merging existing coverage.
  `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-db-repo cargo nextest run --workspace -E
  'binary(repository)'` — 137 tests run, 137 passed, 0 skipped (verified after every file's
  commit, and again at the end; the modified concurrency test
  `artifact_and_decision_reject_concurrently_terminal_attempt` also re-run 5x standalone plus
  3x as part of the full binary to rule out flakiness from the sleep replacement).
- Failure/adversarial case proved: n/a (no new behavior).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: several bodies in `execution_repo.rs` remain over
  the 60-line hard cap (see *What was removed* and the note below); this was not worsened,
  only some individually shrunk.
- Secrets/logging review: n/a (test-only changes, no logging or secret paths touched).
- Safe merge order and likely conflicts: independent of the sibling `tack-api`
  `runner_protocol` card (different crate, different binary). No conflicts expected against
  `develop` since only `crates/tack-db/tests/repository/**` and the one root file were read;
  nothing else was touched (confirmed by reverting an accidental `docs/adr/0064-fixed-waits.txt`
  regeneration — see *Amendments*-style note in *What was removed*, item 8).
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| All behavior in the seven files is unchanged | `cargo nextest run --workspace -E 'binary(repository)'` — 137/137 pass, same assertions per case as before (see *What was removed* for exact case-to-case mapping on every split/merge) |
| The one fixed wait in this binary is gone, replaced by a real state check | `grep -n 'sleep(' crates/tack-db/tests/repository/*.rs` — no hits; `artifact_and_decision_reject_concurrently_terminal_attempt` re-run 8x total (5x isolated + 3x in the full binary), all pass |
| Every test name in this binary is ≤60 chars, no articles/narrative | `grep -oP '(?<=^async fn )\w+|(?<=^fn )\w+' crates/tack-db/tests/repository/*.rs \| awk '{ if (length($0) > 60) print }'` — empty |
| Every file's preamble is ≤10 lines | inspected each file's leading `//!`/`//` block by hand; `orch_repo.rs` (11→6), `version_concurrency.rs` (11→5) were the only two with a preamble at all and both were trimmed |

## Measured numbers

`CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-db-repo python3 scripts/maintainability.py measure crates/tack-db/tests/repository*`

Before (measured at card start, 2026-09-11/12):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-db/tests/repository/execution_repo.rs               0     0   0%   4323   61    64  174   79    0
crates/tack-db/tests/repository/integration.rs                  0     0   0%   1400   27    48   88   46    0
crates/tack-db/tests/repository/orch_repo.rs                    0     0   0%   1012   22    41   79   87   11
crates/tack-db/tests/repository/event_artifact_retention.rs      0     0   0%    996   11    63  101   82    5
crates/tack-db/tests/repository/execution_retention.rs          0     0   0%    650    6    57  138   87   13
crates/tack-db/tests/repository/status_update_checked.rs        0     0   0%    224    5    39   70   81    9
crates/tack-db/tests/repository/version_concurrency.rs          0     0   0%    176    4    37   98   69   11
crates/tack-db/tests/repository.rs                              0     0   0%     24    0     0    0    0    6
totals: prod=0 (comments 0) test=8805 ratio=0.0 tests=136 sleeps_in_tests=1 env_gated=0
```

After (this handoff):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-db/tests/repository/execution_repo.rs               0     0   0%   4308   61    65  174   60    0
crates/tack-db/tests/repository/integration.rs                  0     0   0%   1285   27    44   71   41    0
crates/tack-db/tests/repository/orch_repo.rs                    0     0   0%   1007   22    41   79   56    6
crates/tack-db/tests/repository/event_artifact_retention.rs      0     0   0%    912    7    53   77   57    5
crates/tack-db/tests/repository/execution_retention.rs          0     0   0%    679    9    40   56   56    7
crates/tack-db/tests/repository/status_update_checked.rs        0     0   0%    224    5    39   70   48    9
crates/tack-db/tests/repository/version_concurrency.rs          0     0   0%    181    6    26   59   56    5
crates/tack-db/tests/repository.rs                              0     0   0%     24    0     0    0    0    6
totals: prod=0 (comments 0) test=8620 ratio=0.0 tests=137 sleeps_in_tests=0 env_gated=0
```

Net: 8805 -> 8620 test lines (-185, -2.1%); 136 -> 137 tests; every file's `name` max ≤60
(was up to 87); `sleeps_in_tests` 1 -> 0. Test count *before* cross-checked against the sum of
each file's `#t` column both before (61+27+22+11+6+5+4=136) and after
(61+27+22+7+9+5+6=137) — matches the nextest run counts observed at every commit
(138 after file 1's +2, 138 after file 2, 141 after file 3's +3, 137 after file 4's -4, 137
after file 5 (rename-only), 137 after file 6 (rename+shrink-only), 137 after file 7
(rename+sleep-fix-only)).

## What was removed

Per-file, with the §2.2 rule number for each change (`1: variants → rows`, `2: name`,
`3: body`, `6: preamble`, `8: fixed wait`):

**`version_concurrency.rs`** — `1`: `version_increments_on_every_item_update` (99-line body,
three unrelated write-path claims) split into `update_item_bumps_version`,
`update_item_status_checked_bumps_version`, `check_and_update_parent_status_bumps_version`
(4 tests → 6). `2`: `a_freshly_created_item_starts_at_version_one` →
`fresh_item_starts_at_version_one`; `claim_item_version_succeeds_exactly_once_for_a_given_expected_version`
(71 chars) → `claim_item_version_succeeds_once_and_rejects_stale_reuse`;
`claim_item_version_against_an_unknown_item_returns_false_not_an_error` (70 chars, articles) →
`claim_version_on_unknown_item_returns_false`. `6`: preamble 11 → 5 lines.

**`status_update_checked.rs`** — `2` only: all five names renamed to drop articles
(`applies_the_transition_when_under_the_limit` → `applies_transition_under_limit`, etc.); no
structural change, already one claim per test and under the body/preamble budgets.

**`execution_retention.rs`** — `1`: `execution_fleet_snapshot_reports_bounded_id_free_counts`
(137-line body, five unrelated claims: runner state counts, request state counts, needs-operator
age, stale-lease count, events-in-window) split into `runner_state_counts_report_all_three_states`,
`request_state_counts_include_needs_operator_and_queued`,
`stale_lease_count_excludes_terminal_attempts`,
`events_ingested_in_window_counts_only_recent_events` (6 tests → 9). `2`: remaining four names
shortened/de-articled (`purge_stale_execution_replays_deletes_only_rows_older_than_cutoff_across_all_six_tables`
→ `purge_replays_deletes_only_rows_older_than_cutoff`, etc.). `6`: preamble 13 → 7 lines.

**`event_artifact_retention.rs`** — `1`: the two checkpoint-failure tests
(`event_batch_insert_failure_leaves_checkpoint_and_rows_untouched`,
`event_batch_insert_failure_on_a_fresh_attempt_leaves_checkpoint_null`) merged into one
table-driven `failed_event_batch_leaves_checkpoint_and_rows_untouched` with a
`CheckpointFailureCase` table (both cases' exact trigger condition, seed state and expected
checkpoint preserved); the three artifact-delete-guard race tests
(`delete_unresolved_execution_artifacts_by_row_ids_skips_a_row_resolved_concurrently`,
`unconditional_delete_would_have_orphaned_the_racily_resolved_row`,
`delete_unresolved_execution_artifacts_by_row_ids_deletes_when_nothing_raced`) merged into
`artifact_delete_guard_skips_only_racily_resolved_row` with a `GuardCase` table (each case's
race/no-race, guarded/unconditional choice and expected deleted-row count preserved exactly,
plus the two original per-case follow-up assertions — content_reference survives when the row
survives, remaining-row-count otherwise); the two empty-list no-op tests merged into
`delete_by_row_ids_is_a_no_op_on_empty_list` (11 tests → 7). `3`: both merges' per-case logic
(`run_checkpoint_failure_case`, `run_guard_case`) extracted into plain (non-`#[test]`) async
helpers so the `#[tokio::test]` bodies are only the case table, the loop and the assertions —
without this the merged bodies measured 127 and 136 lines (worse than this file's 101-line
baseline and would have failed `check --changed`); after extraction the file's max body is 77.
Added `patch_artifact()`, `backdate_artifact()`, `checkpoint_for()`, `event_count_for()` local
helpers (each used 2+ times in this file) to cut repeated `NewArtifact`/SQL boilerplate. `6`:
the ~21-line `//` block ahead of the merged guard test trimmed to a 9-line `///` doc comment.

**`orch_repo.rs`** — `2` only: dropped the redundant `test_` prefix from all 22 names (every
name in the file already stated its claim without it), then shortened/de-articled the five that
were still over 60 chars or carried "its"/"an"/"the" after the prefix drop
(`upsert_orch_runs_batch_is_idempotent_and_supports_unattributed_runs` →
`orch_runs_upsert_idempotent_keeps_unattributed_runs`, etc.). `6`: preamble 11 → 6 lines. No
`1` or `3`: every test here already covered one distinct CRUD/upsert-idempotency claim and every
body was already at or under this file's 79-line baseline max.

**`integration.rs`** — `2`: dropped the `test_` prefix from all 27 names (none needed further
shortening; longest was already 46 chars). `3`: added a `text_field()` local builder (used 3x)
and switched `custom_field_value_upsert`, `custom_field_cascade_delete`,
`create_and_list_custom_fields` and `create_and_list_items` from hand-rolled
`CreateProject`/`CreateItem` literals to the shared `common::make_project`/`make_item` helpers
(already exported by `tack-test-support` from IX-M3-db) — the swapped project name/item
type/priority fields were not asserted by any of these tests, so behavior is identical. File max
body 88 → 71 lines; the two worst offenders (89, 79 lines) dropped to well under 60.

**`execution_repo.rs`** (the big one) — `2`: 28 of 61 names were over 60 chars (this file
predates the budget); all renamed, e.g.
`attempt_start_transition_rejects_wrong_order_and_stale_authority_without_writes` (79 chars) →
`attempt_start_rejects_wrong_order_and_stale_authority` (54). `8`: the file's one fixed wait —
`tokio::time::sleep(Duration::from_millis(150))` racing two writers against a held
`BEGIN IMMEDIATE` in `artifact_and_decision_reject_concurrently_terminal_attempt` — replaced
with a bounded poll on `repo.pool().num_idle()`: it waits for the real, observable fact that
both writer futures have each checked a connection out of the pool (meaning each has begun its
own `BEGIN IMMEDIATE` and is blocked on the lock) rather than guessing a duration long enough,
bounded by a 2-second deadline and yielding via `tokio::task::yield_now()` (no `sleep(` literal
remains in the binary). In practice the condition is already true the first time it's checked
(`tokio::join!` polls the writer futures before the poll loop), so this is also faster than the
150ms it replaced — verified stable across 5 isolated runs plus 3 full-binary runs. A comment
budget fix that also happened to free the line budget the poll code spent: the 38-line `//`
design-rationale block on the same test trimmed to 14 lines (kept: what breaks, why a
file-backed DB is required, why racing the futures alone isn't sufficient; dropped: the
"confirmed empirically... a first version... passed 20/20" historical narrative, per CLAUDE.md's
comment rule).

  Explicitly considered and rejected for `1`: the eight
  `concurrent_duplicate_*_have_one_*_writer` tests (completion reports, operator requeues,
  transition reports, heartbeats, recovery observations, cancellation observations, enqueues,
  event batches) share one *shape* (race two identical calls, assert exactly one authoritative
  writer) but each proves the invariant for a *different* production write path — eight
  distinct claims, not eight variants of one claim. `duplicate-tests` does not flag them as
  near-identical (it compares text similarity, not structural shape), and merging them into one
  table would require boxing futures of eight different types behind `dyn Future` for no size
  win (each case's setup is already almost entirely non-shared) at real risk to eight
  concurrency-correctness tests that are each load-bearing for a specific production method.
  Left as eight separately named tests. Also left as-is: `m060_quarantines_all_nonterminal_malformed_legacy_snapshots`
  (175 lines) — a single genuine multi-stage migration-checkpoint proof, not a variant family;
  splitting it would mean re-running the same expensive multi-migration setup per case for no
  behavioral gain. Several sequential state-transition tests elsewhere in this file and in
  `integration.rs` (`attempt_start_transitions_are_idempotent_and_freeze_facts`,
  `status_transition_timestamps`, `update_item_persists_and_clears_sprint_id`) were left
  similarly unsplit for the same reason — each step's assertion depends on the previous step's
  committed state, so splitting would either duplicate setup or lose the "and it stays true
  across the next transition too" claim.

  **Item 8 addendum (regeneration, reverted):** ran
  `python3 scripts/list-fixed-waits.py > docs/adr/0064-fixed-waits.txt` per the sub-card's
  instruction after the sleep fix. The diff showed only unrelated drift (line-number shifts and
  entry changes in `tack-orch`/`tack-api`/`tack-cli` files from other work already on `develop`
  since this file was last generated) — zero `tack-db` entries in either version, confirming
  this file's one sleep was never in the ≥200ms inventory to begin with. Committing the
  regeneration would have put out-of-scope, unrelated changes in this card's diff, so it was
  reverted (`git checkout -- docs/adr/0064-fixed-waits.txt`) and the file was left untouched.

**Rule 5 (third-layer over-pinning):** no removal made under this rule in any file. Two files'
own preambles already document a deliberate one-layer split
(`event_artifact_retention.rs` says HTTP-level artifact-content tests live in
`tack-api/tests/runner_protocol/artifact_events.rs`; `status_update_checked.rs` says the
concurrent WIP-limit race lives in `tack-api/tests/security/wip_limit_race.rs`) — both already
correctly scoped to one layer, nothing to remove. No other invariant in this binary was found
provably duplicated at the router layer within the time available for this card; per the
sub-card's own instruction ("if unsure, leave it and note it in the handoff"), none were removed
on a guess.

## Re-baselined?

`no`. Nothing in `scripts/maintainability-baseline.json` was touched — every file's post-change
numbers are within (mostly well under) its existing baseline entry, so the ratchet holds without
a re-baseline: `python3 scripts/maintainability.py check crates/tack-db/tests/repository.rs
crates/tack-db/tests/repository/*.rs` → `✓ maintainability budgets hold (8 files checked)`.

## Budget check

`python3 scripts/maintainability.py check crates/tack-db/tests/repository.rs crates/tack-db/tests/repository/*.rs` (final tree; `--changed` alone reports 0 files since every change is already committed and the working tree is clean):

```
✓ maintainability budgets hold (8 files checked)
```

`python3 scripts/maintainability.py measure --totals` — before/after (workspace-wide; other
IX-M cards and the sibling `tack-api` card may also move this number, so only the *this-card's
files* numbers above are load-bearing for this handoff):

- Before this card's changes (not separately captured at session start; the cold-start capsule's
  workspace-wide baseline was "1 498 tests" / "74 014" test lines, but that predates this
  session's other closed cards too, so it is not a clean before/after pair for this card alone).
- After this card: `totals: prod=56223 (comments 11931) test=73147 ratio=1.301 tests=1499 sleeps_in_tests=73 env_gated=0`.

`python3 scripts/maintainability.py measure crates/tack-db/tests/repository*` before/after: see
*Measured numbers* above — this is the load-bearing before/after pair for this sub-card.

## What a stranger still cannot do

A stranger arriving from outside this repository still cannot look at `execution_repo.rs`'s
eight `concurrent_duplicate_*_have_one_*_writer` tests and immediately see, from names alone,
which eight production write paths they each cover without reading each body — the shared
naming convention groups them visually but the file doesn't index them. They also cannot yet
run any single command that proves the whole IX-M4 program's cross-binary invariant-layering
claim (rule 5) — that requires reading both this binary and its paired API-layer tests by hand,
per binary, which this card did only for the two invariants its own preambles already
documented.

## Context spent

- Tokens read before the first edit (cold start): read `TODO.md` §IX.0–§IX.3 and the IX-M4 card
  block (not the whole 199k-token file), `docs/plans/human-maintainability.md` §2.2/§5/Appendix A,
  and the two handoff templates — a few thousand tokens, in line with the block's own estimate.
- Context size at handoff: large — this is the single biggest IX-M4 sub-card by design (61 tests
  in one file alone) and was worked across a rate-limit interruption and resume.
- Files opened and not used: none beyond the read-list; `docs/adr/0064-fixed-waits.txt` was
  opened and regenerated but the regeneration was reverted (see *What was removed*, item 8
  addendum) rather than committed.
- Read-list lines that were wrong: none — the seven-file, smallest-first order in the dispatch
  prompt matched the actual measured sizes exactly.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*
