# IX-M4-tack-runner-crash_matrix handoff

- Base SHA / branch / final SHA: `ddd9f88` / `agent/ix-m4-runner-crash_matrix` / `a68d5b9`
- Files changed (must equal ownership list): `crates/tack-runner/tests/crash_matrix.rs` —
  confirmed by `git diff --stat ddd9f88..HEAD --name-only`. No file under
  `crates/tack-runner/src/**` touched, and `crates/tack-runner/tests/common/mod.rs`
  was read but not edited (its two helpers, `temp_dir`/`usage`, were already imported
  and needed no changes).
- Contract fixtures consumed: none new — the two `docs/contracts/runner-v1/` fixtures
  this file already `include_str!`s (`claim.response.json`,
  `recovery-observation.response.json`) are unchanged and still referenced identically.
- Behavior implemented: none — pruning only, no production code touched.
- Tests added and exact commands/results: none added; two merged into one. The
  `completion_response_loss_stays_in_terminal_outbox_without_duplicate_send` and
  `cancellation_response_loss_stays_in_terminal_outbox_without_duplicate_send` tests
  were a near-identical pair (same setup shape, same `TerminalReportPending` outcome,
  differing only in which `FailurePoint` fires and which counters that implies) and
  became one table-driven `terminal_report_loss_keeps_attempt_pending_without_resend`
  with two rows. `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-runner-crash_matrix cargo
  nextest run --workspace -E 'binary(crash_matrix)'` — `6 tests run: 6 passed, 0
  skipped` (was 7 before the merge).
