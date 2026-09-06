# VI-C25 handoff

- Base SHA / branch / final SHA: worktree dispatched at `e789435` (`docs(board): card the
  stranded journal records as VI-C25`) on branch `agent/vi-c25-stranded-journal-records`,
  which is `develop`'s own tip — no rebase or merge needed. Final SHA is HEAD of this branch
  after the single commit that includes this file (not hardcoded here, for the same reason
  VI-C23's handoff gives).
- Files changed (equals the card's ownership line): `crates/tack-runner/src/engine.rs` only
  — `report_recovery_and_apply_disposition`, the `FakeProtocol` test double it's exercised
  through, and two new tests. No other file in the crate or workspace touched.
- Contract fixtures consumed: none. This is a client-side disposition change; the wire shape
  of `recovery-observation.request.json`/`.response.json` and every `StableErrorCode`
  variant are unchanged. See "Wire-contract check" below for what was actually verified
  before concluding that.

## Behavior implemented

**Before:** `report_recovery_and_apply_disposition` matched the `observe_recovery` call's
result in two arms only — an exact-matching `Ok` response, or `Ok(_) | Err(_)` for
everything else (a mismatched ack, a transport failure, a stale-lease rejection, any other
protocol error). The second arm always returned `RunCycle::RecoveryPending`, which leaves
the record in the restart scan forever if the underlying condition never changes — exactly
what happens once the server has recreated its database and the runner has self-provisioned
a fresh identity: the old record's `attempt_id`/`fencing_token` pair will never again match
a row, so every future boot repeats the same doomed call.

**After:** a third arm, `Err(ProtocolClientError::StaleLease)`, is matched ahead of the
catch-all and quarantines the record instead — `self.journal.quarantine(record)?` followed
by `Ok(RunCycle::Quarantined { .. })`, the exact call the existing `NeedsOperator`
disposition path already uses. Everything else (`Transport`, `Rejected`, `RunnerRevoked`,
any other `Protocol { code }`, and a syntactically-`Ok` but mismatched response) still falls
through to the unchanged `RecoveryPending` arm.

### Why `StaleLease` is the right, and only currently available, signal

I traced what the server can actually say back for this call, to find a signal that means
"does not know this attempt" without also meaning "did not answer" or "not authorised":

- `crates/tack-api/src/handlers/runner_protocol.rs::observe_recovery` authenticates the
  bearer credential first (`runner_auth::authenticate`), entirely before it ever looks at
  the attempt id in the path. A bad, revoked, inactive or expired credential fails there
  with `StableErrorCode::Unauthorized` or `RunnerRevoked` — codes that mean "this runner's
  own identity is rejected", never "this specific attempt is unknown". `ProtocolClientError`
  keeps `RunnerRevoked` as its own variant and folds `Unauthorized` into
  `Protocol { code }`; both stay on the unchanged, catch-all arm.
- Past authentication, the only place the attempt id is checked is
  `crates/tack-db/src/repo/execution.rs::recover_attempt`'s row lookup: `WHERE a.id=? AND
  a.runner_id=? AND a.fencing_token=? AND r.state='active' AND r.revoked_at IS NULL`. When
  that finds no row, the handler returns `stale_lease` (409) — and that is the **only**
  code this endpoint ever returns for an attempt-id mismatch; it never returns `not_found`
  (grepped: no `StableErrorCode::NotFound` reference anywhere under
  `handlers/runner_protocol.rs` or the `recover_attempt` call chain).
- `fencing_token` is set once, at claim time, and never rewritten for that attempt id again
  (grepped every `UPDATE execution_attempts` statement in `execution.rs`; only
  `last_heartbeat_at`/`lease_expires_at`/`state`/`event_checkpoint`/`completion_id`/
  `terminal_reason`/etc. columns are ever touched by heartbeat or refresh — never
  `fencing_token`). So for a fixed `(attempt_id, runner_id)` pair, "no row matches this
  fencing token" collapses to "no row matches this attempt id under this runner at all" —
  there is no legitimate, still-live case where the same attempt under the same runner
  later carries a different, superseding fencing token that this lookup would reject as
  stale.
- The type's own doc comment states the intent directly:
  `ExecutionError::StaleLease`'s comment in `crates/tack-orch/src/execution/types.rs` reads
  *"`stale_lease` remains a fixed machine-readable code so a runner can safely abandon its
  obsolete fence without parsing prose."* That is this card's exact problem, already
  anticipated by the protocol's own design.

**Stop-and-report check, done, not tripped:** the card's stop condition is a `not_found` vs
`forbidden`/`unauthorized` ambiguity. I confirmed the two are not conflated here —
credential-level rejection (`Unauthorized`/`RunnerRevoked`) happens in a structurally
earlier step (`authenticate`, before the path's attempt id is even read) and never produces
`stale_lease`; only the attempt/fencing lookup produces `stale_lease`, and only for "no
matching row". There *is* a coarser ambiguity inside `stale_lease` itself — the same code
also covers "this exact attempt id belongs to a different runner" — but that is not the
ambiguity the card named, and per the reasoning above it does not arise for the actual bug
(a wiped/recreated database has no other runner's rows to collide with, and the fencing
token can never legitimately roll over for a live attempt under the same runner). Flagging
it here rather than silently assuming it away, in case a future card widens the scope to a
non-DB-wipe stranding scenario.

## "Retired" means quarantine, not deletion

`journal.quarantine()` (`crates/tack-runner/src/journal.rs`) renames the record's file from
`journal/` to a sibling `quarantine/` directory — it never touches the workspace/checkout on
disk, and nothing else in this file's cleanup calls (`self.workspaces.cleanup(...)`, called
only after `RunCycle::Completed`/`Cancelled` in `run_claimed`) ever runs for a `Quarantined`
outcome. So the choice was already made by the existing mechanism this fix reuses:

- **What the operator keeps:** the workspace checkout at the path the record still names,
  exactly as it stood — nothing extra is deleted or moved. The quarantined journal record
  itself, readable at `<state_dir>/runner/<runner_id>/quarantine/<attempt_id>.toml` (or the
  equivalent path for this runner's layout), so the *last known local state* of the attempt
  (its journal state, workspace id/path/base revision, fencing token) is still on record for
  inspection.
- **What the operator loses:** any further retry of this exact attempt — the record is
  permanently out of the restart scan (`JournalState::Quarantined` is not
  `is_unresolved()`), and there is no un-quarantine path in this codebase today. If the
  workspace checkout needs disk space back, that is now a manual decision, same as for every
  other quarantined record; this change does not add a cleanup sweep for quarantined
  checkouts (that would be the workspace manager's cleanup contract, which the card
  explicitly does not assign here).
- Deletion was rejected because the checkout is exactly the evidence acceptance's own
  background paragraph says an operator needs, and destroying it on the runner's own
  say-so (a single `stale_lease` response) trades a one-line disk-space saving for
  information that cannot be reconstructed once gone. Quarantine costs nothing new: it is
  the same outcome, same mechanism, same operator-facing shape as the pre-existing
  `NeedsOperator` disposition, just reached from a different, more specific input.

## What happens to records already stranded by the time this ships

Nothing extra has to run for them. `RunnerEngine::recover` (the restart scan) iterates every
record `self.journal.unresolved()` returns, unconditionally — it has no notion of when a
record was written, only whether its `JournalState` is still one of the unresolved ones
(`Prepared`, `ProcessObservedRunning`, `CancellationRequested`, `TerminalReportPending`). A
record stranded before this ships is, definitionally, still sitting in one of those states
(that is exactly what "stranded" meant — the restart scan kept finding it and never settled
it). On the runner's very next boot on this build, that record goes through the same
`self.adapter.reconcile(&record)` → `report_recovery_and_apply_disposition` call this fix
changed (`crates/tack-runner/src/engine.rs`, `recover()`'s loop body, lines ~288–296), gets
the same `stale_lease` response the server has presumably been returning to it all along,
and is quarantined on that boot. No migration, backfill, or manual sweep is needed or
provided — the fix is retroactive by construction, the first time the fixed binary runs.

## Tests added and exact commands/results

Both new tests live in `crates/tack-runner/src/engine.rs`'s `mod tests`, immediately after
`needs_operator_response_durably_quarantines_stopped_pre_spawn_recovery` (the existing
`NeedsOperator` quarantine test they parallel). Both create a real directory + file on disk
standing in for the harness checkout, so the "checkout retained/untouched" assertion is a
direct filesystem check, not an inference from a status code.

Test infrastructure change (not a test itself): `FakeProtocol` gained a
`recovery_error: Arc<Mutex<Option<ProtocolClientError>>>` field. When set,
`observe_recovery` returns that exact error on every call, indefinitely — distinct from the
pre-existing `recovery_failures_remaining` counter, which models a *finite* number of
transient failures before the server starts answering normally. Default is `None`,
preserving every existing test's behavior unchanged (confirmed by the full-suite run below).

1. `stale_lease_on_recovery_retires_the_record_and_keeps_the_checkout` — server answers
   every `observe_recovery` call with `ProtocolClientError::StaleLease`. Asserts: the
   outcome is `RunCycle::Quarantined`; `journal.unresolved()` is empty afterward;
   `quarantine/` has an entry; the checkout file (`workspace_path.join("evidence.txt")`)
   still exists on disk.
2. `unreachable_server_on_recovery_never_retires_the_record` — server answers every
   `observe_recovery` call with `ProtocolClientError::Transport` (a real "could not reach
   the server", not a response that says "unknown"). Run across **two** separate restarts
   (two fresh `RunnerEngine` instances over the same journal/workspace root), not one, so a
   "quarantines after N tries" regression couldn't hide behind a single-boot pass. Asserts,
   after both restarts: every call returned `RunCycle::RecoveryPending`; `journal.unresolved()`
   still has exactly one record; the `quarantine/` directory is either absent or empty; the
   checkout file still exists.

Commands and results (`CARGO_TARGET_DIR=/var/tmp/tack-agent-targets/VI-C25` throughout):

```
nice -n 19 cargo nextest run --workspace --build-jobs 4 --test-threads 4 \
  -E 'test(stale_lease_on_recovery_retires_the_record_and_keeps_the_checkout) or test(unreachable_server_on_recovery_never_retires_the_record)'
```
→ `2 tests run: 2 passed, 1456 skipped` in 0.016s.

```
nice -n 19 cargo nextest run --workspace --build-jobs 4 --test-threads 4 -E 'test(recovery) + package(tack-runner)'
```
→ `286 tests run: 286 passed, 1172 skipped` (every existing recovery/quarantine/pending test
in the crate, unchanged).

```
nice -n 19 cargo nextest run --workspace --build-jobs 4 --test-threads 4
```
→ `1451 tests run: 1451 passed, 7 skipped` in 25.773s (full workspace, both new tests
included).

## Revert-once proof

Removed only the new `Err(ProtocolClientError::StaleLease) => { ... }` match arm (kept both
new tests and the `FakeProtocol` plumbing exactly as committed), leaving the old two-arm
match (`Ok(response) if ... => response`, `Ok(_) | Err(_) => RecoveryPending`) — i.e. exactly
this card's before-state. Ran:

```
nice -n 19 cargo nextest run --workspace --build-jobs 4 --test-threads 4 \
  -E 'test(stale_lease_on_recovery_retires_the_record_and_keeps_the_checkout) + package(tack-runner)'
```

Result:

```
FAIL [   0.010s] ( 33/260) tack-runner client::engine::tests::stale_lease_on_recovery_retires_the_record_and_keeps_the_checkout
  stdout ───
    thread 'client::engine::tests::stale_lease_on_recovery_retires_the_record_and_keeps_the_checkout' (523750) panicked at crates/tack-runner/src/engine.rs:2945:9:
    assertion failed: matches!(engine.recover(&session()).await.expect("recovery").as_slice(),
        [RunCycle::Quarantined { .. }])
    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
    test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 245 filtered out
```
259 other tests in that run stayed green — only the one test tied to the reverted behavior
failed, and it failed on the exact assertion the fix exists to satisfy (the outcome stayed
`RecoveryPending` instead of becoming `Quarantined`).

Restored the removed arm verbatim (`git diff --stat` afterward: `1 file changed, 179
insertions(+)` — later reformatted by `cargo fmt` to `175 insertions(+)`, same content,
`0` deletions both times — confirming the restore was exact, not a partial patch). Re-ran
the same selector: `286 tests run: 286 passed`.

## Wire-contract check

Read (not edited) `docs/contracts/runner-v1/recovery-observation.{request,response}.json`,
`errors/stale-lease.json`, `errors/not-found.json`, and
`crates/tack-orch/tests/runner_contract.rs`'s pin table. Nothing about the request the
runner sends or the response shapes it parses changed — this fix only adds a new branch on
an error variant the client-side enum (`ProtocolClientError`) already had before this card,
and only changes which local `RunCycle`/journal-state transition follows a response the
wire contract already allows. No fixture edit was made or needed.

One pre-existing discrepancy noticed in passing, **not fixed here** (out of this card's
ownership, and touching it would be a wire-contract change): `errors/stale-lease.json`'s
frozen fixture body carries `details: {attempt_id, current_fencing_token}`, but the actual
`stale_lease()` helper in `crates/tack-api/src/handlers/runner_protocol/runner_auth.rs`
only ever populates `details: {attempt_id}` — `current_fencing_token` is never set on any
call site. This does not affect this card's fix (the client never reads `details` to decide
between `Quarantined` and `RecoveryPending`), but a future reader chasing that field will
find it silently absent.

## Failure/adversarial cases proved

- **StaleLease definitely retires, definitively** — proved by test 1 and the revert-once
  proof above.
- **A genuinely unreachable server never retires anything** — proved by test 2, across two
  restarts, using a real transport failure rather than a response that says "unknown". This
  is the specific distinction acceptance #2 asked for: the two tests exercise different
  `ProtocolClientError` variants, not different configurations of the same one.
- **A bare transport failure (server unreachable, not merely this attempt) still doesn't
  retire** — unchanged, and additionally already covered by the pre-existing
  `failed_ambiguity_delivery_is_retried_on_restart_without_respawn` test (a finite run of
  `ProtocolClientError::Transport` via `recovery_failures_remaining`, distinct from my new
  test's indefinite `recovery_error`); confirmed passing in the full-crate run above.
- **A syntactically-`Ok` but mismatched acknowledgement** (response `attempt_id`/
  `recovery_key` disagree with the request) still falls to the unchanged catch-all arm by
  code inspection — no test in this file, before or after this card, exercises that specific
  sub-case for recovery (`RecoveryResponseConfig` has no mismatch field, unlike its
  completion/cancellation siblings). Noting the gap rather than claiming coverage that
  doesn't exist; the code path is untouched by this fix either way, so behavior there is
  unchanged, just not independently proved.

## Known limitations / not-measured

- The `stale_lease`-covers-two-cases ambiguity noted under "Wire-contract check" above
  (attempt truly absent vs. attempt owned by a different, valid runner) is real at the
  protocol level but does not arise for the bug this card fixes, for the reasons given
  there. If a later card strands a journal record by some path other than a full database
  recreation (e.g. selective retention/pruning of individual attempt rows while the runner
  table survives), that path should re-examine whether the ownership-collision case can
  actually occur before assuming this fix's reasoning still holds unchanged.
- No production log line was added or changed by this fix, so there is nothing new to prove
  redacted; the existing `tracing` around `recover()`/`report_recovery_and_apply_disposition`
  is untouched.

## Secrets/logging review

No log line, credential, or config value touched. The new code path adds no `tracing` call
of its own; it reuses `self.journal.quarantine(record)?`, an existing call already used by
the `NeedsOperator` path, whose own logging (if any) is unchanged.

## Safe merge order and likely conflicts

Single file, single crate, no fixture, no migration. The only plausible conflict is another
card also editing `report_recovery_and_apply_disposition`, `apply_recovery_disposition`, or
`FakeProtocol`'s `observe_recovery` in the same file — no other open Part VI/VII card names
that ownership. Safe to merge in any order relative to other open branches.

## Checklist

No unowned files touched (only `crates/tack-runner/src/engine.rs`, matching the card's
ownership line exactly). No live secret. No panic stub (`self.journal.quarantine(record)?`
propagates via the existing `EngineError::Journal` conversion, same as every other call site
in this function). No blind retry — the fix's entire point is to stop retrying an attempt
the server will never recognize again, while leaving every genuinely-retryable case
untouched.

## Claim → evidence

| Claim (user-visible, added or kept) | Evidence — command, test name, or transcript |
|---|---|
| A journal record naming an attempt the server has no lease for is retired (quarantined) on the next boot, not rescanned | `stale_lease_on_recovery_retires_the_record_and_keeps_the_checkout`: outcome `Quarantined`, `journal.unresolved()` empty, `quarantine/` non-empty |
| The checkout is kept, not deleted, when a record is retired | same test: `workspace_path.join("evidence.txt").exists()` asserted true after quarantine |
| A record whose acknowledgement merely failed to arrive (server unreachable) is never retired | `unreachable_server_on_recovery_never_retires_the_record`: two restarts, both `RecoveryPending`, `journal.unresolved()` still length 1, no quarantine entry |
| The fix is load-bearing, not a no-op | revert-once proof: removing only the new match arm fails exactly `stale_lease_on_recovery_retires_the_record_and_keeps_the_checkout`, at the exact assertion, with 259 sibling tests still green |
| Every pre-existing recovery/quarantine/pending behavior in the crate is unchanged | `test(recovery) + package(tack-runner)`: `286 tests run: 286 passed` |
| Nothing elsewhere in the workspace regressed | full-workspace `nextest run --workspace`: `1451 tests run: 1451 passed, 7 skipped` |
| No wire-contract change | fixtures and `runner_contract.rs` pin table read, not edited; `ProtocolClientError::StaleLease` already existed before this card |

A row with no evidence is a claim to delete, not a row to leave blank.

## Surface-map delta

None — no route, console command, or UI surface touched. This is a purely internal runner
disposition change.

## Context spent

- Files read in full or in large part before the first edit: `crates/tack-runner/src/
  engine.rs` (the whole `report_recovery_and_apply_disposition`/`apply_recovery_disposition`/
  `recover`/`FakeProtocol` region, plus several existing recovery tests as templates),
  `crates/tack-api/src/handlers/runner_protocol.rs`'s `observe_recovery` handler,
  `crates/tack-api/src/handlers/runner_protocol/runner_auth.rs` (`authenticate`,
  `require_matching_attempt`, `stale_lease`), `crates/tack-db/src/repo/execution.rs`'s
  `recover_attempt` and every `UPDATE execution_attempts` statement in the file (to confirm
  `fencing_token` is never rewritten), `crates/tack-orch/src/execution/types.rs`'s
  `StableErrorCode`/`ExecutionError` definitions, `crates/tack-runner/src/journal.rs`'s
  `quarantine`/`JournalState::is_unresolved`, and `docs/contracts/runner-v1/errors/
  stale-lease.json` + `not-found.json`.
- Files opened and not used for the fix itself: none — the investigation into whether
  `not_found`/`forbidden` are ever conflated with `stale_lease` was necessary to clear the
  card's stop-and-report condition, not incidental.
- Read-list lines that were wrong: n/a — this card's dispatch did not hand down a specific
  file/line read-list.

## Amendments

*(Appended by later readers, dated. The original text above is never rewritten — the
history of what was believed and later falsified is the point.)*
