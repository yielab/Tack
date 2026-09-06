# VI-C21 handoff

- Base SHA / branch / final SHA: worktree dispatched at `f06ce56` on branch
  `agent/vi-c21-stale-credential`, which is `develop`'s own tip — no rebase or merge
  needed. Final SHA is HEAD of this branch after the single commit that includes this file
  (not hardcoded here — the hash is only fixed once this handoff is itself part of the
  tree being hashed).
- Files changed (broader than the card's literal ownership line — see note below):
  - `crates/tack-cli/src/local_runner.rs` — the fix: `ensure_runner_credential` now asks
    whether the stored session is orphaned before reusing it.
  - `crates/tack-cli/src/local_enrollment.rs` — `stored_session_orphaned`, the check
    itself, plus two unit tests for its own "nothing to check" branches.
  - `crates/tack-cli/tests/embedded_runner_orphaned_credential.rs` (new) — the real
    two-subprocess-boot acceptance proof.
  - `crates/tack-runner/src/transport.rs` — `persisted_session_runner_id`, reading a
    session's runner id off disk without constructing (or trusting) a full session.
  - `crates/tack-runner/src/client.rs` — re-exports the above from the crate's public
    surface, since `tack-cli` cannot reach `tack_runner::transport` directly.
  - `crates/tack-api/src/handlers/runner_admin.rs` — `local_runner_id_exists`, the
    authoritative "does this database have a row for this id" query.
  - `crates/tack-api/tests/handlers/executions_runner_admin.rs` — one test proving that
    query reports both presence and absence against a real, migrated, file-backed
    database.

  I did not shrink this to just `local_runner.rs` and its tests. The card's own acceptance
  criterion #4 ("stop and report ... if distinguishing 'this credential is for another
  database' from 'the database is temporarily unreachable' requires a guess") makes the
  check itself load-bearing, and answering it correctly needs two things `local_runner.rs`
  cannot do on its own: reading the runner id out of a persisted session without pulling in
  a full session/refresh cycle (`tack-runner`'s job — nothing in `tack-cli` parses
  `session.json`), and querying the database directly with the same authority
  `provision_local_runner` already has (`tack-api`'s job — `tack-cli` never opens the
  database). Both additions are narrow, single-purpose functions with their own tests, not
  a redesign of either crate.
- Contract fixtures consumed: none. No runner-v1 wire shape changed — `refresh` still
  either succeeds or is refused exactly as before; this card only changes what happens
  *before* that call, in-process, using data the server side of the protocol was never
  involved in.
- Behavior implemented:
  1. **The problem, precisely.** `ensure_runner_credential` used to treat "a session file
     exists on disk" as sufficient reason to hand `establish_session` a placeholder and
     stop — it never asked whether the *database this process just opened* still had a row
     for the runner id that session names. Deleting `frontend/e2e.db` without also deleting
     `storage-e2e` is exactly this: `storage-e2e/runner/session.json` still names a real
     runner id, but the recreated database has never heard of it. `establish_session`'s own
     `refresh` call would eventually discover this and fall back to enrollment — but only
     if it had a real credential to enroll with, and the placeholder path never gave it
     one, so the runner stalled forever short of `active`.
  2. **The fix separates "orphaned" from "unreachable" using the one fact available that
     settles it.** `stored_session_orphaned` (`local_enrollment.rs`) is only ever called
     from inside `ensure_runner_credential`, which runs after this same process's own
     `serve_inner` has already opened `database_url` successfully (`ensure_runner_credential`'s
     own doc comment already says it only runs after `ensure_loopback` has passed, which is
     after the server started). By the time this check runs, "the database is momentarily
     unreachable" is no longer a live case to confuse the answer with — the caller's own
     server already reached it. So `local_runner_id_exists` opens its own short-lived pool
     against that exact `database_url` (mirroring `provision_local_runner`, not inventing a
     new access pattern) and asks a direct row-existence question: `false` means *this*
     database has no such row, not "I could not tell." This is why the card's stop-and-report
     condition was never triggered — the transient-outage case is structurally excluded
     before the check ever runs, not guessed around.
  3. **The "cannot even identify a runner id" case falls back safely, not silently.**
     `persisted_session_runner_id` (`transport.rs`) returns `None` for a missing or
     unparseable session, and `stored_session_orphaned` reports `false` in that case — not
     because the session is known-good, but because there is nothing positive to pin an
     "orphaned" verdict on. The caller's existing behavior for that file (reuse the
     placeholder, let `establish_session`'s own `refresh`/fall-back-to-enrollment path
     handle whatever is actually wrong with it) is unchanged. This card only *adds* a new,
     narrow, provably-correct reason to skip the placeholder — it never removes the
     existing safety net under it.
  4. **The orphaned file itself is left alone.** `ensure_runner_credential` does not delete
     or rewrite `session.json` when it detects an orphan; it simply does not hand it to the
     runner as a credential this boot. `establish_session` (`transport.rs`) still loads it,
     still calls `refresh` first (unchanged control flow), gets refused by the same
     database for the same reason, and falls through to redeeming the fresh credential this
     fix just provisioned via `self_provision` — the same fallback path that already
     existed for a refused session, now reached with a real token on the first attempt
     instead of a placeholder that could never have worked. `store_session` then overwrites
     `session.json` in place with the new identity once redemption succeeds — proved by the
     acceptance test's `session_before != session_after` assertion.
  5. **The log line.** One `tracing::info!` in `ensure_runner_credential`, fired exactly
     once per boot that detects an orphan, fixed text with no interpolated field — no path,
     no old id, no new id (the new id does not exist yet at the point this line is logged),
     no credential. Acceptance #1's stricter-than-usual bar ("no path, no id, and no
     credential") is met by construction, not by redaction: there is nothing in the format
     string to redact.
