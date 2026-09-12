# IX-M3-cli handoff

- Base SHA / branch / final SHA: `8fbf8ae` / `agent/ix-m3-cli-test-helpers` / `b281c87`
- Files changed (must equal ownership list): `crates/tack-cli/tests/common/mod.rs` (new),
  `crates/tack-cli/tests/e6_scheduler_e2e_test.rs`,
  `crates/tack-cli/tests/embedded_runner_live_secret.rs`,
  `crates/tack-cli/tests/embedded_runner_orphaned_credential.rs`,
  `crates/tack-cli/tests/embedded_runner_state_scoping.rs`
- Contract fixtures consumed: none
- Behavior implemented: none — pure relocation, no behavior change (§IX.1 rule 1)
- Tests added and exact commands/results: none added; one duplicated helper (`free_port`)
  moved. `cargo nextest run --workspace -E 'package(tack-cli)'`: 119 tests run, 119 passed,
  0 skipped — identical before and after (measured both ways, see *Measured numbers*)
- Failure/adversarial case proved: n/a — relocation only, nothing new to adversarially test
- Schema/API/contract change requested from another owner: none
- Known limitations or `not_measured` fields: n/a
- Secrets/logging review: n/a — no secrets or logging touched
- Safe merge order and likely conflicts: depends on IX-M3-db (`crates/tack-test-support`
  existing), already integrated. No conflicts expected with IX-M3-orch/api/runner — this
  card touches only `crates/tack-cli/tests/**`, which §IX.2 assigns to this sub-card alone.
  This crate did not end up wiring `tack-test-support` as a dev-dependency (see *What a
  stranger still cannot do*), so there is no shared-file contention there either.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| The plan's Appendix A helper duplication (`project()`, `request()`, `runner()`/`claim()`) does not exist in this crate | `git ls-files 'crates/tack-cli/tests/*.rs' \| xargs grep -lE 'fn (create_project\|seed_project\|make_project\|new_project\|project)\('` and the `request`/`runner`/`claim` variants — all three return no output |
| A different duplication the literal greps missed — `free_port()` — was byte-identical across all four real-subprocess test files | `git diff 8fbf8ae..b281c87 -- crates/tack-cli/tests/` shows the same 4-line body deleted from each of the 4 files |
| `free_port()` now lives in exactly one place | `crates/tack-cli/tests/common/mod.rs`; `grep -rn "fn free_port" crates/tack-cli/tests/` shows one definition and four `use common::free_port;` imports |
| `wait_for_ready`/`wait_for_active_runner` were deliberately NOT consolidated | see *What a stranger still cannot do*; each still has its per-file duplicate copy, untouched from `develop` |
| No test removed, no assertion changed | `git diff 8fbf8ae..b281c87` — zero `#[test]`/`#[tokio::test]` lines added or removed (`grep -E '^[+-].*#\[test\]\|^[+-].*#\[tokio::test\]'` on the diff returns nothing) |
| `tack-cli`'s test *count* is unchanged | `cargo nextest run --workspace -E 'package(tack-cli)'` — 119 passed / 0 skipped, both before (`develop`) and after |
| Workspace still builds and lints clean | `cargo clippy -p tack-cli --all-targets -- -D warnings` → clean; `cargo fmt --all --check` → clean |

## Measured numbers

- `cargo nextest run --workspace -E 'package(tack-cli)'`, on `develop` (checked out the four
  touched files back to `develop` to measure): `119 tests run: 119 passed, 0 skipped`
  (7 binaries).
- Same command, on the final tree: `119 tests run: 119 passed, 0 skipped` (7 binaries) —
  identical.
- `python3 scripts/maintainability.py measure --totals`, `develop` tree:
  `prod=56223 (comments 11931) test=73734 ratio=1.311 tests=1498 sleeps_in_tests=74 env_gated=0`
- Same command, final tree (**after** `git add -A` — an unstaged new file is invisible to
  this tool's `git ls-files`-based scan, the exact footgun IX-M3-db's own handoff and
  CLAUDE.md both warn about; hit it once here too before staging):
  `prod=56223 (comments 11931) test=73729 ratio=1.311 tests=1498 sleeps_in_tests=74 env_gated=0`
  — production unchanged, test lines down by 5 (four ~4-line duplicate bodies removed, one
  ~9-line shared file added), test and sleep counts identical.
- `python3 scripts/maintainability.py check --changed` on the final, staged tree:
  `✓ maintainability budgets hold (5 files checked)`.

## What a stranger still cannot do

Nothing changed for a user of `tack` itself; this only removes one duplicated helper among
this crate's tests. A contributor writing a **new** real-subprocess test in `tack-cli`
still cannot reach a shared `wait_for_ready`/`wait_for_active_runner` — those stayed
duplicated per-file, deliberately: an early version of this card moved both into
`tests/common/mod.rs`, and `check --changed` correctly rejected it, because a **brand-new**
file must meet every budget (§2.3, "new files must meet every budget") and these polling
loops' `std::thread::sleep(...)` calls (bounded, not fixed waits, but the checker's `SLEEP`
regex doesn't distinguish) have a budget of zero in a file with no prior baseline. The four
existing test files are already "over" that budget and grandfathered by the ratchet; a new
shared file starts at zero tolerance. Untangling that — giving this crate's poll helpers a
home that doesn't reset their grandfathering, or waiting for IX-M4 to replace the `sleep()`
pattern crate-wide with something the ratchet doesn't flag — is not this card's job under
§IX.1 rule 6 ("no new mechanism") and is left for whoever picks up polling infrastructure
next (IX-M4 empties `docs/adr/0064-fixed-waits.txt` crate-wide; this crate's four `sleep()`
call sites there are still open entries).

This crate also never gained a `tack-test-support` dev-dependency: every real call site in
`crates/tack-cli/tests/*.rs` drives a real `tack` subprocess and real HTTP, or (in
`cli_test.rs`) a `wiremock` server — none of them touch a SQLite pool or a controllable
clock directly, so `tack-test-support`'s `setup_test_db`/`ControllableClock`/fixture-loading
have no call site here to wire in. Adding the dependency unused would itself be new-mechanism
scope creep the card's own instructions warned against.

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree:

```
✓ maintainability budgets hold (5 files checked)
```

`python3 scripts/maintainability.py measure --totals`:

- Before: `prod=56223 (comments 11931) test=73734 ratio=1.311 tests=1498 sleeps_in_tests=74 env_gated=0`
- After: `prod=56223 (comments 11931) test=73729 ratio=1.311 tests=1498 sleeps_in_tests=74 env_gated=0`

## What was removed

- `free_port() -> u16` in `crates/tack-cli/tests/e6_scheduler_e2e_test.rs`,
  `embedded_runner_live_secret.rs`, `embedded_runner_orphaned_credential.rs` and
  `embedded_runner_state_scoping.rs`: four byte-identical (module-doc-comment-only
  differences) copies of the same ephemeral-port helper, replaced by one definition in
  `crates/tack-cli/tests/common/mod.rs`. Reason class: plan §2.1 ("`tests/common/mod.rs` —
  the ONLY place a helper lives"), the same class IX-M3-db used for its four `tack-db`
  helpers. This is a helper, not a test, so none of §2.2's per-test reason codes apply —
  no test body, name, or assertion changed.

## Re-baselined?

`no`. `check --changed` reported budgets holding on the first run after staging the new
file; no file was brought down from an existing over-budget state, so there is nothing to
re-baseline.

## Context spent

- Tokens read before the first edit (cold start): `TODO.md` §IX.0–§IX.4 (~6k tokens:
  cold-start capsule, §IX.1 rules, §IX.2 ownership, §IX.3 dependency graph, the IX-M3 card
  block), plan `docs/plans/human-maintainability.md` §2.1–§2.2 (~1.5k), both handoff
  templates (`part-ix/TEMPLATE.md`, `part-vi/TEMPLATE.md`), the sibling `IX-M3-db.md`
  handoff (read after the fact, to match its "final SHA" convention and its measured-number
  format), and all five `crates/tack-cli/tests/*.rs` files in full (1940 lines total — small
  enough to skim rather than grep-sample). Roughly 15–18k tokens total, most of it the five
  test files themselves.
- Context size at handoff: moderate; no full-file reads of unrelated crates.
- Files opened and not used (each one is a finding for the dispatch README): none — every
  file read fed directly into either the grep confirmation, the consolidation, or the gate
  commands.
- Read-list lines that were wrong (a range that missed, a size that was off): the task
  brief's claim that the three literal Appendix A greps return zero matches held exactly;
  what the brief flagged as "look broader" surfaced real but narrower duplication than a
  first read suggested — `wait_for_ready`/`wait_for_active_runner` looked like consolidation
  candidates on first read (same shape, cross-referencing doc comments) and were only ruled
  out after attempting the move and hitting the ratchet's new-file sleep budget. Worth
  flagging forward: a future card doing this kind of "broader duplication" sweep should
  budget for one wasted consolidation attempt when the duplicate code contains a
  `sleep()`/env-gate/other zero-budget pattern, since the ratchet only surfaces the problem
  after the move is made, not before.

## Amendments

*(none yet)*
