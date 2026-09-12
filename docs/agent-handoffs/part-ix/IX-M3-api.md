# IX-M3-api handoff

- Base SHA / branch / final SHA: `8fbf8ae` / `agent/ix-m3-api-test-helpers` / `85da102`
- Files changed (must equal ownership list): `crates/tack-api/Cargo.toml` (dev-dependency),
  `crates/tack-api/tests/common/mod.rs`, and every file under `crates/tack-api/tests/**` that
  defined a `project()`/`request()`/`claim()`-shaped helper, plus the three top-level test
  binaries (`wiring.rs`, `runner_protocol.rs`, `runner_vertical_slice.rs`) that did not yet
  declare `mod common;` — 34 files total (list: `git show --stat 85da102`). `Cargo.lock` picked
  up the one new `tack-test-support` edge.
- Contract fixtures consumed: none
- Behavior implemented: none — pure relocation, no behavior change (§IX.1 rule 1). Every HTTP
  request built by a moved helper is byte-identical to what the deleted per-file copy built;
  every response-parsing rule (byte cap, panic-vs-`Value::Null` on non-JSON) that differed
  between two files' same-named helpers was kept as a distinctly-named function rather than
  silently collapsed (see *What was removed*).
- Tests added and exact commands/results: none added; 29 duplicate helper definitions deleted,
  10 shared functions added to `tests/common/mod.rs`.
  `cargo nextest run --workspace -E 'package(tack-api)' --build-jobs 4`: `528 tests run: 528
  passed, 0 skipped` — identical before and after (measured both ways, see *Measured numbers*).
- Failure/adversarial case proved: n/a — relocation only, nothing new to adversarially test.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: n/a.
- Secrets/logging review: n/a — no secrets or logging touched. `chaos_recovery.rs`'s
  `auth()`/`bearer()`-style header builders stayed local (out of this card's three-family
  scope); nothing moved reads or logs a credential differently than before.
- Safe merge order and likely conflicts: lands after IX-M3-db (already merged) and independent
  of IX-M3-orch/IX-M3-runner/IX-M3-cli — this card touches only `crates/tack-api/**` and
  `crates/tack-test-support/Cargo.toml`'s reverse dependency edge in `Cargo.lock`. No
  conflicts expected with the three sibling M3 sub-cards; the only shared file is `Cargo.lock`,
  and the integrator should regenerate it once after merging all four rather than trusting any
  one branch's copy (per root `CLAUDE.md`'s generated-file rule).
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| No test file in `tack-api` defines its own `project()`/`request()`/`claim()`-shaped helper | `git ls-files 'crates/tack-api/tests/**/*.rs' \| xargs grep -lE 'fn (create_project\|seed_project\|make_project\|new_project\|project)\('` → only `tests/common/mod.rs` |
| Same for the `send()`-shaped ("request") family | `git ls-files 'crates/tack-api/tests/**/*.rs' \| xargs grep -lE 'fn (request\|json_request\|post\|get\|send)\('` → only `tests/common/mod.rs` |
| Same for the `claim()`-shaped family | `git ls-files 'crates/tack-api/tests/**/*.rs' \| xargs grep -lE 'fn (claim\|make_claim)\('` → empty (see note below) |
| `tack-test-support` is now a `tack-api` dev-dependency, used for the in-memory pool + migrations in `test_app_with_local_runner` | `crates/tack-api/Cargo.toml` `[dev-dependencies]`; `crates/tack-api/tests/common/mod.rs::test_app_with_local_runner` calls `tack_test_support::setup_test_db()` |
| `tack-api`'s test *count* is unchanged | `cargo nextest run --workspace -E 'package(tack-api)'` — 528 passed / 0 skipped, both before (stashed) and after |
| No behavior change: divergent same-named helpers kept as distinctly-named shared functions, never silently merged | `send`/`send_large`/`send_with_raw`/`send_post_strict`/`send_str_strict` in `tests/common/mod.rs`, each with a doc comment naming which byte cap or JSON-parse rule it preserves |
| Workspace still builds and lints clean | `cargo check -p tack-api --tests` and `cargo clippy -p tack-api --tests --all-targets -- -D warnings` → both clean, no warnings |
| `cargo fmt --all -- --check` is clean | ran after the final edit, no output |

Note on the claim grep: the acceptance criterion (`TODO.md` IX-M3) says the three greps should
return only `tests/common/mod.rs`. For `claim`, the empty result is because the two moved
helpers were given distinct, more accurate names — `claim_runner` (HTTP, from
`chaos_recovery.rs`) and `claim_execution_for_test` (repository layer, from
`repository_crash.rs`) — since they never shared any implementation (see *What was removed*).
Neither name contains the literal substring the grep alternation matches (`claim(` /
`make_claim(`), so `tests/common/mod.rs` doesn't appear in that grep's output either; the
underlying claim — no file outside `tests/common` defines its own claim-shaped helper — holds.

## Measured numbers

