# IX-M4-tack-cli-embedded_runner handoff

- Base SHA / branch / final SHA: `d6619ca` / `agent/ix-m4-cli-embedded_runner` / `812edf3`
- Files changed (must equal ownership list): `crates/tack-cli/tests/embedded_runner_live_secret.rs`,
  `crates/tack-cli/tests/embedded_runner_orphaned_credential.rs`,
  `crates/tack-cli/tests/embedded_runner_state_scoping.rs` — confirmed by
  `git diff --stat d6619ca..HEAD --name-only`. `crates/tack-cli/tests/common/mod.rs`
  was read and, mid-card, briefly edited to hold a shared `poll_until`/`wait_for_ready`
  helper before being reverted back to its exact original content once that design was
  rejected (see *What was removed*, rule 8, for why) — `git diff d6619ca --
  crates/tack-cli/tests/common/mod.rs` is empty, so it is not a changed file. No file
  under `crates/tack-cli/src/**` was touched
  (`git diff --stat d6619ca..HEAD -- crates/tack-cli/src/` empty).
- Contract fixtures consumed: none (`docs/contracts/runner-v1/` not touched).
- Behavior implemented: none — pruning only.
- Tests added and exact commands/results: none added; test count unchanged at 1 per
  file (3 total). `CARGO_TARGET_DIR=/tmp/cargo-target-ix-m4-cli-embedded_runner cargo
  nextest run --workspace -E 'binary(~embedded_runner)'` — `3 tests run: 3 passed, 0
  skipped`, verified after every file's commit and again on the final tree. Each of the
  three tests was additionally stress-run standalone, 20x each, no flakiness (60 runs
  total, 60/60 passed).
- Failure/adversarial case proved: n/a — pruning only, no new behavior; every claim
  (live-secret hot-reload dispatch, orphaned-credential recovery, per-`storage_dir`
  state scoping) is unchanged from before this card.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: `embedded_runner_live_secret.rs`'s one
  test remains over the 60-line hard cap at 84 lines (was 151) — deliberate, as a single
  coherent end-to-end claim, matching the `runner_protocol`/`api-security` siblings' own
  precedent for the same kind of test (see *What was removed*, rule 3). `docs/adr/
  0064-fixed-waits.txt` was not committed — regenerating it surfaced unrelated drift from
  concurrent work on other binaries (see *Re-baselined?* / *Budget check*).
- Secrets/logging review: n/a — test-only changes; no logging or secret-handling
  production code touched. `FAKE_KEY`/`FAKE_MODEL` constants and the `PRINCIPAL_HEADER`
  test-only header are unchanged from before this card.
- Safe merge order and likely conflicts: independent of every other IX-M4 sub-card
  (each owns one binary; `crates/tack-cli/tests/common/mod.rs` was never actually
  changed, so no conflict surface was opened there either). No conflicts expected
  against `develop` since only these three files were touched and no other IX-M4
  sub-card owns them.
- Checklist: no unowned files, no live secret, no panic stub, no blind retry.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| All behavior in the 3 files is unchanged | `cargo nextest run --workspace -E 'binary(~embedded_runner)'` — 3/3 pass, same assertions per test as before |
| No `sleep(` ≥200ms remains in any of the 3 files (ADR-0064 threshold) | `python3 scripts/list-fixed-waits.py \| grep embedded_runner` — no hits (was 3 hits in `embedded_runner_live_secret.rs` at 200/200/250ms before this card) |
| Every remaining `sleep(` sits behind a bounded, condition-checked poll, never a blind wait | `grep -n 'sleep(' crates/tack-cli/tests/embedded_runner_*.rs` — exactly one hit per file, each a 50ms interval inside that file's own private `poll_until(timeout, attempt)`, which re-checks `attempt()` and a deadline every iteration |
| Every test name in this binary is ≤60 chars | `measure`'s `name` column: 46, 50, 47 (was 69, 70, 70) |
| Every file's preamble is ≤10 lines | `measure`'s `mdoc` column: 10, 10, 10 (was 20, 16, 14) |
| Every test body is ≤60 lines except the one documented exception | `measure`'s `max` column: 84 (`embedded_runner_live_secret.rs`, was 151), 46 (`embedded_runner_orphaned_credential.rs`, was 63), 41 (`embedded_runner_state_scoping.rs`, was 55) |
| No production file touched | `git diff --stat d6619ca..HEAD -- crates/tack-cli/src/` — empty |
| No third-layer over-pinning introduced | `python3 scripts/maintainability.py duplicate-tests crates/tack-cli/tests/embedded_runner_*.rs` — `0 near-identical pairs across files` |
| `cargo fmt --all -- --check` is clean | ran after the final commit, no diff |
| `cargo clippy --workspace --all-targets -- -D warnings` is clean | ran after the final commit, no warnings |
| `scripts/check-comments.sh` and `scripts/check-test-hygiene.sh` are clean | both ran green after the final commit |

