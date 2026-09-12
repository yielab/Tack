# IX-M4-tack-api-wiring handoff

- Base SHA / branch / final SHA: `6c52ab4` / `agent/ix-m4-api-wiring` / (this commit)
- Files changed (must equal ownership list): exactly `crates/tack-api/tests/wiring/model.rs`,
  `crates/tack-api/tests/wiring/execution_sweep.rs`, `crates/tack-api/tests/wiring/artifact.rs`
  — confirmed by `git diff --stat 6c52ab4..HEAD --name-only`. `crates/tack-api/tests/wiring.rs`
  (the module-root `mod` declarations file) was read but not touched — it was already 14
  lines with no tests, nothing to prune. No file under `crates/tack-api/src/**` was touched
  (`git diff --stat 6c52ab4..HEAD -- crates/tack-api/src/` is empty).
- Contract fixtures consumed: none (`docs/contracts/runner-v1/` not touched).
- Behavior implemented: none — pruning only. Every fixed-wait replacement is a bounded poll
  or absence-check, not a behavior change; every test merge preserves every original
  assertion (see *What was removed*).
- Tests added and exact commands/results: none added net-new; six variant/split
  changes across three files (net −5 tests: 15 → 10).
  `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-api-wiring cargo nextest run --workspace -E
  'binary(wiring)'` — `10 tests run: 10 passed, 0 skipped` (verified after every file's
  commit, and again at the end). The three fixed-wait replacements
  (`execution_sweep.rs`'s `wait_for` internal tick, and its two `stays_true_for` absence
  polls replacing blind sleeps) were stress-run 20x standalone — all green, no flakiness.
- Failure/adversarial case proved: n/a — pruning only, no new behavior.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none introduced. `fleet_membership`-style
  single coherent scenarios don't occur in this binary's three files; every merge/split
  made here left the same claims provable with the same specificity as before.
- Secrets/logging review: n/a — test-only changes, no logging or secret-handling
  production code touched.
- Safe merge order and likely conflicts: independent of every other IX-M4 sub-card (each
  owns a different binary) and of the concurrent `tack-orch-scheduling` work visible in this
  worktree's sibling directories. No conflicts expected against `develop` since only these
  three files (all exclusive to this binary) were touched.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| All behavior in the three files is unchanged | `cargo nextest run --workspace -E 'binary(wiring)'` — 10/10 pass, same assertions per case as before (see *What was removed* for the case-to-case mapping on every merge/split) |