- `cargo nextest run --workspace -E 'package(tack-api)' --build-jobs 4`, untouched tree
  (stashed the card's changes to measure): `528 tests run: 528 passed, 0 skipped` (9 binaries).
- Same command, final tree: `528 tests run: 528 passed, 0 skipped` (9 binaries) — identical.
- `python3 scripts/maintainability.py measure --totals`, untouched tree:
  `prod=56223 (comments 11931) test=73734 ratio=1.311 tests=1498 sleeps_in_tests=74 env_gated=0`
- Same command, final tree: `prod=56223 (comments 11931) test=73420 ratio=1.306 tests=1498
  sleeps_in_tests=74 env_gated=0` — production unchanged (this card touches no `src/`), test
  lines down by 314, workspace ratio improved (1.311 → 1.306), total test count unchanged
  (1498, matching "no test removed").
- `python3 scripts/maintainability.py check --changed` on the final tree:
  `✓ maintainability budgets hold (32 files checked)`.
- `cargo fmt --all -- --check`: clean.
- `bash scripts/check-comments.sh` and `bash scripts/check-test-hygiene.sh`: both green
  (not part of this card's required gate list but run anyway since they're cheap and part of
  `pre-push`).

## What a stranger still cannot do

Nothing changed for a user of `tack` itself — this only moves where 29 test-only HTTP/DB
helpers live inside `tack-api`'s own test suite. A contributor adding a **new** test file to
`tack-api/tests/` still cannot see `tack-test-support`'s `ControllableClock` or fixture-loading
helpers without their own `use` (this card didn't need them — `tack-api`'s `AppState`/router
plumbing has no seam for a swappable clock the way `tack-orch`/`tack-runner` do); that wiring,
if ever needed, is a future card's job. The three sibling M3 sub-cards (orch/runner/cli) still
each roll their own project/request/claim-shaped helpers until their own cards land — this
card only had to prove `tack-api`'s own family of duplicates could disappear.

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree:

```
✓ maintainability budgets hold (32 files checked)
```

`python3 scripts/maintainability.py measure --totals`:

- Before: `prod=56223 (comments 11931) test=73734 ratio=1.311 tests=1498 sleeps_in_tests=74 env_gated=0`
- After: `prod=56223 (comments 11931) test=73420 ratio=1.306 tests=1498 sleeps_in_tests=74 env_gated=0`

Six files initially regressed past their own (already-over-budget) baseline after the
mechanical consolidation — all fixed before this commit, not re-baselined:

- `crud.rs` (`test_file_lines` 1856→1890, `test_fn_max_lines` 108→110): fixed by changing
  `pid`'s type from `String` back to `Uuid` (and `make_item`/`make_custom_field`'s
  `project_id: &str` parameter to `Uuid`) so `common::create_project(...).await` needed no
  `.to_string()` — the extra `.await.to_string()` chain was tipping ~27 call sites into a
  rustfmt 3-line wrap.
- `executions_runner_admin.rs` (`test_fn_max_lines` 121→123): two call sites' full line
  (`common::send_as_operator(...)`) crossed 100 columns where the original 4-character `send`
  did not. Fixed with a file-local `use crate::common::send_as_operator as snd;` and renamed
  all 35 call sites for consistency, rather than wrapping only the two that needed it.
- `runner_protocol/lifecycle.rs` (`test_fn_max_lines` 270→271): same shape, one call site.
  Since this file uses no other `common::` function, fixed by importing
  `use crate::common::send_str_strict as send;` — restoring the exact original identifier at
  every call site, which reproduces the original file's formatting byte-for-byte.
- `security/board_drag_wip_race.rs` (`test_fn_max_lines` 72→76) and
  `security/wip_limit_race.rs` (85→90): the comment explaining why `"software"`'s WIP-limited
  workflow was chosen had lived inside the now-deleted local `create_project()`; moved inline
  at the call site it added 4–5 lines to the *test* function's own budget. Fixed by folding it
  into the existing `///` doc comment above `#[tokio::test]` instead — outside the measured
  function body, same information, same call site.
- `wiring/artifact.rs` (`test_module_doc_lines` 40→42): the module preamble's claim that "this
  file imports zero test infrastructure from any other test file" became false once it started
  calling `common::send_large`; the first fix (an explanatory aside) added 2 lines. Fixed by
  narrowing the claim to what's still true — "this file builds its own app/router setup —
  never `common::test_app` or another file's harness" — restoring the original line count
  without an inaccurate claim.

## What was removed

