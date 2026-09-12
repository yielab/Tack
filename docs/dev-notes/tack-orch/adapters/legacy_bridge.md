# `crates/tack-orch/src/adapters/legacy_bridge.rs`

Moved out of the module preamble; trim or delete freely.

The Docket compatibility decision and its explicit
 label/policy, plus the pure, DB-free pieces of "one scheduling owner."

 # Decision: maintain

 Three options exist for the legacy Docket bridge: maintain, export, or
 deprecate it. This module maintains it, on this evidence, gathered by
 reading the code rather than assuming:

 - The legacy Docket bridge (`adapters::docket`, `reconciler.rs`, `tack-api::
   dispatcher`, `tack-api::sprint_dispatch`, `tack-api::handlers::orch`, the
   `orch_*` tables) is not a stub or a dead prototype. It is live-verified against
   a real `docket serve` instance (see `adapters::docket`'s module doc,
   "Verified live against a real docket server"), wired into auto-dispatch, sprint
   DAG-ordered dispatch, an approvals inbox, agent-fleet status, and economics —
   none of which the runner-v1 domain replaces today. `runner-v1` has no
   DAG-ordered sprint dispatch and no `pre_input` guardrail-policy engine;
   deprecating the bridge would delete working capability with no replacement,
   not retire dead code.
 - It carries real regression coverage that would need to be reproduced from
   scratch under "deprecate": `docket_adapter_test.rs`, `docket_wire_contract_test.
   rs` (per-method wire oracle), `docket_tick_contract_test.rs` (tick-level
   request-sequence oracle), plus dispatch, reconciler-wiring, auto-dispatch,
   sprint-dispatch, and approvals suites in `tack-api`, and more in `tack-db`.
 - "Export" (migrate `orch_*` data into the neutral `execution_requests`/
   `execution_attempts` shape and drop the bridge) was considered and rejected:
   docket's own capability snapshot (`DocketAdapter::capabilities`) reports
   `cancel: false`, `artifacts: false`, `model_selection: Unsupported` — an
   `execution_attempts` row asserts fields (fencing token, isolated workspace
   identity, capability snapshot used for validation) docket's wire protocol has
   no source data for. Forcing a Docket-origin task into that shape would mean
   inventing values for fields the runner-v1 contract requires to be either
   measured or explicitly `not_measured`/typed-absent — the kind of structural
   zero this codebase's rules forbid. A real export needs its own migration and
   design.
 - `TACK_ORCH_ENABLE` already makes Docket **optional** at the infrastructure
   level — unset, the reconciler never spawns and every `orch_*` route returns
 `409 orchestration_disabled`. "Maintain" does not mean "mandatory";
   it means "keep working, keep tested, keep optional."

 # The compatibility label

 [`LEGACY_DOCKET_COMPATIBILITY_LABEL`] is the one explicit, stable string
 naming this decision. It is not wired into any API response — no route
 in `tack-api::handlers::orch` currently surfaces a compatibility label
 field, and adding one would be a `handlers/orch.rs` response-shape
 change. It exists today as the one place this decision's name and
 meaning are written down for code to reference, and for
 `docs/GITHUB-SYNC.md`/`docs/MCP.md`-style operator docs to quote
 verbatim rather than paraphrase.

 # One scheduling owner

 [`SchedulingOwner`] names the two planes that can claim a Tack item's execution.
 The invariant — Docket is optional and has one documented compatibility state,
 and runner-v1 and Docket must never dual-dispatch the same item — reduces to
 one rule: **runner-v1 always outranks legacy Docket.** If an item has an active
 `execution_requests` row, legacy dispatch must defer; the reverse is not
 enforced (see below) but is asymmetric by design, not by oversight — runner-v1
 is the plan-of-record scheduler, and Docket is explicitly optional and never
 the owner of a new runner request.

 [`decide_scheduling_owner`] is the pure decision function — no I/O, fully unit
 tested here. The actual enforcement is two read-only queries in `repo/orch.rs`,
 one per direction: `tack_api::dispatcher::dispatch_item` calls
 `tack_db::repo::orch::Repository::has_active_execution_request_for_item` so a
 live runner-v1 request makes legacy Docket dispatch defer, and
 `tack-api::handlers::executions::create_execution` (`POST /api/executions`)
 calls `Repository::active_docket_task_for_item` so a live legacy Docket task
 makes a new runner-v1 request defer instead of colliding with it, and names
 the task. Both directions are proven, including the case where orchestration
 is off, by `crates/tack-api/tests/orchestration/dispatch/dual_scheduling.rs`.

 # Provider-scoped ids and the normalized-attempt projection

 An `orch_tasks` row's `remote_task_id` is a bare string minted by docket, with no
 namespace of its own — nothing stops it from colliding, in principle, with an
 opaque model id or a runner-v1 attempt id if either were ever displayed
 side-by-side. [`provider_scoped_task_id`] prefixes it with the fixed provider tag
 `"docket"` (`docket:<remote_task_id>`), mirroring the shape a genuine runner-v1
 request snapshot uses for its requested model-provider and opaque model id
 (each independently nullable) without claiming to *be* one.
 [`LegacyAttemptProjection`] is a read-only, in-memory view that maps an
 `OrchTask` (the existing `tack_db::repo::orch::OrchTask` read-side struct) into
 that provider-scoped shape for display — it does not write into
 `execution_attempts`, does not claim runner-v1 provenance, and is not consulted
 by the scheduler. It exists so a future operator surface has one normalized
 place to render "what is this legacy row, using which provider-scoped id, under
 which scheduling owner" without re-deriving the mapping.
