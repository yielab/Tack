# IX-M4-tack-orch-docket_adapter handoff

- Base SHA / branch / final SHA: `7caee17` / `agent/ix-m4-orch-docket_adapter` / `20cd9f4`
  (two commits: `ec323c0` renamed 8 over-length test names; `20cd9f4` table-drove three
  error-mapping families and extracted their case tables into helper functions to keep
  bodies under the line cap after `cargo fmt`).
- Files changed (must equal ownership list): `crates/tack-orch/tests/docket_adapter_test.rs`
  only — confirmed by `git diff --stat 7caee17..HEAD --name-only`. No file under
  `crates/tack-orch/src/**` touched.
- Contract fixtures consumed: none new — the file's existing `tests/fixtures/*.json`/`.txt`
  fixtures are unchanged and still referenced identically.
- Behavior implemented: none — pruning only, no production code touched.
- Tests added and exact commands/results: none added; 7 removed by consolidation (42 → 35).
  `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-orch-docket_adapter cargo nextest run --workspace
  -E 'binary(docket_adapter_test)'` — `35 tests run: 35 passed, 0 skipped`.
- Failure/adversarial case proved: no new claim; every consolidated case proves the same
  status-code-to-`OrchError`-variant mapping the file already proved before this card, now
  as rows of a table instead of separate functions.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: none newly found.
- Secrets/logging review: n/a — test-only change, no logging or secret-handling production
  code touched. `TOKEN` fixture constant unchanged.
- Safe merge order and likely conflicts: independent of every other IX-M4 sub-card
  (single-file ownership). No conflicts expected against `develop`.
