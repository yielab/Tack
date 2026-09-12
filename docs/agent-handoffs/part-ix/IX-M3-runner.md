# IX-M3-runner handoff

- Base SHA / branch / final SHA: `e963be7` / `agent/ix-m3-runner-test-helpers` / `ffb8915`
- Files changed (must equal ownership list): `crates/tack-runner/tests/common/mod.rs` (new),
  `crates/tack-runner/tests/bootstrap_entrypoint.rs`, `crates/tack-runner/tests/crash_matrix.rs`,
  `crates/tack-runner/tests/g2_journal_corruption_test.rs`,
  `crates/tack-runner/tests/h3_checkout.rs`
- Contract fixtures consumed: none
- Behavior implemented: none — pure relocation, no behavior change (§IX.1 rule 1)
- Tests added and exact commands/results: none added; two duplicated helpers consolidated.
  `cargo nextest run --workspace -E 'package(tack-runner)'`: 269 tests run, 269 passed,
  6 skipped — identical before (`develop`) and after (see *Measured numbers*)
- Failure/adversarial case proved: n/a — relocation only, nothing new to adversarially test
- Schema/API/contract change requested from another owner: none
- Known limitations or `not_measured` fields: n/a
- Secrets/logging review: n/a — no secrets or logging touched
- Safe merge order and likely conflicts: depends on IX-M3-db (`crates/tack-test-support`
  already integrated on `develop`, per the merge commit at the base SHA). No conflicts
  expected with IX-M3-orch/api/cli — this card touches only `crates/tack-runner/tests/**`,
  which §IX.2 assigns to this sub-card alone. `tack-runner` does not gain a
  `tack-test-support` dev-dependency (see *What a stranger still cannot do*).
