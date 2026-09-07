# VI-C42 handoff

- Base SHA / branch / final SHA: `8db262b` / `agent/vi-c42-secret-store-bound` / (this
  card's commit, see `git log -1` on the branch)
- Files changed (must equal ownership list): `crates/tack-runner/src/secrets.rs`,
  `crates/tack-cli/src/doctor.rs`, `docs/CONFIG.md`
- Contract fixtures consumed: none — this card touches neither the wire contract nor any
  fixture under `docs/contracts/`.
- Behavior implemented: `SecretStore::open` no longer lets the platform credential store's
  construction call block indefinitely. A new private `SecretStore::bounded` helper runs
  the probe on a detached thread and gives it `PLATFORM_STORE_TIMEOUT` (3s, `pub const` in
  `secrets.rs`) to answer over an `mpsc` channel; a probe that misses the window is treated
  exactly like any other platform-store failure — `open` falls back to the file backend and
  logs `reason` as it already did. `platform_store()` is now a thin wrapper:
  `Self::bounded(PLATFORM_STORE_TIMEOUT, Self::platform_store_unbounded)`, where
  `platform_store_unbounded` is the renamed, unchanged, target-gated body that used to be
  `platform_store`. `SecretStore::open`'s signature is untouched.
- Tests added and exact commands/results:
  - `secrets::tests::bounded_times_out_promptly_when_the_work_never_answers` — a fake probe
    that sleeps 5s against a 50ms bound; asserts the call returns an `Err` in well under 1s.
  - `secrets::tests::bounded_returns_the_work_s_own_result_when_it_answers_in_time` — proves
    `bounded` is transparent to a fast success or a fast error, not just a timeout gate.
  - `cargo nextest run --workspace -E 'test(/bounded_(times_out|returns_the_work)/)'
    --build-jobs 4 --test-threads 4` → `2 tests run: 2 passed` in 0.057s (both tests, warm
    build).
  - `cargo nextest run --workspace -E 'package(tack-runner) | package(tack-cli)'
    --build-jobs 4 --test-threads 4` → `384 tests run: 384 passed, 6 skipped`.
  - Full suite once: `cargo nextest run --workspace --build-jobs 4 --test-threads 4` →
    `1466 tests run: 1466 passed, 7 skipped` in 26.728s.
  - `.githooks/pre-push` (with `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C42`) →
    `✓ pre-push checks passed` (comments check, test-hygiene check, `cargo fmt --all
    --check` for the workspace, clippy, generated-file freshness all green; one `cargo fmt`
    fixup was needed and applied before the green run).
- Failure/adversarial case proved: reverted `bounded` to call `work()` directly on the
  caller's own thread (the pre-fix shape — no thread, no channel, no timeout), then ran
  only `bounded_times_out_promptly_when_the_work_never_answers` with the same nextest
  invocation. Result: `FAIL [5.006s]` — the test now runs the fake probe's full 5-second
  sleep to completion and then fails its own assertion (`a probe that never answers must
  not be treated as success`), because with no bound the 5s sleep *is* the answer. Restored
  the real `bounded`, rebuilt, reran the same test: `2 tests run: 2 passed` in 0.057s. The
  ~5.006s vs ~0.057s gap is the bound; without it the test both takes 88× longer and fails
  outright.
- Schema/API/contract change requested from another owner: none.
- Known limitations or `not_measured` fields:
  - The bound is proved against a fake probe (`bounded` called directly with a sleeping
    closure), not against the real `zbus-secret-service-keyring-store` `Store::new()` D-Bus
    call — reproducing that hang on demand needs a Secret Service that is actually slow to
    activate, which this sandbox doesn't have a way to force. The mechanism `bounded` uses
    (spawn a thread, `recv_timeout` on a channel) does not depend on which blocking call
    runs inside it, so the fake-probe proof covers the real one by construction, but it is
    construction, not direct observation.
  - `PLATFORM_STORE_TIMEOUT` is a compile-time constant, not a `TACK_*` environment
    variable. Nothing in the card or in `docs/CONFIG.md`'s existing table asked for a
    runtime knob, and adding one with no caller asking for a non-default value would be
    exactly the "mechanism with no caller" pattern this tree's scope-discipline note warns
    against — left as a constant on purpose.
  - The abandoned probe thread is not cancelled when its timeout fires; if the real platform
    store eventually does answer (or panics) after the bound has already produced a file
    fallback, that answer is silently dropped (the channel's receiver is gone by then). This
    is inherent to bounding a blocking, uncancellable OS call from safe Rust, not a gap
    specific to this change — `Store::new()` exposes no cancellation hook to bound instead.
- Secrets/logging review: no new log line touches a secret value. The existing
  `tracing::warn!(backend = "file", reason = %reason, ...)` in `open` is unchanged in shape;
  `reason` was already a `String` built from `Display`-formatted backend errors (never a
  credential), and a timeout now produces a string in that same family
  (`"did not answer within {timeout:?}"` or `"probe thread ended without answering"`) —
  still just backend/timing information, never a secret or an env value. `doctor.rs`'s new
  line prints `PLATFORM_STORE_TIMEOUT` (a duration) and static prose only.
- Safe merge order and likely conflicts: no shared file with VI-C41 or VI-C43 (both
  confirmed against the wave's "share no file" claim before starting). `secrets.rs` is
  edited only by this card in the current wave; `doctor.rs`'s edit is additive (one new
  `println!` block) and should apply cleanly regardless of merge order with the other two
  Wave 18 cards.
- Checklist: no unowned files (only the three named above), no live secret (all tests use
  the file/mock backends or a fake closure — no live keychain write), no panic stub (the
  bound path returns `Result`, same as every other `platform_store` failure), no blind
  retry (a timed-out probe is not retried — `open` makes its choice once, as documented).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| `SecretStore::open` no longer blocks indefinitely if the platform credential store hangs (e.g. a Secret Service that stalls activating over D-Bus) | `secrets::tests::bounded_times_out_promptly_when_the_work_never_answers`; adversarial revert above shows the unbounded version instead running a fake 5s hang to completion |
| `tack runner doctor` names the bound and states the split-brain consequence in plain language | `tack runner doctor` output, `Secret store` section: `backend: keychain` / `note: the platform credential store gets 3s to answer before this falls back to an owner-only file; a store that is slow to start up (a Secret Service activating over D-Bus, for example) can miss that window on one boot and clear it on the next, so which backend answers for the same secret name is not guaranteed to stay the same across restarts.` |
| `SecretStore::open(&Path)`'s signature is unchanged | `crates/tack-runner/src/bootstrap.rs` and `crates/tack-runner/src/local_runner.rs` were not touched (grepped, not opened, per the read list) and the workspace + `tack-cli` builds and the full test suite pass unmodified |

## Measured numbers

- `cargo nextest run --workspace -E 'test(/bounded_(times_out|returns_the_work)/)' --build-jobs 4 --test-threads 4` → `2 tests run: 2 passed, 1471 skipped` in `0.056s`–`0.057s` (two runs).
- Same test, bound bypassed (temporary revert): `1 test run: 0 passed, 1 failed` in `5.006s`.
- `cargo nextest run --workspace -E 'package(tack-runner) | package(tack-cli)' --build-jobs 4 --test-threads 4` → `384 tests run: 384 passed, 6 skipped` in `6.688s`.
- `cargo nextest run --workspace --build-jobs 4 --test-threads 4` (full suite) → `1466 tests run: 1466 passed, 7 skipped` in `26.728s`.
- `PLATFORM_STORE_TIMEOUT` chosen: `Duration::from_secs(3)` — well under the "≥30s to even
  surface" figure `VI-C32.md`'s Known limitations section measured for the D-Bus-activation
  hang, and generous relative to this machine's own real answer: `tack runner doctor` on
  this box picks `backend: keychain` (a live Secret Service answered inside the 3s bound),
  so the bound cost nothing here in practice.

## What a stranger still cannot do

A stranger still cannot configure the bound — there is no environment variable or CLI flag
for it, only the `PLATFORM_STORE_TIMEOUT` constant in `secrets.rs`. Someone whose platform
store legitimately needs longer than 3s to answer on every boot (not just an occasional
slow one) will always land on the file backend, with no way to widen the window short of
editing the constant and rebuilding.

## Surface-map delta

None — this card is a runner-internal robustness fix (`secrets.rs`) plus a `doctor` output
line, not a UI-facing capability from §VI.0's surface map.

## Context spent

- Tokens read before the first edit (cold start): read the card's named files only —
  `README.md` header + Wave 18 block (~3k), `VI-C32.md`'s Known limitations section only
  (~0.6k), ADR 0061 Decision 1 (~1.1k), `secrets.rs` whole (~6k), `doctor.rs`'s targeted
  ranges (~1.5k), `CONFIG.md` grep hits (~0.3k), plus the two `pub fn new` greps into the
  keyring store crate source (~0.1k). Roughly 13k against the block's ≈12k estimate — close.
- Context size at handoff: comfortably under the wave's ceilings; no stop condition hit.
- Files opened and not used: none beyond the named read list; `bootstrap.rs` and
  `local_runner.rs` were confirmed as `open`'s only two callers via `grep -rn` only, never
  opened, as instructed.
- Read-list lines that were wrong: none — every named read was on-target and sufficient;
  no extra file had to be pulled in to finish the card.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
