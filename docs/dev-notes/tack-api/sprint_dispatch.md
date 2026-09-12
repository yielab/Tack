# `crates/tack-api/src/sprint_dispatch.rs`

Moved out of the module preamble; trim or delete freely.

`POST /api/sprints/{id}/dispatch` and
 `GET /api/sprints/{id}/dispatch/dry-run` — dispatch a whole sprint's
 items to the project's linked control plane in dependency order.

 Five decisions this module makes deliberately, rather than letting them
 fall out of a stray `?`:

 1. **Partial failure: skip the one item, continue with the rest.** A
    policy block, a transport error, or a worker-task panic on item 4
    does not abort items 5–10. This is not "skip descendants too" as a
    separate step — it doesn't need to be. Item 4 not reaching a
    Done-category status (it stays wherever it was, or moves to
    whatever `on_running`/`status_map_rejected` left it at — never
    Done) means every item downstream of it in the dependency graph is
    still gated by decision 2 below and reports
    `waiting_on_dependencies` on its own, automatically. The "skip
    descendants" behaviour *emerges* from readiness gating rather than
    needing its own bookkeeping — which is exactly the kind of
    place an emergent, undesigned answer usually
    hides in.
 2. **Readiness = every direct dependency ("blocker") is in a
    Done-category status**, checked against the item's *current*, live
    status at plan time — not "dispatched," not "succeeded" (a
    `RunState`, which this module never touches), just: is the
    blocking item's `status` one this workflow calls Done right now.
    A blocker outside the sprint (even outside the project — nothing
    in the schema forecloses that) is resolved the same way: fetch it,
    fetch *its* project's workflow, check the category. A dispatch
    inside this same call can never make a same-run dependency ready
    — enqueueing only reaches `on_running`/`on_waiting_approval`
    synchronously; Done only happens later, out of band, via
    the reconciler once a run actually finishes. So the plan
    is computed once, up front, and does not need to (and cannot
    usefully) re-check readiness mid-run — see [`plan_sprint_dispatch`].
 3. **Concurrency: a bounded worker pool, not one-at-a-time and not
    all-at-once.** [`dispatch_sprint`] submits every dependency-ready
    item's [`dispatcher::dispatch_item`] call through a
    `tokio::sync::Semaphore` capped at `max_in_flight` (caller-supplied,
    clamped to `[1, MAX_MAX_IN_FLIGHT]`, default [`DEFAULT_MAX_IN_FLIGHT`]).
    Submission order follows the topological order, so with N free
    permits the first N ready items in dependency order start
    immediately and the rest queue behind them — predictable without
    serializing a 40-item sprint into a multi-minute request or firing
    40 concurrent requests at whatever machine is running docket.
 4. **No SQLite write transaction is ever open across an HTTP call
    here.** This module does not open a transaction of its own at all
    — [`plan_sprint_dispatch`] is pure reads, and every write for a
    dispatched item happens inside [`dispatcher::dispatch_item`]'s own
    fetch → HTTP → short-write-txn sequence, one item at a
    time, never spanning this module's loop over items.
 5. **The dry-run and the real run share one planning function.**
    [`plan_sprint_dispatch`] — the topological sort plus the
    dependency-readiness gate — is the only place sprint-dispatch
    ordering and skip logic is expressed, and both
    [`dry_run_sprint_dispatch`] and [`dispatch_sprint`] call it. A
    dry-run item marked `waiting_on_dependencies` and a real-run item
    marked `waiting_on_dependencies` come from the exact same branch of
    the exact same function — they cannot diverge. Per-item *eligibility*
    (is the item's status in `status_map.dispatch_from`; is it already
    in flight) is, unavoidably, evaluated twice — once as a read-only
    preview for the dry run (this module doesn't call
    [`dispatcher::dispatch_item`] at all in dry-run, by design: zero HTTP,
    zero writes), and once for real inside `dispatch_item` itself for the
    real run. To keep those two evaluations from quietly drifting apart,
    the preview calls the exact same helpers `dispatch_item` uses
    internally (`dispatcher::is_dispatch_eligible`,
    `dispatcher::is_active_task_status`) rather than re-deriving the
    same rules by hand.

 # What this module does not do

 - It never calls `ControlPlane` directly — every HTTP call to docket
   goes through [`dispatcher::dispatch_item`], so idempotency
   (`orch_tasks` + the process-wide per-item lock), the `trusted`
   boundary, and `status_map` application are exactly the same code
   path as a single manual dispatch or the auto-dispatch hook
   This module's only new logic is sprint-scoped: gather the
   items, order them, gate them on dependency readiness, and run the
   bounded pool.
 - It does not retry a `waiting_on_dependencies` item within the same
   call. A future poll/webhook-driven "the blocker just finished"
   re-trigger is a natural next call to [`dispatch_sprint`] (or the
   auto-dispatch hook, if the now-unblocked item's own status entered
   `dispatch_from` on its own) — not a loop inside this one.
