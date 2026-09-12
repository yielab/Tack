# IX-M4-tack-runner-g2_journal_corruption_test handoff

- Base SHA / branch / final SHA: `a08822d` / `develop` (no worktree — done directly by
  the orchestrating session as a light-touch card alongside its sibling
  `IX-M4-tack-runner-bootstrap_entrypoint`) / committed directly on `develop`.
- Files changed (must equal ownership list):
  `crates/tack-runner/tests/g2_journal_corruption_test.rs` only. No file under
  `crates/tack-runner/src/**` touched, and `crates/tack-runner/tests/common/mod.rs` was
  not touched.
- Contract fixtures consumed: none.
- Behavior implemented: none — pruning only, no production code touched.
- Tests added and exact commands/results: none added or removed; all three test
  functions were renamed, and the module doc trimmed. `cargo nextest run --workspace -E
  'binary(g2_journal_corruption_test)'` — `3 tests run: 3 passed, 0 skipped` (same as
  before the card).
- Failure/adversarial case proved: n/a — no assertions changed; the diff is three `fn`
  identifier renames plus a shortened module doc comment.
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
| All 3 tests still pass and prove exactly the same claims as before | `cargo nextest run --workspace -E 'binary(g2_journal_corruption_test)'` — 3/3 pass |
| Every test name is ≤60 characters | max name length (measure's `name` column): 75 → 58 |
| Every test body is ≤60 lines (hard cap; target 40) | already true at card start and untouched: max body 55, unchanged |
| The file preamble is ≤10 lines | 12 → 9 (module doc trimmed by 3 lines) |
| No third-layer over-pinning introduced | `python3 scripts/maintainability.py duplicate-tests` — no `g2_journal_corruption_test` names reported as near-identical to any other file's |
| `cargo fmt --all -- --check` is clean | ran after the commit, no diff |
| `cargo clippy --workspace --all-targets -- -D warnings` is clean | ran after the commit, no warnings |
| `python3 scripts/maintainability.py check --changed` passes | `✓ maintainability budgets hold` |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-runner/tests/g2_journal_corruption_test.rs`

Before (card start, base SHA `a08822d`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/tests/g2_journal_corruption_test.rs          0     0   0%    172    3    33   55   75   12
```

After:

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/tests/g2_journal_corruption_test.rs          0     0   0%    169    3    33   55   58    9
```

Net: 3 fewer test lines (module doc trim), test count and body length unchanged. `max
name` moved 75 → 58; `mdoc` (preamble) moved 12 → 9.

## What was removed

- **`2: name`** — all three test names exceeded 60 characters and were shortened without
  changing the claim:
  - `a_bit_rotted_journal_file_is_a_typed_malformed_error_not_a_panic` (64) →
    `a_bit_rotted_journal_file_is_malformed_not_a_panic` (50) — "typed" and "error" are
    redundant with "Malformed" (the typed error variant asserted in the body).
  - `one_corrupted_journal_file_currently_blocks_recovery_of_every_other_attempt` (75) →
    `a_corrupted_journal_file_blocks_recovery_of_other_attempts` (58) — "currently" and
    "every" dropped as narrative, not claim.
  - `a_truncated_zero_length_journal_file_is_malformed_not_missing` (61) →
    `a_truncated_journal_file_is_malformed_not_missing` (49) — "zero_length" is redundant
    with "truncated" for this claim.
- **`6: preamble`** — the file's module doc (a `//!` block) was 12 lines against a
  10-line cap. Condensed from two paragraphs into one shorter pair by cutting repeated
  framing ("This file targets a case neither covers") while keeping every substantive
  fact: what this file covers, what it doesn't duplicate, and why (corruption from
  outside the journal API, not the in-module symlink/mismatch cases already covered
  elsewhere).
- No `1` (variants → rows) applied — the three tests exercise three genuinely distinct
  corruption shapes (bit-rotted content, batch-scan abort-on-first-error, zero-length
  truncation), not a family of near-identical cases.
- No `3` (body) applied — every body was already ≤55 lines (target 40, hard cap 60).
- No `5` (third-layer over-pinning) or `8` (fixed wait) applied — `duplicate-tests`
  reports nothing for this file, and it had no `sleep(` calls before or after.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check --changed` passes on the final tree
without touching `scripts/maintainability-baseline.json`.

## Budget check

```
✓ maintainability budgets hold
```

`cargo fmt --all -- --check`: clean. `cargo nextest run --workspace -E
'binary(g2_journal_corruption_test)'`: `3 tests run: 3 passed, 0 skipped`. `cargo
clippy --workspace --all-targets -- -D warnings`: clean.

## What a stranger still cannot do

Unchanged from before this card: the file still documents, but does not fix, the
finding that one corrupted journal file currently blocks `unresolved()`'s recovery scan
for every other attempt on the same runner restart — that remains explicitly out of
this file's scope (tests/audit only) per its own module doc, and this card did not
touch that boundary.

## Context spent

- This card was small enough (one file, three over-length names, a slightly
  over-budget module doc, everything else already within budget) that it was handled
  directly by the orchestrating session rather than dispatched to a background agent,
  alongside its sibling `IX-M4-tack-runner-bootstrap_entrypoint`.
- Files opened and not used: none.
- Read-list lines that were wrong: n/a — `measure`'s reported counts matched exactly.

## Amendments

*(none yet)*