Every removal below is class **"one place for a helper"** — plan §2.2 rule 5's spirit, applied
to test *helpers* rather than test *assertions* (the rule's literal subject is invariant
pinning across layers; the card's own instructions ask for this citation regardless). No test
was removed, no assertion changed, no test renamed.

`project()`-shaped (16 files → `common::create_project`):
`handlers/crud.rs` (`make_project`, returned `String`; behavior preserved via `Uuid::to_string()`
callers, later simplified — see *Budget check* — by changing the file's own `make_item`/
`make_custom_field` to take `Uuid`), `handlers/economics.rs` (`create_project`, already
`(name, project_type)`-parameterized — identical shape to the shared version),
`handlers/item_concurrency.rs`, `orchestration/auto_dispatch/gate.rs`,
`orchestration/auto_dispatch/hook.rs` (`project_type`-parameterized, name hardcoded),
`orchestration/auto_dispatch/sprint.rs`, `orchestration/control_plane/resource.rs`,
`orchestration/dispatch/dual_scheduling.rs`, `orchestration/dispatch/item.rs`
(`project_type`-parameterized), `orchestration/dispatch/pipeline.rs`
(`project_type`-parameterized, used its own 5-arg `req`/`body_json_val` internally — replaced
with `common::create_project`'s self-contained request), `orchestration/reporting/agent_activity.rs`,
`orchestration/reporting/approvals.rs`, `orchestration/reporting/budget_policy.rs`,
`orchestration/reporting/run_readback.rs` (raw `oneshot` call, no local `req`/`body_json`),
`security/board_drag_wip_race.rs` (explanatory comment about the WIP-limited workflow choice
moved to the test's own doc comment, not lost), `security/wip_limit_race.rs` (same).

`send()`-shaped (11 files → `common::send`/`send_large`/`send_with_raw`/`send_post_strict`/
`send_str_strict`/`send_as`/`send_as_operator`):
`handlers/attempt_lists.rs` and `handlers/attempt_scoping.rs` (identical 3-tuple-returning
`send`, 8MB cap → `send_with_raw`), `handlers/operator_read_routes.rs`,
`handlers/production_router.rs`, `orchestration/fleet_templates/fleet_membership.rs`,
`wiring/model.rs` (four byte-identical `send`s, 8MB cap → `send`), `security/chaos_recovery.rs`
and `wiring/artifact.rs` (identical `send`, 64MB cap → `send_large`),
`runner_protocol/decisions.rs` (`send` fixed to `POST`, panics on non-JSON → `send_post_strict`),
`runner_protocol/lifecycle.rs` (`send` takes a `String` body, panics on non-JSON →
`send_str_strict`), `handlers/executions_runner_admin.rs` (`send`/`send_as` pair, principal
header, 128KB cap, panics on non-JSON → `send_as`/`send_as_operator`).

`claim()`-shaped (2 files → `common::claim_runner` / `common::claim_execution_for_test`):
`security/chaos_recovery.rs`'s HTTP `claim` (POSTs `/api/runner/v1/claim`, built on its own
`send`) and `runner_vertical_slice/repository_crash.rs`'s repository-layer `claim` (calls
`Repository::claim_execution_idempotent_with_snapshot` directly against a local `Fixture`
struct — generalized to take `(&Repository, &dyn ExecutionClock)` instead of the file-local
`Fixture` type, since `tests/common/mod.rs` cannot reference a type private to one test
binary). These two never shared any implementation; moving both out was mechanical
compliance with the card's own acceptance grep, not consolidation of real duplication.

## Re-baselined?

`no`. `check --changed` reports budgets holding on the final, staged tree; every file that
regressed past its own baseline during the mechanical consolidation was fixed (see *Budget
check*) rather than re-baselined — a baseline entry going up is a defect, not something to
paper over.

## Context spent

- Tokens read before the first edit (cold start): `TODO.md` §IX.0–§IX.4 (~6k tokens: cold-start
  capsule, §IX.1 rules, §IX.2 ownership, §IX.3 dependency graph, the IX-M3 card block), plan
  `docs/plans/human-maintainability.md` §2.1–§2.2 (~1.5k), `docs/agent-handoffs/part-ix/IX-M3-db.md`
  and `crates/tack-test-support/src/lib.rs` (to reuse rather than re-invent), `docs/agent-handoffs/
  part-ix/TEMPLATE.md` and the `part-vi/TEMPLATE.md` it points to, the existing
  `crates/tack-api/tests/common/mod.rs`. Roughly 10k tokens for cold start; the bulk of this
  card's cost was reading each of the 29 files' helper definitions and call sites directly
  (`grep`/`sed`/targeted `Read`s), not additional docs.
- Context size at handoff: moderate-to-large — 29 files' full helper bodies were read to
  compare behavior before consolidating (necessary: several "same name" helpers turned out to
  differ in byte cap or JSON-parse strictness, and that could only be caught by reading each
  one, not by assuming the grep-matched name implied identical behavior).
- Files opened and not used: none of significance — `docs/plans/harness-maintainability-audit.md`
  was not opened (irrelevant to this card, per its own §IX.0 scoping note).
- Read-list lines that were wrong: the card's own measured counts (16/11/2) matched the
  repository exactly. One thing the card's brief didn't call out and that cost real time:
  the `send()`-shaped family is only 11 files by the literal grep because most `req()`-style
  helpers (17 files) are named `req`, not `request`/`post`/`get`/`send` — genuinely out of this
  card's scope, not an oversight. A future reader tempted to "finish the job" on `req()` should
  know that's a much larger, explicitly out-of-scope family (also matches this card's own
  `create_project` design decision — `common::create_project` builds its request directly
  rather than depending on any file's local `req()`, specifically so the 14 files that used
  `req()`/`body_json()` for many *other* calls didn't need those left untouched).

## Amendments

*(none yet)*
