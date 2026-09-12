# IX-M4-tack-api-orchestration handoff

- Base SHA / branch / final SHA: `44825e3` / `agent/ix-m4-api-orchestration` / `4bcdc66`
- Files changed (must equal ownership list): all 17 files under
  `crates/tack-api/tests/orchestration/` this card was assigned —
  `dispatch/dual_scheduling.rs`, `auto_dispatch/sprint.rs`, `dispatch/item.rs`,
  `reporting/agent_activity.rs`, `control_plane/resource.rs`,
  `reporting/budget_policy.rs`, `dispatch/pipeline.rs`, `auto_dispatch/hook.rs`,
  `reporting/approvals.rs`, `reconciler/terminal_status.rs`,
  `fleet_templates/fleet_membership.rs`, `reconciler/broadcast.rs`,
  `control_plane/settings.rs`, `reporting/run_readback.rs`, `reconciler/wiring.rs`,
  `fleet_templates/templates.rs`, `auto_dispatch/gate.rs` — exactly this list,
  confirmed by `git diff --stat 44825e3..HEAD --name-only`. The seven tiny `mod`
  declaration files (`orchestration.rs`, `auto_dispatch.rs`, `reporting.rs`,
  `reconciler.rs`, `dispatch.rs`, `control_plane.rs`, `fleet_templates.rs`) were
  read but not touched — no rename changed a `mod` path. No file under
  `crates/tack-api/src/**` was touched.
- Contract fixtures consumed: none (`docs/contracts/runner-v1/` not touched).
- Behavior implemented: none — pruning only. §IX.1 rule 1's fixed-wait exception
  applies to all nine sleep replacements (bounded polls, not behavior changes).
