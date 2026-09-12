# IX-M4-tack-db-migrations handoff

- Base SHA / branch / final SHA: `44825e3` / `agent/ix-m4-db-migrations` / `172bcb8`
- Files changed (must equal ownership list): `crates/tack-db/tests/migrations.rs` (untouched,
  only read), `crates/tack-db/tests/migrations/orch_migrations.rs`, `orch_metrics.rs`,
  `stale_reconcile.rs`, `item_source_migration.rs` — exactly this card's one test binary,
  committed one file at a time, largest first.
- Contract fixtures consumed: none (`docs/contracts/runner-v1/` not touched — this binary has
  no runner-v1 surface).
- Behavior implemented: none — this card prunes tests only; no file under
  `crates/tack-db/src/**` was touched (verified: `git diff develop --name-only` lists only the
  four files above).
- Tests added and exact commands/results: none added as a feature proof; test count moved
  58 -> 46 purely from merging variant families into table-driven tests (see *What was
  removed*). `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-db-migrations cargo nextest run
  --workspace -E 'binary(migrations)'` — 46/46 pass, run after every file's commit and again
  at the end.
- Failure/adversarial case proved: n/a (no new behavior; existing adversarial cases —
  injected-rebuild-failure rollback, checksum tampering, orphan-FK rejection, stale-staging
  recovery — all preserved, some merged into table-driven form, see below).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none found. Every function ended at or under the
  60-line hard cap after this card's changes (max observed: 60, `orch_migrations.rs`'s
  `preexisting_rows_backfill_default_config_and_version`, exactly at the cap).
- Secrets/logging review: n/a (test-only changes; the fixture data these tests insert —
  `docket` tokens, control-plane URLs — was already test-only placeholder data before this
  card and is unchanged).
- Safe merge order and likely conflicts: independent of the sibling `tack-api`
  `orchestration`/`runner_protocol`/`handlers`/`security` cards (different crate, different
  binary). No conflicts expected against `develop` since only
  `crates/tack-db/tests/migrations/**` and the one root file were read; nothing else touched.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| All behavior in the four files is unchanged | `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-db-migrations cargo nextest run --workspace -E 'binary(migrations)'` — 46/46 pass at every commit and at the end; migration ordering/checkpoints (`run_up_to` targets, `run_all` cutoffs) were never altered, only re-expressed |
| No fixed wait in this binary (confirmed, not just believed) | `grep -rn 'sleep(' crates/tack-db/tests/migrations.rs crates/tack-db/tests/migrations/*.rs` — empty, both before and after this card's changes |
| No production file touched | `git diff develop --name-only` — lists only the four owned test files |
| Every test name in this binary is ≤60 characters | `python3 scripts/maintainability.py measure crates/tack-db/tests/migrations/*.rs` — `name` column max is 60 (`orch_migrations.rs`), 58, 55, 59 for the other three |
| Every file's preamble is ≤10 lines | `mdoc` column in the same `measure` output: 10, 9, 10, 10 |
| Every test body is ≤60 lines (hard cap) | `max` column: 60, 54, 34, 45 — all at or under the cap; see *Known limitations* line above for the one file landing exactly at 60 |
| No board vocabulary leaked into test names/messages | `grep -n -i "card_g5\|wave-1\|\bcard\b\|\bphase\b"` across all four files — empty after this card (two names carried `card_g5a`/`card_g5b`, one message said "Wave-1 migrations", all fixed in `orch_migrations.rs`) |
| The two orch_migrations.rs/orch_metrics.rs cross-file "duplicate" pairs are genuinely distinct migration groups, not literal duplicates | Read both files in full before touching either; `orch_migrations.rs` covers migrations 019-038 (Agent-Factory Control Center core tables + two rebuilds), `orch_metrics.rs` covers 025-027 (metrics/retention tables) — different `NEW_TABLES` sets, different FK graphs, different repository functions under test (`upsert_orch_metrics`/`rollup_and_purge_*` vs nothing in `orch_migrations.rs`). Left both; `duplicate-tests` still reports 3 residual cross-file pairs after this card (down from 12), all of this same legitimate shape |

## Measured numbers

`CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-db-migrations python3 scripts/maintainability.py measure crates/tack-db/tests/migrations.rs crates/tack-db/tests/migrations/*.rs`

