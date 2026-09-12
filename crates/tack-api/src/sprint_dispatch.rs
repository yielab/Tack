//! `POST /api/sprints/{id}/dispatch` and
//! `GET /api/sprints/{id}/dispatch/dry-run` — dispatch a whole sprint's
//! items to the project's linked control plane in dependency order.
//!
//! Five decisions this module makes deliberately, rather than letting them
//! fall out of a stray `?`:
//!
//! Design notes: docs/dev-notes/tack-api/sprint_dispatch.md

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Semaphore;
use tracing::warn;
use uuid::Uuid;

use tack_core::dependency::DependencyGraph;
use tack_core::models::{DependencyType, Item, Project};
use tack_db::repo::orch::OrchTask;

use crate::dispatcher::{self, DispatchOutcome, is_active_task_status, is_dispatch_eligible};
use crate::error::ApiError;
use crate::handlers::orch::StatusMap;
use crate::router::AppState;

/// Used when the caller doesn't specify a concurrency cap.
pub const DEFAULT_MAX_IN_FLIGHT: u32 = 5;
/// Hard ceiling regardless of what the caller asks for — a sprint dispatch
/// is still one HTTP request/response cycle from a human's perspective, and
/// an unbounded cap defeats the entire point of decision 3 above.
pub const MAX_MAX_IN_FLIGHT: u32 = 20;

/// `Some(n)` is clamped into `[1, MAX_MAX_IN_FLIGHT]`; `None` becomes
/// [`DEFAULT_MAX_IN_FLIGHT`]. The one place this rule is expressed — both
/// [`dry_run_sprint_dispatch`] and [`dispatch_sprint`] call it, so the cap a
/// dry-run reports is exactly the cap a real run with the same input would
/// use.
pub fn resolve_max_in_flight(requested: Option<u32>) -> u32 {
    requested
        .unwrap_or(DEFAULT_MAX_IN_FLIGHT)
        .clamp(1, MAX_MAX_IN_FLIGHT)
}

// ─────────────────────────────────────────────────────────────────────────
// The shared plan (decision 5)
// ─────────────────────────────────────────────────────────────────────────

/// Why an item was or was not held back — decision 2's "ready" gate.
#[derive(Debug, Clone)]
pub enum Readiness {
    /// Every direct dependency (if any) is in a Done-category status —
    /// eligible to move on to the ordinary [`dispatcher::dispatch_item`]
    /// eligibility checks (`dispatch_from`, already-in-flight).
    Ready,
    /// At least one direct dependency has not reached a Done-category
    /// status yet. `blocked_by` is every such dependency's item id — not
    /// just the first one found — so a caller can show the whole reason,
    /// not a truncated hint.
    WaitingOnDependencies { blocked_by: Vec<Uuid> },
}

/// One sprint item's position in dependency order plus its readiness.
#[derive(Debug, Clone)]
pub struct PlannedItem {
    pub item: Item,
    /// 0-based position in the topological order — the order a real run
    /// submits items to the bounded worker pool.
    pub order: usize,
    pub readiness: Readiness,
}

/// The output of [`plan_sprint_dispatch`] — everything both
/// [`dry_run_sprint_dispatch`] and [`dispatch_sprint`] need, computed once.
pub struct SprintPlan {
    pub sprint_id: Uuid,
    pub project: Project,
    pub status_map: StatusMap,
    /// In topological order (ties broken by board `sort_order` — see
    /// `tack_core::dependency::DependencyGraph::topological_order`).
    pub items: Vec<PlannedItem>,
}