- Failure/adversarial case proved: n/a — pruning only; every original assertion was
  carried over onto its equivalent case row or helper call, verified by reading the
  diff line-by-line against the pre-card file rather than by re-deriving the claims.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: `h3_checkout.rs`, the sibling test binary
  in this same crate, defines its own separate `FakeProtocol` (grep-confirmed:
  `crates/tack-runner/tests/h3_checkout.rs:171`) rather than sharing this file's fakes
  or a `tests/common` version. That is pre-existing duplication across two files this
  card does not own — out of scope here (this card owns only `crash_matrix.rs`) but
  worth flagging for whoever next touches `tests/common/mod.rs` in this crate.
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
| All 6 tests pass and prove the same claims the original 7 did | `cargo nextest run --workspace -E 'binary(crash_matrix)'` — 6/6 pass; diff read in full against the pre-card content |
| No `sleep(` anywhere in this file (already true at card start) | `grep -n "sleep(" crates/tack-runner/tests/crash_matrix.rs` — no hits, before and after |
| Every test name is ≤60 characters | max name length (measure's `name` column): 84 → 58 |
| Every test body is ≤60 lines (hard cap; target 40 for most) | max body length (measure's `max` column): 84 → 56; average (`avg`): 55 → 39 |
| The file preamble is ≤10 lines | added a 4-line `//!` preamble (was none); `mdoc` column: 0 → 4 |
| The completion/cancellation-loss pair is a genuine variant family, not a forced merge | both tests built the identical engine shape, asserted the identical `TerminalReportPending` outcome, and differed only in which `FailurePoint`/counters applied — the table-driven replacement asserts every original value, per-case, with the case label in every failure message |
| The other 5 tests were correctly kept separate, not force-merged | each enters the crash from a different code path (worktree failure inside `run_once`, an ack failure inside `run_once`, a pre-seeded journal entry consumed only by `recover()`, a two-phase retry-then-restart, and a reoffer-after-quarantine idempotency check) — merging them would require branching control flow inside a shared loop body, which the plan's rule 1 does not ask for when the *setup itself* differs, not just the input/expected-output |
| No third-layer over-pinning introduced | `python3 scripts/maintainability.py duplicate-tests` — no crash_matrix test names reported as near-identical to any other file's |
| `cargo fmt --all -- --check` is clean | ran after the final commit, no diff |
| `cargo clippy --workspace --all-targets -- -D warnings` is clean | ran after the final commit, no warnings |
| `scripts/check-comments.sh` and `scripts/check-test-hygiene.sh` are clean | both ran green after the final commit |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-runner/tests/crash_matrix.rs`

Before (card start, base SHA `ddd9f88`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/tests/crash_matrix.rs                        0     0   0%    811    7    55   84   84    0
totals: prod=0 (comments 0) test=811 ratio=0.0 tests=7 sleeps_in_tests=0 env_gated=0
```

After (this handoff, final SHA `a68d5b9`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-runner/tests/crash_matrix.rs                        0     0   0%    826    6    39   56   58    4
totals: prod=0 (comments 0) test=826 ratio=0.0 tests=6 sleeps_in_tests=0 env_gated=0
```

Net: 811 → 826 test lines (+15 overall, despite dropping one test) — the fake
protocol/adapter grew 7 small accessor methods (`has_event`, `event_count`,
`recovery_reports`, `completion_reports`, `cancellation_reports` on `FakeProtocol`;
`starts`, `cancels`, `reconciles` on `FakeAdapter`) and the file gained two new helper
functions (`engine`, `assert_quarantine_recorded`) plus a third
(`recover_and_expect_quarantine`) — all counted as "test" lines by the tool since this
whole file lives under `tests/`. These accessors and helpers are what let every
`#[tokio::test]` body drop its repeated `.lock().expect(...)/drop(...)` boilerplate and
its repeated 5-argument `engine(...)` construction, which is why the *max* and *avg*
per-test numbers fell sharply even though total file lines rose slightly. Test count: 7
→ 6 (the completion/cancellation-loss merge). `sleeps_in_tests`: 0 → 0 (none to begin
with). Max body: 84 → 56 (was over the 60-line hard cap; now under it). Max name: 84 →
58 (was over the 60-char cap on all 7 names; now under it on all 6). Preamble (`mdoc`):
0 → 4 (added, still under the 10-line budget).

## What was removed

One test was removed as a merge, not a deletion — its assertions all survive as rows:

- **`2: name`** — every one of the original 7 test names exceeded 60 characters (69-84
  chars). All 6 surviving names were shortened to state the same claim in ≤58 chars
  (e.g. `after_claim_before_spawn_failure_recovers_as_process_stopped_without_respawn`
  (76) → `before_spawn_worktree_crash_recovers_without_respawn` (52);
  `process_running_recovery_observation_reports_ambiguity_and_quarantines_without_spawn`
  (84, the file's longest) → `running_process_recovery_quarantines_without_respawn`
  (52)).
- **`1: variants → rows`** — `completion_response_loss_stays_in_terminal_outbox_without_duplicate_send`
  and `cancellation_response_loss_stays_in_terminal_outbox_without_duplicate_send`
  merged into `terminal_report_loss_keeps_attempt_pending_without_resend`, a
  table-driven test over a `TerminalLossCase` struct with one row per failure point.
  Both original tests built the identical engine shape and asserted the identical
  `TerminalReportPending` outcome; they differed only in which `FailurePoint` fired,
  whether `cancellation_requested` was set, and which counters (`completion_reports` vs
  `cancellation_reports`, `cancels`) were expected to move. Every original assertion
  is still made, per-case, with the case label attached to every failure message so a
  regression still names which row broke.
- **`3: body`** — 3 of the 5 non-merged tests exceeded the 60-line hard cap
  (`failed_ambiguity_report_...` 84, `process_running_recovery_...` ~68,
  `reoffered_quarantined_attempt_...` ~70). Shrunk via: (a) a shared `engine(...)`
  builder replacing the repeated 5-6 line `RunnerEngine::new(protocol.clone(),
  adapter.clone(), journal.clone(), WorkspaceManager::new(...))` block (10 call sites
  across the file); (b) a shared `recover_and_expect_quarantine(...)` helper replacing
  the repeated "build engine, call `.recover()`, assert exactly one `Quarantined`
  outcome" sequence (used by 2 tests); (c) new accessor methods on `FakeProtocol`/
  `FakeAdapter` replacing the repeated `.evidence.lock().expect("...")...; drop(...)`
  triples with one-line calls (`adapter.starts()` instead of
  `{ let process = adapter.evidence.lock().expect("process evidence"); ...; drop(process); }`).
  The table-driven test's own loop body was further extracted into a private
  `assert_terminal_loss(case)` async helper so the `#[tokio::test]`-annotated function
  itself stays a thin case-array-plus-loop.
- No `5` (third-layer over-pinning) applied — see the duplicate-tests evidence above;
  nothing in this file pins an invariant already proved at another layer, and
  `tack-runner` owns this crash-recovery decision logic itself (as CLAUDE.md's
  architecture section notes for the runner crate generally).
- No `6` (preamble) or `8` (fixed wait) applied — the file had no preamble to shrink
  (0 lines, now 4, still well under budget) and no fixed waits existed at card start
  (confirmed by grep both before and after).

### Rule 1 — why the other 5 tests were NOT merged into the table

The card prompt asked to "look hard for consolidation opportunities" given the
"crash_matrix" name. Beyond the completion/cancellation pair, the remaining 5 tests
were each considered and kept separate:

- `before_spawn_worktree_crash_recovers_without_respawn` and
  `running_process_recovery_quarantines_without_respawn` both end in a `.recover()`
  call and both prove "no respawn happens," but they enter the crash from genuinely
  different code paths — the first crashes via an actual failing `WorktreeProvisioner`
  inside a real `run_once()` call (proving the *engine's own* failure-handling writes
  the right `Prepared` journal state), the second seeds that journal state directly via
  `persist_before_spawn` and never calls `run_once()` at all (proving *recovery alone*,
  independent of how the journal got there). Folding these into one table would require
  a per-case branch on which of two entirely different setup sequences to run, which is
  exactly the kind of setup-varies-not-just-input/output case rule 1 does not ask for.
- `failed_ambiguity_report_retries_once_then_quarantines` is a two-phase test (a first
  `run_once()` that hits `RecoveryPending`, then a restart `recover()` that reaches
  `Quarantined`) proving a retry-exactly-once invariant that no other test in the file
  shares.
- `reoffered_quarantined_attempt_rejected_before_second_spawn` proves an idempotency
  guard (a second `run_once()` on already-quarantined work is rejected before spawning
  anything) that is also unique in this file.

Each remaining test's body was still brought under the line-count budget via the shared
helpers described above, without merging tests that prove genuinely different claims.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check --changed` passes on the final tree
without touching `scripts/maintainability-baseline.json`. This file's own budgets are
now met outright (not merely at-or-under an inherited ceiling), so no baseline entry
needed to move.

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree (working tree
clean, this card's commit landed):

```
✓ maintainability budgets hold (1 files checked)
```

`python3 scripts/maintainability.py measure crates/tack-runner/tests/crash_matrix.rs`
before/after: see *Measured numbers* above (811 → 826 test lines, 7 → 6 tests, max body
84 → 56, max name 84 → 58, preamble 0 → 4).

`cargo fmt --all -- --check`: clean. `cargo nextest run --workspace -E
'binary(crash_matrix)'`: `6 tests run: 6 passed, 0 skipped`. `cargo clippy --workspace
--all-targets -- -D warnings`: clean. `scripts/check-comments.sh` and
`scripts/check-test-hygiene.sh`: both clean. `python3 scripts/maintainability.py
duplicate-tests`: no crash_matrix names flagged.

## What a stranger still cannot do

A stranger reading this file would not learn, from `crash_matrix.rs` alone, that
`h3_checkout.rs` in the same crate defines its own separate `FakeProtocol` rather than
sharing this one — the two fakes have diverged (different fields, different failure
injection shapes) because each was written for its own binary's scenarios. Nothing in
this card unifies them; doing so would mean editing `h3_checkout.rs`, which is outside
this card's ownership list. A stranger trying to add a new crash scenario that needs
both files' behavior would have to duplicate work across two fakes rather than extend
one.

## Context spent

- Tokens read before the first edit (cold start): the card's own dispatch prompt
  (self-contained: file path, line/test/name counts, and the acceptance rules were all
  given directly), `docs/plans/human-maintainability.md` §2.1-2.2, this crate's
  `tests/common/mod.rs` in full, and `crash_matrix.rs` in full before any edit.
- Context size at handoff: moderate — one file, rewritten in a few passes (initial
  full rewrite with the merge and helpers, then a second pass specifically targeting
  two test bodies that were still over the 60-line hard cap after the first pass).
- Files opened and not used: `crates/tack-runner/src/engine.rs` was read to confirm
  `RunnerEngine::new`'s exact generic signature before writing the `engine(...)` test
  helper — used, not wasted, since the helper's return type had to match exactly.
  `crates/tack-runner/tests/h3_checkout.rs` was grepped (not read in full) only to
  check whether it shared this file's fake types before deciding the new helpers
  should stay local rather than move to `tests/common`.
- Read-list lines that were wrong: n/a — the card prompt gave exact file scope (a
  single named file) with no line-range hints to verify.
- One finding worth flagging: the maintainability tool's `measure` command counts
  every line under `tests/` as "test" lines regardless of whether it is inside a
  `#[tokio::test]`-annotated function, a private helper, or a fake trait impl — so a
  file's total test-line count can rise even as every individual test body shrinks,
  if the shrinkage comes from moving code into named, reusable helper functions rather
  than deleting it. That is the intended trade per the plan (duplicated inline code
  counted against a test body budget becomes named helper code that isn't), matching
  the same observation the sibling `IX-M4-tack-orch-ingestion` handoff made.

## Amendments

*(none yet)*
