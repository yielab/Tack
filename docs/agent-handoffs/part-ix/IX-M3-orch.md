# IX-M3-orch handoff

- Base SHA / branch / final SHA: `e963be7` / `agent/ix-m3-orch-test-helpers` / `8508722`
- Files changed (must equal ownership list): `crates/tack-orch/Cargo.toml` (dev-dependency),
  `crates/tack-orch/tests/common/mod.rs` (new), `crates/tack-orch/tests/ingestion.rs`,
  `crates/tack-orch/tests/ingestion/runs.rs`, `crates/tack-orch/tests/ingestion/support.rs`,
  `crates/tack-orch/tests/ingestion/traces.rs`, `crates/tack-orch/tests/scheduling.rs`,
  `crates/tack-orch/tests/scheduling/support.rs`. `Cargo.lock` picked up the one new
  `tack-test-support` edge.
- Contract fixtures consumed: none
- Behavior implemented: none — pure relocation, no behavior change (§IX.1 rule 1). Every moved
  pool/seed helper is byte-identical to the deleted local copy (verbatim SQL, verbatim
  `CreateProject`/`CreateItem` field values); `scheduling/support.rs`'s bespoke
  workspace/project/item/agent-profile seeding (different names, an extra agent profile) is
  untouched — only its pool-creation lines now delegate instead of duplicating.
- Tests added and exact commands/results: none added; two duplicate pool/seed helper sets
  removed, one new crate-wide `tests/common/mod.rs`.
  `cargo nextest run --workspace -E 'package(tack-orch)'`: `290 tests run: 290 passed, 1
  skipped` — identical before and after (measured both ways, see *Measured numbers*).
- Failure/adversarial case proved: n/a — relocation only, nothing new to adversarially test.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: n/a.
- Secrets/logging review: n/a — no secrets or logging touched.
- Safe merge order and likely conflicts: lands after IX-M3-db (already merged) and independent
  of IX-M3-api (already merged)/IX-M3-runner/IX-M3-cli — this card touches only
  `crates/tack-orch/**` and `Cargo.lock`'s reverse dependency edge. The only shared file is
  `Cargo.lock`; the integrator should regenerate it once after merging all four sub-cards
  rather than trusting any one branch's copy (per root `CLAUDE.md`'s generated-file rule).
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| No test file inside `tests/<subdir>/**` defines its own `project()`-shaped helper | `git ls-files 'crates/tack-orch/tests/**/*.rs' \| xargs grep -lE 'fn (create_project\|seed_project\|make_project\|new_project\|project)\('` → empty |
| Same for the `request()`/`claim()`-shaped family | `git ls-files 'crates/tack-orch/tests/**/*.rs' \| xargs grep -lE 'fn (request\|json_request\|post\|get\|send\|claim\|make_claim)\('` → only `runner_contract/fakes.rs` (see note below) |
| `tack-test-support` is now a `tack-orch` dev-dependency, its pool/seed helpers reachable from every binary via `crate::common` | `crates/tack-orch/Cargo.toml` `[dev-dependencies]`; `crates/tack-orch/tests/common/mod.rs::pub(crate) use tack_test_support::*;` |
| `ingestion/support.rs`'s `setup_repo`/`seed_workspace`/`seed_project`/`seed_item` are gone, replaced by `crate::common`'s re-exports at every call site | `grep -n "fn setup_repo\|fn seed_workspace\|fn seed_project\|fn seed_item" crates/tack-orch/tests/ingestion/support.rs` → empty; `crates/tack-orch/tests/ingestion/{runs,traces}.rs` import `setup_test_db`/`create_test_workspace`/`make_project`/`make_item` from `crate::common` |
| `scheduling/support.rs`'s `setup_repo` no longer hand-rolls `init_pool`+`migrations::run_all` | `crates/tack-orch/tests/scheduling/support.rs::setup_repo` calls `crate::common::setup_test_db()`; `grep -n "init_pool\|migrations::run_all" crates/tack-orch/tests/scheduling/support.rs` → empty |
| `tack-orch`'s test *count* is unchanged | `cargo nextest run --workspace -E 'package(tack-orch)'` — 290 passed / 1 skipped, both before (stashed) and after |
| Workspace still builds and lints clean | `cargo clippy -p tack-orch --tests --all-targets -- -D warnings` → clean, no warnings |
| `cargo fmt --all -- --check` is clean | ran after the final edit, no output |

Note on the claim/request grep: `runner_contract/fakes.rs::FakeRunner::claim` is the one
surviving match, and it is **not** a fixture-setup helper of the kind this card eliminates —
see *What was removed* for why it's a deliberate exclusion, matching the card's own caution #1.

## Measured numbers

