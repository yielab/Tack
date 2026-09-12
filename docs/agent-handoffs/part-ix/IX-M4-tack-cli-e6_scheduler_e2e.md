# IX-M4-tack-cli-e6_scheduler_e2e handoff

- Base SHA / branch / final SHA: `a08822d` / `agent/ix-m4-cli-e6_scheduler_e2e` / `9c83937`
- Files changed (must equal ownership list): `crates/tack-cli/tests/e6_scheduler_e2e_test.rs`
  only — confirmed by `git diff --stat a08822d..HEAD --name-only`. No file under
  `crates/tack-cli/src/**` was touched (`git diff --stat a08822d..HEAD --
  crates/tack-cli/src/` empty). `crates/tack-cli/tests/common/mod.rs` was read (for its
  existing `free_port` helper, already used by this file) but not edited.
- Contract fixtures consumed: none (`docs/contracts/runner-v1/` not touched).
- Behavior implemented: none — pruning only. All 5 tests assert exactly what they did
  before this card.
- Tests added and exact commands/results: none added; count unchanged at 5.
  `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-cli-e6_scheduler_e2e cargo nextest run
  --workspace -E 'binary(e6_scheduler_e2e_test)'` — `5 tests run: 5 passed, 0 skipped`,
  both before and after the edits, and again on the final tree. The whole binary was
  additionally stress-run 20x in a loop after the edits (`cargo nextest run` per
  iteration, not per-test isolation, since the file's own tests each spawn a private
  `tack serve` subprocess and do not share state) — 20/20 green.
- Failure/adversarial case proved: n/a — pruning only, no new behavior. The five
  original claims (healthy fleet selection, saturation, exact-runner exclusion,
  unsupported-model rejection, changed-payload idempotency conflict) are unchanged.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none new. `docs/adr/0064-fixed-waits.txt`
  was not committed — regenerating it showed drift this card did not cause (see
  *Re-baselined?*).
- Secrets/logging review: n/a — test-only change; no logging or secret-handling
  production code touched. `OPERATOR_TOKEN`, the capability-report fixture and the
  runner-protocol HTTP calls are byte-identical to before this card.
- Safe merge order and likely conflicts: independent of every other IX-M4 sub-card
  (this card owns the one file in this binary; no other open sub-card touches
  `e6_scheduler_e2e_test.rs` or `crates/tack-cli/tests/common/mod.rs`). No conflicts
  expected against `develop`.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| All 5 tests' behavior is unchanged | `cargo nextest run --workspace -E 'binary(e6_scheduler_e2e_test)'` — 5/5 pass, same assertions as before |
