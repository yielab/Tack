# IX-M4-tack-runner-h3_checkout handoff

- Base SHA / branch / final SHA: `a08822d` / `agent/ix-m4-runner-h3_checkout` / `4023ad4`
- Files changed (must equal ownership list): `crates/tack-runner/tests/h3_checkout.rs` —
  confirmed by `git diff --stat a08822d..HEAD --name-only`. No file under
  `crates/tack-runner/src/**` touched, and `crates/tack-runner/tests/common/mod.rs` was
  read but not edited (its two helpers, `temp_dir`/`usage`, were already imported and
  needed no changes).
- Contract fixtures consumed: none new — the two `docs/contracts/runner-v1/` fixtures
  this file already `include_str!`s (`claim.response.json`,
  `recovery-observation.response.json`) are unchanged and still referenced identically.
- Behavior implemented: none — pruning only, no production code touched.
- Tests added and exact commands/results: none added or removed; two of the six test
  functions were renamed only. `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-runner-h3_checkout
  cargo nextest run --workspace -E 'binary(h3_checkout)'` — `6 tests run: 6 passed, 0
  skipped` (same as before the card).
- Failure/adversarial case proved: n/a — no assertions changed; the diff is two `async fn`
  identifier renames.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: `crash_matrix.rs`, the sibling test binary
  in this same crate, defines its own separate `FakeProtocol` (confirmed still true:
  `grep -n "struct FakeProtocol" crates/tack-runner/tests/*.rs` hits both
  `h3_checkout.rs:171` and `crash_matrix.rs:58`), and the two are not drop-in
  replacements for each other — `crash_matrix`'s carries failure-injection and evidence
  fields (`failure`, `evidence`, `recovery_failures_remaining`) this file has no use for,
  while this file's fixture-building helpers (`work`, `session`, `claim`,
  `actual_execution`) have same-named but differently-signatured counterparts in
  `crash_matrix.rs` too. Pre-existing duplication across two files, flagged by the
  sibling `IX-M4-tack-runner-crash_matrix` card and re-confirmed here; out of scope for
  both single-file cards, worth flagging for whoever next touches `tests/common/mod.rs`
  in this crate.
- Secrets/logging review: n/a — test-only change, no logging or secret-handling
  production code touched. The fake `RunnerCredential::new("test-secret-never-log")`
  literal is unchanged from the original file.
- Safe merge order and likely conflicts: independent of every other IX-M4 sub-card
  (single-file ownership, no shared symbols with any other binary). No conflicts
  expected against `develop`.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| All 6 tests still pass and prove exactly the same claims as before | `cargo nextest run --workspace -E 'binary(h3_checkout)'` — 6/6 pass; diff is a two-line rename, nothing else changed |
