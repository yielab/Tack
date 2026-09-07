# Recovery Runbook

What to do when a `tack-runner` process, its host, or the harness subprocess it
launched dies mid-attempt, and how Tack decides whether it's safe to retry
automatically or requires you to look.

**The governing rule, stated once so nothing below contradicts it:** Tack does not
claim exactly-once harness execution. A database transaction can prevent two valid
leases at once, but a runner or network crash after a process has actually launched
can leave ownership genuinely ambiguous — nothing server-side can prove the process is
dead. The contract is narrower and honest: at most one **valid active lease** at a
time, a monotonically increasing fencing token per attempt, a local runner journal
written before any process spawn, idempotent completion reports, and `needs_operator`
whenever safe retry cannot be proven. **A lease expiring never, by itself, launches a
second process against the same workspace.**

---

## The lifecycle states involved

```
leased ──▶ preparing ──▶ running ──▶ waiting_decision
   │            │            │              │
   └────────────┴────────────┴──────────────┘
                       │  (recovery service observes)
                       ▼
              lost  or  needs_operator
```

- **`lost`** — the recovery machinery is convinced no process was ever actually
  running for this attempt (it crashed or the runner restarted before spawning
  anything). Automatically safe to requeue.
- **`needs_operator`** — anything less certain than that. Not automatically
  retryable. Requires an explicit, audited operator decision.

Only two actors can move an attempt into either state:
`(Leased | Preparing | Running | WaitingDecision) → (Lost | NeedsOperator)`, and only
the **recovery service** (never the scheduler, never a bare lease-owner heartbeat
timeout) may make that transition —
`crates/tack-orch/src/execution/lifecycle.rs::validate_transition`. There is no
background sweep that ages out a stale lease into `lost` on a timer; recovery is
**observation-driven**, described next.

---

## How recovery starts: the runner reports what it saw

When a `tack-runner` process restarts (after a crash, a host reboot, or an operator
kill), it reads its own local journal — written to `TACK_RUNNER_STATE_DIR` *before*
any harness process was ever allowed to spawn
(`crates/tack-runner/src/journal.rs`) — and reports one of three observations to
`POST /api/runner/v1/attempts/{id}/recovery-observation`:

| Observation | What it means |
|---|---|
| `process_stopped` | The runner found no live process for this attempt (checked by PID/process-group, not merely "the runner restarted") |
| `process_running` | The runner found the harness process still alive and reattached to it |
| `ambiguous` | The runner could not determine either way — e.g. the process table is inconclusive, or the journal itself was corrupt/incomplete |

The server decides the disposition from the observation **and** the journal state it
already had on file for that attempt (`crates/tack-db/src/repo/execution.rs`,
`recover_attempt`):

- **`safe_pre_spawn_requeue`** — granted only when the observation is
  `process_stopped`, the attempt's `started_at` was never set, and the last known
  journal state was `prepared` (the harness process itself was never spawned in the
  first place). This is the one case narrow enough to resolve automatically: nothing
  ever ran, so there is nothing ambiguous to reconcile. The attempt moves to `lost`,
  the request moves back to `queued`, no operator involved.
- **`needs_operator`** — every other combination: `process_running` (a live process
  exists but the runner that owned it just changed identity across a restart — no
  safe way to hand it back into scheduling from here), `ambiguous`, or
  `process_stopped` after the process *had* actually started (it may have made
  side effects — a partial commit, a half-written file — that a blind requeue would
  duplicate).
- **`already_terminal`** — the attempt had already reached `succeeded`/`failed`/
  `cancelled` before the observation arrived; the observation is recorded for audit
  but changes nothing.

Recovery observations are idempotent, scoped by a caller-supplied `recovery_key`: a
second call with the byte-identical request replays the same response
(`RecoveryObservationResult::Replayed`); a second call with a *different* body under
the same key returns `409 idempotency_conflict` rather than silently overwriting the
first audit record.

---

## Resolving a `needs_operator` attempt

This is the only path back to `queued` for anything other than the narrow
`safe_pre_spawn_requeue` case. It requires a human decision and an audit trail —
there is no API shortcut that skips the reason field.

