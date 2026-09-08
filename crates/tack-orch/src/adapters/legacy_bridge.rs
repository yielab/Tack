//! The Docket compatibility decision and its explicit
//! label/policy, plus the pure, DB-free pieces of "one scheduling owner."
//!
//! # Decision: maintain
//!
//! Three options exist for the legacy Docket bridge: maintain, export, or
//! deprecate it. This module maintains it, on this evidence, gathered by
//! reading the code rather than assuming:
//!
//! - The legacy Docket bridge (`adapters::docket`, `reconciler.rs`, `tack-api::
//!   dispatcher`, `tack-api::sprint_dispatch`, `tack-api::handlers::orch`, the
//!   `orch_*` tables) is not a stub or a dead prototype. It is live-verified against
//!   a real `docket serve` instance (see `adapters::docket`'s module doc,
//!   "Verified live against a real docket server"), wired into auto-dispatch, sprint
//!   DAG-ordered dispatch, an approvals inbox, agent-fleet status, and economics —
//!   none of which the runner-v1 domain replaces today. `runner-v1` has no
//!   DAG-ordered sprint dispatch and no `pre_input` guardrail-policy engine;
//!   deprecating the bridge would delete working capability with no replacement,
//!   not retire dead code.
//! - It carries real regression coverage that would need to be reproduced from
//!   scratch under "deprecate": `docket_adapter_test.rs`, `docket_wire_contract_test.
//!   rs` (per-method wire oracle), `docket_tick_contract_test.rs` (tick-level
//!   request-sequence oracle), plus dispatch, reconciler-wiring, auto-dispatch,
//!   sprint-dispatch, and approvals suites in `tack-api`, and more in `tack-db`.
//! - "Export" (migrate `orch_*` data into the neutral `execution_requests`/
//!   `execution_attempts` shape and drop the bridge) was considered and rejected:
//!   docket's own capability snapshot (`DocketAdapter::capabilities`) reports
//!   `cancel: false`, `artifacts: false`, `model_selection: Unsupported` — an
//!   `execution_attempts` row asserts fields (fencing token, isolated workspace
//!   identity, capability snapshot used for validation) docket's wire protocol has
//!   no source data for. Forcing a Docket-origin task into that shape would mean
//!   inventing values for fields the runner-v1 contract requires to be either
//!   measured or explicitly `not_measured`/typed-absent — the kind of structural
//!   zero this codebase's rules forbid. A real export needs its own migration and
//!   design.
//! - `TACK_ORCH_ENABLE` already makes Docket **optional** at the infrastructure
//!   level — unset, the reconciler never spawns and every `orch_*` route returns
//! `409 orchestration_disabled`. "Maintain" does not mean "mandatory";
//!   it means "keep working, keep tested, keep optional."
//!
//! # The compatibility label
//!
//! [`LEGACY_DOCKET_COMPATIBILITY_LABEL`] is the one explicit, stable string
//! naming this decision. It is not wired into any API response — no route
//! in `tack-api::handlers::orch` currently surfaces a compatibility label
//! field, and adding one would be a `handlers/orch.rs` response-shape
//! change. It exists today as the one place this decision's name and
//! meaning are written down for code to reference, and for
//! `docs/GITHUB-SYNC.md`/`docs/MCP.md`-style operator docs to quote
//! verbatim rather than paraphrase.
//!
//! # One scheduling owner
//!
//! [`SchedulingOwner`] names the two planes that can claim a Tack item's execution.
//! The invariant — Docket is optional and has one documented compatibility state,
//! and runner-v1 and Docket must never dual-dispatch the same item — reduces to
//! one rule: **runner-v1 always outranks legacy Docket.** If an item has an active
//! `execution_requests` row, legacy dispatch must defer; the reverse is not
//! enforced (see below) but is asymmetric by design, not by oversight — runner-v1
//! is the plan-of-record scheduler, and Docket is explicitly optional and never
//! the owner of a new runner request.
//!
//! [`decide_scheduling_owner`] is the pure decision function — no I/O, fully unit
//! tested here. The actual enforcement is two read-only queries in `repo/orch.rs`,
//! one per direction: `tack_api::dispatcher::dispatch_item` calls
//! `tack_db::repo::orch::Repository::has_active_execution_request_for_item` so a
//! live runner-v1 request makes legacy Docket dispatch defer, and
//! `tack-api::handlers::executions::create_execution` (`POST /api/executions`)
//! calls `Repository::has_active_docket_task_for_item` so a live legacy Docket
//! task makes a new runner-v1 request defer instead of colliding with it. Both
//! directions are proven, including the case where orchestration is off, by
//! `crates/tack-api/tests/orchestration/dispatch/dual_scheduling.rs`.
//!
//! # Provider-scoped ids and the normalized-attempt projection
//!
//! An `orch_tasks` row's `remote_task_id` is a bare string minted by docket, with no
//! namespace of its own — nothing stops it from colliding, in principle, with an
//! opaque model id or a runner-v1 attempt id if either were ever displayed
//! side-by-side. [`provider_scoped_task_id`] prefixes it with the fixed provider tag
//! `"docket"` (`docket:<remote_task_id>`), mirroring the shape a genuine runner-v1
//! request snapshot uses for its requested model-provider and opaque model id
//! (each independently nullable) without claiming to *be* one.
//! [`LegacyAttemptProjection`] is a read-only, in-memory view that maps an
//! `OrchTask` (the existing `tack_db::repo::orch::OrchTask` read-side struct) into
//! that provider-scoped shape for display — it does not write into
//! `execution_attempts`, does not claim runner-v1 provenance, and is not consulted
//! by the scheduler. It exists so a future operator surface has one normalized
//! place to render "what is this legacy row, using which provider-scoped id, under
//! which scheduling owner" without re-deriving the mapping.

