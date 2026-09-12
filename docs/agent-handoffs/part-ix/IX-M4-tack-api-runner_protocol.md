# IX-M4-tack-api-runner_protocol handoff

- Base SHA / branch / final SHA: `9433ef9` / `agent/ix-m4-api-runner_protocol` / `289897b` (code commit; this handoff lands in a second commit on top)
- Files changed (must equal ownership list): `crates/tack-api/tests/runner_protocol.rs` and
  every file under `crates/tack-api/tests/runner_protocol/` (`lifecycle.rs`, `decisions.rs`,
  `artifact_events.rs`, `log_capture.rs`). No other file touched — `tests/common/mod.rs`
  (owned by IX-M3-api) and `crates/tack-api/src/**` were both read but not edited.
- Contract fixtures consumed: none.
- Behavior implemented: none — pruning only (§IX.1 rule 1's two named exceptions apply: both
  removed fixed waits are replaced with a bounded, condition-free cooperative-yield loop, not a
  behavior change). Every original assertion is preserved; table-driven merges keep every
  original case's input and expected outcome exactly.
- Tests added and exact commands/results: none added; two test-function pairs merged into one
  table-driven test each (net −2 tests). `cargo nextest run --workspace -E
  'binary(runner_protocol)'`: `92 tests run: 92 passed, 0 skipped` (includes tests from the two
  `#[path]`-loaded production modules' own colocated unit tests, not owned by this card).
- Failure/adversarial case proved: n/a — pruning only, no new behavior to adversarially test.
  The two rewritten concurrency tests (see *What was removed*, rule 8 rows) were stress-run 30
  and 40 times respectively with zero failures (see *Budget check*) to prove the fixed-wait
  removal didn't trade a slow-but-reliable wait for a flaky one.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: n/a.
- Secrets/logging review: n/a — no logging or secret-handling code touched; the three
  redaction tests (`logs_never_contain_raw_credentials_only_ids`,
  `logs_never_leak_event_payloads_or_artifact_content_only_ids`,
  `logs_never_contain_the_raw_answer_text_or_prompt_only_ids`) are unchanged in substance.
- Safe merge order and likely conflicts: this card owns one test binary exclusively (per
  §IX.4's per-binary ownership rule) and touches no shared file — no conflicts expected with
  the sibling `tack-db-repository` IX-M4 card running in parallel, nor with any other open card.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| No `sleep(` remains in this binary's owned files | `grep -rn 'sleep(' crates/tack-api/tests/runner_protocol/*.rs crates/tack-api/tests/runner_protocol.rs` → no output |
| Every test preamble is ≤10 lines | `measure` `mdoc` column: lifecycle 4, artifact_events 5, decisions 5, log_capture 5, root 5 (was 23/23/16/43/5) |
| Every test name is ≤60 characters | `measure` `name` column: lifecycle 57, artifact_events 59, decisions 57 (was 85/68/97) |
| Variant families collapsed to table-driven tests | `event_batch_conflict_vs_idempotency_conflict_retryable` (lifecycle, was 2 fns), `checksum_mismatch_stages_nothing` (artifact_events, was 2 fns), `bulk_sweep_denies_only_overdue_pending_decisions` (decisions, internal 4-row loop, was 4 sequential blocks) |
| Repeated enqueue+claim boilerplate consolidated | `enqueue_and_claim` helper added to `lifecycle.rs`, replacing the identical 5-line pattern at 8 call sites |
| No production file touched | `git diff --stat` shows only the 4 files under `crates/tack-api/tests/runner_protocol/` |
| The whole binary still passes | `cargo nextest run --workspace -E 'binary(runner_protocol)'` → `92 tests run: 92 passed, 0 skipped` |
| `cargo fmt --all -- --check` is clean | ran after the final edit, no output |
| `cargo clippy -p tack-api --tests --all-targets -- -D warnings` is clean | ran after the final edit, no warnings (not part of this card's required gate, run anyway) |

## What a stranger still cannot do

Nothing changed for a user of `tack` itself — this only prunes the shape of one test binary's
source. A contributor reading `lifecycle.rs`'s `full_runner_protocol_lifecycle_enroll_through_completion`
still finds one 270-line test walking the whole enroll→refresh→claim→accept→start→events→
decisions→artifacts→completion chain — deliberately kept as one continuous claim (see *What was
removed*, "not touched" section) rather than split into shorter per-step tests, since splitting
would multiply the shared setup rather than remove it. `decisions.rs`'s
`concurrent_resolves_serialize_to_exactly_one_winner` is similarly still long (98 lines) for the
same reason a real concurrency proof needs its own file-backed database and both racing calls
inline. Both are flagged, not silently left.

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree:

```
✓ maintainability budgets hold (4 files checked)
```

`python3 scripts/maintainability.py measure --totals`:

- Before (stashed): `prod=56223 (comments 11931) test=73332 ratio=1.304 tests=1498 sleeps_in_tests=74 env_gated=0`
- After: `prod=56223 (comments 11931) test=73012 ratio=1.299 tests=1496 sleeps_in_tests=72 env_gated=0`

`python3 scripts/maintainability.py measure crates/tack-api/tests/runner_protocol*`:

Before:
```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-api/tests/runner_protocol/lifecycle.rs              0     0   0%   1969   15    95  270   85   23
crates/tack-api/tests/runner_protocol/artifact_events.rs        0     0   0%   1187   14    42  105   68   23
crates/tack-api/tests/runner_protocol/decisions.rs              0     0   0%   1179   19    38  164   97   16
crates/tack-api/tests/runner_protocol/log_capture.rs            0     0   0%    120    0     0    0    0   43
crates/tack-api/tests/runner_protocol.rs                        0     0   0%     17    0     0    0    0    5
totals: prod=0 (comments 0) test=4472 ratio=0.0 tests=48 sleeps_in_tests=2 env_gated=0
```

After:
```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-api/tests/runner_protocol/lifecycle.rs              0     0   0%   1821   14    92  270   57    4
crates/tack-api/tests/runner_protocol/artifact_events.rs        0     0   0%   1154   13    44  105   59    5
crates/tack-api/tests/runner_protocol/decisions.rs              0     0   0%   1069   19    33   98   57    5
crates/tack-api/tests/runner_protocol/log_capture.rs            0     0   0%     91    0     0    0    0    5
crates/tack-api/tests/runner_protocol.rs                        0     0   0%     17    0     0    0    0    5
totals: prod=0 (comments 0) test=4152 ratio=0.0 tests=46 sleeps_in_tests=0 env_gated=0
```

`cargo fmt --all -- --check`: clean.

Fixed-wait stress proof (rule 8, both removed waits replaced with a bounded cooperative-yield
loop rather than a fixed delay — see *What was removed*):

```
cargo nextest run --workspace -E 'binary(runner_protocol) and (test(concurrent_refresh_rotations_exactly_one_wins) or test(concurrent_resolves_serialize_to_exactly_one_winner))'
```
run 40 times in a loop: 0 failures / 40.

`docs/adr/0064-fixed-waits.txt`: **not regenerated by this card.** Both removed waits (150ms in
`decisions.rs`, 50ms in `lifecycle.rs`) were already below the script's 200ms floor, so neither
ever appeared in that inventory — this card's fixed-wait removal produces no diff there. Running
`python3 scripts/list-fixed-waits.py` against the current tree *does* produce a large diff
(25→21 waits, several line-number shifts, entries appearing/disappearing in `tack-orch` and
`tack-cli` files this card does not own), which reflects other, unrelated work already landed on
`develop` since that file was last committed — out of this card's ownership to fix, and
regenerating it here would have touched files this card has no business touching. Flagging for
whoever next owns that file (likely a `tack-orch`/`tack-cli` IX-M4 sub-card or the Part IX
integrator).

## What was removed

- `event_checkpoint_conflict_response_carries_contract_correct_retryable_true` +
  `event_batch_replay_changed_content_is_idempotency_conflict_and_writes_nothing` (lifecycle.rs)
  → merged into `event_batch_conflict_vs_idempotency_conflict_retryable`, a 2-case table-driven
  test. Class **`1: variants → rows`** — the file's own comment already documented these as a
  matched "benign resync vs. changed-content replay" pair.
- `checksum_mismatch_stages_nothing` + `same_size_wrong_bytes_is_a_pure_checksum_mismatch_and_stages_nothing`
  (artifact_events.rs) → merged into one `checksum_mismatch_stages_nothing`, a 2-case
  table-driven test (mismatched-length body, same-length body). Class **`1: variants → rows`**.
- `bulk_sweep_denies_only_overdue_pending_decisions` (decisions.rs): four sequential
  `seed_decision` calls + four sequential `decision_row` assertions converted to one 4-row table
  iterated twice (seed, then verify). Class **`1: variants → rows`**.
- `decisions.rs`'s `concurrent_resolves_serialize_to_exactly_one_winner` and the race test's
  file-backed workspace/project/item/runner/profile seeding, previously duplicated inline,
  now shares `setup()`'s seeding via a new `seed_workspace(pool)` helper (setup differs only in
  which pool it's handed). No `1:`/`2:` class — this is plain duplication removal, not a rule
  from the acceptance table, but it brought the function from 178 to 98 lines.
- `lifecycle.rs`: 8 call sites' identical enqueue+claim boilerplate (`enqueue_request` followed
  by a `/claim` POST and pulling `attempt_id`/`fencing_token` out of the response) replaced with
  one `enqueue_and_claim` helper. Same class as above — duplication removal enabling several
  bodies to drop under 60 lines, not itself a numbered §2.2 rule.
- `decisions.rs:1134` `tokio::time::sleep(Duration::from_millis(150))` (inside
  `tokio::join!(resolve_a, resolve_b, delayed_release)`) → a bounded 512-iteration
  `tokio::task::yield_now()` loop. Class **`8: fixed wait`**. `join!` polls all three futures
  within one task, so `resolve_a`/`resolve_b` already reach their first internal await on their
  first poll inside the same `join!` call; the yield loop gives the runtime enough additional
  turns to let both racers' real (file-backed) SQLite I/O actually get dispatched before the
  held transaction commits. Verified: 30/30 passes before this change (with the sleep), 30/30
  passes after (see *Budget check*).
- `lifecycle.rs:1684` `tokio::time::sleep(Duration::from_millis(50))` (before `holder.commit()`,
  racing two `tokio::spawn`ed `/refresh` rotation tasks) → a bounded 512-iteration
  `tokio::task::yield_now()` loop, plus a trimmed version of the surrounding comment (the
  original explained, at length, why a bare `join!`/single `yield_now` had failed historically;
  kept the causal explanation, dropped the narrative "an earlier version... always failed the
  same way" framing per the comment rule). Class **`8: fixed wait`**. Verified empirically before
  choosing this replacement: with the sleep entirely removed (zero wait) the test still passed
  30/30 locally, and with a single `yield_now()` it also passed 20/20 — but since the file's own
  removed comment documented this exact test failing intermittently under `join!`/single-`yield_now`
  on a busier scheduler (plausibly CI), a generous 512-iteration bounded loop was kept rather
  than trusting a thin local margin. Final form re-verified 40/40 (see *Budget check*).
- Test names shortened to ≤60 characters (class **`2: name`**), no other change:
  `operator_and_runner_auth_do_not_substitute` (was
  `operator_auth_cannot_substitute_for_runner_auth_and_vice_versa`),
  `heartbeat_and_completion_replay_vs_conflict` (was
  `heartbeat_and_completion_idempotent_replay_and_conflicting_replay_are_distinguished`),
  `decision_and_artifact_id_reuse_is_idempotency_conflict`,
  `recovery_observation_requeues_and_replays_idempotently`,
  `completion_replay_changed_content_is_idempotency_conflict`,
  `concurrent_refresh_rotations_exactly_one_wins` (was
  `refresh_rotation_with_stale_expected_hash_is_rejected_not_overwritten`),
  `superseded_credential_refresh_returns_conflict_not_401`,
  `submit_artifacts_rejects_lost_and_needs_operator_states` (all lifecycle.rs);
  `crafted_traversal_artifact_id_stays_inside_storage_root`,
  `unverified_manifest_download_is_named_conflict_not_404`,
  `upload_over_default_json_body_ceiling_still_succeeds` (all artifact_events.rs);
  `resolve_pending_decision_matches_operator_answer`,
  `resolving_twice_with_same_answer_is_idempotent`,
  `resolving_with_different_answer_after_resolve_is_conflict`,
  `valid_runner_credential_cannot_self_resolve_decision`,
  `expiry_denies_with_audit_and_never_marks_item_done`,
  `restart_preserves_pending_decision_still_resolvable`,
  `answer_option_id_must_match_declared_options`,
  `freeform_decision_accepts_any_non_empty_option_id`,
  `answer_over_byte_limit_is_payload_too_large`,
  `unconfigured_decision_token_rejects_every_resolve`,
  `wrong_decision_token_rejects_the_resolve`,
  `correct_decision_token_with_valid_principal_resolves`,
  `concurrent_resolves_serialize_to_exactly_one_winner` (all decisions.rs).
- Module preambles trimmed to ≤10 lines (class **preamble**, plan §3): `lifecycle.rs` (23→4),
  `artifact_events.rs` (23→5), `decisions.rs` (16→5), `log_capture.rs` (43→5). In each case the
  displaced content (module-loading mechanics, the `tracing` interest-cache race rationale) was
  moved to a `///`/`//` block directly above the specific item it explains, not deleted — e.g.
  `log_capture.rs`'s race explanation now sits on `ensure_global_log_capture_installed`'s own doc
  comment; the purely historical "this module used to be three separate copies" paragraph was
  dropped outright (narrative `git log` already carries, per the comment rule) rather than moved.

**Not touched, and why:** `full_runner_protocol_lifecycle_enroll_through_completion`
(lifecycle.rs, 270 lines) is one continuous enroll-through-completion journey, not a family of
near-identical variants — splitting it into per-step tests would each need to replay the prior
steps' setup (enroll, refresh, claim, accept, start, ...) to reach the point under test,
multiplying total lines rather than reducing them, and the test's own name and the module's
`//!` preamble both already frame it as one deliberate end-to-end proof. Left over the 60-line
target for the same reason the plan's own M4 row lists `execution_repo.rs`'s largest functions
as expected to persist: the file's `test_fn_max_lines` baseline (270) already reflected this
before this card, and `check --changed`'s ratchet holds it, not worsens it.
`decision_and_artifact_id_reuse_is_idempotency_conflict` (lifecycle.rs) and
`answer_option_id_must_match_declared_options`-style single-assertion tests in `decisions.rs`
were left as single functions rather than force-split into tables: each already tests one
coherent claim in under 60 lines, and further fragmentation would only multiply per-test setup
overhead for no rule-compliance gain. The cross-layer `checksum_mismatch_stages_nothing`
duplicate flagged by `duplicate-tests` (this file's router-level HTTP test vs.
`crates/tack-api/src/handlers/runner_protocol/artifact_storage/tests.rs`'s unit test) was left
in place on both sides: the `src/`-side test drives `ArtifactStorage::store_streaming` directly
with no HTTP, no router, and no repository row — a lower, unit-level layer than either
"repository" or "router" in the plan's vocabulary — while this file's test drives the full
HTTP/DB/filesystem path end to end. They are not proving the identical claim at the identical
layer, so neither is "this card's copy to delete unilaterally" per the card's own instructions;
flagging for whoever later resolves cross-scope layer duplication, and noting the `src/` file
belongs to a different card's ownership.

## Re-baselined?

`no`. `check --changed` reports budgets holding on the final tree without exceeding any file's
existing baseline entry further; no file was brought below a budget it was previously over in a
way that would need the baseline lowered (the two large surviving functions, 270 and 98 lines,
stay within their own already-recorded baseline ceiling).

## Context spent

- Tokens read before the first edit (cold start): `TODO.md` §IX.0–§IX.3 and the `IX-M4` card
  block (as directed, via `grep -n` extraction, not the whole file), `docs/plans/human-maintainability.md`
  §2.2 and §5's M4 row, `docs/agent-handoffs/part-ix/TEMPLATE.md` plus the `part-vi/TEMPLATE.md`
  it points to, `crates/tack-api/tests/common/mod.rs` (to confirm what IX-M3-api already
  provided and reuse rather than reinvent), and the three `measure`/`duplicate-tests`/`grep
  sleep(` commands the dispatch prompt had already run. Roughly 8–9k tokens for cold start.
- Context size at handoff: large — all four owned files were read in full (lifecycle.rs twice,
  once before and once after the first round of edits, since its 1969 lines exceeded one read
  window), plus the cross-layer `artifact_storage/tests.rs` comparison file and
  `crates/tack-db/src/lib.rs`/`crates/tack-test-support/src/lib.rs` while diagnosing the
  concurrency tests' actual synchronization mechanism (pool size, `:memory:` sharing, WAL mode)
  before deciding how to replace their fixed waits.
- Files opened and not used: none of significance.
- Read-list lines that were wrong: the dispatch prompt's two `sleep(` line numbers
  (`decisions.rs:1134`, `lifecycle.rs:1684`) were both exactly right. One thing worth flagging
  for a future IX-M4 card on a concurrency-race test: understanding *why* a fixed wait was
  chosen (here, real OS-thread/connection-pool scheduling, not just "flaky without it") required
  reading the production pool configuration (`max_connections(5)`, `PRAGMA journal_mode=WAL`)
  and empirically stress-testing candidate replacements (30–40 repeated runs each) rather than
  reasoning from the test code alone — a fixed-wait replacement that isn't empirically
  re-verified at volume is not proven, whatever the mechanism looks like on paper.

## Amendments

*(none yet)*