1. **Investigate.** Check the runner host directly: is the harness process actually
   still running? Did it leave a partial workspace/worktree behind
   (`TACK_RUNNER_STATE_DIR`, and the attempt's worktree path recorded in the journal)?
   Check the execution's event timeline (`GET /api/executions/{id}/attempts/{n}/
   events`) for the last thing the runner reported before contact was lost.
2. **Decide and record why.** Once you're confident it's safe — the process is
   confirmed dead, or you've manually cleaned up any side effects — requeue with an
   explicit reason:

   ```sh
   tack execution reconcile <request-id> \
     --recovery-key "ops-2026-08-19-host-restart" \
     --reason "confirmed via ps on runner-3 that the codex process is gone; \
   no uncommitted worktree changes found"
   ```

   This calls `POST /api/executions/{id}/requeue`
   (`crates/tack-api/src/handlers/executions.rs::requeue_needs_operator`), which:
   - succeeds only for a request the recovery service *itself* already
     authoritatively marked `needs_operator` — it is not a general-purpose "force
     requeue anything" escape hatch;
   - is idempotent on `recovery_key`: replaying the identical call returns
     `replayed: true` instead of double-queuing;
   - returns `409 conflict` (`idempotency_conflict`) if the same key is reused with a
     different confirmation, and `409 invalid_transition` — naming the actual current
     state in `details.from` — for any request that isn't in an authoritatively
     recovered `needs_operator` state.

   Verified against a live server for this page: requeuing a request that was never
   put into `needs_operator` in the first place returns exactly
   `{"code":"invalid_transition","details":{"from":"unknown","to":"queued"}}`, never a
   silent success. Reused in
   `crates/tack-cli/tests/e6_scheduler_e2e_test.rs`.
3. **Confirm.** `tack execution get <request-id>` shows `queued` (or, if the reason
   field described something unrecoverable, resolve to `failed` by other means — the
   requeue path only ever re-queues, it does not force a terminal state).

---

## Cancellation is a request, not a guarantee

`POST /api/executions/{id}/cancel` records a **request** for cancellation; the runner
observes and reports the actual outcome via
`POST /api/runner/v1/attempts/{id}/cancellation-observation`. This mirrors the
recovery split deliberately: Tack cannot itself kill a process running on a runner's
host, so it never pretends to. See the [capability matrix](agent-runners.md#capability-matrix)
for why `cancel` is `advisory`, never `supported`, on every in-tree harness adapter —
none of the three can guarantee the underlying process actually stops, because each
harness's own shell tool spawns its subprocess in a new session outside the runner's
process group.

A cancellation observation on an attempt that already reached a terminal state returns
`409 invalid_transition` with the actual terminal state named, not a false success.

---

## What this runbook does not cover

- **Database or migration recovery** (a crashed migration, a corrupted `.db` file) —
  see [Backup and Restore](backup-restore.md) and `docs/adr/0008-transactional-
  migration-rebuild-recovery.md`.
- **Adversarial proof of these boundaries** — duplicated or revoked runner
  credentials, a stale fence retried against every attempt-scoped route, corrupt or
  truncated runner journals, oversized or path-traversal artifact uploads, and
  reordered/replayed event batches — is in
  `crates/tack-api/tests/security/chaos_recovery.rs` and
  `crates/tack-runner/tests/g2_journal_corruption_test.rs`, both run against the real
  production router and a real runner journal, not this page. Killing the API
  process at each protocol step is covered separately by
  `crates/tack-runner/tests/crash_matrix.rs`. Disk-full/`ENOSPC` mid-write has no
  existing test harness in this repository and remains unverified.
- **Chaos/adversarial proof of the runner's own recovery path against a live
  server** beyond what `scripts/smoke.sh`'s restart-recovery step (step 9) already
  covers — see the [Agent Runners](agent-runners.md#what-actually-runs-today) page.
  The runner binary's network transport is wired to the server
  (`tack_runner::bootstrap::build_runtime` attaches the real `HttpPullProtocol`,
  not a stub), and a runner can report a recovery observation over a live
  connection today.
