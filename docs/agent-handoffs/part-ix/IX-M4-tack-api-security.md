# IX-M4-tack-api-security handoff

- Base SHA / branch / final SHA: `69fbdc7` / `agent/ix-m4-api-security` / `ed7dbad`
- Files changed (must equal ownership list): `crates/tack-api/tests/security.rs` (untouched,
  only read), `crates/tack-api/tests/security/chaos_recovery.rs`, `trust_boundary.rs`,
  `wip_limit_race.rs`, `board_drag_wip_race.rs`, `cors.rs` — exactly this card's one test
  binary and directory. `crates/tack-api/tests/common/mod.rs` was read (to confirm what it
  already provides) but not edited; `crates/tack-api/src/**` was never touched.
- Contract fixtures consumed: none (`docs/contracts/runner-v1/` not touched).
- Behavior implemented: none — this card prunes tests only. No production file under
  `crates/tack-api/src/**` was touched.
- Tests added and exact commands/results: none added as a new *feature* proof; test count
  moved 20 -> 18 in this binary purely from merging existing coverage (three near-identical
  origin/bind cases in `trust_boundary.rs` became one table-driven test).
  `CARGO_TARGET_DIR=/mnt/data/home/ox/cargo-target-ix-m4-api-security cargo nextest run
  --workspace -E 'binary(security)'` — 18 tests run, 18 passed, 0 skipped (verified after
  every file's commit, and again after a checkout round-trip used only to measure the
  workspace-wide before/after totals). The four concurrency-race tests
  (`fleet_race_between_two_runners_grants_exactly_one_lease`,
  `duplicated_credential_race_grants_exactly_one_lease`,
  `concurrent_dispatch_into_one_wip_column_stays_under_limit`,
  `concurrent_board_drags_into_one_wip_column_stay_under_limit`) were additionally re-run 5x
  standalone to rule out flakiness introduced by the renames/restructuring: 20/20 passed.
- Failure/adversarial case proved: n/a (no new behavior; every adversarial claim in the
  binary — stale fences, revoked credentials, path traversal, oversized artifacts, corrupted
  rows, CORS gaps, WIP races — is unchanged from before this card).
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: several bodies remain over the 60-line hard cap
  (see *What was removed*) — deliberately, as single coherent adversarial scenarios, matching
  the `runner_protocol`/`db-repository` sibling cards' own precedent for the same kind of test.
- Secrets/logging review: n/a (test-only changes; no logging or secret-handling code touched).
- Safe merge order and likely conflicts: independent of the sibling `tack-api` `handlers`
  IX-M4 card (different binary, different files — that card was running concurrently in a
  separate worktree while this one worked; `/tmp/cargo-target-ix-m4-api-handlers` was
  deliberately left untouched for that reason). No conflicts expected against `develop`
  since only `crates/tack-api/tests/security/**` and the one root file were read.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| All behavior in the five files is unchanged | `cargo nextest run --workspace -E 'binary(security)'` — 18/18 pass, same assertions per case as before (see *What was removed* for the case-to-case mapping on the one merge) |