use tack_db::repo::orch::OrchTask;

/// The one explicit, stable compatibility label naming this decision ("Docket is
/// optional and has one documented compatibility state"). See the module doc's
/// "Decision: maintain" section for the evidence behind it.
///
/// Format is deliberately machine-quotable (`<decision>:<scope>-v<n>`) rather than a
/// prose sentence, so operator docs and a future API field can both embed it
/// verbatim without truncation or paraphrase drift.
pub const LEGACY_DOCKET_COMPATIBILITY_LABEL: &str = "legacy-docket:maintained-bridge-v1";

/// Human-readable justification for [`LEGACY_DOCKET_COMPATIBILITY_LABEL`], suitable
/// for direct embedding in operator-facing documentation without
/// paraphrasing the module doc's "Decision: maintain" section.
pub const LEGACY_DOCKET_COMPATIBILITY_POLICY: &str = "Docket is maintained as an optional legacy bridge (TACK_ORCH_ENABLE, default off). \
     It is never the owner of a new runner-v1 execution request; runner-v1 is the \
     plan-of-record scheduler. An item with an active runner-v1 execution \
     request refuses legacy Docket dispatch (one scheduling owner). Docket-origin work \
     is identified with a provider-scoped id (`docket:<remote_task_id>`), distinct from \
     any runner-v1 attempt or opaque model id.";

/// Which plane currently owns (or would own) scheduling a Tack item's execution. See
/// the module doc's "One scheduling owner" section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulingOwner {
    /// The neutral runner-v1 domain (`execution_requests`/`execution_attempts`) —
    /// the plan-of-record scheduler.
    RunnerV1,
    /// The legacy Docket bridge (`orch_tasks`, dispatched via
    /// `tack_orch::adapters::docket::DocketAdapter`).
    LegacyDocket,
}

/// Pure decision: given whether an item already has an active runner-v1 execution
/// request, may a *new* legacy Docket dispatch proceed? No I/O — the caller
/// (`tack_api::dispatcher::dispatch_item`) is responsible for producing
/// `has_active_runner_request` from `Repository::
/// has_active_execution_request_for_item` and for turning `Err` into whatever error
/// shape its own return type uses.
///
/// `Ok(SchedulingOwner::LegacyDocket)` means the caller may proceed to dispatch (or
/// redispatch — this function does not know about `orch_tasks`' own idempotency
/// state, only about the cross-plane collision). `Err(SchedulingOwner::RunnerV1)`
/// means runner-v1 already owns this item; legacy dispatch must defer.
pub fn decide_scheduling_owner(
    has_active_runner_request: bool,
) -> Result<SchedulingOwner, SchedulingOwner> {
    if has_active_runner_request {
        Err(SchedulingOwner::RunnerV1)
    } else {
        Ok(SchedulingOwner::LegacyDocket)
    }
}