| No `sleep(` remains anywhere in this binary | `grep -rn 'sleep(' crates/tack-api/tests/wiring.rs crates/tack-api/tests/wiring/*.rs` — no hits (was 3, all in `execution_sweep.rs`) |
| Every test name in this binary is ≤60 chars | `grep -oP '(?<=^async fn )\w+' crates/tack-api/tests/wiring/*.rs \| awk '{ if (length($0) > 60) print }'` — empty across all three files |
| Every file's preamble is ≤10 lines | `measure`'s `mdoc` column, final run: model.rs 7, execution_sweep.rs 6, artifact.rs 6 (was 13/15/40) |
| No production file touched | `git diff --stat 6c52ab4..HEAD -- crates/tack-api/src/` — empty |
| The three fixed-wait replacements are stable, not just fast | 20x standalone stress runs of the full binary after the `execution_sweep.rs` commit, all green |
| `cargo fmt --all -- --check` (workspace and `tack-desktop`) is clean | ran after the final commit, no diff either |
| `cargo clippy --workspace --all-targets -- -D warnings` is clean | ran after the final commit, no warnings |
| `python3 scripts/check-comments.sh` and `check-test-hygiene.sh` are clean | both ran green at the end (not this card's required gate, run anyway) |
| No near-duplicate test names against other layers (rule 5) | `python3 scripts/maintainability.py duplicate-tests` grepped for `wiring` — zero hits before and after; nothing to remove under this rule |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-api/tests/wiring.rs crates/tack-api/tests/wiring/*.rs`

Before (measured at card start, base SHA `6c52ab4`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-api/tests/wiring/model.rs                           0     0   0%    895    8    60  106   77   13
crates/tack-api/tests/wiring/execution_sweep.rs                 0     0   0%    683    5    69   91   85   15
crates/tack-api/tests/wiring/artifact.rs                        0     0   0%    538    2    93  159   96   40
crates/tack-api/tests/wiring.rs                                 0     0   0%     14    0     0    0    0    4
totals: prod=0 (comments 0) test=2130 ratio=0.0 tests=15 sleeps_in_tests=3 env_gated=0
```

After (this handoff, final tree):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-api/tests/wiring/model.rs                           0     0   0%    716    4    42   55   60    7
crates/tack-api/tests/wiring/execution_sweep.rs                 0     0   0%    590    3    21   56   53    6
crates/tack-api/tests/wiring/artifact.rs                        0     0   0%    518    3    37   46   57    6
crates/tack-api/tests/wiring.rs                                 0     0   0%     14    0     0    0    0    4
totals: prod=0 (comments 0) test=1838 ratio=0.0 tests=10 sleeps_in_tests=0 env_gated=0
```

Net: 2130 → 1838 test lines (−292, −13.7%); 15 → 10 tests (−5); `sleeps_in_tests` 3 → 0.
Every file's `name` max ≤60 (was up to 96 in `artifact.rs`). Every file's `mdoc` (preamble)
≤10 (was up to 40 in `artifact.rs`). Every file's `max` body ≤60 (was up to 159 in
`artifact.rs`).

Nextest-run test count matched exactly at every step: 15 (base) → 11 (model.rs −4) → 9
(execution_sweep.rs −2) → 10 (artifact.rs +1, one 159-line test split into two) = **10
final**, matching 15 − 5.

`python3 scripts/maintainability.py measure --totals` (workspace-wide, measured on this
worktree at both the base SHA via a throwaway `git worktree add` and the final tree, so this
pair reflects *this card's own delta only*, not concurrent work elsewhere on `develop`):

- Before: `prod=56223 (comments 11931) test=71648 ratio=1.274 tests=1451 sleeps_in_tests=61 env_gated=0`
- After: `prod=56223 (comments 11931) test=71356 ratio=1.269 tests=1446 sleeps_in_tests=58 env_gated=0`

Deltas match the file-level deltas exactly: test lines −292, tests −5, sleeps −3.

## What was removed

Per-file, with the §2.2 rule number for each change (`1: variants → rows`, `2: name`,
`3: body`, `6: preamble`, `8: fixed wait`).

**`model.rs`** (895 → 716 lines, 8 → 4 tests) — `1`:
`create_execution_resolves_agent_profile_default_when_client_omits_both_fields` +
`create_execution_does_not_override_an_explicit_client_supplied_model` +
`create_execution_resolves_fleet_default_only_when_selector_is_fleet` +
`create_execution_resolves_to_auto_select_when_no_tier_is_configured` →
`create_execution_resolves_model_by_precedence_tier` (4-case table over the
agent-profile/fleet/auto-select precedence tiers; every original stored-column assertion
and the request-snapshot no-split-brain check are preserved per case, via an
`assert_model_resolution_case` helper kept out of the test body itself to hold it under the
60-line cap). `attempt_summary_reports_matched_provenance_and_honest_runner_time_cost` +
`attempt_summary_reports_mismatched_provenance_with_both_sides_visible` →
`attempt_summary_reports_provenance_and_cost_after_completion` (2-case table: matched vs.
mismatched actual model; the wall-clock-ms positive control, the runner-time
`not_measured` invariant, and the DB cross-check are asserted identically for both cases via
an `assert_provenance_case` helper). `2`: all eight surviving/merged names renamed for
length, e.g. `attempt_summary_omits_provenance_and_shows_no_wall_clock_before_completion`
(75) → `attempt_summary_hides_provenance_and_costs_before_completion` (60);
`create_execution_succeeds_again_for_an_item_with_a_finished_attempt` (68) →
`create_execution_succeeds_for_item_with_a_finished_attempt` (58). `6`: preamble 13 → 7
lines. `LiveAttempt` gained an `agent_profile_id` field so the repeat-enqueue test could
reuse `claim_accept_start` instead of re-deriving its own claim/accept/start/completion
sequence by hand. **Not merged, and why:** `attempt_summary_hides_provenance_and_costs_before_completion`
proves a structurally different state (no completion has happened at all — `model_provenance`
is `Value::Null`, not a discriminated `matched`/`mismatched` variant) from the two merged
cases below it; folding it into the same table would need a third, differently-shaped
expectation type for one case only. `create_execution_succeeds_for_item_with_a_finished_attempt`
proves a distinct idempotency-replay invariant unrelated to model-policy resolution or
provenance shape, sharing only the file's fixtures.

**`execution_sweep.rs`** (683 → 590 lines, 5 → 3 tests) — `1`:
`retention_enabled_purges_expired_artifact_row_and_blob_but_spares_a_fresh_one` +
`retention_disabled_by_default_leaves_the_same_expired_artifact_row_and_blob_untouched` →
`retention_enable_gates_the_artifact_sweep` (boolean-gated table via an
`assert_artifact_retention_gate(retention_enable)` helper; both branches now also assert the
fresh artifact is never swept, strengthening the disabled branch, which previously omitted
that check). `overdue_decision_expires_via_the_periodic_sweep_while_a_future_one_stays_pending`
+ `overdue_decision_stays_pending_while_retention_is_disabled` →
`retention_enable_gates_overdue_decision_expiry` (same boolean-gated shape via
`assert_decision_expiry_gate`; the "future decision stays pending" check now runs in both
branches instead of only the enabled one). `2`: the one surviving standalone test renamed
(`retention_enabled_purges_an_old_manifest_row_with_no_upload_ever_completed` (76) →
`retention_enabled_purges_a_manifest_only_artifact_row` (53)). `6`: preamble 15 → 6 lines.
`8`: all three fixed waits removed — see the dedicated section below. Also renamed
`RUNNER_ID`/`PROFILE_ID` from `"runner-f6d"`/`"profile-f6d"` to `"runner-wiring"`/
`"profile-wiring"`, and the workspace/project/runner/profile display names from `"F6D"`-style
labels to plain ones, while rewriting this file's helpers — these were vestigial
wave-numbering codenames with no referential value once the file's real name
(`execution_sweep.rs`) already says what it proves. **Not merged, and why:**
`retention_enabled_purges_a_manifest_only_artifact_row` proves a distinct row shape
(`content_reference: None`, never uploaded) racing a different guard
(`set_execution_artifact_content_reference`'s `WHERE content_reference IS NULL`) than the
enabled/disabled pair above it, and is only ever exercised with retention enabled — there is
no natural disabled-branch counterpart to pair it with.

**`artifact.rs`** (538 → 518 lines, 2 → 3 tests) — `3`: the single 159-line
`artifact_content_is_stored_under_configured_storage_dir_and_downloadable_through_the_real_router`
proved two distinct claims in one body (storage-location and download-route-mounted) and was
split into `artifact_bytes_land_under_the_configured_storage_dir` (46 lines) and
`uploaded_artifact_is_downloadable_through_the_real_router` (25 lines), sharing one new
`upload_artifact(label)` helper that does the setup (real router, project/item, running
attempt, manifest + content PUT) once; both original assertion sets are preserved exactly,
now each under its own claim. `2`: all three names renamed/shortened (the split above,
plus `unauthenticated_operator_download_request_is_still_gated_by_a_real_lookup` (73) →
`download_of_an_unknown_artifact_returns_a_named_404` (52)). `6`: preamble 40 → 6 lines
(the two-claim explanation is now redundant with the split test names themselves, so it was
condensed rather than moved to a dev-note). Also renamed the `"F6a"`-style workspace/runner
labels and the `"opaque/model-f6a"` fixture model id to plain `"W"`/`"wiring"`/
`"opaque/model-wiring"` forms while rewriting these helpers. **No `1`:** the surviving 404
test proves a distinct claim (the route is genuinely wired to a real lookup, not a stray
match) unrelated to either half of the split test above it.

### Rule 8 — all three fixed waits removed (`execution_sweep.rs`)

| Site (original line) | Replacement | Stress-test result |
|---|---|---|
| `execution_sweep.rs:300` `sleep(50ms)` (inside `wait_for`'s own retry loop) | `tokio::time::interval(50ms).tick()`, with the interval's immediate first tick discarded before the loop starts | 20/20 full-binary runs green |
| `execution_sweep.rs:435` `sleep(500ms)` (blind wait, then check retention-disabled artifact row/blob survived) | `stays_true_for(condition, 10)` — a new helper that polls the condition on every 50ms tick for a bounded 10 ticks and fails the instant it stops holding, rather than checking once after a blind wait | 20/20 full-binary runs green |
| `execution_sweep.rs:664` `sleep(500ms)` (blind wait, then check retention-disabled decision stayed pending) | same `stays_true_for(condition, 10)` helper | 20/20 full-binary runs green |

No `tokio::task::yield_now()` loop was used anywhere in this fix, per the dispatch prompt's
warning (citing the sibling `ix-m4-api-orchestration` card's finding that `yield_now()` can
starve a background `tokio::spawn`'s real socket/DB I/O on a single-threaded-per-test
runtime). `wait_for` and `stays_true_for` both use `tokio::time::interval(...).tick()`
throughout, which forces a real timer registration and reliably drives the I/O
driver — the same mechanism the orchestration card's own fix converged on. All three of
this file's background sweeps do real SQLite/filesystem I/O via a genuine
`ExecutionRuntime::start` spawn, so this was treated as the I/O-bearing case throughout
rather than risking a `yield_now()` regression.

**Rule 5 (third-layer over-pinning):** no removal made under this rule in any file.
`python3 scripts/maintainability.py duplicate-tests` was run before and after all edits;
grepping its output for `wiring` (or any of this binary's specific test names) returned no
hits either time — none of this binary's tests appear in any reported near-duplicate pair.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check crates/tack-api/tests/wiring.rs
crates/tack-api/tests/wiring/*.rs` → `✓ maintainability budgets hold` on the final tree,
and `python3 scripts/maintainability.py check` (whole tracked tree) →
`✓ maintainability budgets hold (288 files checked)`. Every file's post-change numbers are
now *under* every hard budget outright (not merely under a raised baseline ceiling), so
`scripts/maintainability-baseline.json` was not edited — nothing here needed to trade a
lowered ceiling for a passing check.

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree (all commits already
landed, working tree clean):

```
✓ maintainability budgets hold (0 files checked)
```

(`--changed` diffs the working tree against the index, so it reports 0 once everything is
committed — matching the same behavior the `ix-m4-api-orchestration` handoff already noted.)

`python3 scripts/maintainability.py check crates/tack-api/tests/wiring.rs
crates/tack-api/tests/wiring/*.rs` (explicit file list):

```
✓ maintainability budgets hold (4 files checked)
```

`python3 scripts/maintainability.py measure --totals` — before/after (see *Measured
numbers* above for the full derivation; the "before" figure was captured from a throwaway
`git worktree add /tmp/ix-m4-base 6c52ab4`, so it is this card's base SHA exactly, not a
snapshot taken after any other concurrent card had already landed on top of it):

- Before: `prod=56223 (comments 11931) test=71648 ratio=1.274 tests=1451 sleeps_in_tests=61 env_gated=0`
- After: `prod=56223 (comments 11931) test=71356 ratio=1.269 tests=1446 sleeps_in_tests=58 env_gated=0`

`python3 scripts/maintainability.py measure crates/tack-api/tests/wiring*` before/after: see
*Measured numbers* above — this is the load-bearing before/after pair for this sub-card
(2130 → 1838 test lines, 15 → 10 tests, 3 → 0 sleeps).

`cargo fmt --all -- --check` (workspace) and `(cd crates/tack-desktop && cargo fmt --
--check)`: both clean. `cargo clippy --workspace --all-targets -- -D warnings`: clean.
`scripts/check-comments.sh` and `scripts/check-test-hygiene.sh`: both clean.

`docs/adr/0064-fixed-waits.txt`: **not regenerated by this card.**
`python3 scripts/list-fixed-waits.py` against the final tree confirms both of this card's
≥200ms entries (`execution_sweep.rs:435` and `:664`) are gone from the regenerated list —
the file goes from 25 entries/18.5s total to 14 entries/10.7s total, but the diff also shows
unrelated drift: several `tack-orch/src/reconciler.rs` and `tack-api/src/orch_runtime.rs`
entries present in the *committed* file disappear (already fixed by the
`ix-m4-api-orchestration` merge that landed at this card's own base SHA `6c52ab4`, but never
re-committed into `0064-fixed-waits.txt` itself), and new entries appear in
`tack-cli/tests/embedded_runner_live_secret.rs` and `tack-orch/tests/docket_live_test.rs`
that this card never touched. Committing the regeneration would put out-of-scope changes in
this card's diff, so it was left untouched — matching the precedent the
`ix-m4-api-orchestration` handoff already set for this exact situation (stale committed
inventory vs. a tree that has since moved past it via other cards' merges).

## What a stranger still cannot do

A stranger arriving from outside this repository still cannot tell, from
`execution_sweep.rs`'s `wait_for`/`stays_true_for` helpers alone, that the file's three
background sweeps race real SQLite/filesystem I/O rather than pure in-process work — that
distinction (why `Interval::tick` and not `yield_now()`) is documented in this file's own
`wait_for` doc comment and in this handoff, but verifying it independently still means
reading `ExecutionRuntime::start`'s spawn or the sibling `ix-m4-api-orchestration` handoff's
own account of the same failure mode. They also cannot yet run one command that proves this
Part's cross-binary invariant-layering claim (rule 5) end to end — this card only checked
the one invariant family `duplicate-tests` would have flagged against files it owns (none
were), the same limitation every prior IX-M4 handoff has noted.

## Context spent

- Tokens read before the first edit (cold start): the two project `CLAUDE.md` files (root
  workspace + `objetivosMios`), `docs/plans/human-maintainability.md` §2.1–2.2,
  `crates/tack-api/tests/common/mod.rs` in full (to confirm which helpers already exist
  before writing any new one), and `scripts/maintainability.py`'s parsing logic (`tests_in`,
  `comment_runs`, `measure_file`) to understand exactly what `mdoc`/body-line/name-length
  counts before touching any file — the dispatch prompt's own line numbers for the three
  fixed waits and each file's `measure` baseline were re-verified fresh rather than trusted.
  The `part-ix/TEMPLATE.md` and the `part-vi/TEMPLATE.md` it points to, plus one sibling
  handoff in full (`IX-M4-tack-api-orchestration.md`) for format and to learn the
  `yield_now()`-starves-real-I/O finding before touching `execution_sweep.rs`'s sleeps.
- Context size at handoff: moderate — all four owned/adjacent files
  (`wiring.rs`, `model.rs`, `execution_sweep.rs`, `artifact.rs`) were read in full before
  editing; `duplicate-tests` and `list-fixed-waits.py` output were read once each rather than
  re-run speculatively.
- Files opened and not used: `docs/agent-handoffs/part-vi/TEMPLATE.md` was read only to
  resolve the `part-ix/TEMPLATE.md` pointer, not copied from directly (the actual body
  follows the `IX-M4-tack-api-orchestration.md` sibling's realized structure instead, which
  already resolves that pointer correctly).
- Read-list lines that were wrong: none — the three fixed-wait line numbers in the dispatch
  prompt (`~300`, `~435`, `~664`) matched the actual `grep sleep(` output exactly, and each
  file's largest-first ordering and reported test/name/body counts matched a fresh `measure`
  run.

## Amendments

*(none yet)*