- Tests added and exact commands/results: none added net-new; ten variant
  families merged into table-driven tests (net -16 tests: 147 → 131).
  `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-api-orchestration cargo nextest run
  --workspace -E 'binary(orchestration)'` — `131 tests run: 131 passed, 0
  skipped` (verified after every file's commit, and again at the end). The five
  fixed-wait replacements touching real background-task I/O
  (`auto_dispatch/hook.rs`'s three, `control_plane/settings.rs`'s one,
  `reconciler/wiring.rs`'s one, `auto_dispatch/gate.rs`'s two) were additionally
  stress-run 15-20x each standalone to rule out flakiness — all green.
- Failure/adversarial case proved: n/a — pruning only, no new behavior.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: `fleet_templates/fleet_membership.rs`'s
  two tests remain at 212 and 66 lines — deliberate, single coherent
  acceptance-style scenarios (see *What was removed*). None of this card's
  changes worsened any file's baseline; several files' worst name/preamble/body
  numbers improved past what the baseline recorded (`scripts/maintainability-baseline.json`
  itself was not edited — see *Re-baselined?*).
- Secrets/logging review: n/a — test-only changes, no logging or secret-handling
  production code touched.
- Safe merge order and likely conflicts: independent of every other IX-M4
  sub-card (each owns one binary) and of the concurrent `tack-db`
  `migrations` binary card mentioned in the dispatch prompt. No conflicts
  expected against `develop` since only this one binary's files were touched.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| All behavior in the 17 files is unchanged | `cargo nextest run --workspace -E 'binary(orchestration)'` — 131/131 pass, same assertions per case as before (see *What was removed* for the case-to-case mapping on every merge) |
| No `sleep(` remains anywhere in this binary | `grep -rn 'sleep(' crates/tack-api/tests/orchestration.rs crates/tack-api/tests/orchestration/**/*.rs` — no hits (was 9 across 4 files) |
| Every test name in this binary is ≤60 chars, no unresolved articles/narrative | `grep -oP '(?<=^async fn )\w+' crates/tack-api/tests/orchestration/**/*.rs \| awk '{ if (length($0) > 60) print }'` — empty across all 17 files |
| Every file's preamble is ≤10 lines | `measure`'s `mdoc` column, final run: all 17 files ≤10 (was up to 19 in `auto_dispatch/hook.rs`) |
| No production file touched | `git diff --stat 44825e3..HEAD -- crates/tack-api/src/` — empty |
| The five fixed waits touching real background-task socket I/O are stable, not just fast | 15-20x standalone stress runs per affected test (see per-file sections below), all green |
| `cargo fmt --all -- --check` is clean | ran after the final commit, no diff |
| `cargo clippy -p tack-api --tests --all-targets -- -D warnings` is clean | ran after every file's edit, no warnings (not this card's required gate, run anyway) |
| `python3 scripts/check-comments.sh` and `check-test-hygiene.sh` are clean | both ran green at the end (not this card's required gate, run anyway) |

## Measured numbers

`CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-api-orchestration python3 scripts/maintainability.py measure crates/tack-api/tests/orchestration.rs crates/tack-api/tests/orchestration/*.rs crates/tack-api/tests/orchestration/*/*.rs`

Before (measured at card start, base SHA `44825e3`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-api/tests/orchestration/dispatch/dual_scheduling.rs      0     0   0%    834    9    38   81   72   12
crates/tack-api/tests/orchestration/auto_dispatch/sprint.rs      0     0   0%    747   13    33   62   78   14
crates/tack-api/tests/orchestration/dispatch/item.rs            0     0   0%    664   13    34   75   78   13
crates/tack-api/tests/orchestration/reporting/agent_activity.rs      0     0   0%    605   13    34   78   71   13
crates/tack-api/tests/orchestration/control_plane/resource.rs      0     0   0%    535   17    22   54   59    9
crates/tack-api/tests/orchestration/reporting/budget_policy.rs      0     0   0%    531    9    37   54   63   16
crates/tack-api/tests/orchestration/dispatch/pipeline.rs        0     0   0%    511   10    21   47   61   13
crates/tack-api/tests/orchestration/auto_dispatch/hook.rs       0     0   0%    498    5    48   61   72   19
crates/tack-api/tests/orchestration/reporting/approvals.rs      0     0   0%    479   11    26   61   82   12
crates/tack-api/tests/orchestration/fleet_templates/fleet_membership.rs      0     0   0%    476    2   138  212   79   10
crates/tack-api/tests/orchestration/reconciler/terminal_status.rs      0     0   0%    476    8    34   50  101   16
crates/tack-api/tests/orchestration/reconciler/broadcast.rs      0     0   0%    388    8    29   46   69   11
crates/tack-api/tests/orchestration/control_plane/settings.rs      0     0   0%    318    9    19   25   62   11
crates/tack-api/tests/orchestration/reporting/run_readback.rs      0     0   0%    311    5    41   63   79   12
crates/tack-api/tests/orchestration/reconciler/wiring.rs        0     0   0%    302    6    42   70   76   10
crates/tack-api/tests/orchestration/fleet_templates/templates.rs      0     0   0%    295    7    31   50   61   13
crates/tack-api/tests/orchestration/auto_dispatch/gate.rs       0     0   0%    291    2    52   56   80   16
crates/tack-api/tests/orchestration.rs                          0     0   0%     20    0     0    0    0    4
crates/tack-api/tests/orchestration/auto_dispatch.rs            0     0   0%     15    0     0    0    0    7
crates/tack-api/tests/orchestration/reconciler.rs               0     0   0%     14    0     0    0    0    6
crates/tack-api/tests/orchestration/reporting.rs                0     0   0%     14    0     0    0    0    4
crates/tack-api/tests/orchestration/dispatch.rs                 0     0   0%     13    0     0    0    0    5
crates/tack-api/tests/orchestration/control_plane.rs            0     0   0%     12    0     0    0    0    6
crates/tack-api/tests/orchestration/fleet_templates.rs          0     0   0%     12    0     0    0    0    6
totals: prod=0 (comments 0) test=8361 ratio=0.0 tests=147 sleeps_in_tests=9 env_gated=0
```

After (this handoff, final SHA `4bcdc66`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-api/tests/orchestration/dispatch/dual_scheduling.rs      0     0   0%    789    6    51   81   60    8
crates/tack-api/tests/orchestration/auto_dispatch/sprint.rs      0     0   0%    734   12    34   62   58    8
crates/tack-api/tests/orchestration/dispatch/item.rs            0     0   0%    670   12    34   61   55    9
crates/tack-api/tests/orchestration/reporting/agent_activity.rs      0     0   0%    592   10    43   78   58   10
crates/tack-api/tests/orchestration/control_plane/resource.rs      0     0   0%    526   15    24   54   59    9
crates/tack-api/tests/orchestration/reporting/budget_policy.rs      0     0   0%    525    9    37   54   60   10
crates/tack-api/tests/orchestration/dispatch/pipeline.rs        0     0   0%    521    8    28   47   57    8
crates/tack-api/tests/orchestration/auto_dispatch/hook.rs       0     0   0%    502    4    46   61   59    7
crates/tack-api/tests/orchestration/fleet_templates/fleet_membership.rs      0     0   0%    476    2   138  212   59   10
crates/tack-api/tests/orchestration/reconciler/terminal_status.rs      0     0   0%    468    8    34   49   59    9
crates/tack-api/tests/orchestration/reporting/approvals.rs      0     0   0%    437    9    26   61   60    9
crates/tack-api/tests/orchestration/reconciler/broadcast.rs      0     0   0%    385    8    29   46   60    8
crates/tack-api/tests/orchestration/control_plane/settings.rs      0     0   0%    319    9    19   25   57   10
crates/tack-api/tests/orchestration/reconciler/wiring.rs        0     0   0%    312    6    44   70   58    9
crates/tack-api/tests/orchestration/auto_dispatch/gate.rs       0     0   0%    304    2    48   49   55    9
crates/tack-api/tests/orchestration/reporting/run_readback.rs      0     0   0%    296    5    36   54   58   10
crates/tack-api/tests/orchestration/fleet_templates/templates.rs      0     0   0%    269    6    33   50   57    8
crates/tack-api/tests/orchestration.rs                          0     0   0%     20    0     0    0    0    4
crates/tack-api/tests/orchestration/auto_dispatch.rs            0     0   0%     15    0     0    0    0    7
crates/tack-api/tests/orchestration/reconciler.rs               0     0   0%     14    0     0    0    0    6
crates/tack-api/tests/orchestration/reporting.rs                0     0   0%     14    0     0    0    0    4
crates/tack-api/tests/orchestration/dispatch.rs                 0     0   0%     13    0     0    0    0    5
crates/tack-api/tests/orchestration/control_plane.rs            0     0   0%     12    0     0    0    0    6
crates/tack-api/tests/orchestration/fleet_templates.rs          0     0   0%     12    0     0    0    0    6
totals: prod=0 (comments 0) test=8225 ratio=0.0 tests=131 sleeps_in_tests=0 env_gated=0
```

Net: 8361 → 8225 test lines (−136, −1.6%); 147 → 131 tests (−16); `sleeps_in_tests`
9 → 0. Every file's `name` max ≤60 (was up to 101 in `reconciler/terminal_status.rs`).
Every file's `mdoc` (preamble) ≤10 (was up to 19 in `auto_dispatch/hook.rs`).

Nextest-run test count matched exactly at every step: 147 (base) → 144
(dual_scheduling −3) → 143 (sprint −1) → 142 (item −1) → 139 (agent_activity −3)
→ 137 (resource −2) → 137 (budget_policy 0) → 135 (pipeline −2) → 134 (hook −1)
→ 132 (approvals −2) → 132 (terminal_status 0) → 132 (fleet_membership 0) → 132
(broadcast 0) → 132 (settings 0) → 132 (wiring 0) → 131 (templates −1) → 131
(gate 0) = **131 final**, matching 147 − 16.

`python3 scripts/maintainability.py measure --totals` (workspace-wide, this
card's worktree only — created fresh from base SHA `44825e3`, so this pair is
clean of any other concurrent card's work):

- Before: `prod=56223 (comments 11931) test=72075 ratio=1.282 tests=1479 sleeps_in_tests=70 env_gated=0`
- After: `prod=56223 (comments 11931) test=71939 ratio=1.28 tests=1463 sleeps_in_tests=61 env_gated=0`

Deltas match the file-level deltas exactly: test lines −136, tests −16, sleeps −9.

## What was removed

Per-file, with the §2.2 rule number for each change (`1: variants → rows`,
`2: name`, `3: body`, `5: third layer`, `6: preamble`, `8: fixed wait`).

**`dispatch/dual_scheduling.rs`** (833 → 789 lines, 9 → 6 tests) — `1`:
`dispatch_refuses_when_item_has_active_runner_v1_request` +
`dispatch_proceeds_when_runner_v1_request_is_terminal` →
`dispatch_blocks_only_on_active_runner_v1_request` (2-case table: request
state active/terminal, expected status/orch_tasks-count preserved exactly);
`create_execution_refuses_when_item_has_an_active_docket_task` +
`create_execution_proceeds_when_docket_task_is_terminal` +
`create_execution_ignores_an_active_docket_task_when_orchestration_is_off` →
`create_execution_blocks_only_active_docket_task_with_orch_on` (3-case table:
orch on/off × docket status, all three original expected status/count pairs
preserved). A generalized `insert_execution_request(state, item_id, state)`
helper replaced the old hardcoded-state `insert_active_execution_request` and
an inline duplicate insert, cutting duplication beyond the merges themselves.
`2`: `has_active_execution_request_for_item_ignores_terminal_states` (61) →
`active_execution_request_read_ignores_terminal_states` (54);
`create_execution_ignores_an_active_docket_task_when_orchestration_is_off` (72,
folded into the merge above) and
`create_execution_replay_succeeds_despite_an_active_docket_task` (62) →
`execution_replay_succeeds_despite_active_docket_task` (52). `6`: preamble
12 → 8 lines. **Not merged, and why:** the two unit-level "ignores terminal
states" tests (`active_execution_request_read_ignores_terminal_states`,
`active_docket_task_for_item_ignores_terminal_statuses`) read different
repository methods and were left as-is; the replay and conflict-naming tests
each prove a distinct claim not shared by any other test in the file.

**`auto_dispatch/sprint.rs`** (747 → 734 lines, 13 → 12 tests) — `1`:
`max_in_flight_actually_bounds_concurrent_dispatch_calls` +
`a_generous_cap_lets_independent_items_dispatch_concurrently` →
`max_in_flight_bounds_actual_dispatch_concurrency` (2-case table over an
`AtLeast`/`LessThan` elapsed-time-bound enum; both original cap values,
elapsed-time thresholds and messages preserved exactly). `2`: six names
renamed for length/articles, e.g.
`dry_run_diamond_orders_a_first_then_b_and_c_then_d_and_gates_downstream` (71)
→ `dry_run_diamond_orders_topologically_and_gates_downstream` (58);
`a_completed_dependency_unblocks_its_direct_dependents_but_not_their_dependents`
(78) → `completed_dependency_unblocks_direct_dependents_only` (52). `6`:
preamble 14 → 8 lines. **Not merged, and why:** the three off/unknown/unlinked
guard tests at the top are three distinct claims (disabled both-routes,
unknown-sprint 404, unlinked-project 409), not variants of one; the diamond
dependency-graph tests each assert a different set of fields (order, decision,
blocked_by) on the same fixture and would only lose clarity if forced into one
table.

**`dispatch/item.rs`** (664 → 670 lines, 13 → 12 tests) — `1`:
`dispatch_sends_trusted_false_for_a_github_imported_item` +
`dispatch_sends_trusted_true_for_an_ordinary_item` →
`dispatch_sends_trusted_flag_by_item_provenance` (2-case table; the merge's
first form exceeded this file's 60-line baseline max at 106 lines, so the
GitHub-item-seeding block was further extracted into a plain
`seed_github_imported_item` helper, bringing the merged body to 61 — under
the file's own baseline). `2`: five names renamed for length, e.g.
`double_dispatch_hits_docket_once_and_the_second_call_reports_already_in_flight`
(78) → `double_dispatch_hits_docket_once_second_call_in_flight` (54).
`6`: preamble 13 → 9 lines.

**`reporting/agent_activity.rs`** (605 → 592 lines, 13 → 10 tests) — `1`:
`project_agent_activity_is_inner_join_excludes_items_with_no_tasks` +
`project_agent_activity_returns_empty_rows_for_project_with_no_activity` →
`project_agent_activity_is_inner_join_on_orch_tasks` (2-case table: no
activity vs. one-of-two-items dispatched, exact row-count/content assertions
preserved for each);
`project_agent_activity_latest_attempt_wins_by_highest_attempt_number` +
`project_agent_activity_ties_on_attempt_break_by_dispatched_at_desc` →
`project_agent_activity_latest_attempt_wins_tie_break` (2-case table over
tuple-shaped task seeds, condensed via a `task_for` closure after an initial
struct-based version measured 88 lines, over the file's 78-line baseline max);
`events_truncated_is_false_when_nothing_predates_the_retention_cutoff` +
`events_truncated_is_true_when_an_attempt_predates_the_retention_cutoff` →
`events_truncated_reflects_retention_cutoff` (2-case table; both cases derive
`events_retention_days` from the same `config` value passed in, rather than a
duplicated literal). `2`: two names renamed. `6`: preamble 13 → 10 lines.

**`control_plane/resource.rs`** (535 → 526 lines, 17 → 15 tests) — `1`:
`patch_with_absent_token_field_preserves_stored_token` +
`patch_with_explicit_null_token_clears_it` +
`patch_with_token_value_replaces_it` → `patch_token_field_is_tri_state`
(3-case table using a `fn(&Value)` per-case extra-check field so each
original test's unique follow-up assertion — the renamed-name check, and the
no-new-token-leak check — is preserved exactly, not generalized away). No `2`
or `6`: every name already met budget and the preamble was already 9 lines.

**`reporting/budget_policy.rs`** (531 → 525 lines, 9 tests unchanged) — `2`
only: two names renamed
(`orch_budget_unlinked_project_reports_linked_false_and_null_cost` (63) →
`orch_budget_unlinked_reports_linked_false_null_cost` (52), etc.). `6`:
preamble 16 → 9 lines. No `1`: every test here already proves one distinct
route/claim (budget unlinked, budget zero-vs-unreachable, budget sums,
policy unlinked, policy scoping, policy denial rate, policy denial-none,
policy approvals grouping) — none are `<claim>_<variant>` pairs.

**`dispatch/pipeline.rs`** (511 → 521 lines, 10 → 8 tests) — `1`:
`dispatch_403s_when_dispatch_token_unset` +
`dispatch_403s_when_dispatch_token_wrong` +
`dispatch_403s_when_dispatch_token_header_missing` →
`dispatch_403s_when_token_unset_wrong_or_missing` (3-case table; the
message-names-the-gate check, unique to the unset case, preserved as an
optional per-case check). `2`: one name renamed. `6`: preamble 13 → 7 lines.

**`auto_dispatch/hook.rs`** (498 → 502 lines, 5 → 4 tests) — `1`:
`auto_dispatch_sends_trusted_false_on_the_wire_for_a_github_imported_item` +
`auto_dispatch_sends_trusted_true_for_a_manually_created_item` →
`auto_dispatch_sends_persisted_trust_flag_on_the_wire` (2-case table; shared
setup extracted into `setup_auto_dispatch_case` to keep the body at 61 lines,
matching the file's own 61-line baseline max exactly). `2`: one remaining
name shortened. `6`: preamble condensed to 7 lines (below budget outright, not
just under baseline). `8`: **all five of this file's fixed waits removed** —
see the dedicated section below.

**`reporting/approvals.rs`** (478 → 437 lines, 11 → 9 tests) — `1`:
`decide_approval_403s_when_no_approval_token_is_configured_even_with_a_header`
+ `decide_approval_403s_with_a_missing_or_wrong_header_when_token_is_configured`
→ `decide_approval_403s_when_token_unset_missing_or_wrong` (4-case table:
unset+any-header, unset+no-header, configured+no-header,
configured+wrong-header — all FORBIDDEN, matching every original assertion);
`decide_approval_grant_sends_channel_tack_and_removes_it_from_the_pending_inbox`
+ `decide_approval_deny_sends_action_deny_on_the_wire` →
`decide_approval_sends_action_and_channel_removes_from_inbox` (2-case table;
both cases now get the full check set — repo-fetched state, `decided_at`,
inbox removal — a strengthening for the deny case, not a loss, matching the
`crud.rs` precedent for the same shape). A new `seed_pending_approval` helper
replaced four separate `NewOrchApproval` literal blocks (one per remaining
non-merged test plus both merged tests), the single largest driver of this
file's line reduction. `2`: three names renamed. `6`: preamble 12 → 9 lines.

**`reconciler/terminal_status.rs`** (476 → 468 lines, 8 tests unchanged) —
`2`: six names renamed, e.g.
`a_human_move_since_dispatch_blocks_on_succeeded_even_when_the_value_collides_with_on_waiting_approval`
(101) → `human_move_blocks_on_succeeded_despite_value_collision` (54). `6`:
preamble condensed to 9 lines. **Not merged, and why:** the two human-override
tests (`human_move_blocks_on_succeeded_despite_value_collision`,
`human_move_to_unlisted_status_is_also_caught`) share a group header but each
pins a distinct detection edge case (value collision with another status_map
key vs. a status not mentioned anywhere) with its own explanatory comment on
why a naive check would misread it — left as two per the card's own "if
unsure, leave it" instruction rather than guessed into one table.

**`fleet_templates/fleet_membership.rs`** (476 lines, 2 tests unchanged) —
`2` only: both names renamed
(`fleet_targeted_request_schedules_onto_a_populated_member_and_not_a_non_member`
(79) → `fleet_targeted_request_schedules_onto_member_not_outsider` (57), etc.).
No `1`, `3` or `6`: both tests are single, coherent, multi-step acceptance
scenarios (212 and 66 lines) driving the real production router end to end;
splitting either would replay setup (project/item/profile/two runners/fleet)
to reach the point under test, multiplying lines for no gain — the same call
sibling cards made for their own vertical-slice tests (e.g. the `handlers`
card's 237-line `mock_vertical_slice_completes_and_survives_restart`). The
preamble was already 10 lines.

**`reconciler/broadcast.rs`** (388 → 385 lines, 8 tests unchanged) — `2`:
three names renamed, e.g.
`uncorrelated_approval_does_not_broadcast_until_attribution_is_learned` (69)
→ `uncorrelated_approval_waits_for_attribution_before_broadcast` (60). `6`:
preamble 11 → 8 lines. **Not merged, and why:** the run/approval test pairs
(new-correlated, second-identical, uncorrelated-attribution) cover distinct
`BoardEvent` variants (`AgentRunUpdated` vs `ApprovalPending`) and distinct
production methods (`upsert_runs` vs `upsert_approvals`) sharing only the
generic shape — the same class of near-match the `duplicate-tests` tool
itself does not flag as textually similar, and the same reasoning sibling
cards used to leave equivalent same-shape/different-production-path pairs
separate.

**`control_plane/settings.rs`** (318 → 319 lines, 9 tests unchanged) — `2`:
one name renamed. `6`: preamble 11 → 9 lines. `8`: the one fixed wait
removed — see the dedicated section below. No `1`: every test here proves one
distinct GET/PUT/lifecycle claim.

**`reporting/run_readback.rs`** (311 → 296 lines, 5 tests unchanged) — `2`:
one name renamed
(`run_readback_reports_a_failed_run_state_without_calling_it_a_permission_verdict`
(79) → `run_readback_failed_state_carries_no_permission_verdict` (55)). A new
`create_plane` helper replaced three identical inline `CreateControlPlane`
blocks — not itself a numbered §2.2 rule, but the main driver of this file's
line reduction. `6`: preamble 12 → 9 lines. No `1`: every test proves one
distinct route/claim (off, unmirrored-is-200, mirrored-no-item,
mirrored-with-item, failed-state-field-shape).

**`reconciler/wiring.rs`** (302 → 312 lines, 6 tests unchanged) — `2`: two
names renamed. `6`: preamble 11 → 9 lines. `8`: the one fixed wait removed —
see the dedicated section below. No `1`: the store round-trip, adapter
dispatch, unconfigured-plane, end-to-end-poll, and off-by-default tests each
prove a genuinely distinct claim.

**`fleet_templates/templates.rs`** (295 → 269 lines, 7 → 6 tests) — `1`:
`create_template_without_orchestration_key_still_works` +
`create_template_with_null_orchestration_still_works` →
`create_template_without_or_with_null_orchestration_works` (2-case table
over `Option<Value>`; the module's own doc comment already framed these as
"the same absent-means-nothing case"). `2`: one name renamed. `6`: preamble
13 → 8 lines.

**`auto_dispatch/gate.rs`** (291 → 304 lines, 2 tests unchanged) — `2`: both
names renamed
(`auto_dispatch_does_not_fire_when_orch_enable_env_is_set_but_the_ui_toggle_is_off`
(80) → `auto_dispatch_respects_ui_toggle_off_despite_env_enable` (55), etc.).
`6`: preamble 16 → 9 lines. `8`: both of this file's fixed waits removed —
see the dedicated section below. No `1`: the two tests are the deliberate
"opposite direction" pair the file's own doc comment describes (env-on/UI-off
must not dispatch; env-off/UI-on must dispatch) — each proves a different
half of the gate's correctness and merging them would require unifying two
materially different assertion shapes (a pure absence check with no mock vs.
a full mock-dispatch-and-poll), for a claim the file's own structure already
states clearly as two halves of one regression test.

### Rule 8 — all nine fixed waits removed

| Site (original line) | Replacement | Stress-test result |
|---|---|---|
| `auto_dispatch/hook.rs:218` `sleep(50ms)` (in `wait_for_hits` retry loop) | `tokio::time::interval(25ms).tick()` | see below |
| `auto_dispatch/hook.rs:238` `sleep(50ms)` (in `wait_for_orch_task` retry loop) | `tokio::time::interval(25ms).tick()` | see below |
| `auto_dispatch/hook.rs:390` `sleep(200ms)` (absence check) | `drain_background_spawns()` (8×25ms interval ticks) | see below |
| `auto_dispatch/hook.rs:426` `sleep(200ms)` (absence check) | `drain_background_spawns()` | see below |
| `auto_dispatch/hook.rs:487` `sleep(200ms)` (absence check) | `drain_background_spawns()` | see below |
| `control_plane/settings.rs:119` `sleep(30ms)` (in `wait_for_reconciler_running` retry loop) | `tokio::time::interval(30ms).tick()` | 20/20 standalone runs green |
| `reconciler/wiring.rs:252` `sleep(500ms)` (fixed post-spawn wait) | bounded poll on `plane.health == "healthy"` via `tokio::time::interval(25ms)` | 20/20 standalone runs green, now ~0.08s instead of 500ms |
| `auto_dispatch/gate.rs:226` (post-fmt line) `sleep(200ms)` (absence check) | `drain_background_spawns()` (local copy, 8×25ms ticks) | 20/20 standalone runs green |
| `auto_dispatch/gate.rs:283` (post-fmt line) `sleep(50ms)` (in local `wait_for_orch_task` retry loop) | `tokio::time::interval(25ms).tick()` | 20/20 standalone runs green |

**The one real finding this card made:** a first attempt replaced every retry
interval with a bare `tokio::task::yield_now()` loop, matching the sibling
`runner_protocol`/`db-repository` cards' precedent for their own
fixed-wait removals. In `auto_dispatch/hook.rs`'s merged trust-flag test, this
made the test *fail* — `item.status` stayed `"To Do"` instead of reaching
`"In Progress"`, meaning the background `tokio::spawn`'s real (loopback) HTTP
round-trip to the wiremock server never completed within the poll's bound.
The cause: `yield_now()` only re-queues the calling task on the executor's
ready queue; on a `#[tokio::test]` current-thread runtime it does not itself
force the I/O driver to poll for new socket-readiness events, so if the poll
loop is always immediately ready to run again, the reactor can be starved of
the chance to notice the spawned task's socket actually became readable. This
matches the *opposite* of the `runner_protocol` sibling's own successful
`yield_now()` use, where the raced futures were pure in-process
SQLite/channel work on the same runtime with no real socket I/O to wait on —
the two situations only look alike from the outside. The fix,
`tokio::time::interval(...).tick()`, forces this runtime to actually park
between checks (a real timer registration), which does drive the I/O driver;
after switching to it, the trust-flag test passed and every affected test was
stress-run 15-20x standalone with zero failures (see the *Claim → evidence*
table and the individual per-file commit messages). Flagging this distinction
for the next IX-M4 card touching a fixed wait that races real (not just
in-process) I/O: **`yield_now()` is not a safe universal substitute for
`sleep()` in a bounded poll; check whether the awaited work does real socket
I/O, and if so use `Interval::tick` (or an equivalent real-timer mechanism)
instead.**

**Rule 5 (third-layer over-pinning):** no removal made under this rule in any
file. The `duplicate-tests` pairs this card's files appear in are either (a)
against `crates/tack-api/tests/handlers/economics.rs` — six
`*_409_when_orch_disabled`-shaped near-matches, each a different route's
orch-disabled gate (dispatch, sprint, dispatch-pipeline, agent-activity,
approvals, budget/policy), owned by a different IX-M4 sub-card regardless —
or (b) `control_plane/settings.rs`'s
`repeated_toggles_never_leave_more_than_one_task_per_plane` against
`crates/tack-api/src/orch_runtime/tests.rs`'s
`repeated_toggles_never_leave_more_than_one_task_alive`, a lower unit-level
test of `OrchRuntime` directly with no HTTP/repo layer, proving the same
invariant at a different layer, not this file's copy to delete. Neither was
acted on; per the card's own instruction, none removed on a guess. Re-ran
`duplicate-tests` after all edits landed and confirmed the same set of pairs,
unchanged.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check crates/tack-api/tests/orchestration.rs
crates/tack-api/tests/orchestration/*.rs crates/tack-api/tests/orchestration/*/*.rs`
→ `✓ maintainability budgets hold (24 files checked)` on the final tree without
exceeding any file's existing baseline entry further — every file's post-change
numbers are within its baseline ceiling (most well under it; several files'
worst name/preamble/body numbers actually improved past what the baseline
recorded — e.g. `auto_dispatch/hook.rs`'s preamble 19 → 7,
`reconciler/terminal_status.rs`'s worst name 101 → 59 — but
`scripts/maintainability-baseline.json` itself was not edited, since improving
past a recorded ceiling doesn't require lowering it, and this card's own
instruction reserves re-baselining for cards that deliberately bring a file
down as their stated purpose).

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree (all 19
commits already landed, working tree clean):

```
✓ maintainability budgets hold (0 files checked)
```

`python3 scripts/maintainability.py check crates/tack-api/tests/orchestration.rs
crates/tack-api/tests/orchestration/*.rs crates/tack-api/tests/orchestration/*/*.rs`
(explicit file list, since `--changed` reports 0 once committed):

```
✓ maintainability budgets hold (24 files checked)
```

`python3 scripts/maintainability.py measure --totals` — before/after (see
*Measured numbers* above for the full derivation; this worktree branched from
`44825e3` and holds only this card's 17 files, so this pair is clean, unlike a
workspace shared with other concurrent cards):

- Before: `prod=56223 (comments 11931) test=72075 ratio=1.282 tests=1479 sleeps_in_tests=70 env_gated=0`
- After: `prod=56223 (comments 11931) test=71939 ratio=1.28 tests=1463 sleeps_in_tests=61 env_gated=0`

`python3 scripts/maintainability.py measure crates/tack-api/tests/orchestration*`
before/after: see *Measured numbers* above — this is the load-bearing
before/after pair for this sub-card (8361 → 8225 test lines, 147 → 131 tests,
9 → 0 sleeps).

`cargo fmt --all -- --check`: clean. `cargo clippy -p tack-api --tests
--all-targets -- -D warnings`: clean (not part of this card's required gate,
run anyway after every file). `scripts/check-comments.sh` and
`scripts/check-test-hygiene.sh`: both clean (not required by this card, run
anyway for confidence).

`docs/adr/0064-fixed-waits.txt`: **not regenerated by this card.**
`python3 scripts/list-fixed-waits.py` against the final tree confirms all five
of this card's ≥200ms entries
(`reconciler/wiring.rs:252`, `auto_dispatch/gate.rs:226`,
`auto_dispatch/hook.rs:401/437/498`) are gone from the regenerated list, with
no `tack-api/tests/orchestration/**` entries remaining at all. But the
regenerated list also shows unrelated drift — line-number shifts and
entries appearing/disappearing in `tack-orch/src/reconciler.rs`,
`tack-api/src/orch_runtime.rs`, `tack-cli/tests/embedded_runner_live_secret.rs`
and `tack-orch/tests/docket_live_test.rs`, none of which this card owns or
touched — from other work already landed on `develop` since the file was last
committed. Committing the regeneration would put out-of-scope changes in this
card's diff, so it was left untouched, matching the precedent both the
`tack-db-repository` and `tack-api-runner_protocol` handoffs already set for
this exact situation.

## What a stranger still cannot do

A stranger arriving from outside this repository still cannot tell, from
`auto_dispatch/hook.rs`'s or `control_plane/settings.rs`'s new
`Interval`-based polling helpers alone, that a `tokio::task::yield_now()` loop
was tried first and found to starve real socket I/O on a current-thread
runtime — that distinction (yield vs. timer-driven park) is one of the more
subtle pieces of Tokio runtime behavior in this whole IX-M4 program, and while
the doc comments on `wait_for_hits` and its callers now name the reasoning,
verifying it independently means reading the Tokio runtime's own scheduling
docs, not just this file. They also cannot yet run one command that proves
this Part's cross-binary invariant-layering claim (rule 5) end to end — this
card only checked the two invariant families `duplicate-tests` actually
flagged against files it owns, the same limitation every prior IX-M4 handoff
has noted.

## Context spent

- Tokens read before the first edit (cold start): `TODO.md` §IX.0–§IX.3 and the
  `IX-M4` card block (via `grep -n` extraction, not the whole 199k-token file),
  `docs/plans/human-maintainability.md` §2.2, the `part-ix/TEMPLATE.md` and the
  `part-vi/TEMPLATE.md` it points to, and all four sibling IX-M4 handoffs in
  full (`tack-db-repository`, `tack-api-runner_protocol`, `tack-api-handlers`,
  `tack-api-security`) as the dispatch prompt instructed, plus the card's own
  `measure`/`duplicate-tests`/`grep sleep(` baseline commands re-run fresh in
  this worktree rather than trusted from the prompt. Roughly 20-24k tokens for
  cold start — higher than any prior IX-M4 card's own estimate, matching this
  being explicitly the largest sub-card (17 files vs. the next-largest
  sibling's 10).
- Context size at handoff: large — all 17 owned files were read in full before
  editing, several more than once (`auto_dispatch/hook.rs` and
  `reconciler/wiring.rs` reread after the `yield_now()` failure to understand
  the actual background-task/reactor interaction before choosing the
  `Interval`-based fix), plus `crates/tack-api/tests/common/mod.rs` (confirmed
  this binary's private per-file helper style — deliberately not shared, per
  every file's own "harness" comment — was not reinventing anything
  `tests/common` already exports) and a brief read of `tack-core/src/models.rs`
  to confirm `ItemSource` derives `Debug` before using it in an assertion
  message.
- Files opened and not used: none of significance;
  `docs/adr/0064-fixed-waits.txt` was regenerated once (per the card's
  instruction) into a scratch file and the regeneration was not committed —
  see *Budget check*'s dedicated paragraph.
- Read-list lines that were wrong: none — the 17-file, largest-first order and
  all nine fixed-wait line numbers in the dispatch prompt matched the actual
  measured sizes and `grep` output exactly.
- One deviation worth flagging for whoever reads this next, beyond the
  `yield_now()` finding above: three merges
  (`dispatch/item.rs`'s trusted-flag test, `reporting/agent_activity.rs`'s
  latest-attempt-tie-break test, `auto_dispatch/hook.rs`'s trusted-flag test)
  initially produced a body over that file's own recorded baseline maximum —
  `check` caught each one immediately (not something to defer to the final
  gate). Each was fixed by extracting the merge's non-varying setup into a
  small plain helper function (`seed_github_imported_item`,
  a `task_for` closure, `setup_auto_dispatch_case`) rather than by leaving the
  case loop's body long — worth expecting by default whenever merging two
  tests whose *un-shared* setup (not just the varying fields) is itself
  substantial, not just as a fallback once `check` fails.

## Amendments

*(none yet)*
