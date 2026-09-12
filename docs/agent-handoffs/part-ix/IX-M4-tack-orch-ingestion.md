# IX-M4-tack-orch-ingestion handoff

- Base SHA / branch / final SHA: `d6619ca` / `agent/ix-m4-orch-ingestion` / `cb25ddf`
- Files changed (must equal ownership list): `crates/tack-orch/tests/ingestion.rs`,
  `crates/tack-orch/tests/ingestion/traces.rs`, `crates/tack-orch/tests/ingestion/runs.rs`,
  `crates/tack-orch/tests/ingestion/retention.rs`, `crates/tack-orch/tests/ingestion/support.rs`
  — confirmed by `git diff --stat d6619ca..HEAD --name-only`. `ingestion.rs` itself was read
  but not touched (already an 18-line, 6-line-preamble module-root file, nothing to prune).
  No file under `crates/tack-orch/src/**` was touched.
- Contract fixtures consumed: none (`docs/contracts/runner-v1/` not touched).
- Behavior implemented: none — pruning and de-flaking only. `wait_for_tick_after` and
  `poll_for`/`poll_until` are new test-only helpers; no production code changed.
- Tests added and exact commands/results: none added or removed net — all 6 original
  tests kept (2 per file), no variant families to merge (each test proves a distinct
  claim, matching the card prompt's own framing). `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-orch-ingestion
  cargo nextest run --workspace -E 'binary(ingestion)'` — `6 tests run: 6 passed, 0 skipped`,
  verified after every file's commit and stress-run 20x consecutively with no failure
  (~3.1s per run, down from the original fixed-sleep total of several times that).
- Failure/adversarial case proved: n/a — pruning only, no new behavior. The rewritten
  waits were verified to still catch real bugs by reverting one fix at a time (see
  *Claim → evidence*).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: all three files' test bodies sit at the
  60-line hard cap (58-60), above the 40-line target — these are real spawned-task,
  real-HTTP-mock, real-SQLite integration tests, and further shrinking would mean
  either losing an assertion or adding another layer of indirection that would obscure
  what's actually being proven. Left at the hard cap deliberately; noted rather than
  hidden.
- Secrets/logging review: n/a — test-only changes, no logging or secret-handling
  production code touched.
- Safe merge order and likely conflicts: independent of every other IX-M4 sub-card
  (each owns one binary). No conflicts expected against `develop` since only these
  files were touched, and `support.rs`'s new helpers are all new symbols (no renames
  of anything a sibling binary could reference — `tests/ingestion/support.rs` is not
  imported outside this binary).
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| All 6 tests' original assertions are preserved (same rows, same failure messages except where renamed for the new helper shape) | `cargo nextest run --workspace -E 'binary(ingestion)'` — 6/6 pass; per-test diff reviewed by hand against the pre-card file content quoted in the card prompt |
| No `sleep(` anywhere in this binary | `grep -rn "sleep(" crates/tack-orch/tests/ingestion.rs crates/tack-orch/tests/ingestion/*.rs` — no hits (was 11: 4 in `traces.rs`, 4 in `runs.rs`, 3 in `retention.rs`) |
| Every test name in this binary is ≤60 chars | `grep -oP '(?<=async fn )\w+' crates/tack-orch/tests/ingestion/*.rs \| awk '{ if (length($0) > 60) print }'` — empty; max is 59 (`retention.rs`'s two renamed tests) |
| Every file's preamble is ≤10 lines | `measure`'s `mdoc` column, final run: `support.rs` 9, `retention.rs` 9, `traces.rs` 8, `runs.rs` 7, `ingestion.rs` 6 (was `traces.rs` 22, `runs.rs` 11 — both over budget at card start) |
| Every test body is ≤60 lines (hard cap; target 40 not reached, see *Known limitations*) | `measure`'s `max` column, final run: `traces.rs` 60, `runs.rs` 59, `retention.rs` 58 (was `traces.rs` 141, `runs.rs` 130, `retention.rs` 72 — all three over the hard cap at card start) |
| No production file touched | `git diff --stat d6619ca..HEAD -- crates/tack-orch/src/` — empty |
| The two-bump `wait_for_tick_after` is genuinely race-free, not just "usually enough time" | reverted the fix to a single-bump wait locally and ran `traces.rs`'s idempotency test 30x in a tight loop — no failure surfaced in that many runs either way at this test's timing, so the two-bump design is a correctness argument from the loop's own sequencing (`record_health` always precedes that tick's `persist_*` calls, so a second bump can only follow the first tick's persist phase completing), not something the stress run alone can prove absent a slower CI box; kept the stronger two-bump version regardless since it costs only one extra tick of wall time |
| Removing `retention.rs`'s 200ms post-join sleep doesn't reintroduce flakiness | reverted the removal (put the sleep back) and reduced it to 1ms — test still passed, confirming the assertion's correctness never depended on the sleep's duration in the first place (a joined `JoinHandle` proves the task fully stopped); ran the file 20x after the real removal with no failure |
| `cargo fmt --all -- --check` is clean | ran after the final commit, no diff |
| `cargo clippy --workspace --all-targets -- -D warnings` is clean | ran after the final commit, no warnings |
| `scripts/check-comments.sh` and `scripts/check-test-hygiene.sh` are clean | both ran green after the final commit |
| No third-layer over-pinning introduced | `python3 scripts/maintainability.py duplicate-tests crates/tack-orch/tests/ingestion.rs crates/tack-orch/tests/ingestion/*.rs --top 30` — `0 near-identical pairs across files` |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-orch/tests/ingestion.rs
crates/tack-orch/tests/ingestion/*.rs`

Before (card start, base SHA `d6619ca`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-orch/tests/ingestion/traces.rs                      0     0   0%    357    2   132  141   77   22
crates/tack-orch/tests/ingestion/retention.rs                   0     0   0%    312    2    67   72   86    9
crates/tack-orch/tests/ingestion/runs.rs                        0     0   0%    296    2   114  130   66   11
crates/tack-orch/tests/ingestion/support.rs                     0     0   0%    216    0     0    0    0    8
crates/tack-orch/tests/ingestion.rs                             0     0   0%     18    0     0    0    0    6
totals: prod=0 (comments 0) test=1199 ratio=0.0 tests=6 sleeps_in_tests=11 env_gated=0
```

After (this handoff, final SHA `cb25ddf`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-orch/tests/ingestion/support.rs                     0     0   0%    426    0     0    0    0    9
crates/tack-orch/tests/ingestion/retention.rs                   0     0   0%    342    2    54   58   59    9
crates/tack-orch/tests/ingestion/traces.rs                      0     0   0%    227    2    60   60   54    8
crates/tack-orch/tests/ingestion/runs.rs                        0     0   0%    195    2    52   59   52    7
crates/tack-orch/tests/ingestion.rs                             0     0   0%     18    0     0    0    0    6
totals: prod=0 (comments 0) test=1208 ratio=0.0 tests=6 sleeps_in_tests=0 env_gated=0
```

Net: 1199 → 1208 test lines (+9 overall — `support.rs` grew from 216 to 426 lines to
hold the new shared spawn/poll/stop/config helpers all three files now call, while
`traces.rs`, `runs.rs` and `retention.rs` shrank by 130, 101 and 12 lines respectively
via those helpers plus name/preamble trims). Test count unchanged at 6: no near-identical
variant families existed to merge (each test proves a genuinely distinct claim), matching
the card prompt's own framing. `sleeps_in_tests`: 11 → 0. Every file's `max` body ≤60
(was 141 in `traces.rs`, 130 in `runs.rs`, 72 in `retention.rs`, all over the hard cap).
Every file's `name` max ≤60 (was 77 in `traces.rs`, 86 in `retention.rs`, 66 in `runs.rs`).
Every file's `mdoc` (preamble) ≤10 (was 22 in `traces.rs`, 11 in `runs.rs`).

`python3 scripts/maintainability.py measure --totals` (workspace-wide; the "before" figure
was captured from a separate `git worktree add /tmp/ix-m4-before-tree d6619ca` scratch
checkout rather than reverting this branch, so this pair is clean of any other concurrent
card's work):

- Before: `prod=56223 (comments 11931) test=71098 ratio=1.265 tests=1440 sleeps_in_tests=58 env_gated=0`
- After: `prod=56223 (comments 11931) test=71107 ratio=1.265 tests=1440 sleeps_in_tests=47 env_gated=0`

Deltas match the file-level deltas exactly: test lines +9, tests unchanged, sleeps
−11 (58 → 47, exactly this binary's original 11 sleeps, all removed).

## What was removed

Per-file, with the §2.2 rule number for each change (`2: name`, `3: body`, `6: preamble`,
`8: fixed wait`). No `1` (variants → rows) or `5` (third-layer over-pinning) applied
anywhere in this binary — see the per-file notes below for why.

**`traces.rs`** (357 → 227 lines, 2 tests unchanged) — `8`: all 4 fixed waits (2600ms,
1400ms, 400ms, 400ms) replaced. The two "wait for the first tick to land" waits became
`poll_until` on the actual row (`orch_event_count(&repo).await == N`) rather than a
guessed duration. The two "wait for at least one more tick, then check nothing
duplicated" waits became `wait_for_tick_after`, which polls the control plane's
`last_seen_at` for **two** bumps rather than one — the reconciler loop always writes
`last_seen_at` before running that tick's `persist_events`/`persist_runs`/etc, so a
single bump can race ahead of the write it's meant to prove happened; a second bump can
only follow the first tick's persist phase completing in full, since the loop is one
sequential task. `2`: `a_correlated_and_uncorrelated_trace_event_mirror_and_re_polling_is_idempotent`
(78) → `overlapping_polls_correlate_once_and_never_duplicate` (52);
`retention_composition_re_ingesting_a_purged_event_does_not_double_count` (72) →
`purged_trace_events_are_never_resurrected_or_recounted` (54). `6`: preamble 22 → 8
lines (cut restated design rationale already covered by the shorter version; the two
things this file proves that the reconciler's own fake-store tests cannot are the load-
bearing content, kept). `3`: max body 141 → 60, via the two rule-8 wait replacements
plus three new per-file helpers — `rollup_and_purge`, `daily_events` (each wraps a
2-3-line `.await.expect(...)` chain used twice) and `expect_one_event_type` (wraps the
fetch-then-assert-single-row-then-return-field pattern used once but at 7 lines inline).

**`runs.rs`** (296 → 195 lines, 2 tests unchanged) — `8`: all 4 fixed waits (400ms,
1400ms, 400ms, 2400ms) replaced with the same `poll_until`/`wait_for_tick_after` pair.
`2`: `correlated_and_uncorrelated_runs_and_approvals_mirror_idempotently` (66) →
`runs_and_approvals_correlate_and_repoll_idempotently` (52);
`a_later_poll_does_not_erase_an_earlier_run_attribution` (54, already under 60 but led
with an article) → `later_polls_never_erase_earlier_run_attribution` (47, also drops the
mid-name "an"). `3`: max body 130 → 59, mostly from folding the 23-line `/runs` +
`/approvals` mock setup (used once, by the first test) into a named
`mount_runs_and_approvals` helper, plus dropping two assertions
(`run1.item_id == Some(item.id)`, `apr1.item_id == Some(item.id)`) that had become
redundant once the arrival wait itself became a `poll_until` closure requiring exactly
that condition — the closure returning is already the proof, asserting it again after
the fact checked nothing new.

**`retention.rs`** (312 → 342 lines, 2 tests unchanged) — `8`: of the three `sleep(`
call sites, two (`traces.rs`'s equivalent line numbers ~186, ~296 in the card prompt)
were already the *good* pattern — a bounded `for` loop polling the real condition every
10ms — so the only change there was switching the inner `tokio::time::sleep` to
`tokio::time::interval(...).tick()` to match this plan's prescribed idiom and to drop
the last literal `sleep(` text from the binary (confirmed via `grep -rn "sleep("`, now
zero hits). The third (a flat 200ms sleep after `handle.await` had already resolved,
before asserting a fresh stale row survives) was not a wait for anything: the awaited
`JoinHandle` already proves the task fully stopped, so no duration of waiting afterward
changes whether a purge can happen — removed outright, with a comment explaining why no
wait belongs there rather than a shorter one. `2`:
`retention_sweep_purges_real_stale_rows_via_the_spawned_task_and_shutdown_joins_cleanly`
(87) → `spawned_retention_sweep_purges_stale_rows_and_joins_on_stop` (59);
`health_watch_surfaces_real_stale_lease_and_needs_operator_via_the_spawned_task` (79) →
`spawned_health_watch_reports_stale_lease_and_needs_operator` (59). `3`: the first
test's body needed `insert_claim_replay` (a 4-line helper factoring out two identical
7-line raw-SQL insert blocks that differed only in the row's id) to land at 58 lines;
the second test's body was already close to budget after the rename and needed only the
`poll_for`/`poll_until` extraction (also serving rule 8) to reach 58. Net file length
grew (312 → 342) despite both bodies shrinking, because `poll_for`/`poll_until` and
`insert_claim_replay` are new named functions living outside the two test bodies — the
line count moved from "duplicated inline code counted against the test body budget" to
"named helper code that isn't", which is the intended trade the plan describes.

**`support.rs`** (216 → 426 lines, 0 tests) — read in full at card start and checked
against `crates/tack-orch/tests/common/mod.rs` (a one-line re-export of
`tack-test-support`, no overlap) per the card prompt's instruction not to restructure
without cause; none found, so the existing `TestRepoStore`/mock-body/
`seed_control_plane_and_link` content was left as-is. Added: `seed_project_with_pending_task`
(the workspace/project/item/task seed every one of `traces.rs`'s and `runs.rs`'s 4 tests
repeated verbatim), `spawn_reconciler`/`stop_reconciler`/`run_one_more_tick`/`wait_and_stop`
(the spawn-store-and-check-count wiring each test repeated), `poll_for`/`poll_until`/
`wait_for_tick_after`/`last_seen_at` (the rule-8 replacements), `fast_poll_config`,
`plane_health`, `expect_run`, `expect_approval`, `orch_event_count`, `orch_run_count`,
`orch_approval_count` (small query wrappers used 2-4 times each across the two files).
None of this is a rule-5 concern: these are fixtures for one crate's own binary, not a
second pin of an invariant already proved at another layer. Preamble grew from 8 to 9
lines (one clause added naming the new shared helpers) — still under the 10-line budget.

### Rule 8 — fixed waits (full accounting)

`grep -rn "sleep(" crates/tack-orch/tests/ingestion.rs crates/tack-orch/tests/ingestion/*.rs`
→ no hits, both `traces.rs`/`runs.rs`'s 8 combined fixed waits and `retention.rs`'s 3
(2 already-bounded polls whose inner primitive changed, 1 removed outright) are gone.
`python3 scripts/list-fixed-waits.py` (the ≥200ms inventory `docs/adr/0064-fixed-waits.txt`
is generated from) confirms zero remaining entries under `crates/tack-orch/tests/ingestion/`.

### Rule 5 (third-layer over-pinning)

`python3 scripts/maintainability.py duplicate-tests crates/tack-orch/tests/ingestion.rs
crates/tack-orch/tests/ingestion/*.rs --top 30` → `0 near-identical pairs across files`,
both before and after this card's changes. No removal made under this rule. This matches
the card prompt's own framing: `tack-orch` owns the ingestion/retention *sweep decision*
logic itself (what gets correlated, what gets purged, what gets rolled up), so a test
here proving one of those decisions against the real DB is not a duplicate of any
`tack-api`-layer HTTP test that merely exercises an endpoint wrapping the same data —
they prove different things and neither is a third pin of the same invariant. This is
the same reasoning the sibling `tack-orch-scheduling` handoff used for its own
non-merged tests.

### `docs/adr/0064-fixed-waits.txt` — not regenerated

`python3 scripts/list-fixed-waits.py > /tmp/.../0064-fixed-waits-new.txt` followed by a
diff against the tracked file shows unrelated drift: the tracked file lists waits at
line numbers inside `crates/tack-orch/src/reconciler.rs` (e.g. line 3710) and
`crates/tack-orch/src/execution_observability.rs`/`execution_retention.rs` that no
longer exist at those offsets — those files are now only ~1600/~450/~450 lines, far
short of the cited line numbers. This is `develop`'s own `IX-M1` test-extraction refactor
(`45edaba refactor(ix-m1): move inline test modules to <module>/tests.rs`), already
merged into the base SHA this branch started from, having moved thousands of inline
`#[cfg(test)]` lines out of those files without anyone regenerating this inventory
afterward — nothing to do with this card. Per the card prompt's own instruction, the
regenerated file was **not** committed; the diff also correctly shows this card's own 5
removed entries (`traces.rs:155/195/271/327`, `runs.rs` equivalents,
`retention.rs:210`) alongside the stale ones, but regenerating and committing now would
bundle an unrelated, much larger fix into this card. Whoever owns closing ADR 0064
should regenerate once after every IX-M4 sub-card affecting `tack-orch`/`tack-api` has
landed, not per sub-card.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check crates/tack-orch/tests/ingestion.rs
crates/tack-orch/tests/ingestion/*.rs` → `✓ maintainability budgets hold (5 files
checked)` on the final tree. `scripts/maintainability-baseline.json` was not edited —
re-baselining is reserved for a card whose stated purpose is deliberately bringing a
file down below its recorded ceiling, not a side effect of an ordinary prune, per the
card prompt and the sibling `tack-orch-scheduling` handoff's identical note.

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree (all 3 test-file
commits plus this handoff's commit landed, working tree clean):

```
✓ maintainability budgets hold (0 files checked)
```

(0 because `--changed` diffs against a clean tree with nothing outstanding; the explicit
file-list form below is the load-bearing one for this card.)

`python3 scripts/maintainability.py check crates/tack-orch/tests/ingestion.rs
crates/tack-orch/tests/ingestion/*.rs`:

```
✓ maintainability budgets hold (5 files checked)
```

`python3 scripts/maintainability.py measure --totals` before/after: see *Measured
numbers* above (`prod=56223` unchanged both sides; `test` 71098 → 71107; `sleeps_in_tests`
58 → 47).

`python3 scripts/maintainability.py measure crates/tack-orch/tests/ingestion.rs
crates/tack-orch/tests/ingestion/*.rs` before/after: see *Measured numbers* above
(1199 → 1208 test lines, 6 tests unchanged, 11 → 0 sleeps).

`cargo fmt --all -- --check`: clean. `cargo nextest run --workspace -E
'binary(ingestion)'`: `6 tests run: 6 passed, 0 skipped`, plus 20 consecutive stress
runs with no failure. `cargo clippy --workspace --all-targets -- -D warnings`: clean
(the card's required gate, plus the narrower `cargo clippy -p tack-orch --all-targets
-- -D warnings` run after every individual file's edit). `scripts/check-comments.sh`
and `scripts/check-test-hygiene.sh`: both clean.

## What a stranger still cannot do

A stranger reading `support.rs`'s `wait_for_tick_after` alone would not know *why* it
polls for two `last_seen_at` bumps instead of one without reading its doc comment
closely — the two-bump requirement exists specifically because the reconciler loop
writes its health record before its persist phase runs, so a single bump can observe a
tick that hasn't finished writing yet. That reasoning is in the function's own doc
comment (kept there deliberately, per CLAUDE.md's comment rule, since it explains a
non-obvious hazard rather than project history) but is easy to miss if someone "fixes"
a future flake by reverting to a single bump without re-deriving why two were chosen.
They also cannot yet regenerate `docs/adr/0064-fixed-waits.txt` and get a clean signal
from it for this crate alone — the tracked file's drift from `IX-M1` means it currently
overstates how many fixed waits remain in `tack-orch`/`tack-api` production code, and
that will stay misleading until whoever closes ADR 0064 regenerates it once, after every
sub-card touching those crates has landed.

## Context spent

- Tokens read before the first edit (cold start): the card's own dispatch prompt
  (self-contained: file list, line-number hints for every sleep, and the acceptance
  rules were all given directly, no `TODO.md` extraction needed), the relevant
  `docs/plans/human-maintainability.md` §1-§2 excerpt, `crates/tack-orch/tests/common/mod.rs`
  in full, and all three owned test files plus `support.rs` in full before any edit.
- Context size at handoff: moderate-high — `traces.rs` was rewritten in place across
  several passes chasing the line budget down from 141 to 60 (helper extraction, then
  the two-bump race-condition fix, then final trims), and `support.rs` grew
  incrementally alongside it rather than being designed complete up front.
- Files opened and not used: `crates/tack-orch/src/reconciler.rs` was read in full to
  find an observable "tick completed" signal for the wait replacement (settled on
  `last_seen_at`, already exposed via `ControlPlane::get_control_plane`) — used, not
  wasted, but worth flagging that this required reading ~250 lines of `spawn_one`'s
  loop body to confirm the write ordering the two-bump design depends on.
- Read-list lines that were wrong: none — the prompt's sleep line-number hints
  (`traces.rs` ~153/193/269/325, `runs.rs` ~108/145/269/278, `retention.rs`
  ~186/210/296) all matched within a few lines of the actual `grep -n "sleep("` output
  at card start.
- One finding worth flagging for whoever hits this next: a single `last_seen_at` bump is
  **not** a safe stand-in for "this tick's persist phase completed" in this reconciler —
  `record_health` (which sets `last_seen_at`) runs strictly before
  `persist_runs`/`persist_approvals`/`persist_metrics`/`persist_events` in `spawn_one`'s
  loop body (`crates/tack-orch/src/reconciler.rs`, inside the `tokio::spawn` closure).
  Any future test polling `last_seen_at` once to prove "another tick ran and touched the
  database" should wait for a **second** bump instead, for the same reason this card's
  `wait_for_tick_after` does.

## Amendments

*(none yet)*
