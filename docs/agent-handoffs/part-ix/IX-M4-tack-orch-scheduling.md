# IX-M4-tack-orch-scheduling handoff

- Base SHA / branch / final SHA: `6c52ab4` / `agent/ix-m4-orch-scheduling` / `451859f`
- Files changed (must equal ownership list): `crates/tack-orch/tests/scheduling.rs`,
  `crates/tack-orch/tests/scheduling/wiring.rs`, `crates/tack-orch/tests/scheduling/policy.rs`,
  `crates/tack-orch/tests/scheduling/scheduler.rs` — confirmed by
  `git diff --stat 6c52ab4..HEAD --name-only`. `crates/tack-orch/tests/scheduling/support.rs`
  was read (checked for duplication against `tests/common/mod.rs` and `tack-test-support`,
  found none) but not touched — already within every budget. No file under
  `crates/tack-orch/src/**` was touched.
- Contract fixtures consumed: none (`docs/contracts/runner-v1/` not touched).
- Behavior implemented: none — pruning only.
- Tests added and exact commands/results: none added net-new; six variant/pair
  families in `wiring.rs` and `policy.rs` merged into table-driven or
  shared-helper tests (net −6 tests: 30 → 24).
  `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-orch-scheduling cargo nextest run
  --workspace -E 'binary(scheduling)'` — `24 tests run: 24 passed, 0 skipped`,
  verified after every file's commit.
- Failure/adversarial case proved: n/a — pruning only, no new behavior.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: `policy.rs`'s four precedence-tier
  tests (fleet/project/agent-profile/request-override) were deliberately left
  separate rather than table-driven — see *What was removed*. `scheduler.rs`'s
  ten tests needed only name/preamble trims; every body was already within
  budget at card start.
- Secrets/logging review: n/a — test-only changes, no logging or
  secret-handling production code touched.
- Safe merge order and likely conflicts: independent of every other IX-M4
  sub-card (each owns one binary). No conflicts expected against `develop`
  since only these files were touched.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| All behavior in the 4 changed files is unchanged | `cargo nextest run --workspace -E 'binary(scheduling)'` — 24/24 pass, same assertions per case as before (see *What was removed* for the case-to-case mapping on every merge) |