| No flakiness introduced by the edits | 20x stress loop of the whole binary, 20/20 green (100 individual test executions) |
| Every test name is ≤60 chars | `measure`'s `name` column: 53 (was 82) |
| The file's preamble is ≤10 lines | `measure`'s `mdoc` column: 8 (was 16) |
| Every test body is ≤60 lines (hard cap) | `measure`'s `max` column: 60, unchanged — already at the hard cap before this card, not touched (see *What was removed*) |
| The one `sleep(` in the file is a bounded poll of a real condition, not a fixed wait | `python3 scripts/list-fixed-waits.py \| grep e6_scheduler` — no hit, both before and after (100ms interval, below the 200ms ADR threshold, and the surrounding loop already re-checks the health endpoint and the child's exit status each iteration) |
| No third-layer over-pinning introduced | `python3 scripts/maintainability.py duplicate-tests crates/tack-cli/tests/e6_scheduler_e2e_test.rs` — `0 near-identical pairs across files` |
| `cargo fmt --all -- --check` is clean | ran after the commit, no diff |
| `cargo clippy --workspace --all-targets -- -D warnings` is clean | ran after the commit, no warnings |
| `scripts/check-comments.sh` and `scripts/check-test-hygiene.sh` are clean | both ran green after the commit |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-cli/tests/e6_scheduler_e2e_test.rs`

Before (measured at base SHA `a08822d` via a scratch `git worktree add
/tmp/ix-m4-base-check2 a08822d`, run from inside that worktree since the script
resolves paths relative to its own cwd):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-cli/tests/e6_scheduler_e2e_test.rs                  0     0   0%    585    5    44   60   82   16
totals: prod=0 (comments 0) test=585 ratio=0.0 tests=5 sleeps_in_tests=1 env_gated=0
```

After (this handoff, final SHA `9c83937`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-cli/tests/e6_scheduler_e2e_test.rs                  0     0   0%    580    5    44   60   53    8
totals: prod=0 (comments 0) test=580 ratio=0.0 tests=5 sleeps_in_tests=1 env_gated=0
```

Net: 585 → 580 test lines (−5), 5 tests unchanged, `name` 82 → 53 (under the 60-char
cap), `mdoc` 16 → 8 (under the 10-line cap), `max` unchanged at 60 (already at, not
over, the hard cap — see below), `sleeps_in_tests` unchanged at 1 (the one poll-interval
sleep was kept, not removed — see *What was removed*, rule 8).

`python3 scripts/maintainability.py measure --totals` was not re-run workspace-wide:
this card's own file-scoped before/after fully accounts for its delta, and a
workspace total captured now would mix in unrelated concurrent IX-M4 sub-card work
landing on `develop` (the same drift documented under *Re-baselined?* below), making
a "before vs. after" comparison at that granularity misleading rather than useful for
this handoff.

## What was removed

**`crates/tack-cli/tests/e6_scheduler_e2e_test.rs`** (585 → 580 lines, 5 tests unchanged):

- `6` (preamble): 16 → 8 lines. The original module doc restated the same three facts
  (real subprocess, real router, CLI-has-no-runner-commands-so-HTTP-direct) across
  eleven sentences with some redundant phrasing ("a real SQLite file, the real
  production router — not a stand-in, not a mock" collapsed to "a real SQLite file and
  the production router — not a mock"); condensed to one paragraph stating what the
  file proves and the one non-obvious methodology fact (CLI has no runner-protocol
  commands) without dropping any claim.
- `2` (name), three instances:
  - `healthy_runner_claims_a_cli_created_request_and_the_cli_observes_it` (67 chars) →
    `eligible_runner_claims_request_and_cli_sees_it_leased` (53).
  - `an_exact_runner_request_is_never_claimed_by_a_different_runner` (62) →
    `exact_runner_selector_excludes_every_other_runner` (49, drops the leading article
    per rule 2).
  - `duplicate_idempotency_key_with_a_different_payload_is_a_named_conflict_via_the_cli`
    (82) → `changed_payload_replay_returns_idempotency_conflict` (51, drops "duplicate
    ... with a different payload" narrative phrasing for the same claim stated
    directly).

**Not changed, and why:**

- **Rule 3 (body ≤60 lines):** every body was already at or under the 60-line hard cap
  (31/47/36/40/58 lines; `measure`'s `max` column reports 60 for the longest,
  `duplicate_idempotency_key_...` — its actual body is 58 lines by direct count, the
  1-2 line difference is the script counting the closing brace/attribute lines
  differently, not a discrepancy worth chasing). None is over the hard cap, so rule 3
  required no extraction. All five remain above the 40-line *target*, which the plan
  states as aspirational until M8 and not itself a blocking condition when the hard cap
  already holds.
- **Rule 8 (no fixed waits), the `wait_for_ready` sleep at (pre-card) line 106:** this
  card's dispatch prompt named this line as "the one fixed wait to remove" and
  suggested replacing it with a bounded poll of the real condition. Reading the
  surrounding function shows it already *is* that bounded poll: the loop checks the
  server's `/api/health` endpoint for success, checks the child process for an early
  exit, checks a 15-second deadline, and only then sleeps 100ms before the next
  iteration — the sleep is the poll interval, not a substitute for the condition check.
  `scripts/list-fixed-waits.py` (the tool `docs/adr/0064-fixed-waits.txt` is built from)
  does not flag this line either before or after this card, both because it sits below
  the script's 200ms threshold and, more to the point, because the loop's structure is
  exactly the accepted pattern from §2.2 rule 8 ("poll with a bound"). Rather than
  restructure working, already-compliant code to match a dispatch prompt's assumption,
  this card left the logic as-is and added a one-line inline comment at the sleep call
  itself, making explicit (for the next reader, and for whoever wrote the dispatch
  prompt from a template) that it is a poll interval bounded by the condition check
  above it, not a fixed wait. This judgment call is the "anything non-obvious you
  judged either way" the dispatching session asked to be told about.
- **Rule 5 (third-layer duplicate pinning):** `python3 scripts/maintainability.py
  duplicate-tests crates/tack-cli/tests/e6_scheduler_e2e_test.rs` reports 0 pairs. This
  file's own module doc already argues (and this card's reading confirms) that it is a
  CLI-level wiring proof, not a duplicate of `tack-orch`'s or `tack-api`'s lower-layer
  scheduler unit tests — same underlying scheduling logic, different claim (the CLI's
  production entry points reach that logic end-to-end).

## Re-baselined?

`no`. `python3 scripts/maintainability.py check crates/tack-cli/tests/e6_scheduler_e2e_test.rs`
→ `✓ maintainability budgets hold (1 files checked)` on the final tree, with
`scripts/maintainability-baseline.json` untouched.

`docs/adr/0064-fixed-waits.txt` was regenerated (`python3 scripts/list-fixed-waits.py`)
and diffed against the committed copy to check for this card's effect. This card's own
file produces zero entries in that inventory, both before and after (its one sleep sits
below the 200ms threshold), so this card had nothing to remove from the list. The diff
was not small, however: the committed file lists 25 waits across `tack-orch` and
`tack-api` files this card never touched, while the freshly regenerated version lists
only 2 (one of them — `crates/tack-orch/tests/docket_live_test.rs:171` — not present in
the committed file at all). None of the changed lines are anywhere near this card's
scope (`crates/tack-cli/tests/e6_scheduler_e2e_test.rs` never appears in either list).
This is unrelated concurrent IX-M4/other-card work landing on `develop` between the
committed file's last regeneration and now, per the same pattern
`IX-M4-tack-cli-embedded_runner.md` already documented for its own sibling drift. Not
committed; whoever regenerates it after this branch (and the others causing the drift)
merge will pick up a clean, accurate inventory.

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree (commit landed,
working tree clean, so nothing shows as "changed"):

```
✓ maintainability budgets hold (0 files checked)
```

`python3 scripts/maintainability.py check crates/tack-cli/tests/e6_scheduler_e2e_test.rs`
(explicit path, since `--changed` reports 0 once committed):

```
✓ maintainability budgets hold (1 files checked)
```

`python3 scripts/maintainability.py measure crates/tack-cli/tests/e6_scheduler_e2e_test.rs`
before/after — see *Measured numbers* above (before: `test=585 ... name=82 mdoc=16`;
after: `test=580 ... name=53 mdoc=8`).

`cargo fmt --all -- --check`: clean. `cargo nextest run --workspace -E
'binary(e6_scheduler_e2e_test)'`: `5 tests run: 5 passed, 0 skipped`, plus a 20x
stress loop of the whole binary, 20/20 green. `cargo clippy --workspace --all-targets
-- -D warnings`: clean. `scripts/check-comments.sh` and `scripts/check-test-hygiene.sh`:
both clean. `python3 scripts/maintainability.py duplicate-tests
crates/tack-cli/tests/e6_scheduler_e2e_test.rs`: `0 near-identical pairs across files`.

## What a stranger still cannot do

A stranger reading this file still cannot tell, from the code alone, that the file's
five bodies were deliberately left above the 40-line target (31-58 lines) rather than
split further — that judgment (one continuous end-to-end scheduling claim per test,
matching the precedent set by other IX-M4 CLI/API sub-cards for real-subprocess tests)
lives only in this handoff, not in a code comment. They also cannot tell that the
`wait_for_ready` sleep was flagged by this card's own dispatch instructions as a defect
and deliberately left in place after inspection showed it was already correct — that
reasoning is now a one-line inline comment at the call site plus this handoff, not
restated anywhere else.

## Context spent

- Tokens read before the first edit (cold start): the card's own dispatch prompt
  (self-contained), `docs/plans/human-maintainability.md` §2.1-2.2, `crates/tack-cli/
  tests/common/mod.rs` in full (10 lines), and the full target file (585 lines). Low
  cold start for a single-file, 5-test binary.
- Context size at handoff: low. The file was edited in place with five small, targeted
  edits (preamble + three renames + one inline comment); no full rewrite was needed.
- Files opened and not used: `docs/agent-handoffs/part-ix/IX-M4-tack-cli-embedded_runner.md`
  was read for handoff-structure and judgment-call precedent (particularly the "sleep
  already bounded, kept as-is" and "ADR drift from concurrent work, not committed"
  patterns) and directly informed this handoff's *What was removed* and *Re-baselined?*
  sections.
- Read-list lines that were wrong: the dispatch prompt's claim that line ~106's sleep
  was "a fixed wait to remove" did not hold up against the actual code — it was already
  a bounded, condition-checked poll. This is the same shape of mismatch
  `IX-M4-tack-cli-embedded_runner.md` reported for its own three files (all four/three
  of its sleeps were "already inside bounded, condition-checked poll loops — not blind
  waits"), suggesting the IX-M4 dispatch prompts across this binary set were generated
  from a template that assumed naive fixed sleeps without reading the actual loops
  first. Worth flagging for whoever writes the next wave of dispatch prompts: grep for
  `sleep(` finds the call sites, but not whether they're already inside a poll.

## Amendments

*(none yet)*