/// Build the dependency-ordered, readiness-gated plan for `sprint_id`.
/// Pure reads — no writes, no HTTP calls — so it is safe to call from a
/// dry-run as well as immediately before a real dispatch. See the module
/// doc's decisions 2 and 5.
///
/// # Errors
///
/// - The sprint or its project don't exist (`NotFound`).
/// - The project has no control-plane link (`Conflict`) — mirrors
///   `dispatcher::dispatch_item`'s own per-item version of this check;
///   raised once here since every item in a sprint shares one project and
///   therefore one link.
/// - The sprint's items cannot be topologically sorted (`Internal`) — see
///   "fail loudly" below.
#[tracing::instrument(skip(state))]
pub async fn plan_sprint_dispatch(
    state: &AppState,
    sprint_id: Uuid,
) -> Result<SprintPlan, ApiError> {
    let sprint = state
        .repo
        .get_sprint(sprint_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Sprint {sprint_id} not found")))?;
    let project = state
        .repo
        .get_project(sprint.project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {} not found", sprint.project_id)))?;
    let Some(link) = state.repo.get_orch_link(project.id).await? else {
        return Err(ApiError::Conflict(format!(
            "project {} is not linked to a control plane",
            project.id
        )));
    };
    let status_map: StatusMap = serde_json::from_value(link.status_map.clone()).unwrap_or_default();

    let items = state.repo.list_items_for_sprint(sprint_id).await?; // sort_order ASC
    let item_ids: Vec<Uuid> = items.iter().map(|i| i.id).collect();
    let items_by_id: HashMap<Uuid, &Item> = items.iter().map(|i| (i.id, i)).collect();

    // ── Dependency graph, scoped to what touches this sprint's items ──
    let deps = state.repo.list_dependencies_for_items(&item_ids).await?;
    let edges: Vec<tack_core::dependency::DependencyEdge> = deps
        .iter()
        .filter(|d| {
            matches!(
                d.dependency_type,
                DependencyType::Blocks | DependencyType::IsBlockedBy
            )
        })
        .map(|d| tack_core::dependency::DependencyEdge {
            source: d.source_item_id,
            target: d.target_item_id,
            dep_type: d.dependency_type.clone(),
        })
        .collect();
    let graph = DependencyGraph::from_edges(&edges);

    // Cycles are supposed to be structurally impossible — the DAG validator
    // rejects a cycle-forming edge at creation time (`validate_new_edge`).
    // Fail loudly rather than deadlock or silently truncate the plan if
    // that invariant is ever violated.
    let order = graph.topological_order(&item_ids).map_err(|e| {
        ApiError::Internal(anyhow::anyhow!(
            "sprint {sprint_id}'s dependency graph could not be topologically \
             sorted — this should be structurally impossible (cycles are \
             rejected at dependency-creation time): {e}"
        ))
    })?;

    // ── Readiness: resolve every blocker's status, in or out of the sprint ──
    let mut blockers_per_item: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    let mut out_of_sprint_blockers: Vec<Uuid> = Vec::new();
    for &id in &item_ids {
        let blockers: Vec<Uuid> = graph.blockers_of(id).into_iter().map(|(b, _)| b).collect();
        for &b in &blockers {
            if !items_by_id.contains_key(&b) {
                out_of_sprint_blockers.push(b);
            }
        }
        blockers_per_item.insert(id, blockers);
    }
    out_of_sprint_blockers.sort();
    out_of_sprint_blockers.dedup();

    // status/project lookup, seeded with the sprint's own items (no extra
    // reads needed for the common case: blockers inside the sprint).
    let mut status_of: HashMap<Uuid, (String, Uuid)> = items
        .iter()
        .map(|i| (i.id, (i.status.clone(), i.project_id)))
        .collect();
    let mut workflow_of: HashMap<Uuid, tack_core::workflow::WorkflowConfig> = HashMap::new();
    workflow_of.insert(project.id, project.workflow.clone());

    for blocker_id in out_of_sprint_blockers {
        match state.repo.get_item(blocker_id).await? {
            Some(bi) => {
                if let std::collections::hash_map::Entry::Vacant(e) =
                    workflow_of.entry(bi.project_id)
                    && let Some(p) = state.repo.get_project(bi.project_id).await?
                {
                    e.insert(p.workflow);
                }
                status_of.insert(bi.id, (bi.status, bi.project_id));
            }
            None => {
                // The blocker item itself no longer exists (deleted after
                // the dependency row was created). Leave it out of
                // `status_of` — the readiness check below treats an
                // unresolvable blocker as unmet, never as satisfied.
                warn!(
                    blocker_id = %blocker_id,
                    sprint_id = %sprint_id,
                    "sprint dispatch: a dependency's blocker item no longer exists; treating as not done"
                );
            }
        }
    }

    let is_blocker_done = |blocker_id: Uuid| -> bool {
        status_of
            .get(&blocker_id)
            .and_then(|(status, proj_id)| {
                workflow_of.get(proj_id).map(|wf| wf.is_done_status(status))
            })
            .unwrap_or(false)
    };

    let mut planned = Vec::with_capacity(order.len());
    for (idx, item_id) in order.into_iter().enumerate() {
        let item = (*items_by_id
            .get(&item_id)
            .expect("topological_order only returns ids from the input set"))
        .clone();
        let blocked_by: Vec<Uuid> = blockers_per_item
            .remove(&item_id)
            .unwrap_or_default()
            .into_iter()
            .filter(|b| !is_blocker_done(*b))
            .collect();
        let readiness = if blocked_by.is_empty() {
            Readiness::Ready
        } else {
            Readiness::WaitingOnDependencies { blocked_by }
        };
        planned.push(PlannedItem {
            item,
            order: idx,
            readiness,
        });
    }

    Ok(SprintPlan {
        sprint_id,
        project,
        status_map,
        items: planned,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Dry run — read-only preview (decision 5)
// ─────────────────────────────────────────────────────────────────────────

/// What a real dispatch would do for one item, previewed without calling
/// docket or writing anything. Mirrors `dispatcher::DispatchOutcome`'s
/// early-exit variants (everything short of actually calling
/// `ControlPlane::enqueue_task`) plus this module's own
/// `waiting_on_dependencies`.
#[derive(Debug, Clone)]
pub enum PreviewDecision {
    WaitingOnDependencies {
        blocked_by: Vec<Uuid>,
    },
    NoDispatchPolicy,
    NotEligible {
        current_status: String,
        dispatch_from: Vec<String>,
    },
    AlreadyInFlight {
        task: OrchTask,
    },
    /// Every check a real run would perform before calling docket passed —
    /// a real run would actually enqueue this item. Never a guarantee (the
    /// item's own status, or another dispatch, could change between this
    /// read and a subsequent real run), only a best-effort preview.
    WouldDispatch,
}

pub struct DryRunItem {
    pub item_id: Uuid,
    pub title: String,
    pub status: String,
    pub order: usize,
    pub decision: PreviewDecision,
}

pub struct DryRunPlan {
    pub sprint_id: Uuid,
    pub max_in_flight: u32,
    pub items: Vec<DryRunItem>,
}

/// `GET /api/sprints/{id}/dispatch/dry-run`'s implementation. Calls
/// [`plan_sprint_dispatch`] and nothing else that writes or calls docket —
/// see decision 5 in the module doc.
pub async fn dry_run_sprint_dispatch(
    state: &AppState,
    sprint_id: Uuid,
    max_in_flight: Option<u32>,
) -> Result<DryRunPlan, ApiError> {
    let plan = plan_sprint_dispatch(state, sprint_id).await?;
    let mut items = Vec::with_capacity(plan.items.len());

    for planned in &plan.items {
        let decision = match &planned.readiness {
            Readiness::WaitingOnDependencies { blocked_by } => {
                PreviewDecision::WaitingOnDependencies {
                    blocked_by: blocked_by.clone(),
                }
            }
            Readiness::Ready => preview_eligibility(state, &plan.status_map, &planned.item).await?,
        };
        items.push(DryRunItem {
            item_id: planned.item.id,
            title: planned.item.title.clone(),
            status: planned.item.status.clone(),
            order: planned.order,
            decision,
        });
    }

    Ok(DryRunPlan {
        sprint_id,
        max_in_flight: resolve_max_in_flight(max_in_flight),
        items,
    })
}

/// The read-only half of `dispatcher::dispatch_item`'s eligibility checks
/// (`dispatch_from` membership, already-in-flight), reusing its exact
/// helpers so the dry-run preview cannot quietly diverge from what a real
/// dispatch actually checks. Deliberately does **not** acquire
/// `dispatcher`'s per-item dispatch lock (there is nothing to protect — no
/// write follows) and does not call `ControlPlane::enqueue_task`.
async fn preview_eligibility(
    state: &AppState,
    status_map: &StatusMap,
    item: &Item,
) -> Result<PreviewDecision, ApiError> {
    if status_map.dispatch_from.is_empty() {
        return Ok(PreviewDecision::NoDispatchPolicy);
    }
    if !is_dispatch_eligible(status_map, &item.status) {
        return Ok(PreviewDecision::NotEligible {
            current_status: item.status.clone(),
            dispatch_from: status_map.dispatch_from.clone(),
        });
    }
    let existing = state.repo.list_orch_tasks_for_item(item.id).await?;
    if let Some(latest) = existing.first()
        && is_active_task_status(&latest.remote_status)
    {
        return Ok(PreviewDecision::AlreadyInFlight {
            task: latest.clone(),
        });
    }
    Ok(PreviewDecision::WouldDispatch)
}

// ─────────────────────────────────────────────────────────────────────────
// The real dispatch — bounded concurrency (decisions 1, 3, 4)
// ─────────────────────────────────────────────────────────────────────────

/// What actually happened (or didn't) for one item in a real
/// [`dispatch_sprint`] run.
#[derive(Debug, Clone)]
pub enum ItemResult {
    /// Held back by decision 2 — never reached `dispatch_item` at all.
    WaitingOnDependencies { blocked_by: Vec<Uuid> },
    /// `dispatch_item` ran and returned this outcome (which itself may be
    /// `NoDispatchPolicy`/`NotEligible`/`AlreadyInFlight`/`Blocked`/
    /// `Success` — branch on it the same way a single-item dispatch
    /// response does). Boxed: `DispatchOutcome::Success` carries a full
    /// `OrchTask`, which otherwise makes this the dominant size of every
    /// `ItemResult`, including the common no-op variants.
    Outcome(Box<DispatchOutcome>),
    /// `dispatch_item` returned an `Err`, or its worker task panicked.
    /// Recorded per item, per decision 1 — this item's own row is the only
    /// thing that failed; every other item in the sprint still ran.
    Error(String),
}

pub struct SprintDispatchItem {
    pub item_id: Uuid,
    pub title: String,
    /// The item's status **at plan time**, before this call dispatched
    /// anything — a real dispatch may move it (via `on_running`), but this
    /// field reports where it started, matching what the dry-run preview
    /// would have shown for the same item.
    pub status: String,
    pub order: usize,
    pub result: ItemResult,
}

pub struct SprintDispatchReport {
    pub sprint_id: Uuid,
    pub max_in_flight: u32,
    pub items: Vec<SprintDispatchItem>,
}

/// `POST /api/sprints/{id}/dispatch`'s implementation. Calls
/// [`plan_sprint_dispatch`], then runs every dependency-ready item through
/// [`dispatcher::dispatch_item`] with concurrency capped at
/// `max_in_flight` (see [`resolve_max_in_flight`]). Submission follows
/// topological order; a `tokio::sync::Semaphore` is the bound (decision 3).
/// A failure or panic dispatching one item never aborts the rest (decision
/// 1) — see the module doc.
pub async fn dispatch_sprint(
    state: &AppState,
    sprint_id: Uuid,
    max_in_flight: Option<u32>,
) -> Result<SprintDispatchReport, ApiError> {
    let plan = plan_sprint_dispatch(state, sprint_id).await?;
    let cap = resolve_max_in_flight(max_in_flight);
    let semaphore = Arc::new(Semaphore::new(cap as usize));

    let mut items_out: Vec<SprintDispatchItem> = Vec::with_capacity(plan.items.len());
    // (item_id -> (order, title)) for every item submitted to the pool, so
    // a panicked worker task (whose payload `JoinSet` cannot hand back) can
    // still be attributed to the right row rather than silently vanishing
    // from the report.
    let mut in_flight: HashMap<Uuid, (usize, String, String)> = HashMap::new();
    let mut join_set = tokio::task::JoinSet::new();

    for planned in &plan.items {
        match &planned.readiness {
            Readiness::WaitingOnDependencies { blocked_by } => {
                items_out.push(SprintDispatchItem {
                    item_id: planned.item.id,
                    title: planned.item.title.clone(),
                    status: planned.item.status.clone(),
                    order: planned.order,
                    result: ItemResult::WaitingOnDependencies {
                        blocked_by: blocked_by.clone(),
                    },
                });
            }
            Readiness::Ready => {
                let item = planned.item.clone();
                let order = planned.order;
                in_flight.insert(item.id, (order, item.title.clone(), item.status.clone()));

                let sem = semaphore.clone();
                let state = state.clone();
                let trusted = item.source.is_trusted();
                join_set.spawn(async move {
                    let _permit = sem
                        .acquire_owned()
                        .await
                        .expect("semaphore is never explicitly closed");
                    let outcome = dispatcher::dispatch_item(&state, item.id, trusted).await;
                    (item.id, order, item.title, item.status, outcome)
                });
            }
        }
    }

    while let Some(joined) = join_set.join_next().await {
        match joined {
            Ok((item_id, order, title, status, outcome)) => {
                in_flight.remove(&item_id);
                let result = match outcome {
                    Ok(o) => ItemResult::Outcome(Box::new(o)),
                    Err(e) => {
                        warn!(item_id = %item_id, sprint_id = %sprint_id, error = %e, "sprint dispatch: one item failed; continuing with the rest");
                        ItemResult::Error(e.to_string())
                    }
                };
                items_out.push(SprintDispatchItem {
                    item_id,
                    title,
                    status,
                    order,
                    result,
                });
            }
            Err(join_err) => {
                warn!(sprint_id = %sprint_id, error = %join_err, "sprint dispatch: a worker task panicked");
                // Attributed below, once every remaining task has settled.
            }
        }
    }

    // Any survivor in `in_flight` is a task whose `JoinError` we already
    // logged above and couldn't attribute inline — attribute it now.
    for (item_id, (order, title, status)) in in_flight {
        items_out.push(SprintDispatchItem {
            item_id,
            title,
            status,
            order,
            result: ItemResult::Error(
                "dispatch worker task panicked before completing".to_string(),
            ),
        });
    }

    items_out.sort_by_key(|i| i.order);

    Ok(SprintDispatchReport {
        sprint_id,
        max_in_flight: cap,
        items: items_out,
    })
}