- Tests added and exact commands/results (all run with
  `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C21`):
  - `stored_session_orphaned_is_false_with_nothing_on_disk_to_check` and
    `stored_session_orphaned_is_false_for_an_unparseable_session`
    (`local_enrollment.rs`) — the two branches that settle without ever reaching a
    database, proved against a real (unreachable) `database_url` string so a bug that made
    either branch try to open it would fail loudly instead of coincidentally passing.
  - `persisted_session_runner_id_reads_the_id_without_a_full_session` and
    `persisted_session_runner_id_is_none_for_a_missing_or_unparseable_session`
    (`transport.rs`).
  - `local_runner_id_exists_reports_presence_and_absence_against_a_real_database`
    (`executions_runner_admin.rs`) — presence proved by reading back an id
    `provision_local_runner` just wrote; absence proved against a well-formed id that was
    never written, against the same live, migrated, file-backed database — not an
    inference from a failed query.
  - `embedded_runner_recovers_a_credential_orphaned_by_a_recreated_database`
    (`crates/tack-cli/tests/embedded_runner_orphaned_credential.rs`, new file) — the real
    acceptance proof: one `tack serve --with-runner` subprocess boots against a fresh
    `storage_dir`/database pair, reaches `active`, and its `session.json` is read. The
    database file (plus `-wal`/`-shm`) is deleted and recreated empty; a second subprocess
    boots against the *same* `storage_dir`. Asserts: the second boot reaches `active`
    (bounded 30s poll — this is the exact failure mode the card describes, a stall, so an
    unbounded wait would hang forever instead of failing); the recovered runner id differs
    from the original; `session.json`'s contents differ before vs. after; exactly one log
    line contains `"no longer using"`; and that line contains none of the database path,
    the storage dir path, the old runner id, or the new runner id.
  - Command: `cargo nextest run --workspace --test-threads=4 --build-jobs 4` →
    `1449 tests run: 1449 passed, 7 skipped` (the 7 are the `#[ignore]`d live-harness
    tests, unaffected by this card, not run here).
  - `cargo clippy --workspace --all-targets --build-jobs 4 -- -D warnings` → clean, no
    warnings (only the pre-existing `proc-macro-error2` future-incompat notice, unrelated
    to this change).
  - `./scripts/check-comments.sh` → clean.
  - `./scripts/check-test-hygiene.sh` → clean.

  Note on `cargo nextest`'s own `-j`: this repository's docs describe `-j 4` as the
  compilation-job flag alongside `--test-threads=4`, but this installed nextest
  (`cargo-nextest 0.9.143`) treats `-j`/`--build-jobs` and `--test-threads` as the *same*
  concept renamed (`-j, --test-threads <N>` in its own `--help`), and rejects passing both.
  Used `--test-threads=4 --build-jobs 4` (the two actual, distinct flags this version
  exposes) throughout instead — same load-cap intent, flags that this nextest build
  actually accepts.
