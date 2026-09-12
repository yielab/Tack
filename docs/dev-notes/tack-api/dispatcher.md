# `crates/tack-api/src/dispatcher.rs`

Moved out of the module preamble; trim or delete freely.

The write path that
 makes Tack a control center rather than a dashboard. Given a Tack item
 that has just entered (or is being manually pushed into) a
 dispatch-eligible status, [`dispatch_item`] enqueues a governed task on
 the project's linked control plane and records the outcome.

 # What this module does, end to end

 1. Resolve the item's project → `orch_links` row → `status_map`
    An unlinked project, or a `status_map` with an empty
    `dispatch_from`, are both valid, ordinary states — not errors — see
    [`DispatchOutcome::NoDispatchPolicy`].
 2. Refuse (without touching docket) if the item's current status isn't
    one of `dispatch_from` — [`DispatchOutcome::NotEligible`].
 3. Idempotency: if the item's most recent `orch_tasks` attempt is still
    active (pending/running/waiting_approval), do **not** call docket
    again — [`DispatchOutcome::AlreadyInFlight`]. See "Idempotency and
    `attempt`" below.

 **One scheduling owner.** If the item already has
 an active runner-v1 `execution_requests` row, do **not** call docket at
 all — `Err(ApiError::Conflict(..))`, same shape the concurrent-dispatch
 lock already uses. Checked before any HTTP call. See
 `tack_db::repo::orch`'s module section for the exact "active"
 definition.
 4. Call `ControlPlane::enqueue_task` (`POST /tasks/{project}`,
    live-verified three-outcome contract):
    - **block** → [`DispatchOutcome::Blocked`], no `orch_tasks` row at
      all (docket never created a task).
    - **allow** / **require_approval** → both are `Ok(task_id)` from the
      adapter (see `adapters::docket`'s module doc for why the trait
      can't distinguish them); a follow-up `list_tasks` call recovers the
      real status + approval token.
 5. Persist `orch_tasks` (task id + attempt + trust), then apply the
    `status_map`-named target status (`on_waiting_approval` or
    `on_running`) **through the workflow engine** — never raw SQL
    A transition the engine refuses (WIP limit, an
    explicit-transition workflow like construction's) is recorded as a
    `status_map_rejected` `orch_events` row and surfaced in the response;
    the item is left exactly as it was.

 # Trust is not optional

 [`dispatch_item`]'s `trusted: bool` parameter has **no default** — it is
 not `Option<bool>`, and there is no sibling function that omits it. This
 is deliberate: `core/dispatch.py::enqueue_task`'s own `trusted: bool |
 None` treats an omitted value as "trusted iff `source == \"operator\"\"`,
 which — since docket's `source` is hardcoded to `"operator"` on every
 call — silently grants operator trust.
 Untrusted-source handling calls this function with
 `trusted: false` for GitHub/Linear-imported items; this module's own
 HTTP entry point ([`handlers::orch::dispatch_item`]) defaults
 conservatively too — see that handler's doc comment. A required
 positional `bool` can't stop a caller from passing the wrong *value*,
 but it makes the *unsafe omission* a compile error instead of a silent
 default.

 # Idempotency and `attempt`

 `orch_tasks`' PK is `(item_id, remote_task_id)` — a genuine redispatch
 (after a previous attempt reached a terminal state) is supposed to
 create a new row, not collide with the old one. **`attempt`** is defined
 here as: `1 + the highest existing attempt number for this item`, and a
 new dispatch is only attempted when no existing attempt is still
 "active" (`pending` / `running` / `waiting_approval` — anything else,
 including a status this version of Tack doesn't recognise, is treated as
 terminal and redispatchable). Two protections make "double-dispatching
 the same item creates one task, not two" hold even under concurrency:

 1. **[`DispatchLocks`]** — a process-wide, per-`item_id` mutual-exclusion
    guard (a bare `HashSet<Uuid>` behind a `std::sync::Mutex`, not part of
    `AppState` — see its own doc comment for why). Two concurrent
    dispatch requests for the *same* item never both reach the "check
    existing tasks" step; the second is rejected immediately
    (`ApiError::Conflict`) rather than racing the first.
 2. **The `orch_tasks` read itself**, done once the lock is held, catches
    the sequential case (a caller retries after the first request already
    completed).

 Tack is a single-process, single-SQLite-writer binary (CLAUDE.md), so a
 process-local lock is a complete solution here — it would not be if Tack
 ever ran as multiple replicas.

 # What this module deliberately does *not* do

 - **Terminal-state (`on_succeeded`/`on_failed`/`on_cancelled`)
   application** is not wired *here* — the reconciler applies it once a
   run polled by `orch_runs`
   reaches a terminal `RunState`, via `orch_store.rs`'s
   `reconcile_terminal_status_map`, a call site inside
   `RepoControlPlaneStore::upsert_runs`. [`apply_mapped_status`] is the
   shared engine both call sites use — it is generic over "which target
   status, which trigger name", not specific to
   `on_running`/`on_waiting_approval`.
 - **`ControlPlane::dispatch`** (`POST /dispatch/{project}`, pipeline
   `variables`) is never called. Only `enqueue_task` is used — see
   `adapters::docket`'s module doc for why.
 - Auto-dispatch and sprint DAG-ordered dispatch both call
   [`dispatch_item`] rather than duplicating any of this.
