# `crates/tack-orch/src/scheduler/wiring.rs`

Moved out of the module preamble; trim or delete freely.

Live wiring between the pure [`super::select`]/[`super::batch`] decision
 functions and real `agent_runners`/`agent_fleet_members`/
 `execution_requests` rows.

 [`choose_request_for_runner`] is the one entry point:
 `crates/tack-api/src/handlers/runner_protocol.rs`'s `claim` handler calls
 it with a live `&tack_db::Repository`, gets back the single request id
 (if any) this runner should attempt to claim, and passes that decision
 into `tack_db::repo::execution::Repository::claim_execution_idempotent_with_snapshot`
 via `tack_db::repo::execution::RequestSelection::Scheduled(...)` — the
 only thing that actually grants a fenced lease. This module never writes
 to the database; every query here is a plain `SELECT`. See
 `RequestSelection`'s own doc comment (`tack-db`) for the full reasoning
 on why this two-step shape exists (`tack-db` cannot depend on
 `tack-orch`, so the pure scheduler cannot be called from inside the
 claim transaction itself).

 # Two gaps this module resolves

 - **No `priority` column exists on `execution_requests`.** Adding one
   needs its own migration; until then, this module derives a policy
   from `execution_requests.metadata` instead: [`priority_from_metadata`]
   reads an optional `{"priority": "low" | "normal" | "high"}` key
   (case-insensitive), defaulting to [`super::types::Priority::Normal`] —
   i.e. FIFO — for a missing key, a non-object `metadata`, or any other
   value. This is a convention this module introduces and documents, not
   a contract any other caller is required to honor; a request created
   without this key schedules exactly as it always has (FIFO among
   same-priority peers).
 - **`agent_fleets.concurrency_limit` is not enforced by the pure
   scheduler.** [`fleet_is_saturated`] checks a fleet-selector request's
   target fleet against [`tack_db::repo::execution::FleetConcurrencySnapshot`]
   before that request is ever handed to the pure scheduler at all — a
   saturated fleet's requests are filtered out up front rather than
   taught to the scheduler's per-runner eligibility model, which has no
   notion of a cross-runner fleet ceiling and is not the layer to add one
   to. This keeps `crates/tack-orch/src/scheduler/select.rs`/`batch.rs`
   completely unmodified.