- `cargo nextest run --workspace -E 'package(tack-orch)' --build-jobs 4`, untouched tree
  (stashed the card's changes to measure): `290 tests run: 290 passed, 1 skipped` (9 binaries).
- Same command, final tree: `290 tests run: 290 passed, 1 skipped` (9 binaries) — identical.
- `python3 scripts/maintainability.py measure --totals`, untouched tree:
  `prod=56223 (comments 11931) test=73415 ratio=1.306 tests=1498 sleeps_in_tests=74 env_gated=0`
- Same command, final tree: `prod=56223 (comments 11931) test=73353 ratio=1.305 tests=1498
  sleeps_in_tests=74 env_gated=0` — production unchanged (this card touches no `src/`), test
  lines down by 62, workspace ratio improved fractionally (1.306 → 1.305), total test count
  unchanged (1498, matching "no test removed").
- `python3 scripts/maintainability.py check --changed` on the final, staged tree:
  `✓ maintainability budgets hold (7 files checked)`.
- `cargo fmt --all -- --check`: clean.
- `bash scripts/check-comments.sh` and `bash scripts/check-test-hygiene.sh`: both green (not
  part of this card's required gate list but run anyway since they're cheap and part of
  `pre-push`).

## What a stranger still cannot do

Nothing changed for a user of `tack` itself — this only moves where `tack-orch`'s own
integration tests build their pool/workspace/project/item fixtures. `scheduling/support.rs`'s
own bespoke setup (a "Scheduling"-named workspace/project, an item, and an agent profile) is
still local to that file rather than folded into `tack-test-support`, because it is a
genuinely different fixture shape (extra agent profile, different names, a tuple return of
`(Repository, String)`) than `tack-test-support::make_project`/`make_item` — silently
collapsing the two would be a behavior change the card forbids. A future contributor who wants
an agent-profile-seeded fixture from `tack-test-support` itself still has to add it there; this
card only had to prove the plain pool/workspace/project/item duplication could disappear.

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree:

```
✓ maintainability budgets hold (7 files checked)
```

`python3 scripts/maintainability.py measure --totals`:

- Before: `prod=56223 (comments 11931) test=73415 ratio=1.306 tests=1498 sleeps_in_tests=74 env_gated=0`
- After: `prod=56223 (comments 11931) test=73353 ratio=1.305 tests=1498 sleeps_in_tests=74 env_gated=0`

One file initially regressed past its own (already-over-budget) baseline during the mechanical
consolidation — fixed before this commit, not re-baselined:

- `crates/tack-orch/tests/ingestion/support.rs` (`test_module_doc_lines` 13→14 after adding a
  sentence about where seeding now comes from): fixed by tightening the whole preamble to 8
  lines, under its own 13-line baseline, rather than accepting the regression.

## What was removed

Class **"one place for a helper"** (plan §2.1: `tests/common/mod.rs` is the crate's shared
fixtures, the only place a helper lives), applied to test *fixture-setup helpers* rather than
test *assertions* — no test was removed, no assertion changed, no test renamed.

`project()`/pool-shaped (2 files → `crate::common` re-exporting `tack-test-support`):

- `ingestion/support.rs`: deleted `setup_repo` (`init_pool`+`migrations::run_all`, byte-
  identical to `tack_test_support::setup_test_db`), `seed_workspace` (identical SQL insert to
  `create_test_workspace`), `seed_project`/`seed_item` (identical `CreateProject`/`CreateItem`
  field values to `make_project`/`make_item`). Call sites in `ingestion/runs.rs` and
  `ingestion/traces.rs` now call `crate::common`'s re-exports directly, under
  `tack-test-support`'s own names (`setup_test_db`, `create_test_workspace`, `make_project`,
  `make_item`) rather than keeping the old local aliases — this is the smaller, more
  consistent outcome now that the same names are used the same way in `tack-db`'s own
  `tests/common/mod.rs` (see IX-M3-db's handoff).
- `scheduling/support.rs`: its own `setup_repo` kept its distinct signature and bespoke
  workspace ("Scheduling")/project/item/agent-profile seeding (genuinely different from
  `tack-test-support`'s fixture, so not merged — see *What a stranger still cannot do*), but no
  longer duplicates the raw `init_pool("sqlite::memory:")` + `migrations::run_all(&pool)` pair;
  it now calls `crate::common::setup_test_db()` for that part only.

`request()`/`claim()`-shaped: **none removed, one found and deliberately left**.
`runner_contract/fakes.rs::FakeRunner::claim`/`FakeRunner::report_event`/`FakeRunner::complete`
match the acceptance grep's `claim(` pattern, but they are pure in-memory state-machine methods
on a fence/idempotency-replay simulation with no I/O at all — part of the byte-pinned
`runner_contract` fixture machinery this card's own instructions name as off-limits (caution
#1), not a fixture-setup duplicate of any DB/HTTP helper elsewhere in the crate. Left
untouched, no class assigned because nothing was removed.

`scheduling/scheduler.rs::request()` matches the same grep but is a pure `SchedulingRequest`
value builder for property-style permutation tests with no repository or I/O involved — the
card's own caution #2 example almost verbatim (scheduler-specific setup that happens to match
the regex incidentally). Left untouched, no class assigned.

`ingestion/retention.rs::seed_real_db` also stands up its own file-backed pool + raw-SQL seed,
but its own module doc explains why: it mirrors `tack-db/tests/repository/execution_retention.rs`'s
identical reasoning (a file-backed WAL database, not in-memory, because the production
retention/health-watch tasks it drives need a real file), uses a distinct name
(`seed_real_db`, not `setup_repo`/`project()`), and doesn't match any of the acceptance grep's
patterns. Left untouched, no class assigned — this is the same judgment call IX-M3-api made for
its distinct `send()`-family variants.

`docket_tick_contract_test.rs` (a top-level file, **not** under `tests/**/*.rs`'s subdirectory
glob — `git ls-files 'crates/tack-orch/tests/**/*.rs'` does not match it, which is why the
card's own "last measured" grep commands never surfaced it) defines its own
`setup_repo`/`seed_workspace`/`seed_project`, explicitly commented "mirrors
`ingestion/support.rs`" — a genuine, literal duplicate at the DB layer. **Deliberately left
untouched**: this file is one of the two `docket_*_contract_test` binaries §IX.1 rule 4 names
as "never pruned" alongside `runner_contract`, and it's a byte-pinned golden-file oracle (two
ordered-HTTP-request and DB-row snapshots) outside both this card's measured surface and its
two named cautions. Consolidating its setup would be low-risk (pure DB plumbing, no assertion
touched) but was not in the brief given to this card and touches a file explicitly protected
elsewhere in the board — flagging it here as a known, unaddressed duplicate for a future card
(IX-M4's `tack-orch` sub-card, or a follow-up) rather than expanding this card's scope
unilaterally.

## Re-baselined?

`no`. `check --changed` reports budgets holding on the final, staged tree; the one file that
regressed past its own baseline during the mechanical consolidation (`ingestion/support.rs`)
was fixed by shortening its preamble (see *Budget check*) rather than re-baselined — a baseline
entry going up is a defect, not something to paper over.

## Context spent

- Tokens read before the first edit (cold start): `TODO.md` §IX.0–§IX.4 (~6k tokens: cold-start
  capsule, §IX.1 rules, §IX.2 ownership, §IX.3 dependency graph, the IX-M3 card block), plan
  `docs/plans/human-maintainability.md` §2.1–§2.2 (~1.5k), `docs/agent-handoffs/part-ix/IX-M3-db.md`
  and `IX-M3-api.md` (to reuse rather than re-invent, and to match judgment style),
  `docs/agent-handoffs/part-ix/TEMPLATE.md`, `crates/tack-test-support/src/lib.rs`,
  `crates/tack-orch/Cargo.toml`'s dependency-direction comment. Roughly 10-12k tokens for cold
  start; the rest of the cost was reading each of the ~10 test files in
  `crates/tack-orch/tests/**` directly (`Read`/`grep`) to compare behavior before consolidating,
  plus the two top-level contract files (`docket_tick_contract_test.rs`,
  `docket_wire_contract_test.rs`) to decide whether they were in scope.
- Context size at handoff: moderate — full reads of `ingestion/support.rs`, `scheduling/support.rs`,
  `runner_contract/fakes.rs`, `scheduling/scheduler.rs`, `runner_contract/fixtures.rs`,
  `ingestion/retention.rs`, `ingestion/runs.rs`, `ingestion/traces.rs`, plus preambles of the four
  top-level contract/live files, were all necessary to tell a genuine duplicate from a
  same-shaped-by-coincidence one.
- Files opened and not used: `docs/plans/harness-maintainability-audit.md` was not opened
  (irrelevant to this card). `runner_contract/domain.rs`, `lifecycle.rs`, `protocol.rs` were
  grepped but not fully read once the grep confirmed no project/request/claim-shaped match.
- Read-list lines that were wrong (or worth flagging): the card's own "last measured" grep
  commands use `git ls-files 'crates/tack-orch/tests/**/*.rs'`, and git's glob semantics for
  `**` do not match files directly under `tests/` (only files inside a subdirectory) — so
  `docket_tick_contract_test.rs`, `docket_wire_contract_test.rs`, `docket_live_test.rs`,
  `model_policy_contract.rs`, `ingestion.rs`, `scheduling.rs`, and `runner_contract.rs` were
  silently excluded from the card's own measurement, even though the card's scope line says
  `crates/tack-orch/tests/**` (which, read as English rather than as a shell glob, plainly
  includes them). This is the same category of finding IX-M3-api reported for `req()` vs.
  `request()`: a literal-grep artifact, not a repository fact — worth knowing before trusting
  the "last measured" section of a future card at face value. The one genuine duplicate it
  hid (`docket_tick_contract_test.rs`) is documented above rather than silently fixed, given
  its protected status.

## Amendments

*(none yet)*