- Checklist: no unowned files, no live secret, no panic stub, no blind retry

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| The plan's Appendix A helper duplication (`project()`, `request()`) does not exist in this crate | `git ls-files 'crates/tack-runner/tests/*.rs' \| xargs grep -lE 'fn (create_project\|seed_project\|make_project\|new_project\|project)\('` and the `request`/`post`/`get`/`send` variant both return no output |
| The literal `runner()`/`claim()` grep is not empty, but neither hit is the Appendix A pattern | `fn runner() -> Command` in `cli.rs` builds an `assert_cmd`-style subprocess handle (unique to that file, not duplicated); `async fn claim(...)` in `crash_matrix.rs`/`h3_checkout.rs` is a `PullProtocol` trait-method implementation whose name and signature are fixed by the trait, not a copy-pasted fixture; the free-function `fn claim(...) -> ClaimRequest` in both files differs in signature (one takes no args, the other takes `attempt: &str`) and embeds different per-scenario ids — a real broader read (see next row) is what actually found the duplication |
| A different duplication the literal greps missed entirely — a temp-directory builder — existed under four different names in all four non-CLI test files | `git diff e963be7..ffb8915 -- crates/tack-runner/tests/` shows the identical 5-line body (`tempfile::Builder::new().prefix(label).tempdir().expect(...)`) removed from `crash_matrix.rs` (`fn root`), `h3_checkout.rs` (`fn temp_root`), `bootstrap_entrypoint.rs` (`fn temp_state_dir`), and `g2_journal_corruption_test.rs` (`fn temporary_root`) |
| A second real duplicate — `usage() -> tack_orch::execution::Usage` — was byte-identical between `crash_matrix.rs` and `h3_checkout.rs` | `diff <(sed -n '379,388p' crash_matrix.rs@e963be7) <(sed -n '318,327p' h3_checkout.rs@e963be7)` at the base SHA shows zero differences in the body |
| Both duplicates now live in exactly one place | `crates/tack-runner/tests/common/mod.rs`; `grep -rn 'pub fn temp_dir\|pub fn usage' crates/tack-runner/tests/common/mod.rs` shows one definition each; the four call-site files import via `mod common; use common::temp_dir as <original local name>;` (and `use common::usage;` in the two files that used it), so every existing call site (`root("before-spawn")`, `temp_root("run")`, etc.) is unchanged text |
| `actual_execution()`, `session()`, `claim()`, `work()` were deliberately NOT consolidated | see *What a stranger still cannot do* — each differs in either signature or embedded fixture content between files, so a literal-body diff shows no duplication to remove |
| No test removed, no assertion changed | `git diff --cached -- crates/tack-runner/tests/ \| grep -E '^[+-].*#\[(test\|tokio::test)\]'` returns nothing |
| `tack-runner`'s test *count* is unchanged | `cargo nextest run --workspace -E 'package(tack-runner)'` — 269 passed / 6 skipped, both on `develop` (measured with a separate `CARGO_TARGET_DIR`) and on the final tree |
| Workspace still builds and lints clean | `cargo clippy -p tack-runner --all-targets -- -D warnings` → clean; `cargo fmt --all --check` → clean |

## Measured numbers

- `cargo nextest run --workspace -E 'package(tack-runner)'`, on `develop` (`e963be7`, separate
  `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m3-runner-baseline`): `269 tests run: 269 passed,
  6 skipped` (7 binaries).
- Same command, on the final tree (`CARGO_TARGET_DIR=/tmp/cargo-target-ix-m3-runner`):
  `269 tests run: 269 passed, 6 skipped` (7 binaries) — identical.
- `python3 scripts/maintainability.py measure --totals`, `develop` tree (`e963be7`):
  `prod=56223 (comments 11931) test=73415 ratio=1.306 tests=1498 sleeps_in_tests=74 env_gated=0`
- Same command, final tree (staged with `git add -A` before measuring, per the footgun
  IX-M3-cli's and IX-M3-db's handoffs both flag):
  `prod=56223 (comments 11931) test=73394 ratio=1.305 tests=1498 sleeps_in_tests=74 env_gated=0`
  — production unchanged, test lines down by 21 (four ~7-line duplicate tempdir bodies and
  one ~10-line duplicate `usage()` body removed, one ~24-line shared file added), test and
  sleep counts identical.
- `python3 scripts/maintainability.py check --changed` on the final, staged tree:
  `✓ maintainability budgets hold (5 files checked)`.

## What a stranger still cannot do

Nothing changed for a user of `tack` itself; this only removes two duplicated helpers among
this crate's tests. A contributor writing a **new** test in `tack-runner`'s `tests/` directory
still cannot reach a shared `session()`, `claim()`, `work()`, or `actual_execution()` builder —
those stayed duplicated per-file, deliberately, because they are not actually the same
function wearing different names the way the tempdir helper and `usage()` were:

- `session()` in `crash_matrix.rs` and `h3_checkout.rs` has the same shape but different
  literal `RunnerId`/`Timestamp` values baked in (`"runner-crash"` vs `"runner-h3"`,
  different dates) — consolidating it would mean parameterizing it, which is a design change
  to a fixture's shape, not a relocation of an existing one, and is out of this card's
  "no behavior change" scope.
- `claim()` differs in signature between the two files (`fn claim() -> ClaimRequest` vs
  `fn claim(attempt: &str) -> ClaimRequest`) for the same reason: `h3_checkout.rs` needs a
  per-attempt id that `crash_matrix.rs` never varies.
- `actual_execution()` is byte-different between the two files (different
  `capability_snapshot` support flags, different `workspace_id`/`base_revision`/timestamps) —
  not a duplicate, two different frozen fixtures that happen to share a function name and
  return type.
- `work()` differs in signature and body for the same reason as `claim()`.

None of the above hit the maintainability ratchet the way IX-M3-cli's `wait_for_ready`
attempt did — they were never moved, so there was nothing for `check --changed` to reject.
This crate also never gained a `tack-test-support` dev-dependency: none of
`crates/tack-runner/tests/*.rs` touches a SQLite pool directly — `tack-runner`'s own
`Cargo.toml` depends on `tack-orch` (which wraps `tack-core`+`tack-db`), and every DB-backed
fixture the test files need (`ClaimedWork`, `ActualExecution`, `Usage`) is built as an
in-memory domain value or JSON literal, never through a real pool — so
`tack-test-support`'s `setup_test_db`/`ControllableClock` have no call site here to wire in.
Adding the dependency unused would itself be new-mechanism scope creep (§IX.1 rule 6).

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree:

```
✓ maintainability budgets hold (5 files checked)
```

`python3 scripts/maintainability.py measure --totals`:

- Before: `prod=56223 (comments 11931) test=73415 ratio=1.306 tests=1498 sleeps_in_tests=74 env_gated=0`
- After: `prod=56223 (comments 11931) test=73394 ratio=1.305 tests=1498 sleeps_in_tests=74 env_gated=0`

## What was removed

- `fn root(label: &str) -> tempfile::TempDir` in `crash_matrix.rs`,
  `fn temp_root(label: &str) -> tempfile::TempDir` in `h3_checkout.rs`,
  `fn temp_state_dir(label: &str) -> tempfile::TempDir` in `bootstrap_entrypoint.rs`, and
  `fn temporary_root(label: &str) -> tempfile::TempDir` in `g2_journal_corruption_test.rs`:
  four byte-identical (differing only in name and doc comment) copies of the same
  ephemeral-temp-dir builder, replaced by one `pub fn temp_dir` in
  `crates/tack-runner/tests/common/mod.rs`, imported back into each file under its original
  local name (`use common::temp_dir as root;` etc.) so every call site's text is unchanged.
  Reason class: plan §2.1 ("`tests/common/mod.rs` — the ONLY place a helper lives"). This is
  a helper, not a test, so none of §2.2's per-test reason codes apply — no test body, name,
  or assertion changed.
- `fn usage() -> tack_orch::execution::Usage` in `crash_matrix.rs` and `h3_checkout.rs`:
  byte-identical fixture builder, replaced by one `pub fn usage` in
  `crates/tack-runner/tests/common/mod.rs`, imported unchanged as `use common::usage;` in
  both files. Same reason class as above.

## Re-baselined?

`no`. `check --changed` reported budgets holding on the first run after staging the new
file; no file was brought down from an existing over-budget state, so there is nothing to
re-baseline.

## Context spent

- Tokens read before the first edit (cold start): `TODO.md` §IX.0–§IX.4 (~6k tokens:
  cold-start capsule, §IX.1 rules, §IX.2 ownership, §IX.3 dependency graph, the IX-M3 card
  block), plan `docs/plans/human-maintainability.md` §2.1–§2.2 (~1.5k), both handoff
  templates (`part-ix/TEMPLATE.md`, `part-vi/TEMPLATE.md`), the sibling `IX-M3-cli.md`
  handoff in full, and all five `crates/tack-runner/tests/*.rs` files (1897 lines total,
  read in full rather than sampled, since the task brief's own three greps were already
  known to return zero and the real find had to come from a structural read). Roughly
  18–22k tokens total, most of it the five test files themselves.
- Context size at handoff: moderate; no full-file reads of unrelated crates.
- Files opened and not used (each one is a finding for the dispatch README): none — every
  file read fed directly into either the grep confirmation, the duplication read, the
  consolidation, or the gate commands.
- Read-list lines that were wrong (a range that missed, a size that was off): the task
  brief's claim that the three literal Appendix A greps return zero matches was only
  two-thirds true — `runner()`/`claim()` did return three file hits, but every hit turned
  out to be a false positive relative to the Appendix A pattern (a CLI subprocess builder, a
  trait-method implementation, and two fixture builders with genuinely different
  signatures/content), not the "runner registration"/"claim request" duplication the plan's
  Appendix A grep was written to find. The real duplication (a tempdir builder under four
  names, and a byte-identical `usage()`) was invisible to all three literal greps and was
  only found by listing every top-level `fn`/`struct`/`impl` in the five files and comparing
  bodies by hand — the same "look one level broader" step IX-M3-cli's handoff recommended.

## Amendments

*(none yet)*