- Failure/adversarial case proved: reverted only the fix in `local_runner.rs` (the
  `if crate::local_enrollment::stored_session_orphaned(...)` branch added to
  `ensure_runner_credential`, restored via `git stash push -- crates/tack-cli/src/local_runner.rs`),
  keeping every other file — including the new acceptance test — exactly as this card
  leaves them, then re-ran only the acceptance test:

  ```
  CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C21 cargo nextest run --workspace \
    --test-threads=4 --build-jobs 4 -E 'binary(embedded_runner_orphaned_credential)'
  ```

  Result: `1 test run: 0 passed, 1 failed` after 30.75s (the test's own bounded poll,
  spent entirely waiting because the second boot's runner never reaches `active` without
  the fix), with:

  ```
  thread 'embedded_runner_recovers_a_credential_orphaned_by_a_recreated_database' panicked
  at crates/tack-cli/tests/embedded_runner_orphaned_credential.rs:229:73:
  a credential orphaned by a recreated database must not stall the embedded runner; it
  must recover under a fresh identity and still reach `active`
  ```

  and a `dead_code` warning on the now-unused `stored_session_orphaned`, confirming the
  revert touched exactly the intended call site and nothing else. Restored with
  `git stash pop`; re-ran the same command — `1 test run: 1 passed, 0 skipped` (~1s) — then
  the full workspace suite again to confirm the fix, not the revert, is what ships:
  `1449 tests run: 1449 passed, 7 skipped`.
- Schema/API/contract change requested from another owner: none. `local_runner_id_exists`
  is a new function, not a new HTTP route — nothing added to `docs/openapi.json` or
  `frontend/src/shared/api/schema.gen.ts`, and neither was touched.
