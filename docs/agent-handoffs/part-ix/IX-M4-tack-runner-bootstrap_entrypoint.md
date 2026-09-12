# IX-M4-tack-runner-bootstrap_entrypoint handoff

- Base SHA / branch / final SHA: `a08822d` / `develop` (no worktree — done directly by
  the orchestrating session as a light-touch card, see *Context spent*) / committed
  directly on `develop`.
- Files changed (must equal ownership list): `crates/tack-runner/tests/bootstrap_entrypoint.rs`
  only. No file under `crates/tack-runner/src/**` touched, and
  `crates/tack-runner/tests/common/mod.rs` was not touched.
- Contract fixtures consumed: none.
- Behavior implemented: none — pruning only, no production code touched.
- Tests added and exact commands/results: none added or removed; one of the two test
  functions was renamed only. `cargo nextest run --workspace -E 'binary(bootstrap_entrypoint)'`
  — `2 tests run: 2 passed, 0 skipped` (same as before the card).
- Failure/adversarial case proved: n/a — no assertions changed; the diff is one `async fn`
  identifier rename.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none newly found.
- Secrets/logging review: n/a — test-only change, no logging or secret-handling
  production code touched.
- Safe merge order and likely conflicts: independent of every other IX-M4 sub-card
  (single-file ownership). No conflicts expected against `develop`.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Both tests still pass and prove exactly the same claims as before | `cargo nextest run --workspace -E 'binary(bootstrap_entrypoint)'` — 2/2 pass; diff is a one-line rename |
| Every test name is ≤60 characters | max name length (measure's `name` column): 73 → 60 |
| Every test body is ≤60 lines (hard cap; target 40) | already true at card start and untouched: max body 53, unchanged |
| The file preamble is ≤10 lines | already true at card start and untouched: `mdoc` column 4, unchanged |
| No third-layer over-pinning introduced | `python3 scripts/maintainability.py duplicate-tests` — no `bootstrap_entrypoint` names reported as near-identical to any other file's |
| `cargo fmt --all -- --check` is clean | ran after the commit, no diff |
| `cargo clippy --workspace --all-targets -- -D warnings` is clean | ran after the commit, no warnings |
| `python3 scripts/maintainability.py check --changed` passes | `✓ maintainability budgets hold` |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-runner/tests/bootstrap_entrypoint.rs`

Before (card start, base SHA `a08822d`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/tests/bootstrap_entrypoint.rs                0     0   0%    180    2    37   53   73    4
```

After:

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/tests/bootstrap_entrypoint.rs                0     0   0%    180    2    37   53   60    4
```

Net: line count, test count, and body length are all unchanged. Only `max name` moved:
73 → 60.

## What was removed

Nothing was removed — this file had no test to merge, no body over the line cap, no
fixed wait, and no preamble to shrink. The only change was a rename:

- **`2: name`** — `the_composition_root_stops_on_an_injected_shutdown_with_no_process_signal`
  (73) → `composition_root_stops_on_injected_shutdown_without_a_signal` (60), dropping
  "the"/"an" articles and shortening "with no process signal" to "without a signal" — the
  claim (shutdown is injected programmatically, not delivered as a process signal) is
  unchanged.
- No other rule applied — the file's second test, its preamble, and its lack of fixed
  waits were already within budget and untouched.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check --changed` passes on the final tree
without touching `scripts/maintainability-baseline.json`.

## Budget check

```
✓ maintainability budgets hold
```

`cargo fmt --all -- --check`: clean. `cargo nextest run --workspace -E
'binary(bootstrap_entrypoint)'`: `2 tests run: 2 passed, 0 skipped`. `cargo clippy
--workspace --all-targets -- -D warnings`: clean.

## What a stranger still cannot do

Nothing changed here — this was a single-identifier rename with no behavioral or
structural effect. A stranger reading this file learns exactly what they would have
learned before, just with a test name that fits on one line in most terminals.

## Context spent

- This card was small enough (one file, one over-length name, everything else already
  within budget) that it was handled directly by the orchestrating session rather than
  dispatched to a background agent, alongside its sibling
  `IX-M4-tack-runner-g2_journal_corruption_test`.
- Files opened and not used: none.
- Read-list lines that were wrong: n/a — `measure`'s reported counts matched exactly.

## Amendments

*(none yet)*