| No `sleep(` anywhere in this binary | `grep -rn "sleep(" crates/tack-orch/tests/scheduling.rs crates/tack-orch/tests/scheduling/*.rs` — no hits, confirmed both before and after |
| Every test name in this binary is ≤60 chars | `grep -oP '(?<=async fn )\w+\|(?<=^fn )\w+' crates/tack-orch/tests/scheduling/*.rs \| awk '{ if (length($0) > 60) print }'` — empty (was 11 names over budget across the three files: 4 in `wiring.rs`, 4 in `policy.rs`, 7 in `scheduler.rs`, with 4 shared between the sets after renaming both directions is double counted — see per-file counts below) |
| Every file's preamble is ≤10 lines | `measure`'s `mdoc` column, final run: `wiring.rs` 9, `policy.rs` 7, `scheduler.rs` 6, `support.rs` 6, `scheduling.rs` 5 (was `policy.rs` 16, `scheduler.rs` 13) |
| Every test body is ≤60 lines (target 40) | `measure`'s `max` column, final run: `wiring.rs` 36, `policy.rs` 39, `scheduler.rs` 34 (was `wiring.rs` 79, `policy.rs` 87 — both over the hard cap at card start) |
| No production file touched | `git diff --stat 6c52ab4..HEAD -- crates/tack-orch/src/` — empty |
| No third-layer over-pinning introduced | `python3 scripts/maintainability.py duplicate-tests crates/tack-orch/tests/scheduling.rs crates/tack-orch/tests/scheduling/*.rs --top 30` — `0 near-identical pairs across files` |
| `cargo fmt --all -- --check` is clean | ran after the final commit, no diff |
| `cargo clippy --workspace --all-targets -- -D warnings` is clean | ran after the final commit, no warnings |
| `scripts/check-comments.sh` and `scripts/check-test-hygiene.sh` are clean | both ran green after the final commit |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-orch/tests/scheduling.rs
crates/tack-orch/tests/scheduling/*.rs`

Before (measured by checking out base SHA `6c52ab4` into a scratch worktree
since three of the five files had already changed by the time this was run):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-orch/tests/scheduling/wiring.rs                     0     0   0%    679   13    40   79   81    9
crates/tack-orch/tests/scheduling/policy.rs                     0     0   0%    499    7    39   87   66   16
crates/tack-orch/tests/scheduling/scheduler.rs                  0     0   0%    389   10    24   34   73   13
crates/tack-orch/tests/scheduling/support.rs                    0     0   0%    115    0     0    0    0    6
crates/tack-orch/tests/scheduling.rs                            0     0   0%     17    0     0    0    0    5
totals: prod=0 (comments 0) test=1699 ratio=0.0 tests=30 sleeps_in_tests=0 env_gated=0
```

After (this handoff, final SHA `451859f`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-orch/tests/scheduling/wiring.rs                     0     0   0%    497    7    16   36   58    9
crates/tack-orch/tests/scheduling/policy.rs                     0     0   0%    430    7    18   39   50    7
crates/tack-orch/tests/scheduling/scheduler.rs                  0     0   0%    382   10    24   34   58    6
crates/tack-orch/tests/scheduling/support.rs                    0     0   0%    115    0     0    0    0    6
crates/tack-orch/tests/scheduling.rs                            0     0   0%     17    0     0    0    0    5
totals: prod=0 (comments 0) test=1441 ratio=0.0 tests=24 sleeps_in_tests=0 env_gated=0
```

Net: 1699 → 1441 test lines (−258, −15.2%); 30 → 24 tests (−6). Every file's
`max` body ≤60 (was 79 in `wiring.rs`, 87 in `policy.rs`). Every file's `name`
max ≤60 (was 81 in `wiring.rs`, 66 in `policy.rs`, 73 in `scheduler.rs`).
Every file's `mdoc` (preamble) ≤10 (was 16 in `policy.rs`, 13 in
`scheduler.rs`).

Nextest-run test count matched exactly at every step: 30 (base) → 24
(`wiring.rs`: 13 → 7) → 24 (`policy.rs`: 7 → 7, no net count change) → 24
(`scheduler.rs`: 10 → 10, no net count change) = **24 final**, matching
30 − 6.

`python3 scripts/maintainability.py measure --totals` (workspace-wide; this
worktree was created fresh from base SHA `6c52ab4`, and the "before" figure
was captured from a separate `git worktree add /tmp/ix-m4-before-tree
6c52ab4` scratch checkout rather than reverting this branch, so this pair is
clean of any other concurrent card's work):

- Before: `prod=56223 (comments 11931) test=71648 ratio=1.274 tests=1451 sleeps_in_tests=61 env_gated=0`
- After: `prod=56223 (comments 11931) test=71390 ratio=1.27 tests=1445 sleeps_in_tests=61 env_gated=0`

Deltas match the file-level deltas exactly: test lines −258, tests −6,
sleeps unchanged at 61 (this binary had none to begin with).

## What was removed

Per-file, with the §2.2 rule number for each change (`1: variants → rows`,
`2: name`, `3: body`, `5: third layer`, `6: preamble`, `8: fixed wait`).

**`wiring.rs`** (679 → 497 lines, 13 → 7 tests) — `1`:
`healthy_runner_with_a_matching_declared_combination_is_chosen` +
`a_declared_but_mismatched_model_is_never_chosen` +
`a_runner_with_no_declared_harnesses_never_claims_anything` +
`per_runner_capacity_saturation_leaves_the_request_unchosen` +
`a_stale_heartbeat_disqualifies_a_runner_that_would_otherwise_match` →
`eligibility_needs_capability_capacity_and_fresh_heartbeat` (a shared
`assert_eligibility` helper called once per case — every original
runner-state/model-id/expected-outcome combination preserved exactly, plus
the re-export equality check split out into its own
`reexported_choose_matches_the_primary_function` test since it was proving a
different claim than eligibility);
`high_priority_metadata_wins_over_an_older_normal_priority_request` +
`fifo_within_the_same_priority_picks_the_older_request` →
`priority_and_recency_break_ties_in_the_expected_order` (2-case table via a
shared `assert_priority` helper); `a_saturated_fleet_concurrency_limit_blocks_a_fleet_selector_request`
+ `an_unsaturated_fleet_still_allows_a_member_to_claim` →
`fleet_concurrency_limit_gates_every_member` (2-case table via
`assert_fleet_concurrency`); `no_queued_work_at_all_is_a_clean_none_not_an_error`
+ `an_unknown_runner_id_is_a_clean_none_not_an_error` →
`absence_of_eligible_work_resolves_to_none_not_an_error` (2-case table via
`assert_absence`). New helpers `setup_codex_runner`, `enqueue_for_runner` and
`enqueue_for_fleet` wrap the pre-existing `setup_runner`/`enqueue` for the
common codex/openai/exact-runner and codex/openai/fleet shapes, which is what
let every merged test's body collapse to a handful of one-line calls instead
of inlining the 7-11-argument helpers directly (rustfmt explodes a call or
tuple that wide onto one argument per line regardless of the actual string
lengths involved — confirmed empirically against several minimal repros
before settling on this fix; see *Context spent* for the detail). `2`: three
more names renamed for length. `3`: max body 79 → 36 (both merges and the
new short helper-call style contributed).

**Not merged, and why:** `fresh_runner_with_no_heartbeat_can_claim_its_first_request`
and `stale_capability_report_rejects_despite_no_heartbeat` (renamed from
`a_freshly_enrolled_runner_with_no_heartbeat_yet_can_still_claim_its_first_request`
and `a_never_heartbeated_runner_with_a_stale_capability_report_is_still_rejected`
for length only) stay as two separate tests. Their doc comments explain a
specific, load-bearing deadlock-prevention invariant (a runner's own
capability-report timestamp must stand in for a heartbeat it has never had
the chance to send) and its necessary negative-control pairing (that fallback
must not become "no heartbeat ever means eligible") — folding these into the
eligibility table would have buried the reasoning behind a generic row and
made the pairing's narrative harder to follow, exactly the kind of comment
CLAUDE.md and this plan's §2.2 rule 1 both still want kept as prose, not
compressed into a table cell.

**`policy.rs`** (499 → 430 lines, 7 tests unchanged in count) — `6`: preamble
16 → 7 lines (condensed two paragraphs into one, dropping restated
CLAUDE.md-quoting language, no meaning lost). `2`: four names renamed for
length, e.g. `a_project_default_model_is_read_from_the_real_default_model_column`
(66) → `project_tier_reads_the_real_default_model_column` (48);
`a_fleet_default_model_the_runner_does_declare_leases_successfully` (65) →
`a_declared_fleet_default_model_leases_successfully` (50, also renamed as
part of the merge below). The fleet-default-model positive/negative control
pair (`a_fleet_default_model_the_runner_does_not_declare_never_leases`,
`a_fleet_default_model_the_runner_does_declare_leases_successfully`) was
rewritten around one shared `assert_fleet_default_model_outcome(request_id,
configured_model_id, expect_leased)` helper that runs the full
resolve → enqueue → schedule → claim → assert-database-state pipeline once,
branching only on the boolean outcome — every original assertion (scheduler
wiring, re-exported entry point, claim result, `claimed.lease.request_id` on
the positive case, attempt-count, request-state, runner-capacity) is still
made, just parameterized. `3`: max body 87 → 39 (driven almost entirely by
this merge — the pair was the file's two largest tests at 86 and 63 lines).

**Not merged, and why:** the four precedence-tier tests
(`fleet_tier_reads_the_real_default_policy_column`,
`project_tier_reads_the_real_default_model_column`,
`an_agent_profile_default_beats_a_fleet_default`,
`a_request_override_beats_every_other_tier`) plus
`no_tier_configured_resolves_to_auto_select` were left as five separate
tests. Each configures a genuinely different database mechanism (a fleet's
`default_policy` JSON column, a project's `default_model` column via
`update_project`, an agent profile's `limits` column, an explicit
`ModelSelector` passed as a function argument, or nothing at all) rather than
varying one input across a shared setup — table-driving them would need a
setup closure per tier that hides exactly the mechanism each test exists to
prove is real, which is the same reasoning the sibling
`tack-api-orchestration` handoff used for its own non-merged pairs. They were
already within every budget (17-26 line bodies, ≤48-char names after the one
rename above) so no further action was needed regardless.

**`scheduler.rs`** (389 → 382 lines, 10 tests unchanged) — `6`: preamble 13 →
6 lines (one paragraph instead of two, same content). `2`: seven names
renamed for length, e.g.
`undeclared_pairing_selects_when_the_harness_attests_supported_passthrough`
(73) → `undeclared_pairing_selects_with_supported_passthrough` (53);
`batch_schedule_outcome_is_identical_across_every_permutation_of_requests`
(72) → `batch_schedule_outcome_is_stable_across_every_permutation` (57). No
`1` or `3`: every test here already proved one distinct property
(permutation-invariance of selection, of the no-eligible-runner reason list,
of batch scheduling; the advisory-vs-lease structural pin; the heartbeat
staleness boundary; four `model_passthrough` attestation-level claims) and
every body was already ≤34 lines, under the 40-line target — this file
needed only the mechanical name/preamble trims called out in the card's own
description ("already close to budget, may need only minor trims").

**`support.rs`** (115 lines, unchanged) — read in full and checked against
`crates/tack-orch/tests/common/mod.rs` (which only re-exports
`tack-test-support`) for duplication; `FixedClock`, `setup_repo` and
`codex_capability_snapshot` here are specific to this binary's own
`agent_profiles`/`agent_runners`/execution-request domain and don't exist
anywhere else in the crate. Already within every budget (6-line preamble, no
tests). No change made.

**`scheduling.rs`** (17 lines, unchanged) — just five `mod` declarations
behind a 5-line preamble; nothing to prune.

### Rule 8 — fixed waits

None found: `grep -rn "sleep(" crates/tack-orch/tests/scheduling.rs
crates/tack-orch/tests/scheduling/*.rs` returned no hits both before and
after this card's changes, confirming the card prompt's own note that this
binary had none to begin with.

### Rule 5 (third-layer over-pinning)

`python3 scripts/maintainability.py duplicate-tests
crates/tack-orch/tests/scheduling.rs crates/tack-orch/tests/scheduling/*.rs
--top 30` → `0 near-identical pairs across files` both before and after this
card's changes. No removal made under this rule. This matches the card
prompt's own framing: `tack-orch` owns the scheduling/dispatch *decision*
logic itself, so a test here proving a scheduling decision (e.g.
`fleet_concurrency_limit_gates_every_member`) is not a duplicate of any
`tack-api` HTTP-layer test that merely exercises the endpoint wrapping that
decision — they prove different things and neither is a third pin of the
same invariant.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check crates/tack-orch/tests/scheduling.rs
crates/tack-orch/tests/scheduling/*.rs` → `✓ maintainability budgets hold (5
files checked)` on the final tree without exceeding any file's existing
baseline entry — every file's post-change numbers are within its baseline
ceiling (all comfortably under it: no file's `max`/`name`/`mdoc` in this
binary was ever at its literal budget line, just under whatever the baseline
had recorded from before this card started). `scripts/maintainability-baseline.json`
was not edited, per the card's own instruction that re-baselining is reserved
for a card whose stated purpose is deliberately bringing a file down, not a
side effect of an ordinary prune.

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree (all 3
commits already landed, working tree clean):

```
✓ maintainability budgets hold (0 files checked)
```

`python3 scripts/maintainability.py check crates/tack-orch/tests/scheduling.rs
crates/tack-orch/tests/scheduling/*.rs` (explicit file list, since `--changed`
reports 0 once committed):

```
✓ maintainability budgets hold (5 files checked)
```

`python3 scripts/maintainability.py measure --totals` — before/after (see
*Measured numbers* above for the full derivation; the "before" figure came
from a scratch `git worktree add /tmp/ix-m4-before-tree 6c52ab4` rather than
touching this branch, so this pair is clean of any other concurrent card's
work):

- Before: `prod=56223 (comments 11931) test=71648 ratio=1.274 tests=1451 sleeps_in_tests=61 env_gated=0`
- After: `prod=56223 (comments 11931) test=71390 ratio=1.27 tests=1445 sleeps_in_tests=61 env_gated=0`

`python3 scripts/maintainability.py measure crates/tack-orch/tests/scheduling.rs
crates/tack-orch/tests/scheduling/*.rs` before/after: see *Measured numbers*
above — this is the load-bearing before/after pair for this sub-card
(1699 → 1441 test lines, 30 → 24 tests).

`cargo fmt --all -- --check`: clean. `cargo nextest run --workspace -E
'binary(scheduling)'`: `24 tests run: 24 passed, 0 skipped`. `cargo clippy
--workspace --all-targets -- -D warnings`: clean (the card's required gate,
plus the narrower `cargo clippy -p tack-orch --all-targets -- -D warnings`
run after every individual file's edit). `scripts/check-comments.sh` and
`scripts/check-test-hygiene.sh`: both clean.

## What a stranger still cannot do

A stranger arriving from outside this repository still cannot tell, from
`wiring.rs`'s new `setup_codex_runner`/`enqueue_for_runner`/`enqueue_for_fleet`
wrapper functions alone, that they exist specifically to work around a
rustfmt formatting quirk (a 7-plus-argument function call or tuple literal
gets exploded to one argument per line regardless of whether the resulting
line would actually exceed the 100-column limit — confirmed with several
minimal repro files before concluding the wrapper approach, not a width
problem, was the fix) rather than being pure API design. The wrapper doc
comments name what they fix for (the common codex/openai shape), not why
they were necessary at all; that reasoning lives only in this handoff and the
commit message. They also cannot yet run one command that proves this Part's
cross-binary invariant-layering claim (rule 5) end to end — this card only
checked the one invariant family `duplicate-tests` could evaluate for the
files it owns, the same limitation every prior IX-M4 handoff has noted.

## Context spent

- Tokens read before the first edit (cold start): the card's own dispatch
  prompt (self-contained, no `TODO.md` extraction needed since the prompt
  already carried the acceptance criteria and file list), the relevant
  `docs/plans/human-maintainability.md` §2.1–§2.2 excerpt, and
  `crates/tack-orch/tests/common/mod.rs` in full. Moderate cold start — well
  under the sibling `tack-api-orchestration` card's 20-24k, matching this
  being a 5-file, ~1700-line binary rather than a 24-file, ~8300-line one.
- Context size at handoff: moderate — all 5 owned/adjacent files read in
  full before editing; `wiring.rs` and `policy.rs` were each rewritten more
  than once in place (`wiring.rs` three times, chasing the rustfmt-explosion
  problem down to its actual cause rather than accepting an over-budget
  first table-driven attempt).
- Files opened and not used: none of significance.
- Read-list lines that were wrong: none — the prompt's five-file,
  largest-first list and per-file test/line counts matched the actual
  measured sizes exactly at card start.
- One finding worth flagging for whoever hits this next: **rustfmt explodes
  a 7-or-more-argument tuple literal or function call onto one line per
  argument even when the single-line form would fit comfortably under the
  100-column limit** (confirmed with `rustfmt --edition 2024` on several
  minimal repro files: a 6-element tuple with a 32-character string field
  stays on one line at 80 total columns, a 7-element tuple with the same
  content does not, regardless of further shortening). A table-driven test
  whose row struct/tuple has many fields will look far larger under
  `measure` than the same data expressed as a sequence of short calls to a
  shared per-case helper function — prefer the latter shape by default once
  a case has more than ~5-6 varying fields, rather than debugging rustfmt
  output after the fact.

## Amendments

*(none yet)*