- **Critical carve-out — VIII-C3**: `dispatch_404_maps_to_not_found` (open Part VIII card
  VIII-C3's exclusive subject) is byte-identical to its pre-card state. Verified twice: once
  before the first commit and again after the final `cargo fmt` pass, via
  `awk '/^async fn dispatch_404_maps_to_not_found/,/^}/'` on both the `develop` copy and the
  final tree, diffed with zero output.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Note on how this card was finished

The background agent dispatched for this card (Sonnet, worktree-isolated) completed the 8
renames (commit `ec323c0`) and was mid-way through table-driving the three error-mapping
families when it was terminated by an account-level rate limit (HTTP 429, session reset
12:20pm America/Argentina/Buenos_Aires), with its last visible action being "replace this
whole test with a more compact tuple-based version." The orchestrating session inspected the
worktree directly rather than waiting for the reset: the tree held one clean commit plus an
uncommitted-but-compiling, fully-passing diff (35/35 tests green, carve-out test untouched).
`python3 scripts/maintainability.py check --changed` flagged one real gap the agent hadn't
reached yet — `dispatch_error_mapping_by_status` at 73 lines against the 60-line cap (a case
struct and 3-case array literal, plus their explanatory comments, defined inline inside the
`#[tokio::test]` fn) — which the orchestrating session fixed directly by extracting the case
struct and its builder into a free function (`dispatch_error_mapping_cases() -> Vec<...>`),
the same fix pattern documented by `IX-M4-tack-orch-scheduling`. A second, self-inflicted
instance of the same problem then appeared after running `cargo fmt`: rustfmt exploded a
7-argument-adjacent tuple-array literal in `provision_pod_error_mapping_by_status` across
many more lines, pushing it from 48 to 63 lines — fixed the same way, converting the tuple
array to a named `ProvisionPodErrorCase` struct built by its own free function
(`provision_pod_error_mapping_cases()`). Both fixes were applied, then re-measured,
re-formatted, and re-tested before commit; no further agent dispatch or resume was needed.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| All 35 tests pass and prove the same claims the 42 tests did before | `cargo nextest run --workspace -E 'binary(docket_adapter_test)'` — 35/35 pass |
| `dispatch_404_maps_to_not_found` (VIII-C3's test) is byte-identical | `awk` range-diff between `develop` and final tree — zero output |
| Every test name is ≤60 characters | max name length (measure's `name` column): 83 → 59 |
| Every test body is ≤60 lines (hard cap; target 40) | max body (`max` column): 37 → 60 (at the cap, not over it) |
| The file preamble is unchanged (already ≤10 lines) | `mdoc` column: 9 → 9, untouched |
| No third-layer over-pinning introduced | `python3 scripts/maintainability.py duplicate-tests` — no `docket_adapter_test` names reported |
| No `sleep(` anywhere (already true at card start) | `grep -n "sleep(" crates/tack-orch/tests/docket_adapter_test.rs` — no hits, before and after |
| `cargo fmt --all -- --check` is clean | ran after the final commit, no diff |
| `cargo clippy --workspace --all-targets -- -D warnings` is clean | ran after the final commit, no warnings |
| `python3 scripts/maintainability.py check --changed` passes | `✓ maintainability budgets hold (1 files checked)` |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-orch/tests/docket_adapter_test.rs`

Before (card start, base SHA `7caee17`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-orch/tests/docket_adapter_test.rs                   0     0   0%   1142   42    22   37   83    9
```

After (final SHA `20cd9f4`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-orch/tests/docket_adapter_test.rs                   0     0   0%   1099   35    23   60   59    9
```

Net: 43 fewer test lines, 7 fewer tests (42 → 35), `max name` moved 83 → 59, `max` (longest
test body) moved 37 → 60. The body-length increase is the table-driven consolidation itself:
three families of 3-4 near-identical functions became three functions that each loop over a
small case table, and each stays within (one lands exactly at) the hard cap after its case
table was factored out into its own free function.

## What was removed

- **`2: name`** — all 8 originally over-length names were resolved, either by direct rename
  or by disappearing into a consolidation:
  - `enqueue_task_waiting_approval_still_returns_ok_with_the_task_id` (63) →
    `enqueue_task_waiting_approval_still_returns_ok_and_task_id` (59) — rename only.
  - `malformed_prometheus_body_never_panics_and_returns_what_it_can` (62) →
    `malformed_prometheus_body_never_panics_returns_partial` (54) — rename only.
  - `decide_approval_grant_sends_channel_tack_and_returns_the_resulting_state` (72) →
    `decide_approval_grant_sends_channel_tack_and_returns_state` (59) — rename only.
  - `dispatch_bad_request_without_guardrail_wording_maps_to_http_not_policy_blocked` (78),
    `dispatch_bad_request_with_guardrail_wording_maps_to_policy_blocked` (66), and
    `dispatch_unauthorized_maps_to_auth_error` (not over-length itself, but the same claim
    family) — folded into `dispatch_error_mapping_by_status`, a 3-case table.
  - `decide_approval_409_maps_to_already_decided_with_dockets_message` (64),
    `decide_approval_404_maps_to_not_found_with_dockets_message`, and
    `decide_approval_unauthorized_maps_to_auth_error` — folded into
    `decide_approval_error_mapping_by_status`, a 3-case table.
  - `provision_pod_already_exists_maps_to_already_exists_not_a_generic_http_error` (76),
    `provision_pod_bad_blueprint_surfaces_dockets_message`,
    `provision_pod_operational_failure_after_dockets_own_rollback_surfaces_as_http_error`
    (83), and `provision_pod_unauthorized_maps_to_auth_error` — folded into
    `provision_pod_error_mapping_by_status`, a 4-case table.
- **`1: variants → rows`** — the three consolidations above (10 functions → 3), each over a
  distinct HTTP endpoint's status-code-to-`OrchError` mapping; genuinely parallel cases, not
  forced together across different endpoints.
- **`3: body`** — after consolidation, `dispatch_error_mapping_by_status` and
  `provision_pod_error_mapping_by_status` each had their case-struct definition and case
  array extracted into a private free function (`dispatch_error_mapping_cases()`,
  `provision_pod_error_mapping_cases()`) purely to keep the `#[tokio::test]` fn itself
  (signature to closing brace, per `scripts/maintainability.py`'s measurement) under the
  60-line cap; `decide_approval_error_mapping_by_status` needed no such extraction (59 lines
  inline).
- No `5` (third-layer over-pinning), `6` (preamble), or `8` (fixed wait) applied — none were
  present in this file before or after.
- Left untouched deliberately: `dispatch_404_maps_to_not_found` (VIII-C3 carve-out) — not
  folded into `dispatch_error_mapping_by_status` even though it is itself a 404-status case,
  because doing so would move or restate a test an open Part VIII card owns exclusively
  pending its own re-measurement and decision.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check --changed` passes on the final tree without
touching `scripts/maintainability-baseline.json`. The file's `max` body (60) sits exactly at
the hard cap, not over it, so no ratchet exception was needed.

## Budget check

```
✓ maintainability budgets hold (1 files checked)
```

`cargo fmt --all -- --check`: clean. `cargo nextest run --workspace -E
'binary(docket_adapter_test)'`: `35 tests run: 35 passed, 0 skipped`. `cargo clippy
--workspace --all-targets -- -D warnings`: clean. `python3 scripts/maintainability.py
duplicate-tests`: no `docket_adapter_test` names flagged.

## What a stranger still cannot do

Unchanged from before this card: `dispatch_404_maps_to_not_found`'s own scope and the
finding it documents (or doesn't yet — VIII-C3's live-server re-measurement is still open)
are exactly as they were; this card did not touch that question. Structurally, a stranger
reading the three new `*_error_mapping_by_status` tests learns the same status-code-mapping
claims as before, now as a table plus a loop, with the per-case assertion closures carrying
the "why this status maps to this variant" reasoning that used to live in each function's own
name and body.

## Context spent

- This card was originally dispatched to a background Sonnet agent, which completed the
  rename half (commit `ec323c0`) before being cut off mid-edit by an account rate limit. The
  orchestrating session inspected the worktree directly (`git status`, a build, a full test
  run, and `maintainability.py check --changed`) rather than waiting for the stated reset
  time, found the tree in a compiling, fully-passing, only-partially-finished state, and
  completed the remaining work (finishing the in-progress `dispatch_error_mapping_by_status`
  consolidation, then applying the same case-table-extraction fix a second time after
  `cargo fmt` inflated `provision_pod_error_mapping_by_status` past the cap) directly rather
  than resuming the agent.
- Files opened and not used: none.
- Read-list lines that were wrong: n/a.
- One finding worth flagging: `cargo fmt` re-exploding a tuple/array literal across many more
  lines than its single-line form, discovered independently here after already being
  documented by `IX-M4-tack-orch-scheduling`, confirms this is a recurring cost of the
  tuple-literal table-driving style specifically — a named-struct-plus-builder-function style
  (used for both consolidations in this file) sidesteps it entirely, and should be the
  default choice for any future table-driven test in this workspace rather than a plain tuple
  array.

## Amendments

*(none yet)*
