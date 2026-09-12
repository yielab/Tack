//! The Docket compatibility decision and its explicit
//! label/policy, plus the pure, DB-free pieces of "one scheduling owner."
//!
//! # Decision: maintain
//!
//! Three options exist for the legacy Docket bridge: maintain, export, or
//!
//! Design notes: docs/dev-notes/tack-orch/adapters/legacy_bridge.md

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