Before (measured at card start, 2026-09-12):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-db/tests/migrations/orch_migrations.rs              0     0   0%   1353   31    35  187   74   43
crates/tack-db/tests/migrations/orch_metrics.rs                 0     0   0%    682   16    34   62   78   18
crates/tack-db/tests/migrations/stale_reconcile.rs               0     0   0%    357    6    42   58   55   11
crates/tack-db/tests/migrations/item_source_migration.rs        0     0   0%    230    5    36   78   59   10
crates/tack-db/tests/migrations.rs                              0     0   0%     17    0     0    0    0    5
totals: prod=0 (comments 0) test=2639 ratio=0.0 tests=58 sleeps_in_tests=0 env_gated=0
```

After (this handoff):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-db/tests/migrations/orch_migrations.rs              0     0   0%   1182   21    36   60   57   10
crates/tack-db/tests/migrations/orch_metrics.rs                 0     0   0%    628   14    36   54   58    9
crates/tack-db/tests/migrations/stale_reconcile.rs               0     0   0%    289    6    23   34   55   10
crates/tack-db/tests/migrations/item_source_migration.rs        0     0   0%    232    5    30   45   59   10
crates/tack-db/tests/migrations.rs                              0     0   0%     17    0     0    0    0    5
totals: prod=0 (comments 0) test=2348 ratio=0.0 tests=46 sleeps_in_tests=0 env_gated=0
```

Net: 2639 -> 2348 test lines (-291, -11%); 58 -> 46 tests (-12, all from merging variant
families, zero coverage removed — see *What was removed*); every file's `name` max now
≤60 (was up to 78); every file's `mdoc` (preamble) now ≤10 (was up to 43); every file's `max`
body now ≤60 (was up to 187). Cross-checked against nextest at every commit: 58 (start) -> 48
(after `orch_migrations.rs`, -10) -> 46 (after `orch_metrics.rs`, -2) -> 46 (after
`stale_reconcile.rs`, ±0) -> 46 (after `item_source_migration.rs`, ±0) — matches the sum of
each file's final `#t` column (21+14+6+5=46).

## What was removed

Per-file, with the §2.2 rule number for each change (`1: variants → rows`, `2: name`,
`3: body`, `6: preamble`, `8: fixed wait`):