/// Namespaces a docket `remote_task_id` as `docket:<remote_task_id>` — see the module
/// doc's "Provider-scoped ids" section for why this exists.
pub fn provider_scoped_task_id(remote_task_id: &str) -> String {
    format!("docket:{remote_task_id}")
}

/// A read-only, normalized-for-display projection of one `orch_tasks` row. See the
/// module doc's "Provider-scoped ids and the normalized-attempt projection" section —
/// this is a presentation mapping, not a write into `execution_attempts` and not a
/// runner-v1 attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyAttemptProjection {
    /// `docket:<remote_task_id>` — see [`provider_scoped_task_id`].
    pub provider_scoped_id: String,
    pub item_id: uuid::Uuid,
    /// Docket's own `remote_status` string, unvalidated and shown as-is — same
    /// discipline `repo/orch.rs`'s module doc already documents for this column
    /// ("every remote-state string column stores whatever docket sent, unvalidated").
    pub remote_status: String,
    /// Always [`SchedulingOwner::LegacyDocket`] — every row this projection can be
    /// built from came from the legacy bridge. Carried explicitly, not implied, so a
    /// caller rendering rows from both planes side by side never has to infer which
    /// is which from the id's string shape alone.
    pub scheduling_owner: SchedulingOwner,
}

impl From<&OrchTask> for LegacyAttemptProjection {
    fn from(task: &OrchTask) -> Self {
        Self {
            provider_scoped_id: provider_scoped_task_id(&task.remote_task_id),
            item_id: task.item_id,
            remote_status: task.remote_status.clone(),
            scheduling_owner: SchedulingOwner::LegacyDocket,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn fixture_task(remote_task_id: &str, remote_status: &str) -> OrchTask {
        let item_id = uuid::Uuid::new_v4();
        OrchTask {
            item_id,
            remote_task_id: remote_task_id.to_string(),
            remote_run_id: None,
            remote_status: remote_status.to_string(),
            attempt: 1,
            tokens_in: 0,
            tokens_out: 0,
            cost_usd_estimated: None,
            dispatched_at: Utc::now(),
            trusted: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn runner_active_blocks_legacy_dispatch() {
        assert_eq!(
            decide_scheduling_owner(true),
            Err(SchedulingOwner::RunnerV1)
        );
    }

    #[test]
    fn runner_inactive_allows_legacy_dispatch() {
        assert_eq!(
            decide_scheduling_owner(false),
            Ok(SchedulingOwner::LegacyDocket)
        );
    }

    #[test]
    fn provider_scoped_id_is_namespaced() {
        assert_eq!(provider_scoped_task_id("abc123"), "docket:abc123");
        // Distinct from a bare id — never collides with an unprefixed runner-v1
        // attempt id or opaque model id displayed alongside it.
        assert_ne!(provider_scoped_task_id("abc123"), "abc123");
    }

    #[test]
    fn projection_carries_the_scheduling_owner_explicitly() {
        let task = fixture_task("task-9", "running");
        let projection = LegacyAttemptProjection::from(&task);
        assert_eq!(projection.provider_scoped_id, "docket:task-9");
        assert_eq!(projection.item_id, task.item_id);
        assert_eq!(projection.remote_status, "running");
        assert_eq!(projection.scheduling_owner, SchedulingOwner::LegacyDocket);
    }

    #[test]
    fn unrecognised_remote_status_is_shown_as_is_not_normalized() {
        // repo/orch.rs's own discipline: an unrecognised docket status string is
        // never validated or rewritten by this layer, only carried through.
        let task = fixture_task("task-x", "some_future_docket_status");
        let projection = LegacyAttemptProjection::from(&task);
        assert_eq!(projection.remote_status, "some_future_docket_status");
    }
}
