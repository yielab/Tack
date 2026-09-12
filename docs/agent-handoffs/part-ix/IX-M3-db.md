# IX-M3-db handoff

- Base SHA / branch / final SHA: `5435b48` / `agent/ix-m3-db-test-support` / `ae981c6`
- Files changed (must equal ownership list): `Cargo.toml`, `Cargo.lock`,
  `crates/tack-db/Cargo.toml`, `crates/tack-db/tests/common/mod.rs`,
  `crates/tack-test-support/Cargo.toml` (new), `crates/tack-test-support/src/lib.rs` (new)
- Contract fixtures consumed: none
- Behavior implemented: none — pure relocation, no behavior change (§IX.1 rule 1)
- Tests added and exact commands/results: none added; four existing helpers moved.
  `cargo nextest run --workspace -E 'package(tack-db)'`: 199 tests run, 199 passed, 1 skipped
  — identical before and after (measured both ways, see *Measured numbers*)
- Failure/adversarial case proved: n/a — relocation only, nothing new to adversarially test
- Schema/API/contract change requested from another owner: none
- Known limitations or `not_measured` fields: n/a
- Secrets/logging review: n/a — no secrets or logging touched
- Safe merge order and likely conflicts: must land before IX-M3-orch/api/runner/cli (they
  depend on this crate existing); no conflicts expected — only `tack-db`, the root
  `Cargo.toml`/`Cargo.lock`, and the new `tack-test-support` crate are touched, none of which
  the four sibling sub-cards own
- Checklist: no unowned files, no live secret, no panic stub, no blind retry

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `tack-test-support` exists, depends on `tack-core` and `tack-db` only | `crates/tack-test-support/Cargo.toml` `[dependencies]` block |
| `setup_test_db`, `create_test_workspace`, `make_project`, `make_item` moved, not duplicated | `crates/tack-test-support/src/lib.rs`; `grep -rn "fn setup_test_db\|fn create_test_workspace\|fn make_project\|fn make_item" crates/tack-db/tests` returns nothing |
| `ControllableClock` primitive added, usable by a crate-local `FakeClock` newtype | `crates/tack-test-support/src/lib.rs::ControllableClock` (`new`/`now`/`set`/`advance`) |
| Fixture-loading helper added, following `runner_contract/fixtures.rs`'s manifest-dir convention | `crates/tack-test-support/src/lib.rs::read_fixture`/`read_fixture_string` |
| `tack-db`'s test *count* is unchanged | `cargo nextest run --workspace -E 'package(tack-db)'` — 199 passed / 1 skipped, both before (stashed original tree) and after |
| No test file in `tack-db` defines its own `project()`/`request()`/`runner()`/`claim()`/pool helper | `grep -rn "fn project(\|fn request(\|fn runner(\|fn claim(" crates/tack-db/tests/` → no output |
| Workspace still builds and lints clean with the new member | `cargo clippy --workspace --all-targets -- -D warnings` → clean; `cargo fmt --all --check` → clean |

## Measured numbers

- `cargo nextest run --workspace -E 'package(tack-db)' --build-jobs 4`, on the untouched tree
  (stashed the card's changes to measure): `199 tests run: 199 passed, 1 skipped` (4 binaries).
- Same command, on the final tree: `199 tests run: 199 passed, 1 skipped` (4 binaries) —
  identical.
- `python3 scripts/maintainability.py measure --totals`, untouched tree:
  `prod=56103 (comments 11907) test=73815 ratio=1.316 tests=1498 sleeps_in_tests=74 env_gated=0`
- Same command, final tree (**after** `git add -A` — an unstaged new file is invisible to
  this tool's `git ls-files`-based scan, the footgun CLAUDE.md warns about):
  `prod=56223 (comments 11931) test=73734 ratio=1.311 tests=1498 sleeps_in_tests=74 env_gated=0`
  — production lines up by 120 (the new `lib.rs`, mostly the two additions the plan asked
  for beyond the moved code), test lines down by 81 (`tests/common/mod.rs` shrank from 83
  lines to a 1-line re-export), workspace ratio improved slightly (1.316 → 1.311).
- `python3 scripts/maintainability.py check --changed` on the final, staged tree:
  `✓ maintainability budgets hold (2 files checked)`.

## What a stranger still cannot do

Nothing changed for a user of `tack` itself; this only moves where four crates' test
helpers live. A contributor writing a **new** test in `tack-orch`, `tack-api`,
`tack-runner` or `tack-cli` still cannot reach `tack-test-support`'s
`setup_test_db`/`ControllableClock`/fixture-loading yet — each of those crates needs its own
`[dev-dependencies]` entry added, and where it collides with an existing crate-local helper
of the same name (their own `FakeClock`s, their own pool setup), a decision about which one
wins and how the crate-local clock trait wraps `ControllableClock`. That wiring is
IX-M3-orch's, IX-M3-api's, IX-M3-runner's and IX-M3-cli's job, not this card's — this card
only had to make the crate exist and prove `tack-db` still works unchanged on top of it.

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree:

```
✓ maintainability budgets hold (2 files checked)
```

`python3 scripts/maintainability.py measure --totals`:

- Before: `prod=56103 (comments 11907) test=73815 ratio=1.316 tests=1498 sleeps_in_tests=74 env_gated=0`
- After: `prod=56223 (comments 11931) test=73734 ratio=1.311 tests=1498 sleeps_in_tests=74 env_gated=0`

## What was removed

None. This card moves code (`tack-db/tests/common/mod.rs`'s four helpers into
`tack-test-support`) and adds two new primitives the plan calls for
(`ControllableClock`, fixture loading) — it does not remove or merge any test, and no
assertion changed. The nextest count for `package(tack-db)` is identical before and after
(199 passed, 1 skipped, both runs).

## Re-baselined?

`no`. `check --changed` reported budgets holding on the first run after staging the new
files; the plan explicitly allows but doesn't require a re-baseline for this card since it's
a brand-new file, not a shrink of an existing over-budget one.

## Context spent

- Tokens read before the first edit (cold start): `TODO.md` §IX.0–§IX.4 (~6k tokens: cold-start
  capsule, §IX.1 rules, §IX.2 ownership, §IX.3 dependency graph, the IX-M3 card block), plan
  `docs/plans/human-maintainability.md` §2.1 (~1k), `docs/agent-handoffs/part-ix/TEMPLATE.md`
  and `part-vi/TEMPLATE.md` it points to, `crates/tack-db/tests/common/mod.rs`,
  `crates/tack-orch/src/execution_retention/tests.rs` and `crates/tack-runner/src/runtime.rs`
  (existing `FakeClock` impls, to shape `ControllableClock`'s API without duplicating either
  trait), `crates/tack-orch/tests/runner_contract/fixtures.rs` (the fixture-loading
  convention), root `Cargo.toml` and `crates/tack-db/Cargo.toml`. Roughly 12–15k tokens total.
- Context size at handoff: moderate; no full-file reads of unrelated crates.
- Files opened and not used (each one is a finding for the dispatch README): none — every
  file read fed directly into either the new crate's content or the gate commands.
- Read-list lines that were wrong (a range that missed, a size that was off): none; the task's
  read list matched the repository exactly, including the exact grep count of 12 call sites
  for `common::` across `tack-db/tests` (only 3 `mod common;` declarations — `migrations.rs`,
  `repository.rs`, `perf_test.rs` — so keeping `tests/common/mod.rs` as a one-line re-export
  was the smaller diff over rewriting all 12 call sites to `tack_test_support::`).

## Amendments

*(none yet)*