| No `sleep(` anywhere in this file (already true at card start) | `grep -n "sleep(" crates/tack-runner/tests/h3_checkout.rs` — no hits, before and after |
| Every test name is ≤60 characters | max name length (measure's `name` column): 70 → 60 |
| Every test body is ≤60 lines (hard cap; target 40) | already true at card start and untouched: max body (`max` column) 55, average (`avg`) 39, both unchanged |
| The file preamble is ≤10 lines | already true at card start and untouched: `mdoc` column 9, unchanged |
| No third-layer over-pinning introduced | `python3 scripts/maintainability.py duplicate-tests` — no `h3_checkout` names reported as near-identical to any other file's, before or after |
| `cargo fmt --all -- --check` is clean | ran after the final commit, no diff |
| `cargo clippy --workspace --all-targets -- -D warnings` is clean | ran after the final commit, no warnings |
| `python3 scripts/maintainability.py check --changed` passes | `✓ maintainability budgets hold (1 files checked)` |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-runner/tests/h3_checkout.rs`

Before (card start, base SHA `a08822d`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/tests/h3_checkout.rs                         0     0   0%    664    6    39   55   70    9
totals: prod=0 (comments 0) test=664 ratio=0.0 tests=6 sleeps_in_tests=0 env_gated=0
```

After (this handoff, final SHA `4023ad4`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/tests/h3_checkout.rs                         0     0   0%    664    6    39   55   60    9
totals: prod=0 (comments 0) test=664 ratio=0.0 tests=6 sleeps_in_tests=0 env_gated=0
```

Net: file line count, test count, avg body length and max body length are all unchanged
(664 lines, 6 tests, avg 39, max body 55) — this file was already within every budget
except the name cap. Only `max name` moved: 70 → 60 (the two over-length names, at 70
and 69 chars, were the only ones above the cap; the surviving longest name,
`a_checkout_left_by_a_killed_runner_is_removed_by_the_restart` at exactly 60, was already
at the limit and untouched).

## What was removed

Nothing was removed — this file had no test to merge, no body over the line cap, no
fixed wait, and no preamble to shrink. The only change was a rename:

- **`2: name`** — two of the six test names exceeded 60 characters:
  `a_claimed_attempt_reaches_a_real_harness_process_with_its_own_checkout` (70) →
  `a_claimed_attempt_reaches_the_harness_in_its_own_checkout` (57), dropping "real" and
  "process" — the surrounding module preamble and this file's own doc comment already
  establish that the harness process is real, not mocked, so the word was narrative
  rather than part of the claim.
  `without_a_real_provisioner_the_same_attempt_never_reaches_the_harness` (69) →
  `without_a_provisioner_the_attempt_never_reaches_the_harness` (59), dropping "real" and
  "same" for the same reason. Both renames preserve the exact claim the test proves; no
  assertion, fixture, or comment changed.
- No `1` (variants → rows) applied — all six tests prove genuinely different invariants
  (real checkout reachable, checkout unreachable without a provisioner, checkout cleaned
  up on completion, on cancellation, on restart-after-kill, and kept on quarantine); none
  are a family of near-identical cases.
- No `3` (body) applied — every body was already ≤55 lines (target 40, hard cap 60).
- No `5` (third-layer over-pinning) applied — `duplicate-tests` reports no near-identical
  names against this file, before or after.
- No `6` (preamble) or `8` (fixed wait) applied — the 9-line preamble was already under
  budget and untouched; no fixed waits existed at card start (confirmed by grep both
  before and after).

## Re-baselined?

`no`. `python3 scripts/maintainability.py check --changed` passes on the final tree
without touching `scripts/maintainability-baseline.json`. This file already met every
budget except the name cap, and now meets that one too, so no baseline entry needed to
move.

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree (working tree
clean, this card's commit landed):

```
✓ maintainability budgets hold (1 files checked)
```

`python3 scripts/maintainability.py measure crates/tack-runner/tests/h3_checkout.rs`
before/after: see *Measured numbers* above (664 → 664 test lines, 6 → 6 tests, max body
55 → 55, max name 70 → 60, preamble 9 → 9).

`cargo fmt --all -- --check`: clean. `cargo nextest run --workspace -E
'binary(h3_checkout)'`: `6 tests run: 6 passed, 0 skipped`. `cargo clippy --workspace
--all-targets -- -D warnings`: clean. `python3 scripts/maintainability.py
duplicate-tests`: no `h3_checkout` names flagged.

## What a stranger still cannot do

A stranger reading this file would not learn, from `h3_checkout.rs` alone, that
`crash_matrix.rs` in the same crate defines its own separate `FakeProtocol`, and
same-named-but-differently-shaped `work`/`session`/`claim`/`actual_execution` helpers,
rather than sharing this file's — the two have diverged because each was written for its
own binary's scenarios (this file needs a fixture retargetable at a real on-disk
repository; `crash_matrix` needs failure injection and call-count evidence). Nothing in
this card unifies them; doing so would mean editing `crash_matrix.rs`, which is outside
this card's ownership list (a single named file). A stranger trying to add a new
checkout-lifecycle scenario that also needs crash-injection behavior would have to
duplicate work across two fakes rather than extend one.

## Context spent

- Tokens read before the first edit (cold start): the card's own dispatch prompt
  (self-contained: file path, line/test/name counts, and the acceptance rules were all
  given directly), the relevant slice of `docs/plans/human-maintainability.md` (§1-2.2),
  this crate's `tests/common/mod.rs` in full, and `h3_checkout.rs` in full before any
  edit.
- Context size at handoff: small — one file, two single-line renames, no restructuring
  needed since the file was already within every other budget.
- Files opened and not used: none — `crash_matrix.rs` was grepped (not read in full) only
  to confirm the pre-flagged `FakeProtocol` duplication was still accurate, and that grep
  was used, not wasted, since it is cited above.
- Read-list lines that were wrong: n/a — the card prompt's line/test/name counts (664
  lines, 6 tests, max name 70, max body 55, preamble 9) all matched what
  `scripts/maintainability.py measure` reported at card start.
- One finding worth flagging: this card's own instructions anticipated a "light touch"
  given the file was already close to budget, and that held exactly — the only rule this
  file broke was the 60-character name cap, on 2 of 6 names, and fixing it required no
  change to test bodies, helpers, fixtures, or the preamble. Not every IX-M4 sub-card
  needs a rewrite; some need a rename.

## Amendments

*(none yet)*