## Measured numbers

`python3 scripts/maintainability.py measure crates/tack-cli/tests/embedded_runner_live_secret.rs
crates/tack-cli/tests/embedded_runner_orphaned_credential.rs
crates/tack-cli/tests/embedded_runner_state_scoping.rs crates/tack-cli/tests/common/mod.rs`

Before (measured directly at base SHA `d6619ca` via a scratch `git worktree add
/tmp/ix-m4-base-check2 d6619ca` — `scripts/maintainability-baseline.json`'s own recorded
entries for these three files are 2, 2 and 5 lines higher respectively, i.e. stale
relative to this card's actual base commit; the numbers below match the card's own
dispatch prompt exactly and are what this card's deltas are measured against):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-cli/tests/embedded_runner_live_secret.rs            0     0   0%    527    1   151   151   69   20
crates/tack-cli/tests/embedded_runner_orphaned_credential.rs     0     0   0%    268    1    63    63   70   16
crates/tack-cli/tests/embedded_runner_state_scoping.rs           0     0   0%    233    1    55    55   70   14
crates/tack-cli/tests/common/mod.rs                              0     0   0%     10    0     0     0    0    1
totals (these 4 files): test=1038 tests=3 sleeps_in_tests=10
```

After (this handoff, final SHA `812edf3`):

```
file                                                         prod  cmnt  cm%   test   #t   avg  max name mdoc
crates/tack-cli/tests/embedded_runner_live_secret.rs            0     0   0%    582    1    84    84   46   10
crates/tack-cli/tests/embedded_runner_orphaned_credential.rs     0     0   0%    273    1    46    46   50   10
crates/tack-cli/tests/embedded_runner_state_scoping.rs           0     0   0%    207    1    41    41   47   10
crates/tack-cli/tests/common/mod.rs                              0     0   0%     10    0     0     0    0    1
totals (these 4 files): test=1072 tests=3 sleeps_in_tests=3
```

Net: 1028 → 1062 test lines across the three owned files (+34, +3.3% — the private
`poll_until` helper and its callers cost slightly more lines than the sleep calls they
replaced, in exchange for every wait becoming a real condition poll and every file's
preamble/name/body coming under budget). `sleeps_in_tests` for the three owned files:
10 → 3 (one canonical `sleep(` per file, was 4/3/3 scattered across each file's own
`wait_for_ready`/`wait_for_active_runner`/etc.). `common/mod.rs` unchanged at 10 lines,
0 sleeps — identical before and after, since this card's edits to it were fully reverted.

`python3 scripts/maintainability.py measure --totals` (workspace-wide; "before" captured
from the same scratch `git worktree add /tmp/ix-m4-base-check2 d6619ca` rather than
touching this branch, so this pair is clean of any other concurrent card's work):

- Before: `prod=56223 (comments 11931) test=71098 ratio=1.265 tests=1440 sleeps_in_tests=58 env_gated=0`
- After: `prod=56223 (comments 11931) test=71132 ratio=1.265 tests=1440 sleeps_in_tests=51 env_gated=0`

Deltas match the file-level deltas exactly: test lines +34 (1028 → 1062 across the three
owned files), tests unchanged at 1440, sleeps_in_tests −7 (10 → 3 across the three owned
files, i.e. workspace 58 → 51). Ratio unchanged at 1.265 (the plan's ratchet only fails on
a ratio that grows past baseline, and 1.265 ≤ 1.265 holds).

## What was removed

Per-file, with the §2.2 rule number for each change (`2: name`, `3: body`,
`6: preamble`, `8: fixed wait`).

**`embedded_runner_live_secret.rs`** (527 → 582 lines, 1 test unchanged) — `8`: the four
`sleep()` calls at (pre-card) lines 232 (100ms, `wait_for_ready`), 317 (200ms,
`wait_for_runner_advertising_the_model`), 338 (200ms, `wait_for_active_runner`) and 369
(250ms, `wait_for_terminal_attempt`) were already inside bounded, condition-checked poll
loops — not blind waits — but three of the four sat at or above the 200ms threshold
`docs/adr/0064-fixed-waits.txt` inventories. Consolidated all four into one private
`poll_until(timeout, attempt)` (50ms interval, well under the threshold) that each
`wait_for_*` function now calls; `test_sleeps` for this file dropped from 4 to 1 and it
no longer appears in the regenerated inventory at all. `2`:
`a_key_stored_while_the_runner_is_running_is_used_by_the_next_dispatch` (69 chars) →
`key_stored_while_running_reaches_next_dispatch` (46). `6`: preamble 20 → 10 lines
(condensed five short paragraphs into two, dropping restated detail already visible in
the fake-gateway/fake-claude doc comments themselves). `3`: body 151 → 84 lines via four
extractions — `set_up_live_secret_env` (tempdir, fake-claude shim, git fixture repo,
fake gateway: previously ~15 inline lines), `store_gateway_key` (the `PUT .../secrets/...`
call and its status assertion: ~9 lines), `seed_dispatch_target` (project/item/agent-profile
creation: ~20 lines) and `dispatch_via_gateway` (the execution POST payload: ~33 lines).
**Not split further, and why:** the test proves one claim end-to-end (a key pasted while
the runner is already serving reaches the very next dispatch, with no restart and no
"Re-check") and every remaining step — runner active before the key exists, key stored,
runner now advertises the model, dispatch succeeds through the gateway, the harness was
actually pointed at the gateway — is a link in that one causal chain; splitting it into
separate tests would mean re-deriving most of the same expensive setup (one real `tack
serve --with-runner` subprocess, a fake `claude`, a fake gateway) once per link for a
claim whose entire point is the chain holding together in one continuous run. This
matches the judgment call the card's own dispatch prompt invited and the precedent set by
`IX-M4-tack-api-security.md`'s 73–106-line adversarial-scenario tests and
`IX-M4-tack-api-runner_protocol.md`'s 270-line lifecycle test.

**`embedded_runner_orphaned_credential.rs`** (268 → 273 lines, 1 test unchanged) — `8`:
the three `sleep()` calls (100ms in `wait_for_ready`, 100ms in `wait_for_active_runner`,
50ms in `wait_for_session_containing`) were likewise already bounded polls, all already
under the 200ms ADR threshold — this file never appeared in `0064-fixed-waits.txt` before
this card either. Consolidated into the same private `poll_until` pattern; `test_sleeps`
3 → 1. `2`: `embedded_runner_recovers_a_credential_orphaned_by_a_recreated_database` (70
chars) → `orphaned_credential_recovers_on_recreated_database` (50). `6`: preamble 16 → 10
lines (condensed three paragraphs into two, keeping the single-subprocess-per-boot
rationale and dropping restated detail). `3`: body 63 → 46 lines — extracted
`assert_recovery_logged_once_with_no_identifiers` (the "exactly one recovery log line, and
it carries no path or id" check: ~20 inline lines down to a 5-line call).

**`embedded_runner_state_scoping.rs`** (233 → 207 lines, 1 test unchanged) — `8`: the
three `sleep()` calls (100ms in `wait_for_ready`, 100ms in `wait_for_active_runner`, 50ms
in `wait_for_session_file`) — same shape as the other two files, already bounded, already
under the ADR threshold. Consolidated into the same private `poll_until` pattern;
`test_sleeps` 3 → 1. `2`: `two_servers_on_two_databases_each_see_only_their_own_runner_enrollment`
(70 chars) → `each_server_sees_only_its_own_runner_enrollment` (47). `6`: preamble 14 → 10
lines. `3`: body already under the 60-line hard cap (55) at card start but well above the
40-line target; extracted `assert_server_enrolled` out of the two near-identical "server
enrolls, session lands under its own `storage_dir`" checks for server A and server B
(a genuine `1`-style variant pair — same assertion shape, only the server/root/label
differ), cutting the body to 41 lines. The 12-line `///` doc comment that previously sat
directly above the test (explaining why server A is checked and dropped before server B
starts) was condensed into the shorter doc comment now on the test itself; the reasoning
(a collision is a property of where the *first* server writes) is kept, the restated
production-code narrative (`EmbeddedRunnerControl::new`'s exact old/new derivation) was not,
since `git log`/the original card handoff for that fix already carry it.

### Why `poll_until` was made private per file, not shared via `tests/common/mod.rs`

`wait_for_ready` is byte-identical across all three files (differing only in timeout:
30s/15s/30s), and `wait_for_active_runner` is byte-identical between
`embedded_runner_orphaned_credential.rs` and `embedded_runner_state_scoping.rs` — exactly
the kind of duplication the plan's "helpers used more than once belong in
`tests/common/mod.rs`" rule targets. A first attempt moved `poll_until`, `wait_for_ready`
and `wait_for_active_runner` there. That worked for compilation and passed every gate
*except* `scripts/maintainability.py check --changed`, which flagged
`crates/tack-cli/tests/common/mod.rs: test_sleeps=1 (budget 0, baseline new file)` — the
file postdates `scripts/maintainability-baseline.json` entirely (created in
`b281c87 refactor(ix-m3-cli): consolidate test helpers`, after the baseline's 2026-09-11
measurement), so it has no baseline entry at all, and the check treats a missing entry as
a baseline of 0: any new `sleep(` there is scored as a fresh violation, not a ratchet-safe
decrease. `docs/agent-handoffs/part-ix/TEMPLATE.md` is explicit that "a baseline entry
that went up is a defect; say so and fix it before handing off" — and going from no entry
to `test_sleeps=1` reads as exactly that from the check's own point of view, even though
the *aggregate* count across these four files dropped from 10 to 3. Rather than editing
`scripts/maintainability-baseline.json` to paper over a fresh violation the check itself
calls a defect, this card reverted `common/mod.rs` to its original content and kept
`poll_until`/`wait_for_ready`/`wait_for_active_runner` private to each of the three owned
files instead. This does leave `wait_for_ready`'s ~20 lines duplicated three ways (and
`wait_for_active_runner`'s ~13 lines duplicated two ways) — a real, flagged-here tradeoff.
Whoever next owns `tests/common/mod.rs` (no open IX-M4 sub-card currently does) can move
them there in one step once `scripts/maintainability-baseline.json` is regenerated to
include it, which turns the same `test_sleeps=1` from a fresh violation into a recorded,
ratchet-safe starting point.

## Re-baselined?

`no`. `python3 scripts/maintainability.py check crates/tack-cli/tests/embedded_runner_live_secret.rs
crates/tack-cli/tests/embedded_runner_orphaned_credential.rs
crates/tack-cli/tests/embedded_runner_state_scoping.rs crates/tack-cli/tests/common/mod.rs`
→ `✓ maintainability budgets hold (4 files checked)` on the final tree, with
`scripts/maintainability-baseline.json` untouched — every file's post-change numbers are
at or under its existing baseline ceiling (`common/mod.rs` unchanged from its own
pre-card content, so it needed no baseline entry either). See the section above for the
one place a baseline gap (a file with no recorded entry) shaped this card's design instead
of being worked around by editing the baseline.

`docs/adr/0064-fixed-waits.txt` was regenerated (`python3 scripts/list-fixed-waits.py`)
and diffed against the committed copy: this card's three targeted lines
(`embedded_runner_live_secret.rs:317/338/369`, all ≥200ms) are gone as expected, but the
regenerated file also drops several entries this card never touched (`tack-orch/src/
reconciler.rs`, `tack-orch/src/execution_{observability,retention}.rs`,
`tack-api/tests/orchestration/reconciler/wiring.rs`, `tack-api/tests/wiring/
execution_sweep.rs`, `tack-api/src/orch_runtime.rs`, several `tack-api/tests/
orchestration/auto_dispatch/*.rs` lines) and adds one this card never touched either
(`tack-orch/tests/docket_live_test.rs:171`) — concurrent work on other IX-M4/other
branches landing on `develop` between the committed file's last regeneration and now, per
the card's own instruction to skip committing when regeneration shows drift beyond this
card's own removals. Not committed; whoever next regenerates it after this branch merges
will pick up both sets of changes cleanly.

## Budget check

`python3 scripts/maintainability.py check --changed` on the final tree (all 3 commits
landed, working tree clean, so nothing shows as "changed"):

```
✓ maintainability budgets hold (0 files checked)
```

`python3 scripts/maintainability.py check crates/tack-cli/tests/embedded_runner_live_secret.rs
crates/tack-cli/tests/embedded_runner_orphaned_credential.rs
crates/tack-cli/tests/embedded_runner_state_scoping.rs crates/tack-cli/tests/common/mod.rs`
(explicit file list, since `--changed` reports 0 once committed):

```
✓ maintainability budgets hold (4 files checked)
```

`python3 scripts/maintainability.py measure --totals` before/after — see *Measured
numbers* above for the full derivation (before: `prod=56223 (comments 11931) test=71098
ratio=1.265 tests=1440 sleeps_in_tests=58 env_gated=0`; after: `prod=56223 (comments
11931) test=71132 ratio=1.265 tests=1440 sleeps_in_tests=51 env_gated=0`).

`python3 scripts/maintainability.py measure crates/tack-cli/tests/embedded_runner_live_secret.rs
crates/tack-cli/tests/embedded_runner_orphaned_credential.rs
crates/tack-cli/tests/embedded_runner_state_scoping.rs crates/tack-cli/tests/common/mod.rs`
before/after: see *Measured numbers* above.

`cargo fmt --all -- --check`: clean. `cargo nextest run --workspace -E
'binary(~embedded_runner)'`: `3 tests run: 3 passed, 0 skipped` (plus 20x standalone
stress runs per test, 60/60 passed). `cargo clippy --workspace --all-targets -- -D
warnings`: clean (the card's required gate; also ran the narrower `cargo clippy -p
tack-cli --tests --all-targets -- -D warnings` after every individual file's edit).
`scripts/check-comments.sh` and `scripts/check-test-hygiene.sh`: both clean.
`python3 scripts/maintainability.py duplicate-tests crates/tack-cli/tests/embedded_runner_*.rs`:
`0 near-identical pairs across files`.

## What a stranger still cannot do

A stranger reading `embedded_runner_live_secret.rs`'s private `poll_until` cannot tell
from the code alone that an earlier version of this same helper lived in
`tests/common/mod.rs` and was deliberately moved back out — that reasoning (the
maintainability baseline's ratchet treats a shared file with no recorded entry as
baselined at zero, so adding the first `sleep(` there reads as a fresh violation
regardless of the aggregate improvement) lives only in this handoff and the commit
history, not in a code comment, since `CLAUDE.md`'s own comment rule keeps board/process
reasoning out of source. They also cannot yet see `wait_for_ready`/`wait_for_active_runner`
consolidated into one place across these three binaries — that duplication (documented
above) is still live in the tree, waiting on a baseline regeneration this card
deliberately did not perform.

## Context spent

- Tokens read before the first edit (cold start): the card's own dispatch prompt
  (self-contained — file list, line numbers, and acceptance criteria all given directly,
  no `TODO.md` extraction needed beyond confirming the IX-M4 card-list/acceptance section),
  `docs/plans/human-maintainability.md` §1–§2.3, `crates/tack-cli/tests/common/mod.rs` in
  full, and all three owned test files in full. Moderate cold start for a 3-file, ~1000-line
  binary set.
- Context size at handoff: moderate. `embedded_runner_live_secret.rs` was written twice in
  full (once with the shared-`common` design, once reverted to the private-helper design)
  after the baseline check surfaced the `common/mod.rs` problem; the other two files were
  each written once.
- Files opened and not used: none of significance — `docs/agent-handoffs/part-ix/
  IX-M4-tack-api-security.md` and `IX-M4-tack-orch-scheduling.md` were read for
  judgment-call precedent (single-test body sizing, handoff structure) and both directly
  informed this handoff's shape and the live-secret sizing decision.
- Read-list lines that were wrong: none — the card prompt's line numbers for every
  `sleep(` call matched the actual pre-card file content exactly, and the prompt's own
  framing ("read the surrounding test... these use `std::thread::sleep`, not tokio")
  correctly anticipated that every wait was already a bounded poll rather than a genuinely
  blind one.
- One finding worth flagging for whoever hits this next: **a shared test-crate fixture
  file that postdates `scripts/maintainability-baseline.json` has no baseline entry at
  all, and `check`'s ratchet treats a missing entry as a baseline of zero** — so the very
  first `sleep(`, over-length name, or over-budget preamble anyone ever adds to such a
  file reads as a fresh violation no matter how much it improves the files that depend on
  it. `crates/tack-cli/tests/common/mod.rs` (created in `b281c87`, after the baseline's
  2026-09-11 measurement) is one instance; there may be others crate-side. Re-running
  `scripts/maintainability.py baseline` once, after the current wave of IX-M4 sub-cards
  lands, would close this gap for every such file at once rather than one sub-card
  discovering it at a time.

## Amendments

*(none yet)*
