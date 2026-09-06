# VI-C15 handoff

**Base note: this worktree started on the wrong branch.** `git log --oneline -1` at dispatch
showed `worktree-agent-a7c803532b635f90d` at `e5206c7` — an ancestor of, but 40 commits
behind, `develop`'s real tip. Recreated explicitly: `git checkout -b
agent/vi-c15-runner-state-dir develop` (local `develop` resolves to `13cdce5`, the base this
card names), confirmed `git merge-base --is-ancestor 13cdce5 HEAD` succeeds, and the tree was
clean before the first edit.

**Mechanism, confirmed by reading before changing anything.** `EmbeddedRunnerControl::new`
(`crates/tack-cli/src/local_runner.rs`) called `load_runner_config(ConfigOverrides::default(),
None)` — no `state_dir` override at any tier. `RunnerConfig::defaults()`
(`crates/tack-runner/src/config.rs`) seeds `state_dir` to the literal `.tack-runner`, and
`RunnerConfig::environment_overrides()` only replaces it when `TACK_RUNNER_STATE_DIR` is set.
Nothing in that chain ever reads `AppConfig.storage_dir` or `AppConfig.database_url`. Two
servers sharing a working directory — the exact shape of `tack serve --with-runner` run twice
from one shell, or `frontend/`'s E2E suite reusing one cwd across a recreated `e2e.db` (VI-C13's
escalation) — therefore resolve to the identical `.tack-runner`, regardless of how different
their databases are. The two paths are not linked by any code that scopes them together; this
is a real, unconditional gap, not a false alarm.

**The fix:** `EmbeddedRunnerControl::new` now derives its default `state_dir` from
`server_config.storage_dir` (`<storage_dir>/runner` — one level under it, mirroring how
`execution-artifacts` already nests under `storage_dir` instead of colliding with attachments)
whenever `TACK_RUNNER_STATE_DIR` is unset; when it *is* set, it still wins, exactly as it does
for the standalone `tack-runner` binary. `storage_dir` was the right anchor, not
`database_url`'s own path, for two reasons: it is already the established per-install scoping
mechanism this crate uses for every other artifact (attachments, `execution-artifacts`,
`remote_backup`'s snapshot walk), and it is already varied *together* with `TACK_DATABASE_URL`
everywhere in this tree that stands up a second install on purpose (`frontend/playwright.config.ts`
sets both `TACK_DATABASE_URL=sqlite:e2e.db` and `TACK_STORAGE_DIR=./storage-e2e` as a pair, never
one without the other). `crates/tack-desktop/src/paths.rs` (VII-B3) independently reached the
same anchor for its own per-user-data-root case — it sets `TACK_RUNNER_STATE_DIR` explicitly
alongside `TACK_STORAGE_DIR` for the sidecar it supervises, so this default and that one never
conflict; the desktop path simply never falls through to `EmbeddedRunnerControl::new`'s own
default because it always supplies the override.

## The migration decision — the argument, not just the mechanism

**Picked: a one-time, best-effort move, reported at `info`/`warn` level with no path or
credential in the log line, never a silent permanent fallback and never an unconditional
refusal to start.** The three options the card named, and why the other two lose:

- **Silent migration with no log line at all** was rejected because a security-relevant
  relocation of a live credential that never says anything, ever, is the wrong default even
  when the mechanics are safe — an operator watching logs during an upgrade should be able to
  tell *something* moved, without the log line being useful to anyone who doesn't already hold
  the machine.
- **An unconditional refusal** (treat any legacy directory as a hard stop) was rejected because
  it breaks the single most common case — one operator, one install, no second database ever
  in play — for a problem that case does not have. Refusing correctly requires distinguishing
  "this legacy state belongs to me" from "this legacy state belongs to a different install
  sharing my cwd," and the code has no way to tell those apart other than by acting: the same
  distinguishing signal (does the moved credential redeem against *this* database) is available
  whether the code moves the directory and finds out, or refuses and makes the operator find out
  by hand. Silently refusing to ever try is strictly more friction for the same information.
- **The move, once, only when safe** — never touching either directory once the new one already
  exists (so a previous boot's migration or a fresh self-provision is never clobbered), and
  falling through to provisioning a fresh identity rather than reusing anything when the rename
  itself fails — satisfies the card's own worry directly: "silently reuse whatever we find" is
  exactly what this does *not* do. A failed rename leaves the legacy directory exactly as it
  was and the new server gets a *correctly scoped, fresh* credential instead of a foreign one;
  nothing is ever reused without the filesystem itself proving the move succeeded intact.

**The one case this does not resolve perfectly, and why leaving it is still correct:** two (or
more) installs that already, today, share both a cwd *and* a legacy `.tack-runner` — the literal
pre-existing bug, live on some machine right now — migrate in the order whichever install
happens to boot first after the upgrade. That install inherits the shared legacy credential;
every other one finds the legacy directory already gone (moved by the winner) and correctly
self-provisions a fresh identity in its own database. This is not a regression: those installs
were *already* indistinguishably sharing one file under the old code, so "whichever starts
first keeps it, everyone else gets a fresh one" is a real improvement (every process now ends up
correctly scoped) over the status quo (every process indistinguishably fighting over one file),
not a new failure mode. Recorded here rather than solved because solving it would mean
guessing, from the credential alone, which of several installs it was originally enrolled
against — information nothing in `session.json` carries.

- Base SHA / branch / final SHA: base `develop` at `13cdce5` (recreated, see above), branch
  `agent/vi-c15-runner-state-dir`, not committed (card rule: no commit/push/merge/rebase).
- Files changed (matches ownership — `EmbeddedRunnerControl::new` and
  `load_runner_config`'s state-directory argument, its tests, this handoff):
  - `crates/tack-cli/src/local_runner.rs` — added `embedded_default_state_dir` and
    `migrate_legacy_state_dir`; `EmbeddedRunnerControl::new` now derives its default
    `state_dir` from `storage_dir` unless `TACK_RUNNER_STATE_DIR` is set; four new unit tests
    for the two helpers.
  - `crates/tack-cli/tests/embedded_runner_state_scoping.rs` (new) — the two-subprocess
    acceptance test.
  - `docs/CONFIG.md` — rewrote the embedded runner's "State directory" bullet to describe the
    new default, the still-honored override, the standalone binary's unchanged default, and
    the one-time migration.
  - `docs/agent-handoffs/part-vi/VI-C15.md` — this handoff.
- Contract fixtures consumed: none — no runner-v1 wire shape changed.
- Behavior implemented: the embedded runner's default state directory now follows
  `TACK_STORAGE_DIR` instead of the process's bare working directory, with `TACK_RUNNER_STATE_DIR`
  preserved as an explicit override and a one-time migration for state already on disk under
  the old default. No API, schema, or wire-contract change.
- Tests added and exact commands/results:
  - `cargo nextest run --workspace -E 'package(tack-cli)'` → `109 tests run: 109 passed, 0 skipped`
    (includes the four new pure-logic unit tests and the new integration test).
  - `cargo nextest run --workspace -E 'test(two_servers_on_two_databases)'` → `1 test run: 1
    passed` — run three consecutive times with no failures (`2.77s`, `3.08s`, `2.82s`, `3.50s`
    across four total runs including the one folded into the full-suite run above).
  - `cargo nextest run --workspace` (full suite) → `1436 tests run: 1436 passed, 7 skipped`.
- Failure/adversarial case proved: reverted `EmbeddedRunnerControl::new` to its original
  `load_runner_config(ConfigOverrides::default(), None)` call (no `storage_dir` derivation),
  rebuilt, and re-ran `cargo nextest run --workspace -E
  'test(two_servers_on_two_databases)'` alone. It failed exactly where the derivation is
  supposed to prevent it from failing:
  ```
  thread 'two_servers_on_two_databases_each_see_only_their_own_runner_enrollment' panicked at
  crates/tack-cli/tests/embedded_runner_state_scoping.rs:166:5:
  server A's enrolled session must live under its own storage_dir, not the shared cwd
  test result: FAILED. 0 passed; 1 failed
  ```
  Re-applied the fix, rebuilt, and the same command passed again (see above) — the two
  now-unused helper functions also produced `dead_code` warnings while reverted, an independent
  signal that they are the only thing wiring the new behavior in.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields: the multi-install-shared-legacy-directory
  attribution case described above (first booter after upgrade keeps the shared credential;
  every other install correctly self-provisions fresh) is a known, accepted limitation, not a
  gap left unmeasured. A cross-filesystem rename failure during migration is exercised only by
  the unit tests' successful-rename and already-exists paths, not by a real cross-filesystem
  failure (not reproducible without a second mounted filesystem in this sandbox) — the failure
  branch's behavior (log a generic warning, leave the legacy directory alone, fall through to a
  fresh identity at the new path) is read, not independently measured under a real EXDEV.
- Secrets/logging review: `migrate_legacy_state_dir`'s two log lines name neither directory
  path nor any credential value; `%error` on the `warn!` lines is `std::io::Error`'s own
  `Display`, which is the OS's strerror text plus an error number, never a path (confirmed by
  reading `std::fs::rename`'s and `std::fs::create_dir_all`'s error construction — neither
  attaches the path to the returned `io::Error`). No new secret, credential, or path reaches a
  log line anywhere in this change.
- Safe merge order and likely conflicts: `local_runner.rs` was touched by VI-C13's
  investigation (which flagged this exact bug) but not edited by it — VI-C13's own diff is
  scoped to `frontend/e2e/*.spec.ts`. No other in-flight VI-C card owns this file. Independent
  of every other card; safe to merge in any order relative to them.
- Checklist: no unowned files touched (`git status --porcelain` before the handoff shows only
  the four files above), no live secret, no panic stub, no blind retry (the migration's failure
  branch reports and falls through rather than retrying the rename).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| Two `tack serve --with-runner` processes sharing a working directory, pointed at two different databases, each enroll their own runner rather than sharing/colliding on one credential | `cargo nextest run --workspace -E 'test(two_servers_on_two_databases)'` → 1 passed, run four times total across this handoff with zero failures |
| The fix is what makes that true, not incidental | Same test, same binary, reverted `EmbeddedRunnerControl::new` only → fails at `crates/tack-cli/tests/embedded_runner_state_scoping.rs:166:5` ("server A's enrolled session must live under its own storage_dir, not the shared cwd"); re-applied → passes again |
| `TACK_RUNNER_STATE_DIR` still overrides the new default | `embedded_default_state_dir`/the `new()` branch is only reached when `std::env::var_os("TACK_RUNNER_STATE_DIR").is_none()` — read directly in the diff; no test drives this specific branch with the env var set (see "What is left") |
| A pre-existing legacy directory is moved, once, without clobbering an already-provisioned new one | `migrate_legacy_state_dir_moves_an_existing_legacy_directory_once`, `migrate_legacy_state_dir_never_touches_an_already_provisioned_new_directory`, `migrate_legacy_state_dir_is_a_no_op_when_neither_directory_exists` — `cargo nextest run --workspace -E 'package(tack-cli)'` → all pass |
| No credential or path reaches a log line from this change | Source read: neither `tracing::info!`/`tracing::warn!` call in `migrate_legacy_state_dir` interpolates a path or the session contents; `%error` is `std::io::Error`'s `Display`, which carries no path |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

- `cargo nextest run --workspace -E 'package(tack-cli)'` → `109 tests run: 109 passed, 0 skipped`
  (`~11s`).
- `cargo nextest run --workspace` → `1436 tests run: 1436 passed, 7 skipped` (`~25s`).
- `cargo nextest run --workspace -E 'test(two_servers_on_two_databases)'` → `1 test run: 1
  passed`, individually timed at `2.77s`, `2.82s`, `3.08s`, `3.50s` across four separate
  invocations in this handoff (three explicit repeats plus the one inside the full-suite run).
- `cargo clippy --workspace --all-targets -- -D warnings` → clean, no output beyond the
  pre-existing `proc-macro-error2` future-incompatibility notice (unrelated to this change,
  present before it).
- `./scripts/check-comments.sh` → `✓ no board archaeology in crates/`.
- `./scripts/check-test-hygiene.sh` → `✓ tests take their temporary paths from a guard`.

## What a stranger still cannot do

Nothing new is blocked — this card only changes where state already written by the embedded
runner lives, and fixes a correctness gap a stranger would have hit invisibly (a second
`tack serve --with-runner` against a second database silently failing to enroll, or worse,
inheriting a foreign credential) rather than a capability a stranger was missing before. A
stranger running `tack serve --with-runner` for the first time sees identical behavior to
before this card; a stranger running it a *second* time against a *different* database now
gets a working, correctly scoped runner instead of the pre-existing silent failure.

## Surface-map delta

None. This card is a correctness fix inside the runner's own state management, not a step on
§VI.0's console-to-UI surface map — no row moved, and none needed to.

## Context spent

- Tokens read before the first edit (cold start): card + capsule extracts, `local_runner.rs`
  (full file, ~810 lines pre-change), `tack-runner/src/config.rs` (full file), `tack-api/src/config.rs`
  (relevant ~260 lines), `tack-desktop/src/paths.rs` (full file, found independently while
  checking whether VII-B3 had already solved the same coupling — it had, for its own,
  different case), `docs/CONFIG.md`'s embedded-runner section, `docs/agent-handoffs/part-vi/VI-C13.md`
  in full (the escalation this card starts from), and `crates/tack-cli/tests/e6_scheduler_e2e_test.rs`
  (found while designing the acceptance test, to reuse this crate's own established
  real-subprocess pattern rather than inventing a second one).
- Context size at handoff: not separately measured against a fixed budget for this card.
- Files opened and not used in the final diff: an initial acceptance-test draft lived inside
  `local_runner.rs`'s own `#[cfg(test)] mod tests` as an in-process two-server test; it hit a
  real, pre-existing constraint (`tack_api::server::serve_inner` installs a process-global
  `tracing` subscriber once per process, so a second in-process `serve_with_ready_and_local_runner`
  call in the same test panics on `.init()`'s second call) and was replaced with the
  two-subprocess integration test described above rather than left half-working — the discarded
  attempt is not present in the final diff, only described in the code comment that explains
  why the acceptance test lives where it does.
- Read-list lines that were wrong: none supplied for this card beyond the card text and
  cold-start capsule; both matched what the code actually does.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
