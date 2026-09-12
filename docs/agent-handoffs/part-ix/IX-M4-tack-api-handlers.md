# IX-M4-tack-api-handlers handoff

- Base SHA / branch / final SHA: `69fbdc7` / `agent/ix-m4-api-handlers` / `136f285`
- Files changed (must equal ownership list): `crates/tack-api/tests/handlers/crud.rs`,
  `production_router.rs`, `executions_runner_admin.rs`, `economics.rs`,
  `item_concurrency.rs`, `attempt_scoping.rs`, `attempt_lists.rs`,
  `operator_read_routes.rs`, `provisioning.rs`, `local_runner.rs` — exactly this card's
  ten files. `crates/tack-api/tests/handlers.rs` (the root file) was read but not touched
  (its 8-line preamble and module list were already within budget). No other file touched,
  confirmed by `git diff --stat 69fbdc7 HEAD --name-only`.
- Contract fixtures consumed: none (`docs/contracts/runner-v1/` not touched).
- Behavior implemented: none — pruning only. `crud.rs`'s one fixed wait is the sole
  §IX.1 rule 1 exception (replaced with a bounded, condition-free cooperative-yield loop,
  not a behavior change).
- Tests added and exact commands/results: none added net-new; several variant families
  merged into table-driven tests (net −16 tests: 122 → 106 run by default).
  `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-api-handlers cargo nextest run --workspace -E
  'binary(handlers)'` — `106 tests run: 106 passed, 0 skipped` (verified after every
  file's commit, and again at the end).
- Failure/adversarial case proved: n/a — pruning only, no new behavior.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: several bodies remain over the 60-line
  target (`production_router.rs`'s 237-line vertical-slice test,
  `executions_runner_admin.rs`'s 129-line enrollment lifecycle test,
  `operator_read_routes.rs`'s 104-line events test) — each is one coherent multi-step
  claim whose setup would only be duplicated by splitting; see *What was removed* for the
  per-file reasoning. None of these were worsened; several were already at or near their
  current size before this card.
- Secrets/logging review: n/a — test-only changes, no logging or secret-handling
  production code touched. `local_runner.rs`'s secret-echo assertion
  (`!body.to_string().contains("positive-control-marker-value")`) is unchanged in
  substance, only renamed.
- Safe merge order and likely conflicts: independent of the sibling `tack-api` `security`
  binary card (different file, different binary) and of every other IX-M4 sub-card (each
  owns one binary). No conflicts expected against `develop`.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| All behavior in the ten files is unchanged | `cargo nextest run --workspace -E 'binary(handlers)'` — 106/106 pass, same assertions per case as before (see *What was removed* for exact case-to-case mapping on every merge) |
| The one fixed wait in this binary is gone, replaced by a bounded poll | `grep -rn 'sleep(' crates/tack-api/tests/handlers.rs crates/tack-api/tests/handlers/*.rs` — no hits; `completing_github_item_pushes_issue_close` (`crud.rs`) re-run standalone and as part of the full binary, passes reliably |
| Every test name in this binary is ≤60 chars, no unresolved articles/narrative | `grep -oP '(?<=^async fn )\w+' crates/tack-api/tests/handlers/*.rs \| awk '{ if (length($0) > 60) print }'` — empty across all ten files |
| Every file's preamble is ≤10 lines | `measure`'s `mdoc` column: crud 5, production_router 9, executions_runner_admin 3, economics 10, item_concurrency 9, attempt_scoping 9, attempt_lists 9, operator_read_routes 6, provisioning 9, local_runner 9 (was 5/9/3/10/23/20/16/6/15/11 — only `item_concurrency.rs` and `attempt_scoping.rs` were actually over budget; the rest already held) |
| No production file touched | `git diff --stat 69fbdc7 HEAD -- crates/tack-api/src/` — empty |
| `cargo fmt --all -- --check` is clean | ran after the final edit, no output |
| `cargo clippy -p tack-api --tests --all-targets -- -D warnings` is clean | ran after every file's edit, no warnings |

## Measured numbers

`CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-api-handlers python3 scripts/maintainability.py measure crates/tack-api/tests/handlers*`

Before (measured at card start, base SHA `69fbdc7`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-api/tests/handlers/crud.rs                          0     0   0%   1837   49    33  108   78    5
crates/tack-api/tests/handlers/production_router.rs             0     0   0%   1071    7   100  237   78    9
crates/tack-api/tests/handlers/executions_runner_admin.rs       0     0   0%    884   16    40  121   84    3
crates/tack-api/tests/handlers/economics.rs                     0     0   0%    595   10    37   55   80   10
crates/tack-api/tests/handlers/item_concurrency.rs              0     0   0%    578   11    33   49   83   23
crates/tack-api/tests/handlers/attempt_scoping.rs               0     0   0%    569    4    36   56   59   20
crates/tack-api/tests/handlers/attempt_lists.rs                 0     0   0%    564   10    24   34   69   16
crates/tack-api/tests/handlers/operator_read_routes.rs          0     0   0%    459    6    41  104   70    6
crates/tack-api/tests/handlers/provisioning.rs                  0     0   0%    449    8    34   62   67   15
crates/tack-api/tests/handlers/local_runner.rs                  0     0   0%    290    4    44   73   71   11
crates/tack-api/tests/handlers.rs                               0     0   0%     32    0     0    0    0    8
totals: prod=0 (comments 0) test=7328 ratio=0.0 tests=125 sleeps_in_tests=1 env_gated=0
```

After (this handoff):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-api/tests/handlers/crud.rs                          0     0   0%   1294   40    28   79   58    5
crates/tack-api/tests/handlers/production_router.rs             0     0   0%   1022    7    93  237   56    9
crates/tack-api/tests/handlers/executions_runner_admin.rs       0     0   0%    884   16    40  121   59    3
crates/tack-api/tests/handlers/economics.rs                     0     0   0%    595   10    37   55   60   10
crates/tack-api/tests/handlers/item_concurrency.rs              0     0   0%    564   11    33   49   58    9
crates/tack-api/tests/handlers/attempt_scoping.rs               0     0   0%    558    4    36   56   59    9
crates/tack-api/tests/handlers/attempt_lists.rs                 0     0   0%    520    5    36   49   55    9
crates/tack-api/tests/handlers/operator_read_routes.rs          0     0   0%    459    6    41  104   55    6
crates/tack-api/tests/handlers/provisioning.rs                  0     0   0%    432    7    38   62   54    9
crates/tack-api/tests/handlers/local_runner.rs                  0     0   0%    281    3    57   73   54    9
crates/tack-api/tests/handlers.rs                               0     0   0%     32    0     0    0    0    8
totals: prod=0 (comments 0) test=6641 ratio=0.0 tests=109 sleeps_in_tests=0 env_gated=0
```

Net: 7328 → 6641 test lines (−687, −9.4%); 125 → 109 `#[tokio::test]` functions in the
`measure` count (109 includes 3 `#[cfg(feature = "embed-spa")]` tests never compiled by
default); `sleeps_in_tests` 1 → 0. Every file's `name` max ≤60 (was up to 84).

Nextest-run test count (default build, excludes the 3 `embed-spa`-gated tests both
before and after): **122 → 106** (`-16`), matching the `#t` delta exactly. Verified at
every commit: 113 after `crud.rs` (−9), 113 after `production_router.rs` (0 net — its one
merge combined two blocks inside a single pre-existing test, not two tests), 113 after
`executions_runner_admin.rs` (0, renames only), 113 after `economics.rs` (0, renames
only), 113 after `item_concurrency.rs` (0, renames+preamble only), 113 after
`attempt_scoping.rs` (0, preamble only), 108 after `attempt_lists.rs` (−5), 108 after
`operator_read_routes.rs` (0, renames only), 107 after `provisioning.rs` (−1), 106 after
`local_runner.rs` (−1).

`python3 scripts/maintainability.py measure --totals` (workspace-wide, this card's worktree
only — no other card's work landed in it):

- Before (scratch `git worktree add --detach` at base SHA `69fbdc7`):
  `prod=56223 (comments 11931) test=72827 ratio=1.295 tests=1497 sleeps_in_tests=71 env_gated=0`
- After: `prod=56223 (comments 11931) test=72140 ratio=1.283 tests=1481 sleeps_in_tests=70 env_gated=0`

Matches the file-level deltas exactly: test lines −687, tests −16, sleeps −1.

## What was removed

Per-file, with the §2.2 rule number for each change (`1: variants → rows`, `2: name`,
`3: body`, `6: preamble`, `8: fixed wait`):

**`crud.rs`** — `1`: three variant families merged into table-driven tests: the four
API-token-gate tests (`no_token_configured_allows_request`,
`correct_token_allows_request`, `wrong_token_rejected`, `missing_token_rejected`) →
`token_gate_by_config_and_header` (4 cases); the two restore-invalid-body tests
(`restore_invalid_bytes_returns_bad_request`,
`restore_rejects_non_sqlite_body_with_structured_error_envelope`) →
`restore_rejects_non_sqlite_body` (2 cases, both header-presence variants, both now
checking the structured envelope — a strengthening, not a loss); the six
custom-field-value-validation tests (`set_custom_field_value_correct_type_returns_ok`,
`_wrong_type_returns_422`, `set_custom_field_select_invalid_option_returns_422`,
`set_custom_field_value_passes_pattern_validation`, `_fails_pattern_validation`,
`set_custom_field_number_out_of_range_returns_422`) →
`custom_field_value_validated_by_type_and_rule` (6 cases). Every original field
definition, request value and expected status preserved exactly. `2`: `github_import_...`
and `github_import_redirect_...` renamed (see below) as part of the same edits.
`8`: `github_import_links_items_then_completion_pushes_close` (renamed
`completing_github_item_pushes_issue_close`)'s
`tokio::time::sleep(Duration::from_millis(50))` polling loop (waiting for the
fire-and-forget GitHub-close push to reach the mock server) replaced with a
2-second-wall-clock-deadline `tokio::task::yield_now()` loop — a bounded poll on real
elapsed time, not a fixed per-iteration delay, matching CLAUDE.md's rule. Also: the
**entire file's manual `Request::builder()`/`.oneshot()`/`to_bytes()`/`serde_json::from_slice()`
boilerplate (79 call sites) replaced with the existing `tests/common` helpers**
(`common::send`, `send_with_raw`, `send_str_strict`, `create_project`) that
`production_router.rs`/`operator_read_routes.rs` already used but this file, predating
them, never adopted — not a numbered §2.2 rule, but the single largest driver of this
file's 1837 → 1294 line reduction (this alone cut ~500 lines before any test was merged).
The redundant local `create_test_project` helper (a byte-for-byte duplicate of
`common::create_project` returning `String` instead of `Uuid`) was deleted; its two
callers switched to `common::create_project`. A `mount_single_issue` helper was extracted
for the two GitHub-mock-setup blocks shared by `github_import_source_untrusted_survives_export_reimport`
and `completing_github_item_pushes_issue_close`, cutting the latter from 104 to ~80 lines.
Names shortened (class `2`): `github_imported_item_source_is_untrusted_and_survives_export_import_round_trip`
(78) → `github_import_source_untrusted_survives_export_reimport` (56);
`github_import_redirect_does_not_leak_user_token_to_private_destination` (72) →
`github_import_redirect_never_leaks_user_token` (46);
`github_import_links_items_then_completion_pushes_close` (56, narrative "then") →
`completing_github_item_pushes_issue_close` (42).
**Explicitly considered and rejected for `1`:** `health_returns_ok` vs.
`health_response_contains_version_and_migration_count` — the first test's only assertion
is a strict subset of the second's, but neither is a named `<claim>_<variant>` pair, and
per the card's own instruction ("if unsure, leave it") this was left as two tests rather
than deleted without a rule to cite. The export tests (json/csv/yaml) were also
considered and left separate — each already under the 40-line target and each proves a
format-specific claim (object/array shape, CSV header, YAML round-trip through import)
that a shared table would only obscure.

**`production_router.rs`** — `1`: `runner_v1_body_limit_is_the_lesser_of_configured_and_protocol_ceiling`'s
two inline-scoped blocks ("configured limit below the ceiling", "configured limit above
the ceiling") merged into one loop over a 2-case table — this was always one
`#[tokio::test]` function, so the *test count* is unchanged; only the internal
duplication (setup, assertions, sanity check repeated twice) was removed. `2`: six of
seven names renamed for length:
`production_router_completes_the_mock_vertical_slice_and_survives_restart` (72) →
`mock_vertical_slice_completes_and_survives_restart` (51);
`x_tack_principal_from_an_external_client_is_stripped_and_overridden` (67) →
`x_tack_principal_from_client_is_stripped_and_overridden` (56);
`operator_and_runner_credentials_are_not_substitutable_on_the_production_router` (78) →
`operator_and_runner_credentials_are_not_substitutable` (54);
`every_execution_and_runner_v1_path_requires_authentication_live` (63) →
`every_execution_and_runner_v1_path_requires_auth_live` (54);
`openapi_document_enumerates_the_mounted_operator_and_runner_v1_routes` (69) →
`openapi_enumerates_mounted_operator_and_runner_v1_routes` (57);
`runner_v1_and_execution_routes_share_the_global_cors_policy` (59, had "the") →
`runner_v1_and_execution_routes_share_global_cors_policy` (56);
`runner_v1_body_limit_is_the_lesser_of_configured_and_protocol_ceiling` (69) →
`runner_v1_body_limit_is_lesser_of_configured_and_ceiling` (57).
**Not touched, and why:** `mock_vertical_slice_completes_and_survives_restart` stays at
237 lines — one continuous enroll→profile→enroll-runner→create→claim→accept→start→events→
complete→restart→replay→redaction-scan journey, the same shape the sibling
`runner_protocol` card's 270-line lifecycle test was left at for the identical reason:
splitting would replay the prior steps' setup to reach the point under test, multiplying
lines rather than reducing them.

**`executions_runner_admin.rs`** — `2` only: ten of thirteen names renamed for length
(the full list is in the commit); no structural change. **Not touched, and why:**
`enrollment_token_hash_only_revoke_or_redeem_blocks_reuse` (129 lines) covers one
continuous hash-only-issuance → redeem → re-redeem-rejected → second-enrollment →
revoke → redeem-after-revoke-rejected chain; the four `list_executions_item_ids_*` tests
each prove a genuinely different claim about the same query parameter (finds a
predating row, returns the latest row per item, takes precedence over `item_id`/`limit`,
rejects a malformed id) rather than variants of one claim, so none were merged.

**`economics.rs`** — `2` only: five of ten names renamed for length; no structural
change. `rework_rate_excludes_stale_attempts_from_the_denominator`
(`src/handlers/economics/tests.rs`, a different layer — pure aggregation math with no
HTTP) and this file's `summary_excludes_stale_attempts_from_the_rework_denominator`
(HTTP plumbing) are a `duplicate-tests` near-match left as-is: different layers per the
module's own preamble, same precedent the `runner_protocol` card set for this shape.

**`item_concurrency.rs`** — `6`: the 23-line preamble trimmed to 9 lines by dropping its
paragraph-length restatement of what the three sequential tests' own doc comments
already say in full (kept: what the module covers, the one-line pointer to "read each
sequential test's own doc comment"). `2`: seven of eleven names renamed for length, with
their cross-references in the two concurrent tests' doc comments (`stale_if_match_is_...`
mentioned by name three times) updated to match. **Not touched, and why:** no `1` or `3`
— every test here proves one distinct concurrency-boundary claim (this file's own
preamble already documents why the sequential tests, not the concurrent ones, are the
deterministic gate), and every body was already under this file's own baseline.

**`attempt_scoping.rs`** — `6` only: the 20-line preamble trimmed to 9 lines. All four
names were already ≤60 chars (max 59); no `1` — each test proves a genuinely distinct
route (`.../events` vs. `.../artifacts/{id}/content`), confirmed by re-reading both
bodies rather than assuming from the name-similarity `duplicate-tests` flags against
`attempt_lists.rs`.

**`attempt_lists.rs`** (the file with the real payoff) — `1`: the five `attempt_artifacts_*`
tests and their five structurally identical `attempt_decisions_*` counterparts merged
into five table-driven tests over a new `ResourceKind` enum (`Artifact`/`Decision`):
`attempt_artifacts_requires_operator_auth_and_leaks_nothing_without_it` +
`attempt_decisions_requires_operator_auth_and_leaks_nothing_without_it` →
`attempt_list_requires_auth_and_leaks_nothing_without_it`;
`attempt_artifacts_is_empty_before_any_manifest` +
`attempt_decisions_is_empty_before_any_decision_raised` →
`attempt_list_is_empty_before_any_row`;
`attempt_artifacts_are_returned_oldest_first` +
`attempt_decisions_are_returned_oldest_first` → `attempt_list_is_returned_oldest_first`;
`attempt_artifacts_unknown_attempt_number_is_404` +
`attempt_decisions_unknown_attempt_number_is_404` →
`attempt_list_unknown_attempt_number_is_404`;
`attempt_artifacts_from_a_different_execution_is_404` +
`attempt_decisions_from_a_different_execution_is_404` →
`attempt_list_from_a_different_execution_is_404` (10 tests → 5). Every original case's
seeded id, `created_at` ordering, cross-execution setup and expected status/body
preserved exactly — this was a genuine rule-1 family (identical shape, only the resource
noun, URL segment and response field name varied), unlike the same-named-looking pairs
left alone in `attempt_scoping.rs`. `2`: one merged name still landed at 64 chars
(`attempt_list_requires_operator_auth_and_leaks_nothing_without_it`) and was shortened
to `attempt_list_requires_auth_and_leaks_nothing_without_it` (55). `6`: preamble
16 → 9 lines.

**`operator_read_routes.rs`** — `2` only: two of six names renamed for length. **Not
touched, and why:** `events_reflect_a_real_batch_then_unknown_attempt_is_404` (104
lines) joins two assertions (a real event batch round-trips; a distinct attempt number
404s) sharing one expensive enroll/profile/execution/claim setup — splitting would
duplicate ~25 lines of setup for a claim that could reuse none of it, the same call this
Part's other cards made for similarly-shaped sequential tests.

**`provisioning.rs`** — `1`: `docket_400_rolls_back_the_project` +
`docket_409_already_exists_rolls_back_the_project` → `docket_error_rolls_back_the_project`
(2-case table: docket status code, response body, expected HTTP status, expected message
substring — all four values preserved per case exactly). `bad_status_map_rolls_back_...`
was considered for the same table and rejected: it mounts no `/pods` mock at all and
carries an extra "never reached docket" assertion no other case shares, a materially
different claim, not a variant. `2`: the two remaining over-60 names shortened
(`orch_link_write_failure_after_a_successful_pod_leaves_both_standing` → 55 chars,
`bad_status_map_rolls_back_the_project_without_ever_calling_docket` → 49 chars). `6`:
preamble 15 → 9 lines.

**`local_runner.rs`** — `1`: `routes_are_absent_on_a_non_loopback_bind_even_with_a_control_wired_in`
+ `routes_are_absent_on_a_loopback_bind_with_no_control_wired_in` →
`routes_are_absent_without_both_loopback_and_a_control` (2-case table: config + control
presence, both still asserting the identical genuine-404 claim; the original's
architectural comment about why this is axum's own fallback, not a 409/403 envelope,
kept and generalized to cover both cases). `2`: the two untouched tests renamed
(`a_loopback_bind_with_a_control_wired_in_mounts_the_routes_and_starts_it` → 55 chars,
`the_enable_preference_is_the_only_app_meta_row_a_secret_write_ever_adds` → 52 chars).
`6`: preamble 11 → 9 lines. A clippy `type_complexity` lint on the merged test's case
tuple was resolved with a local `type MaybeControl = Option<Arc<dyn LocalRunnerControl>>;`
alias, not a new mechanism.

**Rule 5 (third-layer over-pinning):** no removal made under this rule in any file.
`crud.rs`'s `github_import_redirect_does_not_leak_user_token_to_private_destination`
(now `github_import_redirect_never_leaks_user_token`) near-matches
`crates/tack-api/src/github_sync.rs`'s `redirect_does_not_leak_github_token_to_private_destination`
— a lower unit layer (drives the redirect-following function directly, no HTTP/router)
proving the same invariant at a different layer, not this file's copy to delete.
`economics.rs`'s rework-denominator overlap and `production_router.rs`'s body-limit
overlap with `runner_protocol.rs` are the same shape and were left for the same reason.
Several `both_routes_409_when_orch_disabled`-style near-matches between `economics.rs`
and files under `tests/orchestration/**` were checked and are each a *different* route's
orch-disabled gate (dispatch, sprint, approvals, agent_activity) sharing only the
generic 409 shape — genuinely distinct claims, and those files belong to a different
IX-M4 sub-card regardless. `production_router.rs`'s
`operator_and_runner_credentials_are_not_substitutable` near-matches
`runner_protocol.rs`'s `operator_and_runner_auth_do_not_substitute` — also a different
file, outside this card's ownership, flagged for the integrator rather than acted on.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check crates/tack-api/tests/handlers.rs
crates/tack-api/tests/handlers/*.rs` → `✓ maintainability budgets hold (11 files checked)`
on the final tree without exceeding any file's existing baseline entry further; every
file's post-change numbers are within its baseline ceiling (most well under it — several
files' worst-name/preamble numbers actually improved past what the baseline recorded, but
`scripts/maintainability-baseline.json` itself was not edited, since improving past a
recorded ceiling doesn't require lowering it and this card's own instruction reserves
re-baselining for cards that "deliberately brought files down" as their stated purpose).

## Budget check

`python3 scripts/maintainability.py check crates/tack-api/tests/handlers.rs crates/tack-api/tests/handlers/*.rs` (final tree; `--changed` alone reports 0 files since every change is already committed and the working tree is clean):

```
✓ maintainability budgets hold (11 files checked)
```

`python3 scripts/maintainability.py measure --totals` — before/after (see *Measured
numbers* above for the full derivation; this worktree branched from `69fbdc7` and holds
only this card's ten files, so this pair is clean, unlike a workspace shared with other
concurrent cards):

- Before: `prod=56223 (comments 11931) test=72827 ratio=1.295 tests=1497 sleeps_in_tests=71 env_gated=0`
- After: `prod=56223 (comments 11931) test=72140 ratio=1.283 tests=1481 sleeps_in_tests=70 env_gated=0`

`python3 scripts/maintainability.py measure crates/tack-api/tests/handlers*` before/after:
see *Measured numbers* above — this is the load-bearing before/after pair for this
sub-card.

`cargo fmt --all -- --check`: clean. `cargo clippy -p tack-api --tests --all-targets --
-D warnings`: clean (not part of this card's required gate, run anyway after every file).

## What a stranger still cannot do

A stranger arriving from outside this repository still cannot tell, from
`attempt_lists.rs`'s new `ResourceKind` enum alone, that the two routes it parameterizes
over (`.../artifacts` and `.../decisions`) are *not* the same pattern used by
`attempt_scoping.rs`'s near-identically-named tests one file over — that file's
`attempt_events_*`/`attempt_artifact_download_*` pairs look the same shape from
`duplicate-tests`' output but cover two entirely different routes and were correctly
left unmerged; telling the two situations apart required reading every body, not just
the names. They also cannot run one command that proves this Part's cross-binary
invariant-layering claim (rule 5) end to end — that still requires reading this binary
and its paired `src/`-level or sibling-binary tests by hand, which this card did only for
the pairs `duplicate-tests` actually flagged.

## Context spent

- Tokens read before the first edit (cold start): `TODO.md` §IX.0–§IX.3 and the `IX-M4`
  card block (via `grep -n` extraction, not the whole file),
  `docs/plans/human-maintainability.md` §2.2, the two sibling IX-M4 handoffs
  (`tack-db-repository`, `tack-api-runner_protocol`) in full, and
  `docs/agent-handoffs/part-ix/TEMPLATE.md` plus the `part-vi/TEMPLATE.md` it points to —
  roughly 15-18k tokens, higher than the sibling cards' because both full sibling
  handoffs were read (as instructed) rather than skimmed.
- Context size at handoff: large — all ten files were read in full before editing
  (`crud.rs` once, 1837 lines, in one pass), plus `tests/common/mod.rs` to confirm which
  helpers already existed before reusing them in `crud.rs`.
- Files opened and not used: none of significance; `docs/adr/0064-fixed-waits.txt` was
  regenerated once (per the card's instruction) and the regeneration reverted after
  confirming the diff was entirely unrelated drift from other, already-landed work in
  `tack-orch`/`tack-cli` — see the *Item 8* precedent in the `tack-db-repository` handoff,
  which this card repeated exactly.
- Read-list lines that were wrong: none — the ten-file, largest-first order in the
  dispatch prompt matched the actual measured sizes exactly, and the one flagged sleep
  (`crud.rs:1830`) was at the exact line named.
- One deviation worth flagging for whoever reads this next: `crud.rs`'s single largest
  line-count reduction (~500 of its ~540 total lines cut) came from replacing 79 manual
  `Request::builder()`/`.oneshot()` call sites with the `tests/common` helpers two sibling
  files in this same binary (`production_router.rs`, `operator_read_routes.rs`) already
  used — this wasn't in the dispatch prompt's rule list (it predates §2.2's numbering)
  but is squarely "reuse what tests/common already provides, don't reinvent" per
  `docs/plans/human-maintainability.md` §2.1, and was the single highest-leverage change
  in this card.

## Amendments

*(none yet)*