**`orch_migrations.rs`** (1353 -> 1182 lines, 31 -> 21 tests) — `1`: four variant families
merged into table-driven tests: the five `test_orch_*_rejects_orphan_control_plane`/
`test_orch_links_rejects_orphan_project`/`test_orch_tasks_rejects_orphan_item` functions into
one `orphan_fk_insert_is_rejected` (a `[(&str, String)]` case list of label + generated SQL);
the five `test_migration_03{2..6}_adds_*_column` functions into one
`migrations_032_to_036_each_add_one_column` (a `ColumnAddCase` table); the two
`test_a_stale_orch_{runs,approvals}_staging_table_is_recovered_without_a_boot_loop` functions
into one `stale_rebuild_staging_table_recovers_without_boot_loop` (a `StaleStagingCase` table,
preserving each case's exact original `CREATE TABLE` staging-table SQL and the approvals
case's extra "037 stays recorded" assertion via a `prior_migrations_stay_recorded` field); the
two `test_every_orch_{runs,approvals}_rebuild_statement_rolls_back_and_retries` wrapper
functions into one `rebuild_failure_at_every_step_rolls_back_and_retries` looping over
`[(migration, steps); 2]`. `2`: dropped the `test_` prefix from all 21 surviving names
(matching the convention `orch_repo.rs` established in IX-M4-tack-db-repository); removed two
board-vocabulary names (`test_upgrade_from_031_applies_card_g5a_migrations_in_place` →
`upgrade_from_031_applies_032_through_036_in_place`,
`test_upgrade_from_036_applies_card_g5b_rebuilds_in_place` →
`upgrade_from_036_applies_037_and_038_rebuilds_in_place`) and one board-vocabulary assert
message ("Wave-1 migrations" → "the first 24 migrations"). `3`: the two rebuild
row/field-preservation tests were the file's worst offenders —
`migration_037_rebuild_preserves_rows_and_fields` was 187 lines (14 hand-written `assert_eq!`
blocks across two rows), cut to 60 by deriving `PartialEq` on the row struct and comparing
whole rows at once against an `expected()` method computed from the fixture's own documented
backfill rule (`run_attempt`/`correlation_id` always backfill the same way, `created_at`
always equals `started_at`, etc.), then looping over both fixtures instead of duplicating the
insert+fetch+compare code per row;
`migration_038_rebuild_preserves_rows_and_fields` (77 lines) similarly cut to 53 by extracting
its struct-fetch boilerplate into `fetch_038_approval`;
`preexisting_rows_backfill_default_config_and_version` (92 lines) cut to 60 by reusing
`seed_item` (see below) instead of hand-rolling the workspace/project/item insert, and by
extracting the two near-identical "select version, assert ==1" blocks into
`assert_version_backfilled_to_one`. `6`: preamble 43 -> 10 lines (the original preamble
carried the full Phase 1/2/3 design narrative from `agnostic-control-plane.md`; kept the
"what this proves" summary and the two authoritative-comment pointers, cut the rest — design
rationale lives in that doc, not here). Also renamed `seed_item`'s return type from `Uuid` to
`(Uuid, Uuid)` (project id + item id) so `preexisting_rows_backfill_default_config_and_version`
could reuse it instead of duplicating its three-insert body; updated the three existing call
sites to destructure `(_, item_id)`. No `5` (third-layer over-pinning): this binary tests
schema/upgrade behavior with no repository- or router-layer equivalent to be redundant against.

**`orch_metrics.rs`** (682 -> 628 lines, 16 -> 14 tests) — `1`: the three
`test_orch_{metrics,events_daily,metrics_daily}_rejects_orphan_control_plane` functions merged
into one `orphan_fk_insert_is_rejected` (same case-list shape as `orch_migrations.rs`'s, kept
separate per-file rather than sharing a helper since the two files are separate modules with
different table sets); the three-call repeated-insert sequences in
`list_latest_orch_metrics_returns_most_recent_sample` (was
`test_list_latest_orch_metrics_returns_the_most_recent_sample_per_series`) and
`rollup_and_purge_orch_metrics_preserves_sum_min_max` (was
`test_rollup_and_purge_orch_metrics_preserves_sum_min_max_and_deletes_raw_rows`) were each
turned into a 3-line `for (value, offset) in [...]` loop instead of three repeated
multi-line `insert_raw_metric(...)` calls. `2`: dropped `test_` prefix from all 14 names;
de-articled/shortened five names that carried "the"/"a"/redundant clauses (e.g.
`..._preserves_totals_and_deletes_raw_rows` → `..._preserves_totals_deletes_raw`,
`..._is_a_noop` → `..._is_noop`, `..._sweeps_a_backlog_larger_than_one_batch` →
`..._sweeps_backlog_over_one_batch`). `6`: preamble 18 -> 9 lines (dropped the itemized
`Covers:` bullet list, folded into two prose sentences carrying the same claims). No `3`
(body) or `5` needed: the worst body was already 62 lines and needed only the loop compaction
above to land at 54; no repository/router duplication found.

**`stale_reconcile.rs`** (357 -> 289 lines, 6 -> 6 tests, none merged) — `3`: the
"create a control plane, then `upsert_orch_link` it to the project" setup (25 lines, byte-
identical except the plane's `name`) was duplicated across all four `orch_tasks` tests;
extracted into `make_linked_control_plane(repo, project_id, name) -> Uuid`. The two
`orch_approvals` tests' bare `create_control_plane` call was extracted into a second helper,
`make_control_plane`, since they never call `upsert_orch_link`. `2`: two names carrying
"...touched_by_the_sweep" shortened to "...touched_by_sweep" (dropped "the"). `6`: preamble
11 -> 10 lines. No `1`: each of the six tests proves a distinct branch of the sweep's `WHERE`
clause (long-unreachable → stale, healthy-with-stale-`last_seen_at` → untouched,
recently-unreachable → untouched, terminal-status → untouched, ×2 for `orch_tasks`/
`orch_approvals`'s pending/decided pair) — not a variant family, a coverage matrix; merging
into one table-driven test was considered and rejected because each row needs a different
sequence of setup calls (`update_control_plane_health` vs a raw `UPDATE ... health = ?`), not
just different data, which would have made the "table" harder to read than the six named
functions it would replace.

**`item_source_migration.rs`** (230 -> 232 lines\*, 5 -> 5 tests, none merged) — `3`: the one
body over budget, `upgrade_in_place_backfills_pre_migration_items_to_untrusted` (78 lines), cut
to 45 by extracting its workspace/project/item raw-SQL seed into `seed_pre_029_item` (used
once — still worth extracting on its own per-function-length grounds even though it doesn't
meet the "used 2+ times" bar rule 3 states for moving code to `tests/common`, since this stays
local to the one file, not shared). `2`: one article dropped
(`migration_029_is_applied_on_a_fresh_db` → `migration_029_is_applied_on_fresh_db`); the other
four names were already ≤60 chars with no articles at the top level (a few carry "the"/"a"
inside prose *messages*, which the rule targets at test *names*, not assertion text). No `1`
or `6`: no variant family present, preamble was already 10 lines. \*Line count rose slightly
(230→232) because extracting a named, documented helper function costs a few lines in
signature and doc-comment overhead versus the inline block it replaced, even though the calling
test's own body shrank by 33 lines (78→45) — the `#t`/`max`/`name` columns are the load-bearing
numbers for this file, not raw line count.

**Rule 5 (third-layer over-pinning):** no removal made under this rule in any of the four
files. This binary is entirely schema/upgrade-path/reconciliation-sweep coverage with no
repository- or router-layer test elsewhere in the tree proving the same fact — migrations
tests are, by the plan's own prediction (§IX.4 IX-M4 card text), "something unique (DB schema
behavior under upgrade) not covered elsewhere." Confirmed by reading, not assumed.

## Re-baselined?

`no`. Nothing in `scripts/maintainability-baseline.json` was touched — every file's
post-change numbers are within its existing baseline entry (all four files got *smaller*
across every measured dimension), so the ratchet holds without a re-baseline:
`python3 scripts/maintainability.py check crates/tack-db/tests/migrations.rs
crates/tack-db/tests/migrations/*.rs` → `✓ maintainability budgets hold (5 files checked)`.

## Budget check

`python3 scripts/maintainability.py check --changed` (working tree clean, all committed) →
`✓ maintainability budgets hold (0 files checked)`. The load-bearing form, checking the actual
files regardless of git status:

```
$ python3 scripts/maintainability.py check crates/tack-db/tests/migrations.rs crates/tack-db/tests/migrations/*.rs
✓ maintainability budgets hold (5 files checked)
```

`python3 scripts/maintainability.py measure --totals` — workspace-wide, **not** a clean
before/after pair for this card alone (the sibling `tack-api` cards and other IX-M4 sub-cards
were running concurrently against the same `develop` history; per IX-M4-tack-db-repository's
handoff, this number is informational only):

- After this card (this worktree only, based on `develop` at `44825e3`):
  `totals: prod=56223 (comments 11931) test=71784 ratio=1.277 tests=1467 sleeps_in_tests=70 env_gated=0`.

`measure crates/tack-db/tests/migrations*` before/after: see *Measured numbers* above — this
is the load-bearing before/after pair for this sub-card.

## What a stranger still cannot do

A stranger arriving from outside this repository still cannot tell, from
`orph_fk_insert_is_rejected`'s name alone in either `orch_migrations.rs` or `orch_metrics.rs`,
which specific tables that file's version covers without opening the case list — the merge
traded five/three self-describing function names for one generically-named table-driven test.
They also still cannot run one command that proves this binary's schema coverage is complete
against every migration in `crates/tack-db/src/migrations.rs`; each file's own preamble states
which migration range it owns, but nothing cross-checks that every migration number has a
file, and `item_source_migration.rs` predates the `orch_*` naming convention entirely (it
covers migration 029, alone, with no numeric range in its own name) — a newcomer has to
already know the migration's number to find its test file.

## Context spent

- Tokens read before the first edit (cold start): `TODO.md` §IX.0-§IX.3 and the `IX-M4` card
  block (not the whole file), `docs/plans/human-maintainability.md` §2.2 and the M4 row of §5,
  `IX-M4-tack-db-repository.md`'s full handoff (for judgment-call style), and
  `TEMPLATE.md` — a few thousand tokens, in line with the cold-start capsule's own estimate.
- Context size at handoff: moderate — four files, the largest (`orch_migrations.rs`) took
  several iterations to get its two rebuild-preservation tests under the 60-line hard cap
  (187 → 127 → 102 → 66 → 60 lines), including discovering that `rustfmt` always expands a
  macro argument containing `.await` onto its own line regardless of the 100-char width limit,
  which is what caused the first two size-reduction attempts to undershoot.
- Files opened and not used: none beyond the read-list.
- Read-list lines that were wrong: the four files' sizes matched the dispatch prompt's
  estimates exactly (1352/681/356/229 vs measured 1353/682/357/230 — off by one line each,
  consistent with the file having a trailing newline the prompt's line count didn't).

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten.)*