| No `sleep(` anywhere in this binary (confirmed, not just believed) | `grep -rn 'sleep(' crates/tack-api/tests/security/*.rs crates/tack-api/tests/security.rs` — no hits, before or after this card |
| No `duplicate-tests` pair names a file under `tests/security/` | `python3 scripts/maintainability.py duplicate-tests` — grepped for `security/` in the output, no hits, before or after |
| Every test name in this binary is ≤60 chars, no articles/narrative | `grep -oP '(?<=^async fn )\w+' crates/tack-api/tests/security/*.rs \| awk '{ if (length($0) > 60) print }'` — empty |
| Every file's preamble is ≤10 lines | `measure`'s `mdoc` column: chaos_recovery 9 (was 17), trust_boundary 6 (unchanged, already compliant), wip_limit_race 10 (was 20), board_drag_wip_race 9 (was 21), cors 6 (was 12) |
| The three origin/bind variant tests in `trust_boundary.rs` are now one table-driven test | `board_live_handshake_origin_authorization` (52-line body, 4-case table); the three original functions and their exact origin/expected-outcome pairs are gone, absorbed as table rows |
| The 230-line stale-fence test is now 70 lines with the same claim | `stale_fence_writes_nothing_across_every_mutation_route`; setup and per-route "no write" checks extracted into helpers (`setup_superseded_fence`, `assert_stale_rejected`, `assert_no_heartbeat_recorded`, `assert_no_decision_row`, `assert_no_artifact_row`) |
| `cargo fmt --all -- --check` is clean | ran after the final commit, no diff |
| `cargo clippy -p tack-api --tests --all-targets -- -D warnings` is clean | ran after the final commit, no warnings (not this card's required gate, run anyway) |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-api/tests/security.rs
crates/tack-api/tests/security/*.rs`

Before (measured at card start, 2026-09-12):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-api/tests/security/chaos_recovery.rs                0     0   0%   1289    8   107  230   90   17
crates/tack-api/tests/security/trust_boundary.rs                0     0   0%    333    7    29   70   83    6
crates/tack-api/tests/security/wip_limit_race.rs                0     0   0%    275    1    85   85   76   20
crates/tack-api/tests/security/board_drag_wip_race.rs           0     0   0%    214    2    46   72   78   21
crates/tack-api/tests/security/cors.rs                          0     0   0%    147    2    32   52   61   12
crates/tack-api/tests/security.rs                               0     0   0%     20    0     0    0    0    6
totals: prod=0 (comments 0) test=2278 ratio=0.0 tests=20 sleeps_in_tests=0 env_gated=0
```

After (this handoff):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-api/tests/security/chaos_recovery.rs                0     0   0%   1243    8    86  106   59    9
crates/tack-api/tests/security/trust_boundary.rs                0     0   0%    342    5    43   70   53    6
crates/tack-api/tests/security/wip_limit_race.rs                0     0   0%    265    1    85   85   57   10
crates/tack-api/tests/security/board_drag_wip_race.rs           0     0   0%    202    2    46   72   59    9
crates/tack-api/tests/security/cors.rs                          0     0   0%    141    2    32   52   53    6
crates/tack-api/tests/security.rs                               0     0   0%     20    0     0    0    0    6
totals: prod=0 (comments 0) test=2213 ratio=0.0 tests=18 sleeps_in_tests=0 env_gated=0
```

Net: 2278 -> 2213 test lines (-65, -2.9%); 20 -> 18 tests; every file's `name` max ≤60 (was
up to 90); every file's `mdoc` (preamble) ≤10 (was up to 21); `chaos_recovery.rs`'s worst
body 230 -> 106 lines (the stale-fence extraction; its remaining bodies over 60 are
documented single-scenario exceptions, unchanged in length from before this card).
`sleeps_in_tests` 0 -> 0 (already true, reconfirmed).

`python3 scripts/maintainability.py measure --totals` — workspace-wide, before/after this
card (captured via a temporary detached checkout of the base commit `69fbdc7`, measured, then
returned to the branch tip — `git status` was clean both before and after the round trip and
the full binary was re-run afterward to confirm nothing drifted):

- Before: `totals: prod=56223 (comments 11931) test=72827 ratio=1.295 tests=1497 sleeps_in_tests=71 env_gated=0`
- After: `totals: prod=56223 (comments 11931) test=72762 ratio=1.294 tests=1495 sleeps_in_tests=71 env_gated=0`

(The workspace-wide `-2` tests and `-65` lines match this card's own binary-scoped numbers
exactly, confirming no other open card's files moved between the two measurements.)

## What was removed

Per-file, with the §2.2 rule number for each change (`1: variants → rows`, `2: name`,
`3: body`, `6: preamble`):

**`chaos_recovery.rs`** — `2`: all 8 test names shortened to ≤60 chars and de-articled, e.g.
`two_distinct_runners_in_the_same_fleet_race_to_claim_one_request_and_exactly_one_wins` (90
chars) → `fleet_race_between_two_runners_grants_exactly_one_lease` (56);
`a_duplicated_credential_used_concurrently_never_grants_two_leases_for_the_same_request` →
`duplicated_credential_race_grants_exactly_one_lease`;
`a_revoked_runner_credential_is_rejected_everywhere_and_cannot_advance_its_leased_attempt` →
`revoked_credential_rejected_everywhere_freezes_attempt`;
`stale_fence_writes_nothing_on_heartbeat_decisions_artifacts_cancellation_and_recovery` →
`stale_fence_writes_nothing_across_every_mutation_route`;
`oversized_artifact_declared_size_is_rejected_per_item_and_cumulative_without_writing_a_row`
→ `oversized_artifact_rejected_per_item_and_cumulative_caps`;
`artifact_id_path_traversal_payloads_never_escape_the_configured_storage_root` →
`path_traversal_artifact_ids_never_escape_storage_root`;
`event_batch_checkpoint_mismatch_is_rejected_and_a_byte_identical_replay_stays_idempotent` →
`checkpoint_mismatch_rejected_identical_replay_is_idempotent`;
`a_corrupted_request_snapshot_row_degrades_to_a_typed_error_not_a_panic` →
`corrupted_snapshot_row_degrades_to_typed_error_not_panic`. `6`: preamble 17 → 9 lines,
updating the two test-name cross-references it makes to their new names. `1`+`3`: the
stale-fence test's five sequential "call this route with the stale fence, assert
conflict/stale_lease, assert nothing was written" blocks were the file's one genuine variant
family (explicitly framed as such by its own old name: "on heartbeat, decisions, artifacts,
cancellation, and recovery") — extracted `setup_superseded_fence` (the shared
claim-supersede-and-reclaim setup, previously inlined in the test body) and
`assert_stale_rejected` (the repeated status/error-code assertion) as plain async helpers, plus
three small per-route "nothing written" checks (`assert_no_heartbeat_recorded`,
`assert_no_decision_row`, `assert_no_artifact_row`). The test itself dropped from 230 to 70
lines while keeping every original assertion (same five routes, same stale fence, same final
"exactly 2 fences minted" check) — kept as one test rather than five, since the setup
(claim → recovery-observation → reclaim, to produce one already-superseded fence) is itself
the expensive, shared part; splitting into five tests would re-derive that same superseded
state five times for a claim whose point is that *one* supersession event blocks every route.

  Explicitly considered and rejected for `1`: the other seven tests (fleet race, duplicated
  credential, revoked credential, oversized artifact, path traversal, checkpoint
  mismatch/replay, corrupted row) are each a single coherent adversarial scenario proving a
  distinct claim, not a variant family — matching the `runner_protocol` sibling's own
  judgment for its 270-line lifecycle test and 98-line concurrency test. Their bodies (73–106
  lines) are unchanged from before this card and remain over the 60-line target; splitting
  any of them would fragment one claim (e.g. "ten artifacts accumulate to just under the cap,
  an eleventh overflows it" in `oversized_artifact_rejected_per_item_and_cumulative_caps`)
  into pieces that don't individually prove anything.

**`trust_boundary.rs`** — `1`: `board_live_handshake_from_the_vite_dev_origin_is_authorized_on_a_loopback_bind`,
`board_live_handshake_judges_the_origin_host_as_an_address_not_a_prefix` (itself already two
sequential cases inline) and `board_live_handshake_from_a_loopback_origin_is_still_refused_on_a_non_loopback_bind`
merged into one table-driven `board_live_handshake_origin_authorization` (4-case `OriginCase`
table: vite-dev-origin/loopback-bind/authorized, `127.`-prefix-hostname/loopback-bind/refused,
IPv6-loopback-literal/loopback-bind/authorized, loopback-origin/non-loopback-bind/refused) —
each original origin/bind/expected-outcome triple preserved exactly as a row (7 tests → 5).
`2`: `suffix_lookalikes_stay_behind_the_bearer_gate` → `suffix_lookalikes_stay_behind_bearer_gate`;
`split_origin_websocket_handshake_accepts_subprotocol_credential_without_query_token` (85
chars) → `handshake_credential_travels_in_subprotocol_not_query` (53);
`board_live_handshake_selects_the_tack_v1_subprotocol` → `board_live_handshake_selects_tack_v1_subprotocol`
(dropped "the"). Preamble was already 6 lines — no `6` change needed.

**`wip_limit_race.rs`** — `2`: `concurrent_dispatch_into_the_same_wip_limited_column_never_exceeds_the_limit`
(76 chars, 3 articles) → `concurrent_dispatch_into_one_wip_column_stays_under_limit` (57).
`6`: preamble 20 → 9 lines (kept: what's raced, why the mock delay widens the window, why
per-item locks don't help; dropped nothing substantive, only compressed the phrasing). No `1`
or `3`: the file's only function is already one distinct concurrency claim, body unchanged
(85 lines, a single coherent race proof, same exception class as `chaos_recovery.rs`'s large
scenario tests).

**`board_drag_wip_race.rs`** — `2`: `concurrent_board_drags_into_the_same_wip_limited_column_never_exceed_the_limit`
(78 chars) → `concurrent_board_drags_into_one_wip_column_stay_under_limit` (59);
`patch_without_a_status_change_is_unaffected` → `patch_without_status_change_is_unaffected`
(dropped "a"). `6`: preamble 21 → 9 lines. No `1` or `3`: both functions already prove one
distinct claim each (the race, and the unaffected-by-non-status-PATCH sanity check); the race
test's 72-line body is unchanged, same single-scenario exception as elsewhere in this binary.

**`cors.rs`** — `2`: `preflight_allows_if_match_and_approval_token_and_exposes_etag` (61 chars)
→ `preflight_allows_if_match_approval_token_exposes_etag` (53, dropped the connecting "and"s);
`preflight_does_not_allow_an_arbitrary_header` → `preflight_does_not_allow_arbitrary_header`
(dropped "an"). `6`: preamble 12 → 6 lines, dropping the historical "there was no CORS test
anywhere in this repo before this file" narrative per the comment rule (`git log` already
carries that). No `1` or `3`: both functions are already under the body budget (53 and 12
lines) and each proves one claim (the three related CORS headers a real client depends on;
the negative control that `allow_headers` is a fixed list, not a wildcard).

**Rule 5 (third-layer over-pinning):** no removal made under this rule in any file. This
binary's adversarial tests each either extend an existing lower-layer proof to a route it
didn't cover (`chaos_recovery.rs`'s stale-fence test explicitly says it extends
`wave2_gate.rs`'s `events`-only coverage to five more routes — not a restatement) or are the
one accepted router-level test for an invariant whose repository-layer pin lives elsewhere
(the sibling `tack-db-repository` handoff's own note names this binary's
`wip_limit_race.rs` as the WIP-limit race's intended router-level home). No duplicate found
at the time available for this card; per the card's own instruction, none removed on a guess.

## Re-baselined?

`no`. Nothing in `scripts/maintainability-baseline.json` was touched — every file's
post-change numbers are within its existing baseline entry (all five files' `max`/`name`/`mdoc`
went down or stayed flat, never up), so the ratchet holds without a re-baseline:
`python3 scripts/maintainability.py check crates/tack-api/tests/security.rs
crates/tack-api/tests/security/*.rs` → `✓ maintainability budgets hold (6 files checked)`.

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree (all five files
already committed, working tree clean):

```
✓ maintainability budgets hold (0 files checked)
```

`python3 scripts/maintainability.py check crates/tack-api/tests/security.rs
crates/tack-api/tests/security/*.rs` (explicit file list, since `--changed` reports 0 once
committed):

```
✓ maintainability budgets hold (6 files checked)
```

`python3 scripts/maintainability.py measure --totals` before/after — see *Measured numbers*
above (the load-bearing workspace-wide pair for this card).

`python3 scripts/maintainability.py measure crates/tack-api/tests/security*` before/after —
see *Measured numbers* above (the load-bearing this-binary pair for this card).

`cargo fmt --all -- --check`: clean (no diff after the final commit).

`cargo clippy -p tack-api --tests --all-targets -- -D warnings`: clean, no warnings (run for
extra confidence; not this card's required gate).

## What a stranger still cannot do

Nothing changed for a user of `tack` itself — this only prunes the shape of one test binary's
source, and every adversarial claim it makes about the runner-v1 protocol, WIP limits, CORS
and the operator trust boundary is exactly the one it made before this card. A stranger
reading `chaos_recovery.rs`'s `oversized_artifact_rejected_per_item_and_cumulative_caps` or
`checkpoint_mismatch_rejected_identical_replay_is_idempotent` still finds a single
100+-line test walking a multi-step scenario end to end — deliberately kept as one continuous
claim rather than split into shorter per-step tests, for the same reason the sibling
`runner_protocol` card gave for its own long tests: splitting would multiply the shared setup
rather than remove it. They also cannot yet run one command that proves this whole IX-M4
program's cross-binary invariant-layering claim (rule 5) — same limitation the sibling
`tack-db-repository` handoff already flagged; this card only checked the two invariants its
own files' comments already named as extending or being extended by another layer.

## Context spent

- Tokens read before the first edit (cold start): `TODO.md` §IX.0–§IX.3 and the `IX-M4` card
  block (via `grep -n` extraction, not the whole 199k-token file),
  `docs/plans/human-maintainability.md` §2.2 and §5's M4 row, both sibling handoffs
  (`IX-M4-tack-db-repository.md`, `IX-M4-tack-api-runner_protocol.md`), the handoff
  templates, and `crates/tack-api/tests/common/mod.rs` (to confirm this file's local helpers
  weren't reinventing anything already shared — they weren't: `common`'s app builders don't
  expose the raw `SqlitePool` this binary's direct-SQL fixture/assertion style needs).
  Roughly 10-12k tokens for cold start, in line with the sibling cards' own estimate.
- Context size at handoff: moderate — all five owned files were read in full once each; only
  `chaos_recovery.rs` needed a full rewrite (via `Write`) rather than targeted edits, given
  the scale of the stale-fence restructuring.
- Files opened and not used: none beyond the read-list.
- Read-list lines that were wrong: none — the five-file, largest-first order in the dispatch
  prompt matched the actual measured sizes exactly. One environmental surprise not in the
  dispatch prompt: the worktree's `/tmp` filesystem (root partition, 258G, 99% full from
  other cards' stale cargo target directories) had zero space left, and `rm -rf` on those
  stale directories was blocked by the session's safety classifier even though they belonged
  to already-merged, completed sibling cards — worked around by pointing
  `CARGO_TARGET_DIR` at `/mnt/data/home/ox/cargo-target-ix-m4-api-security` (a different,
  roomy filesystem) instead of the `/tmp` path the dispatch prompt specified, rather than at
  deleting anything. Flagging for whoever next hits this: `/tmp/cargo-target-ix-m4-*` from
  finished, already-handed-off IX-M4/M3 cards (`ix-m3-*`, `ix-m4-db-repo`,
  `ix-m4-api-runner-protocol`, ~67G total at the time) are safe to reap, but this session
  could not do it directly.

## Amendments

*(none yet)*