- Known limitations or `not_measured` fields:
  - The abandoned journal (acceptance #3): the runner's attempt journal
    (`crates/tack-runner/src/journal.rs`, `OwnerOnlyJournal`) is keyed by `state_dir` and
    `attempt_id` only — its file path (`<state_dir>/journal/<hex(attempt_id)>.toml`) never
    includes a runner id, even though each `AttemptJournal` record carries the runner id
    that created it as a data field. Since this card's fix changes only which credential
    gets used, never `state_dir` itself, the journal directory is not orphaned in the
    sense of becoming unreachable dead storage — the freshly-provisioned identity reads
    the *same* `<state_dir>/journal/` on its very next boot (`HttpRunnerClient::serve`
    calls `engine.recover(&session)` immediately after `establish_session`, before
    claiming any new work). Any record left `unresolved` from before the database was
    replaced gets replayed under the new identity via `observe_recovery`. But the attempt
    (and the runner row) that record refers to no longer exists server-side — the whole
    database was recreated — so that specific HTTP call will not find a match
    (`engine.rs`'s `report_recovery_and_apply_disposition` treats any `Err` or
    `attempt_id`/`recovery_key` mismatch from `observe_recovery` identically: it returns
    `RunCycle::RecoveryPending` and leaves the journal record exactly as it was, still in
    the unresolved scan — it does not error out of `recover()`, and it does not quarantine
    or clean up the record). The practical effect: a journal record for an attempt that
    predates the database swap becomes a permanent `RecoveryPending` that gets retried,
    harmlessly, on every subsequent boot, forever — never blocking new work, never
    corrupting anything, but never resolved either. This is a pre-existing property of the
    journal/recovery design (it was never keyed by runner id, and `observe_recovery`
    failure was never distinguished from "try again later" vs. "this attempt is gone for
    good"), not something this card's fix introduces or could fix within its own scope —
    the card owns the boot-time credential check, not the recovery engine's disposition
    logic. Flagging it here rather than silently treating "the journal survives" as
    equivalent to "the journal resolves cleanly."
  - No new secret, database column, or config knob was introduced by this card.
- Secrets/logging review: the new `tracing::info!` line in `local_runner.rs` is a fixed
  string with no interpolated fields at all — verified by reading the call site directly,
  not by inference from a redaction rule. `local_runner_id_exists` and
  `stored_session_orphaned` return only a `bool`; neither logs anything. No new database
  column was added, so `remote_backup.rs::scrub_snapshot_secrets` needs no update —
  confirmed with `grep -n "local_runner_id_exists\|stored_session_orphaned" crates/tack-api/src/remote_backup.rs`
  → no matches.
- Safe merge order and likely conflicts: touches `crates/tack-api/src/handlers/runner_admin.rs`
  (an addition at the end of the file, after `provision_local_runner`, before the
  `#[utoipa::path]`-annotated revoke handler) and its sibling test file — both append-only
  changes, low conflict risk with other in-flight VI-C cards that were not described as
  touching `runner_admin.rs`. `local_runner.rs`'s change is scoped to
  `ensure_runner_credential`'s body and its own doc comment; any other card also editing
  that function's neighborhood should diff carefully, but nothing else in this card's
  reading suggested a specific collision.
- Checklist: no unowned files touched — every file above traces directly to implementing
  or testing the orphaned-credential check; no live secret used or printed (test session
  files and runner ids are ordinary test fixtures); no panic stub; no blind retry (the
  fresh-credential path is the existing `self_provision` call, reached the same way a
  refused session already reached it before this card).

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A `session.json` naming a runner id absent from the database no longer stalls the embedded runner; it reaches `active` under a new identity | `embedded_runner_recovers_a_credential_orphaned_by_a_recreated_database`, full run above |
| The recovery is reported exactly once, at `info`, with no path/id/credential in the line | Same test: asserts exactly one line containing `"no longer using"`, and that the line contains neither database path, storage dir, old runner id, nor new runner id |
| The orphaned session file is overwritten with the new identity's own credential, not left stale | Same test: `session_before != session_after` |
| The check never confuses "wrong database" with "database temporarily unreachable" | `local_runner_id_exists`/`stored_session_orphaned` only run after this process's own server already opened `database_url` successfully — the transient-outage case is structurally excluded before the check runs, not distinguished by a guess (see "Behavior implemented" #2) |
| The fix is load-bearing, not incidental | Reverted only the `local_runner.rs` call site; the acceptance test failed with the exact panic text quoted in "Failure/adversarial case proved"; restored; full suite green again |
| No secret, id, or path is logged even under acceptance's stricter bar | The log line is a fixed string with zero interpolated fields — read directly from source, not inferred |

A row with no evidence is a claim to delete, not a row to leave blank.

## Measured numbers

Every command below was run with `CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C21`.

```
cargo build --workspace --build-jobs 4
```
→ compiles clean, ~24.6s from a warm dependency cache.

```
cargo nextest run --workspace --test-threads=4 --build-jobs 4
```
→ `1449 tests run: 1449 passed, 7 skipped`, ~25.8s.

```
cargo clippy --workspace --all-targets --build-jobs 4 -- -D warnings
```
→ clean.

```
./scripts/check-comments.sh
```
→ `no board archaeology in crates/ frontend/src`.

```
./scripts/check-test-hygiene.sh
```
→ `tests take their temporary paths from a guard`.

Revert-once proof timings: acceptance test alone without the fix, 30.75s (bounded stall,
fails); with the fix restored, ~1s (passes immediately).

## What a stranger still cannot do

A stranger who deletes `frontend/e2e.db` (or any `tack.db`) without also deleting the
`storage`/`storage-e2e` directory beside it now gets a working embedded runner on the next
boot — it silently re-enrolls under a fresh identity instead of stalling short of `active`.
A stranger still cannot recover the *history* that identity change discards: any attempt
journal left unresolved from before the swap will be retried forever without ever settling
(see "Known limitations" above) — harmless, but not clean. A stranger deleting the
`storage`/`state_dir` itself (not just the database) was already fine before this card and
remains fine — that shape re-provisions from nothing, with no orphaned file to reason
about at all.

## Surface-map delta

None — this is a boot-time recovery fix with no user-visible surface (console, UI, or
API route) added, moved, or removed.

## Context spent

- Tokens read before the first edit: this handoff is a resumption of a previous agent's
  work after a machine reboot left it uncommitted — the diff and the new test file were
  read in full first (all six modified files, the one new test file), rather than trusting
  the dispatch prompt's summary of what state it was in. `docs/CONFIG.md`'s `state_dir`
  entry, `crates/tack-runner/src/journal.rs`, `crates/tack-runner/src/engine.rs`'s
  `recover`/`report_recovery_and_apply_disposition`, and `crates/tack-runner/src/transport.rs`'s
  `establish_session`/`serve` were read to answer acceptance #3 (the abandoned journal)
  precisely rather than by inference.
- Files opened and not used: none of consequence — every file read fed directly into
  either verifying the existing diff or writing this handoff's journal section.
- Read-list lines that were wrong: n/a — this card's dispatch did not hand down a specific
  file/line read-list; the "existing uncommitted work" diff was the starting point instead.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
