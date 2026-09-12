# `crates/tack-orch/src/scheduler/mod.rs`

Moved out of the module preamble; trim or delete freely.

The deterministic fleet scheduler.

 This module is a pure decision library: given a candidate set of runners
 (their current health/capacity/labels/declared harness and model
 support) and a request (exact runner or fleet selector, required
 harness, optional provider/model, priority), it returns either a
 selected runner or a typed reason none qualify. It performs **no I/O**
 — no database, no reconciler, no network client — and it **never grants
 the authoritative lease**; only the repository's fenced claim
 (`docs/contracts/runner-v1/`) can do that.
 [`wiring::choose_request_for_runner`] wires this module's inputs to the
 real `agent_runners` / `agent_fleet_members` / `execution_requests`
 tables (`crates/tack-db/src/migrations.rs`, migrations 039–044) and is
 called ahead of, or as a replacement for, the naive `ORDER BY created_at
 LIMIT 1` match in `crates/tack-db/src/repo/execution.rs`'s claim query.

 # Two entry points

 - [`select::select_runner`] — one request against a candidate pool.
   Pure, synchronous, deterministic.
 - [`batch::schedule`] — several requests sharing one candidate pool,
   ordered by priority then FIFO fairness, with capacity consumed across
   the pass. Built on top of [`select::select_runner`], not a separate
   algorithm.

 # Why this crate, not `tack-db` or `tack-api`

 `tack-orch` already depends inward on `tack-core` and `tack-db` only
 (see this crate's top-level doc comment) and must never depend on
 `tack-api` — exactly the shape a pure selection library needs: it can
 reuse `execution::` domain types (`HarnessKind`, `RunnerId`,
 `RunnerSelector`, `HarnessCapability`, …) without pulling in HTTP,
 `sqlx`, or any transport concern.

 # Live wiring

 [`wiring::choose_request_for_runner`] fetches real
 `agent_runners`/`agent_fleet_members`/`execution_requests` rows via a
 live `tack_db::Repository`, builds the [`RunnerCandidate`]/
 [`SchedulingRequest`] values above from them, and calls
 [`select::select_runner`]/[`batch::schedule`] — still performing no
 write of its own. `crates/tack-api/src/handlers/runner_protocol.rs`'s
 `claim` handler is the only caller in production.
